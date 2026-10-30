//! Commander Lab engine.
//!
//! Everything the app does lives here; the desktop/Android shell (Tauri) and
//! the web server are thin wrappers around [`api::Engine`].

pub mod ai;
pub mod ai_runtime;
pub mod analysis;
pub mod api;
pub mod archetype;
pub mod archidekt;
pub mod bracket;
pub mod commander;
pub mod data;
pub mod deck;
pub mod deck_history;
pub mod deep_agent;
pub mod deep_learn;
pub mod edhrec;
pub mod generator;
pub mod goldfish;
pub mod guide;
pub mod integrity;
pub mod intelligence;
pub mod jobs;
pub mod llama;
pub mod logging;
pub mod meta_sim;
pub mod moxfield;
pub mod mpc;
pub mod ollama;
pub mod printings;
pub mod references;
pub mod share;
pub mod simulation;
pub mod sources;
pub mod spellbook;
pub mod user_reference;
pub mod versions;
pub mod web_research;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
