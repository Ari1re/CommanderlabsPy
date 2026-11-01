//! Official Commander precons from MTGJSON (https://mtgjson.com), an open
//! dataset of every product decklist. The index is cached for a few days;
//! each deck is fetched when the user picks it.

use crate::deck::{DeckEntry, ParsedDeck};
use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    path::Path,
    time::Duration,
};

const INDEX_URL: &str = "https://mtgjson.com/api/v5/DeckList.json";
const INDEX_CACHE: &str = "./data/source_cache/mtgjson_decklist.json";
const INDEX_MAX_AGE: Duration = Duration::from_secs(3 * 24 * 60 * 60);

#[derive(Debug, Clone, Serialize)]
pub struct Precon {
    /// MTGJSON's file name, used to fetch the deck.
    pub file: String,
    pub name: String,
    pub set_code: String,
    pub release_date: String,
}

fn client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("CommanderLab/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

fn fetch_json(url: &str) -> Result<Value> {
    client()?
        .get(url)
        .send()
        .context("Couldn't reach MTGJSON")?
        .error_for_status()
        .context("MTGJSON refused the request")?
        .json()
        .context("MTGJSON sent an unreadable answer")
}

fn cached_index() -> Option<Value> {
    let path = Path::new(INDEX_CACHE);
    let age = fs::metadata(path).ok()?.modified().ok()?.elapsed().ok()?;
    if age > INDEX_MAX_AGE {
        return None;
    }
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// Every Commander precon, newest first.
pub fn list() -> Result<Vec<Precon>> {
    let index = match cached_index() {
        Some(index) => index,
        None => match fetch_json(INDEX_URL) {
            Ok(index) => {
                if let Some(parent) = Path::new(INDEX_CACHE).parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(INDEX_CACHE, index.to_string());
                index
            }
            // Offline: an old list beats none.
            Err(err) => fs::read_to_string(INDEX_CACHE)
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .ok_or(err)?,
        },
    };
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let mut out: Vec<Precon> = index
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|d| d.get("type").and_then(Value::as_str) == Some("Commander Deck"))
        .map(|d| Precon { file: s(d, "fileName"), name: s(d, "name"), set_code: s(d, "code"), release_date: s(d, "releaseDate") })
        .filter(|p| !p.file.is_empty())
        .collect();
    out.sort_by(|a, b| b.release_date.cmp(&a.release_date).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

/// Fetch one precon's decklist, keeping each card's printing.
pub fn load(file: &str) -> Result<ParsedDeck> {
    // File names are letters, digits and underscores; refuse anything else
    // rather than build an odd URL.
    if file.is_empty() || !file.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(anyhow!("Unknown precon {file}"));
    }
    let body = fetch_json(&format!("https://mtgjson.com/api/v5/decks/{file}.json"))?;
    let data = body.get("data").ok_or_else(|| anyhow!("MTGJSON's precon file had no deck in it"))?;
    let zone = |key: &str| -> Vec<DeckEntry> {
        data.get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|card| {
                let name = card.get("name").and_then(Value::as_str)?;
                let mut entry = DeckEntry::new(card.get("count").and_then(Value::as_u64).unwrap_or(1) as u32, name);
                entry.set_code = card.get("setCode").and_then(Value::as_str).map(str::to_ascii_uppercase);
                entry.collector_number = card.get("number").and_then(Value::as_str).map(str::to_string);
                entry.foil = card.get("isFoil").and_then(Value::as_bool).unwrap_or(false);
                Some(entry)
            })
            .collect()
    };
    let commanders = zone("commander");
    if commanders.is_empty() {
        return Err(anyhow!("That precon has no commander listed."));
    }
    Ok(ParsedDeck {
        name: data.get("name").and_then(Value::as_str).unwrap_or(file).to_string(),
        commanders,
        main: zone("mainBoard"),
        sideboard: Vec::new(),
        format: Default::default(),
        considering: Vec::new(),
    })
}
