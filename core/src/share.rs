//! Deck sharing by QR code or share code.
//!
//! Only what's needed to rebuild the deck is shared: name, commander(s),
//! cards and counts. Printings and the sideboard are optional extras.
//!
//! Formats (all decode on any device, no server involved):
//! * `commanderlab://deck/CLD2.…` — what QR codes hold: the text code as an
//!   app link, so a phone camera offers to open it in Commander Lab (the app
//!   registers the scheme; see desktop/src-tauri). About 30% denser than the
//!   old Base45 QR, but a plain-text QR gave "no app can open this".
//! * `CLD2:` + Base45(deflate(payload)) — older QR codes, still accepted.
//! * `CLD2.` + base64url(deflate(payload)) — the copyable text code (no
//!   spaces, survives chat apps).
//! * `CLD1.` — older JSON codes, still accepted.
//!
//! Payload: `name \x1f decklist`, where the decklist is the normal text
//! format.
//!
//! Things to work on:
//! - Tags travel with the extras (like printings), which makes QR codes
//!   denser. If codes get too big to scan, drop tags before printings.

use crate::deck::{deck_to_text, DeckEntry, ParsedDeck};
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use flate2::{read::DeflateDecoder, write::DeflateEncoder, Compression};
use qrcode::{render::svg, EcLevel, QrCode};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

const V1: &str = "CLD1.";
const V2_TEXT: &str = "CLD2.";
const V2_QR: &str = "CLD2:";
/// Prefix that makes a QR code an app link (`commanderlab://deck/<code>`).
pub const APP_LINK: &str = "commanderlab://deck/";
const SEP: char = '\u{1f}';
const BASE45: &[u8; 45] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";

#[derive(Debug, Serialize, Deserialize)]
struct PayloadV1 {
    n: String,
    d: String,
}

/// The deck reduced to what's needed to rebuild it.
pub fn essentials(deck: &ParsedDeck, extras: bool) -> ParsedDeck {
    let strip = |list: &[DeckEntry]| {
        list.iter()
            .map(|e| {
                if extras {
                    e.clone()
                } else {
                    DeckEntry::new(e.quantity, e.name.clone())
                }
            })
            .collect::<Vec<_>>()
    };
    ParsedDeck {
        name: deck.name.clone(),
        commanders: strip(&deck.commanders),
        main: strip(&deck.main),
        sideboard: if extras { strip(&deck.sideboard) } else { Vec::new() },
        format: deck.format,
        considering: if extras { strip(&deck.considering) } else { Vec::new() },
    }
}

/// Deflate "name␟decklist" at best compression. The "Deck" header line is
/// dropped first (`parse_v2` puts it back), since every byte makes the QR
/// denser.
fn compressed(name: &str, deck: &ParsedDeck) -> Result<Vec<u8>> {
    // "Deck" header and the trailing newline carry no information.
    let text = deck_to_text(deck).replacen("\nDeck\n", "\n", 1);
    let payload = format!("{}{SEP}{}", name.trim(), text.trim_end());
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::best());
    enc.write_all(payload.as_bytes())?;
    Ok(enc.finish()?)
}

pub struct ShareCodes {
    /// Copyable text code.
    pub text: String,
    /// What the QR image holds: the text code as a commanderlab:// link.
    pub qr: String,
}

/// Both share codes for a deck from the same compressed bytes: base64url text
/// to copy, and Base45 for the QR. `extras` keeps printings, sideboard and
/// tags.
pub fn encode_deck(name: &str, deck: &ParsedDeck, extras: bool) -> Result<ShareCodes> {
    let bytes = compressed(name, &essentials(deck, extras))?;
    let text = format!("{V2_TEXT}{}", URL_SAFE_NO_PAD.encode(&bytes));
    Ok(ShareCodes { qr: format!("{APP_LINK}{text}"), text })
}

/// Legacy text code (kept for tests / older callers).
pub fn encode(name: &str, deck_text: &str) -> Result<String> {
    let json = serde_json::to_vec(&PayloadV1 { n: name.to_string(), d: deck_text.to_string() })?;
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::best());
    enc.write_all(&json)?;
    Ok(format!("{V1}{}", URL_SAFE_NO_PAD.encode(enc.finish()?)))
}

/// Undo the deflate, capped at 4 MB so a crafted code can't blow up memory.
fn inflate(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    DeflateDecoder::new(bytes)
        .take(4 * 1024 * 1024)
        .read_to_end(&mut out)
        .context("The share code is damaged or incomplete.")?;
    Ok(out)
}

/// Returns (deck name, decklist text). Accepts any code format, on its own or
/// embedded in surrounding text (e.g. a pasted message).
pub fn decode(code: &str) -> Result<(String, String)> {
    let damaged = || anyhow!("The share code is damaged or incomplete.");

    if let Some(start) = code.find(V2_QR) {
        let body: String = code[start + V2_QR.len()..]
            .chars()
            .take_while(|c| c.is_ascii() && BASE45.contains(&(*c as u8)))
            .collect();
        return parse_v2(&inflate(&base45_decode(body.trim_end()).ok_or_else(damaged)?)?);
    }
    if let Some(start) = code.find(V2_TEXT) {
        let body = b64_body(&code[start + V2_TEXT.len()..]);
        let bytes = URL_SAFE_NO_PAD.decode(body).map_err(|_| damaged())?;
        return parse_v2(&inflate(&bytes)?);
    }
    if let Some(start) = code.find(V1) {
        let body = b64_body(&code[start + V1.len()..]);
        let bytes = URL_SAFE_NO_PAD.decode(body).map_err(|_| damaged())?;
        let payload: PayloadV1 = serde_json::from_slice(&inflate(&bytes)?).context("The share code is damaged.")?;
        if payload.d.trim().is_empty() {
            bail!("The shared deck is empty.");
        }
        return Ok((payload.n, payload.d));
    }
    bail!("That isn't a Commander Lab share code.")
}

/// The base64url characters after the prefix, stopping at the first other
/// character (end of a pasted message, a quote, a space).
fn b64_body(s: &str) -> String {
    s.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

/// Split the inflated payload into (name, deck text) and rebuild what
/// `compressed` stripped: the "Deck" header after the commander block, and a
/// leading "// Format:" line kept in front.
fn parse_v2(bytes: &[u8]) -> Result<(String, String)> {
    let text = String::from_utf8(bytes.to_vec()).context("The share code is damaged.")?;
    let (name, deck) = text.split_once(SEP).ok_or_else(|| anyhow!("The share code is damaged."))?;
    if deck.trim().is_empty() {
        bail!("The shared deck is empty.");
    }
    // A leading "// Format: …" line (non-Commander decks) stays in front.
    let (prefix, deck) = match deck.strip_prefix("//") {
        Some(_) => match deck.split_once('\n') {
            Some((line, rest)) => (format!("{line}\n"), rest),
            None => (String::new(), deck),
        },
        None => (String::new(), deck),
    };
    // Restore the "Deck" header removed when encoding.
    let deck = match deck.find("\n\n") {
        Some(i) if deck.starts_with("Commander\n") => format!("{}\n\nDeck\n{}\n", &deck[..i], &deck[i + 2..]),
        _ => format!("{deck}\n"),
    };
    Ok((name.to_string(), format!("{prefix}{deck}")))
}

/// RFC 9285 Base45: two bytes become three characters (one byte becomes two),
/// all from the QR alphanumeric set.
// Only tests make Base45 now (to check old QR codes still decode).
#[cfg_attr(not(test), allow(dead_code))]
fn base45_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3 / 2 + 2);
    for chunk in bytes.chunks(2) {
        if chunk.len() == 2 {
            let n = chunk[0] as u32 * 256 + chunk[1] as u32;
            for v in [n % 45, (n / 45) % 45, n / 2025] {
                out.push(BASE45[v as usize] as char);
            }
        } else {
            let n = chunk[0] as u32;
            for v in [n % 45, n / 45] {
                out.push(BASE45[v as usize] as char);
            }
        }
    }
    out
}

/// Reverse of `base45_encode`. None on a character outside the alphabet, a
/// group that overflows, or a dangling single character.
fn base45_decode(s: &str) -> Option<Vec<u8>> {
    let vals: Vec<u32> = s
        .bytes()
        .map(|b| BASE45.iter().position(|&c| c == b).map(|p| p as u32))
        .collect::<Option<_>>()?;
    let mut out = Vec::with_capacity(vals.len() * 2 / 3 + 1);
    for chunk in vals.chunks(3) {
        match chunk {
            [c, d, e] => {
                let n = c + d * 45 + e * 2025;
                if n > 0xFFFF {
                    return None;
                }
                out.push((n / 256) as u8);
                out.push((n % 256) as u8);
            }
            [c, d] => {
                let n = c + d * 45;
                if n > 0xFF {
                    return None;
                }
                out.push(n as u8);
            }
            _ => return None,
        }
    }
    Some(out)
}

/// QR code as an SVG string (dark modules on a light background so any
/// phone camera can read it).
pub fn qr_svg(code: &str) -> Result<String> {
    let qr = QrCode::with_error_correction_level(code.as_bytes(), EcLevel::L)
        .map_err(|_| anyhow!("This deck is too large for one QR code. Share the code as text instead."))?;
    let svg = qr
        .render::<svg::Color>()
        .min_dimensions(240, 240)
        .quiet_zone(true)
        .dark_color(svg::Color("#14101b"))
        .light_color(svg::Color("#ffffff"))
        .build();
    Ok(responsive_svg(&svg))
}

/// QR version (size class 1–40) a code needs; lower scans more easily.
pub fn qr_version(code: &str) -> Option<i16> {
    QrCode::with_error_correction_level(code.as_bytes(), EcLevel::L)
        .ok()
        .map(|qr| match qr.version() {
            qrcode::Version::Normal(v) => v,
            qrcode::Version::Micro(v) => v,
        })
}

/// Drop the XML declaration and the fixed width/height so the QR scales to
/// its container (a fixed 400–700px size made Android zoom the page out).
fn responsive_svg(svg: &str) -> String {
    let body = match svg.find("<svg") {
        Some(i) => &svg[i..],
        None => svg,
    };
    let end = body.find('>').unwrap_or(0);
    let (open, rest) = body.split_at(end);
    let open: String = open
        .split(' ')
        .filter(|part| !part.starts_with("width=") && !part.starts_with("height="))
        .collect::<Vec<_>>()
        .join(" ");
    format!("{open} width=\"100%\" preserveAspectRatio=\"xMidYMid meet\"{rest}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::parse_deck_text;

    /// Test helper: a small deck with a printing and a sideboard card.
    fn sample() -> ParsedDeck {
        let mut text = String::from("Commander\n1 Atraxa, Praetors' Voice\n\nDeck\n");
        for i in 0..90 {
            text.push_str(&format!("1 Some Reasonably Long Card Name {i} (CMM) {} *F*\n", 100 + i));
        }
        text.push_str("9 Forest (SLD) 2080\n\nSideboard\n1 Arcane Signet\n");
        parse_deck_text("Big", &text).unwrap()
    }

    #[test]
    fn qr_holds_an_app_link_that_decodes() {
        let codes = encode_deck("Link", &sample(), false).unwrap();
        assert!(codes.qr.starts_with("commanderlab://deck/CLD2."));
        assert_eq!(decode(&codes.qr).unwrap(), decode(&codes.text).unwrap());
        assert!(qr_svg(&codes.qr).is_ok());
    }

    #[test]
    fn base45_round_trips() {
        for data in [&b""[..], b"a", b"ab", b"abc", &[0u8, 255, 254, 1, 2][..]] {
            assert_eq!(base45_decode(&base45_encode(data)).unwrap(), data);
        }
        assert_eq!(base45_encode(b"AB"), "BB8");
    }

    #[test]
    fn essentials_drop_printings_and_sideboard() {
        let deck = sample();
        let codes = encode_deck("Big", &deck, false).unwrap();
        for code in [&codes.text, &codes.qr] {
            let (name, text) = decode(&format!("try this: {code}")).unwrap();
            assert_eq!(name, "Big");
            let back = parse_deck_text(&name, &text).unwrap();
            assert_eq!(back.commanders[0].name, "Atraxa, Praetors' Voice");
            assert_eq!(back.main.len(), 91);
            assert!(back.sideboard.is_empty());
            assert!(back.main.iter().all(|e| e.set_code.is_none() && !e.foil));
            assert_eq!(back.main.last().unwrap().quantity, 9);
        }
    }

    #[test]
    fn extras_keep_printings_and_sideboard() {
        let deck = sample();
        let codes = encode_deck("Big", &deck, true).unwrap();
        let (_, text) = decode(&codes.qr).unwrap();
        let back = parse_deck_text("Big", &text).unwrap();
        assert_eq!(back.sideboard.len(), 1);
        assert_eq!(back.main[0].set_code.as_deref(), Some("CMM"));
        assert!(back.main[0].foil);
    }

    #[test]
    fn essentials_make_a_smaller_qr() {
        let deck = sample();
        let old = encode("Big", &deck_to_text(&deck)).unwrap();
        let new = encode_deck("Big", &deck, false).unwrap();
        let (old_v, new_v) = (qr_version(&old).unwrap(), qr_version(&new.qr).unwrap());
        assert!(new_v < old_v, "old version {old_v}, new {new_v}");
        let svg = qr_svg(&new.qr).unwrap();
        assert!(svg.starts_with("<svg") && svg[..svg.find('>').unwrap()].contains("width=\"100%\""));
    }

    #[test]
    fn legacy_codes_still_decode() {
        let text = "Commander\n1 Kefka, Court Mage\n\nDeck\n1 Sol Ring (C21) 263 *F*\n30 Island\n";
        let code = encode("Kefka", text).unwrap();
        assert_eq!(decode(&code).unwrap(), ("Kefka".to_string(), text.to_string()));
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode("hello").is_err());
        assert!(decode("CLD1.!!!").is_err());
        assert!(decode("CLD2:@@@").is_err());
    }

    #[test]
    fn format_survives_sharing() {
        let mut deck = crate::deck::parse_deck_text("P", "Commander\n1 Murmuring Mystic\n\nDeck\n1 Ponder\n98 Island\n").unwrap();
        deck.format = crate::format::Format::Pdh;
        let codes = encode_deck("P", &deck, false).unwrap();
        for code in [codes.text, codes.qr] {
            let (_, text) = decode(&code).unwrap();
            let back = crate::deck::parse_deck_text("P", &text).unwrap();
            assert_eq!(back.format, crate::format::Format::Pdh);
            assert_eq!(back.commanders.len(), 1);
        }
    }
}
