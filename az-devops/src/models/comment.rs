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
            Some(content) => find_mention_matches(content)
                .into_iter()
                .map(|m| m.id.to_uppercase())
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

        let new_content = self
            .content
            .as_ref()
            .map(|content| replace_mentions(content, &upper_name_map));

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

const MENTION_START_PATTERN: &str = "@<";
const MENTION_START_PATTERN_LEN: usize = MENTION_START_PATTERN.len();
const MENTION_END_PATTERN: &str = ">";
const MENTION_END_PATTERN_LEN: usize = MENTION_END_PATTERN.len();

#[derive(Debug, Clone, PartialEq)]
struct MentionMatch {
    start: usize,
    end: usize,
    id: String,
}

/// Returns all mention matches in the content
fn find_mention_matches(content: &str) -> Vec<MentionMatch> {
    let mut matches = Vec::new();

    let mut search_start = 0;
    while let Some(start_pos) = content[search_start..].find(MENTION_START_PATTERN) {
        let absolute_start = search_start + start_pos;
        let search_from = absolute_start + MENTION_START_PATTERN_LEN;

        if let Some(end_pos) = content[search_from..].find(MENTION_END_PATTERN) {
            let mention_id_end = search_from + end_pos;
            let mention_id = content[search_from..mention_id_end].to_string();

            matches.push(MentionMatch {
                start: absolute_start,
                end: mention_id_end + MENTION_END_PATTERN_LEN,
                id: mention_id,
            });

            search_start = mention_id_end + MENTION_END_PATTERN_LEN;
        } else {
            break;
        }
    }

    matches
}

/// Replaces mentions in the content while preserving all surrounding text and formatting.
/// This maintains the original structure of the content, including punctuation and whitespace.
fn replace_mentions(content: &str, name_map: &HashMap<String, String>) -> String {
    let matches = find_mention_matches(content);

    if matches.is_empty() {
        return content.to_string();
    }

    let mut result = String::new();

    let mut last_end = 0;
    for mention in matches {
        // Add content before this mention
        result.push_str(&content[last_end..mention.start]);

        // Replace the mention if we have a mapping, otherwise keep original
        if let Some(name) = name_map.get(&mention.id.to_uppercase()) {
            result.push_str(&format!("@<{name}>"));
        } else {
            result.push_str(&content[mention.start..mention.end]);
        }

        last_end = mention.end;
    }

    // Add any remaining content after the last mention
    result.push_str(&content[last_end..]);
    result
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

#[cfg(test)]
mod tests {
    use super::*;
    use azure_devops_rust_api::git::models::comment::CommentType;
    use time::OffsetDateTime;

    fn create_test_comment(content: Option<String>) -> Comment {
        Comment {
            id: 1,
            author: Identity {
                display_name: "Test User".to_string(),
                id: "test-id".to_string(),
                unique_name: "test@example.com".to_string(),
                avatar_url: None,
            },
            content,
            comment_type: Some(CommentType::Text),
            is_deleted: Some(false),
            published_at: OffsetDateTime::now_utc(),
            liked_by: vec![],
        }
    }

    #[test]
    fn test_find_mentions_with_punctuation() {
        // Tests both basic mentions and mentions followed by punctuation
        let content = "Hey @<user123>! What about @<user456>?";
        let mentions: Vec<String> = find_mention_matches(content)
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(mentions, vec!["user123", "user456"]);
    }

    #[test]
    fn test_find_mentions_multiple_and_boundaries() {
        // Tests multiple mentions and mentions at start/end of content
        let content = "@<alice> and @<bob> should review @<charlie>'s code@<end>";
        let mentions: Vec<String> = find_mention_matches(content)
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(mentions, vec!["alice", "bob", "charlie", "end"]);
    }

    #[test]
    fn test_find_mentions_edge_cases() {
        // Tests empty content, no mentions, and malformed mentions
        assert_eq!(find_mention_matches(""), Vec::<MentionMatch>::new());
        assert_eq!(
            find_mention_matches("This is just regular text"),
            Vec::<MentionMatch>::new()
        );
        assert_eq!(
            find_mention_matches("This has @< incomplete and @<no-closing"),
            Vec::<MentionMatch>::new()
        );
    }

    #[test]
    fn test_mentions_method_integration() {
        // Tests the public method with uppercase conversion and None content
        let comment = create_test_comment(Some("Hello @<user123> and @<UserABC>".to_string()));
        assert_eq!(comment.mentions(), vec!["USER123", "USERABC"]);

        let empty_comment = create_test_comment(None);
        assert_eq!(empty_comment.mentions(), Vec::<String>::new());
    }

    #[test]
    fn test_replace_mentions_comprehensive() {
        // Tests basic replacement and punctuation preservation
        let content = "Hey @<user123>! What about @<user456>?";
        let mut name_map = HashMap::new();
        name_map.insert("USER123".to_string(), "Alice".to_string());
        name_map.insert("USER456".to_string(), "Bob".to_string());

        let result = replace_mentions(content, &name_map);
        assert_eq!(result, "Hey @<Alice>! What about @<Bob>?");
    }

    #[test]
    fn test_replace_mentions_partial_and_empty() {
        // Tests partial mapping and empty content scenarios
        let content = "@<user123> and @<user456> are here";
        let mut name_map = HashMap::new();
        name_map.insert("USER123".to_string(), "Alice".to_string());

        let result = replace_mentions(content, &name_map);
        assert_eq!(result, "@<Alice> and @<user456> are here");

        // Test empty content
        assert_eq!(replace_mentions("", &HashMap::new()), "");
    }

    #[test]
    fn test_replace_mentions_complex_formatting() {
        // Tests complex formatting preservation (newlines, multiple mentions)
        let content = "Review needed:\n- @<user123>: frontend\n- @<user456>: backend\n\nThanks!";
        let mut name_map = HashMap::new();
        name_map.insert("USER123".to_string(), "Alice Smith".to_string());
        name_map.insert("USER456".to_string(), "Bob Johnson".to_string());

        let result = replace_mentions(content, &name_map);
        assert_eq!(
            result,
            "Review needed:\n- @<Alice Smith>: frontend\n- @<Bob Johnson>: backend\n\nThanks!"
        );
    }

    #[test]
    fn test_with_replaced_mentions_integration() {
        // Tests the public method end-to-end integration
        let comment =
            create_test_comment(Some("Hey @<user123>! Can you help @<user456>?".to_string()));
        let mut name_map = HashMap::new();
        name_map.insert("user123".to_string(), "Alice".to_string());
        name_map.insert("user456".to_string(), "Bob".to_string());

        let updated_comment = comment.with_replaced_mentions(&name_map);
        assert_eq!(
            updated_comment.content,
            Some("Hey @<Alice>! Can you help @<Bob>?".to_string())
        );
    }
}
