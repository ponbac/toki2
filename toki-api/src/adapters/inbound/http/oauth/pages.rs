//! The authorization server's own pages: consent and errors. They are
//! server-rendered HTML in Toki's look, with no scripts; besides Toki's own
//! logo they load only the web app's fonts. Every value that comes from a
//! request or a client is escaped.

use axum::{
    http::{
        header::{
            CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, REFERRER_POLICY, X_FRAME_OPTIONS,
        },
        HeaderValue, StatusCode,
    },
    response::{Html, IntoResponse, Response},
};

use crate::domain::models::{OAuthClient, OAuthClientKind, RedirectTarget, RedirectUri};

/// Where the pages load Toki's logo from.
pub(super) const LOGO_PATH: &str = "/oauth/toki-logo.png";
const LOGO: &[u8] = include_bytes!("../../../../../assets/toki-logo.png");

/// Inline styles, the web app's fonts and Toki's logo; nothing may run, frame
/// the page or rebase it.
const CONTENT_SECURITY_POLICY_VALUE: &str = "default-src 'none'; \
    style-src 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; \
    img-src 'self'; frame-ancestors 'none'; base-uri 'none'";

/// The web app's fonts, escaped for an attribute.
const FONTS: &str = "https://fonts.googleapis.com/css2?family=Manrope:wght@500;600;700;800\
    &amp;family=IBM+Plex+Mono:wght@500&amp;display=swap";

/// The web app's design tokens (`app/src/index.css`): warm paper with a burnt
/// amber accent, or deep slate with a bright amber one, following the
/// browser's colour scheme.
const STYLE: &str = "
:root { color-scheme: light; --glow: 27 63% 45%;
  --bg: hsl(45 22% 92%); --card-top: hsl(44 26% 96%); --card-bottom: hsl(43 20% 90%);
  --text: hsl(32 18% 19%); --text-2: hsl(31 10% 33%);
  --line: hsl(40 14% 74%); --tile: hsl(44 16% 85%);
  --primary: hsl(27 63% 39%); --on-primary: hsl(45 32% 98%);
  --caution: hsl(32 70% 42% / .10); --caution-line: hsl(32 60% 42% / .35);
  --shadow: 0 1px 2px hsl(32 24% 10% / .09), 0 8px 18px hsl(28 35% 25% / .10),
    0 0 0 1px hsl(var(--glow) / .10) inset; }
@media (prefers-color-scheme: dark) { :root { color-scheme: dark; --glow: 38 95% 55%;
  --bg: hsl(222 25% 8%); --card-top: hsl(222 22% 12%); --card-bottom: hsl(222 22% 10%);
  --text: hsl(40 20% 92%); --text-2: hsl(40 10% 60%);
  --line: hsl(222 15% 18%); --tile: hsl(222 18% 16%);
  --primary: hsl(38 95% 55%); --on-primary: hsl(222 25% 8%);
  --caution: hsl(38 95% 55% / .07); --caution-line: hsl(38 95% 55% / .28);
  --shadow: 0 1px 2px hsl(0 0% 0% / .2), 0 4px 12px hsl(0 0% 0% / .25),
    0 0 0 1px hsl(var(--glow) / .05) inset, 0 0 40px -10px hsl(var(--glow) / .1); } }
* { box-sizing: border-box; }
body { margin: 0; min-height: 100vh; display: grid; place-items: center; padding: 24px 16px;
  background: radial-gradient(ellipse 80% 50% at 50% -20%, hsl(var(--glow) / .08), transparent),
    radial-gradient(ellipse 60% 40% at 100% 100%, hsl(var(--glow) / .04), transparent), var(--bg);
  color: var(--text); font: 500 15px/1.55 Manrope, 'IBM Plex Sans', system-ui, sans-serif;
  -webkit-font-smoothing: antialiased; -moz-osx-font-smoothing: grayscale; }
main { width: 100%; max-width: 440px; padding: 28px; overflow-wrap: anywhere;
  background: linear-gradient(180deg, var(--card-top), var(--card-bottom));
  border: 1px solid var(--line); border-radius: 12px; box-shadow: var(--shadow); }
.link { display: flex; align-items: center; gap: 12px; margin: 0 0 22px; }
.link img, .tile { width: 48px; height: 48px; border-radius: 12px; flex: none; }
.tile { display: grid; place-items: center; background: var(--tile);
  border: 1px solid var(--line); color: var(--text-2); font-size: 21px; font-weight: 800; }
.wire { width: 44px; border-top: 2px dashed hsl(var(--glow) / .55); }
.brand { display: flex; align-items: center; gap: 10px; margin: 0 0 20px;
  font-size: 18px; font-weight: 800; letter-spacing: -.01em; }
.brand img { width: 32px; height: 32px; border-radius: 8px; }
h1 { margin: 0 0 4px; font-size: 23px; font-weight: 700; line-height: 1.25;
  letter-spacing: -.02em; }
p { margin: 0; }
.quiet { color: var(--text-2); }
strong { font-weight: 700; color: var(--text); }
.facts { list-style: none; margin: 22px 0 0; padding: 18px 0 0; border-top: 1px solid var(--line);
  display: grid; gap: 12px; }
.facts li { display: flex; gap: 10px; }
.facts svg { flex: none; width: 18px; height: 18px; margin-top: 2px; color: var(--text-2); }
.facts .caution { padding: 10px 12px; border-radius: 10px; background: var(--caution);
  border: 1px solid var(--caution-line); }
.facts .caution svg { color: var(--primary); }
.address { font-family: 'IBM Plex Mono', ui-monospace, monospace; font-size: 13.5px; font-weight: 500; }
.abilities { margin: 22px 0 0; padding: 18px 0 0; border-top: 1px solid var(--line); }
.abilities ul { margin: 8px 0 0; padding-left: 20px; color: var(--text-2); }
.abilities li + li { margin-top: 4px; }
form { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin-top: 26px; }
button { padding: 11px 16px; border-radius: 10px; border: 1px solid var(--line);
  background: transparent; color: var(--text); font: inherit; font-weight: 700; cursor: pointer; }
button:hover { background: hsl(var(--glow) / .06); }
button.allow { background: var(--primary); border-color: var(--primary); color: var(--on-primary); }
button.allow:hover { filter: brightness(.94); }
button:focus-visible { outline: 2px solid var(--primary); outline-offset: 2px; }
.detail { margin-top: 16px; padding-top: 14px; border-top: 1px solid var(--line);
  color: var(--text-2); font: 500 12.5px/1.5 'IBM Plex Mono', ui-monospace, monospace; }
.return { display: block; margin-top: 20px; padding: 11px 16px; border-radius: 10px;
  border: 1px solid var(--line); color: var(--text); font-weight: 700; text-align: center;
  text-decoration: none; }
.return:hover { background: hsl(var(--glow) / .06); }
.return:focus-visible { outline: 2px solid var(--primary); outline-offset: 2px; }
.footnote { margin-top: 16px; color: var(--text-2); font-size: 13px; text-align: center; }
";

/// Lucide icons, drawn in the text colour.
const ICON_VERIFIED: &str = "<svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" \
    stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\">\
    <path d=\"M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 \
    4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z\"/>\
    <path d=\"m9 12 2 2 4-4\"/></svg>";
const ICON_UNVERIFIED: &str = "<svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" \
    stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\">\
    <path d=\"M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 \
    4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z\"/>\
    <path d=\"M12 8v4\"/><path d=\"M12 16h.01\"/></svg>";
const ICON_WEB: &str = "<svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" \
    stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\">\
    <path d=\"M7 7h10v10\"/><path d=\"M7 17 17 7\"/></svg>";
const ICON_COMPUTER: &str = "<svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" \
    stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\">\
    <rect width=\"20\" height=\"14\" x=\"2\" y=\"3\" rx=\"2\"/><path d=\"M8 21h8\"/>\
    <path d=\"M12 17v4\"/></svg>";

/// Toki's logo, for the pages to show.
pub(super) async fn logo() -> Response {
    (
        [
            (CONTENT_TYPE, HeaderValue::from_static("image/png")),
            (
                CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=86400"),
            ),
        ],
        LOGO,
    )
        .into_response()
}

/// Escapes `raw` for use in HTML text and quoted attribute values.
pub(super) fn escape(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len());
    for character in raw.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// A page telling the user the request cannot go on. `message` is plain text.
pub(super) fn error_page(status: StatusCode, title: &str, message: &str) -> Response {
    error_page_with_detail(status, title, message, None)
}

/// An error page that also shows the protocol error behind it, for whoever
/// configures the app. `message` and `detail` are plain text.
pub(super) fn error_page_with_detail(
    status: StatusCode,
    title: &str,
    message: &str,
    detail: Option<&str>,
) -> Response {
    error_content(status, title, message, detail, "")
}

/// An error page with a link back to the app, which receives the error when
/// the user follows it. `href` is the app's redirect URI with the error.
pub(super) fn error_page_with_return(
    status: StatusCode,
    title: &str,
    message: &str,
    detail: &str,
    href: &str,
    destination: &str,
) -> Response {
    let link = format!(
        "\n<a class=\"return\" href=\"{}\">Return to {}</a>",
        escape(href),
        escape(destination)
    );
    error_content(status, title, message, Some(detail), &link)
}

/// `after` must already be escaped.
fn error_content(
    status: StatusCode,
    title: &str,
    message: &str,
    detail: Option<&str>,
    after: &str,
) -> Response {
    let detail = detail
        .filter(|detail| !detail.is_empty())
        .map(|detail| format!("\n<p class=\"detail\">{}</p>", escape(detail)))
        .unwrap_or_default();
    let content = format!(
        "<p class=\"brand\"><img src=\"{LOGO_PATH}\" alt=\"\">Toki</p>
<h1>{}</h1>
<p class=\"quiet\">{}</p>{detail}{after}",
        escape(title),
        escape(message)
    );
    page(status, title, &content, "no-referrer")
}

/// What the consent page shows.
pub(super) struct Consent<'a> {
    pub client: &'a OAuthClient,
    pub email: &'a str,
    /// The redirect URI the response goes to, as the client presented it.
    pub redirect_uri: &'a str,
    /// The registered redirect URI it matched.
    pub registered_redirect_uri: &'a RedirectUri,
    /// The handle the decision is posted with.
    pub request: &'a str,
    /// Where the decision is posted.
    pub form_action: &'a str,
}

pub(super) fn consent_page(consent: &Consent<'_>) -> Response {
    let name = escape(&consent.client.name);
    let monogram = consent
        .client
        .name
        .chars()
        .find(|character| character.is_alphanumeric())
        .map_or_else(|| "?".to_string(), |first| first.to_uppercase().to_string());
    let identity = match (consent.client.kind, consent.client.verified_host()) {
        (OAuthClientKind::MetadataDocument, Some(host)) => format!(
            "<li>{ICON_VERIFIED}<p>Identified by <strong>{}</strong>, which publishes its \
             details.</p></li>",
            escape(&host)
        ),
        _ => format!(
            "<li class=\"caution\">{ICON_UNVERIFIED}<p>The app registered this name itself, so \
             Toki can't verify it.</p></li>"
        ),
    };

    // The presented URI names the actual destination, such as the loopback
    // port a native app listens on.
    let destination = RedirectUri::parse(consent.redirect_uri)
        .unwrap_or_else(|| consent.registered_redirect_uri.clone());
    let destination_text = escape(&destination.display_destination());
    let only_if_yours = "Continue only if you just started connecting it yourself.";
    let destination = match consent.registered_redirect_uri.target() {
        RedirectTarget::Web => format!(
            "<li>{ICON_WEB}<p>Afterwards you'll go back to <strong>{destination_text}</strong>.\
             </p></li>"
        ),
        RedirectTarget::Loopback => format!(
            "<li class=\"caution\">{ICON_COMPUTER}<p>Afterwards you'll go back to an app on \
             this computer, at <strong class=\"address\">{destination_text}</strong>. \
             {only_if_yours}</p></li>"
        ),
        RedirectTarget::PrivateUseScheme => format!(
            "<li class=\"caution\">{ICON_COMPUTER}<p>Afterwards you'll go back to the app that \
             opens <strong class=\"address\">{destination_text}</strong> links on this \
             computer. {only_if_yours}</p></li>"
        ),
    };

    let content = format!(
        "<div class=\"link\" aria-hidden=\"true\"><span class=\"tile\">{monogram}</span>\
<span class=\"wire\"></span><img src=\"{LOGO_PATH}\" alt=\"\"></div>
<h1>Connect {name} to Toki</h1>
<p class=\"quiet\">Signed in as <strong>{email}</strong></p>
<ul class=\"facts\">
{identity}
{destination}
</ul>
<div class=\"abilities\">
<p>If you allow it, {name} can:</p>
<ul>
<li>Read and change your time tracking, work items and AI usage in Toki, acting as you.</li>
<li>Never use admin powers, even if your account has them.</li>
</ul>
</div>
<form method=\"post\" action=\"{action}\">
<input type=\"hidden\" name=\"request\" value=\"{request}\">
<button type=\"submit\" name=\"decision\" value=\"deny\">Deny</button>
<button type=\"submit\" name=\"decision\" value=\"allow\" class=\"allow\">Allow</button>
</form>
<p class=\"footnote\">Disconnect it any time under Connected apps in your Toki account settings.\
</p>",
        monogram = escape(&monogram),
        email = escape(consent.email),
        action = escape(consent.form_action),
        request = escape(consent.request),
    );
    // `same-origin` rather than `no-referrer`: under `no-referrer`, browsers
    // send `Origin: null` with the form's POST, which the decision endpoint's
    // Origin check would refuse. Cross-origin requests still get no referrer.
    page(
        StatusCode::OK,
        &format!("Connect {} to Toki", consent.client.name),
        &content,
        "same-origin",
    )
}

/// Wraps `content`, which must already be escaped, in the page layout and
/// sets the headers every page carries.
fn page(status: StatusCode, title: &str, content: &str, referrer_policy: &'static str) -> Response {
    let html = format!(
        "<!doctype html>
<html lang=\"en\">
<head>
<meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>{title} · Toki</title>
<link rel=\"stylesheet\" href=\"{FONTS}\">
<style>{STYLE}</style>
</head>
<body>
<main>
{content}
</main>
</body>
</html>
",
        title = escape(title),
    );

    let mut response = (status, Html(html)).into_response();
    let headers = response.headers_mut();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY_VALUE),
    );
    headers.insert(X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static(referrer_policy));
    response
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;

    use super::*;

    async fn body(response: Response) -> String {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[test]
    fn escape_replaces_every_html_special_character() {
        assert_eq!(
            escape(r#"<a href="x" title='y'>Tom & Jerry</a>"#),
            "&lt;a href=&quot;x&quot; title=&#39;y&#39;&gt;Tom &amp; Jerry&lt;/a&gt;"
        );
        assert_eq!(escape("plain åäö"), "plain åäö");
        assert_eq!(escape("&amp;"), "&amp;amp;");
    }

    #[tokio::test]
    async fn error_pages_escape_their_text_and_forbid_framing_and_caching() {
        let response = error_page(
            StatusCode::BAD_REQUEST,
            "Bad <request>",
            "client_id \"<script>\" is unknown",
        );

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let headers = response.headers();
        assert_eq!(headers[CACHE_CONTROL], "no-store");
        assert_eq!(
            headers[CONTENT_SECURITY_POLICY],
            CONTENT_SECURITY_POLICY_VALUE
        );
        assert_eq!(headers[X_FRAME_OPTIONS], "DENY");
        assert_eq!(headers[REFERRER_POLICY], "no-referrer");
        let html = body(response).await;
        assert!(html.contains("<title>Bad &lt;request&gt; · Toki</title>"));
        assert!(html.contains("client_id &quot;&lt;script&gt;&quot; is unknown"));
        assert!(!html.contains("<script>"));
    }

    fn client(kind: OAuthClientKind, id: &str, redirect: &str) -> OAuthClient {
        OAuthClient {
            id: id.to_string(),
            name: "Evil <b>App</b>".to_string(),
            kind,
            redirect_uris: vec![RedirectUri::parse(redirect).unwrap()],
        }
    }

    async fn consent_html(client: &OAuthClient, presented: &str) -> String {
        let response = consent_page(&Consent {
            client,
            email: "ada@example.com",
            redirect_uri: presented,
            registered_redirect_uri: &client.redirect_uris[0],
            request: "tmcp_ar_handle",
            form_action: "https://toki.example/oauth/authorize",
        });
        assert_eq!(response.headers()[REFERRER_POLICY], "same-origin");
        body(response).await
    }

    #[tokio::test]
    async fn consent_names_a_metadata_document_clients_verified_host() {
        let client = client(
            OAuthClientKind::MetadataDocument,
            "https://claude.ai/oauth/mcp-oauth-client-metadata",
            "https://claude.ai/api/mcp/auth_callback",
        );
        let html = consent_html(&client, "https://claude.ai/api/mcp/auth_callback").await;

        assert!(html.contains("Connect Evil &lt;b&gt;App&lt;/b&gt; to Toki"));
        assert!(html.contains("Signed in as <strong>ada@example.com</strong>"));
        assert!(html.contains("Identified by <strong>claude.ai</strong>"));
        assert!(html.contains("go back to <strong>claude.ai</strong>."));
        assert!(!html.contains("can't verify"));
        assert!(!html.contains("Continue only if"));
        assert!(html.contains(r#"<span class="tile">E</span>"#));
        assert!(html.contains(&format!(r#"<img src="{LOGO_PATH}""#)));
        assert!(html.contains(r#"action="https://toki.example/oauth/authorize""#));
        assert!(html.contains(r#"name="request" value="tmcp_ar_handle""#));
        assert!(html.contains(r#"name="decision" value="allow""#));
        assert!(html.contains(r#"name="decision" value="deny""#));
        assert!(!html.contains("<b>"));
        assert!(!html.contains("<script"));
    }

    #[tokio::test]
    async fn consent_warns_about_unverified_and_local_destinations() {
        let loopback = client(
            OAuthClientKind::Registered,
            "0123456789abcdef0123456789abcdef",
            "http://127.0.0.1/callback",
        );
        let html = consent_html(&loopback, "http://127.0.0.1:53682/callback").await;
        assert!(html.contains("registered this name itself, so Toki can't verify it"));
        assert!(html.contains(
            r#"an app on this computer, at <strong class="address">127.0.0.1:53682</strong>"#
        ));
        assert!(html.contains("Continue only if you just started connecting it yourself."));

        let custom = client(
            OAuthClientKind::Registered,
            "0123456789abcdef0123456789abcdef",
            "cursor://anysphere.cursor-mcp/oauth/callback",
        );
        let html = consent_html(&custom, "cursor://anysphere.cursor-mcp/oauth/callback").await;
        assert!(html.contains(r#"opens <strong class="address">cursor://</strong> links"#));
        assert!(html.contains("Continue only if you just started connecting it yourself."));
    }

    #[tokio::test]
    async fn the_logo_is_a_cacheable_png() {
        let response = logo().await;

        assert_eq!(response.headers()[CONTENT_TYPE], "image/png");
        assert!(response.headers()[CACHE_CONTROL]
            .to_str()
            .unwrap()
            .starts_with("public"));
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert!(bytes.starts_with(b"\x89PNG"));
    }
}
