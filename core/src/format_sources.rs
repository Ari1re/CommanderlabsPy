//! Deck-building evidence for formats other than Commander.
//!
//! - Pauper Commander: PDHREC (pdhrec.com), the PDH counterpart of EDHREC:
//!   per-commander card inclusion and synergy across its tracked decks. Public
//!   Archidekt decks in the "Pauper EDH" format (format 17) feed the reference
//!   corpus (see `references.rs`).
//! - Canadian Highlander: tournament decklists from MTGTop8's Canadian
//!   Highlander section (f=CHL) and the most viewed Moxfield decks in the
//!   "highlanderCanadian" format.
//!
//! Everything is cached under data/format-sources so repeat builds don't hit
//! the sites again.
//!
//! Things to work on:
//! - PDHREC pages are scraped HTML; keep the parser tolerant and log when a
//!   page yields no cards.
//! - Canadian Highlander lists come from MTGTop8 and Moxfield's search; the
//!   same Moxfield filter problem as `references.rs` may apply.

use crate::{
    data::CardDb,
    deck::{parse_deck_text, ParsedDeck},
    edhrec::EdhrecCardSignal,
    format::Format,
    logging,
};
use anyhow::{anyhow, Result};
use regex::Regex;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    cell::Cell,
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::OnceLock,
    thread,
    time::{Duration, SystemTime},
};

const CACHE_DIR: &str = "./data/format-sources";
const PDHREC_TTL: Duration = Duration::from_secs(3 * 24 * 3600);
const LIST_TTL: Duration = Duration::from_secs(2 * 24 * 3600);
const UA: &str = concat!("Mozilla/5.0 CommanderLab/", env!("CARGO_PKG_VERSION"), " (deck builder)");

// ---------------------------------------------------------------------------
// Which format the current generation run is gathering evidence for. Set by
// the generator around the commander research so the shared reference code
// (Archidekt search, caches) asks for the right format.
// ---------------------------------------------------------------------------

thread_local! {
    static ACTIVE: Cell<Format> = const { Cell::new(Format::Commander) };
}

/// The format the current build is for (Commander unless a caller set
/// another with `with_format`). Read deep inside research code so PDH
/// builds use PDHREC and Pauper EDH decklists without passing the format
/// through every function.
pub fn active_format() -> Format {
    ACTIVE.with(Cell::get)
}

/// Run `run` with `format` as the active format, then restore the old one.
/// Thread-local, so it only applies to the current job's thread.
pub fn with_format<T>(format: Format, run: impl FnOnce() -> T) -> T {
    let previous = ACTIVE.with(|a| a.replace(format));
    let out = run();
    ACTIVE.with(|a| a.set(previous));
    out
}

// ---------------------------------------------------------------------------
// Small HTTP + cache helpers
// ---------------------------------------------------------------------------

/// HTTP client for these sources (25 s timeout, follows redirects).
fn client() -> Option<Client> {
    Client::builder()
        .timeout(Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .ok()
}

/// Cache file named by kind and a sanitized key (stable across builds,
/// unlike references.rs's hashed names).
fn cache_file(kind: &str, key: &str) -> PathBuf {
    let key: String = key
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .take(120)
        .collect();
    PathBuf::from(CACHE_DIR).join(format!("{kind}-{key}.json"))
}

/// Is the file younger than `ttl`?
fn fresh(path: &PathBuf, ttl: Duration) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .map(|age| age < ttl)
        .unwrap_or(false)
}

/// Read a cache file; with a `ttl`, only if it's fresh enough.
fn read_cache<T: for<'de> Deserialize<'de>>(path: &PathBuf, ttl: Option<Duration>) -> Option<T> {
    if let Some(ttl) = ttl {
        if !fresh(path, ttl) {
            return None;
        }
    }
    fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok())
}

/// Save a cache file (errors ignored: caching is best-effort).
fn write_cache<T: Serialize>(path: &PathBuf, value: &T) {
    let _ = fs::create_dir_all(CACHE_DIR);
    if let Ok(text) = serde_json::to_string(value) {
        let _ = fs::write(path, text);
    }
}

/// GET a page's text, failing on HTTP errors.
fn get_text(client: &Client, url: &str) -> Result<String> {
    let response = client.get(url).header("User-Agent", UA).send()?;
    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!("{url} returned {status}"));
    }
    Ok(response.text()?)
}

// ---------------------------------------------------------------------------
// PDHREC
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PdhrecCard {
    pub name: String,
    pub synergy: f64,
    pub decks: u64,
    pub type_line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PdhrecPage {
    pub url: String,
    pub decks: u64,
    pub cards: Vec<PdhrecCard>,
}

impl PdhrecPage {
    /// The same shape EDHREC data has, so the generator can use it as-is.
    pub fn signals(&self) -> Vec<EdhrecCardSignal> {
        self.cards
            .iter()
            .map(|c| {
                let share = if self.decks == 0 { 0.0 } else { c.decks as f64 / self.decks as f64 };
                EdhrecCardSignal {
                    name: c.name.clone(),
                    synergy: c.synergy,
                    inclusion: c.decks,
                    potential_decks: self.decks,
                    category: if c.synergy >= 0.25 {
                        "highsynergycards".into()
                    } else if share >= 0.4 {
                        "topcards".into()
                    } else {
                        "pdhrec".into()
                    },
                }
            })
            .collect()
    }
}

/// PDHREC's page name for a commander (pair): lower case, words joined by
/// hyphens, partners' names one after the other.
pub fn pdhrec_slug(names: &[String]) -> String {
    let joined = names
        .iter()
        .map(|n| n.split(" // ").next().unwrap_or(n).to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let mut slug = String::new();
    let mut dash = false;
    for c in joined.to_lowercase().chars() {
        if c == '\'' || c == '’' || c == ',' || c == '.' {
            continue;
        }
        if c.is_alphanumeric() {
            slug.push(c);
            dash = false;
        } else if !dash && !slug.is_empty() {
            slug.push('-');
            dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

/// Parse a PDHREC commander page: each card is
/// `<a class="gallery-item" synergy=".." popularity="N" typeline=".."><img alt="Name"`.
pub fn parse_pdhrec(html: &str, url: &str) -> PdhrecPage {
    static CARD: OnceLock<Regex> = OnceLock::new();
    static DECKS: OnceLock<Regex> = OnceLock::new();
    let card_re = CARD.get_or_init(|| {
        Regex::new(r#"(?s)class="gallery-item"\s+synergy="([-0-9.eE]+)"\s+popularity="(\d+)"\s+typeline="([^"]*)"\s*>\s*<img[^>]*?alt="([^"]+)""#)
            .expect("pdhrec card pattern")
    });
    let decks_re = DECKS.get_or_init(|| Regex::new(r"(?s)<br>\s*In (\d+) decks\.").expect("pdhrec decks pattern"));
    let decks = decks_re
        .captures(html)
        .and_then(|c| c[1].parse().ok())
        .unwrap_or(0);
    let mut seen = std::collections::HashSet::new();
    let cards = card_re
        .captures_iter(html)
        .filter_map(|c| {
            let name = html_unescape(&c[4]);
            seen.insert(name.to_ascii_lowercase()).then(|| PdhrecCard {
                name,
                synergy: c[1].parse().unwrap_or(0.0),
                decks: c[2].parse().unwrap_or(0),
                type_line: html_unescape(&c[3]),
            })
        })
        .collect();
    PdhrecPage { url: url.to_string(), decks, cards }
}

/// Undo the few HTML entities PDHREC uses in card names.
fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

/// PDHREC data for a commander (pair), cached for three days.
pub fn pdhrec_commander(names: &[String]) -> Option<PdhrecPage> {
    let slug = pdhrec_slug(names);
    if slug.is_empty() {
        return None;
    }
    let path = cache_file("pdhrec", &slug);
    if let Some(page) = read_cache::<PdhrecPage>(&path, Some(PDHREC_TTL)) {
        return Some(page);
    }
    let url = format!("https://pdhrec.com/commander/{slug}/");
    let result = client().ok_or_else(|| anyhow!("no client")).and_then(|c| get_text(&c, &url));
    match result {
        Ok(html) => {
            let page = parse_pdhrec(&html, &url);
            logging::record("PDHREC", format!("{url} • decks={} • cards={}", page.decks, page.cards.len()));
            if page.cards.is_empty() {
                return read_cache(&path, None);
            }
            write_cache(&path, &page);
            Some(page)
        }
        Err(err) => {
            logging::record("PDHREC", format!("{url} failed: {err:#}"));
            // Stale data beats none.
            read_cache(&path, None)
        }
    }
}

// ---------------------------------------------------------------------------
// Canadian Highlander decklists
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlanderDeck {
    pub source: String,
    pub title: String,
    pub url: String,
    /// Archetype or deck name, used to match play styles.
    pub archetype: String,
    pub cards: Vec<String>,
    pub colors: String,
}

#[derive(Debug, Clone, Default)]
pub struct HighlanderEvidence {
    pub decks: Vec<HighlanderDeck>,
    /// Total weight behind the play rates: exact color matches count 1,
    /// looser ones less. Tells how far to trust the numbers.
    pub strength: f64,
    /// Share of matching decks that play each card (lowercase name), weighted
    /// toward decks that fit the requested play styles.
    pub share: HashMap<String, f64>,
}

impl HighlanderEvidence {
    /// Weighted share of matching Highlander decks that play this card.
    pub fn share_of(&self, name: &str) -> f64 {
        self.share.get(&name.to_ascii_lowercase()).copied().unwrap_or(0.0)
    }
}

/// Colors an archetype name implies ("Jeskai Midrange" → WUR), if any.
fn archetype_colors(name: &str) -> Option<String> {
    let n = name.to_ascii_lowercase();
    let table: &[(&str, &str)] = &[
        ("5c", "WUBRG"), ("five", "WUBRG"), ("4c", ""), ("jeskai", "WUR"), ("esper", "WUB"),
        ("grixis", "UBR"), ("jund", "BRG"), ("naya", "WRG"), ("bant", "WUG"), ("abzan", "WBG"),
        ("temur", "URG"), ("mardu", "WBR"), ("sultai", "UBG"), ("azorius", "WU"), ("dimir", "UB"),
        ("rakdos", "BR"), ("gruul", "RG"), ("selesnya", "WG"), ("orzhov", "WB"), ("izzet", "UR"),
        ("golgari", "BG"), ("boros", "WR"), ("simic", "UG"), ("red deck", "R"), ("mono-red", "R"),
        ("mono red", "R"), ("mono-white", "W"), ("mono-blue", "U"), ("mono-black", "B"),
        ("mono-green", "G"), ("rg ", "RG"), ("ub ", "UB"), ("ur ", "UR"), ("bg ", "BG"),
        ("wr ", "WR"), ("wu ", "WU"), ("goblins", "R"), ("elfball", "G"),
    ];
    let padded = format!("{n} ");
    table
        .iter()
        .find(|(k, _)| padded.contains(k))
        .and_then(|(_, c)| (!c.is_empty()).then(|| c.to_string()))
}

/// A deck's colors from its cards, in WUBRG order.
fn colors_of(cards: &[String], db: &CardDb) -> String {
    let mut seen = String::new();
    for name in cards {
        if let Ok(Some(card)) = db.get_card(name) {
            if card.type_line.contains("Land") {
                continue;
            }
            for c in card.color_identity.chars() {
                if !seen.contains(c) {
                    seen.push(c);
                }
            }
        }
    }
    ['W', 'U', 'B', 'R', 'G'].iter().filter(|c| seen.contains(**c)).collect()
}

/// Are all of `colors` inside `of`?
fn subset(colors: &str, of: &str) -> bool {
    colors.chars().all(|c| of.contains(c))
}

/// Every card name in a deck (main and command zone).
fn deck_cards(deck: &ParsedDeck) -> Vec<String> {
    deck.commanders
        .iter()
        .chain(deck.main.iter())
        .filter(|e| !crate::format::is_basic_land_name(&e.name))
        .map(|e| e.name.clone())
        .collect()
}

/// MTGTop8 Canadian Highlander archetypes: (id, meta, name).
fn mtgtop8_archetypes(client: &Client) -> Vec<(String, String, String)> {
    let path = cache_file("mtgtop8", "chl-archetypes");
    if let Some(list) = read_cache::<Vec<(String, String, String)>>(&path, Some(LIST_TTL)) {
        return list;
    }
    let Ok(html) = get_text(client, "https://www.mtgtop8.com/format?f=CHL") else {
        return read_cache(&path, None).unwrap_or_default();
    };
    let re = Regex::new(r"archetype\?a=(\d+)&meta=(\d+)&f=CHL[^>]*>([^<]{2,40})</a>").expect("mtgtop8 archetype pattern");
    let mut out: Vec<(String, String, String)> = Vec::new();
    for c in re.captures_iter(&html) {
        let name = html_unescape(c[3].trim());
        if name.ends_with('%') || out.iter().any(|(id, _, _)| *id == c[1]) {
            continue;
        }
        out.push((c[1].to_string(), c[2].to_string(), name));
    }
    if !out.is_empty() {
        write_cache(&path, &out);
    }
    out
}

/// Recent deck ids for an MTGTop8 archetype: (event, deck, name).
fn mtgtop8_archetype_decks(client: &Client, id: &str, meta: &str) -> Vec<(String, String, String)> {
    let path = cache_file("mtgtop8", &format!("chl-arch-{id}"));
    if let Some(list) = read_cache::<Vec<(String, String, String)>>(&path, Some(LIST_TTL)) {
        return list;
    }
    let url = format!("https://www.mtgtop8.com/archetype?a={id}&meta={meta}&f=CHL");
    let Ok(html) = get_text(client, &url) else {
        return read_cache(&path, None).unwrap_or_default();
    };
    let re = Regex::new(r"event\?e=(\d+)&d=(\d+)&f=CHL>([^<]{1,60})</a>").expect("mtgtop8 deck pattern");
    let mut out: Vec<(String, String, String)> = Vec::new();
    for c in re.captures_iter(&html) {
        if !out.iter().any(|(_, d, _)| *d == c[2]) {
            out.push((c[1].to_string(), c[2].to_string(), html_unescape(c[3].trim())));
        }
    }
    write_cache(&path, &out);
    out
}

/// One MTGTop8 deck as a list (cached forever: tournament lists don't change).
fn mtgtop8_deck(client: &Client, event: &str, deck: &str, archetype: &str, db: &CardDb) -> Option<HighlanderDeck> {
    let path = cache_file("mtgtop8-deck", deck);
    if let Some(cached) = read_cache::<HighlanderDeck>(&path, None) {
        return Some(cached);
    }
    let text = get_text(client, &format!("https://www.mtgtop8.com/mtgo?d={deck}")).ok()?;
    let parsed = parse_deck_text(archetype, &text).ok()?;
    let cards = deck_cards(&parsed);
    if cards.len() < 40 {
        return None;
    }
    let out = HighlanderDeck {
        source: "MTGTop8".into(),
        title: format!("{archetype} (MTGTop8)"),
        url: format!("https://www.mtgtop8.com/event?e={event}&d={deck}&f=CHL"),
        archetype: archetype.to_string(),
        colors: colors_of(&cards, db),
        cards,
    };
    write_cache(&path, &out);
    Some(out)
}

/// Most viewed Moxfield Canadian Highlander decks: (public id, name, colors).
fn moxfield_highlander_list(client: &Client) -> Vec<(String, String, String)> {
    let path = cache_file("moxfield", "chl-top");
    if let Some(list) = read_cache::<Vec<(String, String, String)>>(&path, Some(LIST_TTL)) {
        return list;
    }
    let mut out = Vec::new();
    for page in 1..=3 {
        let url = format!(
            "https://api2.moxfield.com/v2/decks/search?pageNumber={page}&pageSize=64&sortType=Views&sortDirection=Descending&fmt=highlanderCanadian"
        );
        let Ok(text) = get_text(client, &url) else { break };
        let Ok(value) = serde_json::from_str::<Value>(&text) else { break };
        for d in value.get("data").and_then(Value::as_array).cloned().unwrap_or_default() {
            let id = d.get("publicId").and_then(Value::as_str).unwrap_or_default().to_string();
            let name = d.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
            let colors: String = d
                .get("colorIdentity")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if !id.is_empty() {
                out.push((id, name, colors));
            }
        }
        thread::sleep(Duration::from_millis(400));
    }
    if !out.is_empty() {
        write_cache(&path, &out);
    } else {
        return read_cache(&path, None).unwrap_or_default();
    }
    out
}

/// Import one Moxfield Canadian Highlander deck as Highlander evidence.
fn moxfield_deck(id: &str, name: &str, db: &CardDb) -> Option<HighlanderDeck> {
    let path = cache_file("moxfield-deck", id);
    if let Some(cached) = read_cache::<HighlanderDeck>(&path, Some(Duration::from_secs(30 * 24 * 3600))) {
        return Some(cached);
    }
    let url = format!("https://www.moxfield.com/decks/{id}");
    let parsed = crate::moxfield::import_moxfield(&url).ok()?;
    let cards = deck_cards(&parsed);
    if cards.len() < 40 {
        return None;
    }
    let out = HighlanderDeck {
        source: "Moxfield".into(),
        title: format!("{name} (Moxfield)"),
        url,
        archetype: name.to_string(),
        colors: colors_of(&cards, db),
        cards,
    };
    write_cache(&path, &out);
    Some(out)
}

/// Play-style words a deck's archetype or title should contain to count as
/// the same kind of deck.
fn theme_words(themes: &[String]) -> Vec<String> {
    themes
        .iter()
        .flat_map(|t| {
            let t = t.to_ascii_lowercase();
            let extra: &[&str] = match t.as_str() {
                "aggro" => &["aggro", "red deck", "burn", "zoo", "goblins", "white weenie"],
                "burn" => &["burn", "red deck"],
                "control" => &["control"],
                "tempo" => &["tempo", "delver"],
                "midrange" => &["midrange", "rocks", "jund", "blood"],
                "ramp" => &["ramp", "elfball", "rocks", "big"],
                "combo" => &["combo", "storm", "breach", "oracle", "pattern"],
                "reanimator" => &["reanimator", "rector", "reanimate"],
                "storm" => &["storm"],
                "artifacts" => &["artifact", "welder", "academy", "affinity", "eggs"],
                _ => &[],
            };
            std::iter::once(t.clone()).chain(extra.iter().map(|s| s.to_string())).collect::<Vec<_>>()
        })
        .collect()
}

/// Canadian Highlander decks in (a subset of) `colors`, from MTGTop8
/// tournament results and popular Moxfield lists, and how often each card
/// appears in them. `budget` caps new deck downloads this run; everything is
/// cached, so the evidence grows with each build.
pub fn highlander_evidence(colors: &str, themes: &[String], db: &CardDb, budget: usize, mut progress: impl FnMut(String)) -> HighlanderEvidence {
    let Some(client) = client() else { return HighlanderEvidence::default() };
    let mut decks: Vec<HighlanderDeck> = Vec::new();
    let mut fetched = 0usize;

    // MTGTop8: archetypes whose colors fit (or can't be told from the name).
    let archetypes = mtgtop8_archetypes(&client);
    progress(format!("MTGTop8: {} Canadian Highlander archetypes", archetypes.len()));
    for (id, meta, name) in &archetypes {
        if let Some(c) = archetype_colors(name) {
            if !subset(&c, colors) {
                continue;
            }
        }
        let list = mtgtop8_archetype_decks(&client, id, meta);
        for (event, deck, _) in list.iter().take(6) {
            let cached = cache_file("mtgtop8-deck", deck).exists();
            if !cached {
                if fetched >= budget {
                    continue;
                }
                fetched += 1;
                thread::sleep(Duration::from_millis(250));
            }
            if let Some(d) = mtgtop8_deck(&client, event, deck, name, db) {
                decks.push(d);
            }
        }
    }

    // Moxfield: most viewed lists whose color identity fits.
    let list = moxfield_highlander_list(&client);
    progress(format!("Moxfield: {} popular Canadian Highlander decks to compare", list.len()));
    for (id, name, deck_colors) in list.iter() {
        if !deck_colors.is_empty() && !subset(deck_colors, colors) {
            continue;
        }
        let cached = cache_file("moxfield-deck", id).exists();
        if !cached {
            if fetched >= budget + budget / 2 {
                continue;
            }
            fetched += 1;
            thread::sleep(Duration::from_millis(400));
        }
        if let Some(d) = moxfield_deck(id, name, db) {
            decks.push(d);
        }
    }

    // Keep decks inside the colors; weight exact color matches and decks of
    // the requested play style higher.
    decks.retain(|d| subset(&d.colors, colors) && !d.colors.is_empty() || colors.is_empty() && d.colors.is_empty());
    let words = theme_words(themes);
    let mut weight_total = 0.0;
    let mut share: HashMap<String, f64> = HashMap::new();
    // Lists in fewer colors (mono-red Goblins for an Izzet request) are only
    // loosely related; they count for much less than exact color matches.
    for d in &decks {
        let missing = colors.len().saturating_sub(d.colors.len());
        let mut w = match missing {
            0 => 1.0,
            1 if d.colors.len() >= 2 => 0.35,
            _ => 0.1,
        };
        let label = format!("{} {}", d.archetype, d.title).to_ascii_lowercase();
        if words.iter().any(|t| label.contains(t.as_str())) {
            w *= 1.8;
        }
        weight_total += w;
        for card in &d.cards {
            *share.entry(card.to_ascii_lowercase()).or_default() += w;
        }
    }
    if weight_total > 0.0 {
        for v in share.values_mut() {
            *v /= weight_total;
        }
    }
    logging::record(
        "HIGHLANDER",
        format!("colors={colors} • decks={} • new downloads={fetched}", decks.len()),
    );
    HighlanderEvidence { decks, share, strength: weight_total }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_match_pdhrec() {
        assert_eq!(pdhrec_slug(&["Murmuring Mystic".into()]), "murmuring-mystic");
        assert_eq!(
            pdhrec_slug(&["Breeches, Brazen Plunderer".into(), "Malcolm, Keen-Eyed Navigator".into()]),
            "breeches-brazen-plunderer-malcolm-keen-eyed-navigator"
        );
        assert_eq!(pdhrec_slug(&["Gut, True-Soul Zealot".into()]), "gut-true-soul-zealot");
    }

    #[test]
    fn parses_pdhrec_cards() {
        let html = r#"<div class="big-gallery-item">
              Murmuring Mystic <br> In 193 decks.
            </div>
  <a
    class="gallery-item"
    synergy="0.6619235240731903"
    popularity="130"
    typeline="Sorcery"
    >
    <img
      loading="lazy"
      alt="Airborne Aid"
      src="x.jpg">
  </a>
  <a class="gallery-item" synergy="-0.05" popularity="172" typeline="Instant"><img alt="Opt" src="y"></a>"#;
        let page = parse_pdhrec(html, "u");
        assert_eq!(page.decks, 193);
        assert_eq!(page.cards.len(), 2);
        assert_eq!(page.cards[0].name, "Airborne Aid");
        assert_eq!(page.cards[0].decks, 130);
        assert!((page.cards[1].synergy + 0.05).abs() < 1e-9);
        let s = page.signals();
        assert_eq!(s[0].category, "highsynergycards");
        assert!((s[1].inclusion_pct() - 172.0 / 193.0).abs() < 1e-9);
    }

    #[test]
    fn archetype_names_give_colors() {
        assert_eq!(archetype_colors("Jeskai Midrange").as_deref(), Some("WUR"));
        assert_eq!(archetype_colors("Red Deck Wins").as_deref(), Some("R"));
        assert_eq!(archetype_colors("Other - Control"), None);
        assert_eq!(archetype_colors("4c Blood"), None);
        assert!(subset("UR", "WUR") && !subset("BG", "WUR"));
    }
}
