//! MCP adapter: Toki's agent surface over the Model Context Protocol.
//!
//! `/mcp` serves Streamable HTTP. Clients on protocol `2026-07-28` and later
//! are served statelessly; older clients get a session, which their
//! server-to-client elicitation needs. When OAuth is configured, the endpoint
//! is an OAuth resource server and publishes its Protected Resource Metadata.

mod auth;
mod confirm;
mod entra;
mod tools;

use std::sync::Arc;

use axum::{routing::get, Json, Router};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tower::ServiceBuilder;
use url::Url;

pub use auth::{McpAuth, McpOAuth, ProtectedResourceMetadata};
pub use entra::EntraTokenValidator;

use crate::app_state::AppState;

pub const MCP_PATH: &str = "/mcp";
const METADATA_PATH: &str = "/.well-known/oauth-protected-resource";

/// The OAuth resource identifier MCP clients request tokens for.
pub fn resource_url(api_url: &Url) -> String {
    format!("{}{MCP_PATH}", api_url.as_str().trim_end_matches('/'))
}

/// Where the resource's metadata is served (RFC 9728, section 3.1).
pub fn metadata_url(api_url: &Url) -> String {
    format!(
        "{}{METADATA_PATH}{MCP_PATH}",
        api_url.as_str().trim_end_matches('/')
    )
}

/// Protected Resource Metadata naming `issuer` as the authorization server.
///
/// The scopes include `offline_access`: clients request the scopes listed here,
/// and Entra issues a refresh token only for that scope, so without it a
/// connection would expire with its first access token.
pub fn protected_resource_metadata(
    api_url: &Url,
    issuer: String,
    scope: &str,
) -> ProtectedResourceMetadata {
    let resource = resource_url(api_url);
    ProtectedResourceMetadata {
        scopes_supported: vec![format!("{resource}/{scope}"), "offline_access".to_string()],
        resource,
        authorization_servers: vec![issuer],
        bearer_methods_supported: vec!["header".to_string()],
        resource_name: "Toki".to_string(),
    }
}

/// The issuer of Entra v2 access tokens for `tenant_id`.
pub fn entra_issuer(tenant_id: &str) -> String {
    entra::issuer(tenant_id)
}

/// The `/mcp` endpoint and, with OAuth, its metadata documents.
pub fn router<S>(app_state: AppState, auth: McpAuth) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let config = server_config(&app_state.api_url, &app_state.app_url);
    let metadata = auth.oauth.as_ref().map(|oauth| oauth.metadata.clone());
    let confirmations = Arc::new(confirm::Confirmations::new());
    let mcp = StreamableHttpService::new(
        move || {
            Ok(tools::TokiMcp::new(
                app_state.clone(),
                confirmations.clone(),
            ))
        },
        Arc::new(LocalSessionManager::default()),
        config,
    );
    let endpoint = ServiceBuilder::new()
        .layer(axum::middleware::from_fn_with_state(
            Arc::new(auth),
            auth::authenticate,
        ))
        .service(mcp);

    let router = Router::new().route_service(MCP_PATH, endpoint);
    match metadata {
        Some(metadata) => {
            let document = move || {
                let metadata = metadata.clone();
                async move { Json(metadata) }
            };
            router
                .route(&format!("{METADATA_PATH}{MCP_PATH}"), get(document.clone()))
                .route(METADATA_PATH, get(document))
        }
        None => router,
    }
}

fn server_config(api_url: &Url, app_url: &Url) -> StreamableHttpServerConfig {
    // Host validation guards against DNS rebinding: accept only the API's own
    // authority. Origin validation rejects browser pages other than Toki's.
    let mut allowed_hosts = Vec::new();
    if let Some(host) = api_url.host_str() {
        allowed_hosts.push(host.to_string());
        if let Some(port) = api_url.port() {
            allowed_hosts.push(format!("{host}:{port}"));
        }
    }
    let app_origin = app_url.origin().ascii_serialization();
    let app_origin = match app_url.port_or_known_default() {
        Some(port) if app_url.port().is_none() => format!("{app_origin}:{port}"),
        _ => app_origin,
    };

    StreamableHttpServerConfig::default()
        .with_allowed_hosts(allowed_hosts)
        .with_allowed_origins([app_origin])
        .enforce_origin_validation()
        .with_json_response(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_resource_and_metadata_follow_the_api_url() {
        let api_url = Url::parse("https://toki-api.spinit.se").unwrap();

        assert_eq!(resource_url(&api_url), "https://toki-api.spinit.se/mcp");
        assert_eq!(
            metadata_url(&api_url),
            "https://toki-api.spinit.se/.well-known/oauth-protected-resource/mcp"
        );
        let metadata = protected_resource_metadata(&api_url, entra_issuer("tenant"), "mcp");
        assert_eq!(
            metadata.scopes_supported,
            ["https://toki-api.spinit.se/mcp/mcp", "offline_access"]
        );
        assert_eq!(
            metadata.authorization_servers,
            ["https://login.microsoftonline.com/tenant/v2.0"]
        );
    }

    #[test]
    fn only_the_api_host_and_app_origin_are_allowed() {
        let config = server_config(
            &Url::parse("http://localhost:8180").unwrap(),
            &Url::parse("https://toki.spinit.se").unwrap(),
        );

        assert_eq!(config.allowed_hosts, ["localhost", "localhost:8180"]);
        assert_eq!(config.allowed_origins, ["https://toki.spinit.se:443"]);
    }
}
