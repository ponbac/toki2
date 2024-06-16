use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use base64::prelude::*;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use reqwest::multipart::Form;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::time::SystemTime;
use thiserror::Error;

#[derive(Debug, Serialize)]
pub struct Credentials {
    pub username: Option<String>,
    pub csrf_token: String,
    pub session_id: String,
    pub user_id: String,
    pub valid_until: Option<SystemTime>,
}

#[derive(Error, Debug)]
pub enum IntoCredentialsError {
    #[error("Missing CSRF token")]
    MissingCSRFToken,
    #[error("Missing session id")]
    MissingSessionId,
    #[error("Expired session id")]
    ExpiredSessionId,
}

static COOKIE_PREFIX: &str = "mt";

impl TryFrom<CookieJar> for Credentials {
    type Error = IntoCredentialsError;

    fn try_from(jar: CookieJar) -> Result<Credentials, Self::Error> {
        let milltime_credentials = Credentials {
            username: None,
            csrf_token: if let Some(c) = jar.get(&format!("{COOKIE_PREFIX}_CSRFToken")) {
                c.value().to_string()
            } else {
                return Err(IntoCredentialsError::MissingCSRFToken);
            },
            session_id: if let Some(c) = jar.get(&format!("{COOKIE_PREFIX}_milltimesessionid")) {
                c.value().to_string()
            } else {
                return Err(IntoCredentialsError::MissingSessionId);
            },
            user_id: if let Some(c) = jar.get(&format!("{COOKIE_PREFIX}_milltimesessionid")) {
                let (_, jwt_payload) =
                    decode_session_id(c.value()).expect("Failed to decode session id");
                jwt_payload.userid
            } else {
                return Err(IntoCredentialsError::MissingSessionId);
            },
            valid_until: if let Some(c) = jar.get(&format!("{COOKIE_PREFIX}_milltimesessionid")) {
                c.expires_datetime().map(|t| t.into())
            } else {
                None
            },
        };

        if let Some(valid_until) = milltime_credentials.valid_until {
            if valid_until < SystemTime::now() {
                return Err(IntoCredentialsError::ExpiredSessionId);
            }
        }

        Ok(milltime_credentials)
    }
}

impl Credentials {
    pub async fn new(username: &str, password: &str) -> Result<Credentials, Box<dyn Error>> {
        let login_url = env::var("MILLTIME_URL").expect("MILLTIME_URL must be set") + "/api/login";

        let client = Client::new();

        let form = Form::new()
            .text("userlogin", username.to_string())
            .text("password", password.to_string())
            .text("instanceid", "000224.1".to_string());

        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/108.0.0.0 Safari/537.36"));

        let csrf_token = get_csrf_token().await?;
        headers.insert("X-CSRF-Token", HeaderValue::from_str(&csrf_token)?);
        headers.insert(
            "Cookie",
            HeaderValue::from_str(&format!("CSRFToken={}; Secure; SameSite=Lax", csrf_token))?,
        );

        let resp = client
            .post(&login_url)
            .headers(headers)
            .multipart(form)
            .send()
            .await?;

        let session_id_cookie = resp
            .cookies()
            .find(|c| c.name() == "milltimesessionid")
            .ok_or("milltimesessionid cookie not found")?;
        let session_id = session_id_cookie.value().to_string();
        let (_, jwt_payload) = decode_session_id(&session_id)?;
        let valid_until = session_id_cookie.expires();

        Ok(Credentials {
            username: Some(username.to_string()),
            csrf_token,
            session_id,
            user_id: jwt_payload.userid,
            valid_until,
        })
    }

    pub fn auth_cookies(&self, domain: String) -> Vec<Cookie<'static>> {
        vec![
            Cookie::build((
                format!("{COOKIE_PREFIX}_CSRFToken"),
                self.csrf_token.clone(),
            ))
            .expires(self.valid_until.map(|t| t.into()))
            .same_site(SameSite::Lax)
            .path("/")
            .secure(true)
            // .domain(domain.clone())
            .build(),
            Cookie::build((
                format!("{COOKIE_PREFIX}_milltimeinstanceid"),
                "000224.1".to_string(),
            ))
            .expires(self.valid_until.map(|t| t.into()))
            .same_site(SameSite::Lax)
            .path("/")
            .secure(false)
            // .domain(domain.clone())
            .build(),
            Cookie::build((
                format!("{COOKIE_PREFIX}_milltimesessionid"),
                self.session_id.clone(),
            ))
            .expires(self.valid_until.map(|t| t.into()))
            .same_site(SameSite::Lax)
            .path("/")
            .secure(true)
            // .domain(domain)
            .build(),
        ]
    }

    pub fn auth_cookies_str(&self) -> Vec<String> {
        vec![
            format!("CSRFToken={}; Secure; SameSite=Lax", self.csrf_token),
            "milltimeinstanceid=000224.1; SameSite=Lax".to_string(),
            format!(
                "milltimesessionid={}; Secure; SameSite=Lax", // TODO: Expires!
                self.session_id
            ),
        ]
    }

    pub fn as_cookie_header(&self) -> String {
        self.auth_cookies_str().join("; ")
    }
}

async fn get_csrf_token() -> Result<String, Box<dyn Error>> {
    let csrf_url = env::var("MILLTIME_URL").expect("MILLTIME_URL must be set") + "/api/login";

    let client = Client::new();

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/108.0.0.0 Safari/537.36"));

    let resp = client.post(&csrf_url).headers(headers).send().await?;

    // Retrieve CSRF token from the cookies iterator
    if let Some(c) = resp.cookies().find(|c| c.name() == "CSRFToken") {
        return Ok(c.value().to_string());
    }

    Err("CSRFToken cookie not found".into())
}

fn decode_session_id(session_id: &str) -> Result<(JWTHeader, JWTPayload), Box<dyn Error>> {
    let parts = session_id.split('.').take(2).collect::<Vec<&str>>();

    let decoded_jwt_header = BASE64_STANDARD_NO_PAD.decode(parts[0])?;
    let decoded_jwt_payload = BASE64_STANDARD_NO_PAD.decode(parts[1])?;
    let jwt_header: JWTHeader = serde_json::from_slice(&decoded_jwt_header)?;
    let jwt_payload: JWTPayload = serde_json::from_slice(&decoded_jwt_payload)?;

    Ok((jwt_header, jwt_payload))
}

#[derive(Deserialize)]
struct JWTHeader {
    alg: String,
    sub: String,
    typ: String,
}

#[derive(Deserialize)]
struct JWTPayload {
    exp: u64,
    iat: u64,
    instancedb: String,
    instanceid: String,
    instancename: String,
    ruserid: String,
    sid: String,
    sub: String,
    userid: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_session_id() {
        let session_id = "eyJhbGciOiJIUzUxMiIsInN1YiI6IjEwNCIsInR5cCI6IkpXVCJ9.eyJleHAiOjE3MjA3OTYwOTcsImlhdCI6MTcxODIwNDA5NywiaW5zdGFuY2VkYiI6Im1pbGx0aW1lX3NwaW5pdCIsImluc3RhbmNlaWQiOiIwMDAyMjQuMSIsImluc3RhbmNlbmFtZSI6IlNwaW5pdCIsInJ1c2VyaWQiOiIxMDQiLCJzaWQiOiIzNjRhZWI2MzY0ZjgzYjY5YWIyOWM2YjBkZTJhNTMyNjQxYjEiLCJzdWIiOiIxMDQiLCJ1c2VyaWQiOiIxMDQifQ.NPtN42xmLJF6wKZskup3sxN81jNkXuYDatrgCl0gIhOsvSv-cKpQcIesbxTKlCgrFqoqBy27iB5eOjJoR_rDkg";
        let (jwt_header, jwt_payload) = decode_session_id(session_id).unwrap();

        assert_eq!(jwt_header.alg, "HS512");
        assert_eq!(jwt_header.sub, "104");
        assert_eq!(jwt_payload.userid, "104");
    }
}
