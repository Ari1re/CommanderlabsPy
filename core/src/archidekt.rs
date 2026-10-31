use crate::deck::{DeckEntry, ParsedDeck};
use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, USER_AGENT};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

const USER_AGENT_VALUE: &str =
    concat!("CommanderLab/", env!("CARGO_PKG_VERSION"), " (local Commander deck analyzer)");

pub fn import_archidekt(url: &str) -> Result<ParsedDeck> {
    let deck_id = extract_deck_id(url)
        .ok_or_else(|| anyhow!("Could not extract an Archidekt numeric deck ID."))?;

    let api_url = format!("https://archidekt.com/api/decks/{deck_id}/");

    let payload: Value = Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?
        .get(&api_url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "application/json")
        .send()
        .context("Could not contact Archidekt")?
        .error_for_status()
        .context("Archidekt rejected the deck request")?
        .json()
        .context("Could not parse Archidekt deck JSON")?;

    parse_archidekt_payload(&payload, &deck_id)
}

pub fn parse_archidekt_payload(
    payload: &Value,
    fallback_name: &str,
) -> Result<ParsedDeck> {
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(fallback_name)
        .to_string();

    let mut commander_categories = HashSet::new();
    let mut included_categories = HashSet::new();
    let mut excluded_categories = HashSet::new();

    if let Some(categories) = payload.get("categories").and_then(Value::as_array) {
        for category in categories {
            let Some(category_name) =
                category.get("name").and_then(Value::as_str)
            else {
                continue;
            };

            let included = category
                .get("includedInDeck")
                .and_then(Value::as_bool)
                .unwrap_or(true);

            let premier = category
                .get("isPremier")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            if premier
                || category_name.eq_ignore_ascii_case("commander")
                || category_name.eq_ignore_ascii_case("command zone")
            {
                commander_categories.insert(category_name.to_string());
            } else if included
                && !category_name.eq_ignore_ascii_case("sideboard")
                && !category_name.eq_ignore_ascii_case("maybeboard")
            {
                included_categories.insert(category_name.to_string());
            } else {
                excluded_categories.insert(category_name.to_string());
            }
        }
    }

    let mut commanders = HashMap::<String, u32>::new();
    let mut main = HashMap::<String, u32>::new();
    let mut sideboard = HashMap::<String, u32>::new();

    for entry in payload
        .get("cards")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let quantity = entry
            .get("quantity")
            .and_then(Value::as_u64)
            .or_else(|| entry.get("qty").and_then(Value::as_u64))
            .unwrap_or(1) as u32;

        if quantity == 0 {
            continue;
        }

        let Some(card_name) = archidekt_card_name(entry) else {
            continue;
        };

        let categories = entry
            .get("categories")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let primary_category = entry
            .get("category")
            .and_then(Value::as_str)
            .map(str::to_string);

        let is_commander = categories
            .iter()
            .any(|category| commander_categories.contains(category))
            || primary_category
                .as_ref()
                .map(|category| commander_categories.contains(category))
                .unwrap_or(false);

        let is_sideboard = categories.iter().any(|category| {
            excluded_categories.contains(category)
                || category.eq_ignore_ascii_case("sideboard")
                || category.eq_ignore_ascii_case("maybeboard")
        }) || primary_category
            .as_ref()
            .map(|category| {
                excluded_categories.contains(category)
                    || category.eq_ignore_ascii_case("sideboard")
                    || category.eq_ignore_ascii_case("maybeboard")
            })
            .unwrap_or(false);

        let is_included = categories.is_empty()
            || categories
                .iter()
                .any(|category| included_categories.contains(category));

        let target = if is_commander {
            &mut commanders
        } else if is_sideboard {
            &mut sideboard
        } else if is_included {
            &mut main
        } else {
            &mut sideboard
        };

        *target.entry(card_name).or_default() += quantity;
    }

    Ok(ParsedDeck {
        name,
        commanders: map_entries(commanders),
        main: map_entries(main),
        sideboard: map_entries(sideboard),
        format: Default::default(),
        considering: Vec::new(),
    })
}

fn archidekt_card_name(entry: &Value) -> Option<String> {
    let card = entry.get("card")?;

    card.get("oracleCard")
        .and_then(|oracle| oracle.get("name"))
        .and_then(Value::as_str)
        .or_else(|| {
            card.get("oracle_card")
                .and_then(|oracle| oracle.get("name"))
                .and_then(Value::as_str)
        })
        .or_else(|| card.get("displayName").and_then(Value::as_str))
        .or_else(|| card.get("name").and_then(Value::as_str))
        .map(str::to_string)
}

fn map_entries(map: HashMap<String, u32>) -> Vec<DeckEntry> {
    let mut entries = map
        .into_iter()
        .map(|(name, quantity)| DeckEntry { quantity, name, ..Default::default() })
        .collect::<Vec<_>>();

    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    entries
}

fn extract_deck_id(url: &str) -> Option<String> {
    let trimmed = url.trim();

    if trimmed.chars().all(|c| c.is_ascii_digit()) && !trimmed.is_empty() {
        return Some(trimmed.to_string());
    }

    let parts = trimmed
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();

    let decks_index = parts
        .iter()
        .position(|part| part.eq_ignore_ascii_case("decks"))?;

    let id = parts.get(decks_index + 1)?;
    let numeric = id
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();

    (!numeric.is_empty()).then_some(numeric)
}
