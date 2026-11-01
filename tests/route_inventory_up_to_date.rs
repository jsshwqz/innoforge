//! 路由清单守卫（#62/#67）：`docs/API-ROUTES.md` 必须与 `src/common.rs` 的 `.route(...)`
//! 声明逐条一致——否则"文档覆盖率"这件事又会悄悄漂回"没人统计"。
//!
//! 解析方式与生成脚本同构：按括号配对扫描（跳过字符串字面量），单行/多行两种写法都能吃到。

use regex::Regex;

fn route_path_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"\.route\(\s*"([^"]+)""#).expect("route path regex"))
}

fn method_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b(get|post|put|delete|patch|head|options)\s*\(").expect("method regex")
    })
}

fn handler_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b(?:get|post|put|delete|patch|head|options)\s*\(\s*([A-Za-z_][A-Za-z0-9_:]*)")
            .expect("handler regex")
    })
}

fn doc_row_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new("^\\| \\d+ \\| ([A-Z /?]+) \\| `([^`]+)` \\| `([^`]*)` \\|")
            .expect("doc row regex")
    })
}

/// 从 common.rs 抽出 `(方法串, 路径, 处理函数)`，保持出现顺序。
fn extract_routes(src: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut search_from = 0usize;
    while let Some(rel) = src[search_from..].find(".route(") {
        let start = search_from + rel;
        search_from = start + 1;

        // 与 `.route(` 的括号配对（跳过字符串字面量）
        let mut depth = 0i32;
        let mut in_str = false;
        let mut esc = false;
        let mut end = start;
        for (off, ch) in src[start + 6..].char_indices() {
            if in_str {
                if esc {
                    esc = false;
                } else if ch == '\\' {
                    esc = true;
                } else if ch == '"' {
                    in_str = false;
                }
                continue;
            }
            match ch {
                '"' => in_str = true,
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = start + 6 + off;
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(seg) = src.get(start..=end) else {
            continue;
        };
        let Some(path_cap) = route_path_re().captures(seg) else {
            continue;
        };
        let path = path_cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let after_path = path_cap.get(0).map(|m| m.end()).unwrap_or(seg.len());
        let handler = handler_re()
            .captures(seg)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let methods: Vec<String> = method_re()
            .captures_iter(seg.get(after_path..).unwrap_or(""))
            .filter_map(|c| c.get(1))
            .map(|m| m.as_str().to_uppercase())
            .collect();
        let methods = if methods.is_empty() {
            "?".to_string()
        } else {
            methods.join(" / ")
        };
        out.push((methods, path, handler));
    }
    out
}

#[test]
fn route_inventory_matches_code() {
    let src = std::fs::read_to_string("src/common.rs").expect("read src/common.rs");
    let doc = std::fs::read_to_string("docs/API-ROUTES.md").expect("read docs/API-ROUTES.md");

    let code = extract_routes(&src);
    assert!(
        !code.is_empty(),
        "从 common.rs 一条路由都没解析出来——解析器坏了，不是'没有路由'"
    );

    let doc_rows: Vec<(String, String, String)> = doc
        .lines()
        .filter_map(|line| doc_row_re().captures(line))
        .map(|c| {
            (
                c.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
                c.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
                c.get(3).map(|m| m.as_str().to_string()).unwrap_or_default(),
            )
        })
        .collect();
    assert!(
        !doc_rows.is_empty(),
        "清单里一条路由都没有——空清单不允许算通过"
    );

    assert_eq!(
        code.len(),
        doc_rows.len(),
        "路由条数不一致：代码 {} 条，清单 {} 条。请重跑 `node scripts/gen_route_inventory.js`",
        code.len(),
        doc_rows.len()
    );

    // 条数相等也可能是"换了别的一批"，逐条比对顺序与内容。
    let mismatches: Vec<String> = code
        .iter()
        .zip(doc_rows.iter())
        .enumerate()
        .filter(|(_, (c, d))| *c != *d)
        .map(|(i, (c, d))| {
            format!(
                "第 {} 条：代码 {:?} vs 清单 {:?}",
                i + 1,
                (c.0.as_str(), c.1.as_str(), c.2.as_str()),
                (d.0.as_str(), d.1.as_str(), d.2.as_str())
            )
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "清单与代码不一致（{} 条），请重跑 `node scripts/gen_route_inventory.js`：\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}
