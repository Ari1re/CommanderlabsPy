//! Every printing of a card, from Scryfall, for the printing picker.

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime},
};

const CACHE_DIR: &str = "./data/printings";
const CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Printing {
    pub set_code: String,
    pub set_name: String,
    pub collector_number: String,
    pub released_at: String,
    pub image: String,
    pub finishes: Vec<String>,
    pub price_usd: Option<String>,
    pub price_usd_foil: Option<String>,
    pub price_eur: Option<String>,
    pub lang: String,
}

#[derive(Deserialize)]
struct Page {
    data: Vec<Value>,
    #[serde(default)]
    has_more: bool,
    #[serde(default)]
    next_page: Option<String>,
}

type Value = serde_json::Value;

fn cache_path(name: &str) -> PathBuf {
    let key: String = name
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    PathBuf::from(CACHE_DIR).join(format!("{key}.json"))
}

fn image_of(card: &Value) -> String {
    let pick = |uris: &Value| {
        uris.get("normal")
            .or_else(|| uris.get("large"))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    card.get("image_uris")
        .and_then(pick)
        .or_else(|| {
            card.get("card_faces")
                .and_then(Value::as_array)
                .and_then(|faces| faces.first())
                .and_then(|f| f.get("image_uris"))
                .and_then(pick)
        })
        .unwrap_or_default()
}

fn parse(card: &Value) -> Option<Printing> {
    let s = |k: &str| card.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let price = |k: &str| {
        card.get("prices")
            .and_then(|p| p.get(k))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    if s("set").is_empty() {
        return None;
    }
    Some(Printing {
        set_code: s("set").to_ascii_uppercase(),
        set_name: s("set_name"),
        collector_number: s("collector_number"),
        released_at: s("released_at"),
        image: image_of(card),
        finishes: card
            .get("finishes")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default(),
        price_usd: price("usd"),
        price_usd_foil: price("usd_foil"),
        price_eur: price("eur"),
        lang: s("lang"),
    })
}

/// All English paper printings of `name`, newest first. Cached for a week.
pub fn printings(name: &str) -> Result<Vec<Printing>> {
    let path = cache_path(name);
    if let Ok(meta) = fs::metadata(&path) {
        let fresh = meta
            .modified()
            .ok()
            .and_then(|m| SystemTime::now().duration_since(m).ok())
            .map(|age| age < CACHE_TTL)
            .unwrap_or(false);
        if fresh {
            if let Ok(cached) = fs::read_to_string(&path).map(|t| serde_json::from_str(&t)) {
                if let Ok(list) = cached {
                    return Ok(list);
                }
            }
        }
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("CommanderLab/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let front = name.split(" // ").next().unwrap_or(name);
    let query = format!("!\"{front}\" game:paper");
    let mut url = format!(
        "https://api.scryfall.com/cards/search?unique=prints&order=released&dir=desc&q={}",
        urlencoding::encode(&query)
    );

    let mut out = Vec::new();
    for _ in 0..4 {
        let page: Page = client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .context("Could not reach Scryfall")?
            .error_for_status()
            .context("Scryfall didn't return printings for that card")?
            .json()?;
        out.extend(page.data.iter().filter_map(parse));
        match (page.has_more, page.next_page) {
            (true, Some(next)) => {
                url = next;
                std::thread::sleep(Duration::from_millis(100));
            }
            _ => break,
        }
    }

    let _ = fs::create_dir_all(CACHE_DIR);
    let _ = fs::write(&path, serde_json::to_string(&out)?);
    Ok(out)
}
