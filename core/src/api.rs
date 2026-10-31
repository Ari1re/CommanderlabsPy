//! JSON API used by every Commander Lab front end.
//!
//! One entry point: [`Engine::call`]`(command, args) -> JSON`. The Tauri app
//! forwards `invoke("api", { cmd, args })` here and the web server forwards
//! `POST /api/<cmd>` here, so both run exactly the same logic.
//!
//! All file paths are relative to the data directory; [`Engine::new`] makes it
//! the process working directory, which is what the engine modules expect
//! (`./decks`, `./data`, `./exports`).

use crate::{
    ai::{chat_with_deck_agent, model_presets, AgentChatMessage, AiConfig, AiProvider},
    ai_runtime::LocalAiRuntime,
    analysis::{analyze_deck, classify_card, identity_subset, CardRoles},
    archidekt::import_archidekt,
    bracket::refine_with_combo_report,
    commander::{commander_name_suggestions, resolve_generation_commanders},
    data::{Card, CardDb},
    deck::{
        deck_to_moxfield_text, deck_to_text, load_deck_files, parse_deck_file, parse_deck_text,
        safe_deck_filename, write_deck_file, DeckEntry, ExportPrinting, ParsedDeck,
    },
    deck_history::{delete_history, load_history, render_history_lines, save_agent_session},
    deep_agent::{run_optimization_agent, OptimizationGoal},
    deep_learn::{simulate_deep_learn_games, TestingIntensity},
    edhrec::commander_build_cards,
    generator::{
        game_plan_markdown, generate_ai_upgrade, generate_deck_with_options, resolve_pdh_commanders,
        suggest_generation_themes, GamePlan, GeneratedDeck, GenerationBracket, GenerationOptions,
    },
    format::{check_deck, Format, CANLANDER_MAX_POINTS, CANLANDER_POINTS, CANLANDER_POINTS_DATE},
    highlander::{generate_highlander, HIGHLANDER_THEMES},
    oracle_search::deep_search,
    guide::build_guide,
    printings::printings,
    share,
    spellbook::ComboReport,
    versions,
    jobs::Jobs,
    moxfield::import_moxfield,
    mpc::{MpcClient, MpcConfig, MpcOrder, OrderLine},
    simulation::run_draw_simulation,
    sources::{
        delete_sources_sidecar, load_sources_for_deck, save_sources_for_deck,
        sources_for_existing_deck, DeckSource,
    },
    spellbook::find_my_combos,
    user_reference::import_user_reference,
};
use anyhow::{anyhow, bail, Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::UNIX_EPOCH,
};

pub const DB_PATH: &str = "./data/cards.sqlite";
const DECKS_DIR: &str = "./decks";
const EXPORTS_DIR: &str = "./exports";
const SETTINGS_PATH: &str = "./data/ui_settings.json";
const GAMES_PATH: &str = "./data/games.json";

pub struct Engine {
    root: PathBuf,
    platform: String,
    db: Mutex<CardDb>,
    jobs: Arc<Jobs>,
    mpc_orders: Mutex<HashMap<String, MpcOrder>>,
    ai_runtime: Mutex<Option<LocalAiRuntime>>,
}

fn arg<T: DeserializeOwned>(args: &Value, key: &str) -> Result<T> {
    let value = args.get(key).cloned().unwrap_or(Value::Null);
    serde_json::from_value(value).with_context(|| format!("Missing or invalid argument '{key}'"))
}

fn opt<T: DeserializeOwned>(args: &Value, key: &str) -> Option<T> {
    args.get(key)
        .cloned()
        .filter(|v| !v.is_null())
        .and_then(|v| serde_json::from_value(v).ok())
}

fn to_value<T: Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(Into::into)
}

// ---------------------------------------------------------------------------
// View models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct CardView {
    #[serde(flatten)]
    pub card: Card,
    pub roles: CardRoles,
    pub kind: &'static str,
    pub set_code: Option<String>,
    pub collector_number: Option<String>,
    pub double_faced: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryView {
    pub quantity: u32,
    pub name: String,
    pub zone: &'static str,
    pub card: Option<CardView>,
    pub set_code: Option<String>,
    pub collector_number: Option<String>,
    pub foil: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckSummary {
    pub name: String,
    pub commanders: Vec<String>,
    pub identity: String,
    pub cards: u32,
    pub image: String,
    pub modified_ms: u128,
    pub has_plan: bool,
    pub price_usd: f64,
    pub format: String,
}

/// Primary card type used for grouping, Moxfield-style.
pub fn card_kind(type_line: &str) -> &'static str {
    let front = type_line.split("//").next().unwrap_or(type_line).to_ascii_lowercase();
    if front.contains("creature") {
        "Creature"
    } else if front.contains("planeswalker") {
        "Planeswalker"
    } else if front.contains("battle") {
        "Battle"
    } else if front.contains("instant") {
        "Instant"
    } else if front.contains("sorcery") {
        "Sorcery"
    } else if front.contains("land") {
        "Land"
    } else if front.contains("artifact") {
        "Artifact"
    } else if front.contains("enchantment") {
        "Enchantment"
    } else {
        "Other"
    }
}

impl Engine {
    /// Create the engine with `root` as the data directory (decks, card
    /// database, exports, settings).
    pub fn new(root: impl Into<PathBuf>, platform: &str) -> Result<Arc<Self>> {
        let root = root.into();
        fs::create_dir_all(&root).with_context(|| format!("Could not create {}", root.display()))?;
        std::env::set_current_dir(&root)
            .with_context(|| format!("Could not enter {}", root.display()))?;
        for dir in [DECKS_DIR, "./data", "./data/images", EXPORTS_DIR] {
            fs::create_dir_all(dir)?;
        }

        let db = CardDb::open(DB_PATH)?;

        Ok(Arc::new(Self {
            root,
            platform: platform.to_string(),
            db: Mutex::new(db),
            jobs: Arc::new(Jobs::default()),
            mpc_orders: Mutex::new(HashMap::new()),
            ai_runtime: Mutex::new(None),
        }))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn shutdown(&self) {
        if let Some(runtime) = self.ai_runtime.lock().unwrap().as_mut() {
            runtime.shutdown();
        }
    }

    /// Dispatch one API command.
    pub fn call(self: &Arc<Self>, cmd: &str, args: Value) -> Result<Value> {
        let a = &args;
        match cmd {
            // --- app ----------------------------------------------------------
            "status" => self.status(),
            "settings_get" => Ok(read_settings()),
            "settings_set" => self.settings_set(a),

            // --- jobs ---------------------------------------------------------
            "job" => {
                let id: u64 = arg(a, "id")?;
                to_value(self.jobs.get(id).ok_or_else(|| anyhow!("Unknown job {id}"))?)
            }
            "jobs" => to_value(self.jobs.list()),

            // --- card data ------------------------------------------------------
            "update_cards" => Ok(json!({ "job": self.start_update_cards() })),
            "update_check" => to_value(crate::updater::check(&self.platform, opt(a, "force").unwrap_or(false))?),
            "update_install" => {
                let platform = self.platform.clone();
                Ok(json!({ "job": self.jobs.spawn("update_install", "", move |job| {
                    job.progress(1, "Updating", "");
                    let outcome = crate::updater::install(&platform, |pct, detail| job.progress(pct, "Updating", detail))?;
                    to_value(outcome)
                }) }))
            }
            "import_cards_file" => {
                let path: String = arg(a, "path")?;
                let engine = self.clone();
                Ok(json!({ "job": self.jobs.spawn("update_cards", "", move |job| {
                    job.progress(10, "Importing card file", path.clone());
                    let mut db = CardDb::open(DB_PATH)?;
                    let count = db.import_file(Path::new(&path))?;
                    *engine.db.lock().unwrap() = CardDb::open(DB_PATH)?;
                    Ok(json!({ "cards": count }))
                }) }))
            }
            "card" => self.card(&arg::<String>(a, "name")?),
            "search_cards" => self.search_cards(&arg::<String>(a, "query")?, opt(a, "limit").unwrap_or(20)),
            "scryfall_search" => self.scryfall_search(a),
            "commander_suggestions" => self.commander_suggestions(&arg::<String>(a, "query")?, &opt::<String>(a, "format").unwrap_or_default()),
            "deep_search" => self.deep_search_cards(a),
            "formats" => Ok(formats_json()),

            // --- deck library --------------------------------------------------
            "list_decks" => self.list_decks(),
            "deck" => self.deck(&arg::<String>(a, "name")?),
            "deck_text" => {
                let deck = self.load(&arg::<String>(a, "name")?)?;
                Ok(json!({ "text": deck_to_text(&deck) }))
            }
            "save_deck_text" => self.save_deck_text(&arg::<String>(a, "name")?, &arg::<String>(a, "text")?, opt(a, "overwrite").unwrap_or(false)),
            "import_url" => Ok(json!({ "job": self.start_import_url(arg(a, "url")?, opt(a, "name")) })),
            "delete_deck" => self.delete_deck(&arg::<String>(a, "name")?),
            "rename_deck" => self.rename_deck(&arg::<String>(a, "name")?, &arg::<String>(a, "new_name")?),
            "duplicate_deck" => self.duplicate_deck(&arg::<String>(a, "name")?, &arg::<String>(a, "new_name")?),
            "edit_deck" => self.edit_deck(&arg::<String>(a, "name")?, a),
            "apply_suggestion" => self.apply_suggestion(&arg::<String>(a, "name")?, opt(a, "cut"), opt(a, "add")),
            "printings" => to_value(printings(&self.canonical(&arg::<String>(a, "card")?))?),
            "set_printing" => self.set_printing(&arg::<String>(a, "name")?, a),

            // --- history ------------------------------------------------------
            "versions" => self.versions(&arg::<String>(a, "name")?),
            "version_text" => Ok(json!({ "text": versions::text(&arg::<String>(a, "name")?, arg(a, "id")?)? })),
            "restore_version" => self.restore_version(&arg::<String>(a, "name")?, arg(a, "id")?),

            // --- sharing --------------------------------------------------------
            "share" => self.share(&arg::<String>(a, "name")?, opt(a, "extras").unwrap_or(false)),
            "share_preview" => {
                let (name, text) = share::decode(&arg::<String>(a, "code")?)?;
                let deck = parse_deck_text(&name, &text)?;
                Ok(json!({
                    "name": name,
                    "commanders": deck.commanders.iter().map(|e| e.name.clone()).collect::<Vec<_>>(),
                    "cards": deck.commanders.iter().chain(deck.main.iter()).map(|e| e.quantity).sum::<u32>(),
                }))
            }
            "import_share" => self.import_share(&arg::<String>(a, "code")?, opt(a, "name")),

            // --- improvement --------------------------------------------------
            "guide" => self.guide(&arg::<String>(a, "name")?),
            "recommendations" => Ok(json!({ "job": self.start_recommendations(arg(a, "name")?) })),
            "upgrade" => Ok(json!({ "job": self.start_upgrade(arg(a, "name")?, opt(a, "bracket").unwrap_or(3))? })),
            "win_speed" => Ok(json!({ "job": self.start_win_speed(arg(a, "name")?) })),

            // --- game tracker -------------------------------------------------
            "games" => Ok(read_games()),
            "game_record" => {
                let mut games = read_games();
                let mut record = args.get("game").cloned().ok_or_else(|| anyhow!("Missing game"))?;
                record["id"] = json!(chrono::Utc::now().timestamp_millis());
                if let Some(list) = games.as_array_mut() {
                    list.insert(0, record);
                    list.truncate(1000);
                }
                fs::write(GAMES_PATH, serde_json::to_string(&games)?)?;
                Ok(json!({ "saved": true }))
            }
            "game_delete" => {
                let id: i64 = arg(a, "id")?;
                let mut games = read_games();
                if let Some(list) = games.as_array_mut() {
                    list.retain(|g| g.get("id").and_then(Value::as_i64) != Some(id));
                }
                fs::write(GAMES_PATH, serde_json::to_string(&games)?)?;
                Ok(games)
            }

            // --- analysis ---------------------------------------------------------
            "analyze" => self.analyze(&arg::<String>(a, "name")?),
            "simulate" => self.simulate(&arg::<String>(a, "name")?, opt(a, "runs").unwrap_or(50_000)),
            "combos" => Ok(json!({ "job": self.start_combos(arg(a, "name")?) })),
            "history" => {
                let path = self.deck_path(&arg::<String>(a, "name")?)?;
                Ok(json!({ "lines": render_history_lines(&path) }))
            }

            // --- sources ----------------------------------------------------------
            "sources" => self.sources(&arg::<String>(a, "name")?),
            "add_reference" => Ok(json!({ "job": self.start_add_reference(arg(a, "name")?, arg(a, "url")?) })),
            "remove_reference" => self.remove_reference(&arg::<String>(a, "name")?, &arg::<String>(a, "url")?),

            // --- generation -------------------------------------------------------
            "generation_themes" => self.generation_themes(&opt::<String>(a, "commander").unwrap_or_default(), &opt::<String>(a, "format").unwrap_or_default()),
            "generate" => Ok(json!({ "job": self.start_generate(
                opt(a, "commander").unwrap_or_default(),
                opt(a, "theme").unwrap_or_default(),
                opt(a, "bracket").unwrap_or(3),
                GenerationOptions {
                    format: Format::parse(&opt::<String>(a, "format").unwrap_or_default()),
                    deep_search: opt(a, "deep_search").unwrap_or(false),
                },
                opt(a, "colors").unwrap_or_default(),
            )? })),
            "set_format" => self.set_format(&arg::<String>(a, "name")?, &arg::<String>(a, "format")?),

            // --- exports ----------------------------------------------------------
            "export_moxfield" => self.export_moxfield(&arg::<String>(a, "name")?),
            "export_archidekt" => {
                let name: String = arg(a, "name")?;
                let deck = self.load(&name)?;
                let text = crate::tts::deck_to_archidekt_text(&deck, &self.db.lock().unwrap());
                let path = PathBuf::from(EXPORTS_DIR).join(format!("{}.archidekt.txt", safe_deck_filename(&name)));
                fs::write(&path, &text)?;
                Ok(json!({ "text": text, "path": self.root.join(&path).display().to_string() }))
            }
            "export_tts" => self.export_tts(&arg::<String>(a, "name")?),

            // --- MPC ----------------------------------------------------------------
            "mpc_config" => to_value(MpcConfig::load_or_create()),
            "mpc_config_set" => self.mpc_config_set(a),
            "mpc_order" => self.mpc_order(&arg::<String>(a, "name")?, opt(a, "rebuild").unwrap_or(false)),
            "mpc_search" => Ok(json!({ "job": self.start_mpc_search(arg(a, "name")?)? })),
            "mpc_select" => self.mpc_select(&arg::<String>(a, "name")?, arg(a, "index")?, opt(a, "back").unwrap_or(false), arg(a, "selected")?),
            "mpc_export" => Ok(json!({ "job": self.start_mpc_export(arg(a, "name")?) })),
            "mpc_more" => self.mpc_more(&arg::<String>(a, "name")?, arg(a, "index")?, opt(a, "back").unwrap_or(false)),

            // --- AI -----------------------------------------------------------------
            "ai_status" => self.ai_status(),
            "ai_config_set" => self.ai_config_set(a),
            "ai_start" => Ok(json!({ "job": self.start_ai_runtime() })),
            "ai_stop" => {
                if let Some(mut runtime) = self.ai_runtime.lock().unwrap().take() {
                    runtime.shutdown();
                }
                self.ai_status()
            }
            "agent_state" => self.agent_state(&arg::<String>(a, "name")?),
            "agent_run" => Ok(json!({ "job": self.start_agent(a)? })),
            "agent_chat" => Ok(json!({ "job": self.start_chat(arg(a, "name")?, arg(a, "message")?)? })),
            "agent_chat_clear" => self.chat_clear(&arg::<String>(a, "name")?),
            "agent_save" => self.agent_save(&arg::<String>(a, "name")?, opt(a, "iteration")),

            other => bail!("Unknown command '{other}'"),
        }
    }

    // -----------------------------------------------------------------------
    // App
    // -----------------------------------------------------------------------

    fn status(&self) -> Result<Value> {
        let cards = self.db.lock().unwrap().card_count().unwrap_or(0);
        let decks = load_deck_files(DECKS_DIR).map(|d| d.len()).unwrap_or(0);
        Ok(json!({
            "version": crate::VERSION,
            "build": crate::updater::current_build(),
            "format_data_ready": self.db.lock().unwrap().has_format_data(),
            "platform": self.platform,
            "data_dir": self.root.display().to_string(),
            "cards": cards,
            "card_data_ready": cards > 1000,
            "decks": decks,
            "ai_running": self.ai_runtime.lock().unwrap().is_some(),
        }))
    }

    fn settings_set(&self, args: &Value) -> Result<Value> {
        let mut settings = read_settings();
        if let (Some(target), Some(patch)) = (settings.as_object_mut(), args.get("settings").and_then(Value::as_object)) {
            for (k, v) in patch {
                target.insert(k.clone(), v.clone());
            }
        }
        fs::write(SETTINGS_PATH, serde_json::to_string_pretty(&settings)?)?;
        Ok(settings)
    }

    // -----------------------------------------------------------------------
    // Card data
    // -----------------------------------------------------------------------

    fn card_view(&self, db: &CardDb, card: Card) -> CardView {
        let printing = db.printing(&card.name).ok().flatten();
        CardView {
            roles: classify_card(&card),
            kind: card_kind(&card.type_line),
            set_code: printing.as_ref().map(|p| p.set_code.clone()),
            collector_number: printing.as_ref().map(|p| p.collector_number.clone()),
            double_faced: printing.map(|p| p.double_faced).unwrap_or(false),
            card,
        }
    }

    fn card(&self, name: &str) -> Result<Value> {
        let db = self.db.lock().unwrap();
        let card = db.get_card(name)?.ok_or_else(|| anyhow!("'{name}' is not in the card database."))?;
        to_value(self.card_view(&db, card))
    }

    fn search_cards(&self, query: &str, limit: usize) -> Result<Value> {
        let db = self.db.lock().unwrap();
        let cards = db.search_cards(query, limit.clamp(1, 60))?;
        to_value(cards.into_iter().map(|c| self.card_view(&db, c)).collect::<Vec<_>>())
    }

    /// Search Scryfall with its own syntax (what the Scryfall and Moxfield
    /// search boxes accept). Results carry the local card data when we have it.
    fn scryfall_search(&self, a: &Value) -> Result<Value> {
        let q: String = arg(a, "q")?;
        if q.trim().is_empty() {
            bail!("Type a search, for example t:creature o:\"draw a card\" mv<=3");
        }
        let order: String = opt(a, "order").unwrap_or_else(|| "name".into());
        let dir: String = opt(a, "dir").unwrap_or_else(|| "auto".into());
        let unique: String = opt(a, "unique").unwrap_or_else(|| "cards".into());
        let page: u32 = opt(a, "page").unwrap_or(1).max(1);
        let url = reqwest::Url::parse_with_params(
            "https://api.scryfall.com/cards/search",
            &[("q", q.as_str()), ("order", order.as_str()), ("dir", dir.as_str()), ("unique", unique.as_str()), ("page", &page.to_string())],
        )?;
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .user_agent(concat!("CommanderLab/", env!("CARGO_PKG_VERSION")))
            .build()?;
        let mut body: Value = Value::Null;
        for attempt in 0..3 {
            let response = client.get(url.clone()).header("Accept", "application/json").send().context("Couldn't reach Scryfall")?;
            let status = response.status().as_u16();
            if status == 429 {
                std::thread::sleep(std::time::Duration::from_millis(800 * (attempt + 1)));
                continue;
            }
            body = response.json().context("Scryfall sent an unreadable answer")?;
            if status == 404 {
                return Ok(json!({ "total": 0, "has_more": false, "cards": [], "warnings": [] }));
            }
            if status >= 400 {
                bail!("{}", body.get("details").and_then(Value::as_str).unwrap_or("Scryfall couldn't run that search."));
            }
            break;
        }
        let db = self.db.lock().unwrap();
        let face_image = |c: &Value| -> String {
            c.get("image_uris")
                .and_then(|u| u.get("normal"))
                .or_else(|| c.get("card_faces").and_then(|f| f.get(0)).and_then(|f| f.get("image_uris")).and_then(|u| u.get("normal")))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let cards: Vec<Value> = body
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|c| {
                let name = c.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
                let local = db.get_card(&name).ok().flatten().map(|card| self.card_view(&db, card));
                let s = |k: &str| c.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
                json!({
                    "name": name,
                    "image": face_image(c),
                    "mana_cost": if s("mana_cost").is_empty() {
                        c.get("card_faces").and_then(|f| f.get(0)).and_then(|f| f.get("mana_cost")).and_then(Value::as_str).unwrap_or_default().to_string()
                    } else { s("mana_cost") },
                    "type_line": s("type_line"),
                    "set": s("set").to_ascii_uppercase(),
                    "set_name": s("set_name"),
                    "collector_number": s("collector_number"),
                    "rarity": s("rarity"),
                    "price_usd": c.get("prices").and_then(|p| p.get("usd")).and_then(Value::as_str),
                    "card": local,
                })
            })
            .collect();
        Ok(json!({
            "total": body.get("total_cards").and_then(Value::as_u64).unwrap_or(cards.len() as u64),
            "has_more": body.get("has_more").and_then(Value::as_bool).unwrap_or(false),
            "warnings": body.get("warnings").cloned().unwrap_or(json!([])),
            "cards": cards,
        }))
    }

    fn commander_suggestions(&self, query: &str, format: &str) -> Result<Value> {
        let db = self.db.lock().unwrap();
        let cards = if Format::parse(format) == Format::Pdh {
            if !db.has_format_data() {
                bail!("Update card data (Settings → Update card data) to search Pauper Commander commanders.");
            }
            db.search_pdh_commanders(query, 12)?
        } else {
            commander_name_suggestions(query, &db, 12)?
        };
        to_value(cards.into_iter().map(|c| self.card_view(&db, c)).collect::<Vec<_>>())
    }

    fn start_update_cards(self: &Arc<Self>) -> u64 {
        if let Some(id) = self.jobs.running("update_cards", "") {
            return id;
        }
        let engine = self.clone();
        self.jobs.spawn("update_cards", "", move |job| {
            job.progress(5, "Downloading Scryfall card data", "This takes a minute or two.");
            let mut db = CardDb::open(DB_PATH)?;
            let count = db.update_from_scryfall()?;
            job.progress(95, "Reloading card database", format!("{count} cards"));
            *engine.db.lock().unwrap() = CardDb::open(DB_PATH)?;
            Ok(json!({ "cards": count }))
        })
    }

    // -----------------------------------------------------------------------
    // Deck library
    // -----------------------------------------------------------------------

    /// Check a deck against its format's construction rules.
    fn format_check(&self, db: &CardDb, deck: &ParsedDeck) -> crate::format::FormatCheck {
        let commanders: Vec<Card> = deck
            .commanders
            .iter()
            .filter_map(|e| db.get_card(&e.name).ok().flatten())
            .collect();
        let identity = crate::commander::combined_identity(&commanders);
        let main: Vec<(u32, Card)> = deck
            .main
            .iter()
            .filter_map(|e| db.get_card(&e.name).ok().flatten().map(|c| (e.quantity, c)))
            .collect();
        let mut check = check_deck(deck.format, &commanders, &main, &identity);
        if deck.format.needs_format_data() && !db.has_format_data() {
            check.problems.insert(0, "Update card data to check this format's rules exactly.".into());
        }
        check
    }

    fn set_format(&self, name: &str, format: &str) -> Result<Value> {
        let mut deck = self.load(name)?;
        let format = Format::parse(format);
        if deck.format == format {
            return self.deck(name);
        }
        if format == Format::Canlander {
            // No command zone: the commander joins the deck.
            let moved = std::mem::take(&mut deck.commanders);
            deck.main.splice(0..0, moved);
        }
        deck.format = format;
        self.save(name, &deck, &format!("Changed format to {}", format.label()))?;
        self.deck(name)
    }

    /// Search every card's rules text for cards that fit themes.
    fn deep_search_cards(&self, a: &Value) -> Result<Value> {
        let db = self.db.lock().unwrap();
        let themes: Vec<String> = match a.get("themes") {
            Some(Value::Array(list)) => list.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
            _ => crate::archetype::theme_list(&opt::<String>(a, "theme").unwrap_or_default()),
        };
        if themes.is_empty() {
            bail!("Name a play style or card ability to search for.");
        }
        let mut format = Format::parse(&opt::<String>(a, "format").unwrap_or_default());
        let mut identity: Option<String> = opt(a, "identity");
        let mut exclude = std::collections::HashSet::<String>::new();
        if let Some(name) = opt::<String>(a, "deck") {
            let deck = self.load(&name)?;
            format = deck.format;
            let commanders: Vec<Card> = deck.commanders.iter().filter_map(|e| db.get_card(&e.name).ok().flatten()).collect();
            if format.has_commander() && !commanders.is_empty() {
                identity = Some(crate::commander::combined_identity(&commanders));
            } else if identity.is_none() {
                let mut colors = String::new();
                for e in &deck.main {
                    if let Ok(Some(c)) = db.get_card(&e.name) {
                        for ch in c.color_identity.chars() {
                            if !colors.contains(ch) {
                                colors.push(ch);
                            }
                        }
                    }
                }
                identity = Some(colors);
            }
            exclude.extend(deck.commanders.iter().chain(deck.main.iter()).map(|e| e.name.to_ascii_lowercase()));
        }
        let limit: usize = opt(a, "limit").unwrap_or(40);
        let finds = deep_search(&db, &themes, identity.as_deref(), format, limit + exclude.len().min(200))?;
        let out: Vec<Value> = finds
            .into_iter()
            .filter(|f| !exclude.contains(&f.name.to_ascii_lowercase()))
            .take(limit)
            .filter_map(|f| {
                let card = db.get_card(&f.name).ok().flatten()?;
                Some(json!({ "find": f, "card": self.card_view(&db, card) }))
            })
            .collect();
        Ok(json!({ "themes": themes, "results": out }))
    }

    fn deck_path(&self, name: &str) -> Result<PathBuf> {
        load_deck_files(DECKS_DIR)?
            .into_iter()
            .find(|d| d.name == name)
            .map(|d| d.path)
            .ok_or_else(|| anyhow!("Deck '{name}' was not found."))
    }

    fn load(&self, name: &str) -> Result<ParsedDeck> {
        let mut deck = parse_deck_file(&self.deck_path(name)?)?;
        deck.name = name.to_string();
        Ok(deck)
    }

    /// Write a deck and record a version with `message`. If the deck has no
    /// history yet, its previous contents are kept as the starting version.
    fn commit(&self, name: &str, path: &Path, deck: &ParsedDeck, message: &str) -> Result<()> {
        if versions::list(name).is_empty() && path.exists() {
            if let Ok(previous) = parse_deck_file(path) {
                let _ = versions::record(name, &previous, "Starting version");
            }
        }
        write_deck_file(path, deck)?;
        let _ = versions::record(name, deck, message);
        self.mpc_orders.lock().unwrap().remove(name);
        Ok(())
    }

    fn save(&self, name: &str, deck: &ParsedDeck, message: &str) -> Result<()> {
        let path = self.deck_path(name)?;
        self.commit(name, &path, deck, message)
    }

    fn unique_name(&self, wanted: &str) -> Result<String> {
        let base = safe_deck_filename(wanted);
        let existing: Vec<String> = load_deck_files(DECKS_DIR)?.into_iter().map(|d| d.name).collect();
        if !existing.iter().any(|n| n.eq_ignore_ascii_case(&base)) {
            return Ok(base);
        }
        for i in 2.. {
            let candidate = format!("{base} ({i})");
            if !existing.iter().any(|n| n.eq_ignore_ascii_case(&candidate)) {
                return Ok(candidate);
            }
        }
        unreachable!()
    }

    fn list_decks(&self) -> Result<Value> {
        let db = self.db.lock().unwrap();
        let mut out = Vec::new();
        for file in load_deck_files(DECKS_DIR)? {
            let Ok(deck) = parse_deck_file(&file.path) else { continue };
            let commanders: Vec<Card> = deck
                .commanders
                .iter()
                .filter_map(|e| db.get_card(&e.name).ok().flatten())
                .collect();
            let mut identity = String::new();
            let mut price = 0.0;
            let mut cover: Option<String> = None;
            let mut main_colors = String::new();
            for entry in deck.commanders.iter().chain(deck.main.iter()) {
                if let Ok(Some(card)) = db.get_card(&entry.name) {
                    price += card.price_usd.unwrap_or(0.0) * entry.quantity as f64;
                    main_colors.push_str(&card.color_identity);
                    // Highlander decks have no commander: show the priciest
                    // nonland card as the cover.
                    if cover.is_none() && !card.type_line.contains("Land") && !card.image_uri.is_empty() {
                        cover = Some(card.image_uri.clone());
                    }
                }
            }
            for c in ['W', 'U', 'B', 'R', 'G'] {
                let has = if commanders.is_empty() {
                    main_colors.contains(c)
                } else {
                    commanders.iter().any(|card| card.color_identity.contains(c))
                };
                if has {
                    identity.push(c);
                }
            }
            let modified_ms = fs::metadata(&file.path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis())
                .unwrap_or(0);
            out.push(DeckSummary {
                name: file.name.clone(),
                commanders: deck.commanders.iter().map(|e| e.name.clone()).collect(),
                identity,
                cards: deck.commanders.iter().chain(deck.main.iter()).map(|e| e.quantity).sum(),
                image: commanders.first().map(|c| c.image_uri.clone()).or(cover).unwrap_or_default(),
                format: deck.format.id().to_string(),
                modified_ms,
                has_plan: file.path.with_extension("plan.json").exists(),
                price_usd: (price * 100.0).round() / 100.0,
            });
        }
        out.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
        to_value(out)
    }

    fn deck(&self, name: &str) -> Result<Value> {
        let deck = self.load(name)?;
        let path = self.deck_path(name)?;
        let db = self.db.lock().unwrap();

        let mut entries = Vec::new();
        for (zone, list) in [("commander", &deck.commanders), ("main", &deck.main), ("sideboard", &deck.sideboard), ("considering", &deck.considering)] {
            for entry in list {
                let card = db.get_card(&entry.name).ok().flatten().map(|c| self.card_view(&db, c));
                entries.push(EntryView {
                    quantity: entry.quantity,
                    name: card.as_ref().map(|c| c.card.name.clone()).unwrap_or_else(|| entry.name.clone()),
                    zone,
                    card,
                    set_code: entry.set_code.clone(),
                    collector_number: entry.collector_number.clone(),
                    foil: entry.foil,
                });
            }
        }

        let plan: Option<GamePlan> = fs::read_to_string(path.with_extension("plan.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok());

        let check = self.format_check(&db, &deck);

        Ok(json!({
            "name": name,
            "entries": entries,
            "plan": plan,
            "plan_markdown": fs::read_to_string(path.with_extension("plan.md")).ok(),
            "format": deck.format.id(),
            "format_label": deck.format.label(),
            "format_check": check,
        }))
    }

    fn save_deck_text(&self, name: &str, text: &str, overwrite: bool) -> Result<Value> {
        let name = name.trim();
        if name.is_empty() {
            bail!("Give the deck a name.");
        }
        let mut deck = parse_deck_text(name, text)?;
        if deck.main.is_empty() && deck.commanders.is_empty() {
            bail!("No cards were recognised in that list.");
        }
        let final_name = if overwrite && self.deck_path(name).is_ok() {
            safe_deck_filename(name)
        } else {
            self.unique_name(name)?
        };
        let existed = overwrite && final_name == safe_deck_filename(name);
        deck.name = final_name.clone();
        let path = PathBuf::from(DECKS_DIR).join(format!("{final_name}.txt"));
        self.commit(&final_name, &path, &deck, if existed { "Edited the decklist as text" } else { "Created from a pasted list" })?;
        Ok(json!({ "name": final_name }))
    }

    fn start_import_url(self: &Arc<Self>, url: String, name: Option<String>) -> u64 {
        let engine = self.clone();
        self.jobs.spawn("import_url", &url.clone(), move |job| {
            job.progress(20, "Importing deck", url.clone());
            let lower = url.to_ascii_lowercase();
            let deck = if lower.contains("moxfield.com") {
                import_moxfield(&url)?
            } else if lower.contains("archidekt.com") {
                import_archidekt(&url)?
            } else {
                bail!("Paste a Moxfield or Archidekt deck link.");
            };
            let wanted = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| deck.name.clone());
            let final_name = engine.unique_name(&wanted)?;
            let mut deck = deck;
            deck.name = final_name.clone();
            let source = if lower.contains("moxfield") { "Moxfield" } else { "Archidekt" };
            engine.commit(&final_name, &PathBuf::from(DECKS_DIR).join(format!("{final_name}.txt")), &deck, &format!("Imported from {source}"))?;
            Ok(json!({ "name": final_name }))
        })
    }

    fn sidecars(path: &Path) -> Vec<PathBuf> {
        vec![
            path.with_extension("plan.md"),
            path.with_extension("plan.json"),
            path.with_extension("combos.json"),
        ]
    }

    fn delete_deck(&self, name: &str) -> Result<Value> {
        let path = self.deck_path(name)?;
        fs::remove_file(&path)?;
        delete_sources_sidecar(&path);
        delete_history(&path);
        for extra in Self::sidecars(&path) {
            let _ = fs::remove_file(extra);
        }
        versions::delete(name);
        self.mpc_orders.lock().unwrap().remove(name);
        Ok(json!({ "deleted": name }))
    }

    fn copy_deck(&self, name: &str, new_name: &str, keep_original: bool) -> Result<Value> {
        let from = self.deck_path(name)?;
        let final_name = self.unique_name(new_name)?;
        let to = PathBuf::from(DECKS_DIR).join(format!("{final_name}.txt"));
        let mut deck = parse_deck_file(&from)?;
        deck.name = final_name.clone();
        write_deck_file(&to, &deck)?;
        for (a, b) in Self::sidecars(&from).into_iter().zip(Self::sidecars(&to)) {
            if a.exists() {
                let _ = fs::copy(&a, &b);
            }
        }
        if let Ok(Some(sources)) = load_sources_for_deck(&from) {
            let _ = save_sources_for_deck(&to, &sources);
        }
        versions::rename(name, &final_name, true);
        let _ = versions::record(
            &final_name,
            &deck,
            &if keep_original { format!("Duplicated from {name}") } else { format!("Renamed from {name}") },
        );
        if !keep_original {
            self.delete_deck(name)?;
        }
        Ok(json!({ "name": final_name }))
    }

    fn rename_deck(&self, name: &str, new_name: &str) -> Result<Value> {
        if new_name.trim().is_empty() {
            bail!("Give the deck a name.");
        }
        self.copy_deck(name, new_name, false)
    }

    fn duplicate_deck(&self, name: &str, new_name: &str) -> Result<Value> {
        self.copy_deck(name, new_name, true)
    }

    fn zone_mut<'a>(deck: &'a mut ParsedDeck, zone: &str) -> Result<&'a mut Vec<DeckEntry>> {
        Ok(match zone {
            "commander" => &mut deck.commanders,
            "main" => &mut deck.main,
            "sideboard" => &mut deck.sideboard,
            "considering" => &mut deck.considering,
            other => bail!("Unknown zone '{other}'"),
        })
    }

    fn canonical(&self, name: &str) -> String {
        self.db
            .lock()
            .unwrap()
            .get_card(name)
            .ok()
            .flatten()
            .map(|c| c.name)
            .unwrap_or_else(|| name.trim().to_string())
    }

    fn take(list: &mut Vec<DeckEntry>, card: &str, qty: Option<u32>) -> u32 {
        let Some(index) = list.iter().position(|e| e.name.eq_ignore_ascii_case(card)) else {
            return 0;
        };
        let take = qty.unwrap_or(list[index].quantity).min(list[index].quantity);
        list[index].quantity -= take;
        if list[index].quantity == 0 {
            list.remove(index);
        }
        take
    }

    fn put(list: &mut Vec<DeckEntry>, card: &str, qty: u32) {
        if qty == 0 {
            return;
        }
        if let Some(entry) = list.iter_mut().find(|e| e.name.eq_ignore_ascii_case(card)) {
            entry.quantity += qty;
        } else {
            list.push(DeckEntry { quantity: qty, name: card.to_string(), ..Default::default() });
        }
    }

    /// Moxfield-style editing: add, remove, set quantity, move between zones.
    fn edit_deck(&self, name: &str, args: &Value) -> Result<Value> {
        let mut deck = self.load(name)?;
        let action: String = arg(args, "action")?;
        let card_arg: String = arg(args, "card")?;
        let card = self.canonical(&card_arg);
        let zone: String = opt(args, "zone").unwrap_or_else(|| "main".to_string());
        let qty: Option<u32> = opt(args, "qty");

        let zone_label = |z: &str| match z {
            "commander" => "the command zone",
            "sideboard" => "the sideboard",
            "considering" => "Considering",
            _ => "the deck",
        };
        let message = match action.as_str() {
            "add" => format!("Added {} {card}", qty.unwrap_or(1)),
            "remove" => match qty {
                Some(n) => format!("Removed {n} {card}"),
                None => format!("Removed {card}"),
            },
            "set_qty" => format!("Set {card} to {}", qty.unwrap_or(0)),
            "move" => format!("Moved {card} to {}", zone_label(&opt::<String>(args, "to").unwrap_or_default())),
            _ => action.clone(),
        };

        match action.as_str() {
            "add" => Self::put(Self::zone_mut(&mut deck, &zone)?, &card, qty.unwrap_or(1)),
            "remove" => {
                let list = Self::zone_mut(&mut deck, &zone)?;
                if Self::take(list, &card, qty) == 0 {
                    Self::take(list, &card_arg, qty);
                }
            }
            "set_qty" => {
                let list = Self::zone_mut(&mut deck, &zone)?;
                Self::take(list, &card, None);
                Self::put(list, &card, qty.unwrap_or(0));
            }
            "move" => {
                let to: String = arg(args, "to")?;
                let moved = Self::take(Self::zone_mut(&mut deck, &zone)?, &card, qty);
                let moved = if moved == 0 {
                    Self::take(Self::zone_mut(&mut deck, &zone)?, &card_arg, qty)
                } else {
                    moved
                };
                if moved == 0 {
                    bail!("{card} is not in the {zone}.");
                }
                let to_list = Self::zone_mut(&mut deck, &to)?;
                Self::put(to_list, &card, if to == "commander" { 1 } else { moved });
            }
            other => bail!("Unknown edit action '{other}'"),
        }

        self.save(name, &deck, &message)?;
        self.deck(name)
    }

    fn apply_suggestion(&self, name: &str, cut: Option<String>, add: Option<String>) -> Result<Value> {
        let mut deck = self.load(name)?;
        let mut parts = Vec::new();
        if let Some(cut) = cut {
            let cut = self.canonical(&cut);
            let moved = Self::take(&mut deck.main, &cut, None);
            Self::put(&mut deck.sideboard, &cut, moved.max(1));
            parts.push(format!("cut {cut} to the sideboard"));
        }
        if let Some(add) = add {
            let add = self.canonical(&add);
            Self::take(&mut deck.sideboard, &add, None);
            Self::put(&mut deck.main, &add, 1);
            parts.push(format!("added {add}"));
        }
        let mut message = parts.join(", ");
        if let Some(first) = message.get(0..1) {
            message = first.to_uppercase() + &message[1..];
        }
        self.save(name, &deck, &message)?;
        self.deck(name)
    }

    // -----------------------------------------------------------------------
    // Analysis
    // -----------------------------------------------------------------------

    fn analyze(&self, name: &str) -> Result<Value> {
        let deck = self.load(name)?;
        let db = self.db.lock().unwrap();
        if !db.has_cards() {
            bail!("Download the card database first (Update card data).");
        }
        to_value(analyze_deck(&deck, &db)?)
    }

    fn simulate(&self, name: &str, runs: u32) -> Result<Value> {
        let deck = self.load(name)?;
        let db = self.db.lock().unwrap();
        to_value(run_draw_simulation(&deck, &db, runs.clamp(1_000, 200_000))?)
    }

    fn start_combos(self: &Arc<Self>, name: String) -> u64 {
        if let Some(id) = self.jobs.running("combos", &name) {
            return id;
        }
        let engine = self.clone();
        self.jobs.spawn("combos", &name.clone(), move |job| {
            job.progress(20, "Asking Commander Spellbook", name.clone());
            let deck = engine.load(&name)?;
            let report = find_my_combos(&deck)?;
            if let Ok(path) = engine.deck_path(&name) {
                let _ = fs::write(path.with_extension("combos.json"), serde_json::to_string(&report)?);
            }
            let db = CardDb::open(DB_PATH)?;
            let mut analysis = analyze_deck(&deck, &db)?;
            refine_with_combo_report(&mut analysis.bracket, &report);
            let cards = combo_card_images(&report, &deck, &db);
            Ok(json!({ "report": report, "bracket": analysis.bracket, "cards": cards }))
        })
    }

    // -----------------------------------------------------------------------
    // Sources
    // -----------------------------------------------------------------------

    fn current_sources(&self, name: &str) -> Result<(PathBuf, Vec<DeckSource>)> {
        let path = self.deck_path(name)?;
        if let Some(saved) = load_sources_for_deck(&path)? {
            return Ok((path, saved));
        }
        let deck = self.load(name)?;
        let db = self.db.lock().unwrap();
        let commanders: Vec<Card> = deck
            .commanders
            .iter()
            .filter_map(|e| db.get_card(&e.name).ok().flatten())
            .collect();
        let bracket = analyze_deck(&deck, &db).map(|a| a.bracket.bracket).unwrap_or(3);
        Ok((path, sources_for_existing_deck(&deck, &commanders, bracket)))
    }

    fn sources(&self, name: &str) -> Result<Value> {
        let (_, sources) = self.current_sources(name)?;
        let slim: Vec<Value> = sources
            .iter()
            .map(|s| {
                json!({
                    "source": s.source, "title": s.title, "url": s.url, "detail": s.detail,
                    "user_provided": s.user_provided, "is_deck": s.reference_deck.is_some(),
                })
            })
            .collect();
        Ok(Value::Array(slim))
    }

    fn start_add_reference(self: &Arc<Self>, name: String, url: String) -> u64 {
        let engine = self.clone();
        self.jobs.spawn("add_reference", &name.clone(), move |job| {
            job.progress(30, "Fetching reference", url.clone());
            let source = import_user_reference(&url)?;
            let (path, mut sources) = engine.current_sources(&name)?;
            sources.retain(|s| s.url != source.url);
            sources.insert(0, source);
            save_sources_for_deck(&path, &sources)?;
            engine.sources(&name)
        })
    }

    fn remove_reference(&self, name: &str, url: &str) -> Result<Value> {
        let (path, mut sources) = self.current_sources(name)?;
        sources.retain(|s| !(s.user_provided && s.url == url));
        save_sources_for_deck(&path, &sources)?;
        self.sources(name)
    }

    // -----------------------------------------------------------------------
    // Generation
    // -----------------------------------------------------------------------

    fn generation_themes(&self, commander: &str, format: &str) -> Result<Value> {
        let db = self.db.lock().unwrap();
        let format = Format::parse(format);
        if format == Format::Canlander {
            let themes: Vec<Value> = HIGHLANDER_THEMES
                .iter()
                .enumerate()
                .map(|(i, t)| json!({ "name": t, "score": 100 - i as i32, "reason": "Canadian Highlander play style" }))
                .collect();
            return Ok(json!({ "commanders": [], "themes": themes }));
        }
        let commanders = if format == Format::Pdh {
            resolve_pdh_commanders(commander, &db)?
        } else {
            resolve_generation_commanders(commander, &db)?
        };
        let mut themes = suggest_generation_themes(&commanders, &db)?;
        if format == Format::Pdh && themes.len() < 8 {
            // EDHREC has little Pauper Commander data; offer common PDH plans.
            for name in ["Tokens", "+1/+1 Counters", "-1/-1 Counters", "Sacrifice", "Lifegain", "Graveyard", "Spellslinger", "Artifacts", "Flying", "Blink", "Ninjutsu", "Landfall"] {
                if themes.len() >= 12 {
                    break;
                }
                if !themes.iter().any(|t| t.name.eq_ignore_ascii_case(name)) {
                    themes.push(crate::generator::GenerationTheme {
                        name: name.to_string(),
                        score: 10,
                        reason: "Common Pauper Commander strategy".to_string(),
                    });
                }
            }
        }
        Ok(json!({
            "commanders": commanders.into_iter().map(|c| self.card_view(&db, c)).collect::<Vec<_>>(),
            "themes": themes,
        }))
    }

    fn start_generate(
        self: &Arc<Self>,
        commander: String,
        theme: String,
        bracket: u8,
        options: GenerationOptions,
        colors: String,
    ) -> Result<u64> {
        if !self.db.lock().unwrap().has_cards() {
            bail!("Download the card database first (Update card data).");
        }
        if options.format != Format::Canlander && commander.trim().is_empty() {
            bail!("Choose a commander first.");
        }
        let engine = self.clone();
        let label = if options.format == Format::Canlander { format!("Highlander {colors}") } else { commander.clone() };
        Ok(self.jobs.spawn("generate", &label, move |job| {
            let db = CardDb::open(DB_PATH)?;
            let generated = if options.format == Format::Canlander {
                generate_highlander(&colors, &theme, bracket, options.deep_search, &db, |p| {
                    job.progress(p.percent, p.step, p.detail)
                })?
            } else {
                generate_deck_with_options(
                    &commander,
                    &theme,
                    GenerationBracket::new(bracket),
                    options,
                    &db,
                    |p| job.progress(p.percent, p.step, p.detail),
                )?
            };
            let power = if options.format.is_commander() { format!("Bracket {bracket}") } else { format!("{} power {bracket}", options.format.label()) };
            let deep = if options.deep_search { ", deep search" } else { "" };
            let message = if theme.trim().is_empty() {
                format!("Generated at {power}{deep}")
            } else {
                format!("Generated at {power} ({theme}{deep})")
            };
            let name = engine.save_generated(generated, &message)?;
            Ok(json!({ "name": name }))
        }))
    }

    fn save_generated(&self, mut generated: GeneratedDeck, message: &str) -> Result<String> {
        let name = self.unique_name(&generated.deck.name)?;
        generated.deck.name = name.clone();
        let path = PathBuf::from(DECKS_DIR).join(format!("{name}.txt"));
        self.commit(&name, &path, &generated.deck, message)?;
        fs::write(path.with_extension("plan.md"), game_plan_markdown(&generated.plan))?;
        fs::write(path.with_extension("plan.json"), serde_json::to_string_pretty(&generated.plan)?)?;
        save_sources_for_deck(&path, &generated.plan.sources)?;
        Ok(name)
    }

    // -----------------------------------------------------------------------
    // Moxfield export
    // -----------------------------------------------------------------------

    fn export_moxfield(&self, name: &str) -> Result<Value> {
        let deck = self.load(name)?;
        let db = self.db.lock().unwrap();
        let text = deck_to_moxfield_text(&deck, |entry| match db.printing(&entry.name) {
            Ok(Some(p)) => ExportPrinting {
                name: p.name,
                set_code: Some(p.set_code),
                collector_number: Some(p.collector_number),
            },
            _ => ExportPrinting {
                name: db.get_card(&entry.name).ok().flatten().map(|c| c.name).unwrap_or_else(|| entry.name.clone()),
                set_code: None,
                collector_number: None,
            },
        });
        let path = PathBuf::from(EXPORTS_DIR).join(format!("{}.moxfield.txt", safe_deck_filename(name)));
        fs::write(&path, &text)?;
        Ok(json!({
            "text": text,
            "path": self.root.join(&path).display().to_string(),
            "with_printings": text.lines().filter(|l| l.ends_with(|c: char| c.is_ascii_alphanumeric()) && l.contains(") ")).count(),
        }))
    }

    /// Tabletop Simulator saved object; also written straight into TTS'
    /// Saved Objects folder when TTS is installed on this computer.
    fn export_tts(&self, name: &str) -> Result<Value> {
        let deck = self.load(name)?;
        let value = crate::tts::deck_to_tts(&deck, &self.db.lock().unwrap());
        let text = serde_json::to_string_pretty(&value)?;
        let file = format!("{}.json", safe_deck_filename(name));
        let path = PathBuf::from(EXPORTS_DIR).join(format!("{}.tts.json", safe_deck_filename(name)));
        fs::write(&path, &text)?;
        let installed = if self.platform == "android" {
            None
        } else {
            crate::tts::saved_objects_dir().and_then(|dir| {
                let target = dir.join(&file);
                fs::write(&target, &text).ok().map(|_| target.display().to_string())
            })
        };
        let cards = deck.commanders.iter().chain(deck.main.iter()).map(|e| e.quantity).sum::<u32>();
        Ok(json!({
            "json": text,
            "file_name": file,
            "path": self.root.join(&path).display().to_string(),
            "installed_path": installed,
            "cards": cards,
        }))
    }

    // -----------------------------------------------------------------------
    // MPC Autofill
    // -----------------------------------------------------------------------

    fn mpc_config_set(&self, args: &Value) -> Result<Value> {
        let mut current = to_value(MpcConfig::load_or_create())?;
        if let (Some(target), Some(patch)) = (current.as_object_mut(), args.get("config").and_then(Value::as_object)) {
            for (k, v) in patch {
                target.insert(k.clone(), v.clone());
            }
        }
        let config: MpcConfig = serde_json::from_value(current)?;
        config.save()?;
        // Keep open orders in sync with stock/foil.
        for order in self.mpc_orders.lock().unwrap().values_mut() {
            order.stock = config.stock.clone();
            order.foil = config.foil;
            if !config.cardback_id.is_empty() {
                order.cardback_id = config.cardback_id.clone();
            }
        }
        to_value(config)
    }

    fn build_mpc_order(&self, name: &str) -> Result<MpcOrder> {
        let deck = self.load(name)?;
        let config = MpcConfig::load_or_create();
        let db = self.db.lock().unwrap();

        let mut entries = deck.commanders.clone();
        entries.extend(deck.main.iter().cloned());
        if config.include_sideboard {
            entries.extend(deck.sideboard.iter().cloned());
        }

        let lines: Vec<OrderLine> = entries
            .into_iter()
            .map(|entry| {
                let printing = db.printing(&entry.name).ok().flatten();
                let canonical = printing
                    .as_ref()
                    .map(|p| p.name.clone())
                    .or_else(|| db.get_card(&entry.name).ok().flatten().map(|c| c.name))
                    .unwrap_or_else(|| entry.name.clone());
                let faces = printing.filter(|p| p.double_faced).and_then(|_| {
                    let mut split = canonical.split(" // ");
                    match (split.next(), split.next()) {
                        (Some(f), Some(b)) => Some((f.trim().to_string(), b.trim().to_string())),
                        _ => None,
                    }
                });
                // Split, adventure and flip cards print on one face: search the
                // full name; the search also tries "fire ice" and the first half.
                let name = canonical;
                OrderLine { name, quantity: entry.quantity, faces }
            })
            .collect();

        Ok(MpcOrder::from_lines(name, &lines, &config))
    }

    fn mpc_order_view(order: &MpcOrder) -> Result<Value> {
        let (resolved, total) = order.resolved_faces();
        let mut value = to_value(order)?;
        value["total_slots"] = json!(order.total_slots());
        value["bracket"] = json!(order.bracket());
        value["resolved"] = json!(resolved);
        value["faces"] = json!(total);
        Ok(value)
    }

    fn mpc_order(&self, name: &str, rebuild: bool) -> Result<Value> {
        let mut orders = self.mpc_orders.lock().unwrap();
        if rebuild || !orders.contains_key(name) {
            drop(orders);
            let mut fresh = self.build_mpc_order(name)?;
            orders = self.mpc_orders.lock().unwrap();
            // Keep art already chosen for cards still in the list.
            if let Some(previous) = orders.get(name) {
                for card in &mut fresh.cards {
                    if let Some(old) = previous.cards.iter().find(|c| c.name == card.name) {
                        card.front = old.front.clone();
                        if card.back.is_some() {
                            card.back = old.back.clone();
                        }
                    }
                }
                if fresh.cardback_id.is_empty() {
                    fresh.cardback_id = previous.cardback_id.clone();
                }
            }
            orders.insert(name.to_string(), fresh);
        }
        Self::mpc_order_view(orders.get(name).unwrap())
    }

    fn start_mpc_search(self: &Arc<Self>, name: String) -> Result<u64> {
        self.mpc_order(&name, true)?;
        let queries = self.mpc_orders.lock().unwrap().get(&name).map(|o| o.queries()).unwrap_or_default();
        let config = MpcConfig::load_or_create();
        let engine = self.clone();
        // Card and face names for the official-art lookup.
        let faces: Vec<(String, String)> = {
            let orders = self.mpc_orders.lock().unwrap();
            let db = self.db.lock().unwrap();
            orders
                .get(&name)
                .map(|o| {
                    o.cards
                        .iter()
                        .flat_map(|c| {
                            let full = db.get_card(&c.name).ok().flatten().map(|card| card.name).unwrap_or_else(|| c.name.clone());
                            let mut v = vec![(full.clone(), c.front.query.clone())];
                            if let Some(b) = &c.back {
                                v.push((full, b.query.clone()));
                            }
                            v
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        Ok(self.jobs.spawn("mpc_search", &name.clone(), move |job| {
            job.progress(2, "Searching MPC Autofill", format!("{} faces at {}", queries.len(), config.backend_url));
            // Community scans. If the server can't be reached, carry on with
            // official art and say so.
            let mut warning = None;
            let outcome = match MpcClient::new(&config).and_then(|client| {
                client.search(&queries, &config, |done, total| {
                    let pct = 5 + (done * 55 / total.max(1)) as u16;
                    job.progress(pct, "Searching MPC Autofill", format!("{done}/{total}"));
                })
            }) {
                Ok(outcome) => outcome,
                Err(err) => {
                    warning = Some(format!("MPC Autofill search failed ({err:#}); showing official art only."));
                    crate::mpc::SearchOutcome::default()
                }
            };

            // Official scans: every distinct artwork from Scryfall.
            let mut official: HashMap<String, Vec<crate::mpc::MpcImage>> = HashMap::new();
            if config.include_official {
                let total = faces.len();
                for (i, (card, face)) in faces.iter().enumerate() {
                    let key = crate::mpc::search_query(face);
                    if !official.contains_key(&key) {
                        official.insert(key, crate::mpc::official_images(card, face));
                    }
                    job.progress(60 + (i * 38 / total.max(1)) as u16, "Finding official art on Scryfall", format!("{}/{total}", i + 1));
                }
            }

            let mut orders = engine.mpc_orders.lock().unwrap();
            let order = orders.get_mut(&name).ok_or_else(|| anyhow!("Order was closed"))?;
            order.apply_results(&outcome.results, &outcome.pending, &official, config.prefer_official);
            if order.cardback_id.is_empty() {
                if let Some(cb) = outcome.default_cardback {
                    order.cardback_id = cb;
                }
            }
            let not_found: Vec<String> = order
                .cards
                .iter()
                .flat_map(|c| std::iter::once(&c.front).chain(c.back.iter()))
                .filter(|f| f.candidates.is_empty() && f.pending_ids.is_empty())
                .map(|f| f.query.clone())
                .collect();
            let mut view = Self::mpc_order_view(order)?;
            view["not_found"] = json!(not_found);
            view["warning"] = json!(warning);
            Ok(view)
        }))
    }

    /// Load the rest of a face's MPC Autofill results.
    fn mpc_more(&self, name: &str, index: usize, back: bool) -> Result<Value> {
        let pending = {
            let orders = self.mpc_orders.lock().unwrap();
            let order = orders.get(name).ok_or_else(|| anyhow!("Open the MPC order first."))?;
            let card = order.cards.get(index).ok_or_else(|| anyhow!("No card at {index}"))?;
            let face = if back { card.back.as_ref().unwrap_or(&card.front) } else { &card.front };
            face.pending_ids.clone()
        };
        let config = MpcConfig::load_or_create();
        let metadata = if pending.is_empty() { HashMap::new() } else { MpcClient::new(&config)?.images(&pending)? };
        let mut orders = self.mpc_orders.lock().unwrap();
        let order = orders.get_mut(name).ok_or_else(|| anyhow!("Open the MPC order first."))?;
        let card = order.cards.get_mut(index).ok_or_else(|| anyhow!("No card at {index}"))?;
        let face = match (back, card.back.as_mut()) {
            (true, Some(b)) => b,
            _ => &mut card.front,
        };
        let mut more: Vec<crate::mpc::MpcImage> = pending.iter().filter_map(|id| metadata.get(id).cloned()).collect();
        crate::mpc::rank_images(&mut more, &config.preferred_sources);
        // Keep official scans where they were; add community images after
        // the community images already shown.
        let insert_at = face.candidates.iter().rposition(|c| !c.official).map(|i| i + 1).unwrap_or(if config.prefer_official { face.candidates.len() } else { 0 });
        let added = more.len();
        let had_any = !face.candidates.is_empty();
        for (k, img) in more.into_iter().enumerate() {
            face.candidates.insert(insert_at + k, img);
        }
        // Keep the same image selected when images were inserted before it.
        if had_any && insert_at <= face.selected {
            face.selected += added;
        }
        face.pending_ids.clear();
        Self::mpc_order_view(order)
    }

    fn mpc_select(&self, name: &str, index: usize, back: bool, selected: usize) -> Result<Value> {
        let mut orders = self.mpc_orders.lock().unwrap();
        let order = orders.get_mut(name).ok_or_else(|| anyhow!("Open the MPC order first."))?;
        let card = order.cards.get_mut(index).ok_or_else(|| anyhow!("No card at {index}"))?;
        let face = match (back, card.back.as_mut()) {
            (true, Some(b)) => b,
            _ => &mut card.front,
        };
        if selected < face.candidates.len() {
            face.selected = selected;
        }
        Self::mpc_order_view(order)
    }

    fn start_mpc_export(self: &Arc<Self>, name: String) -> u64 {
        let engine = self.clone();
        self.jobs.spawn("mpc_export", &name.clone(), move |job| {
            job.progress(5, "Preparing the order", "");
            engine.mpc_export(&name, |done, total| {
                job.progress(5 + (done * 90 / total.max(1)) as u16, "Downloading official scans", format!("{done}/{total}"));
            })
        })
    }

    fn mpc_export(&self, name: &str, progress: impl FnMut(usize, usize)) -> Result<Value> {
        let order = {
            let orders = self.mpc_orders.lock().unwrap();
            orders.get(name).cloned()
        };
        let mut order = match order {
            Some(o) => o,
            None => self.build_mpc_order(name)?,
        };
        // Official scans aren't on Google Drive: save them next to the order
        // and reference the files (MPC Autofill desktop tool v4+).
        let images_dir = PathBuf::from(EXPORTS_DIR).join(format!("{} images", safe_deck_filename(name)));
        let (official, failed) = crate::mpc::localize_official(&mut order, &images_dir, progress)?;
        let base = PathBuf::from(EXPORTS_DIR).join(safe_deck_filename(name));
        let text_path = base.with_extension("mpc.txt");
        let text = order.to_text();
        fs::write(&text_path, &text)?;
        let (xml, xml_path, error) = match order.to_xml() {
            Ok(xml) => {
                let path = base.with_extension("mpc.xml");
                fs::write(&path, &xml)?;
                (Some(xml), Some(self.root.join(path).display().to_string()), None)
            }
            Err(err) => (None, None, Some(err.to_string())),
        };
        Ok(json!({
            "text": text,
            "text_path": self.root.join(text_path).display().to_string(),
            "xml": xml,
            "xml_path": xml_path,
            "error": error,
            "file_stem": safe_deck_filename(name),
            "official_images": official,
            "download_failed": failed,
            "images_dir": if official > 0 { Some(self.root.join(&images_dir).display().to_string()) } else { None },
        }))
    }

    // -----------------------------------------------------------------------
    // AI
    // -----------------------------------------------------------------------

    fn ai_status(&self) -> Result<Value> {
        let config = AiConfig::load_or_create();
        let presets = |p| {
            model_presets(p)
                .iter()
                .map(|m| json!({ "name": m.name, "model": m.model, "description": m.description }))
                .collect::<Vec<_>>()
        };
        let runtime = self.ai_runtime.lock().unwrap();
        Ok(json!({
            "available": self.platform != "android",
            "running": runtime.is_some(),
            "status": runtime.as_ref().map(|r| r.status_line()),
            "config": {
                "provider": config.provider,
                "model": config.model,
                "base_url": config.base_url,
                "api_base_url": config.api_base_url,
                "api_model": config.api_model,
                "has_api_key": !config.api_key.is_empty(),
                "web_research": config.web_research,
            },
            "presets": {
                "llama_cpp": presets(AiProvider::LlamaCpp),
                "ollama": presets(AiProvider::Ollama),
            },
        }))
    }

    fn ai_config_set(&self, args: &Value) -> Result<Value> {
        let mut config = AiConfig::load_or_create();
        if let Some(provider) = opt::<AiProvider>(args, "provider") {
            config.provider = provider;
        }
        if let Some(model) = opt::<String>(args, "model") {
            config.model = model;
        }
        if let Some(url) = opt::<String>(args, "api_base_url") {
            config.api_base_url = url;
        }
        if let Some(model) = opt::<String>(args, "api_model") {
            config.api_model = model;
        }
        if let Some(key) = opt::<String>(args, "api_key") {
            config.api_key = key;
        }
        if let Some(web) = opt::<bool>(args, "web_research") {
            config.web_research = web;
        }
        config.save()?;
        self.ai_status()
    }

    fn start_ai_runtime(self: &Arc<Self>) -> u64 {
        if let Some(id) = self.jobs.running("ai_start", "") {
            return id;
        }
        let engine = self.clone();
        self.jobs.spawn("ai_start", "", move |job| {
            if engine.platform == "android" {
                bail!("The local AI model isn't available on Android.");
            }
            job.progress(10, "Starting AI backend", "First start may download the model.");
            let runtime = LocalAiRuntime::ensure_running()?;
            let line = runtime.status_line();
            *engine.ai_runtime.lock().unwrap() = Some(runtime);
            Ok(json!({ "status": line }))
        })
    }

    fn agent_state(&self, name: &str) -> Result<Value> {
        let path = self.deck_path(name)?;
        let history = load_history(&path);
        Ok(json!({
            "chat": history.agent_chat,
            "brief": history.agent_last_brief,
            "agent": history.latest_agent,
        }))
    }

    fn start_agent(self: &Arc<Self>, args: &Value) -> Result<u64> {
        let name: String = arg(args, "name")?;
        let bracket: u8 = opt(args, "bracket").unwrap_or(3);
        let goal = match opt::<String>(args, "goal").as_deref() {
            Some("stax") | Some("stax_lock") => OptimizationGoal::StaxLock,
            _ => OptimizationGoal::FastWin,
        };
        let intensity = match opt::<String>(args, "intensity").as_deref() {
            Some("quick") => TestingIntensity::Quick,
            Some("thorough") => TestingIntensity::Thorough,
            Some("exhaustive") => TestingIntensity::Exhaustive,
            _ => TestingIntensity::Standard,
        };
        let rounds: u16 = match intensity {
            TestingIntensity::Quick => 1,
            TestingIntensity::Standard => 3,
            TestingIntensity::Thorough => 5,
            TestingIntensity::Exhaustive => 8,
        };
        let brief: String = opt(args, "brief").unwrap_or_default();
        let archetype: Option<String> = opt(args, "archetype");

        if let Some(id) = self.jobs.running("agent", &name) {
            return Ok(id);
        }
        let engine = self.clone();
        Ok(self.jobs.spawn("agent", &name.clone(), move |job| {
            let deck = engine.load(&name)?;
            let path = engine.deck_path(&name)?;
            let db = CardDb::open(DB_PATH)?;
            let (_, sources) = engine.current_sources(&name)?;
            let user_refs: Vec<DeckSource> = sources.into_iter().filter(|s| s.user_provided).collect();

            // Fold recent chat direction into the brief, like the old Chat screen.
            let history = load_history(&path);
            let chat_direction: Vec<String> = history
                .agent_chat
                .iter()
                .rev()
                .filter(|m| m.role == "user")
                .take(4)
                .map(|m| m.content.clone())
                .collect();
            let mut full_brief = brief.clone();
            if !chat_direction.is_empty() {
                full_brief.push_str("\n\nRecent direction from chat:\n");
                for line in chat_direction.iter().rev() {
                    full_brief.push_str(&format!("- {line}\n"));
                }
            }

            let result = run_optimization_agent(
                &deck,
                &db,
                bracket,
                goal,
                archetype,
                Some(full_brief.clone()).filter(|b| !b.trim().is_empty()),
                user_refs,
                intensity,
                None,
                rounds,
                |p| {
                    job.progress(p.percent as u16, p.task.clone(), p.detail.clone());
                    if let Some(done) = &p.completed {
                        job.log(format!("✓ {done}"));
                    }
                },
            )?;

            let _ = save_agent_session(
                &path,
                &history.agent_chat,
                &brief,
                Some(&result.research),
                Some(&result),
                history.latest_meta_matchup.as_ref(),
            );
            to_value(&result)
        }))
    }

    fn start_chat(self: &Arc<Self>, name: String, message: String) -> Result<u64> {
        if message.trim().is_empty() {
            bail!("Type a message first.");
        }
        let engine = self.clone();
        Ok(self.jobs.spawn("agent_chat", &name.clone(), move |job| {
            job.progress(20, "Agent is thinking", "");
            let deck = engine.load(&name)?;
            let path = engine.deck_path(&name)?;
            let db = CardDb::open(DB_PATH)?;
            let history = load_history(&path);
            let (_, sources) = engine.current_sources(&name)?;
            let user_refs: Vec<DeckSource> = sources.into_iter().filter(|s| s.user_provided).collect();
            let agent = history.latest_agent.clone();
            let reply = chat_with_deck_agent(
                &deck,
                &db,
                history.latest_research.as_ref().or(agent.as_ref().map(|a| &a.research)),
                agent.as_ref().map(|a| &a.ai_profile),
                Some(history.agent_last_brief.as_str()).filter(|b| !b.is_empty()),
                &user_refs,
                &history.agent_chat,
                "",
                &message,
            )?;
            let mut chat = history.agent_chat.clone();
            chat.push(AgentChatMessage { role: "user".into(), content: message.clone() });
            chat.push(AgentChatMessage { role: "assistant".into(), content: reply });
            save_agent_session(
                &path,
                &chat,
                &history.agent_last_brief,
                history.latest_research.as_ref(),
                history.latest_agent.as_ref(),
                history.latest_meta_matchup.as_ref(),
            )?;
            Ok(json!({ "chat": chat }))
        }))
    }

    fn chat_clear(&self, name: &str) -> Result<Value> {
        let path = self.deck_path(name)?;
        let history = load_history(&path);
        save_agent_session(
            &path,
            &[],
            &history.agent_last_brief,
            history.latest_research.as_ref(),
            history.latest_agent.as_ref(),
            history.latest_meta_matchup.as_ref(),
        )?;
        Ok(json!({ "chat": [] }))
    }

    fn agent_save(&self, name: &str, iteration: Option<usize>) -> Result<Value> {
        let path = self.deck_path(name)?;
        let agent = load_history(&path)
            .latest_agent
            .ok_or_else(|| anyhow!("Run the agent first."))?;
        let generated = match iteration {
            Some(i) => agent
                .top_iterations
                .get(i)
                .map(|it| it.deck.clone())
                .ok_or_else(|| anyhow!("No iteration {i}"))?,
            None => agent.best.clone(),
        };
        let mut generated = generated;
        generated.deck.name = format!("{name} - Agent B{}", agent.target_bracket);
        let saved = self.save_generated(generated, "Saved from the AI Agent")?;
        Ok(json!({ "name": saved }))
    }
}

impl Engine {
    // -----------------------------------------------------------------------
    // Printings
    // -----------------------------------------------------------------------

    fn set_printing(&self, name: &str, args: &Value) -> Result<Value> {
        let mut deck = self.load(name)?;
        let zone: String = opt(args, "zone").unwrap_or_else(|| "main".into());
        let card: String = arg(args, "card")?;
        let set_code: Option<String> = opt::<String>(args, "set_code").filter(|s| !s.is_empty());
        let number: Option<String> = opt::<String>(args, "collector_number").filter(|s| !s.is_empty());
        let foil: Option<bool> = opt(args, "foil");
        let list = Self::zone_mut(&mut deck, &zone)?;
        let entry = list
            .iter_mut()
            .find(|e| e.name.eq_ignore_ascii_case(&card))
            .ok_or_else(|| anyhow!("{card} is not in that part of the deck."))?;
        if args.get("set_code").is_some() {
            entry.set_code = set_code.map(|s| s.to_ascii_uppercase());
            entry.collector_number = number;
        }
        if let Some(f) = foil {
            entry.foil = f;
        }
        let message = match (&entry.set_code, &entry.collector_number) {
            (Some(set), Some(num)) => format!(
                "Changed {} to {set} #{num}{}",
                entry.name,
                if entry.foil { " (foil)" } else { "" }
            ),
            _ => format!("Reset {} to the default printing", entry.name),
        };
        self.save(name, &deck, &message)?;
        self.deck(name)
    }

    // -----------------------------------------------------------------------
    // Version history
    // -----------------------------------------------------------------------

    fn versions(&self, name: &str) -> Result<Value> {
        let path = self.deck_path(name)?;
        let mut list = versions::list(name);
        if list.is_empty() {
            if let Ok(deck) = parse_deck_file(&path) {
                versions::record(name, &deck, "Starting version")?;
                list = versions::list(name);
            }
        }
        list.reverse();
        to_value(list)
    }

    fn restore_version(&self, name: &str, id: u64) -> Result<Value> {
        let mut deck = versions::load(name, id)?;
        deck.name = name.to_string();
        let when = chrono::DateTime::from_timestamp_millis(id as i64)
            .map(|t| t.with_timezone(&chrono::Local).format("%b %-d, %H:%M").to_string())
            .unwrap_or_else(|| id.to_string());
        self.save(name, &deck, &format!("Restored the version from {when}"))?;
        self.deck(name)
    }

    // -----------------------------------------------------------------------
    // Sharing
    // -----------------------------------------------------------------------

    fn share(&self, name: &str, extras: bool) -> Result<Value> {
        let deck = self.load(name)?;
        let codes = share::encode_deck(name, &deck, extras)?;
        let (svg, error) = match share::qr_svg(&codes.qr) {
            Ok(svg) => (Some(svg), None),
            Err(err) => (None, Some(err.to_string())),
        };
        let has_extras = !deck.sideboard.is_empty()
            || deck.commanders.iter().chain(deck.main.iter()).any(|e| e.set_code.is_some() || e.foil);
        Ok(json!({
            "code": codes.text,
            "svg": svg,
            "error": error,
            "extras": extras,
            "has_extras": has_extras,
            "qr_version": share::qr_version(&codes.qr),
        }))
    }

    fn import_share(&self, code: &str, name: Option<String>) -> Result<Value> {
        let (shared_name, text) = share::decode(code)?;
        let wanted = name.filter(|n| !n.trim().is_empty()).unwrap_or(shared_name);
        let final_name = self.unique_name(&wanted)?;
        let mut deck = parse_deck_text(&final_name, &text)?;
        deck.name = final_name.clone();
        let path = PathBuf::from(DECKS_DIR).join(format!("{final_name}.txt"));
        self.commit(&final_name, &path, &deck, "Imported from a share code")?;
        Ok(json!({ "name": final_name }))
    }

    // -----------------------------------------------------------------------
    // Play guide and improvements
    // -----------------------------------------------------------------------

    fn cached_combos(&self, name: &str) -> Option<ComboReport> {
        let path = self.deck_path(name).ok()?;
        fs::read_to_string(path.with_extension("combos.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
    }

    fn cached_plan(&self, name: &str) -> Option<GamePlan> {
        let path = self.deck_path(name).ok()?;
        fs::read_to_string(path.with_extension("plan.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
    }

    fn guide(&self, name: &str) -> Result<Value> {
        let deck = self.load(name)?;
        let combos = self.cached_combos(name);
        let plan = self.cached_plan(name);
        let db = self.db.lock().unwrap();
        if !db.has_cards() {
            bail!("Download the card database first (Update card data).");
        }
        let analysis = analyze_deck(&deck, &db)?;
        let guide = build_guide(&deck, &db, &analysis, combos.as_ref(), plan.as_ref())?;
        let mut value = to_value(guide)?;
        value["has_combos"] = json!(combos.is_some());
        Ok(value)
    }

    fn start_recommendations(self: &Arc<Self>, name: String) -> u64 {
        if let Some(id) = self.jobs.running("recommendations", &name) {
            return id;
        }
        let engine = self.clone();
        self.jobs.spawn("recommendations", &name.clone(), move |job| {
            let deck = engine.load(&name)?;
            let db = CardDb::open(DB_PATH)?;
            let commanders: Vec<Card> = deck.commanders.iter().filter_map(|e| db.get_card(&e.name).ok().flatten()).collect();
            let mut identity = String::new();
            let main_cards: Vec<Card> = deck.main.iter().filter_map(|e| db.get_card(&e.name).ok().flatten()).collect();
            for c in ['W', 'U', 'B', 'R', 'G'] {
                let has = if commanders.is_empty() {
                    main_cards.iter().any(|card| card.color_identity.contains(c))
                } else {
                    commanders.iter().any(|card| card.color_identity.contains(c))
                };
                if has {
                    identity.push(c);
                }
            }
            let format = deck.format;
            let in_deck: std::collections::HashSet<String> = deck
                .commanders
                .iter()
                .chain(deck.main.iter())
                .filter_map(|e| db.get_card(&e.name).ok().flatten().map(|c| c.name.to_ascii_lowercase()))
                .collect();
            let fits = |card: &Card| {
                format.card_allowed(card).is_ok()
                    && !card.is_basic
                    && !in_deck.contains(&card.name.to_ascii_lowercase())
                    && (identity.is_empty() && commanders.is_empty() || identity_subset(&card.color_identity, &identity))
            };

            let mut sections = Vec::new();

            // Cards that complete a combo you're one card away from.
            if let Some(report) = engine.cached_combos(&name) {
                let mut items = Vec::new();
                for line in report.variants.iter().filter(|v| v.almost) {
                    for piece in &line.cards {
                        if let Ok(Some(card)) = db.get_card(piece) {
                            if fits(&card) && !items.iter().any(|i: &Value| i["card"]["name"] == card.name) {
                                items.push(json!({
                                    "card": engine.card_view(&db, card),
                                    "reason": format!("Completes {}: {}", line.cards.join(" + "), line.result.lines().next().unwrap_or("")),
                                }));
                            }
                        }
                    }
                }
                if !items.is_empty() {
                    items.truncate(24);
                    sections.push(json!({ "id": "combos", "title": "Completes a combo", "items": items }));
                }
            }

            job.progress(20, if format == Format::Pdh { "Reading PDHREC for your commander" } else { "Reading EDHREC for your commander" }, "");
            let theme = engine.cached_plan(&name).and_then(|p| p.primary_tags.first().cloned()).unwrap_or_default();
            let signals = if commanders.is_empty() {
                Vec::new()
            } else if format == Format::Pdh {
                // Pauper Commander: PDHREC instead of EDHREC.
                let names: Vec<String> = commanders.iter().map(|c| c.name.clone()).collect();
                crate::format_sources::pdhrec_commander(&names).map(|p| p.signals()).unwrap_or_default()
            } else {
                commander_build_cards(&commanders, &theme, 300)
            };
            job.progress(70, "Ranking suggestions", format!("{} candidates", signals.len()));

            let mut synergy = Vec::new();
            let mut popular = Vec::new();
            for signal in &signals {
                let Ok(Some(card)) = db.get_card(&signal.name) else { continue };
                if !fits(&card) {
                    continue;
                }
                let pct = (signal.inclusion_pct() * 100.0).round();
                let item = json!({
                    "card": engine.card_view(&db, card),
                    "reason": if signal.synergy > 0.0 {
                        format!("In {pct}% of decks · +{:.0}% synergy", signal.synergy * 100.0)
                    } else {
                        format!("In {pct}% of this commander's decks")
                    },
                    "inclusion": pct,
                    "synergy": signal.synergy,
                });
                if signal.synergy >= 0.15 {
                    synergy.push(item);
                } else {
                    popular.push(item);
                }
            }
            synergy.truncate(36);
            popular.truncate(48);
            if !synergy.is_empty() {
                sections.push(json!({ "id": "synergy", "title": "High synergy with your commander", "items": synergy }));
            }
            if !popular.is_empty() {
                sections.push(json!({ "id": "popular", "title": "Popular in decks like yours", "items": popular }));
            }

            // Deep card search: cards whose rules text fits the deck's play
            // styles, including ones no decklist runs.
            job.progress(85, "Reading rules text for your play styles", "");
            let mut themes: Vec<String> = engine.cached_plan(&name).map(|p| p.primary_tags).unwrap_or_default();
            if themes.is_empty() {
                themes = crate::archetype::detect_deck_tags(&deck, &db)
                    .unwrap_or_default()
                    .into_iter()
                    .take(2)
                    .map(|t| t.name)
                    .collect();
            }
            themes.truncate(3);
            if !themes.is_empty() {
                let finds = deep_search(&db, &themes, Some(&identity), format, 80).unwrap_or_default();
                let items: Vec<Value> = finds
                    .into_iter()
                    .filter_map(|f| db.get_card(&f.name).ok().flatten().map(|c| (f, c)))
                    .filter(|(_, c)| fits(c))
                    .take(30)
                    .map(|(f, c)| json!({ "card": engine.card_view(&db, c), "reason": f.reason }))
                    .collect();
                if !items.is_empty() {
                    sections.push(json!({ "id": "deep", "title": "Found by rules text", "items": items }));
                }
            }

            // Role gaps from the deck analysis.
            if let Ok(analysis) = analyze_deck(&deck, &db) {
                let items: Vec<Value> = analysis
                    .add_suggestions
                    .iter()
                    .filter_map(|s| db.get_card(&s.card).ok().flatten().map(|c| (s, c)))
                    .filter(|(_, c)| fits(c))
                    .take(30)
                    .map(|(s, c)| json!({ "card": engine.card_view(&db, c), "reason": format!("Fills a gap: {}", s.role) }))
                    .collect();
                if !items.is_empty() {
                    sections.push(json!({ "id": "gaps", "title": "Fills a gap in your deck", "items": items }));
                }
            }

            Ok(json!({ "sections": sections, "theme": theme }))
        })
    }

    fn start_upgrade(self: &Arc<Self>, name: String, bracket: u8) -> Result<u64> {
        let engine = self.clone();
        Ok(self.jobs.spawn("upgrade", &name.clone(), move |job| {
            let deck = engine.load(&name)?;
            let db = CardDb::open(DB_PATH)?;
            let theme = engine.cached_plan(&name).and_then(|p| p.primary_tags.first().cloned());
            let mut generated = generate_ai_upgrade(
                &deck,
                GenerationBracket::new(bracket),
                theme.as_deref(),
                &[],
                &db,
                |p| job.progress(p.percent, p.step, p.detail),
            )?;
            generated.deck.name = format!("{name} - Bracket {bracket}");
            let saved = engine.save_generated(generated, &format!("Upgraded from {name} to Bracket {bracket}"))?;
            Ok(json!({ "name": saved }))
        }))
    }

    fn start_win_speed(self: &Arc<Self>, name: String) -> u64 {
        let engine = self.clone();
        self.jobs.spawn("win_speed", &name.clone(), move |job| {
            let deck = engine.load(&name)?;
            let db = CardDb::open(DB_PATH)?;
            let summary = simulate_deep_learn_games(&deck, &db, TestingIntensity::Standard, |line| {
                job.progress(50, "Goldfishing games", line)
            })?;
            to_value(summary)
        })
    }
}

/// Finished games from the Game tab, newest first.
/// Image and deck status for every card named in a combo report, keyed by
/// the name Commander Spellbook used. Cards in the deck show the printing
/// chosen for them.
fn combo_card_images(report: &ComboReport, deck: &ParsedDeck, db: &CardDb) -> Value {
    let mut out = Map::new();
    for line in &report.variants {
        for name in &line.cards {
            if out.contains_key(name) {
                continue;
            }
            let card = db.get_card(name).ok().flatten();
            let canonical = card.as_ref().map(|c| c.name.to_ascii_lowercase()).unwrap_or_else(|| name.to_ascii_lowercase());
            let entry = deck
                .commanders
                .iter()
                .chain(deck.main.iter())
                .find(|e| e.name.eq_ignore_ascii_case(&canonical) || e.name.eq_ignore_ascii_case(name));
            let image = match entry.and_then(|e| e.set_code.as_ref().zip(e.collector_number.as_ref())) {
                Some((set, num)) => format!(
                    "https://api.scryfall.com/cards/{}/{}?format=image&version=normal",
                    set.to_ascii_lowercase(),
                    urlencoding::encode(num)
                ),
                None => card.as_ref().map(|c| c.image_uri.clone()).unwrap_or_default(),
            };
            out.insert(
                name.clone(),
                json!({
                    "name": card.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| name.clone()),
                    "image": image,
                    "in_deck": entry.is_some(),
                    "type_line": card.as_ref().map(|c| c.type_line.clone()).unwrap_or_default(),
                }),
            );
        }
    }
    Value::Object(out)
}

/// Formats the app builds, with their rules, for the Build screen.
fn formats_json() -> Value {
    let points: Vec<Value> = CANLANDER_POINTS.iter().map(|(n, p)| json!({ "name": n, "points": p })).collect();
    json!([
        { "id": "commander", "label": "Commander", "life": 40, "commander_damage": 21, "has_commander": true,
          "summary": "100 cards with a legendary commander, singleton, Commander banned list, brackets 1–5." },
        { "id": "pdh", "label": "Pauper Commander", "life": 30, "commander_damage": 16, "has_commander": true,
          "summary": "An uncommon creature commander and 99 commons (paper or MTGO printings). 30 life, 16 commander damage. Mystic Remora and Rhystic Study banned." },
        { "id": "canlander", "label": "Canadian Highlander", "life": 20, "commander_damage": null, "has_commander": false,
          "max_points": CANLANDER_MAX_POINTS, "points_date": CANLANDER_POINTS_DATE, "points": points,
          "summary": "100+ cards, singleton, no commander or sideboard, one-on-one at 20 life. Vintage banned list and at most 10 points from the points list." },
    ])
}

fn read_games() -> Value {
    fs::read_to_string(GAMES_PATH)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(Value::is_array)
        .unwrap_or_else(|| Value::Array(Vec::new()))
}

fn read_settings() -> Value {
    fs::read_to_string(SETTINGS_PATH)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(Map::new()))
}

/// Default data directory for desktop builds.
pub fn default_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("COMMANDER_LAB_HOME") {
        return PathBuf::from(dir);
    }
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("CommanderLab");
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("commander-lab");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/share/commander-lab");
    }
    PathBuf::from(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_group_like_moxfield() {
        assert_eq!(card_kind("Artifact Creature — Golem"), "Creature");
        assert_eq!(card_kind("Legendary Land"), "Land");
        assert_eq!(card_kind("Artifact"), "Artifact");
        assert_eq!(card_kind("Kindred Instant — Elf"), "Instant");
        assert_eq!(card_kind("Creature — Human // Land"), "Creature");
    }

    #[test]
    fn take_and_put_merge_quantities() {
        let mut list = vec![DeckEntry { quantity: 3, name: "Island".into(), ..Default::default() }];
        assert_eq!(Engine::take(&mut list, "island", Some(2)), 2);
        assert_eq!(list[0].quantity, 1);
        Engine::put(&mut list, "Island", 4);
        assert_eq!(list[0].quantity, 5);
        assert_eq!(Engine::take(&mut list, "Island", None), 5);
        assert!(list.is_empty());
    }
}
