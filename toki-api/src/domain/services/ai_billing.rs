use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use time::OffsetDateTime;

use super::{AiExchangeRates, AiMeteredMonths};
use crate::domain::{
    models::{
        add_metered_lines, allocate_metered_credits, assess_completeness, bill_month,
        billed_day_usage, convert_bill, currencies_to_convert, for_user, AiBillingDeveloper,
        AiBillingMonth, AiConvertedBill, AiCurrency, AiDeveloperMonth, AiExchangeRate,
        AiExchangeRateValue, AiMachineLabel, AiMeteredAllocation, AiMeteredCutover, AiMonthBill,
        AiMonthCompleteness, AiMonthOverview, AiSubscription, AiSubscriptionScope, AiUsageTimeZone,
        TimeTrackingCompany, UserId,
    },
    ports::{
        inbound::AiBillingService,
        outbound::{
            AiBillingRepository, AiExchangeRateRepository, AiMeteredUsageRepository,
            AiProviderAccountRepository, AiSubscriptionRepository,
        },
    },
    AiBillingError, AiSubscriptionError,
};

pub struct AiBillingServiceImpl<R, S, X, M, A> {
    repository: Arc<R>,
    subscriptions: Arc<S>,
    rates: AiExchangeRates<X>,
    time_zone: AiUsageTimeZone,
    mapping_company: Option<TimeTrackingCompany>,
    /// The metered payer's months, read as reconciliation reads them.
    metered: AiMeteredMonths<M, A>,
    cutover: Option<AiMeteredCutover>,
}

impl<R, S, X: AiExchangeRateRepository, M, A> AiBillingServiceImpl<R, S, X, M, A> {
    /// `time_zone` defines usage days and billing months. Only project mappings
    /// to `mapping_company`, the configured time-tracking company, resolve; with
    /// `None`, all usage is Unassigned. Exchange rates are stored and fetched
    /// through `rates`, which bounds the fetches. From `cutover` on, the
    /// metered payer's usage bills from its credits, read through `metered`;
    /// `None` keeps every month on estimates.
    pub fn new(
        repository: Arc<R>,
        subscriptions: Arc<S>,
        rates: AiExchangeRates<X>,
        time_zone: AiUsageTimeZone,
        mapping_company: Option<TimeTrackingCompany>,
        metered: AiMeteredMonths<M, A>,
        cutover: Option<AiMeteredCutover>,
    ) -> Self {
        Self {
            repository,
            subscriptions,
            rates,
            time_zone,
            mapping_company,
            metered,
            cutover,
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
}

impl<R, S: AiSubscriptionRepository, X, M, A> AiBillingServiceImpl<R, S, X, M, A> {
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

impl<R, S, X, M, A> AiBillingServiceImpl<R, S, X, M, A>
where
    M: AiMeteredUsageRepository,
    A: AiProviderAccountRepository,
{
    /// The metered payer's lines of `month`'s UTC month, over every account,
    /// when the month bills them from credits.
    async fn metered_allocation(
        &self,
        month: AiBillingMonth,
    ) -> Result<Option<AiMeteredAllocation>, AiBillingError> {
        if !self
            .cutover
            .is_some_and(|cutover| cutover.applies_to(month))
        {
            return Ok(None);
        }
        let data = self.metered.load(month, None).await?;
        Ok(Some(allocate_metered_credits(&data.input)?))
    }
}

#[async_trait]
impl<R, S, X, M, A> AiBillingService for AiBillingServiceImpl<R, S, X, M, A>
where
    R: AiBillingRepository,
    S: AiSubscriptionRepository,
    X: AiExchangeRateRepository,
    M: AiMeteredUsageRepository,
    A: AiProviderAccountRepository,
{
    fn time_zone(&self) -> &AiUsageTimeZone {
        &self.time_zone
    }

    fn metered_cutover(&self) -> Option<AiMeteredCutover> {
        self.cutover
    }

    async fn month_overview(
        &self,
        month: AiBillingMonth,
    ) -> Result<AiMonthOverview, AiBillingError> {
        let cutover = self.cutover.as_ref();
        let usage = self
            .repository
            .daily_usage(
                month.dates(),
                &self.time_zone,
                self.mapping_company.as_ref(),
                None,
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
                None,
                cutover,
            )
            .await?;
        let subscriptions = self.subscriptions(AiSubscriptionScope::AllUsers).await?;
        let mut bill = bill_month(month, &usage, &subscriptions)?;
        if let Some(allocation) = self.metered_allocation(month).await? {
            bill = add_metered_lines(bill, allocation)?;
        }
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
            .chain(personal.iter().map(|personal| personal.user_id))
            .collect();
        let developers = self
            .repository
            .users()
            .await?
            .into_iter()
            .filter(|user| billed.contains(&user.user_id))
            .collect();

        Ok(AiMonthOverview {
            bill,
            developers,
            converted,
            personal,
        })
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
        // Bills from the same per-day sums as the overview, so both agree to
        // the hundredth; the finer rows are for display only.
        let daily = self
            .repository
            .daily_usage(
                month.dates(),
                &self.time_zone,
                self.mapping_company.as_ref(),
                Some(&user_id),
                cutover,
            )
            .await?;
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
        // Read right after the usage it complements, as in the overview.
        let personal = self
            .repository
            .personal_usage(
                month.dates(),
                &self.time_zone,
                self.mapping_company.as_ref(),
                Some(&user_id),
                cutover,
            )
            .await?
            .into_iter()
            .next()
            .map(|personal| personal.usage);
        let subscriptions = self
            .subscriptions(AiSubscriptionScope::User(user_id))
            .await?;
        let mut bill = bill_month(month, &daily, &subscriptions)?;
        // Every account is allocated, as in the overview, and the developer's
        // lines taken from it: both agree to the cent.
        if let Some(allocation) = self.metered_allocation(month).await? {
            bill = add_metered_lines(bill, for_user(allocation, user_id))?;
        }
        let converted = self.convert(&bill).await?;
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
            converted,
            usage: billed_day_usage(user_id, usage, &subscriptions),
            personal,
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
}
