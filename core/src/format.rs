//! Deck formats Commander Lab can build and check: Commander, Pauper
//! Commander (PDH) and Canadian Highlander.
//!
//! Sources for the rules encoded here:
//! - PDH: PDH Home Base rules (pdhhomebase.com/rules). Commander is an uncommon
//!   creature, Vehicle or Spacecraft (uncommon Backgrounds may be the second
//!   commander); the 99 are commons; rarity counts paper and MTGO printings
//!   only; 30 life, 16 commander damage; banned: Mystic Remora, Rhystic Study,
//!   ante cards, Conspiracies and cards Wizards removed from constructed play.
//! - Canadian Highlander: canadianhighlander.ca. 100 cards minimum, singleton,
//!   no sideboard, the Vintage banned list plus a few card categories, and no
//!   more than 10 points from the points list (Fall 2026 list, effective
//!   1 September 2026). 20 life, played one-on-one.

use crate::data::{pdh_commander_type, Card};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    #[default]
    Commander,
    Pdh,
    Canlander,
}

/// When the Canadian Highlander points list below was last checked.
pub const CANLANDER_POINTS_DATE: &str = "1 September 2026";
pub const CANLANDER_MAX_POINTS: u32 = 10;

/// Canadian Highlander points list (Fall 2026 update).
pub const CANLANDER_POINTS: &[(&str, u32)] = &[
    ("Ancestral Recall", 8),
    ("Black Lotus", 7),
    ("Flash", 7),
    ("Time Vault", 7),
    ("Mana Crypt", 5),
    ("Thassa's Oracle", 5),
    ("Time Walk", 5),
    ("Demonic Tutor", 3),
    ("Mox Emerald", 3),
    ("Mox Jet", 3),
    ("Mox Pearl", 3),
    ("Mox Ruby", 3),
    ("Mox Sapphire", 3),
    ("Sol Ring", 3),
    ("Tinker", 3),
    ("Underworld Breach", 3),
    ("Mystical Tutor", 2),
    ("Tolarian Academy", 2),
    ("Vampiric Tutor", 2),
    ("Ancient Tomb", 1),
    ("Balance", 1),
    ("Dig Through Time", 1),
    ("Enlightened Tutor", 1),
    ("Forth Eorlingas!", 1),
    ("Gifts Ungiven", 1),
    ("Imperial Seal", 1),
    ("Intuition", 1),
    ("Mana Drain", 1),
    ("Mana Vault", 1),
    ("Merchant Scroll", 1),
    ("Nadu, Winged Wisdom", 1),
    ("Natural Order", 1),
    ("Psychic Frog", 1),
    ("Reanimate", 1),
    ("Spellseeker", 1),
    ("Strip Mine", 1),
    ("Tainted Pact", 1),
    ("Tamiyo, Inquisitive Student", 1),
    ("The Fantasticar", 1),
    ("The One Ring", 1),
    ("Treasure Cruise", 1),
    ("True-Name Nemesis", 1),
    ("Urza's Saga", 1),
    ("White Plume Adventurer", 1),
    ("Wishclaw Talisman", 1),
    ("Wrenn and Six", 1),
];

/// Banned in multiplayer PDH by name (on top of the categories).
const PDH_BANNED: &[&str] = &["Mystic Remora", "Rhystic Study"];

/// Cards Wizards removed from all constructed play (June 2020). Banned in
/// PDH; Canadian Highlander bans them through the Vintage list.
const REMOVED_FROM_CONSTRUCTED: &[&str] = &[
    "Invoke Prejudice",
    "Cleanse",
    "Stone-Throwing Devils",
    "Pradesh Gypsies",
    "Jihad",
    "Imprison",
    "Crusade",
];

/// Basic land names, which every format allows any number of.
pub fn is_basic_land_name(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "plains" | "island" | "swamp" | "mountain" | "forest" | "wastes"
            | "snow-covered plains" | "snow-covered island" | "snow-covered swamp"
            | "snow-covered mountain" | "snow-covered forest" | "snow-covered wastes"
    )
}

fn same_card(card_name: &str, listed: &str) -> bool {
    let card = card_name.to_ascii_lowercase();
    let listed = listed.to_ascii_lowercase();
    card == listed || card.split(" // ").next() == Some(listed.as_str())
}

/// Canadian Highlander points for a card (0 if it isn't on the list).
pub fn canlander_points(name: &str) -> u32 {
    CANLANDER_POINTS
        .iter()
        .find(|(listed, _)| same_card(name, listed))
        .map(|(_, p)| *p)
        .unwrap_or(0)
}

/// Ante cards ("Remove this card from your deck before playing if you're not
/// playing for ante").
fn mentions_ante(card: &Card) -> bool {
    let text = card.oracle_text.to_ascii_lowercase();
    text.split(|c: char| !c.is_ascii_alphabetic()).any(|w| w == "ante")
}

impl Format {
    pub fn parse(value: &str) -> Format {
        match value.trim().to_ascii_lowercase().replace(['-', '_'], " ").as_str() {
            "pdh" | "pauper commander" | "pauper edh" | "pedh" | "pauper" => Format::Pdh,
            "canlander" | "canadian highlander" | "ch" | "highlander" => Format::Canlander,
            _ => Format::Commander,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Format::Commander => "commander",
            Format::Pdh => "pdh",
            Format::Canlander => "canlander",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Commander => "Commander",
            Format::Pdh => "Pauper Commander",
            Format::Canlander => "Canadian Highlander",
        }
    }

    pub fn has_commander(self) -> bool {
        !matches!(self, Format::Canlander)
    }

    pub fn is_commander(self) -> bool {
        matches!(self, Format::Commander)
    }

    /// Cards in a finished deck (commanders included).
    pub fn deck_size(self) -> usize {
        100
    }

    /// Commander formats need exactly 100; Canadian Highlander is a minimum.
    pub fn exact_size(self) -> bool {
        self.has_commander()
    }

    pub fn starting_life(self) -> u32 {
        match self {
            Format::Commander => 40,
            Format::Pdh => 30,
            Format::Canlander => 20,
        }
    }

    pub fn commander_damage(self) -> Option<u32> {
        match self {
            Format::Commander => Some(21),
            Format::Pdh => Some(16),
            Format::Canlander => None,
        }
    }

    pub fn max_points(self) -> Option<u32> {
        matches!(self, Format::Canlander).then_some(CANLANDER_MAX_POINTS)
    }

    /// Whether the format's legality needs the per-format columns that card
    /// data from Commander Lab 0.27+ provides.
    pub fn needs_format_data(self) -> bool {
        !self.is_commander()
    }

    /// May this card be in the main deck (the 99, or the 100 for Highlander)?
    pub fn card_allowed(self, card: &Card) -> Result<(), String> {
        if card.is_basic {
            return Ok(());
        }
        // Sticker sheets and Attractions live outside the deck.
        let ty = card.type_line.to_ascii_lowercase();
        if ty.starts_with("stickers") || ty.contains("attraction") {
            return Err("not a card you put in a deck".into());
        }
        match self {
            Format::Commander => {
                if card.commander_legal {
                    Ok(())
                } else {
                    Err("not legal in Commander".into())
                }
            }
            Format::Pdh => {
                if PDH_BANNED.iter().chain(REMOVED_FROM_CONSTRUCTED).any(|b| same_card(&card.name, b)) {
                    return Err("banned in Pauper Commander".into());
                }
                if card.type_line.to_ascii_lowercase().contains("conspiracy") || mentions_ante(card) {
                    return Err("banned in Pauper Commander".into());
                }
                // Joke, Acorn and playtest cards aren't tournament legal even
                // when Scryfall lists them for Pauper Commander.
                if card.vintage_legal == "not_legal" {
                    return Err("not a tournament-legal card".into());
                }
                match card.pdh_legal.as_str() {
                    "legal" => Ok(()),
                    "banned" => Err("banned in Pauper Commander".into()),
                    "" if card.rarity.eq_ignore_ascii_case("common") => Ok(()),
                    _ => Err("never printed at common in paper or on MTGO".into()),
                }
            }
            Format::Canlander => {
                if REMOVED_FROM_CONSTRUCTED.iter().any(|b| same_card(&card.name, b))
                    || card.type_line.to_ascii_lowercase().contains("conspiracy")
                    || mentions_ante(card)
                {
                    return Err("banned in Canadian Highlander".into());
                }
                match card.vintage_legal.as_str() {
                    "legal" | "restricted" => Ok(()),
                    "banned" => Err("on the Vintage banned list".into()),
                    "" if card.commander_legal => Ok(()),
                    _ => Err("not a tournament-legal card".into()),
                }
            }
        }
    }

    /// May this card lead the deck? `second` is true for a partner or
    /// Background slot.
    pub fn commander_allowed(self, card: &Card, second: bool) -> Result<(), String> {
        match self {
            Format::Commander => {
                if card.commander_eligible && card.commander_legal {
                    Ok(())
                } else {
                    Err("can't be a Commander commander".into())
                }
            }
            Format::Pdh => {
                let ty = card.type_line.to_ascii_lowercase();
                if PDH_BANNED.iter().chain(REMOVED_FROM_CONSTRUCTED).any(|b| same_card(&card.name, b))
                    || mentions_ante(card)
                {
                    return Err("banned in Pauper Commander".into());
                }
                if !pdh_commander_type(&ty) {
                    return Err("PDH commanders must be creatures, Vehicles or Spacecraft".into());
                }
                if ty.contains("background") && !ty.contains("creature") && !second {
                    return Err("a Background can only be the second commander".into());
                }
                if card.vintage_legal == "not_legal" {
                    return Err("not a tournament-legal card".into());
                }
                let uncommon = card.pdh_commander
                    || card.rarity.eq_ignore_ascii_case("uncommon") && !card.pdh_legal.is_empty();
                if uncommon {
                    Ok(())
                } else {
                    Err("PDH commanders must have been printed at uncommon in paper or on MTGO".into())
                }
            }
            Format::Canlander => Err("Canadian Highlander has no commander".into()),
        }
    }
}

/// A finished deck checked against its format.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FormatCheck {
    pub format: &'static str,
    pub label: &'static str,
    pub cards: usize,
    pub points: u32,
    pub max_points: Option<u32>,
    pub pointed_cards: Vec<(String, u32)>,
    pub problems: Vec<String>,
}

impl FormatCheck {
    pub fn legal(&self) -> bool {
        self.problems.is_empty()
    }
}

/// Check a deck (commanders + main) against a format's construction rules.
pub fn check_deck(
    format: Format,
    commanders: &[Card],
    main: &[(u32, Card)],
    color_identity: &str,
) -> FormatCheck {
    let mut check = FormatCheck {
        format: format.id(),
        label: format.label(),
        max_points: format.max_points(),
        ..Default::default()
    };
    let cards = commanders.len() + main.iter().map(|(q, _)| *q as usize).sum::<usize>();
    check.cards = cards;

    if format.exact_size() && cards != format.deck_size() {
        check.problems.push(format!("{} decks have exactly 100 cards (this one has {cards}).", format.label()));
    } else if !format.exact_size() && cards < format.deck_size() {
        check.problems.push(format!("{} decks need at least 100 cards (this one has {cards}).", format.label()));
    }

    if format.has_commander() {
        if commanders.is_empty() {
            check.problems.push("Choose a commander.".into());
        }
        for (i, c) in commanders.iter().enumerate() {
            if let Err(why) = format.commander_allowed(c, i > 0) {
                check.problems.push(format!("{}: {why}.", c.name));
            }
        }
    } else if !commanders.is_empty() {
        check.problems.push(format!("{} has no commander; move it into the deck.", format.label()));
    }

    for (qty, card) in main {
        if *qty > 1 && !card.is_basic && !crate::analysis::has_copy_rule_override(card) {
            check.problems.push(format!("{}: only one copy allowed ({qty}).", card.name));
        }
        if let Err(why) = format.card_allowed(card) {
            check.problems.push(format!("{}: {why}.", card.name));
        }
        if format.has_commander()
            && !crate::analysis::identity_subset(&card.color_identity, color_identity)
        {
            check.problems.push(format!("{}: outside the commander's colors.", card.name));
        }
        if format == Format::Canlander {
            let p = canlander_points(&card.name);
            if p > 0 {
                check.points += p;
                check.pointed_cards.push((card.name.clone(), p));
            }
        }
    }

    if let Some(max) = format.max_points() {
        if check.points > max {
            check.problems.push(format!("{} points used; the limit is {max}.", check.points));
        }
    }
    check
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, ty: &str) -> Card {
        Card {
            oracle_id: name.into(),
            name: name.into(),
            mana_cost: String::new(),
            cmc: 2.0,
            type_line: ty.into(),
            oracle_text: String::new(),
            color_identity: String::new(),
            produced_mana: String::new(),
            commander_legal: true,
            edhrec_rank: None,
            keywords: String::new(),
            is_basic: false,
            commander_eligible: false,
            image_uri: String::new(),
            price_usd: None,
            price_usd_foil: None,
            price_eur: None,
            price_eur_foil: None,
            price_tix: None,
            tcgplayer_uri: String::new(),
            cardmarket_uri: String::new(),
            cardhoarder_uri: String::new(),
            game_changer: false,
            rarity: "common".into(),
            pdh_legal: "legal".into(),
            vintage_legal: "legal".into(),
            pdh_commander: false,
        }
    }

    #[test]
    fn parses_format_names() {
        assert_eq!(Format::parse("Pauper EDH"), Format::Pdh);
        assert_eq!(Format::parse("canadian-highlander"), Format::Canlander);
        assert_eq!(Format::parse(""), Format::Commander);
    }

    #[test]
    fn pdh_rules() {
        let mut rhystic = card("Rhystic Study", "Enchantment");
        rhystic.pdh_legal = "banned".into();
        assert!(Format::Pdh.card_allowed(&rhystic).is_err());

        let mut rare = card("Esper Sentinel", "Artifact Creature");
        rare.pdh_legal = "not_legal".into();
        assert!(Format::Pdh.card_allowed(&rare).is_err());
        assert!(Format::Pdh.card_allowed(&card("Ninja of the Deep Hours", "Creature — Human Ninja")).is_ok());

        let mut uncommon = card("Murktide Rager", "Creature — Merfolk");
        uncommon.pdh_commander = true;
        assert!(Format::Pdh.commander_allowed(&uncommon, false).is_ok());
        let mut bg = card("Agent of the Iron Throne", "Legendary Enchantment — Background");
        bg.pdh_commander = true;
        assert!(Format::Pdh.commander_allowed(&bg, false).is_err());
        assert!(Format::Pdh.commander_allowed(&bg, true).is_ok());
        let mut land = card("Some Land", "Land Creature");
        land.pdh_commander = true;
        assert!(Format::Pdh.commander_allowed(&land, false).is_err());
    }

    #[test]
    fn canlander_points_and_bans() {
        assert_eq!(canlander_points("Ancestral Recall"), 8);
        assert_eq!(canlander_points("Tamiyo, Inquisitive Student // Tamiyo, Seasoned Scholar"), 1);
        assert_eq!(canlander_points("Lightning Bolt"), 0);
        let mut orb = card("Chaos Orb", "Artifact");
        orb.vintage_legal = "banned".into();
        assert!(Format::Canlander.card_allowed(&orb).is_err());
        let mut lotus = card("Black Lotus", "Artifact");
        lotus.vintage_legal = "restricted".into();
        assert!(Format::Canlander.card_allowed(&lotus).is_ok());

        let main: Vec<(u32, Card)> = ["Black Lotus", "Sol Ring", "Mana Vault"]
            .iter()
            .map(|n| (1, card(n, "Artifact")))
            .collect();
        let check = check_deck(Format::Canlander, &[], &main, "");
        assert_eq!(check.points, 11);
        assert!(check.problems.iter().any(|p| p.contains("limit is 10")));
    }
}
