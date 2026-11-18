//! T-103 C ABI 同进程双周期可重入启停冒烟测试。

use innoforge::{
    innoforge_get_bound_port, innoforge_shutdown_server, innoforge_start_server_on_port,
};
use reqwest::StatusCode;
use serde_json::Value;

#[test]
fn ffi_server_supports_reentrant_start_shutdown_cycles() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db_path = temp.path().join("ffi_smoke.db");
    std::env::set_var("INNOFORGE_DB", db_path.to_string_lossy().as_ref());
    std::env::remove_var("INNOFORGE_ACCESS_TOKEN");

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let client = reqwest::Client::new();

    // 第一轮：启动 -> 请求 /api/healthz -> 拒绝重复启动 -> 关闭
    assert_eq!(innoforge_start_server_on_port(0), 0);
    let port1 = innoforge_get_bound_port();
    assert!(port1 > 0, "expected positive bound port, got {port1}");

    let (status1, body1) = rt.block_on(async {
        let resp = client
            .get(format!("http://127.0.0.1:{port1}/api/healthz"))
            .send()
            .await
            .expect("cycle 1 healthz");
        let status = resp.status();
        let body: Value = resp.json().await.expect("cycle 1 json");
        (status, body)
    });
    assert_eq!(status1, StatusCode::OK);
    assert_eq!(body1["status"], "ok");

    assert_ne!(
        innoforge_start_server_on_port(0),
        0,
        "duplicate start while running must be rejected"
    );

    assert_eq!(innoforge_shutdown_server(), 0);
    assert_eq!(innoforge_get_bound_port(), -1);

    // 第二轮：同进程再次启动 -> 请求 /api/healthz -> 关闭
    assert_eq!(innoforge_start_server_on_port(0), 0);
    let port2 = innoforge_get_bound_port();
    assert!(
        port2 > 0,
        "expected positive bound port in cycle 2, got {port2}"
    );

    let (status2, body2) = rt.block_on(async {
        let resp = client
            .get(format!("http://127.0.0.1:{port2}/api/healthz"))
            .send()
            .await
            .expect("cycle 2 healthz");
        let status = resp.status();
        let body: Value = resp.json().await.expect("cycle 2 json");
        (status, body)
    });
    assert_eq!(status2, StatusCode::OK);
    assert_eq!(body2["status"], "ok");

    assert_eq!(innoforge_shutdown_server(), 0);
    assert_eq!(innoforge_get_bound_port(), -1);
}
