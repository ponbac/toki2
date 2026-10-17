use time::Date;

use super::{dex, rng::Mulberry32};
use crate::domain::models::{DailyTrainer, SpeciesId};

/// The level of both of the daily trainer's buddies.
pub const TRAINER_LEVEL: u8 = 25;

/// The daily trainer is one of these, picked by the date.
const TRAINER_NAMES: [&str; 16] = [
    "Ace Trainer Kim",
    "Hiker Bosse",
    "Youngster Joey",
    "Lass Maja",
    "Bug Catcher Elis",
    "Swimmer Lina",
    "Black Belt Kenji",
    "Psychic Noor",
    "Picnicker Ellen",
    "Camper Theo",
    "Veteran Ingrid",
    "Scientist Ravi",
    "Fisherman Erik",
    "Beauty Sofia",
    "Gentleman Gustav",
    "Ranger Astrid",
];

/// The trainer everyone battles on `date`: two distinct species at Lv 25 and a
/// name, all drawn from a generator seeded with the date as `yyyymmdd`, so every
/// server agrees on the day's trainer without storing it.
pub fn daily_trainer(date: Date) -> DailyTrainer {
    let mut rng = Mulberry32::new(date_seed(date));
    let species: Vec<SpeciesId> = dex::all_species_ids().collect();

    let lead = species[rng.index(species.len())];
    // The dex has at least two species, so this ends.
    let partner = loop {
        let candidate = species[rng.index(species.len())];
        if candidate != lead {
            break candidate;
        }
    };
    let name = TRAINER_NAMES[rng.index(TRAINER_NAMES.len())];

    DailyTrainer {
        name: name.to_owned(),
        team: vec![(lead, TRAINER_LEVEL), (partner, TRAINER_LEVEL)],
    }
}

/// The date as the number `yyyymmdd`, e.g. 20261016.
pub(super) fn date_seed(date: Date) -> u32 {
    date.year().unsigned_abs() * 10_000
        + u32::from(u8::from(date.month())) * 100
        + u32::from(date.day())
}
