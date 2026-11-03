//! Matchup simulation against reference decks.
//!
//! How it fits: [`simulate_meta_matchups`] races the deck against reference
//! decks (`references.rs`) by comparing goldfish speed and interaction, and
//! returns [`MetaMatchupSummary`] for the agent.
//!
//! Things to work on:
//! - It's a speed race, not a game: no interaction between the decks. Treat
//!   results as a rough "faster/slower than the field" signal.
//! - Opponents come from the same commander's references, so it compares a
//!   deck mostly to variants of itself; a general meta sample would be more
//!   useful.

use crate::{
    data::CardDb,
    deck::{DeckEntry, ParsedDeck},
    goldfish::{goldfish_with_combo_lines, GoldfishResult},
    references::ReferenceDeck,
};
use anyhow::{anyhow, Result};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaOpponentResult {
    pub title: String,
    pub source: String,
    pub sampled_games: u32,
    pub head_to_head_win_pct: f64,
    pub opponent_earliest_turn: Option<u8>,
    pub opponent_likely_turn: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaMatchupSummary {
    pub runs: u32,
    pub opponents_used: usize,
    pub outright_wins: u64,
    pub shared_first_place: u64,
    pub losses: u64,
    pub outright_win_pct: f64,
    pub equity_win_pct: f64,
    pub shared_first_pct: f64,
    pub loss_pct: f64,
    pub average_winning_turn: Option<f64>,
    pub user_goldfish: GoldfishResult,
    pub opponents: Vec<MetaOpponentResult>,
    pub caveat: String,
}

/// A reference decklist as a deck that can be goldfished (counts merged).
fn reference_as_deck(reference: &ReferenceDeck) -> ParsedDeck {
    let mut counts = HashMap::<String, (String, u32)>::new();

    for name in &reference.cards {
        let key = name.to_ascii_lowercase();
        let entry = counts
            .entry(key)
            .or_insert((name.clone(), 0));
        entry.1 = entry.1.saturating_add(1);
    }

    let mut main = counts
        .into_values()
        .map(|(name, quantity)| DeckEntry {
            quantity,
            name, ..Default::default() })
        .collect::<Vec<_>>();

    main.sort_by(|a, b| a.name.cmp(&b.name));

    ParsedDeck {
        name: reference.title.clone(),
        commanders: Vec::new(),
        main,
        sideboard: Vec::new(),
        format: Default::default(),
        considering: Vec::new(),
    }
}

/// Draw one "win turn" from a goldfish result's distribution (None = it
/// didn't win in the simulated turns).
fn sample_turn<R: Rng + ?Sized>(
    result: &GoldfishResult,
    rng: &mut R,
) -> Option<u8> {
    if result.runs == 0 {
        return None;
    }

    let roll = rng.random_range(0..result.runs as u64);
    let mut cursor = 0u64;

    for (turn, count) in &result.combo_turn_counts {
        cursor = cursor.saturating_add(*count);

        if roll < cursor {
            return Some(*turn);
        }
    }

    None
}

/// Who wins a race: the earliest win turn takes it; ties split the win.
/// Returns (won, tied, equity).
fn race_equity(
    hero: Option<u8>,
    opponents: &[Option<u8>],
) -> (bool, bool, f64) {
    let Some(hero_turn) = hero else {
        return (false, false, 0.0);
    };

    let best_opponent = opponents.iter().flatten().min().copied();

    match best_opponent {
        None => (true, false, 1.0),
        Some(turn) if hero_turn < turn => (true, false, 1.0),
        Some(turn) if hero_turn > turn => (false, false, 0.0),
        Some(turn) => {
            let tied_opponents = opponents
                .iter()
                .filter(|candidate| **candidate == Some(turn))
                .count();

            (
                false,
                true,
                1.0 / (tied_opponents as f64 + 1.0),
            )
        }
    }
}

/// Race the deck against reference decks: goldfish each, then sample win
/// turns for the deck and a pod of opponents many times and count how often
/// it wins first. A speed comparison only: decks never interact.
pub fn simulate_meta_matchups<F>(
    deck: &ParsedDeck,
    db: &CardDb,
    references: &[ReferenceDeck],
    runs: u32,
    mut progress: F,
) -> Result<MetaMatchupSummary>
where
    F: FnMut(String),
{
    if runs == 0 {
        return Err(anyhow!("Meta simulation needs at least one game."));
    }

    let opponents = references
        .iter()
        .filter(|deck| deck.cards.len() >= 70 && !deck.combos.is_empty())
        .take(16)
        .cloned()
        .collect::<Vec<_>>();

    if opponents.len() < 3 {
        return Err(anyhow!(
            "Need at least three cached reference decks with known combo lines for a meta pod simulation."
        ));
    }

    progress(format!(
        "[META] Building hero model for {runs} requested pod games"
    ));

    let hero_combos = crate::spellbook::find_my_combos(deck)
        .ok()
        .map(|report| {
            report
                .variants
                .into_iter()
                .filter(|line| !line.almost && line.cards.len() >= 2)
                .map(|line| line.cards)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let hero = goldfish_with_combo_lines(
        deck,
        db,
        &hero_combos,
        runs,
    )?;

    let opponent_sample_runs =
        (runs / opponents.len() as u32).max(100);

    let mut opponent_models =
        Vec::<(ReferenceDeck, GoldfishResult)>::new();

    for (index, opponent) in opponents.iter().enumerate() {
        progress(format!(
            "[META] Opponent model {}/{} • {}",
            index + 1,
            opponents.len(),
            opponent.title,
        ));

        let parsed = reference_as_deck(opponent);
        let combo_lines = opponent
            .combos
            .iter()
            .map(|combo| combo.cards.clone())
            .collect::<Vec<_>>();

        if let Ok(result) = goldfish_with_combo_lines(
            &parsed,
            db,
            &combo_lines,
            opponent_sample_runs,
        ) {
            opponent_models.push((opponent.clone(), result));
        }
    }

    if opponent_models.len() < 3 {
        return Err(anyhow!(
            "Fewer than three cached meta decks could be simulated with the local card database."
        ));
    }

    progress(format!(
        "[META] Playing {runs} four-player race games against {} cached meta decks",
        opponent_models.len(),
    ));

    let mut rng = rand::rng();
    let mut outright_wins = 0u64;
    let mut shared_first_place = 0u64;
    let mut losses = 0u64;
    let mut equity = 0.0f64;
    let mut winning_turn_total = 0u64;
    let mut winning_turn_samples = 0u64;
    let mut head_wins = vec![0.0f64; opponent_models.len()];
    let mut head_games = vec![0u64; opponent_models.len()];

    for game in 0..runs {
        let hero_turn = sample_turn(&hero, &mut rng);
        let start = game as usize % opponent_models.len();
        let seats = [
            start,
            (start + 1) % opponent_models.len(),
            (start + 2) % opponent_models.len(),
        ];

        let opponent_turns = seats
            .iter()
            .map(|index| {
                sample_turn(&opponent_models[*index].1, &mut rng)
            })
            .collect::<Vec<_>>();

        let (outright, shared, game_equity) =
            race_equity(hero_turn, &opponent_turns);

        equity += game_equity;

        if outright {
            outright_wins += 1;
        } else if shared {
            shared_first_place += 1;
        } else {
            losses += 1;
        }

        if game_equity > 0.0 {
            if let Some(turn) = hero_turn {
                winning_turn_total += turn as u64;
                winning_turn_samples += 1;
            }
        }

        for (seat, opponent_turn) in
            seats.iter().zip(opponent_turns.iter())
        {
            head_games[*seat] += 1;

            let score = match (hero_turn, *opponent_turn) {
                (Some(hero), Some(other)) if hero < other => 1.0,
                (Some(hero), Some(other)) if hero == other => 0.5,
                (Some(_), None) => 1.0,
                _ => 0.0,
            };

            head_wins[*seat] += score;
        }
    }

    let pct = |count: u64| {
        count as f64 * 100.0 / runs as f64
    };

    let opponent_results = opponent_models
        .iter()
        .enumerate()
        .map(|(index, (reference, result))| {
            MetaOpponentResult {
                title: reference.title.clone(),
                source: reference.source.clone(),
                sampled_games: head_games[index] as u32,
                head_to_head_win_pct: if head_games[index] == 0 {
                    0.0
                } else {
                    head_wins[index] * 100.0
                        / head_games[index] as f64
                },
                opponent_earliest_turn:
                    result.earliest_combo_turn,
                opponent_likely_turn:
                    result.most_likely_combo_turn,
            }
        })
        .collect::<Vec<_>>();

    Ok(MetaMatchupSummary {
        runs,
        opponents_used: opponent_models.len(),
        outright_wins,
        shared_first_place,
        losses,
        outright_win_pct: pct(outright_wins),
        equity_win_pct: equity * 100.0 / runs as f64,
        shared_first_pct: pct(shared_first_place),
        loss_pct: pct(losses),
        average_winning_turn: if winning_turn_samples == 0 {
            None
        } else {
            Some(
                winning_turn_total as f64
                    / winning_turn_samples as f64,
            )
        },
        user_goldfish: hero,
        opponents: opponent_results,
        caveat: "Meta matchup testing is a four-player race model built from cached reference decklists and known combo lines. It models relative execution speed and ties, not full priority, interaction choices, combat, mulligan strategy, or complete Magic rules.".to_string(),
    })
}
