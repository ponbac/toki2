use crate::domain::DbNotificationType;
use crate::domain::PushSubscriptionInfo;
use crate::repositories::NotificationRepository;
use crate::repositories::PushSubscriptionRepository;
use crate::utils::client_hints::ClientHints;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post},
    Router,
};
use serde::Deserialize;
use std::collections::HashMap;
use tracing::instrument;

use crate::{
    app_state::AppState,
    auth::AuthUser,
    domain::{Notification, NotificationRule, PrNotificationException, PushNotification},
    repositories::NewPushSubscription,
};
use strum::IntoEnumIterator;

use super::ApiError;

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

#[instrument(name = "subscribe")]
async fn subscribe(
    user: AuthUser,
    client_hints: ClientHints,
    State(app_state): State<AppState>,
    Json(body): Json<SubscribePayload>,
) -> Result<StatusCode, ApiError> {
    let push_subscription_repo = app_state.push_subscriptions_repo.clone();

    let new_push_subscription = NewPushSubscription {
        user_id: user.id.as_i32(),
        device: body
            .device_name
            .unwrap_or_else(|| client_hints.identifier()),
        endpoint: body.subscription.endpoint,
        auth: body.subscription.keys.auth,
        p256dh: body.subscription.keys.p256dh,
    };

    push_subscription_repo
        .upsert_push_subscription(new_push_subscription)
        .await?;

    Ok(StatusCode::OK)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IsSubscribedPayload {
    device_name: Option<String>,
}

#[instrument(name = "is_subscribed")]
async fn is_subscribed(
    user: AuthUser,
    client_hints: ClientHints,
    State(app_state): State<AppState>,
    Json(body): Json<IsSubscribedPayload>,
) -> Result<Json<bool>, ApiError> {
    let push_subscription_repo = app_state.push_subscriptions_repo.clone();

    let user_subscriptions = push_subscription_repo
        .get_user_push_subscriptions(user.id.as_ref())
        .await?;

    let device_name = body
        .device_name
        .unwrap_or_else(|| client_hints.identifier());
    let is_subscribed_with_device_name = user_subscriptions
        .iter()
        .any(|sub| sub.device == device_name);

    Ok(Json(is_subscribed_with_device_name))
}

#[instrument(name = "get_push_subscriptions")]
async fn get_push_subscriptions(
    user: AuthUser,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<PushSubscriptionInfo>>, ApiError> {
    let push_subscription_repo = app_state.push_subscriptions_repo.clone();

    let subscriptions = push_subscription_repo
        .get_user_push_subscriptions(user.id.as_ref())
        .await?;

    Ok(Json(
        subscriptions
            .into_iter()
            .map(PushSubscriptionInfo::from)
            .collect(),
    ))
}

#[instrument(name = "delete_push_subscription")]
async fn delete_push_subscription(
    user: AuthUser,
    State(app_state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<StatusCode, ApiError> {
    let push_subscription_repo = app_state.push_subscriptions_repo.clone();

    let user_push_subscriptions = push_subscription_repo
        .get_user_push_subscriptions(user.id.as_ref())
        .await?;

    if !user_push_subscriptions.iter().any(|sub| sub.id == id) {
        return Err(ApiError::not_found("Push subscription not found"));
    }

    push_subscription_repo.delete_push_subscription(&id).await?;

    Ok(StatusCode::OK)
}

#[instrument(name = "test_push")]
async fn test_push(State(app_state): State<AppState>) -> Result<StatusCode, ApiError> {
    let push_subscription_repo = app_state.push_subscriptions_repo.clone();
    let subscribers = push_subscription_repo.get_push_subscriptions().await?;

    let content = PushNotification::new(
        "Hello, World!",
        "This is a test notification",
        Some("https://toki.spinit.se"),
        None,
    );
    for subscriber in subscribers {
        let push_message = content
            .to_web_push_message(&subscriber.as_subscription_info())
            .map_err(|e| {
                tracing::error!("Failed to create push message: {:?}", e);
                ApiError::internal("Failed to create push message")
            })?;

        app_state.push_notification(push_message).await?;
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
    user: AuthUser,
    Query(params): Query<NotificationParams>,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<Notification>>, ApiError> {
    let notification_repo = app_state.notification_repo.clone();

    let notifications = notification_repo
        .get_user_notifications(
            user.id.as_i32(),
            params.include_viewed.unwrap_or(false),
            params.max_age_days.unwrap_or(30),
        )
        .await?;

    Ok(Json(notifications))
}

async fn mark_notification_viewed(
    user: AuthUser,
    Path(id): Path<i32>,
    State(app_state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    let notification_repo = app_state.notification_repo.clone();

    notification_repo
        .mark_as_viewed(id, user.id.as_i32())
        .await?;

    Ok(StatusCode::OK)
}

async fn mark_all_notifications_viewed(
    user: AuthUser,
    State(app_state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    let notification_repo = app_state.notification_repo.clone();

    notification_repo
        .mark_all_notifications_viewed(user.id.as_i32())
        .await?;

    Ok(StatusCode::OK)
}

async fn delete_notification(
    user: AuthUser,
    Path(id): Path<i32>,
    State(app_state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    let notification_repo = app_state.notification_repo.clone();

    notification_repo
        .delete_notification(id, user.id.as_i32())
        .await?;

    Ok(StatusCode::OK)
}

async fn get_preferences(
    user: AuthUser,
    State(app_state): State<AppState>,
    Path(repository_id): Path<i32>,
) -> Result<Json<Vec<NotificationRule>>, ApiError> {
    let notification_repo = app_state.notification_repo.clone();

    let existing_rules = notification_repo
        .get_repository_rules(user.id.as_i32(), repository_id)
        .await?;

    let rules_map: HashMap<DbNotificationType, NotificationRule> = existing_rules
        .into_iter()
        .map(|rule| (rule.notification_type, rule))
        .collect();

    let all_rules = DbNotificationType::iter()
        .map(|notification_type| {
            rules_map
                .get(&notification_type)
                .cloned()
                .unwrap_or_else(|| NotificationRule {
                    id: 0, // Indicates a default, non-DB rule
                    user_id: user.id.as_i32(),
                    repository_id,
                    notification_type,
                    enabled: notification_type.default_enabled(),
                    push_enabled: false,
                })
        })
        .collect();

    Ok(Json(all_rules))
}

async fn update_preferences(
    user: AuthUser,
    State(app_state): State<AppState>,
    Path(repository_id): Path<i32>,
    Json(rule): Json<NotificationRule>,
) -> Result<StatusCode, ApiError> {
    // Validate that the rule belongs to the authenticated user
    if rule.user_id != user.id.as_i32() {
        return Err(ApiError::forbidden("Cannot modify rules for other users"));
    }

    // Validate that the repository_id in the path matches the rule
    if rule.repository_id != repository_id {
        return Err(ApiError::bad_request("Repository ID mismatch"));
    }

    let notification_repo = app_state.notification_repo.clone();

    notification_repo.update_rule(&rule).await?;

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
    user: AuthUser,
    Path(params): Path<PrExceptionPath>,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<PrNotificationException>>, ApiError> {
    let notification_repo = app_state.notification_repo.clone();

    let exceptions = notification_repo
        .get_pr_exceptions(
            user.id.as_i32(),
            params.repository_id,
            params.pull_request_id,
        )
        .await?;

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
    user: AuthUser,
    Path(params): Path<PrExceptionPath>,
    State(app_state): State<AppState>,
    Json(payload): Json<UpdateExceptionPayload>,
) -> Result<Json<PrNotificationException>, ApiError> {
    let notification_repo = app_state.notification_repo.clone();
    let existing_exceptions = notification_repo
        .get_pr_exceptions(
            user.id.as_i32(),
            payload.repository_id,
            params.pull_request_id,
        )
        .await?;

    let existing_exception = existing_exceptions
        .iter()
        .find(|e| e.notification_type == payload.notification_type)
        .cloned();

    // Validate that the exception belongs to the authenticated user
    if let Some(existing_exception) = &existing_exception {
        if existing_exception.user_id != user.id.as_i32() {
            return Err(ApiError::forbidden(
                "Cannot modify exceptions for other users",
            ));
        }
    }

    let new_exception = PrNotificationException {
        id: 0, // DB will assign actual ID, fix this later...
        user_id: user.id.as_i32(),
        repository_id: payload.repository_id,
        pull_request_id: params.pull_request_id,
        notification_type: payload.notification_type,
        enabled: payload.enabled,
    };

    let updated_exception = notification_repo.set_pr_exception(&new_exception).await?;

    Ok(Json(updated_exception))
}

async fn remove_pr_exception(
    user: AuthUser,
    Path(params): Path<RemovePrExceptionPath>,
    State(app_state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    let notification_repo = app_state.notification_repo.clone();

    notification_repo
        .remove_pr_exception(
            user.id.as_i32(),
            params.repository_id,
            params.pull_request_id,
            params.notification_type,
        )
        .await?;

    Ok(StatusCode::OK)
}
