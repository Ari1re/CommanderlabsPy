use crate::{
    data::Card,
    deck::ParsedDeck,
    references::ReferenceSnapshot,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckSource {
    pub source: String,
    pub title: String,
    pub url: String,
    pub detail: String,
    #[serde(default)]
    pub user_provided: bool,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub reference_deck: Option<ParsedDeck>,
    #[serde(default)]
    pub content_excerpt: String,
}

impl DeckSource {
    pub fn automatic(
        source: impl Into<String>,
        title: impl Into<String>,
        url: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            source: source.into(),
            title: title.into(),
            url: url.into(),
            detail: detail.into(),
            user_provided: false,
            priority: 0,
            reference_deck: None,
            content_excerpt: String::new(),
        }
    }
}

pub fn generation_sources(
    commanders: &[Card],
    selected_theme: &str,
    bracket: u8,
    references: &ReferenceSnapshot,
) -> Vec<DeckSource> {
    let mut out = if crate::format_sources::active_format() == crate::format::Format::Pdh {
        pdh_sources(commanders)
    } else {
        standard_sources(commanders, selected_theme, bracket)
    };

    for deck in &references.decks {
        push_unique(
            &mut out,
            DeckSource::automatic(
                deck.source.clone(),
                deck.title.clone(),
                deck.url.clone(),
                format!(
                    "Reference deck used for card-frequency, card-package, shell, and combo comparison ({} distinct cards, {} complete Spellbook combo(s) detected).",
                    deck.cards.len(),
                    deck.combos.len(),
                ),
            ),
        );
    }

    out
}

/// Sources behind a Pauper Commander build: PDHREC instead of EDHREC, the
/// PDH Home Base rules, and Archidekt's Pauper EDH decks (added per deck).
pub fn pdh_sources(commanders: &[Card]) -> Vec<DeckSource> {
    let names: Vec<String> = commanders.iter().map(|c| c.name.clone()).collect();
    let slug = crate::format_sources::pdhrec_slug(&names);
    vec![
        DeckSource::automatic(
            "PDHREC",
            format!("{} — PDHREC", names.join(" + ")),
            format!("https://pdhrec.com/commander/{slug}/"),
            "Pauper Commander card inclusion and synergy for this commander across the decks PDHREC tracks.",
        ),
        DeckSource::automatic(
            "PDH Home Base",
            "Pauper Commander rules",
            "https://pdhhomebase.com/rules/",
            "Commander and card eligibility, banned list, 30 life and 16 commander damage.",
        ),
        DeckSource::automatic(
            "Scryfall",
            "Scryfall Bulk Data",
            "https://scryfall.com/docs/api/bulk-data",
            "Rarity across printings, Pauper Commander legality, Oracle text, prices and images.",
        ),
        DeckSource::automatic(
            "Commander Spellbook",
            "Commander Spellbook",
            "https://commanderspellbook.com/",
            "Known combo validation and combo-card relationships.",
        ),
    ]
}

pub fn standard_sources(
    commanders: &[Card],
    selected_theme: &str,
    bracket: u8,
) -> Vec<DeckSource> {
    let mut out = vec![
        DeckSource::automatic(
            "Scryfall",
            "Scryfall Bulk Data",
            "https://scryfall.com/docs/api/bulk-data",
            "Oracle text, legality, colors, prices, images, EDHREC rank, and storefront links.",
        ),
        DeckSource::automatic(
            "Commander Spellbook",
            "Commander Spellbook",
            "https://commanderspellbook.com/",
            "Known combo validation and combo-card relationships.",
        ),
        DeckSource::automatic(
            "Commander Rules",
            "Commander Rules",
            "https://mtgcommander.net/index.php/rules/",
            "Commander construction and color-identity rules.",
        ),
    ];

    if bracket >= 5 {
        out.insert(
            0,
            DeckSource::automatic(
                "cEDH Decklist Database",
                "cEDH Decklist Database",
                "https://cedh-decklist-database.com/",
                "Tournament-proven competitive lists. At Bracket 5 these are the primary evidence; casual decklists and EDHREC only break ties.",
            ),
        );
    }

    let themes = crate::archetype::theme_list(selected_theme);
    let pages: Vec<Option<String>> = if themes.is_empty() {
        vec![None]
    } else {
        themes.iter().cloned().map(Some).collect()
    };

    for commander in commanders {
        let slug = slugify(&commander.name);

        for page in &pages {
            let url = match page {
                None => format!("https://edhrec.com/commanders/{slug}"),
                Some(theme) => format!("https://edhrec.com/commanders/{}/{}", slug, slugify(theme)),
            };
            let label = page.as_deref().unwrap_or("base commander page");

            push_unique(
                &mut out,
                DeckSource::automatic(
                    "EDHREC",
                    match page {
                        None => format!("{} — EDHREC", commander.name),
                        Some(theme) => format!("{} — EDHREC ({theme})", commander.name),
                    },
                    url,
                    format!(
                        "Commander-specific themes, inclusion rates, synergy signals, and normal-build card data. Selected play-style preference: {label}."
                    ),
                ),
            );
        }
    }


    push_unique(
        &mut out,
        DeckSource::automatic(
            "Archidekt",
            "Archidekt public deck references",
            "https://archidekt.com/",
            "Public Commander lists sampled for card-frequency and shell comparison.",
        ),
    );

    push_unique(
        &mut out,
        DeckSource::automatic(
            "Moxfield",
            "Moxfield public deck references",
            "https://www.moxfield.com/decks/public",
            "Best-effort public Commander deck references used as a secondary shell signal.",
        ),
    );

    if bracket >= 5 {
        push_unique(
            &mut out,
            DeckSource::automatic(
                "cEDH DDB",
                "cEDH Decklist Database",
                "https://cedh-decklist-database.com/",
                "Bracket 5 competitive reference layer; linked decklists are compared when matching commanders are found.",
            ),
        );
    }

    out
}

pub fn sources_for_existing_deck(
    deck: &ParsedDeck,
    commanders: &[Card],
    bracket: u8,
) -> Vec<DeckSource> {
    let mut out = match deck.format {
        crate::format::Format::Pdh => pdh_sources(commanders),
        crate::format::Format::Canlander => vec![
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
        ],
        crate::format::Format::Commander => standard_sources(commanders, "", bracket),
    };

    if deck
        .name
        .to_ascii_lowercase()
        .contains("generated")
    {
        push_unique(
            &mut out,
            DeckSource::automatic(
                "Commander Lab",
                "Commander Lab local generation engine",
                "https://github.com/",
                "Local scoring combines commander rules text, EDHREC commander signals, public reference shells, structural role targets, bracket constraints, and combo relevance.",
            ),
        );
    }

    out
}

pub fn save_sources_for_deck(
    deck_path: &Path,
    sources: &[DeckSource],
) -> Result<PathBuf> {
    let path = sidecar_path(deck_path);

    fs::write(
        &path,
        serde_json::to_string_pretty(sources)?,
    )
    .with_context(|| {
        format!("Could not write {}", path.display())
    })?;

    Ok(path)
}

pub fn load_sources_for_deck(
    deck_path: &Path,
) -> Result<Option<Vec<DeckSource>>> {
    let path = sidecar_path(deck_path);

    if !path.exists() {
        return Ok(None);
    }

    let text = fs::read_to_string(&path)
        .with_context(|| {
            format!("Could not read {}", path.display())
        })?;

    Ok(Some(
        serde_json::from_str(&text).with_context(|| {
            format!("Could not parse {}", path.display())
        })?,
    ))
}

pub fn delete_sources_sidecar(deck_path: &Path) {
    let path = sidecar_path(deck_path);

    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

pub fn sidecar_path(deck_path: &Path) -> PathBuf {
    let stem = deck_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("deck");

    deck_path
        .parent()
        .unwrap_or_else(|| Path::new("./decks"))
        .join(format!("{stem}.sources.json"))
}

pub fn push_unique(
    out: &mut Vec<DeckSource>,
    source: DeckSource,
) {
    if !out.iter().any(|existing| {
        existing.url.eq_ignore_ascii_case(&source.url)
            && existing
                .title
                .eq_ignore_ascii_case(&source.title)
    }) {
        out.push(source);
    }
}

pub fn user_sources(
    sources: &[DeckSource],
) -> Vec<DeckSource> {
    sources
        .iter()
        .filter(|source| source.user_provided)
        .cloned()
        .collect()
}

pub fn merge_user_sources(
    target: &mut Vec<DeckSource>,
    user_sources: &[DeckSource],
) {
    for source in user_sources {
        push_unique(target, source.clone());
    }
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;

    for c in value.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !slug.is_empty() {
            slug.push('-');
            dash = true;
        }
    }

    slug.trim_matches('-').to_string()
}
