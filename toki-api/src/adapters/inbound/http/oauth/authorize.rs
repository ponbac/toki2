use axum::{
    extract::{RawQuery, State},
    http::{
        header::{CACHE_CONTROL, ORIGIN},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::{IntoResponse, Redirect, Response},
};
use time::Duration;
use url::Url;

use super::{
    pages::{self, Consent},
    params::{Form, ParamError, Params},
    ServerState, AUTHORIZE_PATH,
};
use crate::{
    auth::{signed_in_within, start_sign_in, AuthSession, LoginContinuation},
    domain::{
        models::{
            AuthorizationOutcome, AuthorizationParams, AuthorizationResponse, RedirectTarget,
            RedirectUri,
        },
        AuthorizationError, OAuthError,
    },
};

/// Consent follows an Entra sign-in at most this old, so a connection never
/// starts from a session whose Entra account may since have been disabled.
const RECENT_SIGN_IN: Duration = Duration::minutes(15);
const SEC_FETCH_SITE: &str = "sec-fetch-site";

/// `GET /oauth/authorize`: validates the request, makes sure the user signed
/// in recently, and asks for consent.
pub(super) async fn authorize(
    State(state): State<ServerState>,
    auth_session: AuthSession,
    RawQuery(query): RawQuery,
) -> Response {
    let query = query.unwrap_or_default();
    let mut params = match Params::parse(query.as_bytes()) {
        Ok(params) => params,
        Err(error) => return invalid_request_page(&error),
    };
    let request = AuthorizationParams {
        client_id: params.take("client_id"),
        redirect_uri: params.take("redirect_uri"),
        response_type: params.take("response_type"),
        code_challenge: params.take("code_challenge"),
        code_challenge_method: params.take("code_challenge_method"),
        resource: params.take("resource"),
        state: params.take("state"),
    };

    let authorization = match state.service.validate_authorization(request).await {
        Ok(authorization) => authorization,
        Err(AuthorizationError::Unredirectable(error)) => return unredirectable_page(error),
        Err(AuthorizationError::Redirect(response)) => {
            return request_error(&state.config.issuer, &response)
        }
    };

    let user = match &auth_session.user {
        Some(user) if signed_in_within(&auth_session.session, RECENT_SIGN_IN).await => user,
        _ => {
            // The sign-in callback returns here, and the request is validated
            // again then.
            let continuation = LoginContinuation::Authorize(format!("{AUTHORIZE_PATH}?{query}"));
            return start_sign_in(&auth_session, continuation).await;
        }
    };

    let request = match state.service.begin_consent(&user.id, &authorization).await {
        Ok(request) => request,
        Err(error) => {
            tracing::error!("Failed to record an OAuth consent request: {error}");
            return server_error_page();
        }
    };

    pages::consent_page(&Consent {
        client: &authorization.client,
        email: &user.email,
        redirect_uri: &authorization.redirect_uri,
        registered_redirect_uri: &authorization.registered_redirect_uri,
        request: request.as_str(),
        form_action: &state.config.endpoint(AUTHORIZE_PATH),
    })
}

/// `POST /oauth/authorize`: the user's decision on the consent page.
pub(super) async fn decide(
    State(state): State<ServerState>,
    headers: HeaderMap,
    auth_session: AuthSession,
    form: Result<Form, ParamError>,
) -> Response {
    if !is_same_origin(&headers, &state.config.api_origin) {
        return pages::error_page(
            StatusCode::FORBIDDEN,
            "Request blocked",
            "This decision did not come from Toki's consent page. Start connecting again from \
             the app.",
        );
    }
    let Some(user) = auth_session.user else {
        return pages::error_page(
            StatusCode::UNAUTHORIZED,
            "Signed out",
            "You are no longer signed in to Toki. Start connecting again from the app.",
        );
    };
    let mut params = match form {
        Ok(Form(params)) => params,
        Err(error) => return invalid_request_page(&error),
    };
    let (Some(request), Some(allow)) = (
        params.take("request"),
        match params.take("decision").as_deref() {
            Some("allow") => Some(true),
            Some("deny") => Some(false),
            _ => None,
        },
    ) else {
        return pages::error_page(
            StatusCode::BAD_REQUEST,
            "Invalid request",
            "Choose Allow or Deny on the consent page.",
        );
    };

    match state.service.decide(&user.id, &request, allow).await {
        Ok(response) => authorization_redirect(&state.config.issuer, &response),
        Err(OAuthError::NotFound) => expired_page(),
        Err(OAuthError::Storage(message)) => {
            tracing::error!("Failed to decide an OAuth consent request: {message}");
            server_error_page()
        }
        Err(error) => pages::error_page_with_detail(
            StatusCode::BAD_REQUEST,
            "Invalid request",
            "Toki couldn't record your choice. Start connecting again from the app.",
            Some(&error.to_string()),
        ),
    }
}

/// Whether a decision was posted from Toki's own page. Browsers send
/// `Origin` with form posts, and most send `Sec-Fetch-Site`; a request
/// without them is allowed, as the session cookie is `SameSite=Lax` too.
fn is_same_origin(headers: &HeaderMap, api_origin: &str) -> bool {
    let origin_matches = headers.get(ORIGIN).is_none_or(|origin| {
        origin
            .to_str()
            .is_ok_and(|origin| origin.eq_ignore_ascii_case(api_origin.trim_end_matches('/')))
    });
    let site_matches = headers
        .get(SEC_FETCH_SITE)
        .is_none_or(|site| site.as_bytes() == b"same-origin");
    origin_matches && site_matches
}

/// Answers a request whose client and redirect URI were validated, but which
/// is otherwise invalid, before the user has signed in or seen anything.
///
/// An app on the user's computer gets the error at once, as OAuth clients
/// expect. A web redirect URI is only as trustworthy as the client that
/// registered it, which anyone can be, so the browser is not sent there
/// unasked: that would make Toki an open redirector (RFC 9700, section
/// 4.11.2). The page shows the error with a link back instead.
fn request_error(issuer: &str, response: &AuthorizationResponse) -> Response {
    let destination = RedirectUri::parse(&response.redirect_uri);
    let local = destination.as_ref().is_some_and(|uri| {
        matches!(
            uri.target(),
            RedirectTarget::Loopback | RedirectTarget::PrivateUseScheme
        )
    });
    if local {
        return authorization_redirect(issuer, response);
    }

    let (Some(destination), Some(location)) =
        (destination, authorization_redirect_url(issuer, response))
    else {
        tracing::error!("An OAuth redirect URI could not be parsed");
        return server_error_page();
    };
    let detail = match &response.outcome {
        AuthorizationOutcome::Error { code, description } => {
            format!("{}: {description}", code.as_str())
        }
        AuthorizationOutcome::Code(_) => String::new(),
    };
    pages::error_page_with_return(
        StatusCode::BAD_REQUEST,
        "This app's request can't be completed",
        "The app asked Toki for something it doesn't support. Make sure the app is up to date, \
         or connect it with a Toki API token instead.",
        &detail,
        &location,
        &destination.display_destination(),
    )
}

/// Sends the authorization response to the client's redirect URI, keeping any
/// query it has and naming this server as the issuer (RFC 9207).
fn authorization_redirect(issuer: &str, response: &AuthorizationResponse) -> Response {
    let Some(location) = authorization_redirect_url(issuer, response) else {
        tracing::error!("An OAuth redirect URI could not be parsed");
        return server_error_page();
    };
    let mut response = Redirect::to(&location).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn authorization_redirect_url(issuer: &str, response: &AuthorizationResponse) -> Option<String> {
    let mut url = Url::parse(&response.redirect_uri).ok()?;
    {
        let mut query = url.query_pairs_mut();
        match &response.outcome {
            AuthorizationOutcome::Code(code) => {
                query.append_pair("code", code.as_str());
            }
            AuthorizationOutcome::Error { code, description } => {
                query.append_pair("error", code.as_str());
                query.append_pair("error_description", description);
            }
        }
        if let Some(state) = &response.state {
            query.append_pair("state", state);
        }
        query.append_pair("iss", issuer);
    }
    Some(url.into())
}

fn unredirectable_page(error: OAuthError) -> Response {
    match error {
        OAuthError::Storage(message) => {
            tracing::error!("Failed to validate an OAuth authorization request: {message}");
            server_error_page()
        }
        error => pages::error_page_with_detail(
            StatusCode::BAD_REQUEST,
            "This app can't connect to Toki",
            "Toki can't accept the app's sign-in request. If you set the app up with a client ID \
             or secret for Toki, remove Toki from it and add it again with just its URL. \
             Otherwise make sure the app is up to date, or connect it with a Toki API token.",
            Some(&error.to_string()),
        ),
    }
}

fn invalid_request_page(error: &ParamError) -> Response {
    pages::error_page_with_detail(
        StatusCode::BAD_REQUEST,
        "Invalid request",
        "This sign-in link is malformed. Start connecting again from the app.",
        Some(&error.to_string()),
    )
}

fn expired_page() -> Response {
    pages::error_page(
        StatusCode::BAD_REQUEST,
        "This request has expired",
        "This connection request expired or was already answered. Start connecting again from \
         the app.",
    )
}

fn server_error_page() -> Response {
    pages::error_page(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Something went wrong",
        "Toki could not complete this request. Try again in a moment.",
    )
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;
    use crate::domain::models::{AuthorizationErrorCode, OAuthSecret, OAuthSecretKind};

    const ISSUER: &str = "https://toki.example";

    fn query_pairs(location: &str) -> Vec<(String, String)> {
        Url::parse(location)
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect()
    }

    #[test]
    fn redirects_carry_the_code_state_and_issuer_after_an_existing_query() {
        let code = OAuthSecret::generate(OAuthSecretKind::AuthorizationCode);
        let location = authorization_redirect_url(
            ISSUER,
            &AuthorizationResponse {
                redirect_uri: "https://app.example/cb?tenant=a%20b".to_string(),
                state: Some("s&t=1".to_string()),
                outcome: AuthorizationOutcome::Code(code.clone()),
            },
        )
        .unwrap();

        assert!(location.starts_with("https://app.example/cb?tenant=a%20b&code="));
        assert_eq!(
            query_pairs(&location),
            [
                ("tenant".to_string(), "a b".to_string()),
                ("code".to_string(), code.as_str().to_string()),
                ("state".to_string(), "s&t=1".to_string()),
                ("iss".to_string(), ISSUER.to_string()),
            ]
        );
    }

    #[test]
    fn error_redirects_carry_the_error_and_omit_an_unsent_state() {
        let location = authorization_redirect_url(
            ISSUER,
            &AuthorizationResponse {
                redirect_uri: "cursor://anysphere.cursor-mcp/oauth/callback".to_string(),
                state: None,
                outcome: AuthorizationOutcome::Error {
                    code: AuthorizationErrorCode::AccessDenied,
                    description: "The user denied access.".to_string(),
                },
            },
        )
        .unwrap();

        assert!(location.starts_with("cursor://anysphere.cursor-mcp/oauth/callback?"));
        assert_eq!(
            query_pairs(&location),
            [
                ("error".to_string(), "access_denied".to_string()),
                (
                    "error_description".to_string(),
                    "The user denied access.".to_string()
                ),
                ("iss".to_string(), ISSUER.to_string()),
            ]
        );
    }

    #[test]
    fn decisions_must_come_from_the_api_origin() {
        let headers = |pairs: &[(&'static str, &'static str)]| {
            let mut headers = HeaderMap::new();
            for (name, value) in pairs {
                headers.insert(*name, HeaderValue::from_static(value));
            }
            headers
        };
        let origin = "https://toki.example";

        assert!(is_same_origin(&headers(&[]), origin));
        assert!(is_same_origin(
            &headers(&[
                ("origin", "https://toki.example"),
                ("sec-fetch-site", "same-origin")
            ]),
            "https://toki.example/"
        ));
        assert!(!is_same_origin(
            &headers(&[("origin", "https://evil.example")]),
            origin
        ));
        assert!(!is_same_origin(&headers(&[("origin", "null")]), origin));
        assert!(!is_same_origin(
            &headers(&[("sec-fetch-site", "same-site")]),
            origin
        ));
        assert!(!is_same_origin(
            &headers(&[
                ("origin", "https://toki.example"),
                ("sec-fetch-site", "cross-site")
            ]),
            origin
        ));
    }
}
