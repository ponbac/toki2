use std::collections::HashMap;

use async_trait::async_trait;
use time::Date;

use super::ai_usage_reads::local_today;
use crate::{
    db::DbPool,
    domain::{
        models::{
            AiAdoptionCredit, AiAdoptionCredits, AiAdoptionPerson, AiAdoptionSeat,
            AiAdoptionUsageRow, AiBillingUsage, AiCreditKind, AiProvider, AiTokenCounts,
            AiUsageDateRange, AiUsageTimeZone, AiUsdNanos, UserId,
        },
        ports::outbound::AiAdoptionRepository,
        AiBillingError,
    },
};

/// Adoption reads of AI usage and imported credits. Usage groups buckets by
/// local day and model at the finest, so no hour or session leaves the
/// database through it.
pub struct PostgresAiAdoptionRepository {
    pool: DbPool,
}

impl PostgresAiAdoptionRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AiAdoptionRepository for PostgresAiAdoptionRepository {
    async fn today(&self, time_zone: &AiUsageTimeZone) -> Result<Date, AiBillingError> {
        local_today(&self.pool, time_zone)
            .await
            .map_err(storage_error)
    }

    async fn usage(
        &self,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
        credits: Option<&AiAdoptionCredits>,
        user_id: Option<&UserId>,
    ) -> Result<Vec<AiAdoptionUsageRow>, AiBillingError> {
        // A bucket belongs to the local date its hour starts on, and is
        // credited by the exact instant its hour starts at, so one local day
        // can hold both. Costs are summed exactly per group, then rounded to
        // billionths of a dollar. The range uses `ai_usage_buckets_hour_start_idx`,
        // or the `(user_id, hour_start)` index for one user.
        let payer = credits.map(|credits| credits.payer.as_str());
        let from = credits.map(AiAdoptionCredits::starts_at);
        let rows = sqlx::query!(
            r#"
            SELECT
                bucket.user_id,
                (bucket.hour_start AT TIME ZONE $3)::date AS "day!",
                bucket.provider,
                bucket.billing_provider,
                bucket.model,
                COALESCE(
                    COALESCE(bucket.billing_provider, bucket.provider) = $5
                        AND bucket.hour_start >= $6::timestamptz,
                    false
                ) AS "credited!",
                SUM(bucket.input_tokens)::int8 AS "input_tokens!",
                SUM(bucket.cache_read_tokens)::int8 AS "cache_read_tokens!",
                SUM(bucket.cache_write_tokens)::int8 AS "cache_write_tokens!",
                SUM(bucket.output_tokens)::int8 AS "output_tokens!",
                SUM(bucket.records)::int8 AS "records!",
                round(COALESCE(SUM(bucket.estimated_cost_usd), 0) * 1000000000)::int8
                    AS "priced_cost_nanos!",
                SUM(bucket.unpriced_records)::int8 AS "unpriced_records!"
            FROM ai_usage_buckets AS bucket
            WHERE bucket.hour_start >= ($1::date::timestamp AT TIME ZONE $3)
              AND bucket.hour_start < ($2::date::timestamp AT TIME ZONE $3)
              AND ($4::int4 IS NULL OR bucket.user_id = $4)
            GROUP BY bucket.user_id, 2, bucket.provider, bucket.billing_provider, bucket.model, 6
            ORDER BY bucket.user_id, 2, bucket.provider, bucket.model
            "#,
            dates.start(),
            dates.end(),
            time_zone.as_str(),
            user_id.map(UserId::as_i32),
            payer,
            from,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(AiAdoptionUsageRow {
                    user_id: UserId::new(row.user_id),
                    day: row.day,
                    provider: provider(&row.provider)?,
                    billing_provider: row.billing_provider.as_deref().map(provider).transpose()?,
                    model: row.model,
                    credited: row.credited,
                    usage: AiBillingUsage {
                        tokens: AiTokenCounts {
                            input: row.input_tokens,
                            cache_read: row.cache_read_tokens,
                            cache_write: row.cache_write_tokens,
                            output: row.output_tokens,
                        },
                        records: row.records,
                        priced_cost: nanos(row.priced_cost_nanos)?,
                        unpriced_records: row.unpriced_records,
                    },
                })
            })
            .collect()
    }

    async fn user_credits(
        &self,
        credits: &AiAdoptionCredits,
        dates: AiUsageDateRange,
    ) -> Result<Vec<AiAdoptionCredit>, AiBillingError> {
        // An account is a user's on the days its disposition to them covers.
        sqlx::query!(
            r#"
            SELECT
                disposition.user_id AS "user_id!",
                usage.day,
                SUM(usage.gross_usd_nanos)::int8 AS "amount!"
            FROM ai_provider_account_usage AS usage
            JOIN ai_provider_account_dispositions AS disposition
                ON disposition.provider = usage.provider
                AND disposition.account_id = usage.account_id
                AND disposition.target = 'user'
                AND usage.day >= disposition.valid_from
                AND (disposition.valid_to IS NULL OR usage.day <= disposition.valid_to)
            WHERE usage.provider = $1
              AND usage.day >= GREATEST($2::date, $4::date)
              AND usage.day < $3
              AND usage.kind <> $5
            GROUP BY disposition.user_id, usage.day
            ORDER BY disposition.user_id, usage.day
            "#,
            credits.payer.as_str(),
            dates.start(),
            dates.end(),
            credits.from,
            AiCreditKind::RepositoryBilled.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(|row| {
            Ok(AiAdoptionCredit {
                user_id: UserId::new(row.user_id),
                day: row.day,
                amount: nanos(row.amount)?,
            })
        })
        .collect()
    }

    async fn company_credits(
        &self,
        credits: &AiAdoptionCredits,
        dates: AiUsageDateRange,
    ) -> Result<Vec<(Date, AiUsdNanos)>, AiBillingError> {
        sqlx::query!(
            r#"
            SELECT day, SUM(gross_usd_nanos)::int8 AS "amount!"
            FROM ai_provider_org_credits
            WHERE provider = $1
              AND day >= GREATEST($2::date, $4::date)
              AND day < $3
            GROUP BY day
            ORDER BY day
            "#,
            credits.payer.as_str(),
            dates.start(),
            dates.end(),
            credits.from,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(|row| Ok((row.day, nanos(row.amount)?)))
        .collect()
    }

    async fn credits_complete_through(
        &self,
        credits: &AiAdoptionCredits,
        today: Date,
    ) -> Result<Option<Date>, AiBillingError> {
        // An account that failed to import leaves its owner's credits short
        // while the organization's are complete, so both bound the date.
        let through = sqlx::query_scalar!(
            r#"
            WITH accounts AS (
                SELECT
                    disposition.account_id,
                    COALESCE(
                        MAX(slice.day) FILTER (WHERE slice.complete_at IS NOT NULL),
                        $3::date - 1
                    ) AS through
                FROM ai_provider_account_dispositions AS disposition
                LEFT JOIN ai_provider_usage_slices AS slice
                    ON slice.provider = disposition.provider
                    AND slice.source = 'account_credits'
                    AND slice.account_id = disposition.account_id
                WHERE disposition.provider = $1
                  AND disposition.target = 'user'
                  AND disposition.valid_from <= $2
                  AND (disposition.valid_to IS NULL OR disposition.valid_to >= $2)
                GROUP BY disposition.account_id
            )
            SELECT LEAST(
                (
                    SELECT MAX(day)
                    FROM ai_provider_usage_slices
                    WHERE provider = $1 AND source = 'org_credits' AND complete_at IS NOT NULL
                ),
                (SELECT MIN(through) FROM accounts)
            )
            "#,
            credits.payer.as_str(),
            today,
            credits.from,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(through.filter(|day| *day >= credits.from))
    }

    async fn people(
        &self,
        today: Date,
        time_zone: &AiUsageTimeZone,
    ) -> Result<Vec<AiAdoptionPerson>, AiBillingError> {
        let users = sqlx::query!(
            r#"
            SELECT
                users.id,
                users.full_name,
                EXISTS (
                    SELECT 1 FROM time_tracking_user_links AS link
                    WHERE link.user_id = users.id AND link.active
                ) AS "linked!",
                (
                    SELECT MIN(machine.first_seen_at AT TIME ZONE $1)::date
                    FROM ai_usage_machines AS machine
                    WHERE machine.user_id = users.id
                ) AS first_synced_on
            FROM users
            ORDER BY users.full_name, users.id
            "#,
            time_zone.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        // A seat is someone's while its account's disposition to them covers
        // today; it is held since the seat was assigned, a local day.
        let seats = sqlx::query!(
            r#"
            SELECT
                disposition.user_id AS "user_id!",
                seat.provider,
                MIN(seat.seat_created_at AT TIME ZONE $2)::date AS "since!"
            FROM ai_provider_seats AS seat
            JOIN ai_provider_account_dispositions AS disposition
                ON disposition.provider = seat.provider
                AND disposition.account_id = seat.account_id
                AND disposition.target = 'user'
                AND disposition.valid_from <= $1
                AND (disposition.valid_to IS NULL OR disposition.valid_to >= $1)
            WHERE seat.unlisted_at IS NULL
            GROUP BY disposition.user_id, seat.provider
            ORDER BY disposition.user_id, seat.provider
            "#,
            today,
            time_zone.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        let mut seats_of: HashMap<i32, Vec<AiAdoptionSeat>> = HashMap::new();
        for seat in seats {
            seats_of
                .entry(seat.user_id)
                .or_default()
                .push(AiAdoptionSeat {
                    provider: provider(&seat.provider)?,
                    since: seat.since,
                });
        }

        Ok(users
            .into_iter()
            .map(|user| AiAdoptionPerson {
                user_id: UserId::new(user.id),
                full_name: user.full_name,
                linked: user.linked,
                first_synced_on: user.first_synced_on,
                seats: seats_of.remove(&user.id).unwrap_or_default(),
            })
            .collect())
    }

    async fn user_exists(&self, user_id: &UserId) -> Result<bool, AiBillingError> {
        sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM users WHERE id = $1) AS "exists!""#,
            user_id.as_i32(),
        )
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)
    }
}

fn provider(raw: &str) -> Result<AiProvider, AiBillingError> {
    AiProvider::parse(raw)
        .ok_or_else(|| AiBillingError::Storage(format!("unknown provider {raw:?}")))
}

fn nanos(raw: i64) -> Result<AiUsdNanos, AiBillingError> {
    AiUsdNanos::new(raw).ok_or_else(|| AiBillingError::Storage(format!("negative amount {raw}")))
}

fn storage_error(error: sqlx::Error) -> AiBillingError {
    if error
        .as_database_error()
        .and_then(|error| error.code())
        .as_deref()
        == Some("22003")
    {
        return AiBillingError::NumericRange;
    }
    AiBillingError::Storage(error.to_string())
}

#[cfg(test)]
mod tests;
