use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use time::OffsetDateTime;

use super::{AiExchangeRateSettings, AiExchangeRates};
use crate::domain::{
    models::{
        assess_completeness, bill_month, billed_day_usage, convert_bill, currencies_to_convert,
        AiBillingDeveloper, AiBillingMonth, AiConvertedBill, AiCurrency, AiDeveloperMonth,
        AiExchangeRate, AiExchangeRateValue, AiMachineLabel, AiMonthBill, AiMonthCompleteness,
        AiMonthOverview, AiSubscription, AiSubscriptionScope, AiUsageTimeZone, TimeTrackingCompany,
        UserId,
    },
    ports::{
        inbound::AiBillingService,
        outbound::{
            AiBillingRepository, AiExchangeRateRepository, AiSubscriptionRepository,
            ExchangeRateProvider,
        },
    },
    AiBillingError, AiSubscriptionError,
};

pub struct AiBillingServiceImpl<R, S, X> {
    repository: Arc<R>,
    subscriptions: Arc<S>,
    rates: AiExchangeRates<X>,
    time_zone: AiUsageTimeZone,
    mapping_company: Option<TimeTrackingCompany>,
}

impl<R, S, X: AiExchangeRateRepository> AiBillingServiceImpl<R, S, X> {
    /// `time_zone` defines usage days and billing months. Only project mappings
    /// to `mapping_company`, the configured time-tracking company, resolve; with
    /// `None`, all usage is Unassigned. Exchange rates are stored in `rates`
    /// and fetched from `rate_provider` when missing or due, as
    /// `AiExchangeRates` bounds it.
    pub fn new(
        repository: Arc<R>,
        subscriptions: Arc<S>,
        rates: Arc<X>,
        rate_provider: Arc<dyn ExchangeRateProvider>,
        time_zone: AiUsageTimeZone,
        mapping_company: Option<TimeTrackingCompany>,
    ) -> Self {
        Self::with_rate_settings(
            repository,
            subscriptions,
            rates,
            rate_provider,
            time_zone,
            mapping_company,
            AiExchangeRateSettings::default(),
        )
    }

    pub fn with_rate_settings(
        repository: Arc<R>,
        subscriptions: Arc<S>,
        rates: Arc<X>,
        rate_provider: Arc<dyn ExchangeRateProvider>,
        time_zone: AiUsageTimeZone,
        mapping_company: Option<TimeTrackingCompany>,
        settings: AiExchangeRateSettings,
    ) -> Self {
        Self {
            repository,
            subscriptions,
            rates: AiExchangeRates::new(rates, rate_provider, time_zone.clone(), settings),
            time_zone,
            mapping_company,
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

impl<R, S: AiSubscriptionRepository, X> AiBillingServiceImpl<R, S, X> {
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

#[async_trait]
impl<R, S, X> AiBillingService for AiBillingServiceImpl<R, S, X>
where
    R: AiBillingRepository,
    S: AiSubscriptionRepository,
    X: AiExchangeRateRepository,
{
    fn time_zone(&self) -> &AiUsageTimeZone {
        &self.time_zone
    }

    async fn month_overview(
        &self,
        month: AiBillingMonth,
    ) -> Result<AiMonthOverview, AiBillingError> {
        let usage = self
            .repository
            .daily_usage(
                month.dates(),
                &self.time_zone,
                self.mapping_company.as_ref(),
                None,
            )
            .await?;
        let subscriptions = self.subscriptions(AiSubscriptionScope::AllUsers).await?;
        let bill = bill_month(month, &usage, &subscriptions)?;
        let converted = self.convert(&bill).await?;

        let billed: HashSet<UserId> = bill
            .lines
            .iter()
            .map(|line| line.user_id)
            .chain(
                bill.subscriptions
                    .iter()
                    .map(|month| month.subscription.user_id),
            )
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
        // Bills from the same per-day sums as the overview, so both agree to
        // the hundredth; the finer rows are for display only.
        let daily = self
            .repository
            .daily_usage(
                month.dates(),
                &self.time_zone,
                self.mapping_company.as_ref(),
                Some(&user_id),
            )
            .await?;
        let usage = self
            .repository
            .developer_usage(
                &user_id,
                month.dates(),
                &self.time_zone,
                self.mapping_company.as_ref(),
            )
            .await?;
        let subscriptions = self
            .subscriptions(AiSubscriptionScope::User(user_id))
            .await?;
        let bill = bill_month(month, &daily, &subscriptions)?;
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
