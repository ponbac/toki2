use super::dex::{self, DexSpecies};
use crate::domain::models::{BattleError, BattleMon, BattleStats, BattleTeam, SpeciesId};

/// Base stat totals are pulled into this band before stats are computed, so weak
/// species keep a chance and legendaries do not dominate.
const BST_BAND: (f64, f64) = (380.0, 540.0);
/// Every buddy fights with perfect individual values and no effort values.
const IV: f64 = 31.0;

/// The level the engine computes with, from 47.07 at Lv 1 to 53.67 at Lv 100, so a
/// veteran buddy has an edge over a new one but never an overwhelming one.
pub fn battle_level(level: u8) -> f64 {
    47.0 + f64::from(level) / 15.0
}

/// Snapshots a team of one or two buddies, given as species and level (1..=100), in
/// send-out order.
pub fn build_team(members: &[(SpeciesId, u8)]) -> Result<BattleTeam, BattleError> {
    let mons = members
        .iter()
        .map(|&(species_id, level)| build_mon(species_id, level))
        .collect::<Result<Vec<_>, _>>()?;
    BattleTeam::new(mons)
}

fn build_mon(species_id: SpeciesId, level: u8) -> Result<BattleMon, BattleError> {
    let species = dex::species(species_id).ok_or(BattleError::UnknownSpecies(species_id))?;
    let (max_hp, stats) = battle_stats(species, battle_level(level));
    Ok(BattleMon {
        species_id,
        name: species.name.clone(),
        level,
        types: species.types.clone(),
        max_hp,
        stats,
        moves: species.moves.clone(),
    })
}

/// Max HP and stats at battle level `level`, from base stats scaled into the BST band.
fn battle_stats(species: &DexSpecies, level: f64) -> (u16, BattleStats) {
    let total: f64 = species.base_stats.iter().copied().map(f64::from).sum();
    let scale = total.clamp(BST_BAND.0, BST_BAND.1) / total;
    let base = species.base_stats.map(|stat| f64::from(stat) * scale);

    let stat = |base: f64| ((2.0 * base + IV) * level / 100.0).floor() as u16 + 5;
    let max_hp = ((2.0 * base[0] + IV) * level / 100.0 + level).floor() as u16 + 10;
    let stats = BattleStats {
        attack: stat(base[1]),
        defense: stat(base[2]),
        special_attack: stat(base[3]),
        special_defense: stat(base[4]),
        speed: stat(base[5]),
    };
    (max_hp, stats)
}
