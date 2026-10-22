//! Regression for issue #98: /api/ai/models only probed Ollama (hardcoded),
//! never the configured provider, and top-level `status` was always "ok"
//! even when all providers failed (fake green).
//!
//! Fixes:
//! 1. Multi-provider mode now probes the configured AI_BASE_URL provider.
//! 2. Top-level `status` is "ok" only if at least one provider has models.
//!
//! Negative controls:
//! - A: stop the provider endpoint → top-level status must NOT be "ok".
//! - B: provider returns 401 → must report auth error, not "ok", not
//!   fall back to Ollama.

mod common;
use common::{free_port, spawn_test_server};
use std::io::Write;
use std::net::TcpListener;

/// A minimal OpenAI-compatible stub server that responds to GET /v1/models.
/// `response_type` controls what it returns:
/// - "ok" → 200 with one model
/// - "401" → 401 Unauthorized
/// - "empty" → 200 with empty model list
struct StubServer {
    child: std::process::Child,
    port: u16,
}

impl Drop for StubServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Spawn a stub HTTP server on a free port.
/// Returns the server handle and the port it's listening on.
fn spawn_stub(response_type: &str) -> StubServer {
    let port = free_port();
    let script = format!(
        r#"
import http.server, json, sys

rt = sys.argv[1]

class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if "/models" in self.path:
            if rt == "ok":
                body = json.dumps({{"data": [{{"id": "stub-model"}}]}}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            elif rt == "401":
                self.send_response(401)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(b'{{"error": "invalid api key"}}')
            elif rt == "empty":
                body = json.dumps({{"data": []}}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            else:
                self.send_response(500)
                self.end_headers()
        else:
            self.send_response(404)
            self.end_headers()
    def log_message(self, *a):
        pass

http.server.HTTPServer(("127.0.0.1", {port}), H).serve_forever()
"#,
        port = port
    );

    let mut tmp = tempfile::NamedTempFile::new().expect("create temp script");
    tmp.write_all(script.as_bytes()).expect("write script");
    tmp.flush().expect("flush");
    let script_path = tmp.into_temp_path();
    let script_path_str = script_path.to_str().unwrap().to_string();
    std::mem::forget(script_path); // keep alive for child duration

    let child = std::process::Command::new("python3")
        .arg(&script_path_str)
        .arg(response_type)
        .spawn()
        .expect("spawn stub server");

    // Wait for stub to be ready
    for _ in 0..50 {
        if TcpListener::bind(("127.0.0.1", port)).is_err() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    StubServer { child, port }
}

/// #98 positive: configured provider is reachable → providers list includes
/// it, top-level status is "ok", count >= 1, total_count >= 1.
#[test]
fn issue_98_configured_provider_probed_and_ok() {
    let stub = spawn_stub("ok");
    let port = free_port();
    let server = spawn_test_server(
        port,
        &[
            ("AI_BASE_URL", &format!("http://127.0.0.1:{}/v1", stub.port)),
            ("AI_API_KEY", "test-key"),
        ],
    );
    let client = reqwest::blocking::Client::new();

    let resp = client
        .get(format!("http://127.0.0.1:{}/api/ai/models", port))
        .send()
        .expect("models must respond");
    assert!(resp.status().is_success());
    let json: serde_json::Value = resp.json().expect("models json");

    // Top-level status must be "ok" (at least one provider has models)
    let status = json["status"].as_str().unwrap_or("");
    assert_eq!(
        status, "ok",
        "#98: top-level status must be 'ok' when configured provider is reachable"
    );

    // total_count must be >= 1
    let total = json["total_count"].as_u64().unwrap_or(0);
    assert!(
        total >= 1,
        "#98: total_count must be >= 1 when provider has models, got {total}"
    );

    // The providers array must contain the configured provider
    let providers = json["providers"].as_array().expect("providers array");
    let has_custom = providers.iter().any(|p| {
        let name = p["name"].as_str().unwrap_or("");
        let pstatus = p["status"].as_str().unwrap_or("");
        let count = p["count"].as_u64().unwrap_or(0);
        (name == "Custom" || name == "Ollama") && pstatus == "ok" && count >= 1
    });
    assert!(
        has_custom,
        "#98: providers must include the configured provider with status 'ok' and count >= 1"
    );

    drop(server);
    drop(stub);
}

/// #98 negative control A: provider endpoint is unreachable → top-level
/// status must NOT be "ok".
#[test]
fn issue_98_provider_unreachable_top_status_not_ok() {
    let dead_port = free_port(); // nothing listening here
    let port = free_port();
    let server = spawn_test_server(
        port,
        &[
            ("AI_BASE_URL", &format!("http://127.0.0.1:{}/v1", dead_port)),
            ("AI_API_KEY", "test-key"),
        ],
    );
    let client = reqwest::blocking::Client::new();

    let resp = client
        .get(format!("http://127.0.0.1:{}/api/ai/models", port))
        .send()
        .expect("models must respond");
    assert!(resp.status().is_success());
    let json: serde_json::Value = resp.json().expect("models json");

    let status = json["status"].as_str().unwrap_or("");
    assert_ne!(
        status, "ok",
        "#98 neg-A: top-level status must NOT be 'ok' when provider is unreachable"
    );

    // The error must be visible somewhere in the response
    let providers = json["providers"].as_array().expect("providers array");
    let has_error = providers
        .iter()
        .any(|p| p["status"].as_str() == Some("error") || p["status"].as_str() == Some("timeout"));
    assert!(
        has_error,
        "#98 neg-A: at least one provider must show error/timeout status"
    );

    drop(server);
}

/// #98 negative control B: provider returns 401 → must report auth error,
/// not "ok", not fall back to Ollama.
#[test]
fn issue_98_provider_401_must_report_auth_error() {
    let stub = spawn_stub("401");
    let port = free_port();
    let server = spawn_test_server(
        port,
        &[
            ("AI_BASE_URL", &format!("http://127.0.0.1:{}/v1", stub.port)),
            ("AI_API_KEY", "invalid-key"),
        ],
    );
    let client = reqwest::blocking::Client::new();

    let resp = client
        .get(format!("http://127.0.0.1:{}/api/ai/models", port))
        .send()
        .expect("models must respond");
    assert!(resp.status().is_success());
    let json: serde_json::Value = resp.json().expect("models json");

    let status = json["status"].as_str().unwrap_or("");
    assert_ne!(
        status, "ok",
        "#98 neg-B: top-level status must NOT be 'ok' when provider returns 401"
    );

    // Must report an authentication-type error, not just "Query failed"
    let providers = json["providers"].as_array().expect("providers array");
    let has_auth_error = providers.iter().any(|p| {
        let error = p["error"].as_str().unwrap_or("");
        error.contains("uth") || error.contains("401") || error.contains("key")
    });
    assert!(
        has_auth_error,
        "#98 neg-B: must report authentication-type error, not generic 'Query failed'"
    );

    drop(server);
    drop(stub);
}
