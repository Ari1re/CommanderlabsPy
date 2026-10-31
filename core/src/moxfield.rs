use crate::deck::{DeckEntry, ParsedDeck};
use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{
    ACCEPT, CACHE_CONTROL, ORIGIN, REFERER, USER_AGENT,
};
use serde_json::Value;

const USER_AGENT_VALUE: &str =
    concat!("Mozilla/5.0 CommanderLab/", env!("CARGO_PKG_VERSION"), " (local Commander deck analyzer)");

pub fn import_moxfield(url: &str) -> Result<ParsedDeck> {
    let deck_id = extract_deck_id(url)
        .ok_or_else(|| anyhow!("Could not extract a Moxfield deck ID from that URL."))?;

    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    // Moxfield's public deck endpoints are undocumented and may change.
    // Try the newer route first, then the historically common v2 route.
    let candidates = [
        format!("https://api2.moxfield.com/v3/decks/all/{deck_id}"),
        format!("https://api2.moxfield.com/v2/decks/all/{deck_id}"),
    ];

    let mut errors = Vec::new();

    for api_url in candidates {
        match request_deck(&client, &api_url) {
            Ok(response) => {
                let name = response
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or(&deck_id)
                    .to_string();

                let commanders = collect_zone(&response, "commanders");
                let main = collect_zone(&response, "mainboard");
                let sideboard = collect_zone(&response, "sideboard");

                if !main.is_empty() || !commanders.is_empty() {
                    return Ok(ParsedDeck {
                        name,
                        commanders,
                        main,
                        sideboard,
                        format: Default::default(),
                    });
                }

                errors.push(format!(
                    "{api_url}: deck returned, but no supported command/mainboard zones were found"
                ));
            }
            Err(err) => errors.push(format!("{api_url}: {err}")),
        }
    }

    Err(anyhow!(
        "Moxfield import failed. Their deck API is undocumented and can be blocked by Cloudflare. \
         Paste a Moxfield text export instead if needed. Attempts:\n{}",
        errors.join("\n")
    ))
}

fn request_deck(client: &Client, api_url: &str) -> Result<Value> {
    client
        .get(api_url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "application/json, text/plain, */*")
        .header(ORIGIN, "https://www.moxfield.com")
        .header(REFERER, "https://www.moxfield.com/")
        .header(CACHE_CONTROL, "no-cache")
        .send()
        .context("Could not contact Moxfield")?
        .error_for_status()
        .context("Moxfield rejected the import request")?
        .json()
        .context("Could not parse the Moxfield deck response")
}

fn extract_deck_id(url: &str) -> Option<String> {
    let trimmed = url.trim();

    if !trimmed.contains('/') {
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    }

    let parts = trimmed
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();

    let deck_position = parts
        .iter()
        .position(|part| part.eq_ignore_ascii_case("decks"))?;

    let id = parts.get(deck_position + 1)?;
    let id = id
        .split('?')
        .next()
        .unwrap_or(id)
        .split('#')
        .next()
        .unwrap_or(id)
        .trim();

    (!id.is_empty()).then(|| id.to_string())
}

fn collect_zone(root: &Value, key: &str) -> Vec<DeckEntry> {
    // v2-ish shape:
    // { "mainboard": { "Card Name": { quantity, card: {...} } } }
    if let Some(zone) = root.get(key) {
        let cards = collect_zone_value(zone);
        if !cards.is_empty() {
            return cards;
        }
    }

    // v3-ish shape commonly seen in open-source clients:
    // { "boards": { "mainboard": { "cards": {...} } } }
    if let Some(board) = root.get("boards").and_then(|boards| boards.get(key)) {
        if let Some(cards_value) = board.get("cards") {
            let cards = collect_zone_value(cards_value);
            if !cards.is_empty() {
                return cards;
            }
        }

        let cards = collect_zone_value(board);
        if !cards.is_empty() {
            return cards;
        }
    }

    Vec::new()
}

fn collect_zone_value(zone: &Value) -> Vec<DeckEntry> {
    match zone {
        Value::Object(map) => map
            .values()
            .filter_map(parse_card_object)
            .collect(),

        Value::Array(items) => items
            .iter()
            .filter_map(parse_card_object)
            .collect(),

        _ => Vec::new(),
    }
}

fn parse_card_object(value: &Value) -> Option<DeckEntry> {
    let quantity = value
        .get("quantity")
        .and_then(Value::as_u64)
        .or_else(|| value.get("count").and_then(Value::as_u64))
        .unwrap_or(1) as u32;

    let card = value.get("card").unwrap_or(value);

    let name = card
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| value.get("name").and_then(Value::as_str))
        .or_else(|| card.get("faceName").and_then(Value::as_str))
        .or_else(|| card.get("face_name").and_then(Value::as_str))?;

    Some(DeckEntry {
        quantity,
        name: name.to_string(), ..Default::default() })
}
