use std::fmt;

use crate::domain::Email;

use super::{PushNotification, PushSubscription};
use az_devops::Comment;

#[derive(Debug, Clone, PartialEq)]
pub enum PRChangeEvent {
    PullRequestClosed,
    ThreadAdded(az_devops::Thread),
    ThreadUpdated(az_devops::Thread),
    CommentMentioned(Comment, Email),
}

impl fmt::Display for PRChangeEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PRChangeEvent::PullRequestClosed => {
                write!(f, "PullRequestClosed")
            }
            PRChangeEvent::ThreadAdded(thread) => {
                write!(f, "ThreadAdded({})", thread.id)
            }
            PRChangeEvent::ThreadUpdated(thread) => {
                write!(f, "ThreadUpdated({})", thread.id)
            }
            PRChangeEvent::CommentMentioned(comment, mentioned_email) => {
                write!(
                    f,
                    "CommentMentioned(comment:{}, mentioned:{})",
                    comment.id, mentioned_email
                )
            }
        }
    }
}

impl PRChangeEvent {
    pub fn applies_to(&self, user_email: &str, pr_author: &str) -> bool {
        match self {
            PRChangeEvent::PullRequestClosed => true,
            PRChangeEvent::ThreadAdded(thread) => {
                thread.author().unique_name != user_email
                    && pr_author == user_email
                    && thread
                        .comments
                        .first()
                        .and_then(|comment| comment.content.as_ref())
                        .map_or(false, |content| !Self::is_ignored_message_content(&content))
            }
            PRChangeEvent::ThreadUpdated(thread) => {
                let most_recent_author = &thread.most_recent_comment().author;

                // Don't notify if you're the one who just commented
                if most_recent_author.unique_name == user_email {
                    return false;
                }

                // Don't notify if ignored message content
                let has_ignored_content = thread
                    .comments
                    .last()
                    .and_then(|c| c.content.as_ref())
                    .map_or(true, |content| Self::is_ignored_message_content(content));
                if has_ignored_content {
                    return false;
                }

                // Notify if you're the PR author
                if user_email == pr_author {
                    return true;
                }

                // Or if you've participated in this thread before
                thread
                    .comments
                    .iter()
                    .any(|comment| comment.author.unique_name == user_email)
            }
            PRChangeEvent::CommentMentioned(_, mentioned_email) => {
                mentioned_email.to_lowercase() == user_email.to_lowercase()
            }
        }
    }

    pub fn to_web_push_message(
        &self,
        sub: &PushSubscription,
        pr: &az_devops::PullRequest,
        url: &str,
    ) -> web_push::WebPushMessage {
        self.to_push_notification(pr, url)
            .to_web_push_message(&sub.as_subscription_info())
            .expect("Failed to create web push message")
    }

    pub fn to_push_notification(&self, pr: &az_devops::PullRequest, url: &str) -> PushNotification {
        match self {
            PRChangeEvent::PullRequestClosed => PushNotification::new(
                format!("{}: Pull Request Closed", pr.title).as_str(),
                format!("!{} has been closed.", pr.id).as_str(),
                Some(url),
                None,
            ),
            PRChangeEvent::ThreadAdded(thread) => PushNotification::new(
                format!("{}: New Thread", pr.title).as_str(),
                format!("{} has created a new thread.", thread.author().display_name).as_str(),
                Some(url),
                None,
            ),
            PRChangeEvent::ThreadUpdated(thread) => PushNotification::new(
                format!("{}: Thread Updated", pr.title).as_str(),
                format!(
                    "{} has replied in a thread you are a part of.",
                    thread.most_recent_comment().author.display_name
                )
                .as_str(),
                Some(url),
                None,
            ),
            PRChangeEvent::CommentMentioned(comment, _mentioned_email) => PushNotification::new(
                format!("{}: You were mentioned", pr.title).as_str(),
                format!(
                    "{} mentioned you in a comment.",
                    comment.author.display_name
                )
                .as_str(),
                Some(url),
                None,
            ),
        }
    }

    fn is_ignored_message_content(content: &str) -> bool {
        // Slash commands are ignored
        content.starts_with("/")
    }
}
