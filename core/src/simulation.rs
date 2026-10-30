use crate::{
    analysis::classify_card,
    data::CardDb,
    deck::ParsedDeck,
};
use anyhow::Result;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default)]
struct SimCard {
    land: bool,
    early_ramp: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawSimulation {
    pub runs: u32,
    pub average_opening_lands: f64,
    pub keepable_opening_pct: f64,
    pub three_lands_by_t3_pct: f64,
    pub four_lands_by_t4_pct: f64,
    pub five_lands_by_t5_pct: f64,
    pub early_ramp_by_t2_pct: f64,
}

pub fn run_draw_simulation(
    deck: &ParsedDeck,
    db: &CardDb,
    runs: u32,
) -> Result<DrawSimulation> {
    let mut library = Vec::new();

    for entry in &deck.main {
        let card = db.get_card(&entry.name)?;

        let (land, early_ramp) = if let Some(card) = card {
            let roles = classify_card(&card);
            (
                card.type_line.to_ascii_lowercase().contains("land"),
                roles.ramp && card.cmc <= 2.0,
            )
        } else {
            (false, false)
        };

        for _ in 0..entry.quantity {
            library.push(SimCard { land, early_ramp });
        }
    }

    if library.len() < 20 {
        anyhow::bail!("Deck library is too small to run a useful simulation.");
    }

    let mut rng = rand::rng();

    let mut opening_land_total = 0u64;
    let mut keepable = 0u64;
    let mut three_t3 = 0u64;
    let mut four_t4 = 0u64;
    let mut five_t5 = 0u64;
    let mut ramp_t2 = 0u64;

    for _ in 0..runs {
        library.shuffle(&mut rng);

        let opening = &library[..library.len().min(7)];
        let opening_lands = opening.iter().filter(|c| c.land).count();

        opening_land_total += opening_lands as u64;

        if (2..=5).contains(&opening_lands) {
            keepable += 1;
        }

        let t2_seen = &library[..library.len().min(9)];
        let t3_seen = &library[..library.len().min(10)];
        let t4_seen = &library[..library.len().min(11)];
        let t5_seen = &library[..library.len().min(12)];

        if t3_seen.iter().filter(|c| c.land).count() >= 3 {
            three_t3 += 1;
        }
        if t4_seen.iter().filter(|c| c.land).count() >= 4 {
            four_t4 += 1;
        }
        if t5_seen.iter().filter(|c| c.land).count() >= 5 {
            five_t5 += 1;
        }
        if t2_seen.iter().any(|c| c.early_ramp) {
            ramp_t2 += 1;
        }
    }

    let pct = |v: u64| (v as f64 / runs as f64) * 100.0;

    Ok(DrawSimulation {
        runs,
        average_opening_lands: opening_land_total as f64 / runs as f64,
        keepable_opening_pct: pct(keepable),
        three_lands_by_t3_pct: pct(three_t3),
        four_lands_by_t4_pct: pct(four_t4),
        five_lands_by_t5_pct: pct(five_t5),
        early_ramp_by_t2_pct: pct(ramp_t2),
    })
}
