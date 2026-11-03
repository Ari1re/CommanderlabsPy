//! Task playbooks: step-by-step instructions that tell the AI model how to do
//! each kind of request, instead of relying on a model tuned for Magic.
//!
//! How it fits: `deck_chat::ai_answer` asks [`route`] which playbook fits the
//! question ("make it cheaper" → Budget swaps). [`gather_data`] runs that
//! playbook's tools against Commander Lab's own data (analysis, bracket,
//! combos, the card database), and [`prompt`] turns the playbook plus that
//! data into a short, structured brief. Small on-device models do far better
//! with this than with the whole deck and an open question. After the
//! model answers, [`check_answer`] checks every ADD/CUT line against the card
//! database and the deck's format and colours, and drops the rest.
//!
//! Adding a playbook: add a [`Playbook`] to [`PLAYBOOKS`] with trigger words,
//! a goal, steps (each a [`Tool`] plus what the model should do with its
//! data), rules and the answer format. If it needs data no tool gives yet,
//! add a [`Tool`] variant and its section in [`gather_data`]. Then add a
//! routing test at the bottom of this file. Order matters: on a tie, the
//! earlier playbook wins, so keep specific ones above general ones.
//!
//! Things to work on:
//! - Routing is keyword scoring; questions that mix tasks ("cheaper and
//!   faster") get one playbook. Running two playbooks' tools would cover them.
//! - No rulings data: rules questions lean on the model's own knowledge of
//!   the comprehensive rules. Scryfall's rulings endpoint would ground them.

use crate::{
    analysis::{classify_card, identity_subset},
    data::{Card, CardDb},
    deck_chat::{has_word, normalize, one_line, price_of, role_names, DeckFacts},
};
use std::collections::HashSet;

/// One piece of data a playbook step asks Commander Lab for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// Commander, format, size, bracket, themes, price, curve and role counts.
    Overview,
    /// Full text and roles of the cards named in the question (in the deck
    /// or not).
    MentionedCards,
    /// Combos the deck has, and ones it's one card away from.
    Combos,
    /// Combos the deck could add (pieces it doesn't run yet).
    PossibleCombos,
    /// Bracket estimate with its reasons and counts.
    Bracket,
    /// What pushes the deck's power up: Game Changers, fast mana, tutors,
    /// extra turns, high-bracket combos.
    PowerCards,
    /// Role counts against the usual targets.
    RoleGaps,
    /// Commander Lab's weakest-card estimate.
    CutCandidates,
    /// Legal, on-colour cards not in the deck that fill the weakest roles.
    AddCandidates,
    /// Lands, coloured pips against sources, fixing.
    ManaBase,
    /// The priciest cards and the total.
    Prices,
    /// Cheaper legal cards sharing a role with the priciest ones.
    BudgetSwaps,
    /// The play guide: plan, mulligans, early turns, how it wins.
    GamePlan,
    /// One line per card: cost, type, roles, price.
    CardList,
}

/// One step of a playbook: the data to fetch and what to do with it.
pub struct Step {
    pub tool: Tool,
    /// Shown in the Agent tab while the step runs.
    pub label: &'static str,
    /// What the model should do with this step's data.
    pub instruction: &'static str,
}

/// How to do one kind of request.
pub struct Playbook {
    pub id: &'static str,
    pub title: &'static str,
    /// Words or phrases that route a question here (see [`route`]).
    pub triggers: &'static [&'static str],
    pub goal: &'static str,
    pub steps: &'static [Step],
    pub rules: &'static [&'static str],
    pub output: &'static str,
}

/// Rules every playbook adds to its own.
const COMMON_RULES: &[&str] = &[
    "Use only the DATA below for card text, prices, counts, combos and the bracket. It comes from Commander Lab and is correct.",
    "Never invent card names, rules text, prices or combos. Quote card names exactly as written in DATA.",
    "Answer the question first, in plain language, then only the detail that helps.",
];

/// Answer format for any playbook that suggests cards. [`check_answer`]
/// reads these lines.
const CARD_LINES: &str = "Write each suggested change on its own line exactly like this:\nADD: Card Name — why\nCUT: Card Name — why\nOnly cut cards listed in the deck. Only add cards from the candidates in DATA unless you are certain the card exists, is legal in this format and fits the colours.";

pub const PLAYBOOKS: &[Playbook] = &[
    Playbook {
        id: "power_down",
        title: "Lower the power level",
        triggers: &["lower bracket", "tone it down", "down to bracket", "lower", "power down", "weaker", "less powerful", "tone down", "too strong", "casual", "bracket down", "nerf", "downgrade"],
        goal: "Bring the deck down to the requested power level while keeping its theme and how it wins.",
        steps: &[
            Step { tool: Tool::Bracket, label: "Checking the bracket", instruction: "Note the current bracket and what puts it there." },
            Step { tool: Tool::PowerCards, label: "Finding the high-power cards", instruction: "These are the cards to cut first: Game Changers, fast mana, cheap tutors, extra turns and high-bracket combo pieces." },
            Step { tool: Tool::AddCandidates, label: "Finding fair replacements", instruction: "Replace each cut with a candidate that does a similar job at a fairer power level." },
        ],
        rules: &["Pair every CUT with an ADD so the deck stays at its size.", "Keep the commander's theme and at least one way to win."],
        output: CARD_LINES,
    },
    Playbook {
        id: "upgrade",
        title: "Upgrade the deck",
        triggers: &["upgrade", "improve", "stronger", "better", "more powerful", "optimize", "optimise", "power up", "bracket up", "higher bracket", "tune", "what should i add", "what to add"],
        goal: "Make the deck stronger at its current bracket (or the one asked for) by fixing its weakest roles and cutting its weakest cards.",
        steps: &[
            Step { tool: Tool::Overview, label: "Reading the deck", instruction: "Note the commander, themes, bracket and curve." },
            Step { tool: Tool::RoleGaps, label: "Checking roles", instruction: "Find the roles that are below target; those come first." },
            Step { tool: Tool::CutCandidates, label: "Finding the weakest cards", instruction: "Choose cuts from this list." },
            Step { tool: Tool::AddCandidates, label: "Finding candidates", instruction: "Choose adds from these candidates, matching the missing roles and the themes." },
            Step { tool: Tool::Combos, label: "Checking combos", instruction: "If a combo is one card away and fits the bracket, its missing piece is a strong add." },
        ],
        rules: &["Suggest 3 to 8 changes. Pair every CUT with an ADD.", "Respect the bracket: no Game Changers past the bracket's limit, no fast combos below bracket 4."],
        output: CARD_LINES,
    },
    Playbook {
        id: "budget",
        title: "Budget swaps",
        triggers: &["budget", "cheaper", "cheap", "expensive", "price", "cost less", "save money", "costs", "how much", "afford", "dollars"],
        goal: "Lower the deck's price while keeping each expensive card's job.",
        steps: &[
            Step { tool: Tool::Prices, label: "Pricing the deck", instruction: "Start from the priciest cards." },
            Step { tool: Tool::BudgetSwaps, label: "Finding cheaper cards", instruction: "For each pricey card, pick a cheaper candidate that has the same role." },
        ],
        rules: &["Give the price of each card you cut and add, from DATA.", "If the question only asks what the deck costs, answer that and skip the swaps."],
        output: CARD_LINES,
    },
    Playbook {
        id: "mana",
        title: "Fix the mana base",
        triggers: &["mana base", "manabase", "lands", "land count", "fixing", "colour screw", "color screw", "mana screw", "flood", "color sources", "colour sources", "mana", "ramp"],
        goal: "Check the lands and ramp against what the spells need, and fix the gaps.",
        steps: &[
            Step { tool: Tool::Overview, label: "Reading the deck", instruction: "Note the curve and the land count." },
            Step { tool: Tool::ManaBase, label: "Counting sources", instruction: "Compare each colour's pips with its sources; a colour with many pips and few sources needs fixing." },
            Step { tool: Tool::RoleGaps, label: "Checking ramp", instruction: "Ramp below target is part of the mana problem." },
            Step { tool: Tool::AddCandidates, label: "Finding candidates", instruction: "Use these for ramp or fixing adds." },
        ],
        rules: &["Usual Commander targets: 35–38 lands with about 10 ramp; fewer lands for a low curve, more for a high one.", "Suggest land swaps only when a colour is short of sources."],
        output: CARD_LINES,
    },
    Playbook {
        id: "combos",
        title: "Combos",
        triggers: &["combo", "infinite", "combos", "win con", "wincon", "win condition", "how does it win", "interaction between"],
        goal: "Explain the deck's combos: the pieces, the steps and what they do; and which ones it could add.",
        steps: &[
            Step { tool: Tool::Combos, label: "Checking Commander Spellbook", instruction: "List complete combos first, then the ones a card away." },
            Step { tool: Tool::PossibleCombos, label: "Finding combos to add", instruction: "Mention these only if the question asks what could be added." },
            Step { tool: Tool::Bracket, label: "Checking the bracket", instruction: "Say if a combo pushes the deck past its bracket." },
        ],
        rules: &["Only describe combos listed in DATA. If none were found, say so and suggest running the Combos tab."],
        output: "A short list: the pieces, then what they do, in one or two lines each. Use ADD lines only for missing pieces the user asked to add.",
    },
    Playbook {
        id: "power",
        title: "Power level check",
        triggers: &["bracket", "power level", "how strong", "how powerful", "game changer", "game changers", "competitive", "cedh"],
        goal: "Say how strong the deck is and why.",
        steps: &[
            Step { tool: Tool::Bracket, label: "Checking the bracket", instruction: "Give the bracket and its main reasons." },
            Step { tool: Tool::PowerCards, label: "Finding the high-power cards", instruction: "Name the cards that set the bracket." },
            Step { tool: Tool::Combos, label: "Checking combos", instruction: "Mention combos that affect the bracket." },
        ],
        rules: &["Bracket 1 Exhibition, 2 Core, 3 Upgraded (up to 3 Game Changers, no early 2-card combos), 4 Optimized, 5 cEDH."],
        output: "Two to five sentences, or a short list. No card changes unless asked.",
    },
    Playbook {
        id: "cuts",
        title: "What to cut",
        triggers: &["cut", "cuts", "remove", "too many cards", "over 100", "trim", "weakest", "worst card", "take out"],
        goal: "Choose the cards to cut, weakest first, without breaking the deck's roles.",
        steps: &[
            Step { tool: Tool::Overview, label: "Reading the deck", instruction: "Note the size and themes." },
            Step { tool: Tool::CutCandidates, label: "Finding the weakest cards", instruction: "Cut from this list first." },
            Step { tool: Tool::RoleGaps, label: "Checking roles", instruction: "Don't cut a card whose role is already below target." },
        ],
        rules: &["Cut only cards in the deck. Give as many cuts as asked (default 5)."],
        output: CARD_LINES,
    },
    Playbook {
        id: "explain",
        title: "Explain cards and rules",
        // Question stems only: "how can I make it cheaper" must still go to
        // Budget, so nothing as broad as "can i" belongs here.
        triggers: &["what does", "how does", "explain", "rules", "ruling", "interact", "interaction", "work with", "why is", "why does", "what is"],
        goal: "Explain what the named cards do, how they interact, and why they're in this deck.",
        steps: &[
            Step { tool: Tool::MentionedCards, label: "Reading the cards", instruction: "Base the explanation on this exact rules text." },
            Step { tool: Tool::Overview, label: "Reading the deck", instruction: "Use the themes to say why the card fits." },
        ],
        rules: &["For rules questions, follow the cards' text and the Magic comprehensive rules; say when something is a judgment call.", "If no card in DATA matches the question, say which card you need."],
        output: "A direct answer in a few sentences, then the steps if it's an interaction.",
    },
    Playbook {
        id: "play",
        title: "How to play it",
        triggers: &["mulligan", "how to play", "pilot", "opening hand", "keep", "first turns", "early game", "game plan", "strategy", "matchup", "play this"],
        goal: "Explain how to pilot the deck: what to keep, the first turns and how it wins.",
        steps: &[
            Step { tool: Tool::GamePlan, label: "Reading the play guide", instruction: "Follow the guide; it was built from this exact list." },
            Step { tool: Tool::Overview, label: "Reading the deck", instruction: "Use the curve and roles for specifics." },
        ],
        rules: &["Name real cards from the deck as examples."],
        output: "A short list of practical tips.",
    },
    Playbook {
        id: "general",
        title: "General question",
        triggers: &[],
        goal: "Answer the question about this deck.",
        steps: &[
            Step { tool: Tool::Overview, label: "Reading the deck", instruction: "The deck's summary." },
            Step { tool: Tool::MentionedCards, label: "Reading the cards you mentioned", instruction: "Use this text for any card named in the question." },
            Step { tool: Tool::CardList, label: "Listing the cards", instruction: "Every card in the deck." },
        ],
        rules: &["Suggest changes only when asked."],
        output: "A few sentences or a short list. If you suggest changes, use ADD:/CUT: lines.",
    },
];

/// The fallback playbook (always the last one).
pub fn general() -> &'static Playbook {
    PLAYBOOKS.last().unwrap()
}

/// How well a question matches a playbook: 2 per matched phrase (several
/// words), 1 per matched word.
fn score(playbook: &Playbook, question: &str) -> u32 {
    playbook
        .triggers
        .iter()
        .filter(|t| has_word(question, &[t]))
        .map(|t| if t.contains(' ') { 2 } else { 1 })
        .sum()
}

/// The playbooks a question matches, best first, with their scores. Empty
/// when nothing matches (use [`general`]).
pub fn ranked(question: &str) -> Vec<(&'static Playbook, u32)> {
    let q = normalize(question);
    let mut hits: Vec<(&Playbook, u32)> = PLAYBOOKS.iter().map(|p| (p, score(p, &q))).filter(|(_, s)| *s > 0).collect();
    // Stable sort: on a tie, the playbook listed first wins.
    hits.sort_by(|a, b| b.1.cmp(&a.1));
    hits
}

/// The best playbook for a question, or [`general`].
pub fn route(question: &str) -> &'static Playbook {
    ranked(question).first().map(|(p, _)| *p).unwrap_or_else(general)
}

/// Is routing unsure: two playbooks tied at the top? Then `deck_chat` lets
/// the model choose between them.
pub fn is_tied(ranked: &[(&'static Playbook, u32)]) -> bool {
    ranked.len() >= 2 && ranked[0].1 == ranked[1].1
}

/// The short prompt that asks the model to pick between tied playbooks.
pub fn choice_prompt(question: &str, options: &[&'static Playbook]) -> String {
    let list: Vec<String> = options.iter().map(|p| format!("{}: {}", p.id, p.goal)).collect();
    format!(
        "Which task does this request ask for? Reply with only the id.\nRequest: {question}\nTasks:\n{}",
        list.join("\n")
    )
}

/// Read the model's pick: the first playbook id in its reply.
pub fn parse_choice(reply: &str, options: &[&'static Playbook]) -> Option<&'static Playbook> {
    let reply = reply.to_ascii_lowercase();
    options.iter().copied().filter_map(|p| reply.find(p.id).map(|i| (i, p))).min_by_key(|(i, _)| *i).map(|(_, p)| p)
}

// ---------------------------------------------------------------------------
// Tools: Commander Lab's data for each step
// ---------------------------------------------------------------------------

/// Usual role targets for a 100-card Commander deck: (role, target).
const ROLE_TARGETS: &[(&str, u32)] = &[("ramp", 10), ("card draw", 10), ("removal", 8), ("board wipes", 2), ("protection", 3), ("recursion", 2)];

/// The deck's count for one of [`ROLE_TARGETS`].
fn role_count(facts: &DeckFacts, role: &str) -> u32 {
    let r = &facts.analysis.roles;
    match role {
        "ramp" => r.ramp,
        "card draw" => r.draw,
        "removal" => r.removal + r.counters,
        "board wipes" => r.wipes,
        "protection" => r.protection,
        _ => r.recursion,
    }
}

/// Roles below target, biggest gap first.
fn role_gaps(facts: &DeckFacts) -> Vec<(&'static str, u32, u32)> {
    let mut gaps: Vec<(&str, u32, u32)> = ROLE_TARGETS
        .iter()
        .map(|(role, target)| (*role, role_count(facts, role), *target))
        .filter(|(_, have, target)| have < target)
        .collect();
    gaps.sort_by_key(|(_, have, target)| std::cmp::Reverse(target - have));
    gaps
}

/// Does a card have this role (as named in [`ROLE_TARGETS`])?
fn has_role(card: &Card, role: &str) -> bool {
    let r = classify_card(card);
    match role {
        "ramp" => r.ramp,
        "card draw" => r.draw,
        "removal" => r.removal || r.counter,
        "board wipes" => r.wipe,
        "protection" => r.protection,
        _ => r.recursion,
    }
}

/// Lower-case names of everything in the deck (all zones), so candidates
/// never repeat a card the player already owns in this list.
fn deck_names(facts: &DeckFacts) -> HashSet<String> {
    let d = &facts.deck;
    d.commanders
        .iter()
        .chain(d.main.iter())
        .chain(d.sideboard.iter())
        .chain(d.considering.iter())
        .map(|e| e.name.to_ascii_lowercase())
        .chain(facts.cards.iter().map(|(_, c, _)| c.name.to_ascii_lowercase()))
        .collect()
}

/// May this card go in the deck: legal in its format and inside the
/// commander's colours (Highlander has no commander, so any colour)?
fn fits_deck(facts: &DeckFacts, card: &Card) -> bool {
    let format = facts.deck.format;
    format.card_allowed(card).is_ok() && (!format.has_commander() || identity_subset(&card.color_identity, &facts.analysis.identity))
}

/// Popular legal cards not in the deck, best EDHREC rank first, that pass
/// `keep`. Reads the top 4,000 cards, so it stays fast.
fn candidates(facts: &DeckFacts, db: &CardDb, limit: usize, keep: impl Fn(&Card) -> bool) -> Vec<Card> {
    let owned = deck_names(facts);
    db.top_ranked_cards(4_000)
        .unwrap_or_default()
        .into_iter()
        .filter(|c| !c.is_basic && !owned.contains(&c.name.to_ascii_lowercase()) && fits_deck(facts, c) && keep(c))
        .take(limit)
        .collect()
}

/// One line describing a candidate card for the model.
fn card_line(c: &Card) -> String {
    format!(
        "{} | {} | {} | {} | {} | {}",
        c.name,
        c.mana_cost,
        c.type_line,
        role_names(classify_card(c)).join(", "),
        price_of(c),
        one_line(&c.oracle_text, 140)
    )
}

/// Cards named in the question: from the deck first, then any card in the
/// database whose full name appears in it (for "should I add Rhystic
/// Study?").
pub fn mentioned_cards(facts: &DeckFacts, db: &CardDb, question: &str) -> Vec<Card> {
    let mut found = crate::deck_chat::mentioned(facts, question);
    let words: Vec<&str> = question.split(|c: char| c.is_whitespace() || c == '?' || c == '"').filter(|w| !w.is_empty()).collect();
    // Longest spans first, so "Sol Ring" wins over "Ring".
    for len in (1..=6).rev() {
        for start in 0..words.len().saturating_sub(len - 1) {
            let span = words[start..start + len].join(" ");
            let span = span.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'');
            if span.len() < 4 || found.len() >= 6 {
                continue;
            }
            if let Ok(Some(card)) = db.get_card(span) {
                if card.name.eq_ignore_ascii_case(span) && !found.iter().any(|c| c.name == card.name) {
                    found.push(card);
                }
            }
        }
    }
    found
}

/// Run one tool and write its data section.
fn run_tool(tool: Tool, facts: &DeckFacts, db: &CardDb, question: &str) -> String {
    let a = &facts.analysis;
    let mut out = String::new();
    let mut line = |s: String| {
        out.push_str(&s);
        out.push('\n');
    };
    match tool {
        Tool::Overview => {
            let commanders: Vec<&str> = facts.cards.iter().filter(|(_, _, cmd)| *cmd).map(|(_, c, _)| c.name.as_str()).collect();
            line(format!(
                "Deck: {} — {} — {} cards — commander(s): {}",
                facts.deck.name,
                facts.deck.format.label(),
                a.total_cards,
                if commanders.is_empty() { "none".into() } else { commanders.join(" + ") }
            ));
            for (_, c, _) in facts.cards.iter().filter(|(_, _, cmd)| *cmd) {
                line(format!("Commander text: {}: {}", c.name, c.oracle_text.replace('\n', " ")));
            }
            line(format!("Colours: {}", if a.identity.is_empty() { "colourless" } else { &a.identity }));
            line(format!("Bracket: {}", a.bracket.display_name()));
            if !a.tags.is_empty() {
                line(format!("Themes: {}", a.tags.iter().take(5).map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")));
            }
            line(format!("Average mana value {:.2}; curve MV 0..7+: {:?}", a.avg_cmc, a.curve));
            line(format!(
                "Lands {}, creatures {}, instants {}, sorceries {}, artifacts {}, enchantments {}, planeswalkers {}",
                a.types.lands, a.types.creatures, a.types.instants, a.types.sorceries, a.types.artifacts, a.types.enchantments, a.types.planeswalkers
            ));
            line(format!("Price: ${:.2}", a.total_price_usd));
            if !a.legality_issues.is_empty() {
                line(format!("Legality problems: {}", a.legality_issues.join("; ")));
            }
        }
        Tool::MentionedCards => {
            let cards = mentioned_cards(facts, db, question);
            if cards.is_empty() {
                line("No card name found in the question.".into());
            }
            let owned = deck_names(facts);
            for c in cards {
                line(format!(
                    "{} | {} | {} | roles: {} | {} | {} | {}",
                    c.name,
                    c.mana_cost,
                    c.type_line,
                    role_names(classify_card(&c)).join(", "),
                    price_of(&c),
                    if owned.contains(&c.name.to_ascii_lowercase()) { "in this deck" } else { "not in this deck" },
                    c.oracle_text.replace('\n', " ")
                ));
            }
        }
        Tool::Combos => match &facts.combos {
            Some(r) if !r.variants.is_empty() => {
                for v in r.variants.iter().take(12) {
                    line(format!(
                        "{}{} → {}{}",
                        v.cards.join(" + "),
                        if v.almost { " (one piece missing)" } else { "" },
                        v.result,
                        v.bracket.map(|b| format!(" [bracket {b}]")).unwrap_or_default()
                    ));
                }
            }
            Some(_) => line("Commander Spellbook found no combos in this deck.".into()),
            None => line("Combos haven't been checked yet (the Combos tab checks Commander Spellbook).".into()),
        },
        Tool::PossibleCombos => match &facts.combos {
            Some(r) if !r.possible.is_empty() => {
                for v in r.possible.iter().take(8) {
                    line(format!("{} → {} (missing: {})", v.cards.join(" + "), v.result, v.missing.join(", ")));
                }
            }
            _ => line("No combos to add found yet (the Combos tab's \"Could add\" list searches for them).".into()),
        },
        Tool::Bracket => {
            let b = &a.bracket;
            line(format!("Estimated: {}", b.display_name()));
            line(format!(
                "Game Changers {} ({}); fast mana {}; tutors {}; extra-turn cards {}; mass land denial {}; high-bracket combos {}",
                b.game_changers.len(),
                if b.game_changers.is_empty() { "none".into() } else { b.game_changers.join(", ") },
                b.fast_mana,
                b.tutors,
                b.extra_turn_cards,
                b.mass_land_denial,
                b.ruthless_combos
            ));
            for r in b.reasons.iter().take(6) {
                line(format!("- {r}"));
            }
        }
        Tool::PowerCards => {
            let combo_pieces: HashSet<String> = facts
                .combos
                .iter()
                .flat_map(|r| r.variants.iter())
                .filter(|v| !v.almost && v.bracket.unwrap_or(0) >= 4)
                .flat_map(|v| v.cards.iter().map(|c| c.to_ascii_lowercase()))
                .collect();
            for (_, c, _) in facts.cards.iter().filter(|(_, _, cmd)| !cmd) {
                let roles = classify_card(c);
                let o = c.oracle_text.to_ascii_lowercase();
                let ty = c.type_line.to_ascii_lowercase();
                let mut why = Vec::new();
                if c.game_changer {
                    why.push("Game Changer");
                }
                if !ty.contains("land") && c.cmc <= 1.0 && o.contains("add {") {
                    why.push("fast mana");
                }
                if roles.tutor {
                    why.push("tutor");
                }
                if o.contains("extra turn") {
                    why.push("extra turns");
                }
                if combo_pieces.contains(&c.name.to_ascii_lowercase()) {
                    why.push("high-bracket combo piece");
                }
                if !why.is_empty() {
                    line(format!("{} — {}", c.name, why.join(", ")));
                }
            }
            if out.is_empty() {
                out.push_str("No Game Changers, fast mana, tutors or high-bracket combo pieces.\n");
            }
        }
        Tool::RoleGaps => {
            for (role, target) in ROLE_TARGETS {
                let have = role_count(facts, role);
                line(format!("{role}: {have} (target {target}){}", if have < *target { " — LOW" } else { "" }));
            }
        }
        Tool::CutCandidates => {
            if a.cut_candidates.is_empty() {
                line("No clear weak cards.".into());
            }
            for c in a.cut_candidates.iter().take(10) {
                line(format!("{} — {}", c.card, c.reason));
            }
        }
        Tool::AddCandidates => {
            let gaps = role_gaps(facts);
            let roles: Vec<&str> = if gaps.is_empty() { vec!["card draw", "removal", "ramp"] } else { gaps.iter().map(|(r, _, _)| *r).collect() };
            for role in roles.iter().take(4) {
                let found = candidates(facts, db, 6, |c| has_role(c, role) && !c.type_line.contains("Land"));
                if !found.is_empty() {
                    line(format!("For {role}:"));
                    for c in found {
                        line(format!("  {}", card_line(&c)));
                    }
                }
            }
            // Analysis' own picks for the deck's themes, checked for format
            // and colours here (the Analysis tab doesn't filter them).
            let owned = deck_names(facts);
            let themed: Vec<Card> = a
                .add_suggestions
                .iter()
                .filter_map(|s| db.get_card(&s.card).ok().flatten())
                .filter(|c| !owned.contains(&c.name.to_ascii_lowercase()) && fits_deck(facts, c))
                .take(6)
                .collect();
            if !themed.is_empty() {
                line("For the deck's themes:".into());
                for c in themed {
                    line(format!("  {}", card_line(&c)));
                }
            }
        }
        Tool::ManaBase => {
            let colours = ['W', 'U', 'B', 'R', 'G'];
            for (i, c) in colours.iter().enumerate() {
                if a.pip_demand[i] > 0 || a.direct_sources[i] > 0 {
                    line(format!("{c}: {} pips in spells, {} sources", a.pip_demand[i], a.direct_sources[i]));
                }
            }
            line(format!("Lands {}, fetch lands {}, ramp pieces {}", a.types.lands, a.fetch_lands, a.roles.ramp));
            let lands: Vec<String> = facts
                .cards
                .iter()
                .filter(|(_, c, _)| c.type_line.contains("Land") && !c.is_basic)
                .map(|(_, c, _)| format!("{} (makes {})", c.name, if c.produced_mana.is_empty() { "nothing" } else { &c.produced_mana }))
                .collect();
            line(format!("Nonbasic lands: {}", if lands.is_empty() { "none".into() } else { lands.join(", ") }));
            let fixing = candidates(facts, db, 8, |c| {
                c.type_line.contains("Land") && c.produced_mana.chars().filter(|m| "WUBRG".contains(*m)).count() >= 2
            });
            if !fixing.is_empty() {
                line("Fixing lands that fit:".into());
                for c in fixing {
                    line(format!("  {} | {} | {}", c.name, c.produced_mana, price_of(&c)));
                }
            }
        }
        Tool::Prices => {
            line(format!("Total: ${:.2}{}", a.total_price_usd, if a.unpriced_cards > 0 { format!(" ({} unpriced)", a.unpriced_cards) } else { String::new() }));
            let mut priced: Vec<&Card> = facts.cards.iter().map(|(_, c, _)| c).filter(|c| c.price_usd.is_some() && !c.is_basic).collect();
            priced.sort_by(|x, y| y.price_usd.partial_cmp(&x.price_usd).unwrap_or(std::cmp::Ordering::Equal));
            for c in priced.iter().take(12) {
                line(format!("{} — {}", c.name, price_of(c)));
            }
        }
        Tool::BudgetSwaps => {
            let mut priced: Vec<&Card> = facts
                .cards
                .iter()
                .filter(|(_, c, cmd)| !cmd && !c.is_basic && c.price_usd.unwrap_or(0.0) >= 5.0)
                .map(|(_, c, _)| c)
                .collect();
            priced.sort_by(|x, y| y.price_usd.partial_cmp(&x.price_usd).unwrap_or(std::cmp::Ordering::Equal));
            for pricey in priced.iter().take(6) {
                let roles = classify_card(pricey);
                let land = pricey.type_line.contains("Land");
                let cap = pricey.price_usd.unwrap_or(0.0) / 4.0;
                let found = candidates(facts, db, 3, |c| {
                    let r = classify_card(c);
                    c.price_usd.is_some_and(|p| p <= cap.max(1.0))
                        && c.type_line.contains("Land") == land
                        && if land {
                            c.produced_mana.len() >= pricey.produced_mana.len().min(2)
                        } else {
                            (roles.ramp && r.ramp) || (roles.draw && r.draw) || (roles.removal && r.removal) || (roles.counter && r.counter) || (roles.tutor && r.tutor) || (roles.wipe && r.wipe) || (roles.protection && r.protection)
                        }
                });
                if found.is_empty() {
                    line(format!("{} ({}): no cheaper card with the same role found", pricey.name, price_of(pricey)));
                } else {
                    line(format!("{} ({}) could become:", pricey.name, price_of(pricey)));
                    for c in found {
                        line(format!("  {}", card_line(&c)));
                    }
                }
            }
            if out.is_empty() {
                out.push_str("No card in the deck costs $5 or more.\n");
            }
        }
        Tool::GamePlan => match &facts.guide {
            Some(g) => {
                line(format!("Plan: {}", g.summary));
                if !g.strengths.is_empty() {
                    line(format!("Strengths: {}", g.strengths.join(" | ")));
                }
                if !g.weaknesses.is_empty() {
                    line(format!("Weaknesses: {}", g.weaknesses.join(" | ")));
                }
                for s in &g.sections {
                    line(format!("{}: {} {}", s.title, s.text, s.points.join(" ")));
                }
            }
            None => line("No play guide yet.".into()),
        },
        Tool::CardList => {
            for (q, c, _) in &facts.cards {
                line(format!("{q} | {} | {} | {} | {}", c.name, c.mana_cost, c.type_line, role_names(classify_card(c)).join(", ")));
            }
        }
    }
    out
}

/// Run every step's tool, reporting each step's label through `step`.
/// Returns the DATA block for [`prompt`].
pub fn gather_data(playbook: &Playbook, facts: &DeckFacts, db: &CardDb, question: &str, mut step: impl FnMut(&str)) -> String {
    let mut data = String::new();
    for s in playbook.steps {
        step(s.label);
        data.push_str(&format!("## {}\n{}\n", s.label, run_tool(s.tool, facts, db, question).trim_end()));
    }
    data
}

/// The system prompt for one playbook: the task, the steps, the rules, the
/// answer format, then the data Commander Lab gathered.
pub fn prompt(playbook: &Playbook, data: &str, notes: &str) -> String {
    let mut out = format!(
        "You are Commander Lab's assistant for one Magic: The Gathering deck.\nTASK: {}\nGOAL: {}\n\nSTEPS (the data for each is in DATA under the same heading):\n",
        playbook.title, playbook.goal
    );
    for (i, s) in playbook.steps.iter().enumerate() {
        out.push_str(&format!("{}. {}: {}\n", i + 1, s.label, s.instruction));
    }
    out.push_str("\nRULES:\n");
    for r in COMMON_RULES.iter().chain(playbook.rules.iter()) {
        out.push_str(&format!("- {r}\n"));
    }
    out.push_str(&format!("\nANSWER FORMAT:\n{}\n", playbook.output));
    if !notes.trim().is_empty() {
        out.push_str(&format!("\nPLAYER'S NOTES ABOUT THIS DECK:\n{}\n", notes.trim()));
    }
    out.push_str(&format!("\nDATA:\n{data}"));
    out
}

// ---------------------------------------------------------------------------
// Checking the answer
// ---------------------------------------------------------------------------

/// The model's answer after checking its ADD/CUT lines.
#[derive(Debug, Default)]
pub struct CheckedAnswer {
    pub text: String,
    /// Suggestions removed, with why ("Made Up Card: not a real card").
    pub dropped: Vec<String>,
    /// Names in the prose that aren't cards (left in, but flagged).
    pub unknown: Vec<String>,
    pub adds: usize,
    pub cuts: usize,
}

/// Split "ADD: Name — why" into ("ADD", "Name", "why"). Accepts a leading
/// list marker or bold, and "-", ":" or "—" before the reason.
fn parse_change(line: &str) -> Option<(&'static str, String, String)> {
    let t = line.trim().trim_start_matches(['-', '*', '•', ' ']).trim_start_matches("**");
    let upper = t.to_ascii_uppercase();
    // "ADD:" as asked, or "**Add**" copied from earlier checked answers.
    let (kind, rest) = if upper.starts_with("ADD:") || upper.starts_with("ADD**") {
        ("ADD", t[3..].trim_start_matches([':', '*']))
    } else if upper.starts_with("CUT:") || upper.starts_with("CUT**") {
        ("CUT", t[3..].trim_start_matches([':', '*']))
    } else {
        return None;
    };
    let rest = rest.trim().trim_start_matches("**").trim();
    let (name, why) = match rest.find(" — ").or_else(|| rest.find(" - ")).or_else(|| rest.find(" – ")) {
        Some(i) => (&rest[..i], rest[i..].trim_start_matches([' ', '—', '-', '–']).trim()),
        None => (rest, ""),
    };
    // Models copy whole DATA lines ("Name | {1}{B} | …") or add a price
    // ("Name ($38.71)"): the name ends there.
    let name = [" | ", " (", "|"].iter().filter_map(|sep| name.find(sep)).min().map_or(name, |i| &name[..i]);
    let name = name.trim().trim_matches(['*', '"', '[', ']']).trim();
    (!name.is_empty()).then(|| (kind, name.to_string(), why.to_string()))
}

/// Check every ADD/CUT line: an ADD must be a real card that isn't already
/// in the deck and fits its format and colours; a CUT must be in the deck.
/// Good lines are rewritten as "**Add** Name — why"; bad ones are removed
/// and listed in `dropped`. Other text is kept as written.
pub fn check_answer(answer: &str, facts: &DeckFacts, db: &CardDb) -> CheckedAnswer {
    let owned = deck_names(facts);
    let mut checked = CheckedAnswer::default();
    let mut lines = Vec::new();
    for raw in answer.lines() {
        let Some((kind, name, why)) = parse_change(raw) else {
            lines.push(raw.to_string());
            continue;
        };
        let card = db.get_card(&name).ok().flatten();
        let problem = match (kind, &card) {
            (_, None) => Some("not a real card".to_string()),
            ("CUT", Some(c)) if !owned.contains(&c.name.to_ascii_lowercase()) => Some("not in this deck".into()),
            ("ADD", Some(c)) if owned.contains(&c.name.to_ascii_lowercase()) => Some("already in the deck".into()),
            ("ADD", Some(c)) => match facts.deck.format.card_allowed(c) {
                Err(why) => Some(why),
                Ok(()) if facts.deck.format.has_commander() && !identity_subset(&c.color_identity, &facts.analysis.identity) => {
                    Some("outside the commander's colours".into())
                }
                Ok(()) => None,
            },
            _ => None,
        };
        match (problem, card) {
            (Some(why_not), _) => checked.dropped.push(format!("{name}: {why_not}")),
            (None, Some(c)) => {
                let verb = if kind == "ADD" { "Add" } else { "Cut" };
                if kind == "ADD" {
                    checked.adds += 1;
                } else {
                    checked.cuts += 1;
                }
                let price = if kind == "ADD" { format!(" ({})", price_of(&c)) } else { String::new() };
                lines.push(if why.is_empty() { format!("- **{verb}** {}{price}", c.name) } else { format!("- **{verb}** {}{price} — {why}", c.name) });
            }
            (None, None) => {}
        }
    }
    let mut text = lines.join("\n").trim().to_string();
    // Names the model quoted or bolded in its prose that aren't cards.
    for name in quoted_names(&text) {
        if db.get_card(&name).ok().flatten().is_none() && !checked.dropped.iter().any(|d| d.starts_with(&name)) {
            checked.unknown.push(name);
        }
    }
    if !checked.dropped.is_empty() {
        text.push_str(&format!(
            "\n\n_Commander Lab removed {} suggestion(s) that didn't check out: {}._",
            checked.dropped.len(),
            checked.dropped.join("; ")
        ));
    }
    if !checked.unknown.is_empty() {
        text.push_str(&format!("\n\n_Not real cards, so ignore them: {}._", checked.unknown.join(", ")));
    }
    checked.text = text;
    checked
}

/// Spans in "quotes" or **bold** that look like card names: one to six
/// words, each starting with a capital (small words like "of" allowed).
fn quoted_names(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for delim in ["\"", "**", "“"] {
        let close = if delim == "“" { "”" } else { delim };
        let mut rest = text;
        while let Some(i) = rest.find(delim) {
            let after = &rest[i + delim.len()..];
            let Some(j) = after.find(close) else { break };
            let span = after[..j].trim();
            rest = &after[j + close.len()..];
            let words: Vec<&str> = span.split_whitespace().collect();
            let card_like = (1..=6).contains(&words.len())
                && words.iter().all(|w| {
                    w.chars().next().is_some_and(|c| c.is_uppercase())
                        || ["of", "the", "to", "and", "a", "in", "from", "on", "//"].contains(w)
                })
                && !span.ends_with(':')
                && !["Add", "Cut"].contains(&span);
            if card_like && !out.iter().any(|n| n == span) {
                out.push(span.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_common_requests() {
        assert_eq!(route("Can you upgrade this deck?").id, "upgrade");
        assert_eq!(route("make it cheaper please").id, "budget");
        assert_eq!(route("How can I make this deck cheaper?").id, "budget");
        assert_eq!(route("how much does this deck cost").id, "budget");
        assert_eq!(route("what infinite combos do I have").id, "combos");
        assert_eq!(route("Is my mana base ok? I keep getting color screw").id, "mana");
        assert_eq!(route("what bracket is this").id, "power");
        assert_eq!(route("tone it down to bracket 2").id, "power_down");
        assert_eq!(route("what should I cut to get to 100").id, "cuts");
        assert_eq!(route("what does Rhystic Study do").id, "explain");
        assert_eq!(route("when should I mulligan").id, "play");
        assert_eq!(route("hello there").id, "general");
    }

    #[test]
    fn every_playbook_is_reachable_and_complete() {
        for p in PLAYBOOKS {
            assert!(!p.steps.is_empty(), "{} has no steps", p.id);
            assert!(!p.goal.is_empty() && !p.output.is_empty());
            if p.id != "general" {
                assert!(p.triggers.iter().any(|t| route(t).id == p.id), "{} can't be routed to", p.id);
            }
        }
        assert_eq!(general().id, "general");
    }

    #[test]
    fn ties_go_to_the_model() {
        let r = ranked("cheaper combo");
        assert!(is_tied(&r));
        let options: Vec<&Playbook> = r.iter().map(|(p, _)| *p).collect();
        assert!(choice_prompt("cheaper combo", &options).contains("budget"));
        assert_eq!(parse_choice("I think `combos` fits best", &options).unwrap().id, "combos");
        assert!(parse_choice("no idea", &options).is_none());
    }

    #[test]
    fn reads_change_lines() {
        assert_eq!(parse_change("ADD: Sol Ring — fast mana"), Some(("ADD", "Sol Ring".into(), "fast mana".into())));
        assert_eq!(parse_change("- **CUT:** Grizzly Bears - vanilla"), Some(("CUT", "Grizzly Bears".into(), "vanilla".into())));
        assert_eq!(parse_change("add: Arcane Signet"), Some(("ADD", "Arcane Signet".into(), String::new())));
        assert_eq!(parse_change("Adding cards is good"), None);
        assert_eq!(parse_change("ADD: Deadly Dispute | {1}{B} | Instant | $0.31").unwrap().1, "Deadly Dispute");
        assert_eq!(parse_change("- **Cut** Necropotence ($38.71) — pricey").unwrap().1, "Necropotence");
    }

    #[test]
    fn finds_quoted_card_like_names() {
        let names = quoted_names("Swap \"Soulbringer\" for **Sol Ring**, it's \"very good value\" and **Add** it.");
        assert_eq!(names, vec!["Soulbringer".to_string(), "Sol Ring".to_string()]);
    }

    #[test]
    fn prompt_lists_steps_rules_and_data() {
        let p = route("upgrade it");
        let text = prompt(p, "## Reading the deck\nDeck: Test", "keep it mono-black");
        assert!(text.contains("TASK: Upgrade the deck"));
        assert!(text.contains("1. Reading the deck"));
        assert!(text.contains("Never invent card names"));
        assert!(text.contains("ADD: Card Name"));
        assert!(text.contains("keep it mono-black"));
        assert!(text.ends_with("Deck: Test"));
    }
}
