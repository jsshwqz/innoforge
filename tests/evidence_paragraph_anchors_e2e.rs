//! 证据表段落锚点的网关级验收：`POST /api/ai/oa-collect-evidence`。
//!
//! 语义：桩（模型）返回的 `source` 一律不采信，由 Rust 拿 `content` 回材料里定位后回写。
//! 三条桩输出分别覆盖三种结局：
//! - 没报锚点但内容在说明书第 2 段 ⇒ 配上 `[说明书:第0002段]`；
//! - 报了 `[D1:第0002段]` 但内容在该文档的另一块 ⇒ 改写成 `[D1:第0001段]`，`corrected=1`；
//! - 内容在材料里根本没有 ⇒ 前置 `【未定位】`，`unlocated=1`，**绝不**给它一个看着能翻的锚点。
//!
//! 负控（红通道）：去掉 `collect_evidence` 里的 `anchor_items` 调用 ⇒ 三条断言全红
//! （source 退回 "说明书" / "[D1:第0002段]" / "说明书第[0009]段"，stats 里没有 anchor 字段）。
//!
//! 夹具一律通用技术词与合成号，无案件专有信息。

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;

const STUB_EVIDENCE: &str = r#"{
  "evidence": [
    {
      "evidenceType": "specSupport",
      "source": "说明书",
      "content": "输送带上方设有导料板，导料板与输送带之间的间距为20～30厘米",
      "relevance": 0.9
    },
    {
      "evidenceType": "distinction",
      "source": "[D1:第0002段]",
      "content": "对比文件D1公开了一种机架结构与驱动组件的布置方式",
      "relevance": 0.8
    },
    {
      "evidenceType": "techEffect",
      "source": "说明书第[0009]段",
      "content": "采用隔离罩后粉尘浓度下降九成以上，效果十分显著",
      "relevance": 0.7
    }
  ],
  "summary": "共收集到 3 条证据"
}"#;

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
                    "message": {"role": "assistant", "content": STUB_EVIDENCE},
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

fn request_body() -> serde_json::Value {
    // 路由按字符串读 my_patent / refs（前端传的就是序列化后的文本），
    // 所以这里也必须送 JSON **文本**，否则 my_patent 读成空串直接报错。
    let patent = serde_json::json!({
        "claims": "1. 一种物料输送装置，包括机架和输送带。",
        "description": "[0001] 本发明涉及一种物料输送装置。\n[0002] 输送带上方设有导料板，导料板与输送带之间的间距为20～30厘米。"
    })
    .to_string();
    let refs = serde_json::json!([
        {
            "label": "D1",
            "publication_number": "CN990001001A",
            "full_text": "说明书\n\n[0001] 对比文件D1公开了一种机架结构与驱动组件的布置方式。\n\n[0002] 该布置方式未涉及导料板。"
        }
    ])
    .to_string();
    serde_json::json!({
        "my_patent": patent,
        "refs": refs,
        "oa_text": "审查意见：权利要求1不具备专利法第22条第3款规定的创造性。"
    })
}

#[test]
fn collected_evidence_carries_verified_paragraph_anchors() {
    let (port, _s) = spawn_with_stub();
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .expect("client");
    let resp: serde_json::Value = client
        .post(format!(
            "http://127.0.0.1:{port}/api/ai/oa-collect-evidence"
        ))
        .header("Content-Type", "application/json")
        .body(request_body().to_string())
        .send()
        .expect("post")
        .json()
        .expect("json");

    assert_eq!(resp["status"], "ok", "resp={resp}");
    let items = resp["evidence"].as_array().expect("evidence array");
    assert_eq!(items.len(), 3);
    assert_eq!(
        items[0]["source"], "[说明书:第0002段]",
        "内容在说明书第2段 ⇒ 必须给出段号级锚点"
    );
    assert_eq!(
        items[1]["source"], "[D1:第0001段]",
        "模型报第0002段，但这句话在 D1 的第0001段 ⇒ 按定位结果改写，并计入 corrected"
    );
    assert!(
        items[2]["source"]
            .as_str()
            .unwrap()
            .starts_with("【未定位】"),
        "材料里没有这句话 ⇒ 必须可见地标未定位，而不是配个锚点：{}",
        items[2]["source"]
    );

    let stats = &resp["stats"]["anchor"];
    assert_eq!(stats["total"], 3, "stats={stats}");
    assert_eq!(stats["located"], 2, "两条能定位: {stats}");
    assert_eq!(stats["unlocated"], 1, "一条定位不到: {stats}");
    assert_eq!(stats["corrected"], 1, "报了错段号的一条要单独记: {stats}");
    assert_eq!(stats["verified"], 0, "本例没有自报正确的锚点: {stats}");
    // 不变式：total = located + unlocated
    assert_eq!(
        stats["total"].as_u64().unwrap(),
        stats["located"].as_u64().unwrap() + stats["unlocated"].as_u64().unwrap()
    );
    assert!(
        resp["summary"]
            .as_str()
            .unwrap()
            .contains("锚点核对：2/3 条已定位到段落"),
        "summary 要带上核对结论：{}",
        resp["summary"]
    );
}
