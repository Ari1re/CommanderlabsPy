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
//! - Linux (installed by the .sh, NixOS included): run the new .sh with
//!   `--extract`, then restart through the launcher.
//! - Linux AppImage or browser-edition binary run on its own: replace the
//!   file and restart it.
//! - .deb: download it and install it with pkexec (the system asks for the
//!   password), then restart.
//! - Android: download the APK; the app then opens Android's installer, which
//!   installs over the old app and keeps all data.
//! - Built from a git checkout (./run-linux.sh): there's no build number, so
//!   it compares with the remote branch, then pulls, rebuilds and restarts.
//!
//! Things to work on:
//! - Builds are published from every push to main; a beta/stable channel
//!   would let risky changes (like the built-in AI engine) bake first.

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
/// A quick relaunch reuses the last answer; anything older asks GitHub again.
const RECHECK_SECS: u64 = 10 * 60;

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
    /// What kind of copy this is: "windows-installer", "windows-portable",
    /// "linux-script", "appimage", "linux-binary", "deb", "android",
    /// "source" or "manual".
    #[serde(default)]
    pub kind: String,
    /// How the update is applied on this device, in a sentence.
    pub how: String,
    pub checked_at: u64,
}

/// Seconds since 1970, for `checked_at` and the re-check timer.
fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// HTTP client with our user agent; `timeout` in seconds (20 to check, 600
/// to download).
fn client(timeout: u64) -> Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(timeout))
        .user_agent(concat!("CommanderLab/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

/// How this copy of the app was installed.
#[derive(Debug, Clone, PartialEq)]
enum Install {
    WindowsInstaller,
    WindowsPortable(PathBuf),
    /// Installed by the .sh into ~/.local/share/commander-lab/app.
    LinuxScript { app_dir: PathBuf, web: bool },
    LinuxAppImage(PathBuf),
    /// The browser-edition binary run from wherever it was downloaded to.
    LinuxBinary(PathBuf),
    /// Installed from the .deb (the program lives under /usr).
    Deb(PathBuf),
    Android,
    /// Built by ./run-linux.sh in a git checkout.
    Source { repo: PathBuf, web: bool },
    /// Anything else: the update is opened in the browser.
    Manual,
}

impl Install {
    /// Short name stored in `UpdateInfo.kind`. The cached check is thrown away
    /// when this changes (for example after moving from AppImage to the .sh install).
    fn kind(&self) -> &'static str {
        match self {
            Install::WindowsInstaller => "windows-installer",
            Install::WindowsPortable(_) => "windows-portable",
            Install::LinuxScript { .. } => "linux-script",
            Install::LinuxAppImage(_) => "appimage",
            Install::LinuxBinary(_) => "linux-binary",
            Install::Deb(_) => "deb",
            Install::Android => "android",
            Install::Source { .. } => "source",
            Install::Manual => "manual",
        }
    }
}

/// Where the Linux .sh installs the app: $COMMANDER_LAB_HOME, then
/// $XDG_DATA_HOME/commander-lab, then ~/.local/share/commander-lab, plus "/app".
/// Must stay in step with the install script.
fn linux_app_dir() -> PathBuf {
    let base = std::env::var_os("COMMANDER_LAB_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_DATA_HOME").map(|d| PathBuf::from(d).join("commander-lab")))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/commander-lab")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("app")
}

/// The git checkout this program was built in by ./run-linux.sh, if any.
fn source_checkout() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .find(|dir| dir.join(".git").exists() && dir.join("run-linux.sh").exists())
        .map(Path::to_path_buf)
}

/// Can we replace files here? Writes and deletes an empty probe file.
fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(".commander-lab-write-test");
    let ok = fs::write(&probe, b"").is_ok();
    let _ = fs::remove_file(&probe);
    ok
}

/// Did the .deb put this program here? Only under /usr on a dpkg system,
/// confirmed with `dpkg -S`.
fn installed_by_dpkg(exe: &Path) -> bool {
    exe.starts_with("/usr")
        && Path::new("/var/lib/dpkg").exists()
        && Command::new("dpkg")
            .arg("-S")
            .arg(exe)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

/// Work out how this copy was installed, so the right update file and method
/// are used. Order matters: Android; on Windows, an uninstaller beside the exe
/// means the installer, otherwise portable; on Linux, the .sh app dir, then
/// AppImage, then a git checkout (only for builds with no build number), then
/// .deb, then a standalone browser-edition binary in a writable folder.
/// Anything else is Manual (open the download page).
fn detect(platform: &str) -> Install {
    if platform == "android" {
        return Install::Android;
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
        if current_build().is_none() {
            if let Some(repo) = source_checkout() {
                return Install::Source { repo, web: platform == "web" };
            }
        }
        if let Some(exe) = exe {
            if installed_by_dpkg(&exe) {
                return Install::Deb(exe);
            }
            if platform == "web" && exe.parent().map(dir_writable).unwrap_or(false) {
                return Install::LinuxBinary(exe);
            }
        }
    }
    Install::Manual
}

/// Release asset file name for this kind of install. These names must match
/// what CI uploads, or `download_url` comes back empty.
fn asset_for(platform: &str, install: &Install) -> &'static str {
    match (platform, install) {
        ("android", _) => "CommanderLab-android.apk",
        (_, Install::WindowsInstaller) => "CommanderLab-windows-x86_64-setup.exe",
        (_, Install::WindowsPortable(_)) => "CommanderLab-windows-x86_64-portable.exe",
        (_, Install::LinuxScript { .. }) => "CommanderLab-linux-x86_64.sh",
        (_, Install::LinuxAppImage(_)) => "CommanderLab-linux-x86_64.AppImage",
        (_, Install::LinuxBinary(_)) => "CommanderLab-web-linux-x86_64",
        (_, Install::Deb(_)) => "CommanderLab-linux-amd64.deb",
        _ if cfg!(windows) => "CommanderLab-windows-x86_64-setup.exe",
        _ => "CommanderLab-linux-x86_64.sh",
    }
}

/// Latest release, at most every ten minutes unless `force`.
pub fn check(platform: &str, force: bool) -> Result<UpdateInfo> {
    let install = detect(platform);
    if !force {
        if let Ok(Ok(info)) = fs::read_to_string(CACHE_PATH).map(|t| serde_json::from_str::<UpdateInfo>(&t)) {
            if now().saturating_sub(info.checked_at) < RECHECK_SECS
                && info.current_build == current_build()
                && info.kind == install.kind()
            {
                return Ok(info);
            }
        }
    }
    let info = match &install {
        Install::Source { repo, .. } => check_source(repo),
        _ => check_release(platform, &install)?,
    };
    if let Some(parent) = Path::new(CACHE_PATH).parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(CACHE_PATH, serde_json::to_string(&info)?);
    Ok(info)
}

/// Ask GitHub for the latest release, read its build number from the
/// `build-N` tag, find this platform's asset and describe how it would be
/// installed. Builds with no build number (local builds) always report
/// `install: "none"`.
fn check_release(platform: &str, install: &Install) -> Result<UpdateInfo> {
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
    let wanted = asset_for(platform, install);
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
    let (mode, how) = match install {
        Install::Android => ("auto", "Downloads the new version and opens Android's installer. Tap Install; your decks are kept."),
        Install::WindowsInstaller => ("auto", "Downloads the installer, installs the update and reopens Commander Lab."),
        Install::WindowsPortable(_) => ("auto", "Replaces this program file with the new version and reopens it."),
        Install::LinuxScript { web: true, .. } => ("auto", "Installs the update and restarts Commander Lab in a new browser tab."),
        Install::LinuxScript { .. } => ("auto", "Installs the update and reopens Commander Lab."),
        Install::LinuxAppImage(_) => ("auto", "Replaces this AppImage with the new version and reopens it."),
        Install::LinuxBinary(_) => ("auto", "Replaces this program with the new version and restarts it."),
        Install::Deb(_) => ("auto", "Downloads the new package and installs it (your system asks for your password), then reopens Commander Lab."),
        Install::Source { .. } | Install::Manual => ("download", "Downloads the new version for you to install (this copy can't replace itself)."),
    };

    Ok(UpdateInfo {
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
        how: if current.is_none() { "This is a development build, so it doesn't update itself.".into() } else { how.into() },
        kind: install.kind().into(),
        checked_at: now(),
    })
}

/// Run git in the checkout and return trimmed stdout. Fails with git's stderr,
/// and never waits on a credential prompt.
fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        // Never wait on a password prompt nobody can see.
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .context("git isn't installed")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(anyhow!("git {} failed: {}", args.join(" "), err));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The branch this checkout pulls from.
fn upstream(repo: &Path) -> String {
    git(repo, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]).unwrap_or_else(|_| "origin/main".into())
}

/// The newest section of a changelog.
fn top_changelog_section(text: &str) -> String {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with("## ") {
            if !out.is_empty() {
                break;
            }
        }
        if !out.is_empty() || line.starts_with("## ") {
            out.push(line);
        }
    }
    out.join("\n")
}

/// Built from source: compare with the branch on GitHub.
fn check_source(repo: &Path) -> UpdateInfo {
    let mut info = UpdateInfo {
        current_version: crate::VERSION.to_string(),
        page_url: format!("https://github.com/{REPO}/commits/main"),
        install: "auto".into(),
        how: "Pulls the latest code with git, rebuilds (this takes a few minutes) and restarts Commander Lab.".into(),
        kind: "source".into(),
        checked_at: now(),
        ..Default::default()
    };
    let result = (|| -> Result<()> {
        git(repo, &["fetch", "--quiet"])?;
        let up = upstream(repo);
        let behind: u32 = git(repo, &["rev-list", "--count", &format!("HEAD..{up}")])?.parse().unwrap_or(0);
        info.available = behind > 0;
        let cargo = git(repo, &["show", &format!("{up}:Cargo.toml")]).unwrap_or_default();
        info.latest_version = cargo
            .lines()
            .find_map(|l| l.trim().strip_prefix("version = \"").and_then(|v| v.strip_suffix('"')))
            .unwrap_or(crate::VERSION)
            .to_string();
        if behind > 0 {
            info.latest_version = format!("{} ({behind} new commit{})", info.latest_version, if behind == 1 { "" } else { "s" });
            let changelog = git(repo, &["show", &format!("{up}:CHANGELOG.md")]).unwrap_or_default();
            let mut notes = top_changelog_section(&changelog);
            if notes.is_empty() {
                notes = git(repo, &["log", "--format=- %s", &format!("HEAD..{up}")]).unwrap_or_default();
            }
            info.notes = notes.chars().take(4000).collect();
        }
        Ok(())
    })();
    if let Err(err) = result {
        info.install = "none".into();
        info.how = format!(
            "Couldn't compare with GitHub ({err:#}). Run ./run-linux.sh --update in {} to update.",
            repo.display()
        );
    }
    info
}

/// Running from a `cargo build` inside a git checkout (./run-linux.sh).
#[allow(dead_code)]
fn from_git_checkout() -> bool {
    source_checkout().is_some()
}

/// Stream a release file to `<to>.part` in 256 KB chunks with progress, check
/// the size against Content-Length, then rename it into place. A cut-off
/// download never leaves a half file under the real name.
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

/// chmod 755, so a downloaded binary, AppImage or .sh can run.
#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}
/// Windows has no exec bit: nothing to do.
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

/// Is `program` on PATH (`command -v`)? Only called with fixed names like
/// "pkexec"; the name goes into a shell string, so never pass user text.
fn have(program: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {program} >/dev/null 2>&1"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Start `program args…` two seconds after this process has quit.
fn restart_later(program: &Path, args: &[String]) -> Result<()> {
    Command::new("sh")
        .arg("-c")
        .arg("sleep 2; exec \"$0\" \"$@\"")
        .arg(program)
        .args(args)
        .spawn()
        .context("Couldn't restart Commander Lab")?;
    Ok(())
}

/// Swap a running program file for a new one (a running file can be renamed
/// but not always overwritten).
fn swap_file(new: &Path, path: &Path, old_ext: &str) -> Result<()> {
    make_executable(new)?;
    let old = path.with_extension(old_ext);
    let _ = fs::remove_file(&old);
    fs::rename(path, &old).context("Couldn't move the old program aside")?;
    if let Err(err) = fs::copy(new, path) {
        let _ = fs::rename(&old, path);
        return Err(err).context("Couldn't put the new program in place");
    }
    make_executable(path)?;
    Ok(())
}

/// What an update did.
#[derive(Debug, Serialize)]
pub struct InstallOutcome {
    pub message: String,
    /// Android: the downloaded APK, for the app to hand to the installer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apk: Option<String>,
    /// The app quits by itself in a moment.
    pub restarting: bool,
}

/// Download and install the latest version. Desktop copies restart about a
/// second and a half later; on Android the app opens the installer.
pub fn install(platform: &str, mut progress: impl FnMut(u16, String)) -> Result<InstallOutcome> {
    let info = check(platform, true)?;
    if !info.available {
        return Err(anyhow!("You already have the latest version."));
    }
    if info.install != "auto" {
        return Err(anyhow!("This copy can't update itself. {}", info.how));
    }
    let install = detect(platform);
    if let Install::Source { repo, web } = &install {
        return install_source(repo, *web, &info, progress);
    }

    let url = info.download_url.clone().ok_or_else(|| anyhow!("The latest release has no file for this system."))?;
    let dir = if matches!(install, Install::Android) {
        // Android's temp folder is cleared without warning; keep it with the app's data.
        std::env::current_dir()?.join("data").join("updates")
    } else {
        std::env::temp_dir().join("commander-lab-update")
    };
    fs::create_dir_all(&dir)?;
    let file_name = url.rsplit('/').next().unwrap_or("update");
    let target = dir.join(file_name);
    download(&url, &target, |done, total| {
        let pct = if total > 0 { (done * 90 / total) as u16 } else { 50 };
        progress(pct, format!("{:.1} of {:.1} MB", done as f64 / 1e6, total as f64 / 1e6));
    })?;
    progress(92, "Installing".into());

    match install {
        Install::Android => {
            progress(100, "Opening the installer".into());
            return Ok(InstallOutcome {
                message: format!("Downloaded {}. Opening Android's installer…", info.latest_version),
                apk: Some(target.to_string_lossy().into_owned()),
                restarting: false,
            });
        }
        Install::WindowsInstaller => {
            // Passive install (progress bar only) and reopen afterwards.
            Command::new(&target).args(["/P", "/R"]).spawn().context("Couldn't start the installer")?;
        }
        Install::WindowsPortable(exe) => {
            swap_file(&target, &exe, "old.exe")?;
            Command::new(&exe).spawn().context("Couldn't start the new version")?;
        }
        Install::LinuxScript { app_dir, web } => {
            make_executable(&target)?;
            let status = Command::new("sh").arg(&target).arg("--extract").status().context("Couldn't run the installer")?;
            if !status.success() {
                return Err(anyhow!("The installer failed (exit {status})."));
            }
            // Start the new version once this one has quit (and freed the
            // browser edition's port).
            let args = if web { vec!["--web".to_string()] } else { vec![] };
            restart_later(&app_dir.join("commander-lab.sh"), &args)?;
        }
        Install::LinuxAppImage(path) => {
            swap_file(&target, &path, "AppImage.old")?;
            restart_later(&path, &[])?;
        }
        Install::LinuxBinary(exe) => {
            swap_file(&target, &exe, "old")?;
            let args: Vec<String> = std::env::args().skip(1).collect();
            restart_later(&exe, &args)?;
        }
        Install::Deb(exe) => {
            if !have("pkexec") {
                // No graphical password prompt: let the software center take it.
                Command::new("xdg-open").arg(&target).spawn().context("Couldn't open the downloaded package")?;
                return Ok(InstallOutcome {
                    message: "Opened the new package in your software installer. Install it, then reopen Commander Lab.".into(),
                    apk: None,
                    restarting: false,
                });
            }
            let installer: &[&str] = if have("apt-get") { &["apt-get", "install", "-y"] } else { &["dpkg", "-i"] };
            let status = Command::new("pkexec")
                .args(installer)
                .arg(&target)
                .status()
                .context("Couldn't start the package installer")?;
            if !status.success() {
                return Err(anyhow!(
                    "The package wasn't installed (cancelled, or the password was wrong). Try again, or install {} yourself.",
                    target.display()
                ));
            }
            restart_later(&exe, &[])?;
        }
        Install::Source { .. } | Install::Manual => return Err(anyhow!("This copy can't update itself.")),
    }
    progress(100, "Restarting".into());
    quit_soon();
    Ok(InstallOutcome {
        message: format!("Updating to {}. Commander Lab will restart in a moment.", info.latest_version),
        apk: None,
        restarting: true,
    })
}

/// Built from source: git pull, rebuild with ./run-linux.sh, restart.
fn install_source(repo: &Path, web: bool, info: &UpdateInfo, mut progress: impl FnMut(u16, String)) -> Result<InstallOutcome> {
    use std::io::{BufRead, BufReader};
    progress(3, "Getting the latest code".into());
    git(repo, &["pull", "--ff-only"])?;

    // Rough progress: one "Compiling" line per crate in the lockfile.
    let crates = fs::read_to_string(repo.join("Cargo.lock"))
        .map(|t| t.matches("[[package]]").count())
        .unwrap_or(400)
        .max(1);
    progress(8, "Rebuilding (this takes a few minutes)".into());
    let mut cmd = Command::new("sh");
    cmd.arg("run-linux.sh").arg("--build");
    if web {
        cmd.arg("--web");
    }
    let mut child = cmd
        .current_dir(repo)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("Couldn't start the rebuild")?;
    let stdout = child.stdout.take();
    let drain = std::thread::spawn(move || {
        let mut lines = Vec::new();
        if let Some(out) = stdout {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                lines.push(line);
            }
        }
        lines
    });
    let mut compiled = 0usize;
    let mut tail: Vec<String> = Vec::new();
    if let Some(err) = child.stderr.take() {
        for line in BufReader::new(err).lines().map_while(Result::ok) {
            let trimmed = line.trim();
            if let Some(name) = trimmed.strip_prefix("Compiling ") {
                compiled += 1;
                let pct = 8 + (compiled * 87 / crates).min(87) as u16;
                progress(pct, format!("Building {}", name.split_whitespace().next().unwrap_or("")));
            }
            tail.push(line);
            if tail.len() > 40 {
                tail.remove(0);
            }
        }
    }
    let status = child.wait()?;
    let out_lines = drain.join().unwrap_or_default();
    if !status.success() {
        let mut all = out_lines;
        all.extend(tail);
        let errors: Vec<&String> = all.iter().filter(|l| l.contains("error")).collect();
        let shown: Vec<&String> = if errors.is_empty() { all.iter().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect() } else { errors.into_iter().take(8).collect() };
        return Err(anyhow!(
            "The rebuild failed. Run ./run-linux.sh --update in {} to see the details.\n{}",
            repo.display(),
            shown.iter().map(|l| l.as_str()).collect::<Vec<_>>().join("\n")
        ));
    }

    progress(100, "Restarting".into());
    let mut args = vec![repo.join("run-linux.sh").to_string_lossy().into_owned()];
    if web {
        args.push("--web".into());
    }
    // Keep the options it was started with (--port, --data, ...).
    args.extend(std::env::args().skip(1).filter(|a| a != "--web" && a != "--desktop"));
    restart_later(Path::new("sh"), &args)?;
    quit_soon();
    Ok(InstallOutcome {
        message: format!("Updated to {}. Commander Lab will restart in a moment.", info.latest_version),
        apk: None,
        restarting: true,
    })
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
        assert_eq!(asset_for("web", &Install::LinuxBinary(PathBuf::new())), "CommanderLab-web-linux-x86_64");
        assert_eq!(asset_for("desktop", &Install::Deb(PathBuf::new())), "CommanderLab-linux-amd64.deb");
        assert_eq!(detect("android"), Install::Android);
    }

    #[test]
    fn newest_changelog_section() {
        let text = "# Changelog\n\n## 0.31.0\n\n- New\n\n## 0.30.0\n\n- Old\n";
        assert_eq!(top_changelog_section(text), "## 0.31.0\n\n- New\n");
    }
}
