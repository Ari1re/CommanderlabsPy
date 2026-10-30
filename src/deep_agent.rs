use crate::{
    ai::{build_ai_commander_profile, AiCommanderProfile},
    bracket::{estimate_bracket, refine_with_combo_report},
    commander::resolve_generation_commanders,
    data::{Card, CardDb},
    deck::ParsedDeck,
    deep_learn::{
        deep_learn_deck, simulate_deep_learn_games,
        simulate_deep_learn_games_with_runs, DeepLearnReport,
        DeepLearnTestSummary, TestingIntensity,
    },
    generator::{
        generate_ai_upgrade, suggest_generation_themes,
        GeneratedDeck, GenerationBracket,
    },
    spellbook::find_my_combos,
    sources::{merge_user_sources, DeckSource},
};
use anyhow::{anyhow, Result};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationGoal {
    FastWin,
    StaxLock,
}

impl Default for OptimizationGoal {
    fn default() -> Self {
        Self::FastWin
    }
}

impl OptimizationGoal {
    pub fn label(self) -> &'static str {
        match self {
            Self::FastWin => "FAST WIN",
            Self::StaxLock => "STAX LOCK",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::FastWin => {
                "Minimize reliable winning turn while staying exactly inside the chosen bracket."
            }
            Self::StaxLock => {
                "Minimize reliable lock turn; win speed becomes a secondary objective."
            }
        }
    }

}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaxSimulation {
    pub runs: u32,
    pub stax_cards: Vec<String>,
    pub earliest_lock_turn: Option<u8>,
    pub most_likely_lock_turn: Option<u8>,
    pub median_lock_turn: Option<u8>,
    pub lock_turn_counts: Vec<(u8, u64)>,
    pub lock_by_turn_pct: Vec<(u8, f64)>,
    pub no_lock_by_t8_count: u64,
    pub lock_piece_hits: Vec<(String, u64)>,
    pub caveat: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRound {
    pub round: u16,
    pub accepted: bool,
    pub detected_bracket: u8,
    pub objective_score: f64,
    pub summary: String,
    pub added_cards: Vec<String>,
    pub removed_cards: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AgentProgress {
    pub percent: u8,
    pub task: String,
    pub detail: String,
    pub completed: Option<String>,
}

impl AgentProgress {
    fn working(
        percent: u8,
        task: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            percent: percent.min(100),
            task: task.into(),
            detail: detail.into(),
            completed: None,
        }
    }

    fn finished(
        percent: u8,
        task: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        let task = task.into();
        let summary = summary.into();

        Self {
            percent: percent.min(100),
            task: task.clone(),
            detail: summary.clone(),
            completed: Some(format!("{task} — {summary}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDeckIteration {
    pub round: u16,
    pub detected_bracket: u8,
    pub objective_score: f64,
    pub deck: GeneratedDeck,
    pub testing: DeepLearnTestSummary,
    pub stax: Option<StaxSimulation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepAgentResult {
    pub goal: OptimizationGoal,
    pub target_bracket: u8,
    pub ai_profile: AiCommanderProfile,
    pub research: DeepLearnReport,
    pub best: GeneratedDeck,
    pub final_testing: DeepLearnTestSummary,
    pub final_stax: Option<StaxSimulation>,
    pub rounds: Vec<AgentRound>,
    #[serde(default)]
    pub top_iterations: Vec<AgentDeckIteration>,
    pub trace: Vec<String>,
}

pub fn run_optimization_agent<F>(
    starting_deck: &ParsedDeck,
    db: &CardDb,
    target_bracket: u8,
    goal: OptimizationGoal,
    preferred_archetype: Option<String>,
    style_brief: Option<String>,
    user_references: Vec<DeckSource>,
    final_intensity: TestingIntensity,
    exact_final_runs: Option<u32>,
    max_rounds: u16,
    mut progress: F,
) -> Result<DeepAgentResult>
where
    F: FnMut(AgentProgress),
{
    let target_bracket = target_bracket.clamp(1, 5);
    let rounds = max_rounds.max(1);

    progress(AgentProgress::working(
        2,
        "Preparing optimization session",
        format!(
            "{} • target B{} • {} search pass(es)",
            goal.label(),
            target_bracket,
            rounds,
        ),
    ));

    progress(AgentProgress::working(
        5,
        "Researching commander",
        "Collecting commander shell, reference consensus, recurring packages, and verified combo evidence.",
    ));

    let mut research = deep_learn_deck(
        starting_deck,
        db,
        target_bracket,
        |line| {
            progress(AgentProgress::working(
                12,
                "Researching commander",
                line,
            ));
        },
    )?;

    progress(AgentProgress::finished(
        20,
        "Commander research",
        format!(
            "{} reference deck(s), {} EDHREC signal(s), {} recurring combo/package signal(s) collected",
            research.reference_decks,
            research.edhrec_signals,
            research.reference_combos.len(),
        ),
    ));

    if !user_references.is_empty() {
        merge_user_sources(
            &mut research.sources,
            &user_references,
        );

        research.findings.push(format!(
            "{} user-supplied high-priority reference(s) attached to this Agent run.",
            user_references.len(),
        ));

        progress(AgentProgress::finished(
            22,
            "User reference import",
            format!(
                "{} high-priority reference(s) attached",
                user_references.len(),
            ),
        ));
    }

    progress(AgentProgress::working(
        25,
        "Building AI commander model",
        "Grounding the local model with rules, references, community research, deck structure, and the user's build brief.",
    ));

    let ai_profile = build_ai_commander_profile(
        starting_deck,
        db,
        target_bracket,
        goal,
        preferred_archetype.as_deref(),
        style_brief.as_deref(),
        &user_references,
        &research,
        |line| {
            let lower = line.to_ascii_lowercase();

            let pct = if lower.contains("initializing") {
                25
            } else if lower.contains("connected") {
                27
            } else if lower.contains("gathering")
                || lower.contains("searching")
                || lower.contains("checking official")
            {
                30
            } else if lower.contains("building grounded") {
                33
            } else if lower.contains("synthesizing") {
                35
            } else if lower.contains("strategy ready") {
                37
            } else {
                32
            };

            progress(AgentProgress::working(
                pct,
                "Building AI commander model",
                clean_agent_detail(&line),
            ));
        },
    );

    if ai_profile.enabled {
        progress(AgentProgress::finished(
            38,
            "AI commander model",
            format!(
                "recommended {} • {} win package(s) • {} priority card(s) • {} testing request(s)",
                if ai_profile.recommended_archetype.is_empty() {
                    "automatic shell"
                } else {
                    ai_profile.recommended_archetype.as_str()
                },
                ai_profile.win_packages.len(),
                ai_profile.priority_cards.len(),
                ai_profile.testing_requests.len(),
            ),
        ));

        for source in &ai_profile.sources {
            if !research.sources.iter().any(|existing| {
                existing.url.eq_ignore_ascii_case(&source.url)
                    && existing.title.eq_ignore_ascii_case(&source.title)
            }) {
                research.sources.push(source.clone());
            }
        }

        research.findings.push(format!(
            "Local AI ({}) synthesized {} archetype(s), {} win package(s), and {} priority card(s) from the grounded research bundle.",
            ai_profile.model,
            ai_profile.primary_archetypes.len(),
            ai_profile.win_packages.len(),
            ai_profile.priority_cards.len(),
        ));
    } else {
        progress(AgentProgress::finished(
            38,
            "AI commander model",
            format!(
                "local AI unavailable; continuing with deterministic optimizer ({})",
                ai_profile.fallback_reason,
            ),
        ));

        research.findings.push(format!(
            "Local AI unavailable; deterministic AI Agent used. {}",
            ai_profile.fallback_reason,
        ));
    }

    // Search iterations intentionally use QUICK samples so the agent can test
    // several candidate revisions. The winner is re-tested at the user's
    // selected intensity before being returned.
    progress(AgentProgress::working(
        42,
        "Measuring current deck",
        "Running the baseline quick simulation so candidate decks have a fair comparison point.",
    ));

    let baseline_testing = simulate_deep_learn_games(
        starting_deck,
        db,
        TestingIntensity::Quick,
        |line| {
            progress(AgentProgress::working(
                46,
                "Measuring current deck",
                clean_agent_detail(&line),
            ));
        },
    )?;

    let baseline_stax = if goal == OptimizationGoal::StaxLock {
        Some(simulate_stax_lock(
            starting_deck,
            db,
            TestingIntensity::Quick.goldfish_runs(),
        )?)
    } else {
        None
    };

    let baseline_bracket = detect_bracket(starting_deck, db)?;

    progress(AgentProgress::finished(
        50,
        "Baseline simulation",
        format!(
            "detected B{} • earliest {} • most likely {} • T5 {:.2}%",
            baseline_bracket,
            baseline_testing
                .goldfish
                .earliest_combo_turn
                .map(|turn| format!("T{turn}"))
                .unwrap_or_else(|| "none by T8".to_string()),
            baseline_testing
                .goldfish
                .most_likely_combo_turn
                .map(|turn| format!("T{turn}"))
                .unwrap_or_else(|| "none".to_string()),
            baseline_testing.goldfish.combo_by_t5_pct,
        ),
    ));


    let mut best_score = if baseline_bracket == target_bracket {
        objective_score(
            goal,
            &baseline_testing,
            baseline_stax.as_ref(),
        )
    } else {
        // A deck in the wrong bracket is never allowed to beat a valid
        // candidate merely because its raw speed is better.
        -1_000_000.0
    };

    let commander_specification = starting_deck
        .commanders
        .iter()
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>()
        .join(" + ");

    let commander_cards =
        resolve_generation_commanders(&commander_specification, db)?;

    let candidate_themes = if goal == OptimizationGoal::StaxLock {
        vec![Some("Stax".to_string())]
    } else {
        let mut themes = Vec::<Option<String>>::new();

        if let Some(archetype) = preferred_archetype
            .clone()
            .filter(|value| !value.trim().is_empty())
        {
            themes.push(Some(archetype.trim().to_string()));
        }

        if let Some(style) = style_brief
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            let normalized = style.trim();
            let preset_theme = [
                "Combo",
                "Control",
                "Midrange",
                "Value",
                "Reanimator",
                "Tribal",
                "Stax",
                "Battlecruiser",
            ]
            .iter()
            .find(|candidate| {
                normalized
                    .to_ascii_lowercase()
                    .starts_with(&candidate.to_ascii_lowercase())
            });

            if let Some(theme) = preset_theme {
                if !themes.iter().any(|existing| {
                    existing
                        .as_ref()
                        .map(|name| name.eq_ignore_ascii_case(theme))
                        .unwrap_or(false)
                }) {
                    themes.push(Some((*theme).to_string()));
                }
            }
        }

        if ai_profile.enabled
            && !ai_profile.recommended_archetype.trim().is_empty()
            && !themes.iter().any(|existing| {
                existing
                    .as_ref()
                    .map(|name| name.eq_ignore_ascii_case(
                        &ai_profile.recommended_archetype
                    ))
                    .unwrap_or(false)
            })
        {
            themes.push(Some(
                ai_profile.recommended_archetype.clone(),
            ));
        }

        for archetype in ai_profile
            .primary_archetypes
            .iter()
            .take(4)
        {
            if !themes.iter().any(|existing| {
                existing
                    .as_ref()
                    .map(|name| name.eq_ignore_ascii_case(archetype))
                    .unwrap_or(false)
            }) {
                themes.push(Some(archetype.clone()));
            }
        }

        for theme in suggest_generation_themes(
            &commander_cards,
            db,
        )?
        .into_iter()
        .take(rounds.min(4) as usize)
        {
            if themes.iter().any(|existing| {
                existing
                    .as_ref()
                    .map(|name| name.eq_ignore_ascii_case(&theme.name))
                    .unwrap_or(false)
            }) {
                continue;
            }

            themes.push(Some(theme.name));
        }

        if themes.is_empty() {
            themes.push(None);
        }

        themes
    };

    progress(AgentProgress::finished(
        52,
        "Candidate plan",
        format!(
            "testing {} candidate build path(s): {}",
            candidate_themes.len(),
            candidate_themes
                .iter()
                .map(|theme| {
                    theme.clone().unwrap_or_else(|| "Normal shell".to_string())
                })
                .collect::<Vec<_>>()
                .join(" / "),
        ),
    ));

    let mut current = starting_deck.clone();
    let mut best_generated: Option<GeneratedDeck> = None;
    let mut round_reports = Vec::new();
    let mut candidate_iterations = Vec::<AgentDeckIteration>::new();
    let mut trace = vec![format!(
        "[BASELINE] detected B{} • objective score {:.2}{}.",
        baseline_bracket,
        best_score,
        if baseline_bracket == target_bracket {
            ""
        } else {
            " • excluded from comparison because bracket is wrong"
        },
    )];

    for round in 1..=rounds {
        let preferred_theme = candidate_themes[
            ((round - 1) as usize) % candidate_themes.len()
        ]
        .as_deref();

        let round_start_pct =
            53 + ((((round - 1) as u32 * 31) / rounds as u32) as u8);
        let round_mid_pct =
            53 + (((((round - 1) as u32 * 31) + 15) / rounds as u32) as u8);
        let round_end_pct =
            53 + (((round as u32 * 31) / rounds as u32) as u8);

        progress(AgentProgress::working(
            round_start_pct,
            format!("Building candidate {round}/{rounds}"),
            format!(
                "Theme: {} • enforcing Bracket {}",
                preferred_theme.unwrap_or("Normal shell"),
                target_bracket,
            ),
        ));

        let previous = current.clone();

        let candidate = generate_ai_upgrade(
            &current,
            GenerationBracket::new(target_bracket),
            preferred_theme,
            &ai_profile.priority_cards,
            db,
            |update| {
                let local_span =
                    round_mid_pct.saturating_sub(round_start_pct);
                let pct = round_start_pct.saturating_add(
                    ((local_span as u16 * update.percent as u16) / 100)
                        as u8,
                );

                progress(AgentProgress::working(
                    pct,
                    format!("Building candidate {round}/{rounds}"),
                    format!("{} — {}", update.step, update.detail),
                ));
            },
        )?;

        let detected_bracket = detect_bracket(&candidate.deck, db)?;

        if detected_bracket != target_bracket {
            return Err(anyhow!(
                "Agent candidate violated bracket lock: requested B{}, detected B{}.",
                target_bracket,
                detected_bracket,
            ));
        }

        progress(AgentProgress::working(
            round_mid_pct,
            format!("Testing candidate {round}/{rounds}"),
            "Running quick comparison simulations against the current best deck.",
        ));

        let candidate_testing = simulate_deep_learn_games(
            &candidate.deck,
            db,
            TestingIntensity::Quick,
            |line| {
                progress(AgentProgress::working(
                    round_mid_pct,
                    format!("Testing candidate {round}/{rounds}"),
                    clean_agent_detail(&line),
                ));
            },
        )?;

        let candidate_stax = if goal == OptimizationGoal::StaxLock {
            Some(simulate_stax_lock(
                &candidate.deck,
                db,
                TestingIntensity::Quick.goldfish_runs(),
            )?)
        } else {
            None
        };

        let score = objective_score(
            goal,
            &candidate_testing,
            candidate_stax.as_ref(),
        );

        let (added_cards, removed_cards) =
            deck_diff(&previous, &candidate.deck);

        let accepted = score > best_score + 0.01;




        let summary = match goal {
            OptimizationGoal::FastWin => format!(
                "{} • earliest {} • by T4 {:.1}% • by T5 {:.1}% • score {:.2}",
                if accepted { "ACCEPT" } else { "REJECT" },
                candidate_testing
                    .goldfish
                    .earliest_combo_turn
                    .map(|turn| format!("T{turn}"))
                    .unwrap_or_else(|| "none by T8".to_string()),
                candidate_testing.goldfish.combo_by_t4_pct,
                candidate_testing.goldfish.combo_by_t5_pct,
                score,
            ),
            OptimizationGoal::StaxLock => {
                let stax = candidate_stax.as_ref().expect("stax result");
                format!(
                    "{} • earliest lock {} • by T3 {:.1}% • by T4 {:.1}% • score {:.2}",
                    if accepted { "ACCEPT" } else { "REJECT" },
                    stax.earliest_lock_turn
                        .map(|turn| format!("T{turn}"))
                        .unwrap_or_else(|| "none by T8".to_string()),
                    cumulative_pct(&stax.lock_by_turn_pct, 3),
                    cumulative_pct(&stax.lock_by_turn_pct, 4),
                    score,
                )
            }
        };

        trace.push(format!("[ROUND {round}] {summary}"));

        if !added_cards.is_empty() {
            trace.push(format!(
                "[ROUND {round}] Added: {}.",
                added_cards.iter().take(16).cloned().collect::<Vec<_>>().join(", "),
            ));
        }

        if !removed_cards.is_empty() {
            trace.push(format!(
                "[ROUND {round}] Removed: {}.",
                removed_cards.iter().take(16).cloned().collect::<Vec<_>>().join(", "),
            ));
        }

        let added_count = added_cards.len();
        let removed_count = removed_cards.len();

        candidate_iterations.push(AgentDeckIteration {
            round,
            detected_bracket,
            objective_score: score,
            deck: candidate.clone(),
            testing: candidate_testing.clone(),
            stax: candidate_stax.clone(),
        });

        round_reports.push(AgentRound {
            round,
            accepted,
            detected_bracket,
            objective_score: score,
            summary,
            added_cards,
            removed_cards,
        });

        if accepted {
            best_score = score;
            current = candidate.deck.clone();
            best_generated = Some(candidate);
        }

        progress(AgentProgress::finished(
            round_end_pct,
            format!("Candidate {round}/{rounds}"),
            format!(
                "{} • score {:.2} • detected B{} • {} add(s) / {} cut(s)",
                if accepted {
                    "accepted as new best"
                } else {
                    "rejected; previous best retained"
                },
                score,
                detected_bracket,
                added_count,
                removed_count,
            ),
        ));
    }

    let mut best = best_generated.ok_or_else(|| {
        anyhow!(
            "Agent tested all configured candidates but none beat the current deck's objective score inside Bracket {}. The current list is already better than the generated candidates under this test model.",
            target_bracket,
        )
    })?;

    best.deck.name = format!(
        "{} - Agent {} B{}",
        starting_deck.name,
        match goal {
            OptimizationGoal::FastWin => "MinMax",
            OptimizationGoal::StaxLock => "Stax",
        },
        target_bracket,
    );

    progress(AgentProgress::working(
        88,
        "Final validation",
        format!(
            "Re-testing the best accepted deck at {} intensity.",
            final_intensity.label(),
        ),
    ));

    let final_testing = simulate_deep_learn_games_with_runs(
        &best.deck,
        db,
        final_intensity,
        exact_final_runs,
        |line| {
            progress(AgentProgress::working(
                94,
                "Final validation",
                clean_agent_detail(&line),
            ));
        },
    )?;

    let final_stax = if goal == OptimizationGoal::StaxLock {
        progress(AgentProgress::working(
            96,
            "Final lock validation",
            format!(
                "Running {} lock-sequence simulations.",
                exact_final_runs
                    .unwrap_or_else(|| final_intensity.goldfish_runs()),
            ),
        ));

        Some(simulate_stax_lock(
            &best.deck,
            db,
            exact_final_runs
                .unwrap_or_else(|| final_intensity.goldfish_runs()),
        )?)
    } else {
        None
    };

    let final_bracket = detect_bracket(&best.deck, db)?;

    progress(AgentProgress::finished(
        99,
        "Final validation",
        format!(
            "B{} validated • {} games • objective score {:.2} • {} verified combo line(s) checked",
            final_bracket,
            final_testing.goldfish.runs,
            objective_score(
                goal,
                &final_testing,
                final_stax.as_ref(),
            ),
            final_testing.goldfish.combo_lines_checked,
        ),
    ));

    if final_bracket != target_bracket {
        return Err(anyhow!(
            "Final agent deck no longer validates at requested B{}; detected B{}.",
            target_bracket,
            final_bracket,
        ));
    }

    best.plan.notes.push(format!(
        "AI Agent objective: {}. Optimized over {} accepted/rejected search round(s), then re-tested with {}.",
        goal.label(),
        round_reports.len(),
        exact_final_runs
            .map(|runs| format!("{runs} user-requested simulations"))
            .unwrap_or_else(|| {
                format!("{} intensity", final_intensity.label())
            }),
    ));

    if let Some(style) = style_brief.as_deref() {
        best.plan.notes.push(format!(
            "User-selected deck style: {}",
            style,
        ));
    }

    if ai_profile.enabled {
        best.plan.notes.push(format!(
            "Local AI model {} recommended archetype '{}' from grounded Commander Lab/reference/community evidence.",
            ai_profile.model,
            if ai_profile.recommended_archetype.is_empty() {
                "AUTO"
            } else {
                ai_profile.recommended_archetype.as_str()
            },
        ));

        for note in ai_profile
            .gameplan
            .iter()
            .take(8)
        {
            best.plan.notes.push(format!(
                "AI gameplan: {note}"
            ));
        }

        for request in ai_profile.testing_requests.iter().take(6) {
            best.plan.notes.push(format!(
                "User testing preference: {request}"
            ));
        }

        for preference in ai_profile
            .feedback_preferences
            .iter()
            .take(6)
        {
            best.plan.notes.push(format!(
                "User feedback preference: {preference}"
            ));
        }

        for source in &ai_profile.sources {
            if !best.plan.sources.iter().any(|existing| {
                existing.url.eq_ignore_ascii_case(&source.url)
                    && existing.title.eq_ignore_ascii_case(&source.title)
            }) {
                best.plan.sources.push(source.clone());
            }
        }
    }

    for source in &user_references {
        if !best.plan.sources.iter().any(|existing| {
            existing.url.eq_ignore_ascii_case(&source.url)
                && existing.title.eq_ignore_ascii_case(&source.title)
        }) {
            best.plan.sources.push(source.clone());
        }
    }

    match goal {
        OptimizationGoal::FastWin => {
            best.plan.notes.push(format!(
                "Final speed: earliest observed {} • most likely {} • combo by T4 {:.2}% • T5 {:.2}% • T6 {:.2}%.",
                final_testing
                    .goldfish
                    .earliest_combo_turn
                    .map(|turn| format!("T{turn}"))
                    .unwrap_or_else(|| "none by T8".to_string()),
                final_testing
                    .goldfish
                    .most_likely_combo_turn
                    .map(|turn| format!("T{turn}"))
                    .unwrap_or_else(|| "none".to_string()),
                final_testing.goldfish.combo_by_t4_pct,
                final_testing.goldfish.combo_by_t5_pct,
                final_testing.goldfish.combo_by_t6_pct,
            ));
        }
        OptimizationGoal::StaxLock => {
            if let Some(stax) = &final_stax {
                best.plan.notes.push(format!(
                    "Final lock speed: earliest {} • most likely {} • lock by T3 {:.2}% • T4 {:.2}% • T5 {:.2}%.",
                    stax.earliest_lock_turn
                        .map(|turn| format!("T{turn}"))
                        .unwrap_or_else(|| "none by T8".to_string()),
                    stax.most_likely_lock_turn
                        .map(|turn| format!("T{turn}"))
                        .unwrap_or_else(|| "none".to_string()),
                    cumulative_pct(&stax.lock_by_turn_pct, 3),
                    cumulative_pct(&stax.lock_by_turn_pct, 4),
                    cumulative_pct(&stax.lock_by_turn_pct, 5),
                ));
            }
        }
    }

    trace.push(format!(
        "[FINAL] B{} validated • {} objective complete.",
        final_bracket,
        goal.label(),
    ));

        candidate_iterations.sort_by(|a, b| {
        b.objective_score
            .partial_cmp(&a.objective_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.round.cmp(&b.round))
    });
    candidate_iterations.truncate(10);

Ok(DeepAgentResult {
        goal,
        target_bracket,
        ai_profile,
        research,
        best,
        final_testing,
        final_stax,
        rounds: round_reports,
        top_iterations: candidate_iterations,
        trace,
    })
}

fn detect_bracket(deck: &ParsedDeck, db: &CardDb) -> Result<u8> {
    let mut estimate = estimate_bracket(deck, db)?;

    if let Ok(combos) = find_my_combos(deck) {
        refine_with_combo_report(&mut estimate, &combos);
    }

    Ok(estimate.bracket)
}

fn clean_agent_detail(line: &str) -> String {
    let mut value = line.trim().to_string();

    // Internal workers use bracketed prefixes for diagnostics. The Agent UI
    // should describe the work rather than expose those implementation tags.
    while value.starts_with('[') {
        let Some(end) = value.find(']') else {
            break;
        };

        value = value[end + 1..].trim().to_string();
    }

    value
        .replace(" • ", " — ")
        .chars()
        .take(180)
        .collect()
}


fn objective_score(
    goal: OptimizationGoal,
    testing: &DeepLearnTestSummary,
    stax: Option<&StaxSimulation>,
) -> f64 {
    match goal {
        OptimizationGoal::FastWin => {
            let earliest =
                testing.goldfish.earliest_combo_turn.unwrap_or(9);
            let likely =
                testing.goldfish.most_likely_combo_turn.unwrap_or(9);
            let median =
                testing.goldfish.median_combo_turn.unwrap_or(9);

            (9 - earliest) as f64 * 900.0
                + (9 - likely) as f64 * 350.0
                + (9 - median) as f64 * 180.0
                + testing.goldfish.combo_by_t3_pct * 16.0
                + testing.goldfish.combo_by_t4_pct * 11.0
                + testing.goldfish.combo_by_t5_pct * 7.0
                + testing.goldfish.combo_by_t6_pct * 3.0
                + testing.goldfish.keepable_pct
        }
        OptimizationGoal::StaxLock => {
            let Some(stax) = stax else {
                return -1.0;
            };

            let earliest = stax.earliest_lock_turn.unwrap_or(9);
            let likely = stax.most_likely_lock_turn.unwrap_or(9);
            let median = stax.median_lock_turn.unwrap_or(9);

            (9 - earliest) as f64 * 900.0
                + (9 - likely) as f64 * 400.0
                + (9 - median) as f64 * 200.0
                + cumulative_pct(&stax.lock_by_turn_pct, 2) * 20.0
                + cumulative_pct(&stax.lock_by_turn_pct, 3) * 15.0
                + cumulative_pct(&stax.lock_by_turn_pct, 4) * 9.0
                + cumulative_pct(&stax.lock_by_turn_pct, 5) * 4.0
                + testing.goldfish.combo_by_t6_pct * 0.5
        }
    }
}

fn cumulative_pct(items: &[(u8, f64)], turn: u8) -> f64 {
    items
        .iter()
        .find(|(candidate, _)| *candidate == turn)
        .map(|(_, value)| *value)
        .unwrap_or(0.0)
}

fn deck_diff(
    before: &ParsedDeck,
    after: &ParsedDeck,
) -> (Vec<String>, Vec<String>) {
    let before_names = before
        .main
        .iter()
        .map(|entry| entry.name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    let after_names = after
        .main
        .iter()
        .map(|entry| entry.name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    let added = after
        .main
        .iter()
        .filter(|entry| {
            !before_names.contains(&entry.name.to_ascii_lowercase())
        })
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>();

    let removed = before
        .main
        .iter()
        .filter(|entry| {
            !after_names.contains(&entry.name.to_ascii_lowercase())
        })
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>();

    (added, removed)
}

#[derive(Debug, Clone)]
struct StaxCard {
    name: String,
    cmc: f64,
    land: bool,
    ramp: bool,
    lock_weight: u8,
}

pub fn simulate_stax_lock(
    deck: &ParsedDeck,
    db: &CardDb,
    runs: u32,
) -> Result<StaxSimulation> {
    if runs == 0 {
        return Err(anyhow!("Stax simulation needs at least one run."));
    }

    let mut library = Vec::<StaxCard>::new();
    let mut commander_stax = Vec::<StaxCard>::new();
    let mut stax_names = HashSet::<String>::new();

    for entry in &deck.commanders {
        let Some(card) = db.get_card(&entry.name)? else {
            continue;
        };

        let lock_weight = stax_lock_weight(&card);

        if lock_weight > 0 {
            let name = card.name.clone();
            stax_names.insert(name.clone());
            commander_stax.push(StaxCard {
                name,
                cmc: card.cmc,
                land: false,
                ramp: false,
                lock_weight,
            });
        }
    }

    for entry in &deck.main {
        let Some(card) = db.get_card(&entry.name)? else {
            continue;
        };

        let type_line = card.type_line.to_ascii_lowercase();
        let land = type_line.contains("land");
        let ramp = crate::analysis::classify_card(&card).ramp;
        let lock_weight = stax_lock_weight(&card);

        if lock_weight > 0 {
            stax_names.insert(card.name.clone());
        }

        for _ in 0..entry.quantity {
            library.push(StaxCard {
                name: card.name.clone(),
                cmc: card.cmc,
                land,
                ramp,
                lock_weight,
            });
        }
    }

    if library.len() < 40 {
        return Err(anyhow!(
            "Not enough recognized main-deck cards for Stax simulation."
        ));
    }

    let mut rng = rand::rng();
    let mut turn_counts = [0u64; 9];
    let mut piece_hits = HashMap::<String, u64>::new();

    for _ in 0..runs {
        let mut shuffled = library.clone();
        shuffled.shuffle(&mut rng);

        let opening_size = 7usize.min(shuffled.len());
        let mut hand =
            shuffled.drain(..opening_size).collect::<Vec<_>>();
        let mut command_zone = commander_stax.clone();
        let mut lands = 0u8;
        let mut ramp_bonus = 0u8;
        let mut lock_pressure = 0u8;
        let mut lock_turn = None;

        for turn in 1u8..=8 {
            if let Some(card) = shuffled.pop() {
                hand.push(card);
            }

            if let Some(index) = hand.iter().position(|card| card.land) {
                hand.remove(index);
                lands = lands.saturating_add(1);
            }

            let mut mana = lands.saturating_add(ramp_bonus);

            // Cast one affordable ramp piece before the lock piece. This is a
            // deliberate approximation rather than a complete priority engine.
            if let Some(index) = hand
                .iter()
                .enumerate()
                .filter(|(_, card)| {
                    card.ramp
                        && !card.land
                        && card.cmc <= mana as f64
                        && card.cmc <= 3.0
                })
                .min_by(|a, b| a.1.cmc.total_cmp(&b.1.cmc))
                .map(|(index, _)| index)
            {
                let card = hand.remove(index);
                let cost = card.cmc.ceil() as u8;
                mana = mana.saturating_sub(cost.min(mana));
                ramp_bonus = ramp_bonus.saturating_add(1);
            }

            loop {
                let hand_choice = hand
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| {
                        card.lock_weight > 0
                            && card.cmc <= mana as f64
                    })
                    .max_by(|a, b| {
                        a.1.lock_weight
                            .cmp(&b.1.lock_weight)
                            .then_with(|| b.1.cmc.total_cmp(&a.1.cmc))
                    })
                    .map(|(index, card)| {
                        (false, index, card.lock_weight, card.cmc)
                    });

                let commander_choice = command_zone
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| card.cmc <= mana as f64)
                    .max_by(|a, b| {
                        a.1.lock_weight
                            .cmp(&b.1.lock_weight)
                            .then_with(|| b.1.cmc.total_cmp(&a.1.cmc))
                    })
                    .map(|(index, card)| {
                        (true, index, card.lock_weight, card.cmc)
                    });

                let choice = match (hand_choice, commander_choice) {
                    (Some(hand), Some(commander)) => {
                        if commander.2 > hand.2
                            || (commander.2 == hand.2
                                && commander.3 < hand.3)
                        {
                            Some(commander)
                        } else {
                            Some(hand)
                        }
                    }
                    (Some(hand), None) => Some(hand),
                    (None, Some(commander)) => Some(commander),
                    (None, None) => None,
                };

                let Some((from_command_zone, index, _, _)) = choice else {
                    break;
                };

                let card = if from_command_zone {
                    command_zone.remove(index)
                } else {
                    hand.remove(index)
                };

                let cost = card.cmc.ceil() as u8;
                mana = mana.saturating_sub(cost.min(mana));

                lock_pressure =
                    lock_pressure.saturating_add(card.lock_weight);

                *piece_hits.entry(card.name).or_default() += 1;

                // 3 pressure approximates either one hard lock piece or a
                // pair/stack of taxing/restrictive effects.
                if lock_pressure >= 3 {
                    lock_turn = Some(turn);
                    break;
                }
            }

            if lock_turn.is_some() {
                break;
            }
        }

        if let Some(turn) = lock_turn {
            turn_counts[turn as usize] += 1;
        }
    }

    let pct = |count: u64| -> f64 {
        count as f64 / runs as f64 * 100.0
    };

    let lock_turn_counts = (1u8..=8)
        .map(|turn| (turn, turn_counts[turn as usize]))
        .collect::<Vec<_>>();


    let mut cumulative = 0u64;
    let lock_by_turn_pct = lock_turn_counts
        .iter()
        .map(|(turn, count)| {
            cumulative += *count;
            (*turn, pct(cumulative))
        })
        .collect::<Vec<_>>();

    let earliest_lock_turn = (1u8..=8)
        .find(|turn| turn_counts[*turn as usize] > 0);

    let most_likely_lock_turn = (1u8..=8)
        .max_by_key(|turn| turn_counts[*turn as usize])
        .filter(|turn| turn_counts[*turn as usize] > 0);

    let total_locks = turn_counts.iter().sum::<u64>();

    let median_lock_turn = if total_locks == 0 {
        None
    } else {
        let threshold = (total_locks + 1) / 2;
        let mut seen = 0u64;
        let mut median = None;

        for turn in 1u8..=8 {
            seen += turn_counts[turn as usize];

            if seen >= threshold {
                median = Some(turn);
                break;
            }
        }

        median
    };

    let no_lock_by_t8_count =
        runs as u64 - total_locks.min(runs as u64);

    let mut lock_piece_hits = piece_hits.into_iter().collect::<Vec<_>>();
    lock_piece_hits.sort_by(|a, b| {
        b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0))
    });

    let mut stax_cards = stax_names.into_iter().collect::<Vec<_>>();
    stax_cards.sort();

    Ok(StaxSimulation {
        runs,
        stax_cards,
        earliest_lock_turn,
        most_likely_lock_turn,
        median_lock_turn,
        lock_turn_counts,
        lock_by_turn_pct,
        no_lock_by_t8_count,
        lock_piece_hits,
        caveat: "Lock timing is a heuristic pressure model. It recognizes common rule-of-law, tax, denial, untap/search/cast restrictions and treats 3 pressure as a functional lock. It does not simulate opponents or every Magic rules interaction.".to_string(),
    })
}

fn stax_lock_weight(card: &Card) -> u8 {
    let oracle = card.oracle_text.to_ascii_lowercase();
    let name = card.name.to_ascii_lowercase();

    if [
        "stasis",
        "winter orb",
        "static orb",
        "rule of law",
        "drannith magistrate",
        "collector ouphe",
        "null rod",
        "rest in peace",
        "grafdigger's cage",
        "back to basics",
    ]
    .iter()
    .any(|needle| name.contains(needle))
    {
        return 3;
    }

    if oracle.contains("players can't cast")
        || oracle.contains("can't cast spells")
        || oracle.contains("can't activate abilities")
        || oracle.contains("players can't untap")
        || oracle.contains("doesn't untap during")
        || oracle.contains("skip their untap")
        || oracle.contains("can't search libraries")
        || oracle.contains("can't search their libraries")
        || oracle.contains("can't cast more than one spell")
        || oracle.contains("each player can't cast more than one")
    {
        return 3;
    }

    if oracle.contains("spells cost {2} more")
        || oracle.contains("spells cost {1} more")
        || oracle.contains("noncreature spells cost")
        || oracle.contains("creature spells cost")
        || oracle.contains("artifacts enter the battlefield tapped")
        || oracle.contains("lands enter the battlefield tapped")
        || oracle.contains("nonbasic lands")
        || oracle.contains("activated abilities of artifacts can't")
        || oracle.contains("activated abilities of creatures can't")
        || oracle.contains("each opponent can't cast")
    {
        return 2;
    }

    if oracle.contains("unless that player pays {1}")
        || oracle.contains("unless its controller pays {1}")
        || oracle.contains("can't attack you")
        || oracle.contains("can't block")
        || oracle.contains("doesn't untap")
        || oracle.contains("one or more cards from a graveyard")
    {
        return 1;
    }

    0
}
