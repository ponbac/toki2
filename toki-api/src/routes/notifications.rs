use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde::Deserialize;
use tracing::instrument;
use web_push::SubscriptionKeys;

use crate::{app_state::AppState, domain::PushNotification};

pub fn router() -> Router<AppState> {
    Router::new().route("/subscribe", post(subscribe))
}

#[derive(Debug, Deserialize)]
struct PushSubscription {
    endpoint: String,
    #[serde(rename = "expirationTime")]
    expiration_time: Option<String>,
    keys: SubscriptionKeys,
}

impl From<PushSubscription> for web_push::SubscriptionInfo {
    fn from(subscription: PushSubscription) -> Self {
        web_push::SubscriptionInfo {
            endpoint: subscription.endpoint,
            keys: SubscriptionKeys {
                p256dh: subscription.keys.p256dh,
                auth: subscription.keys.auth,
            },
        }
    }
}

#[instrument(name = "subscribe", skip(app_state))]
async fn subscribe(
    State(app_state): State<AppState>,
    Json(body): Json<PushSubscription>,
) -> Result<StatusCode, (StatusCode, String)> {
    let content = PushNotification::new("Hello, World!", "This is a test notification", None);
    let push_message = content.to_web_push_message(body.into()).map_err(|e| {
        tracing::error!("Failed to create push message: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create push message".to_string(),
        )
    })?;

    app_state
        .push_notification(push_message)
        .await
        .map_err(|e| {
            tracing::error!("Failed to send notification: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to send notification".to_string(),
            )
        })?;

    Ok(StatusCode::OK)
}
