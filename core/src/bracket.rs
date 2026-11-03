//! Commander Bracket estimate (the official 1–5 brackets).
//!
//! How it fits: [`estimate_bracket`] counts what the bracket rules care
//! about (Game Changers, tutors, fast mana, extra turns, mass land denial,
//! early two-card combos) and returns a [`BracketEstimate`] with the reasons.
//! [`refine_with_combo_report`] raises it when Commander Spellbook finds
//! combos. Deck header, generator bracket lock and the agent all use it.
//!
//! Things to work on:
//! - The Game Changers list comes from Scryfall's flag; check it's refreshed
//!   when Wizards updates the list.
//! - "Intent" brackets (a casual deck with one Game Changer) can't be
//!   expressed; consider letting the player pin a bracket on a deck.

use crate::{
    analysis::classify_card,
    data::CardDb,
    deck::ParsedDeck,
    spellbook::ComboReport,
};
use anyhow::Result;

/// A deck's estimated bracket with what drove it: Game Changers found, fast
/// mana, tutors, extra turns, mass land denial, fast ("ruthless") combos,
/// reasons in words and a confidence note.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BracketEstimate {
    pub bracket: u8,
    pub label: String,
    pub game_changers: Vec<String>,
    pub fast_mana: u32,
    pub tutors: u32,
    pub extra_turn_cards: u32,
    pub mass_land_denial: u32,
    pub ruthless_combos: usize,
    pub reasons: Vec<String>,
    pub confidence_note: String,
}

impl BracketEstimate {
    /// "Bracket 3 — Upgraded".
    pub fn display_name(&self) -> String {
        format!("Bracket {} — {}", self.bracket, self.label)
    }
}

/// Estimate the bracket from the cards alone (combos come later via
/// `refine_with_combo_report`):
/// - 4+ Game Changers → 4; 1–3 → 3;
/// - otherwise mass land denial or 3+ extra-turn cards → 4,
///   5+ tutors or 4+ fast mana → 3, 2+ tutors or any fast mana → 2, else 1;
/// - bracket 5 only when a 4 also has 4+ Game Changers, 5+ fast mana and 7+
///   tutors (cEDH profile), since 5 is about intent, not a checklist.
pub fn estimate_bracket(deck: &ParsedDeck, db: &CardDb) -> Result<BracketEstimate> {
    let mut game_changers = Vec::new();
    let mut fast_mana = 0u32;
    let mut tutors = 0u32;
    let mut extra_turn_cards = 0u32;
    let mut mass_land_denial = 0u32;

    for entry in deck.commanders.iter().chain(deck.main.iter()) {
        let Some(card) = db.get_card(&entry.name)? else {
            continue;
        };

        if card.game_changer {
            for _ in 0..entry.quantity {
                game_changers.push(card.name.clone());
            }
        }

        let roles = classify_card(&card);
        if roles.tutor {
            tutors += entry.quantity;
        }

        let oracle = card.oracle_text.to_ascii_lowercase();
        let ty = card.type_line.to_ascii_lowercase();

        let makes_mana = roles.ramp
            || oracle.contains("add {")
            || oracle.contains("add one mana")
            || oracle.contains("add two mana")
            || oracle.contains("add three mana");

        if !ty.contains("land") && card.cmc <= 1.0 && makes_mana {
            fast_mana += entry.quantity;
        }

        if oracle.contains("take an extra turn")
            || oracle.contains("takes an extra turn")
        {
            extra_turn_cards += entry.quantity;
        }

        if oracle.contains("destroy all lands")
            || oracle.contains("destroy each land")
            || oracle.contains("lands don't untap")
            || oracle.contains("lands do not untap")
            || oracle.contains("players can't untap more than")
        {
            mass_land_denial += entry.quantity;
        }
    }

    game_changers.sort();
    game_changers.dedup();

    let gc = game_changers.len();

    // The bracket rules, strongest signal first (see the function notes).
    let (mut bracket, mut label) = if gc >= 4 {
        (4, "Optimized".to_string())
    } else if gc >= 1 {
        (3, "Upgraded".to_string())
    } else if mass_land_denial > 0 || extra_turn_cards > 2 {
        (4, "Optimized".to_string())
    } else if tutors >= 5 || fast_mana >= 4 {
        (3, "Upgraded".to_string())
    } else if tutors >= 2 || fast_mana >= 1 {
        (2, "Core".to_string())
    } else {
        (1, "Exhibition".to_string())
    };

    // Bracket 5 is competitive intent, not just a card checklist. We only
    // flag a deck as "cEDH profile" when several objective indicators line up.
    if bracket >= 4
        && fast_mana >= 5
        && tutors >= 7
        && gc >= 4
    {
        bracket = 5;
        label = "cEDH profile".to_string();
    }

    let mut reasons = Vec::new();

    if gc > 0 {
        reasons.push(format!(
            "{} Game Changer(s) detected: {}",
            gc,
            game_changers.join(", ")
        ));
    } else {
        reasons.push("No current built-in Game Changers detected.".to_string());
    }

    reasons.push(format!("{fast_mana} fast-mana card(s) detected."));
    reasons.push(format!("{tutors} tutor card(s) detected."));

    if extra_turn_cards > 0 {
        reasons.push(format!(
            "{extra_turn_cards} extra-turn card(s) detected."
        ));
    }

    if mass_land_denial > 0 {
        reasons.push(format!(
            "{mass_land_denial} mass-land-denial/stax land effect(s) detected."
        ));
    }

    Ok(BracketEstimate {
        bracket,
        label,
        game_changers,
        fast_mana,
        tutors,
        extra_turn_cards,
        mass_land_denial,
        ruthless_combos: 0,
        reasons,
        confidence_note:
            "Bracket detection is an estimate. Official Commander guidance says intent and pregame expectations matter; Bracket 5 especially cannot be proven from a decklist alone."
                .to_string(),
    })
}

/// Raise the estimate for complete combos Commander Spellbook found: a combo
/// tagged bracket 4+ ("ruthless") makes the deck at least 4, 3+ at least 3,
/// 2+ at least 2. Never lowers it. Depends on Spellbook's bracket tags,
/// which spellbook.rs currently fails to read for most combos.
pub fn refine_with_combo_report(
    estimate: &mut BracketEstimate,
    report: &ComboReport,
) {
    let complete = report
        .variants
        .iter()
        .filter(|combo| !combo.almost)
        .collect::<Vec<_>>();

    let ruthless = complete
        .iter()
        .filter(|combo| combo.bracket.unwrap_or(0) >= 4)
        .count();

    let powerful = complete
        .iter()
        .filter(|combo| combo.bracket.unwrap_or(0) >= 3)
        .count();

    let core_or_higher = complete
        .iter()
        .filter(|combo| combo.bracket.unwrap_or(0) >= 2)
        .count();

    estimate.ruthless_combos = ruthless;

    if ruthless > 0 && estimate.bracket < 4 {
        estimate.bracket = 4;
        estimate.label = "Optimized".to_string();
        estimate.reasons.push(format!(
            "{ruthless} Commander Spellbook combo(s) are tagged for bracket 4+."
        ));
    } else if powerful > 0 && estimate.bracket < 3 {
        estimate.bracket = 3;
        estimate.label = "Upgraded".to_string();
        estimate.reasons.push(format!(
            "{powerful} Commander Spellbook combo(s) are tagged for bracket 3+."
        ));
    } else if core_or_higher > 0 && estimate.bracket < 2 {
        estimate.bracket = 2;
        estimate.label = "Core".to_string();
        estimate.reasons.push(format!(
            "{core_or_higher} Commander Spellbook combo(s) are tagged for bracket 2+."
        ));
    }

    let competitive_profile =
        ruthless >= 2
            && estimate.fast_mana >= 4
            && estimate.tutors >= 5
            && estimate.game_changers.len() >= 2;

    let combo_dense_profile =
        ruthless >= 3
            && estimate.fast_mana >= 3
            && estimate.tutors >= 3;

    if estimate.bracket == 4
        && (competitive_profile || combo_dense_profile)
    {
        estimate.bracket = 5;
        estimate.label = "cEDH profile".to_string();

        estimate.reasons.push(
            if competitive_profile {
                "Multiple ruthless combo lines plus competitive fast mana, tutor density, and Game Changers create a cEDH-like profile."
                    .to_string()
            } else {
                "Three or more ruthless combo lines plus efficient fast mana/tutors create a combo-dense cEDH-like profile."
                    .to_string()
            },
        );
    }
}
