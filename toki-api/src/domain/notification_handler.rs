use futures::future;
use sqlx::PgPool;
use web_push::{IsahcWebPushClient, WebPushClient};

use crate::repositories::{
    PushSubscriptionRepository, PushSubscriptionRepositoryImpl, UserRepository, UserRepositoryImpl,
};

use super::{PullRequestDiff, RepoKey, User};

pub struct NotificationHandler {
    push_subscriptions_repo: PushSubscriptionRepositoryImpl,
    user_repo: UserRepositoryImpl,
    web_push_client: IsahcWebPushClient,
}

impl NotificationHandler {
    pub fn new(db_pool: PgPool, web_push_client: IsahcWebPushClient) -> Self {
        Self {
            push_subscriptions_repo: PushSubscriptionRepositoryImpl::new(db_pool.clone()),
            user_repo: UserRepositoryImpl::new(db_pool),
            web_push_client,
        }
    }

    pub async fn notify_affected_users(&self, diffs: Vec<PullRequestDiff>) {
        let users = self.user_repo.get_users().await.unwrap();
        let push_subscriptions = self
            .push_subscriptions_repo
            .get_push_subscriptions()
            .await
            .unwrap();

        for user in users {
            let following = self
                .user_repo
                .followed_repositories(&user.id)
                .await
                .unwrap();
            // Filter out diffs for unfollowed repos
            let diffs_for_user = diffs
                .iter()
                .filter(|diff| following.contains(&RepoKey::from(&diff.pr)));

            let push_subscriptions_for_user: Vec<_> = push_subscriptions
                .iter()
                .filter(|sub| sub.user_id == user.id)
                .collect();

            let mut push_futures = vec![];
            for diff in diffs_for_user {
                if !self.should_notify(&user, diff) {
                    continue;
                }

                for event in diff.changes.iter() {
                    for sub in push_subscriptions_for_user.iter() {
                        let message = event.to_web_push_message(
                            sub,
                            &diff.pr.pull_request_base,
                            &diff.pr.azure_url(),
                        );

                        push_futures.push(self.web_push_client.send(message));
                    }
                }
            }

            future::join_all(push_futures).await;
        }

        // Process notifications concurrently for each subscriber
        // let notification_futures = push_subscriptions.into_iter().map(|subscriber| {
        //     let notifications = diffs.iter().flat_map(|diff| {
        //         let subscriber = subscriber.clone();
        //         diff.changes.iter().map(move |event| {
        //             let message = event.to_web_push_message(
        //                 &subscriber,
        //                 &diff.pr.pull_request_base,
        //                 &diff.pr.azure_url(),
        //             );
        //             let endpoint = subscriber.endpoint.clone();

        //             async move {
        //                 match self.web_push_client.send(message).await {
        //                     Ok(_) => {
        //                         tracing::info!(
        //                             "Successfully sent notification to {} for {}",
        //                             endpoint,
        //                             event
        //                         );
        //                     }
        //                     Err(e) => {
        //                         tracing::error!(
        //                             "Failed to send notification to {} for {}: {}",
        //                             endpoint,
        //                             event,
        //                             e
        //                         );
        //                     }
        //                 }
        //             }
        //         })
        //     });

        //     future::join_all(notifications)
        // });

        // // Wait for all notifications to complete
        // future::join_all(notification_futures).await;
    }

    fn should_notify(&self, user: &User, diff: &PullRequestDiff) -> bool {
        true
    }
}
