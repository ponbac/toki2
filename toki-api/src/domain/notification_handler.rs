use crate::repositories::RepoRepository;
use futures::future;
use sqlx::PgPool;
use web_push::{IsahcWebPushClient, WebPushClient};

use crate::domain::{DbNotificationType, Notification};
use crate::repositories::{
    NotificationRepository, NotificationRepositoryImpl, PushSubscriptionRepository,
    PushSubscriptionRepositoryImpl, RepoRepositoryImpl, UserRepository, UserRepositoryImpl,
};

use super::{PullRequestDiff, RepoKey};

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

    pub async fn notify_affected_users(&self, diffs: Vec<PullRequestDiff>) -> Result<(), String> {
        let users = self
            .user_repo
            .get_users()
            .await
            .map_err(|e| format!("Failed to get users: {e}"))?;
        let repos = self
            .repo_repo
            .get_repositories()
            .await
            .map_err(|e| format!("Failed to get repositories: {e}"))?;
        let push_subscriptions = self
            .push_subscriptions_repo
            .get_push_subscriptions()
            .await
            .map_err(|e| format!("Failed to get push subscriptions: {e}"))?;

        for user in users {
            let following = self
                .user_repo
                .followed_repositories(&user.id)
                .await
                .map_err(|e| {
                    format!(
                        "Failed to get followed repositories for user {}: {}",
                        user.id, e
                    )
                })?;

            // Filter out diffs for unfollowed repos
            let diffs_for_user: Vec<_> = diffs
                .iter()
                .filter(|diff| following.contains(&RepoKey::from(&diff.pr)))
                .collect();

            let push_subscriptions_for_user: Vec<_> = push_subscriptions
                .iter()
                .filter(|sub| sub.user_id == user.id)
                .collect();

            let mut push_futures = vec![];
            for diff in diffs_for_user {
                let repo_id = repos
                    .iter()
                    .find(|r| RepoKey::from(&diff.pr) == RepoKey::from(*r))
                    .ok_or_else(|| {
                        format!(
                            "Repository not found for PR {}",
                            diff.pr.pull_request_base.id
                        )
                    })?
                    .id;
                let pr_id = diff.pr.pull_request_base.id;

                // Get notification rules for this repository
                let rules = self
                    .notification_repo
                    .get_repository_rules(user.id, repo_id)
                    .await
                    .map_err(|e| {
                        format!(
                            "Failed to get notification rules for user {} and repo {}: {}",
                            user.id, repo_id, e
                        )
                    })?;

                // Get PR-specific exceptions
                let exceptions = self
                    .notification_repo
                    .get_pr_exceptions(user.id, repo_id, pr_id)
                    .await
                    .map_err(|e| {
                        format!(
                            "Failed to get PR exceptions for user {} and PR {}: {}",
                            user.id, pr_id, e
                        )
                    })?;

                // Process events that apply to the user
                for event in diff.changes.iter().filter(|e| {
                    e.applies_to(
                        &user.email,
                        &diff.pr.pull_request_base.created_by.unique_name,
                    )
                }) {
                    let notification_type = DbNotificationType::from(event);

                    // Check if notification is enabled via rules/exceptions
                    let rule = rules
                        .iter()
                        .find(|r| r.notification_type == notification_type);
                    let exception = exceptions
                        .iter()
                        .find(|e| e.notification_type == notification_type);

                    let is_enabled = match (rule, exception) {
                        (_, Some(e)) => e.enabled, // exception overrides rule
                        (Some(r), None) => r.enabled,
                        (None, None) => notification_type.default_enabled(),
                    };

                    if !is_enabled {
                        continue;
                    }

                    let push_notification =
                        event.to_push_notification(&diff.pr.pull_request_base, &diff.pr.url);
                    let db_notification = Notification {
                        id: 0, // Will be set by database
                        user_id: user.id,
                        repository_id: repo_id,
                        pull_request_id: pr_id,
                        notification_type,
                        title: diff.pr.pull_request_base.title.clone(),
                        message: push_notification.body.clone(),
                        link: push_notification.url.clone(),
                        viewed_at: None,
                        created_at: time::OffsetDateTime::now_utc(),
                        metadata: serde_json::Value::Null,
                    };

                    if (self
                        .notification_repo
                        .create_notification(&db_notification)
                        .await)
                        .is_ok()
                    {
                        // Send push notification if enabled
                        let push_enabled = match (rule, exception) {
                            (_, Some(e)) => e.enabled, // exception overrides rule
                            (Some(r), None) => r.push_enabled,
                            (None, None) => false,
                        };
                        if push_enabled {
                            for sub in push_subscriptions_for_user.iter() {
                                let message = event.to_web_push_message(
                                    sub,
                                    &diff.pr.pull_request_base,
                                    &diff.pr.url,
                                );
                                push_futures.push(self.web_push_client.send(message));
                            }
                        }
                    }
                }
            }

            future::join_all(push_futures).await;
        }

        Ok(())
    }
}
