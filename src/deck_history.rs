use crate::{
    ai::AgentChatMessage,
    deep_agent::DeepAgentResult,
    deep_learn::DeepLearnReport,
    meta_sim::MetaMatchupSummary,
};
use anyhow::{Context, Result};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeckRunRecord {
    pub timestamp: String,
    pub kind: String,
    pub summary: String,
    #[serde(default)]
    pub log: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeckHistory {
    pub deck_name: String,
    #[serde(default)]
    pub runs: Vec<DeckRunRecord>,
    #[serde(default)]
    pub agent_chat: Vec<AgentChatMessage>,
    #[serde(default)]
    pub agent_last_brief: String,
    #[serde(default)]
    pub latest_research: Option<DeepLearnReport>,
    #[serde(default)]
    pub latest_agent: Option<DeepAgentResult>,
    #[serde(default)]
    pub latest_meta_matchup: Option<MetaMatchupSummary>,
}

pub fn history_path(deck_path: &Path) -> PathBuf {
    let stem = deck_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("deck");

    deck_path
        .parent()
        .unwrap_or_else(|| Path::new("./decks"))
        .join("logs")
        .join(format!("{stem}.history.json"))
}

fn legacy_history_path(deck_path: &Path) -> PathBuf {
    let stem = deck_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("deck");

    deck_path
        .parent()
        .unwrap_or_else(|| Path::new("./decks"))
        .join(format!("{stem}.history.json"))
}

fn migrate_legacy_history(deck_path: &Path) {
    let old_path = legacy_history_path(deck_path);
    let new_path = history_path(deck_path);

    if !old_path.exists() || new_path.exists() {
        return;
    }

    if let Some(parent) = new_path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }

    if fs::rename(&old_path, &new_path).is_err() {
        if fs::copy(&old_path, &new_path).is_ok() {
            let _ = fs::remove_file(old_path);
        }
    }
}

pub fn delete_history(deck_path: &Path) {
    let path = history_path(deck_path);
    let legacy = legacy_history_path(deck_path);

    if path.exists() {
        let _ = fs::remove_file(path);
    }

    if legacy.exists() {
        let _ = fs::remove_file(legacy);
    }
}

pub fn load_history(deck_path: &Path) -> DeckHistory {
    migrate_legacy_history(deck_path);

    let path = history_path(deck_path);

    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| DeckHistory {
            deck_name: deck_path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("deck")
                .to_string(),
            runs: Vec::new(),
            agent_chat: Vec::new(),
            agent_last_brief: String::new(),
            latest_research: None,
            latest_agent: None,
            latest_meta_matchup: None,
        })
}

fn save_history(
    deck_path: &Path,
    history: &DeckHistory,
) -> Result<()> {
    let path = history_path(deck_path);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| {
                format!(
                    "Could not create deck history directory {}",
                    parent.display(),
                )
            })?;
    }

    fs::write(
        &path,
        serde_json::to_string_pretty(history)?,
    )
    .with_context(|| {
        format!("Could not write {}", path.display())
    })?;

    Ok(())
}

pub fn save_agent_session(
    deck_path: &Path,
    chat: &[AgentChatMessage],
    last_brief: &str,
    research: Option<&DeepLearnReport>,
    agent: Option<&DeepAgentResult>,
    meta_matchup: Option<&MetaMatchupSummary>,
) -> Result<()> {
    let mut history = load_history(deck_path);

    history.agent_chat = chat.to_vec();
    history.agent_last_brief = last_brief.to_string();
    history.latest_research = research.cloned();
    history.latest_agent = agent.cloned();
    history.latest_meta_matchup = meta_matchup.cloned();

    save_history(deck_path, &history)
}

pub fn append_run(
    deck_path: &Path,
    kind: impl Into<String>,
    summary: impl Into<String>,
    log: Vec<String>,
) -> Result<()> {
    let mut history = load_history(deck_path);

    history.runs.push(DeckRunRecord {
        timestamp: Local::now()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        kind: kind.into(),
        summary: summary.into(),
        log,
    });

    if history.runs.len() > 80 {
        let remove = history.runs.len() - 80;
        history.runs.drain(0..remove);
    }

    save_history(deck_path, &history)?;

    Ok(())
}

pub fn render_history_lines(deck_path: &Path) -> Vec<String> {
    let history = load_history(deck_path);
    let mut lines = Vec::new();

    for run in history.runs.iter().rev() {
        lines.push(format!(
            "{}  [{}]  {}",
            run.timestamp,
            run.kind,
            run.summary,
        ));

        for line in run.log.iter().take(80) {
            lines.push(format!("    {line}"));
        }

        lines.push(String::new());
    }

    if lines.is_empty() {
        lines.push(
            "No cached tests or Agent logs for this deck yet."
                .to_string(),
        );
    }

    lines
}
