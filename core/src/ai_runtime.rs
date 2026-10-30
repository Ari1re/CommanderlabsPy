use crate::{
    ai::{AiConfig, AiProvider},
    llama::LlamaRuntime,
    ollama::OllamaRuntime,
};
use anyhow::{anyhow, Result};

pub enum LocalAiRuntime {
    Llama(LlamaRuntime),
    Ollama(OllamaRuntime),
    Api,
}

impl LocalAiRuntime {
    pub fn ensure_running() -> Result<Self> {
        let config = AiConfig::load_or_create();

        let runtime = match config.provider {
            AiProvider::LlamaCpp => {
                Self::Llama(LlamaRuntime::ensure_running())
            }
            AiProvider::Ollama => {
                Self::Ollama(OllamaRuntime::ensure_running())
            }
            AiProvider::Api => Self::Api,
        };

        if runtime.is_ready() {
            Ok(runtime)
        } else {
            Err(anyhow!(
                "{} AI backend is not ready. {}",
                config.provider.label(),
                runtime.status_line(),
            ))
        }
    }

    pub fn is_ready(&self) -> bool {
        match self {
            Self::Llama(runtime) => runtime.is_ready(),
            Self::Ollama(runtime) => runtime.is_ready(),
            Self::Api => true,
        }
    }

    pub fn status_line(&self) -> String {
        match self {
            Self::Llama(runtime) => runtime.status_line(),
            Self::Ollama(runtime) => runtime.status_line(),
            Self::Api => {
                let config = AiConfig::load_or_create();
                format!(
                    "API provider configured • model {} • {}",
                    config.model,
                    config.base_url,
                )
            }
        }
    }

    pub fn shutdown(&mut self) {
        match self {
            Self::Llama(runtime) => runtime.shutdown(),
            Self::Ollama(runtime) => runtime.shutdown(),
            Self::Api => {}
        }
    }
}
