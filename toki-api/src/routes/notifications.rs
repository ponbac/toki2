use crate::repositories::PushSubscriptionRepository;
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use tracing::instrument;

use crate::{
    app_state::AppState, auth::AuthSession, domain::PushNotification,
    repositories::NewPushSubscription,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/subscribe", post(subscribe))
        .route("/test-push", post(test_push))
}

#[instrument(name = "subscribe", skip(auth_session, app_state))]
async fn subscribe(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<web_push::SubscriptionInfo>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user_id = auth_session.user.expect("user not found").id;
    let push_subscription_repo = app_state.push_subscriptions_repo.clone();

    let new_push_subscription = NewPushSubscription {
        user_id,
        device: "NOT IMPLEMENTED".to_string(),
        endpoint: body.endpoint,
        auth: body.keys.auth,
        p256dh: body.keys.p256dh,
    };

    push_subscription_repo
        .upsert_push_subscription(new_push_subscription)
        .await
        .map_err(|e| {
            tracing::error!("Failed to upsert push subscription: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to upsert push subscription".to_string(),
            )
        })?;

    Ok(StatusCode::OK)
}

#[instrument(name = "test_push", skip(app_state))]
async fn test_push(State(app_state): State<AppState>) -> Result<StatusCode, (StatusCode, String)> {
    let push_subscription_repo = app_state.push_subscriptions_repo.clone();
    let subscribers = push_subscription_repo
        .get_push_subscriptions()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get push subscriptions: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get push subscriptions".to_string(),
            )
        })?;

    let content = PushNotification::new("Hello, World!", "This is a test notification", None);
    for subscriber in subscribers {
        let push_message = content
            .to_web_push_message(&subscriber.as_subscription_info())
            .map_err(|e| {
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
    }

    Ok(StatusCode::OK)
}
