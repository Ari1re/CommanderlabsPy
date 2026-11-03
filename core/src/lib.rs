//! Commander Lab engine.
//!
//! Everything the app does lives here; the desktop/Android shell (Tauri) and
//! the web server are thin wrappers around [`api::Engine`].
//!
//! Where to start reading: `api.rs` (every command the UI sends), then the
//! module for the feature you're changing. Data lives in the folder the
//! engine opens: `decks/` (text decklists and sidecars), `data/` (card
//! database, caches, settings, models, tools) and `exports/`.
//!
//! Things to work on:
//! - Several modules are 1,500–3,900 lines (`api.rs`, `generator.rs`,
//!   `ai.rs`, `references.rs`, `data.rs`); each has notes on how to split it.

pub mod ai;
pub mod ai_runtime;
pub mod analysis;
pub mod api;
pub mod archetype;
pub mod archidekt;
pub mod bracket;
pub mod budget;
pub mod commander;
pub mod data;
pub mod deck;
pub mod deck_chat;
pub mod deck_history;
pub mod deep_agent;
pub mod deep_generation;
pub mod deep_learn;
pub mod edhrec;
pub mod format;
pub mod format_sources;
pub mod generator;
pub mod goldfish;
pub mod guide;
pub mod highlander;
pub mod integrity;
pub mod intelligence;
pub mod jobs;
pub mod llama;
pub mod local_llm;
pub mod logging;
pub mod meta_sim;
pub mod moxfield;
pub mod precons;
pub mod mpc;
pub mod mpc_autofill;
pub mod oracle_search;
pub mod playbooks;
pub mod ollama;
pub mod printings;
pub mod references;
pub mod scryfall_search;
pub mod share;
pub mod storage;
pub mod simulation;
pub mod sources;
pub mod spellbook;
pub mod tts;
pub mod updater;
pub mod user_reference;
pub mod versions;
pub mod win_lines;
pub mod web_research;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
