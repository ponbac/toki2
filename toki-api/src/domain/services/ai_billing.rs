use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use time::{Date, OffsetDateTime};
use tracing::Instrument;

use super::{AiExchangeRates, AiMeteredMonths};
use crate::domain::{
    models::{
        add_metered_lines, adjustment_lines, allocate_metered_credits, assess_completeness,
        bill_month, billed_day_usage, close_blocker, convert_bill, currencies_to_convert,
        exchange_rate_check, for_user, metered_checks, metered_not_applicable, next_open,
        posting_on, pricing_check, reprice_floor, uploads_check, utc_day, utc_spans_outside,
        AiAdjustedMonth, AiAdjustmentLine, AiBilledEntry, AiBillingDeveloper, AiBillingLinePart,
        AiBillingMonth, AiCloseBlocker, AiCloseReadiness, AiCloseWaiver, AiClosedDeveloperBill,
        AiClosedMonth, AiConvertedBill, AiCurrency, AiDeveloperBill, AiDeveloperMonth, AiEntrySek,
        AiExchangeRate, AiExchangeRateValue, AiImportRun, AiKeySubject, AiMachineLabel,
        AiMeteredAllocation, AiMeteredCutover, AiMonthBill, AiMonthClose, AiMonthCloseState,
        AiMonthCloseStatus, AiMonthCompleteness, AiMonthOverview, AiMonthView, AiNewClose,
        AiNewSnapshotLine, AiPersonalUsage, AiReconciliationInput, AiSubscription,
        AiSubscriptionId, AiSubscriptionScope, AiUsageTimeZone, TimeTrackingCompany, UserId,
    },
    ports::{
        inbound::{AiBillProjector, AiBillingService, AiProjectionInput},
        outbound::{
            AiBillingRepository, AiExchangeRateRepository, AiMeteredUsageRepository,
            AiMonthCloseRepository, AiProviderAccountRepository, AiSubscriptionRepository,
        },
    },
    AiBillingError, AiMonthCloseError, AiSubscriptionError,
};

pub struct AiBillingServiceImpl<R, S, X, M, A, C> {
    repository: Arc<R>,
    subscriptions: Arc<S>,
    rates: AiExchangeRates<X>,
    time_zone: AiUsageTimeZone,
    mapping_company: Option<TimeTrackingCompany>,
    /// The metered payer's months, read as reconciliation reads them.
    metered: AiMeteredMonths<M, A>,
    cutover: Option<AiMeteredCutover>,
    closes: Arc<C>,
    /// The configured reprice floor, which a month close raises
    /// (`reprice_floor_now`).
    reprice_floor: OffsetDateTime,
}

/// A month's own bill, before adjustments and conversion, with what it was
/// read from.
struct OwnBill {
    bill: AiMonthBill,
    personal: Vec<AiPersonalUsage>,
    /// Every user's subscriptions, or the one user's.
    subscriptions: Vec<AiSubscription>,
    /// The metered payer's loaded month and allocation (over every account),
    /// when the month bills from its credits.
    metered: Option<MeteredMonth>,
}

/// The metered payer's loaded month as the bill reads it: its input, read
/// with the live run's marks keeping the checks they marked, and its
/// allocation over every account.
struct MeteredMonth {
    input: AiReconciliationInput,
    allocation: AiMeteredAllocation,
    /// The payer's live import run, if any, which readiness refuses while
    /// it imports the month.
    running: Option<AiImportRun>,
}

/// An open month as the overview shows it, with what readiness reads.
struct OpenMonth {
    overview: AiMonthOverview,
    metered: Option<MeteredMonth>,
}

/// The store's clock for close: today in UTC and in the billing time zone,
/// and when the month is over in both.
struct CloseClock {
    now: OffsetDateTime,
    today_utc: Date,
    today_local: Date,
    over_at: OffsetDateTime,
}

impl<R, S, X: AiExchangeRateRepository, M, A, C> AiBillingServiceImpl<R, S, X, M, A, C> {
    /// `time_zone` defines usage days and billing months. Only project mappings
    /// to `mapping_company`, the configured time-tracking company, resolve; with
    /// `None`, all usage is Unassigned. Exchange rates are stored and fetched
    /// through `rates`, which bounds the fetches. From `cutover` on, the
    /// metered payer's usage bills from its credits, read through `metered`;
    /// `None` keeps every month on estimates. Month closes and their
    /// snapshots are kept in `closes`. Buckets before `reprice_floor`, or
    /// before the first open month at or after its month, are never
    /// repriced, so the bill and close readiness take them as they are.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repository: Arc<R>,
        subscriptions: Arc<S>,
        rates: AiExchangeRates<X>,
        time_zone: AiUsageTimeZone,
        mapping_company: Option<TimeTrackingCompany>,
        metered: AiMeteredMonths<M, A>,
        cutover: Option<AiMeteredCutover>,
        closes: Arc<C>,
        reprice_floor: OffsetDateTime,
    ) -> Self {
        Self {
            repository,
            subscriptions,
            rates,
            time_zone,
            mapping_company,
            metered,
            cutover,
            closes,
            reprice_floor,
        }
    }

    /// The bill in the billing currency, at the month's rates.
    async fn convert(&self, bill: &AiMonthBill) -> Result<AiConvertedBill, AiBillingError> {
        let rates = self
            .rates
            .month_rates(bill.month, &currencies_to_convert(bill))
            .await?;
        convert_bill(bill, &rates.rates, &rates.pending)
    }

    /// The month's rates as stored now, fetching none.
    async fn stored_rates(
        &self,
        month: AiBillingMonth,
    ) -> Result<Vec<AiExchangeRate>, AiBillingError> {
        Ok(self.rates.month_rates(month, &[]).await?.rates)
    }
}

impl<R, S: AiSubscriptionRepository, X, M, A, C> AiBillingServiceImpl<R, S, X, M, A, C> {
    async fn subscriptions(
        &self,
        scope: AiSubscriptionScope,
    ) -> Result<Vec<AiSubscription>, AiBillingError> {
        self.subscriptions
            .list(scope)
            .await
            .map_err(|error| match error {
                AiSubscriptionError::Storage(message) => AiBillingError::Storage(message),
                other => AiBillingError::Storage(other.to_string()),
            })
    }
}

/// Every part of a bill's own lines as the netting sees it.
fn bill_entries(
    bill: &AiMonthBill,
    converted: &AiConvertedBill,
) -> Result<Vec<AiBilledEntry>, AiBillingError> {
    let providers: HashMap<AiSubscriptionId, _> = bill
        .subscriptions
        .iter()
        .map(|month| (month.subscription.id, month.subscription.terms.provider))
        .collect();
    let mut entries = Vec::new();
    for (line, converted) in bill.lines.iter().zip(&converted.lines) {
        for part in line.parts(converted)? {
            entries.push(AiBilledEntry::of_part(&part, |id| {
                providers.get(&id).copied()
            })?);
        }
    }
    Ok(entries)
}

/// An adjustment line as a snapshot row's entry.
fn adjustment_entry(line: &AiAdjustmentLine) -> AiBilledEntry {
    AiBilledEntry {
        key: line.key.clone(),
        subject: line.subject.clone(),
        provider: line.provider,
        project: line.project.clone(),
        unassigned: line.unassigned,
        amount: Some(line.amount.hundredths),
        sek: AiEntrySek::of(&line.converted),
        rate: line.converted.rate().and_then(AiExchangeRate::rate),
        credits: line.credits,
    }
}

impl<R, S, X, M, A, C> AiBillingServiceImpl<R, S, X, M, A, C>
where
    R: AiBillingRepository,
    S: AiSubscriptionRepository,
    X: AiExchangeRateRepository,
    M: AiMeteredUsageRepository,
    A: AiProviderAccountRepository,
    C: AiMonthCloseRepository,
{
    /// The earliest hour a reprice may change now (`reprice_floor`), as the
    /// usage service applies it: no bucket before it is on an old basis that
    /// a reprice could still fix.
    async fn reprice_floor_now(&self) -> Result<OffsetDateTime, AiBillingError> {
        let open_from = self
            .closes
            .open_from(&self.time_zone, self.reprice_floor)
            .await?;
        Ok(reprice_floor(self.reprice_floor, open_from))
    }

    /// The metered payer's month over every account and its allocation, when
    /// `month` bills from its credits. Tracked usage is on an old basis only
    /// from the reprice floor in force on, as reconciliation reads it.
    async fn metered_month(
        &self,
        month: AiBillingMonth,
    ) -> Result<Option<MeteredMonth>, AiBillingError> {
        if !self
            .cutover
            .is_some_and(|cutover| cutover.applies_to(month))
        {
            return Ok(None);
        }
        let data = self.metered.load(month, None).await?;
        let allocation = allocate_metered_credits(&data.input)?;
        Ok(Some(MeteredMonth {
            input: data.input,
            allocation,
            running: data.status.running,
        }))
    }

    /// `month`'s own bill, of everyone or of `user_id`: what bills from the
    /// local-day sums, plus the metered payer's lines. Not converted, without
    /// adjustments.
    async fn own_bill(
        &self,
        month: AiBillingMonth,
        user_id: Option<UserId>,
    ) -> Result<OwnBill, AiBillingError> {
        let cutover = self.cutover.as_ref();
        let usage = self
            .repository
            .daily_usage(
                month.dates(),
                &self.time_zone,
                self.mapping_company.as_ref(),
                user_id.as_ref(),
                cutover,
            )
            .await?;
        // Read right after what bills, before the rates are fetched: a mark
        // made between the two reads puts its usage on both sides or neither,
        // until the next read.
        let personal = self
            .repository
            .personal_usage(
                month.dates(),
                &self.time_zone,
                self.mapping_company.as_ref(),
                user_id.as_ref(),
                cutover,
            )
            .await?;
        let subscriptions = self
            .subscriptions(match user_id {
                Some(user_id) => AiSubscriptionScope::User(user_id),
                None => AiSubscriptionScope::AllUsers,
            })
            .await?;
        let mut bill = bill_month(month, &usage, &subscriptions)?;
        // Every account is allocated, as in the overview, and a developer's
        // lines taken from it: both agree to the cent.
        let metered = self.metered_month(month).await?;
        if let Some(MeteredMonth { allocation, .. }) = &metered {
            let allocation = match user_id {
                Some(user_id) => for_user(allocation.clone(), user_id),
                None => allocation.clone(),
            };
            bill = add_metered_lines(bill, allocation)?;
        }
        Ok(OwnBill {
            bill,
            personal,
            subscriptions,
            metered,
        })
    }

    /// The adjustment lines that post on open month `target`, and the closed
    /// months they correct, oldest first: per closed month `K` posting on
    /// it, `K` recomputed now less its snapshot less the adjustments to it
    /// frozen in active closes (`adjustment_lines`).
    async fn adjustments_on(
        &self,
        target: AiBillingMonth,
        active: &[AiMonthClose],
    ) -> Result<(Vec<AiAdjustmentLine>, Vec<AiAdjustedMonth>), AiBillingError> {
        let adjusted = posting_on(target, active);
        let span = tracing::info_span!(
            "ai_billing.adjustments",
            month = %target,
            adjusted = adjusted.len()
        );
        async {
            let mut lines = Vec::new();
            for month in &adjusted {
                let own = self.own_bill(month.month, None).await?;
                let converted = self.convert(&own.bill).await?;
                let recompute = bill_entries(&own.bill, &converted)?;
                let snapshot = self.closes.own_entries(month.close_id).await?;
                let frozen = self.closes.frozen_adjustments(month.month).await?;
                let plans: HashMap<AiSubscriptionId, String> = own
                    .subscriptions
                    .iter()
                    .map(|subscription| {
                        (
                            subscription.id,
                            subscription.terms.plan.as_str().to_string(),
                        )
                    })
                    .collect();
                lines.extend(adjustment_lines(
                    *month,
                    &recompute,
                    &snapshot,
                    &frozen,
                    &converted.rates,
                    &plans,
                )?);
            }
            Ok((lines, adjusted.clone()))
        }
        .instrument(span)
        .await
    }

    /// Open `month` as the overview shows it: its own bill, the adjustment
    /// lines posting on it, converted.
    async fn open_month(
        &self,
        month: AiBillingMonth,
        active: &[AiMonthClose],
    ) -> Result<OpenMonth, AiBillingError> {
        let own = self.own_bill(month, None).await?;
        let (adjustments, adjusted) = self.adjustments_on(month, active).await?;
        let mut bill = own.bill;
        bill.adjustments = adjustments;
        bill.totals()?;
        let converted = self.convert(&bill).await?;

        let billed: HashSet<UserId> = bill
            .lines
            .iter()
            .filter_map(|line| line.subject.user_id())
            .chain(
                bill.subscriptions
                    .iter()
                    .map(|month| month.subscription.user_id),
            )
            .chain(own.personal.iter().map(|personal| personal.user_id))
            .chain(
                bill.adjustments
                    .iter()
                    .filter_map(|line| line.subject.user_id()),
            )
            .collect();
        let developers = self
            .repository
            .users()
            .await?
            .into_iter()
            .filter(|user| billed.contains(&user.user_id))
            .collect();

        Ok(OpenMonth {
            overview: AiMonthOverview {
                bill,
                developers,
                converted,
                personal: own.personal,
                adjusted,
            },
            metered: own.metered,
        })
    }

    /// A closed month as its snapshot serves it.
    async fn closed_month(
        &self,
        close: &AiMonthClose,
        active: &[AiMonthClose],
    ) -> Result<AiClosedMonth, AiBillingError> {
        Ok(AiClosedMonth {
            close: close.clone(),
            adjustments_post_on: next_open(close.month, active),
            snapshot: self.closes.snapshot(close.id).await?,
            current_rates: self.stored_rates(close.month).await?,
        })
    }

    async fn clock(&self, month: AiBillingMonth) -> Result<CloseClock, AiBillingError> {
        let now = self.closes.now().await?;
        let today_local = self.repository.today(&self.time_zone).await?;
        let local_end = self
            .repository
            .local_days(month.dates(), &self.time_zone)
            .await?
            .last()
            .map(|day| day.end);
        let utc_end = month.dates().end().midnight().assume_utc();
        Ok(CloseClock {
            now,
            today_utc: utc_day(now),
            today_local,
            over_at: local_end.map_or(utc_end, |local_end| local_end.max(utc_end)),
        })
    }

    /// The seven readiness checks of an open month that is over.
    async fn readiness(
        &self,
        open: &OpenMonth,
        now: OffsetDateTime,
    ) -> Result<AiCloseReadiness, AiBillingError> {
        let overview = &open.overview;
        let month = overview.bill.month;
        let metered = open.metered.as_ref();
        let [import, dispositions, unknown_account, repositories] = match metered {
            Some(metered) => metered_checks(
                &metered.input,
                &metered.allocation,
                metered.running.as_ref(),
            ),
            None => metered_not_applicable(),
        };
        // From the floor a reprice can still change on: a bucket before it
        // stays as it is, so asking for its reprice would block for nothing.
        let old_basis = self
            .repository
            .old_basis_usage(
                month.dates(),
                &self.time_zone,
                self.mapping_company.as_ref(),
                self.cutover.as_ref(),
                self.reprice_floor_now().await?,
            )
            .await?;
        let pricing = pricing_check(metered.map(|metered| &metered.allocation), &old_basis);
        let exchange_rates = exchange_rate_check(&overview.bill, &overview.converted);
        let completeness = self.completeness(month).await?;
        let days = self
            .repository
            .local_days(month.dates(), &self.time_zone)
            .await?;
        let spans = if metered.is_some() {
            utc_spans_outside(month, &days)
        } else {
            Vec::new()
        };
        let intervals = match (
            spans.iter().map(|(from, _)| *from).min(),
            spans.iter().map(|(_, until)| *until).max(),
        ) {
            (Some(from), Some(until)) => self.repository.coverage_intervals(from, until).await?,
            _ => Vec::new(),
        };
        let uploads = uploads_check(&completeness, metered.is_some(), &spans, &intervals);

        Ok(AiCloseReadiness {
            month,
            evaluated_at: now,
            checks: vec![
                import,
                dispositions,
                unknown_account,
                repositories,
                pricing,
                exchange_rates,
                uploads,
            ],
        })
    }

    /// Closes `month` while the close lock is held.
    async fn close_locked(
        &self,
        month: AiBillingMonth,
        by: &UserId,
        waivers: Vec<AiCloseWaiver>,
        projector: &dyn AiBillProjector,
    ) -> Result<AiMonthClose, AiMonthCloseError> {
        let active = self.closes.active().await?;
        let clock = self.clock(month).await?;
        match close_blocker(
            month,
            &active,
            clock.today_utc,
            clock.today_local,
            clock.over_at,
        ) {
            Some(AiCloseBlocker::MonthInProgress { over_at }) => {
                return Err(AiMonthCloseError::MonthInProgress { month, over_at })
            }
            Some(AiCloseBlocker::AlreadyClosed) => {
                return Err(AiMonthCloseError::AlreadyClosed { month })
            }
            None => {}
        }
        let open = self.open_month(month, &active).await?;
        let readiness = self.readiness(&open, clock.now).await?.waive(waivers);
        if !readiness.ready() {
            return Err(AiMonthCloseError::NotReady(Box::new(readiness)));
        }

        let id = self.closes.reserve_id().await?;
        let overview = &open.overview;
        let bill = &overview.bill;
        let parts: Vec<AiBillingLinePart<'_>> = bill
            .lines
            .iter()
            .zip(&overview.converted.lines)
            .map(|(line, converted)| line.parts(converted))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        // A part for every developer whose page shows more than an empty
        // bill: lines, subscriptions or adjustments, metered usage on no
        // line, or a metered part kept provisional by a linked account. The
        // rest are served an empty part, as the live page shows it.
        let users: BTreeMap<i32, UserId> = bill
            .lines
            .iter()
            .filter_map(|line| line.subject.user_id())
            .chain(
                bill.subscriptions
                    .iter()
                    .map(|month| month.subscription.user_id),
            )
            .chain(
                bill.adjustments
                    .iter()
                    .filter_map(|line| line.subject.user_id()),
            )
            .chain(
                open.metered
                    .iter()
                    .flat_map(|metered| metered.allocation.user_ids())
                    .map(UserId::new),
            )
            .map(|user_id| (user_id.as_i32(), user_id))
            .collect();
        let developer_bills = users
            .into_values()
            .map(|user_id| {
                let metered = open
                    .metered
                    .as_ref()
                    .map(|metered| for_user(metered.allocation.clone(), user_id).info());
                let (part, converted) = overview.converted.user_part(bill, user_id, metered)?;
                Ok((user_id, part, converted))
            })
            .collect::<Result<Vec<_>, AiBillingError>>()?;
        let projected = projector.project(&AiProjectionInput {
            overview,
            parts: &parts,
            close_id: id,
            developer_bills: &developer_bills,
        })?;

        let rows = parts.len() + bill.adjustments.len();
        let mut csv_positions = vec![None; rows];
        for (position, index) in projected.csv_order.iter().enumerate() {
            if let Some(slot) = csv_positions.get_mut(*index) {
                *slot = u32::try_from(position).ok();
            }
        }
        if projected.lines.len() != parts.len()
            || projected.adjustment_lines.len() != bill.adjustments.len()
            || projected.csv_rows.len() != rows
            || projected.csv_order.len() != rows
            || csv_positions.iter().any(Option::is_none)
        {
            return Err(AiBillingError::Invalid(
                "the bill's projection does not match its lines".to_string(),
            )
            .into());
        }
        let position = |index: usize| {
            u32::try_from(index).map_err(|_| AiMonthCloseError::from(AiBillingError::NumericRange))
        };
        let providers: HashMap<AiSubscriptionId, _> = bill
            .subscriptions
            .iter()
            .map(|month| (month.subscription.id, month.subscription.terms.provider))
            .collect();
        let mut lines = Vec::with_capacity(rows);
        for (index, part) in parts.iter().enumerate() {
            lines.push(AiNewSnapshotLine {
                position: position(index)?,
                csv_position: csv_positions[index].unwrap_or_default(),
                adjusts: None,
                entry: AiBilledEntry::of_part(part, |id| providers.get(&id).copied())?,
                line_json: projected.lines[index].clone(),
                csv_row: projected.csv_rows[index].clone(),
            });
        }
        for (offset, line) in bill.adjustments.iter().enumerate() {
            let index = parts.len() + offset;
            lines.push(AiNewSnapshotLine {
                position: position(index)?,
                csv_position: csv_positions[index].unwrap_or_default(),
                adjusts: Some(line.adjusts),
                entry: adjustment_entry(line),
                line_json: projected.adjustment_lines[offset].clone(),
                csv_row: projected.csv_rows[index].clone(),
            });
        }
        let close = self
            .closes
            .store(&AiNewClose {
                id,
                month,
                by: *by,
                readiness_json: projector.project_readiness(&readiness),
                bill_json: projected.bill_json,
                developer_json: projected.developer_json,
                csv_header: projected.csv_header,
                projection_version: projected.version,
                lines,
            })
            .await?;
        let waived: Vec<&str> = readiness
            .waived()
            .into_iter()
            .map(|check| check.as_str())
            .collect();
        tracing::info!(
            month = %month,
            close_id = close.id.as_i64(),
            lines = parts.len(),
            adjustment_lines = bill.adjustments.len(),
            waived = ?waived,
            "Closed the AI bill"
        );
        Ok(close)
    }
}

#[async_trait]
impl<R, S, X, M, A, C> AiBillingService for AiBillingServiceImpl<R, S, X, M, A, C>
where
    R: AiBillingRepository,
    S: AiSubscriptionRepository,
    X: AiExchangeRateRepository,
    M: AiMeteredUsageRepository,
    A: AiProviderAccountRepository,
    C: AiMonthCloseRepository,
{
    fn time_zone(&self) -> &AiUsageTimeZone {
        &self.time_zone
    }

    fn metered_cutover(&self) -> Option<AiMeteredCutover> {
        self.cutover
    }

    async fn month_overview(&self, month: AiBillingMonth) -> Result<AiMonthView, AiBillingError> {
        let active = self.closes.active().await?;
        if let Some(close) = active.iter().find(|close| close.month == month) {
            return Ok(AiMonthView::Closed(Box::new(
                self.closed_month(close, &active).await?,
            )));
        }
        Ok(AiMonthView::Open(Box::new(
            self.open_month(month, &active).await?.overview,
        )))
    }

    async fn developer_month(
        &self,
        month: AiBillingMonth,
        user_id: UserId,
    ) -> Result<AiDeveloperMonth, AiBillingError> {
        let developer = self
            .repository
            .users()
            .await?
            .into_iter()
            .find(|user| user.user_id == user_id)
            .ok_or(AiBillingError::UserNotFound)?;
        let cutover = self.cutover.as_ref();
        let active = self.closes.active().await?;
        let closed = active.iter().find(|close| close.month == month);
        // Bills from the same per-day sums as the overview, so both agree to
        // the hundredth; the finer rows are for display only.
        let (bill, subscriptions) = match closed {
            Some(close) => {
                // The bill and the personal usage are the overview's, as it
                // closed (`bill_json`): a key marked personal since changes
                // neither, only the live usage by day.
                let closed = self.closed_month(close, &active).await?;
                let subscriptions = self
                    .subscriptions(AiSubscriptionScope::User(user_id))
                    .await?;
                let mine = AiKeySubject::User(user_id);
                let lines_of = |adjustments: bool| {
                    closed
                        .snapshot
                        .lines
                        .iter()
                        .filter(|line| {
                            line.adjusts.is_some() == adjustments && line.subject == mine
                        })
                        .map(|line| line.line_json.clone())
                        .collect()
                };
                let bill = AiDeveloperBill::Closed(Box::new(AiClosedDeveloperBill {
                    close: closed.close.clone(),
                    adjustments_post_on: closed.adjustments_post_on,
                    current_rates: closed.current_rates.clone(),
                    developer: closed
                        .snapshot
                        .developer_json
                        .get(user_id.as_i32().to_string())
                        .cloned(),
                    lines: lines_of(false),
                    adjustment_lines: lines_of(true),
                    projection_version: closed.snapshot.projection_version,
                    bill_json: closed.snapshot.bill_json,
                }));
                (bill, subscriptions)
            }
            None => {
                let own = self.own_bill(month, Some(user_id)).await?;
                let (adjustments, _) = self.adjustments_on(month, &active).await?;
                let mut bill = own.bill;
                bill.adjustments = adjustments
                    .into_iter()
                    .filter(|line| line.subject.user_id() == Some(user_id))
                    .collect();
                bill.totals()?;
                let converted = self.convert(&bill).await?;
                (
                    AiDeveloperBill::Live {
                        bill: Box::new(bill),
                        converted: Box::new(converted),
                        personal: own
                            .personal
                            .into_iter()
                            .next()
                            .map(|personal| personal.usage),
                    },
                    own.subscriptions,
                )
            }
        };
        let usage = self
            .repository
            .developer_usage(
                &user_id,
                month.dates(),
                &self.time_zone,
                self.mapping_company.as_ref(),
                cutover,
            )
            .await?;
        let machines = self
            .repository
            .machines(Some(&user_id), month.dates(), &self.time_zone)
            .await?
            .into_iter()
            .map(|machine| AiMachineLabel {
                machine_id: machine.machine_id,
                label: machine.label,
            })
            .collect();

        Ok(AiDeveloperMonth {
            developer,
            bill,
            usage: billed_day_usage(user_id, usage, &subscriptions),
            machines,
        })
    }

    async fn completeness(
        &self,
        month: AiBillingMonth,
    ) -> Result<AiMonthCompleteness, AiBillingError> {
        let machines = self
            .repository
            .machines(None, month.dates(), &self.time_zone)
            .await?;
        let days = self
            .repository
            .local_days(month.dates(), &self.time_zone)
            .await?;
        let subscriptions = self.subscriptions(AiSubscriptionScope::AllUsers).await?;
        let today = self.repository.today(&self.time_zone).await?;
        let now = OffsetDateTime::now_utc();

        // Developers are those with a machine, or a subscription in the month
        // that billing will charge for even without any upload.
        let concerned: HashSet<UserId> = machines
            .iter()
            .map(|machine| machine.user_id)
            .chain(
                subscriptions
                    .iter()
                    .filter(|subscription| {
                        subscription
                            .terms
                            .period
                            .to_date_range(month.dates())
                            .is_some()
                    })
                    .map(|subscription| subscription.user_id),
            )
            .collect();
        let developers: Vec<AiBillingDeveloper> = self
            .repository
            .users()
            .await?
            .into_iter()
            .filter(|user| concerned.contains(&user.user_id))
            .collect();

        Ok(assess_completeness(
            month,
            &days,
            today,
            now,
            developers,
            machines,
            &subscriptions,
        ))
    }

    async fn users(&self) -> Result<Vec<AiBillingDeveloper>, AiBillingError> {
        self.repository.users().await
    }

    async fn override_exchange_rate(
        &self,
        month: AiBillingMonth,
        currency: &AiCurrency,
        rate: AiExchangeRateValue,
        by: &UserId,
    ) -> Result<AiExchangeRate, AiBillingError> {
        self.rates.set_override(month, currency, rate, by).await
    }

    async fn reset_exchange_rate(
        &self,
        month: AiBillingMonth,
        currency: &AiCurrency,
    ) -> Result<Option<AiExchangeRate>, AiBillingError> {
        self.rates.reset(month, currency).await
    }

    async fn close_status(
        &self,
        month: AiBillingMonth,
    ) -> Result<AiMonthCloseStatus, AiMonthCloseError> {
        let active = self.closes.active().await?;
        let history = self.closes.history(month).await?;
        let state = match active.iter().find(|close| close.month == month) {
            Some(close) => AiMonthCloseState::Closed {
                close: close.clone(),
                readiness_json: self.closes.snapshot(close.id).await?.readiness_json,
                adjustments_post_on: next_open(month, &active),
                held_by: self.closes.months_adjusting(month).await?,
            },
            None => {
                let clock = self.clock(month).await?;
                let adjusts = posting_on(month, &active);
                match close_blocker(
                    month,
                    &active,
                    clock.today_utc,
                    clock.today_local,
                    clock.over_at,
                ) {
                    Some(_) => AiMonthCloseState::Open {
                        over_at: Some(clock.over_at),
                        readiness: None,
                        adjusts,
                    },
                    None => {
                        let open = self.open_month(month, &active).await?;
                        AiMonthCloseState::Open {
                            over_at: None,
                            readiness: Some(self.readiness(&open, clock.now).await?),
                            adjusts,
                        }
                    }
                }
            }
        };
        Ok(AiMonthCloseStatus {
            month,
            state,
            history,
        })
    }

    async fn close_month(
        &self,
        month: AiBillingMonth,
        by: &UserId,
        waivers: Vec<AiCloseWaiver>,
        projector: &dyn AiBillProjector,
    ) -> Result<AiMonthCloseStatus, AiMonthCloseError> {
        let lease = self
            .closes
            .try_lock()
            .await?
            .ok_or(AiMonthCloseError::Busy)?;
        let closed = self.close_locked(month, by, waivers, projector).await;
        // Released on errors too; a failed release only logs, since the lease
        // frees the lock when dropped.
        if let Err(error) = lease.release().await {
            tracing::warn!(%error, "Releasing the month close lock failed");
        }
        closed?;
        self.close_status(month).await
    }

    async fn reopen_month(
        &self,
        month: AiBillingMonth,
        by: &UserId,
    ) -> Result<AiMonthCloseStatus, AiMonthCloseError> {
        let lease = self
            .closes
            .try_lock()
            .await?
            .ok_or(AiMonthCloseError::Busy)?;
        let reopened = async {
            let active = self.closes.active().await?;
            let Some(close) = active.iter().find(|close| close.month == month) else {
                return Err(AiMonthCloseError::NotClosed { month });
            };
            let held_by = self.closes.months_adjusting(month).await?;
            if !held_by.is_empty() {
                return Err(AiMonthCloseError::HeldByAdjustments { month, held_by });
            }
            self.closes.reopen(month, by).await?;
            tracing::info!(
                month = %month,
                close_id = close.id.as_i64(),
                "Re-opened the AI bill"
            );
            Ok(())
        }
        .await;
        if let Err(error) = lease.release().await {
            tracing::warn!(%error, "Releasing the month close lock failed");
        }
        reopened?;
        self.close_status(month).await
    }
}
