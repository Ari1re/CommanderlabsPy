use crate::{
    archidekt::parse_archidekt_payload,
    deck::{DeckEntry, ParsedDeck},
    logging,
    moxfield::import_moxfield,
    spellbook::find_my_combos,
};
use anyhow::{Context, Result};
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs,
    hash::{Hash, Hasher},
    path::PathBuf,
    thread,
    time::{Duration, SystemTime},
};

const USER_AGENT_VALUE: &str =
    concat!("Mozilla/5.0 CommanderLab/", env!("CARGO_PKG_VERSION"), " (local Commander deck analyzer)");
const CACHE_HOURS: u64 = 24;
const SEARCH_CACHE_HOURS: u64 = 8;
const SOURCE_DECK_CACHE_DAYS: u64 = 180;
const ARCHIDEKT_REFERENCE_LIMIT: usize = 24;
const MOXFIELD_REFERENCE_LIMIT: usize = 24;
const CEDH_REFERENCE_LIMIT: usize = 32;
const NEW_DECK_FETCH_BUDGET_PER_SOURCE: usize = 12;
const MAX_REFERENCE_DECKS: usize = 120;
const MAX_COMMANDER_ARCHIVE_DECKS: usize = 500;
const NORMAL_COMBO_ANALYSIS_LIMIT: usize = 24;
const CEDH_COMBO_ANALYSIS_LIMIT: usize = 36;
const CACHE_SCHEMA: u8 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceCombo {
    pub cards: Vec<String>,
    pub result: String,
    pub bracket: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceDeck {
    pub source: String,
    pub title: String,
    pub url: String,
    pub cards: Vec<String>,
    #[serde(default)]
    pub combos: Vec<ReferenceCombo>,
    #[serde(default)]
    pub combo_scan_complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReferenceSnapshot {
    pub query: String,
    pub decks: Vec<ReferenceDeck>,
    pub card_frequency: HashMap<String, u32>,
    #[serde(default)]
    pub card_names: HashMap<String, String>,
    #[serde(default)]
    pub pair_frequency: HashMap<String, u32>,
    #[serde(default)]
    pub pair_cards: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub combo_frequency: HashMap<String, u32>,
    #[serde(default)]
    pub combo_cards: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub combo_results: HashMap<String, String>,
    #[serde(default)]
    pub combo_brackets: HashMap<String, Option<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReferenceCrawlState {
    archidekt_page: usize,
    moxfield_page: usize,
    cedh_refreshes: u64,
}

impl Default for ReferenceCrawlState {
    fn default() -> Self {
        Self {
            archidekt_page: 1,
            moxfield_page: 1,
            cedh_refreshes: 0,
        }
    }
}


impl ReferenceSnapshot {
    pub fn frequency(&self, card_name: &str) -> u32 {
        self.card_frequency
            .get(&card_name.to_ascii_lowercase())
            .copied()
            .unwrap_or(0)
    }

    pub fn deck_count(&self) -> usize {
        self.decks.len()
    }

    pub fn top_cards(&self, limit: usize) -> Vec<(String, u32)> {
        let mut items = self
            .card_frequency
            .iter()
            .map(|(key, count)| {
                (
                    self.card_names
                        .get(key)
                        .cloned()
                        .unwrap_or_else(|| key.clone()),
                    *count,
                )
            })
            .collect::<Vec<_>>();

        items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        items.truncate(limit);
        items
    }


    pub fn package_frequency(&self, card_name: &str) -> u32 {
        let needle = card_name.to_ascii_lowercase();

        self.pair_cards
            .iter()
            .filter(|(_, cards)| {
                cards.iter().any(|name| {
                    name.eq_ignore_ascii_case(&needle)
                })
            })
            .filter_map(|(key, _)| self.pair_frequency.get(key).copied())
            .max()
            .unwrap_or(0)
    }

    pub fn top_pairs(
        &self,
        limit: usize,
        minimum_count: u32,
    ) -> Vec<(Vec<String>, u32)> {
        let mut items = self
            .pair_frequency
            .iter()
            .filter(|(_, count)| **count >= minimum_count)
            .filter_map(|(key, count)| {
                self.pair_cards
                    .get(key)
                    .cloned()
                    .map(|cards| (cards, *count))
            })
            .collect::<Vec<_>>();

        items.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| a.0.join(" + ").cmp(&b.0.join(" + ")))
        });
        items.truncate(limit);
        items
    }

    pub fn combo_frequency_for_cards(&self, cards: &[String]) -> u32 {
        let key = canonical_combo_key(cards);
        self.combo_frequency.get(&key).copied().unwrap_or(0)
    }

    pub fn top_combos(
        &self,
        limit: usize,
    ) -> Vec<(Vec<String>, String, Option<u8>, u32)> {
        let mut items = self
            .combo_frequency
            .iter()
            .filter_map(|(key, count)| {
                self.combo_cards.get(key).cloned().map(|cards| {
                    (
                        cards,
                        self.combo_results
                            .get(key)
                            .cloned()
                            .unwrap_or_default(),
                        self.combo_brackets
                            .get(key)
                            .copied()
                            .flatten(),
                        *count,
                    )
                })
            })
            .collect::<Vec<_>>();

        items.sort_by(|a, b| {
            b.3.cmp(&a.3)
                .then_with(|| a.0.join(" + ").cmp(&b.0.join(" + ")))
        });
        items.truncate(limit);
        items
    }
}

pub fn load_cached_meta_reference_decks(
    limit: usize,
) -> Vec<ReferenceDeck> {
    let directory =
        PathBuf::from("./data/source_cache/reference_decks");

    let Ok(entries) = fs::read_dir(&directory) else {
        return Vec::new();
    };

    let mut decks = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            fs::read_to_string(entry.path()).ok()
        })
        .filter_map(|text| {
            serde_json::from_str::<ReferenceDeck>(&text).ok()
        })
        .filter(|deck| {
            deck.cards.len() >= 70 && !deck.combos.is_empty()
        })
        .collect::<Vec<_>>();

    decks.sort_by(|a, b| {
        let a_cedh = a.source.to_ascii_lowercase().contains("cedh")
            || a.title.to_ascii_lowercase().contains("cedh");
        let b_cedh = b.source.to_ascii_lowercase().contains("cedh")
            || b.title.to_ascii_lowercase().contains("cedh");

        b_cedh
            .cmp(&a_cedh)
            .then_with(|| b.combos.len().cmp(&a.combos.len()))
            .then_with(|| a.title.cmp(&b.title))
    });

    let mut seen = HashSet::<String>::new();
    decks.retain(|deck| {
        let key = canonical_reference_url(&deck.url);
        key.is_empty() || seen.insert(key)
    });
    decks.truncate(limit.max(1));
    decks
}

pub fn invalidate_reference_snapshot(
    commander_names: &[String],
    bracket: u8,
) -> Result<()> {
    let query = commander_names.join(" + ");
    let path = cache_path(&query, bracket);

    if path.exists() {
        fs::remove_file(&path).with_context(|| {
            format!(
                "Could not invalidate reference snapshot {}",
                path.display(),
            )
        })?;
    }

    logging::record(
        "REF",
        format!(
            "FORCED CORPUS REFRESH • query={} • bracket=B{} • preserved per-source cache/archive/crawl state",
            query,
            bracket,
        ),
    );

    Ok(())
}

pub fn gather_reference_decks(
    commander_names: &[String],
    bracket: u8,
) -> ReferenceSnapshot {
    let query = commander_names.join(" + ");

    if let Some(mut cached) = load_cache(&query, bracket) {
            // Reconcile snapshot data into the per-source cache.
        hydrate_reference_decks_from_source_cache(
            &mut cached.decks,
        );

        let mut archive = load_commander_archive(&query);
        archive.extend(cached.decks.clone());
        dedupe_reference_decks(&mut archive);
        let _ = save_commander_archive(&query, &archive);

        logging::record(
            "REF",
            format!(
                "CORPUS CACHE HIT • query={} • bracket=B{} • decks={} • pairs={} • combos={} • commander_archive={} • migrated_to_source_cache=true",
                query,
                bracket,
                cached.deck_count(),
                cached.pair_frequency.len(),
                cached.combo_frequency.len(),
                archive.len(),
            ),
        );
        return cached;
    }

    // Reuse stale snapshots as historical evidence; spend network budget on new material.
    let mut decks = load_commander_archive(&query);

    if let Some(stale_snapshot) =
        load_cache_any_age(&query, bracket)
    {
        decks.extend(stale_snapshot.decks);
    }

    dedupe_reference_decks(&mut decks);

    hydrate_reference_decks_from_source_cache(&mut decks);

    logging::record(
        "REF",
        format!(
            "CORPUS REFRESH • query={} • bracket=B{} • retained_cached_decks={} • max_archive={}",
            query,
            bracket,
            decks.len(),
            MAX_REFERENCE_DECKS,
        ),
    );

    let mut crawl = load_crawl_state(&query);

    let archidekt = fetch_archidekt_references(
        commander_names,
        ARCHIDEKT_REFERENCE_LIMIT,
        crawl.archidekt_page,
    );

    logging::record(
        "REF",
        format!(
            "ARCHIDEKT DISCOVERY • page={} • usable={} • next_page={}",
            crawl.archidekt_page,
            archidekt.len(),
            crawl.archidekt_page.saturating_add(1),
        ),
    );

    decks.extend(archidekt);
    crawl.archidekt_page = if crawl.archidekt_page >= 20 {
        1
    } else {
        crawl.archidekt_page + 1
    };

    let moxfield = fetch_moxfield_references(
        commander_names,
        MOXFIELD_REFERENCE_LIMIT,
        crawl.moxfield_page,
    );

    logging::record(
        "REF",
        format!(
            "MOXFIELD DISCOVERY • page={} • usable={} • next_page={}",
            crawl.moxfield_page,
            moxfield.len(),
            crawl.moxfield_page.saturating_add(1),
        ),
    );

    decks.extend(moxfield);
    crawl.moxfield_page = if crawl.moxfield_page >= 20 {
        1
    } else {
        crawl.moxfield_page + 1
    };

    if bracket >= 5 {
        let cedh = fetch_cedh_ddb_references(
            commander_names,
            CEDH_REFERENCE_LIMIT,
        );

        logging::record(
            "REF",
            format!(
                "CEDH DISCOVERY • usable={} • refresh_number={}",
                cedh.len(),
                crawl.cedh_refreshes.saturating_add(1),
            ),
        );

        decks.extend(cedh);
        crawl.cedh_refreshes = crawl.cedh_refreshes.saturating_add(1);
    }

    let _ = save_crawl_state(&query, &crawl);

    dedupe_reference_decks(&mut decks);

    let _ = save_commander_archive(&query, &decks);

    // Bound the active corpus; older decks remain in the archive.
    if decks.len() > MAX_REFERENCE_DECKS {
        let excess = decks.len() - MAX_REFERENCE_DECKS;
        decks.drain(0..excess);

        logging::record(
            "REF",
            format!(
                "ACTIVE CORPUS ROTATE • removed_oldest={} • active_archive={} • evicted decks remain in per-source cache",
                excess,
                decks.len(),
            ),
        );
    }

    if bracket >= 5 {
        decks.sort_by_key(|deck| {
            if deck.source.to_ascii_lowercase().contains("cedh") {
                0
            } else {
                1
            }
        });
    }

    let combo_scan_limit = if bracket >= 5 {
        CEDH_COMBO_ANALYSIS_LIMIT
    } else {
        NORMAL_COMBO_ANALYSIS_LIMIT
    };

    let already_scanned = decks
        .iter()
        .filter(|deck| deck.combo_scan_complete)
        .count();

    logging::record(
        "REF",
        format!(
            "SPELLBOOK INCREMENTAL SCAN START • archive={} • already_scanned={} • new_scan_budget={}",
            decks.len(),
            already_scanned,
            combo_scan_limit,
        ),
    );

    analyze_reference_combos(
        commander_names,
        &mut decks,
        combo_scan_limit,
    );

    let scanned_total = decks
        .iter()
        .filter(|deck| deck.combo_scan_complete)
        .count();

    let decks_with_combos = decks
        .iter()
        .filter(|deck| !deck.combos.is_empty())
        .count();

    let detected_combos = decks
        .iter()
        .map(|deck| deck.combos.len())
        .sum::<usize>();

    logging::record(
        "REF",
        format!(
            "SPELLBOOK INCREMENTAL SCAN COMPLETE • scanned_total={} • decks_with_combos={} • combo_occurrences={}",
            scanned_total,
            decks_with_combos,
            detected_combos,
        ),
    );

    let _ = merge_commander_archive(
        &query,
        &decks,
    );

    let snapshot = build_snapshot(query.clone(), decks);

    logging::record(
        "REF",
        format!(
            "CORPUS COMPLETE • query={} • decks={} • common_cards={} • recurring_pairs={} • combo_patterns={} • next_arch_page={} • next_mox_page={}",
            query,
            snapshot.deck_count(),
            snapshot.card_frequency.len(),
            snapshot.pair_frequency.len(),
            snapshot.combo_frequency.len(),
            crawl.archidekt_page,
            crawl.moxfield_page,
        ),
    );

    let _ = save_cache(&query, bracket, &snapshot);
    snapshot
}


fn fetch_archidekt_references(
    commander_names: &[String],
    limit: usize,
    page: usize,
) -> Vec<ReferenceDeck> {
    let Some(primary) = commander_names.first() else {
        return Vec::new();
    };

    let client = match Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
    {
        Ok(client) => client,
        Err(_) => return Vec::new(),
    };

    let encoded = urlencoding::encode(primary);
    let page = page.max(1);

    let search_urls = [
        format!(
            "https://archidekt.com/api/decks/v3/?deckFormat=3&commanderName={encoded}&orderBy=-viewCount&page={page}&pageSize={}",
            limit.max(10),
        ),
        format!(
            "https://archidekt.com/api/decks/v3/?deckFormat=3&name={encoded}&orderBy=-viewCount&page={page}&pageSize={}",
            limit.max(10),
        ),
    ];

    let mut ids = Vec::<(String, String)>::new();

    for search_url in search_urls {
        let Some(value) = fetch_json_with_search_cache(
            &client,
            &search_url,
            false,
        ) else {
            continue;
        };

        collect_archidekt_summaries(&value, &mut ids);

        if !ids.is_empty() {
            break;
        }
    }

    ids.truncate(limit);

    let mut out = Vec::new();
    let mut network_fetches = 0usize;

    for (id, title) in ids {
        let public_url = format!("https://archidekt.com/decks/{id}");

        if let Some(cached) = load_cached_reference_deck(&public_url) {
            if reference_deck_matches_commanders(
                &cached,
                commander_names,
            ) {
                out.push(cached);
            }
            continue;
        }

        if network_fetches >= NEW_DECK_FETCH_BUDGET_PER_SOURCE {
            break;
        }

        network_fetches += 1;
        thread::sleep(Duration::from_millis(300));

        let api_url = format!("https://archidekt.com/api/decks/{id}/");

        let Ok(response) = client
            .get(&api_url)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ACCEPT, "application/json")
            .send()
        else {
            continue;
        };

        let Ok(value) = response
            .error_for_status()
            .and_then(|r| r.json::<Value>())
        else {
            continue;
        };

        let Ok(deck) = parse_archidekt_payload(&value, &title) else {
            continue;
        };

        if !deck_matches_commanders(&deck, commander_names) {
            continue;
        }

        let reference = reference_from_parsed(
            "Archidekt",
            title,
            public_url,
            &deck,
        );

        let _ = save_cached_reference_deck(&reference);
        out.push(reference);
    }

    logging::record(
        "REF",
        format!(
            "ARCHIDEKT FETCH BUDGET • page={} • network_deck_fetches={} • returned={}",
            page,
            network_fetches,
            out.len(),
        ),
    );

    out
}

fn fetch_moxfield_references(
    commander_names: &[String],
    limit: usize,
    page: usize,
) -> Vec<ReferenceDeck> {
    let Some(primary) = commander_names.first() else {
        return Vec::new();
    };

    let client = match Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
    {
        Ok(client) => client,
        Err(_) => return Vec::new(),
    };

    let encoded = urlencoding::encode(primary);
    let page = page.max(1);

    let url = format!(
        "https://api2.moxfield.com/v2/decks/search?pageNumber={page}&pageSize={}&sortType=Views&sortDirection=Descending&q={}&fmt=commander&commander={}",
        limit.max(5),
        encoded,
        encoded,
    );

    let Some(value) = fetch_json_with_search_cache(
        &client,
        &url,
        true,
    ) else {
        return Vec::new();
    };

    let summaries = value
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| value.get("results").and_then(Value::as_array))
        .cloned()
        .unwrap_or_default();

    let mut out = Vec::new();
    let mut network_fetches = 0usize;

    for summary in summaries.into_iter().take(limit) {
        let Some(public_id) = summary
            .get("publicId")
            .and_then(Value::as_str)
            .or_else(|| summary.get("public_id").and_then(Value::as_str))
        else {
            continue;
        };

        let title = summary
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(public_id)
            .to_string();

        let deck_url = format!(
            "https://www.moxfield.com/decks/{public_id}"
        );

        if let Some(cached) = load_cached_reference_deck(&deck_url) {
            if reference_deck_matches_commanders(
                &cached,
                commander_names,
            ) {
                out.push(cached);
            }
            continue;
        }

        if network_fetches >= NEW_DECK_FETCH_BUDGET_PER_SOURCE {
            break;
        }

        network_fetches += 1;
        thread::sleep(Duration::from_millis(400));

        let Ok(deck) = import_moxfield(&deck_url) else {
            continue;
        };

        if !deck_matches_commanders(&deck, commander_names) {
            continue;
        }

        let reference = reference_from_parsed(
            "Moxfield",
            title,
            deck_url,
            &deck,
        );

        let _ = save_cached_reference_deck(&reference);
        out.push(reference);
    }

    logging::record(
        "REF",
        format!(
            "MOXFIELD FETCH BUDGET • page={} • network_deck_fetches={} • returned={}",
            page,
            network_fetches,
            out.len(),
        ),
    );

    out
}

fn fetch_cedh_ddb_references(
    commander_names: &[String],
    limit: usize,
) -> Vec<ReferenceDeck> {
    let client = match Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
    {
        Ok(client) => client,
        Err(_) => return Vec::new(),
    };

    let Some(html) = fetch_text_with_search_cache(
        &client,
        "https://cedh-decklist-database.com/",
    ) else {
        return Vec::new();
    };

    let re = Regex::new(
        r#"https?://(?:www\.)?moxfield\.com/decks/[A-Za-z0-9_-]+"#,
    )
    .expect("valid Moxfield URL regex");

    let lower = html.to_ascii_lowercase();
    let mut urls = HashSet::new();

    for commander in commander_names {
        let needle = commander.to_ascii_lowercase();
        let mut start = 0usize;

        while let Some(relative) = lower[start..].find(&needle) {
            let pos = start + relative;
            let left = pos.saturating_sub(3500);
            let right = (pos + 6500).min(html.len());

            for hit in re.find_iter(&html[left..right]) {
                urls.insert(hit.as_str().to_string());

                if urls.len() >= limit {
                    break;
                }
            }

            if urls.len() >= limit {
                break;
            }

            start = pos + needle.len();
        }
    }

    let mut out = Vec::new();
    let mut network_fetches = 0usize;

    for url in urls.into_iter().take(limit) {
        if let Some(mut cached) = load_cached_reference_deck(&url) {
            if reference_deck_matches_commanders(
                &cached,
                commander_names,
            ) {
                cached.source = "cEDH DDB / Moxfield".to_string();
                out.push(cached);
            }
            continue;
        }

        if network_fetches >= NEW_DECK_FETCH_BUDGET_PER_SOURCE {
            break;
        }

        network_fetches += 1;
        thread::sleep(Duration::from_millis(450));

        let Ok(deck) = import_moxfield(&url) else {
            continue;
        };

        if !deck_matches_commanders(&deck, commander_names) {
            continue;
        }

        let reference = reference_from_parsed(
            "cEDH DDB / Moxfield",
            deck.name.clone(),
            url,
            &deck,
        );

        let _ = save_cached_reference_deck(&reference);
        out.push(reference);
    }

    logging::record(
        "REF",
        format!(
            "CEDH FETCH BUDGET • network_deck_fetches={} • returned={}",
            network_fetches,
            out.len(),
        ),
    );

    out
}

fn analyze_reference_combos(
    commander_names: &[String],
    decks: &mut [ReferenceDeck],
    limit: usize,
) {
    let mut newly_scanned = 0usize;

    for deck in decks.iter_mut() {
        if deck.combo_scan_complete {
            continue;
        }

        if newly_scanned >= limit {
            break;
        }

        newly_scanned += 1;

        let commander_entries = commander_names
            .iter()
            .map(|name| DeckEntry {
                quantity: 1,
                name: name.clone(),
            })
            .collect::<Vec<_>>();

        let commander_set = commander_names
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .collect::<HashSet<_>>();

        let main = deck
            .cards
            .iter()
            .filter(|name| {
                !commander_set.contains(&name.to_ascii_lowercase())
            })
            .map(|name| DeckEntry {
                quantity: 1,
                name: name.clone(),
            })
            .collect::<Vec<_>>();

        let parsed = ParsedDeck {
            name: deck.title.clone(),
            commanders: commander_entries,
            main,
            sideboard: Vec::new(),
        };

        match find_my_combos(&parsed) {
            Ok(report) => {
                deck.combos = report
                    .variants
                    .into_iter()
                    .filter(|combo| !combo.almost)
                    .take(12)
                    .map(|combo| ReferenceCombo {
                        cards: combo.cards,
                        result: combo.result,
                        bracket: combo.bracket,
                    })
                    .collect();

                // This matters even when combos is empty. Without the explicit
                // bit, a legitimately combo-free deck would be re-submitted
                // to Spellbook forever.
                deck.combo_scan_complete = true;
                let _ = save_cached_reference_deck(deck);
            }
            Err(err) => {
                logging::record(
                    "REF",
                    format!(
                        "SPELLBOOK SCAN DEFERRED • deck={} • source={} • error={}",
                        deck.title,
                        deck.source,
                        err,
                    ),
                );
            }
        }

        thread::sleep(Duration::from_millis(140));
    }

    logging::record(
        "REF",
        format!(
            "SPELLBOOK NEW WORK • newly_scanned={} • budget={}",
            newly_scanned,
            limit,
        ),
    );
}

fn collect_archidekt_summaries(
    value: &Value,
    output: &mut Vec<(String, String)>,
) {
    match value {
        Value::Object(map) => {
            let id = map
                .get("id")
                .and_then(Value::as_u64)
                .map(|id| id.to_string())
                .or_else(|| {
                    map.get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                });

            if let Some(id) = id {
                let title = map
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(&id)
                    .to_string();

                if !output.iter().any(|(existing, _)| existing == &id) {
                    output.push((id, title));
                }
            }

            for child in map.values() {
                collect_archidekt_summaries(child, output);
            }
        }

        Value::Array(items) => {
            for item in items {
                collect_archidekt_summaries(item, output);
            }
        }

        _ => {}
    }
}

fn reference_from_parsed(
    source: &str,
    title: String,
    url: String,
    deck: &ParsedDeck,
) -> ReferenceDeck {
    let mut cards = deck
        .commanders
        .iter()
        .chain(deck.main.iter())
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>();

    cards.sort();
    cards.dedup();

    ReferenceDeck {
        source: source.to_string(),
        title,
        url,
        cards,
        combos: Vec::new(),
        combo_scan_complete: false,
    }
}

fn deck_matches_commanders(
    deck: &ParsedDeck,
    commander_names: &[String],
) -> bool {
    commander_names.iter().all(|expected| {
        deck.commanders
            .iter()
            .any(|entry| entry.name.eq_ignore_ascii_case(expected))
            || deck
                .name
                .to_ascii_lowercase()
                .contains(&expected.to_ascii_lowercase())
    })
}

fn build_snapshot(
    query: String,
    decks: Vec<ReferenceDeck>,
) -> ReferenceSnapshot {
    let commander_set = query
        .split(" + ")
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty())
        .collect::<HashSet<_>>();

    let mut card_frequency = HashMap::<String, u32>::new();
    let mut card_names = HashMap::<String, String>::new();
    let mut pair_frequency = HashMap::<String, u32>::new();
    let mut pair_cards = HashMap::<String, Vec<String>>::new();
    let mut combo_frequency = HashMap::<String, u32>::new();
    let mut combo_cards = HashMap::<String, Vec<String>>::new();
    let mut combo_results = HashMap::<String, String>::new();
    let mut combo_brackets = HashMap::<String, Option<u8>>::new();

    for deck in &decks {
        let mut seen = HashSet::<String>::new();
        let mut non_commanders = Vec::<String>::new();

        for card in &deck.cards {
            let key = card.to_ascii_lowercase();
            card_names.entry(key.clone()).or_insert_with(|| card.clone());

            if seen.insert(key.clone()) {
                *card_frequency.entry(key.clone()).or_default() += 1;

                if !commander_set.contains(&key) {
                    non_commanders.push(card.clone());
                }
            }
        }

        non_commanders.sort_by_key(|name| name.to_ascii_lowercase());
        non_commanders.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

        // Pair frequencies capture packages: cards that repeatedly occur
        // together across independently-authored lists.
        for left in 0..non_commanders.len() {
            for right in (left + 1)..non_commanders.len() {
                let cards = vec![
                    non_commanders[left].clone(),
                    non_commanders[right].clone(),
                ];
                let key = canonical_combo_key(&cards);

                *pair_frequency.entry(key.clone()).or_default() += 1;
                pair_cards.entry(key).or_insert(cards);
            }
        }

        let mut seen_combos = HashSet::<String>::new();

        for combo in &deck.combos {
            let key = canonical_combo_key(&combo.cards);

            if key.is_empty() || !seen_combos.insert(key.clone()) {
                continue;
            }

            *combo_frequency.entry(key.clone()).or_default() += 1;
            combo_cards
                .entry(key.clone())
                .or_insert_with(|| combo.cards.clone());

            if !combo.result.trim().is_empty() {
                combo_results
                    .entry(key.clone())
                    .or_insert_with(|| combo.result.clone());
            }

            combo_brackets
                .entry(key)
                .or_insert(combo.bracket);
        }
    }

    pair_frequency.retain(|_, count| *count >= 2);
    pair_cards.retain(|key, _| pair_frequency.contains_key(key));

    ReferenceSnapshot {
        query,
        decks,
        card_frequency,
        card_names,
        pair_frequency,
        pair_cards,
        combo_frequency,
        combo_cards,
        combo_results,
        combo_brackets,
    }
}

fn canonical_combo_key(cards: &[String]) -> String {
    let mut names = cards
        .iter()
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();

    names.sort();
    names.dedup();
    names.join("\u{1f}")
}

fn dedupe_reference_decks(decks: &mut Vec<ReferenceDeck>) {
    let mut seen = HashSet::new();

    decks.retain(|deck| {
        let key = if !deck.url.is_empty() {
            canonical_reference_url(&deck.url)
        } else {
            format!(
                "{}:{}",
                deck.source.to_ascii_lowercase(),
                deck.title.to_ascii_lowercase()
            )
        };

        seen.insert(key)
    });
}

fn reference_deck_matches_commanders(
    deck: &ReferenceDeck,
    commander_names: &[String],
) -> bool {
    commander_names.iter().all(|expected| {
        deck.cards
            .iter()
            .any(|name| name.eq_ignore_ascii_case(expected))
            || deck
                .title
                .to_ascii_lowercase()
                .contains(&expected.to_ascii_lowercase())
    })
}

fn source_key(value: &str) -> u64 {
    let mut hasher =
        std::collections::hash_map::DefaultHasher::new();
    value.to_ascii_lowercase().hash(&mut hasher);
    hasher.finish()
}

fn canonical_reference_url(url: &str) -> String {
    url.trim()
        .trim_end_matches('/')
        .replace("https://www.", "https://")
        .replace("http://www.", "http://")
        .to_ascii_lowercase()
}

fn source_deck_cache_path(url: &str) -> PathBuf {
    let canonical = canonical_reference_url(url);

    PathBuf::from("./data/source_cache/reference_decks")
        .join(format!(
            "{:016x}.json",
            source_key(&canonical),
        ))
}

fn load_cached_reference_deck(
    url: &str,
) -> Option<ReferenceDeck> {
    let path = source_deck_cache_path(url);
    let metadata = fs::metadata(&path).ok()?;
    let modified = metadata.modified().ok()?;
    let age = SystemTime::now()
        .duration_since(modified)
        .ok()?;

    let text = fs::read_to_string(&path).ok()?;
    let deck = serde_json::from_str::<ReferenceDeck>(&text).ok()?;

    if age <= Duration::from_secs(
        SOURCE_DECK_CACHE_DAYS * 24 * 60 * 60,
    ) {
        logging::record(
            "CACHE",
            format!(
                "REFERENCE DECK HIT • source={} • title={} • url={}",
                deck.source,
                deck.title,
                deck.url,
            ),
        );
    } else {
        // Deliberately use stale reference-deck data rather than hammering the
        // host. A decklist is historical evidence and remains useful after its
        // freshness window. A future discovery of the same URL can replace it
        // if we later add explicit revalidation.
        logging::record(
            "CACHE",
            format!(
                "REFERENCE DECK STALE-USE • age_days>{} • source={} • title={}",
                SOURCE_DECK_CACHE_DAYS,
                deck.source,
                deck.title,
            ),
        );
    }

    Some(deck)
}

fn save_cached_reference_deck(
    deck: &ReferenceDeck,
) -> Result<()> {
    if deck.url.trim().is_empty() {
        return Ok(());
    }

    let path = source_deck_cache_path(&deck.url);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(
        &path,
        serde_json::to_string_pretty(deck)?,
    )
    .with_context(|| {
        format!(
            "Could not write source deck cache {}",
            path.display(),
        )
    })?;

    Ok(())
}

fn search_cache_path(url: &str, extension: &str) -> PathBuf {
    PathBuf::from("./data/source_cache/search_pages")
        .join(format!(
            "{:016x}.{}",
            source_key(url),
            extension,
        ))
}

fn search_cache_is_fresh(path: &PathBuf) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };

    let Ok(modified) = metadata.modified() else {
        return false;
    };

    let Ok(age) =
        SystemTime::now().duration_since(modified)
    else {
        return false;
    };

    age <= Duration::from_secs(
        SEARCH_CACHE_HOURS * 60 * 60,
    )
}

fn fetch_json_with_search_cache(
    client: &Client,
    url: &str,
    moxfield_headers: bool,
) -> Option<Value> {
    let path = search_cache_path(url, "json");
    let cached_value = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());

    if search_cache_is_fresh(&path) {
        if let Some(value) = cached_value.clone() {
            logging::record(
                "CACHE",
                format!("SEARCH PAGE HIT • url={url}"),
            );
            return Some(value);
        }
    }

    let mut request = client
        .get(url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(
            ACCEPT,
            "application/json, text/plain, */*",
        );

    if moxfield_headers {
        request = request.header(
            REFERER,
            "https://www.moxfield.com/",
        );
    }

    let fetched = request
        .send()
        .ok()
        .and_then(|response| response.error_for_status().ok())
        .and_then(|response| response.json::<Value>().ok());

    if let Some(value) = fetched {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if let Ok(text) = serde_json::to_string_pretty(&value) {
            let _ = fs::write(&path, text);
        }

        logging::record(
            "CACHE",
            format!("SEARCH PAGE MISS/FETCH • url={url}"),
        );

        return Some(value);
    }

    if let Some(value) = cached_value {
        logging::record(
            "CACHE",
            format!(
                "SEARCH PAGE STALE-FALLBACK • host unavailable/rejected request • url={url}"
            ),
        );
        return Some(value);
    }

    None
}

fn fetch_text_with_search_cache(
    client: &Client,
    url: &str,
) -> Option<String> {
    let path = search_cache_path(url, "txt");
    let cached_text = fs::read_to_string(&path).ok();

    if search_cache_is_fresh(&path) {
        if let Some(text) = cached_text.clone() {
            logging::record(
                "CACHE",
                format!("SEARCH PAGE HIT • url={url}"),
            );
            return Some(text);
        }
    }

    let fetched = client
        .get(url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "text/html")
        .send()
        .ok()
        .and_then(|response| response.error_for_status().ok())
        .and_then(|response| response.text().ok());

    if let Some(text) = fetched {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let _ = fs::write(&path, &text);

        logging::record(
            "CACHE",
            format!("SEARCH PAGE MISS/FETCH • url={url}"),
        );

        return Some(text);
    }

    if let Some(text) = cached_text {
        logging::record(
            "CACHE",
            format!(
                "SEARCH PAGE STALE-FALLBACK • host unavailable/rejected request • url={url}"
            ),
        );
        return Some(text);
    }

    None
}

fn hydrate_reference_decks_from_source_cache(
    decks: &mut [ReferenceDeck],
) {
    for deck in decks {
        if deck.url.trim().is_empty() {
            continue;
        }

        let preferred_source = deck.source.clone();

        if let Some(mut cached) =
            load_cached_reference_deck(&deck.url)
        {
            if preferred_source
                .to_ascii_lowercase()
                .contains("cedh")
            {
                cached.source = preferred_source;
            }

            *deck = cached;
        } else {
            if !deck.combos.is_empty() {
                deck.combo_scan_complete = true;
            }

            let _ = save_cached_reference_deck(deck);
        }
    }
}

fn merge_commander_archive(
    query: &str,
    updated: &[ReferenceDeck],
) -> Result<()> {
    let mut archive = load_commander_archive(query);

    for deck in updated {
        let key = if !deck.url.trim().is_empty() {
            deck.url.to_ascii_lowercase()
        } else {
            format!(
                "{}:{}",
                deck.source.to_ascii_lowercase(),
                deck.title.to_ascii_lowercase(),
            )
        };

        if let Some(existing) = archive.iter_mut().find(|existing| {
            let existing_key = if !existing.url.trim().is_empty() {
                existing.url.to_ascii_lowercase()
            } else {
                format!(
                    "{}:{}",
                    existing.source.to_ascii_lowercase(),
                    existing.title.to_ascii_lowercase(),
                )
            };

            existing_key == key
        }) {
            *existing = deck.clone();
        } else {
            archive.push(deck.clone());
        }
    }

    save_commander_archive(query, &archive)
}

fn commander_archive_path(query: &str) -> PathBuf {
    PathBuf::from("./data/source_cache/commander_archives")
        .join(format!(
            "{:016x}.json",
            source_key(query),
        ))
}

fn load_commander_archive(
    query: &str,
) -> Vec<ReferenceDeck> {
    let path = commander_archive_path(query);

    fs::read_to_string(path)
        .ok()
        .and_then(|text| {
            serde_json::from_str::<Vec<ReferenceDeck>>(
                &text,
            )
            .ok()
        })
        .unwrap_or_default()
}

fn save_commander_archive(
    query: &str,
    decks: &[ReferenceDeck],
) -> Result<()> {
    let path = commander_archive_path(query);
    let mut archive = decks.to_vec();

    dedupe_reference_decks(&mut archive);

    if archive.len() > MAX_COMMANDER_ARCHIVE_DECKS {
        let excess =
            archive.len() - MAX_COMMANDER_ARCHIVE_DECKS;
        archive.drain(0..excess);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(
        &path,
        serde_json::to_string_pretty(&archive)?,
    )
    .with_context(|| {
        format!(
            "Could not write commander source archive {}",
            path.display(),
        )
    })?;

    logging::record(
        "CACHE",
        format!(
            "COMMANDER ARCHIVE SAVE • query={} • decks={}",
            query,
            archive.len(),
        ),
    );

    Ok(())
}

fn crawl_state_path(query: &str) -> PathBuf {
    PathBuf::from("./data/source_cache/crawl_state")
        .join(format!(
            "{:016x}.json",
            source_key(query),
        ))
}

fn load_crawl_state(
    query: &str,
) -> ReferenceCrawlState {
    let path = crawl_state_path(query);

    fs::read_to_string(path)
        .ok()
        .and_then(|text| {
            serde_json::from_str::<ReferenceCrawlState>(
                &text,
            )
            .ok()
        })
        .unwrap_or_default()
}

fn save_crawl_state(
    query: &str,
    state: &ReferenceCrawlState,
) -> Result<()> {
    let path = crawl_state_path(query);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(
        &path,
        serde_json::to_string_pretty(state)?,
    )
    .with_context(|| {
        format!(
            "Could not write crawl state {}",
            path.display(),
        )
    })?;

    Ok(())
}

fn cache_path(query: &str, bracket: u8) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    query.to_ascii_lowercase().hash(&mut hasher);
    bracket.hash(&mut hasher);

    PathBuf::from("./data/reference_cache")
        .join(format!(
            "v{}_{}_{:016x}.json",
            CACHE_SCHEMA,
            bracket,
            hasher.finish()
        ))
}

fn load_cache(query: &str, bracket: u8) -> Option<ReferenceSnapshot> {
    let path = cache_path(query, bracket);
    let metadata = fs::metadata(&path).ok()?;
    let modified = metadata.modified().ok()?;
    let age = SystemTime::now().duration_since(modified).ok()?;

    if age > Duration::from_secs(CACHE_HOURS * 60 * 60) {
        return None;
    }

    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn load_cache_any_age(
    query: &str,
    bracket: u8,
) -> Option<ReferenceSnapshot> {
    let path = cache_path(query, bracket);
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_cache(
    query: &str,
    bracket: u8,
    snapshot: &ReferenceSnapshot,
) -> Result<()> {
    let path = cache_path(query, bracket);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(
        &path,
        serde_json::to_string_pretty(snapshot)?,
    )
    .with_context(|| format!("Could not write {}", path.display()))?;

    Ok(())
}
