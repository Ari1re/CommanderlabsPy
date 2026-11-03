//! Deck research and testing for the agent ("Deep Learn").
//!
//! How it fits: [`deep_learn_deck`] gathers what's known about a deck's
//! commander (EDHREC, references, combos) into a [`DeepLearnReport`];
//! [`simulate_deep_learn_games`] goldfishes a deck many times
//! ([`TestingIntensity`] sets how many) and summarizes win turns and keepable
//! hands. `deep_agent.rs` uses both every round.
//!
//! Things to work on:
//! - Name clash: "Deep Learn" (agent research) and "deep learning
//!   generation" (the generator) are different things. Renaming this one
//!   (e.g. "agent research") would avoid confusion.
//! - Testing intensity only changes run counts; also vary opening-hand rules
//!   (mulligans) so results reflect real play.

use crate::{
    analysis::{
        classify_card, has_copy_rule_override, identity_subset,
    },
    data::{Card, CardDb},
    deck::ParsedDeck,
    goldfish::{goldfish, GoldfishResult},
    intelligence::{
        learn_commander_intelligence, CommanderIntelligence,
    },
    simulation::{run_draw_simulation, DrawSimulation},
    spellbook::find_my_combos,
    sources::DeckSource,
};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// How much simulation a test runs (the agent's "testing depth").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestingIntensity {
    Quick,
    Standard,
    Thorough,
    Exhaustive,
}

impl Default for TestingIntensity {
    /// Standard depth by default.
    fn default() -> Self {
        Self::Thorough
    }
}

impl TestingIntensity {
    /// Depth name for logs.
    pub fn label(self) -> &'static str {
        match self {
            TestingIntensity::Quick => "QUICK",
            TestingIntensity::Standard => "STANDARD",
            TestingIntensity::Thorough => "THOROUGH",
            TestingIntensity::Exhaustive => "EXHAUSTIVE",
        }
    }

    /// Opening-hand shuffles: 10k quick … 1M exhaustive.
    pub fn draw_runs(self) -> u32 {
        match self {
            TestingIntensity::Quick => 10_000,
            TestingIntensity::Standard => 75_000,
            TestingIntensity::Thorough => 300_000,
            TestingIntensity::Exhaustive => 1_000_000,
        }
    }

    /// Goldfish games: 5k quick … 500k exhaustive.
    pub fn goldfish_runs(self) -> u32 {
        match self {
            TestingIntensity::Quick => 5_000,
            TestingIntensity::Standard => 30_000,
            TestingIntensity::Thorough => 125_000,
            TestingIntensity::Exhaustive => 500_000,
        }
    }

    /// What the depth is for, in words.
    pub fn description(self) -> &'static str {
        match self {
            TestingIntensity::Quick => {
                "Fast structural check for iteration."
            }
            TestingIntensity::Standard => {
                "Balanced research and game sampling."
            }
            TestingIntensity::Thorough => {
                "Large simulation set for serious optimization."
            }
            TestingIntensity::Exhaustive => {
                "Very large Monte Carlo + goldfish sample; expect a longer run."
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepLearnCard {
    pub name: String,
    pub score: i32,
    pub confidence: u8,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepLearnReport {
    pub commander_names: Vec<String>,
    pub normal_themes: Vec<String>,
    pub reference_decks: usize,
    pub edhrec_signals: usize,
    pub core_coverage_pct: f64,
    pub confidence: u8,
    pub role_targets: String,
    pub reference_core_cards: Vec<String>,
    pub reference_packages: Vec<String>,
    pub reference_combos: Vec<String>,
    pub adds: Vec<DeepLearnCard>,
    pub cuts: Vec<DeepLearnCard>,
    pub findings: Vec<String>,
    pub decision_trace: Vec<String>,
    pub testing: Option<DeepLearnTestSummary>,
    pub sources: Vec<DeckSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepLearnTestSummary {
    pub intensity: TestingIntensity,
    pub draw_runs: u32,
    pub goldfish_runs: u32,
    pub draw: DrawSimulation,
    pub goldfish: GoldfishResult,
}

/// Goldfish a deck at an intensity: opening hands plus games, summarized
/// as win (combo) turns and keepable-hand rates.
pub fn simulate_deep_learn_games<F>(
    deck: &ParsedDeck,
    db: &CardDb,
    intensity: TestingIntensity,
    progress: F,
) -> Result<DeepLearnTestSummary>
where
    F: FnMut(String),
{
    simulate_deep_learn_games_with_runs(
        deck,
        db,
        intensity,
        None,
        progress,
    )
}

/// Same, with explicit run counts (the agent's final re-test).
pub fn simulate_deep_learn_games_with_runs<F>(
    deck: &ParsedDeck,
    db: &CardDb,
    intensity: TestingIntensity,
    exact_runs: Option<u32>,
    mut progress: F,
) -> Result<DeepLearnTestSummary>
where
    F: FnMut(String),
{
    let draw_runs = exact_runs.unwrap_or_else(|| intensity.draw_runs());
    let goldfish_runs =
        exact_runs.unwrap_or_else(|| intensity.goldfish_runs());

    let label = if let Some(runs) = exact_runs {
        format!("USER REQUEST • {runs} RUNS")
    } else {
        intensity.label().to_string()
    };

    progress(format!(
        "[SIM] {} • {} draw simulations",
        label,
        draw_runs,
    ));

    let draw = run_draw_simulation(deck, db, draw_runs)?;

    progress(format!(
        "[SIM] Resource pass complete • keepable={:.2}% • opening_lands={:.2} • 3_lands_T3={:.2}% • 4_lands_T4={:.2}% • 5_lands_T5={:.2}% • early_ramp_T2={:.2}%",
        draw.keepable_opening_pct,
        draw.average_opening_lands,
        draw.three_lands_by_t3_pct,
        draw.four_lands_by_t4_pct,
        draw.five_lands_by_t5_pct,
        draw.early_ramp_by_t2_pct,
    ));

    progress(format!(
        "[SIM] Playing {} heuristic goldfish games through turn 8",
        goldfish_runs,
    ));

    let combo_report = find_my_combos(deck).ok();
    let goldfish = goldfish(
        deck,
        db,
        combo_report.as_ref(),
        goldfish_runs,
    )?;

    progress(format!(
        "[SIM] Finished • earliest={} • likely={} • median={} • by_T3={:.2}% • by_T4={:.2}% • by_T5={:.2}% • by_T6={:.2}% • no_win_T8={}",
        goldfish
            .earliest_combo_turn
            .map(|turn| format!("T{turn}"))
            .unwrap_or_else(|| "none by T8".to_string()),
        goldfish
            .most_likely_combo_turn
            .map(|turn| format!("T{turn}"))
            .unwrap_or_else(|| "none".to_string()),
        goldfish
            .median_combo_turn
            .map(|turn| format!("T{turn}"))
            .unwrap_or_else(|| "none".to_string()),
        goldfish
            .combo_by_turn_pct
            .iter()
            .find(|(turn, _)| *turn == 3)
            .map(|(_, pct)| *pct)
            .unwrap_or(0.0),
        goldfish.combo_by_t4_pct,
        goldfish.combo_by_t5_pct,
        goldfish.combo_by_t6_pct,
        goldfish.no_combo_by_t8_count,
    ));

    if goldfish.combo_line_hits.is_empty() {
        progress(
            "[SIM] No known complete Commander Spellbook line assembled by T8"
                .to_string(),
        );
    } else {
        progress(format!(
            "[SIM] Winning line distribution • {}",
            goldfish
                .combo_line_hits
                .iter()
                .take(8)
                .map(|(cards, count)| {
                    format!("{} = {}", cards.join(" + "), count)
                })
                .collect::<Vec<_>>()
                .join(" | "),
        ));
    }

    Ok(DeepLearnTestSummary {
        intensity,
        draw_runs,
        goldfish_runs,
        draw,
        goldfish,
    })
}


/// The agent's research step: the commander model (intelligence.rs, with
/// EDHREC's top theme as a hint) turned into a report of the cards the
/// evidence supports, missing staples, combos and findings.
pub fn deep_learn_deck<F>(
    deck: &ParsedDeck,
    db: &CardDb,
    bracket: u8,
    mut progress: F,
) -> Result<DeepLearnReport>
where
    F: FnMut(String),
{
    progress(format!(
        "[DEEP] Resolving commander engine • deck={} • requested bracket=B{} • main={} • sideboard={}",
        deck.name,
        bracket,
        deck.main.iter().map(|entry| entry.quantity).sum::<u32>(),
        deck.sideboard.iter().map(|entry| entry.quantity).sum::<u32>(),
    ));

    let commanders = deck
        .commanders
        .iter()
        .filter_map(|entry| db.get_card(&entry.name).ok().flatten())
        .collect::<Vec<Card>>();

    if commanders.is_empty() {
        return Err(anyhow!(
            "Deep Learn needs at least one recognized commander."
        ));
    }

    progress(format!(
        "[DEEP] Commander(s) recognized • {}",
        commanders
            .iter()
            .map(|card| format!(
                "{} [{}] MV{:.1}",
                card.name,
                if card.color_identity.is_empty() {
                    "C".to_string()
                } else {
                    card.color_identity.clone()
                },
                card.cmc,
            ))
            .collect::<Vec<_>>()
            .join(" + "),
    ));

    let normal_theme_hint = crate::edhrec::commander_themes(
        &commanders,
        1,
    )
    .first()
    .map(|theme| theme.name.clone())
    .unwrap_or_default();

    let intelligence = learn_commander_intelligence(
        &commanders,
        &normal_theme_hint,
        bracket,
        db,
        |status| progress(status),
    )?;

    progress(format!(
        "[DEEP] Intelligence model ready • confidence={} • references={} • EDHREC signals={} • role targets={}",
        intelligence.confidence,
        intelligence.references.deck_count(),
        intelligence.edhrec_cards.len(),
        intelligence.role_summary(),
    ));

    progress(
        "[DEEP] Comparing every recognized current card against learned commander/reference support"
            .to_string(),
    );

    build_report(deck, db, intelligence, progress)
}

/// Turn the commander model into the research report the agent and its
/// AI profile read.
fn build_report<F>(
    deck: &ParsedDeck,
    db: &CardDb,
    intelligence: CommanderIntelligence,
    mut progress: F,
) -> Result<DeepLearnReport>
where
    F: FnMut(String),
{
    let current = deck
        .commanders
        .iter()
        .chain(deck.main.iter())
        .map(|entry| entry.name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    let mut decision_trace = vec![
        format!(
            "[MODEL] Commander(s): {} • identity {}.",
            intelligence.commander_names.join(" + "),
            intelligence.identity,
        ),
        format!(
            "[MODEL] Normal shell: {}.",
            intelligence
                .normal_themes
                .iter()
                .take(6)
                .cloned()
                .collect::<Vec<_>>()
                .join(" / "),
        ),
        format!(
            "[EVIDENCE] Confidence {}% from {} EDHREC signals and {} reference deck(s).",
            intelligence.confidence,
            intelligence.edhrec_cards.len(),
            intelligence.references.deck_count(),
        ),
        format!(
            "[ROLES] Learned targets: {}.",
            intelligence.role_summary(),
        ),
    ];

    progress(format!(
        "[RANK] Building ADD candidate pool from {} EDHREC signals + top 220 reference cards",
        intelligence.edhrec_cards.len(),
    ));

    let mut adds = Vec::<DeepLearnCard>::new();
    let mut candidates = intelligence
        .edhrec_cards
        .iter()
        .map(|signal| signal.name.clone())
        .chain(
            intelligence
                .references
                .top_cards(220)
                .into_iter()
                .map(|(name, _)| name),
        )
        .collect::<Vec<_>>();

    candidates.sort();
    candidates.dedup();

    for name in candidates {
        if current.contains(&name.to_ascii_lowercase()) {
            continue;
        }

        let Some(card) = db.get_card(&name)? else {
            continue;
        };

        if !card.commander_legal
            || !identity_subset(
                &card.color_identity,
                &intelligence.identity,
            )
        {
            continue;
        }

        let support = intelligence
            .support_for_card(&card, classify_card(&card));

        if support.score < 70 && support.confidence < 30 {
            continue;
        }

        adds.push(DeepLearnCard {
            name: card.name.clone(),
            score: support.score,
            confidence: support.confidence,
            reason: support.reason,
        });
    }

    adds.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.confidence.cmp(&a.confidence))
            .then_with(|| a.name.cmp(&b.name))
    });
    adds.truncate(36);

    progress(format!(
        "[RANK] ADD ranking complete • {} candidate(s) retained • top={}",
        adds.len(),
        adds.iter()
            .take(6)
            .map(|card| format!(
                "{}({}/{}%)",
                card.name,
                card.score,
                card.confidence,
            ))
            .collect::<Vec<_>>()
            .join(" | "),
    ));

    progress(
        "[RANK] Scoring current nonland cards for CUT pressure"
            .to_string(),
    );

    let mut cuts = Vec::<DeepLearnCard>::new();

    for entry in &deck.main {
        let Some(card) = db.get_card(&entry.name)? else {
            continue;
        };

        if card.is_basic
            || card.type_line.to_ascii_lowercase().contains("land")
        {
            continue;
        }

        if entry.quantity > 1 && has_copy_rule_override(&card) {
            continue;
        }

        let roles = classify_card(&card);
        let support = intelligence.support_for_card(&card, roles);

        // A card should only become a strong cut candidate when the learned
        // shell has meaningful confidence and the card still has little
        // commander/reference support. Structural cards get extra protection.
        let structural_credit = if support.structural { 35 } else { 0 };
        let cut_pressure =
            95 - support.score - structural_credit;

        if cut_pressure > 15 {
            cuts.push(DeepLearnCard {
                name: card.name.clone(),
                score: cut_pressure,
                confidence: intelligence.confidence,
                reason: format!(
                    "{} • learned support {}",
                    support.reason,
                    support.score,
                ),
            });
        }
    }

    cuts.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.name.cmp(&b.name))
    });
    cuts.truncate(28);

    progress(format!(
        "[RANK] CUT ranking complete • {} candidate(s) retained • top={}",
        cuts.len(),
        cuts.iter()
            .take(6)
            .map(|card| format!(
                "{}(pressure {})",
                card.name,
                card.score,
            ))
            .collect::<Vec<_>>()
            .join(" | "),
    ));

    let core = intelligence
        .edhrec_cards
        .iter()
        .take(30)
        .collect::<Vec<_>>();

    let core_present = core
        .iter()
        .filter(|signal| {
            current.contains(&signal.name.to_ascii_lowercase())
        })
        .count();

    let core_coverage_pct = if core.is_empty() {
        0.0
    } else {
        core_present as f64 / core.len() as f64 * 100.0
    };

progress(format!(
    "[CONSENSUS] Comparing reference corpus • decks={} • pair threshold={}%",
    intelligence.references.deck_count(),
    25,
));

let reference_decks = intelligence.references.deck_count();
let minimum_pair_count = if reference_decks >= 20 {
    ((reference_decks as f64) * 0.25).ceil() as u32
} else {
    2
};

let reference_core_cards = intelligence
    .references
    .top_cards(18)
    .into_iter()
    .map(|(name, count)| {
        format!(
            "{} — {}/{} ({:.0}%)",
            name,
            count,
            reference_decks,
            if reference_decks == 0 {
                0.0
            } else {
                count as f64 / reference_decks as f64 * 100.0
            },
        )
    })
    .collect::<Vec<_>>();

let reference_packages = intelligence
    .references
    .top_pairs(12, minimum_pair_count)
    .into_iter()
    .map(|(cards, count)| {
        format!(
            "{} — {}/{} decks",
            cards.join(" + "),
            count,
            reference_decks,
        )
    })
    .collect::<Vec<_>>();

let reference_combos = intelligence
    .references
    .top_combos(12)
    .into_iter()
    .map(|(cards, result, bracket, count)| {
        format!(
            "{} — {}/{} decks{}{}",
            cards.join(" + "),
            count,
            reference_decks,
            bracket
                .map(|value| format!(" • Spellbook B{value}"))
                .unwrap_or_default(),
            if result.trim().is_empty() {
                String::new()
            } else {
                format!(" • {result}")
            },
        )
    })
    .collect::<Vec<_>>();

decision_trace.push(format!(
    "[REFERENCES] {} matched decks in corpus; {} recurring card pairs; {} recurring complete Spellbook combos.",
    reference_decks,
    intelligence.references.pair_frequency.len(),
    intelligence.references.combo_frequency.len(),
));

progress(format!(
    "[CONSENSUS] Results • core_cards={} • recurring_packages={} • recurring_complete_combos={}",
    reference_core_cards.len(),
    reference_packages.len(),
    reference_combos.len(),
));

if !reference_core_cards.is_empty() {
    decision_trace.push(format!(
        "[CONSENSUS] Most common cards: {}.",
        reference_core_cards
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join(" | "),
    ));
}

if !reference_combos.is_empty() {
    decision_trace.push(format!(
        "[COMBOS] Reference decks repeatedly use: {}.",
        reference_combos
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .join(" | "),
    ));
}

progress(format!(
    "[REPORT] Finalizing Deep Learn report • core coverage={:.1}% • adds={} • cuts={}",
    core_coverage_pct,
    adds.len(),
    cuts.len(),
));

let mut findings = vec![
        format!(
            "Learned normal build identity: {}.",
            if intelligence.normal_themes.is_empty() {
                "No EDHREC tags returned".to_string()
            } else {
                intelligence
                    .normal_themes
                    .iter()
                    .take(6)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" / ")
            }
        ),
        format!(
            "Research confidence: {}%. Learned {} EDHREC card signals and {} matching public reference deck(s).",
            intelligence.confidence,
            intelligence.edhrec_cards.len(),
            intelligence.references.deck_count(),
        ),
        format!(
            "Learned structural targets: {}.",
            intelligence.role_summary(),
        ),
        format!(
            "Current list contains {:.0}% of the top-30 learned commander core.",
            core_coverage_pct,
        ),
        format!(
            "Reference corpus: {} independently sourced matching deck(s).",
            reference_decks,
        ),
        format!(
            "Reference combo scan found {} distinct complete Commander Spellbook line(s) across sampled lists.",
            intelligence.references.combo_frequency.len(),
        ),
    ];

    if let Some(top_add) = adds.first() {
        findings.push(format!(
            "Highest-confidence missing card: {} — {} (confidence {}%).",
            top_add.name,
            top_add.reason,
            top_add.confidence,
        ));
    }

    if let Some(top_cut) = cuts.first() {
        findings.push(format!(
            "Weakest-supported current card: {} — {}.",
            top_cut.name,
            top_cut.reason,
        ));
    }

    for card in adds.iter().take(8) {
        decision_trace.push(format!(
            "[ADD] {} • score {} • confidence {}% • {}.",
            card.name,
            card.score,
            card.confidence,
            card.reason,
        ));
    }

    for card in cuts.iter().take(6) {
        decision_trace.push(format!(
            "[CUT] {} • pressure {} • {}.",
            card.name,
            card.score,
            card.reason,
        ));
    }

    decision_trace.push(
        "[READY] Research model complete. Open SIMULATE to run game testing at the selected intensity."
            .to_string(),
    );

    let testing = None;

    progress(format!(
        "[DEEP] COMPLETE • confidence={} • references={} • EDHREC={} • core_coverage={:.1}% • adds={} • cuts={} • reference_combos={}",
        intelligence.confidence,
        intelligence.references.deck_count(),
        intelligence.edhrec_cards.len(),
        core_coverage_pct,
        adds.len(),
        cuts.len(),
        reference_combos.len(),
    ));

    Ok(DeepLearnReport {
        commander_names: intelligence.commander_names.clone(),
        normal_themes: intelligence.normal_themes.clone(),
        reference_decks: intelligence.references.deck_count(),
        edhrec_signals: intelligence.edhrec_cards.len(),
        core_coverage_pct,
        confidence: intelligence.confidence,
        role_targets: intelligence.role_summary(),
        reference_core_cards,
        reference_packages,
        reference_combos,
        adds,
        cuts,
        findings,
        decision_trace,
        testing,
        sources: intelligence.sources.clone(),
    })
}
