use std::fmt;

use super::{PushNotification, PushSubscription};

#[derive(Debug, Clone, PartialEq)]
pub enum PRChangeEvent {
    PullRequestClosed,
    ThreadAdded(az_devops::Thread),
    ThreadUpdated(az_devops::Thread),
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
        }
    }
}

impl PRChangeEvent {
    pub fn applies_to(&self, email: &str, pr_author: &str) -> bool {
        match self {
            PRChangeEvent::PullRequestClosed => true,
            PRChangeEvent::ThreadAdded(thread) => {
                thread.author().unique_name != email && pr_author == email
            }
            PRChangeEvent::ThreadUpdated(thread) => {
                let most_recent_author = &thread.most_recent_comment().author;

                // Don't notify if you're the one who just commented
                if most_recent_author.unique_name == email {
                    return false;
                }

                // Notify if you're the PR author
                if email == pr_author {
                    return true;
                }

                // Or if you've participated in this thread before
                thread
                    .comments
                    .iter()
                    .any(|comment| comment.author.unique_name == email)
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
        }
    }
}
