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

        for subscriber in push_subscriptions {
            for diff in &diffs {
                for event in &diff.changes {
                    let message = event.to_web_push_message(&subscriber, &diff.pr);
                    let _ = self.web_push_client.send(message).await;
                }
            }
        }
    }
}
