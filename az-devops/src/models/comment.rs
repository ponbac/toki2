use azure_devops_rust_api::git::models::{comment::CommentType, Comment as AzureComment};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::Identity;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment {
    pub id: i64,
    pub author: Identity,
    pub content: String,
    pub comment_type: CommentType,
    pub is_deleted: Option<bool>,
    #[serde(with = "time::serde::rfc3339")]
    pub published_at: OffsetDateTime,
    pub liked_by: Vec<Identity>,
}

impl From<AzureComment> for Comment {
    fn from(comment: AzureComment) -> Self {
        Self {
            id: comment.id.unwrap(),
            author: comment.author.unwrap().into(),
            content: comment.content.unwrap(),
            comment_type: comment.comment_type.unwrap(),
            is_deleted: comment.is_deleted,
            published_at: comment.published_date.unwrap(),
            liked_by: comment
                .users_liked
                .into_iter()
                .map(Identity::from)
                .collect(),
        }
    }
}
