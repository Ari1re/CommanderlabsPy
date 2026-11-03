//! Goldfish simulation: how fast a deck wins against no opponent.
//!
//! How it fits: [`goldfish`] plays many games of draw, land, mana and spells
//! (cards simplified to cost, land, ramp, draw and tutor) and records when a
//! known combo line (from `spellbook.rs`) would be assembled.
//! [`GoldfishResult`] holds keepable-hand rates, mana development (3 mana by
//! turn 3…) and combo-turn odds. The agent, guide and analysis use it.
//!
//! Things to work on:
//! - Cards are reduced to a few flags, so engines, value and non-combo wins
//!   aren't modelled; "win turn" really means "combo assembled".
//! - Mulligans are rough: an unkeepable hand (not 2–5 lands) is reshuffled
//!   into one fresh seven, with no card put on the bottom.
//! - Tutors find any combo piece instantly; restricted tutors (creature-only,
//!   land-only) should only find what they can.

use crate::{
    analysis::classify_card,
    data::CardDb,
    deck::ParsedDeck,
    spellbook::ComboReport,
};
use anyhow::{anyhow, Result};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
struct GoldCard {
    name: String,
    cmc: f64,
    land: bool,
    ramp: bool,
    draw: bool,
    tutor: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldfishResult {
    pub runs: u32,
    pub sample_log: Vec<String>,
    pub sample_opening_hand: Vec<String>,
    pub keepable_pct: f64,
    pub three_mana_t3_pct: f64,
    pub four_mana_t4_pct: f64,
    pub five_mana_t5_pct: f64,
    pub combo_by_t3_pct: f64,
    pub combo_by_t4_pct: f64,
    pub combo_by_t5_pct: f64,
    pub combo_by_t6_pct: f64,
    pub earliest_combo_turn: Option<u8>,
    pub most_likely_combo_turn: Option<u8>,
    pub median_combo_turn: Option<u8>,
    pub combo_turn_exact_pct: Vec<(u8, f64)>,
    pub combo_by_turn_pct: Vec<(u8, f64)>,
    pub combo_turn_counts: Vec<(u8, u64)>,
    pub no_combo_by_t8_count: u64,
    pub combo_line_hits: Vec<(Vec<String>, u64)>,
    pub combo_lines_checked: usize,
    pub caveat: String,
}

/// Goldfish a deck using the complete combos from a Spellbook report.
pub fn goldfish(
    deck: &ParsedDeck,
    db: &CardDb,
    combos: Option<&ComboReport>,
    runs: u32,
) -> Result<GoldfishResult> {
    let combo_lines = combos
        .map(|report| {
            report
                .variants
                .iter()
                .filter(|line| !line.almost && line.cards.len() >= 2)
                .map(|line| line.cards.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    goldfish_with_combo_lines(deck, db, &combo_lines, runs)
}

/// Play `runs` solo games (8 turns each) and record mana development and the
/// turn each known combo line comes together. Each game: shuffle, draw 7
/// (keepable = 2–5 lands; otherwise one fresh seven), then each turn draw, play a land, cast ramp and
/// draw first, use a tutor to find a missing combo piece, and check whether
/// every piece of a line has been seen (commanders count from the start).
/// Keeps one sample game log for display.
pub fn goldfish_with_combo_lines(
    deck: &ParsedDeck,
    db: &CardDb,
    combo_lines: &[Vec<String>],
    runs: u32,
) -> Result<GoldfishResult> {

    // The library: every main-deck card as a simple record (cost, land, ramp,
    // draw, tutor). Needs 70+ cards to be meaningful.
    let mut library = Vec::<GoldCard>::new();

    for entry in &deck.main {
        let Some(card) = db.get_card(&entry.name)? else {
            continue;
        };

        let roles = classify_card(&card);

        for _ in 0..entry.quantity {
            library.push(GoldCard {
                name: card.name.clone(),
                cmc: card.cmc,
                land: card.type_line.to_ascii_lowercase().contains("land"),
                ramp: roles.ramp,
                draw: roles.draw,
                tutor: roles.tutor,
            });
        }
    }

    if library.len() < 70 {
        return Err(anyhow!(
            "The parsed main deck is too small for a useful goldfish run."
        ));
    }

    let combo_lines = combo_lines
        .iter()
        .filter(|line| line.len() >= 2)
        .map(|line| {
            line.iter()
                .map(|name| name.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();


    let mut keepable = 0u64;
    let mut three_t3 = 0u64;
    let mut four_t4 = 0u64;
    let mut five_t5 = 0u64;
    let mut combo_t3 = 0u64;
    let mut combo_t4 = 0u64;
    let mut combo_t5 = 0u64;
    let mut combo_t6 = 0u64;
    let mut combo_turn_counts = [0u64; 9];
    let mut combo_line_hit_counts = vec![0u64; combo_lines.len()];

    let mut rng = rand::rng();
    let mut sample_log = Vec::new();
    let mut sample_opening_hand = Vec::new();

    for run in 0..runs {
        library.shuffle(&mut rng);

        let mut hand = library.iter().take(7).cloned().collect::<Vec<_>>();
        let mut position = 7usize;

        let opening_lands = hand.iter().filter(|card| card.land).count();

        if (2..=5).contains(&opening_lands) {
            keepable += 1;
        } else {
            // Simple London-mulligan approximation: draw a fresh seven once.
            library.shuffle(&mut rng);
            hand = library.iter().take(7).cloned().collect();
            position = 7;
        }

        if run == 0 {
            sample_opening_hand = hand
                .iter()
                .map(|card| card.name.clone())
                .collect();
            sample_log.push(format!(
                "Opening hand: {}",
                sample_opening_hand.join(", ")
            ));
        }

        let mut lands_in_play = 0u32;
        let mut ramp_mana = 0u32;
        let mut seen = hand
            .iter()
            .map(|card| card.name.to_ascii_lowercase())
            .collect::<HashSet<_>>();

        for commander in &deck.commanders {
            seen.insert(commander.name.to_ascii_lowercase());
        }

        let mut hand_counts = build_counts(&hand);

        let mut combo_turn = None::<u8>;

        for turn in 1u8..=8 {
            if position < library.len() {
                let drawn = library[position].clone();
                position += 1;
                *hand_counts.entry(drawn.name.to_ascii_lowercase()).or_default() += 1;
                seen.insert(drawn.name.to_ascii_lowercase());

                if run == 0 {
                    sample_log.push(format!("T{turn} draw: {}", drawn.name));
                }
            }

            if remove_first_kind(&mut hand, &mut hand_counts, |card| card.land).is_some() {
                lands_in_play += 1;
                if run == 0 {
                    sample_log.push(format!("T{turn}: play land ({lands_in_play} lands)"));
                }
            }

            let mut mana = lands_in_play + ramp_mana;

            // Cast cheap ramp first. Ramp added this turn is assumed usable
            // next turn unless the card itself is zero-cost/ritual-like.
            loop {
                let candidate = hand
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| card.ramp && !card.land)
                    .filter(|(_, card)| card.cmc <= mana as f64)
                    .min_by(|a, b| {
                        a.1.cmc
                            .partial_cmp(&b.1.cmc)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(index, _)| index);

                let Some(index) = candidate else {
                    break;
                };

                let card = hand.remove(index);
                decrement_count(&mut hand_counts, &card.name);

                let cost = card.cmc.ceil() as u32;
                mana = mana.saturating_sub(cost.min(mana));
                ramp_mana += 1;

                if run == 0 {
                    sample_log.push(format!(
                        "T{turn}: cast ramp {} (future mana +1)",
                        card.name
                    ));
                }
            }

            // Tutors are modeled as finding one missing combo piece.
            if !combo_lines.is_empty() {
                if let Some(index) = hand
                    .iter()
                    .position(|card| card.tutor && card.cmc <= mana as f64)
                {
                    let tutor = hand.remove(index);
                    decrement_count(&mut hand_counts, &tutor.name);
                    let cost = tutor.cmc.ceil() as u32;
                    mana = mana.saturating_sub(cost.min(mana));

                    if let Some(missing) = first_missing_combo_piece(&combo_lines, &seen) {
                        seen.insert(missing.clone());
                        if run == 0 {
                            sample_log.push(format!(
                                "T{turn}: cast {} and tutor {}",
                                tutor.name, missing
                            ));
                        }
                    }
                }
            }

            // A basic draw spell is treated as drawing one extra card.
            if let Some(index) = hand
                .iter()
                .position(|card| card.draw && card.cmc <= mana as f64)
            {
                let draw_spell = hand.remove(index);
                decrement_count(&mut hand_counts, &draw_spell.name);

                // No further spell is cast from this mana pool after the
                // draw-spell branch, so subtracting its cost here had no
                // effect on any later decision in the turn.
                if position < library.len() {
                    let extra = library[position].clone();
                    position += 1;
                    seen.insert(extra.name.to_ascii_lowercase());
                    hand.push(extra.clone());
                    *hand_counts.entry(extra.name.to_ascii_lowercase()).or_default() += 1;

                    if run == 0 {
                        sample_log.push(format!(
                            "T{turn}: cast {} → draw {}",
                            draw_spell.name, extra.name
                        ));
                    }
                }
            }

            let available_next = lands_in_play + ramp_mana;

            if turn == 3 && available_next >= 3 {
                three_t3 += 1;
            }
            if turn == 4 && available_next >= 4 {
                four_t4 += 1;
            }
            if turn == 5 && available_next >= 5 {
                five_t5 += 1;
            }

            if combo_turn.is_none() {
                if let Some(line_index) = combo_lines.iter().position(|line| {
                    line.iter().all(|name| seen.contains(name))
                }) {
                    combo_turn = Some(turn);

                    if let Some(count) =
                        combo_line_hit_counts.get_mut(line_index)
                    {
                        *count += 1;
                    }

                    if run == 0 {
                        sample_log.push(format!(
                            "T{turn}: known Spellbook combo assembled: {}",
                            combo_lines[line_index].join(" + ")
                        ));
                    }
                }
            }
        }

        if combo_turn.is_some_and(|turn| turn <= 3) {
            combo_t3 += 1;
        }
        if combo_turn.is_some_and(|turn| turn <= 4) {
            combo_t4 += 1;
        }
        if combo_turn.is_some_and(|turn| turn <= 5) {
            combo_t5 += 1;
        }
        if combo_turn.is_some_and(|turn| turn <= 6) {
            combo_t6 += 1;
        }

        if let Some(turn) = combo_turn {
            if (turn as usize) < combo_turn_counts.len() {
                combo_turn_counts[turn as usize] += 1;
            }
        }
    }

    let pct = |value: u64| value as f64 / runs.max(1) as f64 * 100.0;

    let combo_turn_exact_pct = (1u8..=8)
        .map(|turn| {
            (
                turn,
                pct(combo_turn_counts[turn as usize]),
            )
        })
        .collect::<Vec<_>>();

    let mut cumulative = 0u64;
    let combo_by_turn_pct = (1u8..=8)
        .map(|turn| {
            cumulative += combo_turn_counts[turn as usize];
            (turn, pct(cumulative))
        })
        .collect::<Vec<_>>();

    let earliest_combo_turn = (1u8..=8)
        .find(|turn| combo_turn_counts[*turn as usize] > 0);

    let most_likely_combo_turn = (1u8..=8)
        .max_by_key(|turn| combo_turn_counts[*turn as usize])
        .filter(|turn| combo_turn_counts[*turn as usize] > 0);

    let total_wins = combo_turn_counts.iter().sum::<u64>();
    let median_combo_turn = if total_wins == 0 {
        None
    } else {
        let threshold = (total_wins + 1) / 2;
        let mut seen_wins = 0u64;
        let mut found = None;

        for turn in 1u8..=8 {
            seen_wins += combo_turn_counts[turn as usize];

            if seen_wins >= threshold {
                found = Some(turn);
                break;
            }
        }

        found
    };

    let combo_turn_counts = (1u8..=8)
        .map(|turn| (turn, combo_turn_counts[turn as usize]))
        .collect::<Vec<_>>();

    let total_combo_wins =
        combo_turn_counts.iter().map(|(_, count)| *count).sum::<u64>();

    let no_combo_by_t8_count =
        runs as u64 - total_combo_wins.min(runs as u64);

    let combo_line_hits = combo_lines
        .iter()
        .cloned()
        .zip(combo_line_hit_counts.into_iter())
        .filter(|(_, count)| *count > 0)
        .collect::<Vec<_>>();

    Ok(GoldfishResult {
        runs,
        sample_log,
        sample_opening_hand,
        keepable_pct: pct(keepable),
        three_mana_t3_pct: pct(three_t3),
        four_mana_t4_pct: pct(four_t4),
        five_mana_t5_pct: pct(five_t5),
        combo_by_t3_pct: pct(combo_t3),
        combo_by_t4_pct: pct(combo_t4),
        combo_by_t5_pct: pct(combo_t5),
        combo_by_t6_pct: pct(combo_t6),
        earliest_combo_turn,
        most_likely_combo_turn,
        median_combo_turn,
        combo_turn_exact_pct,
        combo_by_turn_pct,
        combo_turn_counts,
        no_combo_by_t8_count,
        combo_line_hits,
        combo_lines_checked: combo_lines.len(),
        caveat:
            "Goldfish uses a deterministic heuristic pilot: lands, cheap ramp, one-card draw, tutors, and known Spellbook combo assembly. It is not a complete Magic rules engine, so treat win-turn results as comparative estimates."
                .to_string(),
    })
}


/// How many of each card are in hand.
fn build_counts(hand: &[GoldCard]) -> HashMap<String, u32> {
    let mut counts = HashMap::new();

    for card in hand {
        *counts.entry(card.name.to_ascii_lowercase()).or_default() += 1;
    }

    counts
}

/// One fewer of a card in hand.
fn decrement_count(counts: &mut HashMap<String, u32>, name: &str) {
    let key = name.to_ascii_lowercase();

    if let Some(value) = counts.get_mut(&key) {
        *value = value.saturating_sub(1);
    }
}

/// Take the first card in hand matching `predicate` (e.g. a land).
fn remove_first_kind<F>(
    hand: &mut Vec<GoldCard>,
    counts: &mut HashMap<String, u32>,
    predicate: F,
) -> Option<GoldCard>
where
    F: Fn(&GoldCard) -> bool,
{
    let index = hand.iter().position(predicate)?;
    let card = hand.remove(index);
    decrement_count(counts, &card.name);
    Some(card)
}


/// The first piece of any combo line that hasn't been seen yet (what a
/// tutor fetches).
fn first_missing_combo_piece(
    combo_lines: &[Vec<String>],
    seen: &HashSet<String>,
) -> Option<String> {
    combo_lines
        .iter()
        .min_by_key(|line| {
            line.iter()
                .filter(|name| !seen.contains(*name))
                .count()
        })
        .and_then(|line| {
            line.iter()
                .find(|name| !seen.contains(*name))
                .cloned()
        })
}
