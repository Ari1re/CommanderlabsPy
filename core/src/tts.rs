//! Tabletop Simulator export: a saved object (JSON) holding the deck as a
//! custom deck of Scryfall images, with the commander(s) as separate cards
//! beside it. Drop the file in TTS' "Saved Objects" folder and spawn it from
//! Objects → Saved Objects.

use crate::{
    data::CardDb,
    deck::{DeckEntry, ParsedDeck},
};
use serde_json::{json, Value};
use std::path::PathBuf;

/// Scryfall's standard Magic card back.
pub const CARD_BACK: &str =
    "https://backs.scryfall.io/large/0/a/0aeebaf5-8c7d-4636-9e82-8c27447861f7.jpg";

/// Table position for a deck object: side by side along x, face down
/// (rotated 180° on two axes, as TTS expects for a deck).
fn transform(x: f64) -> Value {
    json!({ "posX": x, "posY": 1.0, "posZ": 0.0, "rotX": 0.0, "rotY": 180.0, "rotZ": 180.0,
            "scaleX": 1.0, "scaleY": 1.0, "scaleZ": 1.0 })
}

/// Large Scryfall image for an entry: its chosen printing, else the default.
fn face_urls(entry: &DeckEntry, db: &CardDb) -> (String, Option<String>, String, String) {
    let card = db.get_card(&entry.name).ok().flatten();
    let name = card.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| entry.name.clone());
    let text = card.as_ref().map(|c| format!("{}\n{}", c.type_line, c.oracle_text)).unwrap_or_default();
    let printing = db.printing(&entry.name).ok().flatten();
    let double = printing.as_ref().map(|p| p.double_faced).unwrap_or(false);
    let by_printing = |set: &str, num: &str, back: bool| {
        format!(
            "https://api.scryfall.com/cards/{}/{}?format=image&version=large{}",
            set.to_ascii_lowercase(),
            num,
            if back { "&face=back" } else { "" }
        )
    };
    let (front, back) = match (&entry.set_code, &entry.collector_number) {
        (Some(set), Some(num)) => (by_printing(set, num, false), double.then(|| by_printing(set, num, true))),
        _ => match &printing {
            Some(p) => (
                by_printing(&p.set_code, &p.collector_number, false),
                double.then(|| by_printing(&p.set_code, &p.collector_number, true)),
            ),
            None => (
                card.as_ref().map(|c| c.image_uri.replace("/normal/", "/large/")).unwrap_or_default(),
                None,
            ),
        },
    };
    (front, back, name, text)
}

/// One TTS card: its CardID (deck key × 100 + index), name and rules text
/// as the tooltip, plus a CustomDeck entry pointing at the face and the shared
/// card back.
fn card_object(id: u32, deck_key: u32, face: &str, name: &str, text: &str, x: f64) -> Value {
    json!({
        "Name": "Card",
        "Transform": transform(x),
        "Nickname": name,
        "Description": text,
        "CardID": id,
        "CustomDeck": { deck_key.to_string(): {
            "FaceURL": face, "BackURL": CARD_BACK, "NumWidth": 1, "NumHeight": 1,
            "BackIsHidden": true, "UniqueBack": false, "Type": 0
        }},
    })
}

/// One card, with its back face as a second state (flip with a number key
/// or the right-click menu in TTS).
fn card_with_states(next: &mut u32, entry: &DeckEntry, db: &CardDb, x: f64) -> (Value, u32, u32) {
    let (front, back, name, text) = face_urls(entry, db);
    let key = *next;
    *next += 1;
    let id = key * 100;
    let mut card = card_object(id, key, &front, &name, &text, x);
    if let Some(back) = back {
        let back_key = *next;
        *next += 1;
        let back_name = name.split(" // ").nth(1).unwrap_or(&name).to_string();
        card["States"] = json!({ "2": card_object(back_key * 100, back_key, &back, &back_name, &text, x) });
    }
    (card, id, key)
}

/// Build the TTS saved object for a deck.
pub fn deck_to_tts(deck: &ParsedDeck, db: &CardDb) -> Value {
    let mut next = 1u32;
    let mut objects = Vec::new();

    // Commanders face up beside the deck.
    for (i, entry) in deck.commanders.iter().enumerate() {
        let (mut card, _, _) = card_with_states(&mut next, entry, db, 3.0 + 2.6 * i as f64);
        card["Transform"]["rotZ"] = json!(0.0);
        objects.push(card);
    }

    let mut deck_ids = Vec::new();
    let mut contained = Vec::new();
    let mut custom = serde_json::Map::new();
    for entry in &deck.main {
        let (card, id, key) = card_with_states(&mut next, entry, db, 0.0);
        if let Some(def) = card["CustomDeck"].get(key.to_string()) {
            custom.insert(key.to_string(), def.clone());
        }
        for _ in 0..entry.quantity.max(1) {
            deck_ids.push(id);
            contained.push(card.clone());
        }
    }

    if !contained.is_empty() {
        objects.push(json!({
            "Name": "DeckCustom",
            "Transform": transform(0.0),
            "Nickname": deck.name,
            "Description": "Made with Commander Lab",
            "DeckIDs": deck_ids,
            "CustomDeck": custom,
            "ContainedObjects": contained,
        }));
    }

    json!({
        "SaveName": deck.name,
        "GameMode": "",
        "Date": "",
        "Table": "",
        "Sky": "",
        "Note": "",
        "Rules": "",
        "XmlUI": "",
        "LuaScript": "",
        "ObjectStates": objects,
        "TabStates": {},
        "VersionNumber": "",
    })
}

/// Tabletop Simulator's "Saved Objects" folder on this computer, if TTS is
/// installed (Windows: Documents\My Games; Linux: ~/.local/share).
pub fn saved_objects_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from)?;
    let candidates = [
        home.join("Documents/My Games/Tabletop Simulator/Saves/Saved Objects"),
        home.join("OneDrive/Documents/My Games/Tabletop Simulator/Saves/Saved Objects"),
        home.join(".local/share/Tabletop Simulator/Saves/Saved Objects"),
        home.join("Library/Tabletop Simulator/Saves/Saved Objects"),
    ];
    candidates.into_iter().find(|p| p.is_dir())
}

/// Archidekt's text import format: commanders tagged into the Commander
/// category, the sideboard kept out of the deck.
pub fn deck_to_archidekt_text(deck: &ParsedDeck, db: &CardDb) -> String {
    let line = |e: &DeckEntry, tag: &str| {
        let name = db.get_card(&e.name).ok().flatten().map(|c| c.name).unwrap_or_else(|| e.name.clone());
        let printing = match (&e.set_code, &e.collector_number) {
            (Some(s), Some(n)) => format!(" ({}) {}", s.to_ascii_lowercase(), n),
            _ => db
                .printing(&e.name)
                .ok()
                .flatten()
                .map(|p| format!(" ({}) {}", p.set_code.to_ascii_lowercase(), p.collector_number))
                .unwrap_or_default(),
        };
        format!("{}x {}{}{}{}\n", e.quantity, name, printing, if e.foil { " *F*" } else { "" }, tag)
    };
    let mut out = String::new();
    for e in &deck.commanders {
        out.push_str(&line(e, " [Commander{top}]"));
    }
    for e in &deck.main {
        out.push_str(&line(e, ""));
    }
    for e in &deck.sideboard {
        out.push_str(&line(e, " [Sideboard{noDeck}]"));
    }
    out
}
