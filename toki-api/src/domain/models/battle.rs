//! Timer buddy battles: the team snapshots and the event log of one battle.
//!
//! A battle is resolved once, when it starts, by the deterministic engine in
//! `domain::services::battle`. Both teams are stored as snapshots of the buddies at
//! that moment, together with the seed, the engine version and the events, so a
//! replay never depends on later level-ups or rule changes. These types serialise
//! in the shape that is stored as JSON and replayed by the app.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The most buddies a team has: the lead and the partner.
pub const MAX_BATTLE_TEAM_SIZE: usize = 2;
/// The most moves a buddy knows.
pub const MAX_BATTLE_MOVES: usize = 4;
/// The highest buddy level.
pub const MAX_BUDDY_LEVEL: u8 = 100;

/// A national dex number, which identifies a buddy species.
///
/// Wraps i16 to match the database SMALLINT type. Any number fits; whether the dex
/// knows the species is `domain::services::battle::is_known_species`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SpeciesId(i16);

impl SpeciesId {
    pub const fn new(id: i16) -> Self {
        Self(id)
    }

    pub fn as_i16(&self) -> i16 {
        self.0
    }
}

impl fmt::Display for SpeciesId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<i16> for SpeciesId {
    fn from(id: i16) -> Self {
        Self(id)
    }
}

impl From<SpeciesId> for i16 {
    fn from(id: SpeciesId) -> Self {
        id.0
    }
}

/// A generation 5 type, such as Fire. Serialises as its name, e.g. `"Fire"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MonType {
    Normal,
    Fire,
    Water,
    Electric,
    Grass,
    Ice,
    Fighting,
    Poison,
    Ground,
    Flying,
    Psychic,
    Bug,
    Rock,
    Ghost,
    Dragon,
    Dark,
    Steel,
}

impl MonType {
    /// Every type, in the conventional order.
    pub const ALL: [MonType; 17] = [
        MonType::Normal,
        MonType::Fire,
        MonType::Water,
        MonType::Electric,
        MonType::Grass,
        MonType::Ice,
        MonType::Fighting,
        MonType::Poison,
        MonType::Ground,
        MonType::Flying,
        MonType::Psychic,
        MonType::Bug,
        MonType::Rock,
        MonType::Ghost,
        MonType::Dragon,
        MonType::Dark,
        MonType::Steel,
    ];

    /// The position of the type in `ALL`.
    pub fn index(self) -> usize {
        self as usize
    }
}

/// Which pair of stats a move uses: attack against defense, or special attack
/// against special defense.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MoveCategory {
    Physical,
    Special,
}

/// A move a buddy knows, as the dex lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BattleMove {
    /// Display name, such as `Thunderbolt`.
    pub name: String,
    #[serde(rename = "type")]
    pub move_type: MonType,
    pub category: MoveCategory,
    /// Power per use, with the expected number of hits of a multi-hit move already
    /// applied. The engine clamps it to 60..=100.
    pub power: u16,
    /// Hit chance in percent, or `None` for a move that never misses.
    pub accuracy: Option<u8>,
    pub pp: u8,
    /// Moves with a higher priority go first, whatever the speeds.
    pub priority: i8,
    /// Critical hit stage: 0 is the normal 1/16 chance, 1 a high-critical move.
    pub crit_stage: u8,
}

/// A buddy's stats at its battle level, apart from HP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BattleStats {
    pub attack: u16,
    pub defense: u16,
    pub special_attack: u16,
    pub special_defense: u16,
    pub speed: u16,
}

impl BattleStats {
    fn all_positive(&self) -> bool {
        [
            self.attack,
            self.defense,
            self.special_attack,
            self.special_defense,
            self.speed,
        ]
        .iter()
        .all(|stat| *stat > 0)
    }
}

/// A buddy as it entered a battle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BattleMon {
    pub species_id: SpeciesId,
    pub name: String,
    /// The buddy's level (1..=100), not the battle level the engine derives from it.
    pub level: u8,
    pub types: Vec<MonType>,
    pub max_hp: u16,
    pub stats: BattleStats,
    pub moves: Vec<BattleMove>,
}

/// One side's buddies in send-out order: one or two buddies, each with one to four
/// moves, a level in 1..=100 and positive stats. Serialises as a JSON array of
/// `BattleMon`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<BattleMon>", into = "Vec<BattleMon>")]
pub struct BattleTeam(Vec<BattleMon>);

impl BattleTeam {
    pub fn new(members: Vec<BattleMon>) -> Result<Self, BattleError> {
        if members.is_empty() || members.len() > MAX_BATTLE_TEAM_SIZE {
            return Err(BattleError::InvalidTeamSize(members.len()));
        }
        for mon in &members {
            if mon.level == 0 || mon.level > MAX_BUDDY_LEVEL {
                return Err(BattleError::InvalidLevel(mon.level));
            }
            if mon.moves.is_empty() || mon.moves.len() > MAX_BATTLE_MOVES {
                return Err(BattleError::InvalidMoves {
                    species_id: mon.species_id,
                    count: mon.moves.len(),
                });
            }
            if mon.types.is_empty() || mon.max_hp == 0 || !mon.stats.all_positive() {
                return Err(BattleError::InvalidMon(mon.species_id));
            }
        }
        Ok(Self(members))
    }

    pub fn members(&self) -> &[BattleMon] {
        &self.0
    }
}

impl TryFrom<Vec<BattleMon>> for BattleTeam {
    type Error = BattleError;

    fn try_from(members: Vec<BattleMon>) -> Result<Self, Self::Error> {
        Self::new(members)
    }
}

impl From<BattleTeam> for Vec<BattleMon> {
    fn from(team: BattleTeam) -> Self {
        team.0
    }
}

/// The two sides of a battle. Serialises as `"challenger"` or `"defender"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BattleSide {
    /// The user who started the battle.
    Challenger,
    /// The colleague or daily trainer who was battled.
    Defender,
}

impl BattleSide {
    pub fn opponent(self) -> Self {
        match self {
            Self::Challenger => Self::Defender,
            Self::Defender => Self::Challenger,
        }
    }
}

/// One step of a battle, in the order a replay shows them. Serialises internally
/// tagged by `"type"`, e.g. `{"type":"move","side":"challenger","moveIndex":2}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BattleEvent {
    /// `side` sends out the team member at `index`.
    Send { side: BattleSide, index: u8 },
    /// `side`'s active buddy uses the move at `move_index`.
    Move { side: BattleSide, move_index: u8 },
    /// The move of the attacker, `side`, missed.
    Miss { side: BattleSide },
    /// The target, `side`, is immune to the move.
    Immune { side: BattleSide },
    /// The target, `side`, took `damage` and has `hp_after` left. `effectiveness` is
    /// the type multiplier, such as 2.0 for a super-effective hit.
    Hit {
        side: BattleSide,
        damage: u16,
        hp_after: u16,
        effectiveness: f64,
        crit: bool,
    },
    /// `side`'s active buddy fainted.
    Faint { side: BattleSide },
    /// The battle is over; no winner is a draw.
    End { winner: Option<BattleSide> },
}

/// A resolved battle: every event, ending with `BattleEvent::End`, and its winner.
#[derive(Debug, Clone, PartialEq)]
pub struct BattleOutcome {
    pub events: Vec<BattleEvent>,
    /// `None` when the battle hit the turn limit, which is a draw.
    pub winner: Option<BattleSide>,
}

/// The NPC team everyone can battle once on a local day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyTrainer {
    pub name: String,
    /// Species and levels, in send-out order, ready for `build_team`.
    pub team: Vec<(SpeciesId, u8)>,
}

/// Why a battle team could not be built.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BattleError {
    #[error("unknown buddy species {0}")]
    UnknownSpecies(SpeciesId),
    #[error("buddy level {0} is outside 1..={MAX_BUDDY_LEVEL}")]
    InvalidLevel(u8),
    #[error("a team has 1 to {MAX_BATTLE_TEAM_SIZE} buddies, not {0}")]
    InvalidTeamSize(usize),
    #[error("buddy species {species_id} knows {count} moves, not 1 to {MAX_BATTLE_MOVES}")]
    InvalidMoves { species_id: SpeciesId, count: usize },
    #[error("buddy species {0} has no types or a zero stat")]
    InvalidMon(SpeciesId),
}
