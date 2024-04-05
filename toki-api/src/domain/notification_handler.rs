use sqlx::PgPool;
use web_push::{IsahcWebPushClient, WebPushClient};

use crate::{
    domain::PushNotification,
    repositories::{PushSubscriptionRepository, PushSubscriptionRepositoryImpl},
};

use super::{PRChangeEvent, PullRequest};

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

    pub async fn notify_affected_users(&self, events: Vec<(PullRequest, Vec<PRChangeEvent>)>) {
        println!("Handling events: {:?}", events);

        let push_subscriptions = self
            .push_subscriptions_repo
            .get_push_subscriptions()
            .await
            .unwrap();

        for subscriber in push_subscriptions {
            for (pr, changes) in &events {
                let content =
                    PushNotification::new(&pr.pull_request_base.title, "Has an event!", None);
                let message = content
                    .to_web_push_message(&subscriber.as_subscription_info())
                    .expect("Failed to create web push message");

                self.web_push_client
                    .send(message)
                    .await
                    .expect("Failed to send notification");
            }
        }
    }
}
