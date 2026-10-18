//! Strict parsing of OAuth parameters. RFC 6749 forbids sending a parameter
//! more than once; refusing repeats, rather than picking one, keeps a request
//! from meaning one thing here and another to the client that built it.

use std::collections::HashMap;

use axum::{
    body::{Body, Bytes},
    extract::{FromRequest, Request},
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use url::form_urlencoded;

/// The largest form or JSON body an OAuth endpoint reads.
pub(super) const MAX_BODY_BYTES: usize = 16 * 1024;
/// The longest parameter name or value accepted.
const MAX_VALUE_BYTES: usize = 2 * 1024;
const FORM_CONTENT_TYPE: &str = "application/x-www-form-urlencoded";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(super) enum ParamError {
    #[error("The request must be sent as application/x-www-form-urlencoded.")]
    UnsupportedContentType,
    #[error("The request body is too large or could not be read.")]
    UnreadableBody,
    #[error("The parameter {0} is sent more than once.")]
    Repeated(String),
    #[error("The parameter {0} is too long.")]
    TooLong(String),
}

impl IntoResponse for ParamError {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, self.to_string()).into_response()
    }
}

/// Form or query parameters, each sent at most once.
#[derive(Debug, Default)]
pub(super) struct Params(HashMap<String, String>);

impl Params {
    pub(super) fn parse(raw: &[u8]) -> Result<Self, ParamError> {
        let mut params = HashMap::new();
        for (name, value) in form_urlencoded::parse(raw) {
            if name.len() > MAX_VALUE_BYTES {
                return Err(ParamError::TooLong("name".to_string()));
            }
            if value.len() > MAX_VALUE_BYTES {
                return Err(ParamError::TooLong(name.into_owned()));
            }
            if params.contains_key(name.as_ref()) {
                return Err(ParamError::Repeated(name.into_owned()));
            }
            params.insert(name.into_owned(), value.into_owned());
        }
        Ok(Self(params))
    }

    /// Removes and returns `name`'s value. An empty value counts as absent
    /// (RFC 6749, section 3.1).
    pub(super) fn take(&mut self, name: &str) -> Option<String> {
        self.0.remove(name).filter(|value| !value.is_empty())
    }

    pub(super) fn contains(&self, name: &str) -> bool {
        self.0.get(name).is_some_and(|value| !value.is_empty())
    }
}

/// A form-encoded request body, parsed strictly.
pub(super) struct Form(pub Params);

impl<S> FromRequest<S> for Form
where
    S: Send + Sync,
{
    type Rejection = ParamError;

    async fn from_request(request: Request, _state: &S) -> Result<Self, Self::Rejection> {
        if !is_form(request.headers()) {
            return Err(ParamError::UnsupportedContentType);
        }
        let body = read_body(request.into_body()).await?;
        Params::parse(&body).map(Self)
    }
}

/// Reads at most [`MAX_BODY_BYTES`] of `body`.
pub(super) async fn read_body(body: Body) -> Result<Bytes, ParamError> {
    axum::body::to_bytes(body, MAX_BODY_BYTES)
        .await
        .map_err(|_| ParamError::UnreadableBody)
}

fn is_form(headers: &HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case(FORM_CONTENT_TYPE))
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn parameters_are_decoded_and_empty_values_count_as_absent() {
        let mut params =
            Params::parse(b"client_id=https%3A%2F%2Fclaude.ai%2Fcb&scope=mcp+offline&state=")
                .unwrap();

        assert_eq!(
            params.take("client_id").as_deref(),
            Some("https://claude.ai/cb")
        );
        assert_eq!(params.take("scope").as_deref(), Some("mcp offline"));
        assert!(!params.contains("state"));
        assert_eq!(params.take("state"), None);
        assert_eq!(params.take("missing"), None);
    }

    #[test]
    fn repeated_parameters_are_rejected() {
        assert_eq!(
            Params::parse(b"state=a&client_id=x&state=b").unwrap_err(),
            ParamError::Repeated("state".to_string())
        );
        assert_eq!(
            Params::parse(b"state=&state=b").unwrap_err(),
            ParamError::Repeated("state".to_string())
        );
    }

    #[test]
    fn values_and_names_over_two_kilobytes_are_rejected() {
        let at_limit = format!("state={}", "a".repeat(MAX_VALUE_BYTES));
        assert!(Params::parse(at_limit.as_bytes()).is_ok());

        let long_value = format!("state={}", "a".repeat(MAX_VALUE_BYTES + 1));
        assert_eq!(
            Params::parse(long_value.as_bytes()).unwrap_err(),
            ParamError::TooLong("state".to_string())
        );

        // Percent-encoding does not count against the limit; the decoded value does.
        let encoded = format!("state={}", "%41".repeat(MAX_VALUE_BYTES));
        assert!(Params::parse(encoded.as_bytes()).is_ok());

        let long_name = format!("{}=1", "n".repeat(MAX_VALUE_BYTES + 1));
        assert_eq!(
            Params::parse(long_name.as_bytes()).unwrap_err(),
            ParamError::TooLong("name".to_string())
        );
    }

    #[test]
    fn only_form_content_types_are_forms() {
        let headers = |value: &'static str| {
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static(value));
            headers
        };

        assert!(is_form(&headers("application/x-www-form-urlencoded")));
        assert!(is_form(&headers(
            "Application/X-WWW-Form-Urlencoded; charset=UTF-8"
        )));
        assert!(!is_form(&headers("application/json")));
        assert!(!is_form(&headers("multipart/form-data; boundary=x")));
        assert!(!is_form(&headers("text/plain")));
        assert!(!is_form(&HeaderMap::new()));
    }

    #[tokio::test]
    async fn form_bodies_over_the_limit_are_rejected() {
        let request = |body: Vec<u8>| {
            Request::builder()
                .header(CONTENT_TYPE, FORM_CONTENT_TYPE)
                .body(Body::from(body))
                .unwrap()
        };

        let mut fits = b"a=".to_vec();
        fits.resize(MAX_BODY_BYTES, b'&');
        assert!(Form::from_request(request(fits), &()).await.is_ok());

        let mut too_large = b"a=".to_vec();
        too_large.resize(MAX_BODY_BYTES + 1, b'&');
        assert_eq!(
            Form::from_request(request(too_large), &()).await.err(),
            Some(ParamError::UnreadableBody)
        );
    }
}
