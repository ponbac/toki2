use std::sync::Mutex;

use sqlx::PgPool;
use time::{
    macros::{date, datetime},
    Duration,
};

use super::*;
use crate::{
    adapters::outbound::postgres::PostgresBuddyRepository,
    domain::{
        models::{BuddyXpDay, NewBuddyProfile, SpeciesId},
        ports::outbound::BuddyRepository,
        services::battle,
    },
};

const PIKACHU: SpeciesId = SpeciesId::new(25);
const SQUIRTLE: SpeciesId = SpeciesId::new(7);

fn db(pool: &PgPool) -> DbPool {
    sqlx_tracing::PoolBuilder::from(pool.clone()).build()
}

fn repository(pool: &PgPool) -> PostgresBuddyRepository {
    PostgresBuddyRepository::new(db(pool))
}

fn time_zone() -> AiUsageTimeZone {
    AiUsageTimeZone::parse("Europe/Stockholm").unwrap()
}

async fn insert_user(pool: &PgPool, full_name: &str) -> UserId {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, full_name, picture, access_token)
         VALUES ($1, $2, '', '')
         RETURNING id",
    )
    .bind(format!(
        "{}@example.com",
        full_name.to_lowercase().replace(' ', ".")
    ))
    .bind(full_name)
    .fetch_one(pool)
    .await
    .unwrap();
    UserId::new(id)
}

/// A user with a lead buddy, switched on or off.
async fn enrolled(pool: &PgPool, full_name: &str, lead: SpeciesId, enabled: bool) -> UserId {
    let user_id = insert_user(pool, full_name).await;
    repository(pool)
        .enrol(
            &user_id,
            &NewBuddyProfile {
                enabled,
                battle_notifications: true,
                lead_species_id: lead,
                xp_since: date!(2026 - 10 - 01),
            },
        )
        .await
        .unwrap()
        .unwrap();
    user_id
}

/// A stored battle, written directly to set its day, time and result.
struct Row {
    challenger: UserId,
    defender: Option<UserId>,
    day: Date,
    created_at: OffsetDateTime,
    winner: Option<&'static str>,
}

impl Row {
    fn new(challenger: UserId, defender: Option<UserId>, day: Date) -> Self {
        Self {
            challenger,
            defender,
            day,
            created_at: day.midnight().assume_utc() + Duration::hours(12),
            winner: Some("challenger"),
        }
    }

    fn won_by(self, winner: Option<&'static str>) -> Self {
        Self { winner, ..self }
    }

    fn at(self, created_at: OffsetDateTime) -> Self {
        Self { created_at, ..self }
    }

    async fn insert(self, pool: &PgPool) -> BattleId {
        let team = serde_json::to_value(battle::build_team(&[(PIKACHU, 5)]).unwrap()).unwrap();
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO buddy_battles (
                 kind, challenger_id, defender_id, battle_day, seed, engine_version, winner,
                 challenger_team, defender_team, events, trainer_name, created_at
             )
             VALUES (
                 CASE WHEN $2::int4 IS NULL THEN 'trainer' ELSE 'colleague' END,
                 $1, $2, $3, 1, 1, $4, $5, $5, '[]',
                 CASE WHEN $2::int4 IS NULL THEN 'Ace Trainer Kim' END,
                 $6
             )
             RETURNING id",
        )
        .bind(self.challenger.as_i32())
        .bind(self.defender.map(|user_id| user_id.as_i32()))
        .bind(self.day)
        .bind(self.winner)
        .bind(team)
        .bind(self.created_at)
        .fetch_one(pool)
        .await
        .unwrap();
        BattleId::new(id)
    }
}

async fn battle_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM buddy_battles")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Resolves a battle between the setup's leads, or against the day's trainer, with
/// `seed`, whatever the rules say.
fn fight(setup: &BattleSetup, seed: u32) -> Result<NewBuddyBattle, BuddyError> {
    let challenger = setup.challenger.as_ref().ok_or(BuddyError::NotEnrolled)?;
    let trainer = battle::daily_trainer(setup.clock.today);
    let (defender, trainer_name) = match &setup.defender {
        Some(defender) => (defender.team(false), None),
        None => (trainer.team[..1].to_vec(), Some(trainer.name)),
    };
    let challenger_team = battle::build_team(&challenger.team(false)).unwrap();
    let defender_team = battle::build_team(&defender).unwrap();
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

#[sqlx::test]
async fn a_started_battle_is_stored_with_both_teams_and_read_back_whole(pool: PgPool) {
    let repository = repository(&pool);
    let ada = enrolled(&pool, "Ada Lovelace", PIKACHU, true).await;
    let anna = enrolled(&pool, "Anna Lind", SQUIRTLE, true).await;
    // Ten hours are 600 XP, Lv 10.
    repository
        .save_logged_days(&ada, &[BuddyXpDay::new(date!(2026 - 10 - 02), 10.0)])
        .await
        .unwrap();
    let clock = repository.clock(&time_zone()).await.unwrap();

    let (battle, setup) = repository
        .start_battle(
            &ada,
            BattleOpponent::Colleague(anna),
            &time_zone(),
            &|setup| fight(setup, u32::MAX),
        )
        .await
        .unwrap();

    assert_eq!(setup.clock.today, clock.today);
    let challenger = setup.challenger.as_ref().unwrap();
    assert_eq!(challenger.user_id(), ada);
    assert_eq!(challenger.full_name, "Ada Lovelace");
    assert_eq!(challenger.total_xp, 600);
    let defender = setup.defender.as_ref().unwrap();
    assert_eq!(
        (
            defender.user_id(),
            defender.full_name.as_str(),
            defender.total_xp
        ),
        (anna, "Anna Lind", 0)
    );
    assert_eq!(setup.last_colleague_battle_at, None);
    assert!(!setup.fought_today);

    let planned = fight(&setup, u32::MAX).unwrap();
    assert_eq!(battle.seed, u32::MAX);
    assert_eq!(battle.engine_version, battle::ENGINE_VERSION);
    assert_eq!(battle.events, planned.events);
    let summary = &battle.summary;
    assert_eq!(summary.kind, BattleKind::Colleague);
    assert_eq!(
        summary.challenger,
        BattleParticipant {
            user_id: Some(ada),
            name: "Ada Lovelace".to_string(),
            team: planned.challenger_team,
        }
    );
    assert_eq!(
        summary.defender,
        BattleParticipant {
            user_id: Some(anna),
            name: "Anna Lind".to_string(),
            team: planned.defender_team,
        }
    );
    assert_eq!(summary.winner, planned.winner);
    assert_eq!(summary.defender_seen_at, None);
    assert_eq!(summary.created_at, setup.clock.now);

    assert_eq!(
        repository.battle(summary.id).await.unwrap(),
        Some(battle.clone())
    );
    assert_eq!(
        repository
            .battle(BattleId::new(summary.id.as_i32() + 1))
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        repository.last_colleague_battle_at(&ada).await.unwrap(),
        Some(summary.created_at)
    );
    assert_eq!(
        repository.last_colleague_battle_at(&anna).await.unwrap(),
        None
    );
    let xp = repository.xp(&ada, clock.today).await.unwrap();
    assert_eq!((xp.tally.battles, xp.today.battles), (1, 1));
}

#[sqlx::test]
async fn a_trainer_battle_names_the_trainer_and_has_no_defender(pool: PgPool) {
    let repository = repository(&pool);
    let ada = enrolled(&pool, "Ada Lovelace", PIKACHU, true).await;

    let (battle, setup) = repository
        .start_battle(&ada, BattleOpponent::Trainer, &time_zone(), &|setup| {
            fight(setup, 7)
        })
        .await
        .unwrap();

    let trainer = battle::daily_trainer(setup.clock.today);
    assert_eq!(setup.defender, None);
    assert_eq!(battle.summary.kind, BattleKind::Trainer);
    assert_eq!(battle.summary.defender.user_id, None);
    assert_eq!(battle.summary.defender.name, trainer.name);
    assert_eq!(
        repository.battle(battle.summary.id).await.unwrap(),
        Some(battle.clone())
    );
    assert_eq!(
        repository
            .trainer_battle(&ada, setup.clock.today)
            .await
            .unwrap(),
        Some(TrainerBattle {
            id: battle.summary.id,
            winner: battle.summary.winner,
        })
    );
    let yesterday = setup.clock.today.previous_day().unwrap();
    assert_eq!(
        repository.trainer_battle(&ada, yesterday).await.unwrap(),
        None
    );
    // A trainer battle is no colleague battle.
    assert_eq!(
        repository.last_colleague_battle_at(&ada).await.unwrap(),
        None
    );
}

#[sqlx::test]
async fn the_daily_keys_refuse_a_second_battle_that_a_plan_let_through(pool: PgPool) {
    let repository = repository(&pool);
    let ada = enrolled(&pool, "Ada Lovelace", PIKACHU, true).await;
    let anna = enrolled(&pool, "Anna Lind", PIKACHU, true).await;
    let bertil = enrolled(&pool, "Bertil Ek", PIKACHU, true).await;
    let fought_today = Mutex::new(Vec::new());
    let careless = |setup: &BattleSetup| {
        fought_today.lock().unwrap().push(setup.fought_today);
        fight(setup, 1)
    };
    let time_zone = time_zone();
    let start = |opponent| repository.start_battle(&ada, opponent, &time_zone, &careless);

    start(BattleOpponent::Colleague(anna)).await.unwrap();
    let again = start(BattleOpponent::Colleague(anna)).await;
    assert!(matches!(again, Err(BuddyError::BattledToday)), "{again:?}");
    // Another colleague and the trainer are other battles.
    start(BattleOpponent::Colleague(bertil)).await.unwrap();
    start(BattleOpponent::Trainer).await.unwrap();
    let again = start(BattleOpponent::Trainer).await;
    assert!(
        matches!(again, Err(BuddyError::TrainerAlreadyFought)),
        "{again:?}"
    );
    // Anna may battle back the same day.
    repository
        .start_battle(&anna, BattleOpponent::Colleague(ada), &time_zone, &careless)
        .await
        .unwrap();

    assert_eq!(
        *fought_today.lock().unwrap(),
        vec![false, true, false, false, true, false]
    );
    assert_eq!(battle_count(&pool).await, 4);
}

#[sqlx::test]
async fn a_refused_battle_stores_nothing(pool: PgPool) {
    let repository = repository(&pool);
    let ada = enrolled(&pool, "Ada Lovelace", PIKACHU, true).await;
    let anna = enrolled(&pool, "Anna Lind", PIKACHU, true).await;

    let refused = repository
        .start_battle(&ada, BattleOpponent::Colleague(anna), &time_zone(), &|_| {
            Err(BuddyError::OpponentUnavailable)
        })
        .await;

    assert!(
        matches!(refused, Err(BuddyError::OpponentUnavailable)),
        "{refused:?}"
    );
    assert_eq!(battle_count(&pool).await, 0);

    // Someone who never enrolled has nothing to lock and is planned without a profile.
    let stranger = insert_user(&pool, "Stranger").await;
    let refused = repository
        .start_battle(&stranger, BattleOpponent::Trainer, &time_zone(), &|setup| {
            fight(setup, 1)
        })
        .await;
    assert!(
        matches!(refused, Err(BuddyError::NotEnrolled)),
        "{refused:?}"
    );
}

#[sqlx::test]
async fn concurrent_battles_of_one_challenger_are_planned_one_after_the_other(pool: PgPool) {
    let repository = repository(&pool);
    let ada = enrolled(&pool, "Ada Lovelace", PIKACHU, true).await;
    let anna = enrolled(&pool, "Anna Lind", PIKACHU, true).await;
    let bertil = enrolled(&pool, "Bertil Ek", PIKACHU, true).await;
    // At most one colleague battle, which the daily keys alone would not enforce.
    let once = |setup: &BattleSetup| match setup.last_colleague_battle_at {
        Some(last) => Err(BuddyError::Cooldown {
            wait: last + Duration::hours(1) - setup.clock.now,
        }),
        None => fight(setup, 1),
    };

    let time_zone = time_zone();

    let (first, second) = tokio::join!(
        repository.start_battle(&ada, BattleOpponent::Colleague(anna), &time_zone, &once),
        repository.start_battle(&ada, BattleOpponent::Colleague(bertil), &time_zone, &once),
    );

    let refused = [&first, &second]
        .iter()
        .filter(|result| matches!(result, Err(BuddyError::Cooldown { .. })))
        .count();
    assert_eq!(refused, 1, "{first:?} {second:?}");
    assert!(first.is_ok() || second.is_ok());
    assert_eq!(battle_count(&pool).await, 1);
}

#[sqlx::test]
async fn rivals_are_switched_on_colleagues_by_name_with_the_record_both_ways(pool: PgPool) {
    let repository = repository(&pool);
    let today = date!(2026 - 10 - 16);
    let yesterday = date!(2026 - 10 - 15);
    let ada = enrolled(&pool, "Ada Lovelace", PIKACHU, true).await;
    let bertil = enrolled(&pool, "Bertil Ek", PIKACHU, true).await;
    let anna = enrolled(&pool, "Anna Lind", SQUIRTLE, true).await;
    let cecilia = enrolled(&pool, "Cecilia Berg", PIKACHU, false).await;
    insert_user(&pool, "Dora Never").await;
    repository
        .save_logged_days(&anna, &[BuddyXpDay::new(yesterday, 2.0)])
        .await
        .unwrap();

    // Against Anna: a win and a draw Ada started, a loss Anna started.
    Row::new(ada, Some(anna), today).insert(&pool).await;
    Row::new(ada, Some(anna), yesterday)
        .won_by(None)
        .insert(&pool)
        .await;
    Row::new(anna, Some(ada), today).insert(&pool).await;
    // Against Bertil: a win Bertil started; Ada has not battled him today.
    Row::new(bertil, Some(ada), today)
        .won_by(Some("defender"))
        .insert(&pool)
        .await;
    // Neither the trainer nor others' battles count.
    Row::new(ada, None, today).insert(&pool).await;
    Row::new(anna, Some(bertil), today).insert(&pool).await;
    Row::new(cecilia, Some(ada), today).insert(&pool).await;

    let rivals = repository.rivals(&ada, today).await.unwrap();

    assert_eq!(
        rivals
            .iter()
            .map(|rival| (
                rival.competitor.full_name.as_str(),
                rival.record,
                rival.battled_today
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                "Anna Lind",
                BattleTally {
                    wins: 1,
                    losses: 1,
                    draws: 1,
                },
                true
            ),
            (
                "Bertil Ek",
                BattleTally {
                    wins: 1,
                    losses: 0,
                    draws: 0,
                },
                false
            ),
        ]
    );
    let anna_rival = &rivals[0].competitor;
    assert_eq!(anna_rival.user_id(), anna);
    assert_eq!(anna_rival.profile.lead.species_id, SQUIRTLE);
    // 120 XP of hours and two battles she started.
    assert_eq!(anna_rival.total_xp, 160);
    // The next day nobody has been battled yet.
    let tomorrow = repository
        .rivals(&ada, date!(2026 - 10 - 17))
        .await
        .unwrap();
    assert!(tomorrow.iter().all(|rival| !rival.battled_today));
    assert_eq!(repository.rivals(&cecilia, today).await.unwrap().len(), 3);
}

#[sqlx::test]
async fn standings_count_the_battles_started_in_the_local_month(pool: PgPool) {
    let repository = repository(&pool);
    let ada = enrolled(&pool, "Ada Lovelace", PIKACHU, true).await;
    let anna = enrolled(&pool, "Anna Lind", SQUIRTLE, true).await;
    let bertil = enrolled(&pool, "Bertil Ek", PIKACHU, true).await;
    let cecilia = enrolled(&pool, "Cecilia Berg", PIKACHU, false).await;

    Row::new(ada, Some(anna), date!(2026 - 10 - 01))
        .insert(&pool)
        .await;
    Row::new(ada, Some(bertil), date!(2026 - 10 - 01))
        .won_by(Some("defender"))
        .insert(&pool)
        .await;
    Row::new(ada, Some(anna), date!(2026 - 10 - 31))
        .won_by(None)
        .insert(&pool)
        .await;
    Row::new(ada, None, date!(2026 - 10 - 02))
        .insert(&pool)
        .await;
    Row::new(ada, None, date!(2026 - 10 - 03))
        .insert(&pool)
        .await;
    Row::new(ada, None, date!(2026 - 10 - 04))
        .won_by(Some("defender"))
        .insert(&pool)
        .await;
    // Battles of other months stay out.
    Row::new(ada, Some(anna), date!(2026 - 09 - 30))
        .insert(&pool)
        .await;
    Row::new(ada, None, date!(2026 - 11 - 01))
        .insert(&pool)
        .await;
    // A lost trainer battle is still a battle Cecilia started; Bertil only defended.
    Row::new(cecilia, None, date!(2026 - 10 - 20))
        .won_by(Some("defender"))
        .insert(&pool)
        .await;
    Row::new(anna, Some(ada), date!(2026 - 10 - 05))
        .won_by(Some("defender"))
        .insert(&pool)
        .await;

    let standings = repository
        .standings(AiBillingMonth::parse("2026-10").unwrap())
        .await
        .unwrap();

    assert_eq!(
        standings
            .iter()
            .map(|standing| (
                standing.competitor.user_id(),
                standing.colleague,
                standing.trainer_wins
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                ada,
                BattleTally {
                    wins: 1,
                    losses: 1,
                    draws: 1,
                },
                2
            ),
            (
                anna,
                BattleTally {
                    wins: 0,
                    losses: 1,
                    draws: 0,
                },
                0
            ),
            (cecilia, BattleTally::default(), 0),
        ]
    );
    assert_eq!(standings[0].competitor.full_name, "Ada Lovelace");
    // Ada started eight battles in all.
    assert_eq!(standings[0].competitor.total_xp, 160);
    assert!(repository
        .standings(AiBillingMonth::parse("2026-08").unwrap())
        .await
        .unwrap()
        .is_empty());
}

#[sqlx::test]
async fn recent_battles_are_the_newest_of_both_roles_and_the_defender_marks_them_seen(
    pool: PgPool,
) {
    let repository = repository(&pool);
    let day = date!(2026 - 10 - 16);
    let ada = enrolled(&pool, "Ada Lovelace", PIKACHU, true).await;
    let anna = enrolled(&pool, "Anna Lind", PIKACHU, true).await;
    let bertil = enrolled(&pool, "Bertil Ek", PIKACHU, true).await;
    let started = Row::new(ada, Some(anna), day)
        .at(datetime!(2026-10-16 09:00 UTC))
        .insert(&pool)
        .await;
    let defended = Row::new(bertil, Some(ada), day)
        .at(datetime!(2026-10-16 11:00 UTC))
        .insert(&pool)
        .await;
    let trainer = Row::new(ada, None, day)
        .at(datetime!(2026-10-16 10:00 UTC))
        .insert(&pool)
        .await;
    Row::new(ada, None, date!(2026 - 09 - 01))
        .at(datetime!(2026-09-01 10:00 UTC))
        .insert(&pool)
        .await;
    Row::new(anna, Some(bertil), day).insert(&pool).await;
    let since = datetime!(2026-09-16 10:00 UTC);

    let battles = repository.recent_battles(&ada, since, 50).await.unwrap();

    assert_eq!(
        battles
            .iter()
            .map(|battle| (battle.id, battle.role_of(ada)))
            .collect::<Vec<_>>(),
        vec![
            (defended, Some(BattleSide::Defender)),
            (trainer, Some(BattleSide::Challenger)),
            (started, Some(BattleSide::Challenger)),
        ]
    );
    assert_eq!(battles[0].challenger.name, "Bertil Ek");
    assert_eq!(battles[1].defender.name, "Ace Trainer Kim");
    assert_eq!(
        repository
            .recent_battles(&ada, since, 1)
            .await
            .unwrap()
            .iter()
            .map(|battle| battle.id)
            .collect::<Vec<_>>(),
        vec![defended]
    );

    // Only the defender marks a battle seen, and only the first time counts.
    repository.mark_battle_seen(started, &ada).await.unwrap();
    repository.mark_battle_seen(defended, &ada).await.unwrap();
    let seen_at = repository
        .battle(defended)
        .await
        .unwrap()
        .unwrap()
        .summary
        .defender_seen_at
        .expect("the defender saw it");
    repository.mark_battle_seen(defended, &ada).await.unwrap();
    assert_eq!(
        repository
            .battle(defended)
            .await
            .unwrap()
            .unwrap()
            .summary
            .defender_seen_at,
        Some(seen_at)
    );
    assert_eq!(
        repository
            .battle(started)
            .await
            .unwrap()
            .unwrap()
            .summary
            .defender_seen_at,
        None
    );
}
