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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckEntry {
    pub quantity: u32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedDeck {
    pub name: String,
    pub commanders: Vec<DeckEntry>,
    pub main: Vec<DeckEntry>,
    pub sideboard: Vec<DeckEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Main,
    Commander,
    Sideboard,
    Ignore,
}

#[derive(Debug, Clone)]
struct ParsedLine {
    entry: DeckEntry,
    section: Section,
    explicit_commander: bool,
    block: usize,
}

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

pub fn parse_deck_file(path: &Path) -> Result<ParsedDeck> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("Could not read {}", path.display()))?;

    let name = path
        .file_stem()
        .and_then(|x| x.to_str())
        .unwrap_or("Unknown Deck");

    parse_deck_text(name, &text)
}

pub fn parse_deck_text(deck_name: &str, text: &str) -> Result<ParsedDeck> {
    let filename_hints = commander_filename_hints(deck_name);

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

    let mut sideboard = Vec::new();
    let mut included = Vec::new();

    for line in parsed {
        if line.section == Section::Sideboard {
            sideboard.push(line.entry);
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
    })
}

pub fn deck_to_text(deck: &ParsedDeck) -> String {
    let mut out = String::new();

    if !deck.commanders.is_empty() {
        out.push_str("Commander\n");

        for entry in &deck.commanders {
            out.push_str(&format!("{} {}\n", entry.quantity, entry.name));
        }

        out.push('\n');
    }

    out.push_str("Deck\n");

    for entry in &deck.main {
        out.push_str(&format!("{} {}\n", entry.quantity, entry.name));
    }

    if !deck.sideboard.is_empty() {
        out.push_str("\nSideboard\n");

        for entry in &deck.sideboard {
            out.push_str(&format!("{} {}\n", entry.quantity, entry.name));
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

fn moxfield_line(quantity: u32, printing: &ExportPrinting) -> String {
    match (&printing.set_code, &printing.collector_number) {
        (Some(set), Some(number)) => {
            format!("{} {} ({}) {}", quantity, printing.name, set, number)
        }
        _ => format!("{} {}", quantity, printing.name),
    }
}

/// Moxfield-importable decklist.
///
/// Uses the Commander / Deck / Sideboard section format that Moxfield's
/// "Import" dialog accepts, and adds `(SET) collector#` to each line when a
/// printing is known so Moxfield picks the same card art.
pub fn deck_to_moxfield_text<F>(deck: &ParsedDeck, mut lookup: F) -> String
where
    F: FnMut(&str) -> ExportPrinting,
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
            out.push_str(&moxfield_line(entry.quantity, &lookup(&entry.name)));
            out.push('\n');
        }
    };

    section("Commander", &deck.commanders, &mut out);
    section("Deck", &deck.main, &mut out);
    section("Sideboard", &deck.sideboard, &mut out);

    out
}

pub fn write_deck_file(path: &Path, deck: &ParsedDeck) -> Result<()> {
    fs::write(path, deck_to_text(deck))
        .with_context(|| format!("Could not write {}", path.display()))?;

    Ok(())
}

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
        | "considering"
        | "consider"
        | "companion"
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

fn is_comment(line: &str) -> bool {
    line.starts_with("//")
        || line.starts_with('#')
        || line.starts_with(';')
}

fn parse_card_line(line: &str) -> Option<(DeckEntry, bool)> {
    static QUANTITY_RE: OnceLock<Regex> = OnceLock::new();
    static SET_SUFFIX_RE: OnceLock<Regex> = OnceLock::new();
    static TRAILING_FLAGS_RE: OnceLock<Regex> = OnceLock::new();

    let quantity_re = QUANTITY_RE.get_or_init(|| {
        Regex::new(r"(?i)^\s*(\d+)\s*(?:x\s*)?(.+?)\s*$")
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

    if let Some(hash_pos) = name.find(" #") {
        name.truncate(hash_pos);
        name = name.trim().to_string();
    }

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

    Some((
        DeckEntry {
            quantity,
            name,
        },
        explicit_commander,
    ))
}

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

pub fn normalize_multiface_separator(value: &str) -> String {
    static SINGLE_SLASH_RE: OnceLock<Regex> = OnceLock::new();

    let re = SINGLE_SLASH_RE.get_or_init(|| {
        Regex::new(r"\s+/+\s+")
            .expect("valid multiface slash regex")
    });

    re.replace_all(value.trim(), " // ").to_string()
}

fn collapse_spaces(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

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
        let text = deck_to_moxfield_text(&deck, |name| ExportPrinting {
            name: name.to_string(),
            set_code: (name == "Sol Ring").then(|| "C21".to_string()),
            collector_number: (name == "Sol Ring").then(|| "263".to_string()),
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
