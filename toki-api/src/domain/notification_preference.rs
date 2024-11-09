use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[sqlx(type_name = "notification_type", rename_all = "snake_case")]
pub enum DbNotificationType {
    PrClosed,
    ThreadAdded,
    ThreadUpdated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationRule {
    pub id: i32,
    pub user_id: i32,
    pub repository_id: i32,
    pub notification_type: DbNotificationType,
    pub enabled: bool,
    pub push_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrNotificationException {
    pub id: i32,
    pub user_id: i32,
    pub repository_id: i32,
    pub pull_request_id: i32,
    pub notification_type: DbNotificationType,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: i32,
    pub user_id: i32,
    pub repository_id: i32,
    pub pull_request_id: i32,
    pub notification_type: DbNotificationType,
    pub title: String,
    pub message: String,
    pub link: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub viewed_at: Option<time::OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    pub metadata: serde_json::Value,
}
