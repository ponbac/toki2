use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, HeaderMap},
};

#[derive(Debug, Default)]
pub struct ClientHints {
    pub mobile: Option<String>,
    pub platform: Option<String>,
    pub ua_full_version: Option<String>,
    pub user_agent: Option<String>,
}

impl ClientHints {
    /// Returns a string that identifies the client.
    ///
    /// Concatenates the `UA full version`, `platform`, and `mobile` values.
    /// If no UA information is available, it returns the `User-Agent` header.
    pub fn identifier(&self) -> String {
        if self.mobile.is_some() || self.platform.is_some() || self.ua_full_version.is_some() {
            format!(
                "[{}]-[{}]-[{}]",
                self.ua_full_version.as_deref().unwrap_or_default(),
                self.platform.as_deref().unwrap_or_default(),
                self.mobile.as_deref().unwrap_or_default()
            )
        } else {
            self.user_agent.clone().unwrap_or_default()
        }
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for ClientHints
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let headers = &parts.headers;

        Ok(ClientHints {
            mobile: get_string_header(headers, "Sec-Ch-Ua-Mobile"),
            platform: get_string_header(headers, "Sec-Ch-Ua-Platform"),
            ua_full_version: get_string_header(headers, "Sec-Ch-Ua"),
            user_agent: get_string_header(headers, "User-Agent"),
        })
    }
}

fn get_string_header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(String::from)
}
