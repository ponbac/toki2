use azure_devops_rust_api::git::models::{comment_thread::Status, CommentThread};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::comment::Comment;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Thread {
    pub id: i32,
    pub comments: Vec<Comment>,
    pub status: Option<Status>,
    pub is_deleted: Option<bool>,
    #[serde(with = "time::serde::rfc3339")]
    pub last_updated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub published_at: OffsetDateTime,
}

impl From<CommentThread> for Thread {
    fn from(thread: CommentThread) -> Self {
        Self {
            id: thread.id.unwrap(),
            comments: thread.comments.into_iter().map(Comment::from).collect(),
            status: thread.status,
            is_deleted: thread.is_deleted,
            last_updated_at: thread.last_updated_date.unwrap(),
            published_at: thread.published_date.unwrap(),
        }
    }
}
