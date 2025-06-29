use async_trait::async_trait;
use sqlx::PgPool;

use crate::domain::{DbNotificationType, Notification, NotificationRule, PrNotificationException};
use crate::repositories::repo_error::RepositoryError;

#[async_trait]
pub trait NotificationRepository {
    async fn create_notification(
        &self,
        notification: &Notification,
    ) -> Result<i32, RepositoryError>;
    async fn get_user_notifications(
        &self,
        user_id: i32,
        include_viewed: bool,
        max_age_days: i32,
    ) -> Result<Vec<Notification>, RepositoryError>;
    async fn mark_as_viewed(
        &self,
        notification_id: i32,
        user_id: i32,
    ) -> Result<(), RepositoryError>;
    async fn mark_all_notifications_viewed(&self, user_id: i32) -> Result<(), RepositoryError>;
    async fn delete_notification(
        &self,
        notification_id: i32,
        user_id: i32,
    ) -> Result<(), RepositoryError>;

    async fn get_repository_rules(
        &self,
        user_id: i32,
        repository_id: i32,
    ) -> Result<Vec<NotificationRule>, RepositoryError>;
    async fn update_rule(
        &self,
        rule: &NotificationRule,
    ) -> Result<NotificationRule, RepositoryError>;

    async fn get_pr_exceptions(
        &self,
        user_id: i32,
        repository_id: i32,
        pull_request_id: i32,
    ) -> Result<Vec<PrNotificationException>, RepositoryError>;
    async fn set_pr_exception(
        &self,
        exception: &PrNotificationException,
    ) -> Result<PrNotificationException, RepositoryError>;
    async fn remove_pr_exception(
        &self,
        user_id: i32,
        repository_id: i32,
        pull_request_id: i32,
        notification_type: DbNotificationType,
    ) -> Result<(), RepositoryError>;
}

pub struct NotificationRepositoryImpl {
    pool: PgPool,
}

impl NotificationRepositoryImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl NotificationRepository for NotificationRepositoryImpl {
    async fn create_notification(
        &self,
        notification: &Notification,
    ) -> Result<i32, RepositoryError> {
        Ok(sqlx::query_scalar!(
            r#"
            INSERT INTO notifications (
                user_id, repository_id, pull_request_id, notification_type,
                title, message, link, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id
            "#,
            notification.user_id,
            notification.repository_id,
            notification.pull_request_id,
            notification.notification_type.clone() as DbNotificationType,
            notification.title,
            notification.message,
            notification.link,
            notification.metadata
        )
        .fetch_one(&self.pool)
        .await?)
    }

    async fn get_user_notifications(
        &self,
        user_id: i32,
        include_viewed: bool,
        max_age_days: i32,
    ) -> Result<Vec<Notification>, RepositoryError> {
        Ok(sqlx::query_as!(
            Notification,
            r#"SELECT id, user_id, repository_id, pull_request_id,
                     notification_type as "notification_type: DbNotificationType",
                     title, message, link, viewed_at, created_at, metadata
              FROM notifications
              WHERE user_id = $1
                AND ($2 OR viewed_at IS NULL)
                AND created_at > NOW() - make_interval(days => $3)
              ORDER BY created_at DESC"#,
            user_id,
            include_viewed,
            max_age_days
        )
        .fetch_all(&self.pool)
        .await?)
    }

    async fn mark_as_viewed(
        &self,
        notification_id: i32,
        user_id: i32,
    ) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            UPDATE notifications
            SET viewed_at = CURRENT_TIMESTAMP
            WHERE id = $1 AND user_id = $2
            "#,
            notification_id,
            user_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn mark_all_notifications_viewed(&self, user_id: i32) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            UPDATE notifications
            SET viewed_at = CURRENT_TIMESTAMP
            WHERE user_id = $1 AND viewed_at IS NULL
            "#,
            user_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_notification(
        &self,
        notification_id: i32,
        user_id: i32,
    ) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            DELETE FROM notifications
            WHERE id = $1 AND user_id = $2
            "#,
            notification_id,
            user_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_repository_rules(
        &self,
        user_id: i32,
        repository_id: i32,
    ) -> Result<Vec<NotificationRule>, RepositoryError> {
        Ok(sqlx::query_as!(
            NotificationRule,
            r#"
            SELECT 
                id, user_id, repository_id,
                notification_type as "notification_type: DbNotificationType",
                enabled, push_enabled
            FROM notification_rules
            WHERE user_id = $1 AND repository_id = $2
            "#,
            user_id,
            repository_id
        )
        .fetch_all(&self.pool)
        .await?)
    }

    async fn update_rule(
        &self,
        rule: &NotificationRule,
    ) -> Result<NotificationRule, RepositoryError> {
        Ok(sqlx::query_as!(
            NotificationRule,
            r#"
            INSERT INTO notification_rules (
                user_id, repository_id, notification_type, enabled, push_enabled
            )
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id, repository_id, notification_type)
            DO UPDATE SET 
                enabled = EXCLUDED.enabled, 
                push_enabled = EXCLUDED.push_enabled,
                updated_at = CURRENT_TIMESTAMP
            RETURNING 
                id, user_id, repository_id,
                notification_type as "notification_type: DbNotificationType",
                enabled, push_enabled
            "#,
            rule.user_id,
            rule.repository_id,
            rule.notification_type.clone() as DbNotificationType,
            rule.enabled,
            rule.push_enabled
        )
        .fetch_one(&self.pool)
        .await?)
    }

    async fn get_pr_exceptions(
        &self,
        user_id: i32,
        repository_id: i32,
        pull_request_id: i32,
    ) -> Result<Vec<PrNotificationException>, RepositoryError> {
        Ok(sqlx::query_as!(
            PrNotificationException,
            r#"
            SELECT 
                id, user_id, repository_id, pull_request_id,
                notification_type as "notification_type: DbNotificationType",
                enabled
            FROM pr_notification_exceptions
            WHERE user_id = $1 
            AND repository_id = $2 
            AND pull_request_id = $3
            "#,
            user_id,
            repository_id,
            pull_request_id
        )
        .fetch_all(&self.pool)
        .await?)
    }

    async fn set_pr_exception(
        &self,
        exception: &PrNotificationException,
    ) -> Result<PrNotificationException, RepositoryError> {
        Ok(sqlx::query_as!(
            PrNotificationException,
            r#"
            INSERT INTO pr_notification_exceptions (
                user_id, repository_id, pull_request_id, notification_type, enabled
            )
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id, repository_id, pull_request_id, notification_type)
            DO UPDATE SET enabled = EXCLUDED.enabled, updated_at = CURRENT_TIMESTAMP
            RETURNING 
                id, user_id, repository_id, pull_request_id,
                notification_type as "notification_type: DbNotificationType",
                enabled
            "#,
            exception.user_id,
            exception.repository_id,
            exception.pull_request_id,
            exception.notification_type.clone() as DbNotificationType,
            exception.enabled
        )
        .fetch_one(&self.pool)
        .await?)
    }

    async fn remove_pr_exception(
        &self,
        user_id: i32,
        repository_id: i32,
        pull_request_id: i32,
        notification_type: DbNotificationType,
    ) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            DELETE FROM pr_notification_exceptions
            WHERE user_id = $1 
            AND repository_id = $2 
            AND pull_request_id = $3
            AND notification_type = $4
            "#,
            user_id,
            repository_id,
            pull_request_id,
            notification_type as DbNotificationType
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
