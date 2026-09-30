use async_trait::async_trait;
use time::Date;

use crate::domain::{
    models::{
        AiLocalCalendar, AiPlanMismatches, AiSubscription, AiSubscriptionId, AiSubscriptionScope,
        AiSubscriptionTerms, UserId,
    },
    AiSubscriptionError,
};

/// Declared AI subscriptions. Callers choose the scope: a developer acts in
/// `AiSubscriptionScope::User` with their own id, and only admins act for other
/// users or in `AiSubscriptionScope::AllUsers`.
#[async_trait]
pub trait AiSubscriptionService: Send + Sync + 'static {
    /// Subscriptions in `scope`, ordered by user, provider and latest start first.
    async fn list(
        &self,
        scope: AiSubscriptionScope,
    ) -> Result<Vec<AiSubscription>, AiSubscriptionError>;

    /// Declares a subscription for `user_id`. Fails with `Overlap` when the
    /// period shares a day with another of the user's subscriptions to the same
    /// provider, and with `UserNotFound` for an unknown user.
    async fn create(
        &self,
        user_id: &UserId,
        terms: &AiSubscriptionTerms,
    ) -> Result<AiSubscription, AiSubscriptionError>;

    /// Replaces the terms of a subscription in `scope`; `NotFound` when there is
    /// none, including when it belongs to a user outside the scope.
    async fn update(
        &self,
        scope: AiSubscriptionScope,
        id: AiSubscriptionId,
        terms: &AiSubscriptionTerms,
    ) -> Result<AiSubscription, AiSubscriptionError>;

    /// Deletes a subscription in `scope`; `NotFound` as for `update`.
    async fn delete(
        &self,
        scope: AiSubscriptionScope,
        id: AiSubscriptionId,
    ) -> Result<(), AiSubscriptionError>;

    /// The time zone whose calendar days subscription dates are, and today's
    /// date there.
    async fn calendar(&self) -> Result<AiLocalCalendar, AiSubscriptionError>;

    /// Mismatch runs in `scope` between the local days `from` and `to`, both
    /// inclusive: days on which a provider reported a paid plan, but no declared
    /// subscription to that provider covers the day, so the day bills as API
    /// usage. `to` defaults to today and `from` to `DEFAULT_MISMATCH_DAYS` days
    /// ending with `to`. Fails with `Invalid` when `from` is after `to`.
    async fn plan_mismatches(
        &self,
        scope: AiSubscriptionScope,
        from: Option<Date>,
        to: Option<Date>,
    ) -> Result<AiPlanMismatches, AiSubscriptionError>;
}
