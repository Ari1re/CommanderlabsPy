//! The Search tab's Scryfall searches: combining the user's search with the
//! deck's "fits this deck" filter, and suggesting searches that work when one
//! finds nothing (old card wording like "unblockable", tag names that don't
//! exist like otag:wrath).

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, fs, time::Duration};

const TAGS_PATH: &str = "./data/scryfall-oracle-tags.json";
const TAGS_MAX_AGE_SECS: u64 = 7 * 24 * 3600;

pub fn client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(25))
        .user_agent(concat!("CommanderLab/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

/// Split a search into terms, keeping quoted phrases together.
pub fn terms(q: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in q.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                cur.push(ch);
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn key_of(term: &str) -> Option<(String, String)> {
    let t = term.trim_start_matches(['-', '(', '!']).trim_end_matches(')');
    let idx = t.find([':', '<', '>', '='])?;
    let (k, rest) = t.split_at(idx);
    let v = rest.trim_start_matches([':', '<', '>', '=']);
    Some((k.to_ascii_lowercase(), v.trim_matches('"').to_string()))
}

/// The user's search plus the deck filter, grouped so `or` in the search
/// doesn't escape the filter, and without filter parts the search already sets.
pub fn compose(user: &str, filter: &str) -> String {
    let user = user.trim();
    if filter.trim().is_empty() {
        return user.to_string();
    }
    let keys: Vec<String> = terms(user).iter().filter_map(|t| key_of(t)).map(|(k, _)| k).collect();
    let sets_format = keys.iter().any(|k| matches!(k.as_str(), "f" | "format" | "legal" | "banned" | "restricted"));
    let sets_colors = keys.iter().any(|k| matches!(k.as_str(), "id" | "identity" | "ci" | "commander" | "cmd"));
    let kept: Vec<String> = terms(filter)
        .into_iter()
        .filter(|t| match key_of(t).map(|(k, _)| k) {
            Some(k) if matches!(k.as_str(), "f" | "format" | "legal") => !sets_format,
            Some(k) if matches!(k.as_str(), "id" | "identity" | "ci") => !sets_colors,
            _ => true,
        })
        .collect();
    if kept.is_empty() {
        user.to_string()
    } else {
        format!("({user}) {}", kept.join(" "))
    }
}

/// One page of results, or None when nothing matches.
pub fn search_page(client: &Client, q: &str, order: &str, dir: &str, unique: &str, page: u32) -> Result<Option<Value>> {
    let url = reqwest::Url::parse_with_params(
        "https://api.scryfall.com/cards/search",
        &[("q", q), ("order", order), ("dir", dir), ("unique", unique), ("page", &page.to_string())],
    )?;
    for attempt in 0..3u64 {
        let response = client.get(url.clone()).header("Accept", "application/json").send().context("Couldn't reach Scryfall")?;
        let status = response.status().as_u16();
        if status == 429 {
            std::thread::sleep(Duration::from_millis(800 * (attempt + 1)));
            continue;
        }
        let body: Value = response.json().context("Scryfall sent an unreadable answer")?;
        if status == 404 {
            return Ok(None);
        }
        if status >= 400 {
            return Err(anyhow!("{}", body.get("details").and_then(Value::as_str).unwrap_or("Scryfall couldn't run that search.")));
        }
        return Ok(Some(body));
    }
    Err(anyhow!("Scryfall is busy. Try again in a moment."))
}

fn count(client: &Client, q: &str) -> u64 {
    std::thread::sleep(Duration::from_millis(110));
    search_page(client, q, "edhrec", "auto", "cards", 1)
        .ok()
        .flatten()
        .and_then(|b| b.get("total_cards").and_then(Value::as_u64))
        .unwrap_or(0)
}

#[derive(Serialize, Deserialize, Default)]
struct TagCache {
    fetched_at: u64,
    /// Oracle tag label → number of cards.
    tags: HashMap<String, u64>,
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Scryfall's oracle tag names (from Tagger), kept for a week. The full list
/// is large, so it's only fetched when a search needs it.
fn oracle_tags(client: &Client) -> HashMap<String, u64> {
    let cached: Option<TagCache> = fs::read_to_string(TAGS_PATH).ok().and_then(|t| serde_json::from_str(&t).ok());
    if let Some(c) = &cached {
        if now().saturating_sub(c.fetched_at) < TAGS_MAX_AGE_SECS && !c.tags.is_empty() {
            return c.tags.clone();
        }
    }
    let fresh = client
        .get("https://api.scryfall.com/private/tags/oracle")
        .header("Accept", "application/json")
        .timeout(Duration::from_secs(60))
        .send()
        .ok()
        .filter(|r| r.status().is_success())
        .and_then(|r| r.json::<Value>().ok())
        .map(|v| {
            v.get("data")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|t| {
                    let label = t.get("label")?.as_str()?.to_string();
                    let n = t.get("oracle_ids").and_then(Value::as_array).map(|a| a.len() as u64).unwrap_or(0);
                    Some((label, n))
                })
                .collect::<HashMap<_, _>>()
        });
    match fresh {
        Some(tags) if !tags.is_empty() => {
            let _ = fs::create_dir_all("./data");
            let _ = fs::write(TAGS_PATH, serde_json::to_string(&TagCache { fetched_at: now(), tags: tags.clone() }).unwrap_or_default());
            tags
        }
        _ => cached.map(|c| c.tags).unwrap_or_default(),
    }
}

/// Words people search card text for that cards don't (or no longer) say,
/// and what finds them.
fn wording(word: &str) -> &'static [&'static str] {
    match word {
        "unblockable" => &["o:\"can't be blocked\"", "otag:unblockable"],
        "etb" | "cip" | "enters the battlefield" | "comes into play" | "enters-the-battlefield" => &["o:enters"],
        "ltb" => &["o:\"leaves the battlefield\""],
        "card draw" | "draw cards" | "carddraw" | "card-draw" => &["otag:draw"],
        "removal" | "spot removal" => &["otag:removal"],
        "ramp" => &["otag:ramp"],
        "wipe" | "wrath" | "boardwipe" | "board wipe" | "board-wipe" | "sweeper" | "mass removal" => &["otag:board-wipe"],
        "tutor" | "tutors" => &["otag:tutor"],
        "counterspell" | "counter spell" | "counters spells" => &["otag:counterspell"],
        "blink" | "flicker" => &["otag:flicker"],
        "anthem" => &["otag:anthem"],
        "lifegain" | "life gain" | "gain life" => &["otag:lifegain"],
        "sac outlet" | "sacrifice outlet" | "sac-outlet" => &["otag:sacrifice-outlet"],
        "mana rock" | "manarock" => &["otag:mana-rock"],
        "mana dork" | "dork" => &["otag:mana-dork"],
        "evasion" => &["otag:evasion"],
        "graveyard hate" => &["otag:graveyard-hate"],
        "stax" => &["otag:stax"],
        _ => &[],
    }
}

fn slug(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Real tags that look like `wanted`: sharing its words, most-used first.
fn close_tags(wanted: &str, tags: &HashMap<String, u64>) -> Vec<String> {
    // Generic words ("card", "spell") would match hundreds of unrelated tags.
    const GENERIC: &[&str] = &["card", "cards", "spell", "spells", "creature", "creatures", "the", "and", "your", "type", "matters"];
    let parts: Vec<&str> = wanted.split('-').filter(|p| p.len() >= 3 && !GENERIC.contains(p)).collect();
    if parts.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(&String, u64, usize)> = tags
        .iter()
        .filter_map(|(label, n)| {
            let shared = parts.iter().filter(|p| label.split('-').any(|w| w == **p || (w.len() >= 4 && (w.starts_with(**p) || p.starts_with(w))))).count();
            (shared > 0).then_some((label, *n, shared))
        })
        .collect();
    hits.sort_by(|a, b| b.2.cmp(&a.2).then(b.1.cmp(&a.1)));
    hits.into_iter().take(3).map(|(l, _, _)| l.clone()).collect()
}

#[derive(Serialize)]
pub struct Suggestion {
    /// The user's search with one term changed (without the deck filter).
    pub q: String,
    /// Why, in a few words.
    pub reason: String,
    pub total: u64,
}

/// Searches like the user's that find cards, for when theirs found none.
pub fn suggestions(client: &Client, user: &str, filter: &str) -> Vec<Suggestion> {
    let ts = terms(user);
    let mut candidates: Vec<(String, String)> = Vec::new();
    let mut tags: Option<HashMap<String, u64>> = None;

    for (i, term) in ts.iter().enumerate() {
        let negated = term.starts_with('-');
        let Some((key, value)) = key_of(term) else { continue };
        let replace = |with: &str| {
            let mut t = ts.clone();
            t[i] = if negated { format!("-{with}") } else { with.to_string() };
            t.join(" ")
        };
        let value_l = value.to_lowercase();
        match key.as_str() {
            "o" | "oracle" | "fo" | "fulloracle" => {
                for w in wording(&value_l) {
                    candidates.push((replace(w), format!("cards say it differently than \"{value}\"")));
                }
                if wording(&value_l).is_empty() {
                    let tags = tags.get_or_insert_with(|| oracle_tags(client));
                    let s = slug(&value_l);
                    if tags.contains_key(&s) {
                        candidates.push((replace(&format!("otag:{s}")), format!("Scryfall's \"{s}\" tag")));
                    }
                }
            }
            "otag" | "oracletag" | "function" | "atag" | "arttag" | "art" => {
                for w in wording(&value_l) {
                    if w.starts_with("otag:") {
                        candidates.push((replace(w), "the tag's name on Scryfall".into()));
                    }
                }
                if key.starts_with('o') || key == "function" {
                    let tags = tags.get_or_insert_with(|| oracle_tags(client));
                    if !tags.contains_key(&slug(&value_l)) {
                        for t in close_tags(&slug(&value_l), tags) {
                            candidates.push((replace(&format!("otag:{t}")), "a similar tag".into()));
                        }
                    }
                }
            }
            "kw" | "keyword" => {
                candidates.push((replace(&format!("o:\"{value}\"")), "search the card text instead".into()));
                for w in wording(&value_l) {
                    candidates.push((replace(w), format!("cards say it differently than \"{value}\"")));
                }
            }
            _ => {}
        }
    }

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (q, reason) in candidates {
        if out.len() >= 4 || !seen.insert(q.clone()) {
            continue;
        }
        let total = count(client, &compose(&q, filter));
        if total > 0 {
            out.push(Suggestion { q, reason, total });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_wraps_the_search() {
        assert_eq!(compose("otag:ramp or otag:draw", "f:commander id<=U"), "(otag:ramp or otag:draw) f:commander id<=U");
        assert_eq!(compose("t:elf f:pauper", "f:commander id<=G"), "(t:elf f:pauper) id<=G");
        assert_eq!(compose("id:esper t:angel", "f:commander id<=WU"), "(id:esper t:angel) f:commander");
        assert_eq!(compose("t:elf", ""), "t:elf");
    }

    #[test]
    fn terms_keep_quotes() {
        assert_eq!(terms(r#"o:"can't be blocked" t:creature"#), vec![r#"o:"can't be blocked""#, "t:creature"]);
        assert_eq!(key_of("-o:unblockable"), Some(("o".into(), "unblockable".into())));
    }

    #[test]
    fn close_tags_share_words() {
        let tags: HashMap<String, u64> = [("draw".to_string(), 4000), ("draw-engine".to_string(), 300), ("ramp".to_string(), 2000)].into();
        assert_eq!(close_tags("card-draw", &tags), vec!["draw".to_string(), "draw-engine".to_string()]);
    }
}
