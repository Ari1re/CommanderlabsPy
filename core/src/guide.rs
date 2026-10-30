//! "How to play this deck": a deck-specific guide built from the actual
//! cards, the analysis, known combos and a quick draw simulation.

use crate::{
    analysis::{classify_card, DeckAnalysis},
    data::{Card, CardDb},
    deck::ParsedDeck,
    generator::GamePlan,
    simulation::run_draw_simulation,
    spellbook::ComboReport,
};
use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct GuideSection {
    pub title: String,
    pub text: String,
    pub points: Vec<String>,
    /// Cards this section refers to, so the UI can show them.
    pub cards: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayGuide {
    pub summary: String,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
    pub sections: Vec<GuideSection>,
}

fn names(cards: &[&Card], limit: usize) -> Vec<String> {
    cards.iter().take(limit).map(|c| c.name.clone()).collect()
}

fn list(cards: &[String]) -> String {
    match cards.len() {
        0 => String::new(),
        1 => cards[0].clone(),
        2 => format!("{} and {}", cards[0], cards[1]),
        _ => format!("{}, and {}", cards[..cards.len() - 1].join(", "), cards[cards.len() - 1]),
    }
}

fn is_land(c: &Card) -> bool {
    c.type_line.split("//").next().unwrap_or("").to_ascii_lowercase().contains("land")
}

pub fn build_guide(
    deck: &ParsedDeck,
    db: &CardDb,
    analysis: &DeckAnalysis,
    combos: Option<&ComboReport>,
    plan: Option<&GamePlan>,
) -> Result<PlayGuide> {
    let commanders: Vec<Card> = deck.commanders.iter().filter_map(|e| db.get_card(&e.name).ok().flatten()).collect();
    let cards: Vec<Card> = deck.main.iter().filter_map(|e| db.get_card(&e.name).ok().flatten()).collect();
    let by_role = |pred: &dyn Fn(&Card) -> bool| -> Vec<&Card> {
        let mut v: Vec<&Card> = cards.iter().filter(|c| pred(c)).collect();
        v.sort_by(|a, b| a.cmc.partial_cmp(&b.cmc).unwrap_or(std::cmp::Ordering::Equal).then(a.name.cmp(&b.name)));
        v
    };

    let ramp = by_role(&|c| classify_card(c).ramp && !is_land(c));
    let early_ramp: Vec<&Card> = ramp.iter().copied().filter(|c| c.cmc <= 3.0).collect();
    let draw = by_role(&|c| classify_card(c).draw);
    let tutors = by_role(&|c| classify_card(c).tutor);
    let removal = by_role(&|c| classify_card(c).removal);
    let counters = by_role(&|c| classify_card(c).counter);
    let wipes = by_role(&|c| classify_card(c).wipe);
    let protection = by_role(&|c| classify_card(c).protection);
    let recursion = by_role(&|c| classify_card(c).recursion);

    let commander_names: Vec<String> = commanders.iter().map(|c| c.name.clone()).collect();
    let themes: Vec<String> = analysis.tags.iter().take(3).map(|t| t.name.clone()).collect();
    let lands = analysis.types.lands;

    let sim = run_draw_simulation(deck, db, 10_000).ok();

    // --- Summary -----------------------------------------------------------
    let summary = match plan.map(|p| p.summary.trim()).filter(|s| !s.is_empty()) {
        Some(s) => s.to_string(),
        None => format!(
            "{} leads a {} deck{}. Average mana value {:.2} with {} lands.",
            if commander_names.is_empty() { "Your commander".to_string() } else { list(&commander_names) },
            analysis.bracket.label.to_lowercase(),
            if themes.is_empty() { String::new() } else { format!(" built around {}", list(&themes).to_lowercase()) },
            analysis.avg_cmc,
            lands
        ),
    };

    let mut sections = Vec::new();

    // --- Opening hand ----------------------------------------------------
    let mut points = vec![
        "Keep hands with 3 or 4 lands, or 2 lands plus a cheap ramp spell.".to_string(),
        "Mulligan 0–1 land hands and hands of 6+ lands unless they hold card draw.".to_string(),
    ];
    if let Some(s) = &sim {
        points.push(format!(
            "In 10,000 simulated openers, {:.0}% were keepable and you averaged {:.1} lands.",
            s.keepable_opening_pct, s.average_opening_lands
        ));
    }
    if !early_ramp.is_empty() {
        points.push(format!("Best ramp to keep: {}.", list(&names(&early_ramp, 4))));
    }
    sections.push(GuideSection {
        title: "Opening hand".into(),
        text: format!("You run {lands} lands and {} ramp spell{}.", ramp.len(), if ramp.len() == 1 { "" } else { "s" }),
        points,
        cards: names(&early_ramp, 6),
    });

    // --- Early turns ------------------------------------------------------
    let commander_cmc = commanders.iter().map(|c| c.cmc).fold(0.0, f64::max);
    let mut points = Vec::new();
    if !early_ramp.is_empty() {
        points.push("Turns 1–3: play lands every turn and spend early mana on ramp before threats.".into());
    }
    if commander_cmc > 0.0 {
        points.push(format!(
            "Plan to cast {} around turn {}{}.",
            if commander_names.len() == 1 { commander_names[0].clone() } else { "your commanders".into() },
            ((commander_cmc as i64) - (early_ramp.len().min(2) as i64)).max(2),
            if early_ramp.is_empty() { "" } else { " with a ramp spell" }
        ));
    }
    points.push("Every time your commander dies it costs 2 more — recast it only when it will survive or win.".into());
    sections.push(GuideSection {
        title: "Early turns".into(),
        text: "Set up mana first; the deck's best turns come once you can cast two spells a turn.".into(),
        points,
        cards: names(&ramp, 8),
    });

    // --- Card advantage --------------------------------------------------
    if !draw.is_empty() {
        sections.push(GuideSection {
            title: "Keeping your hand full".into(),
            text: format!("{} cards draw or generate card advantage.", draw.len()),
            points: vec![
                "Repeatable draw engines are worth more than one-shot draw: deploy them early.".into(),
                "Use one-shot draw at the end of an opponent's turn when you can.".into(),
            ],
            cards: names(&draw, 10),
        });
    }

    // --- Winning ----------------------------------------------------------
    let mut points = Vec::new();
    let mut win_cards = Vec::new();
    if let Some(report) = combos {
        for line in report.variants.iter().filter(|v| !v.almost).take(4) {
            points.push(format!("Combo: {} — {}", line.cards.join(" + "), line.result.lines().next().unwrap_or("")));
            win_cards.extend(line.cards.iter().cloned());
        }
    } else if let Some(plan) = plan {
        for combo in plan.combos.iter().take(4) {
            points.push(format!("Combo: {} — {}", combo.cards.join(" + "), combo.result));
            win_cards.extend(combo.cards.iter().cloned());
        }
    }
    let finishers: Vec<&Card> = {
        let mut v: Vec<&Card> = cards
            .iter()
            .filter(|c| {
                let o = c.oracle_text.to_ascii_lowercase();
                o.contains("each opponent loses")
                    || o.contains("you win the game")
                    || o.contains("damage to each opponent")
                    || o.contains("creatures you control get +")
                    || o.contains("extra combat")
            })
            .collect();
        v.sort_by(|a, b| b.cmc.partial_cmp(&a.cmc).unwrap_or(std::cmp::Ordering::Equal));
        v
    };
    if !finishers.is_empty() {
        points.push(format!("Finishers: {}.", list(&names(&finishers, 4))));
        win_cards.extend(names(&finishers, 4));
    }
    if !tutors.is_empty() {
        points.push(format!(
            "Your {} tutor{} ({}) should usually find {}.",
            tutors.len(),
            if tutors.len() == 1 { "" } else { "s" },
            list(&names(&tutors, 3)),
            if win_cards.is_empty() { "your best engine or answer" } else { "the missing combo piece or finisher" }
        ));
    }
    if points.is_empty() {
        points.push("No combos or dedicated finishers were found. Win by building a board and attacking; check the Combos tab and suggested adds for a clearer win condition.".into());
    }
    win_cards.sort();
    win_cards.dedup();
    sections.push(GuideSection {
        title: "How this deck wins".into(),
        text: if combos.is_none() && plan.map(|p| p.combos.is_empty()).unwrap_or(true) {
            "Run Combos to include Commander Spellbook lines here.".into()
        } else {
            String::new()
        },
        points,
        cards: win_cards,
    });

    // --- Interaction ------------------------------------------------------
    let interaction: Vec<&Card> = removal.iter().chain(counters.iter()).chain(wipes.iter()).copied().collect();
    let mut points = Vec::new();
    if !removal.is_empty() {
        points.push(format!("Spot removal ({}): save it for engines and commanders that threaten to win, not the first creature you see.", removal.len()));
    }
    if !counters.is_empty() {
        points.push(format!("Counterspells ({}): hold mana up for combo pieces, wipes aimed at you, and game-winning spells.", counters.len()));
    }
    if !wipes.is_empty() {
        points.push(format!("Board wipes ({}): cast them when you're behind on board, ideally right after deploying a commander you can recast.", wipes.len()));
    }
    if !protection.is_empty() {
        points.push(format!("Protect key pieces with {}.", list(&names(&protection, 3))));
    }
    if !recursion.is_empty() {
        points.push(format!("Recursion ({}) brings back what gets answered.", list(&names(&recursion, 3))));
    }
    if !points.is_empty() {
        sections.push(GuideSection {
            title: "Interaction".into(),
            text: format!("{} interactive spells.", interaction.len()),
            points,
            cards: names(&interaction, 12),
        });
    }

    // --- Strengths & weaknesses ------------------------------------------
    let mut strengths = Vec::new();
    let mut weaknesses = Vec::new();
    let r = &analysis.roles;
    if r.ramp >= 10 { strengths.push("Plenty of ramp to accelerate past opponents.".into()) } else if r.ramp < 7 { weaknesses.push(format!("Only {} ramp spell{} — aim for 8–12 so you don't fall behind.", r.ramp, if r.ramp == 1 { "" } else { "s" })) }
    if r.draw >= 10 { strengths.push("Strong card advantage keeps you from running out of gas.".into()) } else if r.draw < 7 { weaknesses.push(format!("Only {} card draw source{} — the deck may run out of cards.", r.draw, if r.draw == 1 { "" } else { "s" })) }
    let interaction_count = r.removal + r.counters + r.wipes;
    if interaction_count >= 10 { strengths.push("Lots of interaction to stop opponents.".into()) } else if interaction_count < 7 { weaknesses.push(format!("Only {interaction_count} interaction spell{} — add removal so you can answer threats.", if interaction_count == 1 { "" } else { "s" })) }
    if r.tutors >= 4 { strengths.push("Tutors make the deck consistent.".into()) }
    if analysis.avg_cmc > 3.6 { weaknesses.push(format!("High average mana value ({:.2}); lower the curve or add ramp.", analysis.avg_cmc)) } else if analysis.avg_cmc < 2.8 { strengths.push("Low curve lets you cast multiple spells a turn.".into()) }
    if lands < 32 && r.ramp < 12 { weaknesses.push(format!("{lands} lands is light without heavy ramp.")) }
    if lands > 40 { weaknesses.push(format!("{lands} lands risks flooding.")) }
    let colors = ['W', 'U', 'B', 'R', 'G'];
    for (i, c) in colors.iter().enumerate() {
        let demand = analysis.pip_demand[i];
        let sources = analysis.direct_sources[i];
        if demand >= 10 && (sources as f64) < (demand as f64) * 0.55 {
            weaknesses.push(format!(
                "{} sources look thin: {sources} sources for {demand} {} symbols.",
                match c { 'W' => "White", 'U' => "Blue", 'B' => "Black", 'R' => "Red", _ => "Green" },
                match c { 'W' => "white", 'U' => "blue", 'B' => "black", 'R' => "red", _ => "green" },
            ));
        }
    }
    if let Some(report) = combos {
        if report.included > 0 {
            strengths.push(format!("{} complete combo{} in the deck.", report.included, if report.included == 1 { "" } else { "s" }));
        }
    }
    for issue in analysis.legality_issues.iter().take(3) {
        weaknesses.push(issue.clone());
    }

    Ok(PlayGuide { summary, strengths, weaknesses, sections })
}

#[cfg(test)]
mod tests {
    use super::list;

    #[test]
    fn lists_read_naturally() {
        assert_eq!(list(&["A".into()]), "A");
        assert_eq!(list(&["A".into(), "B".into()]), "A and B");
        assert_eq!(list(&["A".into(), "B".into(), "C".into()]), "A, B, and C");
    }
}
