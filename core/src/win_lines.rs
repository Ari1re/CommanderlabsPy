//! Win lines: how decks with this commander usually win.
//!
//! Generation studies the commander's reference decklists for two kinds of
//! win line: combos that recur across them, and finishers (cards that end
//! the game: alternate wins, drains, overruns, extra combats, damage
//! doublers, infect). The most common finishers that fit the deck are added
//! while it's built; every line is recorded in the game plan, and the ones
//! the deck doesn't run go into the primer as potential changes, with what
//! they'd need and why they were left out.

use crate::{
    analysis::identity_subset,
    data::{Card, CardDb},
    deck::ParsedDeck,
    references::ReferenceSnapshot,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WinLine {
    /// "combo" or "finisher".
    pub kind: String,
    /// What kind of finisher ("Overrun", "Drain"…) or the combo's result.
    pub label: String,
    pub cards: Vec<String>,
    /// Reference decks that run it, and the share of all reference decks.
    pub decks: u32,
    pub share: f64,
    /// Every card is in the finished deck.
    pub included: bool,
    /// Cards the deck would need to add.
    #[serde(default)]
    pub missing: Vec<String>,
    /// Why it isn't in the deck (format, colors, budget, bracket, or space).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why_not: Option<String>,
    /// Commander Spellbook's bracket for a combo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bracket: Option<u8>,
}

/// The kind of finisher a card is, from its rules text, or None.
pub fn finisher_kind(card: &Card) -> Option<&'static str> {
    let t = card.oracle_text.to_lowercase();
    let kw = card.keywords.to_lowercase();
    if t.contains("you win the game") || t.contains("loses the game") && t.contains("opponent") {
        return Some("Alternate win");
    }
    if t.contains("additional combat phase") || t.contains("extra combat") {
        return Some("Extra combat");
    }
    if t.contains("deals double that damage") || t.contains("double damage") || t.contains("damage to that permanent or player instead") && t.contains("twice") {
        return Some("Damage doubler");
    }
    if kw.contains("infect") && !card.type_line.to_lowercase().contains("creature") || t.contains("poison counter") && t.contains("opponent") && !kw.contains("infect") {
        return Some("Infect / poison");
    }
    if (t.contains("creatures you control get +") || t.contains("creatures you control gain trample")) && (t.contains("trample") || t.contains("until end of turn")) && !t.contains("other creatures you control get +1/+1.") {
        return Some("Overrun");
    }
    if t.contains("each opponent loses") && (t.contains(" x ") || t.contains("equal to") || t.contains("for each")) {
        return Some("Drain");
    }
    if t.contains("damage to each opponent") && (t.contains("equal to") || t.contains("x damage")) {
        return Some("Burn");
    }
    None
}

/// Study the reference decks: recurring combos and the finishers they run,
/// most common first. Lines in fewer than `min_share` of the decks are left out.
pub fn study(refs: &ReferenceSnapshot, db: &CardDb, min_share: f64) -> Vec<WinLine> {
    let total = refs.deck_count().max(1) as f64;
    let mut out = Vec::new();
    for (cards, result, bracket, count) in refs.top_combos(24) {
        let share = count as f64 / total;
        if share < min_share || cards.is_empty() {
            continue;
        }
        out.push(WinLine {
            kind: "combo".into(),
            label: result.lines().next().unwrap_or("Combo").trim().to_string(),
            cards,
            decks: count,
            share,
            included: false,
            missing: Vec::new(),
            why_not: None,
            bracket,
        });
    }
    for (name, count) in refs.top_cards(600) {
        let share = count as f64 / total;
        if share < min_share {
            break;
        }
        let Ok(Some(card)) = db.get_card(&name) else { continue };
        if let Some(kind) = finisher_kind(&card) {
            out.push(WinLine {
                kind: "finisher".into(),
                label: kind.into(),
                cards: vec![card.name.clone()],
                decks: count,
                share,
                included: false,
                missing: Vec::new(),
                why_not: None,
                bracket: None,
            });
        }
    }
    out.sort_by(|a, b| b.share.partial_cmp(&a.share).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(20);
    out
}

/// Fill in which lines the finished deck runs, what the others need, and why
/// they were left out. `blocked` explains why a card can't go in (format,
/// colors, budget, bracket), or None if it could.
pub fn mark(lines: &mut [WinLine], deck: &ParsedDeck, db: &CardDb, blocked: impl Fn(&Card) -> Option<String>) {
    let in_deck: std::collections::HashSet<String> = deck
        .commanders
        .iter()
        .chain(deck.main.iter())
        .map(|e| db.get_card(&e.name).ok().flatten().map(|c| c.name).unwrap_or_else(|| e.name.clone()).to_lowercase())
        .collect();
    for line in lines.iter_mut() {
        line.missing.clear();
        line.why_not = None;
        let mut reasons: Vec<String> = Vec::new();
        for name in &line.cards {
            let card = db.get_card(name).ok().flatten();
            let canonical = card.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| name.clone());
            if in_deck.contains(&canonical.to_lowercase()) {
                continue;
            }
            line.missing.push(canonical.clone());
            match card {
                None => reasons.push(format!("{canonical} isn't in the card data")),
                Some(c) => {
                    if let Some(why) = blocked(&c) {
                        reasons.push(format!("{canonical} is {why}"));
                    }
                }
            }
        }
        line.included = line.missing.is_empty();
        if !line.included {
            line.why_not = Some(if reasons.is_empty() {
                "it fits, but its slots went to cards that scored higher for this build".into()
            } else {
                reasons.join("; ")
            });
        }
    }
}

/// Why `card` can't go in a deck with these limits, or None.
pub fn why_blocked(
    card: &Card,
    format: crate::format::Format,
    identity: &str,
    budget: &crate::budget::Budget,
    game_changers_allowed: bool,
) -> Option<String> {
    if let Err(why) = format.card_allowed(card) {
        return Some(why);
    }
    if !identity_subset(&card.color_identity, identity) {
        return Some("outside the deck's colors".into());
    }
    if !budget.card_ok(card) {
        return Some(format!("over the budget ({})", card.price_usd.map(|p| format!("${p:.2}")).unwrap_or_default()));
    }
    if card.game_changer && !game_changers_allowed {
        return Some("a Game Changer, which this bracket doesn't allow".into());
    }
    None
}

/// One line for the primer.
pub fn describe(line: &WinLine) -> String {
    let what = if line.kind == "combo" {
        format!("{} ({})", line.cards.join(" + "), line.label)
    } else {
        format!("{} ({})", line.cards.join(" + "), line.label.to_lowercase())
    };
    format!("{what}: in {:.0}% of reference decks ({})", line.share * 100.0, line.decks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, text: &str, kw: &str, ty: &str) -> Card {
        Card { name: name.into(), oracle_text: text.into(), keywords: kw.into(), type_line: ty.into(), ..Default::default() }
    }

    #[test]
    fn finishers_are_recognized() {
        let hoof = card("Craterhoof Behemoth", "Haste\nWhen Craterhoof Behemoth enters, creatures you control gain trample and get +X/+X until end of turn, where X is the number of creatures you control.", "Haste", "Creature — Beast");
        assert_eq!(finisher_kind(&hoof), Some("Overrun"));
        let ex = card("Exsanguinate", "Each opponent loses X life. You gain life equal to the life lost this way.", "", "Sorcery");
        assert_eq!(finisher_kind(&ex), Some("Drain"));
        let oracle = card("Thassa's Oracle", "When Thassa's Oracle enters, look at the top X cards... If X is greater than or equal to the number of cards in your library, you win the game.", "", "Creature");
        assert_eq!(finisher_kind(&oracle), Some("Alternate win"));
        let anthem = card("Glorious Anthem", "Creatures you control get +1/+1.", "", "Enchantment");
        assert_eq!(finisher_kind(&anthem), None);
        let bolt = card("Lightning Bolt", "Lightning Bolt deals 3 damage to any target.", "", "Instant");
        assert_eq!(finisher_kind(&bolt), None);
    }
}
