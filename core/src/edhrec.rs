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
    pub fn inclusion_pct(&self) -> f64 {
        if self.potential_decks == 0 {
            0.0
        } else {
            self.inclusion as f64 / self.potential_decks as f64
        }
    }
}

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

fn load_or_fetch_build_json(
    commander: &Card,
    selected_theme: Option<&str>,
) -> Result<Value> {
    let commander_slug = slugify(&commander.name);
    let theme_slug = selected_theme
        .map(slugify)
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

fn cache_path(slug: &str) -> PathBuf {
    let mut hasher =
        std::collections::hash_map::DefaultHasher::new();

    slug.hash(&mut hasher);

    PathBuf::from("./data/edhrec_cache")
        .join(format!("{:016x}.json", hasher.finish()))
}

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
