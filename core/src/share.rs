//! Deck sharing by QR code or share code.
//!
//! A share code is `CLD1.` + base64url(deflate(JSON {n, d})) where `d` is the
//! decklist text (printings included). No server is involved: the receiving
//! app decodes the code, so sharing works offline and between any platforms.

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use flate2::{read::DeflateDecoder, write::DeflateEncoder, Compression};
use qrcode::{render::svg, EcLevel, QrCode};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

const PREFIX: &str = "CLD1.";

#[derive(Debug, Serialize, Deserialize)]
struct Payload {
    /// Deck name.
    n: String,
    /// Decklist text.
    d: String,
}

pub fn encode(name: &str, deck_text: &str) -> Result<String> {
    let json = serde_json::to_vec(&Payload { n: name.to_string(), d: deck_text.to_string() })?;
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::best());
    enc.write_all(&json)?;
    Ok(format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(enc.finish()?)))
}

/// Returns (deck name, decklist text). Accepts the code on its own or
/// embedded in surrounding text (e.g. a pasted message).
pub fn decode(code: &str) -> Result<(String, String)> {
    let start = code.find(PREFIX).ok_or_else(|| anyhow!("That isn't a Commander Lab share code."))?;
    let body: String = code[start + PREFIX.len()..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let bytes = URL_SAFE_NO_PAD.decode(body).context("The share code is damaged or incomplete.")?;
    let mut json = Vec::new();
    DeflateDecoder::new(&bytes[..])
        .take(4 * 1024 * 1024)
        .read_to_end(&mut json)
        .context("The share code is damaged or incomplete.")?;
    let payload: Payload = serde_json::from_slice(&json).context("The share code is damaged.")?;
    if payload.d.trim().is_empty() {
        bail!("The shared deck is empty.");
    }
    Ok((payload.n, payload.d))
}

/// QR code as an SVG string (dark modules on a light background so any
/// phone camera can read it).
pub fn qr_svg(code: &str) -> Result<String> {
    let qr = QrCode::with_error_correction_level(code.as_bytes(), EcLevel::L)
        .map_err(|_| anyhow!("This deck is too large for one QR code. Share the code as text instead."))?;
    Ok(qr
        .render::<svg::Color>()
        .min_dimensions(320, 320)
        .quiet_zone(true)
        .dark_color(svg::Color("#14101b"))
        .light_color(svg::Color("#ffffff"))
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let text = "Commander\n1 Kefka, Court Mage\n\nDeck\n1 Sol Ring (C21) 263 *F*\n30 Island\n";
        let code = encode("Kefka", text).unwrap();
        assert!(code.starts_with(PREFIX));
        let (name, back) = decode(&format!("check this out: {code} !")).unwrap();
        assert_eq!(name, "Kefka");
        assert_eq!(back, text);
    }

    #[test]
    fn full_deck_fits_in_a_qr() {
        let mut text = String::from("Commander\n1 Atraxa, Praetors' Voice\n\nDeck\n");
        for i in 0..99 {
            text.push_str(&format!("1 Some Reasonably Long Card Name {i} (CMM) {}\n", 100 + i));
        }
        let code = encode("Big", &text).unwrap();
        assert!(qr_svg(&code).is_ok(), "code length {}", code.len());
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode("hello").is_err());
        assert!(decode("CLD1.!!!").is_err());
    }
}
