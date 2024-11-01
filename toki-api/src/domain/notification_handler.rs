use futures::future;
use sqlx::PgPool;
use web_push::{IsahcWebPushClient, WebPushClient};

use crate::repositories::{PushSubscriptionRepository, PushSubscriptionRepositoryImpl};

use super::PullRequestDiff;

pub struct NotificationHandler {
    push_subscriptions_repo: PushSubscriptionRepositoryImpl,
    web_push_client: IsahcWebPushClient,
}

impl NotificationHandler {
    pub fn new(db_pool: PgPool, web_push_client: IsahcWebPushClient) -> Self {
        Self {
            push_subscriptions_repo: PushSubscriptionRepositoryImpl::new(db_pool),
            web_push_client,
        }
    }

    pub async fn notify_affected_users(&self, diffs: Vec<PullRequestDiff>) {
        let push_subscriptions = self
            .push_subscriptions_repo
            .get_push_subscriptions()
            .await
            .unwrap();

        // Process notifications concurrently for each subscriber
        let notification_futures = push_subscriptions.into_iter().map(|subscriber| {
            let notifications = diffs.iter().flat_map(|diff| {
                let subscriber = subscriber.clone();
                diff.changes.iter().map(move |event| {
                    let message = event.to_web_push_message(&subscriber, &diff.pr, &diff.url);
                    let endpoint = subscriber.endpoint.clone();

                    async move {
                        match self.web_push_client.send(message).await {
                            Ok(_) => {
                                tracing::info!(
                                    "Successfully sent notification to {} for {}",
                                    endpoint,
                                    event
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to send notification to {} for {}: {}",
                                    endpoint,
                                    event,
                                    e
                                );
                            }
                        }
                    }
                })
            });

            future::join_all(notifications)
        });

        // Wait for all notifications to complete
        future::join_all(notification_futures).await;
    }
}
