mod ai;
mod ai_runtime;
mod analysis;
mod archidekt;
mod app;
mod archetype;
mod bracket;
mod commander;
mod data;
mod deck;
mod deck_history;
mod deep_learn;
mod deep_agent;
mod edhrec;
mod generator;
mod goldfish;
mod intelligence;
mod logging;
mod llama;
mod meta_sim;
mod moxfield;
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

    println!("Preparing selected local AI backend...");
    let mut ai_runtime =
        ai_runtime::LocalAiRuntime::ensure_running()?;
    println!("{}", ai_runtime.status_line());
    println!("AI backend ready. Starting Commander Lab...");

    let mut app = App::new()?;
    app.status = ai_runtime.status_line();

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

    ai_runtime.shutdown();

    run_result?;
    Ok(())
}
