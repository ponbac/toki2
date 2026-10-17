//! Buddy battles: the colleagues to battle, the daily trainer, the caller's battles
//! with their replays, and the monthly leaderboard.
//!
//! Battles are shown from the caller's side: `role` is the side they fought on and
//! `result` how it ended for them.

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

use super::JsonBody;
use crate::{
    adapters::inbound::http::ai_subscriptions::iso_date,
    auth::AuthUser,
    domain::{
        models::{
            AiBillingMonth, BattleEvent, BattleId, BattleKind, BattleMon, BattleMove,
            BattleParticipant, BattleResult, BattleSide, BattleTally, BattleTeam, BattleView,
            BuddyBattle, BuddyBattleSummary, BuddyLeaderboard, BuddyLeaderboardRow, BuddyOpponent,
            BuddyOpponents, DailyTrainerStatus, MonType, MoveCategory, SpeciesId, UserId,
        },
        ports::inbound::BuddyService,
    },
    routes::ApiError,
};

/// A path parameter whose rejection is a `400` JSON error.
type PathParam<T> = WithRejection<Path<T>, ApiError>;
/// A query string whose rejection is a `400` JSON error.
type QueryParams<T> = WithRejection<Query<T>, ApiError>;

pub(super) fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    Arc<dyn BuddyService>: FromRef<S>,
{
    Router::new()
        .route("/opponents", get(opponents))
        .route("/battles", get(list_battles).post(battle_colleague))
        .route("/battles/{battle_id}", get(get_battle))
        .route("/battles/{battle_id}/seen", post(mark_battle_seen))
        .route("/trainer", get(trainer))
        .route("/trainer/battles", post(battle_trainer))
        .route("/leaderboard", get(leaderboard))
}

/// A buddy with its level, as a team shows it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TeamMemberResponse {
    species_id: i16,
    level: u8,
}

fn team_response(team: &[(SpeciesId, u8)]) -> Vec<TeamMemberResponse> {
    team.iter()
        .map(|&(species_id, level)| TeamMemberResponse {
            species_id: species_id.as_i16(),
            level,
        })
        .collect()
}

fn snapshot_team_response(team: &BattleTeam) -> Vec<TeamMemberResponse> {
    team.members()
        .iter()
        .map(|mon| TeamMemberResponse {
            species_id: mon.species_id.as_i16(),
            level: mon.level,
        })
        .collect()
}

/// Wins, losses and draws from the caller's side.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BattleRecordResponse {
    wins: i64,
    losses: i64,
    draws: i64,
}

impl From<BattleTally> for BattleRecordResponse {
    fn from(tally: BattleTally) -> Self {
        Self {
            wins: tally.wins,
            losses: tally.losses,
            draws: tally.draws,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum BattleKindResponse {
    Colleague,
    Trainer,
}

impl From<BattleKind> for BattleKindResponse {
    fn from(kind: BattleKind) -> Self {
        match kind {
            BattleKind::Colleague => Self::Colleague,
            BattleKind::Trainer => Self::Trainer,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum BattleResultResponse {
    Win,
    Loss,
    Draw,
}

impl From<BattleResult> for BattleResultResponse {
    fn from(result: BattleResult) -> Self {
        match result {
            BattleResult::Win => Self::Win,
            BattleResult::Loss => Self::Loss,
            BattleResult::Draw => Self::Draw,
        }
    }
}

/// The colleagues the caller can battle, by name. `nextBattleAt` is when the hourly
/// limit on colleague battles ends, `null` when a battle can start now.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OpponentsResponse {
    #[serde(with = "time::serde::rfc3339::option")]
    next_battle_at: Option<OffsetDateTime>,
    opponents: Vec<OpponentResponse>,
}

impl From<BuddyOpponents> for OpponentsResponse {
    fn from(opponents: BuddyOpponents) -> Self {
        Self {
            next_battle_at: opponents.next_battle_at,
            opponents: opponents.opponents.into_iter().map(Into::into).collect(),
        }
    }
}

/// A colleague with the team that would fight the caller's, the head-to-head record
/// from the caller's side, and whether the caller battled them today.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OpponentResponse {
    user_id: i32,
    full_name: String,
    team: Vec<TeamMemberResponse>,
    record: BattleRecordResponse,
    battled_today: bool,
}

impl From<BuddyOpponent> for OpponentResponse {
    fn from(opponent: BuddyOpponent) -> Self {
        Self {
            user_id: opponent.user_id.as_i32(),
            full_name: opponent.full_name,
            team: team_response(&opponent.team),
            record: opponent.record.into(),
            battled_today: opponent.battled_today,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BattleColleagueRequest {
    opponent_id: i32,
}

/// The other side of a battle: a colleague, or the daily trainer with a `null` user.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OpponentRefResponse {
    user_id: Option<i32>,
    name: String,
}

/// A battle in the caller's list. `seen` is whether the caller opened it, always true
/// for a battle they started.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BattleSummaryResponse {
    id: i32,
    kind: BattleKindResponse,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    role: BattleSide,
    opponent: OpponentRefResponse,
    my_team: Vec<TeamMemberResponse>,
    their_team: Vec<TeamMemberResponse>,
    result: BattleResultResponse,
    seen: bool,
}

impl BattleSummaryResponse {
    fn new(role: BattleSide, battle: &BuddyBattleSummary) -> Self {
        let mine = battle.participant(role);
        let theirs = battle.participant(role.opponent());
        Self {
            id: battle.id.as_i32(),
            kind: battle.kind.into(),
            created_at: battle.created_at,
            role,
            opponent: OpponentRefResponse {
                user_id: theirs.user_id.map(|user_id| user_id.as_i32()),
                name: theirs.name.clone(),
            },
            my_team: snapshot_team_response(&mine.team),
            their_team: snapshot_team_response(&theirs.team),
            result: battle.result_for(role).into(),
            seen: battle.seen_by(role),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BattleListResponse {
    battles: Vec<BattleSummaryResponse>,
}

/// A battle with everything its replay needs: both teams as they entered it and every
/// event in order.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BattleDetailResponse {
    #[serde(flatten)]
    summary: BattleSummaryResponse,
    /// The engine's seed as 8 hex digits.
    seed: String,
    engine_version: i16,
    challenger: ParticipantResponse,
    defender: ParticipantResponse,
    events: Vec<BattleEvent>,
}

impl From<BattleView<BuddyBattle>> for BattleDetailResponse {
    fn from(view: BattleView<BuddyBattle>) -> Self {
        let battle = view.battle;
        Self {
            summary: BattleSummaryResponse::new(view.role, &battle.summary),
            seed: format!("{:08x}", battle.seed),
            engine_version: battle.engine_version,
            challenger: battle.summary.challenger.into(),
            defender: battle.summary.defender.into(),
            events: battle.events,
        }
    }
}

/// One side of a battle; the daily trainer has a `null` user.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ParticipantResponse {
    user_id: Option<i32>,
    name: String,
    team: Vec<BattleMonResponse>,
}

impl From<BattleParticipant> for ParticipantResponse {
    fn from(participant: BattleParticipant) -> Self {
        Self {
            user_id: participant.user_id.map(|user_id| user_id.as_i32()),
            name: participant.name,
            team: participant
                .team
                .members()
                .iter()
                .map(BattleMonResponse::from)
                .collect(),
        }
    }
}

/// A buddy as it entered the battle. Its battle stats stay on the server.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BattleMonResponse {
    species_id: i16,
    name: String,
    level: u8,
    types: Vec<MonType>,
    max_hp: u16,
    moves: Vec<BattleMoveResponse>,
}

impl From<&BattleMon> for BattleMonResponse {
    fn from(mon: &BattleMon) -> Self {
        Self {
            species_id: mon.species_id.as_i16(),
            name: mon.name.clone(),
            level: mon.level,
            types: mon.types.clone(),
            max_hp: mon.max_hp,
            moves: mon.moves.iter().map(BattleMoveResponse::from).collect(),
        }
    }
}

/// A move as the dex lists it; `accuracy` is `null` for a move that never misses.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BattleMoveResponse {
    name: String,
    #[serde(rename = "type")]
    move_type: MonType,
    category: MoveCategory,
    power: u16,
    accuracy: Option<u8>,
    pp: u8,
}

impl From<&BattleMove> for BattleMoveResponse {
    fn from(battle_move: &BattleMove) -> Self {
        Self {
            name: battle_move.name.clone(),
            move_type: battle_move.move_type,
            category: battle_move.category,
            power: battle_move.power,
            accuracy: battle_move.accuracy,
            pp: battle_move.pp,
        }
    }
}

/// Today's daily trainer with the team that would fight the caller's, and the
/// caller's battle against it if they fought it today.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrainerResponse {
    #[serde(with = "iso_date")]
    date: Date,
    name: String,
    team: Vec<TeamMemberResponse>,
    battle_id: Option<i32>,
    result: Option<BattleResultResponse>,
}

impl From<DailyTrainerStatus> for TrainerResponse {
    fn from(status: DailyTrainerStatus) -> Self {
        Self {
            date: status.date,
            name: status.name,
            team: team_response(&status.team),
            battle_id: status.battle.map(|battle| battle.id.as_i32()),
            result: status.battle.map(|battle| battle.result().into()),
        }
    }
}

/// `month` is `YYYY-MM`, by default the current local month.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LeaderboardQuery {
    month: Option<String>,
}

/// Everyone who started a battle in the month, in no particular order.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LeaderboardResponse {
    month: String,
    rows: Vec<LeaderboardRowResponse>,
}

impl From<BuddyLeaderboard> for LeaderboardResponse {
    fn from(leaderboard: BuddyLeaderboard) -> Self {
        Self {
            month: leaderboard.month.to_string(),
            rows: leaderboard.rows.into_iter().map(Into::into).collect(),
        }
    }
}

/// The battles one user started in the month: colleague battles by result, and the
/// days they beat the daily trainer.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LeaderboardRowResponse {
    user_id: i32,
    full_name: String,
    team: Vec<TeamMemberResponse>,
    wins: i64,
    losses: i64,
    draws: i64,
    trainer_wins: i64,
}

impl From<BuddyLeaderboardRow> for LeaderboardRowResponse {
    fn from(row: BuddyLeaderboardRow) -> Self {
        Self {
            user_id: row.user_id.as_i32(),
            full_name: row.full_name,
            team: team_response(&row.team),
            wins: row.colleague.wins,
            losses: row.colleague.losses,
            draws: row.colleague.draws,
            trainer_wins: row.trainer_wins,
        }
    }
}

/// The colleagues the caller can battle.
async fn opponents(
    user: AuthUser,
    State(service): State<Arc<dyn BuddyService>>,
) -> Result<Json<OpponentsResponse>, ApiError> {
    let opponents = service.opponents(&user.id).await?;
    Ok(Json(opponents.into()))
}

/// Battles a colleague's buddies; `201` with the battle and its replay.
async fn battle_colleague(
    user: AuthUser,
    State(service): State<Arc<dyn BuddyService>>,
    WithRejection(Json(request), _): JsonBody<BattleColleagueRequest>,
) -> Result<(StatusCode, Json<BattleDetailResponse>), ApiError> {
    let battle = service
        .battle_colleague(&user.id, &UserId::new(request.opponent_id))
        .await?;
    Ok((StatusCode::CREATED, Json(battle.into())))
}

/// Today's daily trainer.
async fn trainer(
    user: AuthUser,
    State(service): State<Arc<dyn BuddyService>>,
) -> Result<Json<TrainerResponse>, ApiError> {
    let trainer = service.trainer(&user.id).await?;
    Ok(Json(trainer.into()))
}

/// Battles today's daily trainer; `201` with the battle and its replay. Any body is
/// ignored, since there is nothing to choose.
async fn battle_trainer(
    user: AuthUser,
    State(service): State<Arc<dyn BuddyService>>,
) -> Result<(StatusCode, Json<BattleDetailResponse>), ApiError> {
    let battle = service.battle_trainer(&user.id).await?;
    Ok((StatusCode::CREATED, Json(battle.into())))
}

/// The caller's recent battles, newest first.
async fn list_battles(
    user: AuthUser,
    State(service): State<Arc<dyn BuddyService>>,
) -> Result<Json<BattleListResponse>, ApiError> {
    let battles = service.battles(&user.id).await?;
    Ok(Json(BattleListResponse {
        battles: battles
            .iter()
            .map(|view| BattleSummaryResponse::new(view.role, &view.battle))
            .collect(),
    }))
}

/// A battle the caller took part in, with its replay; `404` for any other.
async fn get_battle(
    user: AuthUser,
    State(service): State<Arc<dyn BuddyService>>,
    WithRejection(Path(battle_id), _): PathParam<i32>,
) -> Result<Json<BattleDetailResponse>, ApiError> {
    let battle = service.battle(&user.id, BattleId::new(battle_id)).await?;
    Ok(Json(battle.into()))
}

/// Records that the caller saw a battle they defended; `204`.
async fn mark_battle_seen(
    user: AuthUser,
    State(service): State<Arc<dyn BuddyService>>,
    WithRejection(Path(battle_id), _): PathParam<i32>,
) -> Result<StatusCode, ApiError> {
    service
        .mark_battle_seen(&user.id, BattleId::new(battle_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The leaderboard of a local month.
async fn leaderboard(
    State(service): State<Arc<dyn BuddyService>>,
    WithRejection(Query(query), _): QueryParams<LeaderboardQuery>,
) -> Result<Json<LeaderboardResponse>, ApiError> {
    let month = query
        .month
        .map(|raw| {
            AiBillingMonth::parse(&raw).ok_or_else(|| {
                ApiError::bad_request(format!("expected a month as YYYY-MM, got {raw:?}"))
            })
        })
        .transpose()?;
    let leaderboard = service.leaderboard(month).await?;
    Ok(Json(leaderboard.into()))
}
