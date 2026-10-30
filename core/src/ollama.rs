use crate::{
    ai::{AiConfig, AiProvider},
    logging,
};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::{
    env,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const OFFICIAL_WINDOWS_INSTALL_SCRIPT: &str =
    "https://ollama.com/install.ps1";

#[derive(Debug, Clone)]
pub enum OllamaStartupState {
    Disabled,
    ExternalServer(String),
    AlreadyRunning,
    Started(PathBuf),
    InstalledAndStarted(PathBuf),
    NotInstalled,
    Failed(String),
}

impl OllamaStartupState {
    pub fn status_line(&self) -> String {
        match self {
            Self::Disabled =>
                "Local AI disabled in data/ai_config.json.".to_string(),
            Self::ExternalServer(url) =>
                format!("AI server configured externally: {url}"),
            Self::AlreadyRunning =>
                "Ollama already running; Commander Lab will leave it running on exit."
                    .to_string(),
            Self::Started(path) =>
                format!(
                    "Ollama started automatically: {}",
                    path.display(),
                ),
            Self::InstalledAndStarted(path) =>
                format!(
                    "Ollama installed and started automatically: {}",
                    path.display(),
                ),
            Self::NotInstalled =>
                "Ollama is not installed and automatic installation is unavailable on this platform."
                    .to_string(),
            Self::Failed(err) =>
                format!(
                    "Could not prepare Ollama automatically: {err}"
                ),
        }
    }
}

pub struct OllamaRuntime {
    child: Option<Child>,
    managed_background_server: bool,
    state: OllamaStartupState,
    model_status: String,
}

impl OllamaRuntime {
    pub fn ensure_running() -> Self {
        let config = AiConfig::load_or_create();

        if config.provider != AiProvider::Ollama {
            return Self::new(
                None,
                false,
                OllamaStartupState::Disabled,
                "Ollama not selected in Settings".to_string(),
            );
        }

        if !config.enabled {
            return Self::new(
                None,
                false,
                OllamaStartupState::Disabled,
                "model not required".to_string(),
            );
        }

        if !is_local_ollama_url(&config.base_url) {
            return Self::new(
                None,
                false,
                OllamaStartupState::ExternalServer(
                    config.base_url.clone(),
                ),
                format!("configured model {}", config.model),
            );
        }

        println!("Commander Lab AI setup: checking Ollama...");

        if server_responding(&config.base_url) {
            println!("Ollama server is already running.");

            let model_status =
                ensure_model_available(
                    &config.base_url,
                    &config.model,
                );

            return Self::new(
                None,
                false,
                OllamaStartupState::AlreadyRunning,
                model_status,
            );
        }

        let mut installed_now = false;
        let mut executable = find_ollama_executable();

        if executable.is_none() {
            println!(
                "Ollama was not found. Commander Lab will install it from Ollama's official Windows installer script."
            );

            match install_ollama_if_supported() {
                Ok(true) => {
                    installed_now = true;
                    executable = wait_for_ollama_executable(
                        Duration::from_secs(20),
                    );
                }
                Ok(false) => {
                    return Self::new(
                        None,
                        false,
                        OllamaStartupState::NotInstalled,
                        "model unavailable".to_string(),
                    );
                }
                Err(err) => {
                    return Self::new(
                        None,
                        false,
                        OllamaStartupState::Failed(
                            format!(
                                "automatic Ollama installation failed: {err}"
                            ),
                        ),
                        "model unavailable".to_string(),
                    );
                }
            }
        }

        let Some(executable) = executable else {
            return Self::new(
                None,
                false,
                OllamaStartupState::Failed(
                    "Ollama installer finished but ollama.exe could not be located."
                        .to_string(),
                ),
                "model unavailable".to_string(),
            );
        };

        // The Windows installer may start Ollama; recheck before spawning a server.
        if installed_now && server_responding(&config.base_url) {
            println!("Ollama installer started the local server.");

            let model_status =
                ensure_model_available(
                    &config.base_url,
                    &config.model,
                );

            return Self::new(
                None,
                true,
                OllamaStartupState::InstalledAndStarted(
                    executable,
                ),
                model_status,
            );
        }

        println!(
            "Starting Ollama server from {}...",
            executable.display(),
        );

        match spawn_ollama_server(&executable) {
            Ok(mut child) => {
                let deadline =
                    Instant::now() + Duration::from_secs(15);

                loop {
                    if server_responding(&config.base_url) {
                        println!("Ollama server is ready.");

                        let model_status =
                            ensure_model_available(
                                &config.base_url,
                                &config.model,
                            );

                        let state = if installed_now {
                            OllamaStartupState::InstalledAndStarted(
                                executable.clone(),
                            )
                        } else {
                            OllamaStartupState::Started(
                                executable.clone(),
                            )
                        };

                        return Self::new(
                            Some(child),
                            false,
                            state,
                            model_status,
                        );
                    }

                    if let Ok(Some(status)) = child.try_wait() {
                        // On Windows the Ollama desktop app / installer can
                        // race our explicit `ollama serve`: our child loses
                        // the port bind and exits with code 1 even though a
                        // perfectly usable server appears milliseconds later.
                        //
                        // Do not fail immediately. Give the localhost API a
                        // short grace period and accept the responding server
                        // if another Ollama process took ownership.
                        let grace_deadline =
                            Instant::now() + Duration::from_secs(5);

                        while Instant::now() < grace_deadline {
                            if server_responding(&config.base_url) {
                                println!(
                                    "Ollama's explicit server process exited with {status}, but the local Ollama API is responding. Using the active background server."
                                );

                                logging::record(
                                    "OLLAMA",
                                    format!(
                                        "SERVE RACE RECOVERED • child_status={} • api={} • installed_now={}",
                                        status,
                                        config.base_url,
                                        installed_now,
                                    ),
                                );

                                let model_status =
                                    ensure_model_available(
                                        &config.base_url,
                                        &config.model,
                                    );

                                let state = if installed_now {
                                    OllamaStartupState::InstalledAndStarted(
                                        executable.clone(),
                                    )
                                } else {
                                    OllamaStartupState::AlreadyRunning
                                };

                                return Self::new(
                                    None,
                                    installed_now,
                                    state,
                                    model_status,
                                );
                            }

                            thread::sleep(
                                Duration::from_millis(250),
                            );
                        }

                        return Self::new(
                            None,
                            false,
                            OllamaStartupState::Failed(
                                format!(
                                    "ollama serve exited early with {status}; localhost API still did not respond after a 5-second recovery probe"
                                ),
                            ),
                            "model unavailable".to_string(),
                        );
                    }

                    if Instant::now() >= deadline {
                        // One final probe catches a server that became ready
                        // at the same instant our explicit child timed out.
                        if server_responding(&config.base_url) {
                            let model_status =
                                ensure_model_available(
                                    &config.base_url,
                                    &config.model,
                                );

                            return Self::new(
                                Some(child),
                                false,
                                if installed_now {
                                    OllamaStartupState::InstalledAndStarted(
                                        executable.clone(),
                                    )
                                } else {
                                    OllamaStartupState::Started(
                                        executable.clone(),
                                    )
                                },
                                model_status,
                            );
                        }

                        let _ = child.kill();
                        let _ = child.wait();

                        return Self::new(
                            None,
                            false,
                            OllamaStartupState::Failed(
                                format!(
                                    "server did not respond at {} within 15 seconds. Try running `ollama serve` once manually to see Ollama's own startup error.",
                                    config.base_url,
                                ),
                            ),
                            "model unavailable".to_string(),
                        );
                    }

                    thread::sleep(Duration::from_millis(250));
                }
            }
            Err(err) => Self::new(
                None,
                false,
                OllamaStartupState::Failed(err.to_string()),
                "model unavailable".to_string(),
            ),
        }
    }

    fn new(
        child: Option<Child>,
        managed_background_server: bool,
        state: OllamaStartupState,
        model_status: String,
    ) -> Self {
        logging::record("OLLAMA", state.status_line());
        logging::record(
            "OLLAMA",
            format!("MODEL • {model_status}"),
        );

        Self {
            child,
            managed_background_server,
            state,
            model_status,
        }
    }

    pub fn is_ready(&self) -> bool {
        let server_ready = matches!(
            self.state,
            OllamaStartupState::ExternalServer(_)
                | OllamaStartupState::AlreadyRunning
                | OllamaStartupState::Started(_)
                | OllamaStartupState::InstalledAndStarted(_)
        );

        server_ready && self.model_status.contains("ready")
    }

    pub fn status_line(&self) -> String {
        format!(
            "{} • {}",
            self.state.status_line(),
            self.model_status,
        )
    }

    pub fn shutdown(&mut self) {
        if self.child.is_none()
            && self.managed_background_server
        {
            logging::record(
                "OLLAMA",
                "SHUTDOWN • stopping installer-started Ollama process tree owned by Commander Lab",
            );

            #[cfg(windows)]
            {
                let _ = Command::new("taskkill")
                    .args([
                        "/IM",
                        "ollama.exe",
                        "/T",
                        "/F",
                    ])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }

            self.managed_background_server = false;

            logging::record(
                "OLLAMA",
                "SHUTDOWN COMPLETE • installer-started Ollama stopped",
            );

            return;
        }

        let Some(mut child) = self.child.take() else {
            return;
        };

        logging::record(
            "OLLAMA",
            "SHUTDOWN • stopping Ollama server launched by Commander Lab",
        );

        #[cfg(windows)]
        {
            let pid = child.id();

            let _ = Command::new("taskkill")
                .args([
                    "/PID",
                    &pid.to_string(),
                    "/T",
                    "/F",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }

        #[cfg(not(windows))]
        {
            let _ = child.kill();
        }

        let _ = child.wait();

        logging::record(
            "OLLAMA",
            "SHUTDOWN COMPLETE • managed Ollama process tree stopped",
        );
    }
}

impl Drop for OllamaRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn is_local_ollama_url(url: &str) -> bool {
    let lower = url
        .trim()
        .trim_end_matches('/')
        .to_ascii_lowercase();

    lower == "http://127.0.0.1:11434"
        || lower == "http://localhost:11434"
}

fn server_responding(base_url: &str) -> bool {
    let Ok(client) = Client::builder()
        .timeout(Duration::from_millis(900))
        .build()
    else {
        return false;
    };

    let url = format!(
        "{}/api/tags",
        base_url.trim_end_matches('/'),
    );

    client
        .get(url)
        .send()
        .ok()
        .and_then(|response| {
            response.error_for_status().ok()
        })
        .is_some()
}

fn model_available(
    base_url: &str,
    requested_model: &str,
) -> bool {
    let Ok(client) = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    else {
        return false;
    };

    let url = format!(
        "{}/api/tags",
        base_url.trim_end_matches('/'),
    );

    let Ok(value) = client
        .get(url)
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.json::<Value>())
    else {
        return false;
    };

    let requested_base = requested_model
        .split(':')
        .next()
        .unwrap_or(requested_model);

    value
        .get("models")
        .and_then(Value::as_array)
        .map(|models| {
            models.iter().any(|entry| {
                let Some(name) =
                    entry.get("name").and_then(Value::as_str)
                else {
                    return false;
                };

                name.eq_ignore_ascii_case(requested_model)
                    || name
                        .split(':')
                        .next()
                        .unwrap_or(name)
                        .eq_ignore_ascii_case(requested_base)
            })
        })
        .unwrap_or(false)
}

fn ensure_model_available(
    base_url: &str,
    model: &str,
) -> String {
    if model_available(base_url, model) {
        println!("AI model '{model}' is already installed.");
        return format!("model {model} ready");
    }

    println!(
        "AI model '{model}' is not installed. Downloading it now; this can take several minutes..."
    );

    match pull_model_with_progress(base_url, model) {
        Ok(()) => {
            println!("AI model '{model}' is ready.");
            format!("model {model} ready")
        }
        Err(err) => {
            eprintln!(
                "Could not download AI model '{model}': {err}"
            );
            format!(
                "model {model} missing; deterministic AI fallback available"
            )
        }
    }
}

fn pull_model_with_progress(
    base_url: &str,
    model: &str,
) -> Result<(), String> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60 * 60 * 4))
        .build()
        .map_err(|err| err.to_string())?;

    let url = format!(
        "{}/api/pull",
        base_url.trim_end_matches('/'),
    );

    let response = client
        .post(url)
        .json(&json!({
            "name": model,
            "stream": true,
        }))
        .send()
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?;

    let mut reader = BufReader::new(response);
    let mut line = String::new();
    let mut last_percent = None::<u64>;

    loop {
        line.clear();

        let read = reader
            .read_line(&mut line)
            .map_err(|err| err.to_string())?;

        if read == 0 {
            break;
        }

        let Ok(value) =
            serde_json::from_str::<Value>(line.trim())
        else {
            continue;
        };

        if let Some(error) =
            value.get("error").and_then(Value::as_str)
        {
            return Err(error.to_string());
        }

        let status = value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("downloading");

        let completed = value
            .get("completed")
            .and_then(Value::as_u64);
        let total =
            value.get("total").and_then(Value::as_u64);

        if let (Some(completed), Some(total)) =
            (completed, total)
        {
            if total > 0 {
                let pct =
                    completed.saturating_mul(100) / total;

                if last_percent != Some(pct) {
                    print!(
                        "\rOllama model download: {:>3}% • {}",
                        pct,
                        status,
                    );
                    let _ = std::io::stdout().flush();
                    last_percent = Some(pct);
                }
            }
        } else if status == "success" {
            println!("\rOllama model download: 100% • success");
        }
    }

    if model_available(base_url, model) {
        Ok(())
    } else {
        Err(
            "Ollama pull finished but the requested model was not listed by /api/tags."
                .to_string(),
        )
    }
}

#[cfg(windows)]
fn install_ollama_if_supported() -> Result<bool, String> {
    println!(
        "Installing Ollama using the official script: {OFFICIAL_WINDOWS_INSTALL_SCRIPT}"
    );

    logging::record(
        "OLLAMA",
        format!(
            "INSTALL START • source={OFFICIAL_WINDOWS_INSTALL_SCRIPT}"
        ),
    );

    let command = format!(
        "irm '{}' | iex",
        OFFICIAL_WINDOWS_INSTALL_SCRIPT,
    );

    let status = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &command,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|err| err.to_string())?;

    if !status.success() {
        return Err(format!(
            "official Ollama installer returned {status}"
        ));
    }

    logging::record(
        "OLLAMA",
        "INSTALL COMPLETE • official PowerShell installer returned success",
    );

    Ok(true)
}

#[cfg(not(windows))]
fn install_ollama_if_supported() -> Result<bool, String> {
    Ok(false)
}

fn wait_for_ollama_executable(
    timeout: Duration,
) -> Option<PathBuf> {
    let deadline = Instant::now() + timeout;

    loop {
        if let Some(path) = find_ollama_executable() {
            return Some(path);
        }

        if Instant::now() >= deadline {
            return None;
        }

        thread::sleep(Duration::from_millis(500));
    }
}

fn find_ollama_executable() -> Option<PathBuf> {
    if let Some(path) = find_in_path() {
        return Some(path);
    }

    let mut candidates = Vec::<PathBuf>::new();

    if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
        let base = PathBuf::from(local_app_data);

        candidates.push(
            base.join("Programs")
                .join("Ollama")
                .join("ollama.exe"),
        );

        candidates.push(
            base.join("Ollama")
                .join("ollama.exe"),
        );
    }

    if let Ok(program_files) = env::var("ProgramFiles") {
        candidates.push(
            PathBuf::from(program_files)
                .join("Ollama")
                .join("ollama.exe"),
        );
    }

    if let Ok(program_files_x86) =
        env::var("ProgramFiles(x86)")
    {
        candidates.push(
            PathBuf::from(program_files_x86)
                .join("Ollama")
                .join("ollama.exe"),
        );
    }

    candidates.into_iter().find(|path| path.is_file())
}

fn find_in_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let output = Command::new("where.exe")
            .arg("ollama.exe")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout =
            String::from_utf8_lossy(&output.stdout);

        return stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .find(|path| path.is_file());
    }

    #[cfg(not(windows))]
    {
        let output = Command::new("which")
            .arg("ollama")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let path = PathBuf::from(
            String::from_utf8_lossy(&output.stdout)
                .trim(),
        );

        path.is_file().then_some(path)
    }
}

fn spawn_ollama_server(
    executable: &Path,
) -> std::io::Result<Child> {
    let mut command = Command::new(executable);

    command
        .arg("serve")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x08000000;

        command.creation_flags(CREATE_NO_WINDOW);
    }

    command.spawn()
}
