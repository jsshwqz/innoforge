//! issue #30 回归测试：AI 后端「能连上但永不回复」时，分析端点必须给出
//! **降级应答**（HTTP 200 + 结构化失败标记），而不是连接级失败或无限挂起。
//!
//! 现场件（issue 描述）：部署实例上 /api/cad/status 与 /api/ai/office-action-response
//! 在可选后端缺失时 ERR_EMPTY_RESPONSE。cad/status 的探测超时已在 v0.9.37 修复
//! （connect_timeout 500ms + 探活 3s）；本测试锁死 AI 路径的同类场景：
//!
//! - 桩：本地 listener，accept 后持有连接**永不写响应**（比 RST 更阴险——
//!   客户端只能靠自己的超时脱身）；
//! - 服务器：AI_BASE_URL 指向死桩，`INNOFORGE_AI_HTTP_TIMEOUT_SECS=5` 把
//!   AI 客户端超时从 300s 压到 5s（环境变量逐请求生效）；
//! - 断言：请求在有限时间内返回 HTTP 2xx + 合法 JSON + 逐角色降级标记
//!   （oa-panel 的四角色立论在 AI 失败时按 `立论失败: {e}` 降级——设计即如此）。

mod common;

#[test]
fn dead_ai_backend_degrades_to_structured_response() {
    // 1) never-responds 桩：accept 后持有连接 20s 再静默关闭
    let stub = std::net::TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let stub_port = stub.local_addr().expect("stub addr").port();
    std::thread::spawn(move || {
        for stream in stub.incoming() {
            match stream {
                Ok(s) => {
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(20));
                        drop(s);
                    });
                }
                Err(_) => break,
            }
        }
    });

    // 2) 被测服务器：AI 指向死桩 + AI 客户端超时压到 5s
    let server = common::spawn_test_server_on_free_port(&[
        ("AI_BASE_URL", &format!("http://127.0.0.1:{}/v1", stub_port)),
        ("AI_API_KEY", "test-key-not-real"),
        ("INNOFORGE_AI_HTTP_TIMEOUT_SECS", "5"),
    ]);
    let port = server.port;

    // 3) 触发会真实发起 AI 调用的端点（oa-panel 四角色立论，最小输入即可）
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .expect("test client");
    let started = std::time::Instant::now();
    let resp = client
        .post(format!("http://127.0.0.1:{}/api/ai/oa-panel", port))
        .json(&serde_json::json!({
            "my_patent": "一种测试装置，包括壳体、加热元件与温控单元。",
            "oa_text": "审查意见：权利要求1不具备创造性，对比文件1公开了壳体。"
        }))
        .send()
        .expect("AI 后端挂死时必须仍返回 HTTP 响应（不得连接级失败/无限挂起）");

    // 4) 降级应答断言：有限时间内 2xx + 合法 JSON + 逐角色降级标记
    assert!(
        resp.status().is_success(),
        "降级应答应为 2xx，实际 {}（issue #30：可选后端缺失必须优雅降级）",
        resp.status()
    );
    assert!(
        started.elapsed().as_secs() < 60,
        "降级应在 60s 内完成（AI 超时 5s × 3 轮 + 余量），实际 {}s",
        started.elapsed().as_secs()
    );
    let body: serde_json::Value = resp.json().expect("响应体必须为合法 JSON");
    let dumped = body.to_string();
    assert!(
        dumped.contains("立论失败"),
        "四角色立论应逐个降级为「立论失败」标记（issue #30 要求的结构化降级）：{}",
        &dumped[..dumped.len().min(400)]
    );
}
