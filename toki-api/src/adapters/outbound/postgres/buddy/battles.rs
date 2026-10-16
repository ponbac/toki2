//! Buddy battles in Postgres: starting and reading battles, the colleagues a user can
//! battle, and the monthly standings.
//!
//! Battle teams and events are stored as the JSON the domain types serialise to.

use std::collections::HashMap;

use sqlx::{types::Json, Executor, Postgres};
use time::{Date, OffsetDateTime};

use super::{profile_from_rows, read_clock, storage_error, write_error, StoredBuddy};
use crate::{
    db::DbPool,
    domain::{
        models::{
            AiBillingMonth, AiUsageTimeZone, BattleEvent, BattleId, BattleKind, BattleOpponent,
            BattleParticipant, BattleSetup, BattleSide, BattleTally, BattleTeam, BuddyBattle,
            BuddyBattleSummary, BuddyCompetitor, BuddyRival, BuddyStanding, BuddyXpTally,
            NewBuddyBattle, TrainerBattle, UserId,
        },
        ports::outbound::BattlePlan,
        BuddyError,
    },
};

/// A row of `buddy_profiles` joined with one of its `buddies`, the user's name and the
/// stored sources of their XP.
struct StoredCompetitor {
    user_id: i32,
    enabled: bool,
    battle_notifications: bool,
    xp_since: Date,
    hours_synced_at: Option<OffsetDateTime>,
    hours_sync_attempted_at: Option<OffsetDateTime>,
    slot: Option<i16>,
    species_id: Option<i16>,
    visible: Option<bool>,
    xp_base: Option<i64>,
    full_name: String,
    hours_xp: i64,
    battles: i64,
}

impl StoredCompetitor {
    fn buddy_row(&self) -> StoredBuddy {
        StoredBuddy {
            user_id: self.user_id,
            enabled: self.enabled,
            battle_notifications: self.battle_notifications,
            xp_since: self.xp_since,
            hours_synced_at: self.hours_synced_at,
            hours_sync_attempted_at: self.hours_sync_attempted_at,
            slot: self.slot,
            species_id: self.species_id,
            visible: self.visible,
            xp_base: self.xp_base,
        }
    }
}

/// The competitors in `rows`, which hold each user's buddies next to each other.
fn competitors_from_rows(rows: Vec<StoredCompetitor>) -> Result<Vec<BuddyCompetitor>, BuddyError> {
    let mut competitors = Vec::new();
    let mut rows = rows.into_iter().peekable();
    while let Some(first) = rows.next() {
        let mut buddies = vec![first.buddy_row()];
        while let Some(row) = rows.next_if(|row| row.user_id == first.user_id) {
            buddies.push(row.buddy_row());
        }
        if let Some(profile) = profile_from_rows(buddies)? {
            let tally = BuddyXpTally {
                hours_xp: first.hours_xp,
                battles: first.battles,
            };
            competitors.push(BuddyCompetitor {
                profile,
                full_name: first.full_name,
                total_xp: tally.total_xp(),
            });
        }
    }
    Ok(competitors)
}

/// The enrolled users among `user_ids`, by name.
async fn load_competitors<'e, E>(
    executor: E,
    user_ids: &[i32],
) -> Result<Vec<BuddyCompetitor>, BuddyError>
where
    E: Executor<'e, Database = Postgres>,
{
    let rows = sqlx::query_as!(
        StoredCompetitor,
        r#"
        SELECT
            profile.user_id,
            profile.enabled,
            profile.battle_notifications,
            profile.xp_since,
            profile.hours_synced_at,
            profile.hours_sync_attempted_at,
            buddy.slot AS "slot?",
            buddy.species_id AS "species_id?",
            buddy.visible AS "visible?",
            buddy.xp_base AS "xp_base?",
            users.full_name,
            hours.xp AS "hours_xp!",
            started.battles AS "battles!"
        FROM buddy_profiles AS profile
        JOIN users ON users.id = profile.user_id
        LEFT JOIN buddies AS buddy ON buddy.user_id = profile.user_id
        CROSS JOIN LATERAL (
            SELECT COALESCE(sum(xp), 0)::int8 AS xp
            FROM buddy_xp_days
            WHERE user_id = profile.user_id
        ) AS hours
        CROSS JOIN LATERAL (
            SELECT count(*) AS battles
            FROM buddy_battles
            WHERE challenger_id = profile.user_id
        ) AS started
        WHERE profile.user_id = ANY($1)
        ORDER BY users.full_name, profile.user_id, buddy.slot
        "#,
        user_ids,
    )
    .fetch_all(executor)
    .await
    .map_err(storage_error)?;

    competitors_from_rows(rows)
}

pub(super) async fn rivals(
    pool: &DbPool,
    user_id: &UserId,
    today: Date,
) -> Result<Vec<BuddyRival>, BuddyError> {
    let rival_ids = sqlx::query_scalar!(
        r#"
        SELECT user_id
        FROM buddy_profiles
        WHERE enabled
          AND user_id <> $1
        "#,
        user_id.as_i32(),
    )
    .fetch_all(pool)
    .await
    .map_err(storage_error)?;
    let competitors = load_competitors(pool, &rival_ids).await?;

    // Colleague battles either side started, counted from the user's side.
    let mut records: HashMap<i32, (BattleTally, bool)> = sqlx::query!(
        r#"
        SELECT
            CASE WHEN challenger_id = $1 THEN defender_id ELSE challenger_id END
                AS "opponent_id!",
            count(*) FILTER (
                WHERE winner = CASE WHEN challenger_id = $1 THEN 'challenger' ELSE 'defender' END
            ) AS "wins!",
            count(*) FILTER (
                WHERE winner = CASE WHEN challenger_id = $1 THEN 'defender' ELSE 'challenger' END
            ) AS "losses!",
            count(*) FILTER (WHERE winner IS NULL) AS "draws!",
            COALESCE(bool_or(challenger_id = $1 AND battle_day = $2), false)
                AS "battled_today!"
        FROM buddy_battles
        WHERE kind = 'colleague'
          AND (challenger_id = $1 OR defender_id = $1)
        GROUP BY 1
        "#,
        user_id.as_i32(),
        today,
    )
    .fetch_all(pool)
    .await
    .map_err(storage_error)?
    .into_iter()
    .map(|row| {
        let record = BattleTally {
            wins: row.wins,
            losses: row.losses,
            draws: row.draws,
        };
        (row.opponent_id, (record, row.battled_today))
    })
    .collect();

    Ok(competitors
        .into_iter()
        .map(|competitor| {
            let (record, battled_today) = records
                .remove(&competitor.user_id().as_i32())
                .unwrap_or_default();
            BuddyRival {
                competitor,
                record,
                battled_today,
            }
        })
        .collect())
}

pub(super) async fn last_colleague_battle_at<'e, E>(
    executor: E,
    user_id: &UserId,
) -> Result<Option<OffsetDateTime>, BuddyError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar!(
        r#"
        SELECT max(created_at)
        FROM buddy_battles
        WHERE challenger_id = $1
          AND kind = 'colleague'
        "#,
        user_id.as_i32(),
    )
    .fetch_one(executor)
    .await
    .map_err(storage_error)
}

/// Whether the challenger already fought `opponent` on the local date `day`.
async fn fought_on<'e, E>(
    executor: E,
    challenger_id: &UserId,
    opponent: BattleOpponent,
    day: Date,
) -> Result<bool, BuddyError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM buddy_battles
            WHERE challenger_id = $1
              AND kind = $2
              AND defender_id IS NOT DISTINCT FROM $3
              AND battle_day = $4
        ) AS "fought!"
        "#,
        challenger_id.as_i32(),
        kind_name(opponent.kind()),
        colleague_id(opponent),
        day,
    )
    .fetch_one(executor)
    .await
    .map_err(storage_error)
}

pub(super) async fn start_battle(
    pool: &DbPool,
    challenger_id: &UserId,
    opponent: BattleOpponent,
    time_zone: &AiUsageTimeZone,
    plan: &BattlePlan<'_>,
) -> Result<(BuddyBattle, BattleSetup), BuddyError> {
    let mut transaction = pool.begin().await.map_err(storage_error)?;

    // A challenger's concurrent battles wait here for each other, so each is planned
    // knowing the ones before it. A user without a profile has nothing to lock, and
    // the plan refuses them.
    sqlx::query_scalar!(
        r#"
        SELECT user_id
        FROM buddy_profiles
        WHERE user_id = $1
        FOR UPDATE
        "#,
        challenger_id.as_i32(),
    )
    .fetch_optional(&mut transaction.executor())
    .await
    .map_err(storage_error)?;

    let clock = read_clock(&mut transaction.executor(), time_zone).await?;
    let user_ids: Vec<i32> = std::iter::once(challenger_id.as_i32())
        .chain(colleague_id(opponent))
        .collect();
    let mut competitors = load_competitors(&mut transaction.executor(), &user_ids).await?;
    let mut take = |user_id: UserId| {
        competitors
            .iter()
            .position(|competitor| competitor.user_id() == user_id)
            .map(|index| competitors.swap_remove(index))
    };
    let challenger = take(*challenger_id);
    let defender = match opponent {
        BattleOpponent::Colleague(defender_id) => take(defender_id),
        BattleOpponent::Trainer => None,
    };
    let last_colleague_battle_at =
        last_colleague_battle_at(&mut transaction.executor(), challenger_id).await?;
    let fought_today = fought_on(
        &mut transaction.executor(),
        challenger_id,
        opponent,
        clock.today,
    )
    .await?;
    let setup = BattleSetup {
        clock,
        challenger,
        defender,
        last_colleague_battle_at,
        fought_today,
    };

    // A refused battle rolls back and stores nothing.
    let battle = plan(&setup)?;
    let battle_id = insert_battle(
        &mut transaction.executor(),
        challenger_id,
        opponent,
        &battle,
    )
    .await?;
    let stored = load_battle(&mut transaction.executor(), battle_id)
        .await?
        .ok_or_else(|| BuddyError::Storage(format!("battle {battle_id} vanished on insert")))?;

    transaction.commit().await.map_err(storage_error)?;
    Ok((stored, setup))
}

async fn insert_battle<'e, E>(
    executor: E,
    challenger_id: &UserId,
    opponent: BattleOpponent,
    battle: &NewBuddyBattle,
) -> Result<BattleId, BuddyError>
where
    E: Executor<'e, Database = Postgres>,
{
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO buddy_battles (
            kind, challenger_id, defender_id, battle_day, seed, engine_version, winner,
            challenger_team, defender_team, events, trainer_name
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING id
        "#,
        kind_name(opponent.kind()),
        challenger_id.as_i32(),
        colleague_id(opponent),
        battle.battle_day,
        i64::from(battle.seed),
        battle.engine_version,
        battle.winner.map(side_name),
        to_json(&battle.challenger_team)?,
        to_json(&battle.defender_team)?,
        to_json(&battle.events)?,
        battle.trainer_name.as_deref(),
    )
    .fetch_one(executor)
    .await
    .map_err(write_error)?;

    Ok(BattleId::new(id))
}

/// A battle row without its seed and events, with the names of both sides.
struct StoredBattleSummary {
    id: i32,
    kind: String,
    created_at: OffsetDateTime,
    challenger_id: i32,
    challenger_name: String,
    defender_id: Option<i32>,
    defender_name: Option<String>,
    trainer_name: Option<String>,
    winner: Option<String>,
    challenger_team: Json<BattleTeam>,
    defender_team: Json<BattleTeam>,
    defender_seen_at: Option<OffsetDateTime>,
}

impl TryFrom<StoredBattleSummary> for BuddyBattleSummary {
    type Error = BuddyError;

    fn try_from(row: StoredBattleSummary) -> Result<Self, Self::Error> {
        let kind = parse_kind(&row.kind)?;
        let defender_name = match kind {
            BattleKind::Colleague => row.defender_name,
            BattleKind::Trainer => row.trainer_name,
        }
        .ok_or_else(|| BuddyError::Storage(format!("battle {} has no defender", row.id)))?;

        Ok(Self {
            id: BattleId::new(row.id),
            kind,
            created_at: row.created_at,
            challenger: BattleParticipant {
                user_id: Some(UserId::new(row.challenger_id)),
                name: row.challenger_name,
                team: row.challenger_team.0,
            },
            defender: BattleParticipant {
                user_id: row.defender_id.map(UserId::new),
                name: defender_name,
                team: row.defender_team.0,
            },
            winner: row.winner.as_deref().map(parse_side).transpose()?,
            defender_seen_at: row.defender_seen_at,
        })
    }
}

/// A whole battle row, with the names of both sides.
struct StoredBattle {
    id: i32,
    kind: String,
    created_at: OffsetDateTime,
    challenger_id: i32,
    challenger_name: String,
    defender_id: Option<i32>,
    defender_name: Option<String>,
    trainer_name: Option<String>,
    winner: Option<String>,
    challenger_team: Json<BattleTeam>,
    defender_team: Json<BattleTeam>,
    defender_seen_at: Option<OffsetDateTime>,
    seed: i64,
    engine_version: i16,
    events: Json<Vec<BattleEvent>>,
}

impl TryFrom<StoredBattle> for BuddyBattle {
    type Error = BuddyError;

    fn try_from(row: StoredBattle) -> Result<Self, Self::Error> {
        let seed = u32::try_from(row.seed)
            .map_err(|_| BuddyError::Storage(format!("battle {} has seed {}", row.id, row.seed)))?;
        let summary = StoredBattleSummary {
            id: row.id,
            kind: row.kind,
            created_at: row.created_at,
            challenger_id: row.challenger_id,
            challenger_name: row.challenger_name,
            defender_id: row.defender_id,
            defender_name: row.defender_name,
            trainer_name: row.trainer_name,
            winner: row.winner,
            challenger_team: row.challenger_team,
            defender_team: row.defender_team,
            defender_seen_at: row.defender_seen_at,
        };

        Ok(Self {
            summary: summary.try_into()?,
            seed,
            engine_version: row.engine_version,
            events: row.events.0,
        })
    }
}

pub(super) async fn load_battle<'e, E>(
    executor: E,
    battle_id: BattleId,
) -> Result<Option<BuddyBattle>, BuddyError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as!(
        StoredBattle,
        r#"
        SELECT
            battle.id,
            battle.kind,
            battle.created_at,
            battle.challenger_id,
            challenger.full_name AS challenger_name,
            battle.defender_id,
            defender.full_name AS "defender_name?",
            battle.trainer_name,
            battle.winner,
            battle.challenger_team AS "challenger_team: Json<BattleTeam>",
            battle.defender_team AS "defender_team: Json<BattleTeam>",
            battle.defender_seen_at,
            battle.seed,
            battle.engine_version,
            battle.events AS "events: Json<Vec<BattleEvent>>"
        FROM buddy_battles AS battle
        JOIN users AS challenger ON challenger.id = battle.challenger_id
        LEFT JOIN users AS defender ON defender.id = battle.defender_id
        WHERE battle.id = $1
        "#,
        battle_id.as_i32(),
    )
    .fetch_optional(executor)
    .await
    .map_err(storage_error)?
    .map(TryInto::try_into)
    .transpose()
}

pub(super) async fn recent_battles(
    pool: &DbPool,
    user_id: &UserId,
    since: OffsetDateTime,
    limit: i64,
) -> Result<Vec<BuddyBattleSummary>, BuddyError> {
    sqlx::query_as!(
        StoredBattleSummary,
        r#"
        SELECT
            battle.id,
            battle.kind,
            battle.created_at,
            battle.challenger_id,
            challenger.full_name AS challenger_name,
            battle.defender_id,
            defender.full_name AS "defender_name?",
            battle.trainer_name,
            battle.winner,
            battle.challenger_team AS "challenger_team: Json<BattleTeam>",
            battle.defender_team AS "defender_team: Json<BattleTeam>",
            battle.defender_seen_at
        FROM buddy_battles AS battle
        JOIN users AS challenger ON challenger.id = battle.challenger_id
        LEFT JOIN users AS defender ON defender.id = battle.defender_id
        WHERE (battle.challenger_id = $1 OR battle.defender_id = $1)
          AND battle.created_at >= $2
        ORDER BY battle.created_at DESC, battle.id DESC
        LIMIT $3
        "#,
        user_id.as_i32(),
        since,
        limit,
    )
    .fetch_all(pool)
    .await
    .map_err(storage_error)?
    .into_iter()
    .map(TryInto::try_into)
    .collect()
}

pub(super) async fn trainer_battle(
    pool: &DbPool,
    user_id: &UserId,
    day: Date,
) -> Result<Option<TrainerBattle>, BuddyError> {
    let row = sqlx::query!(
        r#"
        SELECT id, winner
        FROM buddy_battles
        WHERE kind = 'trainer'
          AND challenger_id = $1
          AND battle_day = $2
        "#,
        user_id.as_i32(),
        day,
    )
    .fetch_optional(pool)
    .await
    .map_err(storage_error)?;

    row.map(|row| {
        Ok(TrainerBattle {
            id: BattleId::new(row.id),
            winner: row.winner.as_deref().map(parse_side).transpose()?,
        })
    })
    .transpose()
}

pub(super) async fn mark_battle_seen(
    pool: &DbPool,
    battle_id: BattleId,
    defender_id: &UserId,
) -> Result<(), BuddyError> {
    sqlx::query!(
        r#"
        UPDATE buddy_battles
        SET defender_seen_at = now()
        WHERE id = $1
          AND defender_id = $2
          AND defender_seen_at IS NULL
        "#,
        battle_id.as_i32(),
        defender_id.as_i32(),
    )
    .execute(pool)
    .await
    .map_err(storage_error)?;

    Ok(())
}

pub(super) async fn standings(
    pool: &DbPool,
    month: AiBillingMonth,
) -> Result<Vec<BuddyStanding>, BuddyError> {
    let mut results: HashMap<i32, (BattleTally, i64)> = sqlx::query!(
        r#"
        SELECT
            challenger_id,
            count(*) FILTER (WHERE kind = 'colleague' AND winner = 'challenger') AS "wins!",
            count(*) FILTER (WHERE kind = 'colleague' AND winner = 'defender') AS "losses!",
            count(*) FILTER (WHERE kind = 'colleague' AND winner IS NULL) AS "draws!",
            count(DISTINCT battle_day) FILTER (WHERE kind = 'trainer' AND winner = 'challenger')
                AS "trainer_wins!"
        FROM buddy_battles
        WHERE battle_day BETWEEN $1 AND $2
        GROUP BY challenger_id
        "#,
        month.first_day(),
        month.last_day(),
    )
    .fetch_all(pool)
    .await
    .map_err(storage_error)?
    .into_iter()
    .map(|row| {
        let colleague = BattleTally {
            wins: row.wins,
            losses: row.losses,
            draws: row.draws,
        };
        (row.challenger_id, (colleague, row.trainer_wins))
    })
    .collect();

    let user_ids: Vec<i32> = results.keys().copied().collect();
    Ok(load_competitors(pool, &user_ids)
        .await?
        .into_iter()
        .filter_map(|competitor| {
            let (colleague, trainer_wins) = results.remove(&competitor.user_id().as_i32())?;
            Some(BuddyStanding {
                competitor,
                colleague,
                trainer_wins,
            })
        })
        .collect())
}

fn colleague_id(opponent: BattleOpponent) -> Option<i32> {
    match opponent {
        BattleOpponent::Colleague(user_id) => Some(user_id.as_i32()),
        BattleOpponent::Trainer => None,
    }
}

fn kind_name(kind: BattleKind) -> &'static str {
    match kind {
        BattleKind::Colleague => "colleague",
        BattleKind::Trainer => "trainer",
    }
}

fn parse_kind(name: &str) -> Result<BattleKind, BuddyError> {
    match name {
        "colleague" => Ok(BattleKind::Colleague),
        "trainer" => Ok(BattleKind::Trainer),
        _ => Err(BuddyError::Storage(format!("unknown battle kind {name:?}"))),
    }
}

fn side_name(side: BattleSide) -> &'static str {
    match side {
        BattleSide::Challenger => "challenger",
        BattleSide::Defender => "defender",
    }
}

fn parse_side(name: &str) -> Result<BattleSide, BuddyError> {
    match name {
        "challenger" => Ok(BattleSide::Challenger),
        "defender" => Ok(BattleSide::Defender),
        _ => Err(BuddyError::Storage(format!("unknown battle side {name:?}"))),
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<serde_json::Value, BuddyError> {
    serde_json::to_value(value).map_err(|error| BuddyError::Storage(error.to_string()))
}

#[cfg(test)]
mod tests;
