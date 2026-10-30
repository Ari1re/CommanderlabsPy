mod ai;
mod ai_runtime;
mod analysis;
mod archidekt;
mod app;
mod archetype;
mod bracket;
mod clipboard;
mod commander;
mod data;
mod deck;
mod deck_history;
mod deep_learn;
mod deep_agent;
mod edhrec;
mod generator;
mod integrity;
mod goldfish;
mod intelligence;
mod logging;
mod llama;
mod meta_sim;
mod moxfield;
mod mpc;
mod ollama;
mod references;
mod runtime_stats;
mod simulation;
mod spellbook;
mod sources;
mod theme;
mod ui;
mod user_reference;
mod web_research;

use anyhow::Result;
use app::App;
use crossterm::{
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste,
        EnableMouseCapture,
    },
    execute,
};
use std::io::stdout;

fn main() -> Result<()> {
    std::fs::create_dir_all("./decks")?;
    std::fs::create_dir_all("./data")?;
    std::fs::create_dir_all("./data/images")?;

    // The local AI backend is optional: deck building, analysis, MPC and
    // exports all work without it. Android never starts a local model.
    let skip_ai = cfg!(target_os = "android")
        || std::env::var_os("COMMANDER_LAB_NO_AI").is_some();

    let mut ai_runtime = if skip_ai {
        None
    } else {
        println!("Preparing selected local AI backend...");
        match ai_runtime::LocalAiRuntime::ensure_running() {
            Ok(runtime) => {
                println!("{}", runtime.status_line());
                println!("AI backend ready. Starting Commander Lab...");
                Some(runtime)
            }
            Err(err) => {
                println!("AI backend unavailable ({err}). Continuing without AI features.");
                None
            }
        }
    };

    let mut app = App::new()?;
    app.status = match &ai_runtime {
        Some(runtime) => runtime.status_line(),
        None => "AI backend off — deterministic generator, analysis, MPC and exports are available.".to_string(),
    };

    execute!(
        stdout(),
        EnableMouseCapture,
        EnableBracketedPaste,
    )?;

    let run_result = ratatui::run(|terminal| app.run(terminal));

    let _ = execute!(
        stdout(),
        DisableBracketedPaste,
        DisableMouseCapture,
    );

    if let Some(runtime) = ai_runtime.as_mut() {
        runtime.shutdown();
    }

    run_result?;
    Ok(())
}
