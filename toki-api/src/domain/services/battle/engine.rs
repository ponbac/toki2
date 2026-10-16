use std::cmp::Ordering;

use super::{dex, rng::Mulberry32, team::battle_level};
use crate::domain::models::{
    BattleEvent, BattleMon, BattleMove, BattleOutcome, BattleSide, BattleTeam, MoveCategory,
};

/// A battle still running after this many turns is a draw.
pub(super) const MAX_TURNS: usize = 60;
/// Move power is clamped into this range, so weak moves still hurt and strong ones
/// do not end battles in one hit.
const POWER_RANGE: (u16, u16) = (60, 100);
/// Critical hit chance by crit stage; higher stages use the last entry.
const CRIT_CHANCE: [f64; 5] = [1.0 / 16.0, 1.0 / 8.0, 1.0 / 4.0, 1.0 / 3.0, 1.0 / 2.0];
/// The damage roll is a whole percentage from 85 to 100.
const ROLL_MIN: u64 = 85;
const ROLL_STEPS: f64 = 16.0;

/// The buddy a side has out and its remaining HP. Earlier team members have fainted.
struct Fighter<'a> {
    team: &'a [BattleMon],
    active: usize,
    hp: u16,
}

impl<'a> Fighter<'a> {
    fn new(team: &'a BattleTeam) -> Self {
        let team = team.members();
        Self {
            team,
            active: 0,
            hp: team[0].max_hp,
        }
    }

    fn mon(&self) -> &'a BattleMon {
        &self.team[self.active]
    }

    fn fainted(&self) -> bool {
        self.hp == 0
    }

    /// Sends out the next team member, returning its index, or `None` when the side
    /// has no buddy left.
    fn send_next(&mut self) -> Option<usize> {
        let next = self.active + 1;
        let mon = self.team.get(next)?;
        self.active = next;
        self.hp = mon.max_hp;
        Some(next)
    }
}

/// Resolves a battle. Pure and deterministic: the same teams and seed always give
/// the same events. Each turn both active buddies pick a random move; the higher
/// priority, then the faster buddy, then a coin flip goes first. A fainted buddy is
/// replaced at the end of the turn; a side with none left loses. After 60 turns
/// the battle is a draw.
pub fn resolve(challenger: &BattleTeam, defender: &BattleTeam, seed: u32) -> BattleOutcome {
    let mut rng = Mulberry32::new(seed);
    let mut fighters = [Fighter::new(challenger), Fighter::new(defender)];
    let mut events = vec![
        BattleEvent::Send {
            side: BattleSide::Defender,
            index: 0,
        },
        BattleEvent::Send {
            side: BattleSide::Challenger,
            index: 0,
        },
    ];

    for _ in 0..MAX_TURNS {
        let picks = [
            rng.index(fighters[0].mon().moves.len()),
            rng.index(fighters[1].mon().moves.len()),
        ];
        let order = if challenger_goes_first(&mut rng, &fighters, picks) {
            [BattleSide::Challenger, BattleSide::Defender]
        } else {
            [BattleSide::Defender, BattleSide::Challenger]
        };

        for side in order {
            let [challenger_fighter, defender_fighter] = &mut fighters;
            let (attacker, target) = match side {
                BattleSide::Challenger => (&*challenger_fighter, defender_fighter),
                BattleSide::Defender => (&*defender_fighter, challenger_fighter),
            };
            if !attacker.fainted() {
                attack(
                    &mut rng,
                    &mut events,
                    side,
                    attacker,
                    target,
                    picks[slot(side)],
                );
            }
        }

        for side in [BattleSide::Challenger, BattleSide::Defender] {
            let fighter = &mut fighters[slot(side)];
            if !fighter.fainted() {
                continue;
            }
            match fighter.send_next() {
                Some(index) => events.push(BattleEvent::Send {
                    side,
                    index: as_event_index(index),
                }),
                None => {
                    let winner = Some(side.opponent());
                    events.push(BattleEvent::End { winner });
                    return BattleOutcome { events, winner };
                }
            }
        }
    }

    events.push(BattleEvent::End { winner: None });
    BattleOutcome {
        events,
        winner: None,
    }
}

fn slot(side: BattleSide) -> usize {
    match side {
        BattleSide::Challenger => 0,
        BattleSide::Defender => 1,
    }
}

/// Team and move indices are below `MAX_BATTLE_TEAM_SIZE` and `MAX_BATTLE_MOVES`, which
/// `BattleTeam` guarantees.
fn as_event_index(index: usize) -> u8 {
    u8::try_from(index).expect("team and move indices fit in a u8")
}

fn challenger_goes_first(rng: &mut Mulberry32, fighters: &[Fighter; 2], picks: [usize; 2]) -> bool {
    let [challenger, defender] = fighters;
    let challenger_move = &challenger.mon().moves[picks[0]];
    let defender_move = &defender.mon().moves[picks[1]];
    let order = challenger_move.priority.cmp(&defender_move.priority).then(
        challenger
            .mon()
            .stats
            .speed
            .cmp(&defender.mon().stats.speed),
    );
    match order {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => rng.next_f64() < 0.5,
    }
}

/// One attack by `side`'s active buddy on the other side's, which is still standing.
fn attack(
    rng: &mut Mulberry32,
    events: &mut Vec<BattleEvent>,
    side: BattleSide,
    attacker: &Fighter<'_>,
    target: &mut Fighter<'_>,
    move_index: usize,
) {
    let target_side = side.opponent();
    let battle_move = &attacker.mon().moves[move_index];
    events.push(BattleEvent::Move {
        side,
        move_index: as_event_index(move_index),
    });

    if let Some(accuracy) = battle_move.accuracy {
        if rng.next_f64() * 100.0 >= f64::from(accuracy) {
            events.push(BattleEvent::Miss { side });
            return;
        }
    }

    let effectiveness = dex::effectiveness(battle_move.move_type, &target.mon().types);
    if effectiveness == 0.0 {
        events.push(BattleEvent::Immune { side: target_side });
        return;
    }

    let crit_stage = usize::from(battle_move.crit_stage).min(CRIT_CHANCE.len() - 1);
    let crit = rng.next_f64() < CRIT_CHANCE[crit_stage];
    let roll = ROLL_MIN + (rng.next_f64() * ROLL_STEPS) as u64;
    let raw_damage = damage(
        attacker.mon(),
        target.mon(),
        battle_move,
        crit,
        roll,
        effectiveness,
    );

    let dealt = target.hp.min(u16::try_from(raw_damage).unwrap_or(u16::MAX));
    target.hp -= dealt;
    events.push(BattleEvent::Hit {
        side: target_side,
        damage: dealt,
        hp_after: target.hp,
        effectiveness,
        crit,
    });
    if target.fainted() {
        events.push(BattleEvent::Faint { side: target_side });
    }
}

/// The generation 5 damage formula with whole-number rounding at every step: the
/// clamped power, a critical hit doubling, the roll in percent, a 1.5 same-type
/// bonus and the type multiplier, and always at least 1.
pub(super) fn damage(
    attacker: &BattleMon,
    target: &BattleMon,
    battle_move: &BattleMove,
    crit: bool,
    roll: u64,
    effectiveness: f64,
) -> u64 {
    let level_factor = (2.0 * battle_level(attacker.level) / 5.0 + 2.0).floor() as u64;
    let power = u64::from(battle_move.power.clamp(POWER_RANGE.0, POWER_RANGE.1));
    let (attack, defense) = match battle_move.category {
        MoveCategory::Physical => (attacker.stats.attack, target.stats.defense),
        MoveCategory::Special => (attacker.stats.special_attack, target.stats.special_defense),
    };

    let mut damage = level_factor * power * u64::from(attack) / u64::from(defense) / 50 + 2;
    if crit {
        damage *= 2;
    }
    damage = damage * roll / 100;
    if attacker.types.contains(&battle_move.move_type) {
        damage = damage * 3 / 2;
    }
    ((damage as f64 * effectiveness).floor() as u64).max(1)
}
