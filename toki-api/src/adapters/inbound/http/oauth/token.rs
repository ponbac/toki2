use axum::{
    extract::{FromRequest, Request, State},
    http::{header::AUTHORIZATION, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use super::{
    invalid_client, no_store, oauth_error,
    params::{Form, ParamError, Params},
    ServerState,
};
use crate::domain::{
    models::{CodeExchange, IssuedTokens, TokenRefresh, OAUTH_SCOPE},
    OAuthError,
};

/// Parameters by which a client would authenticate. Every client is public,
/// so a client that sends credentials expects a check that would not happen.
const CLIENT_AUTHENTICATION_PARAMS: [&str; 3] =
    ["client_secret", "client_assertion", "client_assertion_type"];

#[derive(Debug, Serialize)]
struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: i64,
    refresh_token: String,
    scope: &'static str,
}

impl From<IssuedTokens> for TokenResponse {
    fn from(tokens: IssuedTokens) -> Self {
        Self {
            access_token: tokens.access_token.as_str().to_string(),
            token_type: "Bearer",
            expires_in: tokens.expires_in.whole_seconds(),
            refresh_token: tokens.refresh_token.as_str().to_string(),
            scope: OAUTH_SCOPE,
        }
    }
}

/// `POST /oauth/token`: the `authorization_code` and `refresh_token` grants.
/// Every refusal is logged with its reason: a client acts on it silently, often
/// by dropping its connection.
pub(super) async fn token(
    State(state): State<ServerState>,
    form: Result<PublicClientForm, PublicClientFormRejection>,
) -> Response {
    let PublicClientForm(mut params) = match form {
        Ok(form) => form,
        Err(rejection) => {
            tracing::info!("OAuth token request refused: {rejection}");
            return rejection.into_response();
        }
    };
    let grant_type = params.take("grant_type");
    let client_id = params.take("client_id");

    match issue_tokens(&state, grant_type.as_deref(), client_id.clone(), params).await {
        Ok(tokens) => no_store(Json(TokenResponse::from(tokens)).into_response()),
        Err(refusal) => {
            if !matches!(refusal, TokenRefusal::Error(OAuthError::Storage(_))) {
                tracing::info!(
                    grant_type = grant_type.as_deref(),
                    client_id = client_id.as_deref(),
                    "OAuth token request refused: {refusal}"
                );
            }
            refusal.into_response()
        }
    }
}

async fn issue_tokens(
    state: &ServerState,
    grant_type: Option<&str>,
    client_id: Option<String>,
    mut params: Params,
) -> Result<IssuedTokens, TokenRefusal> {
    let missing_client_id = || OAuthError::InvalidClient("client_id is required.".to_string());
    Ok(match grant_type {
        Some("authorization_code") => {
            let client_id = client_id.ok_or_else(missing_client_id)?;
            let (Some(code), Some(redirect_uri), Some(code_verifier)) = (
                params.take("code"),
                params.take("redirect_uri"),
                params.take("code_verifier"),
            ) else {
                return Err(OAuthError::InvalidRequest(
                    "code, redirect_uri and code_verifier are required.".to_string(),
                )
                .into());
            };
            let exchange = CodeExchange {
                code,
                redirect_uri,
                client_id,
                code_verifier,
                resource: params.take("resource"),
            };
            state.service.exchange_code(exchange).await?
        }
        Some("refresh_token") => {
            let client_id = client_id.ok_or_else(missing_client_id)?;
            let Some(refresh_token) = params.take("refresh_token") else {
                return Err(
                    OAuthError::InvalidRequest("refresh_token is required.".to_string()).into(),
                );
            };
            // `scope` may only narrow a refresh, and there is just one scope.
            let refresh = TokenRefresh {
                refresh_token,
                client_id,
                resource: params.take("resource"),
            };
            state.service.refresh(refresh).await?
        }
        Some(_) => return Err(TokenRefusal::UnsupportedGrantType),
        None => {
            return Err(OAuthError::InvalidRequest("grant_type is required.".to_string()).into())
        }
    })
}

/// Why the token endpoint refuses a request.
#[derive(Debug, thiserror::Error)]
enum TokenRefusal {
    #[error("Only the authorization_code and refresh_token grants are supported.")]
    UnsupportedGrantType,
    #[error(transparent)]
    Error(#[from] OAuthError),
}

impl IntoResponse for TokenRefusal {
    fn into_response(self) -> Response {
        match self {
            Self::UnsupportedGrantType => oauth_error(
                StatusCode::BAD_REQUEST,
                "unsupported_grant_type",
                &self.to_string(),
            ),
            Self::Error(error) => token_error(error),
        }
    }
}

/// `POST /oauth/revoke`: revokes the connection a token belongs to.
pub(super) async fn revoke(
    State(state): State<ServerState>,
    PublicClientForm(mut params): PublicClientForm,
) -> Response {
    let Some(token) = params.take("token") else {
        return invalid_request("token is required.");
    };
    let client_id = params.take("client_id");

    match state
        .service
        .revoke_token(&token, client_id.as_deref())
        .await
    {
        Ok(()) => no_store(StatusCode::OK.into_response()),
        // A client retries a revocation that may not have happened.
        Err(OAuthError::Storage(message)) => {
            tracing::error!("Failed to revoke an OAuth token: {message}");
            oauth_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "server_error",
                "The token could not be revoked. Try again later.",
            )
        }
        Err(error) => token_error(error),
    }
}

/// The form of a request to the token or revocation endpoint, refused when it
/// is malformed or the client tries to authenticate.
pub(super) struct PublicClientForm(Params);

#[derive(Debug, thiserror::Error)]
pub(super) enum PublicClientFormRejection {
    #[error(transparent)]
    Malformed(ParamError),
    /// The client sent credentials, by `Basic` authentication or otherwise.
    #[error("Toki's clients are public and must not authenticate.")]
    Authenticating { basic: bool },
}

impl IntoResponse for PublicClientFormRejection {
    fn into_response(self) -> Response {
        match self {
            Self::Malformed(error) => invalid_request(&error.to_string()),
            Self::Authenticating { basic } => invalid_client(&self.to_string(), basic),
        }
    }
}

impl<S> FromRequest<S> for PublicClientForm
where
    S: Send + Sync,
{
    type Rejection = PublicClientFormRejection;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        if let Some(authorization) = request.headers().get(AUTHORIZATION) {
            let basic = authorization
                .as_bytes()
                .get(..6)
                .is_some_and(|scheme| scheme.eq_ignore_ascii_case(b"basic "));
            return Err(PublicClientFormRejection::Authenticating { basic });
        }
        let Form(params) = Form::from_request(request, state)
            .await
            .map_err(PublicClientFormRejection::Malformed)?;
        if CLIENT_AUTHENTICATION_PARAMS
            .iter()
            .any(|name| params.contains(name))
        {
            return Err(PublicClientFormRejection::Authenticating { basic: false });
        }
        Ok(Self(params))
    }
}

fn token_error(error: OAuthError) -> Response {
    match error {
        OAuthError::InvalidRequest(message) => invalid_request(&message),
        OAuthError::InvalidClient(message) => invalid_client(&message, false),
        OAuthError::InvalidGrant(message) => {
            oauth_error(StatusCode::BAD_REQUEST, "invalid_grant", &message)
        }
        OAuthError::Storage(message) => {
            tracing::error!("OAuth token request failed: {message}");
            oauth_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "The request could not be completed.",
            )
        }
        error => invalid_request(&error.to_string()),
    }
}

fn invalid_request(description: &str) -> Response {
    oauth_error(StatusCode::BAD_REQUEST, "invalid_request", description)
}
