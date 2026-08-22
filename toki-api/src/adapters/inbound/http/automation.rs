//! Curated agent OpenAPI document.
//!
//! Runtime routing and authentication are composed independently. A route is
//! exposed to agents only by being listed in this document.

use axum::{routing::get, Json, Router};
use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    openapi::OpenApi as OpenApiDocument,
    Modify, OpenApi,
};
const BEARER_SCHEME: &str = "bearerAuth";

/// Agent-facing OpenAPI metadata.
#[derive(OpenApi)]
#[openapi(
    paths(crate::routes::time_tracking::get_timer),
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

/// Generate the curated agent OpenAPI document.
pub fn agent_openapi() -> OpenApiDocument {
    AgentApi::openapi()
}

/// Public, unauthenticated OpenAPI document. This path is not an agent operation.
pub fn openapi_spec_router<S>(document: OpenApiDocument) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().route(
        "/openapi.json",
        get(move || {
            let document = document.clone();
            async move { Json(document) }
        }),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;

    const EXPECTED_OPERATIONS: &[(&str, &str, &str)] =
        &[("get", "/time-tracking/timer", "getActiveTimer")];

    fn spec() -> Value {
        serde_json::to_value(agent_openapi()).expect("OpenAPI document should serialize")
    }

    fn operations(spec: &Value) -> Vec<(&str, &str, &str)> {
        let mut listed = Vec::new();
        let paths = spec["paths"].as_object().expect("OpenAPI paths object");
        for (path, item) in paths {
            let item = item.as_object().expect("path item object");
            for method in [
                "get", "put", "post", "delete", "patch", "options", "head", "trace",
            ] {
                if let Some(operation) = item.get(method) {
                    let operation_id = operation["operationId"].as_str().unwrap_or_default();
                    listed.push((method, path.as_str(), operation_id));
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
        assert_eq!(spec["security"].as_array().map(Vec::len), Some(1));
        assert!(spec["security"][0].get(BEARER_SCHEME).is_some());

        let serialized = spec.to_string();
        let real_token = regex::Regex::new(r"toki_[0-9a-fA-F]{20,}").unwrap();
        assert!(
            !real_token.is_match(&serialized),
            "OpenAPI document must not contain token secrets"
        );
    }

    #[test]
    fn document_contains_exactly_the_allowlisted_operations() {
        let spec = spec();
        assert_eq!(operations(&spec), EXPECTED_OPERATIONS);
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
                ids.insert(operation_id),
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
            assert!(
                responses.contains_key("403"),
                "{operation_id} is missing a 403 response"
            );
        }
    }

    #[tokio::test]
    async fn openapi_json_is_served_without_authentication() {
        let document = agent_openapi();
        let expected = serde_json::to_value(&document).unwrap();
        let app = openapi_spec_router::<()>(document);
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
        assert_eq!(json, expected, "served document must match the catalog");
    }
}
