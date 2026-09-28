use async_trait::async_trait;
use time::Date;

use super::ai_usage_reads::local_today;
use crate::{
    db::DbPool,
    domain::{
        models::{
            AiCurrency, AiMonthlyCost, AiPlanMismatchDay, AiProvider, AiSubscription,
            AiSubscriptionId, AiSubscriptionPeriod, AiSubscriptionPlan, AiSubscriptionScope,
            AiSubscriptionTerms, AiUsageDateRange, AiUsageTimeZone, UserId, UNPAID_PLAN_HINTS,
        },
        ports::outbound::AiSubscriptionRepository,
        AiSubscriptionError,
    },
};

const NO_OVERLAP_CONSTRAINT: &str = "ai_subscriptions_no_overlap";
const USER_CONSTRAINT: &str = "ai_subscriptions_user_id_fkey";

pub struct PostgresAiSubscriptionRepository {
    pool: DbPool,
}

impl PostgresAiSubscriptionRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

/// A row of `ai_subscriptions`, with the fee in hundredths of its currency unit.
struct StoredSubscription {
    id: i32,
    user_id: i32,
    provider: String,
    plan: String,
    monthly_cost_hundredths: i64,
    currency: String,
    valid_from: Date,
    valid_to: Option<Date>,
}

impl TryFrom<StoredSubscription> for AiSubscription {
    type Error = AiSubscriptionError;

    fn try_from(row: StoredSubscription) -> Result<Self, Self::Error> {
        let id = row.id;
        let corrupt =
            || AiSubscriptionError::Storage(format!("stored ai subscription {id} is invalid"));
        let terms = AiSubscriptionTerms {
            provider: AiProvider::parse(&row.provider).ok_or_else(corrupt)?,
            plan: AiSubscriptionPlan::parse(&row.plan).map_err(|_| corrupt())?,
            monthly_cost: AiMonthlyCost::from_hundredths(row.monthly_cost_hundredths)
                .ok_or_else(corrupt)?,
            currency: AiCurrency::parse(&row.currency).map_err(|_| corrupt())?,
            period: AiSubscriptionPeriod::new(row.valid_from, row.valid_to)
                .map_err(|_| corrupt())?,
        };

        Ok(Self {
            id: AiSubscriptionId::new(id),
            user_id: UserId::new(row.user_id),
            terms,
        })
    }
}

#[async_trait]
impl AiSubscriptionRepository for PostgresAiSubscriptionRepository {
    async fn list(
        &self,
        scope: AiSubscriptionScope,
    ) -> Result<Vec<AiSubscription>, AiSubscriptionError> {
        let rows = sqlx::query_as!(
            StoredSubscription,
            r#"
            SELECT
                id,
                user_id,
                provider,
                plan,
                (monthly_cost * 100)::int8 AS "monthly_cost_hundredths!",
                currency,
                valid_from,
                valid_to
            FROM ai_subscriptions
            WHERE ($1::int4 IS NULL OR user_id = $1)
            ORDER BY user_id, provider, valid_from DESC
            "#,
            scope.user_id().map(|user_id| user_id.as_i32()),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        rows.into_iter().map(AiSubscription::try_from).collect()
    }

    async fn insert(
        &self,
        user_id: &UserId,
        terms: &AiSubscriptionTerms,
    ) -> Result<AiSubscription, AiSubscriptionError> {
        // The exclusion constraint rejects overlapping periods, also between
        // concurrent requests.
        sqlx::query_as!(
            StoredSubscription,
            r#"
            INSERT INTO ai_subscriptions (
                user_id, provider, plan, monthly_cost, currency, valid_from, valid_to
            )
            VALUES ($1, $2, $3, $4::int8 / 100.0, $5, $6, $7)
            RETURNING
                id,
                user_id,
                provider,
                plan,
                (monthly_cost * 100)::int8 AS "monthly_cost_hundredths!",
                currency,
                valid_from,
                valid_to
            "#,
            user_id.as_i32(),
            terms.provider.as_str(),
            terms.plan.as_str(),
            terms.monthly_cost.hundredths(),
            terms.currency.as_str(),
            terms.period.valid_from(),
            terms.period.valid_to(),
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| write_error(error, terms))?
        .try_into()
    }

    async fn update(
        &self,
        scope: AiSubscriptionScope,
        id: AiSubscriptionId,
        terms: &AiSubscriptionTerms,
    ) -> Result<AiSubscription, AiSubscriptionError> {
        sqlx::query_as!(
            StoredSubscription,
            r#"
            UPDATE ai_subscriptions
            SET provider = $3,
                plan = $4,
                monthly_cost = $5::int8 / 100.0,
                currency = $6,
                valid_from = $7,
                valid_to = $8,
                updated_at = now()
            WHERE id = $1
              AND ($2::int4 IS NULL OR user_id = $2)
            RETURNING
                id,
                user_id,
                provider,
                plan,
                (monthly_cost * 100)::int8 AS "monthly_cost_hundredths!",
                currency,
                valid_from,
                valid_to
            "#,
            id.as_i32(),
            scope.user_id().map(|user_id| user_id.as_i32()),
            terms.provider.as_str(),
            terms.plan.as_str(),
            terms.monthly_cost.hundredths(),
            terms.currency.as_str(),
            terms.period.valid_from(),
            terms.period.valid_to(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| write_error(error, terms))?
        .ok_or(AiSubscriptionError::NotFound)?
        .try_into()
    }

    async fn delete(
        &self,
        scope: AiSubscriptionScope,
        id: AiSubscriptionId,
    ) -> Result<(), AiSubscriptionError> {
        let result = sqlx::query!(
            r#"
            DELETE FROM ai_subscriptions
            WHERE id = $1
              AND ($2::int4 IS NULL OR user_id = $2)
            "#,
            id.as_i32(),
            scope.user_id().map(|user_id| user_id.as_i32()),
        )
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        if result.rows_affected() == 0 {
            return Err(AiSubscriptionError::NotFound);
        }

        Ok(())
    }

    async fn today(&self, time_zone: &AiUsageTimeZone) -> Result<Date, AiSubscriptionError> {
        local_today(&self.pool, time_zone)
            .await
            .map_err(storage_error)
    }

    async fn plan_mismatch_days(
        &self,
        scope: AiSubscriptionScope,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
    ) -> Result<Vec<AiPlanMismatchDay>, AiSubscriptionError> {
        // Hints are grouped by the local date their hour starts on, converting in
        // the zone so days follow daylight-saving changes. The range bounds stay
        // comparable with the (user_id, hour_start) index, and the covering check
        // matches the no-overlap constraint's index expression.
        let rows = sqlx::query!(
            r#"
            WITH hint_days AS (
                SELECT hints.user_id, hints.provider, local.day,
                    array_agg(DISTINCT hints.plan ORDER BY hints.plan) AS plans
                FROM ai_usage_provider_hints AS hints
                CROSS JOIN LATERAL (
                    SELECT (hints.hour_start AT TIME ZONE $4)::date AS day
                ) AS local
                WHERE ($1::int4 IS NULL OR hints.user_id = $1)
                  AND hints.hour_start >= ($2::date::timestamp AT TIME ZONE $4)
                  AND hints.hour_start < ($3::date::timestamp AT TIME ZONE $4)
                  AND lower(btrim(hints.plan)) <> ALL($5::text[])
                GROUP BY hints.user_id, hints.provider, local.day
            )
            SELECT
                hint_days.user_id AS "user_id!",
                hint_days.provider AS "provider!",
                hint_days.day AS "day!",
                hint_days.plans AS "plans!",
                (
                    SELECT min(later.valid_from)
                    FROM ai_subscriptions AS later
                    WHERE later.user_id = hint_days.user_id
                      AND later.provider = hint_days.provider
                      AND later.valid_from > hint_days.day
                ) AS next_subscription_from
            FROM hint_days
            WHERE NOT EXISTS (
                SELECT 1
                FROM ai_subscriptions AS covering
                WHERE covering.user_id = hint_days.user_id
                  AND covering.provider = hint_days.provider
                  AND daterange(covering.valid_from, covering.valid_to, '[]') @> hint_days.day
            )
            ORDER BY hint_days.user_id, hint_days.provider, hint_days.day
            "#,
            scope.user_id().map(|user_id| user_id.as_i32()),
            dates.start(),
            dates.end(),
            time_zone.as_str(),
            UNPAID_PLAN_HINTS as &[&str],
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        rows.into_iter()
            .map(|row| {
                let provider = AiProvider::parse(&row.provider).ok_or_else(|| {
                    AiSubscriptionError::Storage(format!(
                        "stored plan hint has an unknown provider {:?}",
                        row.provider
                    ))
                })?;

                Ok(AiPlanMismatchDay {
                    user_id: UserId::new(row.user_id),
                    provider,
                    day: row.day,
                    plans: row.plans,
                    next_subscription_from: row.next_subscription_from,
                })
            })
            .collect()
    }
}

/// Maps the constraints a write can break to domain errors.
fn write_error(error: sqlx::Error, terms: &AiSubscriptionTerms) -> AiSubscriptionError {
    match error
        .as_database_error()
        .and_then(|error| error.constraint())
    {
        Some(NO_OVERLAP_CONSTRAINT) => AiSubscriptionError::Overlap(terms.provider),
        Some(USER_CONSTRAINT) => AiSubscriptionError::UserNotFound,
        _ => storage_error(error),
    }
}

fn storage_error(error: sqlx::Error) -> AiSubscriptionError {
    AiSubscriptionError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sqlx::PgPool;
    use time::{
        macros::{date, datetime},
        OffsetDateTime,
    };

    use crate::{
        adapters::outbound::postgres::PostgresAiUsageRepository,
        domain::{
            models::{
                resolve_billing_mode, AiBillingMode, AiCoverageStatus, AiMachine, AiMachineId,
                AiPricing, AiPricingStatus, AiProviderCoverage, AiProviderHint, AiUsageUpload,
                AiUsageWindow,
            },
            ports::{inbound::AiSubscriptionService, outbound::AiUsageRepository},
            services::AiSubscriptionServiceImpl,
        },
    };

    use super::*;

    fn db(pool: &PgPool) -> DbPool {
        sqlx_tracing::PoolBuilder::from(pool.clone()).build()
    }

    fn repository(pool: &PgPool) -> PostgresAiSubscriptionRepository {
        PostgresAiSubscriptionRepository::new(db(pool))
    }

    fn stockholm() -> AiUsageTimeZone {
        AiUsageTimeZone::parse("Europe/Stockholm").unwrap()
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

    fn terms(
        provider: AiProvider,
        valid_from: Date,
        valid_to: Option<Date>,
    ) -> AiSubscriptionTerms {
        AiSubscriptionTerms {
            provider,
            plan: AiSubscriptionPlan::parse("ChatGPT Pro").unwrap(),
            monthly_cost: AiMonthlyCost::parse("2290").unwrap(),
            currency: AiCurrency::parse("SEK").unwrap(),
            period: AiSubscriptionPeriod::new(valid_from, valid_to).unwrap(),
        }
    }

    fn is_overlap(result: Result<AiSubscription, AiSubscriptionError>) -> bool {
        matches!(result, Err(AiSubscriptionError::Overlap(_)))
    }

    #[sqlx::test]
    async fn closed_periods_of_one_provider_may_not_share_a_day(pool: PgPool) {
        let repository = repository(&pool);
        let dev = insert_user(&pool, "dev@example.com").await;
        let other_dev = insert_user(&pool, "other@example.com").await;
        let codex = |from, to| terms(AiProvider::Codex, from, Some(to));

        repository
            .insert(&dev, &codex(date!(2026 - 01 - 01), date!(2026 - 01 - 31)))
            .await
            .unwrap();

        // `valid_to` is inclusive, so a successor may start the next day only.
        for (from, to) in [
            (date!(2026 - 01 - 31), date!(2026 - 02 - 28)),
            (date!(2025 - 12 - 01), date!(2026 - 01 - 01)),
            (date!(2026 - 01 - 10), date!(2026 - 01 - 20)),
            (date!(2025 - 12 - 01), date!(2026 - 03 - 01)),
        ] {
            assert!(
                is_overlap(repository.insert(&dev, &codex(from, to)).await),
                "{from}..={to}"
            );
        }

        for (user, subscription) in [
            (dev, codex(date!(2026 - 02 - 01), date!(2026 - 02 - 28))),
            (dev, codex(date!(2025 - 12 - 01), date!(2025 - 12 - 31))),
            (
                dev,
                terms(
                    AiProvider::Claude,
                    date!(2026 - 01 - 01),
                    Some(date!(2026 - 01 - 31)),
                ),
            ),
            (
                other_dev,
                codex(date!(2026 - 01 - 01), date!(2026 - 01 - 31)),
            ),
        ] {
            repository.insert(&user, &subscription).await.unwrap();
        }
        assert_eq!(
            repository
                .list(AiSubscriptionScope::AllUsers)
                .await
                .unwrap()
                .len(),
            5
        );
    }

    #[sqlx::test]
    async fn an_ongoing_subscription_blocks_every_later_day(pool: PgPool) {
        let repository = repository(&pool);
        let dev = insert_user(&pool, "dev@example.com").await;
        repository
            .insert(
                &dev,
                &terms(AiProvider::Claude, date!(2026 - 03 - 01), None),
            )
            .await
            .unwrap();

        for (from, to) in [
            (date!(2026 - 03 - 01), None),
            (date!(2025 - 01 - 01), None),
            (date!(2031 - 01 - 01), Some(date!(2031 - 01 - 31))),
            (date!(2026 - 02 - 01), Some(date!(2026 - 03 - 01))),
        ] {
            assert!(
                is_overlap(
                    repository
                        .insert(&dev, &terms(AiProvider::Claude, from, to))
                        .await
                ),
                "{from}..{to:?}"
            );
        }

        // A period that ends the day before the ongoing one starts is fine.
        repository
            .insert(
                &dev,
                &terms(
                    AiProvider::Claude,
                    date!(2026 - 01 - 01),
                    Some(date!(2026 - 02 - 28)),
                ),
            )
            .await
            .unwrap();
    }

    #[sqlx::test]
    async fn an_update_may_move_its_own_period_but_not_onto_another(pool: PgPool) {
        let repository = repository(&pool);
        let dev = insert_user(&pool, "dev@example.com").await;
        let scope = AiSubscriptionScope::User(dev);
        let plus = repository
            .insert(&dev, &terms(AiProvider::Codex, date!(2026 - 01 - 01), None))
            .await
            .unwrap();

        // Ending the ongoing subscription makes room for its successor.
        let ended = repository
            .update(
                scope,
                plus.id,
                &terms(
                    AiProvider::Codex,
                    date!(2026 - 01 - 01),
                    Some(date!(2026 - 05 - 31)),
                ),
            )
            .await
            .unwrap();
        assert_eq!(ended.terms.period.valid_to(), Some(date!(2026 - 05 - 31)));
        let pro = repository
            .insert(&dev, &terms(AiProvider::Codex, date!(2026 - 06 - 01), None))
            .await
            .unwrap();

        assert!(is_overlap(
            repository
                .update(
                    scope,
                    plus.id,
                    &terms(
                        AiProvider::Codex,
                        date!(2026 - 01 - 01),
                        Some(date!(2026 - 06 - 01)),
                    ),
                )
                .await
        ));
        assert!(is_overlap(
            repository
                .update(
                    scope,
                    pro.id,
                    &terms(AiProvider::Codex, date!(2026 - 05 - 31), None),
                )
                .await
        ));
        assert_eq!(
            repository.list(scope).await.unwrap(),
            [pro, ended],
            "a rejected update changes nothing"
        );
    }

    #[sqlx::test]
    async fn a_user_scope_reaches_only_that_users_subscriptions(pool: PgPool) {
        let repository = repository(&pool);
        let owner = insert_user(&pool, "owner@example.com").await;
        let other = insert_user(&pool, "other@example.com").await;
        let owned = repository
            .insert(
                &owner,
                &terms(AiProvider::Claude, date!(2026 - 01 - 01), None),
            )
            .await
            .unwrap();
        let changed = terms(AiProvider::Claude, date!(2026 - 02 - 01), None);

        let foreign = AiSubscriptionScope::User(other);
        assert!(repository.list(foreign).await.unwrap().is_empty());
        assert!(matches!(
            repository.update(foreign, owned.id, &changed).await,
            Err(AiSubscriptionError::NotFound)
        ));
        assert!(matches!(
            repository.delete(foreign, owned.id).await,
            Err(AiSubscriptionError::NotFound)
        ));
        assert_eq!(
            repository
                .list(AiSubscriptionScope::User(owner))
                .await
                .unwrap(),
            std::slice::from_ref(&owned)
        );

        // Every user's subscriptions are in scope for admins.
        let updated = repository
            .update(AiSubscriptionScope::AllUsers, owned.id, &changed)
            .await
            .unwrap();
        assert_eq!((updated.user_id, updated.terms), (owner, changed));
        repository
            .delete(AiSubscriptionScope::AllUsers, owned.id)
            .await
            .unwrap();
        assert!(matches!(
            repository
                .delete(AiSubscriptionScope::AllUsers, owned.id)
                .await,
            Err(AiSubscriptionError::NotFound)
        ));
    }

    #[sqlx::test]
    async fn fees_are_stored_as_exact_decimals(pool: PgPool) {
        let repository = repository(&pool);
        let dev = insert_user(&pool, "dev@example.com").await;
        let mut usd = terms(AiProvider::Copilot, date!(2026 - 01 - 01), None);
        usd.plan = AiSubscriptionPlan::parse("Copilot Business").unwrap();
        usd.monthly_cost = AiMonthlyCost::parse("19.99").unwrap();
        usd.currency = AiCurrency::parse("usd").unwrap();
        let mut largest = terms(AiProvider::Grok, date!(2026 - 01 - 01), None);
        largest.monthly_cost = AiMonthlyCost::parse("9999999999.99").unwrap();

        for subscription in [&usd, &largest] {
            let stored = repository.insert(&dev, subscription).await.unwrap();
            assert_eq!(&stored.terms, subscription);
        }

        let stored: Vec<(String, String)> = sqlx::query_as(
            "SELECT monthly_cost::text, currency FROM ai_subscriptions ORDER BY provider",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            stored,
            [
                ("19.99".to_string(), "USD".to_string()),
                ("9999999999.99".to_string(), "SEK".to_string()),
            ]
        );
    }

    #[sqlx::test]
    async fn subscription_dates_fit_the_four_digit_http_contract(pool: PgPool) {
        let dev = insert_user(&pool, "dev@example.com").await;
        for (valid_from, valid_to) in [
            ("0001-01-01 BC", None),
            ("10000-01-01", None),
            ("2026-01-01", Some("10000-01-01")),
        ] {
            let error = sqlx::query(
                "INSERT INTO ai_subscriptions (
                    user_id, provider, plan, monthly_cost, currency, valid_from, valid_to
                ) VALUES ($1, 'codex', 'Synthetic plan', 1, 'SEK', $2::text::date, $3::text::date)",
            )
            .bind(dev.as_i32())
            .bind(valid_from)
            .bind(valid_to)
            .execute(&pool)
            .await
            .unwrap_err();
            assert_eq!(
                error
                    .as_database_error()
                    .and_then(|error| error.constraint()),
                Some("ai_subscriptions_period_check"),
            );
        }

        let last_day = date!(9999 - 12 - 31);
        let stored = repository(&pool)
            .insert(&dev, &terms(AiProvider::Codex, last_day, Some(last_day)))
            .await
            .unwrap();
        assert_eq!(stored.terms.period.valid_from(), last_day);
        assert_eq!(stored.terms.period.valid_to(), Some(last_day));
    }

    #[sqlx::test]
    async fn a_subscription_for_an_unknown_user_is_rejected(pool: PgPool) {
        let result = repository(&pool)
            .insert(
                &UserId::new(4_040),
                &terms(AiProvider::Claude, date!(2026 - 01 - 01), None),
            )
            .await;

        assert!(matches!(result, Err(AiSubscriptionError::UserNotFound)));
    }

    /// Stores Codex plan hints from one machine of `user`, replacing that
    /// machine's hints between the first and the last.
    async fn upload_hints(
        pool: &PgPool,
        user: &UserId,
        machine_id: &str,
        hints: &[(OffsetDateTime, &str)],
    ) {
        let first = hints.iter().map(|&(hour, _)| hour).min().unwrap();
        let last = hints.iter().map(|&(hour, _)| hour).max().unwrap();
        let upload = AiUsageUpload::new(
            AiMachine {
                id: AiMachineId::parse(machine_id).unwrap(),
                label: "laptop".to_string(),
                client_version: "0.2.0".to_string(),
                time_zone: "Europe/Stockholm".to_string(),
            },
            AiUsageWindow::new(first, last + time::Duration::HOUR).unwrap(),
            AiPricing {
                status: AiPricingStatus::Fresh,
                fetched_at: None,
                source: "https://example.com/prices.json".to_string(),
            },
            Vec::new(),
            vec![AiProviderCoverage {
                provider: AiProvider::Codex,
                status: AiCoverageStatus::Ok,
                files: 1,
                unreadable: 0,
                malformed_lines: 0,
                skipped_records: 0,
                duplicates: 0,
            }],
            hints
                .iter()
                .map(|&(hour_start, plan)| AiProviderHint {
                    provider: AiProvider::Codex,
                    hour_start,
                    plan: plan.to_string(),
                })
                .collect(),
        )
        .unwrap();
        PostgresAiUsageRepository::new(db(pool))
            .replace_window(user, &upload)
            .await
            .unwrap();
    }

    const LAPTOP: &str = "5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a";
    const DESKTOP: &str = "0b7a3c52-9d1e-4f6a-8b2c-3e4d5f6a7b8c";
    const OTHER_LAPTOP: &str = "7e57c0de-0000-4000-8000-000000000001";

    /// `(user, day, plans, next_subscription_from)` of each mismatch day.
    type MismatchRow = (UserId, Date, Vec<String>, Option<Date>);

    async fn mismatch_days(
        repository: &PostgresAiSubscriptionRepository,
        scope: AiSubscriptionScope,
        first_day: Date,
        last_day: Date,
    ) -> Vec<MismatchRow> {
        repository
            .plan_mismatch_days(
                scope,
                AiUsageDateRange::from_inclusive(first_day, last_day).unwrap(),
                &stockholm(),
            )
            .await
            .unwrap()
            .into_iter()
            .map(|day| {
                assert_eq!(day.provider, AiProvider::Codex);
                (day.user_id, day.day, day.plans, day.next_subscription_from)
            })
            .collect()
    }

    fn plans(plans: &[&str]) -> Vec<String> {
        plans.iter().map(ToString::to_string).collect()
    }

    #[sqlx::test]
    async fn mismatch_days_are_paid_plan_days_that_no_subscription_covers(pool: PgPool) {
        let repository = repository(&pool);
        let dev = insert_user(&pool, "dev@example.com").await;
        let other_dev = insert_user(&pool, "other@example.com").await;
        upload_hints(
            &pool,
            &dev,
            LAPTOP,
            &[
                (datetime!(2026 - 09 - 15 10:00 UTC), "Free"),
                // 23:00 on 30 September in Stockholm (UTC+2).
                (datetime!(2026 - 09 - 30 21:00 UTC), "pro"),
                // Midnight on 1 October in Stockholm, the subscription's first day.
                (datetime!(2026 - 09 - 30 22:00 UTC), "pro"),
                // 23:00 on 31 October (UTC+1), its last day, inclusive.
                (datetime!(2026 - 10 - 31 22:00 UTC), "pro"),
                // Midnight on 1 November.
                (datetime!(2026 - 10 - 31 23:00 UTC), "pro"),
                (datetime!(2026 - 11 - 02 10:00 UTC), "free"),
                (datetime!(2026 - 11 - 03 10:00 UTC), "FREE"),
            ],
        )
        .await;
        upload_hints(
            &pool,
            &dev,
            DESKTOP,
            &[(datetime!(2026 - 11 - 03 11:00 UTC), "plus")],
        )
        .await;
        upload_hints(
            &pool,
            &other_dev,
            OTHER_LAPTOP,
            &[(datetime!(2026 - 10 - 15 10:00 UTC), "plus")],
        )
        .await;
        for (user, subscription) in [
            (
                dev,
                terms(
                    AiProvider::Codex,
                    date!(2026 - 10 - 01),
                    Some(date!(2026 - 10 - 31)),
                ),
            ),
            (dev, terms(AiProvider::Codex, date!(2026 - 12 - 01), None)),
            // Another provider's subscription explains no Codex plans.
            (
                other_dev,
                terms(AiProvider::Claude, date!(2026 - 01 - 01), None),
            ),
        ] {
            repository.insert(&user, &subscription).await.unwrap();
        }

        let autumn = (date!(2026 - 09 - 01), date!(2026 - 11 - 30));
        let dev_days = [
            (
                dev,
                date!(2026 - 09 - 30),
                plans(&["pro"]),
                Some(date!(2026 - 10 - 01)),
            ),
            (
                dev,
                date!(2026 - 11 - 01),
                plans(&["pro"]),
                Some(date!(2026 - 12 - 01)),
            ),
            // The free hint that day is no evidence; the other machine's is.
            (
                dev,
                date!(2026 - 11 - 03),
                plans(&["plus"]),
                Some(date!(2026 - 12 - 01)),
            ),
        ];
        assert_eq!(
            mismatch_days(
                &repository,
                AiSubscriptionScope::User(dev),
                autumn.0,
                autumn.1
            )
            .await,
            dev_days
        );

        let mut everyone = dev_days.to_vec();
        everyone.push((other_dev, date!(2026 - 10 - 15), plans(&["plus"]), None));
        assert_eq!(
            mismatch_days(
                &repository,
                AiSubscriptionScope::AllUsers,
                autumn.0,
                autumn.1
            )
            .await,
            everyone
        );

        // Both bounds are local days: 1 November starts at 23:00 UTC.
        assert_eq!(
            mismatch_days(
                &repository,
                AiSubscriptionScope::User(dev),
                date!(2026 - 11 - 01),
                date!(2026 - 11 - 02),
            )
            .await,
            [dev_days[1].clone()]
        );
    }

    #[sqlx::test]
    async fn mismatch_days_are_the_days_that_resolve_to_api_billing(pool: PgPool) {
        let repository = repository(&pool);
        let dev = insert_user(&pool, "dev@example.com").await;
        let first_day = date!(2027 - 12 - 25);
        let last_day = date!(2028 - 03 - 05);
        let days = (0..)
            .map(|offset| first_day + time::Duration::days(offset))
            .take_while(|day| *day <= last_day)
            .collect::<Vec<_>>();
        // A hint at noon in Stockholm on every day.
        let hints = days
            .iter()
            .map(|day| (day.with_hms(11, 0, 0).unwrap().assume_utc(), "pro"))
            .collect::<Vec<_>>();
        upload_hints(&pool, &dev, LAPTOP, &hints).await;

        let mut subscriptions = Vec::new();
        for subscription in [
            terms(
                AiProvider::Codex,
                date!(2028 - 01 - 01),
                Some(date!(2028 - 01 - 31)),
            ),
            terms(
                AiProvider::Codex,
                date!(2028 - 02 - 10),
                Some(date!(2028 - 02 - 10)),
            ),
            terms(
                AiProvider::Codex,
                date!(2028 - 02 - 29),
                Some(date!(2028 - 03 - 01)),
            ),
            terms(AiProvider::Codex, date!(2028 - 03 - 04), None),
            terms(AiProvider::Claude, date!(2027 - 01 - 01), None),
        ] {
            subscriptions.push(repository.insert(&dev, &subscription).await.unwrap());
        }

        let expected = days
            .iter()
            .filter(|day| {
                resolve_billing_mode(&subscriptions, dev, AiProvider::Codex, **day)
                    == AiBillingMode::Api
            })
            .map(|day| {
                let next = subscriptions
                    .iter()
                    .filter(|subscription| subscription.terms.provider == AiProvider::Codex)
                    .map(|subscription| subscription.terms.period.valid_from())
                    .filter(|valid_from| valid_from > day)
                    .min();
                (dev, *day, plans(&["pro"]), next)
            })
            .collect::<Vec<_>>();

        // Late December, early February, the rest of February, and 2–3 March.
        assert_eq!(expected.len(), 7 + 9 + 18 + 2);
        assert_eq!(
            mismatch_days(
                &repository,
                AiSubscriptionScope::User(dev),
                first_day,
                last_day
            )
            .await,
            expected
        );
    }

    fn service(pool: &PgPool) -> AiSubscriptionServiceImpl<PostgresAiSubscriptionRepository> {
        AiSubscriptionServiceImpl::new(Arc::new(repository(pool)), stockholm())
    }

    #[sqlx::test]
    async fn the_default_mismatch_search_covers_the_last_ninety_local_days(pool: PgPool) {
        let service = service(&pool);
        let dev = insert_user(&pool, "dev@example.com").await;
        let today: Date =
            sqlx::query_scalar("SELECT (now() AT TIME ZONE 'Europe/Stockholm')::date")
                .fetch_one(&pool)
                .await
                .unwrap();
        let calendar = service.calendar().await.unwrap();
        assert_eq!(
            (calendar.time_zone.as_str(), calendar.today),
            ("Europe/Stockholm", today)
        );

        // The current hour is local today in a zone with whole-hour offsets.
        let this_hour = OffsetDateTime::now_utc()
            .replace_minute(0)
            .and_then(|hour| hour.replace_second(0))
            .and_then(|hour| hour.replace_nanosecond(0))
            .unwrap();
        upload_hints(
            &pool,
            &dev,
            LAPTOP,
            &[
                (this_hour - time::Duration::days(100), "pro"),
                (this_hour - time::Duration::days(30), "plus"),
                (this_hour, "pro"),
            ],
        )
        .await;

        let mismatches = service
            .plan_mismatches(AiSubscriptionScope::User(dev), None, None)
            .await
            .unwrap();
        assert_eq!(mismatches.dates.last_day(), today);
        assert_eq!(mismatches.dates.start(), today - time::Duration::days(89));
        let [run] = mismatches.runs.as_slice() else {
            panic!("expected one run, got {:?}", mismatches.runs);
        };
        assert_eq!(
            (
                run.last_day,
                run.days,
                run.plans.clone(),
                run.last_uncovered_day
            ),
            (today, 2, plans(&["plus", "pro"]), None)
        );

        assert!(matches!(
            service
                .plan_mismatches(
                    AiSubscriptionScope::User(dev),
                    Some(today),
                    today.previous_day()
                )
                .await,
            Err(AiSubscriptionError::Invalid(_))
        ));
    }
}
