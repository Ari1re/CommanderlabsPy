//! In-app updates from the GitHub releases page.
//!
//! Every CI build is published as release `build-N` (N = the workflow run
//! number) and the number is baked into the app as `COMMANDERLAB_BUILD`. The
//! app compares it with the latest release and, when asked, installs the
//! update for the platform it runs on:
//!
//! - Windows installer: download the setup exe and run it with `/P /R`
//!   (progress only, reopen the app afterwards), then quit.
//! - Windows portable exe: swap the exe for the new one and restart it.
//! - Linux (installed by the .sh, incl. NixOS' browser edition): run the new
//!   .sh with `--extract`, then restart through the launcher.
//! - Linux AppImage run on its own: replace the file and restart it.
//! - Android and the .deb: the UI opens the download; the system installer
//!   takes over (Android installs over the old app, keeping all data).

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const REPO: &str = "Ari1re/CommanderlabsPy";
const CACHE_PATH: &str = "./data/update.json";
const RECHECK_SECS: u64 = 6 * 3600;

/// Build number of this binary (None for local development builds).
pub fn current_build() -> Option<u32> {
    option_env!("COMMANDERLAB_BUILD").and_then(|b| b.trim().parse().ok())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateInfo {
    pub current_version: String,
    pub current_build: Option<u32>,
    pub latest_build: Option<u32>,
    pub latest_version: String,
    pub available: bool,
    pub published_at: String,
    pub notes: String,
    pub page_url: String,
    /// The file for this platform.
    pub download_url: Option<String>,
    /// "auto": the app installs it and restarts; "download": open the file
    /// and let the system install it; "none": no update for this build.
    pub install: String,
    /// How the update is applied on this device, in a sentence.
    pub how: String,
    pub checked_at: u64,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn client(timeout: u64) -> Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(timeout))
        .user_agent(concat!("CommanderLab/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

/// How this copy of the app was installed (desktop and web only).
#[derive(Debug, Clone, PartialEq)]
enum Install {
    WindowsInstaller,
    WindowsPortable(PathBuf),
    /// Installed by the .sh into ~/.local/share/commander-lab/app.
    LinuxScript { app_dir: PathBuf, web: bool },
    LinuxAppImage(PathBuf),
    /// .deb, Android or anything else the app can't replace itself.
    Manual,
}

fn linux_app_dir() -> PathBuf {
    let base = std::env::var_os("COMMANDER_LAB_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_DATA_HOME").map(|d| PathBuf::from(d).join("commander-lab")))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/commander-lab")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("app")
}

fn detect(platform: &str) -> Install {
    if platform == "android" {
        return Install::Manual;
    }
    let exe = std::env::current_exe().ok();
    if cfg!(windows) {
        let Some(exe) = exe else { return Install::Manual };
        let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
        // The NSIS install puts an uninstaller beside the app.
        let installed = fs::read_dir(&dir)
            .map(|entries| {
                entries.flatten().any(|e| e.file_name().to_string_lossy().to_ascii_lowercase().starts_with("uninstall"))
            })
            .unwrap_or(false);
        return if installed { Install::WindowsInstaller } else { Install::WindowsPortable(exe) };
    }
    if cfg!(target_os = "linux") {
        let app_dir = linux_app_dir();
        let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
        let inside = |p: &Path| p.starts_with(&app_dir);
        if appimage.as_deref().map(inside).unwrap_or(false) || exe.as_deref().map(inside).unwrap_or(false) {
            return Install::LinuxScript { app_dir, web: platform == "web" };
        }
        if let Some(path) = appimage {
            return Install::LinuxAppImage(path);
        }
    }
    Install::Manual
}

fn asset_for(platform: &str, install: &Install) -> &'static str {
    match (platform, install) {
        ("android", _) => "CommanderLab-android.apk",
        (_, Install::WindowsInstaller) => "CommanderLab-windows-x86_64-setup.exe",
        (_, Install::WindowsPortable(_)) => "CommanderLab-windows-x86_64-portable.exe",
        (_, Install::LinuxScript { .. }) => "CommanderLab-linux-x86_64.sh",
        (_, Install::LinuxAppImage(_)) => "CommanderLab-linux-x86_64.AppImage",
        _ if cfg!(windows) => "CommanderLab-windows-x86_64-setup.exe",
        _ => "CommanderLab-linux-x86_64.sh",
    }
}

/// Latest release, at most every six hours unless `force`.
pub fn check(platform: &str, force: bool) -> Result<UpdateInfo> {
    if !force {
        if let Ok(cached) = fs::read_to_string(CACHE_PATH).map(|t| serde_json::from_str::<UpdateInfo>(&t)) {
            if let Ok(info) = cached {
                if now().saturating_sub(info.checked_at) < RECHECK_SECS && info.current_build == current_build() {
                    return Ok(info);
                }
            }
        }
    }

    let release: Value = client(20)?
        // COMMANDERLAB_RELEASE_URL lets tests point the check elsewhere.
        .get(std::env::var("COMMANDERLAB_RELEASE_URL").unwrap_or_else(|_| format!("https://api.github.com/repos/{REPO}/releases/latest")))
        .header("Accept", "application/vnd.github+json")
        .send()
        .context("Couldn't reach GitHub to check for updates")?
        .error_for_status()
        .context("GitHub didn't return the latest release")?
        .json()?;

    let tag = release.get("tag_name").and_then(Value::as_str).unwrap_or_default();
    let latest_build = tag.strip_prefix("build-").and_then(|n| n.parse::<u32>().ok());
    let install = detect(platform);
    let wanted = asset_for(platform, &install);
    let download_url = release
        .get("assets")
        .and_then(Value::as_array)
        .and_then(|assets| {
            assets.iter().find(|a| a.get("name").and_then(Value::as_str) == Some(wanted))
        })
        .and_then(|a| a.get("browser_download_url").and_then(Value::as_str))
        .map(str::to_string);

    let current = current_build();
    let available = matches!((current, latest_build), (Some(c), Some(l)) if l > c);
    let (mode, how) = match (&install, platform) {
        (_, "android") => ("download", "Downloads the new app; Android then asks to install it over this one. Your decks are kept."),
        (Install::WindowsInstaller, _) => ("auto", "Downloads the installer, installs the update and reopens Commander Lab."),
        (Install::WindowsPortable(_), _) => ("auto", "Replaces this program file with the new version and reopens it."),
        (Install::LinuxScript { web: true, .. }, _) => ("auto", "Installs the update and restarts Commander Lab in a new browser tab."),
        (Install::LinuxScript { .. }, _) => ("auto", "Installs the update and reopens Commander Lab."),
        (Install::LinuxAppImage(_), _) => ("auto", "Replaces this AppImage with the new version and reopens it."),
        (Install::Manual, _) => ("download", "Downloads the new version for you to install (this copy can't replace itself)."),
    };

    let info = UpdateInfo {
        current_version: crate::VERSION.to_string(),
        current_build: current,
        latest_build,
        latest_version: release
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(tag)
            .trim_start_matches("Commander Lab ")
            .to_string(),
        available,
        published_at: release.get("published_at").and_then(Value::as_str).unwrap_or_default().to_string(),
        notes: release.get("body").and_then(Value::as_str).unwrap_or_default().chars().take(4000).collect(),
        page_url: release
            .get("html_url")
            .and_then(Value::as_str)
            .unwrap_or("https://github.com/Ari1re/CommanderlabsPy/releases/latest")
            .to_string(),
        download_url,
        install: if current.is_none() { "none".into() } else { mode.into() },
        how: if current.is_none() {
            if from_git_checkout() {
                "Built from source: run ./run-linux.sh --update in the Commander Lab folder to pull and rebuild the latest version.".into()
            } else {
                "This is a development build, so it doesn't update itself.".into()
            }
        } else {
            how.into()
        },
        checked_at: now(),
    };
    if let Some(parent) = Path::new(CACHE_PATH).parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(CACHE_PATH, serde_json::to_string(&info)?);
    Ok(info)
}

/// Running from a `cargo build` inside a git checkout (./run-linux.sh).
fn from_git_checkout() -> bool {
    std::env::current_exe()
        .ok()
        .map(|exe| exe.ancestors().any(|dir| dir.join(".git").exists() && dir.join("run-linux.sh").exists()))
        .unwrap_or(false)
}

fn download(url: &str, to: &Path, mut progress: impl FnMut(u64, u64)) -> Result<()> {
    use std::io::{Read, Write};
    let mut response = client(600)?
        .get(url)
        .send()
        .context("Couldn't download the update")?
        .error_for_status()
        .context("GitHub didn't serve the update file")?;
    let total = response.content_length().unwrap_or(0);
    let part = to.with_extension("part");
    let mut file = fs::File::create(&part)?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        let n = response.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        done += n as u64;
        progress(done, total);
    }
    file.flush()?;
    drop(file);
    if total > 0 && done != total {
        return Err(anyhow!("The download was cut short ({done} of {total} bytes). Try again."));
    }
    fs::rename(&part, to)?;
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}
#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<()> {
    Ok(())
}

/// Quit shortly, so the UI can show "Restarting…" first.
fn quit_soon() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(1500));
        std::process::exit(0);
    });
}

/// Download and install the latest release, then restart. Returns a message
/// for the user. The process exits about a second and a half later.
pub fn install(platform: &str, mut progress: impl FnMut(u16, String)) -> Result<String> {
    let info = check(platform, true)?;
    if !info.available {
        return Err(anyhow!("You already have the latest version."));
    }
    if info.install != "auto" {
        return Err(anyhow!("This copy can't update itself. {}", info.how));
    }
    let url = info.download_url.clone().ok_or_else(|| anyhow!("The latest release has no file for this system."))?;
    let install = detect(platform);
    let dir = std::env::temp_dir().join("commander-lab-update");
    fs::create_dir_all(&dir)?;
    let file_name = url.rsplit('/').next().unwrap_or("update");
    let target = dir.join(file_name);
    download(&url, &target, |done, total| {
        let pct = if total > 0 { (done * 90 / total) as u16 } else { 50 };
        progress(pct, format!("{:.1} of {:.1} MB", done as f64 / 1e6, total as f64 / 1e6));
    })?;
    progress(92, "Installing".into());

    match install {
        Install::WindowsInstaller => {
            // Passive install (progress bar only) and reopen afterwards.
            Command::new(&target).args(["/P", "/R"]).spawn().context("Couldn't start the installer")?;
        }
        Install::WindowsPortable(exe) => {
            // A running exe can be renamed but not overwritten.
            let old = exe.with_extension("old.exe");
            let _ = fs::remove_file(&old);
            fs::rename(&exe, &old).context("Couldn't move the old program aside")?;
            if let Err(err) = fs::copy(&target, &exe) {
                let _ = fs::rename(&old, &exe);
                return Err(err).context("Couldn't put the new program in place");
            }
            Command::new(&exe).spawn().context("Couldn't start the new version")?;
        }
        Install::LinuxScript { app_dir, web } => {
            make_executable(&target)?;
            let status = Command::new("sh").arg(&target).arg("--extract").status().context("Couldn't run the installer")?;
            if !status.success() {
                return Err(anyhow!("The installer failed (exit {status})."));
            }
            let launcher = app_dir.join("commander-lab.sh");
            // Start the new version once this one has quit (and freed the
            // browser edition's port).
            let flag = if web { " --web" } else { "" };
            Command::new("sh")
                .arg("-c")
                .arg(format!("sleep 2; exec \"{}\"{flag}", launcher.display()))
                .spawn()
                .context("Couldn't restart Commander Lab")?;
        }
        Install::LinuxAppImage(path) => {
            make_executable(&target)?;
            let old = path.with_extension("AppImage.old");
            let _ = fs::remove_file(&old);
            fs::rename(&path, &old).context("Couldn't move the old AppImage aside")?;
            if let Err(err) = fs::copy(&target, &path) {
                let _ = fs::rename(&old, &path);
                return Err(err).context("Couldn't put the new AppImage in place");
            }
            make_executable(&path)?;
            Command::new("sh")
                .arg("-c")
                .arg(format!("sleep 2; exec \"{}\"", path.display()))
                .spawn()
                .context("Couldn't restart Commander Lab")?;
        }
        Install::Manual => return Err(anyhow!("This copy can't update itself.")),
    }
    progress(100, "Restarting".into());
    quit_soon();
    Ok(format!("Updating to {}. Commander Lab will restart in a moment.", info.latest_version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_downloads_the_apk() {
        assert_eq!(asset_for("android", &Install::Manual), "CommanderLab-android.apk");
        assert_eq!(asset_for("desktop", &Install::WindowsInstaller), "CommanderLab-windows-x86_64-setup.exe");
        assert_eq!(
            asset_for("web", &Install::LinuxScript { app_dir: PathBuf::new(), web: true }),
            "CommanderLab-linux-x86_64.sh"
        );
        assert_eq!(detect("android"), Install::Manual);
    }
}
