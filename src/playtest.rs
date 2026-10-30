use crate::deck::ParsedDeck;
use rand::seq::SliceRandom;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaytestZone {
    Hand,
    Battlefield,
    Graveyard,
}

#[derive(Debug, Clone)]
pub struct PlaytestSelection {
    pub zone: PlaytestZone,
    pub index: usize,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct PlaytestState {
    pub library: Vec<String>,
    pub hand: Vec<String>,
    pub battlefield: Vec<String>,
    pub graveyard: Vec<String>,
    pub turn: u32,
    pub log: Vec<String>,
    pub selected: Option<PlaytestSelection>,
}

impl PlaytestState {
    pub fn new(deck: &ParsedDeck) -> Self {
        let mut state = Self {
            library: build_library(deck),
            hand: Vec::new(),
            battlefield: Vec::new(),
            graveyard: Vec::new(),
            turn: 0,
            log: Vec::new(),
            selected: None,
        };
        state.shuffle();
        state.draw_n(7);
        state.log.push("Opening seven drawn.".to_string());
        state
    }

    pub fn reset(&mut self, deck: &ParsedDeck) {
        *self = Self::new(deck);
    }

    pub fn mulligan(&mut self, deck: &ParsedDeck) {
        *self = Self::new(deck);
        self.log.clear();
        self.log.push(
            "Fresh seven drawn. Bottom cards manually if your mulligan needs it."
                .to_string(),
        );
    }

    pub fn shuffle(&mut self) {
        let mut rng = rand::rng();
        self.library.shuffle(&mut rng);
    }

    pub fn draw(&mut self) {
        if let Some(card) = self.library.pop() {
            self.log.push(format!("Drew {card}."));
            self.hand.push(card);
        } else {
            self.log.push("Tried to draw from an empty library.".to_string());
        }
    }

    pub fn draw_n(&mut self, count: usize) {
        for _ in 0..count {
            let Some(card) = self.library.pop() else {
                break;
            };
            self.hand.push(card);
        }
    }

    pub fn next_turn(&mut self) {
        self.turn += 1;
        self.log.push(format!("Turn {} begins.", self.turn));
        self.draw();
    }

    pub fn select(
        &mut self,
        zone: PlaytestZone,
        index: usize,
        name: String,
    ) {
        self.selected = Some(PlaytestSelection { zone, index, name });
    }

    pub fn play_selected(&mut self) {
        let Some(selection) = self.selected.clone() else {
            self.log.push("Select a card first.".to_string());
            return;
        };

        if selection.zone != PlaytestZone::Hand
            || selection.index >= self.hand.len()
        {
            self.log.push("PLAY requires a selected hand card.".to_string());
            return;
        }

        let card = self.hand.remove(selection.index);
        self.battlefield.push(card.clone());
        self.log.push(format!("Played/cast {card}."));
        self.selected = None;
    }

    pub fn graveyard_selected(&mut self) {
        let Some(selection) = self.selected.clone() else {
            self.log.push("Select a card first.".to_string());
            return;
        };

        let card = match selection.zone {
            PlaytestZone::Hand if selection.index < self.hand.len() => {
                self.hand.remove(selection.index)
            }
            PlaytestZone::Battlefield
                if selection.index < self.battlefield.len() =>
            {
                self.battlefield.remove(selection.index)
            }
            _ => {
                self.log.push(
                    "TO GRAVE requires a hand or battlefield card.".to_string(),
                );
                return;
            }
        };

        self.graveyard.push(card.clone());
        self.log.push(format!("{card} moved to graveyard."));
        self.selected = None;
    }

    pub fn return_selected_to_hand(&mut self) {
        let Some(selection) = self.selected.clone() else {
            self.log.push("Select a card first.".to_string());
            return;
        };

        if selection.zone != PlaytestZone::Graveyard
            || selection.index >= self.graveyard.len()
        {
            self.log.push("RETURN requires a graveyard card.".to_string());
            return;
        }

        let card = self.graveyard.remove(selection.index);
        self.hand.push(card.clone());
        self.log.push(format!("{card} returned to hand."));
        self.selected = None;
    }

    pub fn selected_name(&self) -> Option<&str> {
        self.selected.as_ref().map(|s| s.name.as_str())
    }
}

fn build_library(deck: &ParsedDeck) -> Vec<String> {
    let mut library = Vec::new();
    for entry in &deck.main {
        for _ in 0..entry.quantity {
            library.push(entry.name.clone());
        }
    }
    library
}
