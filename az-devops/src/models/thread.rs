use std::collections::HashMap;

use azure_devops_rust_api::git::models::{comment_thread::Status, CommentThread};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::Identity;

use super::comment::Comment;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
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

impl Thread {
    /// Checks if the first comment in the thread is a system comment.
    pub fn is_system_thread(&self) -> bool {
        self.comments.first().is_some_and(|c| c.is_system_comment())
    }

    pub fn author(&self) -> &Identity {
        &self
            .comments
            .first()
            .expect("Thread has no comments, should this be possible!?")
            .author
    }

    pub fn most_recent_comment(&self) -> &Comment {
        self.comments
            .last()
            .expect("Thread has no comments, should this be possible!?")
    }

    /// Expects a map of `<ID, Name>`
    pub fn with_replaced_mentions(&self, name_map: &HashMap<String, String>) -> Thread {
        Thread {
            id: self.id,
            comments: self
                .comments
                .iter()
                .map(|c| c.with_replaced_mentions(name_map))
                .collect(),
            status: self.status.clone(),
            is_deleted: self.is_deleted,
            last_updated_at: self.last_updated_at,
            published_at: self.published_at,
        }
    }
}
