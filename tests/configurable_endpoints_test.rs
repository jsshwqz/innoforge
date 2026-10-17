//! Tests for configurable OAuth callback URL and UMI-OCR address (issue #33, G4).
//!
//! Verifies that:
//! 1. `oauth_callback_url()` reads `INNOFORGE_OAUTH_CALLBACK_URL` env var
//! 2. `umi_ocr_base_url()` reads `INNOFORGE_UMI_OCR_URL` env var
//! 3. Defaults are correct when env vars are not set
//! 4. Default callback URL port follows `INNOFORGE_PORT`
//! 5. Explicit callback URL takes priority over port-derived default
//! 6. `/api/version` endpoint includes the configured values
//!
//! Negative verification: before G4, `REDIRECT_URI` and `UMI_OCR_BASE_URL`
//! were hardcoded consts — env vars had no effect, and `/api/version` did
//! not include `oauth_callback_url` or `umi_ocr_url` fields. After G4,
//! these are functions reading env vars, and `/api/version` includes them.

use std::sync::Mutex;

/// Mutex to serialize env-var-dependent tests (env vars are process-global).
/// Use `into_inner()` to recover from poison (a previous test panicked).
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn oauth_callback_url_default_is_localhost_3000() {
    let _guard = lock();
    std::env::remove_var("INNOFORGE_OAUTH_CALLBACK_URL");
    std::env::remove_var("INNOFORGE_PORT");

    let url = innoforge::common::oauth_callback_url();
    assert_eq!(
        url, "http://localhost:3000/api/auth/google/callback",
        "default OAuth callback URL should be localhost:3000"
    );
}

#[test]
fn oauth_callback_url_env_var_overrides_default() {
    let _guard = lock();
    std::env::set_var(
        "INNOFORGE_OAUTH_CALLBACK_URL",
        "http://example.com:8080/auth/callback",
    );

    let url = innoforge::common::oauth_callback_url();
    assert_eq!(
        url, "http://example.com:8080/auth/callback",
        "INNOFORGE_OAUTH_CALLBACK_URL should override default"
    );

    std::env::remove_var("INNOFORGE_OAUTH_CALLBACK_URL");
}

#[test]
fn oauth_callback_url_port_follows_innoforge_port() {
    let _guard = lock();
    std::env::remove_var("INNOFORGE_OAUTH_CALLBACK_URL");
    std::env::set_var("INNOFORGE_PORT", "3921");

    let url = innoforge::common::oauth_callback_url();
    assert_eq!(
        url, "http://localhost:3921/api/auth/google/callback",
        "default callback URL port should follow INNOFORGE_PORT"
    );

    std::env::remove_var("INNOFORGE_PORT");
}

#[test]
fn oauth_callback_url_env_var_takes_priority_over_port() {
    let _guard = lock();
    std::env::set_var(
        "INNOFORGE_OAUTH_CALLBACK_URL",
        "https://custom.example.com/callback",
    );
    std::env::set_var("INNOFORGE_PORT", "5000");

    let url = innoforge::common::oauth_callback_url();
    assert_eq!(
        url, "https://custom.example.com/callback",
        "explicit INNOFORGE_OAUTH_CALLBACK_URL should take priority over INNOFORGE_PORT-derived default"
    );

    std::env::remove_var("INNOFORGE_OAUTH_CALLBACK_URL");
    std::env::remove_var("INNOFORGE_PORT");
}

#[test]
fn umi_ocr_base_url_default_is_localhost_1224() {
    let _guard = lock();
    std::env::remove_var("INNOFORGE_UMI_OCR_URL");

    let url = innoforge::common::umi_ocr_base_url();
    assert_eq!(
        url, "http://127.0.0.1:1224",
        "default UMI-OCR URL should be http://127.0.0.1:1224"
    );
}

#[test]
fn umi_ocr_base_url_env_var_overrides_default() {
    let _guard = lock();
    std::env::set_var("INNOFORGE_UMI_OCR_URL", "http://ocr-server:8080");

    let url = innoforge::common::umi_ocr_base_url();
    assert_eq!(
        url, "http://ocr-server:8080",
        "INNOFORGE_UMI_OCR_URL should override default"
    );

    std::env::remove_var("INNOFORGE_UMI_OCR_URL");
}

#[test]
fn version_endpoint_includes_configurable_fields() {
    // Verify that /api/version JSON includes oauth_callback_url and umi_ocr_url.
    // We test this by starting the server binary and curling /api/version.
    //
    // Negative verification: before G4, /api/version did not include these
    // fields → assertion on missing field would fail → RED.
    // After G4: fields present and match env var values → GREEN.

    let _guard = lock();

    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::process::Command;

    // Find a free port for the server
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let server_port = listener.local_addr().unwrap().port();
    drop(listener);

    // Start server with custom env vars
    let mut child = Command::new(env!("CARGO_BIN_EXE_innoforge-server"))
        .env("INNOFORGE_PORT", server_port.to_string())
        .env("INNOFORGE_NO_OPEN", "1")
        .env(
            "INNOFORGE_OAUTH_CALLBACK_URL",
            "http://test-host:9999/callback",
        )
        .env("INNOFORGE_UMI_OCR_URL", "http://test-ocr:2222")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("start server");

    // Wait for server to be ready
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Make HTTP request to /api/version
    let result = (|| {
        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", server_port))?;
        let request = "GET /api/version HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
        stream.write_all(request.as_bytes())?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        Ok::<String, std::io::Error>(response)
    })();

    // Clean up server
    let _ = child.kill();
    let _ = child.wait();

    let response = result.expect("connect and read from server");

    // Parse JSON from response body (skip HTTP headers)
    let body_start = response.find("\r\n\r\n").expect("find body start");
    let body = &response[body_start + 4..];

    // Handle chunked encoding — find JSON start
    let json_start = body.find('{').expect("find JSON start");
    let json_str = &body[json_start..];

    let json: serde_json::Value = serde_json::from_str(json_str)
        .unwrap_or_else(|e| panic!("failed to parse JSON: {}\nbody: {}", e, body));

    // Verify oauth_callback_url is present and matches
    assert!(
        json.get("oauth_callback_url").is_some(),
        "/api/version must include oauth_callback_url field"
    );
    assert_eq!(
        json["oauth_callback_url"], "http://test-host:9999/callback",
        "/api/version oauth_callback_url should match env var"
    );

    // Verify umi_ocr_url is present and matches
    assert!(
        json.get("umi_ocr_url").is_some(),
        "/api/version must include umi_ocr_url field"
    );
    assert_eq!(
        json["umi_ocr_url"], "http://test-ocr:2222",
        "/api/version umi_ocr_url should match env var"
    );
}
