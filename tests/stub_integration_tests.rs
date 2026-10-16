//! Step 4: Extended stub-based integration tests.
//!
//! These tests use the fake AI stub to verify server behavior without
//! any external network calls. They cover:
//! - Stub response correctness (chat, embeddings, unknown endpoints)
//! - Server + stub integration (AI endpoint returns synthetic response)
//! - Concurrent request handling
//! - Malformed input graceful handling
//! - Stub lifecycle (start, serve, drop)

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;

// ── Port allocation ──

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    listener.local_addr().unwrap().port()
}

// ── Fake AI stub (self-contained for this test file) ──

fn start_stub(port: u16) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
            Ok(l) => l,
            Err(_) => return,
        };
        for mut stream in listener.incoming().flatten() {
            handle_stub(&mut stream);
        }
    })
}

fn handle_stub(stream: &mut TcpStream) {
    let mut buf = [0u8; 8192];
    let _ = stream.read(&mut buf);
    let request = String::from_utf8_lossy(&buf);
    let body = if request.contains("/chat/completions") {
        r#"{"choices":[{"message":{"role":"assistant","content":"Synthetic response."},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#
    } else if request.contains("/embeddings") {
        r#"{"data":[{"embedding":[0.1,0.2,0.3]}],"usage":{"prompt_tokens":3,"total_tokens":3}}"#
    } else {
        r#"{"status":"ok","message":"unknown endpoint"}"#
    };
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(resp.as_bytes());
}

// ── Server helpers ──

fn start_server(server_port: u16, ai_port: u16) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_innoforge-server"))
        .env("INNOFORGE_PORT", server_port.to_string())
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

// ── Stub response correctness ──

/// Verify the stub returns a valid chat completion response.
#[test]
fn stub_chat_completions_returns_valid_json() {
    let port = free_port();
    let _handle = start_stub(port);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/chat/completions", port))
        .header("Content-Type", "application/json")
        .body(r#"{"model":"test","messages":[]}"#)
        .send()
        .expect("stub must respond");

    assert!(resp.status().is_success(), "stub must return 200");
    let json: serde_json::Value = resp.json().expect("stub must return valid JSON");
    assert!(
        json["choices"].is_array(),
        "response must have choices array"
    );
    assert!(
        json["choices"][0]["message"]["content"].is_string(),
        "response must have message content"
    );
}

/// Verify the stub returns a valid embeddings response.
#[test]
fn stub_embeddings_returns_valid_json() {
    let port = free_port();
    let _handle = start_stub(port);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/embeddings", port))
        .header("Content-Type", "application/json")
        .body(r#"{"input":"test"}"#)
        .send()
        .expect("stub must respond");

    assert!(resp.status().is_success());
    let json: serde_json::Value = resp.json().expect("valid JSON");
    assert!(
        json["data"].is_array(),
        "embeddings response must have data array"
    );
    assert!(
        json["data"][0]["embedding"].is_array(),
        "embedding must be an array"
    );
}

/// Verify the stub handles unknown endpoints gracefully.
#[test]
fn stub_unknown_endpoint_returns_ok() {
    let port = free_port();
    let _handle = start_stub(port);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/unknown", port))
        .expect("stub must respond");
    assert!(
        resp.status().is_success(),
        "unknown endpoint should still return 200"
    );
}

// ── Server + stub integration ──

/// Verify the server starts and serves pages when using the stub as AI backend.
#[test]
fn server_serves_pages_with_stub_ai() {
    let server_port = free_port();
    let ai_port = free_port();

    let _stub = start_stub(ai_port);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let server = start_server(server_port, ai_port);
    assert!(wait_for_server(server_port, 15), "server must start");

    // Key pages must be reachable
    for path in &["/", "/idea", "/search", "/compare"] {
        let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}{}", server_port, path))
            .expect("page must respond");
        assert!(
            resp.status().is_success(),
            "page {} must return 200, got {}",
            path,
            resp.status()
        );
    }

    kill_server(server);
}

/// Verify the /api/version endpoint returns correct version info.
#[test]
fn version_endpoint_with_stub_ai() {
    let server_port = free_port();
    let ai_port = free_port();

    let _stub = start_stub(ai_port);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let server = start_server(server_port, ai_port);
    assert!(wait_for_server(server_port, 15));

    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/api/version", server_port))
        .expect("version endpoint must respond");
    assert!(resp.status().is_success());
    let json: serde_json::Value = resp.json().expect("valid JSON");
    assert!(
        json["version"].is_string(),
        "version endpoint must return version string"
    );

    kill_server(server);
}

// ── Malformed input handling ──

/// Verify the server doesn't crash on malformed AI request body.
#[test]
fn malformed_ai_request_does_not_crash() {
    let server_port = free_port();
    let ai_port = free_port();

    let _stub = start_stub(ai_port);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let server = start_server(server_port, ai_port);
    assert!(wait_for_server(server_port, 15));

    let client = reqwest::blocking::Client::new();

    // Send malformed JSON to AI endpoint
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"this is not valid json"#)
        .timeout(std::time::Duration::from_secs(30))
        .send();

    match resp {
        Ok(r) => {
            let status = r.status().as_u16();
            // Should be a 4xx error, not 5xx crash
            assert!(
                status < 500,
                "malformed input should return 4xx, not 5xx crash (got {})",
                status
            );
        }
        Err(e) => {
            panic!("server must not hang on malformed input: {}", e);
        }
    }

    // Server must still be alive after the malformed request
    let alive = reqwest::blocking::get(format!("http://127.0.0.1:{}/api/version", server_port))
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    assert!(alive, "server must still respond after malformed request");

    kill_server(server);
}

/// Verify the server handles empty request body gracefully.
#[test]
fn empty_ai_request_does_not_crash() {
    let server_port = free_port();
    let ai_port = free_port();

    let _stub = start_stub(ai_port);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let server = start_server(server_port, ai_port);
    assert!(wait_for_server(server_port, 15));

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body("")
        .timeout(std::time::Duration::from_secs(30))
        .send();

    match resp {
        Ok(r) => {
            let status = r.status().as_u16();
            assert!(
                status < 500,
                "empty body should return 4xx, not 5xx (got {})",
                status
            );
        }
        Err(e) => {
            panic!("server must not hang on empty input: {}", e);
        }
    }

    kill_server(server);
}

// ── Concurrent request handling ──

/// Verify the stub handles multiple concurrent requests without panicking.
#[test]
fn stub_handles_concurrent_requests() {
    let port = free_port();
    let _handle = start_stub(port);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let mut handles = Vec::new();
    for i in 0..5 {
        let url = format!("http://127.0.0.1:{}/chat/completions", port);
        handles.push(std::thread::spawn(move || {
            let client = reqwest::blocking::Client::new();
            let body = format!(
                r#"{{"model":"test","messages":[{{"role":"user","content":"msg{}"}}]}}"#,
                i
            );
            let resp = client
                .post(&url)
                .header("Content-Type", "application/json")
                .body(body)
                .send()
                .expect("concurrent request must succeed");
            resp.status().is_success()
        }));
    }

    for handle in handles {
        let ok = handle.join().expect("thread must not panic");
        assert!(ok, "concurrent request must return 200");
    }
}

/// Verify the server handles multiple concurrent page requests.
#[test]
fn server_handles_concurrent_page_requests() {
    let server_port = free_port();
    let ai_port = free_port();

    let _stub = start_stub(ai_port);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let server = start_server(server_port, ai_port);
    assert!(wait_for_server(server_port, 15));

    let mut handles = Vec::new();
    for i in 0..4 {
        let url = format!("http://127.0.0.1:{}/", server_port);
        handles.push(std::thread::spawn(move || {
            let resp = reqwest::blocking::get(&url).expect("concurrent page request must succeed");
            resp.status().is_success()
        }));
        let _ = i; // suppress unused warning
    }

    for handle in handles {
        let ok = handle.join().expect("thread must not panic");
        assert!(ok, "concurrent page request must return 200");
    }

    kill_server(server);
}

// ── Server resilience without AI ──

/// Verify the server starts and serves pages even when AI is completely unreachable.
#[test]
fn server_serves_pages_without_ai() {
    let server_port = free_port();
    let dead_ai_port = free_port(); // nothing listening here

    let server = start_server(server_port, dead_ai_port);
    assert!(
        wait_for_server(server_port, 15),
        "server must start without AI"
    );

    // Pages must still render
    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/", server_port))
        .expect("home page must respond");
    assert!(
        resp.status().is_success(),
        "home page must return 200 without AI"
    );

    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/idea", server_port))
        .expect("idea page must respond");
    assert!(
        resp.status().is_success(),
        "idea page must return 200 without AI"
    );

    kill_server(server);
}
