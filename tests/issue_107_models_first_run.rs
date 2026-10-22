//! Regression for issue #107: first-run default (Ollama) makes
//! /api/ai/models return an empty providers array.
//!
//! Root cause: two separate dedup conditions in src/routes/idea.rs
//! both skipped the Ollama entry when the configured URL WAS Ollama,
//! leaving providers == [].
//!
//! Fix (#107): single dedup pass — when the configured URL matches a
//! known provider, merge into one entry with the known name and
//! `configured: true`.  Providers must never be empty.
//!
//! Negative control: reverting to the old two-skip logic makes
//! `first_run_ollama_not_empty` fail (providers is empty, Ollama not found).

mod common;
use common::{free_port, spawn_test_server};
use std::io::{Read, Write};
use std::net::TcpListener;

/// In-process stub that responds to GET /v1/models with one model.
/// Uses TcpListener + thread — no external interpreter.
struct ModelsStub {
    _handle: std::thread::JoinHandle<()>,
    port: u16,
}

impl ModelsStub {
    fn start() -> Self {
        let port = free_port();
        let handle = std::thread::spawn(move || {
            let listener = match TcpListener::bind(("127.0.0.1", port)) {
                Ok(l) => l,
                Err(_) => return,
            };
            for mut stream in listener.incoming().flatten() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let req = String::from_utf8_lossy(&buf);
                if req.contains("/models") {
                    let body = r#"{"data":[{"id":"stub-model-107"}]}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes());
                } else {
                    let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n");
                }
            }
        });
        // Ready check: poll until the port accepts connections
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
        }
        // Verify stub is actually up
        if std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from((std::net::Ipv4Addr::new(127, 0, 0, 1), port)),
            std::time::Duration::from_secs(2),
        )
        .is_err()
        {
            panic!(
                "stub did not start: could not connect to 127.0.0.1:{}",
                port
            );
        }
        ModelsStub {
            _handle: handle,
            port,
        }
    }
}

/// #107 positive A: first-run default (AI_BASE_URL = Ollama, no Ollama running)
/// ⇒ providers must contain Ollama with status != "ok", top-level status == "error".
/// This is the case that was broken: providers was [] after PR #104.
#[test]
fn first_run_ollama_not_empty() {
    let port = free_port();
    // No AI_BASE_URL set → server defaults to http://localhost:11434/v1 (Ollama)
    // No Ollama is running in the test environment
    let server = spawn_test_server(port, &[]);

    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(format!("http://127.0.0.1:{}/api/ai/models", port))
        .send()
        .expect("models must respond");
    assert!(resp.status().is_success());
    let json: serde_json::Value = resp.json().expect("models json");

    let providers = json["providers"]
        .as_array()
        .expect("providers must be an array");

    // Hard constraint: providers must NOT be empty
    assert!(
        !providers.is_empty(),
        "#107: providers must not be empty on first run (was [] after PR #104)"
    );

    // Ollama must be present
    let ollama = providers
        .iter()
        .find(|p| p["name"].as_str() == Some("Ollama"));
    assert!(
        ollama.is_some(),
        "#107: providers must contain Ollama entry on first run, got: {:?}",
        providers
    );

    if let Some(oll) = ollama {
        let status = oll["status"].as_str().unwrap_or("");
        assert_ne!(
            status, "ok",
            "#107: Ollama status must not be 'ok' when no Ollama is running"
        );
        // The configured flag should be true since the default URL is Ollama
        let configured = oll["configured"].as_bool();
        assert_eq!(
            configured,
            Some(true),
            "#107: Ollama entry should have configured=true when it's the configured provider"
        );
    }

    // Top-level status must be "error" (no models available)
    let top_status = json["status"].as_str().unwrap_or("");
    assert_eq!(
        top_status, "error",
        "#107: top-level status must be 'error' when no provider has models"
    );

    drop(server);
}

/// #107 positive B: configured provider is a stub returning 1 model
/// ⇒ providers contains that entry, top-level status == "ok", count >= 1.
#[test]
fn configured_stub_provider_ok() {
    let stub = ModelsStub::start();
    let port = free_port();
    let server = spawn_test_server(
        port,
        &[
            ("AI_BASE_URL", &format!("http://127.0.0.1:{}/v1", stub.port)),
            ("AI_API_KEY", "test-key-107"),
        ],
    );

    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(format!("http://127.0.0.1:{}/api/ai/models", port))
        .send()
        .expect("models must respond");
    assert!(resp.status().is_success());
    let json: serde_json::Value = resp.json().expect("models json");

    let top_status = json["status"].as_str().unwrap_or("");
    assert_eq!(
        top_status, "ok",
        "#107-B: top-level status must be 'ok' when stub returns a model"
    );

    let total = json["total_count"].as_u64().unwrap_or(0);
    assert!(total >= 1, "#107-B: total_count must be >= 1, got {total}");

    let providers = json["providers"].as_array().expect("providers array");
    assert!(!providers.is_empty(), "#107-B: providers must not be empty");

    // The configured provider (Custom) should have status ok and count >= 1
    let has_ok = providers
        .iter()
        .any(|p| p["status"].as_str() == Some("ok") && p["count"].as_u64().unwrap_or(0) >= 1);
    assert!(
        has_ok,
        "#107-B: at least one provider must have status 'ok' with count >= 1"
    );

    drop(server);
}

/// #107 cross-check: when configured URL is Ollama, both #68's assertion
/// (Ollama present, status != ok) and #98's assertion (top-level not fake green)
/// must hold on the SAME response simultaneously.
#[test]
fn issue_68_and_98_assertions_hold_simultaneously() {
    let port = free_port();
    // Default config → Ollama, no Ollama running
    let server = spawn_test_server(port, &[]);

    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(format!("http://127.0.0.1:{}/api/ai/models", port))
        .send()
        .expect("models must respond");
    let json: serde_json::Value = resp.json().expect("models json");

    // #68 assertion: Ollama must be in providers
    let providers = json["providers"].as_array().expect("providers array");
    let ollama_exists = providers
        .iter()
        .any(|p| p["name"].as_str() == Some("Ollama"));
    assert!(ollama_exists, "#107 cross: Ollama must be present (#68)");

    // #98 assertion: top-level status must not be fake "ok"
    let top_status = json["status"].as_str().unwrap_or("");
    assert_ne!(
        top_status, "ok",
        "#107 cross: top-level must not be fake green (#98)"
    );

    // Both assertions hold on the same response — this is the cross-point
    // that was not covered by existing tests.

    drop(server);
}
