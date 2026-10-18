//! InnoForge MCP Server
//!
//! Exposes InnoForge capabilities as MCP tools via stdio JSON-RPC.
//! Requires InnoForge web server running (default localhost:3000).
//!
//! Base URL resolution order:
//!   1. `INNOFORGE_MCP_BASE_URL` env var (full URL, e.g. `http://192.168.1.10:3921`)
//!   2. `INNOFORGE_PORT` env var → `http://127.0.0.1:$INNOFORGE_PORT`
//!   3. Default → `http://127.0.0.1:3000`
//!
//! Usage in MCP config:
//! ```json
//! {
//!   "mcpServers": {
//!     "innoforge": {
//!       "command": "innoforge-mcp",
//!       "args": []
//!     }
//!   }
//! }
//! ```

use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

const SERVER_NAME: &str = "innoforge-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Resolve the InnoForge base URL from environment, falling back to default.
///
/// Priority: `INNOFORGE_MCP_BASE_URL` → `INNOFORGE_PORT` → default 3000.
fn resolve_base_url() -> String {
    if let Ok(url) = std::env::var("INNOFORGE_MCP_BASE_URL") {
        let trimmed = url.trim_end_matches('/').to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    let port = std::env::var("INNOFORGE_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(3000);
    format!("http://127.0.0.1:{}", port)
}

fn main() {
    let base_url = resolve_base_url();
    eprintln!("[innoforge-mcp] Using base URL: {}", base_url);

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l.trim().to_string(),
            Err(_) => break,
        };
        if line.is_empty() {
            continue;
        }

        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Invalid JSON-RPC request: {}", e);
                continue;
            }
        };

        let id = req.get("id").cloned();
        let method = req["method"].as_str().unwrap_or("");

        let result = match method {
            "initialize" => handle_initialize(),
            "notifications/initialized" => continue,
            "tools/list" => handle_tools_list(),
            "tools/call" => handle_tools_call(&req),
            "ping" => json!({}),
            other => {
                json!({"error": {"code": -32601, "message": format!("Unknown method: {}", other)}})
            }
        };

        // For notifications (id is None), no response is expected
        if id.is_none() {
            continue;
        }

        let id = id.unwrap();

        let response = if let Some(err) = result.get("error") {
            json!({"jsonrpc": "2.0", "id": id, "error": err})
        } else {
            json!({"jsonrpc": "2.0", "id": id, "result": result})
        };

        let out = serde_json::to_string(&response).unwrap_or_default();
        let _ = writeln!(stdout, "{}", out);
        let _ = stdout.flush();
    }
}

fn handle_initialize() -> Value {
    json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {"tools": {}},
        "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION}
    })
}

fn handle_tools_list() -> Value {
    json!({
        "tools": [
            {"name": "patent_search", "description": "Search patents by keyword, applicant, inventor, or patent number.", "inputSchema": {"type": "object", "properties": {"query": {"type": "string"}, "country": {"type": "string"}, "page": {"type": "integer"}, "online": {"type": "boolean"}}, "required": ["query"]}},
            {"name": "patent_detail", "description": "Get full patent details.", "inputSchema": {"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}},
            {"name": "patent_analyze", "description": "AI-powered patent analysis.", "inputSchema": {"type": "object", "properties": {"patent_id": {"type": "string"}}, "required": ["patent_id"]}},
            {"name": "patent_compare", "description": "AI-powered comparison of two patents.", "inputSchema": {"type": "object", "properties": {"patent_id_1": {"type": "string"}, "patent_id_2": {"type": "string"}}, "required": ["patent_id_1", "patent_id_2"]}},
            {"name": "idea_validate", "description": "Validate a creative idea.", "inputSchema": {"type": "object", "properties": {"title": {"type": "string"}, "description": {"type": "string"}}, "required": ["title", "description"]}},
            {"name": "patent_chat", "description": "Ask a question about a patent.", "inputSchema": {"type": "object", "properties": {"patent_id": {"type": "string"}, "message": {"type": "string"}}, "required": ["patent_id", "message"]}},
        ]
    })
}

fn handle_tools_call(req: &Value) -> Value {
    let tool_name = req["params"]["name"].as_str().unwrap_or("");
    let args = match req.get("params").and_then(|p| p.get("arguments")) {
        Some(v) => v.clone(),
        None => json!({}),
    };

    let result = match tool_name {
        "patent_search" => call_patent_search(&args),
        "patent_detail" => call_patent_detail(&args),
        "patent_analyze" => call_patent_analyze(&args),
        "patent_compare" => call_patent_compare(&args),
        "idea_validate" => call_idea_validate(&args),
        "patent_chat" => call_patent_chat(&args),
        _ => Err(format!("Unknown tool: {}", tool_name)),
    };

    match result {
        Ok(text) => json!({
            "content": [{"type": "text", "text": text}]
        }),
        Err(e) => json!({
            "content": [{"type": "text", "text": format!("Error: {}", e)}],
            "isError": true
        }),
    }
}

fn http_get(path: &str) -> Result<Value, String> {
    let base_url = resolve_base_url();
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("{}{}", base_url, path);
    let resp = client
        .get(&url)
        .send()
        .map_err(|e| format!("HTTP error (is innoforge running?): {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let body_text = resp.text().unwrap_or_default();
        let preview = truncate(&body_text, 200);
        return Err(format!(
            "HTTP {} {}: {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or(""),
            preview
        ));
    }
    resp.json::<Value>().map_err(|e| e.to_string())
}

fn http_post(path: &str, body: &Value) -> Result<Value, String> {
    let base_url = resolve_base_url();
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("{}{}", base_url, path);
    let resp = client
        .post(&url)
        .json(body)
        .send()
        .map_err(|e| format!("HTTP error (is innoforge running?): {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let body_text = resp.text().unwrap_or_default();
        let preview = truncate(&body_text, 200);
        return Err(format!(
            "HTTP {} {}: {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or(""),
            preview
        ));
    }
    resp.json::<Value>().map_err(|e| e.to_string())
}

fn call_patent_search(args: &Value) -> Result<String, String> {
    let query = args["query"].as_str().ok_or("Missing 'query'")?;
    let country = args["country"].as_str().map(|s| s.to_string());
    let page = args["page"].as_u64().unwrap_or(1) as usize;

    let body = json!({
        "query": query,
        "page": page,
        "page_size": 10,
        "country": country,
    });

    let data = http_post("/api/search", &body)?;
    let total = data["total"].as_u64().unwrap_or(0);
    let patents = data["patents"].as_array();
    let mut output = format!("Found {} patents (page {})\n\n", total, page);

    if let Some(patents) = patents {
        for (i, p) in patents.iter().enumerate() {
            let score = p["relevance_score"].as_f64().unwrap_or(0.0);
            output.push_str(&format!(
                "{}. [{}] {} (Score: {:.0}%)\n   Applicant: {} | Inventor: {}\n   ID: {}\n\n",
                i + 1,
                p["patent_number"].as_str().unwrap_or("N/A"),
                p["title"].as_str().unwrap_or("Untitled"),
                score,
                p["applicant"].as_str().unwrap_or("N/A"),
                p["inventor"].as_str().unwrap_or("N/A"),
                p["id"].as_str().unwrap_or(""),
            ));
        }
    }

    Ok(output)
}

fn call_patent_detail(args: &Value) -> Result<String, String> {
    let id = args["id"].as_str().ok_or("Missing 'id'")?;
    let _ = http_get(&format!("/api/patent/enrich/{}", id));

    let body = json!({"query": id, "page": 1, "page_size": 1});
    let _search = http_post("/api/search", &body)?;

    let empty = json!({});
    let p = &empty;

    let output = format!(
        "Patent: {} - Not found\n\nTry searching for a different patent number or ID.\n",
        p["patent_number"].as_str().unwrap_or(id)
    );

    Ok(output)
}

fn call_patent_analyze(args: &Value) -> Result<String, String> {
    let patent_id = args["patent_id"].as_str().ok_or("Missing 'patent_id'")?;
    let body = json!({ "patent_number": patent_id });
    let data = http_post("/api/ai/summarize", &body)?;
    Ok(data["content"]
        .as_str()
        .unwrap_or("AI analysis failed")
        .to_string())
}

fn call_patent_compare(args: &Value) -> Result<String, String> {
    let _ = args["patent_id_1"]
        .as_str()
        .ok_or("Missing 'patent_id_1'")?;
    let _ = args["patent_id_2"]
        .as_str()
        .ok_or("Missing 'patent_id_2'")?;
    let body = json!({"patent_ids": [
        args["patent_id_1"].as_str().unwrap_or(""),
        args["patent_id_2"].as_str().unwrap_or(""),
    ]});
    let data = http_post("/api/ai/compare", &body)?;
    Ok(data["content"]
        .as_str()
        .unwrap_or("Comparison failed")
        .to_string())
}

fn call_idea_validate(args: &Value) -> Result<String, String> {
    let title = args["title"].as_str().ok_or("Missing 'title'")?;
    let description = args["description"]
        .as_str()
        .ok_or("Missing 'description'")?;
    let body = json!({"title": title, "description": description});
    let data = http_post("/api/idea/submit", &body)?;

    Ok(format!(
        "Submitted. Status: {}",
        data["status"].as_str().unwrap_or("unknown")
    ))
}

fn call_patent_chat(args: &Value) -> Result<String, String> {
    let patent_id = args["patent_id"].as_str().ok_or("Missing 'patent_id'")?;
    let message = args["message"].as_str().ok_or("Missing 'message'")?;
    let body = json!({"message": message, "patent_id": patent_id});
    let data = http_post("/api/ai/chat", &body)?;
    Ok(data["content"]
        .as_str()
        .unwrap_or("No response")
        .to_string())
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}
