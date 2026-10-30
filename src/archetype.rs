use crate::{
    data::{Card, CardDb},
    deck::ParsedDeck,
};
use anyhow::Result;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ArchetypeTag {
    pub name: String,
    pub score: i32,
    pub matched_cards: u32,
}

const TAGS: &[&str] = &[
    "Aristocrats",
    "Artifacts",
    "Auras",
    "Big Mana",
    "Blink",
    "Counters",
    "Enchantments",
    "Equipment",
    "Graveyard",
    "Landfall",
    "Lands",
    "Legends",
    "Lifegain",
    "Mill",
    "Proliferate",
    "Reanimator",
    "Sacrifice",
    "Spellslinger",
    "Stax",
    "Storm",
    "Tokens",
    "Treasure",
    "Typal",
    "Voltron",
    "Wheels",
];

pub fn detect_deck_tags(deck: &ParsedDeck, db: &CardDb) -> Result<Vec<ArchetypeTag>> {
    let mut scores: HashMap<String, (i32, u32)> = HashMap::new();

    for tag in TAGS {
        scores.insert((*tag).to_string(), (0, 0));
    }

    for entry in deck.commanders.iter().chain(deck.main.iter()) {
        let Some(card) = db.get_card(&entry.name)? else {
            continue;
        };

        let weight = if deck
            .commanders
            .iter()
            .any(|commander| commander.name.eq_ignore_ascii_case(&entry.name))
        {
            8
        } else {
            1
        };

        for tag in tags_for_card(&card) {
            let slot = scores.entry(tag).or_default();
            slot.0 += weight * entry.quantity as i32;
            slot.1 += entry.quantity;
        }
    }

    let mut tags = scores
        .into_iter()
        .filter_map(|(name, (score, matched_cards))| {
            if score <= 0 {
                None
            } else {
                Some(ArchetypeTag {
                    name,
                    score,
                    matched_cards,
                })
            }
        })
        .collect::<Vec<_>>();

    tags.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.matched_cards.cmp(&a.matched_cards))
            .then_with(|| a.name.cmp(&b.name))
    });

    tags.truncate(8);
    Ok(tags)
}

pub fn tag_fit_score(card: &Card, deck_tags: &[ArchetypeTag]) -> i32 {
    let card_tags = tags_for_card(card);

    deck_tags
        .iter()
        .take(5)
        .filter(|deck_tag| card_tags.iter().any(|tag| tag == &deck_tag.name))
        .map(|deck_tag| {
            if deck_tag.score >= 12 {
                7
            } else if deck_tag.score >= 7 {
                5
            } else {
                3
            }
        })
        .sum()
}

pub fn tag_names_for_card(card: &Card) -> Vec<String> {
    tags_for_card(card)
}

fn tags_for_card(card: &Card) -> Vec<String> {
    let oracle = card.oracle_text.to_ascii_lowercase();
    let type_line = card.type_line.to_ascii_lowercase();
    let name = card.name.to_ascii_lowercase();

    let mut tags = Vec::<String>::new();

    let mut add = |tag: &str| {
        if !tags.iter().any(|existing| existing == tag) {
            tags.push(tag.to_string());
        }
    };

    if oracle.contains("sacrifice")
        || oracle.contains("when another creature dies")
        || oracle.contains("whenever another creature dies")
    {
        add("Sacrifice");
    }

    if (oracle.contains("dies")
        || oracle.contains("sacrifice"))
        && (oracle.contains("lose 1 life")
            || oracle.contains("loses 1 life")
            || oracle.contains("drain")
            || oracle.contains("create a")
            || oracle.contains("draw a card"))
    {
        add("Aristocrats");
    }

    if type_line.contains("artifact") || oracle.contains("artifact") {
        add("Artifacts");
    }

    if type_line.contains("enchantment") || oracle.contains("enchantment") {
        add("Enchantments");
    }

    if type_line.contains("aura") || oracle.contains("aura") {
        add("Auras");
    }

    if type_line.contains("equipment")
        || oracle.contains("equip ")
        || oracle.contains("equipped creature")
    {
        add("Equipment");
        add("Voltron");
    }

    if oracle.contains("commander you control")
        || oracle.contains("commander creature")
        || oracle.contains("commander damage")
    {
        add("Voltron");
    }

    if oracle.contains("+1/+1 counter")
        || oracle.contains("counter on")
        || oracle.contains("counters on")
    {
        add("Counters");
    }

    if oracle.contains("proliferate") {
        add("Proliferate");
        add("Counters");
    }

    if oracle.contains("graveyard")
        || oracle.contains("mill ")
        || oracle.contains("surveil")
    {
        add("Graveyard");
    }

    if (oracle.contains("return target")
        || oracle.contains("return a ")
        || oracle.contains("put target"))
        && oracle.contains("graveyard")
        && (oracle.contains("battlefield")
            || oracle.contains("onto the battlefield"))
    {
        add("Reanimator");
        add("Graveyard");
    }

    if oracle.contains("landfall")
        || oracle.contains("whenever a land enters")
        || oracle.contains("whenever one or more lands enter")
    {
        add("Landfall");
        add("Lands");
    }

    if oracle.contains("play an additional land")
        || oracle.contains("lands from your graveyard")
        || oracle.contains("land card from your graveyard")
        || oracle.contains("land cards from your graveyard")
    {
        add("Lands");
    }

    if oracle.contains("create")
        && (oracle.contains("creature token")
            || oracle.contains("creature tokens"))
    {
        add("Tokens");
    }

    if oracle.contains("treasure token")
        || oracle.contains("treasure tokens")
    {
        add("Treasure");
    }

    if oracle.contains("draw") && oracle.contains("discard") {
        add("Wheels");
    }

    if oracle.contains("copy target instant")
        || oracle.contains("copy target sorcery")
        || oracle.contains("instant or sorcery")
        || oracle.contains("whenever you cast an instant")
        || oracle.contains("whenever you cast a sorcery")
        || oracle.contains("noncreature spell")
    {
        add("Spellslinger");
    }

    if oracle.contains("storm")
        || oracle.contains("spells you've cast this turn")
        || oracle.contains("whenever you cast a spell")
    {
        add("Storm");
    }

    if oracle.contains("each opponent can't")
        || oracle.contains("players can't")
        || oracle.contains("spells your opponents cast cost")
        || oracle.contains("permanents your opponents control enter")
        || oracle.contains("doesn't untap")
    {
        add("Stax");
    }

    if oracle.contains("gain life")
        || oracle.contains("gains life")
        || oracle.contains("whenever you gain life")
    {
        add("Lifegain");
    }

    if oracle.contains("mill ")
        || oracle.contains("puts the top")
            && oracle.contains("library")
            && oracle.contains("graveyard")
    {
        add("Mill");
    }

    if type_line.contains("legendary")
        || oracle.contains("legendary permanent")
        || oracle.contains("legendary spell")
    {
        add("Legends");
    }

    if oracle.contains("mana value")
        && (oracle.contains("greater")
            || oracle.contains("equal to")
            || oracle.contains("x is"))
    {
        add("Big Mana");
    }

    if card.cmc >= 7.0 {
        add("Big Mana");
    }

    if oracle.contains("creature type")
        || oracle.contains("creatures you control of the chosen type")
        || oracle.contains("choose a creature type")
    {
        add("Typal");
    }

    if oracle.contains("exile")
        && oracle.contains("return")
        && oracle.contains("battlefield")
    {
        add("Blink");
    }

    if name.contains("panharmonicon")
        || oracle.contains("enters the battlefield causes")
        || oracle.contains("enters,")
    {
        if type_line.contains("creature") || oracle.contains("creature") {
            add("Blink");
        }
    }

    tags
}
