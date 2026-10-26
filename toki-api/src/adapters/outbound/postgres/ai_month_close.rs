use async_trait::async_trait;
use time::{Date, OffsetDateTime};

use crate::{
    db::{DbPool, DbTransaction},
    domain::{
        models::{
            AiAdjustedKind, AiAdjustmentKey, AiBilledEntry, AiBillingMonth, AiBillingSubject,
            AiCurrency, AiEntrySek, AiExchangeRateValue, AiKeyPayer, AiKeyProject, AiKeySubject,
            AiMappedProject, AiMonthClose, AiMonthCloseId, AiNewClose, AiProvider,
            AiStoredSnapshot, AiStoredSnapshotLine, AiSubscriptionId, AiUsageTimeZone, UserId,
            AI_BILL_SCOPE,
        },
        ports::outbound::{AiCloseLease, AiMonthCloseRepository},
        AiBillingError, AiMonthCloseError,
    },
};

/// The unique index that allows one active close per scope and month.
const ACTIVE_CLOSE_KEY: &str = "ai_billing_month_closes_active_key";

/// Month closes and their snapshots.
pub struct PostgresAiMonthCloseRepository {
    pool: DbPool,
}

impl PostgresAiMonthCloseRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

/// Holds the close lock for as long as its transaction is open.
struct PostgresCloseLease {
    transaction: DbTransaction,
}

#[async_trait]
impl AiCloseLease for PostgresCloseLease {
    async fn release(self: Box<Self>) -> Result<(), AiBillingError> {
        self.transaction.commit().await.map_err(storage_error)
    }
}

fn storage_error(error: sqlx::Error) -> AiBillingError {
    AiBillingError::Storage(error.to_string())
}

/// When the first open month of the AI bill starts: the earliest month
/// without an active close, at or after the month `configured_floor` falls in
/// (the earlier of its UTC and its `time_zone` month), from the earlier of
/// its UTC and its `time_zone` midnight, so both of its bases (UTC metered
/// months, local-day months) start at or after it. The month close, the usage
/// store and the metered usage store all answer with it, so repricing, the
/// bill, reconciliation and close readiness share one floor.
pub(super) async fn open_from(
    pool: &DbPool,
    time_zone: &AiUsageTimeZone,
    configured_floor: OffsetDateTime,
) -> Result<OffsetDateTime, sqlx::Error> {
    // The first open month is the floor's month itself or a month right after
    // an active close at or after it; the earliest such month without an
    // active close of its own.
    sqlx::query_scalar!(
        r#"
        WITH active AS (
            SELECT month
            FROM ai_billing_month_closes
            WHERE scope = $1 AND reopened_at IS NULL
        ), floor_month AS (
            SELECT LEAST(
                date_trunc('month', $3::timestamptz AT TIME ZONE 'UTC'),
                date_trunc('month', $3::timestamptz AT TIME ZONE $2::text)
            )::date AS month
        ), first_open AS (
            SELECT MIN(candidate.month) AS month
            FROM (
                SELECT month FROM floor_month
                UNION ALL
                SELECT (active.month + INTERVAL '1 month')::date
                FROM active, floor_month
                WHERE active.month >= floor_month.month
            ) AS candidate
            WHERE candidate.month NOT IN (SELECT month FROM active)
        )
        SELECT LEAST(
            first_open.month::timestamp AT TIME ZONE 'UTC',
            first_open.month::timestamp AT TIME ZONE $2::text
        ) AS "at!"
        FROM first_open
        "#,
        AI_BILL_SCOPE,
        time_zone.as_str(),
        configured_floor,
    )
    .fetch_one(pool)
    .await
}

fn corrupt(message: impl Into<String>) -> AiBillingError {
    AiBillingError::Storage(message.into())
}

fn month_of(day: Date) -> Result<AiBillingMonth, AiBillingError> {
    AiBillingMonth::containing(day)
        .filter(|month| month.first_day() == day)
        .ok_or_else(|| {
            corrupt(format!(
                "a stored close month is not a month's first day: {day}"
            ))
        })
}

fn position(value: i32) -> Result<u32, AiBillingError> {
    u32::try_from(value).map_err(|_| corrupt("a stored snapshot position is negative"))
}

/// A stored close with the names of who closed and re-opened it.
struct CloseRow {
    id: i64,
    month: Date,
    closed_at: OffsetDateTime,
    closed_by: Option<String>,
    reopened_at: Option<OffsetDateTime>,
    reopened_by: Option<String>,
}

impl CloseRow {
    fn into_close(self) -> Result<AiMonthClose, AiBillingError> {
        Ok(AiMonthClose {
            id: AiMonthCloseId::new(self.id),
            month: month_of(self.month)?,
            closed_at: self.closed_at,
            closed_by: self.closed_by,
            reopened_at: self.reopened_at,
            reopened_by: self.reopened_by,
        })
    }
}

/// A stored row's key columns, as the netting reads them.
struct EntryRow {
    subject: String,
    subject_label: Option<String>,
    project: String,
    project_name: Option<String>,
    kind: String,
    currency: String,
    provider: String,
    subscription_id: Option<i32>,
    amount: Option<i64>,
    sek_amount: Option<i64>,
    rate_millionths: Option<i64>,
    credits: Option<i64>,
}

impl EntryRow {
    fn into_entry(self) -> Result<AiBilledEntry, AiBillingError> {
        let subject = AiKeySubject::parse(&self.subject)
            .ok_or_else(|| corrupt(format!("corrupt snapshot subject {:?}", self.subject)))?;
        let project = AiKeyProject::parse(&self.project)
            .ok_or_else(|| corrupt(format!("corrupt snapshot project {:?}", self.project)))?;
        let kind = AiAdjustedKind::parse(&self.kind)
            .ok_or_else(|| corrupt(format!("corrupt snapshot kind {:?}", self.kind)))?;
        let currency = AiCurrency::parse(&self.currency)
            .map_err(|_| corrupt(format!("corrupt snapshot currency {:?}", self.currency)))?;
        let provider = AiProvider::parse(&self.provider)
            .ok_or_else(|| corrupt(format!("corrupt snapshot provider {:?}", self.provider)))?;
        let payer = match (kind.is_fee(), self.subscription_id) {
            (true, Some(id)) => AiKeyPayer::Subscription(AiSubscriptionId::new(id)),
            (false, None) => AiKeyPayer::Provider(provider),
            _ => {
                return Err(corrupt(
                    "a snapshot row's subscription does not fit its kind",
                ))
            }
        };
        let display = subject
            .subject(self.subject_label.as_deref())
            .ok_or_else(|| corrupt("a snapshot row's subject has no label"))?;
        let (mapped, unassigned) = match &project {
            AiKeyProject::Project(id) => (
                Some(AiMappedProject {
                    id: id.clone(),
                    name: self
                        .project_name
                        .clone()
                        .ok_or_else(|| corrupt("a snapshot row's project has no name"))?,
                }),
                None,
            ),
            AiKeyProject::Unassigned(kind) => (None, Some(*kind)),
            AiKeyProject::None => (None, None),
        };
        Ok(AiBilledEntry {
            key: AiAdjustmentKey {
                subject,
                project,
                kind,
                currency,
                payer,
            },
            subject: display,
            provider,
            project: mapped,
            unassigned,
            amount: self.amount,
            sek: self
                .sek_amount
                .map_or(AiEntrySek::Unknown, AiEntrySek::Known),
            rate: self
                .rate_millionths
                .map(|millionths| {
                    AiExchangeRateValue::from_millionths(millionths)
                        .ok_or_else(|| corrupt("a snapshot row's rate is out of range"))
                })
                .transpose()?,
            credits: self.credits,
        })
    }
}

#[async_trait]
impl AiMonthCloseRepository for PostgresAiMonthCloseRepository {
    async fn now(&self) -> Result<OffsetDateTime, AiBillingError> {
        sqlx::query_scalar!(r#"SELECT now() AS "now!""#)
            .fetch_one(&self.pool)
            .await
            .map_err(storage_error)
    }

    async fn open_from(
        &self,
        time_zone: &AiUsageTimeZone,
        configured_floor: OffsetDateTime,
    ) -> Result<OffsetDateTime, AiBillingError> {
        open_from(&self.pool, time_zone, configured_floor)
            .await
            .map_err(storage_error)
    }

    async fn try_lock(&self) -> Result<Option<Box<dyn AiCloseLease>>, AiBillingError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let locked = sqlx::query_scalar!(
            r#"
            SELECT pg_try_advisory_xact_lock(hashtextextended('ai_billing_month_close', 0))
                AS "locked!"
            "#,
        )
        .fetch_one(&mut transaction.executor())
        .await
        .map_err(storage_error)?;
        Ok(locked.then(|| Box::new(PostgresCloseLease { transaction }) as Box<dyn AiCloseLease>))
    }

    async fn active(&self) -> Result<Vec<AiMonthClose>, AiBillingError> {
        sqlx::query_as!(
            CloseRow,
            r#"
            SELECT
                close.id,
                close.month,
                close.closed_at,
                closer.full_name AS "closed_by?",
                close.reopened_at,
                reopener.full_name AS "reopened_by?"
            FROM ai_billing_month_closes AS close
            LEFT JOIN users AS closer ON closer.id = close.closed_by
            LEFT JOIN users AS reopener ON reopener.id = close.reopened_by
            WHERE close.scope = $1 AND close.reopened_at IS NULL
            ORDER BY close.month
            "#,
            AI_BILL_SCOPE,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(CloseRow::into_close)
        .collect()
    }

    async fn history(&self, month: AiBillingMonth) -> Result<Vec<AiMonthClose>, AiBillingError> {
        sqlx::query_as!(
            CloseRow,
            r#"
            SELECT
                close.id,
                close.month,
                close.closed_at,
                closer.full_name AS "closed_by?",
                close.reopened_at,
                reopener.full_name AS "reopened_by?"
            FROM ai_billing_month_closes AS close
            LEFT JOIN users AS closer ON closer.id = close.closed_by
            LEFT JOIN users AS reopener ON reopener.id = close.reopened_by
            WHERE close.scope = $1 AND close.month = $2
            ORDER BY close.closed_at DESC, close.id DESC
            "#,
            AI_BILL_SCOPE,
            month.first_day(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(CloseRow::into_close)
        .collect()
    }

    async fn reserve_id(&self) -> Result<AiMonthCloseId, AiBillingError> {
        let id = sqlx::query_scalar!(
            r#"SELECT nextval(pg_get_serial_sequence('ai_billing_month_closes', 'id')) AS "id!""#
        )
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(AiMonthCloseId::new(id))
    }

    async fn store(&self, close: &AiNewClose) -> Result<AiMonthClose, AiMonthCloseError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let stored = sqlx::query!(
            r#"
            INSERT INTO ai_billing_month_closes (
                id, month, scope, closed_by, readiness_json, bill_json, developer_json,
                csv_header, projection_version
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING closed_at
            "#,
            close.id.as_i64(),
            close.month.first_day(),
            AI_BILL_SCOPE,
            close.by.as_i32(),
            close.readiness_json,
            close.bill_json,
            close.developer_json,
            &close.csv_header,
            close.projection_version,
        )
        .fetch_one(&mut transaction.executor())
        .await;
        let closed_at = match stored {
            Ok(row) => row.closed_at,
            Err(error)
                if error
                    .as_database_error()
                    .and_then(|error| error.constraint())
                    == Some(ACTIVE_CLOSE_KEY) =>
            {
                return Err(AiMonthCloseError::AlreadyClosed { month: close.month });
            }
            Err(error) => return Err(storage_error(error).into()),
        };

        // The closes whose adjustments the snapshot holds must still be
        // active: a re-open in between would leave rows adjusting an open
        // month. The close lock keeps re-opens out; this guards the store.
        let mut adjusted: Vec<i64> = close
            .lines
            .iter()
            .filter_map(|line| line.adjusts.map(|adjusts| adjusts.close_id.as_i64()))
            .collect();
        adjusted.sort_unstable();
        adjusted.dedup();
        if !adjusted.is_empty() {
            let still_active = sqlx::query_scalar!(
                r#"
                SELECT id
                FROM ai_billing_month_closes
                WHERE id = ANY($1) AND reopened_at IS NULL
                FOR SHARE
                "#,
                &adjusted,
            )
            .fetch_all(&mut transaction.executor())
            .await
            .map_err(storage_error)?;
            if still_active.len() != adjusted.len() {
                return Err(AiMonthCloseError::Busy);
            }
        }

        for line in &close.lines {
            let entry = &line.entry;
            let (subject, project, kind, currency) = entry.key.columns();
            let label = match &entry.subject {
                AiBillingSubject::ProviderAccount { login, .. } => Some(login.as_str().to_string()),
                AiBillingSubject::Repository { name, .. } => Some(name.clone()),
                AiBillingSubject::User(_) | AiBillingSubject::UnknownAccount { .. } => None,
            };
            let position = i32::try_from(line.position)
                .map_err(|_| AiMonthCloseError::from(AiBillingError::NumericRange))?;
            let csv_position = i32::try_from(line.csv_position)
                .map_err(|_| AiMonthCloseError::from(AiBillingError::NumericRange))?;
            sqlx::query!(
                r#"
                INSERT INTO ai_billing_snapshot_lines (
                    close_id, position, csv_position, adjusts_month, adjusts_close_id,
                    subject, subject_label, project, project_name, kind, currency, provider,
                    subscription_id, amount, sek_amount, rate, credits, line_json, csv_row
                )
                VALUES (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                    $16::text::numeric, $17, $18, $19
                )
                "#,
                close.id.as_i64(),
                position,
                csv_position,
                line.adjusts.map(|adjusts| adjusts.month.first_day()),
                line.adjusts.map(|adjusts| adjusts.close_id.as_i64()),
                subject,
                label,
                project,
                entry.project.as_ref().map(|project| project.name.as_str()),
                kind,
                currency,
                entry.provider.as_str(),
                entry.key.subscription_id().map(|id| id.as_i32()),
                entry.amount,
                entry.sek.stored(),
                entry.rate.map(AiExchangeRateValue::to_decimal),
                entry.credits,
                line.line_json,
                &line.csv_row,
            )
            .execute(&mut transaction.executor())
            .await
            .map_err(storage_error)?;
        }
        transaction.commit().await.map_err(storage_error)?;

        let mut history = self.history(close.month).await?;
        history.retain(|stored| stored.id == close.id);
        history.pop().ok_or_else(|| {
            AiMonthCloseError::from(corrupt(format!(
                "the close of {} stored at {closed_at} cannot be read back",
                close.month
            )))
        })
    }

    async fn own_entries(
        &self,
        close: AiMonthCloseId,
    ) -> Result<Vec<AiBilledEntry>, AiBillingError> {
        sqlx::query_as!(
            EntryRow,
            r#"
            SELECT
                subject, subject_label, project, project_name, kind, currency, provider,
                subscription_id, amount, sek_amount,
                round(rate * 1000000)::int8 AS rate_millionths, credits
            FROM ai_billing_snapshot_lines
            WHERE close_id = $1 AND adjusts_month IS NULL
            ORDER BY position
            "#,
            close.as_i64(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(EntryRow::into_entry)
        .collect()
    }

    async fn frozen_adjustments(
        &self,
        month: AiBillingMonth,
    ) -> Result<Vec<AiBilledEntry>, AiBillingError> {
        sqlx::query_as!(
            EntryRow,
            r#"
            SELECT
                line.subject, line.subject_label, line.project, line.project_name, line.kind,
                line.currency, line.provider, line.subscription_id, line.amount,
                line.sek_amount, round(line.rate * 1000000)::int8 AS rate_millionths,
                line.credits
            FROM ai_billing_snapshot_lines AS line
            JOIN ai_billing_month_closes AS close ON close.id = line.close_id
            WHERE line.adjusts_month = $1
              AND close.scope = $2
              AND close.reopened_at IS NULL
            ORDER BY close.month, line.position
            "#,
            month.first_day(),
            AI_BILL_SCOPE,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(EntryRow::into_entry)
        .collect()
    }

    async fn months_adjusting(
        &self,
        month: AiBillingMonth,
    ) -> Result<Vec<AiBillingMonth>, AiBillingError> {
        sqlx::query_scalar!(
            r#"
            SELECT DISTINCT close.month
            FROM ai_billing_snapshot_lines AS line
            JOIN ai_billing_month_closes AS close ON close.id = line.close_id
            WHERE line.adjusts_month = $1
              AND close.scope = $2
              AND close.reopened_at IS NULL
            ORDER BY close.month
            "#,
            month.first_day(),
            AI_BILL_SCOPE,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(month_of)
        .collect()
    }

    async fn snapshot(&self, close: AiMonthCloseId) -> Result<AiStoredSnapshot, AiBillingError> {
        let stored = sqlx::query!(
            r#"
            SELECT bill_json, developer_json, readiness_json, csv_header, projection_version
            FROM ai_billing_month_closes
            WHERE id = $1
            "#,
            close.as_i64(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?
        .ok_or_else(|| corrupt(format!("close {} does not exist", close.as_i64())))?;
        let lines = sqlx::query!(
            r#"
            SELECT csv_position, adjusts_month, subject, line_json, csv_row
            FROM ai_billing_snapshot_lines
            WHERE close_id = $1
            ORDER BY position
            "#,
            close.as_i64(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(|row| {
            Ok(AiStoredSnapshotLine {
                csv_position: position(row.csv_position)?,
                adjusts: row.adjusts_month.map(month_of).transpose()?,
                subject: AiKeySubject::parse(&row.subject).ok_or_else(|| {
                    corrupt(format!("corrupt snapshot subject {:?}", row.subject))
                })?,
                line_json: row.line_json,
                csv_row: row.csv_row,
            })
        })
        .collect::<Result<Vec<_>, AiBillingError>>()?;
        Ok(AiStoredSnapshot {
            bill_json: stored.bill_json,
            developer_json: stored.developer_json,
            readiness_json: stored.readiness_json,
            csv_header: stored.csv_header,
            projection_version: stored.projection_version,
            lines,
        })
    }

    async fn reopen(&self, month: AiBillingMonth, by: &UserId) -> Result<(), AiMonthCloseError> {
        let reopened = sqlx::query_scalar!(
            r#"
            UPDATE ai_billing_month_closes
            SET reopened_at = GREATEST(now(), closed_at), reopened_by = $3
            WHERE scope = $1 AND month = $2 AND reopened_at IS NULL
            RETURNING id
            "#,
            AI_BILL_SCOPE,
            month.first_day(),
            by.as_i32(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        match reopened {
            Some(_) => Ok(()),
            None => Err(AiMonthCloseError::NotClosed { month }),
        }
    }
}

#[cfg(test)]
mod tests;
