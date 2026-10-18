//! R23 API 文档-代码守卫
//!
//! 解析 `docs/API.md` 覆盖索引表中声明的端点（Method + Path），
//! 与 `src/common.rs` 路由表（`.route("path", method(handler))`）比对。
//!
//! 规则：文档声明但代码路由表里不存在的端点必须让本测试变红。
//! （代码里有但文档没写的端点不报错——那是覆盖度问题，由计数行追踪。）
//!
//! 最小复现：`cargo test --test api_doc_guard`

use std::collections::HashSet;

use regex::Regex;

/// 从 `src/common.rs` 解析全部 `.route("path", method(...))` 路由。
fn parse_routes_from_src() -> HashSet<(String, String)> {
    let src = include_str!("../src/common.rs");
    let path_re = Regex::new(r#"\.route\(\s*"([^"]+)""#).unwrap();
    let method_re =
        Regex::new(r"\b(get|post|put|delete|patch)\b").unwrap();
    let mut out = HashSet::new();
    for chunk in src.split(".route(").skip(1) {
        let path = match path_re.captures(&format!(".route({chunk}")) {
            Some(c) => c.get(1).unwrap().as_str().to_string(),
            None => continue,
        };
        let method = match method_re.captures(chunk) {
            Some(c) => c.get(1).unwrap().as_str().to_ascii_uppercase(),
            None => continue,
        };
        out.insert((method, path));
    }
    assert!(
        !out.is_empty(),
        "路由表解析结果为空，说明 src/common.rs 结构已变，需更新本测试的解析逻辑"
    );
    out
}

/// 从 `docs/API.md` 覆盖索引表行 `| METHOD | `path` | ... |` 解析声明端点。
fn parse_doc_endpoints() -> Vec<(String, String)> {
    let doc = include_str!("../docs/API.md");
    let row_re =
        Regex::new(r"^\|\s*(GET|POST|PUT|DELETE|PATCH)\s*\|\s*`([^`]+)`").unwrap();
    let mut out = Vec::new();
    for line in doc.lines() {
        if let Some(c) = row_re.captures(line) {
            let method = c.get(1).unwrap().as_str().to_string();
            let path = c.get(2).unwrap().as_str().to_string();
            // 仅纳入 /api/ 开头的端点（页面路由表不含反引号路径，不会误命中）
            if path.starts_with("/api/") || path.starts_with('/') {
                out.push((method, path));
            }
        }
    }
    out
}

#[test]
fn every_documented_endpoint_exists_in_route_table() {
    let routes = parse_routes_from_src();
    let documented = parse_doc_endpoints();

    assert!(
        !documented.is_empty(),
        "未从 docs/API.md 解析到任何端点，说明文档格式已变，需更新本测试"
    );

    let mut ghosts: Vec<(String, String)> = documented
        .into_iter()
        .filter(|ep| !routes.contains(ep))
        .collect();
    ghosts.sort();

    assert!(
        ghosts.is_empty(),
        "docs/API.md 声明了代码路由表中不存在的端点（文档与代码不一致）：\n{:#?}\n\
         请核对 src/common.rs 路由表，要么修正文档，要么补回路由。",
        ghosts
    );
}

#[test]
fn route_table_has_expected_scale() {
    // 防止路由解析逻辑悄悄退化为只解析到几十条。
    let routes = parse_routes_from_src();
    assert!(
        routes.len() >= 120,
        "路由表只解析到 {} 条，低于预期 140 量级，解析逻辑可能失效",
        routes.len()
    );
}
