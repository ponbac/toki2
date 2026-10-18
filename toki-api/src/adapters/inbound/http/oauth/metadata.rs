use axum::{extract::State, Json};
use serde_json::{json, Value};

use super::{ServerState, AUTHORIZE_PATH, REGISTER_PATH, REVOKE_PATH, TOKEN_PATH};
use crate::domain::models::OAUTH_SCOPE;

/// Authorization server metadata (RFC 8414).
pub(super) async fn metadata(State(state): State<ServerState>) -> Json<Value> {
    let config = &state.config;
    Json(json!({
        "issuer": config.issuer,
        "authorization_endpoint": config.endpoint(AUTHORIZE_PATH),
        "token_endpoint": config.endpoint(TOKEN_PATH),
        "registration_endpoint": config.endpoint(REGISTER_PATH),
        "revocation_endpoint": config.endpoint(REVOKE_PATH),
        // `offline_access` is listed because some clients ask for it to get a
        // refresh token; every grant includes one regardless.
        "scopes_supported": [OAUTH_SCOPE, "offline_access"],
        "response_types_supported": ["code"],
        "response_modes_supported": ["query"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "revocation_endpoint_auth_methods_supported": ["none"],
        "client_id_metadata_document_supported": true,
        "authorization_response_iss_parameter_supported": true,
    }))
}
