use crate::{
    archetype::{detect_deck_tags, tag_fit_score, ArchetypeTag},
    bracket::{estimate_bracket, BracketEstimate},
    commander::pair_compatible,
    data::{Card, CardDb},
    deck::ParsedDeck,
};
use anyhow::Result;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, Default)]
pub struct TypeStats {
    pub lands: u32,
    pub creatures: u32,
    pub artifacts: u32,
    pub enchantments: u32,
    pub instants: u32,
    pub sorceries: u32,
    pub planeswalkers: u32,
    pub battles: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RoleStats {
    pub ramp: u32,
    pub draw: u32,
    pub tutors: u32,
    pub removal: u32,
    pub counters: u32,
    pub wipes: u32,
    pub recursion: u32,
    pub protection: u32,
    pub graveyard: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CardRoles {
    pub ramp: bool,
    pub draw: bool,
    pub tutor: bool,
    pub removal: bool,
    pub counter: bool,
    pub wipe: bool,
    pub recursion: bool,
    pub protection: bool,
    pub graveyard: bool,
}

impl CardRoles {
    pub fn count(self) -> u32 {
        [
            self.ramp,
            self.draw,
            self.tutor,
            self.removal,
            self.counter,
            self.wipe,
            self.recursion,
            self.protection,
            self.graveyard,
        ]
        .into_iter()
        .filter(|x| *x)
        .count() as u32
    }
}

#[derive(Debug, Clone)]
pub struct CutCandidate {
    pub card: String,
    pub score: i32,
    pub tag_fit: i32,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct AddSuggestion {
    pub card: String,
    pub role: String,
    pub edhrec_rank: Option<u32>,
    pub tag_fit: i32,
}

#[derive(Debug, Clone)]
pub struct DeckAnalysis {
    pub deck_name: String,
    pub commanders: Vec<String>,
    pub identity: String,
    pub total_cards: u32,
    pub sideboard_count: u32,
    pub known_cards: u32,
    pub unknown_cards: Vec<String>,
    pub legal: bool,
    pub legality_issues: Vec<String>,
    pub types: TypeStats,
    pub roles: RoleStats,
    pub avg_cmc: f64,
    pub curve: [u64; 8],
    pub pip_demand: [u64; 5],
    pub direct_sources: [u64; 5],
    pub fetch_lands: u32,
    pub recommendations: Vec<String>,
    pub cut_candidates: Vec<CutCandidate>,
    pub add_suggestions: Vec<AddSuggestion>,
    pub tags: Vec<ArchetypeTag>,
    pub bracket: BracketEstimate,
    pub total_price_usd: f64,
    pub total_price_eur: f64,
    pub total_price_tix: f64,
    pub sideboard_price_usd: f64,
    pub sideboard_price_eur: f64,
    pub sideboard_price_tix: f64,
    pub unpriced_cards: u32,
}

pub fn analyze_deck(deck: &ParsedDeck, db: &CardDb) -> Result<DeckAnalysis> {
    let mut legality_issues = Vec::new();
    let mut unknown_cards = Vec::new();

    let total_cards = deck
        .commanders
        .iter()
        .chain(deck.main.iter())
        .map(|e| e.quantity)
        .sum::<u32>();

    if total_cards != 100 {
        legality_issues.push(format!(
            "Commander decks must contain exactly 100 cards including commander(s); this list has {total_cards}."
        ));
    }

    if deck.commanders.is_empty() {
        legality_issues.push(
            "No commander was identified. Put the commander under a 'Commander' heading or mark it *CMDR*."
                .to_string(),
        );
    }

    if deck.commanders.len() > 2 {
        legality_issues.push(format!(
            "{} commanders were supplied; normal Commander supports one, or an explicitly legal paired-commanders arrangement.",
            deck.commanders.len()
        ));
    }

    let mut commander_cards = Vec::new();
    let mut identity = String::new();

    for entry in &deck.commanders {
        match db.get_card(&entry.name)? {
            Some(card) => {
                if !card.commander_legal {
                    legality_issues.push(format!("{} is not Commander legal.", card.name));
                }
                if !card.commander_eligible {
                    legality_issues.push(format!(
                        "{} does not appear eligible to be a commander.",
                        card.name
                    ));
                }
                identity = union_identity(&identity, &card.color_identity);
                commander_cards.push(card);
            }
            None => {
                unknown_cards.push(entry.name.clone());
                legality_issues.push(format!(
                    "Commander '{}' was not found in the local card database.",
                    entry.name
                ));
            }
        }
    }

    if commander_cards.len() == 2 && !pair_compatible(&commander_cards[0], &commander_cards[1]) {
        legality_issues.push(
            "Two commanders were supplied, but their pairing ability was not recognized. Verify Partner/Background/Doctor's companion legality manually."
                .to_string(),
        );
    }

    let mut counts: HashMap<String, u32> = HashMap::new();
    for entry in deck.commanders.iter().chain(deck.main.iter()) {
        *counts.entry(entry.name.to_ascii_lowercase()).or_default() += entry.quantity;
    }

    for (name_lower, qty) in &counts {
        if *qty <= 1 {
            continue;
        }

        if let Some(card) = db.get_card(name_lower)? {
            if !copies_allowed(&card, *qty) {
                legality_issues.push(format!(
                    "{} appears {} times and does not permit that many copies.",
                    card.name, qty
                ));
            }
        }
    }

    let mut types = TypeStats::default();
    let mut roles = RoleStats::default();
    let mut curve = [0u64; 8];
    let mut pip_demand = [0u64; 5];
    let mut direct_sources = [0u64; 5];
    let mut fetch_lands = 0u32;
    let mut cmc_sum = 0.0;
    let mut cmc_count = 0u32;
    let mut known_cards = 0u32;
    let mut total_price_usd = 0.0f64;
    let mut total_price_eur = 0.0f64;
    let mut total_price_tix = 0.0f64;
    let mut sideboard_price_usd = 0.0f64;
    let mut sideboard_price_eur = 0.0f64;
    let mut sideboard_price_tix = 0.0f64;
    let mut unpriced_cards = 0u32;

    let mut candidate_cards = Vec::<(Card, u32, CardRoles)>::new();

    for entry in deck.commanders.iter().chain(deck.main.iter()) {
        let Some(card) = db.get_card(&entry.name)? else {
            if !unknown_cards
                .iter()
                .any(|x| x.eq_ignore_ascii_case(&entry.name))
            {
                unknown_cards.push(entry.name.clone());
            }
            continue;
        };

        known_cards += entry.quantity;

        if let Some(price) = card.price_usd {
            total_price_usd += price * entry.quantity as f64;
        } else if !card.is_basic {
            unpriced_cards += entry.quantity;
        }

        if let Some(price) = card.price_eur {
            total_price_eur += price * entry.quantity as f64;
        }

        if let Some(price) = card.price_tix {
            total_price_tix += price * entry.quantity as f64;
        }

        if !card.commander_legal {
            legality_issues.push(format!("{} is not Commander legal.", card.name));
        }

        if !identity.is_empty() && !identity_subset(&card.color_identity, &identity) {
            legality_issues.push(format!(
                "{} has color identity {} outside commander identity {}.",
                card.name,
                pretty_identity(&card.color_identity),
                pretty_identity(&identity)
            ));
        }

        classify_types(&card, entry.quantity, &mut types);

        let card_roles = classify_card(&card);
        add_roles(&mut roles, card_roles, entry.quantity);

        if card
            .type_line
            .to_ascii_lowercase()
            .contains("land")
            && is_fetch_land(&card)
        {
            fetch_lands += entry.quantity;
        }

        if !card.type_line.to_ascii_lowercase().contains("land") {
            cmc_sum += card.cmc * entry.quantity as f64;
            cmc_count += entry.quantity;

            let bucket = if card.cmc >= 7.0 {
                7
            } else {
                card.cmc.floor().max(0.0) as usize
            };
            curve[bucket] += entry.quantity as u64;

            add_pips(&card.mana_cost, entry.quantity, &mut pip_demand);
        }

        add_sources(&card.produced_mana, entry.quantity, &mut direct_sources);
        candidate_cards.push((card, entry.quantity, card_roles));
    }

for entry in &deck.sideboard {
    let Some(card) = db.get_card(&entry.name)? else {
        continue;
    };

    if let Some(price) = card.price_usd {
        sideboard_price_usd += price * entry.quantity as f64;
    } else if !card.is_basic {
        unpriced_cards += entry.quantity;
    }

    if let Some(price) = card.price_eur {
        sideboard_price_eur += price * entry.quantity as f64;
    }

    if let Some(price) = card.price_tix {
        sideboard_price_tix += price * entry.quantity as f64;
    }
}

    if !unknown_cards.is_empty() {
        legality_issues.push(format!(
            "{} card name(s) could not be resolved, so legality could not be fully validated: {}",
            unknown_cards.len(),
            unknown_cards.join(", ")
        ));
    }

    let avg_cmc = if cmc_count > 0 {
        cmc_sum / cmc_count as f64
    } else {
        0.0
    };

    let tags = detect_deck_tags(deck, db)?;
    let bracket = estimate_bracket(deck, db)?;

    let recommendations =
        build_recommendations(types, roles, avg_cmc, &pip_demand, &direct_sources);

    let cut_candidates =
        build_cut_candidates(deck, &candidate_cards, types, &tags);

    let add_suggestions = build_add_suggestions(
        db,
        &identity,
        deck,
        roles,
        types,
        avg_cmc,
        &tags,
    )?;

    legality_issues.sort();
    legality_issues.dedup();

    Ok(DeckAnalysis {
        deck_name: deck.name.clone(),
        commanders: deck.commanders.iter().map(|x| x.name.clone()).collect(),
        identity,
        total_cards,
        sideboard_count: deck.sideboard.iter().map(|entry| entry.quantity).sum(),
        known_cards,
        unknown_cards,
        legal: legality_issues.is_empty(),
        legality_issues,
        types,
        roles,
        avg_cmc,
        curve,
        pip_demand,
        direct_sources,
        fetch_lands,
        recommendations,
        cut_candidates,
        add_suggestions,
        tags,
        bracket,
        total_price_usd,
        total_price_eur,
        total_price_tix,
        sideboard_price_usd,
        sideboard_price_eur,
        sideboard_price_tix,
        unpriced_cards,
    })
}

pub fn classify_card(card: &Card) -> CardRoles {
    let oracle = card.oracle_text.to_ascii_lowercase();
    let type_line = card.type_line.to_ascii_lowercase();

    let is_land = type_line.contains("land");

    let ramp = !is_land
        && (
            oracle.contains("add {")
                || oracle.contains("create a treasure token")
                || oracle.contains("create two treasure tokens")
                || oracle.contains("search your library for a basic land")
                || oracle.contains("search your library for a land card")
                || oracle.contains("put a land card from your hand onto the battlefield")
        );

    let draw = oracle.contains("draw a card")
        || oracle.contains("draw two cards")
        || oracle.contains("draw three cards")
        || oracle.contains("draw cards equal")
        || oracle.contains("draw that many cards")
        || oracle.contains("exile the top card of your library")
        || oracle.contains("exile the top two cards of your library");

    let tutor = oracle.contains("search your library for a card")
        || oracle.contains("search your library for an artifact card")
        || oracle.contains("search your library for an enchantment card")
        || oracle.contains("search your library for a creature card")
        || oracle.contains("search your library for an instant card")
        || oracle.contains("search your library for a sorcery card")
        || oracle.contains("search your library for a permanent card")
        || oracle.contains("search your library for a card with mana value");

    let wipe = oracle.contains("destroy all creatures")
        || oracle.contains("exile all creatures")
        || oracle.contains("destroy all nonland permanents")
        || oracle.contains("exile all nonland permanents")
        || oracle.contains("all creatures get -")
        || oracle.contains("each creature gets -");

    let removal = !wipe
        && (oracle.contains("destroy target")
            || oracle.contains("exile target")
            || oracle.contains("return target creature")
            || oracle.contains("return target permanent")
            || oracle.contains("target creature gets -")
            || oracle.contains("target permanent's controller sacrifices"));

    let counter = oracle.contains("counter target spell")
        || oracle.contains("counter target activated")
        || oracle.contains("counter target triggered");

    let recursion = (oracle.contains("from your graveyard")
        && (oracle.contains("return target")
            || oracle.contains("return a ")
            || oracle.contains("cast ")
            || oracle.contains("play ")))
        || oracle.contains("return all")
            && oracle.contains("graveyard");

    let protection = oracle.contains("hexproof")
        || oracle.contains("indestructible")
        || oracle.contains("phase out")
        || oracle.contains("protection from")
        || oracle.contains("can't be countered");

    let graveyard = oracle.contains("graveyard")
        || oracle.contains("mill ")
        || oracle.contains("surveil");

    CardRoles {
        ramp,
        draw,
        tutor,
        removal,
        counter,
        wipe,
        recursion,
        protection,
        graveyard,
    }
}

fn classify_types(card: &Card, qty: u32, stats: &mut TypeStats) {
    let ty = card.type_line.to_ascii_lowercase();

    if ty.contains("land") {
        stats.lands += qty;
    }
    if ty.contains("creature") {
        stats.creatures += qty;
    }
    if ty.contains("artifact") {
        stats.artifacts += qty;
    }
    if ty.contains("enchantment") {
        stats.enchantments += qty;
    }
    if ty.contains("instant") {
        stats.instants += qty;
    }
    if ty.contains("sorcery") {
        stats.sorceries += qty;
    }
    if ty.contains("planeswalker") {
        stats.planeswalkers += qty;
    }
    if ty.contains("battle") {
        stats.battles += qty;
    }
}

fn add_roles(stats: &mut RoleStats, roles: CardRoles, qty: u32) {
    if roles.ramp {
        stats.ramp += qty;
    }
    if roles.draw {
        stats.draw += qty;
    }
    if roles.tutor {
        stats.tutors += qty;
    }
    if roles.removal {
        stats.removal += qty;
    }
    if roles.counter {
        stats.counters += qty;
    }
    if roles.wipe {
        stats.wipes += qty;
    }
    if roles.recursion {
        stats.recursion += qty;
    }
    if roles.protection {
        stats.protection += qty;
    }
    if roles.graveyard {
        stats.graveyard += qty;
    }
}

fn add_pips(mana_cost: &str, qty: u32, pips: &mut [u64; 5]) {
    for (i, c) in ['W', 'U', 'B', 'R', 'G'].into_iter().enumerate() {
        pips[i] += mana_cost.chars().filter(|x| *x == c).count() as u64 * qty as u64;
    }
}

fn add_sources(produced: &str, qty: u32, sources: &mut [u64; 5]) {
    for (i, c) in ['W', 'U', 'B', 'R', 'G'].into_iter().enumerate() {
        if produced.contains(c) {
            sources[i] += qty as u64;
        }
    }
}

fn is_fetch_land(card: &Card) -> bool {
    let o = card.oracle_text.to_ascii_lowercase();
    let t = card.type_line.to_ascii_lowercase();

    t.contains("land")
        && o.contains("sacrifice")
        && o.contains("search your library")
        && (o.contains("plains")
            || o.contains("island")
            || o.contains("swamp")
            || o.contains("mountain")
            || o.contains("forest")
            || o.contains("land card"))
}

fn build_recommendations(
    types: TypeStats,
    roles: RoleStats,
    avg_cmc: f64,
    pips: &[u64; 5],
    sources: &[u64; 5],
) -> Vec<String> {
    let mut out = Vec::new();

    let land_target = if avg_cmc <= 2.3 {
        (31, 35)
    } else if avg_cmc >= 3.7 {
        (36, 40)
    } else {
        (34, 38)
    };

    if types.lands < land_target.0 {
        out.push(format!(
            "Land count is {}. For this {:.2} average mana value, start testing closer to {}-{} lands.",
            types.lands, avg_cmc, land_target.0, land_target.1
        ));
    } else if types.lands > land_target.1 {
        out.push(format!(
            "Land count is {}. The curve suggests testing whether {}-{} lands is enough.",
            types.lands, land_target.0, land_target.1
        ));
    }

    if roles.ramp < 10 {
        out.push(format!(
            "Only {} cards were detected as ramp. A common Commander baseline is roughly 10-12.",
            roles.ramp
        ));
    }

    if roles.draw < 10 {
        out.push(format!(
            "Only {} cards were detected as repeatable/card-draw effects. Consider increasing card advantage toward 10+ slots.",
            roles.draw
        ));
    }

    let interaction = roles.removal + roles.counters;
    if interaction < 8 {
        out.push(format!(
            "Only {} targeted interaction/counterspell slots were detected. Consider 8-12 depending on pod speed.",
            interaction
        ));
    }

    if roles.wipes < 2 {
        out.push(format!(
            "Only {} broad board wipes were detected. Many mid-power lists want around 2-3.",
            roles.wipes
        ));
    }

    let pip_total: u64 = pips.iter().sum();
    if pip_total > 0 {
        let source_total: u64 = sources.iter().sum();
        if source_total > 0 {
            for i in 0..5 {
                let demand = pips[i] as f64 / pip_total as f64;
                let supply = sources[i] as f64 / source_total as f64;
                if demand - supply > 0.10 {
                    let color = ["White", "Blue", "Black", "Red", "Green"][i];
                    out.push(format!(
                        "{color} mana demand looks materially higher than direct-source share. Check fixing/fetch coverage."
                    ));
                }
            }
        }
    }

    if out.is_empty() {
        out.push(
            "No large structural deficit was detected by the baseline heuristics. Focus next on commander-specific synergy, interaction quality, and win-condition density."
                .to_string(),
        );
    }

    out
}

fn build_cut_candidates(
    deck: &ParsedDeck,
    cards: &[(Card, u32, CardRoles)],
    types: TypeStats,
    tags: &[ArchetypeTag],
) -> Vec<CutCandidate> {
    let commander_names: HashSet<String> = deck
        .commanders
        .iter()
        .map(|x| x.name.to_ascii_lowercase())
        .collect();

    let mut cuts = Vec::new();

    for (card, qty, roles) in cards {
        if commander_names.contains(&card.name.to_ascii_lowercase()) {
            continue;
        }

        if card.type_line.to_ascii_lowercase().contains("land") {
            if types.lands <= 38 {
                continue;
            }
        }

        let mut score = 50i32;
        let mut reasons = Vec::new();

        let tag_fit = tag_fit_score(card, tags);
        score += tag_fit;

        if tag_fit >= 10 {
            reasons.push("strong fit with the deck's dominant archetype tags");
        } else if tag_fit == 0 && !tags.is_empty() {
            score -= 6;
            reasons.push("little overlap with the deck's dominant archetype tags");
        }

        if card.cmc >= 7.0 {
            score -= 18;
            reasons.push("very high mana value");
        } else if card.cmc >= 5.0 {
            score -= 9;
            reasons.push("high mana value");
        } else if card.cmc <= 2.0 {
            score += 5;
        }

        let role_count = roles.count();
        score += (role_count as i32) * 5;

        if role_count == 0 && !card.type_line.to_ascii_lowercase().contains("land") {
            score -= 12;
            reasons.push("no baseline ramp/draw/interaction/tutor/recursion role detected");
        }

        match card.edhrec_rank {
            Some(rank) if rank <= 1000 => score += 20,
            Some(rank) if rank <= 5000 => score += 12,
            Some(rank) if rank <= 15000 => score += 5,
            Some(rank) if rank >= 50000 => {
                score -= 5;
                reasons.push("low broad Commander usage rank");
            }
            None => {
                score -= 3;
                reasons.push("no EDHREC rank available from Scryfall");
            }
            _ => {}
        }

        if *qty > 1
            && !card.is_basic
            && !has_copy_rule_override(&card)
        {
            reasons.push("multiple copies detected");
        }

        if reasons.is_empty() {
            reasons.push("lower heuristic score than stronger structural slots");
        }

        cuts.push(CutCandidate {
            card: card.name.clone(),
            score,
            tag_fit,
            reason: reasons.join(", "),
        });
    }

    cuts.sort_by_key(|x| x.score);
    cuts.truncate(10);
    cuts
}

fn build_add_suggestions(
    db: &CardDb,
    identity: &str,
    deck: &ParsedDeck,
    roles: RoleStats,
    types: TypeStats,
    avg_cmc: f64,
    tags: &[ArchetypeTag],
) -> Result<Vec<AddSuggestion>> {
    let existing: HashSet<String> = deck
        .commanders
        .iter()
        .chain(deck.main.iter())
        .map(|x| x.name.to_ascii_lowercase())
        .collect();

    let mut needs: Vec<(&str, fn(CardRoles) -> bool)> = Vec::new();

    if roles.ramp < 10 {
        needs.push(("Ramp", |r| r.ramp));
    }
    if roles.draw < 10 {
        needs.push(("Card Draw", |r| r.draw));
    }
    if roles.removal + roles.counters < 8 {
        needs.push(("Interaction", |r| r.removal || r.counter));
    }
    if roles.wipes < 2 {
        needs.push(("Board Wipe", |r| r.wipe));
    }
    if roles.protection < 3 {
        needs.push(("Protection", |r| r.protection));
    }

    if needs.is_empty() && types.lands >= 30 && avg_cmc < 5.0 {
        needs.push(("Interaction", |r| r.removal || r.counter));
    }

    if !tags.is_empty() {
        needs.push(("Archetype Fit", |_| true));
    }

    let pool = db.top_ranked_cards(5000)?;
    let mut output = Vec::new();
    let mut used = HashSet::new();

    for (label, predicate) in needs {
        let mut candidates = pool
            .iter()
            .filter(|card| {
                !existing.contains(&card.name.to_ascii_lowercase())
                    && identity_subset(&card.color_identity, identity)
                    && !card.type_line.to_ascii_lowercase().contains("land")
                    && predicate(classify_card(card))
            })
            .map(|card| {
                let tag_fit = tag_fit_score(card, tags);
                let rank_score = match card.edhrec_rank {
                    Some(rank) if rank <= 500 => 25,
                    Some(rank) if rank <= 2_000 => 20,
                    Some(rank) if rank <= 10_000 => 12,
                    Some(rank) if rank <= 30_000 => 6,
                    _ => 0,
                };
                let efficiency = if card.cmc <= 2.0 {
                    8
                } else if card.cmc <= 4.0 {
                    4
                } else {
                    0
                };

                (card, tag_fit + rank_score + efficiency, tag_fit)
            })
            .collect::<Vec<_>>();

        candidates.sort_by(|a, b| b.1.cmp(&a.1));

        for (card, _, tag_fit) in candidates.into_iter().take(6) {
            if used.insert(card.name.to_ascii_lowercase()) {
                output.push(AddSuggestion {
                    card: card.name.clone(),
                    role: label.to_string(),
                    edhrec_rank: card.edhrec_rank,
                    tag_fit,
                });
            }
        }
    }

    Ok(output)
}

pub fn has_copy_rule_override(card: &Card) -> bool {
    if card.is_basic {
        return true;
    }

    let oracle = card.oracle_text.to_ascii_lowercase();

    oracle.contains("a deck can have any number of cards named")
        || oracle.contains("a deck can have up to seven cards named")
        || oracle.contains("a deck can have up to nine cards named")
}

pub fn copies_allowed(card: &Card, qty: u32) -> bool {
    if card.is_basic {
        return true;
    }

    let oracle = card.oracle_text.to_ascii_lowercase();

    if oracle.contains("a deck can have any number of cards named") {
        return true;
    }

    if oracle.contains("a deck can have up to seven cards named") {
        return qty <= 7;
    }

    if oracle.contains("a deck can have up to nine cards named") {
        return qty <= 9;
    }

    qty <= 1
}


pub fn identity_subset(card_identity: &str, commander_identity: &str) -> bool {
    card_identity
        .chars()
        .all(|c| commander_identity.contains(c))
}

fn union_identity(a: &str, b: &str) -> String {
    let mut s = String::new();
    for c in ['W', 'U', 'B', 'R', 'G'] {
        if a.contains(c) || b.contains(c) {
            s.push(c);
        }
    }
    s
}

pub fn pretty_identity(identity: &str) -> String {
    if identity.is_empty() {
        "Colorless".to_string()
    } else {
        identity.to_string()
    }
}
