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

    fn to_push_notification(&self, pr: &az_devops::PullRequest, url: &str) -> PushNotification {
        match self {
            PRChangeEvent::PullRequestClosed => PushNotification::new(
                format!("{}: Pull Request Closed", pr.title).as_str(),
                format!("!{} has been closed", pr.id).as_str(),
                Some(url),
                None,
            ),
            PRChangeEvent::ThreadAdded(thread) => PushNotification::new(
                format!("{}: New Thread", pr.title).as_str(),
                format!("{} has created a new thread", thread.author().display_name).as_str(),
                Some(url),
                None,
            ),
            PRChangeEvent::ThreadUpdated(thread) => PushNotification::new(
                format!("{}: Thread Updated", pr.title).as_str(),
                format!(
                    "{} has replied in a thread you are a part of",
                    thread.most_recent_comment().author.display_name
                )
                .as_str(),
                Some(url),
                None,
            ),
        }
    }
}
