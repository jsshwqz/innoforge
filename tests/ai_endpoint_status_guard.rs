//! #102 守卫：12 个 AI 端点在**入参校验失败**时必须返回 4xx（400），
//! 禁止「HTTP 200 + body error」的假绿形态——监控/重试/告警只看状态码。
//!
//! 负控（先红后绿）：对未修复的实现（return Json(json!({"error"...}))），
//! 本测试全部 200 → FAILED；修复后（(StatusCode::BAD_REQUEST, Json(...))）全绿。
//! 修复面：`src/routes/ai.rs` 的 12 个 handler（见 issue #102 清单）。

mod common;

use common::{free_port, spawn_test_server};

/// 12 个端点（issue #102 清单），请求体为空对象——必填字段全部缺失。
const ENDPOINTS: [&str; 12] = [
    "/api/ai/claim-chart",
    "/api/ai/compare-matrix",
    "/api/ai/oa-quality-score",
    "/api/ai/oa-panel",
    "/api/ai/oa-strategy-recommend",
    "/api/ai/oa-examiner-preview",
    "/api/ai/oa-defense-analysis",
    "/api/ai/oa-collect-evidence",
    "/api/ai/oa-adversarial",
    "/api/ai/oa-claim-simulate",
    "/api/ai/check-amendments",
    "/api/ai/inventiveness-analysis",
];

#[test]
fn missing_field_requests_return_4xx_not_200() {
    let port = free_port();
    let mut server = spawn_test_server(port, &[]);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();
    let mut failures = Vec::new();
    for ep in ENDPOINTS {
        let url = format!("http://127.0.0.1:{port}{ep}");
        let resp = client
            .post(&url)
            .json(&serde_json::json!({}))
            .send()
            .unwrap_or_else(|e| panic!("{ep} request failed: {e}"));
        let status = resp.status().as_u16();
        if status != 400 && !(500..600).contains(&status) {
            failures.push(format!("{ep}: got HTTP {status}"));
        }
    }
    server.child.kill().unwrap();
    let _ = server.child.wait();
    assert!(
        failures.is_empty(),
        "入参校验失败必须返回 4xx（不允许 200+error 假绿）：\n{}",
        failures.join("\n")
    );
}
