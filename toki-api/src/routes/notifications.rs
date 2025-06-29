use crate::domain::DbNotificationType;
use crate::domain::PushSubscriptionInfo;
use crate::repositories::NotificationRepository;
use crate::repositories::PushSubscriptionRepository;
use crate::utils::client_hints::ClientHints;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use tracing::instrument;

use crate::{
    app_state::AppState,
    auth::AuthSession,
    domain::{Notification, NotificationRule, PrNotificationException, PushNotification},
    repositories::NewPushSubscription,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/subscribe", post(subscribe))
        .route("/is-subscribed", post(is_subscribed))
        .route("/push-subscriptions", get(get_push_subscriptions))
        .route("/push-subscriptions/:id", delete(delete_push_subscription))
        .route("/test-push", post(test_push))
        .route("/", get(get_notifications))
        .route("/:id/view", post(mark_notification_viewed))
        .route("/view-all", post(mark_all_notifications_viewed))
        .route("/:id", delete(delete_notification))
        .route("/preferences/:repository_id", get(get_preferences))
        .route("/preferences/:repository_id", post(update_preferences))
        .route(
            "/repositories/:repository_id/pull-requests/:pull_request_id/exceptions",
            get(get_pr_exceptions),
        )
        .route(
            "/repositories/:repository_id/pull-requests/:pull_request_id/exceptions",
            post(set_pr_exception),
        )
        .route(
            "/repositories/:repository_id/pull-requests/:pull_request_id/exceptions/:notification_type",
            delete(remove_pr_exception),
        )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribePayload {
    subscription: web_push::SubscriptionInfo,
    device_name: Option<String>,
}

#[instrument(name = "subscribe", skip(auth_session, app_state))]
async fn subscribe(
    auth_session: AuthSession,
    client_hints: ClientHints,
    State(app_state): State<AppState>,
    Json(body): Json<SubscribePayload>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user_id = auth_session.user.expect("user not found").id;
    let push_subscription_repo = app_state.push_subscriptions_repo.clone();

    let new_push_subscription = NewPushSubscription {
        user_id,
        device: body
            .device_name
            .unwrap_or_else(|| client_hints.identifier()),
        endpoint: body.subscription.endpoint,
        auth: body.subscription.keys.auth,
        p256dh: body.subscription.keys.p256dh,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IsSubscribedPayload {
    device_name: Option<String>,
}

#[instrument(name = "is_subscribed", skip(app_state, user))]
async fn is_subscribed(
    AuthSession { user, .. }: AuthSession,
    client_hints: ClientHints,
    State(app_state): State<AppState>,
    Json(body): Json<IsSubscribedPayload>,
) -> Result<Json<bool>, (StatusCode, String)> {
    let user = user.expect("user not found");
    let push_subscription_repo = app_state.push_subscriptions_repo.clone();

    let user_subscriptions = push_subscription_repo
        .get_user_push_subscriptions(&user.id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get user push subscriptions: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get user push subscriptions".to_string(),
            )
        })?;

    let device_name = body
        .device_name
        .unwrap_or_else(|| client_hints.identifier());
    let is_subscribed_with_device_name = user_subscriptions
        .iter()
        .any(|sub| sub.device == device_name);

    Ok(Json(is_subscribed_with_device_name))
}

#[instrument(name = "get_push_subscriptions", skip(app_state, user))]
async fn get_push_subscriptions(
    AuthSession { user, .. }: AuthSession,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<PushSubscriptionInfo>>, (StatusCode, String)> {
    let user = user.expect("user not found");
    let push_subscription_repo = app_state.push_subscriptions_repo.clone();

    let subscriptions = push_subscription_repo
        .get_user_push_subscriptions(&user.id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get push subscriptions: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get push subscriptions".to_string(),
            )
        })?;

    Ok(Json(
        subscriptions
            .into_iter()
            .map(PushSubscriptionInfo::from)
            .collect(),
    ))
}

#[instrument(name = "delete_push_subscription", skip(app_state))]
async fn delete_push_subscription(
    AuthSession { user, .. }: AuthSession,
    State(app_state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user = user.expect("user not found");
    let push_subscription_repo = app_state.push_subscriptions_repo.clone();

    let user_push_subscriptions = push_subscription_repo
        .get_user_push_subscriptions(&user.id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get user push subscriptions: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get user push subscriptions".to_string(),
            )
        })?;

    if !user_push_subscriptions.iter().any(|sub| sub.id == id) {
        return Err((
            StatusCode::NOT_FOUND,
            "Push subscription not found".to_string(),
        ));
    }

    push_subscription_repo
        .delete_push_subscription(&id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete push subscription: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete push subscription".to_string(),
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

    let content = PushNotification::new(
        "Hello, World!",
        "This is a test notification",
        Some("https://ponbac.xyz"),
        None,
    );
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationParams {
    include_viewed: Option<bool>,
    max_age_days: Option<i32>,
}

async fn get_notifications(
    AuthSession { user, .. }: AuthSession,
    Query(params): Query<NotificationParams>,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<Notification>>, (StatusCode, String)> {
    let user = user.expect("user not found");
    let notification_repo = app_state.notification_repo.clone();

    let notifications = notification_repo
        .get_user_notifications(
            user.id,
            params.include_viewed.unwrap_or(false),
            params.max_age_days.unwrap_or(30),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to get user notifications: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get user notifications".to_string(),
            )
        })?;

    Ok(Json(notifications))
}

async fn mark_notification_viewed(
    AuthSession { user, .. }: AuthSession,
    Path(id): Path<i32>,
    State(app_state): State<AppState>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user = user.expect("user not found");
    let notification_repo = app_state.notification_repo.clone();

    notification_repo
        .mark_as_viewed(id, user.id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to mark notification as viewed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to mark notification as viewed".to_string(),
            )
        })?;

    Ok(StatusCode::OK)
}

async fn mark_all_notifications_viewed(
    AuthSession { user, .. }: AuthSession,
    State(app_state): State<AppState>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user = user.expect("user not found");
    let notification_repo = app_state.notification_repo.clone();

    notification_repo
        .mark_all_notifications_viewed(user.id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to mark all notifications as viewed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to mark all notifications as viewed".to_string(),
            )
        })?;

    Ok(StatusCode::OK)
}

async fn delete_notification(
    AuthSession { user, .. }: AuthSession,
    Path(id): Path<i32>,
    State(app_state): State<AppState>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user = user.expect("user not found");
    let notification_repo = app_state.notification_repo.clone();

    notification_repo
        .delete_notification(id, user.id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete notification: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete notification".to_string(),
            )
        })?;

    Ok(StatusCode::OK)
}

async fn get_preferences(
    AuthSession { user, .. }: AuthSession,
    State(app_state): State<AppState>,
    Path(repository_id): Path<i32>,
) -> Result<Json<Vec<NotificationRule>>, (StatusCode, String)> {
    let user = user.expect("user not found");
    let notification_repo = app_state.notification_repo.clone();

    let rules = notification_repo
        .get_repository_rules(user.id, repository_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get notification rules: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get notification rules".to_string(),
            )
        })?;

    Ok(Json(rules))
}

async fn update_preferences(
    AuthSession { user, .. }: AuthSession,
    State(app_state): State<AppState>,
    Path(repository_id): Path<i32>,
    Json(rule): Json<NotificationRule>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user = user.expect("user not found");

    // Validate that the rule belongs to the authenticated user
    if rule.user_id != user.id {
        return Err((
            StatusCode::FORBIDDEN,
            "Cannot modify rules for other users".to_string(),
        ));
    }

    // Validate that the repository_id in the path matches the rule
    if rule.repository_id != repository_id {
        return Err((
            StatusCode::BAD_REQUEST,
            "Repository ID mismatch".to_string(),
        ));
    }

    let notification_repo = app_state.notification_repo.clone();

    notification_repo.update_rule(&rule).await.map_err(|e| {
        tracing::error!("Failed to update notification rule: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to update notification rule".to_string(),
        )
    })?;

    Ok(StatusCode::OK)
}

#[derive(Debug, Deserialize)]
struct PrExceptionPath {
    repository_id: i32,
    pull_request_id: i32,
}

#[derive(Debug, Deserialize)]
struct RemovePrExceptionPath {
    repository_id: i32,
    pull_request_id: i32,
    notification_type: DbNotificationType,
}

async fn get_pr_exceptions(
    AuthSession { user, .. }: AuthSession,
    Path(params): Path<PrExceptionPath>,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<PrNotificationException>>, (StatusCode, String)> {
    let user = user.expect("user not found");
    let notification_repo = app_state.notification_repo.clone();

    let exceptions = notification_repo
        .get_pr_exceptions(user.id, params.repository_id, params.pull_request_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get PR exceptions: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get PR exceptions".to_string(),
            )
        })?;

    Ok(Json(exceptions))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateExceptionPayload {
    pub repository_id: i32,
    pub notification_type: DbNotificationType,
    pub enabled: bool,
}

async fn set_pr_exception(
    AuthSession { user, .. }: AuthSession,
    Path(params): Path<PrExceptionPath>,
    State(app_state): State<AppState>,
    Json(payload): Json<UpdateExceptionPayload>,
) -> Result<Json<PrNotificationException>, (StatusCode, String)> {
    let user = user.expect("user not found");

    let notification_repo = app_state.notification_repo.clone();
    let existing_exceptions = notification_repo
        .get_pr_exceptions(user.id, payload.repository_id, params.pull_request_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get PR exceptions: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get PR exceptions".to_string(),
            )
        })?;

    let existing_exception = existing_exceptions
        .iter()
        .find(|e| e.notification_type == payload.notification_type)
        .cloned();

    // Validate that the exception belongs to the authenticated user
    if let Some(existing_exception) = &existing_exception {
        if existing_exception.user_id != user.id {
            return Err((
                StatusCode::FORBIDDEN,
                "Cannot modify exceptions for other users".to_string(),
            ));
        }
    }

    let new_exception = PrNotificationException {
        id: 0, // DB will assign actual ID, fix this later...
        user_id: user.id,
        repository_id: payload.repository_id,
        pull_request_id: params.pull_request_id,
        notification_type: payload.notification_type,
        enabled: payload.enabled,
    };

    let updated_exception = notification_repo
        .set_pr_exception(&new_exception)
        .await
        .map_err(|e| {
            tracing::error!("Failed to set PR exception: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to set PR exception".to_string(),
            )
        })?;

    Ok(Json(updated_exception))
}

async fn remove_pr_exception(
    AuthSession { user, .. }: AuthSession,
    Path(params): Path<RemovePrExceptionPath>,
    State(app_state): State<AppState>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user = user.expect("user not found");
    let notification_repo = app_state.notification_repo.clone();

    notification_repo
        .remove_pr_exception(
            user.id,
            params.repository_id,
            params.pull_request_id,
            params.notification_type,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to remove PR exception: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to remove PR exception".to_string(),
            )
        })?;

    Ok(StatusCode::OK)
}
