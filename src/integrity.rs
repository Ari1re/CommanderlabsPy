//! Deck integrity testing.
//!
//! Every generated or AI-upgraded list passes through this audit before it is
//! handed to the user. Hard rules (size, singleton, colour identity,
//! Commander legality, commander placement) are repaired automatically and
//! re-verified; soft rules (land count, structural role minimums, opening-hand
//! goldfish) are reported as warnings so the user knows the list's risks.

use crate::{
    analysis::{classify_card, copies_allowed, identity_subset},
    data::{Card, CardDb},
    deck::{DeckEntry, ParsedDeck},
    goldfish::goldfish_with_combo_lines,
};
use anyhow::{anyhow, Result};
use std::collections::HashSet;

/// Commander decks are exactly 100 cards including the command zone.
pub const COMMANDER_DECK_SIZE: u32 = 100;

const GOLDFISH_RUNS: u32 = 400;
const MIN_KEEPABLE_PCT: f64 = 70.0;

#[derive(Debug, Clone, Default)]
pub struct IntegrityReport {
    pub hard_failures: Vec<String>,
    pub repairs: Vec<String>,
    pub warnings: Vec<String>,
    pub total_cards: u32,
    pub lands: u32,
    pub ramp: u32,
    pub draw: u32,
    pub interaction: u32,
    pub keepable_pct: Option<f64>,
}

impl IntegrityReport {
    pub fn passed(&self) -> bool {
        self.hard_failures.is_empty()
    }

    pub fn summary(&self) -> String {
        format!(
            "Integrity {}: {} cards • {} lands • ramp {} • draw {} • interaction {}{} • {} repair(s) • {} warning(s)",
            if self.passed() { "PASSED" } else { "FAILED" },
            self.total_cards,
            self.lands,
            self.ramp,
            self.draw,
            self.interaction,
            self.keepable_pct
                .map(|pct| format!(" • keepable hands {pct:.0}%"))
                .unwrap_or_default(),
            self.repairs.len(),
            self.warnings.len(),
        )
    }

    pub fn notes(&self) -> Vec<String> {
        let mut out = vec![self.summary()];
        out.extend(self.repairs.iter().map(|line| format!("Integrity repair: {line}")));
        out.extend(self.warnings.iter().map(|line| format!("Integrity warning: {line}")));
        out.extend(
            self.hard_failures
                .iter()
                .map(|line| format!("Integrity FAILURE: {line}")),
        );
        out
    }
}

fn basic_for_color(color: char) -> &'static str {
    match color {
        'W' => "Plains",
        'U' => "Island",
        'B' => "Swamp",
        'R' => "Mountain",
        'G' => "Forest",
        _ => "Wastes",
    }
}

fn is_land(card: &Card) -> bool {
    card.type_line.to_ascii_lowercase().contains("land")
}

fn deck_total(deck: &ParsedDeck) -> u32 {
    deck.commanders.iter().map(|e| e.quantity).sum::<u32>()
        + deck.main.iter().map(|e| e.quantity).sum::<u32>()
}

/// Audit the deck, repair hard-rule violations in place, then re-verify.
///
/// `protected` names (combo pieces, commander-engine cards) are never chosen
/// when trimming an oversized list.
pub fn audit_and_repair(
    deck: &mut ParsedDeck,
    db: &CardDb,
    identity: &str,
    protected: &[String],
    combo_lines: &[Vec<String>],
) -> Result<IntegrityReport> {
    let mut report = IntegrityReport::default();

    let protected: HashSet<String> = protected
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect();

    // --- Commanders ------------------------------------------------------
    if deck.commanders.is_empty() {
        report
            .hard_failures
            .push("No commander is assigned to the command zone.".into());
    }

    let commander_names: HashSet<String> = deck
        .commanders
        .iter()
        .map(|entry| entry.name.to_ascii_lowercase())
        .collect();

    // A commander must not also appear in the 99.
    let before = deck.main.len();
    deck.main
        .retain(|entry| !commander_names.contains(&entry.name.to_ascii_lowercase()));
    if deck.main.len() != before {
        report
            .repairs
            .push("Removed a commander duplicated in the main deck.".into());
    }

    // --- Per-card rules: exists, legal, identity, singleton ---------------
    let mut seen = HashSet::<String>::new();
    let mut kept = Vec::<DeckEntry>::with_capacity(deck.main.len());

    for mut entry in std::mem::take(&mut deck.main) {
        let Some(card) = db.get_card(&entry.name)? else {
            report
                .repairs
                .push(format!("Removed unknown card '{}'.", entry.name));
            continue;
        };

        if !card.commander_legal {
            report
                .repairs
                .push(format!("Removed {} (not Commander-legal).", card.name));
            continue;
        }

        if !identity_subset(&card.color_identity, identity) {
            report.repairs.push(format!(
                "Removed {} (identity {} outside commander identity {}).",
                card.name,
                if card.color_identity.is_empty() { "C" } else { &card.color_identity },
                if identity.is_empty() { "C" } else { identity },
            ));
            continue;
        }

        let key = card.name.to_ascii_lowercase();

        // Merge duplicates written as separate lines.
        if !seen.insert(key.clone()) {
            if let Some(existing) = kept
                .iter_mut()
                .find(|existing| existing.name.eq_ignore_ascii_case(&card.name))
            {
                existing.quantity += entry.quantity;
            }
            continue;
        }

        entry.name = card.name.clone();
        kept.push(entry);
    }

    for entry in &mut kept {
        if let Some(card) = db.get_card(&entry.name)? {
            if !copies_allowed(&card, entry.quantity) {
                report.repairs.push(format!(
                    "Reduced {} from {} copies to 1 (singleton rule).",
                    entry.name, entry.quantity
                ));
                entry.quantity = 1;
            }
        }
    }

    deck.main = kept;

    // --- Exact deck size ---------------------------------------------------
    let mut total = deck_total(deck);

    if total > COMMANDER_DECK_SIZE {
        let excess = total - COMMANDER_DECK_SIZE;
        trim_to_size(deck, db, &protected, excess, &mut report)?;
    }

    total = deck_total(deck);

    if total < COMMANDER_DECK_SIZE {
        let missing = COMMANDER_DECK_SIZE - total;
        fill_with_basics(deck, identity, missing);
        report.repairs.push(format!(
            "Filled {missing} empty slot(s) with basic lands matching the commander identity."
        ));
    }

    // --- Re-verify hard rules after repairs ---------------------------------
    report.total_cards = deck_total(deck);

    if report.total_cards != COMMANDER_DECK_SIZE {
        report.hard_failures.push(format!(
            "Deck has {} cards after repair; Commander requires exactly {}.",
            report.total_cards, COMMANDER_DECK_SIZE
        ));
    }

    for entry in &deck.main {
        let Some(card) = db.get_card(&entry.name)? else {
            report
                .hard_failures
                .push(format!("'{}' still does not resolve.", entry.name));
            continue;
        };

        if !copies_allowed(&card, entry.quantity) {
            report
                .hard_failures
                .push(format!("{} still violates the singleton rule.", card.name));
        }

        if !identity_subset(&card.color_identity, identity) {
            report
                .hard_failures
                .push(format!("{} still violates colour identity.", card.name));
        }

        // --- Structural counts --------------------------------------------
        let roles = classify_card(&card);
        if is_land(&card) {
            report.lands += entry.quantity;
        }
        if roles.ramp && !is_land(&card) {
            report.ramp += entry.quantity;
        }
        if roles.draw {
            report.draw += entry.quantity;
        }
        if roles.removal || roles.counter || roles.wipe {
            report.interaction += entry.quantity;
        }
    }

    // --- Soft rules -------------------------------------------------------
    if report.lands < 28 {
        report.warnings.push(format!(
            "Only {} lands; most Commander decks run 33–38 (cEDH ~28–31 with heavy fast mana).",
            report.lands
        ));
    } else if report.lands > 42 {
        report.warnings.push(format!(
            "{} lands is high; consider converting some to ramp or card draw.",
            report.lands
        ));
    }

    if report.ramp < 6 {
        report
            .warnings
            .push(format!("Light on ramp ({}; aim for 8–12).", report.ramp));
    }
    if report.draw < 6 {
        report
            .warnings
            .push(format!("Light on card advantage ({}; aim for 8–12).", report.draw));
    }
    if report.interaction < 5 {
        report.warnings.push(format!(
            "Light on interaction ({}; aim for 8+ removal/counters/wipes).",
            report.interaction
        ));
    }

    // --- Goldfish playtest ------------------------------------------------
    match goldfish_with_combo_lines(deck, db, combo_lines, GOLDFISH_RUNS) {
        Ok(result) => {
            report.keepable_pct = Some(result.keepable_pct);
            if result.keepable_pct < MIN_KEEPABLE_PCT {
                report.warnings.push(format!(
                    "Goldfish: only {:.0}% of opening hands were keepable over {} runs.",
                    result.keepable_pct, GOLDFISH_RUNS
                ));
            }
        }
        Err(err) => report.warnings.push(format!("Goldfish test skipped: {err}")),
    }

    Ok(report)
}

/// Run the audit and fail loudly if hard rules cannot be satisfied.
pub fn require_integrity(
    deck: &mut ParsedDeck,
    db: &CardDb,
    identity: &str,
    protected: &[String],
    combo_lines: &[Vec<String>],
) -> Result<IntegrityReport> {
    let report = audit_and_repair(deck, db, identity, protected, combo_lines)?;

    if !report.passed() {
        return Err(anyhow!(
            "Deck integrity test failed — Commander Lab will not hand over an illegal list:\n{}",
            report.hard_failures.join("\n")
        ));
    }

    Ok(report)
}

fn trim_to_size(
    deck: &mut ParsedDeck,
    db: &CardDb,
    protected: &HashSet<String>,
    mut excess: u32,
    report: &mut IntegrityReport,
) -> Result<()> {
    // Rank trimmable cards: non-protected, non-land spells with the weakest
    // EDHREC rank go first; extra basics are trimmed before that.
    let mut candidates = Vec::<(usize, u32, bool)>::new(); // (index, rank, basic)

    for (index, entry) in deck.main.iter().enumerate() {
        if protected.contains(&entry.name.to_ascii_lowercase()) {
            continue;
        }
        let Some(card) = db.get_card(&entry.name)? else { continue };
        if card.is_basic {
            candidates.push((index, u32::MAX, true));
        } else if !is_land(&card) {
            candidates.push((index, card.edhrec_rank.unwrap_or(u32::MAX - 1), false));
        }
    }

    // Basics first, then worst EDHREC rank.
    candidates.sort_by(|a, b| b.2.cmp(&a.2).then(b.1.cmp(&a.1)));

    let mut removed = Vec::new();
    for (index, _, _) in candidates {
        if excess == 0 {
            break;
        }
        let entry = &mut deck.main[index];
        let take = entry.quantity.min(excess);
        entry.quantity -= take;
        excess -= take;
        removed.push(format!("{}×{}", take, entry.name));
    }

    deck.main.retain(|entry| entry.quantity > 0);

    if !removed.is_empty() {
        report.repairs.push(format!(
            "Trimmed oversized list to 100: {}.",
            removed.join(", ")
        ));
    }

    Ok(())
}

fn fill_with_basics(deck: &mut ParsedDeck, identity: &str, missing: u32) {
    let colors: Vec<char> = if identity.is_empty() {
        vec!['C']
    } else {
        identity.chars().collect()
    };

    for i in 0..missing {
        let name = basic_for_color(colors[i as usize % colors.len()]);
        if let Some(entry) = deck
            .main
            .iter_mut()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
        {
            entry.quantity += 1;
        } else {
            deck.main.push(DeckEntry {
                quantity: 1,
                name: name.to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(commander: &str, main: &[(u32, &str)]) -> ParsedDeck {
        ParsedDeck {
            name: "t".into(),
            commanders: vec![DeckEntry { quantity: 1, name: commander.into() }],
            main: main
                .iter()
                .map(|(q, n)| DeckEntry { quantity: *q, name: (*n).into() })
                .collect(),
            sideboard: vec![],
        }
    }

    #[test]
    fn fill_uses_identity_basics() {
        let mut d = deck("X", &[]);
        fill_with_basics(&mut d, "UB", 4);
        assert_eq!(deck_total(&d), 5);
        assert!(d.main.iter().any(|e| e.name == "Island" && e.quantity == 2));
        assert!(d.main.iter().any(|e| e.name == "Swamp" && e.quantity == 2));
    }

    #[test]
    fn fill_colorless_uses_wastes() {
        let mut d = deck("X", &[]);
        fill_with_basics(&mut d, "", 3);
        assert_eq!(d.main[0].name, "Wastes");
        assert_eq!(d.main[0].quantity, 3);
    }
}
