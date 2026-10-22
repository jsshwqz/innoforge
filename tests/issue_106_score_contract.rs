//! #106: Quality score contract test (B approach).
//!
//! The template `templates/office_action_response.html` has two `qualityScore()`
//! functions that read the `/api/ai/oa-quality-score` response.  This test:
//!
//! 1. **Statically extracts** the set of `data.<key>` field accesses from the
//!    template source (keys are discovered by parsing, NOT hand-written).
//! 2. Asserts that a representative `parsed=true` backend response contains
//!    every key the template reads on the success path.
//! 3. Asserts that a representative `parsed=false` backend response contains
//!    every key the template reads on the failure path.
//! 4. **Negative control A**: if the backend renames a key (e.g. `dimensions`
//!    → `dims`) while the template still reads `data.dimensions`, the test
//!    must FAIL and **name the missing key**.
//! 5. **Negative control B**: if the template reads a key the backend never
//!    provides (e.g. `data.summary`), the test must FAIL and **name the
//!    extra key**.
//! 6. **E2E (headless)**: calls the real `/api/ai/oa-quality-score` endpoint
//!    with an in-process stub and asserts the response shape for both
//!    success and failure paths.  Only headless; no visible browser test.

mod common;
use common::{free_port, spawn_test_server};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::TcpListener;

/// The template source, embedded at compile time.
const TEMPLATE_SRC: &str = include_str!("../templates/office_action_response.html");

/// Backend response for parsed=true (mirrors `parse_quality_score` in ai.rs).
fn response_parsed_true() -> serde_json::Value {
    serde_json::json!({
        "status": "ok",
        "parsed": true,
        "score": 8.0,
        "max": 10.0,
        "dimensions": [
            {"name": "logic", "score": 8.5, "comment": ""},
            {"name": "evidence", "score": 7.0, "comment": ""},
            {"name": "distinction", "score": 9.0, "comment": ""},
            {"name": "modification", "score": 8.0, "comment": ""},
            {"name": "tone", "score": 7.5, "comment": ""},
            {"name": "completeness", "score": 8.0, "comment": ""}
        ],
        "raw": "{\"total\":8.0,...}",
        "grade": "B",
        "weaknesses": "证据引用可更充分",
        "suggestions": "增加对D1具体段落的引用"
    })
}

/// Backend response for parsed=false (mirrors `parse_quality_score` in ai.rs).
fn response_parsed_false() -> serde_json::Value {
    serde_json::json!({
        "status": "ok",
        "parsed": false,
        "error": "Failed to parse model output as JSON: ...",
        "raw": "This is free text without JSON structure."
    })
}

/// Backend response with out-of-bounds score.
fn response_out_of_bounds() -> serde_json::Value {
    serde_json::json!({
        "status": "ok",
        "parsed": true,
        "score": 87.0,
        "max": 10.0,
        "dimensions": [],
        "raw": "{\"total\":87.0,...}",
        "grade": "A",
        "weaknesses": "",
        "suggestions": "",
        "out_of_bounds": true,
        "warning": "Score 87 is outside expected range [0, 10]"
    })
}

/// Extract all `data.<key>` field accesses from the template source.
///
/// This is the key discovery mechanism: we parse the template JavaScript to
/// find every `data.foo`, `data.bar`, etc.  The returned set is the contract
/// the template expects from the backend.  Keys are discovered by code,
/// NOT hand-written.
fn extract_template_data_keys() -> HashSet<String> {
    let mut keys = HashSet::new();
    let src = TEMPLATE_SRC;

    // Only extract keys from within the quality-score response handling blocks.
    // We find each occurrence of "/api/ai/oa-quality-score" and extract
    // data.<key> accesses from the response handling code that follows
    // (up to the next "catch(e)" or "finally" block).
    let marker = "/api/ai/oa-quality-score";
    let mut search_pos = 0;
    while let Some(marker_start) = src[search_pos..].find(marker) {
        let block_start = search_pos + marker_start;
        // Find the end of this response handling block (next "catch(e)" or "finally")
        let block_end = src[block_start..]
            .find("catch(e)")
            .map(|p| block_start + p)
            .unwrap_or(src.len());

        let block = &src[block_start..block_end];
        let needle = "data.";
        let mut pos = 0;
        while let Some(start) = block[pos..].find(needle) {
            let abs_start = pos + start + needle.len();
            let mut end = abs_start;
            while end < block.len() {
                let c = block.as_bytes()[end];
                if c.is_ascii_alphanumeric() || c == b'_' {
                    end += 1;
                } else {
                    break;
                }
            }
            if end > abs_start {
                let key = &block[abs_start..end];
                keys.insert(key.to_string());
            }
            pos = abs_start;
        }
        search_pos = block_end + 1;
    }
    keys
}

/// Keys the template reads on the success path (parsed === true).
fn template_success_keys() -> HashSet<String> {
    let all = extract_template_data_keys();
    let mut success = all.clone();
    // 'error' is only read in the guard / failure path, not required for success.
    success.remove("error");
    // 'out_of_bounds' and 'warning' are optional — only present when score
    // is outside [0, 10].  The template guards with `if (data.out_of_bounds === true)`.
    success.remove("out_of_bounds");
    success.remove("warning");
    success
}

/// Keys the template reads on the failure path (parsed === false).
fn template_failure_keys() -> HashSet<String> {
    let mut keys = HashSet::new();
    keys.insert("parsed".to_string());
    keys.insert("error".to_string());
    keys.insert("raw".to_string());
    keys
}

/// Check that a response contains all required keys.  Returns missing key names.
fn missing_keys(response: &serde_json::Value, required: &HashSet<String>) -> Vec<String> {
    let resp_keys: HashSet<String> = response
        .as_object()
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();
    required
        .iter()
        .filter(|k| !resp_keys.contains(*k))
        .cloned()
        .collect()
}

/// Check that the template doesn't read keys the backend doesn't provide.
fn extra_keys(template_keys: &HashSet<String>, backend_keys: &HashSet<String>) -> Vec<String> {
    template_keys
        .iter()
        .filter(|k| !backend_keys.contains(*k))
        .cloned()
        .collect()
}

// ── Contract tests (no server needed) ──

/// #106 contract: template keys are discovered from source, not hand-written.
#[test]
fn issue_106_template_keys_discovered_from_source() {
    let keys = extract_template_data_keys();
    assert!(
        keys.contains("parsed"),
        "template must read data.parsed, keys: {:?}",
        keys
    );
    assert!(
        keys.contains("score"),
        "template must read data.score, keys: {:?}",
        keys
    );
    assert!(
        keys.contains("dimensions"),
        "template must read data.dimensions, keys: {:?}",
        keys
    );
    assert!(
        keys.contains("max"),
        "template must read data.max, keys: {:?}",
        keys
    );
    assert!(
        keys.contains("grade"),
        "template must read data.grade, keys: {:?}",
        keys
    );
    assert!(
        keys.contains("weaknesses"),
        "template must read data.weaknesses, keys: {:?}",
        keys
    );
    assert!(
        keys.contains("suggestions"),
        "template must read data.suggestions, keys: {:?}",
        keys
    );
    assert!(
        keys.contains("raw"),
        "template must read data.raw, keys: {:?}",
        keys
    );
    assert!(
        keys.contains("out_of_bounds"),
        "template must read data.out_of_bounds, keys: {:?}",
        keys
    );
    assert!(
        keys.contains("warning"),
        "template must read data.warning, keys: {:?}",
        keys
    );
    // Old pattern must NOT be present
    assert!(
        !TEMPLATE_SRC.contains("var score = data.score || data;"),
        "template must not use old 'var score = data.score || data;' pattern"
    );
}

/// #106 contract: parsed=true response has all keys the template reads.
#[test]
fn issue_106_parsed_true_response_has_all_template_keys() {
    let resp = response_parsed_true();
    let required = template_success_keys();
    let missing = missing_keys(&resp, &required);
    assert!(
        missing.is_empty(),
        "#106: parsed=true response missing keys the template reads: {:?}",
        missing
    );
}

/// #106 contract: parsed=false response has all failure-path keys.
#[test]
fn issue_106_parsed_false_response_has_failure_keys() {
    let resp = response_parsed_false();
    let required = template_failure_keys();
    let missing = missing_keys(&resp, &required);
    assert!(
        missing.is_empty(),
        "#106: parsed=false response missing keys for failure path: {:?}",
        missing
    );
    assert!(
        resp["score"].as_f64().is_none(),
        "#106: parsed=false must not have a numeric score"
    );
    assert_eq!(
        resp["parsed"].as_bool(),
        Some(false),
        "#106: parsed must be false"
    );
}

/// #106 contract: out-of-bounds response has warning and flag.
#[test]
fn issue_106_out_of_bounds_response_has_warning() {
    let resp = response_out_of_bounds();
    assert_eq!(
        resp["out_of_bounds"].as_bool(),
        Some(true),
        "#106: out-of-bounds must be flagged"
    );
    assert!(
        resp["warning"].as_str().is_some(),
        "#106: out-of-bounds must have warning"
    );
    assert_eq!(
        resp["score"].as_f64(),
        Some(87.0),
        "#106: out-of-bounds score preserved as-is"
    );
}

/// #106 negative control A: backend renames `dimensions` to `dims` →
/// template still reads `data.dimensions` → contract broken, must name the key.
#[test]
fn issue_106_neg_a_backend_renames_key_must_be_detected() {
    let mut resp = response_parsed_true();
    if let Some(obj) = resp.as_object_mut() {
        let dims = obj.remove("dimensions");
        obj.insert("dims".to_string(), dims.unwrap_or(serde_json::json!([])));
    }
    let required = template_success_keys();
    let missing = missing_keys(&resp, &required);
    assert!(
        missing.contains(&"dimensions".to_string()),
        "#106 neg-A: backend renamed 'dimensions'→'dims' but template reads 'data.dimensions'; \
         missing must include 'dimensions', got: {:?}",
        missing
    );
}

/// #106 negative control B: template reads a key the backend doesn't provide.
#[test]
fn issue_106_neg_b_template_reads_extra_key_must_be_detected() {
    let backend_keys: HashSet<String> = [
        "status",
        "parsed",
        "score",
        "max",
        "dimensions",
        "raw",
        "grade",
        "weaknesses",
        "suggestions",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let template_keys = template_success_keys();
    let mut backend_with_optional = backend_keys.clone();
    backend_with_optional.insert("out_of_bounds".to_string());
    backend_with_optional.insert("warning".to_string());
    let extra = extra_keys(&template_keys, &backend_with_optional);
    assert!(
        extra.is_empty(),
        "#106 neg-B: template reads keys the backend doesn't provide: {:?}",
        extra
    );
}

// ── In-process AI stub (no python3, no external process) ──

/// In-process AI stub that binds to port 0 (OS-assigned, exclusively owned)
/// and returns a fixed chat completion response.
struct InProcessStub {
    port: u16,
    _thread: std::thread::JoinHandle<()>,
}

impl InProcessStub {
    fn new(content: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
        let port = listener.local_addr().unwrap().port();
        let content = content.to_string();
        let thread = std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let request = String::from_utf8_lossy(&buf);
                let response_body = if request.contains("/chat/completions") {
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
                        response_body.len(),
                        response_body
                    );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        // Ready check: poll until port is bound (stub is listening)
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
        Self {
            port,
            _thread: thread,
        }
    }
}

const VALID_OUTPUT: &str = r#"{
  "scores": {"logic": 8.5, "evidence": 7.0, "distinction": 9.0, "modification": 8.0, "tone": 7.5, "completeness": 8.0},
  "total": 8.0, "grade": "B", "weaknesses": "证据引用可更充分", "suggestions": "增加引用", "detail": "", "not_applicable": []
}"#;

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

/// #106 E2E (headless): success path — stub returns valid JSON,
/// endpoint returns parsed=true with numeric score.
#[test]
fn issue_106_e2e_success_path() {
    let stub = InProcessStub::new(VALID_OUTPUT);
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

    assert_eq!(
        resp["parsed"].as_bool(),
        Some(true),
        "#106 E2E success: parsed must be true, got: {resp}"
    );
    assert!(
        resp["score"].as_f64().is_some(),
        "#106 E2E success: score must be numeric, got: {resp}"
    );
    assert_eq!(
        resp["score"].as_f64(),
        Some(8.0),
        "#106 E2E success: score must be 8.0"
    );
    assert!(
        resp["dimensions"].is_array(),
        "#106 E2E success: dimensions must be an array"
    );
    assert!(
        resp["raw"].as_str().is_some(),
        "#106 E2E success: raw must be a string"
    );

    drop(server);
}

/// #106 E2E (headless): failure path — stub returns unparseable text,
/// endpoint returns parsed=false with no fake score.
#[test]
fn issue_106_e2e_failure_path() {
    let stub = InProcessStub::new("This is free text without any JSON structure.");
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

    assert_eq!(
        resp["parsed"].as_bool(),
        Some(false),
        "#106 E2E failure: parsed must be false, got: {resp}"
    );
    assert!(
        resp["score"].as_f64().is_none(),
        "#106 E2E failure: must not have a numeric score"
    );
    assert!(
        resp["error"].as_str().is_some(),
        "#106 E2E failure: must have an error message"
    );
    assert!(
        resp["raw"].as_str().is_some(),
        "#106 E2E failure: raw must be preserved"
    );

    drop(server);
}
