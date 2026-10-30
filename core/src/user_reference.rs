use crate::{
    archidekt::import_archidekt,
    deck::ParsedDeck,
    moxfield::import_moxfield,
    sources::DeckSource,
};
use anyhow::{anyhow, Context, Result};
use regex::Regex;
use reqwest::{
    blocking::Client,
    header::{ACCEPT, USER_AGENT},
};
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::PathBuf,
    time::Duration,
};

const USER_AGENT_VALUE: &str =
    concat!("CommanderLab/", env!("CARGO_PKG_VERSION"), " user-reference importer");

pub fn extract_urls(text: &str) -> Vec<String> {
    let re = Regex::new(r#"https?://[^\s<>"']+"#)
        .expect("valid URL regex");

    let mut out = Vec::new();

    for hit in re.find_iter(text) {
        let url = hit
            .as_str()
            .trim_end_matches(|c: char| {
                matches!(
                    c,
                    '.' | ',' | ';' | ':' | ')' | ']' | '}'
                )
            })
            .to_string();

        if !out.iter().any(|existing: &String| {
            existing.eq_ignore_ascii_case(&url)
        }) {
            out.push(url);
        }
    }

    out
}

pub fn import_user_reference(url: &str) -> Result<DeckSource> {
    let trimmed = url.trim();

    if trimmed.is_empty() {
        return Err(anyhow!("Reference URL is empty."));
    }

    if let Some(cached) =
        load_cached_user_reference(trimmed)
    {
        return Ok(cached);
    }

    let lower = trimmed.to_ascii_lowercase();

    let source = if lower.contains("moxfield.com/decks/") {
        let deck = import_moxfield(trimmed)
            .with_context(|| {
                format!(
                    "Could not import Moxfield reference {trimmed}"
                )
            })?;

        reference_from_deck(
            "User • Moxfield",
            trimmed,
            deck,
        )
    } else if lower.contains("archidekt.com/decks/") {
        let deck = import_archidekt(trimmed)
            .with_context(|| {
                format!(
                    "Could not import Archidekt reference {trimmed}"
                )
            })?;

        reference_from_deck(
            "User • Archidekt",
            trimmed,
            deck,
        )
    } else {
        import_web_reference(trimmed)?
    };

    let _ = save_cached_user_reference(&source);

    Ok(source)
}


fn canonical_url(url: &str) -> String {
    url.trim()
        .trim_end_matches('/')
        .replace("https://www.", "https://")
        .replace("http://www.", "http://")
        .to_ascii_lowercase()
}

fn user_reference_cache_path(url: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    canonical_url(url).hash(&mut hasher);

    PathBuf::from(
        "./data/source_cache/user_references",
    )
    .join(format!("{:016x}.json", hasher.finish()))
}

fn load_cached_user_reference(
    url: &str,
) -> Option<DeckSource> {
    let path = user_reference_cache_path(url);
    let text = fs::read_to_string(path).ok()?;
    let source =
        serde_json::from_str::<DeckSource>(&text).ok()?;

    if source.url.trim().is_empty() {
        return None;
    }

    Some(source)
}

fn save_cached_user_reference(
    source: &DeckSource,
) -> Result<()> {
    let path = user_reference_cache_path(&source.url);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .context(
                "Could not create user-reference cache directory",
            )?;
    }

    fs::write(
        &path,
        serde_json::to_string_pretty(source)?,
    )
    .with_context(|| {
        format!(
            "Could not write user reference cache {}",
            path.display(),
        )
    })?;

    Ok(())
}

fn reference_from_deck(
    source: &str,
    url: &str,
    deck: ParsedDeck,
) -> DeckSource {
    let card_count = deck
        .main
        .iter()
        .map(|entry| entry.quantity as usize)
        .sum::<usize>()
        + deck
            .commanders
            .iter()
            .map(|entry| entry.quantity as usize)
            .sum::<usize>();

    DeckSource {
        source: source.to_string(),
        title: deck.name.clone(),
        url: url.to_string(),
        detail: format!(
            "USER REFERENCE • priority evidence • imported decklist with {} card(s). The Agent should compare its gameplan, packages, card overlap, mana structure, interaction, and win conditions against the current deck without blindly copying it.",
            card_count,
        ),
        user_provided: true,
        priority: 100,
        reference_deck: Some(deck),
        content_excerpt: String::new(),
    }
}

fn import_web_reference(url: &str) -> Result<DeckSource> {
    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .context("Could not build user-reference HTTP client")?;

    let response = client
        .get(url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "text/html,text/plain,application/json")
        .send()
        .with_context(|| {
            format!("Could not fetch user reference {url}")
        })?
        .error_for_status()
        .with_context(|| {
            format!("User reference returned an HTTP error: {url}")
        })?;

    let text = response
        .text()
        .context("Could not read user-reference response")?;

    let title = extract_title(&text)
        .unwrap_or_else(|| url.to_string());

    let excerpt = strip_html(&text)
        .chars()
        .take(12_000)
        .collect::<String>();

    Ok(DeckSource {
        source: "User • Web".to_string(),
        title,
        url: url.to_string(),
        detail:
            "USER REFERENCE • priority strategic evidence supplied directly by the user. Cached excerpt is provided to the Agent as guidance, not rules authority."
                .to_string(),
        user_provided: true,
        priority: 90,
        reference_deck: None,
        content_excerpt: excerpt,
    })
}

fn extract_title(html: &str) -> Option<String> {
    let re = Regex::new(
        r"(?is)<title[^>]*>(.*?)</title>",
    )
    .ok()?;

    let raw = re
        .captures(html)?
        .get(1)?
        .as_str();

    let title = strip_html(raw);

    (!title.trim().is_empty()).then_some(title)
}

fn strip_html(value: &str) -> String {
    let without_script = Regex::new(
        r"(?is)<script[^>]*>.*?</script>",
    )
    .ok()
    .map(|re| re.replace_all(value, " ").to_string())
    .unwrap_or_else(|| value.to_string());

    let without_style = Regex::new(
        r"(?is)<style[^>]*>.*?</style>",
    )
    .ok()
    .map(|re| re.replace_all(&without_script, " ").to_string())
    .unwrap_or(without_script);

    let without_tags = Regex::new(r"(?s)<[^>]+>")
        .ok()
        .map(|re| re.replace_all(&without_style, " ").to_string())
        .unwrap_or(without_style);

    without_tags
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
