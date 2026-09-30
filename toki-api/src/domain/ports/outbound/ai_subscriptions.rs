use async_trait::async_trait;
use time::Date;

use crate::domain::{
    models::{
        AiPlanMismatchDay, AiSubscription, AiSubscriptionId, AiSubscriptionScope,
        AiSubscriptionTerms, AiUsageDateRange, AiUsageTimeZone, UserId,
    },
    AiSubscriptionError,
};

#[async_trait]
pub trait AiSubscriptionRepository: Send + Sync + 'static {
    /// Subscriptions in `scope`, ordered by user, provider and latest start first.
    async fn list(
        &self,
        scope: AiSubscriptionScope,
    ) -> Result<Vec<AiSubscription>, AiSubscriptionError>;

    /// Stores a new subscription for `user_id`. The store rejects a period that
    /// shares a day with another of the user's subscriptions to the same
    /// provider (`Overlap`), and an unknown user (`UserNotFound`).
    async fn insert(
        &self,
        user_id: &UserId,
        terms: &AiSubscriptionTerms,
    ) -> Result<AiSubscription, AiSubscriptionError>;

    /// Replaces the terms of the subscription `id` if it lies in `scope`, else
    /// fails with `NotFound`. Overlaps are rejected as for `insert`.
    async fn update(
        &self,
        scope: AiSubscriptionScope,
        id: AiSubscriptionId,
        terms: &AiSubscriptionTerms,
    ) -> Result<AiSubscription, AiSubscriptionError>;

    /// Deletes the subscription `id` if it lies in `scope`, else fails with
    /// `NotFound`.
    async fn delete(
        &self,
        scope: AiSubscriptionScope,
        id: AiSubscriptionId,
    ) -> Result<(), AiSubscriptionError>;

    /// Today's date in `time_zone`, by the store's clock.
    async fn today(&self, time_zone: &AiUsageTimeZone) -> Result<Date, AiSubscriptionError>;

    /// The mismatch days of users in `scope` within `dates`, local days in
    /// `time_zone`: days on which a provider reported a plan other than those in
    /// `UNPAID_PLAN_HINTS`, but none of the user's subscriptions to that provider
    /// covers the day. An hourly hint belongs to the local date its hour starts
    /// on. Ordered by user, provider and day.
    async fn plan_mismatch_days(
        &self,
        scope: AiSubscriptionScope,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
    ) -> Result<Vec<AiPlanMismatchDay>, AiSubscriptionError>;
}
