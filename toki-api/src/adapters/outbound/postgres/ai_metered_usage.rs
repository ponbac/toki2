//! The imported usage of metered payers in Postgres.
//!
//! Decimals go in and out as text (`$n::text[]` cast to `numeric`, read as
//! `col::text`), so quantities stay exactly as the payer gave them. A slice is
//! replaced in one transaction: its rows are read `FOR UPDATE`, compared with
//! the fetched rows, and rewritten only when they differ, so an unchanged
//! re-import leaves every row as it was, `fetched_at` included. The slice's own
//! row records every fetch.
//!
//! - **The payer's content** is what the payer sent: keys, units, unit prices
//!   and amounts. Only a change in it changes the slice (`changed_at`,
//!   `late_changed_at` and with it the settle lag) and moves the rows'
//!   `fetched_at`.
//! - **Toki's own fields** (the normalized model, auto-routing, the kind and the
//!   requesting login) are rewritten in place when they differ, as after a new
//!   label override or a login rename, without changing the slice or the rows'
//!   `fetched_at`.

use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use time::{Date, OffsetDateTime};

use super::ai_usage_reads::{stored_mappings, stored_personal_keys};
use crate::{
    db::{DbPool, DbTransaction},
    domain::{
        models::{
            slice_end, AiBillingMonth, AiBillingProject, AiCreditKey, AiCreditKind,
            AiCreditStoreScope, AiCreditTotal, AiDecimal, AiImportHistory, AiImportKind,
            AiImportPlan, AiImportRun, AiImportRunId, AiImportStatus, AiMappedProject,
            AiMappingResolver, AiMeteredAmounts, AiMeteredCredit, AiMeteredDestination,
            AiMeteredModel, AiMeteredOrgCharge, AiMeteredSource, AiMeteredSummaryLine, AiMonthMode,
            AiOrgChargeKind, AiProvider, AiProviderAccountId, AiRunStatus, AiSliceRecord,
            AiSourceOutcome, AiSourceState, AiSourceStatus, AiStoredAccountCredit,
            AiStoredAccountError, AiStoredOrgCredit, AiStoredRepositoryMapping, AiStoredScope,
            AiSummaryKind, AiTokenCounts, AiTrackedModelDay, AiUnassignedKind, AiUsageCheck,
            AiUsageDateRange, AiUsageTimeZone, AiUsdNanos, ProjectId, TimeTrackingCompany, UserId,
            UNATTRIBUTED_PROJECT_KEY,
        },
        ports::outbound::{AiImportLease, AiMeteredUsageRepository},
        AiMeteredUsageError,
    },
};

/// The account id of organization slices and checks.
const ORGANIZATION: i64 = 0;

pub struct PostgresAiMeteredUsageRepository {
    pool: DbPool,
}

impl PostgresAiMeteredUsageRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

/// The import lock: a transaction-level advisory lock, held by the open
/// transaction until it is released or dropped.
struct PostgresImportLease {
    transaction: DbTransaction,
}

#[async_trait]
impl AiImportLease for PostgresImportLease {
    async fn release(self: Box<Self>) -> Result<(), AiMeteredUsageError> {
        self.transaction.commit().await.map_err(storage_error)
    }
}

fn storage_error(error: sqlx::Error) -> AiMeteredUsageError {
    AiMeteredUsageError::Storage(error.to_string())
}

fn corrupt(message: impl Into<String>) -> AiMeteredUsageError {
    AiMeteredUsageError::Storage(message.into())
}

fn decimal(raw: &str) -> Result<AiDecimal, AiMeteredUsageError> {
    AiDecimal::parse(raw).ok_or_else(|| corrupt(format!("corrupt stored decimal {raw:?}")))
}

fn nanos(raw: i64) -> Result<AiUsdNanos, AiMeteredUsageError> {
    AiUsdNanos::new(raw).ok_or_else(|| corrupt("a stored amount is negative"))
}

fn account_id(raw: i64) -> Result<AiProviderAccountId, AiMeteredUsageError> {
    AiProviderAccountId::new(raw).ok_or_else(|| corrupt(format!("invalid account id {raw}")))
}

fn count(value: i32) -> u32 {
    u32::try_from(value).unwrap_or(0)
}

fn column(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn month_of(day: Date) -> Result<AiBillingMonth, AiMeteredUsageError> {
    AiBillingMonth::containing(day).ok_or_else(|| corrupt(format!("invalid month {day}")))
}

fn scope_id(scope: AiStoredScope) -> i64 {
    scope
        .account_id()
        .map_or(ORGANIZATION, AiProviderAccountId::as_i64)
}

/// How many fetched days changed.
fn changed_count(changed: &[bool]) -> u32 {
    u32::try_from(changed.iter().filter(|changed| **changed).count()).unwrap_or(u32::MAX)
}

fn slice_ends(
    source: AiMeteredSource,
    days: &[Date],
) -> Result<Vec<OffsetDateTime>, AiMeteredUsageError> {
    days.iter()
        .map(|day| slice_end(source, *day).ok_or_else(|| corrupt(format!("no end for {day}"))))
        .collect()
}

/// Records one fetch of each of `days` of a source and account: inserted on
/// the first fetch, otherwise updated, with `changed[i]` telling whether the
/// fetch found other content.
async fn record_slices(
    transaction: &mut DbTransaction,
    payer: AiProvider,
    source: AiMeteredSource,
    account: i64,
    days: &[Date],
    changed: &[bool],
    fetched_at: OffsetDateTime,
) -> Result<(), AiMeteredUsageError> {
    if days.is_empty() {
        return Ok(());
    }
    let ends = slice_ends(source, days)?;
    sqlx::query!(
        r#"
        UPDATE ai_provider_usage_slices AS slice
        SET fetched_at = GREATEST(slice.fetched_at, $4),
            changed_at = CASE WHEN input.changed THEN GREATEST(slice.fetched_at, $4)
                              ELSE slice.changed_at END,
            late_changed_at = CASE WHEN input.changed AND slice.fetched_at >= input.ends THEN $4
                                   ELSE slice.late_changed_at END,
            complete_at = COALESCE(slice.complete_at, CASE WHEN $4 >= input.ends THEN $4 END)
        FROM UNNEST($5::date[], $6::timestamptz[], $7::bool[]) AS input(day, ends, changed)
        WHERE slice.provider = $1
          AND slice.source = $2
          AND slice.account_id = $3
          AND slice.day = input.day
        "#,
        payer.as_str(),
        source.as_str(),
        account,
        fetched_at,
        days,
        &ends,
        changed,
    )
    .execute(&mut transaction.executor())
    .await
    .map_err(storage_error)?;
    sqlx::query!(
        r#"
        INSERT INTO ai_provider_usage_slices
            (provider, source, account_id, day, fetched_at, changed_at, complete_at)
        SELECT $1, $2, $3, input.day, $4, $4, CASE WHEN $4 >= input.ends THEN $4 END
        FROM UNNEST($5::date[], $6::timestamptz[]) AS input(day, ends)
        ON CONFLICT (provider, source, account_id, day) DO NOTHING
        "#,
        payer.as_str(),
        source.as_str(),
        account,
        fetched_at,
        days,
        &ends,
    )
    .execute(&mut transaction.executor())
    .await
    .map_err(storage_error)?;
    Ok(())
}

/// Credits rows in their stored form, by key, for comparing content.
type CreditContent = BTreeMap<AiCreditKey, (AiMeteredCredit, String)>;

fn credit_content<'a>(
    rows: impl IntoIterator<Item = (&'a AiMeteredCredit, &'a str)>,
) -> CreditContent {
    rows.into_iter()
        .map(|(credit, login)| (credit.key(), (credit.clone(), login.to_string())))
        .collect()
}

/// What the payer sent of credits rows, by key: units and amounts, without
/// Toki's own fields.
fn credit_payer_content<'a>(
    rows: impl IntoIterator<Item = &'a AiMeteredCredit>,
) -> BTreeMap<AiCreditKey, (String, AiMeteredAmounts)> {
    rows.into_iter()
        .map(|credit| (credit.key(), (credit.unit.clone(), credit.amounts)))
        .collect()
}

/// The `fetched_at` of rewritten rows: the fetch when the payer's content
/// changed, otherwise the stored rows' own.
fn rewritten_at(
    payer_changed: bool,
    fetched_at: OffsetDateTime,
    stored: impl IntoIterator<Item = OffsetDateTime>,
) -> OffsetDateTime {
    match payer_changed {
        true => fetched_at,
        false => stored.into_iter().max().unwrap_or(fetched_at),
    }
}

/// A stored credits row with its day, login and fetch.
struct StoredCredit {
    day: Date,
    login: String,
    credit: AiMeteredCredit,
    fetched_at: OffsetDateTime,
}

#[allow(clippy::too_many_arguments)]
fn stored_credit(
    day: Date,
    product: String,
    sku: String,
    label: String,
    model: String,
    auto_routed: bool,
    kind: &str,
    unit: String,
    amounts: [&str; 7],
) -> Result<AiMeteredCredit, AiMeteredUsageError> {
    let [unit_price, gross_quantity, gross_amount, discount_quantity, discount_amount, net_quantity, net_amount] =
        amounts;
    Ok(AiMeteredCredit {
        product,
        sku,
        unit,
        model: AiMeteredModel {
            label,
            model,
            auto_routed,
        },
        kind: AiCreditKind::parse(kind)
            .ok_or_else(|| corrupt(format!("corrupt credit kind {kind:?} on {day}")))?,
        amounts: AiMeteredAmounts {
            unit_price: decimal(unit_price)?,
            gross_quantity: decimal(gross_quantity)?,
            gross_amount: decimal(gross_amount)?,
            discount_quantity: decimal(discount_quantity)?,
            discount_amount: decimal(discount_amount)?,
            net_quantity: decimal(net_quantity)?,
            net_amount: decimal(net_amount)?,
        },
    })
}

/// Column arrays of credits rows to insert.
#[derive(Default)]
struct CreditColumns {
    days: Vec<Date>,
    products: Vec<String>,
    skus: Vec<String>,
    labels: Vec<String>,
    models: Vec<String>,
    autos: Vec<bool>,
    kinds: Vec<String>,
    units: Vec<String>,
    unit_prices: Vec<String>,
    gross_quantities: Vec<String>,
    gross_amounts: Vec<String>,
    discount_quantities: Vec<String>,
    discount_amounts: Vec<String>,
    net_quantities: Vec<String>,
    net_amounts: Vec<String>,
    gross_usd: Vec<i64>,
    fetched_ats: Vec<OffsetDateTime>,
}

impl CreditColumns {
    fn push(
        &mut self,
        day: Date,
        credit: &AiMeteredCredit,
        fetched_at: OffsetDateTime,
    ) -> Result<(), AiMeteredUsageError> {
        let gross_usd = credit.gross_usd().ok_or_else(|| {
            AiMeteredUsageError::Invalid(format!(
                "the gross quantity of {:?} on {day} is out of range",
                credit.model.label
            ))
        })?;
        let amounts = &credit.amounts;
        self.days.push(day);
        self.products.push(credit.product.clone());
        self.skus.push(credit.sku.clone());
        self.labels.push(credit.model.label.clone());
        self.models.push(credit.model.model.clone());
        self.autos.push(credit.model.auto_routed);
        self.kinds.push(credit.kind.as_str().to_string());
        self.units.push(credit.unit.clone());
        self.unit_prices.push(amounts.unit_price.to_string());
        self.gross_quantities
            .push(amounts.gross_quantity.to_string());
        self.gross_amounts.push(amounts.gross_amount.to_string());
        self.discount_quantities
            .push(amounts.discount_quantity.to_string());
        self.discount_amounts
            .push(amounts.discount_amount.to_string());
        self.net_quantities.push(amounts.net_quantity.to_string());
        self.net_amounts.push(amounts.net_amount.to_string());
        self.gross_usd.push(gross_usd.nanos());
        self.fetched_ats.push(fetched_at);
        Ok(())
    }
}

/// Organization usage rows in their stored form, by key, for comparing
/// content. The reported instant is not content.
type ChargeKey = (Date, String, String, String, String);

fn charge_key(row: &AiMeteredOrgCharge) -> ChargeKey {
    (
        row.day,
        row.product.clone(),
        row.sku.clone(),
        row.unit.clone(),
        row.repository.clone(),
    )
}

/// What the payer sent of organization usage rows, by key: the unit price and
/// the amounts, without Toki's kind.
fn charge_payer_content<'a>(
    rows: impl IntoIterator<Item = &'a AiMeteredOrgCharge>,
) -> BTreeMap<ChargeKey, [AiDecimal; 5]> {
    rows.into_iter()
        .map(|row| {
            (
                charge_key(row),
                [
                    row.unit_price,
                    row.quantity,
                    row.gross_amount,
                    row.discount_amount,
                    row.net_amount,
                ],
            )
        })
        .collect()
}

fn charge_content<'a>(
    rows: impl IntoIterator<Item = &'a AiMeteredOrgCharge>,
) -> BTreeMap<ChargeKey, AiMeteredOrgCharge> {
    rows.into_iter()
        .map(|row| {
            (
                charge_key(row),
                AiMeteredOrgCharge {
                    reported_at: None,
                    ..row.clone()
                },
            )
        })
        .collect()
}

fn summary_content<'a>(
    rows: impl IntoIterator<Item = &'a AiMeteredSummaryLine>,
) -> BTreeMap<(String, String, String), AiMeteredSummaryLine> {
    rows.into_iter()
        .map(|row| {
            (
                (row.product.clone(), row.sku.clone(), row.unit.clone()),
                row.clone(),
            )
        })
        .collect()
}

/// What the payer sent of a summary, by key: the amounts, without Toki's kind.
fn summary_payer_content<'a>(
    rows: impl IntoIterator<Item = &'a AiMeteredSummaryLine>,
) -> BTreeMap<(String, String, String), AiMeteredAmounts> {
    rows.into_iter()
        .map(|row| {
            (
                (row.product.clone(), row.sku.clone(), row.unit.clone()),
                row.amounts,
            )
        })
        .collect()
}

/// A stored run row.
struct RunRow {
    id: i64,
    kind: String,
    requested_by: Option<i32>,
    months: Vec<Date>,
    full_months: Vec<Date>,
    started_at: OffsetDateTime,
    finished_at: Option<OffsetDateTime>,
    status: String,
    requests: i32,
    slices_fetched: i32,
    slices_changed: i32,
    account_errors: i32,
    error: Option<String>,
}

impl RunRow {
    fn into_run(self, sources: Vec<AiSourceOutcome>) -> Result<AiImportRun, AiMeteredUsageError> {
        let id = self.id;
        let kind = AiImportKind::parse(&self.kind, self.requested_by.map(UserId::new))
            .ok_or_else(|| corrupt(format!("run {id} has a corrupt kind")))?;
        let months = self
            .months
            .iter()
            .map(|day| {
                let mode = if self.full_months.contains(day) {
                    AiMonthMode::Full
                } else {
                    AiMonthMode::Changes
                };
                Ok((month_of(*day)?, mode))
            })
            .collect::<Result<Vec<_>, AiMeteredUsageError>>()?;
        Ok(AiImportRun {
            id: AiImportRunId::new(id),
            kind,
            months,
            started_at: self.started_at,
            finished_at: self.finished_at,
            status: AiRunStatus::parse(&self.status)
                .ok_or_else(|| corrupt(format!("run {id} has a corrupt status")))?,
            requests: count(self.requests),
            slices_fetched: count(self.slices_fetched),
            slices_changed: count(self.slices_changed),
            account_errors: count(self.account_errors),
            error: self.error,
            sources,
        })
    }
}

/// A stored run-source row.
struct RunSourceRow {
    source: String,
    status: String,
    requests: i32,
    slices_fetched: i32,
    slices_changed: i32,
    imported_through: Option<Date>,
    settle_lag_days: Option<i32>,
    error: Option<String>,
}

impl RunSourceRow {
    fn into_outcome(self) -> Result<AiSourceOutcome, AiMeteredUsageError> {
        Ok(AiSourceOutcome {
            source: AiMeteredSource::parse(&self.source)
                .ok_or_else(|| corrupt("corrupt run source"))?,
            status: AiSourceStatus::parse(&self.status)
                .ok_or_else(|| corrupt("corrupt run source status"))?,
            requests: count(self.requests),
            slices_fetched: count(self.slices_fetched),
            slices_changed: count(self.slices_changed),
            imported_through: self.imported_through,
            settle_lag_days: self
                .settle_lag_days
                .map(|days| u32::try_from(days).unwrap_or(0)),
            error: self.error,
        })
    }
}

impl PostgresAiMeteredUsageRepository {
    async fn run_sources(&self, run_id: i64) -> Result<Vec<AiSourceOutcome>, AiMeteredUsageError> {
        sqlx::query_as!(
            RunSourceRow,
            r#"
            SELECT source, status, requests, slices_fetched, slices_changed, imported_through,
                settle_lag_days, error
            FROM ai_provider_import_run_sources
            WHERE run_id = $1
            ORDER BY source
            "#,
            run_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(RunSourceRow::into_outcome)
        .collect()
    }

    /// The stored credits rows of `scope` on `days`, locked for update.
    async fn locked_credits(
        transaction: &mut DbTransaction,
        payer: AiProvider,
        scope: AiStoredScope,
        days: &[Date],
    ) -> Result<Vec<StoredCredit>, AiMeteredUsageError> {
        match scope {
            AiStoredScope::Account(account) => sqlx::query!(
                r#"
                SELECT day, product, sku, model_label, model, auto_routed, kind, unit_type,
                    unit_price::text AS "unit_price!", gross_quantity::text AS "gross_quantity!",
                    gross_amount::text AS "gross_amount!",
                    discount_quantity::text AS "discount_quantity!",
                    discount_amount::text AS "discount_amount!",
                    net_quantity::text AS "net_quantity!", net_amount::text AS "net_amount!",
                    login, fetched_at
                FROM ai_provider_account_usage
                WHERE provider = $1 AND account_id = $2 AND day = ANY($3::date[])
                FOR UPDATE
                "#,
                payer.as_str(),
                account.as_i64(),
                days,
            )
            .fetch_all(&mut transaction.executor())
            .await
            .map_err(storage_error)?
            .into_iter()
            .map(|row| {
                Ok(StoredCredit {
                    day: row.day,
                    credit: stored_credit(
                        row.day,
                        row.product,
                        row.sku,
                        row.model_label,
                        row.model,
                        row.auto_routed,
                        &row.kind,
                        row.unit_type,
                        [
                            &row.unit_price,
                            &row.gross_quantity,
                            &row.gross_amount,
                            &row.discount_quantity,
                            &row.discount_amount,
                            &row.net_quantity,
                            &row.net_amount,
                        ],
                    )?,
                    login: row.login,
                    fetched_at: row.fetched_at,
                })
            })
            .collect(),
            AiStoredScope::Organization => sqlx::query!(
                r#"
                SELECT day, product, sku, model_label, model, auto_routed, kind, unit_type,
                    unit_price::text AS "unit_price!", gross_quantity::text AS "gross_quantity!",
                    gross_amount::text AS "gross_amount!",
                    discount_quantity::text AS "discount_quantity!",
                    discount_amount::text AS "discount_amount!",
                    net_quantity::text AS "net_quantity!", net_amount::text AS "net_amount!",
                    fetched_at
                FROM ai_provider_org_credits
                WHERE provider = $1 AND day = ANY($2::date[])
                FOR UPDATE
                "#,
                payer.as_str(),
                days,
            )
            .fetch_all(&mut transaction.executor())
            .await
            .map_err(storage_error)?
            .into_iter()
            .map(|row| {
                Ok(StoredCredit {
                    day: row.day,
                    credit: stored_credit(
                        row.day,
                        row.product,
                        row.sku,
                        row.model_label,
                        row.model,
                        row.auto_routed,
                        &row.kind,
                        row.unit_type,
                        [
                            &row.unit_price,
                            &row.gross_quantity,
                            &row.gross_amount,
                            &row.discount_quantity,
                            &row.discount_amount,
                            &row.net_quantity,
                            &row.net_amount,
                        ],
                    )?,
                    login: String::new(),
                    fetched_at: row.fetched_at,
                })
            })
            .collect(),
        }
    }

    /// Org usage rows on `days`, read with their reported instants.
    async fn charges(
        &self,
        payer: AiProvider,
        dates: AiUsageDateRange,
    ) -> Result<Vec<AiMeteredOrgCharge>, AiMeteredUsageError> {
        sqlx::query!(
            r#"
            SELECT day, product, sku, unit_type, repository, kind,
                unit_price::text AS "unit_price!", quantity::text AS "quantity!",
                gross_amount::text AS "gross_amount!", discount_amount::text AS "discount_amount!",
                net_amount::text AS "net_amount!", reported_at
            FROM ai_provider_org_usage
            WHERE provider = $1 AND day >= $2 AND day < $3
            ORDER BY day, product, sku, unit_type, repository
            "#,
            payer.as_str(),
            dates.start(),
            dates.end(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(|row| {
            Ok(AiMeteredOrgCharge {
                day: row.day,
                kind: AiOrgChargeKind::parse(&row.kind)
                    .ok_or_else(|| corrupt(format!("corrupt usage kind {:?}", row.kind)))?,
                product: row.product,
                sku: row.sku,
                unit: row.unit_type,
                repository: row.repository,
                unit_price: decimal(&row.unit_price)?,
                quantity: decimal(&row.quantity)?,
                gross_amount: decimal(&row.gross_amount)?,
                discount_amount: decimal(&row.discount_amount)?,
                net_amount: decimal(&row.net_amount)?,
                reported_at: row.reported_at,
            })
        })
        .collect()
    }

    /// A month's summary lines, each with the fetch whose content it is.
    async fn summary_rows(
        executor: &mut DbTransaction,
        payer: AiProvider,
        month: Date,
    ) -> Result<Vec<(AiMeteredSummaryLine, OffsetDateTime)>, AiMeteredUsageError> {
        sqlx::query!(
            r#"
            SELECT product, sku, unit_type, kind, unit_price::text AS "unit_price!",
                gross_quantity::text AS "gross_quantity!", gross_amount::text AS "gross_amount!",
                discount_quantity::text AS "discount_quantity!",
                discount_amount::text AS "discount_amount!",
                net_quantity::text AS "net_quantity!", net_amount::text AS "net_amount!",
                fetched_at
            FROM ai_provider_usage_summaries
            WHERE provider = $1 AND month = $2
            ORDER BY product, sku, unit_type
            "#,
            payer.as_str(),
            month,
        )
        .fetch_all(&mut executor.executor())
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(|row| {
            let line = AiMeteredSummaryLine {
                kind: AiSummaryKind::parse(&row.kind)
                    .ok_or_else(|| corrupt(format!("corrupt summary kind {:?}", row.kind)))?,
                product: row.product,
                sku: row.sku,
                unit: row.unit_type,
                amounts: AiMeteredAmounts {
                    unit_price: decimal(&row.unit_price)?,
                    gross_quantity: decimal(&row.gross_quantity)?,
                    gross_amount: decimal(&row.gross_amount)?,
                    discount_quantity: decimal(&row.discount_quantity)?,
                    discount_amount: decimal(&row.discount_amount)?,
                    net_quantity: decimal(&row.net_quantity)?,
                    net_amount: decimal(&row.net_amount)?,
                },
            };
            Ok((line, row.fetched_at))
        })
        .collect()
    }
}

#[async_trait]
impl AiMeteredUsageRepository for PostgresAiMeteredUsageRepository {
    async fn now(&self) -> Result<OffsetDateTime, AiMeteredUsageError> {
        sqlx::query_scalar!(r#"SELECT now() AS "now!""#)
            .fetch_one(&self.pool)
            .await
            .map_err(storage_error)
    }

    async fn open_from(
        &self,
        time_zone: &AiUsageTimeZone,
        configured_floor: OffsetDateTime,
    ) -> Result<OffsetDateTime, AiMeteredUsageError> {
        super::ai_month_close::open_from(&self.pool, time_zone, configured_floor)
            .await
            .map_err(storage_error)
    }

    async fn try_lock_import(
        &self,
        payer: AiProvider,
    ) -> Result<Option<Box<dyn AiImportLease>>, AiMeteredUsageError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let locked = sqlx::query_scalar!(
            r#"
            SELECT pg_try_advisory_xact_lock(hashtextextended('ai_provider_usage_import:' || $1, 0))
                AS "locked!"
            "#,
            payer.as_str(),
        )
        .fetch_one(&mut transaction.executor())
        .await
        .map_err(storage_error)?;
        Ok(locked.then(|| Box::new(PostgresImportLease { transaction }) as Box<dyn AiImportLease>))
    }

    async fn slices(
        &self,
        payer: AiProvider,
        source: AiMeteredSource,
        dates: AiUsageDateRange,
    ) -> Result<Vec<AiSliceRecord>, AiMeteredUsageError> {
        sqlx::query!(
            r#"
            SELECT account_id, day, fetched_at, changed_at, complete_at, late_changed_at
            FROM ai_provider_usage_slices
            WHERE provider = $1 AND source = $2 AND day >= $3 AND day < $4
            ORDER BY account_id, day
            "#,
            payer.as_str(),
            source.as_str(),
            dates.start(),
            dates.end(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(|row| {
            Ok(AiSliceRecord {
                source,
                account_id: match row.account_id {
                    ORGANIZATION => None,
                    id => Some(account_id(id)?),
                },
                day: row.day,
                fetched_at: row.fetched_at,
                changed_at: row.changed_at,
                complete_at: row.complete_at,
                late_changed_at: row.late_changed_at,
            })
        })
        .collect()
    }

    async fn credit_month_totals(
        &self,
        payer: AiProvider,
        scope: AiStoredScope,
        month: AiBillingMonth,
    ) -> Result<Vec<AiCreditTotal>, AiMeteredUsageError> {
        let dates = month.dates();
        let rows: Vec<(String, String, String, String)> = match scope {
            AiStoredScope::Account(account) => sqlx::query!(
                r#"
                SELECT product, sku, model_label, SUM(gross_quantity)::text AS "gross!"
                FROM ai_provider_account_usage
                WHERE provider = $1 AND account_id = $2 AND day >= $3 AND day < $4
                GROUP BY product, sku, model_label
                ORDER BY product, sku, model_label
                "#,
                payer.as_str(),
                account.as_i64(),
                dates.start(),
                dates.end(),
            )
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?
            .into_iter()
            .map(|row| (row.product, row.sku, row.model_label, row.gross))
            .collect(),
            AiStoredScope::Organization => sqlx::query!(
                r#"
                SELECT product, sku, model_label, SUM(gross_quantity)::text AS "gross!"
                FROM ai_provider_org_credits
                WHERE provider = $1 AND day >= $2 AND day < $3
                GROUP BY product, sku, model_label
                ORDER BY product, sku, model_label
                "#,
                payer.as_str(),
                dates.start(),
                dates.end(),
            )
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?
            .into_iter()
            .map(|row| (row.product, row.sku, row.model_label, row.gross))
            .collect(),
        };
        rows.into_iter()
            .map(|(product, sku, label, gross)| {
                Ok(AiCreditTotal {
                    key: AiCreditKey {
                        product,
                        sku,
                        label,
                    },
                    gross_quantity: decimal(&gross)?,
                })
            })
            .collect()
    }

    async fn replace_credit_days(
        &self,
        payer: AiProvider,
        scope: AiCreditStoreScope<'_>,
        days: &[(Date, Vec<AiMeteredCredit>)],
        fetched_at: OffsetDateTime,
    ) -> Result<u32, AiMeteredUsageError> {
        if days.is_empty() {
            return Ok(0);
        }
        let stored_scope = scope.stored();
        let (source, login) = match scope {
            AiCreditStoreScope::Account { login, .. } => {
                (AiMeteredSource::AccountCredits, login.as_str())
            }
            AiCreditStoreScope::Organization => (AiMeteredSource::OrgCredits, ""),
        };
        let day_list: Vec<Date> = days.iter().map(|(day, _)| *day).collect();

        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let stored = Self::locked_credits(&mut transaction, payer, stored_scope, &day_list).await?;
        let mut stored_by_day: HashMap<Date, Vec<&StoredCredit>> = HashMap::new();
        for row in &stored {
            stored_by_day.entry(row.day).or_default().push(row);
        }

        let mut changed = Vec::with_capacity(days.len());
        let mut columns = CreditColumns::default();
        let mut rewritten_days = Vec::new();
        for (day, rows) in days {
            let stored_rows = stored_by_day
                .get(day)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let payer_changed = credit_payer_content(rows)
                != credit_payer_content(stored_rows.iter().map(|row| &row.credit));
            changed.push(payer_changed);
            let fetched = credit_content(rows.iter().map(|row| (row, login)));
            let previous = credit_content(
                stored_rows
                    .iter()
                    .map(|row| (&row.credit, row.login.as_str())),
            );
            if fetched != previous {
                rewritten_days.push(*day);
                let rows_fetched_at = rewritten_at(
                    payer_changed,
                    fetched_at,
                    stored_rows.iter().map(|row| row.fetched_at),
                );
                for row in fetched.values() {
                    columns.push(*day, &row.0, rows_fetched_at)?;
                }
            }
        }

        if !rewritten_days.is_empty() {
            match scope {
                AiCreditStoreScope::Account { id, login } => {
                    sqlx::query!(
                        r#"
                        DELETE FROM ai_provider_account_usage
                        WHERE provider = $1 AND account_id = $2 AND day = ANY($3::date[])
                        "#,
                        payer.as_str(),
                        id.as_i64(),
                        &rewritten_days,
                    )
                    .execute(&mut transaction.executor())
                    .await
                    .map_err(storage_error)?;
                    sqlx::query!(
                        r#"
                        INSERT INTO ai_provider_account_usage (
                            provider, account_id, day, product, sku, model_label, model,
                            auto_routed, kind, unit_type, unit_price, gross_quantity, gross_amount,
                            discount_quantity, discount_amount, net_quantity, net_amount,
                            gross_usd_nanos, login, fetched_at
                        )
                        SELECT $1, $2, row.day, row.product, row.sku, row.label, row.model,
                            row.auto, row.kind, row.unit, row.unit_price::numeric,
                            row.gross_quantity::numeric, row.gross_amount::numeric,
                            row.discount_quantity::numeric, row.discount_amount::numeric,
                            row.net_quantity::numeric, row.net_amount::numeric, row.gross_usd,
                            $3, row.fetched_at
                        FROM UNNEST(
                            $4::date[], $5::text[], $6::text[], $7::text[], $8::text[],
                            $9::bool[], $10::text[], $11::text[], $12::text[], $13::text[],
                            $14::text[], $15::text[], $16::text[], $17::text[], $18::text[],
                            $19::int8[], $20::timestamptz[]
                        ) AS row(
                            day, product, sku, label, model, auto, kind, unit, unit_price,
                            gross_quantity, gross_amount, discount_quantity, discount_amount,
                            net_quantity, net_amount, gross_usd, fetched_at
                        )
                        "#,
                        payer.as_str(),
                        id.as_i64(),
                        login.as_str(),
                        &columns.days,
                        &columns.products,
                        &columns.skus,
                        &columns.labels,
                        &columns.models,
                        &columns.autos,
                        &columns.kinds,
                        &columns.units,
                        &columns.unit_prices,
                        &columns.gross_quantities,
                        &columns.gross_amounts,
                        &columns.discount_quantities,
                        &columns.discount_amounts,
                        &columns.net_quantities,
                        &columns.net_amounts,
                        &columns.gross_usd,
                        &columns.fetched_ats,
                    )
                    .execute(&mut transaction.executor())
                    .await
                    .map_err(storage_error)?;
                }
                AiCreditStoreScope::Organization => {
                    sqlx::query!(
                        r#"
                        DELETE FROM ai_provider_org_credits
                        WHERE provider = $1 AND day = ANY($2::date[])
                        "#,
                        payer.as_str(),
                        &rewritten_days,
                    )
                    .execute(&mut transaction.executor())
                    .await
                    .map_err(storage_error)?;
                    sqlx::query!(
                        r#"
                        INSERT INTO ai_provider_org_credits (
                            provider, day, product, sku, model_label, model, auto_routed, kind,
                            unit_type, unit_price, gross_quantity, gross_amount, discount_quantity,
                            discount_amount, net_quantity, net_amount, gross_usd_nanos, fetched_at
                        )
                        SELECT $1, row.day, row.product, row.sku, row.label, row.model, row.auto,
                            row.kind, row.unit, row.unit_price::numeric,
                            row.gross_quantity::numeric, row.gross_amount::numeric,
                            row.discount_quantity::numeric, row.discount_amount::numeric,
                            row.net_quantity::numeric, row.net_amount::numeric, row.gross_usd,
                            row.fetched_at
                        FROM UNNEST(
                            $2::date[], $3::text[], $4::text[], $5::text[], $6::text[],
                            $7::bool[], $8::text[], $9::text[], $10::text[], $11::text[],
                            $12::text[], $13::text[], $14::text[], $15::text[], $16::text[],
                            $17::int8[], $18::timestamptz[]
                        ) AS row(
                            day, product, sku, label, model, auto, kind, unit, unit_price,
                            gross_quantity, gross_amount, discount_quantity, discount_amount,
                            net_quantity, net_amount, gross_usd, fetched_at
                        )
                        "#,
                        payer.as_str(),
                        &columns.days,
                        &columns.products,
                        &columns.skus,
                        &columns.labels,
                        &columns.models,
                        &columns.autos,
                        &columns.kinds,
                        &columns.units,
                        &columns.unit_prices,
                        &columns.gross_quantities,
                        &columns.gross_amounts,
                        &columns.discount_quantities,
                        &columns.discount_amounts,
                        &columns.net_quantities,
                        &columns.net_amounts,
                        &columns.gross_usd,
                        &columns.fetched_ats,
                    )
                    .execute(&mut transaction.executor())
                    .await
                    .map_err(storage_error)?;
                }
            }
        }

        record_slices(
            &mut transaction,
            payer,
            source,
            scope_id(stored_scope),
            &day_list,
            &changed,
            fetched_at,
        )
        .await?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(changed_count(&changed))
    }

    async fn replace_org_usage(
        &self,
        payer: AiProvider,
        days: AiUsageDateRange,
        rows: &[AiMeteredOrgCharge],
        fetched_at: OffsetDateTime,
    ) -> Result<u32, AiMeteredUsageError> {
        let day_list: Vec<Date> = {
            let mut list = Vec::new();
            let mut day = days.start();
            while day < days.end() {
                list.push(day);
                day = day
                    .next_day()
                    .ok_or_else(|| corrupt("a day without a successor"))?;
            }
            list
        };
        if let Some(row) = rows
            .iter()
            .find(|row| row.day < days.start() || row.day >= days.end())
        {
            return Err(AiMeteredUsageError::Invalid(format!(
                "a usage row on {} lies outside the replaced days",
                row.day
            )));
        }

        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let stored: Vec<(AiMeteredOrgCharge, OffsetDateTime)> = sqlx::query!(
            r#"
            SELECT day, product, sku, unit_type, repository, kind,
                unit_price::text AS "unit_price!", quantity::text AS "quantity!",
                gross_amount::text AS "gross_amount!", discount_amount::text AS "discount_amount!",
                net_amount::text AS "net_amount!", fetched_at
            FROM ai_provider_org_usage
            WHERE provider = $1 AND day >= $2 AND day < $3
            FOR UPDATE
            "#,
            payer.as_str(),
            days.start(),
            days.end(),
        )
        .fetch_all(&mut transaction.executor())
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(|row| {
            let charge = AiMeteredOrgCharge {
                day: row.day,
                kind: AiOrgChargeKind::parse(&row.kind)
                    .ok_or_else(|| corrupt(format!("corrupt usage kind {:?}", row.kind)))?,
                product: row.product,
                sku: row.sku,
                unit: row.unit_type,
                repository: row.repository,
                unit_price: decimal(&row.unit_price)?,
                quantity: decimal(&row.quantity)?,
                gross_amount: decimal(&row.gross_amount)?,
                discount_amount: decimal(&row.discount_amount)?,
                net_amount: decimal(&row.net_amount)?,
                reported_at: None,
            };
            Ok((charge, row.fetched_at))
        })
        .collect::<Result<_, AiMeteredUsageError>>()?;

        let mut changed = Vec::with_capacity(day_list.len());
        // Rewritten days with the `fetched_at` of their rows.
        let mut rewritten: HashMap<Date, OffsetDateTime> = HashMap::new();
        for day in &day_list {
            let fetched_rows: Vec<&AiMeteredOrgCharge> =
                rows.iter().filter(|row| row.day == *day).collect();
            let stored_rows: Vec<&(AiMeteredOrgCharge, OffsetDateTime)> =
                stored.iter().filter(|(row, _)| row.day == *day).collect();
            let payer_changed = charge_payer_content(fetched_rows.iter().copied())
                != charge_payer_content(stored_rows.iter().map(|(row, _)| row));
            changed.push(payer_changed);
            if charge_content(fetched_rows.iter().copied())
                != charge_content(stored_rows.iter().map(|(row, _)| row))
            {
                rewritten.insert(
                    *day,
                    rewritten_at(
                        payer_changed,
                        fetched_at,
                        stored_rows.iter().map(|(_, at)| *at),
                    ),
                );
            }
        }

        if !rewritten.is_empty() {
            let rewritten_days: Vec<Date> = day_list
                .iter()
                .copied()
                .filter(|day| rewritten.contains_key(day))
                .collect();
            let inserted: Vec<&AiMeteredOrgCharge> = rows
                .iter()
                .filter(|row| rewritten.contains_key(&row.day))
                .collect();
            let text = |value: fn(&AiMeteredOrgCharge) -> &AiDecimal| -> Vec<String> {
                inserted.iter().map(|row| value(row).to_string()).collect()
            };
            let credit_usd = inserted
                .iter()
                .map(|row| match row.kind.is_credits() {
                    true => row
                        .credit_usd()
                        .map(|usd| Some(usd.nanos()))
                        .ok_or_else(|| {
                            AiMeteredUsageError::Invalid(format!(
                                "the quantity of {} on {} is out of range",
                                row.sku, row.day
                            ))
                        }),
                    false => Ok(None),
                })
                .collect::<Result<Vec<Option<i64>>, _>>()?;
            sqlx::query!(
                r#"
                DELETE FROM ai_provider_org_usage
                WHERE provider = $1 AND day = ANY($2::date[])
                "#,
                payer.as_str(),
                &rewritten_days,
            )
            .execute(&mut transaction.executor())
            .await
            .map_err(storage_error)?;
            sqlx::query!(
                r#"
                INSERT INTO ai_provider_org_usage (
                    provider, day, product, sku, unit_type, repository, kind, unit_price, quantity,
                    gross_amount, discount_amount, net_amount, credit_usd_nanos, reported_at,
                    fetched_at
                )
                SELECT $1, row.day, row.product, row.sku, row.unit, row.repository, row.kind,
                    row.unit_price::numeric, row.quantity::numeric, row.gross_amount::numeric,
                    row.discount_amount::numeric, row.net_amount::numeric, row.credit_usd,
                    row.reported_at, row.fetched_at
                FROM UNNEST(
                    $2::date[], $3::text[], $4::text[], $5::text[], $6::text[], $7::text[],
                    $8::text[], $9::text[], $10::text[], $11::text[], $12::text[], $13::int8[],
                    $14::timestamptz[], $15::timestamptz[]
                ) AS row(
                    day, product, sku, unit, repository, kind, unit_price, quantity, gross_amount,
                    discount_amount, net_amount, credit_usd, reported_at, fetched_at
                )
                "#,
                payer.as_str(),
                &inserted.iter().map(|row| row.day).collect::<Vec<_>>(),
                &inserted
                    .iter()
                    .map(|row| row.product.clone())
                    .collect::<Vec<_>>(),
                &inserted
                    .iter()
                    .map(|row| row.sku.clone())
                    .collect::<Vec<_>>(),
                &inserted
                    .iter()
                    .map(|row| row.unit.clone())
                    .collect::<Vec<_>>(),
                &inserted
                    .iter()
                    .map(|row| row.repository.clone())
                    .collect::<Vec<_>>(),
                &inserted
                    .iter()
                    .map(|row| row.kind.as_str().to_string())
                    .collect::<Vec<_>>(),
                &text(|row| &row.unit_price),
                &text(|row| &row.quantity),
                &text(|row| &row.gross_amount),
                &text(|row| &row.discount_amount),
                &text(|row| &row.net_amount),
                &credit_usd as &[Option<i64>],
                &inserted
                    .iter()
                    .map(|row| row.reported_at)
                    .collect::<Vec<_>>() as &[Option<OffsetDateTime>],
                &inserted
                    .iter()
                    .map(|row| rewritten[&row.day])
                    .collect::<Vec<_>>(),
            )
            .execute(&mut transaction.executor())
            .await
            .map_err(storage_error)?;
        }

        record_slices(
            &mut transaction,
            payer,
            AiMeteredSource::OrgUsage,
            ORGANIZATION,
            &day_list,
            &changed,
            fetched_at,
        )
        .await?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(changed_count(&changed))
    }

    async fn replace_summary(
        &self,
        payer: AiProvider,
        month: AiBillingMonth,
        rows: &[AiMeteredSummaryLine],
        fetched_at: OffsetDateTime,
    ) -> Result<bool, AiMeteredUsageError> {
        let first = month.first_day();
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        // The import lock keeps writers apart; the summary is replaced whole.
        let stored = Self::summary_rows(&mut transaction, payer, first).await?;
        let changed = summary_payer_content(rows)
            != summary_payer_content(stored.iter().map(|(line, _)| line));
        if summary_content(rows) != summary_content(stored.iter().map(|(line, _)| line)) {
            let rows_fetched_at =
                rewritten_at(changed, fetched_at, stored.iter().map(|(_, at)| *at));
            let text = |value: fn(&AiMeteredAmounts) -> &AiDecimal| -> Vec<String> {
                rows.iter()
                    .map(|row| value(&row.amounts).to_string())
                    .collect()
            };
            sqlx::query!(
                "DELETE FROM ai_provider_usage_summaries WHERE provider = $1 AND month = $2",
                payer.as_str(),
                first,
            )
            .execute(&mut transaction.executor())
            .await
            .map_err(storage_error)?;
            sqlx::query!(
                r#"
                INSERT INTO ai_provider_usage_summaries (
                    provider, month, product, sku, unit_type, kind, unit_price, gross_quantity,
                    gross_amount, discount_quantity, discount_amount, net_quantity, net_amount,
                    fetched_at
                )
                SELECT $1, $2, row.product, row.sku, row.unit, row.kind, row.unit_price::numeric,
                    row.gross_quantity::numeric, row.gross_amount::numeric,
                    row.discount_quantity::numeric, row.discount_amount::numeric,
                    row.net_quantity::numeric, row.net_amount::numeric, $3
                FROM UNNEST(
                    $4::text[], $5::text[], $6::text[], $7::text[], $8::text[], $9::text[],
                    $10::text[], $11::text[], $12::text[], $13::text[], $14::text[]
                ) AS row(
                    product, sku, unit, kind, unit_price, gross_quantity, gross_amount,
                    discount_quantity, discount_amount, net_quantity, net_amount
                )
                "#,
                payer.as_str(),
                first,
                rows_fetched_at,
                &rows
                    .iter()
                    .map(|row| row.product.clone())
                    .collect::<Vec<_>>(),
                &rows.iter().map(|row| row.sku.clone()).collect::<Vec<_>>(),
                &rows.iter().map(|row| row.unit.clone()).collect::<Vec<_>>(),
                &rows
                    .iter()
                    .map(|row| row.kind.as_str().to_string())
                    .collect::<Vec<_>>(),
                &text(|amounts| &amounts.unit_price),
                &text(|amounts| &amounts.gross_quantity),
                &text(|amounts| &amounts.gross_amount),
                &text(|amounts| &amounts.discount_quantity),
                &text(|amounts| &amounts.discount_amount),
                &text(|amounts| &amounts.net_quantity),
                &text(|amounts| &amounts.net_amount),
            )
            .execute(&mut transaction.executor())
            .await
            .map_err(storage_error)?;
        }
        record_slices(
            &mut transaction,
            payer,
            AiMeteredSource::Summary,
            ORGANIZATION,
            &[first],
            &[changed],
            fetched_at,
        )
        .await?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(changed)
    }

    async fn record_check(
        &self,
        payer: AiProvider,
        source: AiMeteredSource,
        month: AiBillingMonth,
        check: &AiUsageCheck,
    ) -> Result<(), AiMeteredUsageError> {
        let account = check
            .account_id
            .map_or(ORGANIZATION, AiProviderAccountId::as_i64);
        sqlx::query!(
            r#"
            INSERT INTO ai_provider_usage_checks (
                provider, source, account_id, month, checked_at, payer_gross_quantity,
                stored_gross_quantity, matches, error
            )
            VALUES ($1, $2, $3, $4, $5, $6::text::numeric, $7::text::numeric, $8, $9)
            ON CONFLICT (provider, source, account_id, month) DO UPDATE
            SET checked_at = EXCLUDED.checked_at,
                payer_gross_quantity = EXCLUDED.payer_gross_quantity,
                stored_gross_quantity = EXCLUDED.stored_gross_quantity,
                matches = EXCLUDED.matches,
                error = EXCLUDED.error
            "#,
            payer.as_str(),
            source.as_str(),
            account,
            month.first_day(),
            check.checked_at,
            check.payer_gross.map(|gross| gross.to_string()),
            check.stored_gross.to_string(),
            check.matches,
            check.error.as_deref(),
        )
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn start_run(
        &self,
        payer: AiProvider,
        plan: &AiImportPlan,
        started_at: OffsetDateTime,
    ) -> Result<AiImportRunId, AiMeteredUsageError> {
        let months: Vec<Date> = plan
            .months
            .iter()
            .map(|(month, _)| month.first_day())
            .collect();
        let full_months: Vec<Date> = plan
            .months
            .iter()
            .filter(|(_, mode)| *mode == AiMonthMode::Full)
            .map(|(month, _)| month.first_day())
            .collect();
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query!(
            r#"
            UPDATE ai_provider_import_runs
            SET status = 'failed',
                finished_at = GREATEST($2, started_at),
                error = 'interrupted'
            WHERE provider = $1 AND status = 'running'
            "#,
            payer.as_str(),
            started_at,
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?;
        let id = sqlx::query_scalar!(
            r#"
            INSERT INTO ai_provider_import_runs (
                provider, kind, requested_by, months, full_months, started_at, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, 'running')
            RETURNING id
            "#,
            payer.as_str(),
            plan.kind.as_str(),
            plan.kind.requested_by().map(|user| user.as_i32()),
            &months,
            &full_months,
            started_at,
        )
        .fetch_one(&mut transaction.executor())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(AiImportRunId::new(id))
    }

    async fn finish_run(
        &self,
        payer: AiProvider,
        run: &AiImportRun,
    ) -> Result<(), AiMeteredUsageError> {
        let error = |error: &Option<String>| -> Option<String> {
            error
                .as_deref()
                .map(str::trim)
                .filter(|error| !error.is_empty())
                .map(|error| error.chars().take(2000).collect())
        };
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query!(
            r#"
            UPDATE ai_provider_import_runs
            SET status = $3,
                finished_at = $4,
                requests = $5,
                slices_fetched = $6,
                slices_changed = $7,
                account_errors = $8,
                error = $9
            WHERE provider = $1 AND id = $2
            "#,
            payer.as_str(),
            run.id.as_i64(),
            run.status.as_str(),
            run.finished_at,
            column(run.requests),
            column(run.slices_fetched),
            column(run.slices_changed),
            column(run.account_errors),
            error(&run.error),
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?;
        let sources = &run.sources;
        sqlx::query!(
            r#"
            INSERT INTO ai_provider_import_run_sources (
                run_id, source, status, requests, slices_fetched, slices_changed,
                imported_through, settle_lag_days, error
            )
            SELECT $1, row.source, row.status, row.requests, row.fetched, row.changed,
                row.through, row.lag, row.error
            FROM UNNEST(
                $2::text[], $3::text[], $4::int4[], $5::int4[], $6::int4[], $7::date[],
                $8::int4[], $9::text[]
            ) AS row(source, status, requests, fetched, changed, through, lag, error)
            ON CONFLICT (run_id, source) DO NOTHING
            "#,
            run.id.as_i64(),
            &sources
                .iter()
                .map(|s| s.source.as_str().to_string())
                .collect::<Vec<_>>(),
            &sources
                .iter()
                .map(|s| s.status.as_str().to_string())
                .collect::<Vec<_>>(),
            &sources
                .iter()
                .map(|s| column(s.requests))
                .collect::<Vec<_>>(),
            &sources
                .iter()
                .map(|s| column(s.slices_fetched))
                .collect::<Vec<_>>(),
            &sources
                .iter()
                .map(|s| column(s.slices_changed))
                .collect::<Vec<_>>(),
            &sources
                .iter()
                .map(|s| s.imported_through)
                .collect::<Vec<_>>() as &[Option<Date>],
            &sources
                .iter()
                .map(|s| s.settle_lag_days.map(column))
                .collect::<Vec<_>>() as &[Option<i32>],
            &sources.iter().map(|s| error(&s.error)).collect::<Vec<_>>() as &[Option<String>],
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(())
    }

    async fn import_status(
        &self,
        payer: AiProvider,
    ) -> Result<AiImportStatus, AiMeteredUsageError> {
        // A run's `error` is why it stopped, so a nightly run that stopped
        // (a rate limit, the request cap, ...) never counts as the night's
        // refresh, while one with only account errors does.
        let history = sqlx::query!(
            r#"
            SELECT
                max(started_at) AS last_attempt_at,
                max(finished_at) FILTER (WHERE status IN ('succeeded', 'partial'))
                    AS last_success_at,
                max(started_at) FILTER (
                    WHERE kind = 'nightly'
                      AND status IN ('succeeded', 'partial')
                      AND error IS NULL
                ) AS last_nightly_success_at
            FROM ai_provider_import_runs
            WHERE provider = $1
            "#,
            payer.as_str(),
        )
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;

        // A `running` row counts only while some session holds the import
        // lock (`try_lock_import`'s key, as pg_locks splits it): a run whose
        // process died freed the lock but left its row running.
        let running = sqlx::query_as!(
            RunRow,
            r#"
            SELECT id, kind, requested_by, months, full_months, started_at, finished_at, status,
                requests, slices_fetched, slices_changed, account_errors, error
            FROM ai_provider_import_runs
            WHERE provider = $1
              AND status = 'running'
              AND EXISTS (
                  SELECT 1
                  FROM pg_locks
                  WHERE locktype = 'advisory'
                    AND granted
                    AND objsubid = 1
                    AND database = (SELECT oid FROM pg_database WHERE datname = current_database())
                    AND classid = ((hashtextextended('ai_provider_usage_import:' || $1, 0) >> 32)
                        & 4294967295)::oid
                    AND objid = (hashtextextended('ai_provider_usage_import:' || $1, 0)
                        & 4294967295)::oid
              )
            ORDER BY started_at DESC
            LIMIT 1
            "#,
            payer.as_str(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?
        .map(|row| row.into_run(Vec::new()))
        .transpose()?;

        let last_row = sqlx::query_as!(
            RunRow,
            r#"
            SELECT id, kind, requested_by, months, full_months, started_at, finished_at, status,
                requests, slices_fetched, slices_changed, account_errors, error
            FROM ai_provider_import_runs
            WHERE provider = $1 AND status <> 'running'
            ORDER BY finished_at DESC, id DESC
            LIMIT 1
            "#,
            payer.as_str(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        let last_run = match last_row {
            Some(row) => {
                let sources = self.run_sources(row.id).await?;
                Some(row.into_run(sources)?)
            }
            None => None,
        };

        let account_errors = match &last_run {
            Some(run) => sqlx::query!(
                r#"
                SELECT account_id, month, error AS "error!"
                FROM ai_provider_usage_checks
                WHERE provider = $1
                  AND source = 'account_credits'
                  AND error IS NOT NULL
                  AND checked_at >= $2
                ORDER BY month, account_id
                "#,
                payer.as_str(),
                run.started_at,
            )
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?
            .into_iter()
            .map(|row| {
                Ok(AiStoredAccountError {
                    account_id: account_id(row.account_id)?,
                    month: month_of(row.month)?,
                    error: row.error,
                })
            })
            .collect::<Result<Vec<_>, AiMeteredUsageError>>()?,
            None => Vec::new(),
        };

        let latest = sqlx::query!(
            r#"
            SELECT DISTINCT ON (source.source)
                source.source, source.imported_through, source.settle_lag_days, source.error
            FROM ai_provider_import_run_sources AS source
            JOIN ai_provider_import_runs AS run ON run.id = source.run_id
            WHERE run.provider = $1 AND run.status <> 'running'
            ORDER BY source.source, run.finished_at DESC, run.id DESC
            "#,
            payer.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        let successes = sqlx::query!(
            r#"
            SELECT source.source, max(run.finished_at) AS "succeeded_at!"
            FROM ai_provider_import_run_sources AS source
            JOIN ai_provider_import_runs AS run ON run.id = source.run_id
            WHERE run.provider = $1 AND source.status = 'succeeded' AND run.finished_at IS NOT NULL
            GROUP BY source.source
            "#,
            payer.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        let sources = AiMeteredSource::ALL
            .into_iter()
            .map(|source| {
                let row = latest.iter().find(|row| row.source == source.as_str());
                AiSourceState {
                    source,
                    imported_through: row.and_then(|row| row.imported_through),
                    last_success_at: successes
                        .iter()
                        .find(|row| row.source == source.as_str())
                        .map(|row| row.succeeded_at),
                    last_error: row.and_then(|row| row.error.clone()),
                    settle_lag_days: row
                        .and_then(|row| row.settle_lag_days)
                        .map(|days| u32::try_from(days).unwrap_or(0)),
                }
            })
            .collect();

        Ok(AiImportStatus {
            history: AiImportHistory {
                last_attempt_at: history.last_attempt_at,
                last_success_at: history.last_success_at,
                last_nightly_success_at: history.last_nightly_success_at,
            },
            running,
            last_run,
            account_errors,
            sources,
        })
    }

    async fn months(&self, payer: AiProvider) -> Result<Vec<AiBillingMonth>, AiMeteredUsageError> {
        sqlx::query_scalar!(
            r#"
            SELECT DISTINCT date_trunc('month', day)::date AS "month!"
            FROM ai_provider_usage_slices
            WHERE provider = $1
            ORDER BY 1
            "#,
            payer.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(month_of)
        .collect()
    }

    async fn account_credits(
        &self,
        payer: AiProvider,
        dates: AiUsageDateRange,
        accounts: Option<&[AiProviderAccountId]>,
    ) -> Result<Vec<AiStoredAccountCredit>, AiMeteredUsageError> {
        let ids: Option<Vec<i64>> =
            accounts.map(|accounts| accounts.iter().map(|id| id.as_i64()).collect());
        sqlx::query!(
            r#"
            SELECT account_id, day, product, sku, model_label, model, auto_routed, kind, unit_type,
                unit_price::text AS "unit_price!", gross_quantity::text AS "gross_quantity!",
                gross_amount::text AS "gross_amount!",
                discount_quantity::text AS "discount_quantity!",
                discount_amount::text AS "discount_amount!",
                net_quantity::text AS "net_quantity!", net_amount::text AS "net_amount!",
                gross_usd_nanos, login, fetched_at
            FROM ai_provider_account_usage
            WHERE provider = $1 AND day >= $2 AND day < $3
              AND ($4::int8[] IS NULL OR account_id = ANY($4))
            ORDER BY account_id, day, product, sku, model_label
            "#,
            payer.as_str(),
            dates.start(),
            dates.end(),
            ids.as_deref(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(|row| {
            Ok(AiStoredAccountCredit {
                account_id: account_id(row.account_id)?,
                day: row.day,
                credit: stored_credit(
                    row.day,
                    row.product,
                    row.sku,
                    row.model_label,
                    row.model,
                    row.auto_routed,
                    &row.kind,
                    row.unit_type,
                    [
                        &row.unit_price,
                        &row.gross_quantity,
                        &row.gross_amount,
                        &row.discount_quantity,
                        &row.discount_amount,
                        &row.net_quantity,
                        &row.net_amount,
                    ],
                )?,
                login: row.login,
                gross_usd: nanos(row.gross_usd_nanos)?,
                fetched_at: row.fetched_at,
            })
        })
        .collect()
    }

    async fn org_credits(
        &self,
        payer: AiProvider,
        dates: AiUsageDateRange,
    ) -> Result<Vec<AiStoredOrgCredit>, AiMeteredUsageError> {
        sqlx::query!(
            r#"
            SELECT day, product, sku, model_label, model, auto_routed, kind, unit_type,
                unit_price::text AS "unit_price!", gross_quantity::text AS "gross_quantity!",
                gross_amount::text AS "gross_amount!",
                discount_quantity::text AS "discount_quantity!",
                discount_amount::text AS "discount_amount!",
                net_quantity::text AS "net_quantity!", net_amount::text AS "net_amount!",
                gross_usd_nanos
            FROM ai_provider_org_credits
            WHERE provider = $1 AND day >= $2 AND day < $3
            ORDER BY day, product, sku, model_label
            "#,
            payer.as_str(),
            dates.start(),
            dates.end(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(|row| {
            Ok(AiStoredOrgCredit {
                day: row.day,
                credit: stored_credit(
                    row.day,
                    row.product,
                    row.sku,
                    row.model_label,
                    row.model,
                    row.auto_routed,
                    &row.kind,
                    row.unit_type,
                    [
                        &row.unit_price,
                        &row.gross_quantity,
                        &row.gross_amount,
                        &row.discount_quantity,
                        &row.discount_amount,
                        &row.net_quantity,
                        &row.net_amount,
                    ],
                )?,
                gross_usd: nanos(row.gross_usd_nanos)?,
            })
        })
        .collect()
    }

    async fn org_charges(
        &self,
        payer: AiProvider,
        dates: AiUsageDateRange,
    ) -> Result<Vec<AiMeteredOrgCharge>, AiMeteredUsageError> {
        self.charges(payer, dates).await
    }

    async fn summary(
        &self,
        payer: AiProvider,
        month: AiBillingMonth,
    ) -> Result<Option<(Vec<AiMeteredSummaryLine>, OffsetDateTime)>, AiMeteredUsageError> {
        let first = month.first_day();
        let fetched_at = sqlx::query_scalar!(
            r#"
            SELECT fetched_at
            FROM ai_provider_usage_slices
            WHERE provider = $1 AND source = 'summary' AND account_id = 0 AND day = $2
            "#,
            payer.as_str(),
            first,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        let Some(fetched_at) = fetched_at else {
            return Ok(None);
        };
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let rows = Self::summary_rows(&mut transaction, payer, first).await?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(Some((
            rows.into_iter().map(|(line, _)| line).collect(),
            fetched_at,
        )))
    }

    async fn checks(
        &self,
        payer: AiProvider,
        month: AiBillingMonth,
    ) -> Result<Vec<AiUsageCheck>, AiMeteredUsageError> {
        sqlx::query!(
            r#"
            SELECT account_id, checked_at, payer_gross_quantity::text AS payer_gross,
                stored_gross_quantity::text AS "stored_gross!", matches, error
            FROM ai_provider_usage_checks
            WHERE provider = $1 AND month = $2
            ORDER BY source, account_id
            "#,
            payer.as_str(),
            month.first_day(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(|row| {
            Ok(AiUsageCheck {
                account_id: match row.account_id {
                    ORGANIZATION => None,
                    id => Some(account_id(id)?),
                },
                checked_at: row.checked_at,
                payer_gross: row.payer_gross.as_deref().map(decimal).transpose()?,
                stored_gross: decimal(&row.stored_gross)?,
                matches: row.matches,
                error: row.error,
            })
        })
        .collect()
    }

    async fn tracked_usage(
        &self,
        payer: AiProvider,
        dates: AiUsageDateRange,
        reprice_floor: OffsetDateTime,
        mapping_company: Option<&TimeTrackingCompany>,
        user: Option<UserId>,
    ) -> Result<Vec<AiTrackedModelDay>, AiMeteredUsageError> {
        // Keys resolve as the bill resolves them: the shared resolver decides
        // which mappings count, and personal marks apply only to unmapped
        // keys. A bucket is on an old pricing basis when it is at or after
        // the floor and has no basis, or another basis than its own machine's
        // current one: the latest non-null basis of the machine's uploads and
        // repricings, in the order they were applied, over all of its
        // providers. A machine that never sent a basis has none. Each cost is
        // summed exactly per destination and rounded as the user's day and
        // model: a destination gets the rounded running total through it less
        // the one before it.
        let resolver = AiMappingResolver::new(
            stored_mappings(&self.pool).await.map_err(storage_error)?,
            mapping_company.cloned(),
        );
        let (personal_users, personal_keys): (Vec<i32>, Vec<String>) =
            stored_personal_keys(&self.pool, user.as_ref())
                .await
                .map_err(storage_error)?
                .into_iter()
                .filter(|(_, key)| resolver.personal_mark_applies(key))
                .map(|(user_id, key)| (user_id.as_i32(), key))
                .unzip();
        let resolved = resolver.resolved_keys();
        let keys: Vec<&str> = resolved.iter().map(|(key, _)| *key).collect();
        let ids: Vec<&str> = resolved
            .iter()
            .map(|(_, project)| project.id.as_str())
            .collect();
        sqlx::query!(
            r#"
            WITH resolved_projects AS (
                SELECT project_key, project_id
                FROM UNNEST($6::text[], $7::text[]) AS mapping(project_key, project_id)
            ), personal_keys AS (
                SELECT user_id, project_key
                FROM UNNEST($8::int4[], $9::text[]) AS mark(user_id, project_key)
            ), destinations AS (
                SELECT
                    bucket.user_id,
                    (bucket.hour_start AT TIME ZONE 'UTC')::date AS day,
                    bucket.model,
                    personal.project_key IS NOT NULL AS personal,
                    CASE WHEN personal.project_key IS NULL THEN mapping.project_id END
                        AS project_id,
                    personal.project_key IS NULL AND bucket.project_key = $10 AS unattributed,
                    MIN(bucket.project_key) AS project_key,
                    SUM(bucket.input_tokens)::int8 AS input_tokens,
                    SUM(bucket.cache_read_tokens)::int8 AS cache_read_tokens,
                    SUM(bucket.cache_write_tokens)::int8 AS cache_write_tokens,
                    SUM(bucket.output_tokens)::int8 AS output_tokens,
                    SUM(bucket.records)::int8 AS records,
                    SUM(bucket.unpriced_records)::int8 AS unpriced,
                    COALESCE(SUM(bucket.estimated_cost_usd), 0) AS cost,
                    COALESCE(SUM(bucket.estimated_cost_usd) FILTER (
                        WHERE bucket.hour_start >= $4
                          AND (bucket.pricing_basis IS NULL
                               OR bucket.pricing_basis IS DISTINCT FROM machine.pricing_basis)
                    ), 0) AS old_basis_cost,
                    COUNT(*) FILTER (
                        WHERE bucket.hour_start >= $4
                          AND (bucket.pricing_basis IS NULL
                               OR bucket.pricing_basis IS DISTINCT FROM machine.pricing_basis)
                    )::int8 AS old_basis_buckets,
                    COALESCE(SUM(bucket.estimated_cost_usd) FILTER (
                        WHERE bucket.hour_start < $4
                    ), 0) AS before_floor_cost
                FROM ai_usage_buckets AS bucket
                JOIN ai_usage_machines AS machine ON machine.id = bucket.machine_id
                LEFT JOIN resolved_projects AS mapping ON mapping.project_key = bucket.project_key
                LEFT JOIN personal_keys AS personal
                    ON personal.user_id = bucket.user_id
                    AND personal.project_key = bucket.project_key
                WHERE COALESCE(bucket.billing_provider, bucket.provider) = $1
                  AND bucket.hour_start >= ($2::date::timestamp AT TIME ZONE 'UTC')
                  AND bucket.hour_start < ($3::date::timestamp AT TIME ZONE 'UTC')
                  AND ($5::int4 IS NULL OR bucket.user_id = $5)
                GROUP BY 1, 2, 3, 4, 5, 6
            )
            SELECT
                user_id AS "user_id!",
                day AS "day!",
                model AS "model!",
                personal AS "personal!",
                project_id,
                unattributed AS "unattributed!",
                project_key AS "project_key!",
                input_tokens AS "input_tokens!",
                cache_read_tokens AS "cache_read_tokens!",
                cache_write_tokens AS "cache_write_tokens!",
                output_tokens AS "output_tokens!",
                records AS "records!",
                unpriced AS "unpriced!",
                (
                    round(SUM(cost) OVER through * 1000000000)
                    - round(COALESCE(SUM(cost) OVER before, 0) * 1000000000)
                )::int8 AS "priced_nanos!",
                (
                    round(SUM(old_basis_cost) OVER through * 1000000000)
                    - round(COALESCE(SUM(old_basis_cost) OVER before, 0) * 1000000000)
                )::int8 AS "old_basis_nanos!",
                old_basis_buckets AS "old_basis_buckets!",
                (
                    round(SUM(before_floor_cost) OVER through * 1000000000)
                    - round(COALESCE(SUM(before_floor_cost) OVER before, 0) * 1000000000)
                )::int8 AS "before_floor_nanos!"
            FROM destinations
            WINDOW
                destination AS (
                    PARTITION BY user_id, day, model
                    ORDER BY personal, project_id NULLS LAST, unattributed
                ),
                through AS (destination ROWS UNBOUNDED PRECEDING),
                before AS (destination ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING)
            ORDER BY user_id, day, model, personal, project_id NULLS LAST, unattributed
            "#,
            payer.as_str(),
            dates.start(),
            dates.end(),
            reprice_floor,
            user.map(|user| user.as_i32()),
            &keys as &[&str],
            &ids as &[&str],
            &personal_users,
            &personal_keys,
            UNATTRIBUTED_PROJECT_KEY,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(|row| {
            // Key names never leave the adapter: only the destination does.
            let destination = if row.personal {
                AiMeteredDestination::Personal
            } else {
                AiMeteredDestination::Project(match resolver.project(&row.project_key) {
                    Some(project) => AiBillingProject::Mapped(project.clone()),
                    None if row.unattributed => {
                        AiBillingProject::Unassigned(AiUnassignedKind::Unattributed)
                    }
                    None => AiBillingProject::Unassigned(AiUnassignedKind::Unmapped),
                })
            };
            Ok(AiTrackedModelDay {
                user_id: UserId::new(row.user_id),
                day: row.day,
                model: row.model,
                destination,
                tokens: AiTokenCounts {
                    input: row.input_tokens,
                    cache_read: row.cache_read_tokens,
                    cache_write: row.cache_write_tokens,
                    output: row.output_tokens,
                },
                priced_cost: nanos(row.priced_nanos)?,
                records: row.records,
                unpriced_records: row.unpriced,
                old_basis_cost: nanos(row.old_basis_nanos)?,
                old_basis_buckets: row.old_basis_buckets,
                before_floor_cost: nanos(row.before_floor_nanos)?,
            })
        })
        .collect()
    }

    async fn repository_mappings(
        &self,
        payer: AiProvider,
    ) -> Result<Vec<AiStoredRepositoryMapping>, AiMeteredUsageError> {
        let rows = sqlx::query!(
            r#"
            SELECT repository_name, time_tracking_provider, provider_company_id, project_id,
                project_name
            FROM ai_provider_repository_mappings
            WHERE provider = $1
            "#,
            payer.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(rows
            .into_iter()
            .map(|row| AiStoredRepositoryMapping {
                repository: row.repository_name,
                company: TimeTrackingCompany {
                    provider: row.time_tracking_provider,
                    company_id: row.provider_company_id,
                },
                project: AiMappedProject {
                    id: ProjectId::new(row.project_id),
                    name: row.project_name,
                },
            })
            .collect())
    }
}

#[cfg(test)]
mod import_tests;
#[cfg(test)]
mod tests;
