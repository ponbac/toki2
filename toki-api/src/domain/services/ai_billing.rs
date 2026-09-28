use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use time::OffsetDateTime;

use crate::domain::{
    models::{
        assess_completeness, bill_month, billed_day_usage, AiBillingDeveloper, AiBillingMonth,
        AiDeveloperMonth, AiMachineLabel, AiMonthCompleteness, AiMonthOverview, AiSubscription,
        AiSubscriptionScope, AiUsageTimeZone, TimeTrackingCompany, UserId,
    },
    ports::{
        inbound::AiBillingService,
        outbound::{AiBillingRepository, AiSubscriptionRepository},
    },
    AiBillingError, AiSubscriptionError,
};

pub struct AiBillingServiceImpl<R, S> {
    repository: Arc<R>,
    subscriptions: Arc<S>,
    time_zone: AiUsageTimeZone,
    mapping_company: Option<TimeTrackingCompany>,
}

impl<R, S> AiBillingServiceImpl<R, S> {
    /// `time_zone` defines usage days and billing months. Only project mappings
    /// to `mapping_company`, the configured time-tracking company, resolve; with
    /// `None`, all usage is Unassigned.
    pub fn new(
        repository: Arc<R>,
        subscriptions: Arc<S>,
        time_zone: AiUsageTimeZone,
        mapping_company: Option<TimeTrackingCompany>,
    ) -> Self {
        Self {
            repository,
            subscriptions,
            time_zone,
            mapping_company,
        }
    }
}

impl<R, S: AiSubscriptionRepository> AiBillingServiceImpl<R, S> {
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
impl<R, S> AiBillingService for AiBillingServiceImpl<R, S>
where
    R: AiBillingRepository,
    S: AiSubscriptionRepository,
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

        Ok(AiMonthOverview { bill, developers })
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
}
