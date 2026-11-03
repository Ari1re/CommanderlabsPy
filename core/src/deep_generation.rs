//! Deep learning generation: the evidence model behind the default deck
//! generator.
//!
//! Peer-based generation mostly copies what other players run. Deep learning
//! generation also reads the commander's own rules text for its mechanics,
//! sorts every candidate card into evidence tiers (commonly played, staples,
//! synergy, could use), scores cards by synergy rather than raw popularity,
//! and writes a report of every source it used and every pick it made. It
//! needs no AI model; an AI model's suggestions are one more source when on.
//!
//! Things to work on:
//! - The tier bonuses and the power-staple quota are tuned on one commander
//!   (Drana). Build a small benchmark (10 commanders, each format) and
//!   compare engines on it before re-tuning.
//! - Game Changers are still under-used at Bracket 3+ and big single threats
//!   get dropped for synergy cards; the quota only covers structural staples.
//! - The report lists card reasons but not the numeric score; adding the
//!   score breakdown would make tuning far easier.

use crate::{
    analysis::CardRoles,
    data::Card,
    edhrec::EdhrecCardSignal,
    oracle_search::ThemeMatcher,
};
use serde::{Deserialize, Serialize};

/// Which generator builds the deck (Settings → Deck generation).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationEngine {
    /// Evidence tiers, the commander's own mechanics, synergy-first scoring.
    #[default]
    DeepLearning,
    /// The original generator: what peer decklists and EDHREC run.
    PeerBased,
}

impl GenerationEngine {
    /// Read the Settings value ("peer" = peer-based; anything else, including
    /// nothing, = deep learning).
    pub fn parse(value: &str) -> Self {
        match value {
            "peer" | "peer_based" => Self::PeerBased,
            _ => Self::DeepLearning,
        }
    }

    /// Name shown in Settings and the report.
    pub fn label(self) -> &'static str {
        match self {
            Self::DeepLearning => "Deep learning generation",
            Self::PeerBased => "Peer-based generation",
        }
    }
}

/// Play styles a commander's own rules text can point to. Each is a theme
/// [`ThemeMatcher`] knows, so cards are matched by the same rules.
const MECHANIC_THEMES: &[&str] = &[
    "+1/+1 Counters", "-1/-1 Counters", "Tokens", "Treasure", "Aristocrats", "Lifegain", "Landfall",
    "Reanimator", "Graveyard", "Mill", "Spellslinger", "Storm", "Artifacts", "Equipment", "Enchantress",
    "Blink", "Wheels", "Stax", "Infect", "Burn", "Extra Combat", "Group Hug", "Politics", "Superfriends",
    "Discard", "Theft", "Clones", "Vehicles", "Energy", "Sagas", "Cycling", "Big Mana", "X Spells",
    "Pillowfort", "Legends", "Ninjutsu",
];

/// The play styles the commanders' own rules text asks for, strongest
/// first. Unlike EDHREC themes, this works for any commander, including new
/// or unpopular ones with no decklists.
pub fn commander_mechanics(commanders: &[Card]) -> Vec<String> {
    let mut found: Vec<(i32, String)> = Vec::new();
    // Rules text only: every commander's type line says "Legendary
    // Creature", which isn't a play style.
    let texts: Vec<Card> = commanders.iter().map(|c| Card { type_line: String::new(), ..c.clone() }).collect();
    for theme in MECHANIC_THEMES {
        let matcher = ThemeMatcher::new(theme);
        let score: i32 = texts.iter().map(|c| matcher.score(c).0).sum();
        // One strong rule (weight 3+) or two weaker ones; a lone "has
        // flying" doesn't make a theme.
        if score >= 3 {
            found.push((score, theme.to_string()));
        }
    }
    // Typal: the commander's creature type named in its own text
    // ("other Vampires you control").
    for commander in commanders {
        let ty = commander.type_line.to_ascii_lowercase();
        let text = commander.oracle_text.to_ascii_lowercase();
        if let Some((_, subtypes)) = ty.split_once('—') {
            for sub in subtypes.split_whitespace() {
                if sub.len() >= 3 && text.contains(sub) {
                    found.push((5, format!("{}s", capitalize(sub))));
                }
            }
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let mut out: Vec<String> = Vec::new();
    for (_, name) in found {
        if !out.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
            out.push(name);
        }
    }
    out.truncate(4);
    out
}

/// "vampire" → "Vampire".
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// How strongly the evidence backs a card, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Fits this commander specifically: EDHREC synergy, its mechanics, or a
    /// strong rules-text match. The most telling evidence, so it's listed
    /// first when a card has several.
    Synergy,
    /// Played by many peers: high EDHREC inclusion or in many reference decks.
    Core,
    /// A proven card for its role in these colors (ramp, draw, removal…).
    Staple,
    /// Found by rules text, keywords or tags; little play evidence.
    CouldUse,
}

impl Tier {
    /// Name shown in the report and on the Sources tab.
    pub fn label(self) -> &'static str {
        match self {
            Self::Core => "Commonly played",
            Self::Synergy => "Synergy",
            Self::Staple => "Staple",
            Self::CouldUse => "Could use",
        }
    }
}

/// Everything known about one candidate card.
pub struct Evidence<'a> {
    pub edhrec: Option<&'a EdhrecCardSignal>,
    /// Reference decks running the card, and how many were read.
    pub reference_hits: u32,
    pub reference_decks: usize,
    /// Score from the rules-text search, if it found the card.
    pub deep_find: Option<i32>,
    /// Matches one of the commander's own mechanics.
    pub mechanic: bool,
    /// Matches a chosen or suggested theme.
    pub theme: bool,
    /// Named by the AI model as a key card.
    pub ai_key: bool,
    pub roles: CardRoles,
    /// Target bracket: stronger decks lean harder on proven staples.
    pub bracket: u8,
    /// Where the inclusion/synergy numbers come from ("EDHREC", or
    /// "PDHREC" for Pauper Commander).
    pub card_data: &'static str,
}

/// A card's tier, its score bonus and why, from its evidence.
pub fn assess(card: &Card, ev: &Evidence<'_>) -> (Option<Tier>, i32, Vec<String>) {
    let mut reasons = Vec::new();
    let inclusion = ev.edhrec.map(|s| s.inclusion_pct()).unwrap_or(0.0);
    let synergy = ev.edhrec.map(|s| s.synergy).unwrap_or(0.0);
    let ref_share = if ev.reference_decks >= 5 { ev.reference_hits as f64 / ev.reference_decks as f64 } else { 0.0 };
    let structural = ev.roles.ramp || ev.roles.draw || ev.roles.removal || ev.roles.wipe || ev.roles.tutor || ev.roles.protection;

    let mut tiers = Vec::new();
    if synergy >= 0.15 || ev.mechanic || ev.deep_find.map(|s| s >= 90).unwrap_or(false) {
        tiers.push(Tier::Synergy);
    }
    if inclusion >= 0.30 || ref_share >= 0.35 {
        tiers.push(Tier::Core);
    }
    if structural && card.edhrec_rank.map(|r| r <= 1_500).unwrap_or(false) {
        tiers.push(Tier::Staple);
    }
    if ev.theme || ev.deep_find.is_some() || ev.ai_key {
        tiers.push(Tier::CouldUse);
    }

    if inclusion > 0.0 {
        reasons.push(format!("{}: in {:.0}% of decks{}", ev.card_data, inclusion * 100.0, if synergy > 0.0 { format!(", +{:.0}% synergy", synergy * 100.0) } else { String::new() }));
    }
    if ev.reference_hits > 0 {
        reasons.push(format!("in {}/{} reference decks", ev.reference_hits, ev.reference_decks));
    }
    if ev.mechanic {
        reasons.push("fits the commander's own mechanics".into());
    }
    if ev.theme {
        reasons.push("fits the deck's play styles".into());
    }
    if ev.deep_find.is_some() {
        reasons.push("found by rules-text search".into());
    }
    if ev.ai_key {
        reasons.push("suggested by the AI model".into());
    }
    if tiers.contains(&Tier::Staple) {
        reasons.push("a staple for its role".into());
    }

    // Synergy outweighs raw popularity: a card everyone plays in every deck
    // scores below one that's specifically good here.
    let mut bonus = 0;
    for tier in &tiers {
        bonus += match tier {
            Tier::Core => 50 + (inclusion * 60.0) as i32,
            Tier::Synergy => 50 + (synergy.clamp(0.0, 1.0) * 100.0) as i32,
            // Bracket 1: 50, bracket 3: 80, bracket 5: 110. Power comes
            // from staples (fast mana, tutors, efficient removal).
            Tier::Staple => 35 + 15 * ev.bracket.clamp(1, 5) as i32,
            Tier::CouldUse => 15,
        };
    }
    // Several independent kinds of evidence agreeing is worth more than any
    // one alone.
    if tiers.len() >= 3 {
        bonus += 20;
    }

    // Power: how widely the card is played across all of Commander, weighted
    // by bracket. Bracket 1 is about the theme; by bracket 4–5 the strongest
    // cards win slots over cute synergy.
    let power = match card.edhrec_rank {
        Some(r) if r <= 100 => 100,
        Some(r) if r <= 500 => 80,
        Some(r) if r <= 1_500 => 60,
        Some(r) if r <= 5_000 => 35,
        _ => 0,
    };
    let bracket = ev.bracket.clamp(1, 5) as i32;
    bonus += power * (bracket - 1) / 3;
    // Game Changers are the format's most powerful cards; the bracket check
    // caps how many stay.
    if card.game_changer && bracket >= 3 {
        bonus += 40 + 25 * (bracket - 3);
        reasons.push("a Game Changer (high power)".into());
    }
    (tiers.first().copied(), bonus, reasons)
}

/// What one source contributed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLine {
    pub name: String,
    /// "used", "empty", "failed", "skipped", "not needed" or "off".
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeLine {
    pub name: String,
    /// Where it came from: "you", "AI model", "commander's rules text", "EDHREC".
    pub from: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TierCounts {
    pub core: usize,
    pub synergy: usize,
    pub staple: usize,
    pub could_use: usize,
    pub none: usize,
}

impl TierCounts {
    /// Count one card under its tier (or "none").
    pub fn add(&mut self, tier: Option<Tier>) {
        match tier {
            Some(Tier::Core) => self.core += 1,
            Some(Tier::Synergy) => self.synergy += 1,
            Some(Tier::Staple) => self.staple += 1,
            Some(Tier::CouldUse) => self.could_use += 1,
            None => self.none += 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pick {
    pub card: String,
    pub tier: Option<Tier>,
    pub reasons: Vec<String>,
}

/// Saved with the deck's plan: how the deck was built and from what.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GenerationReport {
    pub engine: String,
    pub sources: Vec<SourceLine>,
    pub themes: Vec<ThemeLine>,
    /// Every legal candidate, by tier.
    pub pool_tiers: TierCounts,
    /// The deck's nonland cards, by tier.
    pub deck_tiers: TierCounts,
    pub picks: Vec<Pick>,
    /// Strong candidates that just missed the cut.
    pub near_misses: Vec<Pick>,
    /// Gaps in the evidence or the deck worth knowing about.
    pub weaknesses: Vec<String>,
}

impl GenerationReport {
    /// The report as Markdown, appended to the deck's `.plan.md`.
    pub fn markdown(&self) -> String {
        let mut out = format!("\n## How this deck was built\n\n_{}_\n\n### Sources\n\n", self.engine);
        for s in &self.sources {
            out.push_str(&format!("- **{}** — {}: {}\n", s.name, s.status, s.detail));
        }
        if !self.themes.is_empty() {
            out.push_str("\n### Play styles\n\n");
            for t in &self.themes {
                out.push_str(&format!("- {} (from {})\n", t.name, t.from));
            }
        }
        let t = &self.deck_tiers;
        out.push_str(&format!(
            "\n### Card evidence\n\nDeck (nonland): {} commonly played · {} synergy · {} staples · {} could use · {} other.\n",
            t.core, t.synergy, t.staple, t.could_use, t.none
        ));
        let p = &self.pool_tiers;
        out.push_str(&format!(
            "Candidates considered: {} commonly played · {} synergy · {} staples · {} could use.\n",
            p.core, p.synergy, p.staple, p.could_use
        ));
        if !self.weaknesses.is_empty() {
            out.push_str("\n### Weak spots\n\n");
            for w in &self.weaknesses {
                out.push_str(&format!("- {w}\n"));
            }
        }
        if !self.picks.is_empty() {
            out.push_str("\n### Why each card\n\n");
            for pick in &self.picks {
                out.push_str(&format!(
                    "- **{}** — {}{}\n",
                    pick.card,
                    pick.tier.map(|t| t.label()).unwrap_or("Filler"),
                    if pick.reasons.is_empty() { String::new() } else { format!(": {}", pick.reasons.join("; ")) }
                ));
            }
        }
        if !self.near_misses.is_empty() {
            out.push_str("\n### Just missed the cut\n\n");
            for pick in &self.near_misses {
                out.push_str(&format!("- {} ({})\n", pick.card, pick.reasons.join("; ")));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: a minimal black card with the given type and text.
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
            commander_eligible: true,
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
            rarity: "rare".into(),
            pdh_legal: String::new(),
            vintage_legal: "legal".into(),
            pdh_commander: false,
        }
    }

    #[test]
    fn drana_reads_as_x_spells() {
        let drana = card(
            "Drana, Kalastria Bloodchief",
            "Legendary Creature — Vampire Shaman",
            "Flying\n{X}{B}{B}: Target creature gets -0/-X until end of turn and Drana, Kalastria Bloodchief gets +X/+0 until end of turn.",
        );
        let found = commander_mechanics(&[drana]);
        assert!(found.iter().any(|t| t == "X Spells"), "{found:?}");
        assert!(!found.iter().any(|t| t == "Flying"));
    }

    #[test]
    fn type_line_isnt_a_theme() {
        let plain = card("Someone", "Legendary Creature — Human", "Flying");
        assert!(!commander_mechanics(&[plain]).iter().any(|t| t == "Legends"));
    }

    #[test]
    fn typal_from_own_text() {
        let edgar = card("Edgar", "Legendary Creature — Vampire Noble", "Other Vampires you control get +1/+1.");
        assert!(commander_mechanics(&[edgar]).iter().any(|t| t == "Vampires"));
    }
}
