use crate::{
    analysis::{
        classify_card, has_copy_rule_override, identity_subset, CardRoles,
    },
    archetype::{detect_deck_tags, tag_fit_score, tag_names_for_card, theme_list, ArchetypeTag},
    bracket::{estimate_bracket, refine_with_combo_report, BracketEstimate},
    commander::{combined_identity, resolve_generation_commanders},
    data::{Card, CardDb},
    deck::{DeckEntry, ParsedDeck},
    edhrec::{commander_themes, EdhrecCardSignal},
    logging,
    intelligence::{
        learn_commander_intelligence, LearnedRoleTargets,
    },
    spellbook::find_my_combos,
    sources::DeckSource,
};
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedCombo {
    pub cards: Vec<String>,
    pub result: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GamePlan {
    pub commanders: Vec<String>,
    pub identity: String,
    pub primary_tags: Vec<String>,
    pub summary: String,
    pub combos: Vec<GeneratedCombo>,
    pub notes: Vec<String>,
    pub sources: Vec<DeckSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedDeck {
    pub deck: ParsedDeck,
    pub plan: GamePlan,
}


#[derive(Debug, Clone, serde::Serialize)]
pub struct GenerationTheme {
    pub name: String,
    pub score: i32,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GenerationProgress {
    pub percent: u16,
    pub step: String,
    pub detail: String,
}

impl GenerationProgress {
    fn new(percent: u16, step: &str, detail: impl Into<String>) -> Self {
        Self {
            percent: percent.min(100),
            step: step.to_string(),
            detail: detail.into(),
        }
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationBracket {
    pub bracket: u8,
}

impl GenerationBracket {
    pub fn new(bracket: u8) -> Self {
        Self {
            bracket: bracket.clamp(1, 5),
        }
    }

    pub fn label(self) -> &'static str {
        match self.bracket {
            1 => "Exhibition",
            2 => "Core",
            3 => "Upgraded",
            4 => "Optimized",
            _ => "cEDH profile",
        }
    }

    fn max_game_changers(self) -> usize {
        match self.bracket {
            1 | 2 => 0,
            3 => 3,
            4 => 8,
            _ => usize::MAX,
        }
    }


    fn combo_target(self) -> usize {
        match self.bracket {
            1 => 0,
            2 | 3 => 1,
            4 | 5 => 2,
            _ => 1,
        }
    }

    fn allow_compact_combo(self, template: &ComboTemplate) -> bool {
        match self.bracket {
            1 => false,
            2 => template.cards.len() >= 2 && !template.fast,
            3 => !template.cedh,
            4 | 5 => true,
            _ => true,
        }
    }

fn minimum_game_changers(self) -> u32 {
        match self.bracket {
            1 | 2 => 0,
            3 => 1,
            4 => 4,
            5 => 4,
            _ => 0,
        }
    }

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

pub fn generate_optimized_deck<F>(
    commander_specification: &str,
    selected_theme: &str,
    target_bracket: GenerationBracket,
    db: &CardDb,
    mut progress: F,
) -> Result<GeneratedDeck>
where
    F: FnMut(GenerationProgress),
{
    progress(GenerationProgress::new(
        4,
        "Resolving commander",
        commander_specification,
    ));

    let commanders =
        resolve_generation_commanders(commander_specification, db)?;

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

    let identity = combined_identity(&commanders);
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
    };

    progress(GenerationProgress::new(
        16,
        "Locking deck theme",
        format!("Building around {selected_theme}"),
    ));

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

    let intelligence = learn_commander_intelligence(
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
    )?;

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

    let library_target = 100usize.saturating_sub(commanders.len());
    let land_target = if identity.len() >= 4 { 35 } else { 34 };
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

    let commander_engine_target =
        if commander_engine_terms.is_empty() {
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

    let templates = combo_templates();

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

            if !card.commander_legal
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

    let Some(cards) = resolve_combo_template(template, &identity, db)? else {
        continue;
    };

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

    let spell_pool = pool
        .iter()
        .filter(|card| {
            card.commander_legal
                && identity_subset(&card.color_identity, &identity)
                && !card.type_line.to_ascii_lowercase().contains("land")
                && !used.contains(&card.name.to_ascii_lowercase())
        })
        .map(|card| {
            let roles = classify_card(card);
            let direct_theme = card_matches_theme(card, selected_theme);
            let engine_match =
                card_matches_engine(card, &commander_engine_terms);
            let reference_frequency =
                reference_snapshot.frequency(&card.name);
            let edhrec_signal =
                edhrec_signals.get(&card.name.to_ascii_lowercase());
            let learned_support =
                intelligence.support_for_card(card, roles);

            let score = base_card_score(card, &tags)
                + target_bracket.fast_mana_score(card, roles)
                + bracket_card_score(card, roles, target_bracket)
                + learned_support.score;

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

    while main.len() < spell_target {
        let mut best_index = None;
        let mut best_score = i32::MIN;

        for (index, (card, roles, base, direct_theme, engine_match, reference_frequency, edhrec_core)) in spell_pool.iter().enumerate() {
            if used.contains(&card.name.to_ascii_lowercase()) {
                continue;
            }

            let structural = has_structural_role(*roles);

            let theme_need =
                if *direct_theme && theme_cards < theme_preference_target {
                    90
                } else if *direct_theme {
                    45
                } else {
                    0
                };

            // Off-theme cards are never excluded. Cards with no direct theme,
            // structural, or reference support receive only a modest
            // coherence penalty and can still win the slot on overall value.
            let coherence_adjustment =
                if !*direct_theme && !structural && *reference_frequency == 0 {
                    -24
                } else {
                    0
                };

            let engine_need =
                if *engine_match
                    && commander_engine_cards < commander_engine_target
                {
                    180
                } else if *engine_match {
                    70
                } else {
                    0
                };

            let edhrec_need =
                if *edhrec_core
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

    let mut land_pool = pool
        .iter()
        .filter(|card| {
            card.commander_legal
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

    let mut library_count =
        main.iter().map(|entry| entry.quantity as usize).sum::<usize>();

    if library_count < library_target {
        // Final fill remains coherence-aware, but scores every remaining legal
        // card instead of requiring a theme/role/reference match.
        let mut fallback_cards = pool
            .iter()
            .filter(|card| {
                card.commander_legal
                    && identity_subset(&card.color_identity, &identity)
                    && !card.type_line.to_ascii_lowercase().contains("land")
                    && !used.contains(&card.name.to_ascii_lowercase())
            })
            .map(|card| {
                let roles = classify_card(card);
                let direct_theme =
                    card_matches_theme(card, selected_theme);
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

    if library_count != library_target {
        return Err(anyhow!(
            "The generator could only build a {library_count}-card library. \
             A {}-commander deck needs {library_target} library cards.",
            commanders.len()
        ));
    }

    let mut deck = ParsedDeck {
        name: format!("{generated_name} - Generated"),
        commanders: commander_entries,
        main,
        sideboard: Vec::new(),
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

        if !card.commander_legal
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

let (integrity_report, validated_bracket) = integrity_gate(
    &mut deck,
    target_bracket,
    db,
    &pool,
    &identity,
    &intelligence,
    &final_combos,
)?;

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

notes.insert(
    0,
    format!(
        "Target power: Bracket {} — {}. Final validation: {}. The generator treats the requested bracket as a hard post-build contract, not merely a scoring preference.",
        target_bracket.bracket,
        target_bracket.label(),
        validated_bracket.display_name(),
    ),
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

    if reference_snapshot.deck_count() > 0 {
        notes.push(format!(
            "Compared against {} cached/public reference deck(s) from Archidekt, Moxfield{}. Deep Learn also measured recurring card pairs and {} distinct complete Spellbook combo line(s) across the corpus.",
            reference_snapshot.deck_count(),
            if target_bracket.bracket >= 5 {
                ", and cEDH Decklist Database-linked lists"
            } else {
                ""
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

    let summary =
        build_game_plan_summary(&commanders, selected_theme, &primary_tags, &final_combos);

    let sources = intelligence.sources.clone();

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
        },
    })
}

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
                card.commander_legal
                    && identity_subset(&card.color_identity, identity)
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
                card.commander_legal
                    && identity_subset(&card.color_identity, identity)
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

#[derive(Debug, Clone, Copy, Default)]
struct GeneratedPowerProfile {
    game_changers: u32,
    tutors: u32,
    fast_mana: u32,
}

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

    out.push_str("\n## Build Notes\n\n");
    for note in &plan.notes {
        out.push_str(&format!("- {note}\n"));
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

#[derive(Debug, Clone)]
struct ComboTemplate {
    cards: &'static [&'static str],
    result: &'static str,
    preferred_tags: &'static [&'static str],
    fast: bool,
    cedh: bool,
}

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

fn resolve_combo_template(
    template: &ComboTemplate,
    identity: &str,
    db: &CardDb,
) -> Result<Option<Vec<Card>>> {
    let mut cards = Vec::new();

    for name in template.cards {
        let Some(card) = db.get_card(name)? else {
            return Ok(None);
        };

        if !card.commander_legal
            || !identity_subset(&card.color_identity, identity)
        {
            return Ok(None);
        }

        cards.push(card);
    }

    Ok(Some(cards))
}


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

/// True when the card fits any of the selected play styles.
fn card_matches_theme(card: &Card, selected_theme: &str) -> bool {
    theme_list(selected_theme)
        .iter()
        .any(|theme| card_matches_single_theme(card, theme))
}

fn card_matches_single_theme(card: &Card, selected_theme: &str) -> bool {
    if tag_names_for_card(card)
        .iter()
        .any(|tag| tag.eq_ignore_ascii_case(selected_theme))
    {
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
    fn add_card(&mut self, card: &Card, roles: CardRoles) {
        if card.game_changer {
            self.game_changers += 1;
        }
        self.add(roles);
    }

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
