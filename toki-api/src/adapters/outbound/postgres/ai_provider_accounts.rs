use std::collections::HashMap;

use async_trait::async_trait;
use time::{Date, OffsetDateTime};

use crate::{
    db::DbPool,
    domain::{
        models::{
            AiAccountDisposition, AiAccountDispositionId, AiAccountDispositionTerms,
            AiAccountPeriod, AiAccountTarget, AiBillingDeveloper, AiBillingUsage,
            AiDirectoryStatus, AiMeteredUserDay, AiProvider, AiProviderAccountId,
            AiProviderAccountProfile, AiProviderLogin, AiSeatListing, AiStoredProfile,
            AiStoredProviderAccount, AiStoredSeat, AiTokenCounts, AiUsageDateRange, AiUsdNanos,
            UserId,
        },
        ports::outbound::AiProviderAccountRepository,
        AiProviderAccountError,
    },
};

const ACCOUNT_OVERLAP_CONSTRAINT: &str = "ai_provider_account_dispositions_account_no_overlap";
const USER_OVERLAP_CONSTRAINT: &str = "ai_provider_account_dispositions_user_no_overlap";
const USER_CONSTRAINT: &str = "ai_provider_account_dispositions_user_id_fkey";
const ACCOUNT_CONSTRAINT: &str = "ai_provider_account_dispositions_account_fkey";
const TARGET_USER: &str = "user";
const TARGET_EXTERNAL: &str = "external";

pub struct PostgresAiProviderAccountRepository {
    pool: DbPool,
}

impl PostgresAiProviderAccountRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Maps the constraints a disposition write can break to domain errors,
    /// naming the account or the user's other account. `account` is the
    /// account written; `id` the disposition updated, if any.
    async fn write_error(
        &self,
        error: sqlx::Error,
        payer: AiProvider,
        account: AccountRef,
        terms: &AiAccountDispositionTerms,
    ) -> AiProviderAccountError {
        let constraint = error
            .as_database_error()
            .and_then(|error| error.constraint())
            .map(str::to_string);
        match constraint.as_deref() {
            Some(ACCOUNT_OVERLAP_CONSTRAINT) => match self.account_login(payer, account).await {
                Ok(login) => AiProviderAccountError::AccountOverlap { login },
                Err(error) => error,
            },
            Some(USER_OVERLAP_CONSTRAINT) => {
                match self.overlapping_login(payer, account, terms).await {
                    Ok(login) => AiProviderAccountError::UserOverlap { login },
                    Err(error) => error,
                }
            }
            Some(USER_CONSTRAINT) => AiProviderAccountError::UserNotFound,
            Some(ACCOUNT_CONSTRAINT) => AiProviderAccountError::AccountNotFound,
            _ => storage_error(error),
        }
    }

    /// The login of the written account.
    async fn account_login(
        &self,
        payer: AiProvider,
        account: AccountRef,
    ) -> Result<String, AiProviderAccountError> {
        let (account_id, disposition_id) = account.ids();
        sqlx::query_scalar!(
            r#"
            SELECT account.login
            FROM ai_provider_accounts AS account
            WHERE account.provider = $1
              AND account.account_id = COALESCE(
                  $2::int8,
                  (SELECT disposition.account_id
                   FROM ai_provider_account_dispositions AS disposition
                   WHERE disposition.provider = $1 AND disposition.id = $3)
              )
            "#,
            payer.as_str(),
            account_id,
            disposition_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?
        .ok_or(AiProviderAccountError::AccountNotFound)
    }

    /// The login of another account the terms' user is linked to on one of
    /// the terms' days.
    async fn overlapping_login(
        &self,
        payer: AiProvider,
        account: AccountRef,
        terms: &AiAccountDispositionTerms,
    ) -> Result<String, AiProviderAccountError> {
        let (_, disposition_id) = account.ids();
        let login = sqlx::query_scalar!(
            r#"
            SELECT login
            FROM ai_provider_account_dispositions
            WHERE provider = $1
              AND user_id = $2
              AND daterange(valid_from, valid_to, '[]') && daterange($3, $4, '[]')
              AND ($5::int4 IS NULL OR id <> $5)
            ORDER BY valid_from
            LIMIT 1
            "#,
            payer.as_str(),
            terms.target.user_id().map(|user_id| user_id.as_i32()),
            terms.period.valid_from(),
            terms.period.valid_to(),
            disposition_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        // Only a concurrent change can remove the conflicting row meanwhile.
        Ok(login.unwrap_or_else(|| "another account".to_string()))
    }
}

/// The account a disposition write concerns: by id for an insert, through the
/// disposition for an update.
#[derive(Clone, Copy)]
enum AccountRef {
    Account(AiProviderAccountId),
    Disposition(AiAccountDispositionId),
}

impl AccountRef {
    fn ids(self) -> (Option<i64>, Option<i32>) {
        match self {
            Self::Account(account) => (Some(account.as_i64()), None),
            Self::Disposition(id) => (None, Some(id.as_i32())),
        }
    }
}

/// A row of `ai_provider_account_dispositions`.
struct StoredDisposition {
    id: i32,
    provider: String,
    account_id: i64,
    login: String,
    target: String,
    user_id: Option<i32>,
    valid_from: Date,
    valid_to: Option<Date>,
    updated_at: OffsetDateTime,
    updated_by: Option<i32>,
}

impl TryFrom<StoredDisposition> for AiAccountDisposition {
    type Error = AiProviderAccountError;

    fn try_from(row: StoredDisposition) -> Result<Self, Self::Error> {
        let id = row.id;
        let corrupt = || corrupt(format!("stored disposition {id} is invalid"));
        let target = match (row.target.as_str(), row.user_id) {
            (TARGET_USER, Some(user_id)) => AiAccountTarget::User(UserId::new(user_id)),
            (TARGET_EXTERNAL, None) => AiAccountTarget::External,
            _ => return Err(corrupt()),
        };
        Ok(Self {
            id: AiAccountDispositionId::new(id),
            payer: AiProvider::parse(&row.provider).ok_or_else(corrupt)?,
            account_id: AiProviderAccountId::new(row.account_id).ok_or_else(corrupt)?,
            login: AiProviderLogin::parse(&row.login).map_err(|_| corrupt())?,
            terms: AiAccountDispositionTerms {
                target,
                period: AiAccountPeriod::new(row.valid_from, row.valid_to)
                    .map_err(|_| corrupt())?,
            },
            updated_at: row.updated_at,
            updated_by: row.updated_by.map(UserId::new),
        })
    }
}

fn target_columns(target: AiAccountTarget) -> (&'static str, Option<i32>) {
    match target {
        AiAccountTarget::User(user_id) => (TARGET_USER, Some(user_id.as_i32())),
        AiAccountTarget::External => (TARGET_EXTERNAL, None),
    }
}

#[async_trait]
impl AiProviderAccountRepository for PostgresAiProviderAccountRepository {
    async fn now(&self) -> Result<OffsetDateTime, AiProviderAccountError> {
        sqlx::query_scalar!(r#"SELECT now() AS "now!""#)
            .fetch_one(&self.pool)
            .await
            .map_err(storage_error)
    }

    async fn store_seat_listing(
        &self,
        payer: AiProvider,
        listing: &AiSeatListing,
    ) -> Result<(u32, u32), AiProviderAccountError> {
        let seats = &listing.seats;
        let account_ids: Vec<i64> = seats.iter().map(|seat| seat.account_id.as_i64()).collect();
        let logins: Vec<&str> = seats.iter().map(|seat| seat.login.as_str()).collect();
        let created: Vec<OffsetDateTime> = seats.iter().map(|seat| seat.created_at).collect();
        let pending: Vec<Option<Date>> = seats
            .iter()
            .map(|seat| seat.pending_cancellation_on)
            .collect();
        let activity: Vec<Option<OffsetDateTime>> =
            seats.iter().map(|seat| seat.last_activity_at).collect();
        let clients: Vec<Option<&str>> = seats
            .iter()
            .map(|seat| seat.last_activity_client.as_deref())
            .collect();

        let mut transaction = self.pool.begin().await.map_err(storage_error)?;

        // One listing of a payer is stored at a time.
        sqlx::query_scalar!(
            r#"
            SELECT 1 AS "locked!"
            FROM (SELECT pg_advisory_xact_lock(hashtextextended('ai_provider_seats:' || $1, 0)))
                AS lock
            "#,
            payer.as_str(),
        )
        .fetch_one(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        // Accounts: new ones are added, renamed ones follow.
        sqlx::query!(
            r#"
            INSERT INTO ai_provider_accounts (provider, account_id, login)
            SELECT $1, listed.account_id, listed.login
            FROM UNNEST($2::int8[], $3::text[]) AS listed(account_id, login)
            ON CONFLICT (provider, account_id) DO UPDATE
            SET login = EXCLUDED.login,
                updated_at = now()
            WHERE ai_provider_accounts.login IS DISTINCT FROM EXCLUDED.login
            "#,
            payer.as_str(),
            &account_ids,
            &logins as &[&str],
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        // An account listed with another assignment ends its open seat. The
        // instants compare as instants, whatever offset GitHub rendered.
        let mut ended = sqlx::query!(
            r#"
            UPDATE ai_provider_seats AS seat
            SET unlisted_at = now()
            FROM UNNEST($2::int8[], $3::timestamptz[]) AS listed(account_id, created_at)
            WHERE seat.provider = $1
              AND seat.unlisted_at IS NULL
              AND seat.account_id = listed.account_id
              AND seat.seat_created_at <> listed.created_at
            "#,
            payer.as_str(),
            &account_ids,
            &created,
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?
        .rows_affected();

        // Only a complete listing proves that the seats it omits ended.
        if listing.is_complete() {
            ended += sqlx::query!(
                r#"
                UPDATE ai_provider_seats
                SET unlisted_at = now()
                WHERE provider = $1
                  AND unlisted_at IS NULL
                  AND NOT (account_id = ANY($2::int8[]))
                "#,
                payer.as_str(),
                &account_ids,
            )
            .execute(&mut transaction.executor())
            .await
            .map_err(storage_error)?
            .rows_affected();
        }

        // Listed seats: inserted, refreshed, or reopened.
        let inserted = sqlx::query_scalar!(
            r#"
            INSERT INTO ai_provider_seats (
                provider, account_id, seat_created_at, pending_cancellation_on,
                last_activity_at, last_activity_client, first_listed_at, last_listed_at
            )
            SELECT $1, listed.account_id, listed.created_at, listed.pending_on,
                listed.activity_at, listed.client, now(), now()
            FROM UNNEST($2::int8[], $3::timestamptz[], $4::date[], $5::timestamptz[], $6::text[])
                AS listed(account_id, created_at, pending_on, activity_at, client)
            ON CONFLICT (provider, account_id, seat_created_at) DO UPDATE
            SET pending_cancellation_on = EXCLUDED.pending_cancellation_on,
                last_activity_at = EXCLUDED.last_activity_at,
                last_activity_client = EXCLUDED.last_activity_client,
                last_listed_at = EXCLUDED.last_listed_at,
                unlisted_at = NULL
            RETURNING (xmax = 0) AS "inserted!"
            "#,
            payer.as_str(),
            &account_ids,
            &created,
            &pending as &[Option<Date>],
            &activity as &[Option<OffsetDateTime>],
            &clients as &[Option<&str>],
        )
        .fetch_all(&mut transaction.executor())
        .await
        .map_err(storage_error)?
        .into_iter()
        .filter(|inserted| *inserted)
        .count();

        transaction.commit().await.map_err(storage_error)?;
        Ok((count(inserted as u64), count(ended)))
    }

    async fn account_ids(
        &self,
        payer: AiProvider,
    ) -> Result<Vec<AiProviderAccountId>, AiProviderAccountError> {
        sqlx::query_scalar!(
            r#"
            SELECT account_id
            FROM ai_provider_accounts
            WHERE provider = $1
            ORDER BY account_id
            "#,
            payer.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(account_id)
        .collect()
    }

    async fn store_profiles(
        &self,
        payer: AiProvider,
        profiles: &[AiProviderAccountProfile],
    ) -> Result<(), AiProviderAccountError> {
        if profiles.is_empty() {
            return Ok(());
        }
        let account_ids: Vec<i64> = profiles
            .iter()
            .map(|profile| profile.account_id.as_i64())
            .collect();
        let logins: Vec<&str> = profiles
            .iter()
            .map(|profile| profile.login.as_str())
            .collect();
        let names: Vec<Option<&str>> = profiles
            .iter()
            .map(|profile| profile.name.as_deref())
            .collect();
        let emails: Vec<Option<&str>> = profiles
            .iter()
            .map(|profile| profile.email.as_deref())
            .collect();

        sqlx::query!(
            r#"
            UPDATE ai_provider_accounts AS account
            SET login = profile.login,
                profile_name = profile.name,
                profile_email = profile.email,
                profile_fetched_at = now(),
                updated_at = now()
            FROM UNNEST($2::int8[], $3::text[], $4::text[], $5::text[])
                AS profile(account_id, login, name, email)
            WHERE account.provider = $1
              AND account.account_id = profile.account_id
            "#,
            payer.as_str(),
            &account_ids,
            &logins as &[&str],
            &names as &[Option<&str>],
            &emails as &[Option<&str>],
        )
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn directory_status(
        &self,
        payer: AiProvider,
    ) -> Result<Option<AiDirectoryStatus>, AiProviderAccountError> {
        let row = sqlx::query!(
            r#"
            SELECT last_attempt_at, last_success_at, last_error, seats_listed, seats_reported
            FROM ai_provider_directory_status
            WHERE provider = $1
            "#,
            payer.as_str(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(row.map(|row| AiDirectoryStatus {
            last_attempt_at: row.last_attempt_at,
            last_success_at: row.last_success_at,
            last_error: row.last_error,
            seats_listed: row.seats_listed.and_then(|count| u32::try_from(count).ok()),
            seats_reported: row
                .seats_reported
                .and_then(|count| u32::try_from(count).ok()),
        }))
    }

    async fn record_success(
        &self,
        payer: AiProvider,
        listed: u32,
        reported: Option<u32>,
    ) -> Result<(), AiProviderAccountError> {
        let column = |count: u32| i32::try_from(count).unwrap_or(i32::MAX);
        sqlx::query!(
            r#"
            INSERT INTO ai_provider_directory_status (
                provider, last_attempt_at, last_success_at, last_error, seats_listed, seats_reported
            )
            VALUES ($1, now(), now(), NULL, $2, $3)
            ON CONFLICT (provider) DO UPDATE
            SET last_attempt_at = now(),
                last_success_at = now(),
                last_error = NULL,
                seats_listed = EXCLUDED.seats_listed,
                seats_reported = EXCLUDED.seats_reported
            "#,
            payer.as_str(),
            column(listed),
            reported.map(column),
        )
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn record_failure(
        &self,
        payer: AiProvider,
        error: &str,
    ) -> Result<(), AiProviderAccountError> {
        let error = error.trim();
        let error = if error.is_empty() {
            "unknown error"
        } else {
            error
        };
        sqlx::query!(
            r#"
            INSERT INTO ai_provider_directory_status (provider, last_attempt_at, last_error)
            VALUES ($1, now(), $2)
            ON CONFLICT (provider) DO UPDATE
            SET last_attempt_at = now(),
                last_error = EXCLUDED.last_error
            "#,
            payer.as_str(),
            error,
        )
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn accounts(
        &self,
        payer: AiProvider,
    ) -> Result<Vec<AiStoredProviderAccount>, AiProviderAccountError> {
        let accounts = sqlx::query!(
            r#"
            SELECT account_id, login, profile_name, profile_email, profile_fetched_at
            FROM ai_provider_accounts
            WHERE provider = $1
            ORDER BY lower(login), account_id
            "#,
            payer.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        let seats = sqlx::query!(
            r#"
            SELECT account_id, seat_created_at, pending_cancellation_on, last_activity_at,
                last_activity_client, first_listed_at, last_listed_at, unlisted_at
            FROM ai_provider_seats
            WHERE provider = $1
            ORDER BY account_id, seat_created_at, id
            "#,
            payer.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        let dispositions = sqlx::query_as!(
            StoredDisposition,
            r#"
            SELECT id, provider, account_id, login, target, user_id, valid_from, valid_to,
                updated_at, updated_by
            FROM ai_provider_account_dispositions
            WHERE provider = $1
            ORDER BY account_id, valid_from
            "#,
            payer.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        let mut seats_by_account: HashMap<i64, Vec<AiStoredSeat>> = HashMap::new();
        for seat in seats {
            seats_by_account
                .entry(seat.account_id)
                .or_default()
                .push(AiStoredSeat {
                    created_at: seat.seat_created_at,
                    pending_cancellation_on: seat.pending_cancellation_on,
                    last_activity_at: seat.last_activity_at,
                    last_activity_client: seat.last_activity_client,
                    first_listed_at: seat.first_listed_at,
                    last_listed_at: seat.last_listed_at,
                    unlisted_at: seat.unlisted_at,
                });
        }
        let mut dispositions_by_account: HashMap<i64, Vec<AiAccountDisposition>> = HashMap::new();
        for disposition in dispositions {
            dispositions_by_account
                .entry(disposition.account_id)
                .or_default()
                .push(disposition.try_into()?);
        }

        accounts
            .into_iter()
            .map(|row| {
                let id = account_id(row.account_id)?;
                Ok(AiStoredProviderAccount {
                    account_id: id,
                    login: AiProviderLogin::parse(&row.login).map_err(|_| {
                        corrupt(format!("stored account {id} has an invalid login"))
                    })?,
                    profile: row.profile_fetched_at.map(|fetched_at| AiStoredProfile {
                        name: row.profile_name,
                        email: row.profile_email,
                        fetched_at,
                    }),
                    seats: seats_by_account.remove(&row.account_id).unwrap_or_default(),
                    dispositions: dispositions_by_account
                        .remove(&row.account_id)
                        .unwrap_or_default(),
                })
            })
            .collect()
    }

    async fn insert_disposition(
        &self,
        payer: AiProvider,
        account: AiProviderAccountId,
        terms: &AiAccountDispositionTerms,
        by: &UserId,
    ) -> Result<AiAccountDisposition, AiProviderAccountError> {
        let (target, user_id) = target_columns(terms.target);
        // The exclusion constraints reject overlaps, also between concurrent
        // requests. The login is the account's, from the same statement.
        let inserted = sqlx::query_as!(
            StoredDisposition,
            r#"
            INSERT INTO ai_provider_account_dispositions (
                provider, account_id, login, target, user_id, valid_from, valid_to,
                created_by, updated_by
            )
            SELECT account.provider, account.account_id, account.login, $3, $4, $5, $6, $7, $7
            FROM ai_provider_accounts AS account
            WHERE account.provider = $1
              AND account.account_id = $2
            RETURNING id, provider, account_id, login, target, user_id, valid_from, valid_to,
                updated_at, updated_by
            "#,
            payer.as_str(),
            account.as_i64(),
            target,
            user_id,
            terms.period.valid_from(),
            terms.period.valid_to(),
            by.as_i32(),
        )
        .fetch_optional(&self.pool)
        .await;

        match inserted {
            Ok(Some(row)) => row.try_into(),
            Ok(None) => Err(AiProviderAccountError::AccountNotFound),
            Err(error) => Err(self
                .write_error(error, payer, AccountRef::Account(account), terms)
                .await),
        }
    }

    async fn update_disposition(
        &self,
        payer: AiProvider,
        id: AiAccountDispositionId,
        terms: &AiAccountDispositionTerms,
        by: &UserId,
    ) -> Result<AiAccountDisposition, AiProviderAccountError> {
        let (target, user_id) = target_columns(terms.target);
        let updated = sqlx::query_as!(
            StoredDisposition,
            r#"
            UPDATE ai_provider_account_dispositions AS disposition
            SET target = $3,
                user_id = $4,
                valid_from = $5,
                valid_to = $6,
                login = account.login,
                updated_by = $7,
                updated_at = now()
            FROM ai_provider_accounts AS account
            WHERE disposition.provider = $1
              AND disposition.id = $2
              AND account.provider = disposition.provider
              AND account.account_id = disposition.account_id
            RETURNING disposition.id, disposition.provider, disposition.account_id,
                disposition.login, disposition.target, disposition.user_id,
                disposition.valid_from, disposition.valid_to, disposition.updated_at,
                disposition.updated_by
            "#,
            payer.as_str(),
            id.as_i32(),
            target,
            user_id,
            terms.period.valid_from(),
            terms.period.valid_to(),
            by.as_i32(),
        )
        .fetch_optional(&self.pool)
        .await;

        match updated {
            Ok(Some(row)) => row.try_into(),
            Ok(None) => Err(AiProviderAccountError::NotFound),
            Err(error) => Err(self
                .write_error(error, payer, AccountRef::Disposition(id), terms)
                .await),
        }
    }

    async fn delete_disposition(
        &self,
        payer: AiProvider,
        id: AiAccountDispositionId,
    ) -> Result<(), AiProviderAccountError> {
        let deleted = sqlx::query!(
            r#"
            DELETE FROM ai_provider_account_dispositions
            WHERE provider = $1 AND id = $2
            "#,
            payer.as_str(),
            id.as_i32(),
        )
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        if deleted.rows_affected() == 0 {
            return Err(AiProviderAccountError::NotFound);
        }
        Ok(())
    }

    async fn users(&self) -> Result<Vec<AiBillingDeveloper>, AiProviderAccountError> {
        let rows = sqlx::query!("SELECT id, full_name, email FROM users ORDER BY full_name, id")
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?;

        Ok(rows
            .into_iter()
            .map(|row| AiBillingDeveloper {
                user_id: UserId::new(row.id),
                full_name: row.full_name,
                email: row.email,
            })
            .collect())
    }

    async fn metered_usage(
        &self,
        payer: AiProvider,
        dates: AiUsageDateRange,
    ) -> Result<Vec<AiMeteredUserDay>, AiProviderAccountError> {
        // Days are UTC, the payer's billing days. Personal keys count: any
        // usage the payer paid for needs a linked account.
        let rows = sqlx::query!(
            r#"
            SELECT
                bucket.user_id,
                (bucket.hour_start AT TIME ZONE 'UTC')::date AS "day!",
                SUM(bucket.input_tokens)::int8 AS "input!",
                SUM(bucket.cache_read_tokens)::int8 AS "cache_read!",
                SUM(bucket.cache_write_tokens)::int8 AS "cache_write!",
                SUM(bucket.output_tokens)::int8 AS "output!",
                SUM(bucket.records)::int8 AS "records!",
                round(COALESCE(SUM(bucket.estimated_cost_usd), 0) * 1000000000)::int8
                    AS "priced_nanos!",
                SUM(bucket.unpriced_records)::int8 AS "unpriced!"
            FROM ai_usage_buckets AS bucket
            WHERE COALESCE(bucket.billing_provider, bucket.provider) = $1
              AND bucket.hour_start >= ($2::date::timestamp AT TIME ZONE 'UTC')
              AND bucket.hour_start < ($3::date::timestamp AT TIME ZONE 'UTC')
            GROUP BY 1, 2
            ORDER BY 1, 2
            "#,
            payer.as_str(),
            dates.start(),
            dates.end(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        rows.into_iter()
            .map(|row| {
                let usage = AiBillingUsage {
                    tokens: AiTokenCounts {
                        input: row.input,
                        cache_read: row.cache_read,
                        cache_write: row.cache_write,
                        output: row.output,
                    },
                    records: row.records,
                    priced_cost: AiUsdNanos::new(row.priced_nanos)
                        .ok_or_else(|| corrupt("a summed cost is negative"))?,
                    unpriced_records: row.unpriced,
                };
                usage
                    .ensure_reportable()
                    .map_err(|_| corrupt("metered usage exceeds the supported range"))?;
                Ok(AiMeteredUserDay {
                    user_id: UserId::new(row.user_id),
                    day: row.day,
                    usage,
                })
            })
            .collect()
    }
}

fn account_id(raw: i64) -> Result<AiProviderAccountId, AiProviderAccountError> {
    AiProviderAccountId::new(raw).ok_or_else(|| corrupt(format!("invalid account id {raw}")))
}

fn count(rows: u64) -> u32 {
    u32::try_from(rows).unwrap_or(u32::MAX)
}

fn corrupt(message: impl Into<String>) -> AiProviderAccountError {
    AiProviderAccountError::Storage(message.into())
}

fn storage_error(error: sqlx::Error) -> AiProviderAccountError {
    AiProviderAccountError::Storage(error.to_string())
}

#[cfg(test)]
mod tests;
