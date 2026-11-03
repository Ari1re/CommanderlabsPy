//! MPC Autofill integration.
//!
//! Turns a Commander Lab deck into a MakePlayingCards proxy order:
//!
//! 1. Every card (commanders, 99, optional sideboard) becomes an order slot.
//!    Double-faced cards get a matching back-face slot.
//! 2. The configured MPC Autofill backend is searched for each card name and
//!    the candidate images are ranked (preferred sources, then DPI).
//! 3. The user can cycle art per card in the MPC tab.
//! 4. The order is written as the XML the MPC Autofill desktop tool consumes,
//!    plus a plain-text list for pasting into the mpcfill.com web editor.
//!
//! Search runs on a worker thread; nothing here touches the UI directly.
//!
//! Things to work on:
//! - Searches hit mpcfill.com once per card; batching where the backend
//!   allows would make large decks much faster.
//! - Chosen images are kept in memory per session (`mpc_orders` in
//!   `api.rs`); save them with the deck so choices survive a restart.

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, fs, path::Path, time::Duration};

pub const CONFIG_PATH: &str = "./data/mpc_config.json";

/// The MPC Autofill search server behind mpcautofill.github.io. (The old
/// mpcautofill.com address now answers every search with 405.)
pub const DEFAULT_BACKEND: &str = "https://mpcfill.com";
const RETIRED_BACKENDS: [&str; 2] = ["mpcautofill.com", "www.mpcautofill.com"];

/// MakePlayingCards bracket sizes. An order is rounded up to the smallest
/// bracket that fits every slot.
pub const MPC_BRACKETS: [u32; 16] = [
    18, 36, 55, 72, 90, 108, 126, 144, 162, 180, 198, 216, 234, 396, 504, 612,
];

pub const STOCKS: [&str; 5] = [
    "(S30) Standard Smooth",
    "(S33) Superior Smooth",
    "(M31) Linen",
    "(P10) Plastic",
    "(S27) Smooth",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MpcConfig {
    /// MPC Autofill backend. The mpcfill.com editor can point at any
    /// compatible backend, so this is user-editable.
    pub backend_url: String,
    /// Drive file ID of the cardback image. Empty = use the backend's first
    /// cardback.
    pub cardback_id: String,
    /// Source names to rank first, in order (e.g. "Chilli_Axe").
    pub preferred_sources: Vec<String>,
    pub minimum_dpi: u32,
    pub stock: String,
    pub foil: bool,
    pub include_sideboard: bool,
    /// Max candidate images kept per card (0 = every image found).
    pub max_candidates: usize,
    /// Add Scryfall's official scans (every distinct artwork) as choices.
    pub include_official: bool,
    /// List official art before community scans.
    pub prefer_official: bool,
    /// Where MPC Autofill fills the project (see `mpc_autofill::SITES`).
    pub autofill_site: String,
    /// Browser MPC Autofill drives (see `mpc_autofill::BROWSERS`).
    pub autofill_browser: String,
}

impl Default for MpcConfig {
    /// Fresh install: mpcfill.com, no cardback, the first stock, every DPI and
    /// every image, official scans on, MakePlayingCards in Chrome.
    fn default() -> Self {
        Self {
            backend_url: DEFAULT_BACKEND.to_string(),
            cardback_id: String::new(),
            preferred_sources: Vec::new(),
            minimum_dpi: 0,
            stock: STOCKS[0].to_string(),
            foil: false,
            include_sideboard: false,
            max_candidates: 0,
            include_official: true,
            prefer_official: false,
            autofill_site: "MakePlayingCards".to_string(),
            autofill_browser: "chrome".to_string(),
        }
    }
}

impl MpcConfig {
    /// Read `data/mpc_config.json`, or write the defaults the first time. It
    /// also migrates old saves: a retired server address goes back to the
    /// default, and the untouched pre-0.27.2 limits (300 DPI, 12 images) go to
    /// "show everything". It saves only when it changed something.
    pub fn load_or_create() -> Self {
        if let Ok(text) = fs::read_to_string(CONFIG_PATH) {
            if let Ok(mut config) = serde_json::from_str::<Self>(&text) {
                // Move saved settings off the retired server address.
                let host = config
                    .backend_url
                    .trim()
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_end_matches('/')
                    .to_ascii_lowercase();
                let mut changed = false;
                if config.backend_url.trim().is_empty() || RETIRED_BACKENDS.contains(&host.as_str()) {
                    config.backend_url = DEFAULT_BACKEND.to_string();
                    changed = true;
                }
                // Untouched settings from before 0.27.2 (300 DPI, 12 images)
                // hid most of the art; move them to "show everything".
                if config.minimum_dpi == 300 && config.max_candidates == 12 {
                    config.minimum_dpi = 0;
                    config.max_candidates = 0;
                    changed = true;
                }
                if changed {
                    let _ = config.save();
                }
                return config;
            }
        }
        let config = Self::default();
        let _ = config.save();
        config
    }

    /// Write the settings back as pretty JSON, creating `data/` if needed.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = Path::new(CONFIG_PATH).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(CONFIG_PATH, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Step to the next card stock in `STOCKS`, wrapping around. An unknown stock
    /// counts as the first one.
    pub fn cycle_stock(&mut self) {
        let index = STOCKS
            .iter()
            .position(|stock| stock.eq_ignore_ascii_case(&self.stock))
            .unwrap_or(0);
        self.stock = STOCKS[(index + 1) % STOCKS.len()].to_string();
    }
}

/// One candidate image from the MPC Autofill index.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MpcImage {
    pub identifier: String,
    pub name: String,
    pub source_name: String,
    pub dpi: u32,
    pub extension: String,
    pub thumbnail_url: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Scryfall's official scan rather than a community image.
    #[serde(default)]
    pub official: bool,
    /// Set and collector number of an official scan ("LEA 161").
    #[serde(default)]
    pub printing: String,
}

pub const OFFICIAL_SOURCE: &str = "Scryfall (official)";

impl MpcImage {
    /// Name the image is saved and listed under: name plus extension, unless the
    /// name already has a dot in it.
    pub fn file_name(&self) -> String {
        if self.extension.is_empty() || self.name.contains('.') {
            self.name.clone()
        } else {
            format!("{}.{}", self.name, self.extension)
        }
    }
}

/// A face that needs printing: the search query plus the art chosen for it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MpcFace {
    pub query: String,
    pub candidates: Vec<MpcImage>,
    pub selected: usize,
    /// MPC Autofill results not loaded yet (loaded on request).
    #[serde(default)]
    pub pending_ids: Vec<String>,
}

impl MpcFace {
    /// An empty face that only knows what to search for.
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            candidates: Vec::new(),
            selected: 0,
            pending_ids: Vec::new(),
        }
    }

    /// The art currently picked, or None while nothing was found.
    pub fn chosen(&self) -> Option<&MpcImage> {
        self.candidates.get(self.selected)
    }

    /// Previous/next art arrows: move the selection and wrap at either end.
    pub fn cycle(&mut self, forward: bool) {
        let len = self.candidates.len();
        if len < 2 {
            return;
        }
        self.selected = if forward {
            (self.selected + 1) % len
        } else {
            (self.selected + len - 1) % len
        };
    }
}

/// One deck line in the order.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MpcCard {
    pub name: String,
    pub quantity: u32,
    pub front: MpcFace,
    pub back: Option<MpcFace>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MpcOrder {
    pub deck_name: String,
    pub cards: Vec<MpcCard>,
    pub cardback_id: String,
    pub stock: String,
    pub foil: bool,
}

/// Input for building an order: one entry per deck line.
#[derive(Debug, Clone)]
pub struct OrderLine {
    pub name: String,
    pub quantity: u32,
    /// For double-faced cards: (front face, back face).
    pub faces: Option<(String, String)>,
}

impl MpcOrder {
    /// Turn deck lines into an order with no art yet. Double-faced cards get a
    /// front and a back face; everything else gets a front only (the shared
    /// cardback covers the back). Lines with quantity 0 are dropped. The cardback,
    /// stock and foil come from the saved settings.
    pub fn from_lines(deck_name: &str, lines: &[OrderLine], config: &MpcConfig) -> Self {
        let cards = lines
            .iter()
            .filter(|line| line.quantity > 0)
            .map(|line| match &line.faces {
                Some((front, back)) => MpcCard {
                    name: line.name.clone(),
                    quantity: line.quantity,
                    front: MpcFace::new(front.clone()),
                    back: Some(MpcFace::new(back.clone())),
                },
                None => MpcCard {
                    name: line.name.clone(),
                    quantity: line.quantity,
                    front: MpcFace::new(line.name.clone()),
                    back: None,
                },
            })
            .collect();

        Self {
            deck_name: deck_name.to_string(),
            cards,
            cardback_id: config.cardback_id.clone(),
            stock: config.stock.clone(),
            foil: config.foil,
        }
    }

    /// Number of physical cards (sum of quantities).
    pub fn total_slots(&self) -> u32 {
        self.cards.iter().map(|card| card.quantity).sum()
    }

    /// The MPC order size this many cards needs (see [`bracket_for`]).
    pub fn bracket(&self) -> u32 {
        bracket_for(self.total_slots())
    }

    /// (faces with art, faces in total), for the "34/36 faces ready" counter.
    /// Counted per card line, not per copy.
    pub fn resolved_faces(&self) -> (usize, usize) {
        let mut resolved = 0;
        let mut total = 0;
        for card in &self.cards {
            total += 1;
            if card.front.chosen().is_some() {
                resolved += 1;
            }
            if let Some(back) = &card.back {
                total += 1;
                if back.chosen().is_some() {
                    resolved += 1;
                }
            }
        }
        (resolved, total)
    }

    /// Distinct face queries that still need a search.
    pub fn queries(&self) -> Vec<String> {
        let mut out = Vec::<String>::new();
        let mut push = |query: &str| {
            let query = search_query(query);
            if !out.contains(&query) {
                out.push(query);
            }
        };
        for card in &self.cards {
            push(&card.front.query);
            if let Some(back) = &card.back {
                push(&back.query);
            }
        }
        out
    }

    /// Apply search results keyed by normalized query: community images,
    /// the MPC results still to load, and official scans.
    pub fn apply_results(
        &mut self,
        results: &HashMap<String, Vec<MpcImage>>,
        pending: &HashMap<String, Vec<String>>,
        official: &HashMap<String, Vec<MpcImage>>,
        prefer_official: bool,
    ) {
        let fill = |face: &mut MpcFace| {
            let key = search_query(&face.query);
            let community = results.get(&key).cloned().unwrap_or_default();
            let scans = official.get(&key).cloned().unwrap_or_default();
            face.candidates = if prefer_official {
                scans.into_iter().chain(community).collect()
            } else {
                community.into_iter().chain(scans).collect()
            };
            face.pending_ids = pending.get(&key).cloned().unwrap_or_default();
            face.selected = 0;
        };
        for card in &mut self.cards {
            fill(&mut card.front);
            if let Some(back) = card.back.as_mut() {
                fill(back);
            }
        }
    }

    /// XML order consumed by the MPC Autofill desktop tool (and importable
    /// into the mpcfill.com editor).
    pub fn to_xml(&self) -> Result<String> {
        if self.cards.is_empty() {
            return Err(anyhow!("The order is empty."));
        }

        let total = self.total_slots();
        let mut fronts = String::new();
        let mut backs = String::new();
        let mut slot = 0u32;
        let mut missing = Vec::new();

        for card in &self.cards {
            let slots: Vec<String> =
                (slot..slot + card.quantity).map(|s| s.to_string()).collect();
            slot += card.quantity;
            let slots = slots.join(",");

            match card.front.chosen() {
                Some(image) => fronts.push_str(&card_xml(image, &slots, &card.front.query)),
                None => missing.push(card.name.clone()),
            }

            if let Some(back) = &card.back {
                match back.chosen() {
                    Some(image) => backs.push_str(&card_xml(image, &slots, &back.query)),
                    None => missing.push(format!("{} (back)", card.name)),
                }
            }
        }

        if !missing.is_empty() {
            return Err(anyhow!(
                "{} face(s) have no MPC art yet: {}. Run SEARCH first or remove them.",
                missing.len(),
                missing.iter().take(8).cloned().collect::<Vec<_>>().join(", ")
            ));
        }

        let mut xml = String::new();
        xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<order>\n");
        xml.push_str("  <details>\n");
        xml.push_str(&format!("    <quantity>{total}</quantity>\n"));
        xml.push_str(&format!("    <bracket>{}</bracket>\n", bracket_for(total)));
        xml.push_str(&format!("    <stock>{}</stock>\n", escape(&self.stock)));
        xml.push_str(&format!("    <foil>{}</foil>\n", self.foil));
        xml.push_str("  </details>\n");
        xml.push_str("  <fronts>\n");
        xml.push_str(&fronts);
        xml.push_str("  </fronts>\n");
        if !backs.is_empty() {
            xml.push_str("  <backs>\n");
            xml.push_str(&backs);
            xml.push_str("  </backs>\n");
        }
        if !self.cardback_id.is_empty() {
            xml.push_str(&format!(
                "  <cardback>{}</cardback>\n",
                escape(&self.cardback_id)
            ));
        }
        xml.push_str("</order>\n");
        Ok(xml)
    }

    /// Plain list for the mpcfill.com "Add cards → Text" box. Works even when
    /// the backend could not be reached from this machine.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for card in &self.cards {
            match &card.back {
                Some(back) => out.push_str(&format!(
                    "{} {} | {}\n",
                    card.quantity, card.front.query, back.query
                )),
                None => out.push_str(&format!(
                    "{} {}\n",
                    card.quantity,
                    card.front.query.split(" // ").next().unwrap_or(&card.front.query)
                )),
            }
        }
        out
    }
}

/// One `<card>` element of the order XML: image id, slot list ("0,1,2"),
/// file name and the normalised search query.
fn card_xml(image: &MpcImage, slots: &str, query: &str) -> String {
    format!(
        "    <card>\n      <id>{}</id>\n      <slots>{}</slots>\n      <name>{}</name>\n      <query>{}</query>\n    </card>\n",
        escape(&image.identifier),
        slots,
        escape(&image.file_name()),
        escape(&search_query(query)),
    )
}

/// Smallest MPC bracket (18, 36, 55, …) that fits `quantity` cards. Past
/// the largest bracket it still returns the largest, so an oversized order
/// won't fit. TODO: warn or split the order when it is over the top bracket.
pub fn bracket_for(quantity: u32) -> u32 {
    MPC_BRACKETS
        .iter()
        .copied()
        .find(|bracket| *bracket >= quantity)
        .unwrap_or(*MPC_BRACKETS.last().unwrap())
}

/// MPC Autofill matches lower-cased names without punctuation noise.
pub fn search_query(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .replace(['’', '‘'], "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Escape the five XML special characters. `&` goes first so the others
/// aren't escaped twice.
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ---------------------------------------------------------------------------
// Backend client
// ---------------------------------------------------------------------------

pub struct MpcClient {
    client: Client,
    base: String,
}

#[derive(Debug, Clone, Default)]
pub struct SearchOutcome {
    pub results: HashMap<String, Vec<MpcImage>>,
    /// MPC identifiers found but not loaded yet, per query.
    pub pending: HashMap<String, Vec<String>>,
    pub default_cardback: Option<String>,
    pub not_found: Vec<String>,
}

/// Images per card loaded straight away; the rest load when asked for.
const EAGER_IMAGES: usize = 40;

/// Other spellings to try when a name finds nothing: split cards are
/// indexed under both halves ("fire ice"), and punctuation varies.
fn alternate_queries(query: &str) -> Vec<String> {
    let mut out = Vec::new();
    let joined = query.replace(" // ", " ").replace("//", " ");
    if joined != query {
        out.push(search_query(&joined));
        // Adventure and flip cards are usually indexed by their first half.
        if let Some(first) = query.split("//").next() {
            let first = search_query(first);
            if !first.is_empty() && !out.contains(&first) {
                out.push(first);
            }
        }
    }
    let plain: String = query
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { ' ' })
        .collect();
    let plain = search_query(&plain);
    if plain != query && !out.contains(&plain) {
        out.push(plain);
    }
    out
}

impl MpcClient {
    /// HTTP client for the configured MPC Autofill server (45 s timeout,
    /// trailing slash trimmed off the base URL).
    pub fn new(config: &MpcConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(45))
            .user_agent(concat!("CommanderLab/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("Could not create HTTP client")?;
        Ok(Self {
            client,
            base: config.backend_url.trim_end_matches('/').to_string(),
        })
    }

    /// POST JSON to the backend. A 404/405 almost always means the server
    /// address points at a normal website instead of the search API, so the error
    /// says to reset it. Other failures include the first 200 characters of the
    /// reply.
    fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let url = format!("{}{}", self.base, path);
        let response = self
            .client
            .post(&url)
            .json(body)
            .send()
            .with_context(|| format!("Could not reach MPC Autofill at {url}"))?;
        let status = response.status();
        let text = response.text().unwrap_or_default();
        if status.as_u16() == 405 || status.as_u16() == 404 {
            return Err(anyhow!(
                "{} isn't an MPC Autofill search server (it answered {status}). Set the server to {DEFAULT_BACKEND} in Order options.",
                self.base
            ));
        }
        if !status.is_success() {
            let snippet: String = text.chars().take(200).collect();
            return Err(anyhow!("MPC Autofill {path} returned {status}: {snippet}"));
        }
        serde_json::from_str(&text).with_context(|| format!("Bad JSON from {path}"))
    }

    /// GET JSON from the backend (used for the source list).
    fn get(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base, path);
        let response = self
            .client
            .get(&url)
            .send()
            .with_context(|| format!("Could not reach MPC Autofill at {url}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(anyhow!("MPC Autofill {path} returned {status}"));
        }
        response.json().with_context(|| format!("Bad JSON from {path}"))
    }

    /// Source primary keys, so a search can enable every source.
    fn sources(&self) -> Vec<Value> {
        self.get("/2/sources/")
            .ok()
            .and_then(|value| value.get("results").cloned())
            .map(|results| match results {
                Value::Object(map) => map
                    .values()
                    .filter_map(|source| source.get("pk").cloned())
                    .map(|pk| json!([pk, true]))
                    .collect(),
                Value::Array(list) => list
                    .iter()
                    .filter_map(|source| source.get("pk").cloned())
                    .map(|pk| json!([pk, true]))
                    .collect(),
                _ => Vec::new(),
            })
            .unwrap_or_default()
    }

    /// Ask `/2/editorSearch/` for the image ids matching each query, 20 queries
    /// per request. Queries with no hits map to an empty list.
    /// Note: `search` already hands this batches of 20, so the inner chunking
    /// only matters for other callers.
    fn editor_search(&self, queries: &[String], settings: &Value) -> Result<HashMap<String, Vec<String>>> {
        let mut out = HashMap::new();
        // The backend caps queries per request; batch conservatively.
        for batch in queries.chunks(20) {
            let body = json!({
                "searchSettings": settings,
                "queries": batch
                    .iter()
                    .map(|query| json!({ "query": query, "cardType": "CARD" }))
                    .collect::<Vec<_>>(),
            });
            let response = self.post("/2/editorSearch/", &body)?;
            let results = response.get("results").cloned().unwrap_or(Value::Null);
            for query in batch {
                let ids = results
                    .get(query)
                    .and_then(|by_type| by_type.get("CARD"))
                    .and_then(Value::as_array)
                    .map(|list| list.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>())
                    .unwrap_or_default();
                out.insert(query.clone(), ids);
            }
        }
        Ok(out)
    }

    /// Details (name, source, DPI, thumbnail) for MPC image identifiers.
    pub fn images(&self, ids: &[String]) -> Result<HashMap<String, MpcImage>> {
        let mut metadata = HashMap::<String, MpcImage>::new();
        for chunk in ids.chunks(100) {
            let body = json!({ "cardIdentifiers": chunk });
            let response = self.post("/2/cards/", &body)?;
            if let Some(Value::Object(map)) = response.get("results") {
                for (id, card) in map {
                    metadata.insert(id.clone(), parse_image(id, card));
                }
            }
        }
        Ok(metadata)
    }

    /// The `searchSettings` body: every source enabled, the minimum DPI from
    /// Order options, a 1500 DPI / 30 MB ceiling and NSFW art excluded.
    fn settings(&self, config: &MpcConfig, sources: &[Value], fuzzy: bool) -> Value {
        json!({
            "searchTypeSettings": { "fuzzySearch": fuzzy, "filterCardbacks": false },
            "sourceSettings": { "sources": sources },
            "filterSettings": {
                "minimumDPI": config.minimum_dpi,
                "maximumDPI": 1500,
                "maximumSize": 30,
                "languages": [],
                "includesTags": [],
                "excludesTags": ["NSFW"]
            }
        })
    }

    /// Find art for every face query. Steps: exact search; for names with no
    /// hits, alternate spellings (split cards by half, no punctuation) and then a
    /// fuzzy search; load details for the first few ids per query and keep the
    /// rest as pending. Official Scryfall scans are merged in later by the
/// caller (`MpcOrder::apply_results`), not here.
    /// `progress(done, total)` drives the order dialog's bar.
    pub fn search(
        &self,
        queries: &[String],
        config: &MpcConfig,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<SearchOutcome> {
        let sources = self.sources();
        let search_settings = self.settings(config, &sources, false);
        let total = queries.len();
        let mut identifiers_by_query = HashMap::<String, Vec<String>>::new();
        for (i, batch) in queries.chunks(20).enumerate() {
            identifiers_by_query.extend(self.editor_search(batch, &search_settings)?);
            progress(((i + 1) * 20).min(total), total);
        }

        // Names that found nothing: try other spellings, then fuzzy matching.
        let missing: Vec<String> = queries
            .iter()
            .filter(|q| identifiers_by_query.get(*q).map(|ids| ids.is_empty()).unwrap_or(true))
            .cloned()
            .collect();
        if !missing.is_empty() {
            let mut tries: Vec<(String, String)> = Vec::new();
            for q in &missing {
                for alt in alternate_queries(q) {
                    tries.push((q.clone(), alt));
                }
            }
            if !tries.is_empty() {
                let alts: Vec<String> = tries.iter().map(|(_, a)| a.clone()).collect();
                if let Ok(found) = self.editor_search(&alts, &search_settings) {
                    for (original, alt) in &tries {
                        if let Some(ids) = found.get(alt).filter(|ids| !ids.is_empty()) {
                            identifiers_by_query.entry(original.clone()).or_default().extend(ids.iter().cloned());
                        }
                    }
                }
            }
            let still: Vec<String> = missing
                .into_iter()
                .filter(|q| identifiers_by_query.get(q).map(|ids| ids.is_empty()).unwrap_or(true))
                .collect();
            if !still.is_empty() {
                let fuzzy = self.settings(config, &sources, true);
                if let Ok(found) = self.editor_search(&still, &fuzzy) {
                    for (q, ids) in found {
                        if !ids.is_empty() {
                            identifiers_by_query.insert(q, ids);
                        }
                    }
                }
            }
        }

        let cap = if config.max_candidates == 0 { usize::MAX } else { config.max_candidates };

        // Details for the first images of every card; the rest load on request.
        let mut wanted = Vec::<String>::new();
        let mut seen = std::collections::HashSet::<String>::new();
        for ids in identifiers_by_query.values() {
            for id in ids.iter().take(EAGER_IMAGES.min(cap)) {
                if seen.insert(id.clone()) {
                    wanted.push(id.clone());
                }
            }
        }
        let metadata = self.images(&wanted)?;

        let mut outcome = SearchOutcome::default();
        for query in queries {
            let ids = identifiers_by_query.get(query).cloned().unwrap_or_default();
            let ids: Vec<String> = ids.into_iter().take(cap).collect();
            let (loaded, rest): (Vec<String>, Vec<String>) =
                ids.iter().cloned().partition(|id| metadata.contains_key(id));
            let mut images: Vec<MpcImage> = loaded.iter().filter_map(|id| metadata.get(id).cloned()).collect();
            rank_images(&mut images, &config.preferred_sources);
            if images.is_empty() && rest.is_empty() {
                outcome.not_found.push(query.clone());
            }
            outcome.results.insert(query.clone(), images);
            outcome.pending.insert(query.clone(), rest);
        }

        // Cardbacks are a POST that takes the same search settings.
        outcome.default_cardback = self
            .post("/2/cardbacks/", &json!({ "searchSettings": search_settings }))
            .ok()
            .and_then(|value| value.get("cardbacks").cloned())
            .and_then(|list| list.as_array().and_then(|l| l.first().cloned()))
            .and_then(|first| first.as_str().map(str::to_string));

        Ok(outcome)
    }
}

/// Official scans for a face: one per distinct artwork across every paper
/// printing (newest printing of each artwork), from Scryfall.
pub fn official_images(card_name: &str, face_name: &str) -> Vec<MpcImage> {
    let Ok(printings) = crate::printings::printings(card_name) else {
        return Vec::new();
    };
    let face_key = face_name.to_ascii_lowercase();
    let mut seen = std::collections::HashSet::<String>::new();
    let mut out = Vec::new();
    for p in printings.iter().filter(|p| p.lang.is_empty() || p.lang == "en") {
        let (thumb, png, art) = match p.faces.iter().find(|f| f.name.to_ascii_lowercase() == face_key) {
            Some(face) => (face.image.clone(), face.png.clone(), face.illustration_id.clone()),
            None => (p.image.clone(), p.png.clone(), p.illustration_id.clone()),
        };
        if png.is_empty() {
            continue;
        }
        let key = if art.is_empty() { png.clone() } else { art };
        if !seen.insert(key) {
            continue;
        }
        let label = format!("{} {}", p.set_code, p.collector_number);
        out.push(MpcImage {
            identifier: png,
            name: format!("{} ({}).png", face_name, label).replace(['/', '\\', ':'], "-"),
            source_name: OFFICIAL_SOURCE.to_string(),
            dpi: 300,
            extension: "png".into(),
            thumbnail_url: thumb,
            tags: Vec::new(),
            official: true,
            printing: format!("{} · {}", p.set_name, label),
        });
    }
    out
}

/// Download the official scans an order uses into `dir` and point the order
/// at those files: the MPC Autofill desktop tool (v4+) takes a local file in
/// place of a Google Drive ID.
/// Returns (scans saved, names that couldn't be downloaded).
pub fn localize_official(order: &mut MpcOrder, dir: &Path, mut progress: impl FnMut(usize, usize)) -> Result<(usize, Vec<String>)> {
    let mut faces: Vec<&mut MpcFace> = Vec::new();
    for card in &mut order.cards {
        faces.push(&mut card.front);
        if let Some(back) = card.back.as_mut() {
            faces.push(back);
        }
    }
    let todo: Vec<usize> = faces
        .iter()
        .enumerate()
        .filter(|(_, f)| f.chosen().map(|c| c.official).unwrap_or(false))
        .map(|(i, _)| i)
        .collect();
    if todo.is_empty() {
        return Ok((0, Vec::new()));
    }
    fs::create_dir_all(dir)?;
    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("CommanderLab/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let mut saved = 0;
    let mut failed = Vec::new();
    for (n, i) in todo.iter().enumerate() {
        let face = &mut faces[*i];
        let selected = face.selected;
        let image = &mut face.candidates[selected];
        let path = dir.join(&image.name);
        if !path.exists() && image.identifier.starts_with("http") {
            let fetched = client
                .get(&image.identifier)
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.bytes());
            match fetched {
                Ok(bytes) => {
                    fs::write(&path, &bytes)?;
                    std::thread::sleep(Duration::from_millis(60));
                }
                Err(_) => {
                    failed.push(image.name.clone());
                    progress(n + 1, todo.len());
                    continue;
                }
            }
        }
        image.identifier = fs::canonicalize(&path).unwrap_or(path).display().to_string();
        saved += 1;
        progress(n + 1, todo.len());
    }
    Ok((saved, failed))
}

/// Read one `/2/cards/` entry into an `MpcImage`. It accepts the current and
/// older field names (sourceName / source_verbose / source, and so on) and
/// falls back to the request id when the entry has no identifier.
fn parse_image(id: &str, card: &Value) -> MpcImage {
    let text = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| card.get(*key).and_then(Value::as_str))
            .unwrap_or_default()
            .to_string()
    };
    MpcImage {
        identifier: {
            let identifier = text(&["identifier"]);
            if identifier.is_empty() { id.to_string() } else { identifier }
        },
        name: text(&["name"]),
        source_name: text(&["sourceName", "source_verbose", "source"]),
        dpi: card.get("dpi").and_then(Value::as_u64).unwrap_or(0) as u32,
        extension: text(&["extension"]),
        thumbnail_url: text(&["mediumThumbnailUrl", "smallThumbnailUrl", "medium_thumbnail_url"]),
        tags: card
            .get("tags")
            .and_then(Value::as_array)
            .map(|t| t.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default(),
        official: false,
        printing: String::new(),
    }
}

/// Preferred sources first (in configured order), then highest DPI. The
/// backend's own priority order is kept as the final tiebreak.
pub fn rank_images(images: &mut [MpcImage], preferred: &[String]) {
    let rank = |image: &MpcImage| {
        preferred
            .iter()
            .position(|source| source.eq_ignore_ascii_case(&image.source_name))
            .unwrap_or(usize::MAX)
    };
    images.sort_by(|a, b| rank(a).cmp(&rank(b)).then(b.dpi.cmp(&a.dpi)));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: a community image with just an id, source and DPI.
    fn image(id: &str, source: &str, dpi: u32) -> MpcImage {
        MpcImage {
            identifier: id.into(),
            name: format!("{id}"),
            source_name: source.into(),
            dpi,
            extension: "png".into(),
            thumbnail_url: String::new(),
            ..Default::default()
        }
    }

    #[test]
    fn default_server_is_mpcfill() {
        assert_eq!(MpcConfig::default().backend_url, "https://mpcfill.com");
    }

    #[test]
    fn bracket_rounds_up() {
        assert_eq!(bracket_for(1), 18);
        assert_eq!(bracket_for(100), 108);
        assert_eq!(bracket_for(108), 108);
        assert_eq!(bracket_for(109), 126);
        assert_eq!(bracket_for(9999), 612);
    }

    #[test]
    fn ranking_prefers_sources_then_dpi() {
        let mut images = vec![image("a", "X", 1200), image("b", "Pref", 300), image("c", "Y", 800)];
        rank_images(&mut images, &["Pref".to_string()]);
        let order: Vec<_> = images.iter().map(|i| i.identifier.as_str()).collect();
        assert_eq!(order, ["b", "a", "c"]);
    }

    #[test]
    fn xml_assigns_contiguous_slots_and_backs() {
        let config = MpcConfig { cardback_id: "BACK".into(), ..Default::default() };
        let lines = vec![
            OrderLine { name: "Sol Ring".into(), quantity: 1, faces: None },
            OrderLine { name: "Forest".into(), quantity: 3, faces: None },
            OrderLine {
                name: "Delver of Secrets // Insectile Aberration".into(),
                quantity: 1,
                faces: Some(("Delver of Secrets".into(), "Insectile Aberration".into())),
            },
        ];
        let mut order = MpcOrder::from_lines("Test", &lines, &config);
        let mut results = HashMap::new();
        results.insert("sol ring".into(), vec![image("SR", "S", 600)]);
        results.insert("forest".into(), vec![image("FO", "S", 600)]);
        results.insert("delver of secrets".into(), vec![image("DF", "S", 600)]);
        results.insert("insectile aberration".into(), vec![image("DB", "S", 600)]);
        order.apply_results(&results, &HashMap::new(), &HashMap::new(), false);

        let xml = order.to_xml().unwrap();
        assert!(xml.contains("<quantity>5</quantity>"));
        assert!(xml.contains("<bracket>18</bracket>"));
        assert!(xml.contains("<id>FO</id>\n      <slots>1,2,3</slots>"));
        assert!(xml.contains("<backs>\n    <card>\n      <id>DB</id>\n      <slots>4</slots>"));
        assert!(xml.contains("<cardback>BACK</cardback>"));
        assert_eq!(order.resolved_faces(), (4, 4));
    }

    #[test]
    fn xml_refuses_unresolved_faces() {
        let config = MpcConfig::default();
        let lines = vec![OrderLine { name: "Sol Ring".into(), quantity: 1, faces: None }];
        let order = MpcOrder::from_lines("Test", &lines, &config);
        assert!(order.to_xml().is_err());
        assert_eq!(order.to_text(), "1 Sol Ring\n");
    }

    #[test]
    fn escape_handles_xml_specials() {
        assert_eq!(escape("Tom & Jerry's <x>"), "Tom &amp; Jerry&apos;s &lt;x&gt;");
    }

    #[test]
    fn official_art_merges_and_alternates_cover_split_cards() {
        let config = MpcConfig::default();
        assert_eq!(config.max_candidates, 0);
        let lines = vec![OrderLine { name: "Sol Ring".into(), quantity: 1, faces: None }];
        let mut order = MpcOrder::from_lines("T", &lines, &config);
        let mut community = HashMap::new();
        community.insert("sol ring".to_string(), vec![image("SR", "S", 600)]);
        let mut pending = HashMap::new();
        pending.insert("sol ring".to_string(), vec!["MORE".to_string()]);
        let mut official = HashMap::new();
        official.insert("sol ring".to_string(), vec![MpcImage { identifier: "https://x/sr.png".into(), official: true, ..Default::default() }]);
        order.apply_results(&community, &pending, &official, false);
        let face = &order.cards[0].front;
        assert_eq!(face.candidates.len(), 2);
        assert!(!face.candidates[0].official && face.candidates[1].official);
        assert_eq!(face.pending_ids, vec!["MORE".to_string()]);
        order.apply_results(&community, &pending, &official, true);
        assert!(order.cards[0].front.candidates[0].official);

        assert_eq!(alternate_queries("fire // ice"), vec!["fire ice".to_string(), "fire".to_string()]);
        assert_eq!(alternate_queries("lim-dûl's vault"), vec!["lim dûl s vault".to_string()]);
    }
}
