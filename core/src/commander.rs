//! Commander rules: who can lead a deck, partners, combined colors.
//!
//! How it fits: [`resolve_generation_commanders`] turns "A + B" into checked
//! cards; [`pair_compatible`] / [`can_have_second_commander`] know Partner,
//! Partner with, Friends forever, Choose a Background and Doctor's companion;
//! [`combined_identity`] merges colors; [`commander_name_suggestions`] powers
//! the commander picker.
//!
//! Things to work on:
//! - New pairing mechanics need a code change here; keep a list of the
//!   keyword wordings in one place with tests.

use crate::{
    archetype::tag_names_for_card,
    data::{
        commander_eligibility_reason,
        effective_commander_eligible,
        Card,
        CardDb,
    },
    logging,
};
use anyhow::{anyhow, Result};

/// May these two lead a deck together? Partner + Partner, Friends forever,
/// "Choose a Background" + a Background, Doctor's companion + a Doctor, or
/// "Partner with <the other one>".
pub fn pair_compatible(a: &Card, b: &Card) -> bool {
    let ak = a.keywords.to_ascii_lowercase();
    let bk = b.keywords.to_ascii_lowercase();
    let ao = a.oracle_text.to_ascii_lowercase();
    let bo = b.oracle_text.to_ascii_lowercase();
    let at = a.type_line.to_ascii_lowercase();
    let bt = b.type_line.to_ascii_lowercase();

    (has_keyword(&ak, "partner") && has_keyword(&bk, "partner"))
        || (ak.contains("friends forever") && bk.contains("friends forever"))
        || (ao.contains("choose a background") && is_background(&bt))
        || (bo.contains("choose a background") && is_background(&at))
        || (ao.contains("doctor's companion") && is_doctor(&bt))
        || (bo.contains("doctor's companion") && is_doctor(&at))
        || partner_with_matches(&ao, &b.name)
        || partner_with_matches(&bo, &a.name)
}

/// Does this commander allow a second one at all (any of the mechanics
/// above, from either side)?
pub fn can_have_second_commander(card: &Card) -> bool {
    let keywords = card.keywords.to_ascii_lowercase();
    let oracle = card.oracle_text.to_ascii_lowercase();
    let type_line = card.type_line.to_ascii_lowercase();

    has_keyword(&keywords, "partner")
        || keywords.contains("friends forever")
        || oracle.contains("partner with ")
        || oracle.contains("choose a background")
        || oracle.contains("doctor's companion")
        || is_background(&type_line)
        || is_doctor(&type_line)
}

/// The deck's colors: every color either commander has, in WUBRG order.
pub fn combined_identity(cards: &[Card]) -> String {
    let mut identity = String::new();

    for color in ['W', 'U', 'B', 'R', 'G'] {
        if cards.iter().any(|card| card.color_identity.contains(color)) {
            identity.push(color);
        }
    }

    identity
}

/// "A" or "A + B" → checked commander cards for generation (Commander
/// format; PDH uses generator::resolve_pdh_commanders).
/// Note: a single commander that *can* have a partner gets one picked
/// automatically: its "Partner with" card, else the best-matching partner
/// or Background (`choose_best_partner`). That's surprising if the player
/// wanted it solo.
pub fn resolve_generation_commanders(
    specification: &str,
    db: &CardDb,
) -> Result<Vec<Card>> {
    let parts = split_commander_spec(specification);

    if parts.is_empty() {
        return Err(anyhow!("No commander name was supplied."));
    }

    if parts.len() > 2 {
        return Err(anyhow!(
            "Generation accepts one commander or one legal commander pair."
        ));
    }

    let mut commanders = Vec::new();

    for part in parts {
        let card = resolve_commander_name(&part, db)?
            .ok_or_else(|| anyhow!("Could not confidently match commander '{}'.", part))?;

        validate_commander(&card)?;
        commanders.push(card);
    }

    if commanders.len() == 2 {
        if !pair_compatible(&commanders[0], &commanders[1]) {
            return Err(anyhow!(
                "{} and {} were found, but no supported legal pairing mechanic was detected between them.",
                commanders[0].name,
                commanders[1].name
            ));
        }

        return Ok(commanders);
    }

    let first = commanders.remove(0);

    if !can_have_second_commander(&first) {
        return Ok(vec![first]);
    }

    if let Some(explicit) = explicit_partner_with_name(&first.oracle_text) {
        if let Some(partner) = resolve_commander_name(&explicit, db)? {
            if pair_compatible(&first, &partner) {
                return Ok(vec![first, partner]);
            }
        }
    }

    if let Some(partner) = choose_best_partner(&first, db)? {
        return Ok(vec![first, partner]);
    }

    Ok(vec![first])
}

/// Commander picker results for a typed query: name matches, then matches
/// for each word, then (still short) eligible popular cards, all ranked by
/// how well they match the query (`commander_match_score`).
pub fn commander_name_suggestions(
    query: &str,
    db: &CardDb,
    limit: usize,
) -> Result<Vec<Card>> {
    let query = query.trim();

    if query.is_empty() {
        return Ok(Vec::new());
    }

    let mut candidates = db.search_commanders(query, limit.saturating_mul(3))?;

    if candidates.len() < limit {
        for token in query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|token| token.len() >= 3)
        {
            for card in db.search_commanders(token, limit.saturating_mul(3))? {
                if !candidates
                    .iter()
                    .any(|existing| existing.name.eq_ignore_ascii_case(&card.name))
                {
                    candidates.push(card);
                }
            }
        }
    }

    if candidates.len() < limit {
        for card in db.top_ranked_cards(25_000)? {
            if !effective_commander_eligible(&card)
                || !card.commander_legal
            {
                continue;
            }

            if !candidates
                .iter()
                .any(|existing| existing.name.eq_ignore_ascii_case(&card.name))
            {
                candidates.push(card);
            }
        }
    }

    candidates.sort_by(|a, b| {
        commander_match_score(query, b)
            .cmp(&commander_match_score(query, a))
            .then_with(|| {
                a.edhrec_rank
                    .unwrap_or(u32::MAX)
                    .cmp(&b.edhrec_rank.unwrap_or(u32::MAX))
            })
            .then_with(|| a.name.cmp(&b.name))
    });

    candidates.truncate(limit);
    Ok(candidates)
}

/// Turn typed text into one commander: an exact (or folded) name if it can
/// lead a deck, otherwise the best fuzzy suggestion if it's a confident
/// match. Every decision is logged under COMMANDER.
pub fn resolve_commander_name(
    query: &str,
    db: &CardDb,
) -> Result<Option<Card>> {
    let query = query.trim();

    if query.is_empty() {
        return Ok(None);
    }

    if let Some(card) = db.get_card(query)? {
        logging::record(
            "COMMANDER",
            format!(
                "EXACT LOOKUP • query={} • found={} • type={} • commander_legal={} • stored_or_effective_eligible={} • oracle_chars={} • reason={}",
                query,
                card.name,
                card.type_line,
                card.commander_legal,
                effective_commander_eligible(&card),
                card.oracle_text.chars().count(),
                commander_eligibility_reason(&card),
            ),
        );

        if effective_commander_eligible(&card)
            && card.commander_legal
        {
            return Ok(Some(card));
        }
        // The exact card exists but can't lead a deck: say so instead of
        // quietly picking a similarly named commander (asking for "Delver
        // of Secrets" used to get "Toski, Bearer of Secrets").
        if card.name.eq_ignore_ascii_case(query) || card.name.split(" // ").next().is_some_and(|f| f.eq_ignore_ascii_case(query)) {
            return Err(anyhow!(
                "{} can't be a commander: {}",
                card.name,
                if card.commander_legal {
                    "it isn't a legendary creature, and its text doesn't say it can be your commander"
                } else {
                    "it isn't legal in Commander"
                }
            ));
        }
    }

    let candidates = commander_name_suggestions(query, db, 8)?;

    let Some(best) = candidates.first() else {
        logging::record(
            "COMMANDER",
            format!(
                "NO CANDIDATES • query={query}"
            ),
        );
        return Ok(None);
    };

    logging::record(
        "COMMANDER",
        format!(
            "BEST FUZZY • query={} • candidate={} • type={} • legal={} • eligible={} • reason={}",
            query,
            best.name,
            best.type_line,
            best.commander_legal,
            effective_commander_eligible(best),
            commander_eligibility_reason(best),
        ),
    );

    let score = commander_match_score(query, best);
    let normalized_query = normalize_name(query);
    let normalized_best = normalize_name(&best.name);

    let best_first_name = normalized_best
        .split_whitespace()
        .next()
        .unwrap_or_default();

    let acceptable = score >= 450
        || normalized_query == best_first_name
        || normalized_best.starts_with(&normalized_query)
        || normalized_query.starts_with(&normalized_best)
        || token_overlap(&normalized_query, &normalized_best) >= 0.50;

    if acceptable
        && best.commander_legal
        && effective_commander_eligible(best)
    {
        Ok(Some(best.clone()))
    } else {
        logging::record(
            "COMMANDER",
            format!(
                "REJECT FUZZY • query={} • candidate={} • acceptable={} • legal={} • eligible={} • score={}",
                query,
                best.name,
                acceptable,
                best.commander_legal,
                effective_commander_eligible(best),
                score,
            ),
        );
        Ok(None)
    }
}

/// How well a card name matches a typed query: exact, prefix, compacted
/// match, shared words, edit distance; popularity breaks ties.
fn commander_match_score(query: &str, card: &Card) -> i32 {
    let query = normalize_name(query);
    let name = normalize_name(&card.name);

    if query == name {
        return 10_000;
    }

    let query_compact = compact_name(&query);
    let name_compact = compact_name(&name);

    if !query_compact.is_empty() && query_compact == name_compact {
        return 9_500;
    }

    let first_name = name
        .split_whitespace()
        .next()
        .unwrap_or_default();

    let mut score = 0i32;

    // Commander users very commonly type only the leading proper name:
    // "Shorikai", "Urtet", "Muldrotha", etc. Treat an exact leading-name
    // match as a high-confidence autocomplete signal.
    if query == first_name {
        score += 2_400;
    }

    if name.starts_with(&query) {
        score += 1_500;
    }

    if query.starts_with(&name) {
        score += 1_000;
    }

    if name.contains(&query) {
        score += 700;
    }

    score += (token_overlap(&query, &name) * 500.0).round() as i32;

    let distance = levenshtein(&query, &name);
    let max_len = query.chars().count().max(name.chars().count()).max(1);
    let similarity = 1.0 - distance as f64 / max_len as f64;
    score += (similarity.max(0.0) * 900.0).round() as i32;

    score += match card.edhrec_rank {
        Some(rank) if rank <= 500 => 80,
        Some(rank) if rank <= 2_000 => 60,
        Some(rank) if rank <= 10_000 => 35,
        _ => 0,
    };

    score
}

/// Lowercase words without punctuation.
fn normalize_name(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Letters and digits only ("Kefka,Court Mage" ≈ "kefkacourtmage").
fn compact_name(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Share of words two names have in common (0–1).
fn token_overlap(a: &str, b: &str) -> f64 {
    let a_tokens = a.split_whitespace().collect::<Vec<_>>();
    let b_tokens = b.split_whitespace().collect::<Vec<_>>();

    if a_tokens.is_empty() || b_tokens.is_empty() {
        return 0.0;
    }

    let matches = a_tokens
        .iter()
        .filter(|token| b_tokens.iter().any(|other| other == *token))
        .count();

    matches as f64 / a_tokens.len().min(b_tokens.len()).max(1) as f64
}

/// Edit distance between two names (typos).
fn levenshtein(a: &str, b: &str) -> usize {
    let b_chars = b.chars().collect::<Vec<_>>();
    let mut previous = (0..=b_chars.len()).collect::<Vec<_>>();

    for (i, a_char) in a.chars().enumerate() {
        let mut current = Vec::with_capacity(b_chars.len() + 1);
        current.push(i + 1);

        for (j, b_char) in b_chars.iter().enumerate() {
            let insertion = current[j] + 1;
            let deletion = previous[j + 1] + 1;
            let substitution =
                previous[j] + usize::from(a_char != *b_char);

            current.push(insertion.min(deletion).min(substitution));
        }

        previous = current;
    }

    previous[b_chars.len()]
}

/// Refuse cards that aren't Commander legal or can't lead a deck, with the
/// reason in the error and the log.
fn validate_commander(card: &Card) -> Result<()> {
    if !card.commander_legal {
        return Err(anyhow!(
            "{} is not Commander legal in the local Scryfall database.",
            card.name
        ));
    }

    if !effective_commander_eligible(card) {
        logging::record(
            "COMMANDER",
            format!(
                "VALIDATION FAILED • name={} • type={} • legal={} • oracle_chars={} • reason={}",
                card.name,
                card.type_line,
                card.commander_legal,
                card.oracle_text.chars().count(),
                commander_eligibility_reason(card),
            ),
        );

        return Err(anyhow!(
            "{} does not appear eligible to be a commander. Type='{}', Oracle chars={}, reason='{}'.",
            card.name,
            card.type_line,
            card.oracle_text.chars().count(),
            commander_eligibility_reason(card),
        ));
    }

    logging::record(
        "COMMANDER",
        format!(
            "VALIDATION SUCCESS • name={} • reason={}",
            card.name,
            commander_eligibility_reason(card),
        ),
    );

    Ok(())
}

/// Pick a partner for a commander that allows one: among popular legal
/// commanders it can pair with, prefer shared themes and fewer extra colors.
fn choose_best_partner(card: &Card, db: &CardDb) -> Result<Option<Card>> {
    let pool = db.top_ranked_cards(12_000)?;
    let card_tags = tag_names_for_card(card);
    let mut best: Option<(Card, i32)> = None;

    for candidate in pool {
        if candidate.name.eq_ignore_ascii_case(&card.name)
            || !effective_commander_eligible(&candidate)
            || !candidate.commander_legal
            || !pair_compatible(card, &candidate)
        {
            continue;
        }

        let candidate_tags = tag_names_for_card(&candidate);
        let shared_tags = card_tags
            .iter()
            .filter(|tag| candidate_tags.iter().any(|other| other == *tag))
            .count() as i32;

        let new_colors = candidate
            .color_identity
            .chars()
            .filter(|color| !card.color_identity.contains(*color))
            .count() as i32;

        let rank_score = match candidate.edhrec_rank {
            Some(rank) if rank <= 500 => 40,
            Some(rank) if rank <= 2_000 => 32,
            Some(rank) if rank <= 10_000 => 22,
            Some(rank) if rank <= 30_000 => 12,
            _ => 0,
        };

        let score =
            shared_tags * 18 + new_colors * 8 + rank_score - candidate.cmc as i32;

        if best
            .as_ref()
            .map(|(_, current)| score > *current)
            .unwrap_or(true)
        {
            best = Some((candidate, score));
        }
    }

    Ok(best.map(|(candidate, _)| candidate))
}

/// Split "A + B" (also "A & B" or "A | B") into names.
fn split_commander_spec(specification: &str) -> Vec<String> {
    for separator in [" + ", " & ", " | "] {
        if specification.contains(separator) {
            return specification
                .split(separator)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect();
        }
    }

    let trimmed = specification.trim();

    if trimmed.is_empty() {
        Vec::new()
    } else {
        vec![trimmed.to_string()]
    }
}

/// The card named in "Partner with <name>", if any.
fn explicit_partner_with_name(oracle_text: &str) -> Option<String> {
    let lower = oracle_text.to_ascii_lowercase();
    let needle = "partner with ";
    let start = lower.find(needle)? + needle.len();
    let remainder = &oracle_text[start..];

    let end = remainder
        .find(['\n', '(', '.'])
        .unwrap_or(remainder.len());

    let name = remainder[..end].trim();

    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Is this exact keyword in the card's keyword list?
fn has_keyword(keywords: &str, expected: &str) -> bool {
    keywords
        .split(',')
        .map(str::trim)
        .any(|keyword| keyword == expected)
}

/// Does the text say "Partner with <other_name>"?
fn partner_with_matches(oracle: &str, other_name: &str) -> bool {
    let expected = format!("partner with {}", other_name.to_ascii_lowercase());
    oracle.contains(&expected)
}

/// Legendary Enchantment — Background.
fn is_background(type_line: &str) -> bool {
    type_line.contains("legendary enchantment")
        && type_line.contains("background")
}

/// A Time Lord Doctor creature.
fn is_doctor(type_line: &str) -> bool {
    type_line.contains("time lord doctor")
}

#[cfg(test)]
mod commander_name_tests {
    use super::{compact_name, levenshtein, normalize_name, token_overlap};

    #[test]
    fn punctuation_does_not_break_commander_name_matching() {
        assert_eq!(
            normalize_name("Shorikai, Genesis Engine"),
            "shorikai genesis engine",
        );

        assert_eq!(
            compact_name("Shorikai, Genesis Engine"),
            compact_name("Shorikai Genesis Engine"),
        );
    }

    #[test]
    fn short_commander_names_have_strong_token_overlap() {
        assert!(
            token_overlap(
                &normalize_name("Urtet"),
                &normalize_name("Urtet, Remnant of Memnarch"),
            ) >= 0.99
        );
    }

    #[test]
    fn small_typos_stay_close() {
        assert!(
            levenshtein("shorikai", "shoraki") <= 2
        );
    }
}
