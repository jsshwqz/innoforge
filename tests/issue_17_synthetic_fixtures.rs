//! Issue #17: Synthetic fixtures + fake AI provider stub for CI.
//!
//! Positive: patent detail page has ≥ 22 buttons with fixture data;
//!           OA response page is reachable; git_commit matches HEAD.
//! Negative A: broken AI stub → server still renders, AI endpoint
//!             returns controlled error (not crash/hang).
//! Negative B: fixture has no real patent numbers or applicant names.
//!
//! Each test that starts a server uses its own dynamically allocated
//! port (via TcpListener::bind("127.0.0.1:0")) to avoid collisions
//! when tests run concurrently.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;

const FIXTURE_ID: &str = "synthetic_fixture_001";

// ── Port allocation ──

/// Bind a temporary listener to 127.0.0.1:0, extract the assigned port,
/// then drop the listener to free it. There is a small TOCTOU window
/// where another process could grab the port, but in practice this is
/// fine for test-level allocation.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    listener.local_addr().unwrap().port()
}

// ── Fake AI provider stub ──

fn start_fake_ai_stub(port: u16) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
            Ok(l) => l,
            Err(_) => return,
        };
        for mut stream in listener.incoming().flatten() {
            handle_ai_request(&mut stream);
        }
    })
}

fn handle_ai_request(stream: &mut TcpStream) {
    let mut buf = [0u8; 4096];
    let _ = stream.read(&mut buf);
    let request = String::from_utf8_lossy(&buf);
    let body = if request.contains("/chat/completions") {
        r#"{"choices":[{"message":{"role":"assistant","content":"Synthetic AI response for testing."},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":15,"total_tokens":25}}"#
    } else if request.contains("/embeddings") {
        r#"{"data":[{"embedding":[0.1,0.2,0.3,0.4,0.5]}],"usage":{"prompt_tokens":5,"total_tokens":5}}"#
    } else {
        r#"{"status":"ok"}"#
    };
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(resp.as_bytes());
}

// ── Helpers ──

fn fixture_json() -> String {
    std::fs::read_to_string("tests/fixtures/synthetic_patent.json")
        .expect("fixture file must exist")
}

fn start_server(server_port: u16, ai_port: u16, home_dir: &str) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_innoforge-server"))
        .env("INNOFORGE_PORT", server_port.to_string())
        .env("INNOFORGE_HOME", home_dir)
        .env("AI_BASE_URL", format!("http://127.0.0.1:{}", ai_port))
        .env("AI_API_KEY", "test-key-not-real")
        .spawn()
        .expect("failed to spawn server binary")
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

fn inject_fixture(port: u16) -> bool {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/api/patents/import", port))
        .header("Content-Type", "application/json")
        .body(fixture_json())
        .send();
    match resp {
        Ok(r) => r.status().is_success(),
        Err(_) => false,
    }
}

fn count_buttons(port: u16, path: &str) -> Option<usize> {
    let url = format!("http://127.0.0.1:{}{}", port, path);
    let resp = reqwest::blocking::get(&url).ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let html = resp.text().ok()?;
    Some(html.matches("<button").count() + html.matches("role=\"button\"").count())
}

fn check_http_200(port: u16, path: &str) -> bool {
    let url = format!("http://127.0.0.1:{}{}", port, path);
    reqwest::blocking::get(&url)
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

fn kill_server(mut child: std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

// ── Negative B: fixture privacy guard ──

#[test]
fn fixture_has_no_real_patent_numbers() {
    let fixture = fixture_json();
    let re_cn = regex::Regex::new(r"CN\d{8}[A-Z]").unwrap();
    let re_year_serial = regex::Regex::new(r"\d{4}\d{7}\d").unwrap();
    assert!(
        !re_cn.is_match(&fixture),
        "Fixture contains a CN+digits+letter publication number pattern — prohibited"
    );
    assert!(
        !re_year_serial.is_match(&fixture),
        "Fixture contains a year+serial+checkdigit pattern — prohibited"
    );
}

#[test]
fn fixture_has_no_real_applicant_names() {
    let fixture = fixture_json();
    let forbidden: String = ['\u{738b}', '\u{9759}', '\u{829d}'].iter().collect();
    assert!(
        !fixture.contains(&forbidden),
        "Fixture contains a forbidden applicant name"
    );
}

#[test]
fn fixture_is_marked_synthetic() {
    let fixture = fixture_json();
    assert!(
        fixture.contains("synthetic_fixture") || fixture.contains("虚构"),
        "Fixture must be clearly marked as synthetic/fictional"
    );
}

// ── Positive: patent detail page has data ──

#[test]
fn patent_detail_page_has_22_plus_buttons() {
    let server_port = free_port();
    let ai_port = free_port();
    let home_dir = tempfile::TempDir::new().expect("temp dir for INNOFORGE_HOME");

    let _stub = start_fake_ai_stub(ai_port);
    std::thread::sleep(std::time::Duration::from_millis(500));

    let server = start_server(server_port, ai_port, home_dir.path().to_str().unwrap());
    assert!(
        wait_for_server(server_port, 60),
        "Server must start on port {} within 60s",
        server_port
    );
    assert!(
        inject_fixture(server_port),
        "Fixture injection must succeed"
    );

    let button_count = count_buttons(server_port, &format!("/patent/{}", FIXTURE_ID))
        .expect("Patent detail page must be reachable");
    assert!(
        button_count >= 22,
        "Patent detail page must have >= 22 buttons with data, got {}",
        button_count
    );

    assert!(
        check_http_200(server_port, "/oa-response"),
        "OA response page must return 200"
    );
    assert!(
        check_http_200(server_port, "/"),
        "Home page must return 200"
    );
    assert!(
        check_http_200(server_port, "/idea"),
        "Idea page must return 200"
    );

    // Verify git_commit matches HEAD
    let version_json =
        reqwest::blocking::get(format!("http://127.0.0.1:{}/api/version", server_port))
            .unwrap()
            .text()
            .unwrap();
    let v: serde_json::Value = serde_json::from_str(&version_json).unwrap();
    let api_commit = v["git_commit"].as_str().unwrap();
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let head_str = String::from_utf8(head.stdout).unwrap().trim().to_string();
    assert_eq!(api_commit, head_str, "git_commit must match HEAD");

    kill_server(server);
}

// ── Negative A: broken AI stub ──

#[test]
fn broken_ai_stub_does_not_crash_server() {
    let server_port = free_port();
    let dead_port: u16 = 59999;
    let home_dir = tempfile::TempDir::new().expect("temp dir for INNOFORGE_HOME");

    let server = start_server(server_port, dead_port, home_dir.path().to_str().unwrap());
    assert!(
        wait_for_server(server_port, 60),
        "Server must start on port {} within 60s",
        server_port
    );
    assert!(
        inject_fixture(server_port),
        "Fixture injection must succeed without AI"
    );

    // Patent detail page must still render
    assert!(
        check_http_200(server_port, &format!("/patent/{}", FIXTURE_ID)),
        "Patent detail page must render even when AI stub is unreachable"
    );

    // AI endpoint must return a controlled error, not crash
    let client = reqwest::blocking::Client::new();
    let ai_resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"patent_id":"synthetic_fixture_001","oa_text":"虚构审查意见文本"}"#)
        .timeout(std::time::Duration::from_secs(30))
        .send();
    match ai_resp {
        Ok(r) => {
            let status = r.status();
            assert!(
                status.as_u16() < 500 || status.as_u16() == 503,
                "AI endpoint should return controlled error (got {}), not crash",
                status
            );
        }
        Err(e) => {
            panic!("AI endpoint must not hang or refuse connection: {}", e);
        }
    }

    kill_server(server);
}

// ── Issue #60: test isolation ──

#[test]
fn two_test_cases_resolve_different_db_path() {
    // Each test case gets its own INNOFORGE_HOME (temp dir), so the
    // database files must be in different directories. This verifies
    // test isolation: no shared database state between concurrent tests.
    let dir_a = tempfile::TempDir::new().expect("temp dir A");
    let dir_b = tempfile::TempDir::new().expect("temp dir B");

    // Simulate what the server does: resolve_db_path with INNOFORGE_HOME set.
    // We can't call resolve_db_path directly (it reads env vars globally),
    // but we can verify the paths are different by construction.
    let db_path_a = dir_a.path().join("innoforge.db");
    let db_path_b = dir_b.path().join("innoforge.db");

    assert_ne!(
        db_path_a, db_path_b,
        "two test cases with different INNOFORGE_HOME must resolve different db_path"
    );

    // Verify the directories are actually different (not just the filenames).
    assert_ne!(
        dir_a.path(),
        dir_b.path(),
        "temp dirs must be different directories"
    );

    // Spin up two servers with different INNOFORGE_HOME and verify they
    // don't interfere. Each should start independently and serve its own db.
    let port_a = free_port();
    let port_b = free_port();
    let ai_port = free_port();

    let _stub = start_fake_ai_stub(ai_port);
    std::thread::sleep(std::time::Duration::from_millis(500));

    let server_a = start_server(port_a, ai_port, dir_a.path().to_str().unwrap());
    let server_b = start_server(port_b, ai_port, dir_b.path().to_str().unwrap());

    assert!(
        wait_for_server(port_a, 60),
        "Server A must start on port {}",
        port_a
    );
    assert!(
        wait_for_server(port_b, 60),
        "Server B must start on port {}",
        port_b
    );

    // Both servers should be serving independently.
    assert!(
        check_http_200(port_a, "/api/version"),
        "Server A must respond"
    );
    assert!(
        check_http_200(port_b, "/api/version"),
        "Server B must respond"
    );

    // Verify each server created its own db file in its own INNOFORGE_HOME.
    assert!(
        db_path_a.exists(),
        "Server A should have created a db file in its INNOFORGE_HOME"
    );
    assert!(
        db_path_b.exists(),
        "Server B should have created a db file in its INNOFORGE_HOME"
    );

    kill_server(server_a);
    kill_server(server_b);
}
