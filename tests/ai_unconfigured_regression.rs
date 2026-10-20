//! Regression test for issue #67b: "AI 未配置 — 点击设置" must appear on the
//! homepage when no AI provider is configured.
//!
//! The implementation lives in templates/index.html:431 — the JS checkStatus()
//! function sets the text when /api/settings returns ai_api_key_configured=false.
//! This test verifies both halves:
//!   1. The HTML template contains the "AI 未配置" literal and the status-ai element.
//!   2. The /api/settings endpoint returns ai_api_key_configured=false when no
//!      AI_API_KEY is set, so the frontend can actually trigger the message.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

static DB_TEMPLATE: OnceLock<PathBuf> = OnceLock::new();

fn ensure_db_template() -> PathBuf {
    DB_TEMPLATE
        .get_or_init(|| {
            let template_dir = tempfile::tempdir().expect("create template dir");
            let template_port = free_port();
            let child = Command::new(env!("CARGO_BIN_EXE_innoforge-server"))
                .current_dir(template_dir.path())
                .env("INNOFORGE_PORT", template_port.to_string())
                .env("INNOFORGE_NO_OPEN", "1")
                .env("INNOFORGE_HOME", template_dir.path().to_str().unwrap())
                .spawn()
                .expect("spawn server for template");
            assert!(
                wait_for_server(template_port, 60),
                "template server must start within 60s"
            );
            kill_server(child);
            let db_path = template_dir.path().join("innoforge.db");
            assert!(db_path.exists(), "template db must exist after migration");
            std::mem::forget(template_dir);
            db_path
        })
        .clone()
}

fn fresh_db_dir() -> tempfile::TempDir {
    let template = ensure_db_template();
    let dir = tempfile::tempdir().expect("create fresh db dir");
    std::fs::copy(&template, dir.path().join("innoforge.db")).expect("copy template db");
    dir
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    listener.local_addr().unwrap().port()
}

fn wait_for_server(port: u16, timeout_secs: u64) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < timeout_secs {
        if let Ok(resp) = reqwest::blocking::get(format!("http://127.0.0.1:{}/api/version", port)) {
            if resp.status().is_success() {
                return true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    false
}

fn kill_server(mut child: std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

const INDEX_HTML: &str = include_str!("../templates/index.html");

/// The homepage template must contain the "AI 未配置" literal so the JS
/// checkStatus() function can display it when no provider is configured.
#[test]
fn template_contains_ai_unconfigured_message() {
    assert!(
        INDEX_HTML.contains("AI 未配置"),
        "index.html must contain 'AI 未配置' literal for the unconfigured state"
    );
    assert!(
        INDEX_HTML.contains("status-ai"),
        "index.html must contain the status-ai element id"
    );
    assert!(
        INDEX_HTML.contains("ai_api_key_configured"),
        "index.html must check ai_api_key_configured from /api/settings"
    );
}

/// When no AI_API_KEY is set, /api/settings must return
/// ai_api_key_configured=false so the frontend shows the warning.
#[test]
fn settings_returns_false_when_no_ai_key() {
    let server_port = free_port();

    let home = fresh_db_dir();
    // Start server WITHOUT AI_API_KEY — the env var is deliberately absent.
    let server = Command::new(env!("CARGO_BIN_EXE_innoforge-server"))
        .current_dir(home.path())
        .env("INNOFORGE_PORT", server_port.to_string())
        .env("INNOFORGE_NO_OPEN", "1")
        .env("INNOFORGE_HOME", home.path().to_str().unwrap())
        .env("AI_API_KEY", "")
        .spawn()
        .expect("failed to spawn server");

    assert!(wait_for_server(server_port, 60), "server must start");

    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/api/settings", server_port))
        .expect("/api/settings must respond");
    assert!(resp.status().is_success(), "/api/settings must return 200");

    let json: serde_json::Value = resp.json().expect("valid JSON");
    assert!(
        json["ai_api_key_configured"].is_boolean(),
        "ai_api_key_configured must be a boolean"
    );
    assert_eq!(
        json["ai_api_key_configured"], false,
        "ai_api_key_configured must be false when no AI_API_KEY is set"
    );

    kill_server(server);
}
