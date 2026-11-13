//! Issue #33 integration test: verify OAuth callback URL contains the actual
//! listen port by hitting the real `/api/auth/google/url` endpoint.
//!
//! 2026-11-14（issue #110）：端口分配/预留/失败诊断收编到
//! `common::spawn_test_server_on_free_port`——本文件自带的 find_bindable_port
//! 与手工 spawn/wait/kill 全部删除（3921 落入 Windows 保留段的现场件见 errors.md #75）。

mod common;

#[test]
fn redirect_uri_contains_actual_listen_port() {
    // GOOGLE_CLIENT_ID 走 extra_env；端口由 OS 分配并经预留保护（issue #110）
    let server = common::spawn_test_server_on_free_port(&[(
        "GOOGLE_CLIENT_ID",
        "test-client-id-for-port-assertion",
    )]);
    let port = server.port;

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
        redirect_uri.contains(&format!(":{}", port)),
        "redirect_uri '{}' does not contain expected port {} —          callback URL may be hardcoded to a different port",
        redirect_uri,
        port
    );
    // server drop 自动 kill 子进程
}
