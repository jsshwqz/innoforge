//! Issue #33 integration test: verify OAuth callback URL contains the actual
//! listen port by hitting the real `/api/auth/google/url` endpoint.
//!
//! The server is spawned as a subprocess with `INNOFORGE_PORT` set to a
//! non-default port. We poll `/api/version` for readiness (no `sleep`),
//! then GET `/api/auth/google/url` and assert `redirect_uri` contains the
//! expected port.

use std::process::Command;

fn start_server(port: u16, home_dir: &str) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_innoforge-server"))
        .current_dir(home_dir)
        .env("INNOFORGE_PORT", port.to_string())
        .env("INNOFORGE_HOME", home_dir)
        .env("GOOGLE_CLIENT_ID", "test-client-id-for-port-assertion")
        .env("INNOFORGE_NO_OPEN", "1")
        .spawn()
        .expect("failed to spawn server")
}

/// Poll `/api/version` until it responds 200, with a timeout.
/// Returns true if the server became ready, false on timeout.
fn wait_for_server(port: u16, timeout_secs: u64) -> bool {
    let start = std::time::Instant::now();
    let url = format!("http://127.0.0.1:{}/api/version", port);
    while start.elapsed().as_secs() < timeout_secs {
        if let Ok(resp) = reqwest::blocking::get(&url) {
            if resp.status().is_success() {
                return true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    false
}

/// 从 preferred 起向上探测第一个**当前可绑定**的端口。
///
/// 背景（2026-11-08 现场件）：Windows 的 TCP 保留端口段（Hyper-V/WSL 动态排除范围，
/// `netsh interface ipv4 show excludedportrange`）会在重启后漂移——硬编码 3921 曾整段
/// 落进 3911-4010 保留区，服务器 bind 报 os error 10013 直接退出，测试 30s 超时假红。
/// bind 探测与真实监听之间仍有毫秒级 TOCTOU 窗口（#110 的已知形态），但保留段是
/// **结构性不可绑定**，探测跳过它就是本修复的全部价值。
fn find_bindable_port(preferred: u16) -> u16 {
    // 扫描窗口必须足够宽：Windows 保留段是成块的 100 端口（实测 3911-4010 整段保留），
    // 窄窗口会整段落空。bind 尝试本身就是排除段检测——保留段立即 EACCES（10013），
    // 可绑定端口立即成功，全扫毫秒级。任何 bind 错误（占用/保留）都视为不可用。
    for port in preferred..=preferred.saturating_add(2000) {
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    preferred
}

fn kill_server(mut child: std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn redirect_uri_contains_actual_listen_port() {
    // Use a non-default port to prove the callback URL is not hardcoded to 3000.
    // 端口必须现场探测：Windows 保留段会漂移（见 find_bindable_port 文档）。
    let port = find_bindable_port(3921);

    // Create a temp dir for the test database
    let temp_dir = std::env::temp_dir().join(format!(
        "innoforge-oauth-test-{}-{}",
        std::process::id(),
        port
    ));
    std::fs::create_dir_all(&temp_dir).expect("failed to create temp dir");

    let child = start_server(port, temp_dir.to_str().unwrap());

    assert!(
        wait_for_server(port, 30),
        "server did not become ready on port {} within 30s",
        port
    );

    // GET /api/auth/google/url and check redirect_uri
    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/api/auth/google/url", port))
        .expect("failed to GET /api/auth/google/url");

    assert!(
        resp.status().is_success(),
        "expected 200, got {}",
        resp.status()
    );

    let body: serde_json::Value = resp
        .json()
        .expect("failed to parse JSON response from /api/auth/google/url");

    // The endpoint returns {"url": ..., "redirect_uri": ...} when client_id is set
    assert!(
        body.get("error").is_none(),
        "endpoint returned error: {:?} — ensure GOOGLE_CLIENT_ID is set",
        body.get("error")
    );

    let redirect_uri = body["redirect_uri"]
        .as_str()
        .expect("redirect_uri field missing from response");

    assert!(
        redirect_uri.contains(&format!(":{port}")),
        "redirect_uri '{}' does not contain expected port {} — \
         callback URL may be hardcoded to a different port",
        redirect_uri,
        port
    );

    kill_server(child);

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}
