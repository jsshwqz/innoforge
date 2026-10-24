//! M2-A（基线失败 #1）回归：流式主分析的终止必须可见。
//!
//! 三态：
//! 1. 慢首 token（thinking 形态）→ 首 token 超时必须以 [ERROR] 事件可见，不许静默；
//! 2. 空流（[DONE] 前零内容）→ 必须以 [ERROR] 事件可见；
//! 3. 正常流 → 内容 chunk 正常到达，无 error 事件。
//!
//! 负控（红通道，PR 记录）：把 `routes/ai.rs` 流循环 `None` 分支的空文兜底
//! 与 `patent.rs` spawn_oa_stream_worker 的空文守卫临时移除 → 本文件
//! `empty_stream_must_emit_error` / `slow_first_token_must_timeout_visibly` 必须翻红。
//! （首 token 超时红通道：把 `chat.rs` 首块 `tokio::time::timeout` 临时改为直接
//! `resp.chunk().await` → slow 用例翻红。）
//!
//! 桩：进程内 std TcpListener 手写 SSE（可控时延），不走外网。

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;

/// 可控 SSE 桩模式
enum StubMode {
    /// 先发响应头，然后静默 seconds 秒再关闭（thinking 卡首 token 形态）
    SlowFirstToken(u64),
    /// 立即 [DONE]，零内容
    EmptyDone,
    /// 两个内容 chunk 后 [DONE]
    Normal,
}

fn start_stub(mode: StubMode) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = match stream {
                Ok(s) => s,
                Err(_) => return,
            };
            // 读完请求头+体（Content-Length 内）
            let mut buf = Vec::new();
            let mut tmp = [0u8; 8192];
            let mut header_end = None;
            loop {
                match stream.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        if header_end.is_none() {
                            if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
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
            let sse_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(sse_headers.as_bytes());
            let _ = stream.flush();
            match mode {
                StubMode::SlowFirstToken(secs) => {
                    std::thread::sleep(std::time::Duration::from_secs(secs));
                    // 静默关闭（不发任何 data）
                }
                StubMode::EmptyDone => {
                    let _ = stream.write_all(b"data: [DONE]\n\n");
                    let _ = stream.flush();
                }
                StubMode::Normal => {
                    let body = concat!(
                        "data: {\"choices\":[{\"delta\":{\"content\":\"M2A 正常内容第一句。\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"M2A 正常内容第二句。\"}}]}\n\n",
                        "data: [DONE]\n\n",
                    );
                    let _ = stream.write_all(body.as_bytes());
                    let _ = stream.flush();
                }
            }
            let _ = stream.shutdown(std::net::Shutdown::Both);
            // 单请求即止：listener 循环继续服务后续连接（spawn_test_server 可能发探测）
        }
    });
    port
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn post_stream(port: u16, body: &str) -> String {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .expect("client");
    client
        .post(format!(
            "http://127.0.0.1:{port}/api/ai/office-action-response/stream"
        ))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .expect("post")
        .text()
        .expect("body")
}

fn minimal_body() -> String {
    serde_json::json!({
        "my_patent": {"title": "M2A 测试", "content": "权利要求1：一种测试装置。"},
        "office_action": {"content": "审查意见通知书：权利要求1相对于对比文件1不具备创造性。"},
        "references": [],
        "oa_type": "first_exam",
        "depth": "standard"
    })
    .to_string()
}

#[test]
fn m2a_slow_first_token_and_empty_stream_and_normal() {
    // ── 桩 1：慢首 token（>服务端 1s 首 token 预算）──
    let slow_port = start_stub(StubMode::SlowFirstToken(30));
    let app_port_slow = common::free_port();
    let _s1 = common::spawn_test_server_with_timeout(
        app_port_slow,
        60,
        tempfile::tempdir().expect("dir"),
        &[
            ("AI_BASE_URL", &format!("http://127.0.0.1:{slow_port}/v1")),
            ("AI_API_KEY", "stub-key"),
            ("INNOFORGE_AI_FIRST_TOKEN_TIMEOUT_SECS", "1"),
        ],
    );
    let resp = post_stream(app_port_slow, &minimal_body());
    assert!(
        resp.contains("event:error") && resp.contains("首个响应片段超时"),
        "慢首 token 必须以 error 事件可见，实际响应片段：{}",
        &resp[..resp.len().min(300)]
    );

    // ── 桩 2：空流（[DONE] 前零内容）──
    let empty_port = start_stub(StubMode::EmptyDone);
    let app_port_empty = common::free_port();
    let _s2 = common::spawn_test_server_with_timeout(
        app_port_empty,
        60,
        tempfile::tempdir().expect("dir"),
        &[
            ("AI_BASE_URL", &format!("http://127.0.0.1:{empty_port}/v1")),
            ("AI_API_KEY", "stub-key"),
            ("INNOFORGE_AI_FIRST_TOKEN_TIMEOUT_SECS", "10"),
        ],
    );
    let resp = post_stream(app_port_empty, &minimal_body());
    assert!(
        resp.contains("event:error") && resp.contains("未输出任何内容"),
        "空流必须以 error 事件可见，实际响应片段：{}",
        &resp[..resp.len().min(300)]
    );

    // ── 桩 3：正常流（内容照发、无 error）──
    let normal_port = start_stub(StubMode::Normal);
    let app_port_normal = common::free_port();
    let _s3 = common::spawn_test_server_with_timeout(
        app_port_normal,
        60,
        tempfile::tempdir().expect("dir"),
        &[
            ("AI_BASE_URL", &format!("http://127.0.0.1:{normal_port}/v1")),
            ("AI_API_KEY", "stub-key"),
            ("INNOFORGE_AI_FIRST_TOKEN_TIMEOUT_SECS", "10"),
        ],
    );
    let resp = post_stream(app_port_normal, &minimal_body());
    assert!(
        !resp.contains("event:error"),
        "正常流不得出现 error 事件，响应片段：{}",
        &resp[..resp.len().min(300)]
    );
    // 内容经 JSON 编码传输，解码后应含桩文案
    let decoded = decode_sse(&resp);
    assert!(decoded.contains("M2A 正常内容第一句"), "decoded={decoded}");
}

fn decode_sse(raw: &str) -> String {
    let mut out = String::new();
    for line in raw.split('\n') {
        if let Some(d) = line
            .strip_prefix("data:")
            .map(|d| d.strip_prefix(' ').unwrap_or(d))
        {
            if d.starts_with('"') {
                if let Ok(s) = serde_json::from_str::<String>(d) {
                    out.push_str(&s);
                }
            } else {
                out.push_str(d);
            }
        }
    }
    out
}

// 默认值与 env 覆盖的断言在 `src/ai/client.rs` 单测（m2a_timeout_defaults_and_env_override）——
// 与实现同文件，负控 stash 时随实现一起回退，保证本文件对旧代码可独立编译。
