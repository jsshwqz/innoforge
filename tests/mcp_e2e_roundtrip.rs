//! End-to-end roundtrip integration test for `innoforge-mcp` (issue #67 P2.6).
//!
//! Spawns a real `innoforge-server` on an OS-assigned free port and a real
//! `innoforge-mcp` child process connected over stdio JSON-RPC 2.0, then
//! exercises `initialize`, `notifications/initialized`, `tools/list`, and
//! `tools/call` (`patent_search`) against the live server before cleanly
//! closing stdin and waiting for the child to exit.

mod common;

use common::spawn_test_server_on_free_port;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};

struct ChildGuard(Option<Child>);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn send_json_line(stdin: &mut impl Write, payload: &Value) {
    let line = serde_json::to_string(payload).expect("serialize JSON-RPC request");
    writeln!(stdin, "{}", line).expect("write JSON-RPC line to mcp stdin");
    stdin.flush().expect("flush mcp stdin");
}

fn read_json_line(reader: &mut BufReader<impl std::io::Read>) -> Value {
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .expect("read JSON-RPC line from mcp stdout");
    assert!(n > 0, "unexpected EOF reading from innoforge-mcp stdout");
    serde_json::from_str(line.trim()).expect("parse JSON-RPC response from mcp stdout")
}

#[test]
fn mcp_e2e_stdio_roundtrip() {
    let server = spawn_test_server_on_free_port(&[]);
    let base_url = format!("http://127.0.0.1:{}", server.port);

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_innoforge-mcp"));
    cmd.env("INNOFORGE_MCP_BASE_URL", &base_url)
        .env("INNOFORGE_PORT", server.port.to_string())
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .env_remove("NO_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .env_remove("no_proxy")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let mut child = cmd.spawn().expect("spawn innoforge-mcp");
    let mut stdin = child.stdin.take().expect("take mcp stdin");
    let stdout = child.stdout.take().expect("take mcp stdout");
    let mut guard = ChildGuard(Some(child));
    let mut reader = BufReader::new(stdout);

    // 1. initialize
    send_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "mcp-e2e-roundtrip",
                    "version": "0.1.0"
                }
            }
        }),
    );
    let init_resp = read_json_line(&mut reader);
    assert_eq!(init_resp["jsonrpc"], "2.0");
    assert_eq!(init_resp["id"], 1);
    assert!(
        init_resp["result"]["protocolVersion"].as_str().is_some(),
        "initialize must return protocolVersion: {}",
        init_resp
    );
    assert_eq!(init_resp["result"]["serverInfo"]["name"], "innoforge-mcp");
    assert!(
        init_resp["result"]["serverInfo"]["version"]
            .as_str()
            .is_some(),
        "initialize must return serverInfo.version: {}",
        init_resp
    );

    // 2. notifications/initialized (notification: no response)
    send_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    );

    // 3. tools/list
    send_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        }),
    );
    let list_resp = read_json_line(&mut reader);
    assert_eq!(list_resp["jsonrpc"], "2.0");
    assert_eq!(list_resp["id"], 2);
    let tools = list_resp["result"]["tools"]
        .as_array()
        .expect("tools/list must return result.tools array");
    assert!(!tools.is_empty(), "tools/list must not be empty");
    assert!(
        tools
            .iter()
            .any(|t| t["name"].as_str() == Some("patent_search")),
        "tools/list must include patent_search: {:?}",
        tools
    );

    // 4. tools/call (patent_search — local SQLite FTS5 search, no external AI needed)
    send_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "patent_search",
                "arguments": {
                    "query": "solid state battery",
                    "page": 1
                }
            }
        }),
    );
    let call_resp = read_json_line(&mut reader);
    assert_eq!(call_resp["jsonrpc"], "2.0");
    assert_eq!(call_resp["id"], 3);
    assert!(
        call_resp.get("error").is_none(),
        "tools/call must not return JSON-RPC error: {}",
        call_resp
    );
    assert!(
        call_resp["result"].get("isError").is_none(),
        "tools/call must not return tool error: {}",
        call_resp
    );
    let content = call_resp["result"]["content"]
        .as_array()
        .expect("tools/call must return result.content array");
    assert!(!content.is_empty(), "tools/call content must not be empty");
    assert_eq!(content[0]["type"], "text");
    let text = content[0]["text"]
        .as_str()
        .expect("tools/call content[0].text must be string");
    assert!(
        text.contains("Found"),
        "expected patent_search output to contain 'Found', got: {}",
        text
    );

    // 5. Close stdin and wait for clean exit
    drop(stdin);
    drop(reader);
    let mut child = guard.0.take().expect("mcp child still present");
    let status = child.wait().expect("wait for innoforge-mcp exit");
    assert!(
        status.success(),
        "innoforge-mcp should exit cleanly on stdin EOF, got: {}",
        status
    );
}
