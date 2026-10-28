//! A local stand-in for GitHub's REST API, shared by the GitHub adapter's
//! tests: it mints installation tokens for valid JWTs only, answers seats and
//! profiles from fixtures, and answers billing usage from responses a test
//! registers.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, OnceLock,
    },
};

use aws_lc_rs::{
    encoding::AsDer,
    rsa::KeySize,
    signature::{KeyPair, RsaKeyPair, UnparsedPublicKey, RSA_PKCS1_2048_8192_SHA256},
};
use axum::{
    extract::{OriginalUri, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};

use super::GithubAppCredentials;

pub(super) const SEATS_PAGE_1: &str = include_str!("fixtures/seats_page_1.json");
pub(super) const SEATS_PAGE_2: &str = include_str!("fixtures/seats_page_2.json");
pub(super) const SEATS_PAGE_3: &str = include_str!("fixtures/seats_page_3.json");
pub(super) const USER_1001: &str = include_str!("fixtures/user_1001.json");
pub(super) const CLIENT_ID: &str = "Iv23liSyntheticClient";
pub(super) const INSTALLATION_ID: u64 = 4242;

/// A key generated for the tests: its PKCS#8 DER and its public key.
pub(super) struct TestKey {
    pkcs8: Vec<u8>,
    pub(super) public_key: Vec<u8>,
}

pub(super) fn test_key() -> &'static TestKey {
    static KEY: OnceLock<TestKey> = OnceLock::new();
    KEY.get_or_init(|| {
        let key = RsaKeyPair::generate(KeySize::Rsa2048).unwrap();
        TestKey {
            pkcs8: key.as_der().unwrap().as_ref().to_vec(),
            public_key: key.public_key().as_ref().to_vec(),
        }
    })
}

/// The key as PKCS#8 PEM (`PRIVATE KEY`).
pub(super) fn pkcs8_pem() -> String {
    pem::encode(&pem::Pem::new("PRIVATE KEY", test_key().pkcs8.clone()))
}

/// The key as PKCS#1 PEM (`RSA PRIVATE KEY`), the form GitHub generates: the
/// RSAPrivateKey inside the PKCS#8 structure, after its 26-byte prefix.
pub(super) fn pkcs1_pem() -> String {
    let pkcs8 = &test_key().pkcs8;
    // version, the rsaEncryption algorithm, then an OCTET STRING with a
    // two-byte length.
    assert_eq!(pkcs8[22..24], [0x04, 0x82], "unexpected PKCS#8 layout");
    pem::encode(&pem::Pem::new("RSA PRIVATE KEY", pkcs8[26..].to_vec()))
}

/// A status and headers every mint answers with.
pub(super) type MintFailure = (StatusCode, Vec<(&'static str, String)>);

/// What the fake GitHub received with a GET.
#[derive(Debug, Clone)]
pub(super) struct Received {
    pub(super) path: String,
    pub(super) user_agent: String,
    pub(super) accept: String,
    pub(super) api_version: String,
    pub(super) authorization: String,
    /// When the fake received it, to check request pacing.
    pub(super) arrived: std::time::Instant,
}

/// A registered billing answer: status, extra headers and body.
pub(super) type BillingAnswer = (StatusCode, Vec<(&'static str, String)>, String);

/// A local stand-in for GitHub's REST API. It mints installation tokens for
/// valid JWTs only, answers seats by organization (`example-org` from the
/// fixtures, the others with the failure they are named after), and answers
/// billing usage by exact path and query from `billing`, else `404`.
pub(super) struct FakeGithub {
    pub(super) token_lifetime: Mutex<time::Duration>,
    pub(super) mints: AtomicUsize,
    /// Tokens answered with `401`.
    pub(super) revoked: Mutex<HashSet<String>>,
    pub(super) revoke_every_token: Mutex<bool>,
    /// A failure every mint answers with.
    pub(super) mint_failure: Mutex<Option<MintFailure>>,
    pub(super) jwt_problems: Mutex<Vec<String>>,
    pub(super) received: Mutex<Vec<Received>>,
    pub(super) billing: Mutex<HashMap<String, BillingAnswer>>,
}

impl FakeGithub {
    pub(super) fn received(&self) -> Vec<Received> {
        self.received.lock().unwrap().clone()
    }

    pub(super) fn paths(&self) -> Vec<String> {
        self.received()
            .into_iter()
            .map(|received| received.path)
            .collect()
    }

    pub(super) fn mints(&self) -> usize {
        self.mints.load(Ordering::SeqCst)
    }

    /// Answers `GET {path_and_query}` with `body`.
    pub(super) fn answer(&self, path_and_query: &str, body: &str) {
        self.answer_with(path_and_query, StatusCode::OK, Vec::new(), body);
    }

    pub(super) fn answer_with(
        &self,
        path_and_query: &str,
        status: StatusCode,
        headers: Vec<(&'static str, String)>,
        body: &str,
    ) {
        self.billing.lock().unwrap().insert(
            path_and_query.to_string(),
            (status, headers, body.to_string()),
        );
    }
}

pub(super) fn header(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .map(|value| value.to_str().unwrap().to_string())
        .unwrap_or_default()
}

/// Checks a GitHub App JWT as GitHub does: RS256 with the app's key, issued by
/// its client ID, at most ten minutes long, and not issued in the future. A
/// token that cannot be decoded is a problem too, never a pass.
pub(super) fn check_jwt(authorization: &str) -> Result<(), String> {
    let jwt = authorization
        .strip_prefix("Bearer ")
        .ok_or("no bearer JWT")?;
    let parts: Vec<&str> = jwt.split('.').collect();
    let [header, claims, signature] = parts.as_slice() else {
        return Err("not three parts".to_string());
    };
    let decode = |part: &str| {
        URL_SAFE_NO_PAD
            .decode(part)
            .map_err(|_| format!("{part:?} is not base64url"))
    };
    let parse = |part: &str| {
        serde_json::from_slice::<Value>(&decode(part)?).map_err(|_| format!("{part:?} is not JSON"))
    };
    let header = parse(header)?;
    if header != json!({"alg": "RS256", "typ": "JWT"}) {
        return Err(format!("header {header}"));
    }
    UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, &test_key().public_key)
        .verify(
            format!("{}.{}", parts[0], parts[1]).as_bytes(),
            &decode(signature)?,
        )
        .map_err(|_| "bad signature".to_string())?;
    let claims = parse(claims)?;
    let claim_time = |name: &str| {
        claims[name]
            .as_i64()
            .ok_or_else(|| format!("no numeric {name}"))
    };
    let (iat, exp) = (claim_time("iat")?, claim_time("exp")?);
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    if claims["iss"] != CLIENT_ID {
        return Err(format!("issuer {}", claims["iss"]));
    }
    if exp - iat > 600 || iat > now || exp <= now {
        return Err(format!("times iat={iat} exp={exp} now={now}"));
    }
    Ok(())
}

pub(super) async fn mint(
    State(github): State<Arc<FakeGithub>>,
    Path(installation): Path<u64>,
    headers: HeaderMap,
) -> Response {
    if let Err(problem) = check_jwt(&header(&headers, "authorization")) {
        github.jwt_problems.lock().unwrap().push(problem);
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"message": "bad JWT"})),
        )
            .into_response();
    }
    if let Some((status, extra)) = github.mint_failure.lock().unwrap().clone() {
        let mut response =
            (status, Json(json!({"message": "synthetic mint failure"}))).into_response();
        for (name, value) in extra {
            response.headers_mut().insert(name, value.parse().unwrap());
        }
        return response;
    }
    if installation != INSTALLATION_ID {
        return (StatusCode::NOT_FOUND, Json(json!({"message": "Not Found"}))).into_response();
    }
    let number = github.mints.fetch_add(1, Ordering::SeqCst) + 1;
    let expires_at = time::OffsetDateTime::now_utc() + *github.token_lifetime.lock().unwrap();
    (
        StatusCode::CREATED,
        Json(json!({
            "token": format!("ghs_synthetic_{number}"),
            "expires_at": expires_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap(),
            "permissions": {"organization_copilot_seat_management": "read"},
            "repository_selection": "selected",
        })),
    )
        .into_response()
}

/// Records a GET and refuses revoked tokens.
pub(super) fn admit(github: &FakeGithub, path: String, headers: &HeaderMap) -> Option<Response> {
    let authorization = header(headers, "authorization");
    github.received.lock().unwrap().push(Received {
        path,
        user_agent: header(headers, "user-agent"),
        accept: header(headers, "accept"),
        api_version: header(headers, "x-github-api-version"),
        authorization: authorization.clone(),
        arrived: std::time::Instant::now(),
    });
    let token = authorization.trim_start_matches("Bearer ");
    let revoked = *github.revoke_every_token.lock().unwrap()
        || github.revoked.lock().unwrap().contains(token)
        || !token.starts_with("ghs_synthetic_");
    revoked.then(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"message": "Bad credentials"})),
        )
            .into_response()
    })
}

pub(super) async fn seats(
    State(github): State<Arc<FakeGithub>>,
    Path(organization): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let path = format!(
        "/orgs/{organization}/copilot/billing/seats?per_page={}&page={}",
        query["per_page"], query["page"]
    );
    if let Some(refused) = admit(&github, path, &headers) {
        return refused;
    }
    let message = |text: &str| Json(json!({"message": text}));
    let reset = (time::OffsetDateTime::now_utc().unix_timestamp() + 120).to_string();
    match organization.as_str() {
        "example-org" => {
            let body = match query["page"].as_str() {
                "1" => SEATS_PAGE_1,
                "2" => SEATS_PAGE_2,
                "3" => SEATS_PAGE_3,
                _ => r#"{"total_seats":5,"seats":[]}"#,
            };
            // A Link header that pagination by number must ignore.
            (
                [
                    ("content-type", "application/json"),
                    (
                        "link",
                        "<https://elsewhere.example/seats?page=99>; rel=\"next\"",
                    ),
                ],
                body,
            )
                .into_response()
        }
        "empty-org" => Json(json!({"total_seats": 0, "seats": []})).into_response(),
        // Exactly one full page of seats.
        "exact-org" => match query["page"].as_str() {
            "1" => {
                let mut page: Value = serde_json::from_str(SEATS_PAGE_1).unwrap();
                page["total_seats"] = json!(2);
                Json(page).into_response()
            }
            _ => Json(json!({"total_seats": 2, "seats": []})).into_response(),
        },
        "limited" => (
            StatusCode::FORBIDDEN,
            [
                ("x-ratelimit-remaining", "0".to_string()),
                ("x-ratelimit-reset", reset),
            ],
            message("API rate limit exceeded"),
        )
            .into_response(),
        "secondary" => (
            StatusCode::FORBIDDEN,
            [("retry-after", "30")],
            message("You have exceeded a secondary rate limit"),
        )
            .into_response(),
        "busy" => (StatusCode::TOO_MANY_REQUESTS, [("retry-after", "30")]).into_response(),
        "forbidden" => (
            StatusCode::FORBIDDEN,
            message("Resource not accessible by integration"),
        )
            .into_response(),
        "no-copilot" => (
            StatusCode::UNPROCESSABLE_ENTITY,
            message("Copilot Business is disabled"),
        )
            .into_response(),
        "down" => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        "garbled" => "not json".into_response(),
        _ => (StatusCode::NOT_FOUND, message("Not Found")).into_response(),
    }
}

pub(super) async fn user(
    State(github): State<Arc<FakeGithub>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Some(refused) = admit(&github, format!("/user/{id}"), &headers) {
        return refused;
    }
    match id.as_str() {
        "1001" => ([("content-type", "application/json")], USER_1001).into_response(),
        "1002" => Json(json!({"login": "grace-hopper", "id": 1002, "name": null})).into_response(),
        "1003" => Json(json!({"login": "someone-else", "id": 9999})).into_response(),
        _ => (StatusCode::NOT_FOUND, Json(json!({"message": "Not Found"}))).into_response(),
    }
}

/// `GET /organizations/...`: the registered answer for the exact path and
/// query, else `404`.
async fn billing(
    State(github): State<Arc<FakeGithub>>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Response {
    let path = uri
        .path_and_query()
        .map(|path| path.as_str().to_string())
        .unwrap_or_default();
    if let Some(refused) = admit(&github, path.clone(), &headers) {
        return refused;
    }
    match github.billing.lock().unwrap().get(&path).cloned() {
        Some((status, extra, body)) => {
            let mut response =
                (status, [("content-type", "application/json")], body).into_response();
            for (name, value) in extra {
                response.headers_mut().insert(name, value.parse().unwrap());
            }
            response
        }
        None => (StatusCode::NOT_FOUND, Json(json!({"message": "Not Found"}))).into_response(),
    }
}

pub(super) async fn fake_github() -> (String, Arc<FakeGithub>) {
    let github = Arc::new(FakeGithub {
        token_lifetime: Mutex::new(time::Duration::hours(1)),
        mints: AtomicUsize::new(0),
        revoked: Mutex::new(HashSet::new()),
        revoke_every_token: Mutex::new(false),
        mint_failure: Mutex::new(None),
        jwt_problems: Mutex::new(Vec::new()),
        received: Mutex::new(Vec::new()),
        billing: Mutex::new(HashMap::new()),
    });
    let app = Router::new()
        .route(
            "/app/installations/{installation}/access_tokens",
            post(mint),
        )
        .route("/orgs/{organization}/copilot/billing/seats", get(seats))
        .route("/user/{id}", get(user))
        .route("/organizations/{*rest}", get(billing))
        .with_state(github.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    (format!("http://{address}/"), github)
}

pub(super) fn credentials(
    api_base: &str,
    organization: &str,
    private_key_pem: String,
) -> GithubAppCredentials {
    GithubAppCredentials {
        organization: organization.to_string(),
        api_base: api_base.to_string(),
        client_id: CLIENT_ID.to_string(),
        installation_id: INSTALLATION_ID,
        private_key_pem,
    }
}
