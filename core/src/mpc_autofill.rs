//! Running the MPC Autofill desktop tool (github.com/chilli-axe/mpc-autofill)
//! on a deck's order: it opens a browser, creates the project on
//! MakePlayingCards (or PrinterStudio), uploads every card, and leaves the
//! project ready to review and add to the cart.
//!
//! The tool is downloaded from its GitHub release on first use and run in a
//! terminal window, because it asks questions there (new project or add to
//! an existing one) and waits for Enter when it's done. Desktop only.

use anyhow::{anyhow, bail, Context, Result};
use reqwest::blocking::Client;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

/// The release Commander Lab runs. Newer releases may change their command
/// line, so updates are deliberate.
pub const TOOL_VERSION: &str = "v4.7";
const TOOLS_DIR: &str = "./data/tools/mpc-autofill";

/// Browsers the tool can drive (one must be installed).
pub const BROWSERS: &[&str] = &["chrome", "edge", "brave"];
/// Sites the tool can fill a project into.
pub const SITES: &[&str] = &["MakePlayingCards", "PrinterStudio", "PrinterStudioUK", "PrinterStudioDE", "PrinterStudioES", "PrinterStudioFR"];

/// The release download for this computer, if the tool runs here.
fn release_asset() -> Option<&'static str> {
    if cfg!(target_os = "android") {
        None
    } else if cfg!(target_os = "windows") {
        Some("autofill-windows.zip")
    } else if cfg!(target_os = "macos") {
        Some(if cfg!(target_arch = "aarch64") { "autofill-macos-arm.zip" } else { "autofill-macos-intel.zip" })
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        Some("autofill-linux.zip")
    } else {
        None
    }
}

pub fn supported() -> bool {
    release_asset().is_some()
}

fn install_dir() -> PathBuf {
    Path::new(TOOLS_DIR).join(TOOL_VERSION)
}

/// The installed tool's executable, if it's been downloaded.
pub fn installed_executable() -> Option<PathBuf> {
    fs::read_dir(install_dir())
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_file() && p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with("autofill")).unwrap_or(false))
        .and_then(|p| fs::canonicalize(p).ok())
}

/// Download and unpack the tool. `progress(done_mb, total_mb)`.
pub fn install(mut progress: impl FnMut(u64, u64)) -> Result<PathBuf> {
    if let Some(exe) = installed_executable() {
        return Ok(exe);
    }
    let asset = release_asset().ok_or_else(|| anyhow!("MPC Autofill doesn't run on this device. Export the order and run it on a computer."))?;
    let url = format!("https://github.com/chilli-axe/mpc-autofill/releases/download/{TOOL_VERSION}/{asset}");
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(None)
        .user_agent(concat!("CommanderLab/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let mut response = client
        .get(&url)
        .send()
        .context("Couldn't reach GitHub to download MPC Autofill")?
        .error_for_status()
        .context("GitHub refused the MPC Autofill download")?;
    let total = response.content_length().unwrap_or(50_000_000);
    let mut bytes = Vec::with_capacity(total as usize);
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = response.read(&mut buf).context("The MPC Autofill download was interrupted")?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        progress(bytes.len() as u64 / 1_000_000, total / 1_000_000);
    }

    let dir = install_dir();
    fs::create_dir_all(&dir)?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("The MPC Autofill download wasn't a valid zip file")?;
    let mut exe = None;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if !file.is_file() {
            continue;
        }
        // Only the tool itself, by its bare file name: nothing from the
        // archive can land outside the tools folder.
        let Some(name) = file.enclosed_name().and_then(|p| p.file_name().map(|n| n.to_owned())) else { continue };
        if !name.to_string_lossy().starts_with("autofill") {
            continue;
        }
        let path = dir.join(&name);
        let mut out = fs::File::create(&path).with_context(|| format!("Could not save {}", path.display()))?;
        std::io::copy(&mut file, &mut out)?;
        out.flush()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        }
        exe = Some(path);
    }
    let exe = exe.ok_or_else(|| anyhow!("The MPC Autofill download didn't contain the tool"))?;
    Ok(fs::canonicalize(&exe).unwrap_or(exe))
}

/// Where a deck's run happens: a folder holding only its order file (the
/// tool processes every XML it finds) and the tool's downloaded images.
pub fn run_dir(deck_file_stem: &str) -> PathBuf {
    Path::new("./exports/mpc-autofill").join(deck_file_stem)
}

/// Start the tool on `xml_path` in a new terminal window.
pub fn launch(exe: &Path, xml_path: &Path, deck_file_stem: &str, site: &str, browser: &str) -> Result<String> {
    if !SITES.contains(&site) {
        bail!("Unknown site {site}");
    }
    if !BROWSERS.contains(&browser) {
        bail!("Unknown browser {browser}");
    }
    let dir = run_dir(deck_file_stem);
    fs::create_dir_all(&dir)?;
    // Only this order: older XMLs in the folder would make the tool ask
    // which to run.
    for entry in fs::read_dir(&dir)?.flatten() {
        if entry.path().extension().and_then(|e| e.to_str()) == Some("xml") {
            let _ = fs::remove_file(entry.path());
        }
    }
    fs::copy(xml_path, dir.join(format!("{deck_file_stem}.xml"))).context("Could not copy the order file")?;
    let dir = fs::canonicalize(&dir)?;

    // Passing options skips the tool's setup questions; it still asks
    // whether to start a new project, and waits for Enter at the end.
    let args: Vec<String> = vec![
        "--directory".into(),
        dir.display().to_string(),
        "--site".into(),
        site.into(),
        "--browser".into(),
        browser.into(),
        "--auto-save".into(),
        "--image-post-processing".into(),
    ];
    open_terminal(exe, &args, &dir)
}

/// Run `exe args` in a visible terminal window. Returns which terminal.
fn open_terminal(exe: &Path, args: &[String], dir: &Path) -> Result<String> {
    #[cfg(target_os = "windows")]
    {
        // `start` opens a new console window; the empty string is its title.
        Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("MPC Autofill")
            .arg(exe)
            .args(args)
            .current_dir(dir)
            .spawn()
            .context("Could not open a console window for MPC Autofill")?;
        return Ok("a console window".into());
    }
    #[cfg(target_os = "macos")]
    {
        // Terminal.app runs .command files.
        let script = dir.join("run-mpc-autofill.command");
        let quoted: Vec<String> = std::iter::once(exe.display().to_string()).chain(args.iter().cloned()).map(|a| shell_quote(&a)).collect();
        fs::write(&script, format!("#!/bin/sh\ncd {}\n{}\n", shell_quote(&dir.display().to_string()), quoted.join(" ")))?;
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755))?;
        }
        Command::new("open").arg("-a").arg("Terminal").arg(&script).spawn().context("Could not open Terminal for MPC Autofill")?;
        return Ok("Terminal".into());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // The user's choice first, then common terminals, each with the flag
        // that runs a command.
        let mut candidates: Vec<(String, Vec<&str>)> = Vec::new();
        if let Ok(term) = std::env::var("TERMINAL") {
            candidates.push((term, vec!["-e"]));
        }
        for (name, flags) in [
            ("konsole", vec!["-e"]),
            ("gnome-terminal", vec!["--"]),
            ("kgx", vec!["--"]),
            ("ptyxis", vec!["--"]),
            ("xfce4-terminal", vec!["-x"]),
            ("kitty", vec![]),
            ("alacritty", vec!["-e"]),
            ("foot", vec![]),
            ("wezterm", vec!["start", "--"]),
            ("x-terminal-emulator", vec!["-e"]),
            ("xterm", vec!["-e"]),
        ] {
            candidates.push((name.to_string(), flags));
        }
        for (term, flags) in candidates {
            let found = Command::new("sh")
                .arg("-c")
                .arg(format!("command -v {}", shell_quote(&term)))
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if !found {
                continue;
            }
            let spawned = Command::new(&term).args(&flags).arg(exe).args(args).current_dir(dir).spawn();
            if spawned.is_ok() {
                return Ok(term);
            }
        }
        bail!(
            "No terminal app was found to run MPC Autofill in. Run it yourself: {} {}",
            exe.display(),
            args.join(" ")
        )
    }
    #[allow(unreachable_code)]
    {
        let _ = (exe, args, dir);
        bail!("MPC Autofill doesn't run on this device.")
    }
}

#[cfg(unix)]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_platforms_have_a_release() {
        if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            assert_eq!(release_asset(), Some("autofill-linux.zip"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn quoting_survives_apostrophes() {
        assert_eq!(shell_quote("Urza's deck"), r"'Urza'\''s deck'");
    }
}
