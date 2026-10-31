use crate::{
    analysis::{classify_card, identity_subset, CardRoles},
    archetype::tag_names_for_card,
    commander::combined_identity,
    data::{Card, CardDb},
    edhrec::{commander_build_cards, commander_themes, EdhrecCardSignal},
    references::{gather_reference_decks, ReferenceSnapshot},
    sources::{generation_sources, DeckSource},
};
use anyhow::Result;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct LearnedRoleTargets {
    pub ramp: u32,
    pub draw: u32,
    pub interaction: u32,
    pub tutor: u32,
    pub wipe: u32,
    pub protection: u32,
    pub recursion: u32,
}

#[derive(Debug, Clone)]
pub struct CardSupport {
    pub score: i32,
    pub confidence: u8,
    pub edhrec_inclusion: f64,
    pub reference_frequency: u32,
    pub engine_match: bool,
    pub structural: bool,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct CommanderIntelligence {
    pub commander_names: Vec<String>,
    pub identity: String,
    pub target_bracket: u8,
    pub selected_theme: String,
    pub normal_themes: Vec<String>,
    pub core_tags: Vec<String>,
    pub engine_terms: Vec<String>,
    pub edhrec_cards: Vec<EdhrecCardSignal>,
    pub edhrec_by_name: HashMap<String, EdhrecCardSignal>,
    pub references: ReferenceSnapshot,
    pub role_targets: LearnedRoleTargets,
    pub sources: Vec<DeckSource>,
    pub confidence: u8,
}

impl CommanderIntelligence {
    pub fn support_for_card(
        &self,
        card: &Card,
        roles: CardRoles,
    ) -> CardSupport {
        let key = card.name.to_ascii_lowercase();
        let signal = self.edhrec_by_name.get(&key);
        let reference_frequency = self.references.frequency(&card.name);
        let reference_package_frequency =
            self.references.package_frequency(&card.name);

        let theme_match = card_matches_tag(card, &self.selected_theme);
        let core_tag_match = self
            .core_tags
            .iter()
            .any(|tag| card_matches_tag(card, tag));
        let engine_match = card_matches_engine(card, &self.engine_terms);

        let structural = roles.ramp
            || roles.draw
            || roles.removal
            || roles.counter
            || roles.tutor
            || roles.wipe
            || roles.protection
            || roles.recursion;

        let inclusion = signal
            .map(EdhrecCardSignal::inclusion_pct)
            .unwrap_or(0.0);

        let edhrec_component = signal
            .map(|signal| {
                let category_bonus = match signal.category.as_str() {
                    "highsynergycards" => 145,
                    "topcards" => 115,
                    "gamechangers" => 80,
                    "creatures" => 55,
                    "manaartifacts" => 55,
                    "utilitylands" | "lands" => 48,
                    "instants" | "sorceries" | "enchantments" => 45,
                    _ => 32,
                };

                category_bonus
                    + (signal.inclusion_pct() * 210.0) as i32
                    + (signal.synergy.max(-0.35) * 240.0) as i32
            })
            .unwrap_or(0);

        let reference_share = if self.references.deck_count() == 0 {
            0.0
        } else {
            reference_frequency as f64
                / self.references.deck_count() as f64
        };

        let package_share = if self.references.deck_count() == 0 {
            0.0
        } else {
            reference_package_frequency as f64
                / self.references.deck_count() as f64
        };

        let reference_multiplier = if self.target_bracket >= 5 {
            360.0
        } else if self.target_bracket >= 4 {
            275.0
        } else {
            210.0
        };

        let package_multiplier = if self.target_bracket >= 5 {
            140.0
        } else {
            85.0
        };

        let reference_component =
            (reference_share * reference_multiplier).round() as i32
            + (package_share * package_multiplier).round() as i32;

        let reference_core_bonus = if reference_share >= 0.70 {
            if self.target_bracket >= 5 { 120 } else { 80 }
        } else if reference_share >= 0.50 {
            if self.target_bracket >= 5 { 80 } else { 50 }
        } else if reference_share >= 0.30 {
            30
        } else {
            0
        };

        let engine_component = if engine_match { 125 } else { 0 };
        let core_component = if core_tag_match { 70 } else { 0 };
        let theme_component = if theme_match { 55 } else { 0 };
        let structural_component = if structural { 22 } else { 0 };

        // If a card has no shell evidence at all, don't ban it. Give it a small
        // uncertainty penalty so generically useful cards can still win slots.
        let uncertainty = if signal.is_none()
            && reference_frequency == 0
            && !engine_match
            && !core_tag_match
            && !theme_match
        {
            -32
        } else {
            0
        };

        let score = edhrec_component
            + reference_component
            + reference_core_bonus
            + engine_component
            + core_component
            + theme_component
            + structural_component
            + uncertainty;

        let mut confidence = 0u8;

        if signal.is_some() {
            confidence = confidence.saturating_add(40);
        }
        if reference_frequency > 0 {
            confidence = confidence.saturating_add(
                (reference_share * 35.0).round() as u8,
            );
        }
        if reference_package_frequency > 0 {
            confidence = confidence.saturating_add(
                (package_share * 15.0).round() as u8,
            );
        }
        if engine_match {
            confidence = confidence.saturating_add(15);
        }
        if core_tag_match {
            confidence = confidence.saturating_add(10);
        }

        confidence = confidence.min(100);

        let reason = format!(
            "{}{}{}{}{}{}",
            signal
                .map(|s| format!(
                    "EDHREC {:.0}% inclusion / {:.1}% synergy",
                    s.inclusion_pct() * 100.0,
                    s.synergy * 100.0,
                ))
                .unwrap_or_else(|| "no EDHREC cardlist signal".to_string()),
            if reference_frequency > 0 {
                format!(
                    " • {}/{} reference decks",
                    reference_frequency,
                    self.references.deck_count()
                )
            } else {
                String::new()
            },
            if reference_package_frequency > 0 {
                format!(
                    " • package consensus {}/{}",
                    reference_package_frequency,
                    self.references.deck_count()
                )
            } else {
                String::new()
            },
            if engine_match {
                " • commander-engine match"
            } else {
                ""
            },
            if core_tag_match {
                " • normal-shell tag"
            } else {
                ""
            },
            if theme_match {
                " • chosen-style match"
            } else {
                ""
            },
        );

        CardSupport {
            score,
            confidence,
            edhrec_inclusion: inclusion,
            reference_frequency,
            engine_match,
            structural,
            reason,
        }
    }

    pub fn combo_relevance(
    &self,
    cards: &[String],
    db: &CardDb,
) -> i32 {
    if cards.is_empty() {
        return 0;
    }

    let mut total = 0i32;
    let mut supported_pieces = 0usize;

    for name in cards {
        let Some(card) = db.get_card(name).ok().flatten() else {
            continue;
        };

        let support =
            self.support_for_card(&card, classify_card(&card));

        if support.score >= 80
            || support.engine_match
            || support.reference_frequency > 0
            || support.edhrec_inclusion >= 0.10
        {
            supported_pieces += 1;
        }

        total += support.score.max(0);
    }

    let average = total / cards.len().max(1) as i32;
    let coverage =
        supported_pieces as f64 / cards.len().max(1) as f64;

    let combo_frequency =
        self.references.combo_frequency_for_cards(cards);

    let combo_share = if self.references.deck_count() == 0 {
        0.0
    } else {
        combo_frequency as f64
            / self.references.deck_count() as f64
    };

    let observed_combo_bonus = if combo_frequency == 0 {
        0
    } else if self.target_bracket >= 5 {
        180 + (combo_share * 620.0).round() as i32
    } else {
        100 + (combo_share * 360.0).round() as i32
    };

    average
        + (coverage * 180.0) as i32
        + observed_combo_bonus
}
    pub fn role_summary(&self) -> String {
        format!(
            "ramp {} • draw {} • interaction {} • tutors {} • wipes {} • protection {} • recursion {}",
            self.role_targets.ramp,
            self.role_targets.draw,
            self.role_targets.interaction,
            self.role_targets.tutor,
            self.role_targets.wipe,
            self.role_targets.protection,
            self.role_targets.recursion,
        )
    }
}

pub fn learn_commander_intelligence<F>(
    commanders: &[Card],
    selected_theme: &str,
    bracket: u8,
    db: &CardDb,
    mut progress: F,
) -> Result<CommanderIntelligence>
where
    F: FnMut(String),
{
    let commander_names = commanders
        .iter()
        .map(|card| card.name.clone())
        .collect::<Vec<_>>();

    let identity = combined_identity(commanders);

    progress(format!(
        "[EVIDENCE] Reading EDHREC normal-build tags for {}",
        commander_names.join(" + ")
    ));

    let theme_rows = commander_themes(commanders, 10);
    let normal_themes = theme_rows
        .iter()
        .map(|theme| theme.name.clone())
        .collect::<Vec<_>>();

    let core_tags = normal_themes
        .iter()
        .take(4)
        .cloned()
        .collect::<Vec<_>>();

    progress(format!(
        "[EVIDENCE] Reading commander/theme EDHREC cardlists for {}",
        if selected_theme.trim().is_empty() {
            core_tags
                .first()
                .map(String::as_str)
                .unwrap_or("base commander shell")
        } else {
            selected_theme
        }
    ));

    let mut edhrec_cards =
        commander_build_cards(commanders, selected_theme, 650);

    // Merge the base commander shell too. This is important when the user picks
    // a niche play-style theme: the selected theme should modify the normal
    // commander build rather than erase it.
    let base_cards = commander_build_cards(commanders, "", 650);

    let mut merged = HashMap::<String, EdhrecCardSignal>::new();

    for signal in edhrec_cards
        .drain(..)
        .chain(base_cards.into_iter())
    {
        let key = signal.name.to_ascii_lowercase();

        match merged.get(&key) {
            Some(existing)
                if raw_signal_score(existing)
                    >= raw_signal_score(&signal) => {}
            _ => {
                merged.insert(key, signal);
            }
        }
    }

    let mut edhrec_cards = merged.into_values().collect::<Vec<_>>();
    edhrec_cards.sort_by(|a, b| {
        raw_signal_score(b)
            .partial_cmp(&raw_signal_score(a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });

    progress(
        "[EVIDENCE] Building large reference corpus from Moxfield / Archidekt / cEDH and checking recurring Spellbook combos"
            .to_string(),
    );

    let references =
        gather_reference_decks(&commander_names, bracket);

    progress(format!(
        "[REFERENCES] Corpus ready • decks={} • recurring_pairs={} • complete_combo_patterns={}",
        references.deck_count(),
        references.pair_frequency.len(),
        references.combo_frequency.len(),
    ));

    let mut source_counts = HashMap::<String, usize>::new();
    for deck in &references.decks {
        *source_counts.entry(deck.source.clone()).or_default() += 1;
    }

    let mut source_rows = source_counts.into_iter().collect::<Vec<_>>();
    source_rows.sort_by(|a, b| {
        b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0))
    });

    progress(format!(
        "[REFERENCES] Source breakdown • {}",
        if source_rows.is_empty() {
            "no public reference decks returned".to_string()
        } else {
            source_rows
                .into_iter()
                .map(|(source, count)| format!("{source}={count}"))
                .collect::<Vec<_>>()
                .join(" | ")
        },
    ));

    let top_reference_cards = references
        .top_cards(8)
        .into_iter()
        .map(|(name, count)| format!("{name} ({count})"))
        .collect::<Vec<_>>();

    if !top_reference_cards.is_empty() {
        progress(format!(
            "[REFERENCES] Highest-frequency cards • {}",
            top_reference_cards.join(" | "),
        ));
    }

    let top_reference_combos = references
        .top_combos(6)
        .into_iter()
        .map(|(cards, _, bracket, count)| {
            format!(
                "{} [{} deck(s){}]",
                cards.join(" + "),
                count,
                bracket
                    .map(|value| format!(", Spellbook B{value}"))
                    .unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();

    if !top_reference_combos.is_empty() {
        progress(format!(
            "[REFERENCES] Recurring complete combos • {}",
            top_reference_combos.join(" | "),
        ));
    }

    progress(format!(
        "[EDHREC] Merged commander/theme card signals • {} unique card(s) • top themes={}",
        edhrec_cards.len(),
        if normal_themes.is_empty() {
            "none".to_string()
        } else {
            normal_themes
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join(" / ")
        },
    ));

    let edhrec_by_name = edhrec_cards
        .iter()
        .map(|signal| {
            (signal.name.to_ascii_lowercase(), signal.clone())
        })
        .collect::<HashMap<_, _>>();

    let engine_terms =
        infer_commander_engine_terms(commanders, &core_tags);

    progress("[MODEL] Estimating commander-specific ramp / draw / interaction / tutor / protection density".to_string());

    let role_targets = learn_role_targets(
        &edhrec_cards,
        &references,
        &identity,
        bracket,
        db,
    );

    let sources = generation_sources(
        commanders,
        selected_theme,
        bracket,
        &references,
    );

    let confidence = learn_confidence(
        &theme_rows,
        &edhrec_cards,
        &references,
    );

    progress(format!(
        "[MODEL] Confidence {}% • core {} • {} EDHREC signals • {} references • {}",
        confidence,
        if core_tags.is_empty() {
            "none".to_string()
        } else {
            core_tags.join(" / ")
        },
        edhrec_cards.len(),
        references.deck_count(),
        format!(
            "ramp {} / draw {} / interaction {} / tutors {} / wipes {} / protection {} / recursion {}",
            role_targets.ramp,
            role_targets.draw,
            role_targets.interaction,
            role_targets.tutor,
            role_targets.wipe,
            role_targets.protection,
            role_targets.recursion,
        ),
    ));

    Ok(CommanderIntelligence {
        commander_names,
        identity,
        target_bracket: bracket,
        selected_theme: selected_theme.to_string(),
        normal_themes,
        core_tags,
        engine_terms,
        edhrec_cards,
        edhrec_by_name,
        references,
        role_targets,
        sources,
        confidence,
    })
}

fn learn_role_targets(
    edhrec: &[EdhrecCardSignal],
    references: &ReferenceSnapshot,
    identity: &str,
    bracket: u8,
    db: &CardDb,
) -> LearnedRoleTargets {
    let mut seen = HashSet::<String>::new();
    let mut sample = Vec::<Card>::new();

    for signal in edhrec.iter().take(180) {
        let Some(card) = db.get_card(&signal.name).ok().flatten() else {
            continue;
        };

        if !card.commander_legal
            || !identity_subset(&card.color_identity, identity)
            || card.type_line.to_ascii_lowercase().contains("land")
        {
            continue;
        }

        if seen.insert(card.name.to_ascii_lowercase()) {
            sample.push(card);
        }
    }

    for (name, _) in references.top_cards(120) {
        let Some(card) = db.get_card(&name).ok().flatten() else {
            continue;
        };

        if !card.commander_legal
            || !identity_subset(&card.color_identity, identity)
            || card.type_line.to_ascii_lowercase().contains("land")
        {
            continue;
        }

        if seen.insert(card.name.to_ascii_lowercase()) {
            sample.push(card);
        }
    }

    let mut observed = LearnedRoleTargets::default();

    for card in &sample {
        let roles = classify_card(card);

        if roles.ramp {
            observed.ramp += 1;
        }
        if roles.draw {
            observed.draw += 1;
        }
        if roles.removal || roles.counter {
            observed.interaction += 1;
        }
        if roles.tutor {
            observed.tutor += 1;
        }
        if roles.wipe {
            observed.wipe += 1;
        }
        if roles.protection {
            observed.protection += 1;
        }
        if roles.recursion {
            observed.recursion += 1;
        }
    }

    let sample_size = sample.len().max(1) as f64;

    let scaled = |count: u32, floor: u32, ceiling: u32| -> u32 {
        let density = count as f64 / sample_size;
        ((density * 64.0).round() as u32)
            .clamp(floor, ceiling)
    };

    let bracket_tutors = match bracket {
        1 => (0, 1),
        2 => (1, 3),
        3 => (2, 5),
        4 => (3, 8),
        _ => (5, 12),
    };

    LearnedRoleTargets {
        ramp: scaled(observed.ramp, 8, 14),
        draw: scaled(observed.draw, 8, 14),
        interaction: scaled(observed.interaction, 8, 16),
        tutor: scaled(
            observed.tutor,
            bracket_tutors.0,
            bracket_tutors.1,
        ),
        wipe: scaled(observed.wipe, 1, 4),
        protection: scaled(observed.protection, 2, 8),
        recursion: scaled(observed.recursion, 1, 8),
    }
}

fn learn_confidence(
    themes: &[crate::edhrec::EdhrecTheme],
    edhrec: &[EdhrecCardSignal],
    references: &ReferenceSnapshot,
) -> u8 {
    let theme_score = themes.len().min(10) as u8 * 3;
    let edhrec_score = (edhrec.len().min(300) / 6) as u8;
    let reference_score =
        references.deck_count().min(40) as u8;

    let combo_score = references
        .top_combos(12)
        .len()
        .min(12) as u8;

    theme_score
        .saturating_add(edhrec_score)
        .saturating_add(reference_score)
        .saturating_add(combo_score)
        .min(100)
}

fn raw_signal_score(signal: &EdhrecCardSignal) -> f64 {
    let category = match signal.category.as_str() {
        "highsynergycards" => 2.2,
        "topcards" => 1.8,
        "gamechangers" => 1.5,
        "creatures" => 1.2,
        "manaartifacts" | "utilitylands" | "lands" => 1.0,
        _ => 0.9,
    };

    category
        + signal.inclusion_pct() * 3.0
        + signal.synergy.max(-0.5) * 4.0
}

fn infer_commander_engine_terms(
    commanders: &[Card],
    core_tags: &[String],
) -> Vec<String> {
    let oracle = commanders
        .iter()
        .map(|card| card.oracle_text.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ");

    let mut terms = Vec::<String>::new();

    for tag in core_tags {
        let lower = tag.to_ascii_lowercase();

        if lower.len() >= 4 && oracle.contains(&lower) {
            terms.push(lower);
        }
    }

    for pattern in [
        "eldrazi",
        "dragon",
        "zombie",
        "vampire",
        "goblin",
        "elf",
        "artifact",
        "enchantment",
        "instant",
        "sorcery",
        "land",
        "token",
        "counter",
        "graveyard",
        "equipment",
        "aura",
        "sliver",
        "merfolk",
        "spirit",
        "warrior",
        "knight",
        "cleric",
        "wizard",
    ] {
        if oracle.contains(pattern)
            && !terms.iter().any(|term| term == pattern)
        {
            terms.push(pattern.to_string());
        }
    }

    terms
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

fn card_matches_tag(card: &Card, tag: &str) -> bool {
    if tag.trim().is_empty() {
        return false;
    }

    // Several selected play styles ("Tokens + Treasure"): match any of them.
    let themes = crate::archetype::theme_list(tag);
    if themes.len() > 1 {
        return themes.iter().any(|t| card_matches_tag(card, t));
    }

    tag_names_for_card(card)
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(tag))
        || {
            let lower = tag.to_ascii_lowercase();
            let haystack = format!(
                "{} {}",
                card.type_line.to_ascii_lowercase(),
                card.oracle_text.to_ascii_lowercase(),
            );

            lower.len() >= 4 && haystack.contains(&lower)
        }
}
