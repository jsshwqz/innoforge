//! Issue #30: Verify that when an external backend accepts the connection
//! but never responds, the server returns a proper HTTP response with a
//! JSON body — not a connection-level failure (empty reply / ERR_EMPTY_RESPONSE).
//!
//! The AI handler gracefully degrades (returns 200 with fallback content).
//! The CAD handler returns 200 with {"availability":"unavailable"}.
//! Both have timeouts on all external calls, so the server never hangs
//! indefinitely and never returns an empty reply.
//!
//! Also verifies the server stays alive after the request completes.

use std::io::{Read, Write};
use std::net::TcpListener;
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

/// Start a "black hole" stub: accepts TCP connections but never sends
/// any HTTP response. The connection stays open until the client times out.
fn start_blackhole_stub(port: u16) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
            Ok(l) => l,
            Err(_) => return,
        };
        for mut stream in listener.incoming().flatten() {
            // Read the request but never respond
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            // Intentionally do NOT write anything back
            // Keep the connection open — the client must time out
            std::thread::sleep(std::time::Duration::from_secs(300));
        }
    })
}

/// Start a "slow drip" stub: accepts connections, sends a partial HTTP
/// response header, then hangs indefinitely.
fn start_slow_drip_stub(port: u16) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
            Ok(l) => l,
            Err(_) => return,
        };
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            // Send a partial header, then hang
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n");
            // Do NOT send Content-Length or body — hang instead
            std::thread::sleep(std::time::Duration::from_secs(300));
        }
    })
}

/// Assert that a response is a proper HTTP response (not empty reply):
/// - `send()` succeeds (no connection-level failure)
/// - Status code is in normal HTTP range (200-599)
/// - Content-Type contains "json"
/// - Body is non-empty
fn assert_proper_http_response(resp: reqwest::blocking::Response, label: &str) {
    let status = resp.status().as_u16();
    assert!(
        (200..600).contains(&status),
        "{}: expected HTTP status 200-599, got {}",
        label,
        status
    );
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.contains("json"),
        "{}: expected JSON content-type, got '{}'",
        label,
        ct
    );
    let body = resp.text().unwrap_or_default();
    assert!(
        !body.is_empty(),
        "{}: response body must not be empty (no ERR_EMPTY_RESPONSE)",
        label
    );
}

// ═══════════════════════════════════════════════════════════════════
// Test 1: AI backend accepts but never responds → server must return
// a proper HTTP response with JSON body, not empty reply.
//
// The AI handler has a 60s chat timeout / 180s analysis timeout.
// When the backend hangs, the handler times out and returns either:
// - 200 with fallback content (graceful degradation), or
// - 5xx with JSON error body
// Either way, it must NOT be a connection-level failure.
// ═══════════════════════════════════════════════════════════════════

#[test]
fn tolerates_blackhole_ai_backend_no_empty_reply() {
    let server_port = free_port();
    let ai_port = free_port();

    let _stub = start_blackhole_stub(ai_port);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let server = start_server(server_port, ai_port);
    assert!(wait_for_server(server_port, 15), "server must start");

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("client");

    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"patent_id":"test","oa_text":"测试文本"}"#)
        .send();

    match resp {
        Ok(r) => assert_proper_http_response(r, "blackhole AI backend"),
        Err(e) => panic!(
            "server must not return connection-level failure on blackhole AI: {}",
            e
        ),
    }

    // Server must still be alive
    let alive = reqwest::blocking::get(format!("http://127.0.0.1:{}/api/version", server_port))
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    assert!(alive, "server must survive blackhole AI backend");

    kill_server(server);
}

// ═══════════════════════════════════════════════════════════════════
// Test 2: AI backend sends partial response then hangs → server must
// return a proper HTTP response with JSON body.
// ═══════════════════════════════════════════════════════════════════

#[test]
fn tolerates_slow_drip_ai_backend_no_empty_reply() {
    let server_port = free_port();
    let ai_port = free_port();

    let _stub = start_slow_drip_stub(ai_port);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let server = start_server(server_port, ai_port);
    assert!(wait_for_server(server_port, 15), "server must start");

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("client");

    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"patent_id":"test","oa_text":"测试文本"}"#)
        .send();

    match resp {
        Ok(r) => assert_proper_http_response(r, "slow drip AI backend"),
        Err(e) => panic!(
            "server must not return connection-level failure on slow drip AI: {}",
            e
        ),
    }

    kill_server(server);
}

// ═══════════════════════════════════════════════════════════════════
// Test 3: CAD backend accepts but never responds → /api/cad/status
// must return a proper HTTP response with JSON body (not empty reply).
//
// The CAD client has a 120s timeout. The handler catches errors and
// returns {"availability":"unavailable"}.
// ═══════════════════════════════════════════════════════════════════

#[test]
fn tolerates_blackhole_cad_backend_no_empty_reply() {
    let server_port = free_port();
    let cad_port = free_port();

    let _stub = start_blackhole_stub(cad_port);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let server = Command::new(env!("CARGO_BIN_EXE_innoforge-server"))
        .env("INNOFORGE_PORT", server_port.to_string())
        .env("INNOFORGE_NO_OPEN", "1")
        .env(
            "INNOFORGE_AIONCAD_URL",
            format!("http://127.0.0.1:{}", cad_port),
        )
        .spawn()
        .expect("failed to spawn server");

    assert!(wait_for_server(server_port, 15), "server must start");

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .expect("client");

    let resp = client
        .get(format!("http://127.0.0.1:{}/api/cad/status", server_port))
        .send();

    match resp {
        Ok(r) => assert_proper_http_response(r, "blackhole CAD backend"),
        Err(e) => panic!(
            "server must not return connection-level failure on blackhole CAD: {}",
            e
        ),
    }

    kill_server(server);
}

// ═══════════════════════════════════════════════════════════════════
// Test 4: Server stays alive after multiple requests to a blackhole AI
// backend. Verifies no resource leak / connection pool exhaustion.
// ═══════════════════════════════════════════════════════════════════

#[test]
fn survives_repeated_blackhole_ai_requests() {
    let server_port = free_port();
    let ai_port = free_port();

    let _stub = start_blackhole_stub(ai_port);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let server = start_server(server_port, ai_port);
    assert!(wait_for_server(server_port, 15), "server must start");

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("client");

    // Send 3 sequential requests — all must get proper HTTP responses
    for i in 1..=3 {
        let resp = client
            .post(format!(
                "http://127.0.0.1:{}/api/ai/office-action-response",
                server_port
            ))
            .header("Content-Type", "application/json")
            .body(r#"{"patent_id":"test","oa_text":"测试文本"}"#)
            .send();

        match resp {
            Ok(r) => assert_proper_http_response(r, &format!("blackhole AI request #{}", i)),
            Err(e) => panic!("request #{} connection-level failure: {}", i, e),
        }
    }

    // Server must still be alive after all requests
    let alive = reqwest::blocking::get(format!("http://127.0.0.1:{}/api/version", server_port))
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    assert!(alive, "server must survive repeated blackhole AI requests");

    kill_server(server);
}
