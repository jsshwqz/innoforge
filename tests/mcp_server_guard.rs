//! Static guard tests for innoforge-mcp (issue #61).
//!
//! ① Every tool name declared in `tools/list` must have a matching dispatch
//!    arm in `handle_tools_call`.
//! ② Every path used in `http_post` / `http_get` calls must exist in the
//!    route table defined in `src/common.rs`.
//!
//! These tests parse source files at compile time via `include_str!` so they
//! catch drift without running the server.

/// Source of the MCP server binary.
const MCP_SRC: &str = include_str!("../src/bin/mcp-server.rs");
/// Source of the shared route table.
const COMMON_SRC: &str = include_str!("../src/common.rs");

/// Extract tool names declared in `handle_tools_list`.
///
/// Looks for `"name": "tool_name"` patterns inside the `handle_tools_list` fn.
fn extract_declared_tools(src: &str) -> Vec<String> {
    // Find the handle_tools_list function body
    let start = src
        .find("fn handle_tools_list")
        .expect("handle_tools_list not found");
    let end = src[start..]
        .find("\n}\n")
        .map(|e| start + e)
        .unwrap_or(src.len());
    let body = &src[start..end];

    let mut tools = Vec::new();
    for line in body.lines() {
        if let Some(rest) = line.find("\"name\": \"") {
            let after = &line[rest + "\"name\": \"".len()..];
            if let Some(end_quote) = after.find('"') {
                tools.push(after[..end_quote].to_string());
            }
        }
    }
    tools
}

/// Extract tool names that have a dispatch arm in `handle_tools_call`.
///
/// Looks for `"tool_name" => call_` patterns inside the `handle_tools_call` fn.
fn extract_dispatched_tools(src: &str) -> Vec<String> {
    let start = src
        .find("fn handle_tools_call")
        .expect("handle_tools_call not found");
    let end = src[start..]
        .find("\n}\n")
        .map(|e| start + e)
        .unwrap_or(src.len());
    let body = &src[start..end];

    let mut tools = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        // Match lines like `"patent_search" => call_patent_search(&args),`
        if trimmed.starts_with('"') {
            if let Some(end_quote) = trimmed[1..].find('"') {
                let name = &trimmed[1..1 + end_quote];
                if trimmed[1 + end_quote..].contains("=> call_") {
                    tools.push(name.to_string());
                }
            }
        }
    }
    tools
}

/// Extract HTTP paths used in `http_post` and `http_get` calls.
///
/// Handles both direct string literals (`http_post("/api/search", ...)`) and
/// `format!` templates (`http_get(&format!("/api/patent/enrich/{}", id))`).
/// For `format!` paths, `{}` is replaced with `:id` to match axum route syntax.
fn extract_http_paths(src: &str) -> Vec<String> {
    let mut paths = Vec::new();

    for line in src.lines() {
        let trimmed = line.trim();

        // Direct literal: http_post("/api/...", ...) or http_get("/api/...", ...)
        for func in ["http_post", "http_get"] {
            let needle = format!("{}(\"", func);
            if let Some(rest) = trimmed.find(&needle) {
                let after = &trimmed[rest + needle.len()..];
                if let Some(end_quote) = after.find('"') {
                    paths.push(after[..end_quote].to_string());
                }
            }
        }

        // format! template: http_get(&format!("/api/...", ...))
        if trimmed.contains("format!(\"") {
            if let Some(rest) = trimmed.find("format!(\"") {
                let after = &trimmed[rest + "format!(\"".len()..];
                if let Some(end_quote) = after.find('"') {
                    let template = &after[..end_quote];
                    // Replace {} with :id to match axum route param syntax
                    let route_path = template.replace("{}", ":id");
                    // Only include if it looks like an API path
                    if route_path.starts_with("/api/") || route_path.starts_with('/') {
                        paths.push(route_path);
                    }
                }
            }
        }
    }

    paths
}

/// Extract route paths from `.route("...", ...)` calls in common.rs.
fn extract_route_paths(src: &str) -> Vec<String> {
    let mut routes = Vec::new();

    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.find(".route(\"") {
            let after = &trimmed[rest + ".route(\"".len()..];
            if let Some(end_quote) = after.find('"') {
                routes.push(after[..end_quote].to_string());
            }
        }
        // Also handle .route(\n  "path", ...
        if let Some(rest) = trimmed.find("\"/") {
            // Check if this line is inside a .route( call (previous line ends with .route()
            // This is handled by the multi-line case below
            let _ = rest;
        }
    }

    // Handle multi-line .route( calls where the path is on the next line
    let mut lines = src.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.ends_with(".route(") || trimmed.ends_with(".route(") {
            if let Some(next) = lines.peek() {
                let next_trimmed = next.trim();
                if next_trimmed.starts_with('"') {
                    if let Some(end_quote) = next_trimmed[1..].find('"') {
                        let path = &next_trimmed[1..1 + end_quote];
                        if path.starts_with('/') {
                            routes.push(path.to_string());
                        }
                    }
                }
            }
        }
    }

    routes
}

#[test]
fn guard_every_declared_tool_has_dispatch() {
    let declared = extract_declared_tools(MCP_SRC);
    let dispatched = extract_dispatched_tools(MCP_SRC);

    assert!(
        !declared.is_empty(),
        "No tools found in handle_tools_list — parser may be broken"
    );
    assert!(
        !dispatched.is_empty(),
        "No dispatch arms found in handle_tools_call — parser may be broken"
    );

    let undispatched: Vec<_> = declared
        .iter()
        .filter(|t| !dispatched.contains(t))
        .collect();

    assert!(
        undispatched.is_empty(),
        "Tools declared in tools/list but missing from handle_tools_call dispatch:\n{undispatched:?}"
    );
}

#[test]
fn guard_every_http_path_exists_in_route_table() {
    let http_paths = extract_http_paths(MCP_SRC);
    let route_paths = extract_route_paths(COMMON_SRC);

    assert!(
        !http_paths.is_empty(),
        "No http_post/http_get paths found — parser may be broken"
    );
    assert!(
        !route_paths.is_empty(),
        "No route paths found in common.rs — parser may be broken"
    );

    let missing: Vec<_> = http_paths
        .iter()
        .filter(|p| !route_paths.contains(p))
        .collect();

    assert!(
        missing.is_empty(),
        "HTTP paths used in mcp-server.rs but missing from common.rs route table:\n{missing:?}"
    );
}
