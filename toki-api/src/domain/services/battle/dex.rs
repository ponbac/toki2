use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

use serde::{de::IgnoredAny, Deserialize};

use crate::domain::models::{BattleMove, MonType, SpeciesId, MAX_BATTLE_MOVES};

/// The generated dex, embedded so the engine needs no files at runtime.
pub(super) const DEX_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/buddy_dex.json"));

/// Parsed on first use. The file is generated and checked by the tests, so a parse
/// failure is a build defect.
static DEX: LazyLock<Dex> = LazyLock::new(|| {
    Dex::parse(DEX_JSON)
        .unwrap_or_else(|error| panic!("the embedded buddy dex is invalid: {error}"))
});

const TYPE_COUNT: usize = MonType::ALL.len();

/// A species as the embedded dex lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DexSpecies {
    /// Display name, such as `Pikachu`.
    pub name: String,
    /// One or two distinct types.
    pub types: Vec<MonType>,
    /// Base stats in the order HP, attack, defense, special attack, special
    /// defense, speed.
    pub base_stats: [u16; 6],
    /// One to four moves.
    pub moves: Vec<BattleMove>,
}

#[derive(Debug)]
pub(super) struct Dex {
    species: BTreeMap<SpeciesId, DexSpecies>,
    /// Multiplier by attacking, then defending type index.
    chart: [[f64; TYPE_COUNT]; TYPE_COUNT],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DexFile {
    #[serde(rename = "source")]
    _source: IgnoredAny,
    types: Vec<MonType>,
    type_chart: BTreeMap<MonType, BTreeMap<MonType, f64>>,
    species: BTreeMap<i16, DexSpecies>,
}

impl Dex {
    /// Parses and checks a dex file: all 17 types, multipliers of 0, 0.5 or 2 (1 is
    /// implied), and at least two species, each with a name, one or two distinct
    /// types, positive base stats and one to four valid moves.
    pub(super) fn parse(json: &str) -> Result<Self, String> {
        let file: DexFile = serde_json::from_str(json).map_err(|error| error.to_string())?;

        let types: BTreeSet<MonType> = file.types.iter().copied().collect();
        if file.types.len() != TYPE_COUNT || types.len() != TYPE_COUNT {
            return Err(format!(
                "expected the {TYPE_COUNT} types, got {:?}",
                file.types
            ));
        }

        let mut chart = [[1.0; TYPE_COUNT]; TYPE_COUNT];
        for (attacking, row) in &file.type_chart {
            for (defending, multiplier) in row {
                if ![0.0, 0.5, 2.0].contains(multiplier) {
                    return Err(format!(
                        "{attacking:?} against {defending:?} has multiplier {multiplier}"
                    ));
                }
                chart[attacking.index()][defending.index()] = *multiplier;
            }
        }

        if file.species.len() < 2 {
            return Err("the daily trainer needs at least two species".to_owned());
        }
        let mut species = BTreeMap::new();
        for (id, entry) in file.species {
            check_species(id, &entry)?;
            species.insert(SpeciesId::new(id), entry);
        }

        Ok(Self { species, chart })
    }
}

fn check_species(id: i16, species: &DexSpecies) -> Result<(), String> {
    let invalid = |reason: &str| Err(format!("species {id} ({}) {reason}", species.name));
    if id < 1 {
        return invalid("has no national dex number");
    }
    if species.name.trim().is_empty() {
        return invalid("has no name");
    }
    let distinct_types: BTreeSet<MonType> = species.types.iter().copied().collect();
    if !(1..=2).contains(&species.types.len()) || distinct_types.len() != species.types.len() {
        return invalid("needs one or two distinct types");
    }
    if species.base_stats.contains(&0) {
        return invalid("has a zero base stat");
    }
    if species.moves.is_empty() || species.moves.len() > MAX_BATTLE_MOVES {
        return invalid("needs one to four moves");
    }
    for battle_move in &species.moves {
        let accuracy_ok = battle_move
            .accuracy
            .is_none_or(|accuracy| (1..=100).contains(&accuracy));
        if battle_move.name.trim().is_empty()
            || battle_move.power == 0
            || battle_move.pp == 0
            || !accuracy_ok
        {
            return invalid(&format!("has an invalid move {battle_move:?}"));
        }
    }
    Ok(())
}

/// The species with this dex number, if it is a buddy.
pub fn species(id: SpeciesId) -> Option<&'static DexSpecies> {
    DEX.species.get(&id)
}

/// Every buddy species, by ascending dex number.
pub fn all_species_ids() -> impl Iterator<Item = SpeciesId> {
    DEX.species.keys().copied()
}

/// Whether a buddy can be this species: it is in the dex, so it has sprites and moves.
pub fn is_known_species(id: SpeciesId) -> bool {
    DEX.species.contains_key(&id)
}

/// The multiplier of an attacking type against a defender: the product over its types.
pub(super) fn effectiveness(attacking: MonType, defending: &[MonType]) -> f64 {
    defending
        .iter()
        .map(|defending| DEX.chart[attacking.index()][defending.index()])
        .product()
}
