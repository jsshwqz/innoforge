//! M2-C（基线失败 #6，按独立审计更正后的口径）：答复书/分析流的三态来源标记。
//!
//! 语义（不是"零校验"——≥50 字用分析、OA 兜底、双空报错本就存在；本项补的是
//! **来源可见性**）：
//! - `main_analysis`：主分析 ≥50 字且非冲突报告形态；无横幅；
//! - `conflict_report_only`：主分析为冲突报告形态（M1 实况）→ meta 事件 +
//!   正文首部可见横幅"主分析缺失"；
//! - `oa_text_fallback`：分析 <50 字、OA 兜底 → meta + 横幅；
//! - 双空 → error。
//! meta 事件走 SSE `event:meta`（前端解析器只认 error/done/data，未知事件安全跳过）。
//!
//! 负控（红通道）：stash `src/routes/ai.rs`（回到无来源标记版本）→
//! conflict_report_only / oa_text_fallback 两态的 meta 与横幅断言全部翻红。
//!
//! 桩：答复书生成内部走**非流式** chat（`/chat/completions` 普通 JSON），桩返回
//! 标准 completions JSON（不是 SSE——首版测试在此踩坑，产生"AI 响应解析失败"正文）。

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;

fn start_json_stub() -> u16 {
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
            let resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 152\r\nConnection: close\r\n\r\n{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"M2C 答复书正文内容第一句。M2C 答复书正文内容第二句。\"},\"finish_reason\":\"stop\"}]}";
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

fn meta_source(raw: &str) -> String {
    for line in raw.split('\n') {
        if line.starts_with("event:meta") {
            continue;
        }
        if line.starts_with("data:") && line.contains("\"source\"") {
            // 注意：strip_prefix(' ') 失败时必须用原剩余引用（unwrap_or(d)），
            // unwrap_or("") 会把整个 payload 丢成空串——首版测试在此静默失配
            let d = line
                .strip_prefix("data:")
                .and_then(|r| r.strip_prefix(' ').map_or(Some(r), Some))
                .unwrap_or("");
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(d) {
                return v["source"].as_str().unwrap_or("").to_string();
            }
        }
    }
    String::new()
}

fn spawn_with_stub() -> (u16, common::TestServer) {
    // TestServer 必须由调用方持有——drop 即杀 server（首版测试在此踩坑：连接拒绝）
    let stub_port = start_json_stub();
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
    (app_port, s)
}

#[test]
fn m2c_three_states_and_both_empty() {
    // ── 态 1：main_analysis（≥50 字主分析，无横幅）──
    let (port, _s1) = spawn_with_stub();
    let body = serde_json::json!({
        "analysis": "第一部分 审查意见概述：审查员认为权利要求缺乏创造性，本答复将逐一分析区别特征并论证创造性成立的理由，整体策略围绕区别特征的技术协同展开。",
        "discussion": "[]",
        "office_action": "审查意见通知书正文。",
        "oa_type": "first_exam"
    });
    let resp = post_letter(port, &body);
    assert_eq!(
        meta_source(&resp),
        "main_analysis",
        "态1 meta, resp头300: {}",
        &resp[..resp.len().min(300)]
    );
    let decoded = decode_data(&resp);
    assert!(!decoded.contains("来源提示"), "主分析不得有横幅: {decoded}");
    assert!(
        decoded.contains("M2C 答复书正文内容第一句"),
        "decoded={decoded}"
    );
    // P0-1：meta JSON 不得混进正文（前端按 event 分派后 fullText/导出纯净）
    assert!(
        !decoded.contains("\"source\""),
        "meta JSON 泄漏进正文: {decoded}"
    );

    // ── 态 2a：conflict_report_only 显式声明（P1-8：调用方带回分析流 meta）──
    let (port2, _s2) = spawn_with_stub();
    let conflict_like = "## 转述冲突检测（可主张审查员认定错误）\n\n冲突一：审查意见将对比文件的阀体改写为管道，与原文记载不一致，影响区别特征认定，建议在答复中主张认定有误。";
    let body2 = serde_json::json!({
        "analysis": conflict_like,
        "analysis_source": "conflict_report_only",
        "discussion": "[]",
        "office_action": "审查意见通知书正文。",
        "oa_type": "first_exam"
    });
    let resp2 = post_letter(port2, &body2);
    assert_eq!(
        meta_source(&resp2),
        "conflict_report_only",
        "态2 meta, resp头300: {}",
        &resp2[..resp2.len().min(300)]
    );
    let decoded2 = decode_data(&resp2);
    assert!(
        decoded2.contains("主分析缺失") && decoded2.contains("conflict_report_only"),
        "冲突报告形态正文首部必须有可见横幅: {decoded2}"
    );
    assert!(
        !decoded2.contains("\"source\""),
        "meta JSON 泄漏进正文: {decoded2}"
    );

    // ── 态 2b：仅事实核查段（P1-8 假绿堵口：启发式不得判成 main_analysis）──
    let (port2b, _s2b) = spawn_with_stub();
    let fact_only = "## AI 事实核查（请人工复核）\n可信度评分：100/100。未检测到明显事实性问题。\n\n本次核查覆盖技术领域一致性、段落引用存在性、无来源数据、A33 合规风险与 C 系列检测层。";
    let body2b = serde_json::json!({
        "analysis": fact_only,
        "discussion": "[]",
        "office_action": "审查意见通知书正文。",
        "oa_type": "first_exam"
    });
    let resp2b = post_letter(port2b, &body2b);
    assert_eq!(
        meta_source(&resp2b),
        "conflict_report_only",
        "仅事实核查段必须判非主分析（无横幅=新假绿）, resp头300: {}",
        &resp2b[..resp2b.len().min(300)]
    );
    let decoded2b = decode_data(&resp2b);
    assert!(
        decoded2b.contains("主分析缺失"),
        "仅事实核查段必须有横幅: {decoded2b}"
    );

    // ── 态 3：oa_text_fallback（分析 <50 字，OA 兜底）──
    let (port3, _s3) = spawn_with_stub();
    let body3 = serde_json::json!({
        "analysis": "过短",
        "discussion": "[]",
        "office_action": "审查意见通知书正文，作为兜底上下文使用。",
        "oa_type": "first_exam"
    });
    let resp3 = post_letter(port3, &body3);
    assert_eq!(
        meta_source(&resp3),
        "oa_text_fallback",
        "态3 meta, resp头300: {}",
        &resp3[..resp3.len().min(300)]
    );
    let decoded3 = decode_data(&resp3);
    assert!(
        decoded3.contains("主分析缺失"),
        "OA 兜底必须有横幅: {decoded3}"
    );
    assert!(
        !decoded3.contains("\"source\""),
        "meta JSON 泄漏进正文: {decoded3}"
    );

    // ── 态 4：双空 → error ──
    let (port4, _s4) = spawn_with_stub();
    let body4 = serde_json::json!({
        "analysis": "",
        "discussion": "[]",
        "office_action": "",
        "oa_type": "first_exam"
    });
    let resp4 = post_letter(port4, &body4);
    assert!(
        resp4.contains("event:error") && resp4.contains("缺少分析结果"),
        "双空必须报错，resp头300: {}",
        &resp4[..resp4.len().min(300)]
    );
}
