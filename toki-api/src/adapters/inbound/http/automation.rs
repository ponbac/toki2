//! Curated automation HTTP seam.
//!
//! This module owns both the generated OpenAPI catalog and the runtime route
//! set that is globally eligible for bearer authentication. An endpoint becomes
//! an agent tool only by being registered here. Per-token capabilities can later
//! narrow that set; they must not replace this router as the allowlist.

use axum::{routing::get, Json, Router};
use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    Modify, OpenApi,
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app_state::AppState;

const BEARER_SCHEME: &str = "bearerAuth";

/// Agent-facing OpenAPI metadata. Paths are contributed by [`automation_openapi_router`].
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Toki Agent API",
        version = "1.0.0",
        description = "Curated automation surface for Toki. Browser session, admin, and media endpoints are not included. Authenticate with a Toki personal API token via HTTP bearer; never embed a real token in this document."
    ),
    modifiers(&BearerSecurity),
    security(("bearerAuth" = []))
)]
struct AgentApi;

struct BearerSecurity;

impl Modify for BearerSecurity {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            BEARER_SCHEME,
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("Toki personal API token")
                    .description(Some(
                        "Present `Authorization: Bearer toki_...`. The secret is shown once at issuance and is never included in this document.",
                    ))
                    .build(),
            ),
        );
    }
}

fn automation_openapi_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::<AppState>::with_openapi(AgentApi::openapi())
        .routes(routes!(crate::routes::time_tracking::get_timer))
}

/// Runtime automation routes together with the OpenAPI document they generate.
pub fn automation_parts() -> (Router<AppState>, utoipa::openapi::OpenApi) {
    automation_openapi_router().split_for_parts()
}

/// Generated agent OpenAPI document. Source of truth is the automation router.
pub fn agent_openapi() -> utoipa::openapi::OpenApi {
    automation_openapi_router().into_openapi()
}

/// Public, unauthenticated OpenAPI document. This path is not an agent tool.
pub fn openapi_spec_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/openapi.json", get(get_openapi_spec))
}

async fn get_openapi_spec() -> Json<utoipa::openapi::OpenApi> {
    Json(agent_openapi())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;

    const EXPECTED_OPERATIONS: &[(&str, &str, &str)] =
        &[("get", "/time-tracking/timer", "getActiveTimer")];

    const EXCLUDED_PATH_PREFIXES: &[&str] = &[
        "/login",
        "/logout",
        "/me",
        "/oauth",
        "/users",
        "/notifications",
        "/differs",
        "/repositories",
        "/work-items/image",
        "/work-items/move",
        "/time-tracking/absences",
        "/time-tracking/absence-",
        "/time-tracking/admin",
        "/openapi.json",
    ];

    fn spec() -> Value {
        serde_json::to_value(agent_openapi()).expect("OpenAPI document should serialize")
    }

    fn operations(spec: &Value) -> Vec<(String, String, String)> {
        let mut listed = Vec::new();
        let paths = spec["paths"].as_object().expect("OpenAPI paths object");
        for (path, item) in paths {
            let item = item.as_object().expect("path item object");
            for method in [
                "get", "put", "post", "delete", "patch", "options", "head", "trace",
            ] {
                if let Some(operation) = item.get(method) {
                    let operation_id = operation["operationId"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    listed.push((method.to_string(), path.clone(), operation_id));
                }
            }
        }
        listed.sort();
        listed
    }

    #[test]
    fn document_is_openapi_3_1() {
        let spec = spec();
        let version = spec["openapi"].as_str().expect("openapi version");
        assert!(
            version.starts_with("3.1."),
            "expected OpenAPI 3.1.x, got {version}"
        );
        assert_eq!(spec["info"]["version"], "1.0.0");
        assert_eq!(spec["info"]["title"], "Toki Agent API");
    }

    #[test]
    fn document_declares_bearer_security_without_credentials() {
        let spec = spec();
        let scheme = &spec["components"]["securitySchemes"]["bearerAuth"];
        assert_eq!(scheme["type"], "http");
        assert_eq!(scheme["scheme"], "bearer");

        let serialized = spec.to_string();
        let real_token = regex::Regex::new(r"toki_[0-9a-fA-F]{20,}").unwrap();
        assert!(
            !real_token.is_match(&serialized),
            "OpenAPI document must not contain token secrets"
        );
    }

    #[test]
    fn document_contains_exactly_the_allowlisted_operations() {
        let listed = operations(&spec());
        let expected: Vec<(String, String, String)> = EXPECTED_OPERATIONS
            .iter()
            .map(|(method, path, id)| {
                (
                    (*method).to_string(),
                    (*path).to_string(),
                    (*id).to_string(),
                )
            })
            .collect();
        assert_eq!(listed, expected);
    }

    #[test]
    fn operation_ids_are_unique_and_operations_are_fully_described() {
        let spec = spec();
        let mut ids = HashSet::new();
        for (method, path, operation_id) in operations(&spec) {
            assert!(
                !operation_id.is_empty(),
                "{method} {path} is missing operationId"
            );
            assert!(
                ids.insert(operation_id.clone()),
                "duplicate operationId {operation_id}"
            );

            let operation = &spec["paths"][&path][&method];
            assert!(
                operation["summary"].as_str().is_some_and(|s| !s.is_empty()),
                "{operation_id} is missing summary"
            );
            assert!(
                operation["description"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty()),
                "{operation_id} is missing description"
            );
            assert!(
                operation["security"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|requirement| requirement.get("bearerAuth").is_some())
                    || spec["security"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|requirement| requirement.get("bearerAuth").is_some()),
                "{operation_id} is missing bearer security"
            );

            let responses = operation["responses"]
                .as_object()
                .expect("operation responses");
            assert!(
                responses.contains_key("200"),
                "{operation_id} is missing a 200 response"
            );
            assert!(
                responses["200"].get("content").is_some(),
                "{operation_id} 200 response must be typed"
            );
            assert!(
                responses.contains_key("401"),
                "{operation_id} is missing a 401 response"
            );
        }
    }

    #[test]
    fn excluded_browser_and_admin_paths_are_absent() {
        let spec = spec();
        let paths: BTreeSet<&str> = spec["paths"]
            .as_object()
            .expect("paths")
            .keys()
            .map(String::as_str)
            .collect();
        for prefix in EXCLUDED_PATH_PREFIXES {
            for path in &paths {
                assert!(
                    !path.starts_with(prefix),
                    "{path} should not be part of the agent catalog"
                );
            }
        }
    }

    #[tokio::test]
    async fn openapi_json_is_served_without_authentication() {
        let app = openapi_spec_router::<()>();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["openapi"].as_str().unwrap().starts_with("3.1."));
        assert_eq!(
            operations(&json),
            operations(&spec()),
            "served document must match the generated catalog"
        );
    }
}
