//! EDHREC data: what decks with this commander play.
//!
//! How it fits: commander and theme pages are fetched as JSON (cached under
//! `data/edhrec_cache`) and turned into [`EdhrecCardSignal`]s (inclusion count,
//! potential decks, synergy) and themes. Generation, suggestions and the
//! agent read them; Pauper Commander uses PDHREC instead (`format_sources.rs`).
//!
//! Things to work on:
//! - EDHREC's JSON layout isn't a public API and changes occasionally; keep
//!   the parser tolerant and log when expected fields are missing.
//! - Synergy is the strongest "fits this commander" signal; peer-based
//!   generation still ranks mostly by inclusion.

use crate::{data::Card, logging};
use anyhow::Result;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, USER_AGENT};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    hash::{Hash, Hasher},
    path::PathBuf,
    time::{Duration, SystemTime},
};

const USER_AGENT_VALUE: &str =
    concat!("CommanderLab/", env!("CARGO_PKG_VERSION"), " (local Commander deck analyzer)");
const CACHE_HOURS: u64 = 72;

#[derive(Debug, Clone, serde::Serialize)]
pub struct EdhrecTheme {
    pub name: String,
    pub slug: String,
    pub deck_count: u64,
    pub rank: usize,
}


#[derive(Debug, Clone)]
pub struct EdhrecCardSignal {
    pub name: String,
    pub synergy: f64,
    pub inclusion: u64,
    pub potential_decks: u64,
    pub category: String,
}

impl EdhrecCardSignal {
    /// Share of this commander's EDHREC decks that run the card (0–1).
    pub fn inclusion_pct(&self) -> f64 {
        if self.potential_decks == 0 {
            0.0
        } else {
            self.inclusion as f64 / self.potential_decks as f64
        }
    }
}

/// EDHREC's cards for a commander: one theme page per chosen theme (merged),
/// or the base commander page when no theme page has data. Each card keeps
/// its strongest signal. Partners are fetched one commander at a time (not
/// the pair's own page), then merged.
pub fn commander_build_cards(
    commanders: &[Card],
    selected_theme: &str,
    limit: usize,
) -> Vec<EdhrecCardSignal> {
    use std::collections::HashMap;

    let mut merged = HashMap::<String, EdhrecCardSignal>::new();

    let themes = crate::archetype::theme_list(selected_theme);

    for commander in commanders {
        // One EDHREC theme page per selected play style, merged.
        let mut themed_cards = Vec::new();
        for theme in &themes {
            if let Ok(value) = load_or_fetch_build_json(commander, Some(theme)) {
                themed_cards.extend(extract_build_cards(&value));
            }
        }
        let themed = Some(themed_cards).filter(|cards| !cards.is_empty());

        let cards = themed.unwrap_or_else(|| {
            load_or_fetch_build_json(commander, None)
                .ok()
                .map(|value| extract_build_cards(&value))
                .unwrap_or_default()
        });

        for card in cards {
            let key = card.name.to_ascii_lowercase();

            match merged.get(&key) {
                Some(existing)
                    if build_signal_score(existing)
                        >= build_signal_score(&card) => {}
                _ => {
                    merged.insert(key, card);
                }
            }
        }
    }

    let mut cards = merged.into_values().collect::<Vec<_>>();
    cards.sort_by(|a, b| {
        build_signal_score(b)
            .partial_cmp(&build_signal_score(a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    cards.truncate(limit);
    cards
}

/// Rank cards from EDHREC pages: category weight (high synergy first) plus
/// inclusion and synergy.
fn build_signal_score(card: &EdhrecCardSignal) -> f64 {
    let category_bonus = match card.category.as_str() {
        "highsynergycards" => 2.0,
        "topcards" => 1.6,
        "gamechangers" => 1.3,
        "creatures" => 1.1,
        "utilityartifacts" | "enchantments" | "instants" | "sorceries" => 1.0,
        "manaartifacts" | "utilitylands" | "lands" => 0.9,
        _ => 0.7,
    };

    category_bonus
        + card.synergy.max(-0.5) * 4.0
        + card.inclusion_pct() * 3.0
}

/// A commander's EDHREC page JSON (theme page or base page), cached under
/// `data/edhrec_cache`; stale cache is used if EDHREC can't be reached.
fn load_or_fetch_build_json(
    commander: &Card,
    selected_theme: Option<&str>,
) -> Result<Value> {
    let commander_slug = slugify(&commander.name);
    let theme_slug = selected_theme
        .map(theme_slug)
        .filter(|slug| !slug.is_empty());

    let cache_key = match &theme_slug {
        Some(theme) => format!("{commander_slug}__theme__{theme}"),
        None => format!("{commander_slug}__base"),
    };

    let path = PathBuf::from("./data/edhrec_cache")
        .join(format!("build__{cache_key}.json"));

    let stale_cached = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());

    if cache_is_fresh(&path) {
        if let Some(value) = stale_cached.clone() {
            logging::record(
                "CACHE",
                format!(
                    "EDHREC BUILD HIT • commander={} • theme={}",
                    commander.name,
                    selected_theme.unwrap_or("BASE"),
                ),
            );
            return Ok(value);
        }
    }

    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()?;

    let mut urls = Vec::new();

    if let Some(theme) = &theme_slug {
        urls.push(format!(
            "https://json.edhrec.com/pages/commanders/{commander_slug}/{theme}.json"
        ));
    }

    urls.push(format!(
        "https://json.edhrec.com/pages/commanders/{commander_slug}.json"
    ));

    let mut last_error = None;

    for url in urls {
        match client
            .get(&url)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ACCEPT, "application/json")
            .send()
        {
            Ok(response) => match response.error_for_status() {
                Ok(response) => match response.json::<Value>() {
                    Ok(value) => {
                        if let Some(parent) = path.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        let _ = fs::write(
                            &path,
                            serde_json::to_string_pretty(&value)?,
                        );
                        logging::record(
                            "CACHE",
                            format!(
                                "EDHREC BUILD FETCH • commander={} • theme={} • url={}",
                                commander.name,
                                selected_theme.unwrap_or("BASE"),
                                url,
                            ),
                        );
                        return Ok(value);
                    }
                    Err(err) => last_error = Some(err.to_string()),
                },
                Err(err) => last_error = Some(err.to_string()),
            },
            Err(err) => last_error = Some(err.to_string()),
        }
    }

    if let Some(value) = stale_cached {
        logging::record(
            "CACHE",
            format!(
                "EDHREC BUILD STALE-FALLBACK • commander={} • theme={} • error={}",
                commander.name,
                selected_theme.unwrap_or("BASE"),
                last_error
                    .clone()
                    .unwrap_or_else(|| "unknown error".to_string()),
            ),
        );
        return Ok(value);
    }

    Err(anyhow::anyhow!(
        "Could not fetch EDHREC commander build data: {}",
        last_error.unwrap_or_else(|| "unknown error".to_string())
    ))
}

/// Pull card signals (name, synergy, inclusion, deck count, category) out
/// of an EDHREC page's card lists, wherever they sit in the JSON.
fn extract_build_cards(value: &Value) -> Vec<EdhrecCardSignal> {
    use std::collections::HashMap;

    let Some(cardlists) = value
        .get("container")
        .and_then(|v| v.get("json_dict"))
        .and_then(|v| v.get("cardlists"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    let mut merged = HashMap::<String, EdhrecCardSignal>::new();

    for list in cardlists {
        let category = list
            .get("tag")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase();

        if matches!(
            category.as_str(),
            "newcards" | "similar" | "topcommanders" | "newcommanders"
        ) {
            continue;
        }

        let Some(cardviews) = list.get("cardviews").and_then(Value::as_array)
        else {
            continue;
        };

        for view in cardviews {
            let Some(name) = view.get("name").and_then(Value::as_str) else {
                continue;
            };

            // Skip partner/commander arrays and malformed non-card rows.
            if name.trim().is_empty() {
                continue;
            }

            let signal = EdhrecCardSignal {
                name: name.to_string(),
                synergy: view
                    .get("synergy")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                inclusion: view
                    .get("num_decks")
                    .and_then(Value::as_u64)
                    .or_else(|| view.get("inclusion").and_then(Value::as_u64))
                    .unwrap_or(0),
                potential_decks: view
                    .get("potential_decks")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                category: category.clone(),
            };

            let key = signal.name.to_ascii_lowercase();

            match merged.get(&key) {
                Some(existing)
                    if build_signal_score(existing)
                        >= build_signal_score(&signal) => {}
                _ => {
                    merged.insert(key, signal);
                }
            }
        }
    }

    merged.into_values().collect()
}

/// EDHREC's theme tags for the commander(s), most decks first, renumbered
/// 1..n, de-duplicated across partners.
pub fn commander_themes(
    commanders: &[Card],
    limit: usize,
) -> Vec<EdhrecTheme> {
    let mut combined = Vec::<EdhrecTheme>::new();
    let mut seen = HashSet::<String>::new();

    for commander in commanders {
        let themes = load_or_fetch_commander_themes(commander)
            .unwrap_or_default();

        for theme in themes {
            let key = theme.name.to_ascii_lowercase();

            if seen.insert(key) {
                combined.push(theme);
            }
        }
    }

    combined.sort_by(|a, b| {
        b.deck_count
            .cmp(&a.deck_count)
            .then_with(|| a.rank.cmp(&b.rank))
            .then_with(|| a.name.cmp(&b.name))
    });

    for (index, theme) in combined.iter_mut().enumerate() {
        theme.rank = index + 1;
    }

    combined.truncate(limit);
    combined
}

/// One play style from EDHREC's site-wide tag index (edhrec.com/tags):
/// every theme ("Tokens", "Aristocrats") and typal ("Elves") EDHREC tracks.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EdhrecTag {
    pub name: String,
    /// URL slug as EDHREC writes it ("plus-1-plus-1-counters").
    pub slug: String,
    pub deck_count: u64,
    /// From the Typal list (creature types) rather than Themes.
    pub typal: bool,
}

/// Every theme and typal tag on EDHREC, most decks first. Fetched from
/// `json.edhrec.com/pages/tags/{themes,typal}.json` and cached in
/// `data/edhrec_cache/tag_index.json` for [`CACHE_HOURS`]; a stale cache is
/// used offline. Empty when EDHREC was never reached.
pub fn all_tags() -> Vec<EdhrecTag> {
    let path = PathBuf::from("./data/edhrec_cache/tag_index.json");
    let cached = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<EdhrecTag>>(&text).ok())
        .filter(|tags| !tags.is_empty());

    if cache_is_fresh(&path) {
        if let Some(tags) = cached.clone() {
            return tags;
        }
    }

    match fetch_tag_index() {
        Ok(tags) if !tags.is_empty() => {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(text) = serde_json::to_string(&tags) {
                let _ = fs::write(&path, text);
            }
            logging::record("CACHE", format!("EDHREC TAG INDEX FETCH • {} tags", tags.len()));
            tags
        }
        result => {
            if let Err(err) = &result {
                logging::record("CACHE", format!("EDHREC TAG INDEX STALE-FALLBACK • error={err}"));
            }
            cached.unwrap_or_default()
        }
    }
}

/// Download both tag lists and merge them, typal marked, by deck count.
fn fetch_tag_index() -> Result<Vec<EdhrecTag>> {
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::limited(8))
        .timeout(Duration::from_secs(20))
        .build()?;
    let mut tags = Vec::<EdhrecTag>::new();

    for (page, typal) in [("themes", false), ("typal", true)] {
        let url = format!("https://json.edhrec.com/pages/tags/{page}.json");
        let value = client
            .get(&url)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ACCEPT, "application/json")
            .send()?
            .error_for_status()?
            .json::<Value>()?;
        tags.extend(parse_tag_index(&value, typal));
    }

    tags.sort_by(|a, b| b.deck_count.cmp(&a.deck_count).then_with(|| a.name.cmp(&b.name)));
    let mut seen = HashSet::new();
    tags.retain(|tag| seen.insert(tag.name.to_ascii_lowercase()));
    Ok(tags)
}

/// Read a tag index page: `container.json_dict.cardlists[].cardviews[]`,
/// each with `name`, `url` ("/tags/plus-1-plus-1-counters") and `num_decks`.
/// The entry's own `slug` field is its cover card, not the tag, so the
/// slug comes from `url`.
fn parse_tag_index(value: &Value, typal: bool) -> Vec<EdhrecTag> {
    let lists = value
        .pointer("/container/json_dict/cardlists")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    lists
        .iter()
        .filter_map(|list| list.get("cardviews").and_then(Value::as_array))
        .flatten()
        .filter_map(|entry| {
            let name = entry.get("name").and_then(Value::as_str)?.trim().to_string();
            if !valid_theme_name(&name) {
                return None;
            }
            let slug = entry
                .get("url")
                .and_then(Value::as_str)
                .and_then(|url| url.rsplit('/').next())
                .filter(|slug| !slug.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| fallback_theme_slug(&name));
            Some(EdhrecTag {
                name,
                slug,
                deck_count: entry.get("num_decks").and_then(Value::as_u64).unwrap_or(0),
                typal,
            })
        })
        .collect()
}

/// EDHREC's slug for a theme name: the tag index's own slug when the name
/// is listed (so "Miracles" → "miracle"), else [`fallback_theme_slug`].
pub fn theme_slug(name: &str) -> String {
    let name = name.trim();
    all_tags()
        .into_iter()
        .find(|tag| tag.name.eq_ignore_ascii_case(name))
        .map(|tag| tag.slug)
        .unwrap_or_else(|| fallback_theme_slug(name))
}

/// Slug without the index: EDHREC spells counter signs out
/// ("+1/+1 Counters" → "plus-1-plus-1-counters", "-1/-1" → "minus-1-minus-1")
/// and drops apostrophes ("Dragon's Approach" → "dragons-approach").
fn fallback_theme_slug(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut spelled = String::new();
    for (i, c) in chars.iter().enumerate() {
        let before_digit = chars.get(i + 1).is_some_and(|n| n.is_ascii_digit());
        match c {
            '+' if before_digit => spelled.push_str(" plus "),
            '-' if before_digit => spelled.push_str(" minus "),
            '\'' | '’' => {}
            _ => spelled.push(*c),
        }
    }
    slugify(&spelled)
}

/// Theme tags from the commander's EDHREC page (cached).
fn load_or_fetch_commander_themes(
    commander: &Card,
) -> Result<Vec<EdhrecTheme>> {
    let slug = slugify(&commander.name);
    let cache_path = cache_path(&slug);

    let stale_cached = fs::read_to_string(&cache_path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());

    if cache_is_fresh(&cache_path) {
        if let Some(value) = stale_cached.clone() {
            logging::record(
                "CACHE",
                format!(
                    "EDHREC THEMES HIT • commander={}",
                    commander.name,
                ),
            );
            return Ok(parse_themes(&value, &slug));
        }
    }

    let url = format!(
        "https://json.edhrec.com/pages/commanders/{slug}.json"
    );

    let fetch_result = Client::builder()
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .and_then(|client| {
            client
                .get(&url)
                .header(USER_AGENT, USER_AGENT_VALUE)
                .header(ACCEPT, "application/json")
                .send()
        })
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.json::<Value>());

    match fetch_result {
        Ok(value) => {
            if let Some(parent) = cache_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let _ = fs::write(
                &cache_path,
                serde_json::to_string_pretty(&value)?,
            );

            logging::record(
                "CACHE",
                format!(
                    "EDHREC THEMES FETCH • commander={} • url={}",
                    commander.name,
                    url,
                ),
            );

            Ok(parse_themes(&value, &slug))
        }
        Err(err) => {
            if let Some(value) = stale_cached {
                logging::record(
                    "CACHE",
                    format!(
                        "EDHREC THEMES STALE-FALLBACK • commander={} • error={}",
                        commander.name,
                        err,
                    ),
                );
                Ok(parse_themes(&value, &slug))
            } else {
                Err(anyhow::anyhow!(
                    "Could not fetch EDHREC commander themes: {err}"
                ))
            }
        }
    }
}

/// Read theme tags and their deck counts from an EDHREC page.
fn parse_themes(
    value: &Value,
    commander_slug: &str,
) -> Vec<EdhrecTheme> {
    // Current EDHREC commander JSON exposes the commander's ranked tags at:
    //
    //     panels.taglinks[]
    //
    // with entries containing `value`, `slug`, and `count`.
    if let Some(taglinks) = value
        .get("panels")
        .and_then(|panels| panels.get("taglinks"))
        .and_then(Value::as_array)
    {
        let mut themes = taglinks
            .iter()
            .filter_map(|entry| {
                let name = entry
                    .get("value")
                    .and_then(Value::as_str)
                    .or_else(|| entry.get("name").and_then(Value::as_str))?
                    .trim()
                    .to_string();

                if !valid_theme_name(&name) {
                    return None;
                }

                let slug = entry
                    .get("slug")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| slugify(&name));

                let deck_count = entry
                    .get("count")
                    .and_then(Value::as_u64)
                    .or_else(|| {
                        entry
                            .get("num_decks")
                            .and_then(Value::as_u64)
                    })
                    .unwrap_or(0);

                Some(EdhrecTheme {
                    name,
                    slug,
                    deck_count,
                    rank: 0,
                })
            })
            .collect::<Vec<_>>();

        themes.sort_by(|a, b| {
            b.deck_count
                .cmp(&a.deck_count)
                .then_with(|| a.name.cmp(&b.name))
        });

        themes.dedup_by(|a, b| {
            a.name.eq_ignore_ascii_case(&b.name)
        });

        for (index, theme) in themes.iter_mut().enumerate() {
            theme.rank = index + 1;
        }

        if !themes.is_empty() {
            return themes;
        }
    }

    // Compatibility fallback for older/different EDHREC payloads.
    let mut names = Vec::<String>::new();
    collect_theme_panels(value, commander_slug, &mut names);

    let mut seen = HashSet::<String>::new();

    names
        .into_iter()
        .filter(|name| seen.insert(name.to_ascii_lowercase()))
        .enumerate()
        .map(|(index, name)| EdhrecTheme {
            slug: slugify(&name),
            name,
            deck_count: 0,
            rank: index + 1,
        })
        .collect()
}

/// Walk the page JSON for theme links (`/commanders/<slug>/<theme>`).
fn collect_theme_panels(
    value: &Value,
    commander_slug: &str,
    output: &mut Vec<String>,
) {
    match value {
        Value::Object(map) => {
            let header = map
                .get("header")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();

            if header == "themes"
                || header == "theme"
                || header == "tags"
            {
                if let Some(items) = map.get("items").and_then(Value::as_array)
                {
                    for item in items {
                        let href = item
                            .get("href")
                            .and_then(Value::as_str)
                            .unwrap_or_default();

                        let value = item
                            .get("value")
                            .and_then(Value::as_str)
                            .or_else(|| {
                                item.get("name").and_then(Value::as_str)
                            });

                        let relevant_href = href.starts_with(&format!(
                            "/commanders/{commander_slug}/"
                        ));

                        if relevant_href {
                            if let Some(name) = value {
                                if valid_theme_name(name) {
                                    output.push(name.trim().to_string());
                                }
                            } else if let Some(slug) = href.rsplit('/').next() {
                                let name = title_from_slug(slug);

                                if valid_theme_name(&name) {
                                    output.push(name);
                                }
                            }
                        }
                    }
                }
            }

            // EDHREC's JSON format changes occasionally. Also scan any item
            // whose href is clearly a commander/theme subpage even if the
            // surrounding panel isn't named exactly "Themes".
            if let Some(href) = map.get("href").and_then(Value::as_str) {
                let prefix = format!("/commanders/{commander_slug}/");

                if href.starts_with(&prefix) {
                    if let Some(name) = map
                        .get("value")
                        .and_then(Value::as_str)
                        .or_else(|| map.get("name").and_then(Value::as_str))
                    {
                        if valid_theme_name(name) {
                            output.push(name.trim().to_string());
                        }
                    }
                }
            }

            for child in map.values() {
                collect_theme_panels(child, commander_slug, output);
            }
        }

        Value::Array(items) => {
            for item in items {
                collect_theme_panels(item, commander_slug, output);
            }
        }

        _ => {}
    }
}

/// Skip EDHREC's non-theme links (All, Average Deck, Budget, bracket numbers).
fn valid_theme_name(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();

    !lower.is_empty()
        && lower != "all"
        && lower != "average deck"
        && lower != "as commander"
        && lower != "as card"
        && lower != "budget"
        && !matches!(
            lower.as_str(),
            "1" | "2" | "3" | "4" | "5"
        )
}

/// EDHREC's URL slug: lowercase letters and digits joined by dashes.
/// TODO: non-ASCII letters are dropped, not folded: "Éowyn" becomes
/// "owyn" instead of EDHREC's "eowyn", so accented commanders get no EDHREC
/// data. Running `data::fold_name` first would fix it.
fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;

    for c in value.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !slug.is_empty() {
            slug.push('-');
            dash = true;
        }
    }

    slug.trim_matches('-').to_string()
}

/// "plus-one-counters" → "Plus One Counters" (for theme names from links).
fn title_from_slug(slug: &str) -> String {
    slug.split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();

            match chars.next() {
                Some(first) => format!(
                    "{}{}",
                    first.to_ascii_uppercase(),
                    chars.as_str()
                ),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Theme cache file for a commander slug.
/// Same caveat as references.rs: DefaultHasher names change between Rust
/// versions.
fn cache_path(slug: &str) -> PathBuf {
    let mut hasher =
        std::collections::hash_map::DefaultHasher::new();

    slug.hash(&mut hasher);

    PathBuf::from("./data/edhrec_cache")
        .join(format!("{:016x}.json", hasher.finish()))
}

/// EDHREC data is reused for a few days.
fn cache_is_fresh(path: &PathBuf) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };

    let Ok(modified) = metadata.modified() else {
        return false;
    };

    let Ok(age) = SystemTime::now().duration_since(modified) else {
        return false;
    };

    age <= Duration::from_secs(CACHE_HOURS * 60 * 60)
}

#[cfg(test)]
mod tag_tests {
    use super::*;

    #[test]
    fn fallback_slugs_match_edhrec_urls() {
        assert_eq!(fallback_theme_slug("+1/+1 Counters"), "plus-1-plus-1-counters");
        assert_eq!(fallback_theme_slug("-1/-1 Counters"), "minus-1-minus-1-counters");
        assert_eq!(fallback_theme_slug("Dragon's Approach"), "dragons-approach");
        assert_eq!(fallback_theme_slug("Self-Mill"), "self-mill");
        assert_eq!(fallback_theme_slug("X Spells"), "x-spells");
    }

    #[test]
    fn tag_index_takes_slug_from_url() {
        let page = serde_json::json!({ "container": { "json_dict": { "cardlists": [{ "cardviews": [
            { "name": "Miracles", "slug": "temporal-mastery", "url": "/tags/miracle", "num_decks": 900 },
            { "name": "Elves", "slug": "elvish-archdruid", "url": "/tags/elves", "num_decks": 16575 }
        ]}]}}});
        let tags = parse_tag_index(&page, true);
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].slug, "miracle");
        assert_eq!(tags[1].deck_count, 16575);
        assert!(tags[1].typal);
    }
}
