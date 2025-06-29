use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::domain::PRChangeEvent;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, PartialEq, Eq, Hash, EnumIter)]
#[sqlx(type_name = "notification_type", rename_all = "snake_case")]
pub enum DbNotificationType {
    PrClosed,
    ThreadAdded,
    ThreadUpdated,
    CommentMentioned,
}

impl DbNotificationType {
    pub fn default_enabled(&self) -> bool {
        match self {
            DbNotificationType::PrClosed => false,
            DbNotificationType::ThreadAdded => false,
            DbNotificationType::ThreadUpdated => false,
            DbNotificationType::CommentMentioned => true,
        }
    }
}

impl From<&PRChangeEvent> for DbNotificationType {
    fn from(event: &PRChangeEvent) -> Self {
        match event {
            PRChangeEvent::PullRequestClosed => DbNotificationType::PrClosed,
            PRChangeEvent::ThreadAdded(_) => DbNotificationType::ThreadAdded,
            PRChangeEvent::ThreadUpdated(_) => DbNotificationType::ThreadUpdated,
            PRChangeEvent::CommentMentioned(_, _) => DbNotificationType::CommentMentioned,
        }
    }
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
