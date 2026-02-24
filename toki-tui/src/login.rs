use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const TUI_CALLBACK_PORT: u16 = 9876;
const TUI_LOGIN_PORT: u16 = 9875;

/// Run the interactive login flow:
/// 1. Start a local HTTP server on localhost:9875 serving an auto-submit HTML
///    form that POSTs directly to toki-api /login?next=http://localhost:9876/callback.
///    The browser must make the POST so it owns the session cookie — this is
///    required for the CSRF state to survive the Azure AD round-trip.
/// 2. Open the browser to the local form page; it immediately POSTs to /login.
/// 3. /login stores CSRF state in a session cookie and returns 302 → Azure AD.
/// 4. User authenticates; Azure AD redirects back to /oauth/callback on the API.
/// 5. /oauth/callback validates CSRF (using the session cookie the browser holds),
///    logs the user in, then redirects to localhost:9876/callback?session_id=<value>.
/// 6. The TUI captures the session_id and saves it.
pub async fn run_login(api_url: &str) -> Result<String> {
    let callback_url = format!("http://localhost:{}/callback", TUI_CALLBACK_PORT);
    let login_url = format!(
        "{}/login?next={}",
        api_url,
        urlencoding::encode(&callback_url)
    );

    let form_html = format!(
        r#"<!DOCTYPE html>
<html>
<head><title>Toki Login</title></head>
<body>
<p>Logging in to Toki, please wait...</p>
<form id="f" method="POST" action="{login_url}"></form>
<script>document.getElementById('f').submit();</script>
</body>
</html>"#,
        login_url = login_url,
    );

    let form_page_url = format!("http://localhost:{}/", TUI_LOGIN_PORT);
    println!("Opening browser for login...");
    println!("If the browser doesn't open, visit:\n  {}\n", form_page_url);

    let form_html_clone = form_html.clone();
    tokio::spawn(async move {
        serve_one_page(TUI_LOGIN_PORT, form_html_clone).await;
    });

    // Give the listener a moment to bind before opening the browser
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    open_browser(&form_page_url);

    // Wait for the OAuth callback with the session_id
    let session_id = wait_for_callback().await?;

    crate::config::TokiConfig::save_session(&session_id)?;
    println!("Login successful. Session saved.");

    Ok(session_id)
}

/// Serve a single HTML page on the given port, then stop.
/// Accepts up to 10 connections to handle browser pre-connections/favicon/etc,
/// but exits as soon as a GET / request has been served.
async fn serve_one_page(port: u16, html: String) {
    use tokio::net::TcpListener;

    let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Warning: could not bind login helper port {}: {}", port, e);
            return;
        }
    };

    for _ in 0..10 {
        let Ok((mut stream, _)) = listener.accept().await else {
            break;
        };
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);

        let first_line = request.lines().next().unwrap_or("");
        let is_root = first_line.starts_with("GET / ");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
            html.len(),
            html
        );
        let _ = stream.write_all(response.as_bytes()).await;
        if is_root {
            break;
        }
    }
}

/// Open a URL in the system default browser.
fn open_browser(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .spawn();
}

/// Start a minimal HTTP server, wait for one request to /callback?session_id=<value>,
/// return the session_id.
async fn wait_for_callback() -> Result<String> {
    use tokio::net::TcpListener;

    let listener = TcpListener::bind(format!("127.0.0.1:{}", TUI_CALLBACK_PORT))
        .await
        .with_context(|| format!("Failed to bind to port {}", TUI_CALLBACK_PORT))?;

    println!("Waiting for browser callback on port {}...", TUI_CALLBACK_PORT);

    let (mut stream, _) = listener.accept().await.context("Failed to accept connection")?;

    let mut buf = vec![0u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .context("Failed to read from socket")?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // e.g. "GET /callback?session_id=abc123 HTTP/1.1"
    let session_id = parse_session_id(&request)
        .context("Callback did not contain session_id. Login may have failed.")?;

    let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
        <html><body><h2>Login successful!</h2><p>You can close this tab.</p></body></html>";
    stream
        .write_all(response.as_bytes())
        .await
        .context("Failed to write response")?;

    Ok(session_id)
}

fn parse_session_id(request: &str) -> Option<String> {
    let line = request.lines().next()?;
    // e.g. "GET /callback?session_id=abc-123&foo=bar HTTP/1.1"
    let path = line.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;
    for param in query.split('&') {
        let mut parts = param.splitn(2, '=');
        if parts.next() == Some("session_id") {
            return parts.next().map(|v| v.to_string());
        }
    }
    None
}
