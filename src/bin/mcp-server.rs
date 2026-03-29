//! Patent Hub MCP Server
//!
//! Exposes Patent Hub capabilities as MCP tools via stdio JSON-RPC.
//! Requires patent-hub web server running on localhost:3000.
//!
//! Usage in MCP config:
//! ```json
//! {
//!   "mcpServers": {
//!     "patent-hub": {
//!       "command": "patent-hub-mcp",
//!       "args": []
//!     }
//!   }
//! }
//! ```

use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

const SERVER_NAME: &str = "patent-hub-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
const BASE_URL: &str = "http://127.0.0.1:3000";

fn main() {
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
            Err(_) => continue,
        };

        let id = req.get("id").cloned();
        let method = req["method"].as_str().unwrap_or("");

        let result = match method {
            "initialize" => handle_initialize(),
            "notifications/initialized" => continue, // no response needed
            "tools/list" => handle_tools_list(),
            "tools/call" => handle_tools_call(&req),
            "ping" => json!({}),
            _ => {
                json!({"error": {"code": -32601, "message": format!("Unknown method: {}", method)}})
            }
        };

        let response = if result.get("error").is_some() {
            json!({"jsonrpc": "2.0", "id": id, "error": result["error"]})
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
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": SERVER_NAME,
            "version": SERVER_VERSION
        }
    })
}

fn handle_tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "patent_search",
                "description": "Search patents by keyword, applicant, inventor, or patent number. Returns matching patents with relevance scores.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query (keyword, company name, inventor name, or patent number)" },
                        "country": { "type": "string", "description": "Country filter (CN, US, EP, JP, etc.)" },
                        "page": { "type": "integer", "description": "Page number (default: 1)" },
                        "online": { "type": "boolean", "description": "Search online via SerpAPI/Google Patents (default: false, searches local DB)" }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "patent_detail",
                "description": "Get full patent details including abstract, claims, description, drawings, and metadata.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Patent ID (UUID from search results) or patent number (e.g., CN109028151B)" }
                    },
                    "required": ["id"]
                }
            },
            {
                "name": "patent_analyze",
                "description": "AI-powered patent analysis. Generates a comprehensive summary of the patent including technical field, problem solved, key innovations, and claims analysis.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "patent_id": { "type": "string", "description": "Patent ID to analyze" }
                    },
                    "required": ["patent_id"]
                }
            },
            {
                "name": "patent_compare",
                "description": "AI-powered comparison of two patents. Analyzes similarities, differences, technical approaches, and scope overlap.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "patent_id_1": { "type": "string", "description": "First patent ID" },
                        "patent_id_2": { "type": "string", "description": "Second patent ID" }
                    },
                    "required": ["patent_id_1", "patent_id_2"]
                }
            },
            {
                "name": "idea_validate",
                "description": "Validate a creative idea or invention concept. AI analyzes novelty, feasibility, and searches for similar existing patents.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Idea title" },
                        "description": { "type": "string", "description": "Detailed description of the idea/invention" }
                    },
                    "required": ["title", "description"]
                }
            },
            {
                "name": "patent_chat",
                "description": "Ask a question about a specific patent. AI answers based on the patent content.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "patent_id": { "type": "string", "description": "Patent ID to ask about" },
                        "message": { "type": "string", "description": "Question to ask about the patent" }
                    },
                    "required": ["patent_id", "message"]
                }
            }
        ]
    })
}

fn handle_tools_call(req: &Value) -> Value {
    let tool_name = req["params"]["name"].as_str().unwrap_or("");
    let args = &req["params"]["arguments"];

    let result = match tool_name {
        "patent_search" => call_patent_search(args),
        "patent_detail" => call_patent_detail(args),
        "patent_analyze" => call_patent_analyze(args),
        "patent_compare" => call_patent_compare(args),
        "idea_validate" => call_idea_validate(args),
        "patent_chat" => call_patent_chat(args),
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

fn http_post(path: &str, body: &Value) -> Result<Value, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("{}{}", BASE_URL, path);
    let resp = client
        .post(&url)
        .json(body)
        .send()
        .map_err(|e| format!("HTTP error (is patent-hub running on port 3000?): {}", e))?;
    resp.json::<Value>().map_err(|e| e.to_string())
}

fn http_get(path: &str) -> Result<Value, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("{}{}", BASE_URL, path);
    let resp = client
        .get(&url)
        .send()
        .map_err(|e| format!("HTTP error (is patent-hub running on port 3000?): {}", e))?;
    resp.json::<Value>().map_err(|e| e.to_string())
}

fn call_patent_search(args: &Value) -> Result<String, String> {
    let query = args["query"].as_str().unwrap_or("").to_string();
    let country = args["country"].as_str().map(|s| s.to_string());
    let page = args["page"].as_u64().unwrap_or(1) as usize;
    let online = args["online"].as_bool().unwrap_or(false);

    let body = json!({
        "query": query,
        "page": page,
        "page_size": 10,
        "country": country,
    });

    let path = if online {
        "/api/search/online"
    } else {
        "/api/search"
    };
    let data = http_post(path, &body)?;

    let total = data["total"].as_u64().unwrap_or(0);
    let patents = data["patents"].as_array();

    let mut output = format!("Found {} patents (page {})\n\n", total, page);

    if let Some(patents) = patents {
        for (i, p) in patents.iter().enumerate() {
            let score = p["relevance_score"].as_f64().unwrap_or(0.0);
            output.push_str(&format!(
                "{}. [{}] {} (Score: {:.0}%)\n   Applicant: {} | Inventor: {} | Date: {} | Country: {}\n   Abstract: {}\n   ID: {}\n\n",
                i + 1,
                p["patent_number"].as_str().unwrap_or("N/A"),
                p["title"].as_str().unwrap_or("Untitled"),
                score,
                p["applicant"].as_str().unwrap_or("N/A"),
                p["inventor"].as_str().unwrap_or("N/A"),
                p["filing_date"].as_str().unwrap_or("N/A"),
                p["country"].as_str().unwrap_or("N/A"),
                truncate(p["abstract_text"].as_str().unwrap_or(""), 200),
                p["id"].as_str().unwrap_or(""),
            ));
        }
    }

    if let Some(cats) = data["categories"].as_array() {
        output.push_str("Categories:\n");
        for cat in cats {
            output.push_str(&format!(
                "  - {}: {}\n",
                cat["label"].as_str().unwrap_or(""),
                cat["count"].as_u64().unwrap_or(0)
            ));
        }
    }

    Ok(output)
}

fn call_patent_detail(args: &Value) -> Result<String, String> {
    let id = args["id"].as_str().ok_or("Missing 'id' parameter")?;

    // Try to enrich first (loads full text + images)
    let _ = http_get(&format!("/api/patent/enrich/{}", id));

    // Fetch via internal API - use the search to find by patent number if needed
    let body = json!({"query": id, "page": 1, "page_size": 1});
    let search = http_post("/api/search", &body)?;

    let patent = if let Some(patents) = search["patents"].as_array() {
        if let Some(p) = patents.first() {
            // Get full detail
            let detail_id = p["id"].as_str().unwrap_or(id);
            http_get(&format!("/api/patent/enrich/{}", detail_id)).ok()
        } else {
            None
        }
    } else {
        None
    };

    let empty = json!({});
    let p = patent
        .as_ref()
        .and_then(|d| d.get("patent"))
        .unwrap_or(&empty);

    let mut output = format!(
        "Patent: {} - {}\n\nApplicant: {}\nInventor: {}\nFiling Date: {}\nCountry: {}\nIPC: {}\nLegal Status: {}\n\n",
        p["patent_number"].as_str().unwrap_or(id),
        p["title"].as_str().unwrap_or("N/A"),
        p["applicant"].as_str().unwrap_or("N/A"),
        p["inventor"].as_str().unwrap_or("N/A"),
        p["filing_date"].as_str().unwrap_or("N/A"),
        p["country"].as_str().unwrap_or("N/A"),
        p["ipc_codes"].as_str().unwrap_or("N/A"),
        p["legal_status"].as_str().unwrap_or("N/A"),
    );

    output.push_str("== Abstract ==\n");
    output.push_str(p["abstract_text"].as_str().unwrap_or("Not available"));
    output.push_str("\n\n== Claims ==\n");
    output.push_str(truncate(p["claims"].as_str().unwrap_or("Not loaded"), 2000));
    output.push_str("\n\n== Description ==\n");
    output.push_str(truncate(
        p["description"].as_str().unwrap_or("Not loaded"),
        2000,
    ));

    // Image count
    if let Ok(imgs) = serde_json::from_str::<Vec<String>>(p["images"].as_str().unwrap_or("[]")) {
        if !imgs.is_empty() {
            output.push_str(&format!("\n\n[{} drawings available]", imgs.len()));
        }
    }

    Ok(output)
}

fn call_patent_analyze(args: &Value) -> Result<String, String> {
    let patent_id = args["patent_id"].as_str().ok_or("Missing 'patent_id'")?;
    let body = json!({"patent_number": patent_id});
    let data = http_post("/api/ai/summarize", &body)?;
    Ok(data["content"]
        .as_str()
        .unwrap_or("AI analysis failed")
        .to_string())
}

fn call_patent_compare(args: &Value) -> Result<String, String> {
    let id1 = args["patent_id_1"]
        .as_str()
        .ok_or("Missing 'patent_id_1'")?;
    let id2 = args["patent_id_2"]
        .as_str()
        .ok_or("Missing 'patent_id_2'")?;
    let body = json!({"patent_ids": [id1, id2]});
    let data = http_post("/api/ai/analyze-results", &body)?;
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

    if data["status"].as_str() == Some("ok") {
        let idea = &data["idea"];
        let mut output = format!(
            "Idea Validation: {}\n\nNovelty Score: {}/100\nStatus: {}\n\n",
            idea["title"].as_str().unwrap_or(title),
            idea["novelty_score"].as_f64().unwrap_or(0.0),
            idea["status"].as_str().unwrap_or("pending"),
        );
        output.push_str("== Analysis ==\n");
        output.push_str(idea["analysis"].as_str().unwrap_or("Pending..."));
        Ok(output)
    } else {
        Ok(format!(
            "Submitted. ID: {}. Analysis in progress.",
            data["id"].as_str().unwrap_or("unknown")
        ))
    }
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
        // Find a safe UTF-8 boundary
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}
