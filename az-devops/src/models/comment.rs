use std::collections::HashMap;

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

    pub fn mentions(&self) -> Vec<String> {
        match &self.content {
            Some(content) => content
                .split_whitespace()
                .filter_map(strip_mention)
                .map(|s| s.to_uppercase())
                .collect(),
            None => vec![],
        }
    }

    /// Expects a map of `<ID, Name>`
    pub fn with_replaced_mentions(&self, name_map: &HashMap<String, String>) -> Comment {
        let upper_name_map = name_map
            .iter()
            .map(|(k, v)| (k.to_uppercase(), v.to_string()))
            .collect::<HashMap<String, String>>();

        let new_content = self.content.as_ref().map(|content| {
            content
                .split_whitespace()
                .map(|mention| {
                    strip_mention(mention)
                        .and_then(|id| {
                            upper_name_map
                                .get(&id.to_uppercase())
                                .map(|name| format!("@<{name}>"))
                        })
                        .unwrap_or(mention.to_string())
                })
                .collect::<Vec<String>>()
                .join(" ")
        });

        Comment {
            id: self.id,
            author: self.author.clone(),
            content: new_content,
            comment_type: self.comment_type.clone(),
            is_deleted: self.is_deleted,
            published_at: self.published_at,
            liked_by: self.liked_by.clone(),
        }
    }
}

fn strip_mention(s: &str) -> Option<String> {
    s.strip_prefix("@<")
        .and_then(|s| s.strip_suffix(">"))
        .map(|s| s.to_string())
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
