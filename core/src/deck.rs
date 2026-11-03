//! Decklists: parsing, writing and the deck data types.
//!
//! How it fits: decks are plain text files (Moxfield-style) so they survive
//! outside the app. [`parse_deck_text`] reads nearly any export format:
//! "Commander"/"Deck" headings, `*CMDR*`, printings `(SET) 123`, `*F*` foil
//! and Moxfield tags `#Ramp`. [`deck_to_text`] / [`write_deck_file`] write
//! them back, keeping printings and tags. [`DeckEntry`] is one line;
//! [`ParsedDeck`] holds the zones (commanders, main, sideboard, considering).
//!
//! Things to work on:
//! - The line parser is a series of regexes and string cuts; the order of
//!   steps matters (foil before tags bit us). A small tokenizer would be
//!   sturdier.
//! - Archidekt-style categories ("[Ramp]") could map to tags on import.

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(Debug, Clone)]
pub struct DeckFile {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DeckEntry {
    pub quantity: u32,
    pub name: String,
    /// Chosen printing, kept in the decklist as `(SET) number`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collector_number: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub foil: bool,
    /// The player's tags ("Ramp", "Win Condition"…), kept in the decklist
    /// Moxfield's way: `1 Sol Ring #Ramp #Staple`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl DeckEntry {
    /// A plain line: no printing, foil or tags.
    pub fn new(quantity: u32, name: impl Into<String>) -> Self {
        Self { quantity, name: name.into(), ..Default::default() }
    }

    /// `1 Sol Ring (C21) 263 *F*` — the printing suffix Moxfield understands.
    pub fn line(&self) -> String {
        let mut out = format!("{} {}", self.quantity, self.name);
        if let (Some(set), Some(num)) = (&self.set_code, &self.collector_number) {
            out.push_str(&format!(" ({}) {}", set.to_ascii_uppercase(), num));
        }
        if self.foil {
            out.push_str(" *F*");
        }
        for tag in &self.tags {
            out.push_str(&format!(" #{tag}"));
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ParsedDeck {
    pub name: String,
    pub commanders: Vec<DeckEntry>,
    pub main: Vec<DeckEntry>,
    pub sideboard: Vec<DeckEntry>,
    /// Format the deck is built for. Saved in the decklist as a
    /// `// Format: …` comment line; missing means Commander.
    #[serde(default, skip_serializing_if = "is_commander_format")]
    pub format: crate::format::Format,
    /// Cards you're thinking about (Moxfield's "Considering" / maybeboard):
    /// kept with the deck but not part of it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub considering: Vec<DeckEntry>,
}

/// Serde helper: Commander is the default, so it isn't written out.
fn is_commander_format(f: &crate::format::Format) -> bool {
    f.is_commander()
}

/// Which part of a list a line belongs to while parsing ("Ignore" covers
/// tokens, companions, wishlists).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Main,
    Commander,
    Sideboard,
    Considering,
    Ignore,
}

/// A card line as read, with its section, whether it carried a commander
/// marker, and which blank-line-separated block it was in (used to guess
/// the commander when the list doesn't say).
#[derive(Debug, Clone)]
struct ParsedLine {
    entry: DeckEntry,
    section: Section,
    explicit_commander: bool,
    block: usize,
}

/// Every deck file in the folder (.txt, .dek, .deck), named by file name,
/// sorted by name. Sidecars (.plan.json…) are skipped by extension.
pub fn load_deck_files(directory: &str) -> Result<Vec<DeckFile>> {
    fs::create_dir_all(directory)?;

    let mut decks = Vec::new();

    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let accepted = path
            .extension()
            .and_then(|x| x.to_str())
            .map(|ext| {
                ext.eq_ignore_ascii_case("txt")
                    || ext.eq_ignore_ascii_case("dek")
                    || ext.eq_ignore_ascii_case("deck")
            })
            .unwrap_or(false);

        if !accepted {
            continue;
        }

        let name = path
            .file_stem()
            .and_then(|x| x.to_str())
            .unwrap_or("Unknown Deck")
            .to_string();

        decks.push(DeckFile { name, path });
    }

    decks.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(decks)
}

/// Read and parse a deck file; the file name becomes the deck name.
pub fn parse_deck_file(path: &Path) -> Result<ParsedDeck> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("Could not read {}", path.display()))?;

    let name = path
        .file_stem()
        .and_then(|x| x.to_str())
        .unwrap_or("Unknown Deck");

    parse_deck_text(name, &text)
}

/// Parse nearly any decklist export. Reads line by line: comments
/// (`// Format: …` sets the format), section headings, card lines. Then
/// decides the commander, in order:
/// 1. a "Commander" section or a `*CMDR*`-style marker;
/// 2. one or two cards whose name matches the deck/file name exactly;
/// 3. a last block of 1–2 single cards after 80+ cards (how many exports
///    put the commander at the end);
/// 4. otherwise no commander.
/// TODO: guesses 2 and 3 also run for Canadian Highlander lists, which have
/// no commander; skipping them when `format` isn't a commander format would
/// be safer.
pub fn parse_deck_text(deck_name: &str, text: &str) -> Result<ParsedDeck> {
    let filename_hints = commander_filename_hints(deck_name);
    let mut format = crate::format::Format::Commander;

    let mut section = Section::Main;
    let mut parsed = Vec::<ParsedLine>::new();
    let mut block = 0usize;
    let mut saw_card_in_current_block = false;

    for raw in text.lines() {
        let line = raw.trim();

        if line.is_empty() {
            if saw_card_in_current_block {
                block += 1;
                saw_card_in_current_block = false;
            }
            continue;
        }

        if is_comment(line) {
            if let Some(found) = format_comment(line) {
                format = found;
            }
            continue;
        }

        if let Some(next_section) = detect_section_header(line) {
            section = next_section;

            if saw_card_in_current_block {
                block += 1;
                saw_card_in_current_block = false;
            }

            continue;
        }

        if section == Section::Ignore {
            continue;
        }

        let Some((entry, explicit_commander)) = parse_card_line(line) else {
            continue;
        };

        saw_card_in_current_block = true;

        parsed.push(ParsedLine {
            entry,
            section,
            explicit_commander,
            block,
        });
    }

    // Sideboard and Considering lines are set aside; the rest decide the
    // commander below.
    let mut sideboard = Vec::new();
    let mut considering = Vec::new();
    let mut included = Vec::new();

    for line in parsed {
        if line.section == Section::Sideboard {
            sideboard.push(line.entry);
        } else if line.section == Section::Considering {
            considering.push(line.entry);
        } else {
            included.push(line);
        }
    }

    let explicit_exists = included.iter().any(|line| {
        line.section == Section::Commander || line.explicit_commander
    });

    if explicit_exists {
        let mut commanders = Vec::new();
        let mut main = Vec::new();

        for line in included {
            if line.section == Section::Commander || line.explicit_commander {
                commanders.push(line.entry);
            } else if line.section == Section::Main {
                main.push(line.entry);
            }
        }

        return Ok(ParsedDeck {
            name: deck_name.to_string(),
            commanders,
            main,
            sideboard,
            format: format,
            considering: considering.clone(),
        });
    }

    let filename_matches = included
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            if filename_hints.iter().any(|hint| filename_matches_card(hint, &line.entry.name)) {
                Some(index)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    if !filename_matches.is_empty() && filename_matches.len() <= 2 {
        let mut commanders = Vec::new();
        let mut main = Vec::new();

        for (index, line) in included.into_iter().enumerate() {
            if filename_matches.contains(&index) {
                commanders.push(line.entry);
            } else if line.section == Section::Main {
                main.push(line.entry);
            }
        }

        return Ok(ParsedDeck {
            name: deck_name.to_string(),
            commanders,
            main,
            sideboard,
            format: format,
            considering: considering.clone(),
        });
    }

    if let Some(max_block) = included.iter().map(|line| line.block).max() {
        let final_indices = included
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                if line.block == max_block && line.section == Section::Main {
                    Some(index)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let preceding_quantity: u32 = included
            .iter()
            .filter(|line| line.block < max_block && line.section == Section::Main)
            .map(|line| line.entry.quantity)
            .sum();

        let final_is_small_commander_block =
            (1..=2).contains(&final_indices.len())
                && preceding_quantity >= 80
                && final_indices
                    .iter()
                    .all(|index| included[*index].entry.quantity == 1);

        if final_is_small_commander_block {
            let mut commanders = Vec::new();
            let mut main = Vec::new();

            for (index, line) in included.into_iter().enumerate() {
                if final_indices.contains(&index) {
                    commanders.push(line.entry);
                } else if line.section == Section::Main {
                    main.push(line.entry);
                }
            }

            return Ok(ParsedDeck {
                name: deck_name.to_string(),
                commanders,
                main,
                sideboard,
                format: format,
                considering: considering.clone(),
            });
        }
    }

    let main = included
        .into_iter()
        .filter(|line| line.section == Section::Main)
        .map(|line| line.entry)
        .collect::<Vec<_>>();

    Ok(ParsedDeck {
        name: deck_name.to_string(),
        commanders: Vec::new(),
        main,
        sideboard,
        format,
        considering: considering.clone(),
    })
}

/// Write a deck in Commander Lab's own format (Moxfield-compatible): an
/// optional `// Format:` line, then Commander, Deck, Sideboard and
/// Considering sections, each line with printing, foil and tags.
pub fn deck_to_text(deck: &ParsedDeck) -> String {
    let mut out = String::new();

    if !deck.format.is_commander() {
        out.push_str(&format!("// Format: {}\n", deck.format.label()));
    }

    if !deck.commanders.is_empty() {
        out.push_str("Commander\n");

        for entry in &deck.commanders {
            out.push_str(&entry.line());
            out.push('\n');
        }

        out.push('\n');
    }

    out.push_str("Deck\n");

    for entry in &deck.main {
        out.push_str(&entry.line());
            out.push('\n');
    }

    if !deck.sideboard.is_empty() {
        out.push_str("\nSideboard\n");

        for entry in &deck.sideboard {
            out.push_str(&entry.line());
            out.push('\n');
        }
    }

    if !deck.considering.is_empty() {
        out.push_str("\nConsidering\n");

        for entry in &deck.considering {
            out.push_str(&entry.line());
            out.push('\n');
        }
    }

    out
}

/// Printing details used for set-aware exports.
#[derive(Debug, Clone, Default)]
pub struct ExportPrinting {
    pub name: String,
    pub set_code: Option<String>,
    pub collector_number: Option<String>,
}

/// One export line: the deck's own printing if set, else the default one.
fn moxfield_line(entry: &DeckEntry, printing: &ExportPrinting) -> String {
    DeckEntry {
        quantity: entry.quantity,
        name: printing.name.clone(),
        set_code: entry.set_code.clone().or_else(|| printing.set_code.clone()),
        collector_number: if entry.set_code.is_some() {
            entry.collector_number.clone()
        } else {
            printing.collector_number.clone()
        },
        foil: entry.foil,
        tags: entry.tags.clone(),
    }
    .line()
}

/// Moxfield-importable decklist.
///
/// Uses the Commander / Deck / Sideboard section format that Moxfield's
/// "Import" dialog accepts, and adds `(SET) collector#` to each line when a
/// printing is known so Moxfield picks the same card art.
pub fn deck_to_moxfield_text<F>(deck: &ParsedDeck, mut lookup: F) -> String
where
    F: FnMut(&DeckEntry) -> ExportPrinting,
{
    let mut out = String::new();

    let mut section = |title: &str, entries: &[DeckEntry], out: &mut String| {
        if entries.is_empty() {
            return;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(title);
        out.push('\n');
        for entry in entries {
            out.push_str(&moxfield_line(entry, &lookup(entry)));
            out.push('\n');
        }
    };

    section("Commander", &deck.commanders, &mut out);
    section("Deck", &deck.main, &mut out);
    section("Sideboard", &deck.sideboard, &mut out);

    out
}

/// Save a deck as text (no versioning; `Engine::commit` does that).
pub fn write_deck_file(path: &Path, deck: &ParsedDeck) -> Result<()> {
    fs::write(path, deck_to_text(deck))
        .with_context(|| format!("Could not write {}", path.display()))?;

    Ok(())
}

/// A name that's safe as a file name on every platform: letters, digits,
/// spaces, `-`, `_`, `,` and `'`; everything else becomes `_`.
pub fn safe_deck_filename(name: &str) -> String {
    let cleaned = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric()
                || c == ' '
                || c == '-'
                || c == '_'
                || c == ','
                || c == '\''
            {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();

    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "Imported Deck".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Recognise a section heading in any common spelling ("Commander:",
/// "[Sideboard]", "Maybeboard"…).
fn detect_section_header(line: &str) -> Option<Section> {
    let heading = line
        .trim_matches('/')
        .trim_matches('[')
        .trim_matches(']')
        .trim_end_matches(':')
        .trim()
        .to_ascii_lowercase();

    match heading.as_str() {
        "commander"
        | "commanders"
        | "commander(s)"
        | "command zone"
        | "commandzone" => Some(Section::Commander),

        "deck"
        | "main"
        | "mainboard"
        | "main board"
        | "maindeck"
        | "main deck"
        | "library" => Some(Section::Main),

        "sideboard"
        | "side board"
        | "side"
        | "wishboard"
        | "wish board" => Some(Section::Sideboard),

        "maybeboard"
        | "maybe board"
        | "maybe"
        | "considering"
        | "consider" => Some(Section::Considering),

        "companion"
        | "companions"
        | "tokens"
        | "token"
        | "acquireboard"
        | "acquire board"
        | "wishlist"
        | "wish list" => Some(Section::Ignore),

        _ => None,
    }
}

/// `// Format: Pauper Commander` → the format named in a comment line.
fn format_comment(line: &str) -> Option<crate::format::Format> {
    let body = line.trim_start_matches(['/', '#', ';']).trim();
    let (key, value) = body.split_once(':')?;
    key.trim().eq_ignore_ascii_case("format").then(|| crate::format::Format::parse(value))
}

/// Lines starting with //, # or ; are comments.
fn is_comment(line: &str) -> bool {
    line.starts_with("//")
        || line.starts_with('#')
        || line.starts_with(';')
}

/// Parse "1 Name (SET) 123 *F* #Tag" (and "1x Name", markers, Arena-style
/// suffixes). Returns the entry and whether it was marked as a commander.
/// Order matters: commander markers, trailing flags, tags, printing, flags
/// again, then name clean-up.
fn parse_card_line(line: &str) -> Option<(DeckEntry, bool)> {
    static QUANTITY_RE: OnceLock<Regex> = OnceLock::new();
    static SET_SUFFIX_RE: OnceLock<Regex> = OnceLock::new();
    static TRAILING_FLAGS_RE: OnceLock<Regex> = OnceLock::new();

    let quantity_re = QUANTITY_RE.get_or_init(|| {
        // "1x Sol Ring" / "1 x Sol Ring": the x needs a space after it, or
        // it eats the X of "1 Xantcha, Sleeper Agent" / "1 Xander's Lounge".
        Regex::new(r"(?i)^\s*(\d+)\s*(?:x\s+)?(.+?)\s*$")
            .expect("valid quantity regex")
    });

    let set_suffix_re = SET_SUFFIX_RE.get_or_init(|| {
        Regex::new(
            r"(?i)\s+\([A-Z0-9]+\)\s+[A-Z0-9][A-Z0-9\-★a-z]*\s*$"
        )
        .expect("valid set suffix regex")
    });

    let trailing_flags_re = TRAILING_FLAGS_RE.get_or_init(|| {
        Regex::new(r"(?i)(?:\s+\*[A-Z0-9]+\*)+\s*$")
            .expect("valid trailing flag regex")
    });

    let captures = quantity_re.captures(line)?;

    let quantity = captures
        .get(1)?
        .as_str()
        .parse::<u32>()
        .ok()?;

    if quantity == 0 {
        return None;
    }

    let mut name = captures.get(2)?.as_str().trim().to_string();

    let explicit_commander = contains_commander_marker(&name);

    name = strip_commander_markers(&name);

    name = trailing_flags_re
        .replace(&name, "")
        .trim()
        .to_string();

    // Moxfield tags: "#Ramp #Card Draw" ("#!Tag" marks a global tag).
    let mut tags = Vec::new();
    if let Some(hash_pos) = name.find(" #") {
        for tag in name[hash_pos..].split(" #") {
            let tag = tag.trim().trim_start_matches('!').trim();
            if !tag.is_empty() && !tags.iter().any(|t: &String| t.eq_ignore_ascii_case(tag)) {
                tags.push(tag.to_string());
            }
        }
        name.truncate(hash_pos);
        // "*F*" sits before the tags, so strip it again now it's last.
        name = trailing_flags_re.replace(&name, "").trim().to_string();
    }

    static PRINTING_RE: OnceLock<Regex> = OnceLock::new();
    let printing_re = PRINTING_RE.get_or_init(|| {
        Regex::new(r"(?i)\s+\(([A-Z0-9]+)\)\s+([A-Z0-9][A-Z0-9\-★a-z]*)\s*$")
            .expect("valid printing regex")
    });
    let foil = captures
        .get(2)
        .map(|m| {
            let raw = m.as_str().to_ascii_uppercase();
            raw.contains("*F*") || raw.contains("*E*")
        })
        .unwrap_or(false);
    let printing = printing_re.captures(&name).map(|c| {
        (c[1].to_ascii_uppercase(), c[2].to_string())
    });

    name = set_suffix_re
        .replace(&name, "")
        .trim()
        .to_string();

    name = trailing_flags_re
        .replace(&name, "")
        .trim()
        .to_string();

    name = normalize_multiface_separator(&name);
    name = collapse_spaces(&name);

    if name.is_empty() {
        return None;
    }

    let (set_code, collector_number) = match printing {
        Some((set, num)) => (Some(set), Some(num)),
        None => (None, None),
    };

    Some((
        DeckEntry {
            quantity,
            name,
            set_code,
            collector_number,
            foil,
            tags,
        },
        explicit_commander,
    ))
}

/// `*CMDR*`, `[Commander]`, `{cmdr}` and similar markers.
fn contains_commander_marker(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();

    [
        "*cmdr*",
        "*commander*",
        "[commander]",
        "[cmdr]",
        "{commander}",
        "{cmdr}",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// Remove those markers from a card name.
fn strip_commander_markers(value: &str) -> String {
    let mut result = value.to_string();

    for marker in [
        "*CMDR*",
        "*cmdr*",
        "*COMMANDER*",
        "*commander*",
        "[Commander]",
        "[commander]",
        "[CMDR]",
        "[cmdr]",
        "{Commander}",
        "{commander}",
        "{CMDR}",
        "{cmdr}",
    ] {
        result = result.replace(marker, "");
    }

    result.trim().to_string()
}

/// "Front / Back" or "Front /// Back" → "Front // Back".
pub fn normalize_multiface_separator(value: &str) -> String {
    static SINGLE_SLASH_RE: OnceLock<Regex> = OnceLock::new();

    let re = SINGLE_SLASH_RE.get_or_init(|| {
        Regex::new(r"\s+/+\s+")
            .expect("valid multiface slash regex")
    });

    re.replace_all(value.trim(), " // ").to_string()
}

/// Squeeze runs of whitespace to single spaces.
fn collapse_spaces(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Possible commander names from a deck name: split on " + ", " & " or
/// " | " for partner decks, else the whole name.
fn commander_filename_hints(deck_name: &str) -> Vec<String> {
    for separator in [" + ", " & ", " | "] {
        if deck_name.contains(separator) {
            return deck_name
                .split(separator)
                .map(normalize_for_matching)
                .filter(|hint| !hint.is_empty())
                .collect();
        }
    }

    vec![normalize_for_matching(deck_name)]
}

/// Does a deck-name hint name this card? Exact match on the full name or a
/// face, or a face that starts with the hint ("Kefka" → "Kefka, Court Mage").
fn filename_matches_card(filename_hint: &str, card_name: &str) -> bool {
    if filename_hint.is_empty() {
        return false;
    }

    let full = normalize_for_matching(card_name);

    if filename_hint == full {
        return true;
    }

    for face in card_name.split(" // ") {
        let face_normalized = normalize_for_matching(face);

        if filename_hint == face_normalized {
            return true;
        }

        if face_normalized.starts_with(&format!("{filename_hint} ")) {
            return true;
        }
    }

    false
}

/// Lowercase letters/digits separated by single spaces, for name hints.
fn normalize_for_matching(value: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moxfield_line() {
        let (entry, _) =
            parse_card_line("1 Eternal Witness (F08) 3 *F*").unwrap();
        assert_eq!(entry.name, "Eternal Witness");
    }

    #[test]
    fn moxfield_tags_round_trip() {
        let (entry, _) = parse_card_line("1 Sol Ring (CMM) 400 *F* #Ramp #!Card Draw").unwrap();
        assert_eq!(entry.name, "Sol Ring");
        assert_eq!(entry.set_code.as_deref(), Some("CMM"));
        assert!(entry.foil);
        assert_eq!(entry.tags, vec!["Ramp".to_string(), "Card Draw".to_string()]);
        assert_eq!(entry.line(), "1 Sol Ring (CMM) 400 *F* #Ramp #Card Draw");
        let (again, _) = parse_card_line(&entry.line()).unwrap();
        assert_eq!(again, entry);
    }

    #[test]
    fn mdfc_line() {
        let (entry, _) = parse_card_line(
            "1 Bala Ged Recovery / Bala Ged Sanctuary (ZNR) 180",
        )
        .unwrap();

        assert_eq!(
            entry.name,
            "Bala Ged Recovery // Bala Ged Sanctuary"
        );
    }

    #[test]
    fn moxfield_export_sections_and_printings() {
        let deck = parse_deck_text(
            "T",
            "Commander\n1 Kefka, Court Mage\n\nDeck\n1 Sol Ring\n1 Mystery Card\n\nSideboard\n1 Arcane Signet\n",
        )
        .unwrap();
        let text = deck_to_moxfield_text(&deck, |e| ExportPrinting {
            name: e.name.to_string(),
            set_code: (e.name == "Sol Ring").then(|| "C21".to_string()),
            collector_number: (e.name == "Sol Ring").then(|| "263".to_string()),
        });
        assert!(text.starts_with("Commander\n1 Kefka, Court Mage\n\nDeck\n"));
        assert!(text.contains("1 Sol Ring (C21) 263\n"));
        assert!(text.contains("1 Mystery Card\n"));
        assert!(text.ends_with("Sideboard\n1 Arcane Signet\n"));
        // Round-trips through our own importer.
        let back = parse_deck_text("T", &text).unwrap();
        assert_eq!(back.main.len(), 2);
        assert_eq!(back.main[0].name, "Sol Ring");
    }

    #[test]
    fn sideboard_preserved() {
        let text = "1 Sol Ring\n\nSideboard:\n1 Arcane Signet\n";
        let deck = parse_deck_text("Test", text).unwrap();
        assert_eq!(deck.main.len(), 1);
        assert_eq!(deck.sideboard.len(), 1);
    }

    /// Test helper: a sample decklist from the test fixtures folder, if present.
    fn fixture(name: &str) -> Option<String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        fs::read_to_string(path).ok()
    }

    #[test]
    fn parses_supplied_kefka_fixture() {
        // Fixtures are optional local files; skip when they are absent so
        // CI builds from a clean checkout still pass.
        let Some(text) = fixture("Kefka.txt") else { return };
        let text = text.as_str();
        let deck = parse_deck_text("Kefka", text).unwrap();

        assert_eq!(deck.commanders.len(), 1);
        assert_eq!(deck.commanders[0].name, "Kefka, Court Mage");
        assert!(deck.main.len() > 80);
    }

    #[test]
    fn parses_supplied_casal_fixture_and_sideboard() {
        let Some(text) = fixture("Casal, Lurkwood Pathfinder.txt") else { return };
        let text = text.as_str();

        let deck =
            parse_deck_text("Casal, Lurkwood Pathfinder", text).unwrap();

        assert_eq!(deck.commanders.len(), 1);
        assert_eq!(
            deck.commanders[0].name,
            "Casal, Lurkwood Pathfinder // Casal, Pathbreaker Owlbear"
        );
        assert!(!deck.sideboard.is_empty());
        assert!(
            deck.main
                .iter()
                .any(|entry| entry.name == "Bala Ged Recovery // Bala Ged Sanctuary")
        );
    }
}

#[cfg(test)]
mod considering_tests {
    use super::*;

    #[test]
    fn considering_round_trips() {
        let deck = parse_deck_text("D", "Commander\n1 Kefka, Court Mage\n\nDeck\n1 Sol Ring\n\nSideboard\n1 Duress\n\nMaybeboard\n1 Ponder\n").unwrap();
        assert_eq!(deck.considering.len(), 1);
        assert_eq!(deck.main.len(), 1);
        let text = deck_to_text(&deck);
        assert!(text.contains("Considering\n1 Ponder"));
        let back = parse_deck_text("D", &text).unwrap();
        assert_eq!(back.considering[0].name, "Ponder");
        assert_eq!(back.sideboard[0].name, "Duress");
    }
}

#[cfg(test)]
mod x_name_tests {
    use super::*;

    #[test]
    fn names_starting_with_x_keep_their_x() {
        for (line, qty, name) in [
            ("1 Xander's Lounge", 1, "Xander's Lounge"),
            ("1 Xantcha, Sleeper Agent", 1, "Xantcha, Sleeper Agent"),
            ("1x Xantcha, Sleeper Agent", 1, "Xantcha, Sleeper Agent"),
            ("2x Sol Ring", 2, "Sol Ring"),
            ("3 x Island", 3, "Island"),
            ("1 Xenagos, God of Revels", 1, "Xenagos, God of Revels"),
        ] {
            let (entry, _) = parse_card_line(line).expect(line);
            assert_eq!((entry.quantity, entry.name.as_str()), (qty, name), "{line}");
        }
    }
}
