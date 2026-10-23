//! #83 — serpapi_configured 门极性回归测试
//!
//! `api_search_online`（src/routes/search.rs）用
//! `let serpapi_configured = api_key_opt.is_some()` 区分两条路径：
//! ① 未配 Key → 前置一条 serpapi `Skipped` 记账（error 含「未配置」）
//! ② 已配 Key → 不前置，serpapi 走链上真实 attempt（Success / Failed）
//!
//! 本测试锁定该门的极性（`if !serpapi_configured`）。若被反转为
//! `if serpapi_configured`，两个用例同时翻红：
//! - 用例 A（未配 Key）：修复态应含 Skipped「未配置」行；反转后不前置 → 缺行 → 红。
//! - 用例 B（已配 Key）：修复态不应含 Skipped「未配置」行；反转后前置 → 多行 → 红。

mod common;

use common::spawn_test_server;
use serde_json::Value;

/// 在 attempts 数组中查找 serpapi 源的 Skipped 行且 error 含「未配置」。
///
/// 返回 true 表示「未配置」前置记账存在——用例 A 期望 true，用例 B 期望 false。
fn has_unconfigured_serpapi_skip(attempts: &Value) -> bool {
    let arr = match attempts.as_array() {
        Some(a) => a,
        None => return false,
    };
    arr.iter().any(|entry| {
        entry["source"].as_str() == Some("serpapi")
            && entry["status"].as_str() == Some("Skipped")
            && entry["error"]
                .as_str()
                .map(|e| e.contains("未配置"))
                .unwrap_or(false)
    })
}

/// 发起一次 /api/search/online 请求，返回响应 JSON。
fn search_online(port: u16) -> Value {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .expect("build reqwest client");

    let resp = client
        .post(format!("http://127.0.0.1:{}/api/search/online", port))
        .header("Content-Type", "application/json")
        // 通用关键词，不触发专利号直查（exact_lookup_allowed 要求 PatentNumber 类型）
        .body(r#"{"query":"test query","page":1,"page_size":10}"#)
        .send()
        .expect("search request must respond");

    assert!(
        resp.status().is_success(),
        "search endpoint must return 200, got {}",
        resp.status()
    );

    resp.json().expect("response must be valid JSON")
}

/// 用例 A：未配 SERPAPI_KEY → attempts 必含 serpapi Skipped「未配置」行。
///
/// spawn_test_server 的 env_clear 已移除所有 SERPAPI_* 环境变量，
/// 故 next_serpapi_key() 返回 None → serpapi_configured = false。
/// 修复态（`if !serpapi_configured`）前置 Skipped → 断言通过。
///
/// 反转态（`if serpapi_configured`）：条件为 false → 不前置 →
/// has_unconfigured_serpapi_skip 返回 false → assert 失败 → 红。
#[test]
fn unconfigured_key_prepends_skipped_attempt() {
    let port = common::free_port();
    let server = spawn_test_server(port, &[]);

    let body = search_online(port);
    let attempts = &body["attempts"];
    assert!(
        has_unconfigured_serpapi_skip(attempts),
        "未配 SERPAPI_KEY 时 attempts 必含 serpapi Skipped「未配置」行\n\
         实际 attempts: {attempts}"
    );

    drop(server);
}

/// 用例 B：已配 SERPAPI_KEY → attempts 不应含 serpapi Skipped「未配置」行。
///
/// SERPAPI_BASE_URL 指向本地死端口（port 1），SerpAPI 调用立即失败
/// （ConnectionRefused），记为 Failed 而非 Skipped。
/// 修复态（`if !serpapi_configured`）：条件为 false → 不前置 → 断言通过。
///
/// 反转态（`if serpapi_configured`）：条件为 true → 前置 Skipped「未配置」→
/// has_unconfigured_serpapi_skip 返回 true → assert 失败 → 红。
#[test]
fn configured_key_does_not_prepend_skipped_attempt() {
    let port = common::free_port();
    let server = spawn_test_server(
        port,
        &[
            ("SERPAPI_KEY", "test-key-not-real-0123456789abcdef"),
            // 死端口：连接立即被拒，SerpAPI 记 Failed 而非 Skipped
            ("SERPAPI_BASE_URL", "http://127.0.0.1:1/search.json"),
        ],
    );

    let body = search_online(port);
    let attempts = &body["attempts"];
    assert!(
        !has_unconfigured_serpapi_skip(attempts),
        "已配 SERPAPI_KEY 时 attempts 不应含 serpapi Skipped「未配置」行\n\
         实际 attempts: {attempts}"
    );

    drop(server);
}
