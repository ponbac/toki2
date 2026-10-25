use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    extract::Path,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE, COOKIE, SET_COOKIE},
        Method, Request,
    },
    routing::post,
};
use axum_login::{tower_sessions::SessionManagerLayer, AuthManagerLayerBuilder, AuthnBackend};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
use serde_json::{json, Value};
use sqlx::PgPool;
use time::macros::{date, datetime};
use tower::ServiceExt;
use tower_sessions_moka_store::MokaStore;

use super::*;
use crate::{
    adapters::{
        inbound::http::agent_openapi,
        outbound::postgres::{
            ai_usage_fixtures::{bucket, record, MACHINE_A, MACHINE_B},
            ClockedAiRepositoryMappingRepository, PostgresAiMeteredUsageRepository,
            PostgresAiProjectMappingRepository, PostgresAiRepositoryMappingRepository,
            PostgresApiTokenRepository,
        },
    },
    auth::{authenticate_bearer, require_authenticated, AuthBackend, AuthSession},
    domain::{
        models::{
            AiBillingMonth, AiDecimal, AiMeteredOrgCharge, AiOrgChargeKind, AiProjectKey,
            AiProvider, AiUsageDateRange, Project, TimeTrackingCompany, UserId,
        },
        ports::{
            inbound::{ApiTokenAuthenticator, ApiTokenService},
            outbound::{
                AiMeteredUsageRepository, AiProjectMappingRepository, TimeTrackingProjectCatalog,
            },
        },
        services::{
            AiRepositoryMappingServiceImpl, AiRepositoryMappingSettings, ApiTokenServiceImpl,
        },
        TimeTrackingError,
    },
};

const URI: &str = "/ai-usage/admin/repository-mappings";
/// The store's clock: the overview shows September and October.
const NOW: time::OffsetDateTime = datetime!(2026-10-25 12:00 UTC);

fn db(pool: &PgPool) -> crate::db::DbPool {
    crate::db::DbPool::from(pool.clone())
}

fn company(company_id: &str) -> TimeTrackingCompany {
    TimeTrackingCompany {
        provider: "kleer".to_string(),
        company_id: company_id.to_string(),
    }
}

/// Active projects of the first company.
struct StaticProjects(TimeTrackingCompany);

#[async_trait]
impl TimeTrackingProjectCatalog for StaticProjects {
    fn company(&self) -> &TimeTrackingCompany {
        &self.0
    }

    async fn active_projects(&self) -> Result<Vec<Project>, TimeTrackingError> {
        Ok(vec![
            Project::new("101", "Client A"),
            Project::new("202", "Client B"),
        ])
    }
}

/// Test-only sign-in that starts a browser session for any stored user.
async fn sign_in(mut session: AuthSession, Path(user_id): Path<i64>) -> StatusCode {
    let user = session.backend.get_user(&user_id).await.unwrap().unwrap();
    session.login(&user).await.unwrap();
    StatusCode::NO_CONTENT
}

/// The routes composed as in production, the store's clock at `NOW`;
/// `configured` says whether time tracking is.
fn app(pool: &PgPool, configured: bool) -> Router {
    let db = db(pool);
    let service: Arc<dyn AiRepositoryMappingService> =
        Arc::new(AiRepositoryMappingServiceImpl::new(
            Arc::new(ClockedAiRepositoryMappingRepository {
                inner: PostgresAiRepositoryMappingRepository::new(db.clone()),
                now: NOW,
            }),
            configured.then(|| Arc::new(StaticProjects(company("company-1")))),
            AiProvider::Copilot,
            AiRepositoryMappingSettings {
                key_host: "github.com".to_string(),
                billed_from: AiBillingMonth::parse("2026-10"),
            },
        ));
    let tokens: Arc<dyn ApiTokenAuthenticator> = Arc::new(ApiTokenServiceImpl::new(Arc::new(
        PostgresApiTokenRepository::new(db.clone()),
    )));
    let client = BasicClient::new(ClientId::new("test-client".to_string()))
        .set_client_secret(ClientSecret::new("test-secret".to_string()))
        .set_auth_uri(AuthUrl::new("https://example.com/authorize".to_string()).unwrap())
        .set_token_uri(TokenUrl::new("https://example.com/token".to_string()).unwrap())
        .set_redirect_uri(RedirectUrl::new("https://example.com/callback".to_string()).unwrap());
    let sessions = SessionManagerLayer::new(MokaStore::new(Some(16))).with_secure(false);

    Router::new()
        .nest("/ai-usage/admin", router())
        .route_layer(middleware::from_fn(require_authenticated))
        .layer(middleware::from_fn_with_state(tokens, authenticate_bearer))
        .route("/test-sign-in/{user_id}", post(sign_in))
        .with_state(service)
        .layer(AuthManagerLayerBuilder::new(AuthBackend::new(db, client), sessions).build())
}

/// How a request authenticates.
enum Caller<'a> {
    Anonymous,
    Token(&'a str),
    Session(&'a str),
}

async fn send(
    app: &Router,
    caller: &Caller<'_>,
    method: Method,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(URI);
    request = match caller {
        Caller::Anonymous => request,
        Caller::Token(token) => request.header(AUTHORIZATION, format!("Bearer {token}")),
        Caller::Session(cookie) => request.header(COOKIE, *cookie),
    };
    let body = match body {
        Some(body) => {
            request = request.header(CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes)))
    };
    (status, json)
}

struct TestUser {
    id: UserId,
    token: String,
    cookie: String,
}

async fn test_user(app: &Router, pool: &PgPool, email: &str, roles: &[&str]) -> TestUser {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, full_name, picture, access_token, roles)
         VALUES ($1, 'Test User', '', '', $2)
         RETURNING id",
    )
    .bind(email)
    .bind(roles)
    .fetch_one(pool)
    .await
    .unwrap();
    let token = ApiTokenServiceImpl::new(Arc::new(PostgresApiTokenRepository::new(db(pool))))
        .create(&UserId::new(id), "token-ledger")
        .await
        .unwrap()
        .secret
        .as_str()
        .to_string();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/test-sign-in/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let cookie = response.headers()[SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    TestUser {
        id: UserId::new(id),
        token,
        cookie,
    }
}

/// Code review the organization's rows bill to each repository on 2 and
/// 20 October.
async fn billed(pool: &PgPool, rows: &[(&str, &str)]) {
    let dec = |raw: &str| AiDecimal::parse(raw).unwrap();
    let rows: Vec<AiMeteredOrgCharge> = rows
        .iter()
        .flat_map(|(repository, quantity)| {
            [date!(2026 - 10 - 02), date!(2026 - 10 - 20)].map(|day| AiMeteredOrgCharge {
                day,
                product: "copilot".to_string(),
                sku: "Copilot AI Credits".to_string(),
                unit: "AICredits".to_string(),
                repository: repository.to_string(),
                kind: AiOrgChargeKind::RepositoryCredits,
                unit_price: dec("0.01"),
                quantity: dec(quantity),
                gross_amount: dec(quantity),
                discount_amount: AiDecimal::ZERO,
                net_amount: dec(quantity),
                reported_at: None,
            })
        })
        .collect();
    PostgresAiMeteredUsageRepository::new(db(pool))
        .replace_org_usage(
            AiProvider::Copilot,
            AiUsageDateRange::from_inclusive(date!(2026 - 10 - 01), date!(2026 - 10 - 24)).unwrap(),
            &rows,
            datetime!(2026-10-25 06:00 UTC),
        )
        .await
        .unwrap();
}

/// Maps a project key to a project, as its developer would.
async fn map_key(pool: &PgPool, by: UserId, key: &str, company_id: &str, project_id: &str) {
    PostgresAiProjectMappingRepository::new(db(pool))
        .upsert(
            &AiProjectKey::parse(key).unwrap(),
            &company(company_id),
            &crate::domain::models::AiMappedProject {
                id: crate::domain::models::ProjectId::new(project_id),
                name: format!("Project {project_id}"),
            },
            &by,
        )
        .await
        .unwrap();
}

fn repository<'a>(overview: &'a Value, name: &str) -> &'a Value {
    overview["repositories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["repository"] == name)
        .unwrap_or_else(|| panic!("no repository {name} in {overview}"))
}

#[sqlx::test]
async fn only_admins_in_a_browser_session_reach_repository_mappings(pool: PgPool) {
    let app = app(&pool, true);
    let root = test_user(&app, &pool, "root@example.com", &["Admin"]).await;
    let dev = test_user(&app, &pool, "dev@example.com", &["User"]).await;
    billed(&pool, &[("widget-api", "6")]).await;
    let put = json!({ "repository": "widget-api", "projectId": "101" });
    let delete = json!({ "repository": "widget-api" });

    for (caller, expected) in [
        (Caller::Anonymous, StatusCode::UNAUTHORIZED),
        (Caller::Session(&dev.cookie), StatusCode::FORBIDDEN),
        (Caller::Token(&dev.token), StatusCode::FORBIDDEN),
        (Caller::Token(&root.token), StatusCode::FORBIDDEN),
    ] {
        for (method, body) in [
            (Method::GET, None),
            (Method::PUT, Some(put.clone())),
            (Method::DELETE, Some(delete.clone())),
        ] {
            let (status, response) = send(&app, &caller, method.clone(), body).await;
            assert_eq!(status, expected, "{method} {response}");
        }
    }
    let (status, _) = send(&app, &Caller::Session(&root.cookie), Method::GET, None).await;
    assert_eq!(status, StatusCode::OK);
    // Nothing was mapped by those refused.
    let (_, overview) = send(&app, &Caller::Session(&root.cookie), Method::GET, None).await;
    assert_eq!(repository(&overview, "widget-api")["mapping"], Value::Null);
}

#[sqlx::test]
async fn admins_map_billed_repositories_and_remove_mappings(pool: PgPool) {
    let app = app(&pool, true);
    let root = test_user(&app, &pool, "root@example.com", &["Admin"]).await;
    let admin = Caller::Session(&root.cookie);
    billed(&pool, &[("Widget-API", "6"), ("zephyr-web", "4")]).await;

    let (status, overview) = send(&app, &admin, Method::GET, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(overview["provider"], "copilot");
    assert_eq!(overview["billedFrom"], "2026-10");
    assert_eq!(overview["months"], json!(["2026-09", "2026-10"]));
    // October is the cutover month: its credits bill a line.
    let october = |credits: &str| {
        json!([
            { "month": "2026-09", "credits": "0.00", "billed": false },
            { "month": "2026-10", "credits": credits, "billed": true },
        ])
    };
    // Both need a mapping: by name.
    assert_eq!(
        overview["repositories"],
        json!([
            {
                "repository": "Widget-API",
                "months": october("12.00"),
                "lastBilledOn": "2026-10-20",
                "mapping": null,
                "suggestions": [],
            },
            {
                "repository": "zephyr-web",
                "months": october("8.00"),
                "lastBilledOn": "2026-10-20",
                "mapping": null,
                "suggestions": [],
            },
        ])
    );

    // A repository GitHub never billed has nothing to map.
    let (status, error) = send(
        &app,
        &admin,
        Method::PUT,
        Some(json!({ "repository": "other-repo", "projectId": "101" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{error}");
    assert_eq!(
        error["error"],
        "no credits were ever billed to repository \"other-repo\""
    );
    for (body, expected) in [
        (
            json!({ "repository": "widget-api", "projectId": "999" }),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({ "repository": "widget-api", "projectId": "101", "project": "x" }),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({ "repository": "", "projectId": "101" }),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({ "repository": "widget-api" }),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let (status, error) = send(&app, &admin, Method::PUT, Some(body.clone())).await;
        assert_eq!(status, expected, "{body} {error}");
    }

    let (status, saved) = send(
        &app,
        &admin,
        Method::PUT,
        Some(json!({ "repository": "widget-api", "projectId": "101" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    let editor = json!({ "userId": root.id.as_i32(), "fullName": "Test User" });
    assert_eq!(saved["repository"], "Widget-API");
    assert_eq!(
        saved["project"],
        json!({ "projectId": "101", "projectName": "Client A" })
    );
    assert_eq!(saved["status"], "resolves");
    assert_eq!(saved["createdBy"], editor);
    assert_eq!(saved["updatedBy"], editor);
    assert!(saved["createdAt"].is_string());
    assert!(saved["updatedAt"].is_string());

    let (_, overview) = send(&app, &admin, Method::GET, None).await;
    assert_eq!(repository(&overview, "Widget-API")["mapping"], saved);
    // The unmapped repository needs a look first, though it sorts after.
    assert_eq!(overview["repositories"][0]["repository"], "zephyr-web");
    assert_eq!(overview["repositories"][1]["repository"], "Widget-API");

    let delete = |name: &str| Some(json!({ "repository": name }));
    let (status, _) = send(&app, &admin, Method::DELETE, delete("WIDGET-API")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, error) = send(&app, &admin, Method::DELETE, delete("widget-api")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{error}");
    let (status, _) = send(
        &app,
        &admin,
        Method::DELETE,
        Some(json!({ "repository": "widget-api", "extra": true })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, overview) = send(&app, &admin, Method::GET, None).await;
    assert_eq!(repository(&overview, "Widget-API")["mapping"], Value::Null);
}

#[sqlx::test]
async fn without_time_tracking_nothing_is_mapped(pool: PgPool) {
    let app = app(&pool, false);
    let root = test_user(&app, &pool, "root@example.com", &["Admin"]).await;
    billed(&pool, &[("widget-api", "6")]).await;
    let (status, error) = send(
        &app,
        &Caller::Session(&root.cookie),
        Method::PUT,
        Some(json!({ "repository": "widget-api", "projectId": "101" })),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error["error"], "time tracking is not configured");
    let (status, _) = send(&app, &Caller::Session(&root.cookie), Method::GET, None).await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test]
async fn repository_mapping_suggestions_never_come_from_unmapped_keys(pool: PgPool) {
    use crate::domain::models::AiUsageBucket;
    let app = app(&pool, true);
    let root = test_user(&app, &pool, "root@example.com", &["Admin"]).await;
    let ada = test_user(&app, &pool, "ada@example.com", &["User"]).await;
    let grace = test_user(&app, &pool, "grace@example.com", &["User"]).await;
    let (gadget_key, widget_key) = (
        "github.com/example-org/gadget-web",
        "github.com/example-org/widget-api",
    );
    let hour = datetime!(2026-10-02 08:00 UTC);
    let copilot = |key: &str| AiUsageBucket {
        provider: AiProvider::Copilot,
        model: "gpt-6.1-sol".to_string(),
        ..bucket(hour, key, 2, Some(0.5))
    };
    record(&pool, ada.id, MACHINE_A, vec![copilot(gadget_key)]).await;
    record(&pool, grace.id, MACHINE_B, vec![copilot(widget_key)]).await;
    PostgresAiProjectMappingRepository::new(db(&pool))
        .mark_personal(&grace.id, &AiProjectKey::parse(widget_key).unwrap())
        .await
        .unwrap();
    billed(&pool, &[("gadget-web", "6"), ("widget-api", "4")]).await;
    let admin = Caller::Session(&root.cookie);

    // Neither key is mapped: no suggestion, and neither key is named.
    let (_, overview) = send(&app, &admin, Method::GET, None).await;
    for name in ["gadget-web", "widget-api"] {
        assert_eq!(
            repository(&overview, name)["suggestions"],
            json!([]),
            "{name}"
        );
    }
    let body = overview.to_string();
    assert!(
        !body.contains(gadget_key) && !body.contains(widget_key),
        "{body}"
    );

    // Once Ada maps her key, it suggests its project.
    map_key(&pool, ada.id, gadget_key, "company-1", "101").await;
    let (_, overview) = send(&app, &admin, Method::GET, None).await;
    assert_eq!(
        repository(&overview, "gadget-web")["suggestions"],
        json!([{
            "project": { "projectId": "101", "projectName": "Project 101" },
            "projectKeys": [gadget_key],
        }])
    );
    assert_eq!(
        repository(&overview, "widget-api")["suggestions"],
        json!([])
    );

    // A key mapped to another company's project never suggests.
    map_key(&pool, root.id, widget_key, "company-2", "202").await;
    let (_, overview) = send(&app, &admin, Method::GET, None).await;
    assert_eq!(
        repository(&overview, "widget-api")["suggestions"],
        json!([])
    );

    // A resolving repository mapping needs no suggestion.
    let (status, _) = send(
        &app,
        &admin,
        Method::PUT,
        Some(json!({ "repository": "gadget-web", "projectId": "202" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, overview) = send(&app, &admin, Method::GET, None).await;
    assert_eq!(
        repository(&overview, "gadget-web")["suggestions"],
        json!([])
    );
}

#[test]
fn repository_mappings_stay_off_the_agent_surface() {
    let document = serde_json::to_value(agent_openapi()).unwrap();
    assert!(!document.to_string().contains("repository-mappings"));
}
