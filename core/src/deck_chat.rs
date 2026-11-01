//! "Ask about this deck": answers questions about one deck.
//!
//! With an AI model (built-in llama.cpp, Ollama or an online API) the model
//! gets the whole deck every turn: each card's text and roles, the deck's
//! stats, bracket, price, game plan and known combos. Without one (Android
//! without an API key, or the model not started) Commander Lab answers from
//! its own analysis: card text and roles, curve, mana, price, bracket, combos,
//! mulligans, how the deck wins, what to cut and add.

use crate::{
    ai::LocalAi,
    analysis::{analyze_deck, classify_card, CardRoles, DeckAnalysis},
    data::{Card, CardDb},
    deck::ParsedDeck,
    generator::GamePlan,
    guide::{build_guide, PlayGuide},
    spellbook::ComboReport,
};
use anyhow::Result;

/// Everything known about the deck, gathered once per question.
pub struct DeckFacts {
    pub deck: ParsedDeck,
    /// (quantity, card) for commanders then the main deck.
    pub cards: Vec<(u32, Card, bool)>,
    pub sideboard: Vec<String>,
    pub considering: Vec<String>,
    pub analysis: DeckAnalysis,
    pub guide: Option<PlayGuide>,
    pub combos: Option<ComboReport>,
}

pub fn gather(deck: &ParsedDeck, db: &CardDb, combos: Option<ComboReport>, plan: Option<&GamePlan>) -> Result<DeckFacts> {
    let analysis = analyze_deck(deck, db)?;
    let guide = build_guide(deck, db, &analysis, combos.as_ref(), plan).ok();
    let mut cards = Vec::new();
    for e in &deck.commanders {
        if let Some(c) = db.get_card(&e.name)? {
            cards.push((e.quantity, c, true));
        }
    }
    for e in &deck.main {
        if let Some(c) = db.get_card(&e.name)? {
            cards.push((e.quantity, c, false));
        }
    }
    Ok(DeckFacts {
        deck: deck.clone(),
        cards,
        sideboard: deck.sideboard.iter().map(|e| e.name.clone()).collect(),
        considering: deck.considering.iter().map(|e| e.name.clone()).collect(),
        analysis,
        guide,
        combos,
    })
}

fn role_names(r: CardRoles) -> Vec<&'static str> {
    let mut out = Vec::new();
    for (on, name) in [
        (r.ramp, "ramp"),
        (r.draw, "card draw"),
        (r.tutor, "tutor"),
        (r.removal, "removal"),
        (r.counter, "counterspell"),
        (r.wipe, "board wipe"),
        (r.recursion, "recursion"),
        (r.protection, "protection"),
        (r.graveyard, "graveyard"),
    ] {
        if on {
            out.push(name);
        }
    }
    out
}

fn price_of(c: &Card) -> String {
    match c.price_usd {
        Some(p) if !c.is_basic => format!("${p:.2}"),
        _ => "—".into(),
    }
}

fn one_line(text: &str, max: usize) -> String {
    let flat = text.replace('\n', " ");
    if flat.chars().count() <= max {
        flat
    } else {
        format!("{}…", flat.chars().take(max).collect::<String>())
    }
}

fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '\'' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Cards from the deck named in the question (full name, front face, or the
/// part before a comma for legends: "Atraxa").
pub fn mentioned(facts: &DeckFacts, question: &str) -> Vec<Card> {
    let q = format!(" {} ", normalize(question));
    let mut found: Vec<(usize, Card)> = Vec::new();
    for (_, card, _) in &facts.cards {
        if card.is_basic {
            continue;
        }
        let full = normalize(&card.name);
        let front = normalize(card.name.split(" // ").next().unwrap_or(&card.name));
        let short = normalize(card.name.split(',').next().unwrap_or(""));
        let hit = [full, front, short]
            .into_iter()
            .filter(|n| n.len() >= 4)
            .filter_map(|n| q.find(&format!(" {n} ")).map(|pos| (pos, n.len())))
            .max_by_key(|(_, len)| *len);
        if let Some((pos, _)) = hit {
            if !found.iter().any(|(_, c)| c.name == card.name) {
                found.push((pos, card.clone()));
            }
        }
    }
    found.sort_by_key(|(pos, _)| *pos);
    found.into_iter().map(|(_, c)| c).collect()
}

/// The deck as text for the AI model.
pub fn context_text(facts: &DeckFacts, focus: &[Card]) -> String {
    let a = &facts.analysis;
    let mut out = String::new();
    let commanders: Vec<&Card> = facts.cards.iter().filter(|(_, _, cmd)| *cmd).map(|(_, c, _)| c).collect();
    out.push_str(&format!(
        "DECK: {} — {} — {} cards — commander(s): {}\n",
        facts.deck.name,
        facts.deck.format.label(),
        a.total_cards,
        if commanders.is_empty() { "none".into() } else { commanders.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(" + ") }
    ));
    for c in &commanders {
        out.push_str(&format!("COMMANDER: {} | {} | {} | {}\n", c.name, c.mana_cost, c.type_line, c.oracle_text.replace('\n', " ")));
    }
    out.push_str(&format!(
        "STATS: average mana value {:.2}; curve (MV 0..7+): {:?}; lands {}, creatures {}, artifacts {}, enchantments {}, instants {}, sorceries {}, planeswalkers {}\n",
        a.avg_cmc, a.curve, a.types.lands, a.types.creatures, a.types.artifacts, a.types.enchantments, a.types.instants, a.types.sorceries, a.types.planeswalkers
    ));
    let r = &a.roles;
    out.push_str(&format!(
        "ROLES: ramp {}, card draw {}, tutors {}, removal {}, counterspells {}, board wipes {}, recursion {}, protection {}\n",
        r.ramp, r.draw, r.tutors, r.removal, r.counters, r.wipes, r.recursion, r.protection
    ));
    let colors = ['W', 'U', 'B', 'R', 'G'];
    let mana: Vec<String> = colors
        .iter()
        .enumerate()
        .filter(|(i, _)| a.pip_demand[*i] > 0 || a.direct_sources[*i] > 0)
        .map(|(i, c)| format!("{c}: {} pips / {} sources", a.pip_demand[i], a.direct_sources[i]))
        .collect();
    out.push_str(&format!("MANA: {}\n", if mana.is_empty() { "colorless".into() } else { mana.join("; ") }));
    out.push_str(&format!(
        "BRACKET: {} — {}. Game Changers: {}\n",
        a.bracket.display_name(),
        a.bracket.reasons.join(" "),
        if a.bracket.game_changers.is_empty() { "none".into() } else { a.bracket.game_changers.join(", ") }
    ));
    out.push_str(&format!("PRICE: ${:.2} (USD, Scryfall){}\n", a.total_price_usd, if a.unpriced_cards > 0 { format!(", {} cards unpriced", a.unpriced_cards) } else { String::new() }));
    out.push_str(&format!(
        "LEGALITY: {}\n",
        if a.legality_issues.is_empty() { "legal".into() } else { a.legality_issues.join("; ") }
    ));
    if !a.tags.is_empty() {
        out.push_str(&format!("THEMES: {}\n", a.tags.iter().take(6).map(|t| t.name.clone()).collect::<Vec<_>>().join(", ")));
    }
    if let Some(g) = &facts.guide {
        out.push_str(&format!("GAME PLAN: {}\n", g.summary));
        if !g.strengths.is_empty() {
            out.push_str(&format!("STRENGTHS: {}\n", g.strengths.join(" | ")));
        }
        if !g.weaknesses.is_empty() {
            out.push_str(&format!("WEAKNESSES: {}\n", g.weaknesses.join(" | ")));
        }
        for s in &g.sections {
            out.push_str(&format!("{}: {} {}\n", s.title.to_uppercase(), s.text, s.points.join(" ")));
        }
    }
    match &facts.combos {
        Some(c) if !c.variants.is_empty() => {
            out.push_str("COMBOS (Commander Spellbook):\n");
            for v in c.variants.iter().take(15) {
                out.push_str(&format!(
                    "- {}{} → {}\n",
                    v.cards.join(" + "),
                    if v.almost { " (one card missing)" } else { "" },
                    v.result
                ));
            }
        }
        Some(_) => out.push_str("COMBOS: Commander Spellbook found none.\n"),
        None => out.push_str("COMBOS: not checked yet (the Combos tab checks Commander Spellbook).\n"),
    }
    if !a.cut_candidates.is_empty() {
        out.push_str(&format!(
            "WEAKEST CARDS (Commander Lab's estimate): {}\n",
            a.cut_candidates.iter().take(8).map(|c| format!("{} ({})", c.card, c.reason)).collect::<Vec<_>>().join("; ")
        ));
    }
    out.push_str("CARDS (quantity | name | cost | type | roles | price | rules text):\n");
    for (q, c, _) in &facts.cards {
        let full = focus.iter().any(|f| f.name == c.name);
        let text = if c.is_basic { String::new() } else if full { c.oracle_text.replace('\n', " ") } else { one_line(&c.oracle_text, 170) };
        out.push_str(&format!(
            "{q} | {} | {} | {} | {} | {} | {}\n",
            c.name,
            c.mana_cost,
            c.type_line,
            role_names(classify_card(c)).join(", "),
            price_of(c),
            text
        ));
    }
    if !facts.sideboard.is_empty() {
        out.push_str(&format!("SIDEBOARD: {}\n", facts.sideboard.join(", ")));
    }
    if !facts.considering.is_empty() {
        out.push_str(&format!("CONSIDERING (maybeboard): {}\n", facts.considering.join(", ")));
    }
    out
}

const SYSTEM: &str = r#"You are Commander Lab's assistant for one Magic: The Gathering deck. The user can ask anything about it: what a card does or why it's in the deck, how cards interact, rules questions, how to pilot it, mulligans, what to do in the first turns, how it wins, its combos, matchups and weaknesses, its curve, mana, price or bracket, and what to cut or add.

How to answer:
- Answer the question that was asked, directly, first. Then add only the detail that helps.
- Ground every claim in DECK CONTEXT: the card list and rules text, stats, bracket, price, game plan and combos there are authoritative. Quote card names exactly.
- Never invent cards, rules text, prices, statistics or combos. If something isn't in the context and you aren't sure, say so.
- For rules questions, explain using the cards' actual text and the Magic comprehensive rules; say when a ruling is a judgment call.
- Suggest changes only when asked, or when it clearly answers the question. Name specific cards and why.
- Keep it short: a few sentences or a short list. Use plain language a newer player follows.
- If the user wants the full decklist, write it as:
Commander
1 Card Name

Deck
1 Card Name
"#;

/// Answer with the AI model. Errors if no model is reachable.
pub fn ai_answer(facts: &DeckFacts, history: &[(String, String)], question: &str, notes: &str) -> Result<String> {
    let ai = LocalAi::connect()?;
    let focus = mentioned(facts, question);
    let mut system = format!("{SYSTEM}\nDECK CONTEXT\n{}", context_text(facts, &focus));
    if !notes.trim().is_empty() {
        system.push_str(&format!("\nUSER NOTES ABOUT THIS DECK\n{notes}\n"));
    }
    let mut turns: Vec<(String, String)> = history.iter().rev().take(20).cloned().collect::<Vec<_>>().into_iter().rev().collect();
    turns.push(("user".into(), question.to_string()));
    Ok(ai.chat(&system, &turns)?.trim().to_string())
}

fn has_word(q: &str, words: &[&str]) -> bool {
    let tokens: Vec<&str> = q.split_whitespace().collect();
    words.iter().any(|w| if w.contains(' ') { q.contains(w) } else { tokens.iter().any(|t| t == w || t.trim_end_matches('s') == *w) })
}

fn card_blurb(facts: &DeckFacts, c: &Card) -> String {
    let roles = role_names(classify_card(c));
    let qty = facts.cards.iter().find(|(_, x, _)| x.name == c.name);
    let place = match qty {
        Some((_, _, true)) => "your commander".to_string(),
        Some((q, _, _)) if *q > 1 => format!("{q} copies in the deck"),
        Some(_) => "in the deck".to_string(),
        None => "not in the deck".to_string(),
    };
    let mut why = Vec::new();
    if !roles.is_empty() {
        why.push(format!("it's {}", roles.join(" and ")));
    }
    let themes = crate::archetype::tag_names_for_card(c);
    let deck_themes: Vec<String> = facts.analysis.tags.iter().take(6).map(|t| t.name.clone()).collect();
    let shared: Vec<String> = themes.into_iter().filter(|t| deck_themes.iter().any(|d| d.eq_ignore_ascii_case(t))).collect();
    if !shared.is_empty() {
        why.push(format!("it fits the deck's {} theme", shared.join(" / ")));
    }
    if let Some(combo) = facts.combos.as_ref().and_then(|r| r.variants.iter().find(|v| v.cards.iter().any(|n| n == &c.name))) {
        why.push(format!("it's part of a combo: {} → {}", combo.cards.join(" + "), combo.result));
    }
    if facts.analysis.bracket.game_changers.iter().any(|g| g == &c.name) {
        why.push("it's a Game Changer (counts toward the bracket)".into());
    }
    let cut = facts.analysis.cut_candidates.iter().find(|x| x.card == c.name);
    let mut s = format!(
        "**{}** {} — {} ({}, {})\n{}",
        c.name,
        c.mana_cost,
        c.type_line,
        place,
        price_of(c),
        c.oracle_text.trim()
    );
    if !why.is_empty() {
        s.push_str(&format!("\nWhy it's here: {}.", why.join("; ")));
    }
    if let Some(cut) = cut {
        s.push_str(&format!("\nIt's one of the weaker cards by Commander Lab's estimate: {}.", cut.reason));
    }
    s
}

fn section<'a>(facts: &'a DeckFacts, title: &str) -> Option<&'a crate::guide::GuideSection> {
    facts.guide.as_ref()?.sections.iter().find(|s| s.title.eq_ignore_ascii_case(title))
}

fn section_text(facts: &DeckFacts, title: &str) -> Option<String> {
    let s = section(facts, title)?;
    let mut out = s.text.clone();
    for p in &s.points {
        out.push_str(&format!("\n- {p}"));
    }
    Some(out)
}

/// Answer from Commander Lab's own analysis (no AI model).
pub fn offline_answer(facts: &DeckFacts, question: &str) -> String {
    let q = normalize(question);
    let a = &facts.analysis;
    let mut parts: Vec<String> = Vec::new();

    let cards = mentioned(facts, question);
    for c in cards.iter().take(3) {
        parts.push(card_blurb(facts, c));
    }
    if cards.len() >= 2 && has_word(&q, &["combo", "work together", "interact", "synergy", "with"]) {
        if let Some(r) = &facts.combos {
            let names: Vec<&str> = cards.iter().map(|c| c.name.as_str()).collect();
            if let Some(v) = r.variants.iter().find(|v| names.iter().all(|n| v.cards.iter().any(|x| x == n))) {
                let mut s = format!("**Combo:** {} → {}", v.cards.join(" + "), v.result);
                for (i, step) in v.steps.iter().enumerate().take(8) {
                    s.push_str(&format!("\n{}. {}", i + 1, step));
                }
                parts.push(s);
            }
        }
    }

    let mut topic = |cond: bool, text: Option<String>| {
        if cond {
            if let Some(t) = text.filter(|t| !t.trim().is_empty()) {
                parts.push(t);
            }
        }
    };

    topic(
        has_word(&q, &["mulligan", "keep", "opening hand", "opener", "starting hand"]),
        section_text(facts, "Opening hand").map(|t| format!("**Opening hand**\n{t}")),
    );
    topic(
        has_word(&q, &["early", "turn 1", "turn one", "first turn", "first turns", "early game"]),
        section_text(facts, "Early turns").map(|t| format!("**Early turns**\n{t}")),
    );
    let asks_plan = has_word(&q, &["win", "wincon", "finisher", "game plan", "gameplan", "strategy", "pilot", "how to play", "how do i play", "how should i play", "what does this deck do", "plan"]);
    topic(
        asks_plan,
        facts.guide.as_ref().map(|g| {
            let mut s = format!("**Game plan**\n{}", g.summary);
            if let Some(w) = section_text(facts, "How this deck wins") {
                s.push_str(&format!("\n\n**How it wins**\n{w}"));
            }
            s
        }),
    );
    topic(
        has_word(&q, &["combo", "infinite", "combos"]) && cards.len() < 2,
        Some(match &facts.combos {
            Some(r) if !r.variants.is_empty() => {
                let mut s = format!("**Combos** ({} in the deck, {} one card away)", r.included, r.almost_included);
                for v in r.variants.iter().take(8) {
                    s.push_str(&format!("\n- {}{} → {}", v.cards.join(" + "), if v.almost { " (missing one)" } else { "" }, v.result));
                }
                s
            }
            Some(_) => "Commander Spellbook found no combos in this deck.".into(),
            None => "Combos haven't been checked yet. Open the **Combos** tab to check Commander Spellbook; after that I can answer combo questions.".into(),
        }),
    );
    topic(
        has_word(&q, &["interaction", "answer", "answers", "removal", "deal with", "threat"]),
        Some({
            let mut s = format!(
                "**Interaction:** {} removal, {} counterspells, {} board wipes, {} protection.",
                a.roles.removal, a.roles.counters, a.roles.wipes, a.roles.protection
            );
            if let Some(t) = section_text(facts, "Interaction") {
                s.push_str(&format!("\n{t}"));
            }
            s
        }),
    );
    topic(
        has_word(&q, &["curve", "mana value", "cmc", "average", "mv", "expensive spells", "top heavy"]),
        Some(format!(
            "**Curve:** average mana value {:.2}. By mana value: {}.",
            a.avg_cmc,
            a.curve
                .iter()
                .enumerate()
                .filter(|(_, n)| **n > 0)
                .map(|(i, n)| format!("{}{}: {n}", i, if i == 7 { "+" } else { "" }))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    );
    topic(
        has_word(&q, &["land", "lands", "mana base", "manabase", "color", "colour", "sources", "fixing", "mana"]),
        Some({
            let colors = ['W', 'U', 'B', 'R', 'G'];
            let mana: Vec<String> = colors
                .iter()
                .enumerate()
                .filter(|(i, _)| a.pip_demand[*i] > 0 || a.direct_sources[*i] > 0)
                .map(|(i, c)| format!("{c} {} sources for {} symbols", a.direct_sources[i], a.pip_demand[i]))
                .collect();
            format!(
                "**Mana:** {} lands and {} ramp cards. {}",
                a.types.lands,
                a.roles.ramp,
                if mana.is_empty() { String::new() } else { format!("Colors: {}.", mana.join("; ")) }
            )
        }),
    );
    topic(
        has_word(&q, &["price", "cost", "costs", "expensive", "budget", "cheap", "money", "worth", "dollars"]),
        Some({
            let mut priced: Vec<&Card> = facts.cards.iter().map(|(_, c, _)| c).filter(|c| !c.is_basic && c.price_usd.is_some()).collect();
            priced.sort_by(|x, y| y.price_usd.partial_cmp(&x.price_usd).unwrap_or(std::cmp::Ordering::Equal));
            format!(
                "**Price:** about ${:.2} (Scryfall, USD). Most expensive: {}.",
                a.total_price_usd,
                priced.iter().take(5).map(|c| format!("{} ({})", c.name, price_of(c))).collect::<Vec<_>>().join(", ")
            )
        }),
    );
    topic(
        has_word(&q, &["bracket", "power", "power level", "game changer", "game changers", "competitive", "casual"]),
        Some(format!(
            "**Bracket:** {}. {}{}",
            a.bracket.display_name(),
            a.bracket.reasons.join(" "),
            if a.bracket.game_changers.is_empty() { String::new() } else { format!(" Game Changers: {}.", a.bracket.game_changers.join(", ")) }
        )),
    );
    topic(
        has_word(&q, &["ramp", "draw", "card advantage", "tutor", "tutors", "wipe", "wipes", "counterspell", "counterspells", "protection", "recursion", "how many"]),
        Some(format!(
            "**Counts:** {} ramp, {} card draw, {} tutors, {} removal, {} counterspells, {} board wipes, {} recursion, {} protection, {} lands, {} creatures.",
            a.roles.ramp, a.roles.draw, a.roles.tutors, a.roles.removal, a.roles.counters, a.roles.wipes, a.roles.recursion, a.roles.protection, a.types.lands, a.types.creatures
        )),
    );
    topic(
        has_word(&q, &["weak", "weakest", "cut", "cuts", "worst", "remove", "replace", "swap out"]) && cards.is_empty(),
        Some(if a.cut_candidates.is_empty() {
            "Nothing stands out as weak by Commander Lab's estimate.".into()
        } else {
            format!(
                "**Weakest cards** (Commander Lab's estimate):\n{}",
                a.cut_candidates.iter().take(6).map(|c| format!("- {}: {}", c.card, c.reason)).collect::<Vec<_>>().join("\n")
            )
        }),
    );
    topic(
        has_word(&q, &["add", "improve", "upgrade", "upgrades", "better", "missing", "need", "suggest", "suggestions"]) && cards.is_empty(),
        Some({
            let mut s = String::from("**To improve it:**");
            for r in a.recommendations.iter().take(4) {
                s.push_str(&format!("\n- {r}"));
            }
            if !a.add_suggestions.is_empty() {
                s.push_str(&format!(
                    "\nCards to look at: {}.",
                    a.add_suggestions.iter().take(8).map(|x| format!("{} ({})", x.card, x.role)).collect::<Vec<_>>().join(", ")
                ));
            }
            s.push_str("\nThe **Suggestions** tab has more, with images, and **Search** finds anything else.");
            s
        }),
    );
    topic(
        has_word(&q, &["strength", "strengths", "weakness", "weaknesses", "good at", "bad at", "struggle", "matchup", "against"]),
        facts.guide.as_ref().map(|g| {
            format!(
                "**Strengths:** {}\n**Weaknesses:** {}",
                if g.strengths.is_empty() { "—".into() } else { g.strengths.join(" ") },
                if g.weaknesses.is_empty() { "—".into() } else { g.weaknesses.join(" ") }
            )
        }),
    );
    topic(
        has_word(&q, &["legal", "legality", "banned", "allowed"]),
        Some(if a.legality_issues.is_empty() {
            format!("The deck is legal in {}.", facts.deck.format.label())
        } else {
            format!("**Legality problems:**\n{}", a.legality_issues.iter().map(|i| format!("- {i}")).collect::<Vec<_>>().join("\n"))
        }),
    );

    if parts.is_empty() {
        let mut s = String::new();
        if let Some(g) = &facts.guide {
            s.push_str(&g.summary);
            s.push_str("\n\n");
        }
        s.push_str(&format!(
            "{} cards, {} lands, average mana value {:.2}, {}, about ${:.2}.\n\nI can answer about any card in the deck (name it), the curve, mana, price, bracket, combos, mulligans, early turns, how it wins, interaction, and what to cut or add.",
            a.total_cards,
            a.types.lands,
            a.avg_cmc,
            a.bracket.display_name(),
            a.total_price_usd
        ));
        parts.push(s);
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_match_plurals_and_phrases() {
        assert!(has_word("how many tutors does it run", &["tutor"]));
        assert!(has_word("how should i play this", &["how should i play"]));
        assert!(!has_word("what about removal", &["remove"]));
        assert_eq!(normalize("Atraxa, Praetors' Voice!"), "atraxa praetors' voice");
    }
}
