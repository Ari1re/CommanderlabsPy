use crate::{
    data::{CardDb},
    deck::ParsedDeck,
    deep_agent::OptimizationGoal,
    deep_learn::DeepLearnReport,
    logging,
    sources::DeckSource,
    web_research::{research_commander_web, WebResearchBundle},
};
use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    path::Path,
    time::Duration,
};

const CONFIG_PATH: &str = "./data/ai_config.json";
const LLAMA_BASE_URL: &str = "http://127.0.0.1:8080";
const OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434";
const LLAMA_MODEL: &str = "Qwen/Qwen2.5-1.5B-Instruct-GGUF:Q4_K_M";
const OLLAMA_MODEL: &str = "qwen2.5:7b-instruct";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProvider {
    LlamaCpp,
    Ollama,
    Api,
}

impl Default for AiProvider {
    fn default() -> Self {
        Self::LlamaCpp
    }
}

impl AiProvider {
    pub fn label(self) -> &'static str {
        match self {
            Self::LlamaCpp => "LLAMA.CPP",
            Self::Ollama => "OLLAMA",
            Self::Api => "API",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::LlamaCpp => Self::Ollama,
            Self::Ollama => Self::Api,
            Self::Api => Self::LlamaCpp,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AiModelPreset {
    pub name: &'static str,
    pub model: &'static str,
    pub description: &'static str,
}

/// The built-in engine's downloadable models (see [`crate::local_llm::PRESETS`]).
const LLAMA_MODELS: [AiModelPreset; 3] = [
    AiModelPreset {
        name: "PHONE",
        model: "Qwen/Qwen2.5-1.5B-Instruct-GGUF:Q4_K_M",
        description: "1.5B • recommended • runs on most phones",
    },
    AiModelPreset {
        name: "LIGHT",
        model: "Qwen/Qwen2.5-0.5B-Instruct-GGUF:Q4_K_M",
        description: "0.5B • smallest and fastest • older phones",
    },
    AiModelPreset {
        name: "BALANCED",
        model: "Qwen/Qwen2.5-3B-Instruct-GGUF:Q4_K_M",
        description: "3B • better answers • computers and newer phones",
    },
];

const OLLAMA_MODELS: [AiModelPreset; 4] = [
    AiModelPreset {
        name: "FAST",
        model: "qwen2.5:3b",
        description: "3B • fastest • lower memory",
    },
    AiModelPreset {
        name: "BALANCED",
        model: "qwen2.5:7b-instruct",
        description: "7B • recommended default",
    },
    AiModelPreset {
        name: "QUALITY",
        model: "qwen2.5:14b-instruct",
        description: "14B • stronger reasoning • more VRAM/RAM",
    },
    AiModelPreset {
        name: "MAX",
        model: "qwen2.5:32b-instruct",
        description: "32B • strongest local preset • heavy",
    },
];

pub fn model_presets(provider: AiProvider) -> &'static [AiModelPreset] {
    match provider {
        AiProvider::LlamaCpp => &LLAMA_MODELS,
        AiProvider::Ollama => &OLLAMA_MODELS,
        AiProvider::Api => &[],
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub provider: AiProvider,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_web_research")]
    pub web_research: bool,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_api_base_url")]
    pub api_base_url: String,
    #[serde(default = "default_api_model")]
    pub api_model: String,
}

fn default_enabled() -> bool { true }
fn default_base_url() -> String { LLAMA_BASE_URL.to_string() }
fn default_model() -> String { LLAMA_MODEL.to_string() }
fn default_temperature() -> f64 { 0.15 }
fn default_timeout() -> u64 { 180 }
fn default_web_research() -> bool { true }
fn default_api_base_url() -> String {
    "https://api.openai.com".to_string()
}
fn default_api_model() -> String {
    "gpt-4.1-mini".to_string()
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: AiProvider::LlamaCpp,
            base_url: LLAMA_BASE_URL.to_string(),
            model: LLAMA_MODEL.to_string(),
            temperature: default_temperature(),
            timeout_seconds: default_timeout(),
            web_research: true,
            api_key: String::new(),
            api_base_url: default_api_base_url(),
            api_model: default_api_model(),
        }
    }
}

impl AiConfig {
    pub fn load_or_create() -> Self {
        if let Ok(text) = fs::read_to_string(CONFIG_PATH) {
            if let Ok(value) = serde_json::from_str::<Value>(&text) {
                if value.get("provider").is_some() {
                    if let Ok(config) = serde_json::from_value::<Self>(value) {
                        return config;
                    }
                } else if let Ok(old) = serde_json::from_value::<Self>(value) {
                    // Pre-provider configs were Ollama-only. Migrate them to
                    // the new llama.cpp default rather than silently starting
                    // or downloading Ollama.
                    let mut config = old;
                    config.provider = AiProvider::LlamaCpp;
                    config.base_url = LLAMA_BASE_URL.to_string();
                    config.model = LLAMA_MODEL.to_string();
                    let _ = config.save();
                    return config;
                }
            }
        }

        let config = Self::default();
        let _ = config.save();
        config
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = Path::new(CONFIG_PATH).parent() {
            fs::create_dir_all(parent)
                .context("Could not create AI config directory")?;
        }

        fs::write(
            CONFIG_PATH,
            serde_json::to_string_pretty(self)?,
        )
        .context("Could not save AI configuration")?;

        Ok(())
    }

    pub fn set_provider(&mut self, provider: AiProvider) {
        self.provider = provider;

        match provider {
            AiProvider::LlamaCpp => {
                self.base_url = LLAMA_BASE_URL.to_string();
                self.model = LLAMA_MODEL.to_string();
            }
            AiProvider::Ollama => {
                self.base_url = OLLAMA_BASE_URL.to_string();
                self.model = OLLAMA_MODEL.to_string();
            }
            AiProvider::Api => {
                self.base_url = self.api_base_url.clone();
                self.model = self.api_model.clone();
            }
        }
    }

    /// True when requests go to the engine compiled into the app rather
    /// than a llama.cpp server: the llama.cpp provider at its default local
    /// address, in a build that includes the engine.
    pub fn uses_built_in(&self) -> bool {
        self.provider == AiProvider::LlamaCpp
            && crate::llama::is_local_llama_url(&self.base_url)
            && crate::local_llm::available()
    }

    pub fn cycle_provider(&mut self) {
        self.set_provider(self.provider.next());
    }

    pub fn set_model_preset(&mut self, index: usize) -> bool {
        let Some(preset) = model_presets(self.provider).get(index) else {
            return false;
        };

        self.model = preset.model.to_string();
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiWinPackage {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub cards: Vec<String>,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiCommanderProfile {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub commander_summary: String,
    #[serde(default)]
    pub primary_archetypes: Vec<String>,
    #[serde(default)]
    pub recommended_archetype: String,
    #[serde(default)]
    pub gameplan: Vec<String>,
    #[serde(default)]
    pub win_packages: Vec<AiWinPackage>,
    #[serde(default)]
    pub priority_cards: Vec<String>,
    #[serde(default)]
    pub avoid_or_question_cards: Vec<String>,
    #[serde(default)]
    pub mulligan_priorities: Vec<String>,
    #[serde(default)]
    pub interaction_priorities: Vec<String>,
    #[serde(default)]
    pub mana_notes: Vec<String>,
    #[serde(default)]
    pub community_findings: Vec<String>,
    #[serde(default)]
    pub rules_notes: Vec<String>,
    #[serde(default)]
    pub evidence_notes: Vec<String>,
    #[serde(default)]
    pub testing_requests: Vec<String>,
    #[serde(default)]
    pub feedback_preferences: Vec<String>,
    #[serde(default)]
    pub source_count: usize,
    #[serde(default)]
    pub sources: Vec<DeckSource>,
    #[serde(default)]
    pub fallback_reason: String,
}

impl AiCommanderProfile {
    pub fn fallback(reason: impl Into<String>) -> Self {
        Self {
            enabled: false,
            fallback_reason: reason.into(),
            ..Self::default()
        }
    }
}

pub struct LocalAi {
    config: AiConfig,
    client: Client,
    model: String,
}

fn openai_chat_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');

    // Most services put their OpenAI-compatible API under /v1; Gemini's is
    // under /v1beta/openai.
    if base.ends_with("/v1") || base.ends_with("/openai") {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    }
}

#[cfg(test)]
mod url_tests {
    use super::openai_chat_url;

    #[test]
    fn chat_urls_for_common_services() {
        assert_eq!(openai_chat_url("https://api.openai.com"), "https://api.openai.com/v1/chat/completions");
        assert_eq!(openai_chat_url("https://api.anthropic.com/v1/"), "https://api.anthropic.com/v1/chat/completions");
        assert_eq!(
            openai_chat_url("https://generativelanguage.googleapis.com/v1beta/openai"),
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
        );
        assert_eq!(openai_chat_url("https://api.groq.com/openai/v1"), "https://api.groq.com/openai/v1/chat/completions");
    }
}

fn parse_local_ai_json(content: &str) -> Result<Value> {
    let trimmed = content.trim();

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Ok(value);
    }

    let unfenced = strip_markdown_json_fence(trimmed);

    if let Ok(value) = serde_json::from_str::<Value>(unfenced) {
        return Ok(value);
    }

    if let Some(candidate) = extract_first_json_value(unfenced) {
        if let Ok(value) = serde_json::from_str::<Value>(candidate) {
            return Ok(value);
        }
    }

    Err(anyhow!(
        "Local AI did not return parseable JSON. Response began: {}",
        trimmed.chars().take(320).collect::<String>(),
    ))
}

fn strip_markdown_json_fence(content: &str) -> &str {
    let trimmed = content.trim();

    if !trimmed.starts_with("```") {
        return trimmed;
    }

    let Some(first_newline) = trimmed.find('\n') else {
        return trimmed;
    };

    let body = &trimmed[first_newline + 1..];

    if let Some(end) = body.rfind("```") {
        return body[..end].trim();
    }

    body.trim()
}

fn extract_first_json_value(content: &str) -> Option<&str> {
    let bytes = content.as_bytes();

    for (start, byte) in bytes.iter().enumerate() {
        let (open, close) = match byte {
            b'{' => (b'{', b'}'),
            b'[' => (b'[', b']'),
            _ => continue,
        };

        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;

        for index in start..bytes.len() {
            let current = bytes[index];

            if in_string {
                if escaped {
                    escaped = false;
                    continue;
                }

                match current {
                    b'\\' => escaped = true,
                    b'"' => in_string = false,
                    _ => {}
                }

                continue;
            }

            if current == b'"' {
                in_string = true;
                continue;
            }

            if current == open {
                depth += 1;
                continue;
            }

            if current == close {
                depth = depth.saturating_sub(1);

                if depth == 0 {
                    return content.get(start..=index);
                }
            }
        }
    }

    None
}

impl LocalAi {
    pub fn connect() -> Result<Self> {
        let config = AiConfig::load_or_create();

        if !config.enabled {
            return Err(anyhow!("Local AI is disabled in {CONFIG_PATH}."));
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds.max(15)))
            .build()
            .context("Could not initialize local AI HTTP client")?;

        let available = match config.provider {
            AiProvider::LlamaCpp if config.uses_built_in() => {
                let preset = crate::local_llm::resolve_installed(&config.model).ok_or_else(|| {
                    anyhow!("No AI model is downloaded yet. Download one in Settings → AI model.")
                })?;
                vec![preset.id.to_string()]
            }

            AiProvider::LlamaCpp => {
                let url = format!(
                    "{}/v1/models",
                    config.base_url.trim_end_matches('/'),
                );

                let value = client
                    .get(&url)
                    .send()
                    .context(
                        "Could not connect to llama.cpp. Check the startup status and local AI logs.",
                    )?
                    .error_for_status()
                    .context("llama.cpp rejected the model-list request")?
                    .json::<Value>()
                    .context("Could not parse llama.cpp model list")?;

                value
                    .get("data")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| {
                        entry
                            .get("id")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .collect::<Vec<_>>()
            }

            AiProvider::Ollama => {
                let url = format!(
                    "{}/api/tags",
                    config.base_url.trim_end_matches('/'),
                );

                let value = client
                    .get(&url)
                    .send()
                    .context(
                        "Could not connect to Ollama. Ollama is only started when explicitly selected in Settings.",
                    )?
                    .error_for_status()
                    .context("Ollama rejected the model-list request")?
                    .json::<Value>()
                    .context("Could not parse Ollama model list")?;

                value
                    .get("models")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| {
                        entry
                            .get("name")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .collect::<Vec<_>>()
            }

            AiProvider::Api => {
                vec![config.model.clone()]
            }
        };

        let model = available
            .iter()
            .find(|name| {
                name.eq_ignore_ascii_case(config.model.trim())
            })
            .cloned()
            .or_else(|| available.first().cloned())
            .unwrap_or_else(|| config.model.clone());

        logging::record(
            "AI",
            format!(
                "CONNECTED • provider={} • model={} • base_url={}",
                config.provider.label(),
                model,
                config.base_url,
            ),
        );

        Ok(Self {
            config,
            client,
            model,
        })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn provider(&self) -> AiProvider {
        self.config.provider
    }

    /// A plain chat completion: system prompt plus (role, content) turns.
    pub fn chat(&self, system: &str, turns: &[(String, String)]) -> Result<String> {
        self.chat_messages(system, turns, false)
    }

    /// [`Self::chat`], calling `on_text` with the answer so far as it's
    /// written. Only the built-in engine streams; other providers report
    /// the whole answer once it arrives.
    pub fn chat_streaming(&self, system: &str, turns: &[(String, String)], on_text: &dyn Fn(&str)) -> Result<String> {
        let answer = self.chat_messages_with(system, turns, false, Some(on_text))?;
        on_text(&answer);
        Ok(answer)
    }

    fn chat_messages(
        &self,
        system: &str,
        turns: &[(String, String)],
        json_mode: bool,
    ) -> Result<String> {
        self.chat_messages_with(system, turns, json_mode, None)
    }

    fn chat_messages_with(
        &self,
        system: &str,
        turns: &[(String, String)],
        json_mode: bool,
        on_text: Option<&dyn Fn(&str)>,
    ) -> Result<String> {
        let mut api_messages = vec![json!({
            "role": "system",
            "content": system,
        })];

        for (role, content) in turns {
            let normalized_role = match role.as_str() {
                "assistant" => "assistant",
                "system" => "user",
                _ => "user",
            };

            api_messages.push(json!({
                "role": normalized_role,
                "content": content,
            }));
        }

        match self.config.provider {
            AiProvider::LlamaCpp if self.config.uses_built_in() => {
                let model = crate::local_llm::preset(&self.model)
                    .ok_or_else(|| anyhow!("Unknown built-in AI model {}", self.model))?;
                crate::local_llm::chat(&crate::local_llm::ChatRequest {
                    model,
                    system,
                    turns,
                    temperature: self.config.temperature as f32,
                    json: json_mode,
                    on_text,
                })
            }

            AiProvider::LlamaCpp => {
                let url = openai_chat_url(&self.config.base_url);

                let mut request = json!({
                    "model": self.model,
                    "temperature": self.config.temperature,
                    "messages": api_messages,
                });

                if json_mode {
                    request["response_format"] = json!({
                        "type": "json_object"
                    });
                }

                let response = self
                    .client
                    .post(&url)
                    .json(&request)
                    .send()
                    .context("llama.cpp AI request failed")?
                    .error_for_status()
                    .context("llama.cpp returned an HTTP error")?
                    .json::<Value>()
                    .context("Could not parse llama.cpp response envelope")?;

                response
                    .get("choices")
                    .and_then(Value::as_array)
                    .and_then(|choices| choices.first())
                    .and_then(|choice| choice.get("message"))
                    .and_then(|message| message.get("content"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| {
                        anyhow!(
                            "llama.cpp response did not include choices[0].message.content"
                        )
                    })
            }

            AiProvider::Api => {
                let url = openai_chat_url(&self.config.base_url);

                let request = json!({
                    "model": self.model,
                    "temperature": self.config.temperature,
                    "messages": api_messages,
                });

                // Keep the API path broadly OpenAI-compatible. Some
                // providers reject response_format even though they support
                // chat completions; the grounded prompt plus robust JSON
                // extraction handles structured responses instead.
                let _ = json_mode;

                let mut builder = self
                    .client
                    .post(&url)
                    .json(&request);

                if !self.config.api_key.trim().is_empty() {
                    builder = builder.bearer_auth(
                        self.config.api_key.trim(),
                    );
                }

                let response = builder
                    .send()
                    .context("AI API request failed")?
                    .error_for_status()
                    .context("AI API returned an HTTP error")?
                    .json::<Value>()
                    .context("Could not parse AI API response")?;

                response
                    .get("choices")
                    .and_then(Value::as_array)
                    .and_then(|choices| choices.first())
                    .and_then(|choice| choice.get("message"))
                    .and_then(|message| message.get("content"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| {
                        anyhow!(
                            "AI API response did not include choices[0].message.content"
                        )
                    })
            }

            AiProvider::Ollama => {
                let url = format!(
                    "{}/api/chat",
                    self.config.base_url.trim_end_matches('/'),
                );

                let mut request = json!({
                    "model": self.model,
                    "stream": false,
                    "options": {
                        "temperature": self.config.temperature,
                        "num_ctx": 16384
                    },
                    "messages": api_messages,
                });

                if json_mode {
                    request["format"] = json!("json");
                }

                let response = self
                    .client
                    .post(&url)
                    .json(&request)
                    .send()
                    .context("Ollama AI request failed")?
                    .error_for_status()
                    .context("Ollama returned an HTTP error")?
                    .json::<Value>()
                    .context("Could not parse Ollama response envelope")?;

                response
                    .get("message")
                    .and_then(|message| message.get("content"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| {
                        anyhow!(
                            "Ollama response did not include message.content"
                        )
                    })
            }
        }
    }

    fn chat_content(
        &self,
        system: &str,
        prompt: &str,
        json_mode: bool,
    ) -> Result<String> {
        self.chat_messages(
            system,
            &[("user".to_string(), prompt.to_string())],
            json_mode,
        )
    }

    fn chat_json(
        &self,
        system: &str,
        prompt: &str,
    ) -> Result<Value> {
        let content = self.chat_content(system, prompt, true)?;

        parse_local_ai_json(&content)
    }
}


pub fn build_ai_commander_profile<F>(
    deck: &ParsedDeck,
    db: &CardDb,
    bracket: u8,
    goal: OptimizationGoal,
    preferred_archetype: Option<&str>,
    style_brief: Option<&str>,
    user_references: &[DeckSource],
    research: &DeepLearnReport,
    mut progress: F,
) -> AiCommanderProfile
where
    F: FnMut(String),
{
    progress(
        "[AI] Initializing configured MTG reasoning model.".to_string()
    );

    let ai = match LocalAi::connect() {
        Ok(ai) => ai,
        Err(err) => {
            progress(format!(
                "[AI] AI model unavailable • deterministic optimizer remains active • {err}"
            ));

            return AiCommanderProfile::fallback(format!("{err:#}"));
        }
    };

    progress(format!(
        "[AI] Connected to {} model {}.",
        ai.provider().label(),
        ai.model(),
    ));

    let web = if ai.config.web_research {
        progress(
            "[AI] Gathering cached Reddit/community/rules research."
                .to_string(),
        );

        research_commander_web(
            &research.commander_names,
            bracket,
            goal,
            |line| progress(format!("[AI WEB] {line}")),
        )
        .unwrap_or_default()
    } else {
        WebResearchBundle::default()
    };

    progress(format!(
        "[AI] Building grounded context • reference decks={} • EDHREC signals={} • web sources={}.",
        research.reference_decks,
        research.edhrec_signals,
        web.sources.len(),
    ));

    let context = build_grounded_context(
        deck,
        db,
        bracket,
        goal,
        preferred_archetype,
        style_brief,
        user_references,
        research,
        &web,
    );

    let system = r#"You are Commander Lab's local Magic: The Gathering Commander optimization planner.

You do NOT have authority over the program's deterministic rules, legality, bracket, combo, or simulation engines. Those are authoritative. Your job is strategic synthesis: infer coherent commander gameplans, archetypes, card packages, mulligan priorities, interaction needs, and cards worth testing.

Rules:
- When JSON is requested, return raw JSON only. Do not wrap it in Markdown code fences and do not add prose before or after it.
- Never invent a card, Oracle text, combo, source, statistic, or rule.
- Treat supplied Scryfall/Commander Lab card data as authoritative card text.
- Treat supplied Commander Spellbook results as authoritative combo evidence.
- Treat official/Wizards source excerpts as higher priority than community discussion for rules.
- Treat Reddit/community material as strategic opinion only.
- USER-SUPPLIED HIGH-PRIORITY REFERENCES are intentional exemplars. Analyze what makes those decks play the way they do: engines, packages, mana structure, interaction profile, win conditions, sequencing and card overlap. Use them strongly to guide the requested feel, but do not blindly copy cards that conflict with the current commander, bracket, legality, or explicit user goals.
- Keep the requested Commander bracket as a hard constraint.
- Optimize normal decks for the earliest RELIABLE win, not a single lucky hand.
- If the objective is STAX LOCK, optimize for earliest reliable asymmetric lock; winning afterward is secondary.
- Prefer cards/packages that are individually useful, redundant, mana-efficient, and supported by reference evidence.
- Do not recommend off-color or Commander-illegal cards.
- Read the user's build brief as instructions for both deck construction and evaluation.
- Extract how the user wants the deck tested into testing_requests. Only describe tests Commander Lab can actually support from the supplied deterministic simulation data; if the user asks for an unsupported opponent/meta simulation, flag it as requested feedback rather than pretending it was performed.
- Extract how the user wants results explained into feedback_preferences (for example concise summary, explain every cut, emphasize consistency, discuss mulligans, compare packages).
- Do not expose hidden chain-of-thought. Return concise evidence-backed conclusions only.

Return one JSON object with exactly these keys:
commander_summary: string
primary_archetypes: array of strings
recommended_archetype: string
gameplan: array of strings
win_packages: array of objects {name:string,cards:array of strings,role:string,priority:number 0-100,notes:string}
priority_cards: array of card names
avoid_or_question_cards: array of card names
mulligan_priorities: array of strings
interaction_priorities: array of strings
mana_notes: array of strings
community_findings: array of strings
rules_notes: array of strings
evidence_notes: array of strings
testing_requests: array of strings
feedback_preferences: array of strings
"#;

    progress(
        "[AI] Synthesizing commander strategy and min/max search directions."
            .to_string(),
    );

    let value = match ai.chat_json(system, &context) {
        Ok(value) => value,
        Err(err) => {
            progress(format!(
                "[AI] Strategy synthesis failed • deterministic optimizer continues • {err}"
            ));
            return AiCommanderProfile::fallback(format!("{err:#}"));
        }
    };

    let mut profile =
        match serde_json::from_value::<AiCommanderProfile>(value) {
            Ok(profile) => profile,
            Err(err) => {
                progress(format!(
                    "[AI] Could not decode strategy profile • {err}"
                ));
                return AiCommanderProfile::fallback(format!("{err:#}"));
            }
        };

    profile.enabled = true;
    profile.model = ai.model().to_string();
    profile.sources = web.sources;

    for source in user_references {
        if !profile.sources.iter().any(|existing| {
            existing.url.eq_ignore_ascii_case(&source.url)
                && existing.title.eq_ignore_ascii_case(&source.title)
        }) {
            profile.sources.push(source.clone());
        }
    }

    profile.source_count = profile.sources.len();

    sanitize_profile(deck, db, &mut profile);

    progress(format!(
        "[AI] Strategy ready • archetypes={} • win_packages={} • priority_cards={} • model={}.",
        profile.primary_archetypes.len(),
        profile.win_packages.len(),
        profile.priority_cards.len(),
        profile.model,
    ));

    profile
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentChatMessage {
    pub role: String,
    pub content: String,
}

pub fn chat_with_deck_agent(
    deck: &ParsedDeck,
    db: &CardDb,
    research: Option<&DeepLearnReport>,
    profile: Option<&AiCommanderProfile>,
    style_brief: Option<&str>,
    user_references: &[DeckSource],
    history: &[AgentChatMessage],
    execution_context: &str,
    user_message: &str,
) -> Result<String> {
    let ai = LocalAi::connect()?;

    let commanders = deck
        .commanders
        .iter()
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>();

    let commander_data = commanders
        .iter()
        .filter_map(|name| db.get_card(name).ok().flatten())
        .map(|card| {
            format!(
                "{} | {} | MV {:.1} | colors {} | Oracle: {}",
                card.name,
                card.type_line,
                card.cmc,
                if card.color_identity.is_empty() {
                    "C".to_string()
                } else {
                    card.color_identity
                },
                card.oracle_text.replace('\n', " "),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let decklist = deck
        .main
        .iter()
        .map(|entry| format!("{} {}", entry.quantity, entry.name))
        .collect::<Vec<_>>()
        .join("\n");

    let research_summary = research
        .map(|report| {
            format!(
                "References={} EDHREC={} confidence={} themes={} core={} packages={} combos={} adds={} cuts={}",
                report.reference_decks,
                report.edhrec_signals,
                report.confidence,
                report.normal_themes.join("/"),
                report.reference_core_cards.iter().take(18).cloned().collect::<Vec<_>>().join(", "),
                report.reference_packages.iter().take(12).cloned().collect::<Vec<_>>().join(" | "),
                report.reference_combos.iter().take(12).cloned().collect::<Vec<_>>().join(" | "),
                report.adds.iter().take(12).map(|card| card.name.clone()).collect::<Vec<_>>().join(", "),
                report.cuts.iter().take(12).map(|card| card.name.clone()).collect::<Vec<_>>().join(", "),
            )
        })
        .unwrap_or_else(|| "Deep Learn research has not been run yet.".to_string());

    let ai_summary = profile
        .filter(|profile| profile.enabled)
        .map(|profile| {
            format!(
                "AI commander model: {} | recommended={} | gameplan={} | priority={} | win packages={}",
                profile.commander_summary,
                profile.recommended_archetype,
                profile.gameplan.iter().take(6).cloned().collect::<Vec<_>>().join(" -> "),
                profile.priority_cards.iter().take(18).cloned().collect::<Vec<_>>().join(", "),
                profile.win_packages.iter().take(6).map(|package| {
                    format!("{}: {}", package.name, package.cards.join(" + "))
                }).collect::<Vec<_>>().join(" | "),
            )
        })
        .unwrap_or_else(|| "No completed local-AI commander profile yet.".to_string());

    let user_reference_summary = user_references
        .iter()
        .take(10)
        .map(|source| {
            if let Some(reference_deck) =
                source.reference_deck.as_ref()
            {
                format!(
                    "{} [{}] • {} • commander(s): {} • sample cards: {}",
                    source.title,
                    source.url,
                    source.source,
                    reference_deck
                        .commanders
                        .iter()
                        .map(|entry| entry.name.clone())
                        .collect::<Vec<_>>()
                        .join(" + "),
                    reference_deck
                        .main
                        .iter()
                        .take(30)
                        .map(|entry| entry.name.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            } else {
                format!(
                    "{} [{}] • excerpt: {}",
                    source.title,
                    source.url,
                    source
                        .content_excerpt
                        .chars()
                        .take(1800)
                        .collect::<String>(),
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let recent_history = history
        .iter()
        .filter(|message| {
            message.role == "user" || message.role == "assistant"
        })
        .rev()
        .take(40)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|message| (message.role, message.content))
        .collect::<Vec<_>>();

    let conversational_turns = recent_history
        .iter()
        .filter(|(role, _)| role == "user" || role == "assistant")
        .count();

    let system = r#"You are Commander Lab's deckbuilding partner in a live chat with the user.
Work the way a good pair-programming partner does: listen, ask clarifying questions, summarize what you heard, and refine the plan together before any heavy optimization runs.

Your job in chat:
1. ALWAYS reply to what the user just said. Never ignore their message or only tell them to run a command.
2. Clarify the game plan when anything is ambiguous. Ask targeted follow-up questions (1-3 at a time), such as:
   - target Bracket / power level
   - primary win condition (combo, combat, lock, value grind)
   - what must be preserved vs. what is flexible
   - pod speed / meta if it matters
   - whether they want speed, consistency, interaction, or a specific feel
3. Reflect back a short "here's what I think we're building toward" summary as the plan firms up. Update it when they correct you.
4. Suggest specific cuts/adds/packages as *discussion* — explain tradeoffs. Do not claim the deck was changed unless Commander Lab context says an optimization already ran.
5. When the plan feels clear, offer next steps: "Say run optimization when you want me to apply this" or "Say run simulation with N games to validate."
6. Follow user instructions exactly. Never invent lower caps on simulations or candidate counts — Commander Lab honors their numbers when they say run optimization / run simulation.
7. When they ask for the decklist, output the FULL list in Commander Lab text format:

Commander
1 Card Name

Deck
1 Card Name
...

Use CURRENT DECK from context unless they name a specific saved candidate from AGENT EXECUTION context.

Grounding rules:
- Never invent cards, Oracle text, rules, statistics, sources, or verified combos.
- Scryfall/Commander Lab legality and card text are authoritative.
- Commander Spellbook/reference evidence is authoritative for combo claims.
- Community/Reddit content is opinion only.
- Treat AGENT EXECUTION / PLAYTEST CONTEXT as authoritative for completed work only.

Tone: direct, collaborative, curious. Short paragraphs and bullets. Ask questions when you need information — do not guess the game plan.
"#;

    let context_block = format!(
        r#"COMMANDERS
{commander_data}

USER STYLE
{style}

CURRENT DECK
{decklist}

DEEP LEARN EVIDENCE
{research_summary}

CURRENT AI PROFILE
{ai_summary}

AGENT EXECUTION / CANDIDATE / PLAYTEST CONTEXT
{execution_context}

USER-SUPPLIED REFERENCES
{user_references}
"#,
        commander_data = commander_data,
        style = style_brief.unwrap_or("No explicit style brief yet."),
        decklist = decklist,
        research_summary = research_summary,
        ai_summary = ai_summary,
        execution_context = execution_context,
        user_references = if user_reference_summary.is_empty() {
            "None supplied.".to_string()
        } else {
            user_reference_summary
        },
    );

    let user_payload = if conversational_turns <= 1 {
        format!(
            "{context_block}\n\n{user_message}",
            user_message = user_message,
        )
    } else {
        format!(
            r#"[Deck context — {deck_name}, {main_count} maindeck cards]
{execution_context}

{user_message}"#,
            deck_name = deck.name,
            main_count = deck.main.len(),
            user_message = user_message,
        )
    };

    let mut turns = recent_history;
    turns.push(("user".to_string(), user_payload));

    let content = ai
        .chat_messages(system, &turns, false)?
        .trim()
        .to_string();

    logging::record(
        "AI-CHAT",
        format!(
            "RESPONSE • deck={} • user_chars={} • assistant_chars={}",
            deck.name,
            user_message.chars().count(),
            content.chars().count(),
        ),
    );

    Ok(content)
}

fn sanitize_profile(
    deck: &ParsedDeck,
    db: &CardDb,
    profile: &mut AiCommanderProfile,
) {
    let commander_names = deck
        .commanders
        .iter()
        .map(|entry| entry.name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    let mut identity = HashSet::<char>::new();

    for entry in &deck.commanders {
        if let Ok(Some(card)) = db.get_card(&entry.name) {
            for symbol in card.color_identity.chars() {
                if "WUBRG".contains(symbol) {
                    identity.insert(symbol);
                }
            }
        }
    }

    let identity_allows = |card_identity: &str| {
        card_identity
            .chars()
            .filter(|symbol| "WUBRG".contains(*symbol))
            .all(|symbol| identity.contains(&symbol))
    };

    profile.priority_cards.retain(|name| {
        db.get_card(name)
            .ok()
            .flatten()
            .map(|card| {
                card.commander_legal
                    && identity_allows(&card.color_identity)
            })
            .unwrap_or(false)
            || commander_names.contains(&name.to_ascii_lowercase())
    });

    profile.avoid_or_question_cards.retain(|name| {
        db.get_card(name)
            .ok()
            .flatten()
            .is_some()
    });

    for package in &mut profile.win_packages {
        package.cards.retain(|name| {
            db.get_card(name)
                .ok()
                .flatten()
                .map(|card| {
                    card.commander_legal
                        && identity_allows(&card.color_identity)
                })
                .unwrap_or(false)
                || commander_names.contains(&name.to_ascii_lowercase())
        });

        package.priority = package.priority.min(100);
    }

    profile
        .win_packages
        .retain(|package| package.cards.len() >= 2);

    profile.primary_archetypes.truncate(8);
    profile.priority_cards.truncate(40);
    profile.avoid_or_question_cards.truncate(30);
    profile.win_packages.truncate(12);
    profile.gameplan.truncate(12);
    profile.community_findings.truncate(16);
    profile.rules_notes.truncate(12);
    profile.evidence_notes.truncate(16);
    profile.testing_requests.truncate(12);
    profile.feedback_preferences.truncate(12);
}

fn build_grounded_context(
    deck: &ParsedDeck,
    db: &CardDb,
    bracket: u8,
    goal: OptimizationGoal,
    preferred_archetype: Option<&str>,
    style_brief: Option<&str>,
    user_references: &[DeckSource],
    research: &DeepLearnReport,
    web: &WebResearchBundle,
) -> String {
    let commanders = deck
        .commanders
        .iter()
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>();

    let commander_cards = commanders
        .iter()
        .filter_map(|name| db.get_card(name).ok().flatten())
        .map(|card| {
            format!(
                "{} | {} | MV {:.1} | colors {} | Oracle: {}",
                card.name,
                card.type_line,
                card.cmc,
                if card.color_identity.is_empty() {
                    "C".to_string()
                } else {
                    card.color_identity
                },
                card.oracle_text.replace('\n', " "),
            )
        })
        .collect::<Vec<_>>();

    let current_cards = deck
        .main
        .iter()
        .take(110)
        .map(|entry| format!("{} {}", entry.quantity, entry.name))
        .collect::<Vec<_>>();

    let source_excerpt = web
        .documents
        .iter()
        .take(24)
        .map(|doc| {
            format!(
                "[{}] {} :: {}",
                doc.source,
                doc.title,
                doc.text.chars().take(1000).collect::<String>(),
            )
        })
        .collect::<Vec<_>>();

    let user_reference_excerpt = user_references
        .iter()
        .take(12)
        .map(|source| {
            if let Some(reference_deck) =
                source.reference_deck.as_ref()
            {
                let commanders = reference_deck
                    .commanders
                    .iter()
                    .map(|entry| entry.name.clone())
                    .collect::<Vec<_>>()
                    .join(" + ");

                let cards = reference_deck
                    .main
                    .iter()
                    .take(100)
                    .map(|entry| {
                        format!(
                            "{} {}",
                            entry.quantity,
                            entry.name,
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                format!(
                    "USER REFERENCE (PRIORITY {}): {}\nURL: {}\nCOMMANDERS: {}\nDECKLIST:\n{}",
                    source.priority,
                    source.title,
                    source.url,
                    commanders,
                    cards,
                )
            } else {
                format!(
                    "USER REFERENCE (PRIORITY {}): {}\nURL: {}\nEXCERPT: {}",
                    source.priority,
                    source.title,
                    source.url,
                    source
                        .content_excerpt
                        .chars()
                        .take(6000)
                        .collect::<String>(),
                )
            }
        })
        .collect::<Vec<_>>();

    format!(
        r#"TARGET
Bracket: B{bracket}
Objective: {goal}
Preferred archetype: {preferred}
User deck-style brief: {style_brief}

COMMANDER CARD DATA
{commander_data}

CURRENT DECK
{current_deck}

DEEP LEARN EVIDENCE
Reference decks: {reference_decks}
EDHREC signals: {edhrec_signals}
Confidence: {confidence}
Normal themes: {themes}
Role targets: {roles}

REFERENCE CORE CARDS
{core}

REFERENCE PACKAGES
{packages}

REFERENCE COMBOS
{combos}

CURRENT ADD CANDIDATES
{adds}

CURRENT CUT CANDIDATES
{cuts}

USER-SUPPLIED HIGH-PRIORITY REFERENCES
{user_references}

WEB / COMMUNITY / RULES RESEARCH
{web_docs}

COMMANDER RULE GUARDRAILS
- Commander deck construction is singleton except basic lands and cards whose text permits otherwise.
- Color identity is enforced by Commander Lab's deterministic legality layer.
- Commander eligibility is enforced by Commander Lab/Scryfall data and explicit commander rules.
- Do not infer legality from Reddit/community posts.
- A combo claim must be supported by supplied Commander Spellbook/reference evidence before treating it as verified.
"#,
        bracket=bracket,
        goal=goal.label(),
        preferred=preferred_archetype.unwrap_or("AUTO"),
        style_brief=style_brief.unwrap_or("No additional style brief supplied."),
        commander_data=commander_cards.join("\n"),
        current_deck=current_cards.join("\n"),
        reference_decks=research.reference_decks,
        edhrec_signals=research.edhrec_signals,
        confidence=research.confidence,
        themes=research.normal_themes.join(" / "),
        roles=research.role_targets,
        core=research.reference_core_cards.iter().take(30).cloned().collect::<Vec<_>>().join("\n"),
        packages=research.reference_packages.iter().take(24).cloned().collect::<Vec<_>>().join("\n"),
        combos=research.reference_combos.iter().take(20).cloned().collect::<Vec<_>>().join("\n"),
        adds=research.adds.iter().take(24).map(|card| format!("{} | score={} | {}", card.name, card.score, card.reason)).collect::<Vec<_>>().join("\n"),
        cuts=research.cuts.iter().take(20).map(|card| format!("{} | pressure={} | {}", card.name, card.score, card.reason)).collect::<Vec<_>>().join("\n"),
        user_references=if user_reference_excerpt.is_empty() {
            "None supplied.".to_string()
        } else {
            user_reference_excerpt.join("\n\n")
        },
        web_docs=source_excerpt.join("\n\n"),
    )
}

/// What the AI model suggests before a deck is generated.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GenerationHints {
    /// Play styles for the deck, best first.
    #[serde(default)]
    pub themes: Vec<String>,
    /// Cards that should be in a good list for this commander.
    #[serde(default)]
    pub key_cards: Vec<String>,
    /// Cards that look on-theme but play badly here.
    #[serde(default)]
    pub avoid: Vec<String>,
    /// One or two sentences on how the deck should win.
    #[serde(default)]
    pub plan: String,
    /// The model that answered.
    #[serde(skip_deserializing)]
    pub model: String,
}

/// Ask the configured AI model how to build around `commanders`. The
/// generator treats the answer as suggestions: cards are checked against
/// the card database, format and colors before they count.
pub fn generation_hints(
    commanders: &[crate::data::Card],
    edhrec_themes: &[String],
    user_themes: &str,
    bracket: u8,
    format: &str,
) -> Result<GenerationHints> {
    let ai = LocalAi::connect()?;
    let commander_text = commanders
        .iter()
        .map(|c| format!("{} — {} — {}\n{}", c.name, c.mana_cost, c.type_line, c.oracle_text))
        .collect::<Vec<_>>()
        .join("\n\n");
    let system = "You are an expert Magic: The Gathering Commander deckbuilder. Answer only with JSON. \
        Use exact English card names. Only suggest cards legal in the format and inside the commander's color identity.";
    let prompt = format!(
        "Format: {format}. Target bracket: {bracket} (1 casual … 5 cEDH).\n\
         Commander:\n{commander_text}\n\n\
         Popular themes on EDHREC: {}.\n\
         Themes the player asked for: {}.\n\n\
         Reply with JSON shaped like this example (different content):\n\
         {{\"themes\": [\"Tokens\", \"Aristocrats\"], \
         \"key_cards\": [\"Skullclamp\", \"Pitiless Plunderer\", \"Grave Pact\"], \
         \"avoid\": [\"Card Name\"], \
         \"plan\": \"Make many small creatures, then drain the table by sacrificing them.\"}}\n\
         Give up to 3 themes, 10 to 15 key cards that make this commander's plan work, and up to 5 cards to avoid.",
        if edhrec_themes.is_empty() { "unknown".to_string() } else { edhrec_themes.join(", ") },
        if user_themes.trim().is_empty() { "none, choose for them" } else { user_themes },
    );
    let value = ai.chat_json(system, &prompt)?;
    // Small models stray from the shape (numbers in a name list, a string
    // instead of a list); keep whatever text entries are there.
    let names = |key: &str| -> Vec<String> {
        match value.get(key) {
            Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            Some(Value::String(s)) => s.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            _ => Vec::new(),
        }
    };
    let mut hints = GenerationHints {
        themes: names("themes"),
        key_cards: names("key_cards"),
        avoid: names("avoid"),
        plan: value.get("plan").and_then(Value::as_str).unwrap_or_default().to_string(),
        model: ai.model().to_string(),
    };
    if hints.themes.is_empty() && hints.key_cards.is_empty() {
        return Err(anyhow!("The AI model's answer had no themes or cards in it."));
    }
    hints.themes.truncate(3);
    hints.key_cards.truncate(15);
    hints.avoid.truncate(5);
    Ok(hints)
}

/// Send a one-word request to the configured AI and time it, so Settings can
/// say whether a key, address and model work together.
pub fn test_connection() -> Result<Value> {
    let started = std::time::Instant::now();
    let ai = LocalAi::connect()?;
    let reply = ai.chat("You are a connection test.", &[("user".into(), "Reply with the single word OK.".into())])?;
    Ok(json!({
        "ok": true,
        "model": ai.model(),
        "provider": ai.provider().label(),
        "reply": reply.trim().chars().take(80).collect::<String>(),
        "seconds": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
    }))
}

/// Model names an online API offers (OpenAI-compatible `GET …/models`).
/// Not every service lists models; the caller falls back to typing one.
pub fn list_api_models(base_url: &str, api_key: &str) -> Result<Vec<String>> {
    let base = base_url.trim().trim_end_matches('/');
    let url = if base.ends_with("/v1") || base.ends_with("/openai") { format!("{base}/models") } else { format!("{base}/v1/models") };
    let client = Client::builder().timeout(Duration::from_secs(20)).build()?;
    let mut request = client.get(&url);
    if !api_key.trim().is_empty() {
        request = request.bearer_auth(api_key.trim());
    }
    let value: Value = request
        .send()
        .context("Couldn't reach that address")?
        .error_for_status()
        .context("The service refused the model list (check the key)")?
        .json()
        .context("The service's model list wasn't readable")?;
    let mut names: Vec<String> = value
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    names.sort();
    Ok(names)
}
