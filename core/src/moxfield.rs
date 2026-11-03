//! Importing a deck from a Moxfield link.
//!
//! How it fits: [`import_moxfield`] reads the deck from Moxfield's API (v3,
//! then v2) with browser-like headers and maps its boards to zones.
//!
//! Things to work on:
//! - Moxfield can refuse automated requests; the import dialog tells users to
//!   paste the text export instead. Tags from the API aren't imported yet.

use crate::deck::{DeckEntry, ParsedDeck};
use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{
    ACCEPT, CACHE_CONTROL, ORIGIN, REFERER, USER_AGENT,
};
use serde_json::Value;

const USER_AGENT_VALUE: &str =
    concat!("Mozilla/5.0 CommanderLab/", env!("CARGO_PKG_VERSION"), " (local Commander deck analyzer)");

/// Import a public Moxfield deck from its URL (or bare id). It tries the v3,
/// then the v2 API, and takes the first answer that has cards. Only names and
/// counts come across: printings, tags and the format are dropped.
/// TODO: keep set/collector number and Moxfield's tags, since the deck view
/// supports both now.
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
                        considering: Vec::new(),
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

/// GET one API route with browser-like Origin/Referer headers. Moxfield sits
/// behind Cloudflare, which blocks bare clients more often.
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

/// The id after "/decks/" in a Moxfield URL, without ?query or #fragment.
/// Text with no slash is taken as the id itself.
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

/// Cards in one board ("commanders", "mainboard", "sideboard"). Handles both
/// the v2 shape (board at the top level) and the v3 shape (`boards.<key>.cards`).
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

/// A board's cards, whether keyed by name (object) or listed (array).
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

/// One board entry: quantity (or count, default 1) and the card name from
/// the nested `card`, the entry itself, or a face name.
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
