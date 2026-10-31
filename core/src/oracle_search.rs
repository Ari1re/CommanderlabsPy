//! Deep card search: match a play style ("-1/-1 Counters", "Elves",
//! "Landfall") against every card's Oracle text and type line, so older or
//! rarely played cards that do the job are found even when no decklist runs
//! them.

use crate::{
    analysis::{classify_card, identity_subset},
    data::{Card, CardDb},
    format::Format,
};
use anyhow::Result;
use regex::Regex;
use serde::Serialize;

/// One way a card can fit a theme.
struct Rule {
    /// Matched against `type_line` + `oracle_text` + keywords, lowercased.
    re: Regex,
    /// What the card does, for the reason shown to the user.
    label: &'static str,
    /// Payoffs (cards that reward the theme) weigh more than enablers.
    weight: i32,
}

/// Compiled matcher for one theme.
pub struct ThemeMatcher {
    pub theme: String,
    rules: Vec<Rule>,
    /// Typal themes also match on the creature type line.
    typal: Option<(String, String)>,
}

/// A card deep search turned up for a theme.
#[derive(Debug, Clone, Serialize)]
pub struct DeepFind {
    pub name: String,
    pub theme: String,
    pub reason: String,
    pub score: i32,
    /// EDHREC rank, if any; finds outside the usual card pool have none or a
    /// very high rank.
    pub edhrec_rank: Option<u32>,
}

fn r(pattern: &str, label: &'static str, weight: i32) -> Rule {
    Rule { re: Regex::new(&format!("(?i){pattern}")).expect("theme pattern"), label, weight }
}

/// Plural → singular for creature types (Elves → Elf, Zombies → Zombie).
fn singular(word: &str) -> String {
    let w = word.trim();
    let lower = w.to_ascii_lowercase();
    let irregular = [
        ("elves", "elf"), ("dwarves", "dwarf"), ("wolves", "wolf"), ("werewolves", "werewolf"),
        ("merfolk", "merfolk"), ("faeries", "faerie"), ("sphinxes", "sphinx"), ("fungi", "fungus"),
        ("octopi", "octopus"), ("djinn", "djinn"), ("allies", "ally"), ("mice", "mouse"),
        ("foxes", "fox"), ("leeches", "leech"), ("witches", "witch"), ("sheep", "sheep"),
        ("pegasi", "pegasus"), ("cyclopes", "cyclops"), ("thalids", "thallid"),
    ];
    if let Some((_, s)) = irregular.iter().find(|(p, _)| *p == lower) {
        return s.to_string();
    }
    if let Some(stem) = lower.strip_suffix('s') {
        if CREATURE_TYPES.contains(&stem) {
            return stem.to_string();
        }
    }
    if let Some(stem) = lower.strip_suffix("ies") {
        return format!("{stem}y");
    }
    if lower.ends_with("ss") {
        return lower;
    }
    lower.strip_suffix('s').map(str::to_string).unwrap_or(lower)
}

/// Creature types common enough to be EDHREC themes. A theme that is one of
/// these (in either number) is treated as typal.
const CREATURE_TYPES: &[&str] = &[
    "angel", "ape", "artificer", "assassin", "avatar", "bat", "bear", "beast", "bird", "cat",
    "cleric", "construct", "demon", "devil", "dinosaur", "djinn", "dog", "dragon", "drake",
    "druid", "dwarf", "eldrazi", "elemental", "elephant", "elf", "faerie", "fish", "fox", "frog",
    "fungus", "giant", "gnome", "goblin", "golem", "gorgon", "horror", "human", "hydra",
    "illusion", "insect", "kavu", "knight", "kor", "kraken", "leviathan", "lizard", "merfolk",
    "minotaur", "monk", "mouse", "mutant", "myr", "ninja", "noble", "octopus", "ogre", "ooze",
    "orc", "otter", "pegasus", "phoenix", "phyrexian", "pirate", "plant", "rabbit", "rat", "rogue",
    "samurai", "saproling", "scarecrow", "serpent", "shade", "shaman", "shapeshifter", "skeleton",
    "sliver", "snake", "soldier", "sphinx", "spider", "spirit", "squirrel", "thopter", "treefolk",
    "troll", "turtle", "unicorn", "vampire", "vedalken", "warlock", "warrior", "werewolf",
    "wizard", "wolf", "wurm", "zombie", "cyclops", "dryad", "satyr", "centaur", "nightmare",
    "hound", "salamander", "crab", "badger", "raccoon", "lemur", "mole", "bard", "detective",
];

impl ThemeMatcher {
    pub fn new(theme: &str) -> Self {
        let name = theme.trim().to_string();
        let key = name
            .to_ascii_lowercase()
            .replace(" typal", "")
            .replace(" tribal", "")
            .replace("typal ", "")
            .replace("tribal ", "");
        let key = key.trim();

        let rules: Vec<Rule> = match key {
            "+1/+1 counters" | "+1/+1 counter" | "counters" if key != "counters" => vec![
                r(r"\+1/\+1 counters? on", "puts +1/+1 counters on creatures", 3),
                r(r"(with|has|had) (a|one or more) \+1/\+1 counters?", "rewards creatures with +1/+1 counters", 4),
                r(r"proliferate", "proliferates", 2),
                r(r"\+1/\+1 counters?", "uses +1/+1 counters", 2),
            ],
            "-1/-1 counters" | "-1/-1 counter" | "minus counters" => vec![
                r(r"-1/-1 counters? on", "puts -1/-1 counters on creatures", 3),
                r(r"\bwither\b|\binfect\b", "deals damage as -1/-1 counters (wither/infect)", 3),
                r(r"\bpersist\b", "returns with a -1/-1 counter (persist)", 2),
                r(r"-1/-1 counters? (is|are) (put|placed)|whenever .*-1/-1 counter", "rewards -1/-1 counters being placed", 4),
                r(r"proliferate", "proliferates", 2),
                r(r"-1/-1 counters?", "uses -1/-1 counters", 2),
            ],
            "counters" | "proliferate" | "counters matter" => vec![
                r(r"proliferate", "proliferates", 3),
                r(r"counters? on (it|each|target|that|a|another)", "puts counters on permanents", 2),
                r(r"for each (kind of )?counter", "scales with counters", 3),
                r(r"double the number of .*counters", "doubles counters", 4),
            ],
            "tokens" | "go wide" | "token" => vec![
                r(r"create[s]? .*\btokens?\b", "creates tokens", 2),
                r(r"whenever (one or more )?(a )?(creature )?tokens?.*(enter|you create)", "rewards making tokens", 4),
                r(r"creatures you control get \+\d", "anthem for a wide board", 3),
                r(r"if .* would create .* tokens?, .* twice that many", "doubles tokens", 4),
            ],
            "treasure" | "treasures" => vec![
                r(r"treasure", "makes or uses Treasure", 3),
                r(r"whenever you sacrifice an artifact", "rewards sacrificing artifacts", 2),
            ],
            "clues" | "investigate" => vec![r(r"\bclues?\b|investigate", "makes or uses Clues", 3)],
            "food" => vec![r(r"\bfood\b", "makes or uses Food", 3)],
            "blood" | "blood tokens" => vec![r(r"\bblood token", "makes or uses Blood", 3)],
            "aristocrats" | "sacrifice" | "sac" | "death triggers" => vec![
                r(r"sacrifice (a|another|an|two|x) ", "is a sacrifice outlet or edict", 2),
                r(r"whenever (a|another|one or more) (nontoken )?creatures? (you control )?dies", "pays off creatures dying", 4),
                r(r"whenever you sacrifice", "pays off sacrificing", 4),
                r(r"(each opponent|target player|target opponent) loses \d+ life", "drains", 2),
            ],
            "lifegain" | "life gain" | "lifelink" => vec![
                r(r"whenever you gain life", "pays off gaining life", 4),
                r(r"\blifelink\b", "has lifelink", 2),
                r(r"you gain \d+ life|gain life equal|gains? (x|that much) life", "gains life", 2),
                r(r"if you would gain life", "boosts life gain", 4),
            ],
            "landfall" | "lands matter" | "lands" => vec![
                r(r"\blandfall\b|whenever a land (you control )?enters|whenever one or more lands (you control )?enter", "has landfall", 4),
                r(r"play an additional land|play lands from|lands? cards? from your graveyard", "lets you play extra lands", 3),
                r(r"search your library for (a|up to \w+) (basic )?lands? cards?", "fetches lands", 1),
                r(r"sacrifice a land", "sacrifices lands for value", 2),
            ],
            "reanimator" | "reanimation" => vec![
                r(r"(return|put) .*creature cards? .*from (a|your|any) graveyards? (to|onto) the battlefield", "reanimates creatures", 4),
                r(r"\b(unearth|persist|undying|encore)\b", "comes back from the graveyard", 2),
                r(r"(mill|discard) .*(card|cards)", "fills the graveyard", 1),
            ],
            "graveyard" | "self-mill" | "self mill" | "graveyard matters" => vec![
                r(r"\b(dredge|flashback|delve|escape|unearth|embalm|eternalize|disturb|jump-start|retrace|aftermath|scavenge)\b", "plays from the graveyard", 3),
                r(r"cards? in your graveyard", "scales with your graveyard", 3),
                r(r"\bmills?\b|\bsurveil\b", "fills your graveyard", 2),
                r(r"from your graveyard", "uses your graveyard", 2),
            ],
            "mill" => vec![
                r(r"(target|each) (player|opponent) mills|mills? (\d+|x|that many|half)", "mills", 3),
                r(r"put the top .* cards? of (their|his or her|target player's) library into .* graveyard", "mills (old wording)", 3),
                r(r"whenever .* cards? (is|are) put into an opponent's graveyard", "pays off milling", 4),
            ],
            "spellslinger" | "spells matter" | "instants and sorceries" | "prowess" | "magecraft" => vec![
                r(r"whenever you cast (an instant|a sorcery|an instant or sorcery|a noncreature)", "pays off casting spells", 4),
                r(r"\b(prowess|magecraft)\b", "has prowess or magecraft", 3),
                r(r"instant (and|or) sorcery spells you cast cost", "cheapens spells", 4),
                r(r"copy target (instant|sorcery)", "copies spells", 3),
            ],
            "storm" => vec![
                r(r"\bstorm\b", "has storm", 4),
                r(r"spells? (you've|you have) cast this turn|for each (other )?spell cast", "counts spells cast this turn", 3),
                r(r"add \{[wubrgc]\}\{[wubrgc]\}\{[wubrgc]\}|untap (all|up to)", "makes lots of mana", 1),
            ],
            "artifacts" | "artifact" => vec![
                r(r"whenever (an|one or more) artifacts? (you control )?(enters|is put)", "pays off artifacts entering", 4),
                r(r"for each artifact you control|artifacts you control (get|have)", "scales with artifacts", 3),
                r(r"\bimprovise\b|\baffinity for artifacts\b|\bmetalcraft\b", "artifact mechanic", 3),
                r(r"^artifact|\bartifact (creature|land)\b", "is an artifact", 1),
            ],
            "equipment" | "voltron" | "equip" => vec![
                r(r"equipped creature|\bequip\b", "is or helps Equipment", 3),
                r(r"enchanted creature gets \+", "Aura that grows a creature", 2),
                r(r"attach (target|an|all)|equip abilities you activate cost", "moves or cheapens Equipment", 4),
                r(r"\b(hexproof|shroud|indestructible)\b", "protects a big threat", 1),
            ],
            "auras" | "aura" | "enchantress" | "enchantments" | "enchantment" => vec![
                r(r"whenever you cast an (aura|enchantment)|whenever an (aura|enchantment) (you control )?enters", "pays off enchantments", 4),
                r(r"enchanted creature", "is an Aura", 2),
                r(r"for each (aura|enchantment) you control", "scales with enchantments", 3),
                r(r"\bconstellation\b", "has constellation", 4),
            ],
            "blink" | "flicker" | "etb" | "enter the battlefield" => vec![
                r(r"exile (another |up to \w+ )?(target |other )?(creatures?|permanents?|nonland permanents?)[^.]*(then )?return (it|them|that card|those cards)", "blinks permanents", 4),
                r(r"when(ever)? (this creature|[^.]{0,40}) enters", "has an enter-the-battlefield effect", 1),
                r(r"triggers an additional time", "doubles enter triggers", 4),
            ],
            "wheels" | "wheel" => vec![
                r(r"each player (discards (their|his or her) hand|shuffles (their|his or her) hand).*draws", "wheels", 4),
                r(r"whenever an opponent draws a card", "punishes opponents drawing", 4),
                r(r"draw (seven|\w+) cards", "refills your hand", 1),
            ],
            "stax" | "prison" => vec![
                r(r"players can't|can't cast more than one|each player can't", "restricts what players can do", 3),
                r(r"(spells|abilities) (your opponents cast )?cost \{\d\} more", "taxes spells", 3),
                r(r"doesn't untap|don't untap|skip (their|his or her) untap", "stops untapping", 3),
                r(r"enters? (the battlefield )?tapped", "makes things enter tapped", 2),
                r(r"can't search|activated abilities .* can't be activated", "shuts off abilities", 3),
            ],
            "infect" | "poison" | "toxic" => vec![
                r(r"\binfect\b|\btoxic\b|\bpoison counters?\b", "deals poison", 4),
                r(r"proliferate", "proliferates", 3),
            ],
            "flying" | "fliers" | "flyers" => vec![
                r(r"\bflying\b", "flies", 2),
                r(r"creatures? (you control )?with flying (get|have)", "pays off fliers", 4),
            ],
            "burn" | "direct damage" => vec![
                r(r"deals? \d+ damage to (any target|each opponent|target player|target opponent|each player)", "burns", 3),
                r(r"if a source you control would deal damage", "doubles damage", 4),
            ],
            "extra turns" | "extra turn" => vec![r(r"extra turn", "takes extra turns", 4)],
            "extra combat" | "extra combats" => vec![r(r"additional combat phase", "gives extra combats", 4)],
            "group hug" => vec![
                r(r"each player (draws|may draw|may put|gains|adds)", "helps every player", 3),
            ],
            "politics" | "monarch" | "goad" => vec![
                r(r"\bgoad\b|\bmonarch\b|\bvote\b|\binitiative\b", "political mechanic", 3),
                r(r"each opponent may", "offers opponents a choice", 2),
            ],
            "superfriends" | "planeswalkers" | "planeswalker" => vec![
                r(r"^legendary planeswalker|planeswalker", "planeswalker", 2),
                r(r"loyalty (abilities|counters?)", "boosts loyalty", 4),
                r(r"\bproliferate\b", "proliferates", 2),
            ],
            "discard" => vec![
                r(r"(target|each) (player|opponent) discards", "makes opponents discard", 3),
                r(r"whenever (an opponent|a player) discards", "pays off discard", 4),
                r(r"\bmadness\b", "has madness", 3),
            ],
            "theft" | "steal" | "control magic" => vec![
                r(r"gain control of", "steals permanents", 4),
                r(r"you may cast spells from among|cards? exiled with", "plays opponents' cards", 3),
            ],
            "clones" | "copy" | "clone" => vec![
                r(r"(enter|enters) (the battlefield )?as a copy|becomes a copy|create a token that's a copy", "copies permanents", 4),
            ],
            "spell copy" => vec![r(r"copy (target|that|it|each) (instant|sorcery|spell)", "copies spells", 4)],
            "vehicles" | "vehicle" => vec![
                r(r"\bcrew\b|vehicle", "is or helps Vehicles", 3),
            ],
            "energy" => vec![r(r"\{e\}|energy counters?", "makes or spends energy", 3)],
            "snow" => vec![r(r"\bsnow\b", "snow", 3)],
            "sagas" | "saga" => vec![r(r"\bsaga\b|lore counters?", "Sagas", 3)],
            "cycling" => vec![r(r"\bcycling\b|whenever you cycle", "cycling", 3)],
            "big mana" | "ramp" => vec![
                r(r"add \{[wubrgc]\}\{[wubrgc]\}|add (two|three) mana|double the amount of mana", "makes extra mana", 3),
                r(r"search your library for (a|up to \w+) (basic )?lands? cards?.*battlefield", "ramps", 2),
            ],
            "x spells" | "x" => vec![r(r"\{x\}", "has X in its cost", 3)],
            "chaos" => vec![r(r"\brandom\b|flip a coin|roll (a|\w+) d\d+", "random effects", 3)],
            "pillowfort" => vec![
                r(r"can't attack you( or planeswalkers you control)? unless|creatures can't attack you", "discourages attacks", 4),
                r(r"prevent all (combat )?damage", "prevents damage", 3),
            ],
            "legends" | "legendary" | "historic" => vec![
                r(r"legendary (spell|permanent|creature)s?|\bhistoric\b", "rewards legends", 4),
                r(r"^legendary", "is legendary", 1),
            ],
            "dungeon" | "venture" | "initiative" => vec![r(r"venture into the dungeon|\binitiative\b|dungeon", "ventures", 4)],
            "outlaws" => vec![r(r"\boutlaws?\b|assassin|mercenary|pirate|rogue|warlock", "outlaws", 3)],
            "party" => vec![r(r"\bparty\b|cleric|rogue|warrior|wizard", "party", 2)],
            "ninjutsu" | "ninjas" | "ninja" => vec![r(r"\bninjutsu\b|ninja", "ninjutsu", 4), r(r"can't be blocked", "evasive attacker", 2)],
            _ => Vec::new(),
        };

        let mut matcher = ThemeMatcher { theme: name.clone(), rules, typal: None };

        if matcher.rules.is_empty() {
            // Typal theme?
            let first = key.split_whitespace().next().unwrap_or(key);
            let one = singular(first);
            if CREATURE_TYPES.contains(&one.as_str()) {
                let plural = first.to_ascii_lowercase();
                matcher.typal = Some((one.clone(), plural.clone()));
                matcher.rules = vec![
                    r(&format!(r"\b({one}|{plural})s? (you control|creatures?|spells?|cards?)\b"), "pays off the creature type", 4),
                    r(&format!(r"(other|each|all|target) {one}s?\b|\b{plural}\b"), "mentions the creature type", 3),
                    r(r"choose a creature type|creatures? of the chosen type|changeling|all creature types", "works with any creature type", 2),
                ];
            } else {
                // Unknown theme: match its words in rules text and types.
                let words: Vec<String> = key
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '+' && c != '/' && c != '-')
                    .filter(|w| w.len() >= 4)
                    .map(|w| regex::escape(&singular(w)))
                    .collect();
                if !words.is_empty() {
                    let pattern = format!(r"\b({})(s|es)?\b", words.join("|"));
                    matcher.rules.push(Rule {
                        re: Regex::new(&format!("(?i){pattern}")).expect("word pattern"),
                        label: "mentions the theme",
                        weight: 2,
                    });
                }
            }
        }
        matcher
    }

    /// How strongly a card fits (0 = not at all), and the best reason.
    pub fn score(&self, card: &Card) -> (i32, Option<&'static str>) {
        let text = format!(
            "{}\n{}\n{}",
            card.type_line.to_ascii_lowercase(),
            card.oracle_text.to_ascii_lowercase(),
            card.keywords.to_ascii_lowercase()
        );
        let mut score = 0;
        let mut best: Option<(&'static str, i32)> = None;
        for rule in &self.rules {
            if rule.re.is_match(&text) {
                score += rule.weight;
                if best.map(|(_, w)| rule.weight > w).unwrap_or(true) {
                    best = Some((rule.label, rule.weight));
                }
            }
        }
        if let Some((one, _)) = &self.typal {
            // The creature's own subtypes.
            let ty = card.type_line.to_ascii_lowercase();
            if let Some((_, subtypes)) = ty.split_once('—') {
                if subtypes.split_whitespace().any(|t| t == one) {
                    score += 3;
                    if best.map(|(_, w)| w < 3).unwrap_or(true) {
                        best = Some(("is the creature type", 3));
                    }
                }
            }
        }
        (score, best.map(|(label, _)| label))
    }

    pub fn matches(&self, card: &Card) -> bool {
        self.score(card).0 >= 2
    }
}

/// Search the whole card database for cards that fit `themes`, inside the
/// color identity and legal in `format`. Returns the strongest fits first,
/// favouring cards outside the usual popular pool so the search adds
/// something the normal sources would miss.
pub fn deep_search(
    db: &CardDb,
    themes: &[String],
    identity: Option<&str>,
    format: Format,
    limit: usize,
) -> Result<Vec<DeepFind>> {
    let matchers: Vec<ThemeMatcher> = themes.iter().map(|t| ThemeMatcher::new(t)).collect();
    if matchers.iter().all(|m| m.rules.is_empty()) {
        return Ok(Vec::new());
    }

    let mut finds = Vec::<DeepFind>::new();
    db.scan_cards(|card| {
        if card.is_basic || card.type_line.to_ascii_lowercase().contains("land") && !lands_theme(themes) {
            return;
        }
        if format.card_allowed(&card).is_err() {
            return;
        }
        if let Some(identity) = identity {
            if !identity_subset(&card.color_identity, identity) {
                return;
            }
        }
        let mut best: Option<(i32, &str, &'static str)> = None;
        for m in &matchers {
            let (s, why) = m.score(&card);
            if s >= 2 && best.map(|(b, _, _)| s > b).unwrap_or(true) {
                best = Some((s, &m.theme, why.unwrap_or("fits the theme")));
            }
        }
        let Some((fit, theme, why)) = best else { return };

        // Rank: theme fit first, then whether the card is a real card
        // (efficient, does something structural), then obscurity.
        let roles = classify_card(&card);
        let mut score = fit * 20;
        score += match card.cmc as i32 {
            0..=2 => 12,
            3 => 8,
            4 => 4,
            5 => 0,
            _ => -8,
        };
        if roles.draw || roles.removal || roles.ramp || roles.tutor || roles.protection {
            score += 6;
        }
        let obscure = card.edhrec_rank.map(|r| r > 15_000).unwrap_or(true);
        if obscure {
            score += 10;
        }
        finds.push(DeepFind {
            name: card.name.clone(),
            theme: theme.to_string(),
            reason: if obscure {
                format!("{} ({}); rarely seen in decklists", capitalize(why), theme)
            } else {
                format!("{} ({})", capitalize(why), theme)
            },
            score,
            edhrec_rank: card.edhrec_rank,
        });
    })?;

    finds.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    finds.truncate(limit);
    Ok(finds)
}

fn lands_theme(themes: &[String]) -> bool {
    themes.iter().any(|t| {
        let t = t.to_ascii_lowercase();
        t.contains("land")
    })
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, ty: &str, text: &str) -> Card {
        Card {
            oracle_id: name.into(),
            name: name.into(),
            mana_cost: String::new(),
            cmc: 3.0,
            type_line: ty.into(),
            oracle_text: text.into(),
            color_identity: "B".into(),
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
    fn old_minus_counter_cards_match() {
        let m = ThemeMatcher::new("-1/-1 Counters");
        // Old wording, never seen in a modern list, still matches.
        let old = card("Blight Sickle", "Artifact — Equipment", "Equipped creature gets +1/+0 and has wither.");
        assert!(m.matches(&old));
        let direct = card("Grim Poppet", "Artifact Creature — Scarecrow", "Remove a -1/-1 counter from Grim Poppet: Put a -1/-1 counter on another target creature.");
        assert!(m.score(&direct).0 > m.score(&old).0 - 10);
        assert!(!m.matches(&card("Grizzly Bears", "Creature — Bear", "")));
        // +1/+1 matcher must not grab -1/-1 cards.
        let plus = ThemeMatcher::new("+1/+1 Counters");
        assert!(!plus.matches(&direct));
    }

    #[test]
    fn typal_themes_match_types_and_text() {
        let m = ThemeMatcher::new("Elves");
        assert!(m.matches(&card("Llanowar Elves", "Creature — Elf Druid", "{T}: Add {G}.")));
        assert!(m.matches(&card("Elvish Promenade", "Tribal Sorcery — Elf", "Create a 1/1 green Elf Warrior creature token for each Elf you control.")));
        assert!(!m.matches(&card("Grizzly Bears", "Creature — Bear", "")));
        assert_eq!(singular("Wolves"), "wolf");
        assert_eq!(singular("Zombies"), "zombie");
    }

    #[test]
    fn unknown_themes_fall_back_to_words() {
        let m = ThemeMatcher::new("Exploit");
        assert!(m.matches(&card("Silumgar Butcher", "Creature — Zombie Djinn", "Exploit (When this creature enters, you may sacrifice a creature.)")));
    }
}
