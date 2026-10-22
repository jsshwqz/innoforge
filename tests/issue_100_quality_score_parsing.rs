//! Regression for issue #100: /api/ai/oa-quality-score returned the raw model
//! text wrapped in {"score":{"raw":"..."}} instead of parsing it into a
//! structured result with a numeric score.
//!
//! Fixes:
//! 1. Model output is parsed into {score: number, max: number, dimensions: [...], raw: string}.
//! 2. Parsing failure returns parsed:false with the reason, not a fake success.
//! 3. Out-of-bounds scores are flagged.
//!
//! Three test cases:
//! - Positive: stub returns well-formed JSON → score is numeric, dimensions present.
//! - Neg-A: stub returns free text → parsed:false, no fake score.
//! - Neg-B: stub returns out-of-bounds score (87) → flagged as out_of_bounds.

mod common;
use common::{free_port, spawn_test_server};
use std::net::TcpListener;

/// In-process stub server that responds to POST /v1/chat/completions with a
/// given content string in the OpenAI chat completion format.
/// Also responds to GET /v1/models for provider probe.
///
/// #105: In-process (no python3), port exclusively owned via bind(0).
struct StubAiServer {
    port: u16,
    _thread: std::thread::JoinHandle<()>,
}

impl Drop for StubAiServer {
    fn drop(&mut self) {
        // Thread exits when listener is dropped.
    }
}

fn spawn_ai_stub(content: &str) -> StubAiServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let content = content.to_string();
    let thread = std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let _ = std::io::Read::read(&mut stream, &mut buf);
            let request = String::from_utf8_lossy(&buf);
            let response = if request.contains("/chat/completions") {
                format!(
                    r#"{{"choices":[{{"message":{{"content":{}}},"finish_reason":"stop"}}],"usage":{{"prompt_tokens":10,"completion_tokens":20,"total_tokens":30}}}}"#,
                    serde_json::Value::String(content.clone())
                )
            } else if request.contains("/models") {
                r#"{"data":[{"id":"stub-model"}]}"#.to_string()
            } else {
                r#"{"status":"ok"}"#.to_string()
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                response.len(),
                response
            );
            let _ = std::io::Write::write_all(&mut stream, response.as_bytes());
        }
    });
    // Ready check: poll until stub accepts connections
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < 5 {
        if std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    if start.elapsed().as_secs() >= 5 {
        panic!("stub did not start within 5s");
    }
    StubAiServer {
        port,
        _thread: thread,
    }
}

fn call_quality_score(client: &reqwest::blocking::Client, port: u16) -> serde_json::Value {
    let body = serde_json::json!({
        "response_text": "本申请通过修改权利要求1，增加了特征限定...",
        "oa_text": "审查员指出权利要求1相对于D1不具备创造性...",
        "my_patent": "一种装置，包括特征A和特征B..."
    });
    let resp = client
        .post(format!("http://127.0.0.1:{}/api/ai/oa-quality-score", port))
        .json(&body)
        .send()
        .expect("quality score must respond");
    assert!(resp.status().is_success(), "HTTP status");
    resp.json().expect("json")
}

/// Well-formed model output with valid scores
const VALID_OUTPUT: &str = r#"{
  "scores": {
    "logic": 8.5,
    "evidence": 7.0,
    "distinction": 9.0,
    "modification": 8.0,
    "tone": 7.5,
    "completeness": 8.0
  },
  "total": 8.0,
  "grade": "B",
  "weaknesses": "证据引用可更充分",
  "suggestions": "增加对D1具体段落的引用",
  "detail": "总体质量良好",
  "not_applicable": []
}"#;

/// #100 positive: stub returns well-formed JSON → score is numeric,
/// dimensions have entries, raw preserves original text.
#[test]
fn issue_100_valid_output_parsed_to_numeric_score() {
    let stub = spawn_ai_stub(VALID_OUTPUT);
    let port = free_port();
    let server = spawn_test_server(
        port,
        &[
            ("AI_BASE_URL", &format!("http://127.0.0.1:{}/v1", stub.port)),
            ("AI_API_KEY", "test-key"),
        ],
    );
    let client = reqwest::blocking::Client::new();

    let resp = call_quality_score(&client, port);

    // parsed must be true
    assert_eq!(
        resp["parsed"].as_bool(),
        Some(true),
        "#100: parsed must be true for valid output, got: {resp}"
    );

    // score must be a number (not an object with "raw")
    let score = resp["score"].as_f64();
    assert!(
        score.is_some(),
        "#100: score must be a numeric value, got: {resp}"
    );
    assert_eq!(score.unwrap(), 8.0, "#100: score must be 8.0");

    // max must be 10
    assert_eq!(resp["max"].as_f64(), Some(10.0), "#100: max must be 10.0");

    // dimensions must be a non-empty array
    let dimensions = resp["dimensions"].as_array().expect("dimensions array");
    assert!(
        !dimensions.is_empty(),
        "#100: dimensions must have entries, got empty"
    );
    assert_eq!(dimensions.len(), 6, "#100: must have 6 dimensions");

    // raw must preserve the original text
    let raw = resp["raw"].as_str().expect("raw string");
    assert!(
        raw.contains("total"),
        "#100: raw must preserve original model output"
    );

    drop(server);
    drop(stub);
}

/// #100 negative control A: stub returns free text without scores →
/// parsed must be false, must NOT return {"score":{"raw":...}} as success.
#[test]
fn issue_100_free_text_must_return_parsed_false() {
    let stub = spawn_ai_stub("This is a qualitative assessment without any numeric scores. The response is generally good but could be improved in several areas.");
    let port = free_port();
    let server = spawn_test_server(
        port,
        &[
            ("AI_BASE_URL", &format!("http://127.0.0.1:{}/v1", stub.port)),
            ("AI_API_KEY", "test-key"),
        ],
    );
    let client = reqwest::blocking::Client::new();

    let resp = call_quality_score(&client, port);

    // parsed must be false
    assert_eq!(
        resp["parsed"].as_bool(),
        Some(false),
        "#100 neg-A: parsed must be false for free text, got: {resp}"
    );

    // score must NOT be a number (must not fake a score)
    assert!(
        resp["score"].as_f64().is_none(),
        "#100 neg-A: must not return a numeric score for unparseable text"
    );

    // Must NOT have the old {"score":{"raw":...}} pattern
    let score_obj = &resp["score"];
    assert!(
        score_obj["raw"].is_null(),
        "#100 neg-A: must not wrap raw text in score.raw as fake success"
    );

    // raw must preserve the original text
    assert!(
        resp["raw"].as_str().is_some(),
        "#100 neg-A: raw text must be preserved"
    );

    drop(server);
    drop(stub);
}

/// #100 negative control B: stub returns out-of-bounds score (87) →
/// must be flagged as out_of_bounds, must not silently accept.
#[test]
fn issue_100_out_of_bounds_score_must_be_flagged() {
    let out_of_bounds_output = r#"{
  "scores": {
    "logic": 87.0,
    "evidence": 7.0,
    "distinction": 9.0,
    "modification": 8.0,
    "tone": 7.5,
    "completeness": 8.0
  },
  "total": 87.0,
  "grade": "A",
  "weaknesses": "",
  "suggestions": "",
  "detail": "",
  "not_applicable": []
}"#;

    let stub = spawn_ai_stub(out_of_bounds_output);
    let port = free_port();
    let server = spawn_test_server(
        port,
        &[
            ("AI_BASE_URL", &format!("http://127.0.0.1:{}/v1", stub.port)),
            ("AI_API_KEY", "test-key"),
        ],
    );
    let client = reqwest::blocking::Client::new();

    let resp = call_quality_score(&client, port);

    // parsed should be true (it IS valid JSON), but score must be flagged
    assert_eq!(
        resp["parsed"].as_bool(),
        Some(true),
        "#100 neg-B: parsed should be true (valid JSON), got: {resp}"
    );

    // out_of_bounds must be true
    assert_eq!(
        resp["out_of_bounds"].as_bool(),
        Some(true),
        "#100 neg-B: out-of-bounds score (87) must be flagged, got: {resp}"
    );

    // warning must be present
    assert!(
        resp["warning"].as_str().is_some(),
        "#100 neg-B: must have a warning for out-of-bounds score"
    );

    drop(server);
    drop(stub);
}
