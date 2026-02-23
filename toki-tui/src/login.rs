use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const TUI_CALLBACK_PORT: u16 = 9876;

/// Run the interactive login flow:
/// 1. Start a local HTTP listener on localhost:9876
/// 2. POST to toki-api /login?next=http://localhost:9876/callback
/// 3. Open the returned Azure AD URL in the system browser
/// 4. Wait for the browser to call back with ?session_id=<value>
/// 5. Save the session ID and return it
pub async fn run_login(api_url: &str) -> Result<String> {
    let callback_url = format!("http://localhost:{}/callback", TUI_CALLBACK_PORT);

    // POST /login?next=<callback_url> to get the Azure AD authorization URL
    let client = reqwest::Client::new();
    let auth_url = client
        .post(format!("{}/login", api_url))
        .query(&[("next", &callback_url)])
        .send()
        .await
        .context("Failed to call /login on toki-api. Is toki-api running?")?
        .error_for_status()
        .context("POST /login returned error")?
        .text()
        .await
        .context("Failed to read /login response")?;

    println!("Opening browser for login...");
    println!("If the browser doesn't open, visit:\n  {}\n", auth_url);

    // Open browser
    open_browser(&auth_url);

    // Listen for the callback
    let session_id = wait_for_callback().await?;

    // Save the session
    crate::config::TukiConfig::save_session(&session_id)?;
    println!("Login successful. Session saved.");

    Ok(session_id)
}

/// Open a URL in the system default browser.
fn open_browser(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/c", "start", url]).spawn();
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

    // Read the HTTP request
    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await.context("Failed to read from socket")?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse session_id from query string in the GET line
    // e.g. "GET /callback?session_id=abc123 HTTP/1.1"
    let session_id = parse_session_id(&request)
        .context("Callback did not contain session_id. Login may have failed.")?;

    // Send a simple success response so the browser shows something
    let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
        <html><body><h2>Login successful!</h2><p>You can close this tab.</p></body></html>";
    stream.write_all(response.as_bytes()).await.context("Failed to write response")?;

    Ok(session_id)
}

fn parse_session_id(request: &str) -> Option<String> {
    // Find the GET line
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
