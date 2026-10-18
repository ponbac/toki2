//! Fetches OAuth Client ID Metadata Documents: the JSON document an MCP
//! client publishes at the HTTPS URL it uses as its `client_id`.
//!
//! The URL comes from whoever starts an authorization request, so fetching it
//! is a server-side request forgery risk. The fetcher connects only to
//! globally routable addresses, checked after DNS resolution so a rebinding
//! name cannot point it inward, follows no redirects, uses no proxy, and reads
//! at most a few kilobytes within a short timeout.
//!
//! Anyone can make Toki fetch, so the work is bounded too: documents are
//! cached, concurrent requests for one URL share a fetch, and at most a few
//! fetches run at once. Only fetching and parsing happen here; whether the
//! document is acceptable for its client_id is the domain's decision.

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use moka::future::Cache;
use reqwest::{
    dns::{Addrs, Name, Resolve, Resolving},
    header::ACCEPT,
    StatusCode,
};
use tokio::sync::Semaphore;
use url::{Host, Url};

use crate::domain::{
    models::{ClientIdUrl, ClientMetadataDocument},
    ports::outbound::ClientMetadataResolver,
    OAuthError,
};

const MAX_DOCUMENT_BYTES: usize = 5 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const CACHE_LIFETIME: Duration = Duration::from_secs(60 * 60);
const CACHE_CAPACITY: u64 = 1_000;
/// Fetches allowed at once, across all client_ids.
const MAX_CONCURRENT_FETCHES: usize = 8;
/// How long a fetch waits for one of those before failing.
const PERMIT_TIMEOUT: Duration = Duration::from_secs(5);

/// Client ID Metadata Documents over HTTPS as a `ClientMetadataResolver`.
/// The application holds one, so its fetch limit is global.
pub struct CimdFetcher {
    http: reqwest::Client,
    /// Successful documents only: a failure is retried on the next request.
    cache: Cache<String, Arc<ClientMetadataDocument>>,
    fetches: Semaphore,
    permit_timeout: Duration,
    addresses: AddressPolicy,
}

#[derive(Clone, Copy)]
enum AddressPolicy {
    PublicOnly,
    /// Lets tests fetch from a local server.
    #[cfg(test)]
    Any,
}

impl AddressPolicy {
    fn allows(self, address: IpAddr) -> bool {
        match self {
            Self::PublicOnly => is_public(address),
            #[cfg(test)]
            Self::Any => true,
        }
    }
}

impl CimdFetcher {
    pub fn new() -> Result<Self, OAuthError> {
        Self::with_policy(AddressPolicy::PublicOnly)
    }

    fn with_policy(addresses: AddressPolicy) -> Result<Self, OAuthError> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            // Every fetch goes to an origin an anonymous caller chose, and
            // documents are cached, so keep no idle connections around.
            .pool_max_idle_per_host(0)
            .timeout(FETCH_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .dns_resolver(PolicyResolver(addresses))
            .user_agent("toki-oauth")
            .build()
            .map_err(|error| OAuthError::ClientMetadataUnavailable(error.to_string()))?;

        Ok(Self {
            http,
            cache: Cache::builder()
                .max_capacity(CACHE_CAPACITY)
                .time_to_live(CACHE_LIFETIME)
                .build(),
            fetches: Semaphore::new(MAX_CONCURRENT_FETCHES),
            permit_timeout: PERMIT_TIMEOUT,
            addresses,
        })
    }

    /// The document at `url`, from the cache or fetched within the global
    /// fetch limit.
    async fn resolve_url(&self, url: &Url) -> Result<Arc<ClientMetadataDocument>, OAuthError> {
        self.cache
            .try_get_with(url.as_str().to_string(), async {
                let _permit = tokio::time::timeout(self.permit_timeout, self.fetches.acquire())
                    .await
                    .map_err(|_| {
                        "Too many client metadata documents are being fetched; try again shortly."
                            .to_string()
                    })?
                    .map_err(|_| "Client metadata documents cannot be fetched.".to_string())?;
                self.fetch(url).await.map(Arc::new)
            })
            .await
            .map_err(|error| OAuthError::ClientMetadataUnavailable(error.as_ref().clone()))
    }

    async fn fetch(&self, url: &Url) -> Result<ClientMetadataDocument, String> {
        // reqwest connects to IP literals without asking the resolver.
        let literal = match url.host() {
            Some(Host::Ipv4(address)) => Some(IpAddr::V4(address)),
            Some(Host::Ipv6(address)) => Some(IpAddr::V6(address)),
            _ => None,
        };
        if literal.is_some_and(|address| !self.addresses.allows(address)) {
            return Err("The client's metadata document is not on a public address.".to_string());
        }

        let mut response = self
            .http
            .get(url.clone())
            .header(ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| {
                format!(
                    "The client's metadata document could not be fetched: {}",
                    error.without_url()
                )
            })?;
        if response.status() != StatusCode::OK {
            return Err(format!(
                "The client's metadata document answered {}.",
                response.status()
            ));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_DOCUMENT_BYTES as u64)
        {
            return Err(too_large());
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            format!(
                "The client's metadata document could not be read: {}",
                error.without_url()
            )
        })? {
            body.extend_from_slice(&chunk);
            if body.len() > MAX_DOCUMENT_BYTES {
                return Err(too_large());
            }
        }
        serde_json::from_slice(&body)
            .map_err(|error| format!("The client's metadata document is not valid: {error}"))
    }
}

fn too_large() -> String {
    format!("The client's metadata document is larger than {MAX_DOCUMENT_BYTES} bytes.")
}

#[async_trait]
impl ClientMetadataResolver for CimdFetcher {
    async fn resolve(
        &self,
        client_id: &ClientIdUrl,
    ) -> Result<Arc<ClientMetadataDocument>, OAuthError> {
        self.resolve_url(client_id.url()).await
    }
}

/// Resolves names with the system resolver and keeps only the addresses the
/// policy allows, so a name cannot lead the fetcher to an internal address.
struct PolicyResolver(AddressPolicy);

impl Resolve for PolicyResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let policy = self.0;
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|address| policy.allows(address.ip()))
                .collect();
            if addresses.is_empty() {
                return Err(format!("{host} has no public address").into());
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

/// Whether `address` is globally routable: not loopback, private, link-local,
/// shared, reserved or documentation space, nor an IPv6 address embedding or
/// translating to one. IPv4-mapped addresses are judged as IPv4.
fn is_public(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_v4(address),
        IpAddr::V6(address) => match address.to_ipv4_mapped() {
            Some(mapped) => is_public_v4(mapped),
            None => is_public_v6(address),
        },
    }
}

fn is_public_v4(address: Ipv4Addr) -> bool {
    let [a, b, c, _] = address.octets();
    !(a == 0
        || address.is_loopback()
        || address.is_private()
        || address.is_link_local()
        || address.is_broadcast()
        || address.is_documentation()
        || address.is_multicast()
        // Shared address space (RFC 6598), as used by carrier NAT and Tailscale.
        || (a == 100 && (64..128).contains(&b))
        // IETF protocol assignments.
        || (a == 192 && b == 0 && c == 0)
        // Benchmarking (RFC 2544).
        || (a == 198 && (b == 18 || b == 19))
        // Reserved for future use.
        || a >= 240)
}

fn is_public_v6(address: Ipv6Addr) -> bool {
    let segments = address.segments();
    // ::/16 holds the unspecified and loopback addresses and the
    // IPv4-compatible and IPv4-translated forms; nothing in it is global.
    !(segments[0] == 0
        || address.is_multicast()
        // Unique local, link-local and the deprecated site-local.
        || segments[0] & 0xfe00 == 0xfc00
        || segments[0] & 0xffc0 == 0xfe80
        || segments[0] & 0xffc0 == 0xfec0
        // Documentation (RFC 3849 and RFC 9637).
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
        || segments[0] & 0xfff0 == 0x3ff0
        // IETF protocol assignments, including Teredo; 6to4, NAT64 and the
        // discard prefix, which embed or reach other addresses.
        || (segments[0] == 0x2001 && segments[1] < 0x0200)
        || segments[0] == 0x2002
        || (segments[0] == 0x0064 && segments[1] == 0xff9b)
        || (segments[0] == 0x0100 && segments[1..4] == [0; 3]))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        io::ErrorKind,
        sync::{Arc, Mutex},
    };

    use axum::{
        body::Body,
        extract::{Path, State},
        http::{header, StatusCode as HttpStatus},
        response::{IntoResponse, Response},
        routing::get,
        Router,
    };

    use super::*;

    #[test]
    fn only_globally_routable_addresses_are_public() {
        for public in [
            "8.8.8.8",
            "160.79.104.10",
            "100.63.255.255",
            "100.128.0.1",
            "172.32.0.1",
            "198.17.255.255",
            "198.20.0.1",
            "223.255.255.255",
            "::ffff:8.8.8.8",
            "2606:4700::6810:84e5",
            "2001:4860:4860::8888",
            "2001:200::1",
            "2a00:1450:4001::200e",
        ] {
            assert!(is_public(public.parse().unwrap()), "{public}");
        }
        for internal in [
            // IPv4
            "0.0.0.0",
            "0.1.2.3",
            "127.0.0.1",
            "127.255.255.254",
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "100.100.100.100",
            "100.127.255.255",
            "192.0.0.1",
            "192.0.2.1",
            "198.51.100.1",
            "203.0.113.1",
            "198.18.0.1",
            "198.19.255.255",
            "224.0.0.1",
            "239.255.255.250",
            "240.0.0.1",
            "255.255.255.255",
            // IPv6
            "::",
            "::1",
            "::127.0.0.1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
            "::ffff:0:127.0.0.1",
            "fc00::1",
            "fd7a:115c:a1e0::1",
            "fe80::1",
            "febf::1",
            "fec0::1",
            "ff02::1",
            "ff0e::1",
            "2001:db8::1",
            "3fff::1",
            "2001::1",
            "2001:0:4136:e378:8000:63bf:3fff:fdd2",
            "2002:a9fe:a9fe::1",
            "64:ff9b::a9fe:a9fe",
            "64:ff9b:1::1",
            "100::1",
        ] {
            assert!(!is_public(internal.parse().unwrap()), "{internal}");
        }
    }

    /// A listener that records whether anything connected to it.
    struct Bait(std::net::TcpListener);

    impl Bait {
        fn bind(address: &str) -> Option<Self> {
            let listener = std::net::TcpListener::bind(address).ok()?;
            listener.set_nonblocking(true).unwrap();
            Some(Self(listener))
        }

        fn port(&self) -> u16 {
            self.0.local_addr().unwrap().port()
        }

        /// The kernel completes a connection without `accept`, so a fetch
        /// that reached the listener leaves one waiting.
        fn was_contacted(&self) -> bool {
            match self.0.accept() {
                Ok(_) => true,
                Err(error) if error.kind() == ErrorKind::WouldBlock => false,
                Err(error) => panic!("{error}"),
            }
        }
    }

    fn client_id(raw: &str) -> ClientIdUrl {
        ClientIdUrl::parse(raw).unwrap()
    }

    #[tokio::test]
    async fn internal_addresses_are_never_contacted() {
        let fetcher = CimdFetcher::new().unwrap();

        let v4 = Bait::bind("127.0.0.1:0").unwrap();
        let resolved = fetcher
            .resolve(&client_id(&format!("https://127.0.0.1:{}/x", v4.port())))
            .await;
        assert!(matches!(
            resolved,
            Err(OAuthError::ClientMetadataUnavailable(_))
        ));
        assert!(!v4.was_contacted());

        match Bait::bind("[::1]:0") {
            Some(v6) => {
                let resolved = fetcher
                    .resolve(&client_id(&format!("https://[::1]:{}/x", v6.port())))
                    .await;
                assert!(resolved.is_err());
                assert!(!v6.was_contacted());
            }
            None => eprintln!("skipping [::1]: IPv6 loopback is unavailable"),
        }

        // `localhost` may resolve to either loopback address: bait both.
        let v4 = Bait::bind("127.0.0.1:0").unwrap();
        let v6 = Bait::bind(&format!("[::1]:{}", v4.port()));
        let resolved = fetcher
            .resolve(&client_id(&format!("https://localhost:{}/x", v4.port())))
            .await;
        assert!(resolved.is_err());
        assert!(!v4.was_contacted());
        assert!(v6.is_none_or(|v6| !v6.was_contacted()));
    }

    #[tokio::test]
    async fn the_bait_notices_a_fetch_that_is_allowed_through() {
        // Without the address policy the same fetches do connect (and then
        // fail TLS), so the test above would notice a leak.
        let fetcher = CimdFetcher::with_policy(AddressPolicy::Any).unwrap();
        for host in ["127.0.0.1", "localhost"] {
            let bait = Bait::bind("127.0.0.1:0").unwrap();
            let v6 = Bait::bind(&format!("[::1]:{}", bait.port()));
            // The connection is made at once; the TLS handshake then waits on
            // a listener that never answers, so there is no need to finish.
            let _ = tokio::time::timeout(
                Duration::from_millis(500),
                fetcher.resolve(&client_id(&format!("https://{host}:{}/x", bait.port()))),
            )
            .await;
            assert!(
                bait.was_contacted() || v6.is_some_and(|v6| v6.was_contacted()),
                "{host}"
            );
        }
    }

    const DOCUMENT: &str = r#"{"client_id":"https://claude.ai/oauth/claude-code-client-metadata","client_name":"Claude Code","redirect_uris":["http://localhost/callback"],"token_endpoint_auth_method":"none","logo_uri":"https://claude.ai/logo.png"}"#;

    #[derive(Clone, Default)]
    struct Hits(Arc<Mutex<HashMap<String, usize>>>);

    impl Hits {
        fn of(&self, path: &str) -> usize {
            self.0.lock().unwrap().get(path).copied().unwrap_or(0)
        }
    }

    /// A plain-HTTP client metadata host on a local port.
    async fn fake_host() -> (String, Hits) {
        async fn serve(State(hits): State<Hits>, Path(path): Path<String>) -> Response {
            let count = {
                let mut hits = hits.0.lock().unwrap();
                let count = hits.entry(path.clone()).or_default();
                *count += 1;
                *count
            };
            let oversize = "x".repeat(MAX_DOCUMENT_BYTES + 1);
            match path.as_str() {
                "document" => {
                    ([(header::CONTENT_TYPE, "application/json")], DOCUMENT).into_response()
                }
                "redirect" => (HttpStatus::FOUND, [(header::LOCATION, "/target")]).into_response(),
                "missing" => HttpStatus::NOT_FOUND.into_response(),
                "large" => oversize.into_response(),
                "large-chunked" => {
                    let chunks = oversize
                        .into_bytes()
                        .chunks(1024)
                        .map(|chunk| Ok::<_, std::io::Error>(chunk.to_vec()))
                        .collect::<Vec<_>>();
                    Body::from_stream(futures::stream::iter(chunks)).into_response()
                }
                "invalid" => "<html>not a document</html>".into_response(),
                // Fails once, then serves the document.
                "flaky" if count == 1 => HttpStatus::SERVICE_UNAVAILABLE.into_response(),
                "flaky" => DOCUMENT.into_response(),
                _ => HttpStatus::NOT_FOUND.into_response(),
            }
        }

        let hits = Hits::default();
        let app = Router::new()
            .route("/{*path}", get(serve))
            .with_state(hits.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        (format!("http://{address}/"), hits)
    }

    fn local_fetcher() -> CimdFetcher {
        CimdFetcher::with_policy(AddressPolicy::Any).unwrap()
    }

    fn url(base: &str, path: &str) -> Url {
        Url::parse(base).unwrap().join(path).unwrap()
    }

    #[tokio::test]
    async fn a_document_is_parsed() {
        let (base, _) = fake_host().await;
        let document = local_fetcher()
            .fetch(&url(&base, "document"))
            .await
            .unwrap();

        assert_eq!(
            document.client_id,
            "https://claude.ai/oauth/claude-code-client-metadata"
        );
        assert_eq!(document.client_name.as_deref(), Some("Claude Code"));
        assert_eq!(document.redirect_uris, ["http://localhost/callback"]);
        assert_eq!(document.token_endpoint_auth_method.as_deref(), Some("none"));
    }

    #[tokio::test]
    async fn redirects_are_not_followed() {
        let (base, hits) = fake_host().await;
        let fetched = local_fetcher().fetch(&url(&base, "redirect")).await;

        assert!(fetched.unwrap_err().contains("302"));
        assert_eq!(hits.of("redirect"), 1);
        assert_eq!(hits.of("target"), 0);
    }

    #[tokio::test]
    async fn unusable_responses_fail() {
        let (base, hits) = fake_host().await;
        let fetcher = local_fetcher();

        for path in ["missing", "large", "large-chunked", "invalid"] {
            assert!(fetcher.fetch(&url(&base, path)).await.is_err(), "{path}");
            assert_eq!(hits.of(path), 1, "{path}");
        }
    }

    #[tokio::test]
    async fn documents_are_cached_and_failures_are_not() {
        let (base, hits) = fake_host().await;
        let fetcher = local_fetcher();

        let document = url(&base, "document");
        let first = fetcher.resolve_url(&document).await.unwrap();
        let second = fetcher.resolve_url(&document).await.unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(hits.of("document"), 1);

        let flaky = url(&base, "flaky");
        assert!(matches!(
            fetcher.resolve_url(&flaky).await,
            Err(OAuthError::ClientMetadataUnavailable(_))
        ));
        assert!(fetcher.resolve_url(&flaky).await.is_ok());
        assert_eq!(hits.of("flaky"), 2);
    }

    #[tokio::test]
    async fn a_fetch_waiting_too_long_for_a_permit_fails() {
        let (base, hits) = fake_host().await;
        let mut fetcher = local_fetcher();
        fetcher.permit_timeout = Duration::from_millis(50);
        let document = url(&base, "document");

        let busy = fetcher
            .fetches
            .acquire_many(MAX_CONCURRENT_FETCHES as u32)
            .await
            .unwrap();
        assert!(fetcher.resolve_url(&document).await.is_err());
        assert_eq!(hits.of("document"), 0);

        drop(busy);
        assert!(fetcher.resolve_url(&document).await.is_ok());
        assert_eq!(hits.of("document"), 1);
    }
}
