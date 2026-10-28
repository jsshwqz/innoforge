//! UA3 v2 修改稿特征逐字核验门（M2 第 2 步）：答复书网关（SSE）挂接点的负控。
//!
//! 语义：
//! - 整篇答复书产出后跑门（`api_ai_oa_generate_response_letter` 流结束处）；
//! - 零命中特征句**就地**标 `【未证实：该措辞在原始文本零命中】`；
//! - 零命中时正文**首部**加阻断横幅（复用 M2-C 横幅形态），meta 事件带
//!   `amendment_unsupported=N` 与 `submission_ready=false`。
//!
//! 负控（红通道）：stash `src/routes/ai.rs` 回到"边收边发、无门"版本 → 本文件的
//! 就地标注/首部横幅/结构化计数三条断言全部翻红（正文只有模型原文）。
//!
//! 夹具一律通用技术词，无申请号/人名/公开号/案件专有长句。

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;

/// 原始申请文本（说明书 + 具体实施方式）。通用技术词。
const ORIGINAL: &str = "\
说明书
技术领域
本发明涉及一种物料输送装置。
具体实施方式
[0001] 一种物料输送装置，包括机架，机架上设有输送带，机架侧面固定有驱动电机。
[0002] 输送带上方设有导料板，导料板与输送带之间的间距为20～30厘米。
[0003] 驱动电机的输出轴与输送带的主动辊通过链条连接，链条外侧设有张紧轮。
";

/// 桩返回的答复书：修改稿含零记载措辞，并断言"来源于原说明书的明确记载"。
const STUB_LETTER: &str = "\
=== 意见陈述书 ===

一、关于审查意见的答复
申请人认为对比文件未公开区别技术特征。

二、关于权利要求的修改
修改后的权利要求1：
一种物料输送装置，包括机架，其特征在于，还包括设置在机架上的隔离罩，所述隔离罩与所述输送带之间的间隙小于预设差值。

上述修改均来源于原说明书具体实施方式的明确记载，未超出原始申请文件记载的范围。

三、关于创造性的论述
本申请具备创造性。
";

fn start_stub() -> u16 {
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

fn spawn_with_stub() -> (u16, common::TestServer) {
    let stub_port = start_stub();
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
fn letter_gate_blocks_zero_hit_amendment() {
    let (port, _s) = spawn_with_stub();
    let body = serde_json::json!({
        "analysis": "第一部分 审查意见概述：审查员认为权利要求缺乏创造性，本答复将逐一分析区别特征并论证创造性成立的理由，整体策略围绕区别特征的技术协同展开。",
        "discussion": "[]",
        "office_action": "审查意见通知书正文。",
        "oa_type": "first_exam",
        "my_patent": ORIGINAL
    });
    let resp = post_letter(port, &body);
    assert!(
        !resp.contains("event:error"),
        "不得因门禁而报错（阻断形态是降级标注，不是硬阻断）: {}",
        &resp[..resp.len().min(400)]
    );

    let decoded = decode_data(&resp);
    // 负控点 1：零命中特征句必须就地标注（修复前正文是模型原文，无任何标注）
    assert!(
        decoded.contains("【未证实：该措辞在原始文本零命中】还包括设置在机架上的隔离罩"),
        "零命中特征句必须就地降级标注: {decoded}"
    );
    // 负控点 2：阻断横幅必须在正文首部
    let banner_at = decoded
        .find("修改稿核验阻断")
        .unwrap_or_else(|| panic!("正文首部必须有阻断横幅: {decoded}"));
    let letter_at = decoded
        .find("意见陈述书")
        .unwrap_or_else(|| panic!("正文缺失: {decoded}"));
    assert!(banner_at < letter_at, "横幅必须在正文之前: {decoded}");
    // 负控点 3："明确记载"类断言不得原样存活
    assert!(
        !decoded.contains("来源于原说明书具体实施方式的明确记载")
            && decoded.contains("【已降级：本句原为对修改依据的记载断言"),
        "零命中时“明确记载”断言必须被替换为降级标注: {decoded}"
    );

    // 结构化维度（meta 事件，不进正文）
    let meta = meta_of(&resp, "amendment_unsupported").unwrap_or_else(|| {
        panic!(
            "meta 事件必须带 amendment_unsupported: {}",
            &resp[..resp.len().min(600)]
        )
    });
    assert_eq!(meta["amendment_unsupported"], 2, "meta={meta}");
    assert_eq!(
        meta["submission_ready"], false,
        "提交度维度必须压死: {meta}"
    );
    assert_eq!(meta["amendment_gate"], "degraded", "meta={meta}");
    assert!(
        !decoded.contains("\"amendment_unsupported\""),
        "meta JSON 不得泄漏进正文: {decoded}"
    );
}

#[test]
fn letter_without_amendment_is_untouched() {
    // 正控：无修改后权利要求时门不介入（横幅/标注都不出现），meta 计数为 0
    // 桩内容固定含修改段，这里只验证"没有原文可比对时不指控"这一条（my_patent 为空）。
    let (port, _s) = spawn_with_stub();
    let body = serde_json::json!({
        "analysis": "第一部分 审查意见概述：审查员认为权利要求缺乏创造性，本答复将逐一分析区别特征并论证创造性成立的理由，整体策略围绕区别特征的技术协同展开。",
        "discussion": "[]",
        "office_action": "审查意见通知书正文。",
        "oa_type": "first_exam"
    });
    let resp = post_letter(port, &body);
    let decoded = decode_data(&resp);
    assert!(
        !decoded.contains("修改稿核验阻断")
            && !decoded.contains("【未证实：该措辞在原始文本零命中】"),
        "缺原始文本时不得凭空标注: {decoded}"
    );
    let meta = meta_of(&resp, "amendment_unsupported")
        .unwrap_or_else(|| panic!("meta 事件必须带 amendment_unsupported"));
    assert_eq!(meta["amendment_unsupported"], 0, "meta={meta}");
    assert_eq!(meta["amendment_gate"], "skipped_no_original", "meta={meta}");
}
