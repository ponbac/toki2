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
    paths(
        crate::routes::pull_requests::list_pull_requests,
        crate::routes::time_tracking::connection_status,
        crate::routes::time_tracking::create_time_entry,
        crate::routes::time_tracking::delete_time_entry,
        crate::routes::time_tracking::edit_timer,
        crate::routes::time_tracking::get_time_entries,
        crate::routes::time_tracking::get_time_entry_day_statuses,
        crate::routes::time_tracking::get_time_info,
        crate::routes::time_tracking::get_timer,
        crate::routes::time_tracking::get_timer_history,
        crate::routes::time_tracking::list_activities,
        crate::routes::time_tracking::list_projects,
        crate::routes::time_tracking::save_timer,
        crate::routes::time_tracking::start_timer,
        crate::routes::time_tracking::stop_timer,
        crate::routes::time_tracking::update_time_entry,
        crate::routes::work_items::format_for_llm,
        crate::routes::work_items::get_board,
        crate::routes::work_items::get_iterations,
        crate::routes::work_items::get_projects,
        crate::routes::work_items::move_work_item,
    ),
    info(
        title = "Toki Agent API",
        version = "1.1.0",
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

    const EXPECTED_OPERATIONS: &[(&str, &str, &str)] = &[
        (
            "delete",
            "/time-tracking/time-entries/{registration_id}",
            "deleteTimeEntry",
        ),
        ("delete", "/time-tracking/timer", "discardActiveTimer"),
        ("get", "/pull-requests/list", "listPullRequests"),
        (
            "get",
            "/time-tracking/connection",
            "getTimeTrackingConnection",
        ),
        ("get", "/time-tracking/projects", "listTimeTrackingProjects"),
        (
            "get",
            "/time-tracking/projects/{project_id}/activities",
            "listTimeTrackingActivities",
        ),
        ("get", "/time-tracking/time-entries", "listTimeEntries"),
        (
            "get",
            "/time-tracking/time-entry-day-statuses",
            "listTimeEntryDayStatuses",
        ),
        ("get", "/time-tracking/time-info", "getTimeInfo"),
        ("get", "/time-tracking/timer", "getActiveTimer"),
        ("get", "/time-tracking/timer-history", "listTimerHistory"),
        ("get", "/work-items/board", "getWorkItemBoard"),
        ("get", "/work-items/format-for-llm", "formatWorkItemForLlm"),
        ("get", "/work-items/iterations", "listWorkItemIterations"),
        ("get", "/work-items/projects", "listWorkItemProjects"),
        ("patch", "/time-tracking/timer", "updateActiveTimer"),
        ("post", "/time-tracking/time-entries", "createTimeEntry"),
        ("post", "/time-tracking/timer", "startActiveTimer"),
        ("post", "/time-tracking/timer/save", "saveActiveTimer"),
        ("post", "/work-items/move", "moveWorkItem"),
        (
            "put",
            "/time-tracking/time-entries/{registration_id}",
            "updateTimeEntry",
        ),
    ];

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
        assert_eq!(spec["info"]["version"], "1.1.0");
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
            let (success_status, success_response) = responses
                .iter()
                .find(|(status, _)| {
                    status
                        .parse::<u16>()
                        .is_ok_and(|status| (200..300).contains(&status))
                })
                .unwrap_or_else(|| panic!("{operation_id} is missing a 2xx response"));
            if success_status != "204" {
                assert!(
                    success_response["content"]
                        .as_object()
                        .is_some_and(|content| content
                            .values()
                            .all(|media| media.get("schema").is_some())),
                    "{operation_id} {success_status} response must be typed"
                );
            }
            assert!(
                responses.contains_key("401"),
                "{operation_id} is missing a 401 response"
            );

            if let Some(request_body) = operation.get("requestBody") {
                assert!(
                    request_body["content"].as_object().is_some_and(|content| {
                        !content.is_empty()
                            && content.values().all(|media| media.get("schema").is_some())
                    }),
                    "{operation_id} request body must be typed"
                );
            }
            if let Some(parameters) = operation["parameters"].as_array() {
                for parameter in parameters {
                    let name = parameter["name"].as_str().unwrap_or("<unnamed>");
                    assert!(
                        parameter.get("schema").is_some() || parameter.get("content").is_some(),
                        "{operation_id} parameter {name} must be typed"
                    );
                }
            }
        }
    }

    #[test]
    fn document_does_not_publish_provider_wire_types() {
        let serialized = spec().to_string();
        for leak in [
            "GitCommitRef",
            "IdentityRefWithVote",
            "CommentThread",
            "KleerEventWritable",
            "KleerIdRef",
            "KleerCredentials",
            "X-token",
        ] {
            assert!(
                !serialized.contains(leak),
                "{leak} leaked into the agent OpenAPI document"
            );
        }
    }

    #[test]
    fn mutation_inputs_are_provider_neutral_and_retry_headers_are_required() {
        let spec = spec();
        let schemas = &spec["components"]["schemas"];

        for schema_name in [
            "StartActiveTimerRequest",
            "UpdateActiveTimerRequest",
            "CreateTimeEntryRequestBody",
            "UpdateTimeEntryRequest",
        ] {
            let properties = schemas[schema_name]["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("{schema_name} properties"));
            for forbidden in [
                "projectName",
                "activityName",
                "registrationId",
                "regDay",
                "weekNumber",
            ] {
                assert!(
                    !properties.contains_key(forbidden),
                    "{schema_name} must not expose {forbidden}"
                );
            }
        }

        for path in ["/time-tracking/timer/save", "/time-tracking/time-entries"] {
            let operation = &spec["paths"][path]["post"];
            let header = operation["parameters"]
                .as_array()
                .expect("operation parameters")
                .iter()
                .find(|parameter| parameter["name"] == "Idempotency-Key")
                .unwrap_or_else(|| panic!("{path} Idempotency-Key parameter"));
            assert_eq!(header["in"], "header");
            assert_eq!(header["required"], true);
            assert_eq!(header["schema"]["type"], "string");
        }

        let patch_properties = schemas["UpdateActiveTimerRequest"]["properties"]
            .as_object()
            .expect("timer patch properties");
        assert!(schema_allows_null(&patch_properties["projectId"]));
        assert!(schema_allows_null(&patch_properties["activityId"]));
    }

    fn schema_allows_null(schema: &Value) -> bool {
        schema["type"] == "null"
            || schema["type"]
                .as_array()
                .is_some_and(|types| types.iter().any(|kind| kind == "null"))
            || ["oneOf", "anyOf"]
                .iter()
                .filter_map(|key| schema[*key].as_array())
                .flatten()
                .any(schema_allows_null)
    }

    #[test]
    fn document_preserves_closed_enum_values() {
        let spec = spec();
        let schemas = &spec["components"]["schemas"];

        assert_eq!(
            schemas["TimeEntryStatusResponse"]["enum"],
            serde_json::json!(["open", "approved", "certified"])
        );
        assert_eq!(
            schemas["BoardStateResponse"]["enum"],
            serde_json::json!(["todo", "inProgress", "done"])
        );
        assert_eq!(
            schemas["PullRequestMergeStatusResponse"]["enum"],
            serde_json::json!([
                "notSet",
                "queued",
                "conflicts",
                "succeeded",
                "rejectedByPolicy",
                "failure"
            ])
        );
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
