use crate::deck::normalize_multiface_separator;
use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, USER_AGENT};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
};

/// Scryfall currently returns metadata here containing a `jsonl_download_uri`.
const ORACLE_BULK_METADATA_URL: &str =
    "https://api.scryfall.com/bulk-data/oracle-cards";

const USER_AGENT_VALUE: &str =
    concat!("CommanderLab/", env!("CARGO_PKG_VERSION"), " (local Commander deck analyzer)");

const DATA_DIR: &str = "./data";
const DEBUG_DIR: &str = "./data/debug";
const BULK_GZIP_PATH: &str = "./data/oracle-cards.jsonl.gz";

#[derive(Debug, Clone, serde::Serialize)]
pub struct Card {
    pub oracle_id: String,
    pub name: String,
    pub mana_cost: String,
    pub cmc: f64,
    pub type_line: String,
    pub oracle_text: String,
    pub color_identity: String,
    pub produced_mana: String,
    pub commander_legal: bool,
    pub edhrec_rank: Option<u32>,
    pub keywords: String,
    pub is_basic: bool,
    pub commander_eligible: bool,
    pub image_uri: String,
    pub price_usd: Option<f64>,
    pub price_usd_foil: Option<f64>,
    pub price_eur: Option<f64>,
    pub price_eur_foil: Option<f64>,
    pub price_tix: Option<f64>,
    pub tcgplayer_uri: String,
    pub cardmarket_uri: String,
    pub cardhoarder_uri: String,
    pub game_changer: bool,
    /// Rarity of Scryfall's default printing (common, uncommon, …).
    pub rarity: String,
    /// Scryfall's Pauper Commander legality for the 99: legal / banned / not_legal.
    pub pdh_legal: String,
    /// Scryfall's Vintage legality (legal / restricted / banned / not_legal),
    /// the card pool Canadian Highlander uses.
    pub vintage_legal: String,
    /// Printed at uncommon in paper or MTGO as a creature, Vehicle,
    /// Spacecraft or Background: may lead a Pauper Commander deck.
    pub pdh_commander: bool,
}

/// Columns every card query selects, in `row_to_card` order.
const CARD_COLUMNS: &str = "oracle_id, name, mana_cost, cmc, type_line, oracle_text, \
    color_identity, produced_mana, commander_legal, edhrec_rank, keywords, is_basic, \
    commander_eligible, image_uri, price_usd, price_usd_foil, price_eur, price_eur_foil, \
    price_tix, tcgplayer_uri, cardmarket_uri, cardhoarder_uri, game_changer, rarity, \
    pdh_legal, vintage_legal, pdh_commander";

pub struct CardDb {
    conn: Connection,
}

/// A card's default printing as recorded from Scryfall Oracle bulk data.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PrintingInfo {
    pub name: String,
    pub set_code: String,
    pub collector_number: String,
    pub double_faced: bool,
}

#[derive(Debug, Deserialize, Default)]
struct ImageUris {
    small: Option<String>,
    normal: Option<String>,
    large: Option<String>,
    png: Option<String>,
}

impl ImageUris {
    fn best(&self) -> Option<String> {
        // `normal` is ideal for a terminal preview: good quality without
        // downloading the much larger PNG for every selected card.
        self.normal
            .clone()
            .or_else(|| self.large.clone())
            .or_else(|| self.small.clone())
            .or_else(|| self.png.clone())
    }
}

#[derive(Debug, Deserialize, Default)]
struct Prices {
    usd: Option<String>,
    usd_foil: Option<String>,
    eur: Option<String>,
    eur_foil: Option<String>,
    tix: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ScryfallCard {
    oracle_id: Option<String>,
    name: String,

    #[serde(default)]
    layout: String,

    #[serde(default)]
    set_type: String,

    #[serde(default)]
    mana_cost: Option<String>,

    #[serde(default)]
    cmc: f64,

    #[serde(default)]
    type_line: Option<String>,

    #[serde(default)]
    oracle_text: Option<String>,

    #[serde(default)]
    color_identity: Vec<String>,

    #[serde(default)]
    produced_mana: Option<Vec<String>>,

    #[serde(default)]
    legalities: HashMap<String, String>,

    #[serde(default)]
    edhrec_rank: Option<u32>,

    #[serde(default)]
    keywords: Vec<String>,

    #[serde(default)]
    image_uris: Option<ImageUris>,

    #[serde(default)]
    prices: Option<Prices>,

    #[serde(default)]
    purchase_uris: Option<HashMap<String, String>>,

    #[serde(default)]
    game_changer: Option<bool>,

    #[serde(default)]
    card_faces: Option<Vec<ScryfallFace>>,

    #[serde(default)]
    set: String,

    #[serde(default)]
    collector_number: String,

    #[serde(default)]
    rarity: String,
}

#[derive(Debug, Deserialize)]
struct ScryfallFace {
    name: String,

    #[serde(default)]
    mana_cost: Option<String>,

    #[serde(default)]
    oracle_text: Option<String>,

    #[serde(default)]
    produced_mana: Option<Vec<String>>,

    #[serde(default)]
    image_uris: Option<ImageUris>,
}

impl CardDb {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        let conn = Connection::open(path)
            .with_context(|| format!("Could not open {}", path.display()))?;

        conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;

            CREATE TABLE IF NOT EXISTS cards (
                oracle_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                name_lower TEXT NOT NULL,
                mana_cost TEXT NOT NULL,
                cmc REAL NOT NULL,
                type_line TEXT NOT NULL,
                oracle_text TEXT NOT NULL,
                color_identity TEXT NOT NULL,
                produced_mana TEXT NOT NULL,
                commander_legal INTEGER NOT NULL,
                edhrec_rank INTEGER,
                keywords TEXT NOT NULL,
                is_basic INTEGER NOT NULL,
                commander_eligible INTEGER NOT NULL,
                image_uri TEXT NOT NULL DEFAULT '',
                price_usd REAL,
                price_usd_foil REAL,
                price_eur REAL,
                price_eur_foil REAL,
                price_tix REAL,
                tcgplayer_uri TEXT NOT NULL DEFAULT '',
                cardmarket_uri TEXT NOT NULL DEFAULT '',
                cardhoarder_uri TEXT NOT NULL DEFAULT '',
                game_changer INTEGER NOT NULL DEFAULT 0,
                rarity TEXT NOT NULL DEFAULT '',
                pdh_legal TEXT NOT NULL DEFAULT '',
                vintage_legal TEXT NOT NULL DEFAULT '',
                pdh_commander INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_cards_name_lower
                ON cards(name_lower);

            CREATE INDEX IF NOT EXISTS idx_cards_edhrec_rank
                ON cards(edhrec_rank);

            CREATE TABLE IF NOT EXISTS aliases (
                alias_lower TEXT NOT NULL,
                oracle_id TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_alias_lower
                ON aliases(alias_lower);

            -- Default printing per Oracle card, used for Moxfield export
            -- and MPC Autofill ordering.
            CREATE TABLE IF NOT EXISTS printings (
                oracle_id TEXT PRIMARY KEY,
                set_code TEXT NOT NULL,
                collector_number TEXT NOT NULL,
                double_faced INTEGER NOT NULL DEFAULT 0
            );
            "#,
        )?;

        // Safe migration from older Commander Lab databases.
        // Ignore the expected "duplicate column" error if image_uri is
        // already present.
        let _ = conn.execute(
            "ALTER TABLE cards ADD COLUMN image_uri TEXT NOT NULL DEFAULT ''",
            [],
        );

        for sql in [
            "ALTER TABLE cards ADD COLUMN price_usd REAL",
            "ALTER TABLE cards ADD COLUMN price_usd_foil REAL",
            "ALTER TABLE cards ADD COLUMN price_eur REAL",
            "ALTER TABLE cards ADD COLUMN price_eur_foil REAL",
            "ALTER TABLE cards ADD COLUMN price_tix REAL",
            "ALTER TABLE cards ADD COLUMN tcgplayer_uri TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE cards ADD COLUMN cardmarket_uri TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE cards ADD COLUMN cardhoarder_uri TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE cards ADD COLUMN game_changer INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE printings ADD COLUMN double_faced INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE cards ADD COLUMN rarity TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE cards ADD COLUMN pdh_legal TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE cards ADD COLUMN vintage_legal TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE cards ADD COLUMN pdh_commander INTEGER NOT NULL DEFAULT 0",
        ] {
            let _ = conn.execute(sql, []);
        }

        // Remove non-playable Art Series rows that older imports could
        // accidentally persist under a playable card's Oracle identity.
        let _ = conn.execute(
            "DELETE FROM aliases WHERE oracle_id IN (
                SELECT oracle_id FROM cards
                WHERE lower(trim(type_line)) = 'card'
            )",
            [],
        );
        let _ = conn.execute(
            "DELETE FROM cards WHERE lower(trim(type_line)) = 'card'",
            [],
        );

        // Repair eligibility in-place for databases produced by older
        // Commander Lab releases. Runtime checks still remain authoritative.
        let _ = conn.execute(
            r#"
            UPDATE cards
            SET commander_eligible = 1
            WHERE commander_eligible = 0
              AND (
                    (lower(type_line) LIKE '%legendary%'
                     AND lower(type_line) LIKE '%creature%')
                 OR lower(oracle_text) LIKE '%can be your commander%'
                 OR lower(oracle_text) LIKE '%can be a commander%'
                 OR lower(oracle_text) LIKE '%may be your commander%'
                 OR lower(name) = 'shorikai, genesis engine'
                 OR lower(name) = 'grist, the hunger tide'
              )
            "#,
            [],
        );

        Ok(Self { conn })
    }

    pub fn card_count(&self) -> Result<u64> {
        // rusqlite reads SQLite INTEGER values as i64.
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM cards", [], |row| row.get(0))?;

        u64::try_from(count)
            .map_err(|_| anyhow!("SQLite returned an invalid negative card count"))
    }

    pub fn has_cards(&self) -> bool {
        self.card_count().unwrap_or(0) > 1000
    }

    pub fn get_card(&self, name: &str) -> Result<Option<Card>> {
        let normalized = normalize_multiface_separator(name);
        let key = normalized.trim().to_ascii_lowercase();

        if let Some(card) = self.query_card_name(&key)? {
            return Ok(Some(card));
        }

        if let Some(card) = self.query_alias(&key)? {
            return Ok(Some(card));
        }

        // Compatibility fallback for multiface exports. If the combined
        // representation did not match, try each face independently.
        //
        // This handles:
        //   Front / Back
        //   Front // Back
        //
        // and older databases where only face aliases were imported.
        if normalized.contains(" // ") {
            for face in normalized.split(" // ") {
                let face_key = face.trim().to_ascii_lowercase();

                if let Some(card) = self.query_card_name(&face_key)? {
                    return Ok(Some(card));
                }

                if let Some(card) = self.query_alias(&face_key)? {
                    return Ok(Some(card));
                }
            }
        }

        Ok(None)
    }

    fn query_card_name(&self, key: &str) -> Result<Option<Card>> {
        self.conn
            .query_row(
                r#"
                SELECT oracle_id, name, mana_cost, cmc, type_line, oracle_text,
                       color_identity, produced_mana, commander_legal,
                       edhrec_rank, keywords, is_basic, commander_eligible,
                       image_uri, price_usd, price_usd_foil, price_eur,
                       price_eur_foil, price_tix, tcgplayer_uri,
                       cardmarket_uri, cardhoarder_uri, game_changer, rarity,
                   pdh_legal, vintage_legal, pdh_commander
                FROM cards
                WHERE name_lower = ?1
                LIMIT 1
                "#,
                [key],
                row_to_card,
            )
            .optional()
            .map_err(Into::into)
    }

    fn query_alias(&self, key: &str) -> Result<Option<Card>> {
        self.conn
            .query_row(
                r#"
                SELECT c.oracle_id, c.name, c.mana_cost, c.cmc, c.type_line,
                       c.oracle_text, c.color_identity, c.produced_mana,
                       c.commander_legal, c.edhrec_rank, c.keywords,
                       c.is_basic, c.commander_eligible, c.image_uri,
                       c.price_usd, c.price_usd_foil, c.price_eur,
                       c.price_eur_foil, c.price_tix, c.tcgplayer_uri,
                       c.cardmarket_uri, c.cardhoarder_uri, c.game_changer, c.rarity,
                       c.pdh_legal, c.vintage_legal, c.pdh_commander
                FROM aliases a
                JOIN cards c ON c.oracle_id = a.oracle_id
                WHERE a.alias_lower = ?1
                LIMIT 1
                "#,
                [key],
                row_to_card,
            )
            .optional()
            .map_err(Into::into)
    }

    /// Default printing for a card name, if the card database has been
    /// refreshed since printings were recorded.
    pub fn printing(&self, name: &str) -> Result<Option<PrintingInfo>> {
        let Some(card) = self.get_card(name)? else {
            return Ok(None);
        };

        self.conn
            .query_row(
                "SELECT set_code, collector_number, double_faced \
                 FROM printings WHERE oracle_id = ?1",
                [&card.oracle_id],
                |row| {
                    Ok(PrintingInfo {
                        name: card.name.clone(),
                        set_code: row.get(0)?,
                        collector_number: row.get(1)?,
                        double_faced: row.get::<_, i64>(2)? != 0,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn top_ranked_cards(&self, limit: usize) -> Result<Vec<Card>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT oracle_id, name, mana_cost, cmc, type_line, oracle_text,
                   color_identity, produced_mana, commander_legal,
                   edhrec_rank, keywords, is_basic, commander_eligible,
                   image_uri, price_usd, price_usd_foil, price_eur,
                   price_eur_foil, price_tix, tcgplayer_uri,
                   cardmarket_uri, cardhoarder_uri, game_changer, rarity,
                   pdh_legal, vintage_legal, pdh_commander
            FROM cards
            WHERE commander_legal = 1
              AND edhrec_rank IS NOT NULL
            ORDER BY edhrec_rank ASC
            LIMIT ?1
            "#,
        )?;

        let rows = stmt.query_map([limit as i64], row_to_card)?;
        let mut cards = Vec::new();

        for row in rows {
            cards.push(row?);
        }

        Ok(cards)
    }

    /// Download the current Scryfall Oracle Cards JSONL.GZ file and rebuild
    /// the local SQLite card database by streaming one JSON object at a time.
    ///
    /// Nothing close to the full decompressed dataset is held in memory.
    pub fn search_commanders(&self, query: &str, limit: usize) -> Result<Vec<Card>> {
        let pattern = format!("%{}%", query.trim().to_ascii_lowercase());

        let mut stmt = self.conn.prepare(
            r#"
            SELECT oracle_id, name, mana_cost, cmc, type_line, oracle_text,
                   color_identity, produced_mana, commander_legal,
                   edhrec_rank, keywords, is_basic, commander_eligible,
                   image_uri, price_usd, price_usd_foil, price_eur,
                   price_eur_foil, price_tix, tcgplayer_uri,
                   cardmarket_uri, cardhoarder_uri, game_changer, rarity,
                   pdh_legal, vintage_legal, pdh_commander
            FROM cards
            WHERE commander_legal = 1
              AND name_lower LIKE ?1
            ORDER BY
                CASE WHEN name_lower = ?2 THEN 0 ELSE 1 END,
                edhrec_rank IS NULL,
                edhrec_rank ASC,
                name ASC
            LIMIT ?3
            "#,
        )?;

        let exact = query.trim().to_ascii_lowercase();
        let rows = stmt.query_map(
            params![pattern, exact, limit as i64],
            row_to_card,
        )?;

        let mut cards = Vec::new();

        for row in rows {
            let mut card = row?;

            // Do not trust only the persisted commander_eligible bit. Older
            // Commander Lab databases classified "Legendary Artifact Creature"
            // and similar type lines too narrowly.
            card.commander_eligible =
                effective_commander_eligible(&card);

            if card.commander_eligible {
                cards.push(card);
            }
        }

        Ok(cards)
    }

    /// Card-name search for the deck editor: prefix matches first, then
    /// substring matches, most-played (EDHREC rank) first.
    pub fn search_cards(&self, query: &str, limit: usize) -> Result<Vec<Card>> {
        let needle = query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return Ok(Vec::new());
        }

        let mut stmt = self.conn.prepare(
            r#"
            SELECT oracle_id, name, mana_cost, cmc, type_line, oracle_text,
                   color_identity, produced_mana, commander_legal,
                   edhrec_rank, keywords, is_basic, commander_eligible,
                   image_uri, price_usd, price_usd_foil, price_eur,
                   price_eur_foil, price_tix, tcgplayer_uri,
                   cardmarket_uri, cardhoarder_uri, game_changer, rarity,
                   pdh_legal, vintage_legal, pdh_commander
            FROM cards
            WHERE name_lower LIKE ?1
            ORDER BY
                CASE WHEN name_lower = ?2 THEN 0
                     WHEN name_lower LIKE ?3 THEN 1
                     ELSE 2 END,
                edhrec_rank IS NULL,
                edhrec_rank ASC,
                name ASC
            LIMIT ?4
            "#,
        )?;

        let rows = stmt.query_map(
            params![
                format!("%{needle}%"),
                needle,
                format!("{needle}%"),
                limit as i64
            ],
            row_to_card,
        )?;

        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn update_from_scryfall(&mut self) -> Result<usize> {
        std::fs::create_dir_all(DATA_DIR)?;
        std::fs::create_dir_all(DEBUG_DIR)?;

        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .context("Could not create HTTP client")?;

        let metadata_body = download_metadata(&client)?;

        let download_uri = extract_jsonl_download_uri(&metadata_body)
            .context("Scryfall metadata did not contain a usable bulk download URL")?;

        download_gzip(&client, &download_uri, Path::new(BULK_GZIP_PATH))?;

        let inserted = self.rebuild_from_jsonl_gzip(Path::new(BULK_GZIP_PATH))?;

        // Pauper Commander eligibility depends on every printing, not just the
        // default one in the Oracle file. Ask Scryfall; keep the first guess if
        // it can't be reached.
        if let Err(err) = self.refresh_pdh_commanders(&client) {
            crate::logging::record("cards", format!("PDH commander refresh skipped: {err:#}"));
        }

        Ok(inserted)
    }

    /// Rebuild the database from a Scryfall card file saved on this device
    /// (Oracle Cards bulk data, `.jsonl.gz`). For networks that block
    /// Scryfall's bulk download host.
    pub fn import_file(&mut self, path: &Path) -> Result<usize> {
        let inserted = self.rebuild_from_jsonl_gzip(path)?;
        let client = Client::builder().build()?;
        if let Err(err) = self.refresh_pdh_commanders(&client) {
            crate::logging::record("cards", format!("PDH commander refresh skipped: {err:#}"));
        }
        Ok(inserted)
    }

    /// Mark every card printed at uncommon in paper or on MTGO as a creature,
    /// Vehicle, Spacecraft or Background (the PDH commander rule), using
    /// Scryfall's search across all printings.
    pub fn refresh_pdh_commanders(&mut self, client: &Client) -> Result<usize> {
        let query = "r:u (t:creature or t:vehicle or t:spacecraft or t:background) \
                     -t:land -t:conspiracy not:funny (game:paper or game:mtgo)";
        let mut url = reqwest::Url::parse_with_params(
            "https://api.scryfall.com/cards/search",
            &[("q", query), ("unique", "cards"), ("order", "name")],
        )?
        .to_string();
        let mut names = Vec::<String>::new();

        for _page in 0..80 {
            let body: Value = client
                .get(&url)
                .header(USER_AGENT, USER_AGENT_VALUE)
                .header(ACCEPT, "application/json")
                .send()?
                .error_for_status()?
                .json()?;
            if let Some(cards) = body.get("data").and_then(Value::as_array) {
                for card in cards {
                    if let Some(name) = card.get("name").and_then(Value::as_str) {
                        names.push(name.to_ascii_lowercase());
                    }
                }
            }
            match body.get("next_page").and_then(Value::as_str) {
                Some(next) if body.get("has_more").and_then(Value::as_bool) == Some(true) => {
                    url = next.to_string();
                    // Scryfall asks for 50–100 ms between requests.
                    std::thread::sleep(std::time::Duration::from_millis(110));
                }
                _ => break,
            }
        }

        if names.len() < 1000 {
            return Err(anyhow!("Scryfall returned only {} PDH commanders", names.len()));
        }

        let tx = self.conn.transaction()?;
        tx.execute("UPDATE cards SET pdh_commander = 0", [])?;
        {
            let mut stmt = tx.prepare(
                "UPDATE cards SET pdh_commander = 1 WHERE name_lower = ?1 \
                 OR oracle_id IN (SELECT oracle_id FROM aliases WHERE alias_lower = ?1)",
            )?;
            for name in &names {
                stmt.execute([name])?;
            }
        }
        tx.commit()?;
        Ok(names.len())
    }

    /// Ask Scryfall whether one card was printed at uncommon in paper or on
    /// MTGO (the PDH commander rule), for when the bulk refresh didn't run.
    /// Remembers a yes in the database.
    pub fn confirm_pdh_commander(&mut self, name: &str) -> bool {
        let query = format!(
            "!\"{}\" r:u (t:creature or t:vehicle or t:spacecraft or t:background) -t:land (game:paper or game:mtgo)",
            name.split(" // ").next().unwrap_or(name)
        );
        let Ok(url) = reqwest::Url::parse_with_params("https://api.scryfall.com/cards/search", &[("q", query.as_str())]) else {
            return false;
        };
        let found = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()
            .and_then(|c| c.get(url).header(USER_AGENT, USER_AGENT_VALUE).header(ACCEPT, "application/json").send().ok())
            .and_then(|r| r.json::<Value>().ok())
            .and_then(|v| v.get("total_cards").and_then(Value::as_u64))
            .unwrap_or(0)
            > 0;
        if found {
            let _ = self.conn.execute("UPDATE cards SET pdh_commander = 1 WHERE name_lower = ?1", [name.to_ascii_lowercase()]);
        }
        found
    }

    /// How many cards are marked as PDH commanders (thousands once the
    /// Scryfall refresh has run).
    pub fn pdh_commander_count(&self) -> u64 {
        self.conn
            .query_row("SELECT COUNT(*) FROM cards WHERE pdh_commander = 1", [], |r| r.get::<_, i64>(0))
            .map(|n| n.max(0) as u64)
            .unwrap_or(0)
    }

    /// Visit every playable card (optionally only those inside a color
    /// identity). Used by deep card search, which reads Oracle text across the
    /// whole database rather than only popular cards.
    pub fn scan_cards(&self, mut visit: impl FnMut(Card)) -> Result<()> {
        let sql = format!("SELECT {CARD_COLUMNS} FROM cards");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_card)?;
        for row in rows {
            visit(row?);
        }
        Ok(())
    }

    /// Cards that may lead a Pauper Commander deck, most played first.
    pub fn search_pdh_commanders(&self, query: &str, limit: usize) -> Result<Vec<Card>> {
        // If the Scryfall refresh never ran, the flag only reflects each
        // card's default printing; offer every creature-type card that's
        // legal in PDH and let picking it confirm the rule online.
        let filter = if self.pdh_commander_count() >= 1000 {
            "pdh_commander = 1"
        } else {
            "(pdh_commander = 1 OR (pdh_legal IN ('legal', 'not_legal') AND vintage_legal <> 'not_legal' \
             AND (lower(type_line) LIKE '%creature%' OR lower(type_line) LIKE '%vehicle%' OR lower(type_line) LIKE '%background%') \
             AND lower(type_line) NOT LIKE '%land%' AND rarity IN ('common', 'uncommon')))"
        };
        let sql = format!(
            "SELECT {CARD_COLUMNS} FROM cards WHERE {filter} AND name_lower LIKE ?1 \
             ORDER BY CASE WHEN name_lower = ?2 THEN 0 ELSE 1 END, edhrec_rank IS NULL, edhrec_rank, name LIMIT ?3"
        );
        let needle = query.trim().to_ascii_lowercase();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![format!("%{needle}%"), needle, limit as i64], row_to_card)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// True once the database has the format columns filled (card data
    /// downloaded by Commander Lab 0.27 or later).
    pub fn has_format_data(&self) -> bool {
        self.conn
            .query_row("SELECT COUNT(*) FROM cards WHERE vintage_legal <> ''", [], |r| r.get::<_, i64>(0))
            .map(|n| n > 1000)
            .unwrap_or(false)
    }


    fn rebuild_from_jsonl_gzip(&mut self, path: &Path) -> Result<usize> {
        let file = File::open(path)
            .with_context(|| format!("Could not open {}", path.display()))?;

        let decoder = GzDecoder::new(file);
        let reader = BufReader::with_capacity(1024 * 1024, decoder);

        let tx = self.conn.transaction()?;

        tx.execute("DELETE FROM aliases", [])?;
        tx.execute("DELETE FROM cards", [])?;
        tx.execute("DELETE FROM printings", [])?;

        let inserted = import_jsonl(reader, &tx)?;

        tx.commit()?;

        if inserted < 1000 {
            return Err(anyhow!(
                "Only {inserted} cards were imported. The Scryfall bulk file \
                 does not appear to contain a complete Oracle Cards dataset."
            ));
        }

        Ok(inserted)
    }
}

fn download_metadata(client: &Client) -> Result<Vec<u8>> {
    let response = client
        .get(ORACLE_BULK_METADATA_URL)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "application/json")
        .send()
        .context("Could not contact Scryfall Oracle Cards metadata endpoint")?;

    let status = response.status();
    let final_url = response.url().to_string();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("<missing>")
        .to_string();

    let body = response
        .bytes()
        .context("Could not read Scryfall metadata response")?
        .to_vec();

    // Always keep the most recent metadata response. If Scryfall changes its
    // schema again, this makes the next diagnosis straightforward.
    std::fs::write(
        PathBuf::from(DEBUG_DIR).join("scryfall-oracle-cards.body"),
        &body,
    )?;

    std::fs::write(
        PathBuf::from(DEBUG_DIR).join("scryfall-oracle-cards.meta.txt"),
        format!(
            "requested_url={ORACLE_BULK_METADATA_URL}\n\
             final_url={final_url}\n\
             status={status}\n\
             content_type={content_type}\n\
             bytes={}\n",
            body.len()
        ),
    )?;

    if !status.is_success() {
        return Err(anyhow!(
            "Scryfall metadata returned HTTP {status}. \
             Raw response saved under {DEBUG_DIR}."
        ));
    }

    Ok(body)
}

/// Scryfall's current Oracle Cards metadata uses `jsonl_download_uri`.
///
/// For forward/backward compatibility, we also accept several historical or
/// plausible URL field names and recursively search the JSON object.
fn extract_jsonl_download_uri(body: &[u8]) -> Result<String> {
    let value: Value = serde_json::from_slice(body)
        .context("Could not parse Scryfall Oracle Cards metadata JSON")?;

    // Current format observed in 2026.
    for key in [
        "jsonl_download_uri",
        "download_uri",
        "download_url",
        "downloadUrl",
    ] {
        if let Some(uri) = value.get(key).and_then(Value::as_str) {
            if is_http_url(uri) {
                return Ok(uri.to_string());
            }
        }
    }

    // Future-proof recursive fallback.
    if let Some(uri) = recursive_download_url(&value) {
        return Ok(uri);
    }

    Err(anyhow!(
        "No JSONL/bulk download URL was found in Scryfall metadata"
    ))
}

fn recursive_download_url(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if let Some(uri) = child.as_str() {
                    let key_lower = key.to_ascii_lowercase();

                    if (key_lower.contains("download")
                        || key_lower.contains("jsonl"))
                        && is_http_url(uri)
                    {
                        return Some(uri.to_string());
                    }
                }

                if let Some(uri) = recursive_download_url(child) {
                    return Some(uri);
                }
            }
        }

        Value::Array(items) => {
            for item in items {
                if let Some(uri) = recursive_download_url(item) {
                    return Some(uri);
                }
            }
        }

        _ => {}
    }

    None
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn download_gzip(client: &Client, url: &str, path: &Path) -> Result<()> {
    let mut response = client
        .get(url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(
            ACCEPT,
            "application/gzip, application/octet-stream, application/json;q=0.9, */*;q=0.5",
        )
        .send()
        .with_context(|| format!("Could not download Scryfall Oracle Cards from {url}"))?;

    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("<missing>")
        .to_string();

    if !status.is_success() {
        return Err(anyhow!(
            "Scryfall Oracle Cards download returned HTTP {status}"
        ));
    }

    let temp_path = path.with_extension("jsonl.gz.part");
    let mut output = File::create(&temp_path)
        .with_context(|| format!("Could not create {}", temp_path.display()))?;

    let bytes = std::io::copy(&mut response, &mut output)
        .context("Could not save Scryfall Oracle Cards gzip file")?;

    output.flush()?;
    drop(output);

    std::fs::write(
        PathBuf::from(DEBUG_DIR).join("scryfall-oracle-download.meta.txt"),
        format!(
            "url={url}\n\
             status={status}\n\
             content_type={content_type}\n\
             bytes={bytes}\n"
        ),
    )?;

    if bytes < 1_000_000 {
        return Err(anyhow!(
            "Scryfall bulk download was unexpectedly small ({bytes} bytes). \
             The partial file was kept at {}.",
            temp_path.display()
        ));
    }

    // Validate the gzip header before replacing the previous cache.
    let mut probe = File::open(&temp_path)?;
    let mut magic = [0u8; 2];
    probe.read_exact(&mut magic)?;

    if magic != [0x1f, 0x8b] {
        return Err(anyhow!(
            "Scryfall bulk response was not a gzip file. \
             The response was kept at {} for inspection.",
            temp_path.display()
        ));
    }

    if path.exists() {
        std::fs::remove_file(path)?;
    }

    std::fs::rename(&temp_path, path)?;

    Ok(())
}

fn import_jsonl<R: BufRead>(reader: R, tx: &Transaction<'_>) -> Result<usize> {
    let mut card_stmt = tx.prepare(
        r#"
        INSERT OR REPLACE INTO cards (
            oracle_id, name, name_lower, mana_cost, cmc, type_line,
            oracle_text, color_identity, produced_mana,
            commander_legal, edhrec_rank, keywords, is_basic,
            commander_eligible, image_uri, price_usd, price_usd_foil,
            price_eur, price_eur_foil, price_tix, tcgplayer_uri,
            cardmarket_uri, cardhoarder_uri, game_changer, rarity,
            pdh_legal, vintage_legal, pdh_commander
        )
        VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
            ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
            ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28
        )
        "#,
    )?;

    let mut alias_stmt = tx.prepare(
        "INSERT INTO aliases(alias_lower, oracle_id) VALUES (?1, ?2)",
    )?;

    let mut printing_stmt = tx.prepare(
        "INSERT OR REPLACE INTO printings(oracle_id, set_code, collector_number, double_faced) \
         VALUES (?1, ?2, ?3, ?4)",
    )?;

    let mut inserted = 0usize;

    for (line_number, line_result) in reader.lines().enumerate() {
        let line = line_result.with_context(|| {
            format!("Could not read JSONL line {}", line_number + 1)
        })?;

        if line.trim().is_empty() {
            continue;
        }

        let raw: ScryfallCard = serde_json::from_str(&line).with_context(|| {
            let snippet: String = line.chars().take(160).collect();

            format!(
                "Could not parse Scryfall JSONL line {}. Starts with: {}",
                line_number + 1,
                snippet
            )
        })?;

        let nonplayable_object =
            raw.layout.eq_ignore_ascii_case("art_series")
                || raw.set_type.eq_ignore_ascii_case("memorabilia")
                || raw
                    .type_line
                    .as_deref()
                    .map(|value| value.trim().eq_ignore_ascii_case("card"))
                    .unwrap_or(false);

        if nonplayable_object {
            continue;
        }

        let Some(oracle_id) = raw.oracle_id.clone() else {
            continue;
        };

        let type_line = raw.type_line.unwrap_or_default();
        let mut oracle_text = raw.oracle_text.unwrap_or_default();
        let mut mana_cost = raw.mana_cost.unwrap_or_default();
        let mut produced = raw.produced_mana.unwrap_or_default();

        let mut face_names = Vec::new();

        let mut image_uri = raw
            .image_uris
            .as_ref()
            .and_then(ImageUris::best)
            .unwrap_or_default();

        if let Some(faces) = raw.card_faces.as_ref() {
            face_names.extend(faces.iter().map(|face| face.name.clone()));

            if oracle_text.is_empty() {
                oracle_text = faces
                    .iter()
                    .filter_map(|face| face.oracle_text.as_deref())
                    .collect::<Vec<_>>()
                    .join("\n//\n");
            }

            if mana_cost.is_empty() {
                mana_cost = faces
                    .iter()
                    .filter_map(|face| face.mana_cost.as_deref())
                    .collect::<Vec<_>>()
                    .join(" // ");
            }

            for face in faces {
                if let Some(face_mana) = &face.produced_mana {
                    for color in face_mana {
                        if !produced.contains(color) {
                            produced.push(color.clone());
                        }
                    }
                }

                if image_uri.is_empty() {
                    if let Some(uri) =
                        face.image_uris.as_ref().and_then(ImageUris::best)
                    {
                        image_uri = uri;
                    }
                }
            }
        }

        let identity = canonical_colors(&raw.color_identity);
        let produced_mana = canonical_colors(&produced);

        let commander_legal = raw
            .legalities
            .get("commander")
            .map(|value| value == "legal")
            .unwrap_or(false);

        let lower_oracle = oracle_text.to_ascii_lowercase();
        let lower_type = type_line.to_ascii_lowercase();

        let is_basic = lower_type.contains("basic land");

        let commander_eligible =
            commander_eligible_from_text(&lower_type, &lower_oracle)
                || known_commander_exception(&raw.name);

        let prices = raw.prices.unwrap_or_default();
        let price_usd = parse_price(prices.usd.as_deref());
        let price_usd_foil = parse_price(prices.usd_foil.as_deref());
        let price_eur = parse_price(prices.eur.as_deref());
        let price_eur_foil = parse_price(prices.eur_foil.as_deref());
        let price_tix = parse_price(prices.tix.as_deref());

        let purchase_uris = raw.purchase_uris.unwrap_or_default();

        let tcgplayer_uri = purchase_uris
            .get("tcgplayer")
            .cloned()
            .unwrap_or_default();
        let cardmarket_uri = purchase_uris
            .get("cardmarket")
            .cloned()
            .unwrap_or_default();
        let cardhoarder_uri = purchase_uris
            .get("cardhoarder")
            .cloned()
            .unwrap_or_default();

        card_stmt.execute(params![
            oracle_id,
            raw.name,
            raw.name.to_ascii_lowercase(),
            mana_cost,
            raw.cmc,
            type_line,
            oracle_text,
            identity,
            produced_mana,
            commander_legal as i32,
            raw.edhrec_rank,
            raw.keywords.join(","),
            is_basic as i32,
            commander_eligible as i32,
            image_uri,
            price_usd,
            price_usd_foil,
            price_eur,
            price_eur_foil,
            price_tix,
            tcgplayer_uri,
            cardmarket_uri,
            cardhoarder_uri,
            raw.game_changer.unwrap_or(false) as i32,
            raw.rarity,
            raw.legalities.get("paupercommander").cloned().unwrap_or_default(),
            raw.legalities.get("vintage").cloned().unwrap_or_default(),
            pdh_commander_from_print(&raw.rarity, &lower_type) as i32,
        ])?;

        if !raw.set.is_empty() && !raw.collector_number.is_empty() {
            let double_faced = matches!(
                raw.layout.to_ascii_lowercase().as_str(),
                "transform" | "modal_dfc" | "double_faced_token" | "reversible_card"
            );
            printing_stmt.execute(params![
                oracle_id,
                raw.set.to_ascii_uppercase(),
                raw.collector_number,
                double_faced as i32,
            ])?;
        }

        alias_stmt.execute(params![
            raw.name.to_ascii_lowercase(),
            oracle_id
        ])?;

        for face_name in face_names {
            alias_stmt.execute(params![
                face_name.to_ascii_lowercase(),
                oracle_id
            ])?;
        }

        inserted += 1;
    }

    Ok(inserted)
}

pub fn commander_eligible_from_text(
    lower_type: &str,
    lower_oracle: &str,
) -> bool {
    let legendary_creature =
        lower_type.contains("legendary")
            && lower_type.contains("creature");

    // Keep this deliberately phrase-tolerant. Older/local Oracle dumps and
    // alternate exports have used slightly different wording/punctuation.
    let explicit_commander =
        lower_oracle.contains("can be your commander")
            || lower_oracle.contains("can be a commander")
            || lower_oracle.contains("may be your commander");

    let background =
        lower_type.contains("legendary")
            && lower_type.contains("enchantment")
            && lower_type.contains("background");

    legendary_creature || explicit_commander || background
}

pub fn known_commander_exception(name: &str) -> bool {
    // Compatibility safety net for cards whose local Oracle text may be empty,
    // stale, or imported from a dataset that omitted the commander-permission
    // sentence. Runtime text rules remain the primary mechanism.
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "shorikai, genesis engine"
            | "shorikai genesis engine"
            | "grist, the hunger tide"
            | "grist the hunger tide"
    )
}

pub fn effective_commander_eligible(card: &Card) -> bool {
    if card.commander_eligible {
        return true;
    }

    if commander_eligible_from_text(
        &card.type_line.to_ascii_lowercase(),
        &card.oracle_text.to_ascii_lowercase(),
    ) {
        return true;
    }

    known_commander_exception(&card.name)
}

pub fn commander_eligibility_reason(card: &Card) -> &'static str {
    let lower_type = card.type_line.to_ascii_lowercase();
    let lower_oracle = card.oracle_text.to_ascii_lowercase();

    if lower_type.contains("legendary")
        && lower_type.contains("creature")
    {
        return "legendary creature type line";
    }

    if lower_oracle.contains("can be your commander")
        || lower_oracle.contains("can be a commander")
        || lower_oracle.contains("may be your commander")
    {
        return "explicit Oracle commander permission";
    }

    if lower_type.contains("legendary")
        && lower_type.contains("enchantment")
        && lower_type.contains("background")
    {
        return "legendary Background";
    }

    if known_commander_exception(&card.name) {
        return "known commander compatibility exception";
    }

    if card.commander_eligible {
        return "stored eligibility flag";
    }

    "no supported commander eligibility rule matched"
}

fn row_to_card(row: &rusqlite::Row<'_>) -> rusqlite::Result<Card> {
    let type_line: String = row.get(4)?;
    let oracle_text: String = row.get(5)?;
    let stored_commander_eligible = row.get::<_, i64>(12)? != 0;

    let name: String = row.get(1)?;

    let commander_eligible =
        stored_commander_eligible
            || commander_eligible_from_text(
                &type_line.to_ascii_lowercase(),
                &oracle_text.to_ascii_lowercase(),
            )
            || known_commander_exception(&name);

    Ok(Card {
        oracle_id: row.get(0)?,
        name,
        mana_cost: row.get(2)?,
        cmc: row.get(3)?,
        type_line,
        oracle_text,
        color_identity: row.get(6)?,
        produced_mana: row.get(7)?,
        commander_legal: row.get::<_, i64>(8)? != 0,
        edhrec_rank: row.get(9)?,
        keywords: row.get(10)?,
        is_basic: row.get::<_, i64>(11)? != 0,
        commander_eligible,
        image_uri: row.get(13)?,
        price_usd: row.get(14)?,
        price_usd_foil: row.get(15)?,
        price_eur: row.get(16)?,
        price_eur_foil: row.get(17)?,
        price_tix: row.get(18)?,
        tcgplayer_uri: row.get(19)?,
        cardmarket_uri: row.get(20)?,
        cardhoarder_uri: row.get(21)?,
        game_changer: row.get::<_, i64>(22)? != 0,
        rarity: row.get(23)?,
        pdh_legal: row.get(24)?,
        vintage_legal: row.get(25)?,
        pdh_commander: row.get::<_, i64>(26)? != 0,
    })
}

/// First guess at Pauper Commander eligibility from one printing. The card
/// data update replaces it with Scryfall's answer across every paper and MTGO
/// printing (`refresh_pdh_commanders`).
fn pdh_commander_from_print(rarity: &str, lower_type: &str) -> bool {
    rarity.eq_ignore_ascii_case("uncommon") && pdh_commander_type(lower_type)
}

/// PDH commanders are uncommon creatures, Vehicles or Spacecraft (and
/// uncommon Backgrounds as the second commander); never lands.
pub fn pdh_commander_type(lower_type: &str) -> bool {
    !lower_type.contains("land")
        && (lower_type.contains("creature")
            || lower_type.contains("vehicle")
            || lower_type.contains("spacecraft")
            || lower_type.contains("background"))
}

fn parse_price(value: Option<&str>) -> Option<f64> {
    value.and_then(|text| text.parse::<f64>().ok())
}

pub fn canonical_colors(colors: &[String]) -> String {
    const ORDER: [char; 5] = ['W', 'U', 'B', 'R', 'G'];

    let mut result = String::new();

    for color in ORDER {
        if colors
            .iter()
            .any(|value| value.eq_ignore_ascii_case(&color.to_string()))
        {
            result.push(color);
        }
    }

    result
}

#[cfg(test)]
mod commander_eligibility_tests {
    use super::commander_eligible_from_text;

    #[test]
    fn legendary_artifact_creature_is_commander_eligible() {
        assert!(commander_eligible_from_text(
            "legendary artifact creature — myr",
            "",
        ));
    }

    #[test]
    fn legendary_enchantment_creature_is_commander_eligible() {
        assert!(commander_eligible_from_text(
            "legendary enchantment creature — god",
            "",
        ));
    }

    #[test]
    fn explicit_noncreature_commander_is_eligible() {
        assert!(commander_eligible_from_text(
            "legendary artifact — vehicle",
            "shorikai, genesis engine can be your commander.",
        ));
    }

    #[test]
    fn shorikai_has_compatibility_exception() {
        assert!(super::known_commander_exception(
            "Shorikai, Genesis Engine",
        ));
    }

    #[test]
    fn grist_has_compatibility_exception() {
        assert!(super::known_commander_exception(
            "Grist, the Hunger Tide",
        ));
    }

    #[test]
    fn ordinary_legendary_artifact_is_not_eligible() {
        assert!(!commander_eligible_from_text(
            "legendary artifact",
            "",
        ));
    }
}
