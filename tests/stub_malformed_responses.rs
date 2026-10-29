//! Negative control: three types of malformed AI stub responses.
//!
//! For each type (empty body, HTML error page, truncated/overlong JSON),
//! we provide a tolerates-accepts pair:
//! - tolerates: a stub that returns the malformed response, and a test that
//!   verifies the server does NOT crash (if it crashes, test fails = red).
//! - accepts: a stub that returns a valid response, and the same test
//!   passes = green evidence.
//!
//! All ports use 127.0.0.1:0 (OS-assigned) per test. No std::env::set_var
//! — env vars are passed via Command::env() to the child process only.

mod common;

use common::{free_port, spawn_test_server};
use std::io::{Read, Write};
use std::net::TcpListener;

/// Start a stub that returns a fixed raw response for all requests.
///
/// 先 `bind("127.0.0.1:0")` 由内核分配端口并立即进入 listen，再把 listener 移入子线程，
/// 返回 `(port, handle)`。不要退回"先 free_port() 探端口、再在子线程里 bind"：两步之间
/// 端口会被同二进制并行用例抢走，桩线程 bind 失败后静默 return，客户端直接
/// Connection refused（CI run 40630679117 实测）。
fn start_raw_stub(status: u16, body: &str) -> (u16, std::thread::JoinHandle<()>) {
    let body = body.to_string();
    let listener = TcpListener::bind("127.0.0.1:0").expect("stub must bind an ephemeral port");
    let port = listener.local_addr().expect("stub local_addr").port();
    let handle = std::thread::spawn(move || {
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
    });
    (port, handle)
}

/// Start a stub that returns a fixed raw response with custom content-type.
///
/// 端口分配同上：bind(0) 成功后端口已在监听，`local_addr()` 拿到的是**已经绑定**的端口，
/// 调用方拿到 port 即可连接，无需 sleep 等待。
fn start_raw_stub_ct(content_type: &str, body: &str) -> (u16, std::thread::JoinHandle<()>) {
    let body = body.to_string();
    let ct = content_type.to_string();
    let listener = TcpListener::bind("127.0.0.1:0").expect("stub must bind an ephemeral port");
    let port = listener.local_addr().expect("stub local_addr").port();
    let handle = std::thread::spawn(move || {
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
    });
    (port, handle)
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

// ═══════════════════════════════════════════════════════════════════
// Type 1: Empty body
// ═══════════════════════════════════════════════════════════════════

/// tolerates: stub returns empty body. Server must not crash; AI endpoint
/// should return a controlled error (4xx/5xx), not hang.
#[test]
fn tolerates_empty_body_stub_response() {
    let server_port = free_port();

    let (ai_port, _stub) = start_raw_stub(200, "");

    let server = start_server(server_port, ai_port);

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"my_patent":{"title":"测试专利","content":"这是一段测试专利的详细内容描述，用于测试AI分析功能"},"office_action":"这是一段测试审查意见通知书内容，长度超过十个字符"}"#)
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

    drop(server);
}

/// accepts: stub returns valid JSON. Server processes normally.
#[test]
fn accepts_valid_body_stub_response() {
    let server_port = free_port();

    let valid_json = r#"{"choices":[{"message":{"role":"assistant","content":"OK"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
    let (ai_port, _stub) = start_raw_stub(200, valid_json);

    let server = start_server(server_port, ai_port);

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"my_patent":{"title":"测试专利","content":"这是一段测试专利的详细内容描述，用于测试AI分析功能"},"office_action":"这是一段测试审查意见通知书内容，长度超过十个字符"}"#)
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

    drop(server);
}

// ═══════════════════════════════════════════════════════════════════
// Type 2: HTML error page (wrong content-type)
// ═══════════════════════════════════════════════════════════════════

/// tolerates: stub returns HTML error page instead of JSON. Server must not crash.
#[test]
fn tolerates_html_error_page_stub_response() {
    let server_port = free_port();

    let html = "<html><body><h1>503 Service Unavailable</h1></body></html>";
    let (ai_port, _stub) = start_raw_stub_ct("text/html", html);

    let server = start_server(server_port, ai_port);

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"my_patent":{"title":"测试专利","content":"这是一段测试专利的详细内容描述，用于测试AI分析功能"},"office_action":"这是一段测试审查意见通知书内容，长度超过十个字符"}"#)
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

    drop(server);
}

/// accepts: stub returns valid JSON with correct content-type.
#[test]
fn accepts_json_content_type_stub_response() {
    let server_port = free_port();

    let valid_json =
        r#"{"choices":[{"message":{"role":"assistant","content":"OK"},"finish_reason":"stop"}]}"#;
    let (ai_port, _stub) = start_raw_stub_ct("application/json", valid_json);

    let server = start_server(server_port, ai_port);

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"my_patent":{"title":"测试专利","content":"这是一段测试专利的详细内容描述，用于测试AI分析功能"},"office_action":"这是一段测试审查意见通知书内容，长度超过十个字符"}"#)
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

    drop(server);
}

// ═══════════════════════════════════════════════════════════════════
// Type 3: Truncated / overlong JSON
// ═══════════════════════════════════════════════════════════════════

/// tolerates: stub returns truncated JSON (cut mid-string). Server must not crash.
#[test]
fn tolerates_truncated_json_stub_response() {
    let server_port = free_port();

    // Truncated: missing closing braces and quote
    let truncated = r#"{"choices":[{"message":{"role":"assistant","content":"OK"#;
    let (ai_port, _stub) = start_raw_stub(200, truncated);

    let server = start_server(server_port, ai_port);

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"my_patent":{"title":"测试专利","content":"这是一段测试专利的详细内容描述，用于测试AI分析功能"},"office_action":"这是一段测试审查意见通知书内容，长度超过十个字符"}"#)
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

    drop(server);
}

/// tolerates: stub returns overlong JSON (10KB of padding). Server must not crash.
#[test]
fn tolerates_overlong_json_stub_response() {
    let server_port = free_port();

    // Overlong: valid JSON but with 10KB of padding in content
    let padding = "x".repeat(10240);
    let overlong = format!(
        r#"{{"choices":[{{"message":{{"role":"assistant","content":"{}"}},"finish_reason":"stop"}}]}}"#,
        padding
    );
    let (ai_port, _stub) = start_raw_stub(200, &overlong);

    let server = start_server(server_port, ai_port);

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"my_patent":{"title":"测试专利","content":"这是一段测试专利的详细内容描述，用于测试AI分析功能"},"office_action":"这是一段测试审查意见通知书内容，长度超过十个字符"}"#)
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

    drop(server);
}

/// accepts: stub returns well-formed complete JSON. Server processes normally.
#[test]
fn accepts_complete_json_stub_response() {
    let server_port = free_port();

    let valid = r#"{"choices":[{"message":{"role":"assistant","content":"完整的AI回复"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":10,"total_tokens":15}}"#;
    let (ai_port, _stub) = start_raw_stub(200, valid);

    let server = start_server(server_port, ai_port);

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/ai/office-action-response",
            server_port
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"my_patent":{"title":"测试专利","content":"这是一段测试专利的详细内容描述，用于测试AI分析功能"},"office_action":"这是一段测试审查意见通知书内容，长度超过十个字符"}"#)
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

    drop(server);
}

// ── 端口分配机制的负控：证明旧形态的 TOCTOU 窗口真实存在，新形态没有 ──

/// 负控（先红）：`free_port()` 探到端口后 listener 立即 drop，**这个端口在真正 bind
/// 之前是可被抢占的**——旧写法（探端口 → 传进子线程 → 子线程 bind）的失败窗口就在这里：
/// 子线程 bind 失败后 `Err(_) => return` 静默退出，客户端拿到的是一个没人监听的端口。
#[test]
fn free_port_probe_leaves_a_stealable_window() {
    let probed = free_port();
    // 抢占者：在探针释放后立刻把同一端口 bind 走（模拟并行用例/同机其它进程）
    let thief = TcpListener::bind(("127.0.0.1", probed))
        .expect("抢占者应能拿到 free_port 刚释放的端口（否则本负控失去意义）");
    // 旧写法的桩线程此刻 bind ⇒ 必然失败 ⇒ 静默 return ⇒ 端口无人监听
    let stub_bind = TcpListener::bind(("127.0.0.1", probed));
    assert!(
        stub_bind.is_err(),
        "端口 {probed} 未被抢占，负控失效：free_port → 线程内 bind 的窗口必须可复现"
    );
    if let Ok(l) = stub_bind {
        drop(l);
    }
    drop(thief);
}

/// 正控（后绿）：`bind("127.0.0.1:0")` 在**起线程之前**完成，helper 返回端口时该端口
/// 已进入 listen，客户端立刻可连——不需要 sleep，也不存在被抢占的窗口。
#[test]
fn ephemeral_bind_is_listening_before_helper_returns() {
    let (port, _handle) = start_raw_stub(200, r#"{"ok":true}"#);
    assert_ne!(port, 0, "内核应已分配端口");
    let connected = std::net::TcpStream::connect(("127.0.0.1", port));
    assert!(
        connected.is_ok(),
        "bind(0) 返回后端口必须已在监听: {:?}",
        connected.err()
    );
}
