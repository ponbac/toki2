use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Duration,
};

use aws_lc_rs::{
    encoding::AsDer,
    rsa::KeySize,
    signature::{KeyPair, RsaKeyPair, UnparsedPublicKey, RSA_PKCS1_2048_8192_SHA256},
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};
use time::macros::{date, datetime};

use super::{app_auth::parse_private_key, *};

const SEATS_PAGE_1: &str = include_str!("fixtures/seats_page_1.json");
const SEATS_PAGE_2: &str = include_str!("fixtures/seats_page_2.json");
const SEATS_PAGE_3: &str = include_str!("fixtures/seats_page_3.json");
const USER_1001: &str = include_str!("fixtures/user_1001.json");
const CLIENT_ID: &str = "Iv23liSyntheticClient";
const INSTALLATION_ID: u64 = 4242;

/// A key generated for the tests: its PKCS#8 DER and its public key.
struct TestKey {
    pkcs8: Vec<u8>,
    public_key: Vec<u8>,
}

fn test_key() -> &'static TestKey {
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
fn pkcs8_pem() -> String {
    pem::encode(&pem::Pem::new("PRIVATE KEY", test_key().pkcs8.clone()))
}

/// The key as PKCS#1 PEM (`RSA PRIVATE KEY`), the form GitHub generates: the
/// RSAPrivateKey inside the PKCS#8 structure, after its 26-byte prefix.
fn pkcs1_pem() -> String {
    let pkcs8 = &test_key().pkcs8;
    // version, the rsaEncryption algorithm, then an OCTET STRING with a
    // two-byte length.
    assert_eq!(pkcs8[22..24], [0x04, 0x82], "unexpected PKCS#8 layout");
    pem::encode(&pem::Pem::new("RSA PRIVATE KEY", pkcs8[26..].to_vec()))
}

/// A status and headers every mint answers with.
type MintFailure = (StatusCode, Vec<(&'static str, String)>);

/// What the fake GitHub received with a GET.
#[derive(Debug, Clone)]
struct Received {
    path: String,
    user_agent: String,
    accept: String,
    api_version: String,
    authorization: String,
}

/// A local stand-in for GitHub's REST API. It mints installation tokens for
/// valid JWTs only, and answers seats by organization: `example-org` from the
/// fixtures, the others with the failure they are named after.
struct FakeGithub {
    token_lifetime: Mutex<time::Duration>,
    mints: AtomicUsize,
    /// Tokens answered with `401`.
    revoked: Mutex<HashSet<String>>,
    revoke_every_token: Mutex<bool>,
    /// A failure every mint answers with.
    mint_failure: Mutex<Option<MintFailure>>,
    jwt_problems: Mutex<Vec<String>>,
    received: Mutex<Vec<Received>>,
}

impl FakeGithub {
    fn received(&self) -> Vec<Received> {
        self.received.lock().unwrap().clone()
    }

    fn paths(&self) -> Vec<String> {
        self.received()
            .into_iter()
            .map(|received| received.path)
            .collect()
    }

    fn mints(&self) -> usize {
        self.mints.load(Ordering::SeqCst)
    }
}

fn header(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .map(|value| value.to_str().unwrap().to_string())
        .unwrap_or_default()
}

/// Checks a GitHub App JWT as GitHub does: RS256 with the app's key, issued by
/// its client ID, at most ten minutes long, and not issued in the future.
fn jwt_problem(authorization: &str) -> Option<String> {
    let Some(jwt) = authorization.strip_prefix("Bearer ") else {
        return Some("no bearer JWT".to_string());
    };
    let parts: Vec<&str> = jwt.split('.').collect();
    let [header, claims, signature] = parts.as_slice() else {
        return Some("not three parts".to_string());
    };
    let decode = |part: &str| URL_SAFE_NO_PAD.decode(part).ok();
    let header: Value = serde_json::from_slice(&decode(header)?).ok()?;
    if header != json!({"alg": "RS256", "typ": "JWT"}) {
        return Some(format!("header {header}"));
    }
    let verified = UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, &test_key().public_key)
        .verify(
            format!("{}.{}", parts[0], parts[1]).as_bytes(),
            &decode(signature)?,
        );
    if verified.is_err() {
        return Some("bad signature".to_string());
    }
    let claims: Value = serde_json::from_slice(&decode(claims)?).ok()?;
    let (iat, exp) = (claims["iat"].as_i64()?, claims["exp"].as_i64()?);
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    if claims["iss"] != CLIENT_ID {
        return Some(format!("issuer {}", claims["iss"]));
    }
    if exp - iat > 600 || iat > now || exp <= now {
        return Some(format!("times iat={iat} exp={exp} now={now}"));
    }
    None
}

async fn mint(
    State(github): State<Arc<FakeGithub>>,
    Path(installation): Path<u64>,
    headers: HeaderMap,
) -> Response {
    if let Some(problem) = jwt_problem(&header(&headers, "authorization")) {
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
fn admit(github: &FakeGithub, path: String, headers: &HeaderMap) -> Option<Response> {
    let authorization = header(headers, "authorization");
    github.received.lock().unwrap().push(Received {
        path,
        user_agent: header(headers, "user-agent"),
        accept: header(headers, "accept"),
        api_version: header(headers, "x-github-api-version"),
        authorization: authorization.clone(),
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

async fn seats(
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

async fn user(
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

async fn fake_github() -> (String, Arc<FakeGithub>) {
    let github = Arc::new(FakeGithub {
        token_lifetime: Mutex::new(time::Duration::hours(1)),
        mints: AtomicUsize::new(0),
        revoked: Mutex::new(HashSet::new()),
        revoke_every_token: Mutex::new(false),
        mint_failure: Mutex::new(None),
        jwt_problems: Mutex::new(Vec::new()),
        received: Mutex::new(Vec::new()),
    });
    let app = Router::new()
        .route(
            "/app/installations/{installation}/access_tokens",
            post(mint),
        )
        .route("/orgs/{organization}/copilot/billing/seats", get(seats))
        .route("/user/{id}", get(user))
        .with_state(github.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    (format!("http://{address}/"), github)
}

fn credentials(
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

fn directory(api_base: &str, organization: &str) -> GithubAccountDirectory {
    GithubAccountDirectory::new(credentials(api_base, organization, pkcs1_pem()))
        .unwrap()
        .with_page_size(2)
}

fn account(id: i64) -> AiProviderAccountId {
    AiProviderAccountId::new(id).unwrap()
}

#[tokio::test]
async fn seats_are_read_page_by_page_by_number() {
    let (api_base, github) = fake_github().await;
    let listing = directory(&api_base, "example-org").seats().await.unwrap();

    assert_eq!(
        github.paths(),
        (1..=3)
            .map(|page| format!("/orgs/example-org/copilot/billing/seats?per_page=2&page={page}"))
            .collect::<Vec<_>>()
    );
    assert_eq!(listing.reported_total, Some(5));
    assert!(listing.ended_on_short_page);
    // Three users: the organization assignee is skipped, Grace's repeated
    // entry replaces her first.
    assert!(!listing.is_complete());
    let ids: Vec<i64> = listing
        .seats
        .iter()
        .map(|seat| seat.account_id.as_i64())
        .collect();
    assert_eq!(ids, [1001, 1002, 1004]);

    let ada = &listing.seats[0];
    assert_eq!(ada.login.as_str(), "ada-lovelace");
    // The same instant, whatever offset GitHub renders it in.
    assert_eq!(ada.created_at, datetime!(2023-04-17 05:34:52 UTC));
    assert_eq!(
        ada.last_activity_at,
        Some(datetime!(2026-10-24 10:05:06 UTC))
    );
    assert_eq!(ada.last_activity_client.as_deref(), Some("copilot-cli"));
    assert_eq!(ada.pending_cancellation_on, None);

    let grace = &listing.seats[1];
    assert_eq!(grace.pending_cancellation_on, Some(date!(2026 - 10 - 31)));
    assert_eq!(
        grace.last_activity_client.as_deref(),
        Some("vscode/1.139.0/copilot-chat/0.67.0")
    );

    let alan = &listing.seats[2];
    assert_eq!(
        alan.last_activity_client.as_ref().unwrap().chars().count(),
        512
    );

    // One token for every page, sent with GitHub's headers.
    assert_eq!(github.mints(), 1);
    assert!(github.jwt_problems.lock().unwrap().is_empty());
    for received in github.received() {
        assert!(received.user_agent.starts_with("toki-api/"), "{received:?}");
        assert_eq!(received.accept, "application/vnd.github+json");
        assert_eq!(received.api_version, "2022-11-28");
        assert_eq!(received.authorization, "Bearer ghs_synthetic_1");
    }
}

#[tokio::test]
async fn a_listing_ends_only_on_a_short_page() {
    // A full last page may be followed by more, so the next one is read.
    let (api_base, github) = fake_github().await;
    let listing = directory(&api_base, "exact-org").seats().await.unwrap();
    assert_eq!(listing.seats.len(), 2);
    assert!(listing.is_complete());
    assert_eq!(
        github.paths(),
        (1..=2)
            .map(|page| format!("/orgs/exact-org/copilot/billing/seats?per_page=2&page={page}"))
            .collect::<Vec<_>>()
    );

    // An organization without seats.
    let (api_base, github) = fake_github().await;
    let listing = directory(&api_base, "empty-org").seats().await.unwrap();
    assert!(listing.is_complete());
    assert!(listing.seats.is_empty());
    assert_eq!(github.paths().len(), 1);
}

#[tokio::test]
async fn an_installation_token_is_cached_until_shortly_before_it_expires() {
    let (api_base, github) = fake_github().await;
    let directory = directory(&api_base, "example-org");
    directory.profile(account(1001)).await.unwrap();
    directory.profile(account(1001)).await.unwrap();
    assert_eq!(github.mints(), 1);

    // A token that expires within five minutes is renewed.
    let (api_base, github) = fake_github().await;
    *github.token_lifetime.lock().unwrap() = time::Duration::minutes(2);
    let directory = super::tests::directory(&api_base, "example-org");
    directory.profile(account(1001)).await.unwrap();
    directory.profile(account(1001)).await.unwrap();
    assert_eq!(github.mints(), 2);
}

#[tokio::test]
async fn a_rejected_token_is_replaced_and_the_request_retried_once() {
    let (api_base, github) = fake_github().await;
    github
        .revoked
        .lock()
        .unwrap()
        .insert("ghs_synthetic_1".to_string());
    let directory = directory(&api_base, "example-org");

    assert!(directory.profile(account(1001)).await.unwrap().is_some());
    assert_eq!(github.mints(), 2);
    let authorizations: Vec<String> = github
        .received()
        .into_iter()
        .map(|received| received.authorization)
        .collect();
    assert_eq!(
        authorizations,
        ["Bearer ghs_synthetic_1", "Bearer ghs_synthetic_2"]
    );

    // Still rejected after the retry: unauthorized, without a third attempt.
    *github.revoke_every_token.lock().unwrap() = true;
    let error = directory.profile(account(1001)).await.unwrap_err();
    assert!(
        matches!(error, ProviderAccountDirectoryError::Unauthorized(_)),
        "{error}"
    );
    assert_eq!(github.received().len(), 4);
}

#[tokio::test]
async fn profiles_are_read_by_numeric_id() {
    let (api_base, github) = fake_github().await;
    let directory = directory(&api_base, "example-org");

    let ada = directory.profile(account(1001)).await.unwrap().unwrap();
    assert_eq!(ada.account_id, account(1001));
    assert_eq!(ada.login.as_str(), "ada-lovelace");
    assert_eq!(ada.name.as_deref(), Some("Ada Lovelace"));
    // An empty public email is none.
    assert_eq!(ada.email, None);

    let grace = directory.profile(account(1002)).await.unwrap().unwrap();
    assert_eq!((grace.name, grace.email), (None, None));

    // A deleted account has no profile.
    assert_eq!(directory.profile(account(4040)).await.unwrap(), None);
    // GitHub answering for someone else is an error.
    assert!(matches!(
        directory.profile(account(1003)).await,
        Err(ProviderAccountDirectoryError::Response(_))
    ));
    assert_eq!(
        github.paths(),
        ["/user/1001", "/user/1002", "/user/4040", "/user/1003"]
    );
}

#[tokio::test]
async fn github_answers_map_to_directory_errors() {
    let (api_base, _) = fake_github().await;
    let seats_of = |organization: &str| {
        let directory = directory(&api_base, organization);
        async move { directory.seats().await.unwrap_err() }
    };

    // The primary rate limit, until its reset.
    let ProviderAccountDirectoryError::RateLimited {
        retry_after: Some(wait),
    } = seats_of("limited").await
    else {
        panic!("not rate limited");
    };
    assert!(
        (Duration::from_secs(115)..=Duration::from_secs(120)).contains(&wait),
        "{wait:?}"
    );
    // A secondary rate limit, on a 403 and on a 429.
    for organization in ["secondary", "busy"] {
        assert!(
            matches!(
                seats_of(organization).await,
                ProviderAccountDirectoryError::RateLimited { retry_after: Some(wait) }
                    if wait == Duration::from_secs(30)
            ),
            "{organization}"
        );
    }

    let forbidden = seats_of("forbidden").await;
    assert!(
        matches!(forbidden, ProviderAccountDirectoryError::Forbidden(_)),
        "{forbidden}"
    );
    let message = forbidden.to_string();
    assert!(message.contains("Copilot Business: read"), "{message}");
    assert!(message.contains("Resource not accessible"), "{message}");

    let missing = seats_of("missing").await;
    assert!(
        matches!(missing, ProviderAccountDirectoryError::NotFound(_)),
        "{missing}"
    );
    assert!(missing.to_string().contains("not installed"), "{missing}");

    let disabled = seats_of("no-copilot").await;
    assert!(
        matches!(disabled, ProviderAccountDirectoryError::Forbidden(_))
            && disabled.to_string().contains("not enabled"),
        "{disabled}"
    );
    assert!(matches!(
        seats_of("down").await,
        ProviderAccountDirectoryError::Unavailable(_)
    ));
    assert!(matches!(
        seats_of("garbled").await,
        ProviderAccountDirectoryError::Response(_)
    ));

    // Nothing listening.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let closed = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    assert!(matches!(
        directory(&closed, "example-org").seats().await,
        Err(ProviderAccountDirectoryError::Unavailable(_))
    ));
}

#[tokio::test]
async fn token_failures_name_the_setting_to_check() {
    let (api_base, github) = fake_github().await;
    let failing = |status: StatusCode, headers: Vec<(&'static str, String)>| {
        *github.mint_failure.lock().unwrap() = Some((status, headers));
        let directory = directory(&api_base, "example-org");
        async move { directory.seats().await.unwrap_err() }
    };

    let rejected = failing(StatusCode::UNAUTHORIZED, vec![]).await;
    assert!(
        matches!(rejected, ProviderAccountDirectoryError::Unauthorized(_))
            && rejected.to_string().contains("github.app_client_id"),
        "{rejected}"
    );
    let suspended = failing(StatusCode::FORBIDDEN, vec![]).await;
    assert!(
        matches!(suspended, ProviderAccountDirectoryError::Forbidden(_))
            && suspended.to_string().contains("suspended"),
        "{suspended}"
    );
    assert!(matches!(
        failing(StatusCode::TOO_MANY_REQUESTS, vec![("retry-after", "30".to_string())]).await,
        ProviderAccountDirectoryError::RateLimited { retry_after: Some(wait) }
            if wait == Duration::from_secs(30)
    ));
    assert!(matches!(
        failing(StatusCode::BAD_GATEWAY, vec![]).await,
        ProviderAccountDirectoryError::Unavailable(_)
    ));

    // An unknown installation, which the fake answers itself.
    *github.mint_failure.lock().unwrap() = None;
    let other_installation = GithubAccountDirectory::new(GithubAppCredentials {
        installation_id: 1,
        ..credentials(&api_base, "example-org", pkcs1_pem())
    })
    .unwrap();
    let unknown = other_installation.seats().await.unwrap_err();
    assert!(
        matches!(unknown, ProviderAccountDirectoryError::NotFound(_))
            && unknown.to_string().contains("github.installation_id"),
        "{unknown}"
    );
    // GitHub never saw a seats request.
    assert!(github.paths().is_empty());
}

#[tokio::test]
async fn keys_are_read_as_pkcs1_pkcs8_or_with_escaped_newlines() {
    let (api_base, github) = fake_github().await;
    let escaped = pkcs1_pem().trim_end().replace('\n', "\\n");
    assert!(!escaped.contains('\n'));
    for pem in [pkcs1_pem(), pkcs8_pem(), escaped] {
        let directory =
            GithubAccountDirectory::new(credentials(&api_base, "example-org", pem)).unwrap();
        directory.profile(account(1001)).await.unwrap().unwrap();
    }
    // Every key signed JWTs GitHub accepts.
    assert_eq!(github.mints(), 3);
    assert!(github.jwt_problems.lock().unwrap().is_empty());
}

#[test]
fn unusable_settings_are_refused_without_echoing_the_key() {
    let garbage = "not-a-key-but-a-secret-looking-value-1234";
    let error = parse_private_key(garbage).err().unwrap();
    assert!(!error.contains(garbage), "{error}");
    assert!(error.contains("github.app_private_key"), "{error}");

    let truncated = pkcs1_pem().replace("RSA PRIVATE KEY", "PRIVATE KEY");
    let error = parse_private_key(&truncated).err().unwrap();
    assert!(error.contains("not a usable RSA private key"), "{error}");
    let certificate = pem::encode(&pem::Pem::new("CERTIFICATE", vec![1, 2, 3]));
    assert!(parse_private_key(&certificate)
        .err()
        .unwrap()
        .contains("CERTIFICATE"));

    let refused =
        |credentials: GithubAppCredentials| GithubAccountDirectory::new(credentials).err().unwrap();
    let base = credentials(DEFAULT_API_BASE, "Example-AB", pkcs1_pem());
    assert!(refused(GithubAppCredentials {
        organization: "../etc".to_string(),
        ..base.clone()
    })
    .contains("github.organization"));
    assert!(refused(GithubAppCredentials {
        api_base: "ftp://example.com".to_string(),
        ..base.clone()
    })
    .contains("github.api_base"));
    assert!(refused(GithubAppCredentials {
        private_key_pem: garbage.to_string(),
        ..base.clone()
    })
    .contains("github.app_private_key"));

    // Credentials never show their key.
    let shown = format!("{base:?}");
    assert!(
        shown.contains("[redacted]") && !shown.contains("PRIVATE KEY"),
        "{shown}"
    );
}
