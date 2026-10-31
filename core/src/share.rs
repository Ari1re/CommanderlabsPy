//! Deck sharing by QR code or share code.
//!
//! Only what's needed to rebuild the deck is shared: name, commander(s),
//! cards and counts. Printings and the sideboard are optional extras.
//!
//! Formats (all decode on any device, no server involved):
//! * `CLD2:` + Base45(deflate(payload)) — used in QR codes. Base45 only uses
//!   QR "alphanumeric" characters, which pack ~30% denser than bytes.
//! * `CLD2.` + base64url(deflate(payload)) — the copyable text code (no
//!   spaces, survives chat apps).
//! * `CLD1.` — older JSON codes, still accepted.
//!
//! Payload: `name \x1f decklist`, where the decklist is the normal text
//! format.

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
    }
}

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
    /// Denser code for the QR image.
    pub qr: String,
}

pub fn encode_deck(name: &str, deck: &ParsedDeck, extras: bool) -> Result<ShareCodes> {
    let bytes = compressed(name, &essentials(deck, extras))?;
    Ok(ShareCodes {
        text: format!("{V2_TEXT}{}", URL_SAFE_NO_PAD.encode(&bytes)),
        qr: format!("{V2_QR}{}", base45_encode(&bytes)),
    })
}

/// Legacy text code (kept for tests / older callers).
pub fn encode(name: &str, deck_text: &str) -> Result<String> {
    let json = serde_json::to_vec(&PayloadV1 { n: name.to_string(), d: deck_text.to_string() })?;
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::best());
    enc.write_all(&json)?;
    Ok(format!("{V1}{}", URL_SAFE_NO_PAD.encode(enc.finish()?)))
}

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

fn b64_body(s: &str) -> String {
    s.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

fn parse_v2(bytes: &[u8]) -> Result<(String, String)> {
    let text = String::from_utf8(bytes.to_vec()).context("The share code is damaged.")?;
    let (name, deck) = text.split_once(SEP).ok_or_else(|| anyhow!("The share code is damaged."))?;
    if deck.trim().is_empty() {
        bail!("The shared deck is empty.");
    }
    // Restore the "Deck" header removed when encoding.
    let deck = match deck.find("\n\n") {
        Some(i) if deck.starts_with("Commander\n") => format!("{}\n\nDeck\n{}\n", &deck[..i], &deck[i + 2..]),
        _ => format!("{deck}\n"),
    };
    Ok((name.to_string(), deck))
}

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

    fn sample() -> ParsedDeck {
        let mut text = String::from("Commander\n1 Atraxa, Praetors' Voice\n\nDeck\n");
        for i in 0..90 {
            text.push_str(&format!("1 Some Reasonably Long Card Name {i} (CMM) {} *F*\n", 100 + i));
        }
        text.push_str("9 Forest (SLD) 2080\n\nSideboard\n1 Arcane Signet\n");
        parse_deck_text("Big", &text).unwrap()
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
}
