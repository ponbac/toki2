use azure_devops_rust_api::git::models::{comment::CommentType, Comment as AzureComment};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::Identity;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: i64,
    pub author: Identity,
    pub content: Option<String>,
    pub comment_type: Option<CommentType>,
    pub is_deleted: Option<bool>,
    #[serde(with = "time::serde::rfc3339")]
    pub published_at: OffsetDateTime,
    pub liked_by: Vec<Identity>,
}

impl Comment {
    pub fn is_system_comment(&self) -> bool {
        self.comment_type == Some(CommentType::System)
            || self.author.display_name == "Azure Pipelines Test Service"
    }
}

impl From<AzureComment> for Comment {
    fn from(comment: AzureComment) -> Self {
        Self {
            id: comment.id.unwrap(),
            author: comment.author.unwrap().into(),
            content: comment.content,
            comment_type: comment.comment_type,
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
