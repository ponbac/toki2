use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PushSubscription {
    pub id: i32,
    pub user_id: i32,
    pub device: String,
    pub endpoint: String,
    pub auth: String,
    pub p256dh: String,
    pub created_at: Option<time::PrimitiveDateTime>,
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
