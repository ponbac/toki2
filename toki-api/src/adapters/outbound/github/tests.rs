use std::time::Duration;

use axum::http::StatusCode;
use time::macros::{date, datetime};

use super::{app_auth::parse_private_key, test_support::*, *};

/// A shared app for `organization` on the fake GitHub.
fn app(api_base: &str, organization: &str) -> GithubApp {
    GithubApp::new(credentials(api_base, organization, pkcs1_pem())).unwrap()
}

fn directory(api_base: &str, organization: &str) -> GithubAccountDirectory {
    GithubAccountDirectory::new(app(api_base, organization)).with_page_size(2)
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
    let other_installation = GithubAccountDirectory::new(
        GithubApp::new(GithubAppCredentials {
            installation_id: 1,
            ..credentials(&api_base, "example-org", pkcs1_pem())
        })
        .unwrap(),
    );
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
        let directory = GithubAccountDirectory::new(
            GithubApp::new(credentials(&api_base, "example-org", pem)).unwrap(),
        );
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

    let refused = |credentials: GithubAppCredentials| GithubApp::new(credentials).err().unwrap();
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
