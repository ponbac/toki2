use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use web_push::{
    ContentEncoding, SubscriptionInfo, VapidSignatureBuilder, WebPushMessageBuilder, URL_SAFE,
};

use crate::app_state::AppState;

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
            keys: web_push::SubscriptionKeys {
                p256dh: subscription.keys.p256dh,
                auth: subscription.keys.auth,
            },
        }
    }
}

#[derive(Debug, Serialize)]
struct PushNotification {
    title: String,
    body: String,
    icon: Option<String>,
}

#[instrument(name = "subscribe", skip(app_state))]
async fn subscribe(
    State(app_state): State<AppState>,
    Json(body): Json<PushSubscription>,
) -> Result<StatusCode, (StatusCode, String)> {
    tracing::info!("Received subscription: {:?}", body);

    let sub_info: SubscriptionInfo = body.into();
    let sig_builder = VapidSignatureBuilder::from_base64(
        "KaRfTAcDs9ztATKecCL_mBJYdO57X3NvzgWnBNTBQ4c",
        URL_SAFE,
        &sub_info,
    )
    .expect("Could not build VAPID signature builder")
    .build()
    .expect("Could not build VAPID signature");

    let content = PushNotification {
        title: "Hello, World!".to_string(),
        body: "This is a test notification".to_string(),
        icon: None,
    };
    let serialized_content = serde_json::to_string(&content)
        .expect("Could not serialize content")
        .into_bytes();

    let mut builder = WebPushMessageBuilder::new(&sub_info);
    builder.set_payload(ContentEncoding::Aes128Gcm, &serialized_content);
    builder.set_vapid_signature(sig_builder);
    let message = builder.build().expect("Could not build web push message");

    app_state.push_notification(message).await.map_err(|e| {
        tracing::error!("Failed to send notification: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to send notification".to_string(),
        )
    })?;

    Ok(StatusCode::OK)
}

#[derive(Debug, Deserialize)]
struct SubscriptionKeys {
    p256dh: String,
    auth: String,
}
