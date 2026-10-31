//! Canadian Highlander deck builder.
//!
//! Canadian Highlander is a 100-card singleton format played one-on-one at
//! 20 life with no commander and no sideboard. The card pool is Vintage's
//! (with its banned list) and a deck may spend at most 10 points from the
//! points list (see `format.rs`). Because there's no commander, the player
//! picks colors and play styles; the builder reads the rules text of every
//! legal card in those colors, so it doesn't depend on Commander decklists.

use crate::{
    analysis::{classify_card, identity_subset, CardRoles},
    data::{Card, CardDb},
    deck::{DeckEntry, ParsedDeck},
    format::{canlander_points, check_deck, Format, CANLANDER_MAX_POINTS, CANLANDER_POINTS_DATE},
    generator::{GamePlan, GeneratedDeck, GenerationProgress},
    oracle_search::{deep_search, DeepFind, ThemeMatcher},
};
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};

/// Play styles offered for Canadian Highlander (any EDHREC-style theme or
/// creature type also works).
pub const HIGHLANDER_THEMES: &[&str] = &[
    "Aggro", "Midrange", "Control", "Tempo", "Ramp", "Combo", "Burn", "Reanimator", "Storm",
    "Artifacts", "Tokens", "Graveyard", "+1/+1 Counters", "-1/-1 Counters", "Blink",
    "Spellslinger", "Stax", "Landfall", "Sacrifice", "Equipment", "Enchantments", "Lifegain",
    "Flying",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Style {
    Aggro,
    Control,
    Tempo,
    Midrange,
    Ramp,
}

fn style_of(themes: &[String]) -> Style {
    for t in themes {
        match t.to_ascii_lowercase().as_str() {
            "aggro" | "burn" | "equipment" => return Style::Aggro,
            "control" | "stax" | "spellslinger" => return Style::Control,
            "tempo" | "flying" => return Style::Tempo,
            "ramp" | "landfall" | "big mana" => return Style::Ramp,
            _ => {}
        }
    }
    Style::Midrange
}

/// Archetype words that describe a speed rather than card text.
fn is_style_word(theme: &str) -> bool {
    matches!(
        theme.to_ascii_lowercase().as_str(),
        "aggro" | "control" | "tempo" | "midrange" | "ramp" | "combo"
    )
}

fn color_name(colors: &str) -> String {
    let named = [
        ("", "Colorless"), ("W", "Mono-White"), ("U", "Mono-Blue"), ("B", "Mono-Black"),
        ("R", "Mono-Red"), ("G", "Mono-Green"), ("WU", "Azorius"), ("UB", "Dimir"),
        ("BR", "Rakdos"), ("RG", "Gruul"), ("WG", "Selesnya"), ("WB", "Orzhov"),
        ("UR", "Izzet"), ("BG", "Golgari"), ("WR", "Boros"), ("UG", "Simic"),
        ("WUB", "Esper"), ("UBR", "Grixis"), ("BRG", "Jund"), ("WRG", "Naya"),
        ("WUG", "Bant"), ("WBG", "Abzan"), ("URG", "Temur"), ("WBR", "Mardu"),
        ("WUR", "Jeskai"), ("UBG", "Sultai"), ("WUBRG", "Five-Color"),
    ];
    named
        .iter()
        .find(|(c, _)| *c == colors)
        .map(|(_, n)| n.to_string())
        .unwrap_or_else(|| format!("{}-Color", colors.len()))
}

fn canonical(colors: &str) -> String {
    let up = colors.to_ascii_uppercase();
    ['W', 'U', 'B', 'R', 'G'].iter().filter(|c| up.contains(**c)).collect()
}

fn is_land(card: &Card) -> bool {
    card.type_line.to_ascii_lowercase().contains("land")
}

/// Cards that only work with a commander or with several opponents.
fn multiplayer_only(card: &Card) -> bool {
    let t = card.oracle_text.to_ascii_lowercase();
    t.contains("commander") || t.contains("each other player") && t.contains("vote")
}

/// Rough one-on-one card quality from data available offline: popularity
/// (EDHREC rank works as a proxy for "staple"), Vintage restricted status,
/// mana efficiency and what the card does.
fn power_score(card: &Card, roles: CardRoles, style: Style) -> i32 {
    let mut score = match card.edhrec_rank {
        Some(r) if r <= 200 => 46,
        Some(r) if r <= 1_000 => 38,
        Some(r) if r <= 3_000 => 30,
        Some(r) if r <= 8_000 => 22,
        Some(r) if r <= 15_000 => 14,
        Some(r) if r <= 30_000 => 6,
        _ => 0,
    };
    if card.vintage_legal == "restricted" {
        score += 30;
    }
    let cmc = card.cmc as i32;
    let curve_cap = match style {
        Style::Aggro => 3,
        Style::Tempo => 3,
        Style::Control | Style::Midrange => 4,
        Style::Ramp => 6,
    };
    score += match cmc {
        0..=1 => 16,
        2 => 13,
        3 => 8,
        _ if cmc <= curve_cap => 4,
        _ => -6 * (cmc - curve_cap),
    };
    let ty = card.type_line.to_ascii_lowercase();
    let text = card.oracle_text.to_ascii_lowercase();
    match style {
        Style::Aggro => {
            if ty.contains("creature") && cmc <= 3 {
                score += 14;
            }
            if text.contains("haste") || text.contains("deals") && text.contains("damage to any target") {
                score += 8;
            }
            if roles.counter || roles.wipe {
                score -= 12;
            }
        }
        Style::Control => {
            if roles.counter || roles.removal || roles.wipe || roles.draw {
                score += 12;
            }
        }
        Style::Tempo => {
            if (ty.contains("creature") && cmc <= 2) || text.contains("flash") || roles.counter {
                score += 10;
            }
        }
        Style::Ramp => {
            if roles.ramp {
                score += 12;
            }
            if cmc >= 5 && ty.contains("creature") {
                score += 6;
            }
        }
        Style::Midrange => {
            if ty.contains("creature") && (2..=4).contains(&cmc) {
                score += 6;
            }
        }
    }
    score
}

#[derive(Default)]
struct Counts {
    creatures: u32,
    interaction: u32,
    draw: u32,
    ramp: u32,
    theme: u32,
}

fn targets(style: Style) -> Counts {
    match style {
        Style::Aggro => Counts { creatures: 26, interaction: 9, draw: 4, ramp: 2, theme: 22 },
        Style::Tempo => Counts { creatures: 18, interaction: 14, draw: 6, ramp: 2, theme: 20 },
        Style::Control => Counts { creatures: 10, interaction: 20, draw: 10, ramp: 4, theme: 16 },
        Style::Ramp => Counts { creatures: 18, interaction: 9, draw: 6, ramp: 12, theme: 18 },
        Style::Midrange => Counts { creatures: 20, interaction: 13, draw: 7, ramp: 4, theme: 20 },
    }
}

/// Build a Canadian Highlander deck in `colors` around `theme` (one or more
/// themes joined with " + "). `power` 1–5 sets how many of the 10 points it
/// may spend: 1–2 none, 3 up to 4, 4 up to 7, 5 all 10.
pub fn generate_highlander<F>(
    colors: &str,
    theme: &str,
    power: u8,
    use_deep_search: bool,
    db: &CardDb,
    mut progress: F,
) -> Result<GeneratedDeck>
where
    F: FnMut(GenerationProgress),
{
    if !db.has_format_data() {
        return Err(anyhow!(
            "Update card data (Settings → Update card data) to build Canadian Highlander decks; the format rules need the newer card data."
        ));
    }
    let colors = canonical(colors);
    let themes = crate::archetype::theme_list(theme);
    let style = style_of(&themes);
    let card_themes: Vec<String> = themes.iter().filter(|t| !is_style_word(t)).cloned().collect();
    let matchers: Vec<ThemeMatcher> = card_themes.iter().map(|t| ThemeMatcher::new(t)).collect();
    let point_budget = match power {
        0..=2 => 0,
        3 => 4,
        4 => 7,
        _ => CANLANDER_MAX_POINTS,
    };

    progress(GenerationProgress::new_pub(8, "Reading the card pool", format!(
        "Every Vintage-legal card in {}",
        color_name(&colors)
    )));

    let mut pool: Vec<Card> = Vec::new();
    db.scan_cards(|card| {
        if card.is_basic
            || Format::Canlander.card_allowed(&card).is_err()
            || !identity_subset(&card.color_identity, &colors)
            || multiplayer_only(&card)
            || card.type_line.to_ascii_lowercase().contains("plane —")
        {
            return;
        }
        pool.push(card);
    })?;

    let mut deep_finds: HashMap<String, DeepFind> = HashMap::new();
    if use_deep_search && !card_themes.is_empty() {
        progress(GenerationProgress::new_pub(20, "Deep card search", format!(
            "Reading rules text for {}",
            card_themes.join(", ")
        )));
        for find in deep_search(db, &card_themes, Some(&colors), Format::Canlander, 120)? {
            deep_finds.insert(find.name.to_ascii_lowercase(), find);
        }
    }

    // Canadian Highlander's own decklists: MTGTop8 tournament results and
    // popular Moxfield lists in these colors.
    progress(GenerationProgress::new_pub(22, "Reading Canadian Highlander decklists", "MTGTop8 and Moxfield".to_string()));
    let evidence = crate::format_sources::highlander_evidence(&colors, &themes, db, 40, |detail| {
        progress(GenerationProgress::new_pub(26, "Reading Canadian Highlander decklists", detail));
    });
    // How strongly decklist play rate counts. With few decks it's a nudge;
    // with a real sample it leads.
    // One or two matching lists only nudge; a real sample leads.
    let ref_weight = match evidence.strength {
        s if s < 0.5 => 0.0,
        s if s < 2.5 => 35.0,
        s if s < 6.0 => 110.0,
        s if s < 12.0 => 180.0,
        _ => 250.0,
    };
    let ref_bonus = |name: &str| {
        let share = evidence.share_of(name);
        (share * ref_weight) as i32 + if share >= 0.5 && ref_weight >= 110.0 { 30 } else { 0 }
    };

    progress(GenerationProgress::new_pub(30, "Choosing spells", format!(
        "{} legal cards • point budget {point_budget}/{CANLANDER_MAX_POINTS}",
        pool.len()
    )));

    let land_target: usize = match (style, colors.len()) {
        (Style::Aggro, _) => 38,
        (Style::Ramp, _) => 41,
        (_, n) if n >= 3 => 41,
        _ => 40,
    };
    let spell_target = 100 - land_target;
    let goal = targets(style);

    // Score once.
    let scored: Vec<(usize, CardRoles, i32, bool, u32)> = pool
        .iter()
        .enumerate()
        .filter(|(_, c)| !is_land(c))
        .map(|(i, c)| {
            let roles = classify_card(c);
            let fit = matchers.iter().map(|m| m.score(c).0).max().unwrap_or(0);
            let deep = deep_finds.get(&c.name.to_ascii_lowercase());
            let on_theme = fit >= 2 || deep.is_some();
            // Theme fit nudges; card quality and the deck's needs decide.
            let mut score = power_score(c, roles, style) + fit.min(8) * 3 + ref_bonus(&c.name);
            if let Some(f) = deep {
                score += 6 + f.score / 15;
            }
            (i, roles, score, on_theme, canlander_points(&c.name))
        })
        .collect();

    let mut used = HashSet::<usize>::new();
    let mut have = Counts::default();
    let mut points_used = 0u32;
    let mut main = Vec::<DeckEntry>::new();
    let mut picked_points = Vec::<(String, u32)>::new();

    while main.len() < spell_target {
        let mut best: Option<(usize, i32)> = None;
        for (slot, (i, roles, base, on_theme, points)) in scored.iter().enumerate() {
            if used.contains(&slot) || points_used + points > point_budget {
                continue;
            }
            let c = &pool[*i];
            let is_creature = c.type_line.to_ascii_lowercase().contains("creature");
            let interaction = roles.removal || roles.counter || roles.wipe;
            let mut need = 0;
            if is_creature && have.creatures < goal.creatures {
                need += 22;
            } else if is_creature && have.creatures >= goal.creatures + 4 {
                need -= 40;
            }
            if interaction && have.interaction < goal.interaction {
                need += 36;
            }
            if roles.draw && have.draw < goal.draw {
                need += 22;
            }
            if roles.ramp && have.ramp < goal.ramp {
                need += 16;
            } else if roles.ramp && have.ramp >= goal.ramp + 3 {
                need -= 20;
            }
            if *on_theme && have.theme < goal.theme {
                need += 24;
            }
            // Pointed cards are strong; spend points on the most efficient.
            let point_value = if *points > 0 { 25 - (*points as i32) * 2 } else { 0 };
            let score = base + need + point_value;
            if best.map(|(_, b)| score > b).unwrap_or(true) {
                best = Some((slot, score));
            }
        }
        let Some((slot, _)) = best else { break };
        used.insert(slot);
        let (i, roles, _, on_theme, points) = scored[slot];
        let c = &pool[i];
        if c.type_line.to_ascii_lowercase().contains("creature") {
            have.creatures += 1;
        }
        if roles.removal || roles.counter || roles.wipe {
            have.interaction += 1;
        }
        if roles.draw {
            have.draw += 1;
        }
        if roles.ramp {
            have.ramp += 1;
        }
        if on_theme {
            have.theme += 1;
        }
        if points > 0 {
            points_used += points;
            picked_points.push((c.name.clone(), points));
        }
        main.push(DeckEntry::new(1, c.name.clone()));
    }

    progress(GenerationProgress::new_pub(70, "Building the mana base", color_name(&colors)));

    // Nonbasic lands: dual/fetch lands for these colors and utility lands.
    let nonbasic_target = match colors.len() {
        0 | 1 => 7,
        2 => 15,
        _ => 22,
    };
    let mut lands: Vec<(usize, i32, u32)> = pool
        .iter()
        .enumerate()
        .filter(|(_, c)| is_land(c))
        .map(|(i, c)| {
            let produces: usize = colors.chars().filter(|ch| c.produced_mana.contains(*ch)).count();
            let off_color = c.produced_mana.chars().filter(|ch| "WUBRG".contains(*ch) && !colors.contains(*ch)).count();
            let text = c.oracle_text.to_ascii_lowercase();
            let ty = c.type_line.to_ascii_lowercase();
            // Fetch lands that find a land of our colors (by basic type).
            let fetch = text.contains("search your library for")
                && text.contains("onto the battlefield")
                && colors.chars().any(|ch| {
                    let basic = match ch { 'W' => "plains", 'U' => "island", 'B' => "swamp", 'R' => "mountain", _ => "forest" };
                    text.contains(basic) || text.contains("basic land")
                });
            let mut score = match c.edhrec_rank {
                Some(r) if r <= 500 => 30,
                Some(r) if r <= 3_000 => 20,
                Some(r) if r <= 10_000 => 10,
                _ => 0,
            };
            score += produces as i32 * 14;
            // A true dual for exactly these colors (original duals, shocks,
            // fast and check lands) beats a land that taps for anything.
            if colors.len() >= 2 && produces >= 2 && off_color == 0 {
                score += 28;
            }
            if off_color > 0 && c.vintage_legal != "restricted" {
                score -= 8 + 4 * off_color as i32;
            }
            // Lands that only work with a creature type, legends, devotion or
            // an opponent's colors are unreliable in one-on-one.
            if text.contains("creature type") || text.contains("legendary") || text.contains("devotion")
                || text.contains("opponent controls could produce") || text.contains("historic")
            {
                score -= 25;
            }
            if ty.contains("basic") {
                score -= 100;
            }
            if fetch {
                score += 40;
            }
            if text.contains("enters tapped") || text.contains("enters the battlefield tapped") {
                score -= 14;
            }
            if colors.len() >= 2 && produces < 2 && !fetch {
                score -= 20;
            }
            if c.vintage_legal == "restricted" {
                score += 10;
            }
            score += ref_bonus(&c.name) / 2;
            (i, score, canlander_points(&c.name))
        })
        .collect();
    lands.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| pool[a.0].name.cmp(&pool[b.0].name)));
    let mut nonbasics = 0;
    for (i, score, points) in lands {
        if nonbasics >= nonbasic_target || score < 10 {
            continue;
        }
        if points_used + points > point_budget {
            continue;
        }
        if points > 0 {
            points_used += points;
            picked_points.push((pool[i].name.clone(), points));
        }
        main.push(DeckEntry::new(1, pool[i].name.clone()));
        nonbasics += 1;
    }

    // Basics split by colored mana symbols in the spells.
    let basics_needed = 100usize.saturating_sub(main.len());
    let mut weight: Vec<(char, usize)> = colors.chars().map(|c| (c, 0)).collect();
    for e in &main {
        if let Some(card) = pool.iter().find(|c| c.name == e.name) {
            for (c, w) in weight.iter_mut() {
                *w += card.mana_cost.matches(&format!("{{{c}}}")).count();
            }
        }
    }
    add_weighted_basics(&mut main, &weight, basics_needed);

    let deck_cards: Vec<(u32, Card)> = main
        .iter()
        .filter_map(|e| db.get_card(&e.name).ok().flatten().map(|c| (e.quantity, c)))
        .collect();
    let check = check_deck(Format::Canlander, &[], &deck_cards, &colors);
    if !check.legal() {
        return Err(anyhow!(
            "The Canadian Highlander build didn't pass the format check:\n{}",
            check.problems.join("\n")
        ));
    }

    let theme_label = if themes.is_empty() { "Midrange".to_string() } else { themes.join(" + ") };
    let name = format!("{} {} - Highlander", color_name(&colors), theme_label);

    let mut notes = vec![
        format!(
            "Canadian Highlander: 100 cards, one copy of each except basic lands, no sideboard, no commander. Played one-on-one at 20 life. Vintage banned list and the points list (updated {CANLANDER_POINTS_DATE})."
        ),
        if picked_points.is_empty() {
            format!("Points: 0 of {CANLANDER_MAX_POINTS} used (power level {power} keeps pointed cards out).")
        } else {
            format!(
                "Points: {} of {CANLANDER_MAX_POINTS} used — {}.",
                check.points,
                picked_points.iter().map(|(n, p)| format!("{n} ({p})")).collect::<Vec<_>>().join(", ")
            )
        },
        format!(
            "Built as {} with {} lands, {} creatures, {} interaction spells, {} card-draw spells{}.",
            match style {
                Style::Aggro => "an aggressive deck",
                Style::Control => "a control deck",
                Style::Tempo => "a tempo deck",
                Style::Ramp => "a ramp deck",
                Style::Midrange => "a midrange deck",
            },
            100 - spell_target.min(main.len()),
            have.creatures,
            have.interaction,
            have.draw,
            if card_themes.is_empty() { String::new() } else { format!(" and {} cards for {}", have.theme, card_themes.join(" / ")) },
        ),
        if evidence.decks.is_empty() {
            "No Canadian Highlander decklists could be read this time (MTGTop8 and Moxfield unreachable?), so cards were chosen from their rules text, mana value and how widely they're played overall.".to_string()
        } else {
            let mtgtop8 = evidence.decks.iter().filter(|d| d.source == "MTGTop8").count();
            let moxfield = evidence.decks.len() - mtgtop8;
            let exact = evidence.decks.iter().filter(|d| d.colors == colors).count();
            let mut top: Vec<(&String, &f64)> = evidence.share.iter().collect();
            top.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
            let top: Vec<String> = top
                .iter()
                .filter_map(|(n, sh)| db.get_card(n).ok().flatten().map(|c| format!("{} ({:.0}%)", c.name, **sh * 100.0)))
                .take(10)
                .collect();
            format!(
                "Built from Canadian Highlander decklists: {mtgtop8} tournament list(s) from MTGTop8 and {moxfield} popular Moxfield list(s) within these colors ({exact} in exactly these colors), weighted toward {}. Most played there: {}.{}",
                if themes.is_empty() { "exact color matches".to_string() } else { format!("{} decks", themes.join(" / ")) },
                top.join(", "),
                if exact < 3 { " Few lists use exactly these colors, so card quality and the deck's needs carried more of the choice." } else { "" }
            )
        },
    ];
    if use_deep_search && !card_themes.is_empty() {
        let picked: Vec<String> = main
            .iter()
            .filter_map(|e| deep_finds.get(&e.name.to_ascii_lowercase()))
            .filter(|f| f.edhrec_rank.map(|r| r > 15_000).unwrap_or(true))
            .map(|f| format!("{} ({})", f.name, f.reason))
            .collect();
        notes.push(format!(
            "Deep card search found {} cards for {} by rules text{}",
            deep_finds.len(),
            card_themes.join(", "),
            if picked.is_empty() { ".".to_string() } else { format!("; rarely played picks: {}.", picked.join("; ")) }
        ));
    }

    progress(GenerationProgress::new_pub(100, "Done", name.clone()));

    Ok(GeneratedDeck {
        deck: ParsedDeck {
            name,
            commanders: Vec::new(),
            main,
            sideboard: Vec::new(),
            format: Format::Canlander,
            considering: Vec::new(),
        },
        plan: GamePlan {
            commanders: Vec::new(),
            identity: colors.clone(),
            primary_tags: themes.clone(),
            summary: format!(
                "A {} Canadian Highlander deck built around {}. Spend the early turns {}; it has {} points of the format's power cards.",
                color_name(&colors),
                theme_label,
                match style {
                    Style::Aggro => "applying pressure with cheap creatures and burn",
                    Style::Control => "answering threats and drawing cards before landing a finisher",
                    Style::Tempo => "deploying cheap evasive threats and protecting them with counters",
                    Style::Ramp => "ramping into big threats",
                    Style::Midrange => "trading efficiently and pulling ahead on card quality",
                },
                check.points
            ),
            combos: Vec::new(),
            notes,
            sources: highlander_sources(&evidence),
        },
    })
}

fn highlander_sources(evidence: &crate::format_sources::HighlanderEvidence) -> Vec<crate::sources::DeckSource> {
    use crate::sources::DeckSource;
    let mut out = vec![
        DeckSource::automatic(
            "MTGTop8",
            "Canadian Highlander events and metagame",
            "https://www.mtgtop8.com/format?f=CHL",
            "Tournament decklists by archetype.",
        ),
        DeckSource::automatic(
            "Moxfield",
            "Canadian Highlander decks on Moxfield",
            "https://moxfield.com/decks/public?fmt=highlanderCanadian",
            "The most viewed public lists in the format.",
        ),
        DeckSource::automatic(
            "Canadian Highlander",
            "Points list",
            "https://canadianhighlander.ca/points-list/",
            "The 10-point limit and each card's points.",
        ),
    ];
    for d in &evidence.decks {
        out.push(DeckSource::automatic(
            d.source.clone(),
            d.title.clone(),
            d.url.clone(),
            format!("Reference list ({} cards, colors {}) used for card play rates.", d.cards.len(), if d.colors.is_empty() { "C" } else { &d.colors }),
        ));
    }
    out
}

fn add_weighted_basics(main: &mut Vec<DeckEntry>, weight: &[(char, usize)], total: usize) {
    if total == 0 {
        return;
    }
    let basic = |c: char| match c {
        'W' => "Plains",
        'U' => "Island",
        'B' => "Swamp",
        'R' => "Mountain",
        _ => "Forest",
    };
    if weight.is_empty() {
        main.push(DeckEntry::new(total as u32, "Wastes"));
        return;
    }
    let sum: usize = weight.iter().map(|(_, w)| (*w).max(1)).sum();
    let mut left = total;
    for (i, (c, w)) in weight.iter().enumerate() {
        let n = if i + 1 == weight.len() { left } else { (total * (*w).max(1) / sum).min(left) };
        if n > 0 {
            main.push(DeckEntry::new(n as u32, basic(*c)));
        }
        left -= n;
    }
}
