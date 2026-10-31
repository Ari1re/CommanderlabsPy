//! Deck version history.
//!
//! Every change to a deck is snapshotted under `decks/.versions/<deck>/` with
//! a short description and a diff against the previous version, so users can
//! browse what changed and restore any earlier version.

use crate::deck::{deck_to_text, parse_deck_text, safe_deck_filename, ParsedDeck};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

const ROOT: &str = "./decks/.versions";
const MAX_VERSIONS: usize = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub name: String,
    /// Positive = added copies, negative = removed copies.
    pub delta: i64,
    pub zone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionMeta {
    pub id: u64,
    pub timestamp_ms: u64,
    pub message: String,
    pub cards: u32,
    #[serde(default)]
    pub changes: Vec<Change>,
}

fn dir(deck: &str) -> PathBuf {
    PathBuf::from(ROOT).join(safe_deck_filename(deck))
}

fn index_path(deck: &str) -> PathBuf {
    dir(deck).join("index.json")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn list(deck: &str) -> Vec<VersionMeta> {
    fs::read_to_string(index_path(deck))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_index(deck: &str, index: &[VersionMeta]) -> Result<()> {
    fs::create_dir_all(dir(deck))?;
    fs::write(index_path(deck), serde_json::to_string_pretty(index)?)?;
    Ok(())
}

pub fn text(deck: &str, id: u64) -> Result<String> {
    fs::read_to_string(dir(deck).join(format!("{id}.txt")))
        .map_err(|_| anyhow!("That version no longer exists."))
}

pub fn load(deck: &str, id: u64) -> Result<ParsedDeck> {
    parse_deck_text(deck, &text(deck, id)?)
}

fn counts(deck: &ParsedDeck) -> BTreeMap<(String, String), i64> {
    let mut map = BTreeMap::new();
    for (zone, list) in [("commander", &deck.commanders), ("main", &deck.main), ("sideboard", &deck.sideboard)] {
        for e in list {
            *map.entry((zone.to_string(), e.name.clone())).or_insert(0) += e.quantity as i64;
        }
    }
    map
}

pub fn diff(before: Option<&ParsedDeck>, after: &ParsedDeck) -> Vec<Change> {
    let a = before.map(counts).unwrap_or_default();
    let b = counts(after);
    let mut keys: Vec<_> = a.keys().chain(b.keys()).cloned().collect();
    keys.sort();
    keys.dedup();
    let mut out: Vec<Change> = keys
        .into_iter()
        .filter_map(|key| {
            let delta = b.get(&key).copied().unwrap_or(0) - a.get(&key).copied().unwrap_or(0);
            (delta != 0).then(|| Change { name: key.1, delta, zone: key.0 })
        })
        .collect();
    out.sort_by(|x, y| y.delta.signum().cmp(&x.delta.signum()).then(x.name.cmp(&y.name)));
    out
}

/// Record a snapshot of `deck` (skipped when nothing changed).
pub fn record(name: &str, deck: &ParsedDeck, message: &str) -> Result<Option<VersionMeta>> {
    let mut index = list(name);
    let body = deck_to_text(deck);

    let previous = index.last().and_then(|v| load(name, v.id).ok());
    if let Some(last) = index.last() {
        if text(name, last.id).map(|t| t == body).unwrap_or(false) {
            return Ok(None);
        }
    }

    let mut id = now_ms();
    if let Some(last) = index.last() {
        if id <= last.id {
            id = last.id + 1;
        }
    }

    fs::create_dir_all(dir(name))?;
    fs::write(dir(name).join(format!("{id}.txt")), &body)?;

    let meta = VersionMeta {
        id,
        timestamp_ms: id,
        message: message.to_string(),
        cards: deck.commanders.iter().chain(deck.main.iter()).map(|e| e.quantity).sum(),
        changes: diff(previous.as_ref(), deck),
    };
    index.push(meta.clone());

    if index.len() > MAX_VERSIONS {
        let excess = index.len() - MAX_VERSIONS;
        for old in index.drain(..excess) {
            let _ = fs::remove_file(dir(name).join(format!("{}.txt", old.id)));
        }
    }

    save_index(name, &index)?;
    Ok(Some(meta))
}

pub fn rename(from: &str, to: &str, keep_original: bool) {
    let (a, b) = (dir(from), dir(to));
    if !a.exists() {
        return;
    }
    let _ = fs::create_dir_all(&b);
    if let Ok(entries) = fs::read_dir(&a) {
        for entry in entries.flatten() {
            let _ = fs::copy(entry.path(), b.join(entry.file_name()));
        }
    }
    if !keep_original {
        let _ = fs::remove_dir_all(a);
    }
}

pub fn delete(name: &str) {
    let _ = fs::remove_dir_all(dir(name));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::DeckEntry;

    fn deck(main: &[(u32, &str)]) -> ParsedDeck {
        ParsedDeck {
            name: "t".into(),
            commanders: vec![DeckEntry::new(1, "Kefka, Court Mage")],
            main: main.iter().map(|(q, n)| DeckEntry::new(*q, *n)).collect(),
            sideboard: vec![],
            format: Default::default(),
        }
    }

    #[test]
    fn diff_reports_adds_and_removes() {
        let a = deck(&[(1, "Sol Ring"), (30, "Island")]);
        let b = deck(&[(1, "Arcane Signet"), (29, "Island")]);
        let changes = diff(Some(&a), &b);
        assert_eq!(changes.len(), 3);
        assert!(changes.iter().any(|c| c.name == "Arcane Signet" && c.delta == 1));
        assert!(changes.iter().any(|c| c.name == "Sol Ring" && c.delta == -1));
        assert!(changes.iter().any(|c| c.name == "Island" && c.delta == -1));
        // additions sort first
        assert_eq!(changes[0].delta, 1);
    }
}
