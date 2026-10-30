//! 信头五项回填的网关级（SSE）验收：`api_ai_oa_generate_response_letter` 流结束处。
//!
//! 语义：
//! - 桩返回的答复书**故意**带着模板泄漏（`【待填写】`、半截日期、`【从 OA 中提取或标注待填写】`）；
//! - 路由层必须在发出前做确定性回填：抽得到的写真实值，抽不到的统一成
//!   `【未提取｜需人工补录】`，产物里**零**个"待填写"；
//! - meta 事件带 `header_known` / `header_filled` / `header_unresolved_marks`，
//!   且 marks 数必须能在正文里逐处数出来（展示数＝产物数）。
//!
//! 负控（红通道）：stash 掉 `src/routes/ai.rs` 的回填调用 → 本文件"零待填写"
//! "信头写真实值""marks 与正文一致"三条断言全部翻红（正文退回桩的原文）。
//!
//! 夹具一律合成：9999 段申请号、"示例申请人"、通用技术词，无真实案件信息。

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;

const UNRESOLVED: &str = "【未提取｜需人工补录】";

/// 原始申请文本（说明书 + 具体实施方式），通用技术词。
const ORIGINAL: &str = "\
说明书
技术领域
本发明涉及一种物料输送装置。
具体实施方式
[0001] 一种物料输送装置，包括机架，机架上设有输送带，机架侧面固定有驱动电机。
[0002] 输送带上方设有导料板，导料板与输送带之间的间距为20～30厘米。
";

/// 桩返回的答复书：信头三处是模板泄漏，签署日是半截日期（v6 实测形态）。
const STUB_LETTER: &str = "\
=== 意见陈述书 ===

**专利号**：【待填写】
**发明名称**：【从 OA 中提取或标注待填写】

尊敬的审查员：

针对贵局发出的审查意见通知书，申请人特此陈述意见如下。

一、关于审查意见的答复
申请人认为对比文件未公开区别技术特征。

二、关于权利要求的修改（如有）
修改后的权利要求1：
一种物料输送装置，包括机架，其特征在于，输送带上方设有导料板。

三、结论
综上所述，申请人认为本申请符合相关规定。

此致

申请人：【待填写】
日期：2026年【待填写】月【待填写】日
";

/// 通知书表头（合成）：五项齐全。
const NOTICE_WITH_HEADER: &str = "\
国家知识产权局
发文日：2026年07月10日
发文序号：9999010100000001
申请号：999999999999.9
发明名称：一种示例物料输送装置
申请人：示例申请人
正文：权利要求 1 不具备专利法第22条第3款规定的创造性。
";

/// 通知书正文（合成）：完全没有表头标签 ⇒ 抽取必须如实报"抽不到"。
const NOTICE_WITHOUT_HEADER: &str = "\
国家知识产权局
正文：权利要求 1 不具备专利法第22条第3款规定的创造性，理由见下。
";

fn start_stub(capture: std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = match stream {
                Ok(s) => s,
                Err(_) => return,
            };
            let mut buf = Vec::new();
            let mut tmp = [0u8; 8192];
            let mut header_end = None;
            loop {
                match stream.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        if header_end.is_none() {
                            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                                header_end = Some(pos);
                            }
                        }
                        if let Some(he) = header_end {
                            let head = String::from_utf8_lossy(&buf[..he]).to_string();
                            let cl = head
                                .lines()
                                .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
                                .and_then(|l| l.split(':').nth(1))
                                .and_then(|v| v.trim().parse::<usize>().ok())
                                .unwrap_or(0);
                            if buf.len() >= he + 4 + cl {
                                break;
                            }
                        }
                    }
                    Err(_) => return,
                }
            }
            // 记下发给模型的完整请求体：用来断言"提示词里到底有没有原始申请文本"
            capture
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(String::from_utf8_lossy(&buf).to_string());
            let payload = serde_json::json!({
                "choices": [{
                    "message": {"role": "assistant", "content": STUB_LETTER},
                    "finish_reason": "stop"
                }]
            })
            .to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    });
    port
}

fn post_letter(port: u16, body: &serde_json::Value) -> String {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .expect("client");
    client
        .post(format!(
            "http://127.0.0.1:{port}/api/ai/oa-generate-response-letter"
        ))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .expect("post")
        .text()
        .expect("body")
}

fn decode_data(raw: &str) -> String {
    let mut out = String::new();
    for line in raw.split('\n') {
        if line.starts_with("data:") && !line.contains("[DONE]") {
            let d = line
                .strip_prefix("data:")
                .and_then(|r| r.strip_prefix(' ').map_or(Some(r), Some))
                .unwrap_or("");
            if d.starts_with('"') {
                if let Ok(s) = serde_json::from_str::<String>(d) {
                    out.push_str(&s);
                }
            }
        }
    }
    out
}

fn meta_of(raw: &str, key: &str) -> Option<serde_json::Value> {
    for line in raw.split('\n') {
        if !line.starts_with("data:") || !line.contains(key) {
            continue;
        }
        let d = line
            .strip_prefix("data:")
            .and_then(|r| r.strip_prefix(' ').map_or(Some(r), Some))
            .unwrap_or("");
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(d) {
            if v.get(key).is_some() {
                return Some(v);
            }
        }
    }
    None
}

fn spawn_with_stub() -> (
    u16,
    common::TestServer,
    std::sync::Arc<std::sync::Mutex<Vec<String>>>,
) {
    let capture = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let stub_port = start_stub(capture.clone());
    let app_port = common::free_port();
    let s = common::spawn_test_server_with_timeout(
        app_port,
        60,
        tempfile::tempdir().expect("dir"),
        &[
            ("AI_BASE_URL", &format!("http://127.0.0.1:{stub_port}/v1")),
            ("AI_API_KEY", "stub-key"),
        ],
    );
    (app_port, s, capture)
}

fn letter_request(notice: &str) -> serde_json::Value {
    serde_json::json!({
        "analysis": "第一部分 审查意见概述：审查员认为权利要求缺乏创造性，本答复将逐一分析区别特征并论证创造性成立的理由，整体策略围绕区别特征的技术协同展开。",
        "discussion": "[]",
        "office_action": notice,
        "oa_type": "first_exam",
        "my_patent": ORIGINAL
    })
}

#[test]
fn header_is_backfilled_and_no_template_placeholder_reaches_the_reader() {
    let (port, _s, capture) = spawn_with_stub();
    let resp = post_letter(port, &letter_request(NOTICE_WITH_HEADER));
    assert!(
        !resp.contains("event:error"),
        "回填不得引入错误: {}",
        &resp[..resp.len().min(400)]
    );
    let decoded = decode_data(&resp);

    // 提示词必须把「原始申请文本」给到模型：v7 的 22 处零命中，根因就是提示词里
    // 根本没有原文——模型被要求写"修改后的权利要求"，却看不到它应当逐字引用的东西。
    let sent = capture
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .first()
        .cloned()
        .unwrap_or_default();
    assert!(
        sent.contains("输送带上方设有导料板，导料板与输送带之间的间距为20～30厘米"),
        "发给模型的提示词里必须含原始申请文本原文（否则模型只能编修改稿）"
    );
    assert!(
        sent.contains("原始申请文本（修改稿的唯一依据"),
        "缺少『原始申请文本』一节的标题与用途说明"
    );
    assert!(
        sent.contains("逐字组成"),
        "缺少『修改稿必须由原文逐字组成』这条硬规则"
    );

    // 负控点 1：产物里零个"待填写"（修复前桩原文会原样穿过）
    assert!(
        !decoded.contains("待填写"),
        "答复书不得把模板空位交给读者: {decoded}"
    );
    // 负控点 2：五项写的是从通知书表头抽到的真实值
    assert!(
        decoded.contains("**专利号**：999999999999.9"),
        "专利号未回填: {decoded}"
    );
    assert!(
        decoded.contains("**发明名称**：一种示例物料输送装置"),
        "发明名称未回填: {decoded}"
    );
    assert!(
        decoded.contains("申请人：示例申请人"),
        "申请人未回填: {decoded}"
    );
    assert!(
        decoded.contains("发文序号：9999010100000001")
            && decoded.contains("发文日：2026年07月10日"),
        "发文序号/发文日该被补插进信头块: {decoded}"
    );
    // 负控点 3：半截日期整体收敛，不许留下会被当成系统确认过的"2026年"
    assert!(!decoded.contains("2026年【"), "半截日期必须收敛: {decoded}");
    assert!(
        decoded.contains(&format!("日期：{UNRESOLVED}")),
        "签署日由申请人决定 ⇒ 必须是可追形态: {decoded}"
    );

    // 展示数＝产物数：meta 报的 marks 能在正文里逐处数出来
    let meta = meta_of(&resp, "header_unresolved_marks")
        .unwrap_or_else(|| panic!("meta 事件必须带 header_unresolved_marks"));
    assert_eq!(meta["header_known"], 5, "meta={meta}");
    assert_eq!(
        decoded.matches(UNRESOLVED).count(),
        meta["header_unresolved_marks"].as_u64().unwrap() as usize,
        "meta 说的未提取处数必须与正文一致: {decoded} / {meta}"
    );
    assert!(
        !decoded.contains("header_unresolved_marks"),
        "meta JSON 不得泄漏进正文: {decoded}"
    );
}

#[test]
fn extraction_failure_is_shown_as_visible_holes_not_a_clean_letter() {
    // 负控另一侧：通知书里没有表头标签 ⇒ 一个真实值都不许编出来，
    // 但每处空位都要变成可追形态并如实报数（不能报成"信头已完成"）。
    let (port, _s, _capture) = spawn_with_stub();
    let resp = post_letter(port, &letter_request(NOTICE_WITHOUT_HEADER));
    let decoded = decode_data(&resp);
    let meta = meta_of(&resp, "header_unresolved_marks")
        .unwrap_or_else(|| panic!("meta 事件必须带 header_unresolved_marks"));
    assert_eq!(meta["header_known"], 0, "五项都抽不到: {meta}");
    assert_eq!(meta["header_filled"], 0, "没有真实值可填: {meta}");
    assert_eq!(meta["header_unresolved_count"], 5, "五项都要报出来: {meta}");
    assert!(
        !decoded.contains("待填写"),
        "抽不到也不能留裸模板空位: {decoded}"
    );
    assert_eq!(
        decoded.matches(UNRESOLVED).count(),
        meta["header_unresolved_marks"].as_u64().unwrap() as usize,
        "正文空位数与 meta 必须一致: {decoded} / {meta}"
    );
    assert!(
        decoded.matches(UNRESOLVED).count() >= 4,
        "三处信头 + 一处签署日至少四处: {decoded}"
    );
}
