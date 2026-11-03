//! Reference decklists: other players' decks for a commander.
//!
//! How it fits: `intelligence.rs` calls [`gather_reference_decks`] during
//! generation; the cards those decks share become "commonly played"
//! evidence, and their recurring combos become win packages. Decks are
//! cached per site under `data/source_cache/` and merged into a
//! per-commander archive that grows across runs.
//!
//! Sources:
//! - Archidekt (format 3 Commander, 17 Pauper EDH), by commander name, most
//!   viewed first. [`ReferenceDepth`] controls how many pages and decks a run
//!   reads (deep generation reads more than peer-based).
//! - Moxfield search. Currently returns nothing useful: its search ignores
//!   the commander filter and returns the site's most viewed decks, which the
//!   commander check then throws away.
//! - Bracket 5: the cEDH Decklist Database (Commander), or Archidekt Pauper
//!   EDH lists titled as competitive (cPDH, competitive, tournament…).
//!   [`is_cedh_source`] marks these so they're weighted first.
//!
//! Things to work on:
//! - Replace or remove the Moxfield search; it costs a request every run for
//!   nothing.
//! - Archidekt deck fetches often fail the commander check (12 fetched, 5
//!   kept for Drana). Log why each is rejected; some may be partner or
//!   companion layouts the check doesn't understand.
//! - Competitive Pauper detection is title-based and finds few lists. A
//!   curated list of cPDH sources would be more reliable.
//! - The cache never expires old decks for archetype shifts (new sets). Age
//!   out decks older than a year or weight them lower.

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

/// How much reference material one run gathers.
#[derive(Debug, Clone, Copy)]
pub struct ReferenceDepth {
    /// New decks fetched per source page.
    pub fetch_budget: usize,
    /// Archidekt search pages read per run.
    pub pages: u32,
    /// A cached snapshot with fewer decks than this is topped up instead of
    /// reused as is.
    pub min_decks: usize,
}

impl ReferenceDepth {
    pub const PEER: Self = Self { fetch_budget: NEW_DECK_FETCH_BUDGET_PER_SOURCE, pages: 1, min_decks: 0 };
    pub const DEEP: Self = Self { fetch_budget: 24, pages: 3, min_decks: 30 };
}

thread_local! {
    static DEPTH: std::cell::Cell<ReferenceDepth> = const { std::cell::Cell::new(ReferenceDepth::PEER) };
}

/// Run `f` gathering references at `depth` (peer-based generation uses
/// [`ReferenceDepth::PEER`]; deep learning generation reads more).
pub fn with_depth<T>(depth: ReferenceDepth, f: impl FnOnce() -> T) -> T {
    let previous = DEPTH.with(|d| d.replace(depth));
    let out = f();
    DEPTH.with(|d| d.set(previous));
    out
}

/// New decks one source may download this run (see ReferenceDepth).
fn fetch_budget() -> usize {
    DEPTH.with(|d| d.get().fetch_budget)
}
const MAX_REFERENCE_DECKS: usize = 120;
const MAX_COMMANDER_ARCHIVE_DECKS: usize = 500;
const NORMAL_COMBO_ANALYSIS_LIMIT: usize = 24;
const CEDH_COMBO_ANALYSIS_LIMIT: usize = 36;
const CACHE_SCHEMA: u8 = 3;

/// A complete combo Commander Spellbook found in one reference deck.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceCombo {
    pub cards: Vec<String>,
    pub result: String,
    pub bracket: Option<u8>,
}

/// One public decklist for the commander: where it's from, its card names
/// (commanders included), and its combos once Spellbook has scanned it
/// (`combo_scan_complete` stops re-scanning decks that have none).
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

/// The reference corpus for one commander and bracket, with everything
/// counted once so lookups are cheap: how many decks run each card (and
/// how many competitive lists), card pairs that recur together
/// ("packages"), and recurring combos with their results and brackets.
/// Saved under `data/reference_cache/` and reused for CACHE_HOURS.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReferenceSnapshot {
    pub query: String,
    pub decks: Vec<ReferenceDeck>,
    pub card_frequency: HashMap<String, u32>,
    /// Card counts across cEDH Decklist Database lists only.
    #[serde(default)]
    pub cedh_frequency: HashMap<String, u32>,
    #[serde(default)]
    pub cedh_deck_count: usize,
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

/// Which search page to read next per site, so repeated runs discover new
/// decks instead of re-reading page 1 (`data/source_cache/crawl_state/`).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReferenceCrawlState {
    archidekt_page: usize,
    moxfield_page: usize,
    cedh_refreshes: u64,
}

impl Default for ReferenceCrawlState {
    /// Start every crawl at page 1.
    fn default() -> Self {
        Self {
            archidekt_page: 1,
            moxfield_page: 1,
            cedh_refreshes: 0,
        }
    }
}


impl ReferenceSnapshot {
    /// How many reference decks run this card.
    pub fn frequency(&self, card_name: &str) -> u32 {
        self.card_frequency
            .get(&card_name.to_ascii_lowercase())
            .copied()
            .unwrap_or(0)
    }

    /// How many reference decks the snapshot holds.
    pub fn deck_count(&self) -> usize {
        self.decks.len()
    }

    /// How many cEDH Decklist Database lists play this card.
    pub fn cedh_frequency(&self, card_name: &str) -> u32 {
        self.cedh_frequency
            .get(&card_name.to_ascii_lowercase())
            .copied()
            .unwrap_or(0)
    }

    /// The most-run cards, with their proper names restored.
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


    /// The strongest "package" a card is part of: the highest count of any
    /// recurring pair that includes it.
    /// TODO: scans every recurring pair on each call, and the generator calls
    /// it for every candidate card (via support_for_card), which can be
    /// thousands × tens of thousands of pairs. An index from card → best pair
    /// count, built once in `build_snapshot`, would make this instant.
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

    /// Recurring card pairs seen together in at least `minimum_count` decks.
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

    /// How many reference decks contain this exact combo (order doesn't matter).
    pub fn combo_frequency_for_cards(&self, cards: &[String]) -> u32 {
        let key = canonical_combo_key(cards);
        self.combo_frequency.get(&key).copied().unwrap_or(0)
    }

    /// Recurring combos, most common first: (cards, result, Spellbook
    /// bracket, how many decks).
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

/// Every cached reference deck (any commander) with 70+ cards and known
/// combos, cEDH-sourced and combo-rich first. The agent's matchup
/// simulation (meta_sim.rs) uses these as opponents.
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

/// Competitive lists in the local reference cache (any commander): how
/// many there are, and how many of them run each card (lower-case name).
/// Offline only; it reads what earlier bracket-5 builds downloaded. Auto-tag
/// uses it for the "Competitive" tag.
pub fn competitive_card_counts() -> (usize, HashMap<String, u32>) {
    let mut counts = HashMap::<String, u32>::new();
    let mut lists = 0;
    let mut seen = HashSet::<String>::new();
    let Ok(entries) = fs::read_dir("./data/source_cache/reference_decks") else {
        return (0, counts);
    };
    for deck in entries
        .flatten()
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .filter_map(|text| serde_json::from_str::<ReferenceDeck>(&text).ok())
        .filter(|deck| is_cedh_source(deck) && deck.cards.len() >= 50)
    {
        let key = canonical_reference_url(&deck.url);
        if !key.is_empty() && !seen.insert(key) {
            continue;
        }
        lists += 1;
        let names: HashSet<String> = deck.cards.iter().map(|c| c.to_ascii_lowercase()).collect();
        for name in names {
            *counts.entry(name).or_insert(0) += 1;
        }
    }
    (lists, counts)
}

/// Forget the saved snapshot so the next run rebuilds it (per-site caches
/// and the archive are kept).
/// TODO: builds the cache key without the "PDH: " prefix that
/// `gather_reference_decks` uses, so it can't invalidate Pauper snapshots.
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

/// Build (or reuse) the reference corpus for a commander and bracket:
/// 1. a fresh enough cached snapshot with enough decks is returned as is;
/// 2. otherwise start from the commander's archive (decks found on earlier
///    runs) and any stale snapshot;
/// 3. discover new decks: Archidekt (several pages when deep), Moxfield
///    (not for PDH), and competitive lists at bracket 5 (cEDH Decklist
///    Database, or competitive Pauper lists);
/// 4. scan up to 24 (36 at bracket 5) not-yet-scanned decks with Spellbook;
/// 5. count everything into a snapshot and cache it.
pub fn gather_reference_decks(
    commander_names: &[String],
    bracket: u8,
) -> ReferenceSnapshot {
    // Pauper Commander decks are a separate corpus (Archidekt's "Pauper EDH"
    // format) with its own caches.
    let pdh = crate::format_sources::active_format() == crate::format::Format::Pdh;
    let query = if pdh {
        format!("PDH: {}", commander_names.join(" + "))
    } else {
        commander_names.join(" + ")
    };

    // Step 1: a fresh cached snapshot with enough decks for this depth is used
    // as is (after syncing its decks with the per-deck cache).
    let depth = DEPTH.with(|d| d.get());
    if let Some(mut cached) = load_cache(&query, bracket).filter(|c| c.deck_count() >= depth.min_decks) {
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

    // Step 2: start from every deck found on earlier runs.
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

    // Step 3: discover new decks, continuing from where the last run stopped.
    let mut crawl = load_crawl_state(&query);

    for _ in 0..depth.pages.max(1) {
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

        let empty = archidekt.is_empty();
        decks.extend(archidekt);
        crawl.archidekt_page = if crawl.archidekt_page >= 20 {
            1
        } else {
            crawl.archidekt_page + 1
        };
        if empty {
            break;
        }
    }

    // Moxfield's search can't filter Pauper Commander decks by commander.
    let moxfield = if pdh {
        Vec::new()
    } else {
        fetch_moxfield_references(
            commander_names,
            MOXFIELD_REFERENCE_LIMIT,
            crawl.moxfield_page,
        )
    };

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

    // Pauper Commander's competitive lists: the cEDH Decklist Database has
    // none, so read Archidekt's Pauper EDH lists titled as competitive.
    if bracket >= 5 && pdh {
        let competitive = fetch_archidekt_references_where(commander_names, CEDH_REFERENCE_LIMIT, 1, true);
        logging::record(
            "REF",
            format!("COMPETITIVE PAUPER DISCOVERY • usable={}", competitive.len()),
        );
        decks.extend(competitive);
    }

    if bracket >= 5 && !pdh {
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
        decks.sort_by_key(|deck| if is_cedh_source(deck) { 0 } else { 1 });
    }

    // Step 4: Spellbook combo scan, a limited number of new decks per run
    // (each is a network call); scanned decks are remembered.
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

    // Step 5: count it all and cache the snapshot.
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


/// Archidekt lists for the commander (see `fetch_archidekt_references_where`).
fn fetch_archidekt_references(
    commander_names: &[String],
    limit: usize,
    page: usize,
) -> Vec<ReferenceDeck> {
    fetch_archidekt_references_where(commander_names, limit, page, false)
}

/// Archidekt lists for the commander. With `competitive`, reads a larger
/// listing and keeps only decks titled as competitive, labelled
/// [`COMPETITIVE_PAUPER_SOURCE`] (Pauper Commander's stand-in for the cEDH
/// Decklist Database, which has no Pauper lists).
fn fetch_archidekt_references_where(
    commander_names: &[String],
    limit: usize,
    page: usize,
    competitive: bool,
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
    // Archidekt formats: 3 = Commander, 17 = Pauper EDH.
    let deck_format = if crate::format_sources::active_format() == crate::format::Format::Pdh { 17 } else { 3 };

    let search_urls = [
        format!(
            "https://archidekt.com/api/decks/v3/?deckFormat={deck_format}&commanderName={encoded}&orderBy=-viewCount&page={page}&pageSize={}",
            if competitive { 100 } else { limit.max(10) },
        ),
        format!(
            "https://archidekt.com/api/decks/v3/?deckFormat={deck_format}&name={encoded}&orderBy=-viewCount&page={page}&pageSize={}",
            if competitive { 100 } else { limit.max(10) },
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

    if competitive {
        ids.retain(|(_, title)| is_competitive_title(title));
    }
    ids.truncate(limit);
    let label = if competitive { COMPETITIVE_PAUPER_SOURCE } else { "Archidekt" };

    let mut out = Vec::new();
    let mut network_fetches = 0usize;

    for (id, title) in ids {
        let public_url = format!("https://archidekt.com/decks/{id}");

        if let Some(mut cached) = load_cached_reference_deck(&public_url) {
            if reference_deck_matches_commanders(
                &cached,
                commander_names,
            ) {
                if competitive {
                    cached.source = label.to_string();
                }
                out.push(cached);
            }
            continue;
        }

        if network_fetches >= fetch_budget() {
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
            label,
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
            "ARCHIDEKT{} FETCH BUDGET • page={} • network_deck_fetches={} • returned={}",
            if competitive { " COMPETITIVE PAUPER" } else { "" },
            page,
            network_fetches,
            out.len(),
        ),
    );

    out
}

/// Moxfield search for the commander's decks, most viewed first, then each
/// deck's full list (budgeted, cached per deck).
/// Currently returns nothing: Moxfield's search ignores the `commander`
/// and `q` filters and returns its most viewed decks overall, which
/// `deck_matches_commanders` then rejects. Kept for if the API changes.
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

        if network_fetches >= fetch_budget() {
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

/// A competitive list: from the cEDH Decklist Database, a competitive
/// Pauper Commander list, or titled as one. Bracket 5 builds weigh these
/// first.
pub fn is_cedh_source(deck: &ReferenceDeck) -> bool {
    let source = deck.source.to_ascii_lowercase();
    source.contains("cedh") || source.contains("competitive") || deck.title.to_ascii_lowercase().contains("cedh")
}

/// Deck titles players use for competitive lists ("cEDH", "cPDH",
/// "Competitive", "Tournament", "High Power"…). Used to pick competitive
/// Pauper Commander lists, which have no database of their own.
pub fn is_competitive_title(title: &str) -> bool {
    let t = title.to_ascii_lowercase();
    ["cedh", "cpdh", "competitive", "tournament", "high power", "high-power", "tryhard", "try-hard", "fast combo", "turbo"]
        .iter()
        .any(|marker| t.contains(marker))
}

/// The label competitive Pauper Commander lists from Archidekt carry.
pub const COMPETITIVE_PAUPER_SOURCE: &str = "Archidekt (competitive Pauper)";

/// Competitive lists from the cEDH Decklist Database: reads its home page,
/// finds Moxfield links near each mention of the commander's name, and
/// imports those decks (budgeted, cached).
/// TODO: it slices the page around each name by byte offsets; a name next
/// to multi-byte characters (accents, emoji) could split a character and
/// panic. Snap `left`/`right` to char boundaries.
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

        if network_fetches >= fetch_budget() {
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

/// Ask Spellbook which complete combos each not-yet-scanned reference deck
/// contains (up to `limit` decks per run, a short pause between calls).
/// Results are saved with the cached deck so each deck is scanned once.
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
                name: name.clone(), ..Default::default() })
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
                name: name.clone(), ..Default::default() })
            .collect::<Vec<_>>();

        let parsed = ParsedDeck {
            name: deck.title.clone(),
            commanders: commander_entries,
            main,
            sideboard: Vec::new(),
            format: Default::default(),
            considering: Vec::new(),
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

/// Pull (deck id, deck name) pairs out of an Archidekt search response,
/// wherever they sit in the JSON (the layout has changed before).
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

/// Reduce an imported deck to a ReferenceDeck: sorted, de-duplicated names.
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

/// Is this imported deck really for our commander(s)? Each must be in its
/// command zone or named in its title. This is the check that rejects most
/// wrong search results (and, sometimes, decks with unusual layouts).
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

/// Count a set of reference decks into a snapshot: per-card counts
/// (and competitive-only counts), card pairs that recur in 2+ decks, and
/// recurring combos.
/// Note: the pair counting is quadratic per deck (~5,000 pairs for a
/// 100-card list), so large corpora make big snapshot files.
/// TODO: for PDH the query starts with "PDH: ", so the commander isn't
/// recognised in `commander_set` and gets counted in pairs.
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
    let mut cedh_frequency = HashMap::<String, u32>::new();
    let mut cedh_deck_count = 0usize;
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
        let is_cedh = is_cedh_source(deck);
        if is_cedh {
            cedh_deck_count += 1;
        }

        for card in &deck.cards {
            let key = card.to_ascii_lowercase();
            card_names.entry(key.clone()).or_insert_with(|| card.clone());

            if seen.insert(key.clone()) {
                *card_frequency.entry(key.clone()).or_default() += 1;
                if is_cedh {
                    *cedh_frequency.entry(key.clone()).or_default() += 1;
                }

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
        cedh_frequency,
        cedh_deck_count,
        card_names,
        pair_frequency,
        pair_cards,
        combo_frequency,
        combo_cards,
        combo_results,
        combo_brackets,
    }
}

/// One key per combo regardless of card order or capitalization.
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

/// Remove duplicate decks (same URL, or same site and title).
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

/// Cached-deck version of `deck_matches_commanders`: each commander must
/// be among its cards or in its title.
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

/// Hash used for cache file names.
/// TODO: DefaultHasher's output isn't stable across Rust versions, so a
/// compiler update renames every cache file and orphans the old ones
/// (e.g. switching from nightly to stable). A fixed hash (FNV, or a
/// sanitized name) would keep caches across builds.
fn source_key(value: &str) -> u64 {
    let mut hasher =
        std::collections::hash_map::DefaultHasher::new();
    value.to_ascii_lowercase().hash(&mut hasher);
    hasher.finish()
}

/// Same deck, same key: no "www.", no trailing slash, lowercase.
fn canonical_reference_url(url: &str) -> String {
    url.trim()
        .trim_end_matches('/')
        .replace("https://www.", "https://")
        .replace("http://www.", "http://")
        .to_ascii_lowercase()
}

/// Where one reference deck is cached.
fn source_deck_cache_path(url: &str) -> PathBuf {
    let canonical = canonical_reference_url(url);

    PathBuf::from("./data/source_cache/reference_decks")
        .join(format!(
            "{:016x}.json",
            source_key(&canonical),
        ))
}

/// A cached reference deck by URL. Stale copies are still used (a decklist
/// doesn't stop being evidence), just logged as stale.
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

/// Cache one reference deck (skipped for decks without a URL).
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

/// Where a search page response is cached.
fn search_cache_path(url: &str, extension: &str) -> PathBuf {
    PathBuf::from("./data/source_cache/search_pages")
        .join(format!(
            "{:016x}.{}",
            source_key(url),
            extension,
        ))
}

/// Search pages are reused for SEARCH_CACHE_HOURS.
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

/// GET a JSON search page: fresh cache first, then the network (saving the
/// result), then any stale cache if the site is down or refuses.
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

/// Same as `fetch_json_with_search_cache` for HTML pages.
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

/// Refresh decks from their per-deck cache (which has scan results), and
/// seed the cache for decks that aren't in it yet.
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

/// Add or update decks in the commander's archive (every deck ever found
/// for it, up to MAX_COMMANDER_ARCHIVE_DECKS).
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

/// Where a commander's archive is stored.
fn commander_archive_path(query: &str) -> PathBuf {
    PathBuf::from("./data/source_cache/commander_archives")
        .join(format!(
            "{:016x}.json",
            source_key(query),
        ))
}

/// Every deck found for this commander on earlier runs.
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

/// Save the archive, de-duplicated and capped (oldest dropped first).
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

/// Where a commander's crawl state is stored.
fn crawl_state_path(query: &str) -> PathBuf {
    PathBuf::from("./data/source_cache/crawl_state")
        .join(format!(
            "{:016x}.json",
            source_key(query),
        ))
}

/// Which pages to read next for this commander (page 1 if unknown).
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

/// Remember which pages to read next time.
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

/// Snapshot cache file: one per commander and bracket, versioned by
/// CACHE_SCHEMA so format changes don't load old files.
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

/// A snapshot newer than CACHE_HOURS, if any.
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

/// Any saved snapshot, however old (used as extra evidence when
/// rebuilding).
fn load_cache_any_age(
    query: &str,
    bracket: u8,
) -> Option<ReferenceSnapshot> {
    let path = cache_path(query, bracket);
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Save a snapshot for reuse.
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

#[cfg(test)]
mod cedh_tests {
    use super::*;

    /// Test helper: a reference deck from a source and card names.
    fn deck(source: &str, cards: &[&str]) -> ReferenceDeck {
        ReferenceDeck {
            source: source.to_string(),
            title: "t".to_string(),
            url: String::new(),
            cards: cards.iter().map(|c| c.to_string()).collect(),
            combos: Vec::new(),
            combo_scan_complete: true,
        }
    }

    #[test]
    fn cedh_lists_are_tallied_separately() {
        let snap = build_snapshot(
            "Kinnan, Bonder Prodigy".to_string(),
            vec![
                deck("cEDH DDB / Moxfield", &["Kinnan, Bonder Prodigy", "Basalt Monolith", "Mana Crypt"]),
                deck("cEDH DDB / Moxfield", &["Kinnan, Bonder Prodigy", "Basalt Monolith"]),
                deck("Moxfield", &["Kinnan, Bonder Prodigy", "Cultivate"]),
            ],
        );
        assert_eq!(snap.cedh_deck_count, 2);
        assert_eq!(snap.cedh_frequency("Basalt Monolith"), 2);
        assert_eq!(snap.cedh_frequency("Cultivate"), 0);
        assert_eq!(snap.frequency("Cultivate"), 1);
    }
}
