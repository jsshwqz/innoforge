//! Negative control: three types of malformed AI stub responses.
//!
//! For each type (empty body, HTML error page, truncated/overlong JSON),
//! we provide a red-green pair:
//! - Red: a stub that returns the malformed response, and a test that
//!   verifies the server does NOT crash (if it crashes, test fails = red).
//! - Green: a stub that returns a valid response, and the same test
//!   passes = green.
//!
//! All ports use 127.0.0.1:0 (OS-assigned) per test. No std::env::set_var
//! — env vars are passed via Command::env() to the child process only.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    listener.local_addr().unwrap().port()
}

fn start_server(server_port: u16, ai_port: u16) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_innoforge-server"))
        .env("INNOFORGE_PORT", server_port.to_string())
        .env("INNOFORGE_NO_OPEN", "1")
        .env("AI_BASE_URL", format!("http://127.0.0.1:{}", ai_port))
        .env("AI_API_KEY", "test-key-not-real")
        .spawn()
        .expect("failed to spawn server")
}

fn wait_for_server(port: u16, timeout_secs: u64) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < timeout_secs {
        if let Ok(resp) = reqwest::blocking::get(format!("http://127.0.0.1:{}/api/version", port)) {
            if resp.status().is_success() {
                return true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    false
}

fn kill_server(mut child: std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Start a stub that returns a fixed raw response for all requests.
fn start_raw_stub(port: u16, status: u16, body: &str) -> std::thread::JoinHandle<()> {
    let body = body.to_string();
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
            Ok(l) => l,
            Err(_) => return,
        };
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let resp = format!(
                "HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                status,
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    })
}

/// Start a stub that returns a fixed raw response with custom content-type.
fn start_raw_stub_ct(port: u16, content_type: &str, body: &str) -> std::thread::JoinHandle<()> {
    let body = body.to_string();
    let ct = content_type.to_string();
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
            Ok(l) => l,
            Err(_) => return,
        };
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n{}",
                ct,
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    })
}

// ═══════════════════════════════════════════════════════════════════
// Type 1: Empty body
// ═══════════════════════════════════════════════════════════════════

/// Red: stub returns empty body. Server must not crash; AI endpoint
/// should return a controlled error (4xx/5xx), not hang.
#[test]
fn red_empty_body_stub_response() {
    let server_port = free_port();
    let ai_port = free_port();

    let _stub = start_raw_stub(ai_port, 200, "");
    std::thread::sleep(std::time::Duration::from_millis(200));

    let server = start_server(server_port, ai_port);
    assert!(wait_for_server(server_port, 15), "server must start");

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"patent_id":"test","oa_text":"测试文本"}"#)
        .timeout(std::time::Duration::from_secs(30))
        .send();

    match resp {
        Ok(r) => {
            let status = r.status().as_u16();
            assert!(
                status < 500 || status == 503,
                "empty body from AI: expected controlled error, got {}",
                status
            );
        }
        Err(e) => {
            panic!("server must not hang on empty AI response: {}", e);
        }
    }

    // Server must still be alive
    let alive = reqwest::blocking::get(format!("http://127.0.0.1:{}/api/version", server_port))
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    assert!(alive, "server must survive empty AI response");

    kill_server(server);
}

/// Green: stub returns valid JSON. Server processes normally.
#[test]
fn green_valid_body_stub_response() {
    let server_port = free_port();
    let ai_port = free_port();

    let valid_json = r#"{"choices":[{"message":{"role":"assistant","content":"OK"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
    let _stub = start_raw_stub(ai_port, 200, valid_json);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let server = start_server(server_port, ai_port);
    assert!(wait_for_server(server_port, 15), "server must start");

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"patent_id":"test","oa_text":"测试文本"}"#)
        .timeout(std::time::Duration::from_secs(30))
        .send();

    match resp {
        Ok(r) => {
            let status = r.status().as_u16();
            assert!(
                status < 500,
                "valid AI response: expected success or 4xx, got {}",
                status
            );
        }
        Err(e) => {
            panic!("server must not hang on valid AI response: {}", e);
        }
    }

    kill_server(server);
}

// ═══════════════════════════════════════════════════════════════════
// Type 2: HTML error page (wrong content-type)
// ═══════════════════════════════════════════════════════════════════

/// Red: stub returns HTML error page instead of JSON. Server must not crash.
#[test]
fn red_html_error_page_stub_response() {
    let server_port = free_port();
    let ai_port = free_port();

    let html = "<html><body><h1>503 Service Unavailable</h1></body></html>";
    let _stub = start_raw_stub_ct(ai_port, "text/html", html);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let server = start_server(server_port, ai_port);
    assert!(wait_for_server(server_port, 15), "server must start");

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"patent_id":"test","oa_text":"测试文本"}"#)
        .timeout(std::time::Duration::from_secs(30))
        .send();

    match resp {
        Ok(r) => {
            let status = r.status().as_u16();
            assert!(
                status < 500 || status == 503,
                "HTML error from AI: expected controlled error, got {}",
                status
            );
        }
        Err(e) => {
            panic!("server must not hang on HTML AI response: {}", e);
        }
    }

    // Server must still be alive
    let alive = reqwest::blocking::get(format!("http://127.0.0.1:{}/api/version", server_port))
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    assert!(alive, "server must survive HTML AI response");

    kill_server(server);
}

/// Green: stub returns valid JSON with correct content-type.
#[test]
fn green_json_content_type_stub_response() {
    let server_port = free_port();
    let ai_port = free_port();

    let valid_json =
        r#"{"choices":[{"message":{"role":"assistant","content":"OK"},"finish_reason":"stop"}]}"#;
    let _stub = start_raw_stub_ct(ai_port, "application/json", valid_json);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let server = start_server(server_port, ai_port);
    assert!(wait_for_server(server_port, 15), "server must start");

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"patent_id":"test","oa_text":"测试文本"}"#)
        .timeout(std::time::Duration::from_secs(30))
        .send();

    match resp {
        Ok(r) => {
            assert!(
                r.status().as_u16() < 500,
                "valid JSON response should not cause 5xx"
            );
        }
        Err(e) => {
            panic!("server must not hang: {}", e);
        }
    }

    kill_server(server);
}

// ═══════════════════════════════════════════════════════════════════
// Type 3: Truncated / overlong JSON
// ═══════════════════════════════════════════════════════════════════

/// Red: stub returns truncated JSON (cut mid-string). Server must not crash.
#[test]
fn red_truncated_json_stub_response() {
    let server_port = free_port();
    let ai_port = free_port();

    // Truncated: missing closing braces and quote
    let truncated = r#"{"choices":[{"message":{"role":"assistant","content":"OK"#;
    let _stub = start_raw_stub(ai_port, 200, truncated);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let server = start_server(server_port, ai_port);
    assert!(wait_for_server(server_port, 15), "server must start");

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"patent_id":"test","oa_text":"测试文本"}"#)
        .timeout(std::time::Duration::from_secs(30))
        .send();

    match resp {
        Ok(r) => {
            let status = r.status().as_u16();
            assert!(
                status < 500 || status == 503,
                "truncated JSON from AI: expected controlled error, got {}",
                status
            );
        }
        Err(e) => {
            panic!("server must not hang on truncated JSON: {}", e);
        }
    }

    // Server must still be alive
    let alive = reqwest::blocking::get(format!("http://127.0.0.1:{}/api/version", server_port))
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    assert!(alive, "server must survive truncated JSON");

    kill_server(server);
}

/// Red: stub returns overlong JSON (10KB of padding). Server must not crash.
#[test]
fn red_overlong_json_stub_response() {
    let server_port = free_port();
    let ai_port = free_port();

    // Overlong: valid JSON but with 10KB of padding in content
    let padding = "x".repeat(10240);
    let overlong = format!(
        r#"{{"choices":[{{"message":{{"role":"assistant","content":"{}"}},"finish_reason":"stop"}}]}}"#,
        padding
    );
    let _stub = start_raw_stub(ai_port, 200, &overlong);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let server = start_server(server_port, ai_port);
    assert!(wait_for_server(server_port, 15), "server must start");

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"patent_id":"test","oa_text":"测试文本"}"#)
        .timeout(std::time::Duration::from_secs(30))
        .send();

    match resp {
        Ok(r) => {
            let status = r.status().as_u16();
            assert!(
                status < 500,
                "overlong JSON from AI: expected success or 4xx, got {}",
                status
            );
        }
        Err(e) => {
            panic!("server must not hang on overlong JSON: {}", e);
        }
    }

    // Server must still be alive
    let alive = reqwest::blocking::get(format!("http://127.0.0.1:{}/api/version", server_port))
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    assert!(alive, "server must survive overlong JSON");

    kill_server(server);
}

/// Green: stub returns well-formed complete JSON. Server processes normally.
#[test]
fn green_complete_json_stub_response() {
    let server_port = free_port();
    let ai_port = free_port();

    let valid = r#"{"choices":[{"message":{"role":"assistant","content":"完整的AI回复"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":10,"total_tokens":15}}"#;
    let _stub = start_raw_stub(ai_port, 200, valid);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let server = start_server(server_port, ai_port);
    assert!(wait_for_server(server_port, 15), "server must start");

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"patent_id":"test","oa_text":"测试文本"}"#)
        .timeout(std::time::Duration::from_secs(30))
        .send();

    match resp {
        Ok(r) => {
            assert!(
                r.status().as_u16() < 500,
                "valid response should not cause 5xx"
            );
        }
        Err(e) => {
            panic!("server must not hang: {}", e);
        }
    }

    kill_server(server);
}
