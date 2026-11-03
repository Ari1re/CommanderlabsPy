//! Commander and Pauper Commander deck generation (both engines).
//!
//! How it fits: `start_generate` in `api.rs` calls
//! [`generate_deck_with_options`]. Canadian Highlander uses `highlander.rs`
//! instead. Settings → Deck generation picks the engine
//! ([`GenerationOptions::engine`]):
//! - Peer-based: the original generator, mostly what EDHREC and other
//!   players' lists run.
//! - Deep learning (default): the same pipeline with the steps in
//!   `deep_generation.rs` switched on (commander mechanics, evidence tiers,
//!   power-staple floor, Spellbook combos, the build report).
//!
//! The pipeline, in order (progress percentages match the log):
//! 1. Resolve the commander(s) and colors; build the legal/budget filter.
//! 2. Themes: the player's, else AI suggestions, else (deep) the commander's
//!    own mechanics and EDHREC's top themes.
//! 3. Research (`intelligence.rs`): EDHREC/PDHREC card signals, reference
//!    decklists (`references.rs`), learned role targets.
//! 4. Candidate pool: top 15k cards + EDHREC + reference cards (+ every legal
//!    common for PDH), rules-text search finds, AI key cards.
//! 5. Win packages: recurring reference combos, Spellbook combos (deep),
//!    built-in combo templates.
//! 6. Spells: (deep) power-staple floor, then a greedy loop picking the best
//!    score each step (role needs, theme, engine, EDHREC, evidence tiers).
//! 7. Mana base, then combo audit, bracket lock (power up/down), integrity
//!    gate, budget fit, game plan text and sources.
//!
//! Things to work on:
//! - ~3,900 lines with one ~1,500-line function. Each numbered step above
//!   should become its own function taking a shared `BuildState`.
//! - Scores are hand-tuned constants added together (theme_need 90,
//!   engine_need 180, EDHREC 150…). They double-count the same evidence and
//!   are hard to reason about. A single scoring function with named weights
//!   (and a debug dump of each card's score breakdown) would make tuning
//!   sane.
//! - Deep generation still under-uses Game Changers (Drana at Bracket 3 got
//!   1 of 3 allowed) and drops big single threats (Gray Merchant, Sheoldred)
//!   that peer-based keeps.
//! - The bracket lock can quietly undo choices made earlier; log what it
//!   swapped and why in the report.
//! - The first deep run for a commander takes ~90 s while references load.
//!   Show the reference crawl as its own progress step.

use crate::{
    analysis::{
        classify_card, has_copy_rule_override, identity_subset, CardRoles,
    },
    archetype::{detect_deck_tags, tag_fit_score, tag_names_for_card, theme_list, ArchetypeTag},
    bracket::{estimate_bracket, refine_with_combo_report, BracketEstimate},
    commander::{combined_identity, resolve_generation_commanders},
    budget::Budget,
    data::{Card, CardDb},
    deck::{DeckEntry, ParsedDeck},
    edhrec::{commander_themes, EdhrecCardSignal},
    logging,
    intelligence::{
        learn_commander_intelligence, LearnedRoleTargets,
    },
    spellbook::find_my_combos,
    sources::DeckSource,
    format::Format,
    oracle_search::{deep_search, DeepFind, ThemeMatcher},
};
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};

/// A combo the generator put in the deck on purpose, with where it came
/// from ("Observed in 4/32 reference decks", "Commander Spellbook…").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedCombo {
    pub cards: Vec<String>,
    pub result: String,
    pub source: String,
}

/// Everything saved next to a generated deck as `<deck>.plan.json` (and
/// rendered to `<deck>.plan.md`): commanders, colors, the deck's main
/// themes, a plan summary, its combos, build notes and sources. Suggestions,
/// the guide and the chat read `primary_tags` from here later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GamePlan {
    pub commanders: Vec<String>,
    pub identity: String,
    pub primary_tags: Vec<String>,
    pub summary: String,
    pub combos: Vec<GeneratedCombo>,
    pub notes: Vec<String>,
    pub sources: Vec<DeckSource>,
    /// How the deck was built (deep learning generation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<crate::deep_generation::GenerationReport>,
    /// How decks with this commander usually win, and which of those lines
    /// this deck runs (see win_lines.rs).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub win_lines: Vec<crate::win_lines::WinLine>,
}

/// What the generators return: the decklist plus its game plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedDeck {
    pub deck: ParsedDeck,
    pub plan: GamePlan,
}


/// One play-style option on the Build screen, with a score for ordering
/// and a short "why" ("EDHREC tag #2 • 4,120 decks").
#[derive(Debug, Clone, serde::Serialize)]
pub struct GenerationTheme {
    pub name: String,
    pub score: i32,
    pub reason: String,
}

/// One progress update during generation (percent, step, detail); the job
/// turns these into the log lines the Build screen shows.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GenerationProgress {
    pub percent: u16,
    pub step: String,
    pub detail: String,
}

impl GenerationProgress {
    /// Public constructor for other modules (highlander.rs) that report
    /// progress the same way.
    pub fn new_pub(percent: u16, step: &str, detail: impl Into<String>) -> Self {
        Self::new(percent, step, detail)
    }

    /// Clamps the percent so a stray step can't push the bar past 100.
    fn new(percent: u16, step: &str, detail: impl Into<String>) -> Self {
        Self {
            percent: percent.min(100),
            step: step.to_string(),
            detail: detail.into(),
        }
    }
}

/// The play styles offered on the Build screen for a commander.
/// Ranked: EDHREC's own theme tags first, then themes the commander's text
/// suggests, then a small bump for themes with lots of playable cards in
/// these colors. Falls back to a few generic themes if nothing is found.
pub fn suggest_generation_themes(
    commanders: &[Card],
    db: &CardDb,
) -> Result<Vec<GenerationTheme>> {
    use std::collections::HashMap;

    let identity = combined_identity(commanders);
    let mut scores =
        HashMap::<String, (i32, String)>::new();

    // EDHREC gets first priority. Its commander pages explicitly expose the
    // ways people are actually building that commander.
    // 1. EDHREC tags, ranked by EDHREC's own order and deck counts.
    let edhrec = commander_themes(commanders, 10);

    let edhrec_count = edhrec.len();

    for theme in edhrec {
        let score = 10_000
            - (theme.rank as i32 * 100)
            + (theme.deck_count.min(5_000) as i32 / 10);

        scores.insert(
            theme.name.clone(),
            (
                score,
                format!(
                    "EDHREC tag #{} • {} deck(s)",
                    theme.rank,
                    theme.deck_count
                ),
            ),
        );
    }

    // Local commander-text inference supplements EDHREC, rather than
    // replacing it. It is especially useful if EDHREC is unavailable or for
    // a brand-new commander with little submitted data.
    // 2. Themes read from the commander's own text (`archetype.rs` rules).
    for commander in commanders {
        for tag in tag_names_for_card(commander) {
            let slot = scores
                .entry(tag.clone())
                .or_insert_with(|| {
                    (
                        if edhrec_count >= 10 { 100 } else { 1_500 },
                        format!(
                            "Local inference: {} directly supports {}",
                            commander.name,
                            tag
                        ),
                    )
                });

            if slot.1.starts_with("EDHREC") {
                slot.0 += 30;
            } else {
                slot.0 += 180;
            }
        }
    }

    // Reinforce themes that have real legal card depth in this commander's
    // colors. This keeps obscure/incidental tags from outranking archetypes
    // with substantial playable support.
    // 3. Card depth: themes with many popular legal cards get small bumps.
    for card in db.top_ranked_cards(5_000)? {
        if !card.commander_legal
            || !identity_subset(&card.color_identity, &identity)
            || card.type_line.to_ascii_lowercase().contains("land")
        {
            continue;
        }

        for tag in tag_names_for_card(&card) {
            if let Some(slot) = scores.get_mut(&tag) {
                slot.0 += match card.edhrec_rank {
                    Some(rank) if rank <= 1_000 => 5,
                    Some(rank) if rank <= 5_000 => 3,
                    _ => 1,
                };
            }
        }
    }

    if scores.is_empty() {
        for fallback in [
            "Big Mana",
            "Legends",
            "Artifacts",
            "Spellslinger",
            "Tokens",
            "Control",
        ] {
            scores.insert(
                fallback.to_string(),
                (
                    10,
                    "Local fallback theme".to_string(),
                ),
            );
        }
    }

    let mut themes = scores
        .into_iter()
        .map(|(name, (score, reason))| GenerationTheme {
            name,
            score,
            reason,
        })
        .collect::<Vec<_>>();

    themes.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.name.cmp(&b.name))
    });

    // Keep enough EDHREC themes to expose broad commander build paths.
    themes.truncate(12);

    Ok(themes)
}

/// The bracket (1–5) a build aims for, with the rules that follow from it:
/// how many Game Changers, combos, tutors and fast mana it should have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationBracket {
    pub bracket: u8,
}

impl GenerationBracket {
    /// Out-of-range brackets snap to 1 or 5.
    pub fn new(bracket: u8) -> Self {
        Self {
            bracket: bracket.clamp(1, 5),
        }
    }

    /// The bracket's official name, for logs and notes.
    pub fn label(self) -> &'static str {
        match self.bracket {
            1 => "Exhibition",
            2 => "Core",
            3 => "Upgraded",
            4 => "Optimized",
            _ => "cEDH profile",
        }
    }

    /// Most Game Changers allowed: none at 1–2, three at 3, eight at 4, any at 5.
    fn max_game_changers(self) -> usize {
        match self.bracket {
            1 | 2 => 0,
            3 => 3,
            4 => 8,
            _ => usize::MAX,
        }
    }


    /// How many combo lines to seed on purpose: none at 1, one at 2–3, two at 4–5.
    fn combo_target(self) -> usize {
        match self.bracket {
            1 => 0,
            2 | 3 => 1,
            4 | 5 => 2,
            _ => 1,
        }
    }

    /// Whether a built-in combo template fits this bracket: none at 1, slow
    /// multi-card ones at 2, anything but cEDH lines at 3, everything at 4–5.
    fn allow_compact_combo(self, template: &ComboTemplate) -> bool {
        match self.bracket {
            1 => false,
            2 => template.cards.len() >= 2 && !template.fast,
            3 => !template.cedh,
            4 | 5 => true,
            _ => true,
        }
    }

/// Fewest Game Changers a bracket "should" have; power_up_to_target adds
/// them when a build comes in under this.
fn minimum_game_changers(self) -> u32 {
        match self.bracket {
            1 | 2 => 0,
            3 => 1,
            4 => 4,
            5 => 4,
            _ => 0,
        }
    }

/// Fewest tutors per bracket (power_up_to_target tops up).
fn minimum_tutors(self) -> u32 {
        match self.bracket {
            1 => 0,
            2 => 2,
            3 => 2,
            4 => 4,
            5 => 7,
            _ => 0,
        }
    }

/// Fewest fast mana pieces per bracket (power_up_to_target tops up).
fn minimum_fast_mana(self) -> u32 {
        match self.bracket {
            1 => 0,
            2 => 0,
            3 => 1,
            4 => 2,
            5 => 5,
            _ => 0,
        }
    }

    /// Score nudge for 0–1 mana rocks/rituals: strongly avoided at low brackets,
    /// wanted at 4–5. Lands never count as fast mana here.
    fn fast_mana_score(self, card: &Card, roles: CardRoles) -> i32 {
        let oracle = card.oracle_text.to_ascii_lowercase();
        let ty = card.type_line.to_ascii_lowercase();

        let makes_mana = roles.ramp
            || oracle.contains("add {")
            || oracle.contains("add one mana")
            || oracle.contains("add two mana")
            || oracle.contains("add three mana");

        let fast = !ty.contains("land") && card.cmc <= 1.0 && makes_mana;

        if !fast {
            return 0;
        }

        match self.bracket {
            1 => -200,
            2 => -70,
            3 => -15,
            4 => 25,
            _ => 50,
        }
    }
}

/// The agent's "upgrade this deck" step: build a fresh deck for the same
/// commander at the target bracket, then pull the best of the player's
/// current cards back in, so the result is an upgrade of *their* deck rather
/// than a stranger's.
/// Steps: generate a new list → protect combo pieces, theme cards and
/// "any number of copies" cards → swap in up to 12 strong current cards over
/// weak flexible slots → swap in up to 8 of the AI model's priority cards →
/// displaced cards go to the sideboard → integrity and bracket check again.
/// Note: research runs twice (here and inside the generation call), and the
/// generation uses default options, which means the deep learning engine.
pub fn generate_ai_upgrade<F>(
    current: &ParsedDeck,
    target_bracket: GenerationBracket,
    preferred_theme: Option<&str>,
    ai_priority_cards: &[String],
    db: &CardDb,
    mut progress: F,
) -> Result<GeneratedDeck>
where
    F: FnMut(GenerationProgress),
{
    if current.commanders.is_empty() {
        return Err(anyhow!(
            "AI Upgrade requires a recognized commander."
        ));
    }

    let commander_specification = current
        .commanders
        .iter()
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>()
        .join(" + ");

    let commanders =
        resolve_generation_commanders(&commander_specification, db)?;

    // Theme: the one asked for, else the commander's top EDHREC theme.
    let selected_theme = preferred_theme
        .filter(|theme| !theme.trim().is_empty())
        .map(|theme| theme.trim().to_string())
        .or_else(|| {
            commander_themes(&commanders, 10)
                .first()
                .map(|theme| theme.name.clone())
        })
        .unwrap_or_else(|| "Synergy".to_string());

    progress(GenerationProgress::new(
        3,
        "Learning current deck",
        format!(
            "{} • normal shell {} • target Bracket {}",
            current.name,
            selected_theme,
            target_bracket.bracket,
        ),
    ));

    let intelligence = learn_commander_intelligence(
        &commanders,
        &selected_theme,
        target_bracket.bracket,
        db,
        |detail| {
            progress(GenerationProgress::new(
                10,
                "Deep Learn upgrade model",
                detail,
            ));
        },
    )?;

    // Build a fresh deck; its progress is squeezed into 15–85% of this job.
    let mut generated = generate_optimized_deck(
        &commander_specification,
        &selected_theme,
        target_bracket,
        db,
        |mut update| {
            update.percent =
                15u16 + ((update.percent * 70u16) / 100u16);
            progress(update);
        },
    )?;

    progress(GenerationProgress::new(
        88,
        "Comparing current list",
        "Retaining strong current cards when learned support beats a generated flexible slot",
    ));

    // Cards the deck runs several copies of because their text allows it
    // (Relentless Rats, Persistent Petitioners…). These must survive the swap.
    let copy_rule_engine = current
        .main
        .iter()
        .filter_map(|entry| {
            if entry.quantity <= 1 {
                return None;
            }

            let card = db.get_card(&entry.name).ok().flatten()?;

            if has_copy_rule_override(&card) {
                Some((
                    entry.name.to_ascii_lowercase(),
                    entry.clone(),
                ))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    // Never swapped out: combo pieces, copy-rule cards and on-theme cards.
    let mut protected = generated
        .plan
        .combos
        .iter()
        .flat_map(|combo| combo.cards.iter())
        .map(|name| name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    for (name, _) in &copy_rule_engine {
        protected.insert(name.clone());
    }

    // Keep the actual selected-theme engine intact while comparing the
    // generated shell against the user's current deck. This is especially
    // important for Stax, where lock pieces may not classify as generic
    // ramp/draw/interaction roles.
    for entry in &generated.deck.main {
        let Some(card) = db.get_card(&entry.name)? else {
            continue;
        };

        if card_matches_theme(&card, &selected_theme) {
            protected.insert(entry.name.to_ascii_lowercase());
        }
    }

    // Put the copy-rule cards back at their original counts, removing
    // unprotected nonland cards to make room.
    for (_, engine_entry) in &copy_rule_engine {
        let already_present = generated
            .deck
            .main
            .iter()
            .position(|entry| {
                entry.name.eq_ignore_ascii_case(&engine_entry.name)
            });

        if let Some(index) = already_present {
            generated.deck.main[index].quantity = engine_entry.quantity;
            continue;
        }

        let needed = engine_entry.quantity as usize;
        let mut removed = 0usize;

        generated.deck.main.retain(|entry| {
            if removed >= needed {
                return true;
            }

            if protected.contains(&entry.name.to_ascii_lowercase()) {
                return true;
            }

            if let Ok(Some(card)) = db.get_card(&entry.name) {
                if card.type_line.to_ascii_lowercase().contains("land") {
                    return true;
                }
            }

            removed += entry.quantity as usize;
            false
        });

        generated.deck.main.push(engine_entry.clone());
    }

    let generated_names = generated
        .deck
        .main
        .iter()
        .map(|entry| entry.name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    let mut retained = 0usize;
    let mut displaced = Vec::<DeckEntry>::new();

    // Keep the player's own good cards: anything from the current list with
    // strong evidence (support ≥125, confidence ≥25) replaces the weakest
    // unprotected, non-structural card it beats by more than 35. At most 12.
    let mut candidates = current
        .main
        .iter()
        .filter_map(|entry| {
            if generated_names.contains(&entry.name.to_ascii_lowercase()) {
                return None;
            }

            let card = db.get_card(&entry.name).ok().flatten()?;

            if card.type_line.to_ascii_lowercase().contains("land") {
                return None;
            }

            let support =
                intelligence.support_for_card(&card, classify_card(&card));

            Some((card, support.score, support.confidence))
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| a.0.name.cmp(&b.0.name))
    });

    for (candidate, score, confidence) in candidates {
        if retained >= 12 || score < 125 || confidence < 25 {
            continue;
        }

        let mut worst: Option<(usize, i32)> = None;

        for (index, entry) in generated.deck.main.iter().enumerate() {
            if protected.contains(&entry.name.to_ascii_lowercase()) {
                continue;
            }

            let Some(card) = db.get_card(&entry.name)? else {
                continue;
            };

            if card.type_line.to_ascii_lowercase().contains("land") {
                continue;
            }

            let roles = classify_card(&card);

            if roles.ramp
                || roles.draw
                || roles.removal
                || roles.counter
                || roles.wipe
                || roles.protection
            {
                continue;
            }

            let support =
                intelligence.support_for_card(&card, roles);

            if score <= support.score + 35 {
                continue;
            }

            if worst
                .as_ref()
                .map(|(_, current_score)| support.score < *current_score)
                .unwrap_or(true)
            {
                worst = Some((index, support.score));
            }
        }

        let Some((index, _)) = worst else {
            continue;
        };

        displaced.push(generated.deck.main[index].clone());

        generated.deck.main[index] = DeckEntry {
            quantity: 1,
            name: candidate.name.clone(), ..Default::default() };

        retained += 1;
    }

    // AI priorities still pass legality, identity, duplicate, engine, and bracket checks.
    // AI priority cards: up to 8 swaps, each still checked for legality and
    // colors, and only over a slot whose support is not far above its own.
    let identity = combined_identity(&commanders);
    let mut ai_promoted = 0usize;

    for requested_name in ai_priority_cards.iter().take(32) {
        if ai_promoted >= 8 {
            break;
        }

        if generated
            .deck
            .main
            .iter()
            .chain(generated.deck.commanders.iter())
            .any(|entry| entry.name.eq_ignore_ascii_case(requested_name))
        {
            continue;
        }

        let Some(candidate) = db.get_card(requested_name)? else {
            continue;
        };

        if !candidate.commander_legal
            || !identity_subset(
                &candidate.color_identity,
                &identity,
            )
            || candidate
                .type_line
                .to_ascii_lowercase()
                .contains("land")
        {
            continue;
        }

        let candidate_support = intelligence.support_for_card(
            &candidate,
            classify_card(&candidate),
        );

        let mut worst: Option<(usize, i32)> = None;

        for (index, entry) in
            generated.deck.main.iter().enumerate()
        {
            if protected.contains(
                &entry.name.to_ascii_lowercase(),
            ) {
                continue;
            }

            let Some(card) = db.get_card(&entry.name)? else {
                continue;
            };

            if card
                .type_line
                .to_ascii_lowercase()
                .contains("land")
            {
                continue;
            }

            let roles = classify_card(&card);

            if roles.ramp
                || roles.draw
                || roles.removal
                || roles.counter
                || roles.wipe
                || roles.protection
            {
                continue;
            }

            let support =
                intelligence.support_for_card(&card, roles);

            // AI suggestions still need some grounded support. A card with
            // weak evidence can only replace an even weaker flexible slot.
            if candidate_support.score + 45
                < support.score
            {
                continue;
            }

            if worst
                .as_ref()
                .map(|(_, current_score)| {
                    support.score < *current_score
                })
                .unwrap_or(true)
            {
                worst = Some((index, support.score));
            }
        }

        let Some((index, old_score)) = worst else {
            continue;
        };

        let removed =
            generated.deck.main[index].clone();

        generated.deck.main[index] = DeckEntry {
            quantity: 1,
            name: candidate.name.clone(), ..Default::default() };

        displaced.push(removed.clone());
        protected.insert(
            candidate.name.to_ascii_lowercase(),
        );
        ai_promoted += 1;

        progress(GenerationProgress::new(
            91,
            "Local AI card test",
            format!(
                "promoted {} (support {}) over {} (support {})",
                candidate.name,
                candidate_support.score,
                removed.name,
                old_score,
            ),
        ));
    }

    // Everything swapped out goes to the sideboard so nothing is lost.
    for entry in displaced.into_iter().take(15) {
        if !generated
            .deck
            .sideboard
            .iter()
            .any(|existing| existing.name.eq_ignore_ascii_case(&entry.name))
        {
            generated.deck.sideboard.push(entry);
        }
    }

    generated.deck.name = format!(
        "{} - AI Upgrade B{}",
        current.name,
        target_bracket.bracket,
    );

    // Retaining cards from the current list can change the power profile, so
    // validate the upgraded copy again after all substitutions.
    // Swaps can change the deck's power, so run the integrity gate (size,
    // legality, bracket) once more with a broad pool to repair from.
    let mut upgrade_pool = db.top_ranked_cards(15_000)?;

    for (name, _) in intelligence.references.top_cards(500) {
        if upgrade_pool
            .iter()
            .any(|card| card.name.eq_ignore_ascii_case(&name))
        {
            continue;
        }

        if let Some(card) = db.get_card(&name)? {
            upgrade_pool.push(card);
        }
    }

    let (integrity_report, validated) = integrity_gate(
        &mut generated.deck,
        target_bracket,
        db,
        &upgrade_pool,
        &identity,
        &intelligence,
        &generated.plan.combos,
    )?;
    generated.plan.notes.extend(integrity_report.notes());

    generated.plan.notes.push(format!(
        "AI Upgrade targeted Bracket {} and compared the learned shell against '{}'. Retained {} high-support cards from the original list. Displaced flexible alternatives were moved to Sideboard/consideration.",
        target_bracket.bracket,
        current.name,
        retained,
    ));

    if ai_promoted > 0 {
        generated.plan.notes.push(format!(
            "Local AI proposed {} grounded priority-card substitution(s); every substitution passed local card/legal/color checks and the completed list was bracket-validated afterward.",
            ai_promoted,
        ));
    }

    generated.plan.notes.push(format!(
        "Post-upgrade bracket validation: {} ({} fast mana, {} tutors, {} Game Changers).",
        validated.display_name(),
        validated.fast_mana,
        validated.tutors,
        validated.game_changers.len(),
    ));

    progress(GenerationProgress::new(
        96,
        "AI upgrade substitutions",
        format!(
            "retained_current={} • displaced_to_sideboard={} • final_main={}",
            retained,
            generated.deck.sideboard.len(),
            generated.deck.main.iter().map(|entry| entry.quantity).sum::<u32>(),
        ),
    ));

    progress(GenerationProgress::new(
        100,
        "AI upgrade complete",
        format!(
            "{} retained current card(s) • validated {}",
            retained,
            validated.display_name(),
        ),
    ));

    Ok(generated)
}

/// The player's own requests for a build (Build screen → Your requests):
/// cards to include ("pet cards"), combos to include, cards to leave out, and
/// instructions in their own words.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildRequests {
    #[serde(default)]
    pub include: Vec<String>,
    /// Each combo is its card names.
    #[serde(default)]
    pub combos: Vec<Vec<String>>,
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Given to the AI model when AI help is on, and kept in the notes.
    #[serde(default)]
    pub instructions: String,
}

impl BuildRequests {
    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.combos.is_empty() && self.exclude.is_empty() && self.instructions.trim().is_empty()
    }
}

/// Extra choices for a generation run.
#[derive(Debug, Clone, Default)]
pub struct GenerationOptions {
    /// Commander or Pauper Commander (Canadian Highlander has its own builder).
    pub format: Format,
    /// Read every card's Oracle text for cards that fit the chosen play
    /// styles, including old cards no decklist runs.
    pub deep_search: bool,
    /// Price limits (USD): a total for the deck and/or a per-card ceiling.
    pub budget: Budget,
    /// Ask the configured AI model for themes and key cards first.
    pub ai_assist: bool,
    /// Deep learning (default) or peer-based generation.
    pub engine: crate::deep_generation::GenerationEngine,
    /// Cards, combos and instructions the player asked for.
    pub requests: BuildRequests,
}

/// Shortcut: Commander format, no budget, default engine (deep learning).
/// Used by the agent's upgrade path.
pub fn generate_optimized_deck<F>(
    commander_specification: &str,
    selected_theme: &str,
    target_bracket: GenerationBracket,
    db: &CardDb,
    progress: F,
) -> Result<GeneratedDeck>
where
    F: FnMut(GenerationProgress),
{
    generate_deck_with_options(
        commander_specification,
        selected_theme,
        target_bracket,
        GenerationOptions::default(),
        db,
        progress,
    )
}

/// Look up PDH commanders ("A" or "A + B") and check them against the PDH
/// commander rules.
pub fn resolve_pdh_commanders(spec: &str, db: &CardDb) -> Result<Vec<Card>> {
    let mut out = Vec::new();
    for (i, name) in spec.split(" + ").map(str::trim).filter(|n| !n.is_empty()).enumerate() {
        let mut card = db
            .get_card(name)?
            .ok_or_else(|| anyhow!("Couldn't find a card named \"{name}\"."))?;
        if let Err(why) = Format::Pdh.commander_allowed(&card, i > 0) {
            // The uncommon-printing flag comes from a Scryfall refresh that
            // may not have run on this device: confirm it online first.
            let rarity_rule = why.contains("uncommon");
            if !(rarity_rule && crate::data::CardDb::open(crate::api::DB_PATH).map(|mut d| d.confirm_pdh_commander(&card.name)).unwrap_or(false)) {
                return Err(anyhow!("{} can't lead a Pauper Commander deck: {why}.", card.name));
            }
            card.pdh_commander = true;
            Format::Pdh
                .commander_allowed(&card, i > 0)
                .map_err(|why| anyhow!("{} can't lead a Pauper Commander deck: {why}.", card.name))?;
        }
        out.push(card);
    }
    if out.len() == 2 {
        let a = out[0].oracle_text.to_ascii_lowercase();
        let b = out[1].oracle_text.to_ascii_lowercase();
        let bg = out[1].type_line.to_ascii_lowercase().contains("background");
        let ok = (a.contains("partner") && b.contains("partner"))
            || (a.contains("choose a background") && bg)
            || (a.contains("friends forever") && b.contains("friends forever"));
        if !ok {
            return Err(anyhow!("{} and {} can't be paired as commanders.", out[0].name, out[1].name));
        }
    }
    Ok(out)
}

/// THE generator. Builds a 100-card Commander or Pauper Commander deck for
/// `commander_specification` ("A" or "A + B"). The steps below are marked
/// "Step N" in comments; the percentages match the job log. The deep learning
/// engine adds to steps 2, 3, 4, 5, 6 and 8 (search "deep").
pub fn generate_deck_with_options<F>(
    commander_specification: &str,
    selected_theme: &str,
    target_bracket: GenerationBracket,
    options: GenerationOptions,
    db: &CardDb,
    mut progress: F,
) -> Result<GeneratedDeck>
where
    F: FnMut(GenerationProgress),
{
    // Step 0: refuse what this builder can't do (Highlander has its own
    // builder; PDH needs the newer card data with format legalities).
    let format = options.format;
    if format == Format::Canlander {
        return Err(anyhow!("Canadian Highlander decks are built without a commander."));
    }
    if format.needs_format_data() && !db.has_format_data() {
        return Err(anyhow!(
            "Update card data (Settings → Update card data) to build {} decks; the format rules need the newer card data.",
            format.label()
        ));
    }
    let budget = options.budget;

    progress(GenerationProgress::new(
        4,
        "Resolving commander",
        commander_specification,
    ));

    // Step 1: commanders. PDH has its own commander rule (uncommon creature).
    let commanders = if format == Format::Pdh {
        resolve_pdh_commanders(commander_specification, db)?
    } else {
        resolve_generation_commanders(commander_specification, db)?
    };

    progress(GenerationProgress::new(
        10,
        "Commander locked",
        commanders
            .iter()
            .map(|card| card.name.as_str())
            .collect::<Vec<_>>()
            .join(" + "),
    ));

    if commanders.is_empty() || commanders.len() > 2 {
        return Err(anyhow!(
            "Generation requires one commander or one legal commander pair."
        ));
    }

    // `allowed` is the one legality filter used everywhere below: legal in
    // the format and under the per-card price ceiling left after the
    // commander's own price.
    let identity = combined_identity(&commanders);
    // Legal in the format and inside the per-card price ceiling (worked out
    // from what's left of the budget after the commander).
    let card_budget = budget.after_spending(commanders.iter().map(crate::budget::price).sum());
    // The player's requests. Requested cards must be legal and in the colors
    // (they're included even over the per-card budget); left-out cards never
    // enter the pool, even through combos or the AI.
    let requests = options.requests.clone();
    let excluded: HashSet<String> = requests
        .exclude
        .iter()
        .filter_map(|n| db.get_card(n).ok().flatten().map(|c| c.name.to_ascii_lowercase()).or_else(|| Some(n.trim().to_ascii_lowercase())))
        .collect();
    let mut request_notes: Vec<String> = Vec::new();
    let mut requested_cards: Vec<Card> = Vec::new();
    let mut requested_combos: Vec<GeneratedCombo> = Vec::new();
    {
        let take = |name: &str, notes: &mut Vec<String>| -> Option<Card> {
            let Some(card) = db.get_card(name.trim()).ok().flatten() else {
                notes.push(format!("Couldn't find a card named \"{}\".", name.trim()));
                return None;
            };
            if commanders.iter().any(|c| c.name == card.name) {
                return None;
            }
            if let Err(why) = format.card_allowed(&card) {
                notes.push(format!("Left out {}: {why}.", card.name));
                return None;
            }
            if !identity_subset(&card.color_identity, &identity) {
                notes.push(format!("Left out {}: it's outside the commander's colors.", card.name));
                return None;
            }
            Some(card)
        };
        for name in &requests.include {
            if let Some(card) = take(name, &mut request_notes) {
                if !requested_cards.iter().any(|c| c.name == card.name) {
                    requested_cards.push(card);
                }
            }
        }
        for combo in &requests.combos {
            let cards: Vec<Card> = combo.iter().filter_map(|n| take(n, &mut request_notes)).collect();
            if cards.len() == combo.iter().filter(|n| !n.trim().is_empty()).count() && !cards.is_empty() {
                requested_combos.push(GeneratedCombo {
                    cards: cards.iter().map(|c| c.name.clone()).collect(),
                    result: "Your combo".into(),
                    source: "Your request".into(),
                });
                for card in cards {
                    if !requested_cards.iter().any(|c| c.name == card.name) {
                        requested_cards.push(card);
                    }
                }
            } else {
                request_notes.push(format!("Couldn't include the combo {}: one of its cards can't go in this deck.", combo.join(" + ")));
            }
        }
    }
    let requested_names: HashSet<String> = requested_cards.iter().map(|c| c.name.to_ascii_lowercase()).collect();
    let allowed = |card: &Card| {
        format.card_allowed(card).is_ok()
            && card_budget.card_ok(card)
            && !excluded.contains(&card.name.to_ascii_lowercase())
    };
    let commander_entries = commanders
        .iter()
        .map(|card| DeckEntry {
            quantity: 1,
            name: card.name.clone(), ..Default::default() })
        .collect::<Vec<_>>();

    let generated_name = commanders
        .iter()
        .map(|card| card.name.clone())
        .collect::<Vec<_>>()
        .join(" + ");

    let shell = ParsedDeck {
        name: format!("{generated_name} - Generated"),
        commanders: commander_entries.clone(),
        main: Vec::new(),
        sideboard: Vec::new(),
        format: Default::default(),
        considering: Vec::new(),
    };

    progress(GenerationProgress::new(
        16,
        "Locking deck theme",
        if selected_theme.trim().is_empty() {
            "No play style chosen".to_string()
        } else {
            format!("Building around {selected_theme}")
        },
    ));

    // Step 2: themes. Start from what the commander alone suggests, then add
    // EDHREC's top themes for it (they define the "normal" shell), then the
    // player's chosen themes on top.
    let mut tags = detect_deck_tags(&shell, db)?;

    // A user's selected play style modifies the commander's normal shell; it
    // does not replace that shell. Pull the commander's dominant EDHREC tags
    // into the build profile first.
    let commander_edhrec_themes = commander_themes(&commanders, 10);
    let commander_core_tags = commander_edhrec_themes
        .iter()
        .take(4)
        .map(|theme| theme.name.clone())
        .collect::<Vec<_>>();

    for theme in commander_edhrec_themes.iter().take(6) {
        if let Some(existing) = tags
            .iter_mut()
            .find(|tag| tag.name.eq_ignore_ascii_case(&theme.name))
        {
            existing.score += 240 - (theme.rank as i32 * 15);
        } else {
            tags.push(ArchetypeTag {
                name: theme.name.clone(),
                score: 240 - (theme.rank as i32 * 15),
                matched_cards: 0,
            });
        }
    }

    // AI assist: the model's themes (used when the player chose none), key
    // cards and cards to avoid. Its card names are checked against the card
    // database, format and colors before they count.
    // Step 2a (optional): "Let the AI model help" from the Build screen.
    // Failure is logged and the build carries on without it.
    let mut ai_hints: Option<crate::ai::GenerationHints> = None;
    if options.ai_assist {
        progress(GenerationProgress::new(18, "Asking the AI model", "Themes, key cards and cards to avoid"));
        let edhrec_names: Vec<String> = commander_edhrec_themes.iter().take(6).map(|t| t.name.clone()).collect();
        match crate::ai::generation_hints(&commanders, &edhrec_names, selected_theme, target_bracket.bracket, format.label(), &requests) {
            Ok(hints) => {
                progress(GenerationProgress::new(
                    20,
                    "AI suggestions",
                    format!(
                        "[AI] {} • themes={} • key cards={} • avoid={} • plan: {}",
                        hints.model,
                        hints.themes.join(" / "),
                        hints.key_cards.join(", "),
                        hints.avoid.join(", "),
                        hints.plan,
                    ),
                ));
                ai_hints = Some(hints);
            }
            Err(err) => progress(GenerationProgress::new(20, "AI suggestions", format!("[AI] skipped: {err:#}"))),
        }
    }
    // Play styles, and where each came from. The player's own choice wins.
    // Otherwise: the AI model's, then (deep learning) the commander's own
    // mechanics, then its most popular EDHREC themes.
    // Step 2b: settle the play styles. `theme_lines` remembers where each came
    // from for the report. `selected_theme` is shadowed with the chosen spec so
    // everything below sees it as if the player had picked it.
    let deep = options.engine == crate::deep_generation::GenerationEngine::DeepLearning;
    let mechanics = if deep { crate::deep_generation::commander_mechanics(&commanders) } else { Vec::new() };
    let mut theme_lines: Vec<crate::deep_generation::ThemeLine> = Vec::new();
    let line = |name: &str, from: &str| crate::deep_generation::ThemeLine { name: name.to_string(), from: from.to_string() };
    if !selected_theme.trim().is_empty() {
        theme_lines.extend(theme_list(selected_theme).iter().map(|t| line(t, "you")));
    } else {
        if let Some(hints) = &ai_hints {
            theme_lines.extend(hints.themes.iter().map(|t| line(t, "the AI model")));
        }
        if deep {
            theme_lines.extend(mechanics.iter().map(|t| line(t, "the commander's rules text")));
            theme_lines.extend(commander_edhrec_themes.iter().take(2).map(|t| line(&t.name, "EDHREC")));
        }
        let mut seen = HashSet::new();
        theme_lines.retain(|t| seen.insert(t.name.to_ascii_lowercase()));
        theme_lines.truncate(3);
    }
    let auto_theme_spec;
    let selected_theme = if selected_theme.trim().is_empty() && !theme_lines.is_empty() {
        auto_theme_spec = theme_lines.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(crate::archetype::THEME_SEPARATOR);
        progress(GenerationProgress::new(
            21,
            "Play styles chosen",
            theme_lines.iter().map(|t| format!("{} (from {})", t.name, t.from)).collect::<Vec<_>>().join(" • "),
        ));
        auto_theme_spec.as_str()
    } else {
        selected_theme
    };

    // Chosen themes jump to the front of `tags`, then the commander's EDHREC
    // core tags are sorted first; the top 4 become `primary_tags` (saved in the
    // plan and used for scoring with `plan_bonus`).
    let selected_themes = theme_list(selected_theme);
    for theme in selected_themes.iter().rev() {
        if let Some(existing) = tags
            .iter_mut()
            .find(|tag| tag.name.eq_ignore_ascii_case(theme))
        {
            existing.score += 80;
        } else {
            tags.insert(
                0,
                ArchetypeTag {
                    name: theme.clone(),
                    score: 100,
                    matched_cards: 1,
                },
            );
        }
    }

    tags.sort_by(|a, b| {
        let a_core = commander_core_tags
            .iter()
            .position(|tag| tag.eq_ignore_ascii_case(&a.name));
        let b_core = commander_core_tags
            .iter()
            .position(|tag| tag.eq_ignore_ascii_case(&b.name));

        match (a_core, b_core) {
            (Some(ai), Some(bi)) => ai.cmp(&bi),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => {
                let a_selected = selected_themes.iter().any(|t| a.name.eq_ignore_ascii_case(t));
                let b_selected = selected_themes.iter().any(|t| b.name.eq_ignore_ascii_case(t));

                b_selected
                    .cmp(&a_selected)
                    .then_with(|| b.score.cmp(&a.score))
                    .then_with(|| a.name.cmp(&b.name))
            }
        }
    });

    let primary_tags = tags
        .iter()
        .take(4)
        .map(|tag| tag.name.clone())
        .collect::<Vec<_>>();

    progress(GenerationProgress::new(
        22,
        "Deep Learn commander model",
        "Learning EDHREC shell, public references, engine terms, and normal role distribution",
    ));

    // Research the commander in the format's own sources (PDHREC and
    // Archidekt's Pauper EDH decks for PDH).
    // Step 3: research (intelligence.rs). Deep learning reads more reference
    // decklists; `with_format` makes the sources PDH-specific for Pauper.
    let depth = if deep { crate::references::ReferenceDepth::DEEP } else { crate::references::ReferenceDepth::PEER };
    let intelligence = crate::references::with_depth(depth, || crate::format_sources::with_format(format, || {
        learn_commander_intelligence(
            &commanders,
            selected_theme,
            target_bracket.bracket,
            db,
            |detail| {
                progress(GenerationProgress::new(
                    26,
                    "Deep Learn commander model",
                    detail,
                ));
            },
        )
    }))?;

    // Generation and AI Agent share the learned commander model.
    let reference_snapshot = intelligence.references.clone();
    let edhrec_build_cards = intelligence.edhrec_cards.clone();
    let edhrec_signals = intelligence.edhrec_by_name.clone();
    let commander_engine_terms = intelligence.engine_terms.clone();

    progress(GenerationProgress::new(
        30,
        "Commander model ready",
        format!(
            "confidence {}% • {} reference deck(s) • {} EDHREC signals • {}",
            intelligence.confidence,
            reference_snapshot.deck_count(),
            edhrec_build_cards.len(),
            intelligence.role_summary(),
        ),
    ));

    // Step 4: the candidate pool. Start with the 15,000 most played cards,
    // then add EDHREC cards and the 500 most common reference-deck cards that
    // fall outside that cutoff.
    // TODO: the `pool.iter().any(...)` checks are linear scans inside loops;
    // a HashSet of names would make this step much cheaper.
    let mut pool = db.top_ranked_cards(15_000)?;

    // EDHREC high-synergy cards are allowed into the candidate pool even when
    // their global EDHREC rank is outside the generic top-15k cutoff.
    for signal in &edhrec_build_cards {
        if pool
            .iter()
            .any(|card| card.name.eq_ignore_ascii_case(&signal.name))
        {
            continue;
        }

        if let Some(card) = db.get_card(&signal.name)? {
            pool.push(card);
        }
    }

    // The large reference corpus can surface commander-specific cards whose
    // global EDHREC rank is outside the generic pool. Pull the most common
    // reference cards into generation explicitly.
    for (name, _) in reference_snapshot.top_cards(500) {
        if pool
            .iter()
            .any(|card| card.name.eq_ignore_ascii_case(&name))
        {
            continue;
        }

        if let Some(card) = db.get_card(&name)? {
            pool.push(card);
        }
    }

    // Pauper Commander: every legal common in the commander's colors is a
    // candidate, not just the globally popular ones.
    if format == Format::Pdh {
        let mut seen: HashSet<String> = pool.iter().map(|c| c.name.to_ascii_lowercase()).collect();
        db.scan_cards(|card| {
            if allowed(&card)
                && identity_subset(&card.color_identity, &identity)
                && seen.insert(card.name.to_ascii_lowercase())
            {
                pool.push(card);
            }
        })?;
    }
    // AI key cards join the pool even when they're outside the usual
    // popularity cutoff.
    // AI key cards: checked against the card database, the format and the
    // colors; the rest are logged as "ignored" so you can see what the model
    // made up. `ai_key` / `ai_avoid` feed the score bonus in step 6.
    let mut ai_key = HashSet::<String>::new();
    let mut ai_avoid = HashSet::<String>::new();
    if let Some(hints) = &ai_hints {
        let mut rejected = Vec::new();
        for name in &hints.key_cards {
            match db.get_card(name)? {
                Some(card) if allowed(&card) && identity_subset(&card.color_identity, &identity) => {
                    ai_key.insert(card.name.to_ascii_lowercase());
                    if !pool.iter().any(|c| c.name == card.name) {
                        pool.push(card);
                    }
                }
                Some(card) => rejected.push(format!("{} (not legal here)", card.name)),
                None => rejected.push(format!("{name} (no such card)")),
            }
        }
        for name in &hints.avoid {
            if let Some(card) = db.get_card(name)? {
                ai_avoid.insert(card.name.to_ascii_lowercase());
            }
        }
        progress(GenerationProgress::new(
            31,
            "AI suggestions checked",
            format!(
                "[AI] {} key card(s) usable{}",
                ai_key.len(),
                if rejected.is_empty() { String::new() } else { format!(" • ignored: {}", rejected.join(", ")) }
            ),
        ));
    }

    // Drop anything illegal or over the per-card budget in one place.
    pool.retain(|card| allowed(card));

    // Deep card search: Oracle-text matches for the chosen play styles.
    // Step 4b: rules-text search (oracle_search.rs). Always on for deep
    // learning; a Build-screen checkbox for peer-based. Finds join the pool and
    // `deep_finds` remembers their match score for the tiers.
    let mut deep_finds: HashMap<String, DeepFind> = HashMap::new();
    if options.deep_search || deep {
        let mut themes = theme_list(selected_theme);
        if themes.is_empty() {
            themes = primary_tags.clone();
        }
        progress(GenerationProgress::new(
            31,
            "Deep card search",
            format!("Reading every card's rules text for {}", themes.join(", ")),
        ));
        let finds = deep_search(db, &themes, Some(&identity), format, 120)?;
        for find in finds {
            let key = find.name.to_ascii_lowercase();
            if !pool.iter().any(|c| c.name.eq_ignore_ascii_case(&find.name)) {
                if let Some(card) = db.get_card(&find.name)? {
                    pool.push(card);
                }
            }
            deep_finds.insert(key, find);
        }
        progress(GenerationProgress::new(
            31,
            "Deep card search",
            format!("{} matching cards found by rules text", deep_finds.len()),
        ));
    }

    progress(GenerationProgress::new(
        32,
        "Candidate pool ready",
        format!(
            "{} unique candidates • {} EDHREC signals • {} reference decks • identity {}",
            pool.len(),
            edhrec_build_cards.len(),
            reference_snapshot.deck_count(),
            identity,
        ),
    ));

    // Step 5 setup: how many lands vs spells (PDH and 4–5 color decks get more
    // lands), and the running counters the picker updates as it adds cards.
    let library_target = 100usize.saturating_sub(commanders.len());
    // PDH has little mana-rock ramp (no Sol Ring, few signets at common), so
    // it runs a couple more lands.
    let land_target = match (format, identity.len() >= 4) {
        (Format::Pdh, true) => 37,
        (Format::Pdh, false) => 36,
        (_, true) => 35,
        (_, false) => 34,
    };
    let spell_target = library_target.saturating_sub(land_target);

    let mut used = commanders
        .iter()
        .map(|card| card.name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    let mut main = Vec::<DeckEntry>::new();
    let mut role_counts = RoleCounts::default();
    let mut seeded_combos = Vec::<GeneratedCombo>::new();
    let mut theme_cards = 0usize;
    let mut commander_engine_cards = 0usize;
    let mut edhrec_core_cards = 0usize;

    // The commander's own mechanics, matched by the same rules as deep
    // search. Peer-based generation relies on engine words alone, which
    // finds nothing for commanders like Drana whose text names no type.
    // "Engine" targets: how many cards should feed the commander's engine
    // (engine words or, for deep learning, its mechanics), how many EDHREC core
    // cards to favour, and how many direct theme cards (more for Stax).
    // These are soft targets: below them the matching bonus is big, above
    // them it shrinks.
    let mechanic_matchers: Vec<crate::oracle_search::ThemeMatcher> =
        mechanics.iter().map(|m| crate::oracle_search::ThemeMatcher::new(m)).collect();

    let commander_engine_target =
        if commander_engine_terms.is_empty() && mechanic_matchers.is_empty() {
            0
        } else {
            match target_bracket.bracket {
                1 => 18,
                2 => 22,
                3 => 24,
                4 => 26,
                _ => 24,
            }
        };

    let edhrec_core_target =
        edhrec_build_cards.len().min(match target_bracket.bracket {
            1 => 14,
            2 => 16,
            3 => 18,
            4 => 20,
            _ => 22,
        });

    // Theme fit influences score but never excludes otherwise useful cards.
    let theme_preference_target =
        if theme_list(selected_theme).iter().any(|t| t.eq_ignore_ascii_case("stax")) {
            match target_bracket.bracket {
                1 => 16,
                2 => 20,
                3 => 24,
                4 => 28,
                _ => 30,
            }
        } else {
            match target_bracket.bracket {
                1 => 26,
                2 => 24,
                3 => 23,
                4 => 22,
                _ => 20,
            }
        };

    // Step 5: win packages, best evidence first. Combos go in before any other
    // spell so the picker builds around them.
    let templates = combo_templates();

    // Step 4b: the player's requested cards and combos go in first, so the
    // rest of the build works around them.
    for card in &requested_cards {
        if main.len() >= spell_target && !card.type_line.to_ascii_lowercase().contains("land") {
            request_notes.push(format!("No room left for {}.", card.name));
            continue;
        }
        let key = card.name.to_ascii_lowercase();
        if !used.insert(key.clone()) {
            continue;
        }
        role_counts.add_card(card, classify_card(card));
        if card_matches_theme(card, selected_theme) {
            theme_cards += 1;
        }
        if card_matches_engine(card, &commander_engine_terms) {
            commander_engine_cards += 1;
        }
        if edhrec_signals.contains_key(&key) {
            edhrec_core_cards += 1;
        }
        main.push(DeckEntry { quantity: 1, name: card.name.clone(), ..Default::default() });
    }
    seeded_combos.extend(requested_combos.iter().cloned());
    if !requested_cards.is_empty() {
        progress(GenerationProgress::new(
            33,
            "Your requests",
            format!(
                "Included {}{}",
                requested_cards.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "),
                if requested_combos.is_empty() { String::new() } else { format!(" • {} combo(s) of yours", requested_combos.len()) }
            ),
        ));
    }

    progress(GenerationProgress::new(
        34,
        "Learning reference win packages",
        format!(
            "Checking recurring combos across {} matched reference deck(s)",
            reference_snapshot.deck_count()
        ),
    ));

    // Prefer win packages that are repeatedly present in actual reference
    // lists for this commander. Static templates are only a fallback.
    // 5a: combos that recur in this commander's reference decks. Skipped if
    // seen in too few decks, too slow for bracket 5, or not relevant enough to
    // this commander (`combo_relevance`), or if a piece isn't legal here.
    for (combo_cards, result, spellbook_bracket, frequency) in
        reference_snapshot.top_combos(16)
    {
        if seeded_combos.len() >= target_bracket.combo_target() {
            break;
        }

        let minimum_reference_hits = if reference_snapshot.deck_count() >= 20 {
            2
        } else {
            1
        };

        if frequency < minimum_reference_hits {
            continue;
        }

        if target_bracket.bracket >= 5
            && spellbook_bracket.unwrap_or(0) < 3
            && frequency < 3
        {
            continue;
        }

        if intelligence.combo_relevance(&combo_cards, db)
            < if target_bracket.bracket >= 5 { 180 } else { 130 }
        {
            continue;
        }

        let mut cards_to_add = Vec::<Card>::new();
        let mut legal = true;

        for name in &combo_cards {
            let Some(card) = db.get_card(name)? else {
                legal = false;
                break;
            };

            if !allowed(&card)
                || !identity_subset(&card.color_identity, &identity)
            {
                legal = false;
                break;
            }

            if !used.contains(&card.name.to_ascii_lowercase()) {
                cards_to_add.push(card);
            }
        }

        if !legal
            || main.len() + cards_to_add.len() > spell_target
        {
            continue;
        }

        for card in &cards_to_add {
            used.insert(card.name.to_ascii_lowercase());
            let roles = classify_card(card);
            role_counts.add_card(card, roles);

            if card_matches_theme(card, selected_theme) {
                theme_cards += 1;
            }
            if card_matches_engine(card, &commander_engine_terms) {
                commander_engine_cards += 1;
            }
            if edhrec_signals.contains_key(
                &card.name.to_ascii_lowercase()
            ) {
                edhrec_core_cards += 1;
            }

            main.push(DeckEntry {
                quantity: 1,
                name: card.name.clone(), ..Default::default() });
        }

        seeded_combos.push(GeneratedCombo {
            cards: combo_cards.clone(),
            result: if result.trim().is_empty() {
                "Commander Spellbook verified reference combo".to_string()
            } else {
                result.clone()
            },
            source: format!(
                "Observed in {frequency}/{} reference decks",
                reference_snapshot.deck_count(),
            ),
        });
    }

    // Deep learning: Commander Spellbook's combos for this commander, most
    // played first, when the reference decks didn't supply enough.
    // 5b (deep learning only): Commander Spellbook's combos that include the
    // commander, when the reference decks didn't give enough.
    let mut spellbook_found = 0usize;
    let mut spellbook_status = if deep { "not needed" } else { "off" };
    if deep && seeded_combos.len() < target_bracket.combo_target() {
        spellbook_status = "used";
        let legal = match format {
            Format::Pdh => "pauper_commander",
            _ => "commander",
        };
        let ci = if identity.is_empty() { "c".to_string() } else { identity.to_lowercase() };
        for commander in &commanders {
            let query = format!("card:\"{}\" ci<={ci} legal:{legal}", commander.name);
            let Ok(found) = crate::spellbook::search_combos(&query, 30) else {
                spellbook_status = "failed";
                continue;
            };
            spellbook_found += found.len();
            for combo in found {
                if seeded_combos.len() >= target_bracket.combo_target() {
                    break;
                }
                // Compact lines only: two or three cards.
                if combo.cards.len() > 3 {
                    continue;
                }
                let mut to_add = Vec::new();
                let mut ok = true;
                for name in &combo.cards {
                    match db.get_card(name)? {
                        Some(card) if allowed(&card) && identity_subset(&card.color_identity, &identity) => {
                            if !used.contains(&card.name.to_ascii_lowercase()) {
                                to_add.push(card);
                            }
                        }
                        _ => {
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok || main.len() + to_add.len() > spell_target {
                    continue;
                }
                for card in &to_add {
                    used.insert(card.name.to_ascii_lowercase());
                    role_counts.add_card(card, classify_card(card));
                    main.push(DeckEntry { quantity: 1, name: card.name.clone(), ..Default::default() });
                }
                seeded_combos.push(GeneratedCombo {
                    cards: combo.cards.clone(),
                    result: combo.result.lines().next().unwrap_or("Commander Spellbook combo").to_string(),
                    source: "Commander Spellbook, built around this commander".to_string(),
                });
            }
        }
        progress(GenerationProgress::new(
            35,
            "Commander Spellbook",
            format!("{spellbook_found} combo(s) with this commander found • {} seeded in total", seeded_combos.len()),
        ));
    }

    progress(GenerationProgress::new(
        36,
        "Selecting win packages",
        format!(
            "{} recurring reference combo(s) seeded • {}",
            seeded_combos.len(),
            if seeded_combos.is_empty() {
                "no reference combo accepted yet".to_string()
            } else {
                seeded_combos
                    .iter()
                    .map(|combo| {
                        format!(
                            "{} [{}]",
                            combo.cards.join(" + "),
                            combo.source,
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" | ")
            },
        ),
    ));

// 5c: the built-in combo templates (combo_templates), last resort, only
// those the bracket allows and that relate to this commander.
let relevant_templates = templates
    .iter()
    .filter(|template| target_bracket.allow_compact_combo(template))
    .filter(|template| {
        let names = template
            .cards
            .iter()
            .map(|name| (*name).to_string())
            .collect::<Vec<_>>();

        intelligence.combo_relevance(&names, db) >= 120
    });

// Combo packages must belong to the commander's actual build. We do not
// inject a generic two-card win merely because the colors allow it.
for template in relevant_templates
{
    if seeded_combos.len() >= target_bracket.combo_target() {
        break;
    }

    let Some(cards) = resolve_combo_template(template, &identity, format, db)? else {
        continue;
    };
    if !cards.iter().all(|card| budget.card_ok(card)) {
        continue;
    }

    if cards
        .iter()
        .any(|card| used.contains(&card.name.to_ascii_lowercase()))
    {
        continue;
    }

    if main.len() + cards.len() > spell_target {
        continue;
    }

    for card in &cards {
        used.insert(card.name.to_ascii_lowercase());
        let roles = classify_card(card);
        role_counts.add_card(card, roles);

        if card_matches_theme(card, selected_theme) {
            theme_cards += 1;
        }
        if card_matches_engine(card, &commander_engine_terms) {
            commander_engine_cards += 1;
        }
        if edhrec_signals.contains_key(&card.name.to_ascii_lowercase()) {
            edhrec_core_cards += 1;
        }

        main.push(DeckEntry {
            quantity: 1,
            name: card.name.clone(), ..Default::default() });
    }

    seeded_combos.push(GeneratedCombo {
        cards: cards.iter().map(|card| card.name.clone()).collect(),
        result: template.result.to_string(),
        source: "Commander Lab relevant combo seed".to_string(),
    });
}

    // 5d: study how decks with this commander win: recurring combos (above)
    // and finishers. Add the most common finishers that fit; every line is
    // recorded so the primer can list the rest as potential changes.
    let game_changers_ok = target_bracket.max_game_changers() > 0;
    let mut win_lines = if reference_snapshot.deck_count() >= 3 {
        crate::win_lines::study(&reference_snapshot, db, if reference_snapshot.deck_count() >= 10 { 0.15 } else { 0.3 })
    } else {
        Vec::new()
    };
    let finisher_target = if target_bracket.bracket <= 1 { 1 } else { 2 };
    let mut finishers_added: Vec<String> = Vec::new();
    for line in win_lines.iter().filter(|l| l.kind == "finisher") {
        if finishers_added.len() >= finisher_target || main.len() >= spell_target {
            break;
        }
        let Some(card) = db.get_card(&line.cards[0])? else { continue };
        let key = card.name.to_ascii_lowercase();
        if used.contains(&key) {
            finishers_added.push(card.name.clone());
            continue;
        }
        if !allowed(&card) || !identity_subset(&card.color_identity, &identity) || (card.game_changer && !game_changers_ok) {
            continue;
        }
        used.insert(key.clone());
        role_counts.add_card(&card, classify_card(&card));
        if card_matches_theme(&card, selected_theme) {
            theme_cards += 1;
        }
        if card_matches_engine(&card, &commander_engine_terms) {
            commander_engine_cards += 1;
        }
        if edhrec_signals.contains_key(&key) {
            edhrec_core_cards += 1;
        }
        main.push(DeckEntry { quantity: 1, name: card.name.clone(), ..Default::default() });
        finishers_added.push(card.name.clone());
    }
    progress(GenerationProgress::new(
        39,
        "Studying win lines",
        if win_lines.is_empty() {
            "Too few reference decks to study how this commander wins".to_string()
        } else {
            format!(
                "{} common win line(s) in {} reference deck(s) • finishers added: {}",
                win_lines.len(),
                reference_snapshot.deck_count(),
                if finishers_added.is_empty() { "none".to_string() } else { finishers_added.join(", ") }
            )
        },
    ));

    progress(GenerationProgress::new(
        40,
        "Win packages locked",
        format!(
            "{} seeded combo line(s) • {}",
            seeded_combos.len(),
            if seeded_combos.is_empty() {
                "none".to_string()
            } else {
                seeded_combos
                    .iter()
                    .map(|combo| combo.cards.join(" + "))
                    .collect::<Vec<_>>()
                    .join(" | ")
            },
        ),
    ));

    progress(GenerationProgress::new(
        44,
        "Building theme engine",
        format!(
            "Favoring roughly {theme_preference_target} direct {selected_theme} cards without excluding off-theme utility"
        ),
    ));

    // Step 6: pick the spells.
    // `thin_evidence` is true when there's no EDHREC/PDHREC data and no
    // reference decks (new or obscure commanders, offline): engine and theme
    // bonuses are toned down then, because they'd otherwise match far too much.
    // Without EDHREC or decklist evidence (Pauper Commander, or offline), the
    // commander's own keywords ("counter", "token"…) match far too broadly to
    // carry the large bonuses they get when real build data backs them up.
    let thin_evidence = edhrec_build_cards.is_empty() && reference_snapshot.deck_count() == 0;
    // Each candidate's evidence tier and reasons, for the report.
    let mut evidence: HashMap<String, (Option<crate::deep_generation::Tier>, Vec<String>)> = HashMap::new();

    // 6a: score every legal nonland candidate once. Each entry is
    // (card, roles, base score, matches a theme, matches the engine, reference
    // frequency, has EDHREC data). The base score never changes; the loop below
    // adds the "what does the deck still need" bonuses each step.
    let spell_pool = pool
        .iter()
        .filter(|card| {
            allowed(card)
                && identity_subset(&card.color_identity, &identity)
                && !card.type_line.to_ascii_lowercase().contains("land")
                && !used.contains(&card.name.to_ascii_lowercase())
        })
        .map(|card| {
            let roles = classify_card(card);
            let direct_theme = card_matches_theme(card, selected_theme)
                || deep_finds.contains_key(&card.name.to_ascii_lowercase());
            let mechanic_match = mechanic_matchers.iter().any(|m| m.matches(card));
            let engine_match =
                card_matches_engine(card, &commander_engine_terms) || mechanic_match;
            let reference_frequency =
                reference_snapshot.frequency(&card.name);
            let edhrec_signal =
                edhrec_signals.get(&card.name.to_ascii_lowercase());
            let (tier, tier_bonus, reasons) = crate::deep_generation::assess(
                card,
                &crate::deep_generation::Evidence {
                    edhrec: edhrec_signal,
                    reference_hits: reference_frequency,
                    reference_decks: reference_snapshot.deck_count(),
                    deep_find: deep_finds.get(&card.name.to_ascii_lowercase()).map(|f| f.score),
                    mechanic: mechanic_match,
                    theme: card_matches_theme(card, selected_theme),
                    ai_key: ai_key.contains(&card.name.to_ascii_lowercase()),
                    roles,
                    bracket: target_bracket.bracket,
                    card_data: if format == Format::Pdh { "PDHREC" } else { "EDHREC" },
                },
            );
            evidence.insert(card.name.clone(), (tier, reasons));
            let learned_support =
                intelligence.support_for_card(card, roles);

            // Deep-search finds have no EDHREC rank or decklist support, so
            // give them back what the popularity signals would have.
            let deep_bonus = deep_finds
                .get(&card.name.to_ascii_lowercase())
                .map(|find| 8 + (find.score / 10).min(12))
                .unwrap_or(0);

            // Pauper Commander has no EDHREC build data, so a card's overall
            // play rate stands in for "format staple" (Gray Merchant, Sign in
            // Blood…), balancing the theme and engine bonuses.
            let staple_bonus = if format.is_commander() || !thin_evidence {
                0
            } else {
                match card.edhrec_rank {
                    Some(r) if r <= 500 => 95,
                    Some(r) if r <= 2_000 => 70,
                    Some(r) if r <= 6_000 => 45,
                    Some(r) if r <= 15_000 => 15,
                    _ => 0,
                }
            };

            let support = if thin_evidence { learned_support.score / 3 } else { learned_support.score };

            // AI assist: a nudge, not a takeover. Key cards land near the
            // EDHREC-core bonus; avoided cards drop below most utility.
            let key = card.name.to_ascii_lowercase();
            let ai_bonus = if ai_key.contains(&key) { 120 } else if ai_avoid.contains(&key) { -120 } else { 0 };

            // The fixed part of a card's score, summed from independent pieces:
            // AI key/avoid (±120) + evidence tiers (deep learning only) + popularity
            // and fit to the deck's tags + fast mana for the bracket + bracket fit +
            // learned support from research + rules-text find + PDH staple stand-in.
            // TODO: these were tuned one at a time and overlap (EDHREC shows up in
            // `support`, the tiers and again as `edhrec_need` below). A per-card
            // breakdown in the report would make re-tuning much easier.
            let score = ai_bonus
                + if deep { tier_bonus } else { 0 }
                + base_card_score(card, &tags)
                + target_bracket.fast_mana_score(card, roles)
                + bracket_card_score(card, roles, target_bracket)
                + support
                + deep_bonus
                + staple_bonus;

            (
                card,
                roles,
                score,
                direct_theme,
                engine_match,
                reference_frequency,
                edhrec_signal.is_some(),
            )
        })
        .collect::<Vec<_>>();

    // Rarely played deep-search finds earn a place on fit, but a deck built
    // mostly from them loses the proven core, so cap how many get in.
    // At most 12 cards that only the rules-text search found (rank > 15,000),
    // so a deck keeps its proven core.
    const DEEP_OBSCURE_CAP: usize = 12;
    let is_deep_obscure = |card: &Card| {
        deep_finds.contains_key(&card.name.to_ascii_lowercase())
            && card.edhrec_rank.map(|r| r > 15_000).unwrap_or(true)
    };
    let mut deep_obscure_picked = 0usize;

    // Deep learning: a power floor. Before synergy picks, reserve slots for
    // the format's most-played staples and Game Changers in these colors,
    // more at higher brackets (the bracket's Game Changer limit holds).
    if deep {
        let quota = match target_bracket.bracket {
            1 => 0,
            2 => 4,
            3 => 8,
            4 => 14,
            _ => 20,
        };
        let mut staples: Vec<&(&Card, CardRoles, i32, bool, bool, u32, bool)> = spell_pool
            .iter()
            .filter(|(card, roles, ..)| {
                let structural = roles.ramp || roles.draw || roles.removal || roles.wipe || roles.tutor || roles.protection;
                (structural || card.game_changer) && card.edhrec_rank.map(|r| r <= 1_500).unwrap_or(false)
            })
            .collect();
        staples.sort_by_key(|(card, ..)| card.edhrec_rank.unwrap_or(u32::MAX));
        let mut picked = Vec::new();
        for (card, roles, _, direct_theme, engine_match, _, edhrec_core) in staples {
            if picked.len() >= quota || main.len() >= spell_target {
                break;
            }
            if used.contains(&card.name.to_ascii_lowercase()) {
                continue;
            }
            if card.game_changer && role_counts.game_changers as usize >= target_bracket.max_game_changers() {
                continue;
            }
            used.insert(card.name.to_ascii_lowercase());
            role_counts.add_card(card, *roles);
            if *direct_theme {
                theme_cards += 1;
            }
            if *engine_match {
                commander_engine_cards += 1;
            }
            if *edhrec_core {
                edhrec_core_cards += 1;
            }
            main.push(DeckEntry { quantity: 1, name: card.name.clone(), ..Default::default() });
            picked.push(card.name.clone());
        }
        progress(GenerationProgress::new(
            46,
            "Power staples",
            format!("{} of {quota} reserved: {}", picked.len(), picked.join(", ")),
        ));
    }

    // 6b: the greedy picker. Each pass rescans every unused candidate, adds
    // "what the deck still needs" bonuses to its fixed score (theme cards until
    // the theme target, engine cards until the engine target, EDHREC core until
    // its target, role needs from `need_bonus`, the plan's tags), and takes the
    // single best card. Repeats until the spell slots are full.
    // Game Changers over the bracket's limit are skipped (marked used).
    // Cost: candidates × slots (~15k × 65), fine on desktop, slowish on phones.
    while main.len() < spell_target {
        let mut best_index = None;
        let mut best_score = i32::MIN;

        for (index, (card, roles, base, direct_theme, engine_match, reference_frequency, edhrec_core)) in spell_pool.iter().enumerate() {
            if used.contains(&card.name.to_ascii_lowercase()) {
                continue;
            }
            if deep_obscure_picked >= DEEP_OBSCURE_CAP && is_deep_obscure(card) {
                continue;
            }

            let structural = has_structural_role(*roles);

            let theme_need =
                if *direct_theme && theme_cards < theme_preference_target {
                    90
                } else if *direct_theme {
                    if thin_evidence { 10 } else { 45 }
                } else {
                    0
                };

            // Off-theme cards are never excluded. Cards with no direct theme,
            // structural, or reference support receive only a modest
            // coherence penalty and can still win the slot on overall value.
            let widely_played = card.edhrec_rank.map(|r| r <= 6_000).unwrap_or(false);
            let coherence_adjustment =
                if !*direct_theme && !structural && *reference_frequency == 0
                    && !(thin_evidence && widely_played)
                {
                    -24
                } else {
                    0
                };

            let engine_need =
                if *engine_match
                    && commander_engine_cards < commander_engine_target
                {
                    if thin_evidence { 45 } else { 180 }
                } else if *engine_match {
                    if thin_evidence { 15 } else { 70 }
                } else {
                    0
                };

            let edhrec_need =
                if deep {
                    // Already counted, synergy-weighted, in the tier bonus.
                    0
                } else if *edhrec_core
                    && edhrec_core_cards < edhrec_core_target
                {
                    150
                } else if *edhrec_core {
                    55
                } else {
                    0
                };

            let score = *base
                + theme_need
                + engine_need
                + edhrec_need
                + coherence_adjustment
                + need_bonus(*roles, &role_counts, target_bracket, &intelligence.role_targets)
                + plan_bonus(card, &primary_tags);

            if score > best_score {
                best_score = score;
                best_index = Some(index);
            }
        }

        let Some(index) = best_index else {
            break;
        };

        let (card, roles, _, direct_theme, engine_match, _, edhrec_core) = spell_pool[index];

        if card.game_changer
            && role_counts.game_changers as usize >= target_bracket.max_game_changers()
        {
            used.insert(card.name.to_ascii_lowercase());
            continue;
        }

        used.insert(card.name.to_ascii_lowercase());
        role_counts.add_card(card, roles);
        if is_deep_obscure(card) {
            deep_obscure_picked += 1;
        }

        if direct_theme {
            theme_cards += 1;
        }
        if engine_match {
            commander_engine_cards += 1;
        }
        if edhrec_core {
            edhrec_core_cards += 1;
        }

        main.push(DeckEntry {
            quantity: 1,
            name: card.name.clone(), ..Default::default() });
    }

    progress(GenerationProgress::new(
        64,
        "Core shell assembled",
        format!(
            "spells={} • theme={} • engine={} • EDHREC-core={} • ramp={} • draw={} • interaction={} • tutors={} • wipes={} • protection={} • recursion={}",
            main.len(),
            theme_cards,
            commander_engine_cards,
            edhrec_core_cards,
            role_counts.ramp,
            role_counts.draw,
            role_counts.interaction,
            role_counts.tutor,
            role_counts.wipe,
            role_counts.protection,
            role_counts.recursion,
        ),
    ));

    progress(GenerationProgress::new(
        70,
        "Building mana base",
        format!("Color identity {identity}"),
    ));

    // Step 7: mana base. Nonbasic lands ranked by `land_build_score` (EDHREC,
    // reference decks, engine words); take 9 for mono-color, 16 for two colors,
    // 22 for three or more, then fill the rest with basics.
    // TODO: no check that the lands actually make the deck's colors in the
    // right proportions (basics are split by `add_basics`, not by pip count).
    let mut land_pool = pool
        .iter()
        .filter(|card| {
            allowed(card)
                && identity_subset(&card.color_identity, &identity)
                && card.type_line.to_ascii_lowercase().contains("land")
                && !card.is_basic
                && !used.contains(&card.name.to_ascii_lowercase())
        })
        .collect::<Vec<_>>();

    land_pool.sort_by(|a, b| {
        let a_score =
            land_build_score(
                a,
                &edhrec_signals,
                &reference_snapshot,
                &commander_engine_terms,
            );
        let b_score =
            land_build_score(
                b,
                &edhrec_signals,
                &reference_snapshot,
                &commander_engine_terms,
            );

        b_score
            .cmp(&a_score)
            .then_with(|| {
                a.edhrec_rank
                    .unwrap_or(u32::MAX)
                    .cmp(&b.edhrec_rank.unwrap_or(u32::MAX))
            })
            .then_with(|| a.name.cmp(&b.name))
    });

    let nonbasic_target = if identity.len() <= 1 {
        9
    } else if identity.len() == 2 {
        16
    } else {
        22
    };

    for card in land_pool.into_iter().take(nonbasic_target.min(land_target)) {
        used.insert(card.name.to_ascii_lowercase());
        main.push(DeckEntry {
            quantity: 1,
            name: card.name.clone(), ..Default::default() });
    }

    let current_lands = main
        .iter()
        .filter(|entry| {
            db.get_card(&entry.name)
                .ok()
                .flatten()
                .map(|card| {
                    card.type_line.to_ascii_lowercase().contains("land")
                })
                .unwrap_or(false)
        })
        .count();

    add_basics(
        &mut main,
        &identity,
        land_target.saturating_sub(current_lands),
    );

    // 7b: safety net. If the deck is still short (tiny pools: PDH, tight
    // budgets), fill with the best remaining legal spells by a simpler score.
    let mut library_count =
        main.iter().map(|entry| entry.quantity as usize).sum::<usize>();

    if library_count < library_target {
        // Final fill remains coherence-aware, but scores every remaining legal
        // card instead of requiring a theme/role/reference match.
        let mut fallback_cards = pool
            .iter()
            .filter(|card| {
                allowed(card)
                    && identity_subset(&card.color_identity, &identity)
                    && !card.type_line.to_ascii_lowercase().contains("land")
                    && !used.contains(&card.name.to_ascii_lowercase())
            })
            .map(|card| {
                let roles = classify_card(card);
                let direct_theme =
                    card_matches_theme(card, selected_theme)
                        || deep_finds.contains_key(&card.name.to_ascii_lowercase());
                let reference_frequency =
                    reference_snapshot.frequency(&card.name);

                let score = base_card_score(card, &tags)
                    + target_bracket.fast_mana_score(card, roles)
                    + bracket_card_score(card, roles, target_bracket)
                    + reference_card_score(
                        reference_frequency,
                        reference_snapshot.deck_count(),
                        target_bracket,
                    )
                    + need_bonus(roles, &role_counts, target_bracket, &intelligence.role_targets)
                    + plan_bonus(card, &primary_tags)
                    + if direct_theme { 50 } else { 0 }
                    + if has_structural_role(roles) { 25 } else { 0 };

                (card, roles, direct_theme, score)
            })
            .collect::<Vec<_>>();

        fallback_cards.sort_by(|a, b| {
            b.3.cmp(&a.3)
                .then_with(|| {
                    a.0.edhrec_rank
                        .unwrap_or(u32::MAX)
                        .cmp(&b.0.edhrec_rank.unwrap_or(u32::MAX))
                })
                .then_with(|| a.0.name.cmp(&b.0.name))
        });

        for (card, roles, direct_theme, _) in fallback_cards {
            if library_count >= library_target {
                break;
            }

            if card.game_changer
                && role_counts.game_changers as usize
                    >= target_bracket.max_game_changers()
            {
                continue;
            }

            used.insert(card.name.to_ascii_lowercase());
            role_counts.add_card(card, roles);

            main.push(DeckEntry {
                quantity: 1,
                name: card.name.clone(), ..Default::default() });

            if direct_theme {
                theme_cards += 1;
            }

            library_count += 1;
        }
    }

    // Still short after the fallback: give up with a clear error rather than
    // hand over a deck that isn't 100 cards.
    if library_count != library_target {
        return Err(anyhow!(
            "The generator could only build a {library_count}-card library. \
             A {}-commander deck needs {library_target} library cards.",
            commanders.len()
        ));
    }

    // The deck as built so far; steps 8–9 check and repair it in place.
    let mut deck = ParsedDeck {
        name: if format == Format::Pdh {
            format!("{generated_name} - PDH")
        } else {
            format!("{generated_name} - Generated")
        },
        commanders: commander_entries,
        main,
        sideboard: Vec::new(),
        format,
        considering: Vec::new(),
    };

    let mut notes = Vec::new();

progress(GenerationProgress::new(
    82,
    "Checking combo database",
    if target_bracket.bracket >= 5 {
        "Completing competitive Commander Spellbook lines required by the Bracket 5 profile"
    } else {
        "Validating included and one-card-away Commander Spellbook lines"
    },
));

// Step 8: combo check with Commander Spellbook (brackets 3+). Up to 6
// rounds: ask Spellbook what the deck has; if it has no complete combo (or,
// at bracket 5, fewer than two fast "ruthless" ones), take the most
// reference-backed combo that's one card away, and swap that card in over
// the weakest plan card. Each round is a network call.
if target_bracket.bracket >= 3 {
    let desired_ruthless =
        if target_bracket.bracket >= 5 { 2 } else { 0 };

    for _ in 0..6 {
        let Ok(report) = find_my_combos(&deck) else {
            break;
        };

        let complete_ruthless = report
            .variants
            .iter()
            .filter(|line| {
                !line.almost
                    && line.bracket.unwrap_or(0) >= 4
            })
            .count();

        if target_bracket.bracket >= 5 {
            if complete_ruthless >= desired_ruthless {
                break;
            }
        } else if report.included > 0 {
            break;
        }

        let almost = report
            .variants
            .iter()
            .filter(|line| line.almost)
            .filter(|line| {
                target_bracket.bracket < 5
                    || line.bracket.unwrap_or(0) >= 4
            })
            .filter(|line| {
                missing_combo_cards(
                    line.cards.as_slice(),
                    &deck,
                )
                .len()
                    == 1
            })
            .filter(|line| {
                intelligence.combo_relevance(&line.cards, db)
                    >= if target_bracket.bracket >= 5 {
                        180
                    } else {
                        140
                    }
            })
            .max_by_key(|line| {
                reference_snapshot
                    .combo_frequency_for_cards(&line.cards)
            });

        let Some(almost) = almost else {
            break;
        };

        let missing =
            missing_combo_cards(almost.cards.as_slice(), &deck);

        let Some(name) = missing.first() else {
            break;
        };

        let Some(card) = db.get_card(name)? else {
            break;
        };

        if !allowed(&card)
            || !identity_subset(
                &card.color_identity,
                &identity,
            )
        {
            break;
        }

        if !replace_low_plan_card(
            &mut deck,
            &card,
            db,
            &tags,
            &seeded_combos,
        )? {
            break;
        }

        notes.push(format!(
            "Added {} to complete a {}Commander Spellbook line{}.",
            card.name,
            if target_bracket.bracket >= 5 {
                "competitive "
            } else {
                ""
            },
            if reference_snapshot
                .combo_frequency_for_cards(&almost.cards)
                > 0
            {
                " observed in the reference corpus"
            } else {
                ""
            },
        ));
    }
}

    progress(GenerationProgress::new(
        90,
        "Validating game plan",
        "Checking combo density and reference-supported cards",
    ));

    // The combos the plan lists: the seeded ones plus up to 4 more complete
    // lines Spellbook finds that relate to this commander.
    let mut final_combos = seeded_combos.clone();

    if let Ok(report) = find_my_combos(&deck) {
        for line in report
            .variants
            .into_iter()
            .filter(|line| !line.almost)
            .filter(|line| {
                intelligence.combo_relevance(&line.cards, db) >= 140
            })
            .take(4)
        {
            if !final_combos
                .iter()
                .any(|existing| same_card_set(&existing.cards, &line.cards))
            {
                final_combos.push(GeneratedCombo {
                    cards: line.cards,
                    result: line.result,
                    source: "Commander Spellbook verified".to_string(),
                });
            }
        }
    }

progress(GenerationProgress::new(
    92,
    "Final combo audit",
    format!(
        "{} verified/relevant combo line(s) • {}",
        final_combos.len(),
        if final_combos.is_empty() {
            "none".to_string()
        } else {
            final_combos
                .iter()
                .map(|combo| {
                    format!(
                        "{} [{}]",
                        combo.cards.join(" + "),
                        combo.source,
                    )
                })
                .collect::<Vec<_>>()
                .join(" | ")
        },
    ),
));

progress(GenerationProgress::new(
    94,
    "Locking requested bracket",
    format!(
        "Validating finished list against Bracket {} requirements",
        target_bracket.bracket
    ),
));

progress(GenerationProgress::new(
    95,
    "Deck integrity test",
    "Checking 100-card size, singleton, colour identity, legality, structure and goldfish hands".to_string(),
));

// Step 9: integrity gate (integrity.rs): fixes size, singleton, colors and
// legality, then locks the bracket (power up or down). This can swap cards
// picked earlier; the swaps are only in the notes, not the report.
let (integrity_report, validated_bracket) = integrity_gate(
    &mut deck,
    target_bracket,
    db,
    &pool,
    &identity,
    &intelligence,
    &final_combos,
)?;

// Budget: swap the priciest cards for cheaper ones until the deck fits.
// Step 9b: budget. Swap the priciest unprotected cards (commanders and
// combo pieces are protected) for cheaper ones that score well for this
// deck (EDHREC synergy, theme, engine, popularity).
// TODO: runs after the bracket lock, so budget swaps can change the
// bracket again without being re-validated.
if budget.is_set() {
    let protected: HashSet<String> = deck
        .commanders
        .iter()
        .map(|e| e.name.to_ascii_lowercase())
        .chain(final_combos.iter().flat_map(|c| c.cards.iter()).map(|n| n.to_ascii_lowercase()))
        .chain(requested_names.iter().cloned())
        .collect();
    if budget.total.is_some() {
        progress(GenerationProgress::new(96, "Fitting the budget", budget.describe()));
    }
    let report = crate::budget::fit_total(
        &mut deck,
        &budget,
        db,
        &pool,
        &identity,
        &protected,
        |card| {
            let key = card.name.to_ascii_lowercase();
            let synergy = edhrec_signals
                .get(&key)
                .map(|s| (s.inclusion_pct() * 100.0) as i64 + (s.synergy * 100.0) as i64 + 5_000)
                .unwrap_or(0);
            let theme = if card_matches_theme(card, selected_theme) { 4_000 } else { 0 };
            let engine = if card_matches_engine(card, &commander_engine_terms) { 3_000 } else { 0 };
            let rank = 3_000 - (card.edhrec_rank.unwrap_or(30_000) as i64 / 10).min(3_000);
            synergy + theme + engine + rank
        },
        |card| allowed(card),
    )?;
    let mut note = format!("Budget: {}. ", budget.describe());
    if !report.swaps.is_empty() {
        note.push_str(&format!(
            "Swapped {} card(s) for cheaper ones to fit (${:.2} → ${:.2}): {}. ",
            report.swaps.len(),
            report.before,
            report.after,
            report.swaps.iter().take(12).map(|(a, b)| format!("{a} → {b}")).collect::<Vec<_>>().join(", ")
        ));
    }
    if budget.total.is_some() && !report.fits {
        note.push_str(&format!(
            "The deck still costs ${:.2}: its commander, combo pieces and cheapest picks don't fit under the total. ",
            report.after
        ));
    } else {
        note.push_str(&format!("Deck price: ${:.2}.", report.after));
    }
    notes.push(note.trim().to_string());
}

// The player's requests win over every later pass (bracket lock, integrity,
// budget): put back any requested card something cut.
if !requested_cards.is_empty() {
    let mut protect = requested_combos.clone();
    protect.push(GeneratedCombo { cards: requested_cards.iter().map(|c| c.name.clone()).collect(), result: String::new(), source: String::new() });
    protect.extend(final_combos.iter().cloned());
    for card in &requested_cards {
        let present = deck.main.iter().any(|e| e.name.eq_ignore_ascii_case(&card.name));
        if present {
            continue;
        }
        let restored = if card.type_line.to_ascii_lowercase().contains("land") {
            // A land replaces a basic.
            if let Some(i) = deck.main.iter().position(|e| db.get_card(&e.name).ok().flatten().is_some_and(|c| c.is_basic)) {
                if deck.main[i].quantity > 1 {
                    deck.main[i].quantity -= 1;
                } else {
                    deck.main.remove(i);
                }
                deck.main.push(DeckEntry { quantity: 1, name: card.name.clone(), ..Default::default() });
                true
            } else {
                false
            }
        } else {
            replace_low_plan_card(&mut deck, card, db, &tags, &protect)?
        };
        if !restored {
            request_notes.push(format!("Couldn't keep {} in the deck.", card.name));
        }
    }
}
if !requests.is_empty() {
    let mut note = String::from("Your requests: ");
    let kept: Vec<&str> = requested_cards
        .iter()
        .filter(|c| deck.main.iter().any(|e| e.name.eq_ignore_ascii_case(&c.name)))
        .map(|c| c.name.as_str())
        .collect();
    if !kept.is_empty() {
        note.push_str(&format!("included {}. ", kept.join(", ")));
    }
    let over_bracket: Vec<&str> = requested_cards
        .iter()
        .filter(|c| c.game_changer && target_bracket.max_game_changers() == 0 && kept.contains(&c.name.as_str()))
        .map(|c| c.name.as_str())
        .collect();
    if !over_bracket.is_empty() && format.is_commander() {
        note.push_str(&format!(
            "{} {} a Game Changer, which Bracket {} doesn't normally allow; your request came first. ",
            over_bracket.join(", "),
            if over_bracket.len() == 1 { "is" } else { "are" },
            target_bracket.bracket
        ));
    }
    if !requested_combos.is_empty() {
        note.push_str(&format!(
            "Combos: {}. ",
            requested_combos.iter().map(|c| c.cards.join(" + ")).collect::<Vec<_>>().join("; ")
        ));
    }
    if !requests.exclude.is_empty() {
        note.push_str(&format!("Left out on request: {}. ", requests.exclude.join(", ")));
    }
    if !requests.instructions.trim().is_empty() {
        note.push_str(&format!(
            "Instructions: \"{}\"{}. ",
            requests.instructions.trim(),
            if options.ai_assist { " (given to the AI model)" } else { " (turn on AI help to have the AI model follow them)" }
        ));
    }
    for n in &request_notes {
        note.push_str(n);
        note.push(' ');
    }
    notes.insert(0, note.trim().to_string());
}

// Win lines: which common ones the finished deck runs and why the rest
// aren't in it (the primer lists those as potential changes).
crate::win_lines::mark(&mut win_lines, &deck, db, |card| {
    crate::win_lines::why_blocked(card, format, &identity, &budget, game_changers_ok)
});
for line in win_lines.iter_mut().filter(|l| !l.included && l.kind == "combo") {
    if let Some(b) = line.bracket.filter(|b| *b > target_bracket.bracket && format.is_commander()) {
        line.why_not = Some(format!("Commander Spellbook rates it Bracket {b}, above this deck's Bracket {}", target_bracket.bracket));
    }
}
if !win_lines.is_empty() {
    let runs = win_lines.iter().filter(|l| l.included).count();
    notes.push(format!(
        "Win lines studied: {} common in this commander's reference decks; the deck runs {runs}. The rest are listed in the primer under Potential changes.",
        win_lines.len()
    ));
}

progress(GenerationProgress::new(
    97,
    "Bracket validated",
    format!(
        "{} • fast_mana={} • tutors={} • game_changers={} • ruthless_combos={} • extra_turns={} • mass_land_denial={}",
        validated_bracket.display_name(),
        validated_bracket.fast_mana,
        validated_bracket.tutors,
        validated_bracket.game_changers.len(),
        validated_bracket.ruthless_combos,
        validated_bracket.extra_turn_cards,
        validated_bracket.mass_land_denial,
    ),
));

// Step 10: the build notes shown with the game plan (bracket or power
// level first, then source summaries, references, combos, swaps).
notes.insert(
    0,
    if format.is_commander() {
        format!(
            "Target power: Bracket {} — {}. Final validation: {}. The generator treats the requested bracket as a hard post-build contract, not merely a scoring preference.",
            target_bracket.bracket,
            target_bracket.label(),
            validated_bracket.display_name(),
        )
    } else {
        format!(
            "Power level {} of 5 (Commander brackets don't apply to {}; this only set how aggressive the build is).",
            target_bracket.bracket,
            format.label(),
        )
    },
);
    notes.insert(
        1,
        format!(
            "Selected play-style theme: {}. Commander core: {}. Included {} direct style cards, {} commander-engine cards, and {} EDHREC-normal-build cards.",
            selected_theme,
            if commander_core_tags.is_empty() {
                "local inference".to_string()
            } else {
                commander_core_tags.join(" / ")
            },
            theme_cards,
            commander_engine_cards,
            edhrec_core_cards
        ),
    );
    notes.insert(
        2,
        "The generator reserves structural slots for ramp, card advantage, interaction, tutors, protection and recursion before filling flexible value cards.".to_string(),
    );
    notes.insert(
        3,
        "Commander-specific EDHREC inclusion/synergy and the commander's actual rules text are primary build signals; global EDHREC rank is only a secondary tiebreaker.".to_string(),
    );
    notes.insert(
        4,
        format!(
            "Deep Learn model confidence: {}%. Learned structural targets: {}.",
            intelligence.confidence,
            intelligence.role_summary(),
        ),
    );

    if format == Format::Pdh {
        notes.insert(
            1,
            if edhrec_build_cards.is_empty() {
                format!(
                    "PDHREC has no data for this commander yet, so cards came from {} Pauper EDH decklist(s) on Archidekt, rules text and overall play rate.",
                    reference_snapshot.deck_count()
                )
            } else {
                format!(
                    "Built from Pauper Commander sources: PDHREC inclusion and synergy for {} cards, plus {} Pauper EDH decklist(s) from Archidekt.",
                    edhrec_build_cards.len(),
                    reference_snapshot.deck_count()
                )
            },
        );
        notes.insert(
            0,
            "Pauper Commander: the commander was printed at uncommon and every other card at common (paper or MTGO printings). Built for 30 life and 16 commander damage. Mystic Remora, Rhystic Study, ante cards, Conspiracies and cards removed from constructed play are excluded.".to_string(),
        );
    }

    // Note about the rules-text search. TODO: deep learning runs the search
    // even when this checkbox is off, but this note only appears with the
    // checkbox; check `|| deep` here too.
    if options.deep_search {
        let included: Vec<String> = deck
            .main
            .iter()
            .filter_map(|e| deep_finds.get(&e.name.to_ascii_lowercase()))
            .filter(|f| f.edhrec_rank.map(|r| r > 15_000).unwrap_or(true))
            .map(|f| format!("{} ({})", f.name, f.reason))
            .collect();
        notes.push(format!(
            "Deep card search read the rules text of every card for {} and found {} that fit{}",
            theme_list(selected_theme).join(", "),
            deep_finds.len(),
            if included.is_empty() {
                "; none beat the established picks this time.".to_string()
            } else {
                format!(". Picked up from that search: {}.", included.join("; "))
            }
        ));
    }

    notes.extend(integrity_report.notes());

    if commanders.len() == 2 {
        notes.push(format!(
            "Detected paired commanders: {}.",
            commanders
                .iter()
                .map(|card| card.name.as_str())
                .collect::<Vec<_>>()
                .join(" + ")
        ));
    }

    if target_bracket.bracket >= 5 {
        notes.insert(
            1,
            match (format == Format::Pdh, reference_snapshot.cedh_deck_count > 0) {
                (true, true) => format!(
                    "Competitive Pauper sources first: card choices were scored mainly on {} competitive Pauper Commander list(s) from Archidekt for this commander; casual lists and PDHREC were used only as tiebreakers.",
                    reference_snapshot.cedh_deck_count
                ),
                (true, false) => "No competitive Pauper Commander lists were found for this commander, so the power-5 build fell back to the most efficient commons, PDHREC and casual Pauper EDH lists.".to_string(),
                (false, true) => format!(
                    "cEDH sources first: card choices were scored mainly on {} cEDH Decklist Database list(s) for this commander; casual decklists and EDHREC were used only as tiebreakers.",
                    reference_snapshot.cedh_deck_count
                ),
                (false, false) => "No cEDH Decklist Database lists were found for this commander, so the Bracket 5 build fell back to high-power public decklists and EDHREC.".to_string(),
            },
        );
    }

    if reference_snapshot.deck_count() > 0 {
        notes.push(format!(
            "Compared against {} cached/public reference deck(s) from {}{}. Deep Learn also measured recurring card pairs and {} distinct complete Spellbook combo line(s) across the corpus.",
            reference_snapshot.deck_count(),
            if format == Format::Pdh { "Archidekt's Pauper EDH lists" } else { "Archidekt, Moxfield" },
            match (target_bracket.bracket >= 5, format == Format::Pdh) {
                (true, true) => ", competitive Pauper Commander lists first",
                (true, false) => ", and cEDH Decklist Database-linked lists",
                _ => "",
            },
            reference_snapshot.combo_frequency.len(),
        ));

        let top_reference_cards = reference_snapshot
            .top_cards(12)
            .into_iter()
            .map(|(name, count)| format!("{name} ({count})"))
            .collect::<Vec<_>>()
            .join(", ");

        if !top_reference_cards.is_empty() {
            notes.push(format!(
                "Most common cards in the reference sample: {top_reference_cards}."
            ));
        }

        let top_reference_combos = reference_snapshot
            .top_combos(6)
            .into_iter()
            .map(|(cards, _, bracket, count)| {
                format!(
                    "{} ({count} deck(s){})",
                    cards.join(" + "),
                    bracket
                        .map(|value| format!(", Spellbook B{value}"))
                        .unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        if !top_reference_combos.is_empty() {
            notes.push(format!(
                "Recurring combos in the reference corpus: {top_reference_combos}."
            ));
        }
    }

    // Pauper Commander's evidence is PDHREC and Archidekt's Pauper EDH decks:
    // say so instead of EDHREC.
    if format == Format::Pdh {
        for note in notes.iter_mut() {
            if note.starts_with("Commander-specific EDHREC") {
                *note = "PDHREC's inclusion and synergy for this commander and the commander's own rules text are the main build signals; how widely a card is played overall only breaks ties.".to_string();
            } else {
                *note = note
                    .replace("EDHREC-normal-build cards", "PDHREC-backed cards")
                    .replace("from Archidekt, Moxfield", "from Archidekt (Pauper EDH format)");
            }
        }
    }

    // Step 11: the plan summary paragraph, honest sources, and (deep learning)
    // the build report, then hand back the deck and plan.
    let summary =
        build_game_plan_summary(&commanders, selected_theme, &primary_tags, &final_combos);

    // Only credit sources that contributed: a generic "public deck
    // references" entry stays only if that site supplied a deck.
    let decks_from = |site: &str| reference_snapshot.decks.iter().filter(|d| d.source.eq_ignore_ascii_case(site)).count();
    let sources: Vec<DeckSource> = intelligence
        .sources
        .iter()
        .filter(|s| !s.title.to_ascii_lowercase().contains("deck references") || decks_from(&s.source) > 0)
        .cloned()
        .collect();

    // The deep learning build report (deep_generation::GenerationReport):
    // what each source gave, tier counts for the pool and the deck, why each
    // card is in, the 8 best cards that missed, and weak spots worth knowing.
    let report = deep.then(|| {
        use crate::deep_generation::{GenerationReport, Pick, SourceLine, TierCounts};
        let status = |n: usize| if n > 0 { "used" } else { "empty" }.to_string();
        let mut report_sources = vec![
            SourceLine {
                // Pauper Commander's card data comes from PDHREC.
                name: if format == Format::Pdh { "PDHREC".into() } else { "EDHREC".into() },
                status: status(edhrec_build_cards.len()),
                detail: format!("{} cards with inclusion and synergy", edhrec_build_cards.len()),
            },
        ];
        let sites: &[&str] = if format == Format::Pdh {
            if target_bracket.bracket >= 5 { &["Archidekt", crate::references::COMPETITIVE_PAUPER_SOURCE] } else { &["Archidekt"] }
        } else if target_bracket.bracket >= 5 {
            &["Archidekt", "Moxfield", "cEDH Decklist Database"]
        } else {
            &["Archidekt", "Moxfield"]
        };
        for site in sites.iter().copied() {
            let n = decks_from(site);
            report_sources.push(SourceLine {
                name: format!("{site} decklists"),
                status: status(n),
                detail: if site == "Moxfield" && n == 0 {
                    "no decks: Moxfield's search ignores the commander filter".into()
                } else {
                    format!("{n} deck(s)")
                },
            });
        }
        report_sources.push(SourceLine {
            name: "Commander Spellbook".into(),
            status: spellbook_status.to_string(),
            detail: format!("{spellbook_found} combo(s) with this commander"),
        });
        report_sources.push(SourceLine { name: "Rules-text search".into(), status: status(deep_finds.len()), detail: format!("{} matching cards", deep_finds.len()) });
        report_sources.push(SourceLine {
            name: "Commander's rules text".into(),
            status: status(mechanics.len()),
            detail: if mechanics.is_empty() { "no clear mechanic".into() } else { mechanics.join(", ") },
        });
        report_sources.push(SourceLine {
            name: "AI model".into(),
            status: if !options.ai_assist { "off".into() } else if ai_hints.is_some() { "used".into() } else { "skipped".into() },
            detail: match &ai_hints {
                Some(h) => format!("{} key card(s) usable of {}", ai_key.len(), h.key_cards.len()),
                None if options.ai_assist => "didn't answer".into(),
                None => "turn on \"Let the AI model help\" when building".into(),
            },
        });

        let in_deck: HashSet<String> = deck.main.iter().map(|e| e.name.to_ascii_lowercase()).collect();
        let mut pool_tiers = TierCounts::default();
        for (card, ..) in &spell_pool {
            pool_tiers.add(evidence.get(&card.name).and_then(|(t, _)| *t));
        }
        let mut deck_tiers = TierCounts::default();
        let mut picks = Vec::new();
        for entry in &deck.main {
            let Some(card) = db.get_card(&entry.name).ok().flatten() else { continue };
            if card.type_line.to_ascii_lowercase().contains("land") {
                continue;
            }
            let (tier, reasons) = evidence.get(&card.name).cloned().unwrap_or((None, vec!["added to fill a role or fix the bracket".into()]));
            deck_tiers.add(tier);
            picks.push(Pick { card: card.name.clone(), tier, reasons });
        }
        picks.sort_by_key(|p| p.tier.map(|t| t as u8).unwrap_or(9));
        let mut ranked: Vec<_> = spell_pool.iter().filter(|(card, ..)| !in_deck.contains(&card.name.to_ascii_lowercase())).collect();
        ranked.sort_by_key(|(_, _, score, ..)| -*score);
        let near_misses = ranked
            .iter()
            .take(8)
            .map(|(card, ..)| {
                let (tier, reasons) = evidence.get(&card.name).cloned().unwrap_or((None, Vec::new()));
                Pick { card: card.name.clone(), tier, reasons }
            })
            .collect();

        let mut weaknesses = Vec::new();
        let refs = reference_snapshot.deck_count();
        if refs < 10 {
            weaknesses.push(format!("Only {refs} reference decklist(s) were found, so \"commonly played\" rests mostly on EDHREC."));
        }
        if edhrec_build_cards.is_empty() {
            weaknesses.push("No EDHREC data for this commander; picks lean on rules text and staples.".into());
        }
        if mechanics.is_empty() {
            weaknesses.push("The commander's rules text didn't point to a clear mechanic, so there's no engine bonus.".into());
        }
        if deck_tiers.synergy < 10 {
            weaknesses.push(format!("Only {} synergy cards made the deck; it may play like a generic pile.", deck_tiers.synergy));
        }
        if target_bracket.bracket >= 3 && final_combos.is_empty() {
            weaknesses.push("No combo made the final list; the deck wins through combat or value.".into());
        }
        if !options.ai_assist {
            weaknesses.push("Built without an AI model. Turning on AI help adds the model's key cards as another source.".into());
        }

        GenerationReport {
            engine: crate::deep_generation::GenerationEngine::DeepLearning.label().to_string(),
            sources: report_sources,
            themes: theme_lines.clone(),
            pool_tiers,
            deck_tiers,
            picks,
            near_misses,
            weaknesses,
        }
    });
    if let Some(r) = &report {
        progress(GenerationProgress::new(
            99,
            "Generation report",
            format!(
                "deck: {} commonly played • {} synergy • {} staples • {} could use • {} other | weak spots: {}",
                r.deck_tiers.core,
                r.deck_tiers.synergy,
                r.deck_tiers.staple,
                r.deck_tiers.could_use,
                r.deck_tiers.none,
                if r.weaknesses.is_empty() { "none".to_string() } else { r.weaknesses.join(" / ") },
            ),
        ));
    }

    progress(GenerationProgress::new(
        100,
        "Generation complete",
        format!(
            "{} • theme={} • bracket=B{} • main={} • sideboard={} • combos={} • sources={}",
            generated_name,
            selected_theme,
            validated_bracket.bracket,
            deck.main.iter().map(|entry| entry.quantity).sum::<u32>(),
            deck.sideboard.iter().map(|entry| entry.quantity).sum::<u32>(),
            final_combos.len(),
            sources.len(),
        ),
    ));

    Ok(GeneratedDeck {
        deck,
        plan: GamePlan {
            commanders: commanders
                .iter()
                .map(|card| card.name.clone())
                .collect(),
            identity,
            primary_tags,
            summary,
            combos: final_combos,
            notes,
            sources,
            report,
            win_lines,
        },
    })
}

/// Step 9's checker: repair the list (integrity.rs), lock the bracket, then
/// require a clean integrity pass. If the bracket lock swapped cards and that
/// broke something, lock and check once more. Commanders and combo pieces
/// are protected throughout. Returns the merged repair list (shown in the
/// notes) and the final bracket.
/// Final integrity gate: audit + repair the finished list, re-lock the
/// bracket if repairs changed the card pool, then require a clean pass.
fn integrity_gate(
    deck: &mut ParsedDeck,
    target: GenerationBracket,
    db: &CardDb,
    pool: &[Card],
    identity: &str,
    intelligence: &crate::intelligence::CommanderIntelligence,
    combos: &[GeneratedCombo],
) -> Result<(crate::integrity::IntegrityReport, BracketEstimate)> {
    let mut protected: Vec<String> =
        deck.commanders.iter().map(|entry| entry.name.clone()).collect();
    protected.extend(combos.iter().flat_map(|combo| combo.cards.iter().cloned()));

    let combo_lines: Vec<Vec<String>> =
        combos.iter().map(|combo| combo.cards.clone()).collect();

    let first = crate::integrity::audit_and_repair(
        deck, db, identity, &protected, &combo_lines,
    )?;

    let mut estimate =
        enforce_generation_bracket(deck, target, db, pool, identity, intelligence, combos)?;

    // Bracket re-lock may swap cards; re-run until both contracts hold.
    let mut report = crate::integrity::require_integrity(
        deck, db, identity, &protected, &combo_lines,
    )?;

    if !report.repairs.is_empty() {
        estimate = enforce_generation_bracket(
            deck, target, db, pool, identity, intelligence, combos,
        )?;
        report = crate::integrity::require_integrity(
            deck, db, identity, &protected, &combo_lines,
        )?;
    }

    // Keep the first pass's repairs visible to the user.
    let mut merged = report;
    let mut repairs = first.repairs;
    repairs.extend(merged.repairs);
    merged.repairs = repairs;

    Ok((merged, estimate))
}

/// Make the finished deck land on the requested bracket: estimate it (with
/// Spellbook combos), power up if it's under, power down if it's over. Each
/// pass logs to the BRACKET scope. If it still misses, Commander decks fail
/// loudly ("Commander Lab will not save a deck under the wrong bracket");
/// Pauper Commander just reports, since brackets are only a power dial there.
fn enforce_generation_bracket(
    deck: &mut ParsedDeck,
    target: GenerationBracket,
    db: &CardDb,
    pool: &[Card],
    identity: &str,
    intelligence: &crate::intelligence::CommanderIntelligence,
    protected_combos: &[GeneratedCombo],
) -> Result<BracketEstimate> {
    let mut estimate = estimate_bracket(deck, db)?;

    if let Ok(report) = find_my_combos(deck) {
        refine_with_combo_report(&mut estimate, &report);
    }

    logging::record(
        "BRACKET",
        format!(
            "VALIDATE START • deck={} • requested=B{} • detected=B{} • fast_mana={} • tutors={} • game_changers={} • ruthless_combos={}",
            deck.name,
            target.bracket,
            estimate.bracket,
            estimate.fast_mana,
            estimate.tutors,
            estimate.game_changers.len(),
            estimate.ruthless_combos,
        ),
    );

    if estimate.bracket < target.bracket {
        logging::record(
            "BRACKET",
            format!(
                "POWER UP REQUIRED • detected=B{} • target=B{}",
                estimate.bracket,
                target.bracket,
            ),
        );

        power_up_to_target(
            deck,
            target,
            db,
            pool,
            identity,
            intelligence,
            protected_combos,
        )?;

        estimate = estimate_bracket(deck, db)?;

        if let Ok(report) = find_my_combos(deck) {
            refine_with_combo_report(&mut estimate, &report);
        }

        logging::record(
            "BRACKET",
            format!(
                "POWER UP RESULT • detected=B{} • fast_mana={} • tutors={} • game_changers={} • ruthless_combos={}",
                estimate.bracket,
                estimate.fast_mana,
                estimate.tutors,
                estimate.game_changers.len(),
                estimate.ruthless_combos,
            ),
        );
    }

    if estimate.bracket > target.bracket {
        logging::record(
            "BRACKET",
            format!(
                "POWER DOWN REQUIRED • detected=B{} • target=B{}",
                estimate.bracket,
                target.bracket,
            ),
        );

        power_down_to_target(
            deck,
            target,
            db,
            pool,
            identity,
            intelligence,
            protected_combos,
        )?;

        estimate = estimate_bracket(deck, db)?;

        if let Ok(report) = find_my_combos(deck) {
            refine_with_combo_report(&mut estimate, &report);
        }

        logging::record(
            "BRACKET",
            format!(
                "POWER DOWN RESULT • detected=B{} • fast_mana={} • tutors={} • game_changers={} • ruthless_combos={}",
                estimate.bracket,
                estimate.fast_mana,
                estimate.tutors,
                estimate.game_changers.len(),
                estimate.ruthless_combos,
            ),
        );
    }

    if estimate.bracket != target.bracket {
        logging::record(
            "BRACKET",
            format!(
                "VALIDATE FAILED • requested=B{} • detected=B{} • fast_mana={} • tutors={} • game_changers={} • ruthless_combos={}",
                target.bracket,
                estimate.bracket,
                estimate.fast_mana,
                estimate.tutors,
                estimate.game_changers.len(),
                estimate.ruthless_combos,
            ),
        );

        if !deck.format.is_commander() {
            // Commander brackets don't apply outside Commander; the bracket
            // is only a power dial there.
            return Ok(estimate);
        }
        return Err(anyhow!(
            "Requested Bracket {} but the completed deck validates as {}. \
             Commander Lab will not save a deck under the wrong bracket. \
             Final profile: {} Game Changer(s), {} fast-mana card(s), \
             {} tutor(s), {} ruthless combo(s). Try another commander/theme \
             or refresh reference data.",
            target.bracket,
            estimate.display_name(),
            estimate.game_changers.len(),
            estimate.fast_mana,
            estimate.tutors,
            estimate.ruthless_combos,
        ));
    }

    logging::record(
        "BRACKET",
        format!(
            "VALIDATE SUCCESS • B{} • fast_mana={} • tutors={} • game_changers={} • ruthless_combos={}",
            estimate.bracket,
            estimate.fast_mana,
            estimate.tutors,
            estimate.game_changers.len(),
            estimate.ruthless_combos,
        ),
    );

    Ok(estimate)
}

/// Raise a deck to the bracket's minimums (Game Changers, tutors, fast mana)
/// one swap at a time, up to 40 swaps. Each swap brings in the best candidate
/// that fills a missing requirement (cards filling two or three at once
/// score much higher) and removes the least-supported unprotected nonland
/// card that isn't itself a Game Changer, tutor or fast mana.
fn power_up_to_target(
    deck: &mut ParsedDeck,
    target: GenerationBracket,
    db: &CardDb,
    pool: &[Card],
    identity: &str,
    intelligence: &crate::intelligence::CommanderIntelligence,
    protected_combos: &[GeneratedCombo],
) -> Result<()> {
    let protected = protected_combos
        .iter()
        .flat_map(|combo| combo.cards.iter())
        .chain(deck.commanders.iter().map(|entry| &entry.name))
        .map(|name| name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    let mut attempts = 0usize;

    loop {
        attempts += 1;

        if attempts > 40 {
            break;
        }

        let profile = generated_power_profile(deck, db)?;

        let needs_game_changer =
            profile.game_changers < target.minimum_game_changers();
        let needs_tutor =
            profile.tutors < target.minimum_tutors();
        let needs_fast =
            profile.fast_mana < target.minimum_fast_mana();

        if !needs_game_changer && !needs_tutor && !needs_fast {
            break;
        }

        let used = deck
            .commanders
            .iter()
            .chain(deck.main.iter())
            .map(|entry| entry.name.to_ascii_lowercase())
            .collect::<HashSet<_>>();

        let mut candidates = pool
            .iter()
            .filter(|card| {
                identity_subset(&card.color_identity, identity)
                    && !card.type_line.to_ascii_lowercase().contains("land")
                    && !used.contains(&card.name.to_ascii_lowercase())
            })
            .filter_map(|card| {
                let roles = classify_card(card);
                let is_fast = is_fast_mana_card(card, roles);

                let qualifies = (needs_game_changer && card.game_changer)
                    || (needs_tutor && roles.tutor)
                    || (needs_fast && is_fast);

                if !qualifies {
                    return None;
                }

                let support =
                    intelligence.support_for_card(card, roles);

                let multi_role = (card.game_changer as i32)
                    + (roles.tutor as i32)
                    + (is_fast as i32);

                Some((
                    card,
                    support.score
                        + support.reference_frequency as i32 * 45
                        + multi_role * 180
                        - (card.cmc * 10.0) as i32,
                ))
            })
            .collect::<Vec<_>>();

        candidates.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| a.0.cmc.total_cmp(&b.0.cmc))
                .then_with(|| a.0.name.cmp(&b.0.name))
        });

        let Some((candidate, _)) = candidates.first() else {
            break;
        };

        // Pick what to cut: lowest learned support, with structural roles
        // (ramp, draw, removal…) worth an extra 75 so they're kept.
        let victim = deck
            .main
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                if protected.contains(&entry.name.to_ascii_lowercase()) {
                    return None;
                }

                let card = db.get_card(&entry.name).ok().flatten()?;

                if card.type_line.to_ascii_lowercase().contains("land") {
                    return None;
                }

                let roles = classify_card(&card);

                // Never remove existing cards that are already helping satisfy
                // the requested power profile.
                if card.game_changer
                    || roles.tutor
                    || is_fast_mana_card(&card, roles)
                {
                    return None;
                }

                let support =
                    intelligence.support_for_card(&card, roles);

                Some((
                    index,
                    support.score
                        + support.reference_frequency as i32 * 20
                        + if has_structural_role(roles) { 75 } else { 0 },
                ))
            })
            .min_by_key(|(_, score)| *score);

        let Some((victim_index, _)) = victim else {
            break;
        };

        let removed_name = deck.main[victim_index].name.clone();

        logging::record(
            "BRACKET",
            format!(
                "POWER UP SWAP • OUT={} • IN={} • game_changer={} • tutor={} • fast_mana={}",
                removed_name,
                candidate.name,
                candidate.game_changer,
                classify_card(candidate).tutor,
                is_fast_mana_card(candidate, classify_card(candidate)),
            ),
        );

        deck.main[victim_index] = DeckEntry {
            quantity: 1,
            name: candidate.name.clone(), ..Default::default() };
    }

    Ok(())
}

/// Lower a deck that validates above the target bracket: up to 30 rounds of
/// cutting the card most responsible for the overshoot (Game Changers at
/// 1–2, excess tutors or fast mana at 2–4) and replacing it with a
/// same-role card that doesn't push the bracket.
/// TODO: each round asks Commander Spellbook for combos again (a network
/// call), so a stubborn deck can make 30 requests.
fn power_down_to_target(
    deck: &mut ParsedDeck,
    target: GenerationBracket,
    db: &CardDb,
    pool: &[Card],
    identity: &str,
    intelligence: &crate::intelligence::CommanderIntelligence,
    protected_combos: &[GeneratedCombo],
) -> Result<()> {
    let protected = protected_combos
        .iter()
        .flat_map(|combo| combo.cards.iter())
        .chain(deck.commanders.iter().map(|entry| &entry.name))
        .map(|name| name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    for _ in 0..30 {
        let mut estimate = estimate_bracket(deck, db)?;

        if let Ok(report) = find_my_combos(deck) {
            refine_with_combo_report(&mut estimate, &report);
        }

        if estimate.bracket <= target.bracket {
            break;
        }

        let profile = generated_power_profile(deck, db)?;

        let victim = deck
            .main
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                if protected.contains(&entry.name.to_ascii_lowercase()) {
                    return None;
                }

                let card = db.get_card(&entry.name).ok().flatten()?;
                let roles = classify_card(&card);
                let fast = is_fast_mana_card(&card, roles);

                let contributes_to_overshoot = match target.bracket {
                    1 => card.game_changer || roles.tutor || fast,
                    2 => {
                        card.game_changer
                            || (profile.tutors >= 5 && roles.tutor)
                            || (profile.fast_mana >= 4 && fast)
                    }
                    3 => {
                        (profile.game_changers >= 4 && card.game_changer)
                            || (profile.fast_mana >= 5
                                && profile.tutors >= 7
                                && fast)
                    }
                    4 => {
                        estimate.bracket >= 5
                            && (roles.tutor || fast)
                            && !card.game_changer
                    }
                    _ => false,
                };

                if !contributes_to_overshoot {
                    return None;
                }

                let support =
                    intelligence.support_for_card(&card, roles);

                Some((index, support.score))
            })
            .min_by_key(|(_, score)| *score);

        let Some((victim_index, _)) = victim else {
            break;
        };

        let used = deck
            .commanders
            .iter()
            .chain(deck.main.iter())
            .map(|entry| entry.name.to_ascii_lowercase())
            .collect::<HashSet<_>>();

        let replacement = pool
            .iter()
            .filter(|card| {
                identity_subset(&card.color_identity, identity)
                    && !card.type_line.to_ascii_lowercase().contains("land")
                    && !used.contains(&card.name.to_ascii_lowercase())
            })
            .filter_map(|card| {
                let roles = classify_card(card);
                let fast = is_fast_mana_card(card, roles);

                let acceptable = match target.bracket {
                    1 => !card.game_changer && !roles.tutor && !fast,
                    2 => !card.game_changer && !fast,
                    3 => !card.game_changer || profile.game_changers < 3,
                    4 => !(roles.tutor && fast),
                    _ => true,
                };

                if !acceptable {
                    return None;
                }

                let support =
                    intelligence.support_for_card(card, roles);

                Some((
                    card,
                    support.score
                        + support.reference_frequency as i32 * 30
                        + if has_structural_role(roles) { 35 } else { 0 },
                ))
            })
            .max_by_key(|(_, score)| *score);

        let Some((replacement, _)) = replacement else {
            break;
        };

        let removed_name = deck.main[victim_index].name.clone();

        logging::record(
            "BRACKET",
            format!(
                "POWER DOWN SWAP • OUT={} • IN={} • replacement_support={}",
                removed_name,
                replacement.name,
                intelligence
                    .support_for_card(
                        replacement,
                        classify_card(replacement),
                    )
                    .score,
            ),
        );

        deck.main[victim_index] = DeckEntry {
            quantity: 1,
            name: replacement.name.clone(), ..Default::default() };
    }

    Ok(())
}

/// Counts used by the bracket lock: Game Changers, tutors, fast mana.
#[derive(Debug, Clone, Copy, Default)]
struct GeneratedPowerProfile {
    game_changers: u32,
    tutors: u32,
    fast_mana: u32,
}

/// Count Game Changers, tutors and fast mana in the whole deck (commanders
/// included), the three things the bracket rules weigh most.
fn generated_power_profile(
    deck: &ParsedDeck,
    db: &CardDb,
) -> Result<GeneratedPowerProfile> {
    let mut profile = GeneratedPowerProfile::default();

    for entry in deck.commanders.iter().chain(deck.main.iter()) {
        let Some(card) = db.get_card(&entry.name)? else {
            continue;
        };

        let roles = classify_card(&card);

        if card.game_changer {
            profile.game_changers += entry.quantity;
        }
        if roles.tutor {
            profile.tutors += entry.quantity;
        }
        if is_fast_mana_card(&card, roles) {
            profile.fast_mana += entry.quantity;
        }
    }

    Ok(profile)
}

/// "Fast mana": a nonland that makes mana for 0–1 (Sol Ring, Dark Ritual,
/// Mana Vault…). Same rule as `fast_mana_score`; keep the two in sync.
fn is_fast_mana_card(card: &Card, roles: CardRoles) -> bool {
    let oracle = card.oracle_text.to_ascii_lowercase();
    let type_line = card.type_line.to_ascii_lowercase();

    let makes_mana = roles.ramp
        || oracle.contains("add {")
        || oracle.contains("add one mana")
        || oracle.contains("add two mana")
        || oracle.contains("add three mana");

    !type_line.contains("land")
        && card.cmc <= 1.0
        && makes_mana
}

/// Render a plan as the `.plan.md` text: commanders, colors, tags, plan,
/// combos, notes, the build report (deep learning) and sources.
pub fn game_plan_markdown(plan: &GamePlan) -> String {
    let mut out = String::new();

    out.push_str("# Commander Lab Game Plan\n\n");
    out.push_str(&format!(
        "**Commander(s):** {}\n\n",
        plan.commanders.join(" + ")
    ));
    out.push_str(&format!("**Color identity:** {}\n\n", plan.identity));

    if !plan.primary_tags.is_empty() {
        out.push_str(&format!(
            "**Primary plan tags:** {}\n\n",
            plan.primary_tags.join(", ")
        ));
    }

    out.push_str("## Plan\n\n");
    out.push_str(&plan.summary);
    out.push_str("\n\n## Win Packages\n\n");

    if plan.combos.is_empty() {
        out.push_str("- No verified/seeded combo package was available.\n");
    } else {
        for combo in &plan.combos {
            out.push_str(&format!(
                "- {} → {} ({})\n",
                combo.cards.join(" + "),
                combo.result,
                combo.source
            ));
        }
    }

    if !plan.win_lines.is_empty() {
        out.push_str("\n## Common Win Lines\n\n");
        for line in plan.win_lines.iter().filter(|l| l.included) {
            out.push_str(&format!("- In the deck: {}\n", crate::win_lines::describe(line)));
        }
        let skipped: Vec<_> = plan.win_lines.iter().filter(|l| !l.included).collect();
        if !skipped.is_empty() {
            out.push_str("\n## Potential Changes\n\nCommon in this commander's decklists but not in this deck:\n\n");
            for line in skipped {
                out.push_str(&format!(
                    "- {}. Add: {}.{}\n",
                    crate::win_lines::describe(line),
                    line.missing.join(", "),
                    line.why_not.as_ref().map(|w| format!(" Left out: {w}.")).unwrap_or_default()
                ));
            }
        }
    }

    out.push_str("\n## Build Notes\n\n");
    for note in &plan.notes {
        out.push_str(&format!("- {note}\n"));
    }

    if let Some(report) = &plan.report {
        out.push_str(&report.markdown());
    }

    out.push_str("\n## Sources\n\n");
    for source in &plan.sources {
        out.push_str(&format!(
            "- {} — {}\n  {}\n",
            source.source,
            source.title,
            source.url,
        ));
    }

    out
}

/// A hand-written two/three-card combo the generator may add as a last
/// resort. `fast`/`cedh` gate which brackets may use it.
/// `preferred_tags` is not used anywhere yet (the compiler warns about it).
#[derive(Debug, Clone)]
struct ComboTemplate {
    cards: &'static [&'static str],
    result: &'static str,
    preferred_tags: &'static [&'static str],
    fast: bool,
    cedh: bool,
}

/// The built-in combo list. Only used when neither reference decks nor
/// Commander Spellbook supplied a combo for the commander.
fn combo_templates() -> Vec<ComboTemplate> {
    vec![
        ComboTemplate {
            cards: &["Thassa's Oracle", "Demonic Consultation"],
            result: "Win by exiling the library and resolving Thassa's Oracle.",
            preferred_tags: &["Mill", "Spellslinger"],
            fast: true,
            cedh: true,
        },
        ComboTemplate {
            cards: &["Exquisite Blood", "Sanguine Bond"],
            result: "Loop life gain and life loss to drain the table.",
            preferred_tags: &["Lifegain", "Aristocrats"],
            fast: false,
            cedh: false,
        },
        ComboTemplate {
            cards: &["Heliod, Sun-Crowned", "Walking Ballista"],
            result: "Repeat damage using lifelink and +1/+1 counters.",
            preferred_tags: &["Counters", "Lifegain"],
            fast: true,
            cedh: false,
        },
        ComboTemplate {
            cards: &["Dualcaster Mage", "Twinflame"],
            result: "Create arbitrarily many hasty Dualcaster Mage tokens.",
            preferred_tags: &["Spellslinger", "Tokens"],
            fast: true,
            cedh: false,
        },
        ComboTemplate {
            cards: &["Kiki-Jiki, Mirror Breaker", "Zealous Conscripts"],
            result: "Create arbitrarily many hasty Zealous Conscripts.",
            preferred_tags: &["Tokens", "Blink"],
            fast: false,
            cedh: false,
        },
        ComboTemplate {
            cards: &["Devoted Druid", "Vizier of Remedies"],
            result: "Produce arbitrarily large amounts of green mana.",
            preferred_tags: &["Counters", "Big Mana"],
            fast: false,
            cedh: false,
        },
        ComboTemplate {
            cards: &["Isochron Scepter", "Dramatic Reversal"],
            result: "With enough nonland mana, repeatedly untap mana sources for infinite mana.",
            preferred_tags: &["Artifacts", "Spellslinger", "Storm"],
            fast: true,
            cedh: true,
        },
        ComboTemplate {
            cards: &["Deadeye Navigator", "Peregrine Drake"],
            result: "Generate arbitrarily large amounts of mana through repeated blink loops.",
            preferred_tags: &["Blink", "Big Mana"],
            fast: false,
            cedh: false,
        },
        ComboTemplate {
            cards: &["Scurry Oak", "Ivy Lane Denizen"],
            result: "Create arbitrarily many Squirrel tokens and +1/+1 counters.",
            preferred_tags: &["Tokens", "Counters"],
            fast: false,
            cedh: false,
        },
        ComboTemplate {
            cards: &["Rest in Peace", "Helm of Obedience"],
            result: "Exile an opponent's library.",
            preferred_tags: &["Enchantments", "Stax"],
            fast: false,
            cedh: false,
        },
    ]
}

/// Turn a template's names into cards, or None if any piece is missing,
/// illegal in the format or outside the colors.
fn resolve_combo_template(
    template: &ComboTemplate,
    identity: &str,
    format: Format,
    db: &CardDb,
) -> Result<Option<Vec<Card>>> {
    let mut cards = Vec::new();

    for name in template.cards {
        let Some(card) = db.get_card(name)? else {
            return Ok(None);
        };

        if format.card_allowed(&card).is_err()
            || !identity_subset(&card.color_identity, identity)
        {
            return Ok(None);
        }

        cards.push(card);
    }

    Ok(Some(cards))
}


/// Which pieces of a combo the deck doesn't run yet.
fn missing_combo_cards(cards: &[String], deck: &ParsedDeck) -> Vec<String> {
    let existing = deck
        .commanders
        .iter()
        .chain(deck.main.iter())
        .map(|entry| entry.name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    cards
        .iter()
        .filter(|name| !existing.contains(&name.to_ascii_lowercase()))
        .cloned()
        .collect()
}

/// Make room for a combo piece: replace the nonland card with the lowest
/// base score that isn't a combo piece or a ramp/draw/removal/wipe card.
/// Returns false if nothing could be replaced.
fn replace_low_plan_card(
    deck: &mut ParsedDeck,
    new_card: &Card,
    db: &CardDb,
    tags: &[ArchetypeTag],
    protected_combos: &[GeneratedCombo],
) -> Result<bool> {
    let protected = protected_combos
        .iter()
        .flat_map(|combo| combo.cards.iter())
        .map(|name| name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    let mut worst: Option<(usize, i32)> = None;

    for (index, entry) in deck.main.iter().enumerate() {
        if protected.contains(&entry.name.to_ascii_lowercase()) {
            continue;
        }

        let Some(card) = db.get_card(&entry.name)? else {
            continue;
        };

        if card.type_line.to_ascii_lowercase().contains("land") {
            continue;
        }

        let roles = classify_card(&card);

        if roles.ramp
            || roles.draw
            || roles.removal
            || roles.counter
            || roles.wipe
        {
            continue;
        }

        let score = base_card_score(&card, tags);

        if worst
            .as_ref()
            .map(|(_, current)| score < *current)
            .unwrap_or(true)
        {
            worst = Some((index, score));
        }
    }

    let Some((index, _)) = worst else {
        return Ok(false);
    };

    deck.main[index] = DeckEntry {
        quantity: 1,
        name: new_card.name.clone(), ..Default::default() };

    Ok(true)
}

/// Same combo regardless of order or capitalization.
fn same_card_set(a: &[String], b: &[String]) -> bool {
    let left = a
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let right = b
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    left == right
}

/// The plan paragraph: establish the commander, build the theme engine
/// (plus up to two supporting tags), close with the first two combos or
/// "a protected win" when there are none. It's a template, not analysis;
/// the build notes and report carry the specifics.
fn build_game_plan_summary(
    commanders: &[Card],
    selected_theme: &str,
    primary_tags: &[String],
    combos: &[GeneratedCombo],
) -> String {
    let commander_names = commanders
        .iter()
        .map(|card| card.name.as_str())
        .collect::<Vec<_>>()
        .join(" + ");

    let chosen = theme_list(selected_theme);
    let supporting = primary_tags
        .iter()
        .filter(|tag| !chosen.iter().any(|t| tag.eq_ignore_ascii_case(t)))
        .take(2)
        .cloned()
        .collect::<Vec<_>>();

    let plan = if supporting.is_empty() {
        selected_theme.to_string()
    } else {
        format!("{} with {}", selected_theme, supporting.join(" / "))
    };

    let wins = if combos.is_empty() {
        "convert the commander engine into a protected win".to_string()
    } else {
        combos
            .iter()
            .take(2)
            .map(|combo| combo.cards.join(" + "))
            .collect::<Vec<_>>()
            .join(" or ")
    };

    format!(
        "Use early ramp and card selection to establish {commander_names}. \
         Develop the {plan} engine while preserving efficient interaction. \
         Tutors and card-selection effects should find the engine or win piece \
         that unlocks the most cards already in hand. Protect the commander \
         engine instead of spending resources on unrelated value. Close through \
         {wins}."
    )
}

/// Small bonus (10–12 per tag) for cards that literally mention one of the
/// deck's primary tags ("graveyard", "token", artifact type…).
fn plan_bonus(card: &Card, primary_tags: &[String]) -> i32 {
    let oracle = card.oracle_text.to_ascii_lowercase();
    let type_line = card.type_line.to_ascii_lowercase();
    let mut bonus = 0;

    for tag in primary_tags {
        match tag.to_ascii_lowercase().as_str() {
            "graveyard" if oracle.contains("graveyard") => bonus += 12,
            "sacrifice" if oracle.contains("sacrifice") => bonus += 12,
            "artifacts" if type_line.contains("artifact") => bonus += 12,
            "tokens" if oracle.contains("token") => bonus += 12,
            "lands" if oracle.contains("land") => bonus += 10,
            "spellslinger"
                if oracle.contains("instant") || oracle.contains("sorcery") =>
            {
                bonus += 10
            }
            "enchantments" if type_line.contains("enchantment") => bonus += 10,
            "equipment" if type_line.contains("equipment") => bonus += 10,
            _ => {}
        }
    }

    bonus
}


/// True when the card's name, type or text contains one of the commander's
/// engine words (intelligence.rs). Empty word list means no engine bonus.
fn card_matches_engine(card: &Card, terms: &[String]) -> bool {
    if terms.is_empty() {
        return false;
    }

    let haystack = format!(
        "{} {} {}",
        card.name.to_ascii_lowercase(),
        card.type_line.to_ascii_lowercase(),
        card.oracle_text.to_ascii_lowercase(),
    );

    terms.iter().any(|term| haystack.contains(term))
}




/// How much a nonbasic land belongs in this deck: EDHREC inclusion and
/// synergy, how many reference decks run it, engine words. Lands with no
/// evidence score 0 and only fill in after those that have some.
fn land_build_score(
    card: &Card,
    edhrec_signals: &HashMap<String, EdhrecCardSignal>,
    reference_snapshot: &crate::references::ReferenceSnapshot,
    commander_engine_terms: &[String],
) -> i32 {
    let mut score = 0;

    if let Some(signal) =
        edhrec_signals.get(&card.name.to_ascii_lowercase())
    {
        score += 180
            + (signal.inclusion_pct() * 140.0) as i32
            + (signal.synergy.max(0.0) * 120.0) as i32;
    }

    score += reference_snapshot.frequency(&card.name) as i32 * 80;

    if card_matches_engine(card, commander_engine_terms) {
        score += 100;
    }

    score
}

thread_local! {
    static MATCHERS: std::cell::RefCell<HashMap<String, std::rc::Rc<ThemeMatcher>>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Oracle-text matcher for a theme, compiled once per thread.
fn theme_matcher(theme: &str) -> std::rc::Rc<ThemeMatcher> {
    MATCHERS.with(|cache| {
        cache
            .borrow_mut()
            .entry(theme.to_ascii_lowercase())
            .or_insert_with(|| std::rc::Rc::new(ThemeMatcher::new(theme)))
            .clone()
    })
}

/// Does the card fit any of the chosen themes ("Tokens + Treasure")?
/// True when the card fits any of the selected play styles.
fn card_matches_theme(card: &Card, selected_theme: &str) -> bool {
    theme_list(selected_theme)
        .iter()
        .any(|theme| card_matches_single_theme(card, theme))
}

/// One theme. In order: the card's archetype tags, the oracle_search rules
/// for the theme, a few special cases below, and finally the theme's own
/// words appearing in the card text (themes of 4+ letters).
/// TODO: the fallbacks are broad ("tokens" matches any "token" mention;
/// "big mana" matches every 7+ mana card).
fn card_matches_single_theme(card: &Card, selected_theme: &str) -> bool {
    if tag_names_for_card(card)
        .iter()
        .any(|tag| tag.eq_ignore_ascii_case(selected_theme))
    {
        return true;
    }

    if theme_matcher(selected_theme).matches(card) {
        return true;
    }

    let theme = selected_theme.trim().to_ascii_lowercase();
    let oracle = card.oracle_text.to_ascii_lowercase();
    let type_line = card.type_line.to_ascii_lowercase();
    let keywords = card.keywords.to_ascii_lowercase();

    match theme.as_str() {
        "eldrazi" => {
            type_line.contains("eldrazi") || oracle.contains("eldrazi")
        }
        "devoid" => {
            keywords.contains("devoid") || oracle.contains("devoid")
        }
        "ramp" => classify_card(card).ramp,
        "big mana" => {
            card.cmc >= 7.0
                || (classify_card(card).ramp
                    && (oracle.contains("add two")
                        || oracle.contains("add three")
                        || oracle.contains("add {c}{c}")))
        }
        "spell copy" | "copy" => {
            oracle.contains("copy target spell")
                || oracle.contains("copy that spell")
                || oracle.contains("copy it")
                || oracle.contains("copy each")
        }
        "tokens" => {
            oracle.contains("token") || oracle.contains("tokens")
        }
        "stax" => {
            oracle.contains("players can't")
                || oracle.contains("can't cast more than one")
                || oracle.contains("spells cost {1} more")
                || oracle.contains("spells cost {2} more")
                || oracle.contains("doesn't untap")
                || oracle.contains("skip their untap")
                || oracle.contains("can't search")
                || oracle.contains("activated abilities")
                    && oracle.contains("can't be activated")
                || oracle.contains("enter the battlefield tapped")
                    && (oracle.contains("artifact")
                        || oracle.contains("land"))
        }
        "typal" | "tribal" => {
            oracle.contains("creature type")
                || oracle.contains("creatures you control")
        }
        _ => {
            if theme.len() < 4 {
                false
            } else {
                type_line.contains(&theme)
                    || oracle.contains(&theme)
                    || keywords.contains(&theme)
            }
        }
    }
}

/// Ramp, draw, removal, counterspell, tutor, wipe, protection or recursion:
/// the jobs every deck needs regardless of theme.
fn has_structural_role(roles: CardRoles) -> bool {
    roles.ramp
        || roles.draw
        || roles.removal
        || roles.counter
        || roles.tutor
        || roles.wipe
        || roles.protection
        || roles.recursion
}

/// Score from how many reference decks run the card; matters more at
/// higher brackets (proven lists count for more as power goes up).
fn reference_card_score(
    frequency: u32,
    reference_decks: usize,
    target: GenerationBracket,
) -> i32 {
    if reference_decks == 0 || frequency == 0 {
        return 0;
    }

    let share = frequency as f64 / reference_decks as f64;

    let multiplier = match target.bracket {
        1 => 6.0,
        2 => 10.0,
        3 => 18.0,
        4 => 28.0,
        _ => 42.0,
    };

    (share * multiplier).round() as i32
}

/// Push Game Changers and tutors away at low brackets and toward high ones;
/// at bracket 4+ cheap cards get a small bump, at bracket 1 a small dip so
/// casual decks aren't just efficient staples.
fn bracket_card_score(
    card: &Card,
    roles: CardRoles,
    target: GenerationBracket,
) -> i32 {
    let mut score = 0;

    if card.game_changer {
        score += match target.bracket {
            1 | 2 => -250,
            3 => -5,
            4 => 25,
            _ => 55,
        };
    }

    if roles.tutor {
        score += match target.bracket {
            1 => -150,
            2 => -25,
            3 => 5,
            4 => 20,
            _ => 35,
        };
    }

    if target.bracket == 1 && card.cmc <= 2.0 {
        // Exhibition should not simply become a pile of hyper-efficient
        // staples. Keep synergy meaningful without aggressively minimizing MV.
        score -= 8;
    }

    if target.bracket >= 4 && card.cmc <= 2.0 {
        score += 8;
    }

    score
}

/// The plainest score: fit to the deck's tags (×2), overall popularity
/// (EDHREC rank buckets) and cheapness (mana value buckets).
fn base_card_score(card: &Card, tags: &[ArchetypeTag]) -> i32 {
    let rank = match card.edhrec_rank {
        Some(rank) if rank <= 250 => 32,
        Some(rank) if rank <= 1_000 => 27,
        Some(rank) if rank <= 5_000 => 21,
        Some(rank) if rank <= 15_000 => 14,
        Some(rank) if rank <= 40_000 => 6,
        _ => 0,
    };

    let efficiency = if card.cmc <= 1.0 {
        15
    } else if card.cmc <= 2.0 {
        12
    } else if card.cmc <= 3.0 {
        8
    } else if card.cmc <= 4.0 {
        4
    } else {
        0
    };

    tag_fit_score(card, tags) * 2 + rank + efficiency
}

/// "The deck still needs this": bonus for each role the deck is short on
/// compared with what this commander's decks usually run (learned targets
/// from intelligence.rs). Extra tutors beyond the target are penalised at
/// low brackets.
fn need_bonus(
    roles: CardRoles,
    counts: &RoleCounts,
    target: GenerationBracket,
    learned: &LearnedRoleTargets,
) -> i32 {
    let mut score = 0;

    if roles.ramp && counts.ramp < learned.ramp {
        score += 42;
    }
    if roles.draw && counts.draw < learned.draw {
        score += 42;
    }
    if (roles.removal || roles.counter) && counts.interaction < learned.interaction {
        score += 38;
    }
    if roles.tutor && counts.tutor < learned.tutor {
        score += 32;
    } else if roles.tutor && counts.tutor >= learned.tutor {
        score -= match target.bracket {
            1 => 160,
            2 => 70,
            3 => 20,
            _ => 0,
        };
    }
    if roles.wipe && counts.wipe < learned.wipe {
        score += 22;
    }
    if roles.protection && counts.protection < learned.protection {
        score += 22;
    }
    if roles.recursion && counts.recursion < learned.recursion {
        score += 16;
    }

    score
}

/// Running count of each role among cards picked so far; `need_bonus`
/// compares it with the learned targets.
#[derive(Default)]
struct RoleCounts {
    ramp: u32,
    draw: u32,
    interaction: u32,
    tutor: u32,
    wipe: u32,
    protection: u32,
    recursion: u32,
    game_changers: u32,
}

impl RoleCounts {
    /// Count a picked card, Game Changers included.
    fn add_card(&mut self, card: &Card, roles: CardRoles) {
        if card.game_changer {
            self.game_changers += 1;
        }
        self.add(roles);
    }

    /// Count a card's roles (removal and counterspells both count as
    /// interaction).
    fn add(&mut self, roles: CardRoles) {
        if roles.ramp {
            self.ramp += 1;
        }
        if roles.draw {
            self.draw += 1;
        }
        if roles.removal || roles.counter {
            self.interaction += 1;
        }
        if roles.tutor {
            self.tutor += 1;
        }
        if roles.wipe {
            self.wipe += 1;
        }
        if roles.protection {
            self.protection += 1;
        }
        if roles.recursion {
            self.recursion += 1;
        }
    }
}

/// Fill the remaining land slots with basics split evenly across the
/// colors (Wastes for colorless). Doesn't weigh by how many of each color's
/// mana symbols the deck has.
fn add_basics(main: &mut Vec<DeckEntry>, identity: &str, total: usize) {
    if total == 0 {
        return;
    }

    let basics = identity
        .chars()
        .filter_map(|color| match color {
            'W' => Some("Plains"),
            'U' => Some("Island"),
            'B' => Some("Swamp"),
            'R' => Some("Mountain"),
            'G' => Some("Forest"),
            _ => None,
        })
        .collect::<Vec<_>>();

    if basics.is_empty() {
        main.push(DeckEntry {
            quantity: total as u32,
            name: "Wastes".to_string(), ..Default::default() });
        return;
    }

    let each = total / basics.len();
    let remainder = total % basics.len();

    for (index, basic) in basics.iter().enumerate() {
        let quantity = each + usize::from(index < remainder);

        if quantity > 0 {
            main.push(DeckEntry {
                quantity: quantity as u32,
                name: (*basic).to_string(), ..Default::default() });
        }
    }
}
