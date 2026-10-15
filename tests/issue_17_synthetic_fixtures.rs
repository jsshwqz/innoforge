//! Issue #17: Synthetic fixtures + fake AI provider stub for CI.
//!
//! Positive: patent detail page has ≥ 22 buttons with fixture data;
//!           OA response page is reachable; git_commit matches HEAD.
//! Negative A: broken AI stub → server still renders, AI endpoint
//!             returns controlled error (not crash/hang).
//! Negative B: fixture has no real patent numbers or applicant names.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;

const SERVER_PORT: u16 = 3137;
const AI_STUB_PORT: u16 = 3138;
const FIXTURE_ID: &str = "synthetic_fixture_001";

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

fn start_server(ai_port: u16) -> std::process::Child {
    Command::new("./target/release/innoforge-server")
        .env("INNOFORGE_PORT", SERVER_PORT.to_string())
        .env("AI_BASE_URL", format!("http://127.0.0.1:{}", ai_port))
        .env("AI_API_KEY", "test-key-not-real")
        .spawn()
        .expect("run cargo build --release first")
}

fn wait_for_server(timeout_secs: u64) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < timeout_secs {
        if let Ok(resp) =
            reqwest::blocking::get(format!("http://127.0.0.1:{}/api/version", SERVER_PORT))
        {
            if resp.status().is_success() {
                return true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    false
}

fn inject_fixture() -> bool {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/patents/import",
            SERVER_PORT
        ))
        .header("Content-Type", "application/json")
        .body(fixture_json())
        .send();
    match resp {
        Ok(r) => r.status().is_success(),
        Err(_) => false,
    }
}

fn count_buttons(path: &str) -> Option<usize> {
    let url = format!("http://127.0.0.1:{}{}", SERVER_PORT, path);
    let resp = reqwest::blocking::get(&url).ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let html = resp.text().ok()?;
    Some(html.matches("<button").count() + html.matches("role=\"button\"").count())
}

fn check_http_200(path: &str) -> bool {
    let url = format!("http://127.0.0.1:{}{}", SERVER_PORT, path);
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
    let forbidden: String = ['\u{738b}', '\u{9754}', '\u{829d}'].iter().collect();
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
    let _stub = start_fake_ai_stub(AI_STUB_PORT);
    std::thread::sleep(std::time::Duration::from_millis(500));

    let server = start_server(AI_STUB_PORT);
    assert!(wait_for_server(15), "Server must start within 15s");
    assert!(inject_fixture(), "Fixture injection must succeed");

    let button_count = count_buttons(&format!("/patent/{}", FIXTURE_ID))
        .expect("Patent detail page must be reachable");
    assert!(
        button_count >= 22,
        "Patent detail page must have >= 22 buttons with data, got {}",
        button_count
    );

    assert!(
        check_http_200("/oa-response"),
        "OA response page must return 200"
    );
    assert!(check_http_200("/"), "Home page must return 200");
    assert!(check_http_200("/idea"), "Idea page must return 200");

    // Verify git_commit matches HEAD
    let version_json =
        reqwest::blocking::get(format!("http://127.0.0.1:{}/api/version", SERVER_PORT))
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
    let dead_port: u16 = 59999;
    let server = start_server(dead_port);
    assert!(wait_for_server(15), "Server must start within 15s");
    assert!(
        inject_fixture(),
        "Fixture injection must succeed without AI"
    );

    // Patent detail page must still render
    assert!(
        check_http_200(&format!("/patent/{}", FIXTURE_ID)),
        "Patent detail page must render even when AI stub is unreachable"
    );

    // AI endpoint must return a controlled error, not crash
    let client = reqwest::blocking::Client::new();
    let ai_resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            SERVER_PORT
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
