use crate::repositories::RepoRepository;
use futures::future;
use sqlx::PgPool;
use web_push::{IsahcWebPushClient, WebPushClient};

use crate::domain::{Notification, NotificationType};
use crate::repositories::{
    NotificationRepository, NotificationRepositoryImpl, PushSubscriptionRepository,
    PushSubscriptionRepositoryImpl, RepoRepositoryImpl, UserRepository, UserRepositoryImpl,
};

use super::{PRChangeEvent, PullRequestDiff, RepoKey};

pub struct NotificationHandler {
    push_subscriptions_repo: PushSubscriptionRepositoryImpl,
    user_repo: UserRepositoryImpl,
    notification_repo: NotificationRepositoryImpl,
    repo_repo: RepoRepositoryImpl,
    web_push_client: IsahcWebPushClient,
}

impl NotificationHandler {
    pub fn new(db_pool: PgPool, web_push_client: IsahcWebPushClient) -> Self {
        Self {
            push_subscriptions_repo: PushSubscriptionRepositoryImpl::new(db_pool.clone()),
            user_repo: UserRepositoryImpl::new(db_pool.clone()),
            notification_repo: NotificationRepositoryImpl::new(db_pool.clone()),
            repo_repo: RepoRepositoryImpl::new(db_pool),
            web_push_client,
        }
    }

    pub async fn notify_affected_users(&self, diffs: Vec<PullRequestDiff>) {
        let users = self.user_repo.get_users().await.unwrap();
        let repos = self.repo_repo.get_repositories().await.unwrap();
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
                let repo_id = repos
                    .iter()
                    .find(|r| RepoKey::from(&diff.pr) == RepoKey::from(*r))
                    .unwrap()
                    .id;
                let pr_id = diff.pr.pull_request_base.id;

                // Get notification rules for this repository
                let rules = self
                    .notification_repo
                    .get_repository_rules(user.id, repo_id)
                    .await
                    .unwrap();

                // Get PR-specific exceptions
                let exceptions = self
                    .notification_repo
                    .get_pr_exceptions(user.id, repo_id, pr_id)
                    .await
                    .unwrap();

                for event in diff.changes.iter() {
                    // Map event to notification type
                    let notification_type = match event {
                        PRChangeEvent::PullRequestClosed => NotificationType::PrClosed,
                        PRChangeEvent::ThreadAdded(_) => NotificationType::ThreadAdded,
                        PRChangeEvent::ThreadUpdated(_) => NotificationType::ThreadUpdated,
                    };

                    // Check if notification is enabled via rules/exceptions
                    let rule = rules
                        .iter()
                        .find(|r| r.notification_type == notification_type);
                    let exception = exceptions
                        .iter()
                        .find(|e| e.notification_type == notification_type);

                    let is_enabled = match (rule, exception) {
                        (_, Some(e)) => e.enabled,
                        (Some(r), None) => r.enabled,
                        (None, None) => false, // Default to disabled if no rule exists
                    };

                    if !is_enabled {
                        continue;
                    }

                    // Create notification in database
                    let push_notif = event
                        .to_push_notification(&diff.pr.pull_request_base, &diff.pr.azure_url());

                    let notification = Notification {
                        id: 0, // Will be set by database
                        user_id: user.id,
                        repository_id: repo_id,
                        pull_request_id: pr_id,
                        notification_type,
                        title: push_notif.title.clone(),
                        message: push_notif.body.clone(),
                        link: push_notif.url.clone(),
                        viewed_at: None,
                        created_at: time::OffsetDateTime::now_utc(),
                        metadata: serde_json::Value::Null,
                    };

                    if (self
                        .notification_repo
                        .create_notification(&notification)
                        .await)
                        .is_ok()
                    {
                        // Send push notification if enabled
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
            }

            future::join_all(push_futures).await;
        }
    }
}
