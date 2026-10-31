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
    /// Full-size scan (745×1040 PNG) of the front face.
    #[serde(default)]
    pub png: String,
    /// Scryfall's artwork ID: printings that share it share the same art.
    #[serde(default)]
    pub illustration_id: String,
    /// Each face of a double-faced printing, with its own images.
    #[serde(default)]
    pub faces: Vec<PrintingFace>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrintingFace {
    pub name: String,
    pub image: String,
    pub png: String,
    #[serde(default)]
    pub illustration_id: String,
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

fn uri(uris: Option<&Value>, key: &str) -> String {
    uris.and_then(|u| u.get(key)).and_then(Value::as_str).unwrap_or_default().to_string()
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
        png: {
            let own = uri(card.get("image_uris"), "png");
            if own.is_empty() {
                card.get("card_faces")
                    .and_then(Value::as_array)
                    .and_then(|f| f.first())
                    .map(|f| uri(f.get("image_uris"), "png"))
                    .unwrap_or_default()
            } else {
                own
            }
        },
        illustration_id: {
            let own = s("illustration_id");
            if own.is_empty() {
                card.get("card_faces")
                    .and_then(Value::as_array)
                    .and_then(|f| f.first())
                    .and_then(|f| f.get("illustration_id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            } else {
                own
            }
        },
        faces: card
            .get("card_faces")
            .and_then(Value::as_array)
            .map(|faces| {
                faces
                    .iter()
                    .filter(|f| f.get("image_uris").is_some())
                    .map(|f| PrintingFace {
                        name: f.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
                        image: uri(f.get("image_uris"), "normal"),
                        png: uri(f.get("image_uris"), "png"),
                        illustration_id: f.get("illustration_id").and_then(Value::as_str).unwrap_or_default().to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
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
            if let Ok(cached) = fs::read_to_string(&path).map(|t| serde_json::from_str::<Vec<Printing>>(&t)) {
                // Caches from before 0.27.2 lack the full-size scans.
                if let Ok(list) = cached {
                    if list.iter().any(|p| !p.png.is_empty()) {
                        return Ok(list);
                    }
                }
            }
        }
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("CommanderLab/", env!("CARGO_PKG_VERSION")))
        .build()?;

    // Exact full name first (split cards are only found as "Fire // Ice"),
    // then the front face for names saved without their back.
    let front = name.split(" // ").next().unwrap_or(name);
    let search_url = |n: &str| {
        format!(
            "https://api.scryfall.com/cards/search?unique=prints&order=released&dir=desc&q={}",
            urlencoding::encode(&format!("!\"{n}\" game:paper"))
        )
    };
    let mut url = search_url(name);
    if name != front {
        let found = client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        if !found {
            url = search_url(front);
        }
    }

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
