//! Budget limits for generated decks (US dollars, Scryfall's TCGplayer prices).
//!
//! Two limits, either or both:
//! - `per_card`: no card above this price enters the candidate pool.
//! - `total`: the whole deck, commander included. Building already leans
//!   cheap (a total implies a per-card ceiling), and afterwards the priciest
//!   cards are swapped for cheaper ones that fill the same role until the deck
//!   fits.
//!
//! Basic lands count as free; cards with no price are allowed (usually very
//! new printings) and count as free.
//!
//! Things to work on:
//! - Prices are only as fresh as the last card-data update.

use crate::{
    analysis::{classify_card, identity_subset, CardRoles},
    data::{Card, CardDb},
    deck::{DeckEntry, ParsedDeck},
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Budget {
    /// Whole deck, commander included.
    #[serde(default)]
    pub total: Option<f64>,
    /// Most any single card may cost.
    #[serde(default)]
    pub per_card: Option<f64>,
}

/// What fitting the deck to its budget did.
#[derive(Debug, Clone, Default)]
pub struct BudgetReport {
    pub before: f64,
    pub after: f64,
    pub swaps: Vec<(String, String)>,
    pub fits: bool,
}

impl Budget {
    /// Budget from the Build screen; zero, negative or non-numbers mean "no limit".
    pub fn new(total: Option<f64>, per_card: Option<f64>) -> Self {
        let clean = |v: Option<f64>| v.filter(|v| v.is_finite() && *v > 0.0);
        Self { total: clean(total), per_card: clean(per_card) }
    }

    /// Any limit at all?
    pub fn is_set(&self) -> bool {
        self.total.is_some() || self.per_card.is_some()
    }

    /// The price ceiling for any one card. A total budget implies one too:
    /// a few cards at 8% of the budget leave room for the other 95.
    pub fn card_cap(&self) -> Option<f64> {
        let from_total = self.total.map(|t| (t * 0.08).max(0.5));
        match (self.per_card, from_total) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Under the per-card ceiling (always true without one).
    pub fn card_ok(&self, card: &Card) -> bool {
        match self.card_cap() {
            Some(cap) => price(card) <= cap,
            None => true,
        }
    }

    /// The same limits, with the per-card ceiling worked out from what's left
    /// after cards already decided (the commander).
    pub fn after_spending(&self, spent: f64) -> Budget {
        Budget {
            total: self.total.map(|t| (t - spent).max(t * 0.2)),
            per_card: self.per_card,
        }
    }

    /// "$150 total, $5.00 per card at most", for logs and notes.
    pub fn describe(&self) -> String {
        match (self.total, self.per_card) {
            (Some(t), Some(c)) => format!("${t:.0} total, ${c:.2} per card at most"),
            (Some(t), None) => format!("${t:.0} total"),
            (None, Some(c)) => format!("${c:.2} per card at most"),
            (None, None) => "no budget".into(),
        }
    }
}

/// What a card counts for: basics are free, unknown prices count as free.
pub fn price(card: &Card) -> f64 {
    if card.is_basic {
        0.0
    } else {
        card.price_usd.unwrap_or(0.0)
    }
}

/// Deck price (commanders and main deck) using `price`.
pub fn deck_price(deck: &ParsedDeck, db: &CardDb) -> f64 {
    deck.commanders
        .iter()
        .chain(deck.main.iter())
        .map(|e| db.get_card(&e.name).ok().flatten().map(|c| price(&c) * e.quantity as f64).unwrap_or(0.0))
        .sum()
}

/// Any land (basic or not).
fn is_land(card: &Card) -> bool {
    card.type_line.to_ascii_lowercase().contains("land")
}

/// How many of the deck's colors a land makes (produced_mana is like "WUG").
fn colors_made(card: &Card, identity: &str) -> usize {
    identity.chars().filter(|c| card.produced_mana.contains(*c)).count()
}

/// How many roles two cards share: a cheaper swap should do the same jobs.
fn shared_roles(a: CardRoles, b: CardRoles) -> u32 {
    [
        (a.ramp, b.ramp),
        (a.draw, b.draw),
        (a.tutor, b.tutor),
        (a.removal, b.removal),
        (a.counter, b.counter),
        (a.wipe, b.wipe),
        (a.recursion, b.recursion),
        (a.protection, b.protection),
        (a.graveyard, b.graveyard),
    ]
    .iter()
    .filter(|(x, y)| *x && *y)
    .count() as u32
}

/// Which basic to add when a priced land is cut: the deck's color with the
/// fewest basics so far (Wastes for colorless).
fn basic_for(identity: &str, deck: &ParsedDeck) -> &'static str {
    // The color with the fewest basics so far, to keep the mana even.
    let names = [('W', "Plains"), ('U', "Island"), ('B', "Swamp"), ('R', "Mountain"), ('G', "Forest")];
    names
        .iter()
        .filter(|(c, _)| identity.contains(*c))
        .min_by_key(|(_, n)| deck.main.iter().filter(|e| e.name == *n).map(|e| e.quantity).sum::<u32>())
        .map(|(_, n)| *n)
        .unwrap_or("Wastes")
}

/// Add one copy of a card (merging with an existing line).
fn add_one(main: &mut Vec<DeckEntry>, name: &str) {
    if let Some(e) = main.iter_mut().find(|e| e.name == name) {
        e.quantity += 1;
    } else {
        main.push(DeckEntry { quantity: 1, name: name.to_string(), ..Default::default() });
    }
}

/// Swap the priciest cards for cheaper ones until the deck fits `budget.total`.
/// `protected` (lowercase names) are never swapped; `score` ranks replacements
/// (higher is better) and `fits_deck` says whether a card may be added at all.
pub fn fit_total(
    deck: &mut ParsedDeck,
    budget: &Budget,
    db: &CardDb,
    pool: &[Card],
    identity: &str,
    protected: &HashSet<String>,
    score: impl Fn(&Card) -> i64,
    fits_deck: impl Fn(&Card) -> bool,
) -> Result<BudgetReport> {
    let before = deck_price(deck, db);
    let mut report = BudgetReport { before, after: before, fits: true, swaps: Vec::new() };
    let Some(limit) = budget.total else { return Ok(report) };
    let mut total = before;
    let mut tried: HashSet<String> = HashSet::new();

    for _ in 0..150 {
        if total <= limit {
            break;
        }
        // The priciest card that may still be swapped.
        let mut priciest: Option<(usize, Card)> = None;
        for (i, entry) in deck.main.iter().enumerate() {
            let key = entry.name.to_ascii_lowercase();
            if protected.contains(&key) || tried.contains(&key) {
                continue;
            }
            let Some(card) = db.get_card(&entry.name)? else { continue };
            if card.is_basic || price(&card) < 0.30 {
                continue;
            }
            if priciest.as_ref().map(|(_, c)| price(&card) > price(c)).unwrap_or(true) {
                priciest = Some((i, card));
            }
        }
        let Some((index, old)) = priciest else { break };
        tried.insert(old.name.to_ascii_lowercase());

        let in_deck: HashSet<String> = deck
            .commanders
            .iter()
            .chain(deck.main.iter())
            .map(|e| e.name.to_ascii_lowercase())
            .collect();
        let old_price = price(&old);
        let over = total - limit;
        // Cheap enough to matter: at most half the old price, and ideally
        // enough to close the gap on its own.
        let ceiling = (old_price * 0.5).min(budget.card_cap().unwrap_or(f64::MAX));
        let old_roles = classify_card(&old);
        let old_land = is_land(&old);
        // A replacement land must make at least one of the deck's colors
        // (two in a three-plus color deck), or a basic is better.
        let colors_needed = if identity.len() >= 3 { 2 } else { 1 }.min(identity.len().max(1));

        let best = pool
            .iter()
            .filter(|c| {
                !c.is_basic
                    && is_land(c) == old_land
                    && price(c) <= ceiling
                    && !in_deck.contains(&c.name.to_ascii_lowercase())
                    && identity_subset(&c.color_identity, identity)
                    && (!old_land || identity.is_empty() || colors_made(c, identity) >= colors_needed)
                    && fits_deck(c)
            })
            .max_by_key(|c| {
                let roles = shared_roles(old_roles, classify_card(c)) as i64;
                let closes_gap = (old_price - price(c) >= over) as i64;
                // Keep the curve: a similar mana value plays the same role.
                let curve = -((c.cmc - old.cmc).abs().min(6.0) * 8_000.0) as i64;
                let colors = if old_land { colors_made(c, identity) as i64 * 30_000 } else { 0 };
                roles * 100_000 + closes_gap * 20_000 + curve + colors + score(c)
            })
            .cloned();

        let replacement = match best {
            Some(card) => card.name,
            // An expensive land with no cheap stand-in: a basic does the job.
            None if old_land => basic_for(identity, deck).to_string(),
            None => continue,
        };
        let new_price = db.get_card(&replacement)?.map(|c| price(&c)).unwrap_or(0.0);
        let entry = deck.main.remove(index);
        if entry.quantity > 1 {
            deck.main.insert(index, DeckEntry { quantity: entry.quantity - 1, ..entry.clone() });
        }
        add_one(&mut deck.main, &replacement);
        total += new_price - old_price;
        report.swaps.push((old.name.clone(), replacement));
    }

    report.after = deck_price(deck, db);
    report.fits = report.after <= limit + 0.005;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_budget_implies_a_card_ceiling() {
        let b = Budget::new(Some(100.0), None);
        assert_eq!(b.card_cap(), Some(8.0));
        let b = Budget::new(Some(100.0), Some(2.0));
        assert_eq!(b.card_cap(), Some(2.0));
        let b = Budget::new(Some(5.0), None);
        assert_eq!(b.card_cap(), Some(0.5));
        assert!(!Budget::new(Some(0.0), Some(f64::NAN)).is_set());
    }
}
