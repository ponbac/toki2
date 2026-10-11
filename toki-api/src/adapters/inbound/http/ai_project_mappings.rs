//! Team-wide mappings from AI usage project keys to time-tracking projects.
//!
//! Handlers only extract the caller; the mapping service decides what they may
//! do. Keys travel in JSON bodies rather than the path because they contain
//! slashes and other free text.

use std::sync::Arc;

use axum::{
    extract::{FromRef, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{
    adapters::inbound::http::ProjectResponse,
    auth::AuthUser,
    domain::{
        models::{
            AiProjectMappingActor, AiProjectMappingEditor, AiProjectMappingEntry, ProjectId,
            UnmappedAiProjectKey,
        },
        ports::inbound::AiProjectMappingService,
        Role,
    },
    routes::ApiError,
};

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    Arc<dyn AiProjectMappingService>: FromRef<S>,
{
    Router::new()
        .route(
            "/",
            get(list_mappings).put(set_mapping).delete(delete_mapping),
        )
        .route("/unmapped-keys", get(list_unmapped_keys))
        .route("/projects", get(list_projects))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetMappingRequest {
    project_key: String,
    project_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteMappingRequest {
    project_key: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiProjectMappingResponse {
    project_key: String,
    project: ProjectResponse,
    /// The project belongs to another provider or company than the configured
    /// one, so usage of the key is unassigned until an admin maps it again.
    stale: bool,
    /// Whether anyone's stored usage has the key.
    in_use: bool,
    /// Null once that user is deleted.
    created_by: Option<AiProjectMappingEditorResponse>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    /// Null once that user is deleted.
    updated_by: Option<AiProjectMappingEditorResponse>,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiProjectMappingEditorResponse {
    user_id: i32,
    full_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UnmappedAiProjectKeyResponse {
    project_key: String,
    /// `YYYY-MM-DD` in the configured AI usage time zone.
    last_used_on: String,
    /// False for `unattributed` usage and keys with surrounding whitespace, which
    /// token-ledger must attribute to a configured project or Git remote first.
    mappable: bool,
}

impl From<AiProjectMappingEntry> for AiProjectMappingResponse {
    fn from(entry: AiProjectMappingEntry) -> Self {
        let mapping = entry.mapping;
        Self {
            project_key: mapping.project_key,
            project: ProjectResponse {
                project_id: mapping.project.id.to_string(),
                project_name: mapping.project.name,
            },
            stale: entry.stale,
            in_use: mapping.in_use,
            created_by: mapping.created_by.map(Into::into),
            created_at: mapping.created_at,
            updated_by: mapping.updated_by.map(Into::into),
            updated_at: mapping.updated_at,
        }
    }
}

impl From<AiProjectMappingEditor> for AiProjectMappingEditorResponse {
    fn from(editor: AiProjectMappingEditor) -> Self {
        Self {
            user_id: editor.id.as_i32(),
            full_name: editor.full_name,
        }
    }
}

impl From<UnmappedAiProjectKey> for UnmappedAiProjectKeyResponse {
    fn from(key: UnmappedAiProjectKey) -> Self {
        Self {
            project_key: key.project_key,
            last_used_on: key.last_used_on.to_string(),
            mappable: key.mappable,
        }
    }
}

fn actor(user: &AuthUser) -> AiProjectMappingActor {
    AiProjectMappingActor {
        user_id: user.id,
        is_admin: user.roles.contains(&Role::Admin),
        authenticated_by: user.method(),
    }
}

/// Mappings of project keys in the caller's own usage, or every mapping for an
/// admin in a browser session.
async fn list_mappings(
    user: AuthUser,
    State(service): State<Arc<dyn AiProjectMappingService>>,
) -> Result<Json<Vec<AiProjectMappingResponse>>, ApiError> {
    let mappings = service.list_mappings(&actor(&user)).await?;

    Ok(Json(mappings.into_iter().map(Into::into).collect()))
}

/// Maps a project key to an active time-tracking project. Developers may only
/// map an unmapped key in their own usage; admins in a browser session may map
/// or remap any key.
async fn set_mapping(
    user: AuthUser,
    State(service): State<Arc<dyn AiProjectMappingService>>,
    Json(request): Json<SetMappingRequest>,
) -> Result<Json<AiProjectMappingResponse>, ApiError> {
    let mapping = service
        .set_mapping(
            &actor(&user),
            &request.project_key,
            &ProjectId::new(request.project_id),
        )
        .await?;

    Ok(Json(mapping.into()))
}

/// Removes a project key's mapping, leaving its usage unassigned. Admins in a
/// browser session only.
async fn delete_mapping(
    user: AuthUser,
    State(service): State<Arc<dyn AiProjectMappingService>>,
    Json(request): Json<DeleteMappingRequest>,
) -> Result<StatusCode, ApiError> {
    service
        .delete_mapping(&actor(&user), &request.project_key)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Project keys with usage but no mapping, in the caller's own usage or
/// everyone's for an admin in a browser session. Unmappable keys such as
/// `unattributed` are listed with `mappable: false`.
async fn list_unmapped_keys(
    user: AuthUser,
    State(service): State<Arc<dyn AiProjectMappingService>>,
) -> Result<Json<Vec<UnmappedAiProjectKeyResponse>>, ApiError> {
    let keys = service.list_unmapped_keys(&actor(&user)).await?;

    Ok(Json(keys.into_iter().map(Into::into).collect()))
}

/// The active time-tracking projects a key can be mapped to.
async fn list_projects(
    _user: AuthUser,
    State(service): State<Arc<dyn AiProjectMappingService>>,
) -> Result<Json<Vec<ProjectResponse>>, ApiError> {
    let projects = service.list_projects().await?;

    Ok(Json(projects.into_iter().map(Into::into).collect()))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_trait::async_trait;
    use axum::{
        body::{to_bytes, Body},
        extract::Path,
        http::{
            header::{AUTHORIZATION, CONTENT_TYPE, COOKIE, SET_COOKIE},
            Method, Request,
        },
        middleware,
        routing::post,
    };
    use axum_login::{tower_sessions::SessionManagerLayer, AuthManagerLayerBuilder};
    use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
    use serde_json::{json, Value};
    use sqlx::PgPool;
    use time::{macros::datetime, OffsetDateTime};
    use tower::ServiceExt;
    use tower_sessions_moka_store::MokaStore;

    use crate::{
        adapters::outbound::postgres::{
            PostgresAiProjectMappingRepository, PostgresAiUsageRepository,
        },
        auth::{authenticate_bearer, require_authenticated, AuthBackend, AuthSession},
        domain::{
            models::{
                AiCoverageStatus, AiMachine, AiMachineId, AiPricing, AiPricingStatus, AiProvider,
                AiProviderCoverage, AiTokenCounts, AiUsageBucket, AiUsageTimeZone, AiUsageUpload,
                AiUsageWindow, Project, TimeTrackingCompany, UserId,
            },
            ports::{
                inbound::ApiTokenAuthenticator,
                outbound::{AiUsageRepository, TimeTrackingProjectCatalog},
            },
            services::AiProjectMappingServiceImpl,
            ApiTokenError, TimeTrackingError, UserPrincipal,
        },
        repositories::{UserRepository, UserRepositoryImpl},
    };

    use super::*;

    const APP: &str = "github.com/example/app";
    const API: &str = "github.com/example/api";
    const MACHINE_DEV: &str = "5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a";
    const MACHINE_COLLEAGUE: &str = "0b7a3c52-9d1e-4f6a-8b2c-3e4d5f6a7b8c";
    const MACHINE_ADMIN: &str = "7e57c0de-0000-4000-8000-000000000001";
    const HOUR: OffsetDateTime = datetime!(2026-09-22 08:00 UTC);

    /// Active projects of one company, or an unreachable provider for `None`.
    struct StaticProjects {
        company: TimeTrackingCompany,
        projects: Option<Vec<Project>>,
    }

    impl StaticProjects {
        fn of(company_id: &str) -> Self {
            Self {
                company: TimeTrackingCompany {
                    provider: "kleer".to_string(),
                    company_id: company_id.to_string(),
                },
                projects: Some(vec![
                    Project::new("101", "Client A delivery"),
                    Project::new("202", "Internal tools"),
                ]),
            }
        }

        fn unreachable() -> Self {
            Self {
                projects: None,
                ..Self::of("company-1")
            }
        }
    }

    #[async_trait]
    impl TimeTrackingProjectCatalog for StaticProjects {
        fn company(&self) -> &TimeTrackingCompany {
            &self.company
        }

        async fn active_projects(&self) -> Result<Vec<Project>, TimeTrackingError> {
            self.projects
                .clone()
                .ok_or_else(|| TimeTrackingError::unknown("provider unavailable"))
        }
    }

    /// Authenticates each bearer token as its user.
    struct Principals(HashMap<String, UserPrincipal>);

    #[async_trait]
    impl ApiTokenAuthenticator for Principals {
        async fn authenticate(
            &self,
            presented: &str,
        ) -> Result<Option<UserPrincipal>, ApiTokenError> {
            Ok(self.0.get(presented).cloned())
        }
    }

    #[derive(Clone, Copy)]
    struct Users {
        dev: UserId,
        colleague: UserId,
        admin: UserId,
    }

    /// Request credentials: a bearer API token or a browser session cookie.
    #[derive(Clone)]
    struct Credential(&'static str, String);

    struct Harness {
        app: Router,
        dev: Credential,
        colleague: Credential,
        /// The admin's personal API token.
        admin_token: Credential,
        /// The admin signed in with a browser session.
        admin: Credential,
    }

    async fn users(pool: &PgPool) -> Users {
        Users {
            dev: insert_user(pool, "dev@example.com", "Dev Example", "User").await,
            colleague: insert_user(pool, "colleague@example.com", "Colleague Example", "User")
                .await,
            admin: insert_user(pool, "admin@example.com", "Admin Example", "Admin").await,
        }
    }

    async fn insert_user(pool: &PgPool, email: &str, full_name: &str, role: &str) -> UserId {
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO users (email, full_name, picture, access_token, roles)
             VALUES ($1, $2, '', '', ARRAY[$3])
             RETURNING id",
        )
        .bind(email)
        .bind(full_name)
        .bind(role)
        .fetch_one(pool)
        .await
        .unwrap();
        UserId::new(id)
    }

    /// The routes behind production authentication: bearer tokens for the
    /// developers and the admin, and a browser session for the admin, signed in
    /// through a test-only route.
    async fn harness(pool: &PgPool, users: Users, projects: Option<StaticProjects>) -> Harness {
        let principal = |id: UserId, role: Role| UserPrincipal {
            id,
            email: format!("{id}@example.com"),
            roles: vec![role],
        };
        let authenticator: Arc<dyn ApiTokenAuthenticator> = Arc::new(Principals(HashMap::from([
            ("dev".to_string(), principal(users.dev, Role::User)),
            (
                "colleague".to_string(),
                principal(users.colleague, Role::User),
            ),
            (
                "admin-token".to_string(),
                principal(users.admin, Role::Admin),
            ),
        ])));

        let db = sqlx_tracing::PoolBuilder::from(pool.clone()).build();
        let service: Arc<dyn AiProjectMappingService> = Arc::new(AiProjectMappingServiceImpl::new(
            Arc::new(PostgresAiProjectMappingRepository::new(db.clone())),
            projects.map(Arc::new),
            AiUsageTimeZone::parse("Europe/Stockholm").unwrap(),
        ));

        let protected = Router::new()
            .nest("/ai-usage/project-mappings", router())
            .route_layer(middleware::from_fn(require_authenticated))
            .layer(middleware::from_fn_with_state(
                authenticator,
                authenticate_bearer,
            ));
        let users_repo = db.clone();
        let sign_in = Router::new().route(
            "/test-sign-in/{user_id}",
            post(
                move |mut session: AuthSession, Path(user_id): Path<i32>| async move {
                    let user = UserRepositoryImpl::new(users_repo)
                        .get_user(UserId::new(user_id))
                        .await
                        .unwrap();
                    session.login(&user).await.unwrap();
                    StatusCode::NO_CONTENT
                },
            ),
        );
        let app = protected
            .merge(sign_in)
            .layer(session_layer(db))
            .with_state(service);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/test-sign-in/{}", users.admin))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = response.headers()[SET_COOKIE].to_str().unwrap();
        let cookie = cookie.split(';').next().unwrap().to_string();

        let token = |token: &str| Credential(AUTHORIZATION.as_str(), format!("Bearer {token}"));
        Harness {
            app,
            dev: token("dev"),
            colleague: token("colleague"),
            admin_token: token("admin-token"),
            admin: Credential(COOKIE.as_str(), cookie),
        }
    }

    fn session_layer(
        db: crate::db::DbPool,
    ) -> axum_login::AuthManagerLayer<AuthBackend, MokaStore> {
        let client = BasicClient::new(ClientId::new("test-client".to_string()))
            .set_client_secret(ClientSecret::new("test-secret".to_string()))
            .set_auth_uri(AuthUrl::new("https://example.com/authorize".to_string()).unwrap())
            .set_token_uri(TokenUrl::new("https://example.com/token".to_string()).unwrap())
            .set_redirect_uri(
                RedirectUrl::new("https://example.com/callback".to_string()).unwrap(),
            );
        let sessions = SessionManagerLayer::new(MokaStore::new(Some(16))).with_secure(false);
        AuthManagerLayerBuilder::new(AuthBackend::new(db, client), sessions).build()
    }

    /// Stores one synthetic bucket per `(project_key, hour_start)` from the machine.
    async fn record_usage(
        pool: &PgPool,
        user: UserId,
        machine_id: &str,
        buckets: &[(&str, OffsetDateTime)],
    ) {
        let upload = AiUsageUpload::new(
            AiMachine {
                id: AiMachineId::parse(machine_id).unwrap(),
                label: "laptop".to_string(),
                client_version: "0.2.0".to_string(),
                time_zone: "Europe/Stockholm".to_string(),
            },
            AiUsageWindow::new(
                datetime!(2026-09-19 00:00 UTC),
                datetime!(2026-09-24 00:00 UTC),
            )
            .unwrap(),
            AiPricing {
                status: AiPricingStatus::Fresh,
                fetched_at: None,
                source: "https://example.com/prices.json".to_string(),
            },
            buckets
                .iter()
                .map(|&(project_key, hour_start)| AiUsageBucket {
                    hour_start,
                    session_key: "9b1f0c6e2d4a8b7c3e5f1a2b4c6d8e0f".to_string(),
                    project_key: project_key.to_string(),
                    provider: AiProvider::Claude,
                    model: "example-model".to_string(),
                    tokens: AiTokenCounts {
                        input: 10,
                        cache_read: 100,
                        cache_write: 5,
                        output: 7,
                    },
                    records: 1,
                    estimated_cost_usd: Some(0.01),
                    unpriced_records: 0,
                })
                .collect(),
            vec![AiProviderCoverage {
                provider: AiProvider::Claude,
                status: AiCoverageStatus::Ok,
                files: 1,
                unreadable: 0,
                malformed_lines: 0,
                skipped_records: 0,
                duplicates: 0,
            }],
            Vec::new(),
        )
        .unwrap();
        let db = sqlx_tracing::PoolBuilder::from(pool.clone()).build();
        PostgresAiUsageRepository::new(db)
            .replace_window(&user, &upload)
            .await
            .unwrap();
    }

    async fn send(
        harness: &Harness,
        method: Method,
        path: &str,
        credential: &Credential,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(format!("/ai-usage/project-mappings{path}"))
            .header(credential.0, &credential.1)
            .header(CONTENT_TYPE, "application/json");
        let body = body.map_or_else(Body::empty, |body| Body::from(body.to_string()));
        let response = harness
            .app
            .clone()
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn put(
        harness: &Harness,
        credential: &Credential,
        project_key: &str,
        project_id: &str,
    ) -> (StatusCode, Value) {
        let body = json!({ "projectKey": project_key, "projectId": project_id });
        send(harness, Method::PUT, "", credential, Some(body)).await
    }

    async fn delete(harness: &Harness, credential: &Credential, project_key: &str) -> StatusCode {
        let body = json!({ "projectKey": project_key });
        send(harness, Method::DELETE, "", credential, Some(body))
            .await
            .0
    }

    /// Each listed mapping as `(projectKey, projectId, stale, inUse)`.
    async fn mappings(
        harness: &Harness,
        credential: &Credential,
    ) -> Vec<(String, String, bool, bool)> {
        let (status, body) = send(harness, Method::GET, "", credential, None).await;
        assert_eq!(status, StatusCode::OK);
        body.as_array()
            .unwrap()
            .iter()
            .map(|mapping| {
                (
                    mapping["projectKey"].as_str().unwrap().to_string(),
                    mapping["project"]["projectId"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                    mapping["stale"].as_bool().unwrap(),
                    mapping["inUse"].as_bool().unwrap(),
                )
            })
            .collect()
    }

    /// Each listed unmapped key as `(projectKey, lastUsedOn, mappable)`.
    async fn unmapped(harness: &Harness, credential: &Credential) -> Vec<(String, String, bool)> {
        let (status, body) = send(harness, Method::GET, "/unmapped-keys", credential, None).await;
        assert_eq!(status, StatusCode::OK);
        body.as_array()
            .unwrap()
            .iter()
            .map(|key| {
                (
                    key["projectKey"].as_str().unwrap().to_string(),
                    key["lastUsedOn"].as_str().unwrap().to_string(),
                    key["mappable"].as_bool().unwrap(),
                )
            })
            .collect()
    }

    fn mapping(
        key: &str,
        project_id: &str,
        stale: bool,
        in_use: bool,
    ) -> (String, String, bool, bool) {
        (key.to_string(), project_id.to_string(), stale, in_use)
    }

    #[sqlx::test]
    async fn developers_only_create_mappings_for_unmapped_keys_in_their_own_usage(pool: PgPool) {
        let users = users(&pool).await;
        let h = harness(&pool, users, Some(StaticProjects::of("company-1"))).await;
        record_usage(&pool, users.dev, MACHINE_DEV, &[(APP, HOUR)]).await;
        record_usage(
            &pool,
            users.colleague,
            MACHINE_COLLEAGUE,
            &[(APP, HOUR), (API, HOUR)],
        )
        .await;

        let (status, error) = put(&h, &h.dev, API, "101").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(
            error["error"],
            "the project key does not appear in your AI usage"
        );

        let (status, saved) = put(&h, &h.dev, APP, "101").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(saved["project"]["projectName"], "Client A delivery");
        for editor in ["createdBy", "updatedBy"] {
            assert_eq!(saved[editor]["userId"], users.dev.as_i32());
            assert_eq!(saved[editor]["fullName"], "Dev Example");
        }

        // One edit would re-attribute everyone's usage of the key, so changing
        // a mapping is left to admins, even for the developer who created it.
        for (credential, project_id) in [(&h.colleague, "202"), (&h.dev, "202"), (&h.dev, "101")] {
            let (status, error) = put(&h, credential, APP, project_id).await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert!(
                error["error"].as_str().unwrap().contains("ask an admin"),
                "{error}"
            );
        }
        assert_eq!(delete(&h, &h.dev, APP).await, StatusCode::FORBIDDEN);
        assert_eq!(delete(&h, &h.colleague, APP).await, StatusCode::FORBIDDEN);

        assert_eq!(
            mappings(&h, &h.admin).await,
            [mapping(APP, "101", false, true)]
        );
    }

    #[sqlx::test]
    async fn admins_change_any_mapping_only_in_a_browser_session(pool: PgPool) {
        let users = users(&pool).await;
        let h = harness(&pool, users, Some(StaticProjects::of("company-1"))).await;
        record_usage(&pool, users.dev, MACHINE_DEV, &[(APP, HOUR)]).await;
        record_usage(&pool, users.admin, MACHINE_ADMIN, &[(APP, HOUR)]).await;
        assert_eq!(put(&h, &h.dev, APP, "101").await.0, StatusCode::OK);

        // An admin's API token has only a developer's powers.
        assert_eq!(
            put(&h, &h.admin_token, APP, "202").await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            put(&h, &h.admin_token, "Client Z", "202").await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(delete(&h, &h.admin_token, APP).await, StatusCode::FORBIDDEN);

        let (status, saved) = put(&h, &h.admin, APP, "202").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(saved["createdBy"]["userId"], users.dev.as_i32());
        assert_eq!(saved["updatedBy"]["userId"], users.admin.as_i32());
        // Admins may map keys that no one has used yet.
        assert_eq!(put(&h, &h.admin, "Client Z", "202").await.0, StatusCode::OK);

        assert_eq!(
            mappings(&h, &h.admin).await,
            [
                mapping("Client Z", "202", false, false),
                mapping(APP, "202", false, true),
            ]
        );
        assert_eq!(
            mappings(&h, &h.admin_token).await,
            [mapping(APP, "202", false, true)],
            "an API token sees only the mappings of its own usage"
        );

        assert_eq!(delete(&h, &h.admin, APP).await, StatusCode::NO_CONTENT);
        assert_eq!(delete(&h, &h.admin, APP).await, StatusCode::NOT_FOUND);
        assert_eq!(
            mappings(&h, &h.admin).await,
            [mapping("Client Z", "202", false, false)]
        );
    }

    #[sqlx::test]
    async fn unmappable_keys_are_listed_but_cannot_be_mapped(pool: PgPool) {
        let users = users(&pool).await;
        let h = harness(&pool, users, Some(StaticProjects::of("company-1"))).await;
        let padded = "github.com/example/padded ";
        record_usage(
            &pool,
            users.dev,
            MACHINE_DEV,
            &[("unattributed", HOUR), (padded, HOUR), (APP, HOUR)],
        )
        .await;

        for key in ["unattributed", padded] {
            for credential in [&h.dev, &h.admin] {
                assert_eq!(
                    put(&h, credential, key, "101").await.0,
                    StatusCode::BAD_REQUEST
                );
            }
            assert_eq!(delete(&h, &h.admin, key).await, StatusCode::BAD_REQUEST);
        }
        let (status, error) = put(&h, &h.dev, APP, "999").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            error["error"],
            "time-tracking project 999 does not exist or is not active"
        );
        assert!(mappings(&h, &h.admin).await.is_empty());

        let key = |key: &str, mappable| (key.to_string(), "2026-09-22".to_string(), mappable);
        assert_eq!(
            unmapped(&h, &h.dev).await,
            [
                key(APP, true),
                key(padded, false),
                key("unattributed", false)
            ]
        );
        assert_eq!(put(&h, &h.dev, APP, "101").await.0, StatusCode::OK);
    }

    #[sqlx::test]
    async fn listings_cover_a_developers_own_usage_and_everyones_for_admins(pool: PgPool) {
        let users = users(&pool).await;
        let h = harness(&pool, users, Some(StaticProjects::of("company-1"))).await;
        let web = "github.com/example/web";
        let copilot_api = "example/api";
        record_usage(
            &pool,
            users.dev,
            MACHINE_DEV,
            &[
                // Midnight on 22 September in Stockholm.
                (APP, datetime!(2026-09-21 22:00 UTC)),
                (web, HOUR),
            ],
        )
        .await;
        record_usage(
            &pool,
            users.colleague,
            MACHINE_COLLEAGUE,
            &[
                (APP, datetime!(2026-09-20 08:00 UTC)),
                (copilot_api, datetime!(2026-09-23 08:00 UTC)),
            ],
        )
        .await;
        assert_eq!(put(&h, &h.dev, web, "101").await.0, StatusCode::OK);
        assert_eq!(put(&h, &h.admin, "Client Z", "202").await.0, StatusCode::OK);

        let key = |key: &str, date: &str| (key.to_string(), date.to_string(), true);
        assert_eq!(unmapped(&h, &h.dev).await, [key(APP, "2026-09-22")]);
        assert_eq!(
            unmapped(&h, &h.colleague).await,
            [key(copilot_api, "2026-09-23"), key(APP, "2026-09-20")]
        );
        assert_eq!(
            unmapped(&h, &h.admin).await,
            [key(copilot_api, "2026-09-23"), key(APP, "2026-09-22")]
        );
        assert!(unmapped(&h, &h.admin_token).await.is_empty());

        assert_eq!(
            mappings(&h, &h.dev).await,
            [mapping(web, "101", false, true)]
        );
        assert!(mappings(&h, &h.colleague).await.is_empty());
        assert_eq!(
            mappings(&h, &h.admin).await,
            [
                mapping("Client Z", "202", false, false),
                mapping(web, "101", false, true),
            ]
        );
    }

    #[sqlx::test]
    async fn mappings_to_another_company_are_stale_until_an_admin_remaps_them(pool: PgPool) {
        let users = users(&pool).await;
        record_usage(&pool, users.dev, MACHINE_DEV, &[(APP, HOUR)]).await;
        let before = harness(&pool, users, Some(StaticProjects::of("company-1"))).await;
        assert_eq!(
            put(&before, &before.dev, APP, "101").await.0,
            StatusCode::OK
        );

        let after = harness(&pool, users, Some(StaticProjects::of("company-2"))).await;
        assert_eq!(
            mappings(&after, &after.admin).await,
            [mapping(APP, "101", true, true)]
        );
        assert!(
            unmapped(&after, &after.dev).await.is_empty(),
            "a stale mapping is listed as a mapping"
        );

        assert_eq!(
            put(&after, &after.admin, APP, "202").await.0,
            StatusCode::OK
        );
        assert_eq!(
            mappings(&after, &after.admin).await,
            [mapping(APP, "202", false, true)]
        );
    }

    #[sqlx::test]
    async fn an_unreachable_or_unconfigured_provider_is_not_an_unknown_project(pool: PgPool) {
        let users = users(&pool).await;
        record_usage(&pool, users.dev, MACHINE_DEV, &[(APP, HOUR)]).await;

        for (projects, message) in [
            (
                Some(StaticProjects::unreachable()),
                "time-tracking projects are unavailable",
            ),
            (None, "time tracking is not configured"),
        ] {
            let h = harness(&pool, users, projects).await;
            let (status, error) = send(&h, Method::GET, "/projects", &h.dev, None).await;
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(error["error"], message);
            let (status, error) = put(&h, &h.dev, APP, "101").await;
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(error["error"], message);
            assert!(mappings(&h, &h.admin).await.is_empty());
        }
    }
}
