use axum::{http::StatusCode, routing::post, Json, Router};
use serde::Deserialize;
use tracing::instrument;
use web_push::{
    ContentEncoding, IsahcWebPushClient, SubscriptionInfo, VapidSignatureBuilder, WebPushClient,
    WebPushMessageBuilder, URL_SAFE_NO_PAD,
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

#[instrument(name = "subscribe")]
async fn subscribe(Json(body): Json<PushSubscription>) -> Result<StatusCode, (StatusCode, String)> {
    tracing::info!("Received subscription: {:?}", body);

    let sub_info: SubscriptionInfo = body.into();
    let sig_builder = VapidSignatureBuilder::from_base64(
        "KaRfTAcDs9ztATKecCL_mBJYdO57X3NvzgWnBNTBQ4c",
        URL_SAFE_NO_PAD,
        &sub_info,
    )
    .expect("Could not build VAPID signature builder")
    .build()
    .expect("Could not build VAPID signature");

    let mut builder = WebPushMessageBuilder::new(&sub_info);
    let content = "Encrypted payload to be sent in the notification".as_bytes();
    builder.set_payload(ContentEncoding::Aes128Gcm, content);
    builder.set_vapid_signature(sig_builder);

    let client = IsahcWebPushClient::new().expect("Could not create web push client");

    //Finally, send the notification!
    client
        .send(builder.build().expect("builder.build()?"))
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

#[derive(Debug, Deserialize)]
struct SubscriptionKeys {
    p256dh: String,
    auth: String,
}
