use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[sqlx(type_name = "notification_type", rename_all = "snake_case")]
pub enum NotificationType {
    PrClosed,
    ThreadAdded,
    ThreadUpdated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRule {
    pub id: i32,
    pub user_id: i32,
    pub repository_id: i32,
    pub notification_type: NotificationType,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrNotificationException {
    pub id: i32,
    pub user_id: i32,
    pub repository_id: i32,
    pub pull_request_id: i32,
    pub notification_type: NotificationType,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: i32,
    pub user_id: i32,
    pub repository_id: i32,
    pub pull_request_id: i32,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub link: Option<String>,
    pub viewed_at: Option<time::OffsetDateTime>,
    pub created_at: time::OffsetDateTime,
    pub metadata: serde_json::Value,
}
