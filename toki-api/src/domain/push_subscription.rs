use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PushSubscription {
    pub id: i32,
    pub user_id: i32,
    pub device: String,
    pub endpoint: String,
    pub auth: String,
    pub p256dh: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PushSubscriptionInfo {
    pub id: i32,
    pub device: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
}

impl PushSubscription {
    pub fn as_subscription_info(&self) -> web_push::SubscriptionInfo {
        web_push::SubscriptionInfo {
            endpoint: self.endpoint.clone(),
            keys: web_push::SubscriptionKeys {
                auth: self.auth.clone(),
                p256dh: self.p256dh.clone(),
            },
        }
    }
}

impl From<PushSubscription> for PushSubscriptionInfo {
    fn from(subscription: PushSubscription) -> Self {
        PushSubscriptionInfo {
            id: subscription.id,
            device: subscription.device,
            created_at: subscription.created_at,
        }
    }
}
