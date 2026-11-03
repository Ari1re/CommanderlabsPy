//! Web research for the agent: community discussion and rules.
//!
//! How it fits: [`research_commander_web`] searches Reddit and DuckDuckGo
//! for the commander and reads Wizards' rules pages, caching results for 14
//! days under `data/ai_research_cache`; the AI profile gets the excerpts.
//! Settings → "Research the web while optimizing" turns it off.
//!
//! Things to work on:
//! - Scraping DuckDuckGo's HTML page is fragile; it will break when their
//!   markup changes. Log failures clearly so it's noticed.
//! - Excerpts are long; on-device models are slow with long prompts, so
//!   summarise or cut them for the built-in engine.

use crate::{
    deep_agent::OptimizationGoal,
    logging,
    sources::DeckSource,
};
use anyhow::Result;
use regex::Regex;
use reqwest::{
    blocking::Client,
    header::{ACCEPT, USER_AGENT},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    hash::{Hash, Hasher},
    path::PathBuf,
    time::{Duration, SystemTime},
};

const CACHE_DIR: &str = "./data/ai_research_cache";
const CACHE_DAYS: u64 = 14;
const USER_AGENT_VALUE: &str =
    concat!("CommanderLab/", env!("CARGO_PKG_VERSION"), " local MTG research assistant");

/// One page found during web research: source, title, URL and a text excerpt.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebResearchDocument {
    pub source: String,
    pub title: String,
    pub url: String,
    pub text: String,
}

/// Everything web research found for one query (cached as a whole).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebResearchBundle {
    pub query: String,
    pub documents: Vec<WebResearchDocument>,
    pub sources: Vec<DeckSource>,
}

/// Web research for the agent's AI profile. Builds one query from the
/// commanders, "cEDH"/"EDH" and the goal (stax or win), then: Reddit search
/// (with top comments), DuckDuckGo results (page excerpts) and Wizards'
/// rules documents. Cached for 14 days; a stale cache is used if the
/// network fails. Note: for Pauper Commander it still searches "EDH".
pub fn research_commander_web<F>(
    commander_names: &[String],
    bracket: u8,
    goal: OptimizationGoal,
    mut progress: F,
) -> Result<WebResearchBundle>
where
    F: FnMut(String),
{
    let query = format!(
        "{} commander {} {}",
        commander_names.join(" + "),
        if bracket >= 5 { "cEDH" } else { "EDH" },
        if goal == OptimizationGoal::StaxLock {
            "stax primer lock"
        } else {
            "primer combo optimization"
        },
    );

    let key = cache_key(&query);
    let path = PathBuf::from(CACHE_DIR)
        .join(format!("{key:016x}.json"));

    if cache_fresh(&path) {
        if let Ok(text) = fs::read_to_string(&path) {
            if let Ok(bundle) =
                serde_json::from_str::<WebResearchBundle>(&text)
            {
                progress(format!(
                    "cache hit • {} document(s)",
                    bundle.documents.len(),
                ));
                return Ok(bundle);
            }
        }
    }

    let stale = fs::read_to_string(&path)
        .ok()
        .and_then(|text| {
            serde_json::from_str::<WebResearchBundle>(&text).ok()
        });

    let client = Client::builder()
        .timeout(Duration::from_secs(18))
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()?;

    let mut bundle = WebResearchBundle {
        query: query.clone(),
        ..WebResearchBundle::default()
    };

    progress("searching Reddit strategy discussions".to_string());

    if let Some(reddit) =
        fetch_reddit_search(&client, &query)
    {
        bundle.documents.extend(reddit);
    }

    progress("searching the public web for primers/build discussions".to_string());

    if let Some(results) =
        fetch_duckduckgo_search(&client, &query)
    {
        bundle.documents.extend(results);
    }

    progress("checking official Magic rules and comprehensive-rules links".to_string());

    for rules in fetch_official_rules_documents(&client) {
        bundle.documents.push(rules);
    }

    dedupe_documents(&mut bundle.documents);
    bundle.documents.truncate(36);

    bundle.sources = bundle
        .documents
        .iter()
        .map(|doc| DeckSource {
            source: doc.source.clone(),
            title: doc.title.clone(),
            url: doc.url.clone(),
            detail: "AI research evidence; community material is strategy evidence, not rules authority.".to_string(),
            user_provided: false,
            priority: 0,
            reference_deck: None,
            content_excerpt: doc.text.clone(),
        })
        .collect();

    if bundle.documents.is_empty() {
        if let Some(stale) = stale {
            progress(
                "network research unavailable; using stale cached research"
                    .to_string(),
            );
            return Ok(stale);
        }
    }

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(text) = serde_json::to_string_pretty(&bundle) {
        let _ = fs::write(&path, text);
    }

    logging::record(
        "AI-WEB",
        format!(
            "RESEARCH COMPLETE • query={} • documents={}",
            query,
            bundle.documents.len(),
        ),
    );

    progress(format!(
        "research complete • {} document(s)",
        bundle.documents.len(),
    ));

    Ok(bundle)
}

/// Reddit's JSON search for the query; each hit's post text and permalink.
fn fetch_reddit_search(
    client: &Client,
    query: &str,
) -> Option<Vec<WebResearchDocument>> {
    let url = format!(
        "https://www.reddit.com/search.json?q={}&limit=20&sort=relevance&t=all&type=link",
        urlencoding::encode(query),
    );

    let value = client
        .get(&url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "application/json")
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json::<Value>()
        .ok()?;

    let children = value
        .pointer("/data/children")
        .and_then(Value::as_array)?;

    let mut docs = Vec::new();

    for child in children.iter().take(20) {
        let data = child.get("data")?;
        let title = data
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Reddit discussion")
            .to_string();
        let selftext = data
            .get("selftext")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let permalink = data
            .get("permalink")
            .and_then(Value::as_str)
            .unwrap_or("");

        if selftext.trim().len() < 40 {
            continue;
        }

        let public_url =
            format!("https://www.reddit.com{permalink}");

        let mut text = strip_html(&selftext);

        if docs.len() < 6 {
            if let Some(comments) =
                fetch_reddit_comments(client, &public_url)
            {
                if !comments.is_empty() {
                    text.push_str("\nCOMMUNITY COMMENTS:\n");
                    text.push_str(&comments.join("\n"));
                }
            }
        }

        docs.push(WebResearchDocument {
            source: "Reddit".to_string(),
            title,
            url: public_url,
            text: text.chars().take(5000).collect(),
        });
    }

    Some(docs)
}

/// Top comments of a Reddit thread (where the useful advice usually is).
fn fetch_reddit_comments(
    client: &Client,
    public_url: &str,
) -> Option<Vec<String>> {
    let url = format!(
        "{}.json?limit=12&sort=top",
        public_url.trim_end_matches('/'),
    );

    let value = client
        .get(&url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "application/json")
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json::<Value>()
        .ok()?;

    let listing = value
        .as_array()?
        .get(1)?
        .pointer("/data/children")?
        .as_array()?;

    let comments = listing
        .iter()
        .filter_map(|child| {
            child
                .pointer("/data/body")
                .and_then(Value::as_str)
        })
        .filter(|body| body.trim().len() >= 30)
        .take(8)
        .map(|body| {
            format!(
                "- {}",
                strip_html(body)
                    .chars()
                    .take(700)
                    .collect::<String>(),
            )
        })
        .collect::<Vec<_>>();

    Some(comments)
}

/// Result links from DuckDuckGo's HTML page, then excerpts of the best few.
/// Fragile: depends on DuckDuckGo's markup.
fn fetch_duckduckgo_search(
    client: &Client,
    query: &str,
) -> Option<Vec<WebResearchDocument>> {
    let url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        urlencoding::encode(query),
    );

    let html = client
        .get(&url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "text/html")
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .ok()?;

    let link_re = Regex::new(
        r#"<a[^>]+class="result__a"[^>]+href="([^"]+)"[^>]*>(.*?)</a>"#
    ).ok()?;

    let snippet_re = Regex::new(
        r#"<a[^>]+class="result__snippet"[^>]*>(.*?)</a>|<div[^>]+class="result__snippet"[^>]*>(.*?)</div>"#
    ).ok()?;

    let links = link_re
        .captures_iter(&html)
        .take(18)
        .map(|cap| {
            (
                normalize_search_url(
                    cap.get(1).map(|m| m.as_str()).unwrap_or("")
                ),
                strip_html(cap.get(2).map(|m| m.as_str()).unwrap_or("")),
            )
        })
        .collect::<Vec<_>>();

    let snippets = snippet_re
        .captures_iter(&html)
        .take(18)
        .map(|cap| {
            strip_html(
                cap.get(1)
                    .or_else(|| cap.get(2))
                    .map(|m| m.as_str())
                    .unwrap_or(""),
            )
        })
        .collect::<Vec<_>>();

    let mut docs = Vec::new();

    for (index, (url, title)) in
        links.into_iter().enumerate()
    {
        if url.is_empty() || title.is_empty() {
            continue;
        }

        let snippet =
            snippets.get(index).cloned().unwrap_or_default();

        let page_text = if docs.len() < 8
            && (url.starts_with("https://")
                || url.starts_with("http://"))
            && !url.contains("reddit.com")
        {
            fetch_page_excerpt(client, &url)
                .unwrap_or_default()
        } else {
            String::new()
        };

        let text = if page_text.is_empty() {
            snippet
        } else if snippet.is_empty() {
            page_text
        } else {
            format!(
                "SEARCH SUMMARY: {snippet}\nPAGE EXCERPT: {page_text}"
            )
        };

        docs.push(WebResearchDocument {
            source: "Web Search".to_string(),
            title,
            url,
            text,
        });
    }

    Some(docs)
}

/// A page's title and the start of its text.
fn fetch_page_excerpt(
    client: &Client,
    url: &str,
) -> Option<String> {
    let html = client
        .get(url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "text/html")
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .ok()?;

    let text = strip_html(&html);

    if text.len() < 80 {
        return None;
    }

    Some(text.chars().take(5000).collect())
}

/// Links to the current Comprehensive Rules and related documents from
/// Wizards' rules page.
fn fetch_official_rules_documents(
    client: &Client,
) -> Vec<WebResearchDocument> {
    let landing_url = "https://magic.wizards.com/en/rules";

    let Some(html) = client
        .get(landing_url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "text/html")
        .send()
        .ok()
        .and_then(|response| response.error_for_status().ok())
        .and_then(|response| response.text().ok())
    else {
        return Vec::new();
    };

    let mut docs = vec![WebResearchDocument {
        source: "Wizards".to_string(),
        title: "Magic: The Gathering Rules".to_string(),
        url: landing_url.to_string(),
        text: strip_html(&html)
            .chars()
            .take(5000)
            .collect(),
    }];

    let link_re = match Regex::new(
        r#"href=["']([^"']*(?:MagicCompRules|CompRules|comprehensive)[^"']*\.(?:txt|html?))["']"#,
    ) {
        Ok(re) => re,
        Err(_) => return docs,
    };

    for capture in link_re.captures_iter(&html).take(3) {
        let Some(raw) = capture.get(1).map(|m| m.as_str()) else {
            continue;
        };

        let url = if raw.starts_with("http://")
            || raw.starts_with("https://")
        {
            raw.to_string()
        } else if raw.starts_with("//") {
            format!("https:{raw}")
        } else if raw.starts_with('/') {
            format!("https://magic.wizards.com{raw}")
        } else {
            format!("https://magic.wizards.com/{raw}")
        };

        let Some(text) = client
            .get(&url)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ACCEPT, "text/plain, text/html")
            .send()
            .ok()
            .and_then(|response| response.error_for_status().ok())
            .and_then(|response| response.text().ok())
        else {
            continue;
        };

        let cleaned = if url.to_ascii_lowercase().ends_with(".txt") {
            text
        } else {
            strip_html(&text)
        };

        docs.push(WebResearchDocument {
            source: "Wizards Comprehensive Rules".to_string(),
            title: "Magic Comprehensive Rules".to_string(),
            url,
            text: cleaned.chars().take(12000).collect(),
        });
    }

    docs
}

/// Drop documents with the same URL.
fn dedupe_documents(docs: &mut Vec<WebResearchDocument>) {
    let mut seen = std::collections::HashSet::new();

    docs.retain(|doc| {
        let key = format!(
            "{}:{}",
            doc.url.to_ascii_lowercase(),
            doc.title.to_ascii_lowercase(),
        );

        seen.insert(key)
    });
}

/// Rough HTML → text.
fn strip_html(value: &str) -> String {
    let without_tags = Regex::new(r"<[^>]+>")
        .ok()
        .map(|re| re.replace_all(value, " ").to_string())
        .unwrap_or_else(|| value.to_string());

    html_decode(&without_tags)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Turn DuckDuckGo's redirect links into the real page URL.
fn normalize_search_url(value: &str) -> String {
    let decoded = html_decode(value);

    if let Some(index) = decoded.find("uddg=") {
        let encoded = &decoded[index + 5..];
        let encoded = encoded
            .split('&')
            .next()
            .unwrap_or(encoded);

        if let Ok(url) = urlencoding::decode(encoded) {
            return url.into_owned();
        }
    }

    if decoded.starts_with("//") {
        return format!("https:{decoded}");
    }

    decoded
}

/// Undo common HTML entities.
fn html_decode(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

/// Hash for cache file names (DefaultHasher: not stable across Rust versions).
fn cache_key(value: &str) -> u64 {
    let mut hasher =
        std::collections::hash_map::DefaultHasher::new();
    value.to_ascii_lowercase().hash(&mut hasher);
    hasher.finish()
}

/// Cached research is reused for CACHE_DAYS (14).
fn cache_fresh(path: &PathBuf) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };

    let Ok(modified) = metadata.modified() else {
        return false;
    };

    let Ok(age) =
        SystemTime::now().duration_since(modified)
    else {
        return false;
    };

    age <= Duration::from_secs(CACHE_DAYS * 24 * 60 * 60)
}
