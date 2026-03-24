use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use axum_login::permission_required;
use kleer::{KleerClient, KleerCredentials, KleerError};
use serde::{Deserialize, Serialize};

use crate::{
    app_state::AppState,
    auth::AuthBackend,
    domain::{
        models::{
            NewTimeTrackingProviderUser, NewTimeTrackingUserLink, TimeTrackingProviderUser,
            TimeTrackingUserLink, UserId, KLEER_TIME_TRACKING_PROVIDER,
        },
        ports::outbound::TimeTrackingUserLinkRepository,
        Role, User,
    },
    repositories::{TimeTrackingUserLinkRepositoryImpl, UserRepository},
    routes::ApiError,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/kleer-users", get(list_mappings))
        .route("/kleer-users/import", post(import_kleer_users))
        .route("/user-links", put(upsert_user_link))
        .route("/user-links/:user_id", delete(deactivate_user_link))
        .route_layer(permission_required!(AuthBackend, Role::Admin))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminTokiUserResponse {
    id: i32,
    email: String,
    full_name: String,
}

impl From<User> for AdminTokiUserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id.as_i32(),
            email: user.email,
            full_name: user.full_name,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminKleerUserResponse {
    provider_user_id: String,
    foreign_id: Option<String>,
    internal_id: Option<String>,
    name: String,
    email: Option<String>,
    active: bool,
    mapped_user_id: Option<i32>,
    mapped_user_email: Option<String>,
    mapped_user_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    last_synced_at: time::OffsetDateTime,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminUserLinkResponse {
    id: i32,
    user_id: i32,
    provider_user_id: String,
    provider_user_email: Option<String>,
    provider_user_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: time::OffsetDateTime,
}

impl From<TimeTrackingUserLink> for AdminUserLinkResponse {
    fn from(link: TimeTrackingUserLink) -> Self {
        Self {
            id: link.id,
            user_id: link.user_id.as_i32(),
            provider_user_id: link.provider_user_id,
            provider_user_email: link.provider_user_email,
            provider_user_name: link.provider_user_name,
            updated_at: link.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminMappingStateResponse {
    users: Vec<AdminTokiUserResponse>,
    kleer_users: Vec<AdminKleerUserResponse>,
    links: Vec<AdminUserLinkResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpsertUserLinkPayload {
    user_id: i32,
    provider_user_id: String,
}

async fn import_kleer_users(
    State(app_state): State<AppState>,
) -> Result<Json<AdminMappingStateResponse>, ApiError> {
    let credentials = kleer_credentials(&app_state)?;
    let company_id = credentials.company_id.clone();
    let users = KleerClient::new(credentials)
        .map_err(kleer_configuration_error)?
        .list_users()
        .await
        .map_err(kleer_request_error)?;

    let imported_users: Vec<_> = users
        .users
        .into_iter()
        .map(|user| NewTimeTrackingProviderUser {
            provider: KLEER_TIME_TRACKING_PROVIDER.to_string(),
            provider_company_id: company_id.clone(),
            provider_user_id: user.id.to_string(),
            foreign_id: user.foreign_id,
            internal_id: user.internal_id,
            name: user.name,
            email: user.email,
            active: user.active,
        })
        .collect();

    let repo = mapping_repo(&app_state);
    repo.upsert_provider_users(&imported_users).await?;

    mapping_state(&app_state).await.map(Json)
}

async fn list_mappings(
    State(app_state): State<AppState>,
) -> Result<Json<AdminMappingStateResponse>, ApiError> {
    mapping_state(&app_state).await.map(Json)
}

async fn upsert_user_link(
    State(app_state): State<AppState>,
    Json(payload): Json<UpsertUserLinkPayload>,
) -> Result<Json<AdminUserLinkResponse>, ApiError> {
    let credentials = kleer_credentials(&app_state)?;
    let repo = mapping_repo(&app_state);
    let provider_user = repo
        .get_provider_user(
            KLEER_TIME_TRACKING_PROVIDER,
            &credentials.company_id,
            &payload.provider_user_id,
        )
        .await?
        .ok_or_else(|| ApiError::not_found("Kleer user has not been imported"))?;

    if !provider_user.active {
        return Err(ApiError::bad_request("Cannot map an inactive Kleer user"));
    }

    let link = repo
        .upsert_active_link(&NewTimeTrackingUserLink {
            user_id: UserId::from(payload.user_id),
            provider: KLEER_TIME_TRACKING_PROVIDER.to_string(),
            provider_company_id: credentials.company_id,
            provider_user_id: provider_user.provider_user_id,
            provider_user_email: provider_user.email,
            provider_user_name: Some(provider_user.name),
        })
        .await?;

    Ok(Json(link.into()))
}

async fn deactivate_user_link(
    Path(user_id): Path<i32>,
    State(app_state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    let repo = mapping_repo(&app_state);
    repo.deactivate_active_link(&UserId::from(user_id), KLEER_TIME_TRACKING_PROVIDER)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn mapping_state(app_state: &AppState) -> Result<AdminMappingStateResponse, ApiError> {
    let credentials = kleer_credentials(app_state)?;
    let repo = mapping_repo(app_state);

    let users = app_state.user_repo.get_users().await?;
    let users_by_id: HashMap<_, _> = users
        .iter()
        .map(|user| {
            (
                user.id.as_i32(),
                (user.email.clone(), user.full_name.clone()),
            )
        })
        .collect();
    let provider_users = repo
        .list_provider_users(KLEER_TIME_TRACKING_PROVIDER, &credentials.company_id)
        .await?;
    let links = repo
        .list_active_links(KLEER_TIME_TRACKING_PROVIDER, &credentials.company_id)
        .await?;

    let links_by_provider_user: HashMap<_, _> = links
        .iter()
        .map(|link| (link.provider_user_id.as_str(), link))
        .collect();

    let kleer_users = provider_users
        .into_iter()
        .map(|provider_user| {
            let mapped_user_id = links_by_provider_user
                .get(provider_user.provider_user_id.as_str())
                .map(|link| link.user_id.as_i32());
            let (mapped_user_email, mapped_user_name) = mapped_user_id
                .and_then(|user_id| users_by_id.get(&user_id))
                .cloned()
                .map_or((None, None), |(email, name)| (Some(email), Some(name)));

            provider_user_response(
                provider_user,
                mapped_user_id,
                mapped_user_email,
                mapped_user_name,
            )
        })
        .collect();

    Ok(AdminMappingStateResponse {
        users: users.into_iter().map(Into::into).collect(),
        kleer_users,
        links: links.into_iter().map(Into::into).collect(),
    })
}

fn provider_user_response(
    user: TimeTrackingProviderUser,
    mapped_user_id: Option<i32>,
    mapped_user_email: Option<String>,
    mapped_user_name: Option<String>,
) -> AdminKleerUserResponse {
    AdminKleerUserResponse {
        provider_user_id: user.provider_user_id,
        foreign_id: user.foreign_id,
        internal_id: user.internal_id,
        name: user.name,
        email: user.email,
        active: user.active,
        mapped_user_id,
        mapped_user_email,
        mapped_user_name,
        last_synced_at: user.last_synced_at,
    }
}

fn mapping_repo(app_state: &AppState) -> TimeTrackingUserLinkRepositoryImpl {
    TimeTrackingUserLinkRepositoryImpl::new((*app_state.db_pool).clone())
}

fn kleer_credentials(app_state: &AppState) -> Result<KleerCredentials, ApiError> {
    app_state
        .kleer_settings
        .credentials()
        .map_err(service_configuration_error)
}

fn service_configuration_error(error: String) -> ApiError {
    ApiError::new(StatusCode::SERVICE_UNAVAILABLE, error)
}

fn kleer_configuration_error(error: KleerError) -> ApiError {
    ApiError::new(StatusCode::SERVICE_UNAVAILABLE, error.to_string())
}

fn kleer_request_error(error: KleerError) -> ApiError {
    match error {
        KleerError::Unauthorized | KleerError::Forbidden => {
            ApiError::new(StatusCode::SERVICE_UNAVAILABLE, error.to_string())
        }
        KleerError::NotFound => ApiError::not_found(error.to_string()),
        KleerError::Response { status, body: _ } => {
            tracing::warn!("Kleer admin request failed: status={status}");
            ApiError::internal(format!("Kleer returned {status}"))
        }
        KleerError::Deserialize { message, body: _ } => {
            tracing::warn!("Failed to deserialize Kleer admin response: {message}");
            ApiError::internal("failed to process Kleer response")
        }
        _ => ApiError::internal(error.to_string()),
    }
}
