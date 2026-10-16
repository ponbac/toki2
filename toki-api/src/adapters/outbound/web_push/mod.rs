//! Web push notifications for timer buddy battles: when a colleague battles a user's
//! buddies, every device the user subscribed for push hears how it went.
//!
//! Subscriptions that the push service reports as gone are deleted, since they never
//! work again, unless the device subscribed again in the meantime. Every other
//! failure is logged and dropped: a notice is best-effort.

use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;
use url::Url;
use web_push::{IsahcWebPushClient, WebPushClient, WebPushError};

use crate::{
    domain::{
        models::{BattleId, BattleNotice, BattleResult},
        ports::outbound::BuddyNotifier,
        PushNotification, PushSubscription,
    },
    repositories::{PushSubscriptionRepository, PushSubscriptionRepositoryImpl},
};

/// How long one push service may take to accept a notification.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// Tells defenders about battles through the push subscriptions of their devices.
pub struct WebPushBuddyNotifier<C = IsahcWebPushClient> {
    subscriptions: PushSubscriptionRepositoryImpl,
    client: C,
    app_url: Url,
    timeout: Duration,
}

impl<C> WebPushBuddyNotifier<C> {
    /// `app_url` is the app's address, which notifications link into.
    pub fn new(subscriptions: PushSubscriptionRepositoryImpl, client: C, app_url: Url) -> Self {
        Self {
            subscriptions,
            client,
            app_url,
            timeout: SEND_TIMEOUT,
        }
    }
}

#[async_trait]
impl<C> BuddyNotifier for WebPushBuddyNotifier<C>
where
    C: WebPushClient + Send + Sync + 'static,
{
    async fn battle_fought(&self, notice: &BattleNotice) {
        let subscriptions = match self
            .subscriptions
            .get_user_push_subscriptions(&notice.defender_id.as_i32())
            .await
        {
            Ok(subscriptions) => subscriptions,
            Err(error) => {
                tracing::warn!(
                    battle_id = %notice.battle_id,
                    defender_id = %notice.defender_id,
                    "Battle notice not sent, push subscriptions are unavailable: {error}"
                );
                return;
            }
        };
        if subscriptions.is_empty() {
            return;
        }

        let url = battle_url(&self.app_url, notice.battle_id);
        let notification = PushNotification::new(
            &battle_title(notice),
            &battle_body(notice),
            Some(&url),
            None,
        );
        join_all(
            subscriptions
                .iter()
                .map(|subscription| self.push(notice, subscription, &notification)),
        )
        .await;
    }
}

impl<C> WebPushBuddyNotifier<C>
where
    C: WebPushClient + Send + Sync,
{
    /// Pushes `notification` to one device.
    async fn push(
        &self,
        notice: &BattleNotice,
        subscription: &PushSubscription,
        notification: &PushNotification,
    ) {
        let message = match notification.to_web_push_message(&subscription.as_subscription_info()) {
            Ok(message) => message,
            Err(error) => {
                tracing::warn!(
                    battle_id = %notice.battle_id,
                    subscription_id = subscription.id,
                    "Battle notice not sent, the push message could not be built: {error:?}"
                );
                return;
            }
        };

        match tokio::time::timeout(self.timeout, self.client.send(message)).await {
            Ok(Ok(())) => {}
            Ok(Err(WebPushError::EndpointNotValid | WebPushError::EndpointNotFound)) => {
                // The browser unsubscribed or the subscription expired.
                tracing::info!(
                    subscription_id = subscription.id,
                    user_id = subscription.user_id,
                    "Deleting a push subscription that no longer exists"
                );
                if let Err(error) = self
                    .subscriptions
                    .delete_push_subscription_if_endpoint(&subscription.id, &subscription.endpoint)
                    .await
                {
                    tracing::warn!(
                        subscription_id = subscription.id,
                        "Push subscription could not be deleted: {error}"
                    );
                }
            }
            Ok(Err(error)) => tracing::warn!(
                battle_id = %notice.battle_id,
                subscription_id = subscription.id,
                "Battle notice not sent: {error:?}"
            ),
            Err(_) => tracing::warn!(
                battle_id = %notice.battle_id,
                subscription_id = subscription.id,
                "Battle notice not sent, the push service did not answer within {} s",
                self.timeout.as_secs_f64()
            ),
        }
    }
}

/// The battle's replay in the app.
fn battle_url(app_url: &Url, battle_id: BattleId) -> String {
    format!(
        "{}/buddies/battles/{battle_id}",
        app_url.as_str().trim_end_matches('/')
    )
}

/// For example "Anna Lind battled your buddies".
fn battle_title(notice: &BattleNotice) -> String {
    format!("{} battled your buddies", notice.challenger_name)
}

/// For example "Your Pikachu and Bulbasaur won!", "Your Pikachu lost." or "It was a
/// draw.", from the defender's side.
fn battle_body(notice: &BattleNotice) -> String {
    let buddies = match notice.defender_buddies.as_slice() {
        [] => "buddies".to_string(),
        [buddy] => buddy.clone(),
        [others @ .., last] => format!("{} and {last}", others.join(", ")),
    };
    match notice.result {
        BattleResult::Win => format!("Your {buddies} won!"),
        BattleResult::Loss => format!("Your {buddies} lost."),
        BattleResult::Draw => "It was a draw.".to_string(),
    }
}

#[cfg(test)]
mod tests;
