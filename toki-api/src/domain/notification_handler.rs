use std::sync::Arc;

use axum::async_trait;
use sqlx::PgPool;

use super::RepoDifferMessage;

#[async_trait]
trait NotificationHandler {
    async fn fetch_users_and_notify(&self, event: RepoDifferMessage);
}

struct DatabaseNotificationHandler {
    db_pool: Arc<PgPool>,
}

impl DatabaseNotificationHandler {
    pub fn new(db_pool: Arc<PgPool>) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl NotificationHandler for DatabaseNotificationHandler {
    async fn fetch_users_and_notify(&self, event: RepoDifferMessage) {
        // Use self.db_pool to fetch users and send notifications
        println!("Handling event: {:?}", event);
        // Placeholder for the actual logic
    }
}
