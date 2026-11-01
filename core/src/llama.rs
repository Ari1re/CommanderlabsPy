use crate::{
    ai::{AiConfig, AiProvider},
    logging,
};
use reqwest::blocking::Client;
use std::{
    env,
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub enum LlamaStartupState {
    Disabled,
    BuiltIn,
    ExternalServer(String),
    AlreadyRunning,
    Started,
    InstalledAndStarted,
    NotInstalled,
    Failed(String),
}

impl LlamaStartupState {
    pub fn status_line(&self) -> String {
        match self {
            Self::Disabled => "llama.cpp is not selected.".to_string(),
            Self::BuiltIn => "Built-in AI engine ready.".to_string(),
            Self::ExternalServer(url) => {
                format!("llama.cpp external server ready: {url}")
            }
            Self::AlreadyRunning => {
                "llama.cpp server already running and ready."
                    .to_string()
            }
            Self::Started => {
                "llama.cpp server started and is ready.".to_string()
            }
            Self::InstalledAndStarted => {
                "llama.cpp installed, started, and is ready."
                    .to_string()
            }
            Self::NotInstalled => {
                "llama.cpp is not installed.".to_string()
            }
            Self::Failed(err) => {
                format!("llama.cpp startup failed: {err}")
            }
        }
    }
}

pub struct LlamaRuntime {
    child: Option<Child>,
    state: LlamaStartupState,
    model_status: String,
}

impl LlamaRuntime {
    pub fn ensure_running() -> Self {
        let config = AiConfig::load_or_create();

        if config.provider != AiProvider::LlamaCpp {
            return Self::new(
                None,
                LlamaStartupState::Disabled,
                "llama.cpp not selected in Settings".to_string(),
            );
        }

        if !config.enabled {
            return Self::new(
                None,
                LlamaStartupState::Disabled,
                "local AI disabled".to_string(),
            );
        }

        if config.uses_built_in() {
            let Some(preset) = crate::local_llm::resolve_installed(&config.model) else {
                return Self::new(
                    None,
                    LlamaStartupState::NotInstalled,
                    "no model downloaded yet (Settings → AI model)".to_string(),
                );
            };
            // Load now so the first answer doesn't wait for it.
            return match crate::local_llm::load(preset) {
                Ok(()) => Self::new(
                    None,
                    LlamaStartupState::BuiltIn,
                    format!("model {} ({}) loaded", preset.name, preset.id),
                ),
                Err(err) => Self::new(
                    None,
                    LlamaStartupState::Failed(format!("{err:#}")),
                    "model unavailable".to_string(),
                ),
            };
        }

        println!(
            "Commander Lab AI startup: llama.cpp • {}",
            config.model,
        );

        if !is_local_llama_url(&config.base_url) {
            println!(
                "Waiting for configured llama.cpp endpoint {}...",
                config.base_url,
            );

            if wait_for_server(
                &config.base_url,
                Duration::from_secs(30),
            ) {
                return Self::new(
                    None,
                    LlamaStartupState::ExternalServer(
                        config.base_url.clone(),
                    ),
                    format!("model {} ready", config.model),
                );
            }

            return Self::new(
                None,
                LlamaStartupState::Failed(format!(
                    "configured endpoint {} did not become ready",
                    config.base_url,
                )),
                "model unavailable".to_string(),
            );
        }

        if server_responding(&config.base_url) {
            println!("llama.cpp server is already ready.");

            return Self::new(
                None,
                LlamaStartupState::AlreadyRunning,
                format!("model {} ready", config.model),
            );
        }

        let installed_before =
            detect_llama_command().is_some();

        if !installed_before {
            println!(
                "llama.cpp was not found. Installing it before Commander Lab starts..."
            );

            match install_llama_if_supported() {
                Ok(true) => {}
                Ok(false) => {
                    return Self::new(
                        None,
                        LlamaStartupState::NotInstalled,
                        "model unavailable".to_string(),
                    );
                }
                Err(err) => {
                    return Self::new(
                        None,
                        LlamaStartupState::Failed(err),
                        "model unavailable".to_string(),
                    );
                }
            }
        }

        let command =
            wait_for_llama_command(Duration::from_secs(30));

        let Some(command) = command else {
            return Self::new(
                None,
                LlamaStartupState::Failed(
                    "llama.cpp installed, but the llama executable could not be located in PATH or the WinGet links directory."
                        .to_string(),
                ),
                "model unavailable".to_string(),
            );
        };

        println!(
            "Starting llama.cpp server. On first launch the selected model may need to download."
        );
        println!(
            "Commander Lab will wait for the AI server before opening the TUI."
        );

        match spawn_llama_server(
            &command,
            &config.model,
            &config.base_url,
        ) {
            Ok(mut child) => {
                let startup_output_visible =
                    Arc::new(AtomicBool::new(true));

                if let Some(stdout) = child.stdout.take() {
                    drain_llama_output(
                        stdout,
                        startup_output_visible.clone(),
                        false,
                    );
                }

                if let Some(stderr) = child.stderr.take() {
                    drain_llama_output(
                        stderr,
                        startup_output_visible.clone(),
                        true,
                    );
                }

                let timeout = Duration::from_secs(60 * 30);
                let started = Instant::now();
                let mut last_print = 0u64;

                while started.elapsed() < timeout {
                    if server_responding(&config.base_url) {
                        startup_output_visible.store(
                            false,
                            Ordering::Release,
                        );

                        println!(
                            "llama.cpp ready after {}s. Starting Commander Lab...",
                            started.elapsed().as_secs(),
                        );

                        return Self::new(
                            Some(child),
                            if installed_before {
                                LlamaStartupState::Started
                            } else {
                                LlamaStartupState::InstalledAndStarted
                            },
                            format!("model {} ready", config.model),
                        );
                    }

                    if let Ok(Some(status)) = child.try_wait() {
                        startup_output_visible.store(
                            false,
                            Ordering::Release,
                        );
                        println!();

                        return Self::new(
                            None,
                            LlamaStartupState::Failed(format!(
                                "llama.cpp server exited early with {status}"
                            )),
                            "model unavailable".to_string(),
                        );
                    }

                    let seconds = started.elapsed().as_secs();

                    if seconds >= last_print.saturating_add(30) {
                        println!(
                            "Commander Lab is still waiting for llama.cpp readiness... {}s",
                            seconds,
                        );
                        last_print = seconds;
                    }

                    thread::sleep(Duration::from_millis(400));
                }

                startup_output_visible.store(
                    false,
                    Ordering::Release,
                );
                println!();
                let _ = child.kill();
                let _ = child.wait();

                Self::new(
                    None,
                    LlamaStartupState::Failed(
                        "llama.cpp did not become ready within 30 minutes."
                            .to_string(),
                    ),
                    "model unavailable".to_string(),
                )
            }

            Err(err) => Self::new(
                None,
                LlamaStartupState::Failed(err),
                "model unavailable".to_string(),
            ),
        }
    }

    fn new(
        child: Option<Child>,
        state: LlamaStartupState,
        model_status: String,
    ) -> Self {
        logging::record("LLAMA", state.status_line());
        logging::record(
            "LLAMA",
            format!("MODEL • {model_status}"),
        );

        Self {
            child,
            state,
            model_status,
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(
            self.state,
            LlamaStartupState::ExternalServer(_)
                | LlamaStartupState::BuiltIn
                | LlamaStartupState::AlreadyRunning
                | LlamaStartupState::Started
                | LlamaStartupState::InstalledAndStarted
        )
    }

    pub fn status_line(&self) -> String {
        format!(
            "{} • {}",
            self.state.status_line(),
            self.model_status,
        )
    }

    pub fn shutdown(&mut self) {
        if matches!(self.state, LlamaStartupState::BuiltIn) {
            // Give the model's memory back.
            crate::local_llm::unload();
        }
        let Some(mut child) = self.child.take() else {
            return;
        };

        logging::record(
            "LLAMA",
            "SHUTDOWN • stopping llama.cpp server launched by Commander Lab",
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
    }
}

impl Drop for LlamaRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Debug, Clone)]
enum LlamaCommand {
    Unified(PathBuf),
    Server(PathBuf),
}

pub(crate) fn is_local_llama_url(url: &str) -> bool {
    let lower = url
        .trim()
        .trim_end_matches('/')
        .to_ascii_lowercase();

    lower == "http://127.0.0.1:8080"
        || lower == "http://localhost:8080"
}

fn server_responding(base_url: &str) -> bool {
    let Ok(client) = Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
    else {
        return false;
    };

    let health = format!(
        "{}/health",
        base_url.trim_end_matches('/'),
    );

    if client
        .get(&health)
        .send()
        .ok()
        .and_then(|response| response.error_for_status().ok())
        .is_some()
    {
        return true;
    }

    let models = format!(
        "{}/v1/models",
        base_url.trim_end_matches('/'),
    );

    client
        .get(models)
        .send()
        .ok()
        .and_then(|response| response.error_for_status().ok())
        .is_some()
}

fn wait_for_server(
    base_url: &str,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        if server_responding(base_url) {
            return true;
        }

        thread::sleep(Duration::from_millis(400));
    }

    false
}

fn command_works(
    path: &PathBuf,
    args: &[&str],
) -> bool {
    Command::new(path)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn detect_llama_command() -> Option<LlamaCommand> {
    let unified = llama_executable_candidates("llama");

    for path in unified {
        if command_works(&path, &["--version"]) {
            return Some(LlamaCommand::Unified(path));
        }
    }

    let legacy = llama_executable_candidates("llama-server");

    for path in legacy {
        if command_works(&path, &["--version"]) {
            return Some(LlamaCommand::Server(path));
        }
    }

    None
}

fn llama_executable_candidates(name: &str) -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from(name)];

    #[cfg(windows)]
    {
        if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
            let base = PathBuf::from(local_app_data);

            candidates.push(
                base.join("Microsoft")
                    .join("WinGet")
                    .join("Links")
                    .join(format!("{name}.exe")),
            );

            candidates.push(
                base.join("Microsoft")
                    .join("WindowsApps")
                    .join(format!("{name}.exe")),
            );
        }
    }

    candidates
}

fn wait_for_llama_command(
    timeout: Duration,
) -> Option<LlamaCommand> {
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        if let Some(command) = detect_llama_command() {
            return Some(command);
        }

        thread::sleep(Duration::from_millis(500));
    }

    None
}

fn spawn_llama_server(
    command: &LlamaCommand,
    model: &str,
    base_url: &str,
) -> Result<Child, String> {
    let port = base_url
        .trim_end_matches('/')
        .rsplit(':')
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8080);

    let mut process = match command {
        LlamaCommand::Unified(path) => {
            let mut process = Command::new(path);
            process.arg("serve");
            process
        }
        LlamaCommand::Server(path) => Command::new(path),
    };

    process
        .args([
            "-hf",
            model,
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "-c",
            "16384",
            "-ngl",
            "99",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    process
        .spawn()
        .map_err(|err| {
            format!("Could not start llama.cpp server: {err}")
        })
}

fn drain_llama_output<R>(
    reader: R,
    visible: Arc<AtomicBool>,
    stderr: bool,
) where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let reader = BufReader::new(reader);

        for line in reader.lines() {
            let Ok(line) = line else {
                break;
            };

            // Always consume the pipe so llama.cpp can never block on a
            // full stdout/stderr buffer. Only mirror it to the terminal
            // during pre-TUI startup.
            if visible.load(Ordering::Acquire) {
                if stderr {
                    eprintln!("{line}");
                } else {
                    println!("{line}");
                }
            }
        }
    });
}

#[cfg(windows)]
fn install_llama_if_supported() -> Result<bool, String> {
    let winget_available = Command::new("winget")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);

    if !winget_available {
        return Ok(false);
    }

    logging::record(
        "LLAMA",
        "INSTALL • winget install llama.cpp",
    );

    let status = Command::new("winget")
        .args([
            "install",
            "llama.cpp",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--silent",
        ])
        .status()
        .map_err(|err| {
            format!("Could not launch winget for llama.cpp: {err}")
        })?;

    if status.success() {
        Ok(true)
    } else {
        Err(format!(
            "winget llama.cpp installation exited with {status}"
        ))
    }
}

#[cfg(not(windows))]
fn install_llama_if_supported() -> Result<bool, String> {
    Ok(false)
}
