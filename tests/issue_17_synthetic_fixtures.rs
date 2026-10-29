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
//!
//! R21: Uses shared `tests/common/mod.rs` — pre-migrated DB template +
//! readiness polling (every 200ms, 60s cap). No fixed sleep for server startup.

mod common;

use common::{free_port, spawn_test_server};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

const FIXTURE_ID: &str = "synthetic_fixture_001";

// ── Fake AI provider stub ──

/// 先 bind(0) 拿到已监听的端口再起线程，返回 `(port, handle)`。
/// 不要退回"先 free_port() 再在线程内 bind"：两步之间的空窗会被并行用例抢走端口，
/// 桩线程 bind 失败静默退出，客户端 Connection refused（CI run 40630679117 实测）。
fn start_fake_ai_stub() -> (u16, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("stub must bind an ephemeral port");
    let port = listener.local_addr().expect("stub local_addr").port();
    let handle = std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            handle_ai_request(&mut stream);
        }
    });
    (port, handle)
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

/// Helper: spawn a test server with AI pointing at `ai_port`.
fn start_server(server_port: u16, ai_port: u16) -> common::TestServer {
    let ai_url = format!("http://127.0.0.1:{}", ai_port);
    spawn_test_server(
        server_port,
        &[
            ("AI_BASE_URL", &ai_url),
            ("AI_API_KEY", "test-key-not-real"),
        ],
    )
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

    let (ai_port, _stub) = start_fake_ai_stub();

    let server = start_server(server_port, ai_port);
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

    // Verify build info injection is working: git_commit must be non-empty,
    // have valid commit shape (40-char hex or "unknown"), and match the
    // build-time constant env!("GIT_COMMIT") — NOT the runtime HEAD.
    // Coupling to runtime HEAD causes false reds when a commit lands between
    // build and test run (issue 109).
    let version_json =
        reqwest::blocking::get(format!("http://127.0.0.1:{}/api/version", server_port))
            .unwrap()
            .text()
            .unwrap();
    let v: serde_json::Value = serde_json::from_str(&version_json).unwrap();
    let api_commit = v["git_commit"].as_str().unwrap();

    // 1. Non-empty
    assert!(
        !api_commit.is_empty(),
        "git_commit must be non-empty (build info injection must work)"
    );

    // 2. Valid shape: either "unknown" (no git) or a 40-char lowercase hex sha
    let is_unknown = api_commit == "unknown";
    let is_full_sha = api_commit.len() == 40
        && api_commit
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
    assert!(
        is_unknown || is_full_sha,
        "git_commit must be 'unknown' or a 40-char hex sha, got: {}",
        api_commit
    );

    // 3. Same-source comparison: the API value must match the build-time
    //    constant baked into this test binary by the same build.rs.
    //    This proves the injection flows correctly from build.rs -> env! -> API
    //    without depending on the runtime working-tree HEAD.
    let build_time_commit = env!("GIT_COMMIT");
    assert_eq!(
        api_commit, build_time_commit,
        "git_commit from API must match the build-time GIT_COMMIT constant"
    );

    drop(server);
}

// ── Negative A: broken AI stub ──

#[test]
fn broken_ai_stub_does_not_crash_server() {
    let server_port = free_port();
    let dead_port: u16 = 59999;

    let server = start_server(server_port, dead_port);
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

    drop(server);
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

    let (ai_port, _stub) = start_fake_ai_stub();

    let ai_url = format!("http://127.0.0.1:{}", ai_port);
    let server_a = spawn_test_server(
        port_a,
        &[
            ("AI_BASE_URL", &ai_url),
            ("AI_API_KEY", "test-key-not-real"),
        ],
    );
    let server_b = spawn_test_server(
        port_b,
        &[
            ("AI_BASE_URL", &ai_url),
            ("AI_API_KEY", "test-key-not-real"),
        ],
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
        server_a
            .home
            .as_ref()
            .unwrap()
            .path()
            .join("innoforge.db")
            .exists(),
        "Server A should have a db file in its INNOFORGE_HOME"
    );
    assert!(
        server_b
            .home
            .as_ref()
            .unwrap()
            .path()
            .join("innoforge.db")
            .exists(),
        "Server B should have a db file in its INNOFORGE_HOME"
    );

    drop(server_a);
    drop(server_b);
}
