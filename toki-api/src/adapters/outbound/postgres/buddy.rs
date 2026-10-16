use async_trait::async_trait;
use sqlx::{Executor, Postgres};
use time::{Date, OffsetDateTime};

use crate::{
    db::DbPool,
    domain::{
        models::{
            AiBillingMonth, AiUsageTimeZone, BattleId, BattleOpponent, BattleSetup, Buddy,
            BuddyBattle, BuddyBattleSummary, BuddyClock, BuddyDay, BuddyProfile,
            BuddyProfileUpdate, BuddyRival, BuddySlot, BuddyStanding, BuddyXpDay, BuddyXpSummary,
            BuddyXpTally, NewBuddyProfile, SpeciesId, TrainerBattle, UserId,
        },
        ports::outbound::{BattlePlan, BuddyRepository},
        BuddyError,
    },
};

mod battles;

const PROFILE_USER_CONSTRAINT: &str = "buddy_profiles_user_id_fkey";
const CHALLENGER_CONSTRAINT: &str = "buddy_battles_challenger_id_fkey";
const DEFENDER_CONSTRAINT: &str = "buddy_battles_defender_id_fkey";
const COLLEAGUE_DAILY_CONSTRAINT: &str = "buddy_battles_colleague_daily_key";
const TRAINER_DAILY_CONSTRAINT: &str = "buddy_battles_trainer_daily_key";

pub struct PostgresBuddyRepository {
    pool: DbPool,
}

impl PostgresBuddyRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

/// A row of `buddy_profiles` joined with one of its `buddies`, if any.
struct StoredBuddy {
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
}

impl StoredBuddy {
    fn buddy(&self) -> Option<Buddy> {
        Some(Buddy {
            slot: BuddySlot::from_number(self.slot?)?,
            species_id: SpeciesId::new(self.species_id?),
            visible: self.visible?,
            xp_base: self.xp_base?,
        })
    }
}

/// The profile in `rows`, one per buddy, or `None` without rows.
fn profile_from_rows(rows: Vec<StoredBuddy>) -> Result<Option<BuddyProfile>, BuddyError> {
    let Some(first) = rows.first() else {
        return Ok(None);
    };
    let user_id = first.user_id;
    let corrupt = || BuddyError::Storage(format!("stored buddies of user {user_id} are invalid"));
    let buddies = rows
        .iter()
        .map(|row| row.buddy().ok_or_else(corrupt))
        .collect::<Result<Vec<_>, _>>()?;
    let (lead, partner) = match buddies.as_slice() {
        [lead] if lead.slot == BuddySlot::Lead => (*lead, None),
        [lead, partner] if lead.slot == BuddySlot::Lead && partner.slot == BuddySlot::Partner => {
            (*lead, Some(*partner))
        }
        _ => return Err(corrupt()),
    };

    Ok(Some(BuddyProfile {
        user_id: UserId::new(user_id),
        enabled: first.enabled,
        battle_notifications: first.battle_notifications,
        xp_since: first.xp_since,
        hours_synced_at: first.hours_synced_at,
        hours_sync_attempted_at: first.hours_sync_attempted_at,
        lead,
        partner,
    }))
}

/// The user's profile, read with `executor`, such as inside a transaction.
async fn load_profile<'e, E>(
    executor: E,
    user_id: &UserId,
) -> Result<Option<BuddyProfile>, BuddyError>
where
    E: Executor<'e, Database = Postgres>,
{
    let rows = sqlx::query_as!(
        StoredBuddy,
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
            buddy.xp_base AS "xp_base?"
        FROM buddy_profiles AS profile
        LEFT JOIN buddies AS buddy ON buddy.user_id = profile.user_id
        WHERE profile.user_id = $1
        ORDER BY buddy.slot
        "#,
        user_id.as_i32(),
    )
    .fetch_all(executor)
    .await
    .map_err(storage_error)?;

    profile_from_rows(rows)
}

/// The clock as `executor` sees it: inside a transaction, `now()` is when it began.
async fn read_clock<'e, E>(
    executor: E,
    time_zone: &AiUsageTimeZone,
) -> Result<BuddyClock, BuddyError>
where
    E: Executor<'e, Database = Postgres>,
{
    let row = sqlx::query!(
        r#"SELECT now() AS "now!", (now() AT TIME ZONE $1)::date AS "today!""#,
        time_zone.as_str(),
    )
    .fetch_one(executor)
    .await
    .map_err(storage_error)?;

    Ok(BuddyClock {
        now: row.now,
        today: row.today,
    })
}

#[async_trait]
impl BuddyRepository for PostgresBuddyRepository {
    async fn clock(&self, time_zone: &AiUsageTimeZone) -> Result<BuddyClock, BuddyError> {
        read_clock(&self.pool, time_zone).await
    }

    async fn profile(&self, user_id: &UserId) -> Result<Option<BuddyProfile>, BuddyError> {
        load_profile(&self.pool, user_id).await
    }

    async fn enrol(
        &self,
        user_id: &UserId,
        profile: &NewBuddyProfile,
    ) -> Result<Option<BuddyProfile>, BuddyError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;

        let created = sqlx::query!(
            r#"
            INSERT INTO buddy_profiles (user_id, enabled, battle_notifications, xp_since)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id) DO NOTHING
            "#,
            user_id.as_i32(),
            profile.enabled,
            profile.battle_notifications,
            profile.xp_since,
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(write_error)?
        .rows_affected()
            == 1;
        if !created {
            return Ok(None);
        }

        sqlx::query!(
            r#"
            INSERT INTO buddies (user_id, slot, species_id)
            VALUES ($1, $2, $3)
            "#,
            user_id.as_i32(),
            BuddySlot::Lead.number(),
            profile.lead_species_id.as_i16(),
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(write_error)?;

        let profile = load_profile(&mut transaction.executor(), user_id).await?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(profile)
    }

    async fn update_profile(
        &self,
        user_id: &UserId,
        update: &BuddyProfileUpdate,
        new_partner_xp_base: i64,
    ) -> Result<BuddyProfile, BuddyError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;

        // Updating the profile row first also locks it, so concurrent updates of one
        // user's buddies apply one after the other.
        let updated = sqlx::query!(
            r#"
            UPDATE buddy_profiles
            SET enabled = COALESCE($2, enabled),
                battle_notifications = COALESCE($3, battle_notifications),
                updated_at = now()
            WHERE user_id = $1
            "#,
            user_id.as_i32(),
            update.enabled,
            update.battle_notifications,
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(write_error)?
        .rows_affected();
        if updated == 0 {
            return Err(BuddyError::NotEnrolled);
        }

        if let Some(species_id) = update.lead_species_id {
            sqlx::query!(
                r#"
                UPDATE buddies
                SET species_id = $3,
                    updated_at = now()
                WHERE user_id = $1
                  AND slot = $2
                "#,
                user_id.as_i32(),
                BuddySlot::Lead.number(),
                species_id.as_i16(),
            )
            .execute(&mut transaction.executor())
            .await
            .map_err(write_error)?;
        }

        // A partner keeps the XP base it was created with; later changes only change
        // its species.
        if let Some(species_id) = update.partner_species_id {
            sqlx::query!(
                r#"
                INSERT INTO buddies (user_id, slot, species_id, xp_base)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (user_id, slot) DO UPDATE
                SET species_id = EXCLUDED.species_id,
                    updated_at = now()
                "#,
                user_id.as_i32(),
                BuddySlot::Partner.number(),
                species_id.as_i16(),
                new_partner_xp_base,
            )
            .execute(&mut transaction.executor())
            .await
            .map_err(write_error)?;
        }

        if let Some(visible) = update.partner_visible {
            let updated = sqlx::query!(
                r#"
                UPDATE buddies
                SET visible = $3,
                    updated_at = now()
                WHERE user_id = $1
                  AND slot = $2
                "#,
                user_id.as_i32(),
                BuddySlot::Partner.number(),
                visible,
            )
            .execute(&mut transaction.executor())
            .await
            .map_err(write_error)?
            .rows_affected();
            if updated == 0 {
                return Err(BuddyError::NoPartner);
            }
        }

        let profile = load_profile(&mut transaction.executor(), user_id)
            .await?
            .ok_or(BuddyError::NotEnrolled)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(profile)
    }

    async fn xp(&self, user_id: &UserId, today: Date) -> Result<BuddyXpSummary, BuddyError> {
        let row = sqlx::query!(
            r#"
            SELECT
                (
                    SELECT COALESCE(sum(xp), 0)::int8
                    FROM buddy_xp_days
                    WHERE user_id = $1
                ) AS "hours_xp!",
                (
                    SELECT count(*)
                    FROM buddy_battles
                    WHERE challenger_id = $1
                ) AS "battles!",
                today.logged_hours AS "today_logged_hours?",
                today.xp AS "today_hours_xp?",
                (
                    SELECT count(*)
                    FROM buddy_battles
                    WHERE challenger_id = $1
                      AND battle_day = $2
                ) AS "today_battles!"
            FROM (SELECT 1) AS one
            LEFT JOIN buddy_xp_days AS today ON today.user_id = $1 AND today.day = $2
            "#,
            user_id.as_i32(),
            today,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(BuddyXpSummary {
            tally: BuddyXpTally {
                hours_xp: row.hours_xp,
                battles: row.battles,
            },
            today: BuddyDay {
                date: today,
                logged_hours: row.today_logged_hours.unwrap_or(0.0),
                hours_xp: row.today_hours_xp.map_or(0, i64::from),
                battles: row.today_battles,
            },
        })
    }

    async fn claim_hours_sync(
        &self,
        user_id: &UserId,
        last_attempted_at: Option<OffsetDateTime>,
    ) -> Result<bool, BuddyError> {
        let claimed = sqlx::query_scalar!(
            r#"
            UPDATE buddy_profiles
            SET hours_sync_attempted_at = now()
            WHERE user_id = $1
              AND hours_sync_attempted_at IS NOT DISTINCT FROM $2
            RETURNING user_id
            "#,
            user_id.as_i32(),
            last_attempted_at,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(claimed.is_some())
    }

    async fn save_logged_days(
        &self,
        user_id: &UserId,
        days: &[BuddyXpDay],
    ) -> Result<OffsetDateTime, BuddyError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;

        sqlx::query!(
            r#"
            INSERT INTO buddy_xp_days (user_id, day, logged_hours, xp)
            SELECT $1, day.day, day.logged_hours, day.xp
            FROM UNNEST($2::date[], $3::float8[], $4::int4[]) AS day (day, logged_hours, xp)
            ON CONFLICT (user_id, day) DO UPDATE
            SET logged_hours = EXCLUDED.logged_hours,
                xp = EXCLUDED.xp,
                updated_at = now()
            WHERE (buddy_xp_days.logged_hours, buddy_xp_days.xp)
                IS DISTINCT FROM (EXCLUDED.logged_hours, EXCLUDED.xp)
            "#,
            user_id.as_i32(),
            &days.iter().map(|day| day.date).collect::<Vec<_>>(),
            &days.iter().map(|day| day.logged_hours).collect::<Vec<_>>(),
            &days.iter().map(|day| day.xp).collect::<Vec<_>>(),
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(write_error)?;

        let synced_at = sqlx::query_scalar!(
            r#"
            UPDATE buddy_profiles
            SET hours_synced_at = now()
            WHERE user_id = $1
            RETURNING hours_synced_at AS "hours_synced_at!"
            "#,
            user_id.as_i32(),
        )
        .fetch_optional(&mut transaction.executor())
        .await
        .map_err(storage_error)?
        .ok_or(BuddyError::NotEnrolled)?;

        transaction.commit().await.map_err(storage_error)?;
        Ok(synced_at)
    }

    async fn rivals(&self, user_id: &UserId, today: Date) -> Result<Vec<BuddyRival>, BuddyError> {
        battles::rivals(&self.pool, user_id, today).await
    }

    async fn last_colleague_battle_at(
        &self,
        user_id: &UserId,
    ) -> Result<Option<OffsetDateTime>, BuddyError> {
        battles::last_colleague_battle_at(&self.pool, user_id).await
    }

    async fn start_battle(
        &self,
        challenger_id: &UserId,
        opponent: BattleOpponent,
        time_zone: &AiUsageTimeZone,
        plan: &BattlePlan<'_>,
    ) -> Result<(BuddyBattle, BattleSetup), BuddyError> {
        battles::start_battle(&self.pool, challenger_id, opponent, time_zone, plan).await
    }

    async fn trainer_battle(
        &self,
        user_id: &UserId,
        day: Date,
    ) -> Result<Option<TrainerBattle>, BuddyError> {
        battles::trainer_battle(&self.pool, user_id, day).await
    }

    async fn recent_battles(
        &self,
        user_id: &UserId,
        since: OffsetDateTime,
        limit: i64,
    ) -> Result<Vec<BuddyBattleSummary>, BuddyError> {
        battles::recent_battles(&self.pool, user_id, since, limit).await
    }

    async fn battle(&self, battle_id: BattleId) -> Result<Option<BuddyBattle>, BuddyError> {
        battles::load_battle(&self.pool, battle_id).await
    }

    async fn mark_battle_seen(
        &self,
        battle_id: BattleId,
        defender_id: &UserId,
    ) -> Result<(), BuddyError> {
        battles::mark_battle_seen(&self.pool, battle_id, defender_id).await
    }

    async fn standings(&self, month: AiBillingMonth) -> Result<Vec<BuddyStanding>, BuddyError> {
        battles::standings(&self.pool, month).await
    }
}

/// Maps the constraints a write can break to domain errors. The daily unique keys back
/// up the rules a battle is planned with, for writes that race past them.
fn write_error(error: sqlx::Error) -> BuddyError {
    match error
        .as_database_error()
        .and_then(|error| error.constraint())
    {
        Some(PROFILE_USER_CONSTRAINT | CHALLENGER_CONSTRAINT) => BuddyError::UserNotFound,
        Some(DEFENDER_CONSTRAINT) => BuddyError::OpponentUnavailable,
        Some(COLLEAGUE_DAILY_CONSTRAINT) => BuddyError::BattledToday,
        Some(TRAINER_DAILY_CONSTRAINT) => BuddyError::TrainerAlreadyFought,
        _ => storage_error(error),
    }
}

fn storage_error(error: sqlx::Error) -> BuddyError {
    BuddyError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;
    use time::macros::date;

    use super::*;

    const PIKACHU: SpeciesId = SpeciesId::new(25);
    const BULBASAUR: SpeciesId = SpeciesId::new(1);
    const CHARMANDER: SpeciesId = SpeciesId::new(4);

    fn db(pool: &PgPool) -> DbPool {
        sqlx_tracing::PoolBuilder::from(pool.clone()).build()
    }

    fn repository(pool: &PgPool) -> PostgresBuddyRepository {
        PostgresBuddyRepository::new(db(pool))
    }

    async fn insert_user(pool: &PgPool, email: &str) -> UserId {
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO users (email, full_name, picture, access_token)
             VALUES ($1, 'Test User', '', '')
             RETURNING id",
        )
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap();
        UserId::new(id)
    }

    fn new_profile(lead_species_id: SpeciesId) -> NewBuddyProfile {
        NewBuddyProfile {
            enabled: true,
            battle_notifications: true,
            lead_species_id,
            xp_since: date!(2026 - 10 - 01),
        }
    }

    async fn enrolled_user(pool: &PgPool, email: &str) -> UserId {
        let user = insert_user(pool, email).await;
        repository(pool)
            .enrol(&user, &new_profile(PIKACHU))
            .await
            .unwrap()
            .unwrap();
        user
    }

    /// A battle started by `challenger`, against `defender` or the daily trainer.
    async fn insert_battle(pool: &PgPool, challenger: UserId, defender: Option<UserId>, day: Date) {
        sqlx::query(
            "INSERT INTO buddy_battles (
                 kind, challenger_id, defender_id, battle_day, seed, engine_version,
                 challenger_team, defender_team, events, trainer_name
             )
             VALUES (
                 CASE WHEN $2::int4 IS NULL THEN 'trainer' ELSE 'colleague' END,
                 $1, $2, $3, 1, 1, '[]', '[]', '[]',
                 CASE WHEN $2::int4 IS NULL THEN 'Ace Trainer Kim' END
             )",
        )
        .bind(challenger.as_i32())
        .bind(defender.map(|user| user.as_i32()))
        .bind(day)
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test]
    async fn a_user_enrols_once_with_a_lead(pool: PgPool) {
        let repository = repository(&pool);
        let user = insert_user(&pool, "dev@example.com").await;

        let profile = repository
            .enrol(&user, &new_profile(PIKACHU))
            .await
            .unwrap();
        let expected = BuddyProfile {
            user_id: user,
            enabled: true,
            battle_notifications: true,
            xp_since: date!(2026 - 10 - 01),
            hours_synced_at: None,
            hours_sync_attempted_at: None,
            lead: Buddy {
                slot: BuddySlot::Lead,
                species_id: PIKACHU,
                visible: true,
                xp_base: 0,
            },
            partner: None,
        };
        assert_eq!(profile.as_ref(), Some(&expected));

        let again = NewBuddyProfile {
            enabled: false,
            ..new_profile(BULBASAUR)
        };
        assert_eq!(repository.enrol(&user, &again).await.unwrap(), None);
        assert_eq!(repository.profile(&user).await.unwrap(), Some(expected));

        let unknown = repository
            .enrol(&UserId::new(user.as_i32() + 1), &new_profile(PIKACHU))
            .await;
        assert!(
            matches!(unknown, Err(BuddyError::UserNotFound)),
            "{unknown:?}"
        );
        assert_eq!(
            repository
                .profile(&UserId::new(user.as_i32() + 1))
                .await
                .unwrap(),
            None
        );
    }

    #[sqlx::test]
    async fn a_partner_keeps_the_xp_base_it_was_created_with(pool: PgPool) {
        let repository = repository(&pool);
        let user = enrolled_user(&pool, "dev@example.com").await;

        let profile = repository
            .update_profile(
                &user,
                &BuddyProfileUpdate {
                    battle_notifications: Some(false),
                    lead_species_id: Some(CHARMANDER),
                    partner_species_id: Some(BULBASAUR),
                    ..BuddyProfileUpdate::default()
                },
                540,
            )
            .await
            .unwrap();
        assert!(profile.enabled);
        assert!(!profile.battle_notifications);
        assert_eq!(profile.lead.species_id, CHARMANDER);
        assert_eq!(profile.lead.xp_base, 0);
        assert_eq!(
            profile.partner,
            Some(Buddy {
                slot: BuddySlot::Partner,
                species_id: BULBASAUR,
                visible: true,
                xp_base: 540,
            })
        );

        let profile = repository
            .update_profile(
                &user,
                &BuddyProfileUpdate {
                    partner_species_id: Some(PIKACHU),
                    partner_visible: Some(false),
                    ..BuddyProfileUpdate::default()
                },
                9_999,
            )
            .await
            .unwrap();
        assert_eq!(
            profile.partner,
            Some(Buddy {
                slot: BuddySlot::Partner,
                species_id: PIKACHU,
                visible: false,
                xp_base: 540,
            })
        );
        assert_eq!(repository.profile(&user).await.unwrap(), Some(profile));
    }

    #[sqlx::test]
    async fn an_update_that_breaks_a_rule_changes_nothing(pool: PgPool) {
        let repository = repository(&pool);
        let user = enrolled_user(&pool, "dev@example.com").await;
        let before = repository.profile(&user).await.unwrap();

        let hidden = repository
            .update_profile(
                &user,
                &BuddyProfileUpdate {
                    enabled: Some(false),
                    lead_species_id: Some(CHARMANDER),
                    partner_visible: Some(false),
                    ..BuddyProfileUpdate::default()
                },
                0,
            )
            .await;
        assert!(matches!(hidden, Err(BuddyError::NoPartner)), "{hidden:?}");
        assert_eq!(repository.profile(&user).await.unwrap(), before);

        let stranger = insert_user(&pool, "stranger@example.com").await;
        let not_enrolled = repository
            .update_profile(
                &stranger,
                &BuddyProfileUpdate {
                    partner_species_id: Some(BULBASAUR),
                    ..BuddyProfileUpdate::default()
                },
                0,
            )
            .await;
        assert!(
            matches!(not_enrolled, Err(BuddyError::NotEnrolled)),
            "{not_enrolled:?}"
        );
        let stranger_buddies: i64 =
            sqlx::query_scalar("SELECT count(*) FROM buddies WHERE user_id = $1")
                .bind(stranger.as_i32())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stranger_buddies, 0);
    }

    #[sqlx::test]
    async fn xp_adds_up_logged_days_and_started_battles(pool: PgPool) {
        let repository = repository(&pool);
        let user = enrolled_user(&pool, "dev@example.com").await;
        let colleague = enrolled_user(&pool, "colleague@example.com").await;
        let today = date!(2026 - 10 - 16);
        let yesterday = date!(2026 - 10 - 15);

        let synced_at = repository
            .save_logged_days(
                &user,
                &[
                    BuddyXpDay::new(yesterday, 11.0),
                    BuddyXpDay::new(today, 6.5),
                ],
            )
            .await
            .unwrap();
        // Hours edited in time tracking replace the stored ones, also with none.
        repository
            .save_logged_days(
                &user,
                &[
                    BuddyXpDay::new(yesterday, 0.0),
                    BuddyXpDay::new(today, 7.25),
                ],
            )
            .await
            .unwrap();
        repository
            .save_logged_days(&colleague, &[BuddyXpDay::new(today, 8.0)])
            .await
            .unwrap();
        insert_battle(&pool, user, Some(colleague), today).await;
        insert_battle(&pool, user, None, today).await;
        insert_battle(&pool, user, None, yesterday).await;
        // Defending gives no XP.
        insert_battle(&pool, colleague, Some(user), today).await;

        assert_eq!(
            repository.xp(&user, today).await.unwrap(),
            BuddyXpSummary {
                tally: BuddyXpTally {
                    hours_xp: 435,
                    battles: 3,
                },
                today: BuddyDay {
                    date: today,
                    logged_hours: 7.25,
                    hours_xp: 435,
                    battles: 2,
                },
            }
        );
        assert_eq!(
            repository
                .xp(&user, date!(2026 - 10 - 17))
                .await
                .unwrap()
                .today,
            BuddyDay {
                date: date!(2026 - 10 - 17),
                logged_hours: 0.0,
                hours_xp: 0,
                battles: 0,
            }
        );
        let profile = repository.profile(&user).await.unwrap().unwrap();
        assert!(profile.hours_synced_at.is_some_and(|at| at >= synced_at));
    }

    #[sqlx::test]
    async fn a_read_of_logged_hours_is_claimed_once_per_attempt_seen(pool: PgPool) {
        let repository = repository(&pool);
        let user = enrolled_user(&pool, "dev@example.com").await;
        let stranger = insert_user(&pool, "stranger@example.com").await;

        let (first, second) = tokio::join!(
            repository.claim_hours_sync(&user, None),
            repository.claim_hours_sync(&user, None),
        );
        let claims = [first.unwrap(), second.unwrap()];
        assert_eq!(
            claims.iter().filter(|claimed| **claimed).count(),
            1,
            "{claims:?}"
        );
        let attempted_at = repository
            .profile(&user)
            .await
            .unwrap()
            .unwrap()
            .hours_sync_attempted_at;
        assert!(attempted_at.is_some());
        // A request that saw an earlier attempt has lost the claim.
        assert!(!repository.claim_hours_sync(&user, None).await.unwrap());
        // Saving the hours that were read leaves the attempt as it was.
        repository.save_logged_days(&user, &[]).await.unwrap();
        assert!(repository
            .claim_hours_sync(&user, attempted_at)
            .await
            .unwrap());
        assert!(!repository.claim_hours_sync(&stranger, None).await.unwrap());
    }

    #[sqlx::test]
    async fn hours_are_saved_only_for_enrolled_users(pool: PgPool) {
        let repository = repository(&pool);
        let user = insert_user(&pool, "dev@example.com").await;
        let today = date!(2026 - 10 - 16);

        let saved = repository
            .save_logged_days(&user, &[BuddyXpDay::new(today, 4.0)])
            .await;

        assert!(matches!(saved, Err(BuddyError::NotEnrolled)), "{saved:?}");
        assert_eq!(repository.xp(&user, today).await.unwrap().tally.hours_xp, 0);
    }
}
