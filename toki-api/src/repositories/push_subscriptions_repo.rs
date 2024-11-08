use sqlx::PgPool;

use crate::domain::PushSubscription;

use super::repo_error::RepositoryError;

pub trait PushSubscriptionRepository {
    async fn get_push_subscriptions(&self) -> Result<Vec<PushSubscription>, RepositoryError>;
    async fn get_user_push_subscriptions(
        &self,
        user_id: i32,
    ) -> Result<Vec<PushSubscription>, RepositoryError>;
    async fn upsert_push_subscription(
        &self,
        push_subscription: NewPushSubscription,
    ) -> Result<(), RepositoryError>;
}

pub struct PushSubscriptionRepositoryImpl {
    pool: PgPool,
}

impl PushSubscriptionRepositoryImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl PushSubscriptionRepository for PushSubscriptionRepositoryImpl {
    async fn get_push_subscriptions(&self) -> Result<Vec<PushSubscription>, RepositoryError> {
        let push_subscriptions = sqlx::query_as!(
            PushSubscription,
            r#"
            SELECT id, user_id, device, endpoint, auth, p256dh, created_at
            FROM push_subscriptions
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(push_subscriptions)
    }

    async fn get_user_push_subscriptions(
        &self,
        user_id: i32,
    ) -> Result<Vec<PushSubscription>, RepositoryError> {
        let push_subscriptions = sqlx::query_as!(
            PushSubscription,
            r#"
            SELECT id, user_id, device, endpoint, auth, p256dh, created_at
            FROM push_subscriptions
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(push_subscriptions)
    }

    async fn upsert_push_subscription(
        &self,
        push_subscription: NewPushSubscription,
    ) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            INSERT INTO push_subscriptions (user_id, device, endpoint, auth, p256dh)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id, device) DO UPDATE
            SET endpoint = EXCLUDED.endpoint,
                auth = EXCLUDED.auth,
                p256dh = EXCLUDED.p256dh
            "#,
            push_subscription.user_id,
            push_subscription.device,
            push_subscription.endpoint,
            push_subscription.auth,
            push_subscription.p256dh
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

pub struct NewPushSubscription {
    pub user_id: i32,
    pub device: String,
    pub endpoint: String,
    pub auth: String,
    pub p256dh: String,
}
