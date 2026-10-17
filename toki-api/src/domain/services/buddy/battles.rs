//! The battle rules: who may battle whom and when, and which buddies fight.
//!
//! These functions decide from data that was already read, so a new battle is decided
//! from a setup read while the challenger's battles are locked.

use crate::domain::{
    models::{
        next_colleague_battle_at, partners_fight, BattleSetup, BuddyLeaderboardRow, BuddyOpponent,
        BuddyProfile, BuddyRival, BuddyStanding, DailyTrainer, NewBuddyBattle, SpeciesId,
    },
    services::battle,
    BuddyError,
};

/// A user battles, and is battled, only with their buddy switched on.
pub(super) fn ensure_enabled(profile: &BuddyProfile) -> Result<(), BuddyError> {
    if profile.enabled {
        Ok(())
    } else {
        Err(BuddyError::Disabled)
    }
}

/// A battle against the colleague in `setup`: both buddies switched on, not yet battled
/// today, and the challenger's last colleague battle at least a cooldown ago.
pub(super) fn plan_colleague_battle(
    setup: &BattleSetup,
    seed: u32,
) -> Result<NewBuddyBattle, BuddyError> {
    let challenger = setup.challenger.as_ref().ok_or(BuddyError::NotEnrolled)?;
    ensure_enabled(&challenger.profile)?;
    let defender = setup
        .defender
        .as_ref()
        .filter(|defender| defender.profile.enabled)
        .ok_or(BuddyError::OpponentUnavailable)?;
    if setup.fought_today {
        return Err(BuddyError::BattledToday);
    }
    if let Some(next_battle_at) =
        next_colleague_battle_at(setup.last_colleague_battle_at, setup.clock.now)
    {
        return Err(BuddyError::Cooldown {
            wait: next_battle_at - setup.clock.now,
        });
    }

    let pairs = partners_fight(&challenger.profile, &defender.profile);
    fight(
        setup,
        seed,
        &challenger.team(pairs),
        &defender.team(pairs),
        None,
    )
}

/// A battle against the day's trainer, once a day, whatever the colleague cooldown.
pub(super) fn plan_trainer_battle(
    setup: &BattleSetup,
    seed: u32,
) -> Result<NewBuddyBattle, BuddyError> {
    let challenger = setup.challenger.as_ref().ok_or(BuddyError::NotEnrolled)?;
    ensure_enabled(&challenger.profile)?;
    if setup.fought_today {
        return Err(BuddyError::TrainerAlreadyFought);
    }

    let trainer = battle::daily_trainer(setup.clock.today);
    let pairs = challenger.profile.partner.is_some();
    fight(
        setup,
        seed,
        &challenger.team(pairs),
        &trainer_team(&trainer, &challenger.profile),
        Some(trainer.name),
    )
}

/// The trainer's buddies that fight a user: both when the user has a partner, as the
/// trainer always does, otherwise its lead alone.
pub(super) fn trainer_team(trainer: &DailyTrainer, user: &BuddyProfile) -> Vec<(SpeciesId, u8)> {
    let fighting = if user.partner.is_some() {
        trainer.team.len()
    } else {
        1
    };
    trainer.team.iter().copied().take(fighting).collect()
}

/// A colleague as `user` sees them: with the team that would fight `user`'s.
pub(super) fn opponent(user: &BuddyProfile, rival: BuddyRival) -> BuddyOpponent {
    let pairs = partners_fight(user, &rival.competitor.profile);
    BuddyOpponent {
        user_id: rival.competitor.user_id(),
        team: rival.competitor.team(pairs),
        full_name: rival.competitor.full_name,
        record: rival.record,
        battled_today: rival.battled_today,
    }
}

/// A leaderboard row shows all of the user's buddies.
pub(super) fn leaderboard_row(standing: BuddyStanding) -> BuddyLeaderboardRow {
    BuddyLeaderboardRow {
        user_id: standing.competitor.user_id(),
        team: standing.competitor.team(true),
        full_name: standing.competitor.full_name,
        colleague: standing.colleague,
        trainer_wins: standing.trainer_wins,
    }
}

/// Snapshots both teams at their levels and resolves the battle on the setup's day.
fn fight(
    setup: &BattleSetup,
    seed: u32,
    challenger: &[(SpeciesId, u8)],
    defender: &[(SpeciesId, u8)],
    trainer_name: Option<String>,
) -> Result<NewBuddyBattle, BuddyError> {
    let challenger_team = battle::build_team(challenger).map_err(BuddyError::Battle)?;
    let defender_team = battle::build_team(defender).map_err(BuddyError::Battle)?;
    let outcome = battle::resolve(&challenger_team, &defender_team, seed);

    Ok(NewBuddyBattle {
        battle_day: setup.clock.today,
        seed,
        engine_version: battle::ENGINE_VERSION,
        challenger_team,
        defender_team,
        trainer_name,
        events: outcome.events,
        winner: outcome.winner,
    })
}
