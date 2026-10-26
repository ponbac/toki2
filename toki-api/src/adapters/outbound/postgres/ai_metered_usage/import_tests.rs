//! The metered usage service's import and reconciliation over this Postgres
//! adapter, with a fake usage source and a fixed clock. They live with the
//! adapter so the domain never depends on it.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use sqlx::PgPool;
use time::{
    macros::{date, datetime},
    Date, Duration, OffsetDateTime,
};

use crate::{
    adapters::outbound::postgres::{
        PostgresAiMeteredUsageRepository, PostgresAiProviderAccountRepository,
    },
    db::DbPool,
    domain::{
        models::*,
        ports::{
            inbound::AiMeteredUsageService,
            outbound::{
                AiImportLease, AiMeteredUsageRepository, AiProviderAccountRepository,
                MeteredUsageSource,
            },
        },
        services::{AiMeteredImportSettings, AiMeteredSourceState, AiMeteredUsageServiceImpl},
        AiMeteredUsageError, MeteredUsageSourceError,
    },
};

const NOW: OffsetDateTime = datetime!(2026-10-25 10:00 UTC);
const ORG: &str = "example-org";

fn dec(raw: &str) -> AiDecimal {
    AiDecimal::parse(raw).unwrap()
}

fn month(raw: &str) -> AiBillingMonth {
    AiBillingMonth::parse(raw).unwrap()
}

fn credit(label: &str, quantity: &str) -> AiMeteredCredit {
    AiMeteredCredit {
        product: "Copilot".to_string(),
        sku: "Copilot AI Credits".to_string(),
        unit: "ai-credits".to_string(),
        model: AiMeteredModel {
            label: label.to_string(),
            model: label.to_lowercase().replace(' ', "-"),
            auto_routed: false,
        },
        kind: AiCreditKind::Trackable,
        amounts: AiMeteredAmounts {
            unit_price: dec("0.01"),
            gross_quantity: dec(quantity),
            gross_amount: dec(quantity),
            discount_quantity: dec(quantity),
            discount_amount: dec(quantity),
            ..AiMeteredAmounts::default()
        },
    }
}

/// Credits rows per login (`None` is the organization) and UTC day.
type DayRows = BTreeMap<(Option<String>, Date), Vec<AiMeteredCredit>>;

/// A stand-in usage source: day rows per scope (`None` is the organization),
/// month totals summed from them, a call log and errors to inject by call.
#[derive(Default)]
struct FakeSource {
    days: Mutex<DayRows>,
    org_usage: Mutex<Vec<AiMeteredOrgCharge>>,
    summary: Mutex<Vec<AiMeteredSummaryLine>>,
    calls: Mutex<Vec<String>>,
    failures: Mutex<HashMap<String, MeteredUsageSourceError>>,
}

impl FakeSource {
    fn set(&self, scope: Option<&str>, day: Date, rows: Vec<AiMeteredCredit>) {
        self.days
            .lock()
            .unwrap()
            .insert((scope.map(str::to_string), day), rows);
    }

    fn fail(&self, call: &str, error: MeteredUsageSourceError) {
        self.failures
            .lock()
            .unwrap()
            .insert(call.to_string(), error);
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn take_calls(&self) -> Vec<String> {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }

    fn record(&self, call: String) -> Result<(), MeteredUsageSourceError> {
        self.calls.lock().unwrap().push(call.clone());
        match self.failures.lock().unwrap().get(&call) {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    /// The month total of a scope, as the payer sums it.
    fn month_rows(&self, scope: Option<&str>, month: AiBillingMonth) -> Vec<AiMeteredCredit> {
        let rows: Vec<AiMeteredCredit> = self
            .days
            .lock()
            .unwrap()
            .iter()
            .filter(|((owner, day), _)| owner.as_deref() == scope && month.contains(*day))
            .flat_map(|(_, rows)| rows.clone())
            .collect();
        merge_credits(rows).unwrap()
    }
}

fn scope_name(scope: AiMeteredScope<'_>) -> Option<String> {
    match scope {
        AiMeteredScope::Account(login) => Some(login.as_str().to_string()),
        AiMeteredScope::Organization => None,
    }
}

#[async_trait]
impl MeteredUsageSource for FakeSource {
    async fn credits(
        &self,
        scope: AiMeteredScope<'_>,
        period: AiMeteredPeriod,
    ) -> Result<Vec<AiMeteredCredit>, MeteredUsageSourceError> {
        let name = scope_name(scope);
        let who = name.clone().unwrap_or_else(|| "org".to_string());
        match period {
            AiMeteredPeriod::Month(month) => {
                self.record(format!("credits {who} {month}"))?;
                Ok(self.month_rows(name.as_deref(), month))
            }
            AiMeteredPeriod::Day(day) => {
                self.record(format!("credits {who} {day}"))?;
                Ok(self
                    .days
                    .lock()
                    .unwrap()
                    .get(&(name, day))
                    .cloned()
                    .unwrap_or_default())
            }
        }
    }

    async fn org_usage(
        &self,
        period: AiMeteredPeriod,
    ) -> Result<Vec<AiMeteredOrgCharge>, MeteredUsageSourceError> {
        let AiMeteredPeriod::Month(month) = period else {
            panic!("the import reads usage by month");
        };
        self.record(format!("usage {month}"))?;
        Ok(self
            .org_usage
            .lock()
            .unwrap()
            .iter()
            .filter(|row| month.contains(row.day))
            .cloned()
            .collect())
    }

    async fn summary(
        &self,
        month: AiBillingMonth,
    ) -> Result<Vec<AiMeteredSummaryLine>, MeteredUsageSourceError> {
        self.record(format!("summary {month}"))?;
        Ok(self.summary.lock().unwrap().clone())
    }
}

/// Postgres with the service's clock fixed, and a credits write to fail:
/// the scope's login (`None` is the organization) and a UTC day it holds.
struct Clocked {
    inner: PostgresAiMeteredUsageRepository,
    now: Mutex<OffsetDateTime>,
    failing_write: Mutex<Option<(Option<String>, Date)>>,
}

#[async_trait]
impl AiMeteredUsageRepository for Clocked {
    async fn now(&self) -> Result<OffsetDateTime, AiMeteredUsageError> {
        Ok(*self.now.lock().unwrap())
    }
    async fn open_from(
        &self,
        time_zone: &AiUsageTimeZone,
        configured_floor: OffsetDateTime,
    ) -> Result<OffsetDateTime, AiMeteredUsageError> {
        self.inner.open_from(time_zone, configured_floor).await
    }
    async fn try_lock_import(
        &self,
        payer: AiProvider,
    ) -> Result<Option<Box<dyn AiImportLease>>, AiMeteredUsageError> {
        self.inner.try_lock_import(payer).await
    }
    async fn slices(
        &self,
        payer: AiProvider,
        source: AiMeteredSource,
        dates: AiUsageDateRange,
    ) -> Result<Vec<AiSliceRecord>, AiMeteredUsageError> {
        self.inner.slices(payer, source, dates).await
    }
    async fn credit_month_totals(
        &self,
        payer: AiProvider,
        scope: AiStoredScope,
        month: AiBillingMonth,
    ) -> Result<Vec<AiCreditTotal>, AiMeteredUsageError> {
        self.inner.credit_month_totals(payer, scope, month).await
    }
    async fn replace_credit_days(
        &self,
        payer: AiProvider,
        scope: AiCreditStoreScope<'_>,
        days: &[(Date, Vec<AiMeteredCredit>)],
        fetched_at: OffsetDateTime,
    ) -> Result<u32, AiMeteredUsageError> {
        let login = match scope {
            AiCreditStoreScope::Account { login, .. } => Some(login.as_str().to_string()),
            AiCreditStoreScope::Organization => None,
        };
        if let Some((failing, day)) = &*self.failing_write.lock().unwrap() {
            if *failing == login && days.iter().any(|(candidate, _)| candidate == day) {
                return Err(AiMeteredUsageError::Storage(
                    "the database went away".to_string(),
                ));
            }
        }
        self.inner
            .replace_credit_days(payer, scope, days, fetched_at)
            .await
    }
    async fn replace_org_usage(
        &self,
        payer: AiProvider,
        days: AiUsageDateRange,
        rows: &[AiMeteredOrgCharge],
        fetched_at: OffsetDateTime,
    ) -> Result<u32, AiMeteredUsageError> {
        self.inner
            .replace_org_usage(payer, days, rows, fetched_at)
            .await
    }
    async fn replace_summary(
        &self,
        payer: AiProvider,
        month: AiBillingMonth,
        rows: &[AiMeteredSummaryLine],
        fetched_at: OffsetDateTime,
    ) -> Result<bool, AiMeteredUsageError> {
        self.inner
            .replace_summary(payer, month, rows, fetched_at)
            .await
    }
    async fn mark_checks_importing(
        &self,
        payer: AiProvider,
        month: AiBillingMonth,
        run: AiImportRunId,
    ) -> Result<(), AiMeteredUsageError> {
        self.inner.mark_checks_importing(payer, month, run).await
    }
    async fn record_check(
        &self,
        payer: AiProvider,
        source: AiMeteredSource,
        month: AiBillingMonth,
        check: &AiUsageCheck,
    ) -> Result<(), AiMeteredUsageError> {
        self.inner.record_check(payer, source, month, check).await
    }
    async fn start_run(
        &self,
        payer: AiProvider,
        plan: &AiImportPlan,
        started_at: OffsetDateTime,
    ) -> Result<AiImportRunId, AiMeteredUsageError> {
        self.inner.start_run(payer, plan, started_at).await
    }
    async fn finish_run(
        &self,
        payer: AiProvider,
        run: &AiImportRun,
    ) -> Result<(), AiMeteredUsageError> {
        self.inner.finish_run(payer, run).await
    }
    async fn import_status(
        &self,
        payer: AiProvider,
    ) -> Result<AiImportStatus, AiMeteredUsageError> {
        self.inner.import_status(payer).await
    }
    async fn months(&self, payer: AiProvider) -> Result<Vec<AiBillingMonth>, AiMeteredUsageError> {
        self.inner.months(payer).await
    }
    async fn account_credits(
        &self,
        payer: AiProvider,
        dates: AiUsageDateRange,
        accounts: Option<&[AiProviderAccountId]>,
    ) -> Result<Vec<AiStoredAccountCredit>, AiMeteredUsageError> {
        self.inner.account_credits(payer, dates, accounts).await
    }
    async fn org_credits(
        &self,
        payer: AiProvider,
        dates: AiUsageDateRange,
    ) -> Result<Vec<AiStoredOrgCredit>, AiMeteredUsageError> {
        self.inner.org_credits(payer, dates).await
    }
    async fn org_charges(
        &self,
        payer: AiProvider,
        dates: AiUsageDateRange,
    ) -> Result<Vec<AiMeteredOrgCharge>, AiMeteredUsageError> {
        self.inner.org_charges(payer, dates).await
    }
    async fn summary(
        &self,
        payer: AiProvider,
        month: AiBillingMonth,
    ) -> Result<Option<(Vec<AiMeteredSummaryLine>, OffsetDateTime)>, AiMeteredUsageError> {
        self.inner.summary(payer, month).await
    }
    async fn checks(
        &self,
        payer: AiProvider,
        month: AiBillingMonth,
        importing: Option<AiImportRunId>,
    ) -> Result<Vec<AiUsageCheck>, AiMeteredUsageError> {
        self.inner.checks(payer, month, importing).await
    }
    async fn tracked_usage(
        &self,
        payer: AiProvider,
        dates: AiUsageDateRange,
        reprice_floor: OffsetDateTime,
        mapping_company: Option<&TimeTrackingCompany>,
        user: Option<UserId>,
    ) -> Result<Vec<AiTrackedModelDay>, AiMeteredUsageError> {
        self.inner
            .tracked_usage(payer, dates, reprice_floor, mapping_company, user)
            .await
    }
    async fn repository_mappings(
        &self,
        payer: AiProvider,
    ) -> Result<Vec<AiStoredRepositoryMapping>, AiMeteredUsageError> {
        self.inner.repository_mappings(payer).await
    }
}

type Service = AiMeteredUsageServiceImpl<Clocked, PostgresAiProviderAccountRepository>;

struct Setup {
    service: Service,
    source: Arc<FakeSource>,
    repository: Arc<Clocked>,
}

fn setup(pool: &PgPool, backfill_from: Date) -> Setup {
    let source = Arc::new(FakeSource::default());
    let repository = Arc::new(Clocked {
        inner: PostgresAiMeteredUsageRepository::new(DbPool::from(pool.clone())),
        now: Mutex::new(NOW),
        failing_write: Mutex::new(None),
    });
    let service = AiMeteredUsageServiceImpl::new(
        repository.clone(),
        Arc::new(PostgresAiProviderAccountRepository::new(DbPool::from(
            pool.clone(),
        ))),
        AiMeteredSourceState::Ready(source.clone()),
        AiProvider::Copilot,
        ORG.to_string(),
        AiMeteredImportSettings {
            backfill_from,
            reprice_floor: datetime!(2026-10-01 00:00 UTC),
            time_zone: AiUsageTimeZone::parse("Europe/Stockholm").unwrap(),
            unattended: false,
        },
        None,
    );
    Setup {
        service,
        source,
        repository,
    }
}

impl Setup {
    fn at(&self, now: OffsetDateTime) {
        *self.repository.now.lock().unwrap() = now;
    }

    /// Fails storing the credits of `login`'s (`None`: the organization's)
    /// `day`.
    fn fail_write(&self, login: Option<&str>, day: Date) {
        *self.repository.failing_write.lock().unwrap() = Some((login.map(str::to_string), day));
    }

    async fn import(&self, month_raw: &str, mode: AiMonthMode) -> AiImportRun {
        let ticket = self
            .service
            .begin_import(month(month_raw), mode, UserId::new(1))
            .await
            .unwrap();
        self.service.run_import(ticket).await.unwrap()
    }
}

async fn insert_admin(pool: &PgPool) {
    sqlx::query(
        "INSERT INTO users (id, email, full_name, picture, access_token, roles)
         VALUES (1, 'root@example.com', 'Root Admin', '', '', ARRAY['Admin'])",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// An account, with a seat from `seat_from` if given.
async fn insert_account(pool: &PgPool, id: i64, login: &str, seat_from: Option<OffsetDateTime>) {
    sqlx::query(
        "INSERT INTO ai_provider_accounts (provider, account_id, login) VALUES ('copilot', $1, $2)",
    )
    .bind(id)
    .bind(login)
    .execute(pool)
    .await
    .unwrap();
    if let Some(from) = seat_from {
        sqlx::query(
            "INSERT INTO ai_provider_seats (provider, account_id, seat_created_at, first_listed_at,
                last_listed_at)
             VALUES ('copilot', $1, $2, $2, $3)",
        )
        .bind(id)
        .bind(from)
        .bind(NOW)
        .execute(pool)
        .await
        .unwrap();
    }
}

/// Ada with usage on the 3rd, 24th and today, Grace on the 10th, Alan
/// without usage, and the organization five credits above them on the 3rd.
async fn october_fixture(pool: &PgPool, source: &FakeSource) {
    insert_admin(pool).await;
    let seat = Some(datetime!(2026-09-01 08:00 UTC));
    insert_account(pool, 1001, "ada-lovelace", seat).await;
    insert_account(pool, 1002, "grace-hopper", seat).await;
    insert_account(pool, 1003, "alan-turing", seat).await;
    source.set(
        Some("ada-lovelace"),
        date!(2026 - 10 - 03),
        vec![credit("GPT-6.1 Sol", "10")],
    );
    source.set(
        Some("ada-lovelace"),
        date!(2026 - 10 - 24),
        vec![credit("GPT-6.1 Sol", "5")],
    );
    source.set(
        Some("ada-lovelace"),
        date!(2026 - 10 - 25),
        vec![credit("GPT-6.1 Sol", "1")],
    );
    source.set(
        Some("grace-hopper"),
        date!(2026 - 10 - 10),
        vec![credit("GPT-5.4", "20")],
    );
    source.set(
        None,
        date!(2026 - 10 - 03),
        vec![credit("GPT-6.1 Sol", "15")],
    );
    source.set(
        None,
        date!(2026 - 10 - 24),
        vec![credit("GPT-6.1 Sol", "5")],
    );
    source.set(
        None,
        date!(2026 - 10 - 25),
        vec![credit("GPT-6.1 Sol", "1")],
    );
    source.set(None, date!(2026 - 10 - 10), vec![credit("GPT-5.4", "20")]);
    source.org_usage.lock().unwrap().push(AiMeteredOrgCharge {
        day: date!(2026 - 10 - 03),
        product: "copilot".to_string(),
        sku: "Copilot AI Credits".to_string(),
        unit: "AICredits".to_string(),
        repository: "widget-api".to_string(),
        kind: AiOrgChargeKind::RepositoryCredits,
        unit_price: dec("0.01"),
        quantity: dec("4"),
        gross_amount: dec("0.04"),
        discount_amount: dec("0.04"),
        net_amount: AiDecimal::ZERO,
        reported_at: Some(datetime!(2026-10-03 09:00 UTC)),
    });
    source.summary.lock().unwrap().push(AiMeteredSummaryLine {
        product: "copilot".to_string(),
        sku: "copilot_ai_unit".to_string(),
        unit: "ai-units".to_string(),
        kind: AiSummaryKind::Credits,
        amounts: AiMeteredAmounts {
            unit_price: dec("0.01"),
            gross_quantity: dec("51"),
            gross_amount: dec("0.51"),
            discount_quantity: dec("51"),
            discount_amount: dec("0.51"),
            ..AiMeteredAmounts::default()
        },
    });
}

fn day_calls(calls: &[String], who: &str) -> Vec<String> {
    calls
        .iter()
        .filter(|call| {
            call.starts_with(&format!("credits {who} "))
                && call.len() > format!("credits {who} 2026-10").len()
        })
        .cloned()
        .collect()
}

fn month_calls(calls: &[String], who: &str) -> usize {
    calls
        .iter()
        .filter(|call| {
            call.starts_with(&format!("credits {who} "))
                && call.len() == format!("credits {who} 2026-10").len()
        })
        .count()
}

/// Every imported row as text, slices excluded.
async fn imported_rows(pool: &PgPool) -> Vec<String> {
    let mut rows = Vec::new();
    for table in [
        "ai_provider_account_usage",
        "ai_provider_org_credits",
        "ai_provider_org_usage",
        "ai_provider_usage_summaries",
    ] {
        let mut table_rows: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT row_to_json(t)::text FROM {table} AS t"
        )))
        .fetch_all(pool)
        .await
        .unwrap();
        table_rows.sort();
        rows.extend(table_rows);
    }
    rows
}

#[sqlx::test]
async fn the_first_run_backfills_and_a_second_changes_nothing(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 10 - 01));
    october_fixture(&pool, &setup.source).await;

    let first = setup.import("2026-10", AiMonthMode::Changes).await;
    assert_eq!(first.status, AiRunStatus::Succeeded, "{:?}", first.error);
    assert_eq!(first.kind, AiImportKind::Admin(Some(UserId::new(1))));
    let calls = setup.source.take_calls();
    // Every missing day of the organization and of accounts with usage.
    assert_eq!(day_calls(&calls, "org").len(), 25);
    assert_eq!(day_calls(&calls, "ada-lovelace").len(), 25);
    assert_eq!(day_calls(&calls, "grace-hopper").len(), 25);
    // An empty month total costs no day calls.
    assert_eq!(day_calls(&calls, "alan-turing").len(), 0);
    assert_eq!(month_calls(&calls, "alan-turing"), 1);
    assert_eq!(first.requests as usize, calls.len());
    assert_eq!(calls.len(), 1 + 25 + 1 + 1 + 26 + 26 + 1);
    let accounts = first
        .sources
        .iter()
        .find(|source| source.source == AiMeteredSource::AccountCredits)
        .unwrap();
    assert_eq!(accounts.status, AiSourceStatus::Succeeded);
    // Slices through yesterday were fetched after their day ended.
    assert_eq!(accounts.imported_through, Some(date!(2026 - 10 - 24)));
    assert_eq!(accounts.slices_fetched, 3 * 25);
    let summary = first
        .sources
        .iter()
        .find(|source| source.source == AiMeteredSource::Summary)
        .unwrap();
    assert_eq!(summary.imported_through, None);

    let before = imported_rows(&pool).await;
    setup.at(NOW + Duration::minutes(5));
    let second = setup.import("2026-10", AiMonthMode::Changes).await;
    assert_eq!(second.status, AiRunStatus::Succeeded);
    assert_eq!(second.slices_changed, 0);
    // Matching month totals: month calls only.
    let mut calls = setup.source.take_calls();
    calls.sort();
    assert_eq!(
        calls,
        [
            "credits ada-lovelace 2026-10",
            "credits alan-turing 2026-10",
            "credits grace-hopper 2026-10",
            "credits org 2026-10",
            "summary 2026-10",
            "usage 2026-10",
        ]
    );
    assert_eq!(calls.len(), 6);
    assert_eq!(imported_rows(&pool).await, before);
}

#[sqlx::test]
async fn changes_fetch_open_days_then_the_whole_month(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 10 - 01));
    october_fixture(&pool, &setup.source).await;
    setup.import("2026-10", AiMonthMode::Changes).await;
    setup.source.take_calls();

    // A change on an open day: the open days only.
    setup.at(NOW + Duration::hours(1));
    setup.source.set(
        Some("ada-lovelace"),
        date!(2026 - 10 - 24),
        vec![credit("GPT-6.1 Sol", "7")],
    );
    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    let calls = setup.source.take_calls();
    assert_eq!(
        day_calls(&calls, "ada-lovelace"),
        ["2026-10-22", "2026-10-23", "2026-10-24", "2026-10-25"]
            .map(|day| format!("credits ada-lovelace {day}"))
    );
    assert_eq!(run.slices_changed, 1);

    // A change outside the open days: then every remaining day.
    setup.at(NOW + Duration::hours(2));
    setup.source.set(
        Some("ada-lovelace"),
        date!(2026 - 10 - 03),
        vec![credit("GPT-6.1 Sol", "12")],
    );
    setup.import("2026-10", AiMonthMode::Changes).await;
    let calls = setup.source.take_calls();
    assert_eq!(day_calls(&calls, "ada-lovelace").len(), 25);
    assert_eq!(day_calls(&calls, "grace-hopper").len(), 0);
    let checks = setup
        .repository
        .checks(AiProvider::Copilot, month("2026-10"), None)
        .await
        .unwrap();
    assert!(checks.iter().all(|check| check.matches == Some(true)));

    // Full fetches every day of accounts with usage.
    setup.at(NOW + Duration::hours(3));
    setup.import("2026-10", AiMonthMode::Full).await;
    let calls = setup.source.take_calls();
    assert_eq!(day_calls(&calls, "org").len(), 25);
    assert_eq!(day_calls(&calls, "grace-hopper").len(), 25);
    assert_eq!(day_calls(&calls, "alan-turing").len(), 0);
}

#[sqlx::test]
async fn accounts_count_from_the_backfill_not_from_their_first_seat(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 10 - 01));
    october_fixture(&pool, &setup.source).await;
    // Toki first saw this seat on the 20th, but the account used credits on
    // the 5th under an earlier seat.
    insert_account(
        &pool,
        1004,
        "seat-reassigned",
        Some(datetime!(2026-10-20 08:00 UTC)),
    )
    .await;
    setup.source.set(
        Some("seat-reassigned"),
        date!(2026 - 10 - 05),
        vec![credit("GPT-5.4", "8")],
    );
    setup.import("2026-10", AiMonthMode::Changes).await;
    let stored = setup
        .repository
        .account_credits(
            AiProvider::Copilot,
            month("2026-10").dates(),
            Some(&[AiProviderAccountId::new(1004).unwrap()]),
        )
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].day, date!(2026 - 10 - 05));
}

#[sqlx::test]
async fn a_new_account_costs_its_own_days_never_a_full_refresh(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 09 - 01));
    october_fixture(&pool, &setup.source).await;
    setup.import("2026-09", AiMonthMode::Changes).await;
    setup.import("2026-10", AiMonthMode::Changes).await;
    setup.source.take_calls();

    insert_account(&pool, 1005, "new-listing", None).await;
    setup.source.set(
        Some("new-listing"),
        date!(2026 - 09 - 10),
        vec![credit("GPT-5.4", "3")],
    );
    setup.at(NOW + Duration::hours(1));
    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    // The new account's empty days are gaps in both months.
    assert_eq!(
        run.months,
        [
            (month("2026-09"), AiMonthMode::Changes),
            (month("2026-10"), AiMonthMode::Changes)
        ]
    );
    let calls = setup.source.take_calls();
    let september_days = calls
        .iter()
        .filter(|call| call.contains(" 2026-09-"))
        .count();
    assert_eq!(september_days, 30);
    assert!(calls
        .iter()
        .filter(|call| call.contains(" 2026-09-"))
        .all(|call| call.starts_with("credits new-listing ")));
    // One month total per month for everyone else.
    assert_eq!(month_calls(&calls, "ada-lovelace"), 2);
    // October: an empty month total, no day calls.
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.starts_with("credits new-listing 2026-10"))
            .count(),
        1
    );
}

#[sqlx::test]
async fn a_rate_limit_stops_the_run(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 10 - 01));
    october_fixture(&pool, &setup.source).await;
    setup.source.fail(
        "credits grace-hopper 2026-10",
        MeteredUsageSourceError::RateLimited { retry_after: None },
    );
    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    assert_eq!(run.status, AiRunStatus::Partial);
    assert!(
        run.error.as_deref().unwrap().contains("rate limiting"),
        "{:?}",
        run.error
    );
    let status = setup
        .repository
        .import_status(AiProvider::Copilot)
        .await
        .unwrap();
    assert_eq!(status.last_run.unwrap().status, AiRunStatus::Partial);
    let accounts = run
        .sources
        .iter()
        .find(|source| source.source == AiMeteredSource::AccountCredits)
        .unwrap();
    assert_ne!(accounts.status, AiSourceStatus::Succeeded);
    // The organization finished.
    let org = run
        .sources
        .iter()
        .find(|source| source.source == AiMeteredSource::OrgCredits)
        .unwrap();
    assert_eq!(org.status, AiSourceStatus::Succeeded);
}

#[sqlx::test]
async fn an_account_month_cut_short_by_a_stop_is_no_longer_confirmed(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 10 - 01));
    october_fixture(&pool, &setup.source).await;
    setup.import("2026-10", AiMonthMode::Changes).await;

    // GitHub posts a late correction on Ada's 12th, which the organization's
    // credits include. The organization refreshes, then GitHub rate-limits
    // Ada's day calls after her first days were replaced, before her 12th.
    let later = NOW + Duration::hours(1);
    setup.at(later);
    let twelfth = date!(2026 - 10 - 12);
    setup.source.set(
        Some("ada-lovelace"),
        twelfth,
        vec![credit("GPT-6.1 Sol", "4")],
    );
    setup
        .source
        .set(None, twelfth, vec![credit("GPT-6.1 Sol", "4")]);
    setup.source.fail(
        "credits ada-lovelace 2026-10-05",
        MeteredUsageSourceError::RateLimited { retry_after: None },
    );
    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    assert_eq!(run.status, AiRunStatus::Partial, "{:?}", run.error);
    assert!(run.error.as_deref().unwrap().contains("rate limiting"));
    assert!(setup
        .source
        .calls()
        .contains(&"credits ada-lovelace 2026-10-04".to_string()));

    // No check from before the run survives it: Ada's partly replaced month
    // reads as interrupted, as would any account the stop cut off.
    let checks = setup
        .repository
        .checks(AiProvider::Copilot, month("2026-10"), None)
        .await
        .unwrap();
    assert_eq!(checks.len(), 4);
    assert!(
        checks.iter().all(|check| check.checked_at == later),
        "{checks:?}"
    );
    let ada = checks
        .iter()
        .find(|check| check.account_id == AiProviderAccountId::new(1001))
        .unwrap();
    assert!(
        ada.error.as_deref().unwrap().contains("rate limiting"),
        "{ada:?}"
    );
    assert!(ada.interrupted);
    assert_eq!(ada.matches, None);

    // So the organization's credits on the 12th are an import gap, not
    // unknown account, and Ada's seat makes the import stale.
    let october = setup
        .service
        .reconciliation(month("2026-10"))
        .await
        .unwrap();
    assert!(october
        .reconciliation
        .residual_days
        .iter()
        .all(|day| day.status == AiResidualStatus::Incomplete));
    assert_eq!(october.reconciliation.totals.unknown_account, 0);
    let flagged = october
        .reconciliation
        .accounts
        .iter()
        .find(|account| account.login == "ada-lovelace")
        .unwrap();
    assert!(flagged.flags.contains(&AiReconciliationFlag::ImportFailed));
    assert!(
        matches!(
            &october.import.freshness,
            AiImportFreshness::Stale {
                reason: AiStaleReason::AccountFailed { login, .. }
            } if login.as_str() == "ada-lovelace"
        ),
        "{:?}",
        october.import.freshness
    );

    // The next run imports her 12th and confirms the month again. (Her late
    // correction stretched the settle lag past the 12th, so the day is
    // provisional rather than settled.)
    setup.source.failures.lock().unwrap().clear();
    setup.at(NOW + Duration::hours(2));
    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    assert_eq!(run.status, AiRunStatus::Succeeded, "{:?}", run.error);
    let october = setup
        .service
        .reconciliation(month("2026-10"))
        .await
        .unwrap();
    let ada = october
        .reconciliation
        .accounts
        .iter()
        .find(|account| account.login == "ada-lovelace")
        .unwrap();
    assert_eq!(ada.check.as_ref().unwrap().matches, Some(true));
    let day = october
        .reconciliation
        .residual_days
        .iter()
        .find(|day| day.day == twelfth)
        .unwrap();
    assert_eq!(day.residual, 0);
    assert_ne!(day.status, AiResidualStatus::Incomplete);
    assert_eq!(
        october.reconciliation.totals.unknown_account,
        5 * 10_000_000
    );
}

#[sqlx::test]
async fn a_storage_error_leaves_the_month_it_cut_short_unconfirmed(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 10 - 01));
    october_fixture(&pool, &setup.source).await;
    setup.import("2026-10", AiMonthMode::Changes).await;
    let checks = || async {
        setup
            .repository
            .checks(AiProvider::Copilot, month("2026-10"), None)
            .await
            .unwrap()
    };

    // A full refresh fails to store Ada's 5th, after her first days, while
    // Grace is still fetching hers: neither keeps the check from 10:00,
    // whether Grace finished or was cut short.
    let later = NOW + Duration::hours(1);
    setup.at(later);
    setup.fail_write(Some("ada-lovelace"), date!(2026 - 10 - 05));
    let run = setup.import("2026-10", AiMonthMode::Full).await;
    assert_eq!(run.status, AiRunStatus::Partial, "{:?}", run.error);
    assert!(run.error.as_deref().unwrap().contains("went away"));
    let checks_after = checks().await;
    let ada = checks_after
        .iter()
        .find(|check| check.account_id == AiProviderAccountId::new(1001))
        .unwrap();
    assert_eq!(ada.checked_at, later);
    assert!(ada.interrupted);
    assert!(
        ada.error.as_deref().unwrap().contains("went away"),
        "{ada:?}"
    );
    assert!(
        checks_after.iter().all(|check| check.checked_at == later),
        "{checks_after:?}"
    );

    // Then one fails to store the organization's 5th: the organization's
    // check fails, and every account's, which the run never reached.
    let latest = NOW + Duration::hours(2);
    setup.at(latest);
    setup.fail_write(None, date!(2026 - 10 - 05));
    let run = setup.import("2026-10", AiMonthMode::Full).await;
    assert_eq!(run.status, AiRunStatus::Failed, "{:?}", run.error);
    let checks_after = checks().await;
    assert_eq!(checks_after.len(), 4);
    assert!(
        checks_after
            .iter()
            .all(|check| check.checked_at == latest && check.interrupted),
        "{checks_after:?}"
    );
    let org = checks_after
        .iter()
        .find(|check| check.account_id.is_none())
        .unwrap();
    assert!(
        org.error.as_deref().unwrap().contains("went away"),
        "{org:?}"
    );
}

#[sqlx::test]
async fn a_month_a_stop_cut_short_is_imported_again(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 09 - 01));
    october_fixture(&pool, &setup.source).await;
    // Zed has no seat in September (seat history starts later) but used
    // Copilot on the 3rd and the 20th, which the organization's credits hold.
    insert_account(&pool, 1004, "zed-rehired", None).await;
    let (third, twentieth) = (date!(2026 - 09 - 03), date!(2026 - 09 - 20));
    for day in [third, twentieth] {
        setup
            .source
            .set(Some("zed-rehired"), day, vec![credit("GPT-6.1 Sol", "7")]);
        setup
            .source
            .set(None, day, vec![credit("GPT-6.1 Sol", "7")]);
    }
    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    assert_eq!(run.status, AiRunStatus::Succeeded, "{:?}", run.error);

    // GitHub corrects Zed's 20th. A full refresh of September refreshes the
    // organization, then GitHub rate-limits Zed's 10th, before his 20th.
    setup.at(NOW + Duration::hours(1));
    setup.source.set(
        Some("zed-rehired"),
        twentieth,
        vec![credit("GPT-6.1 Sol", "9")],
    );
    setup
        .source
        .set(None, twentieth, vec![credit("GPT-6.1 Sol", "9")]);
    setup.source.fail(
        "credits zed-rehired 2026-09-10",
        MeteredUsageSourceError::RateLimited { retry_after: None },
    );
    let run = setup.import("2026-09", AiMonthMode::Full).await;
    assert_eq!(run.status, AiRunStatus::Partial, "{:?}", run.error);

    // The stop is no answer of Zed's own, so his missing seat does not make
    // it informational: he is flagged, the 20th is an import gap rather
    // than unknown account, and the import is stale.
    let september = setup
        .service
        .reconciliation(month("2026-09"))
        .await
        .unwrap();
    let zed = september
        .reconciliation
        .accounts
        .iter()
        .find(|account| account.login == "zed-rehired")
        .unwrap();
    assert!(zed.check.as_ref().unwrap().interrupted, "{zed:?}");
    assert!(zed.flags.contains(&AiReconciliationFlag::ImportFailed));
    assert!(september
        .reconciliation
        .residual_days
        .iter()
        .all(|day| day.status == AiResidualStatus::Incomplete));
    assert_eq!(september.reconciliation.totals.unknown_account, 0);
    assert!(
        matches!(
            &september.import.freshness,
            AiImportFreshness::Stale {
                reason: AiStaleReason::AccountFailed { login, .. }
            } if login.as_str() == "zed-rehired"
        ),
        "{:?}",
        september.import.freshness
    );
    let error = &september.import.account_errors[0];
    assert!(error.interrupted && !error.seated, "{error:?}");

    // Though every September slice was fetched, the next run imports the
    // month again and confirms it.
    setup.source.failures.lock().unwrap().clear();
    setup.at(NOW + Duration::hours(2));
    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    assert_eq!(
        run.months,
        [
            (month("2026-09"), AiMonthMode::Changes),
            (month("2026-10"), AiMonthMode::Changes)
        ]
    );
    assert_eq!(run.status, AiRunStatus::Succeeded, "{:?}", run.error);
    let september = setup
        .service
        .reconciliation(month("2026-09"))
        .await
        .unwrap();
    let zed = september
        .reconciliation
        .accounts
        .iter()
        .find(|account| account.login == "zed-rehired")
        .unwrap();
    assert_eq!(zed.check.as_ref().unwrap().matches, Some(true));
    assert!(!zed.flags.contains(&AiReconciliationFlag::ImportFailed));
    let day = september
        .reconciliation
        .residual_days
        .iter()
        .find(|day| day.day == twentieth)
        .unwrap();
    assert_eq!(day.residual, 0);
    assert_ne!(day.status, AiResidualStatus::Incomplete);
    assert_eq!(september.reconciliation.totals.unknown_account, 0);

    // Done: the run after that is the current month's alone.
    setup.at(NOW + Duration::hours(3));
    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    assert_eq!(run.months, [(month("2026-10"), AiMonthMode::Changes)]);
}

#[sqlx::test]
async fn an_interrupted_account_without_a_seat_is_still_expected(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 09 - 01));
    october_fixture(&pool, &setup.source).await;
    insert_account(&pool, 1004, "zed-rehired", None).await;
    setup.source.set(
        Some("zed-rehired"),
        date!(2026 - 09 - 20),
        vec![credit("GPT-6.1 Sol", "7")],
    );
    let account_coverage = |run: &AiImportRun| {
        run.sources
            .iter()
            .find(|source| source.source == AiMeteredSource::AccountCredits)
            .unwrap()
            .imported_through
    };
    // GitHub answers 404 for Zed's September, his own error without a seat
    // that month: informational, so his days are not expected.
    setup.source.fail(
        "credits zed-rehired 2026-09",
        MeteredUsageSourceError::NotFound("GitHub knows no Copilot usage".to_string()),
    );
    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    assert_eq!(account_coverage(&run), Some(date!(2026 - 10 - 24)));

    // Then his month imports, until GitHub rate-limits his 10th: the stop
    // is no answer of his own, so his days are expected again and hold the
    // coverage back.
    setup.source.failures.lock().unwrap().clear();
    setup.source.fail(
        "credits zed-rehired 2026-09-10",
        MeteredUsageSourceError::RateLimited { retry_after: None },
    );
    setup.at(NOW + Duration::hours(1));
    let run = setup.import("2026-09", AiMonthMode::Changes).await;
    assert_eq!(run.status, AiRunStatus::Partial, "{:?}", run.error);
    assert_eq!(account_coverage(&run), Some(date!(2026 - 09 - 09)));
}

#[sqlx::test]
async fn a_run_whose_process_died_leaves_its_month_interrupted(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 10 - 01));
    october_fixture(&pool, &setup.source).await;
    setup.import("2026-10", AiMonthMode::Changes).await;
    let october = month("2026-10");

    // GitHub posts a late correction on Ada's 12th. A run marks October,
    // refreshes the organization's 12th and is still importing.
    let later = NOW + Duration::hours(1);
    setup.at(later);
    let twelfth = date!(2026 - 10 - 12);
    let corrected = vec![credit("GPT-6.1 Sol", "4")];
    setup
        .source
        .set(Some("ada-lovelace"), twelfth, corrected.clone());
    setup.source.set(None, twelfth, corrected.clone());
    let lease = setup
        .repository
        .try_lock_import(AiProvider::Copilot)
        .await
        .unwrap()
        .unwrap();
    let plan = AiImportPlan {
        kind: AiImportKind::Hourly,
        months: vec![(october, AiMonthMode::Changes)],
    };
    let run_id = setup
        .repository
        .start_run(AiProvider::Copilot, &plan, later)
        .await
        .unwrap();
    setup
        .repository
        .mark_checks_importing(AiProvider::Copilot, october, run_id)
        .await
        .unwrap();
    setup
        .repository
        .replace_credit_days(
            AiProvider::Copilot,
            AiCreditStoreScope::Organization,
            &[(twelfth, corrected)],
            later,
        )
        .await
        .unwrap();

    // While it runs, its marks keep the checks they marked.
    let running = setup.service.reconciliation(october).await.unwrap();
    assert_eq!(running.import.running.as_ref().unwrap().id, run_id);
    let ada = running
        .reconciliation
        .accounts
        .iter()
        .find(|account| account.login == "ada-lovelace")
        .unwrap();
    assert_eq!(ada.check.as_ref().unwrap().matches, Some(true));
    assert!(!ada.flags.contains(&AiReconciliationFlag::ImportFailed));

    // Its process dies, freeing the lock. Its marks are interrupted: the 12th
    // is an import gap, not unknown account, and the import is stale.
    lease.release().await.unwrap();
    let died = setup.service.reconciliation(october).await.unwrap();
    assert!(died.import.running.is_none());
    let checks = setup
        .repository
        .checks(AiProvider::Copilot, october, None)
        .await
        .unwrap();
    assert_eq!(checks.len(), 4);
    assert!(
        checks.iter().all(|check| check.interrupted
            && check.checked_at == later
            && check.error.as_deref()
                == Some("the run stopped before finishing this month: interrupted")),
        "{checks:?}"
    );
    let ada = died
        .reconciliation
        .accounts
        .iter()
        .find(|account| account.login == "ada-lovelace")
        .unwrap();
    assert!(ada.flags.contains(&AiReconciliationFlag::ImportFailed));
    assert!(died
        .reconciliation
        .residual_days
        .iter()
        .all(|day| day.status == AiResidualStatus::Incomplete));
    assert_eq!(died.reconciliation.totals.unknown_account, 0);
    assert!(
        matches!(
            &died.import.freshness,
            AiImportFreshness::Stale {
                reason: AiStaleReason::AccountFailed { login, .. }
            } if login.as_str() == "ada-lovelace"
        ),
        "{:?}",
        died.import.freshness
    );

    // The next run marks the dead run interrupted, imports Ada's 12th and
    // confirms the month again.
    setup.at(NOW + Duration::hours(2));
    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    assert_eq!(run.status, AiRunStatus::Succeeded, "{:?}", run.error);
    let october = setup.service.reconciliation(october).await.unwrap();
    assert_eq!(october.import.freshness, AiImportFreshness::Fresh);
    assert!(october.reconciliation.accounts.iter().all(|account| account
        .check
        .as_ref()
        .unwrap()
        .matches
        == Some(true)));
    let day = october
        .reconciliation
        .residual_days
        .iter()
        .find(|day| day.day == twelfth)
        .unwrap();
    assert_eq!(day.residual, 0);
}

#[sqlx::test]
async fn three_unavailable_answers_in_a_row_stop_the_run(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 10 - 01));
    insert_admin(&pool).await;
    for id in 1..=6 {
        insert_account(&pool, 2000 + id, &format!("person-{id}"), None).await;
    }
    // Every account's empty month is confirmed first.
    setup.import("2026-10", AiMonthMode::Changes).await;
    setup.at(NOW + Duration::hours(1));
    setup.source.take_calls();
    for id in 1..=6 {
        setup.source.fail(
            &format!("credits person-{id} 2026-10"),
            MeteredUsageSourceError::Unavailable("GitHub answered 502".to_string()),
        );
    }
    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    assert_eq!(run.status, AiRunStatus::Partial);
    assert!(run.error.as_deref().unwrap().contains("unavailable"));
    assert_eq!(run.account_errors, 2);
    let account_calls = setup
        .source
        .calls()
        .iter()
        .filter(|call| call.starts_with("credits person-"))
        .count();
    assert!(account_calls < 6, "{account_calls}");
    // No month stays confirmed: two accounts failed on their own answers,
    // and the stop left the others' checks interrupted.
    let checks = setup
        .repository
        .checks(AiProvider::Copilot, month("2026-10"), None)
        .await
        .unwrap();
    let accounts: Vec<&AiUsageCheck> = checks
        .iter()
        .filter(|check| check.account_id.is_some())
        .collect();
    assert_eq!(accounts.len(), 6, "{checks:?}");
    assert!(accounts
        .iter()
        .all(|check| check.matches.is_none() && check.error.is_some()));
    let interrupted = accounts.iter().filter(|check| check.interrupted).count();
    assert_eq!(interrupted, 4, "{checks:?}");
}

#[sqlx::test]
async fn an_account_error_stays_with_the_account(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 10 - 01));
    october_fixture(&pool, &setup.source).await;
    setup.source.fail(
        "credits grace-hopper 2026-10",
        MeteredUsageSourceError::NotFound(
            "GitHub knows no Copilot usage for grace-hopper".to_string(),
        ),
    );
    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    assert_eq!(run.status, AiRunStatus::Partial);
    assert_eq!(run.account_errors, 1);
    assert_eq!(run.error, None);
    let checks = setup
        .repository
        .checks(AiProvider::Copilot, month("2026-10"), None)
        .await
        .unwrap();
    let grace = checks
        .iter()
        .find(|check| check.account_id == AiProviderAccountId::new(1002))
        .unwrap();
    assert!(grace.error.as_deref().unwrap().contains("grace-hopper"));
    let ada = checks
        .iter()
        .find(|check| check.account_id == AiProviderAccountId::new(1001))
        .unwrap();
    assert_eq!(ada.matches, Some(true));
    // Grace held a seat: her missing days make the import stale, her error
    // is listed and her account flagged.
    let october = setup
        .service
        .reconciliation(month("2026-10"))
        .await
        .unwrap();
    assert!(
        matches!(&october.import.freshness, AiImportFreshness::Stale { .. }),
        "{:?}",
        october.import.freshness
    );
    let flagged = october
        .reconciliation
        .accounts
        .iter()
        .find(|account| account.login == "grace-hopper")
        .unwrap();
    assert!(flagged
        .flags
        .contains(&crate::domain::models::AiReconciliationFlag::ImportFailed));
    assert_eq!(october.import.account_errors.len(), 1);
    assert!(october.import.account_errors[0].seated);
}

#[sqlx::test]
async fn a_failed_organization_import_leaves_the_residual_unconfirmed(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 10 - 01));
    october_fixture(&pool, &setup.source).await;
    setup.import("2026-10", AiMonthMode::Changes).await;
    let third = date!(2026 - 10 - 03);
    let residual = |october: &AiMeteredMonth| {
        october
            .reconciliation
            .residual_days
            .iter()
            .find(|day| day.day == third)
            .map(|day| (day.residual, day.status))
            .unwrap()
    };
    let october = setup
        .service
        .reconciliation(month("2026-10"))
        .await
        .unwrap();
    assert_eq!(
        residual(&october),
        (5 * 10_000_000, AiResidualStatus::Settled)
    );

    // GitHub restates the 3rd, and fetching it fails after the month total
    // showed the change: the run stops and the organization's check fails.
    setup.at(NOW + Duration::hours(1));
    setup
        .source
        .set(None, third, vec![credit("GPT-6.1 Sol", "16")]);
    let unavailable = MeteredUsageSourceError::Unavailable("GitHub answered 502".to_string());
    setup.source.fail("credits org 2026-10-03", unavailable);
    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    assert_eq!(run.status, AiRunStatus::Failed, "{:?}", run.error);
    let october = setup
        .service
        .reconciliation(month("2026-10"))
        .await
        .unwrap();
    let check = october.reconciliation.org_check.as_ref().unwrap();
    assert!(check.error.as_deref().unwrap().contains("502"), "{check:?}");
    assert_eq!(check.matches, None);
    // An import gap on every day, not unknown account.
    assert!(october
        .reconciliation
        .residual_days
        .iter()
        .all(|day| day.status == AiResidualStatus::Incomplete));
    assert_eq!(october.reconciliation.totals.unknown_account, 0);

    // The next run fetches the 3rd and confirms the month again.
    setup.source.failures.lock().unwrap().clear();
    setup.at(NOW + Duration::hours(2));
    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    assert_eq!(run.status, AiRunStatus::Succeeded, "{:?}", run.error);
    let october = setup
        .service
        .reconciliation(month("2026-10"))
        .await
        .unwrap();
    let check = october.reconciliation.org_check.as_ref().unwrap();
    assert_eq!(check.matches, Some(true));
    assert_eq!(check.payer_gross, Some(dec("42")));
    assert_eq!(
        residual(&october),
        (6 * 10_000_000, AiResidualStatus::Settled)
    );
}

#[sqlx::test]
async fn a_departed_account_that_fails_never_makes_the_import_stale(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 09 - 01));
    october_fixture(&pool, &setup.source).await;
    // Kathleen's seat ended in August and her login no longer exists, so
    // GitHub answers 404 for every month.
    insert_account(&pool, 1004, "kathleen-booth", None).await;
    sqlx::query(
        "INSERT INTO ai_provider_seats (provider, account_id, seat_created_at, first_listed_at,
            last_listed_at, unlisted_at)
         VALUES ('copilot', 1004, '2026-06-01T08:00:00Z', '2026-06-01T08:00:00Z',
            '2026-08-19T08:00:00Z', '2026-08-20T08:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    for month_raw in ["2026-09", "2026-10"] {
        setup.source.fail(
            &format!("credits kathleen-booth {month_raw}"),
            MeteredUsageSourceError::NotFound(
                "GitHub knows no Copilot usage for kathleen-booth".to_string(),
            ),
        );
    }

    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    assert_eq!(
        run.months,
        [
            (month("2026-09"), AiMonthMode::Changes),
            (month("2026-10"), AiMonthMode::Changes)
        ]
    );
    assert_eq!(run.account_errors, 2);
    // Her missing days are not expected, as in the residual's coverage, so
    // account credits are imported through yesterday.
    let accounts = run
        .sources
        .iter()
        .find(|source| source.source == AiMeteredSource::AccountCredits)
        .unwrap();
    assert_eq!(accounts.imported_through, Some(date!(2026 - 10 - 24)));
    let october = setup
        .service
        .reconciliation(month("2026-10"))
        .await
        .unwrap();
    assert_eq!(october.import.freshness, AiImportFreshness::Fresh);
    // Listed for both months, never seated.
    assert_eq!(october.import.account_errors.len(), 2);
    assert!(october
        .import
        .account_errors
        .iter()
        .all(|error| error.login.as_str() == "kathleen-booth" && !error.seated));

    // Nor are her months gaps: the next hourly run is the current month's.
    setup.at(NOW + Duration::hours(1));
    setup.source.take_calls();
    let run = setup.service.import_if_due().await.unwrap().unwrap();
    assert_eq!(run.kind, AiImportKind::Nightly);
    let run_months: Vec<AiBillingMonth> = run.months.iter().map(|(month, _)| *month).collect();
    assert_eq!(run_months, [month("2026-09"), month("2026-10")]);
    setup.at(NOW + Duration::hours(2));
    let run = setup.service.import_if_due().await.unwrap().unwrap();
    assert_eq!(run.kind, AiImportKind::Hourly);
    assert_eq!(run.months, [(month("2026-10"), AiMonthMode::Changes)]);
}

#[sqlx::test]
async fn one_import_runs_at_a_time_across_instances(pool: PgPool) {
    let setup_one = setup(&pool, date!(2026 - 10 - 01));
    insert_admin(&pool).await;
    let ticket = setup_one
        .service
        .begin_import(month("2026-10"), AiMonthMode::Changes, UserId::new(1))
        .await
        .unwrap();
    assert!(matches!(
        setup_one
            .service
            .begin_import(month("2026-10"), AiMonthMode::Changes, UserId::new(1))
            .await,
        Err(AiMeteredUsageError::ImportRunning)
    ));
    // Another instance on the same database.
    let setup_two = setup(&pool, date!(2026 - 10 - 01));
    assert!(matches!(
        setup_two
            .service
            .begin_import(month("2026-10"), AiMonthMode::Changes, UserId::new(1))
            .await,
        Err(AiMeteredUsageError::ImportRunning)
    ));
    assert!(setup_two.service.import_if_due().await.unwrap().is_none());
    setup_one.service.run_import(ticket).await.unwrap();
    // Free again.
    setup_two.at(NOW + Duration::hours(1));
    let ticket = setup_two
        .service
        .begin_import(month("2026-10"), AiMonthMode::Changes, UserId::new(1))
        .await
        .unwrap();
    setup_two.service.run_import(ticket).await.unwrap();
    // Outside the imported months.
    assert!(matches!(
        setup_two
            .service
            .begin_import(month("2026-08"), AiMonthMode::Changes, UserId::new(1))
            .await,
        Err(AiMeteredUsageError::Invalid(_))
    ));
}

#[sqlx::test]
async fn the_scheduler_imports_only_when_due(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 10 - 01));
    october_fixture(&pool, &setup.source).await;
    // After 02:00 without a nightly run: the nightly full refresh.
    let run = setup.service.import_if_due().await.unwrap().unwrap();
    assert_eq!(run.kind, AiImportKind::Nightly);
    assert_eq!(
        run.months,
        [
            (month("2026-09"), AiMonthMode::Full),
            (month("2026-10"), AiMonthMode::Full)
        ]
        .into_iter()
        .filter(|(month, _)| month.first_day() >= date!(2026 - 10 - 01))
        .collect::<Vec<_>>()
    );
    // Too soon for another.
    setup.at(NOW + Duration::minutes(30));
    assert!(setup.service.import_if_due().await.unwrap().is_none());
    // An hour later: hourly change detection.
    setup.at(NOW + Duration::hours(1));
    let run = setup.service.import_if_due().await.unwrap().unwrap();
    assert_eq!(run.kind, AiImportKind::Hourly);
    assert_eq!(run.status, AiRunStatus::Succeeded);
}

#[sqlx::test]
async fn a_nightly_run_that_stopped_is_retried_in_full(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 10 - 01));
    october_fixture(&pool, &setup.source).await;
    // The organization's steps finish, then GitHub rate-limits an account.
    setup.source.fail(
        "credits grace-hopper 2026-10",
        MeteredUsageSourceError::RateLimited { retry_after: None },
    );
    let run = setup.service.import_if_due().await.unwrap().unwrap();
    assert_eq!(run.kind, AiImportKind::Nightly);
    assert_eq!(run.status, AiRunStatus::Partial, "{:?}", run.error);

    // The full refresh did not finish, so the next scheduled run retries it
    // instead of detecting changes, which misses days whose usage moved
    // without changing the month totals.
    setup.source.failures.lock().unwrap().clear();
    setup.at(NOW + Duration::hours(1));
    let run = setup.service.import_if_due().await.unwrap().unwrap();
    assert_eq!(run.kind, AiImportKind::Nightly);
    assert_eq!(run.months, [(month("2026-10"), AiMonthMode::Full)]);
    assert_eq!(run.status, AiRunStatus::Succeeded, "{:?}", run.error);

    // Done for the night.
    setup.at(NOW + Duration::hours(2));
    let run = setup.service.import_if_due().await.unwrap().unwrap();
    assert_eq!(run.kind, AiImportKind::Hourly);
}

#[sqlx::test]
async fn coverage_and_the_settle_lag_are_recorded_per_source(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 10 - 01));
    october_fixture(&pool, &setup.source).await;
    setup.import("2026-10", AiMonthMode::Changes).await;

    // A day that was complete changes a day later: a lag of 3 days.
    setup.at(datetime!(2026-10-26 10:00 UTC));
    setup.source.set(
        Some("ada-lovelace"),
        date!(2026 - 10 - 23),
        vec![credit("GPT-6.1 Sol", "2")],
    );
    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    let accounts = run
        .sources
        .iter()
        .find(|source| source.source == AiMeteredSource::AccountCredits)
        .unwrap();
    assert_eq!(accounts.settle_lag_days, Some(3));
    // Grace's unchanged month was not fetched again, so yesterday is not
    // complete for her until the nightly refresh.
    assert_eq!(accounts.imported_through, Some(date!(2026 - 10 - 24)));
    let status = setup
        .repository
        .import_status(AiProvider::Copilot)
        .await
        .unwrap();
    assert_eq!(status.settle_days(), 3);
    let org = status
        .sources
        .iter()
        .find(|state| state.source == AiMeteredSource::OrgCredits)
        .unwrap();
    assert_eq!(org.imported_through, Some(date!(2026 - 10 - 24)));
    assert_eq!(org.settle_lag_days, None);
}

#[sqlx::test]
async fn reconciliation_equals_github_for_every_account(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 10 - 01));
    october_fixture(&pool, &setup.source).await;
    setup.import("2026-10", AiMonthMode::Changes).await;

    let october = setup
        .service
        .reconciliation(month("2026-10"))
        .await
        .unwrap();
    assert_eq!(october.namespace, ORG);
    assert_eq!(october.months, [month("2026-10")]);
    assert_eq!(october.import.health, AiMeteredSourceHealth::Ready);
    assert_eq!(october.import.freshness, AiImportFreshness::Fresh);
    let reconciliation = &october.reconciliation;
    for login in ["ada-lovelace", "grace-hopper"] {
        let account = reconciliation
            .accounts
            .iter()
            .find(|account| account.login == login)
            .unwrap();
        let check = account.check.as_ref().unwrap();
        assert_eq!(check.matches, Some(true), "{login}");
        let github =
            credit_totals(&setup.source.month_rows(Some(login), month("2026-10"))).unwrap();
        assert_eq!(check.payer_gross, total_gross_quantity(&github), "{login}");
        assert_eq!(check.payer_gross, Some(check.stored_gross));
    }
    // The organization's stored month equals GitHub's, so the residual
    // rests on a confirmed total: its five extra credits on the 3rd are
    // unknown account.
    let org_check = reconciliation.org_check.as_ref().unwrap();
    assert_eq!(org_check.matches, Some(true));
    let github = credit_totals(&setup.source.month_rows(None, month("2026-10"))).unwrap();
    assert_eq!(org_check.payer_gross, total_gross_quantity(&github));
    assert_eq!(org_check.payer_gross, Some(org_check.stored_gross));
    let third = reconciliation
        .residual_days
        .iter()
        .find(|day| day.day == date!(2026 - 10 - 03))
        .unwrap();
    assert_eq!(third.residual, 5 * 10_000_000);
    assert_eq!(third.status, AiResidualStatus::Settled);
    assert_eq!(reconciliation.totals.unknown_account, 5 * 10_000_000);
    assert_eq!(reconciliation.repositories.len(), 1);
    assert!(reconciliation.pool.is_some());
    // Equal to reconciling the stored data directly.
    let direct = reconcile(AiReconciliationInput {
        payer: AiProvider::Copilot,
        month: month("2026-10"),
        today: date!(2026 - 10 - 25),
        settle_days: 3,
        accounts: PostgresAiProviderAccountRepository::new(DbPool::from(pool.clone()))
            .accounts(AiProvider::Copilot)
            .await
            .unwrap(),
        account_credits: setup
            .repository
            .account_credits(AiProvider::Copilot, month("2026-10").dates(), None)
            .await
            .unwrap(),
        org_credits: setup
            .repository
            .org_credits(AiProvider::Copilot, month("2026-10").dates())
            .await
            .unwrap(),
        org_charges: setup
            .repository
            .org_charges(AiProvider::Copilot, month("2026-10").dates())
            .await
            .unwrap(),
        summary: setup
            .repository
            .summary(AiProvider::Copilot, month("2026-10"))
            .await
            .unwrap(),
        tracked: Vec::new(),
        checks: setup
            .repository
            .checks(AiProvider::Copilot, month("2026-10"), None)
            .await
            .unwrap(),
        coverage: reconciliation
            .residual_days
            .iter()
            .map(|day| {
                (
                    day.day,
                    AiDayCoverage {
                        org_fetched: true,
                        org_complete: day.day < date!(2026 - 10 - 25),
                        org_usage_fetched: true,
                        org_usage_complete: day.day < date!(2026 - 10 - 25),
                        accounts_missing: 0,
                        accounts_incomplete: u32::from(day.day == date!(2026 - 10 - 25)) * 3,
                    },
                )
            })
            .collect(),
        account_slices: setup
            .repository
            .slices(
                AiProvider::Copilot,
                AiMeteredSource::AccountCredits,
                month("2026-10").dates(),
            )
            .await
            .unwrap()
            .into_iter()
            .filter_map(|slice| Some(((slice.account_id?, slice.day), slice.is_complete())))
            .collect(),
        focus: None,
        repository_mappings: AiRepositoryResolver::default(),
    })
    .unwrap();
    assert_eq!(&direct, reconciliation);

    // One developer's view needs a known user.
    assert!(matches!(
        setup
            .service
            .developer_reconciliation(month("2026-10"), UserId::new(99))
            .await,
        Err(AiMeteredUsageError::UserNotFound)
    ));
}

#[sqlx::test]
async fn days_before_the_backfill_are_never_fetched(pool: PgPool) {
    let setup = setup(&pool, date!(2026 - 10 - 20));
    october_fixture(&pool, &setup.source).await;
    let run = setup.import("2026-10", AiMonthMode::Changes).await;
    let calls = setup.source.take_calls();
    // The 20th through today, whatever the month totals say.
    assert_eq!(day_calls(&calls, "org").len(), 6);
    assert_eq!(day_calls(&calls, "ada-lovelace").len(), 6);
    assert_eq!(day_calls(&calls, "grace-hopper").len(), 6);
    assert!(calls
        .iter()
        .all(|call| !call.contains(" 2026-10-0") && !call.contains(" 2026-10-1")));
    // Ada's usage on the 3rd is in GitHub's month total but never imported.
    let checks = setup
        .repository
        .checks(AiProvider::Copilot, month("2026-10"), None)
        .await
        .unwrap();
    let ada = checks
        .iter()
        .find(|check| check.account_id == AiProviderAccountId::new(1001))
        .unwrap();
    assert_eq!(ada.matches, Some(false));
    assert_eq!(run.status, AiRunStatus::Succeeded);
    let usage_days = setup
        .repository
        .slices(
            AiProvider::Copilot,
            AiMeteredSource::OrgUsage,
            month("2026-10").dates(),
        )
        .await
        .unwrap();
    assert_eq!(usage_days.first().unwrap().day, date!(2026 - 10 - 20));
}
