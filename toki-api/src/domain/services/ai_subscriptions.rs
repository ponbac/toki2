use std::sync::Arc;

use async_trait::async_trait;
use time::Date;

use crate::domain::{
    models::{
        default_mismatch_dates, group_mismatch_runs, AiLocalCalendar, AiPlanMismatches,
        AiSubscription, AiSubscriptionId, AiSubscriptionScope, AiSubscriptionTerms,
        AiUsageDateRange, AiUsageTimeZone, UserId,
    },
    ports::{inbound::AiSubscriptionService, outbound::AiSubscriptionRepository},
    AiSubscriptionError,
};

pub struct AiSubscriptionServiceImpl<R> {
    repository: Arc<R>,
    time_zone: AiUsageTimeZone,
}

impl<R> AiSubscriptionServiceImpl<R> {
    /// `time_zone` defines the local days that subscriptions cover.
    pub fn new(repository: Arc<R>, time_zone: AiUsageTimeZone) -> Self {
        Self {
            repository,
            time_zone,
        }
    }
}

#[async_trait]
impl<R: AiSubscriptionRepository> AiSubscriptionService for AiSubscriptionServiceImpl<R> {
    async fn list(
        &self,
        scope: AiSubscriptionScope,
    ) -> Result<Vec<AiSubscription>, AiSubscriptionError> {
        self.repository.list(scope).await
    }

    async fn create(
        &self,
        user_id: &UserId,
        terms: &AiSubscriptionTerms,
    ) -> Result<AiSubscription, AiSubscriptionError> {
        self.repository.insert(user_id, terms).await
    }

    async fn update(
        &self,
        scope: AiSubscriptionScope,
        id: AiSubscriptionId,
        terms: &AiSubscriptionTerms,
    ) -> Result<AiSubscription, AiSubscriptionError> {
        self.repository.update(scope, id, terms).await
    }

    async fn delete(
        &self,
        scope: AiSubscriptionScope,
        id: AiSubscriptionId,
    ) -> Result<(), AiSubscriptionError> {
        self.repository.delete(scope, id).await
    }

    async fn calendar(&self) -> Result<AiLocalCalendar, AiSubscriptionError> {
        Ok(AiLocalCalendar {
            time_zone: self.time_zone.clone(),
            today: self.repository.today(&self.time_zone).await?,
        })
    }

    async fn plan_mismatches(
        &self,
        scope: AiSubscriptionScope,
        from: Option<Date>,
        to: Option<Date>,
    ) -> Result<AiPlanMismatches, AiSubscriptionError> {
        let to = match to {
            Some(to) => to,
            None => self.repository.today(&self.time_zone).await?,
        };
        let out_of_range =
            || AiSubscriptionError::Invalid("from or to is out of range".to_string());
        let dates = match from {
            Some(from) => AiUsageDateRange::from_inclusive(from, to).ok_or_else(|| {
                if from > to {
                    AiSubscriptionError::Invalid("from must not be after to".to_string())
                } else {
                    out_of_range()
                }
            })?,
            None => default_mismatch_dates(to).ok_or_else(out_of_range)?,
        };
        let days = self
            .repository
            .plan_mismatch_days(scope, dates, &self.time_zone)
            .await?;

        Ok(AiPlanMismatches {
            dates,
            runs: group_mismatch_runs(days),
        })
    }
}
