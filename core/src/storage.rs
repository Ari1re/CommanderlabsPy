//! What Commander Lab keeps on disk, and cleaning the parts that can be
//! downloaded or rebuilt again. Decks (with their history, game plans and
//! sources), settings and the game log are never touched.

use anyhow::{bail, Result};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// A group of files the user can clean.
struct Category {
    id: &'static str,
    label: &'static str,
    description: &'static str,
    /// Files or folders, relative to the data folder.
    paths: &'static [&'static str],
    /// Shown before cleaning, when cleaning has a cost.
    warning: Option<&'static str>,
}

const CATEGORIES: &[Category] = &[
    Category {
        id: "research",
        label: "Deck research",
        description: "EDHREC, PDHREC, decklist and tournament data saved while building and suggesting. Downloaded again when needed.",
        paths: &[
            "data/edhrec_cache",
            "data/reference_cache",
            "data/format-sources",
            "data/source_cache/reference_decks",
            "data/source_cache/search_pages",
            "data/source_cache/crawl_state",
            "data/source_cache/commander_archives",
            "data/source_cache/mtgjson_decklist.json",
        ],
        warning: Some("The next build or suggestion for each commander takes longer while it downloads again."),
    },
    Category {
        id: "images",
        label: "Card images",
        description: "Saved card and proxy images. Shown again from Scryfall or MPC Autofill when needed.",
        paths: &["data/images"],
        warning: None,
    },
    Category {
        id: "printings",
        label: "Printings and tags",
        description: "Lists of every printing and Scryfall's tag names, saved when you change a printing or search.",
        paths: &["data/printings", "data/scryfall-oracle-tags.json"],
        warning: None,
    },
    Category {
        id: "ai_research",
        label: "AI web research",
        description: "Pages the AI agent read while researching. Read again next time.",
        paths: &["data/ai_research_cache"],
        warning: None,
    },
    Category {
        id: "logs",
        label: "Logs",
        description: "Diagnostic logs and debug files.",
        paths: &["data/logs", "data/debug"],
        warning: None,
    },
    Category {
        id: "downloads",
        label: "Leftover downloads",
        description: "The raw card file kept after importing card data, and installed updates.",
        paths: &["data/oracle-cards.jsonl.gz", "data/updates"],
        warning: None,
    },
    Category {
        id: "models",
        label: "AI models",
        description: "Downloaded AI models for the built-in AI.",
        paths: &["data/models"],
        warning: Some("The built-in AI won't work until you download a model again in Settings → AI model."),
    },
];

/// Never cleaned, shown so the total adds up.
const KEPT: &[(&str, &[&str])] = &[
    ("Decks, their history and plans", &["decks"]),
    ("Exported files", &["exports"]),
    (
        "Settings, game log and your reference decks",
        &[
            "data/ui_settings.json",
            "data/ai_config.json",
            "data/mpc_config.json",
            "data/games.json",
            "data/update.json",
            "data/source_cache/user_references",
        ],
    ),
];

pub const CARD_DB: &str = "data/cards.sqlite";

#[derive(Serialize)]
pub struct CategoryUsage {
    pub id: String,
    pub label: String,
    pub description: String,
    pub bytes: u64,
    pub files: u64,
    pub warning: Option<String>,
}

#[derive(Serialize)]
pub struct KeptUsage {
    pub label: String,
    pub bytes: u64,
}

#[derive(Serialize)]
pub struct StorageReport {
    pub total: u64,
    pub cleanable: Vec<CategoryUsage>,
    /// The card database: needed, but it can be compacted.
    pub card_db: u64,
    pub kept: Vec<KeptUsage>,
    /// Everything else in the folder.
    pub other: u64,
    pub folder: String,
}

fn size_of(path: &Path) -> (u64, u64) {
    let Ok(meta) = fs::symlink_metadata(path) else { return (0, 0) };
    if meta.is_file() {
        return (meta.len(), 1);
    }
    if !meta.is_dir() {
        return (0, 0);
    }
    let mut bytes = 0;
    let mut files = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let (b, f) = size_of(&entry.path());
            bytes += b;
            files += f;
        }
    }
    (bytes, files)
}

fn paths_size(paths: &[&str]) -> (u64, u64) {
    paths.iter().map(|p| size_of(Path::new(p))).fold((0, 0), |(a, b), (c, d)| (a + c, b + d))
}

fn update_download_dir() -> PathBuf {
    std::env::temp_dir().join("commander-lab-update")
}

/// Sizes of everything in the data folder (the current directory).
pub fn report() -> StorageReport {
    let mut cleanable = Vec::new();
    for c in CATEGORIES {
        let (mut bytes, mut files) = paths_size(c.paths);
        if c.id == "downloads" {
            let (b, f) = size_of(&update_download_dir());
            bytes += b;
            files += f;
        }
        cleanable.push(CategoryUsage {
            id: c.id.into(),
            label: c.label.into(),
            description: c.description.into(),
            bytes,
            files,
            warning: c.warning.map(Into::into),
        });
    }
    let card_db = paths_size(&[CARD_DB, "data/cards.sqlite-wal", "data/cards.sqlite-shm"]).0;
    let kept: Vec<KeptUsage> = KEPT.iter().map(|(label, paths)| KeptUsage { label: (*label).into(), bytes: paths_size(paths).0 }).collect();
    let total = size_of(Path::new(".")).0;
    let accounted = cleanable.iter().map(|c| c.bytes).sum::<u64>() + card_db + kept.iter().map(|k| k.bytes).sum::<u64>()
        - size_of(&update_download_dir()).0;
    StorageReport {
        total,
        cleanable,
        card_db,
        kept,
        other: total.saturating_sub(accounted),
        folder: std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default(),
    }
}

fn remove(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => {
            fs::remove_dir_all(path)?;
            // Folders the app expects to exist come back empty.
            if path == Path::new("data/images") {
                fs::create_dir_all(path)?;
            }
            Ok(())
        }
        Ok(_) => Ok(fs::remove_file(path)?),
        Err(_) => Ok(()),
    }
}

/// Delete the chosen categories. Returns the bytes freed and anything that
/// couldn't be removed (a model file in use on Windows, say).
pub fn clean(ids: &[String]) -> Result<(u64, Vec<String>)> {
    let mut freed = 0;
    let mut problems = Vec::new();
    for id in ids {
        let Some(c) = CATEGORIES.iter().find(|c| c.id == id) else {
            bail!("Unknown storage category '{id}'.");
        };
        let mut paths: Vec<PathBuf> = c.paths.iter().map(PathBuf::from).collect();
        if c.id == "downloads" {
            paths.push(update_download_dir());
        }
        for p in paths {
            let (bytes, _) = size_of(&p);
            match remove(&p) {
                Ok(()) => freed += bytes,
                Err(err) => problems.push(format!("{}: {err}", p.display())),
            }
        }
    }
    Ok((freed, problems))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_never_touch_decks_or_settings() {
        for c in CATEGORIES {
            for p in c.paths {
                assert!(!p.starts_with("decks"), "{p}");
                assert!(!p.starts_with("exports"), "{p}");
                for (_, kept) in KEPT {
                    assert!(!kept.iter().any(|k| p.starts_with(k) || k.starts_with(p)), "{p} overlaps kept {kept:?}");
                }
                assert_ne!(*p, CARD_DB);
            }
        }
    }
}
