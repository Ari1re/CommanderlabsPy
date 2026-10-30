use crate::deck::ParsedDeck;
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde_json::{json, Value};

const URL: &str = "https://backend.commanderspellbook.com/find-my-combos";

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ComboLine {
    pub id: String,
    pub cards: Vec<String>,
    pub result: String,
    pub initial_state: Vec<String>,
    pub prerequisites: Vec<String>,
    pub steps: Vec<String>,
    pub spellbook_url: String,
    pub bracket: Option<u8>,
    pub almost: bool,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ComboReport {
    pub included: usize,
    pub almost_included: usize,
    pub summary_lines: Vec<String>,
    pub variants: Vec<ComboLine>,
}

pub fn find_my_combos(deck: &ParsedDeck) -> Result<ComboReport> {
    let commanders = deck
        .commanders
        .iter()
        .map(|x| json!({"card": x.name.clone(), "quantity": x.quantity}))
        .collect::<Vec<_>>();

    let main = deck
        .main
        .iter()
        .map(|x| json!({"card": x.name.clone(), "quantity": x.quantity}))
        .collect::<Vec<_>>();

    let body = json!({
        "commanders": commanders,
        "main": main
    });

    let response: Value = Client::new()
        .post(URL)
        .json(&body)
        .send()
        .context("Could not contact Commander Spellbook")?
        .error_for_status()
        .context("Commander Spellbook returned an HTTP error")?
        .json()
        .context("Could not parse Commander Spellbook response")?;

    let payload = response.get("results").unwrap_or(&response);

    let included_items = payload
        .get("included")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let almost_items = payload
        .get("almostIncluded")
        .or_else(|| payload.get("almost_included"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let included = included_items.len();
    let almost_included = almost_items.len();

    let mut variants = Vec::new();
    let mut summary_lines = Vec::new();

    for item in included_items.iter().take(25) {
        if let Some(combo) = parse_variant(item, false) {
            summary_lines.push(format!(
                "✓ {}{}",
                combo.cards.join(" + "),
                if combo.result.is_empty() {
                    String::new()
                } else {
                    format!(" → {}", combo.result)
                }
            ));
            variants.push(combo);
        }
    }

    for item in almost_items.iter().take(25) {
        if let Some(combo) = parse_variant(item, true) {
            summary_lines.push(format!(
                "+1 {}{}",
                combo.cards.join(" + "),
                if combo.result.is_empty() {
                    String::new()
                } else {
                    format!(" → {}", combo.result)
                }
            ));
            variants.push(combo);
        }
    }

    if summary_lines.is_empty() {
        summary_lines.push(format!(
            "Spellbook reported {included} included combo(s) and {almost_included} almost-included combo(s)."
        ));
    }

    Ok(ComboReport {
        included,
        almost_included,
        summary_lines,
        variants,
    })
}

fn parse_variant(item: &Value, almost: bool) -> Option<ComboLine> {
    let cards = extract_cards(item);

    if cards.is_empty() {
        return None;
    }

    let id = extract_id(item);
    let result = extract_result(item);
    let initial_state = extract_initial_state(item);
    let prerequisites = extract_prerequisites(item);
    let steps = extract_steps(item);
    let bracket = extract_bracket(item);

    let spellbook_url = if id.is_empty() {
        "https://commanderspellbook.com/find-my-combos/".to_string()
    } else {
        format!("https://commanderspellbook.com/combo/{id}/")
    };

    Some(ComboLine {
        id,
        cards,
        result,
        initial_state,
        prerequisites,
        steps,
        spellbook_url,
        bracket,
        almost,
    })
}

fn extract_id(item: &Value) -> String {
    for key in ["id", "variantId", "variant_id"] {
        if let Some(value) = item.get(key) {
            if let Some(text) = value.as_str() {
                return text.to_string();
            }

            if let Some(number) = value.as_u64() {
                return number.to_string();
            }
        }
    }

    String::new()
}

fn extract_initial_state(item: &Value) -> Vec<String> {
    let mut out = Vec::new();

    for key in [
        "uses",
        "initialState",
        "initial_state",
        "cardState",
        "card_state",
    ] {
        if let Some(value) = item.get(key) {
            collect_display_strings(value, &mut out, 0);
        }
    }

    out.retain(|line| line.trim().len() >= 2);
    out.dedup();
    out.truncate(12);
    out
}

fn extract_prerequisites(item: &Value) -> Vec<String> {
    let mut out = Vec::new();

    for key in [
        "requires",
        "prerequisites",
        "otherPrerequisites",
        "other_prerequisites",
    ] {
        if let Some(value) = item.get(key) {
            collect_display_strings(value, &mut out, 0);
        }
    }

    out.sort();
    out.dedup();
    out.truncate(10);
    out
}

fn extract_steps(item: &Value) -> Vec<String> {
    let mut out = Vec::new();

    for key in [
        "steps",
        "recipe",
        "description",
        "notes",
        "manaNeeded",
        "mana_needed",
    ] {
        if let Some(value) = item.get(key) {
            collect_display_strings(value, &mut out, 0);
        }
    }

    out.retain(|line| line.trim().len() >= 3);
    out.dedup();
    out.truncate(12);
    out
}

fn collect_display_strings(
    value: &Value,
    output: &mut Vec<String>,
    depth: usize,
) {
    if depth > 4 {
        return;
    }

    match value {
        Value::String(text) => {
            let text = text.trim();

            if !text.is_empty() {
                output.push(text.to_string());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_display_strings(item, output, depth + 1);
            }
        }
        Value::Object(map) => {
            for preferred in ["name", "description", "text", "value"] {
                if let Some(child) = map.get(preferred) {
                    collect_display_strings(child, output, depth + 1);
                }
            }
        }
        Value::Number(number) => output.push(number.to_string()),
        _ => {}
    }
}

fn extract_cards(item: &Value) -> Vec<String> {
    let from_uses = item
        .get("uses")
        .and_then(Value::as_array)
        .map(|uses| {
            uses.iter()
                .filter_map(|use_item| {
                    use_item
                        .get("card")
                        .and_then(|card| {
                            card.get("name")
                                .and_then(Value::as_str)
                                .or_else(|| card.as_str())
                        })
                        .map(str::to_string)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if !from_uses.is_empty() {
        return from_uses;
    }

    item.get("cards")
        .and_then(Value::as_array)
        .map(|cards| {
            cards
                .iter()
                .filter_map(|card| {
                    card.as_str()
                        .or_else(|| card.get("name").and_then(Value::as_str))
                })
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn extract_result(item: &Value) -> String {
    if let Some(produces) = item.get("produces").and_then(Value::as_array) {
        let parts = produces
            .iter()
            .filter_map(|produce| {
                produce
                    .get("feature")
                    .and_then(|feature| {
                        feature
                            .get("name")
                            .and_then(Value::as_str)
                            .or_else(|| feature.as_str())
                    })
                    .or_else(|| produce.get("name").and_then(Value::as_str))
            })
            .take(4)
            .collect::<Vec<_>>();

        if !parts.is_empty() {
            return parts.join(", ");
        }
    }

    item.get("result")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn extract_bracket(item: &Value) -> Option<u8> {
    for key in ["bracket", "bracketTag", "bracket_tag"] {
        if let Some(value) = item.get(key) {
            if let Some(number) = value.as_u64() {
                return u8::try_from(number).ok();
            }

            if let Some(text) = value.as_str() {
                if let Some(number) = bracket_from_text(text) {
                    return Some(number);
                }
            }

            if let Some(name) = value.get("name").and_then(Value::as_str) {
                if let Some(number) = bracket_from_text(name) {
                    return Some(number);
                }
            }
        }
    }

    recursive_bracket(item)
}

fn recursive_bracket(value: &Value) -> Option<u8> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let key_lower = key.to_ascii_lowercase();

                if key_lower.contains("bracket") {
                    if let Some(number) = child.as_u64() {
                        return u8::try_from(number).ok();
                    }

                    if let Some(text) = child.as_str() {
                        if let Some(number) = bracket_from_text(text) {
                            return Some(number);
                        }
                    }
                }

                if let Some(found) = recursive_bracket(child) {
                    return Some(found);
                }
            }
        }

        Value::Array(items) => {
            for item in items {
                if let Some(found) = recursive_bracket(item) {
                    return Some(found);
                }
            }
        }

        _ => {}
    }

    None
}

fn bracket_from_text(text: &str) -> Option<u8> {
    let lower = text.to_ascii_lowercase();

    if lower.contains("ruthless") || lower.contains("bracket 4") {
        Some(4)
    } else if lower.contains("spicy")
        || lower.contains("powerful")
        || lower.contains("bracket 3")
    {
        Some(3)
    } else if lower.contains("oddball")
        || lower.contains("core")
        || lower.contains("bracket 2")
    {
        Some(2)
    } else if lower.contains("exhibition") || lower.contains("bracket 1") {
        Some(1)
    } else {
        None
    }
}
