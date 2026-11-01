//! Static guard: every inline <script> in templates/*.html must parse as valid JS.
//!
//! Uses `node --check` as the JS engine. If node is not installed, the test
//! is skipped (marked ok with a note), so CI environments without node don't
//! fail — but in dev/CI where node is present, a syntax error in any inline
//! script will cause this test to fail.
//!
//! Red-then-green: if a non-async function uses `await`, the classic script
//! parser treats `await` as an identifier and throws "Unexpected identifier".
//! This test catches that before it reaches a browser.
//!
//! Template variables `{{...}}` and `{%...%}` are replaced with placeholders
//! before parsing, since they are server-side substitutions.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Extract inline (non-src) <script> blocks from an HTML file.
/// Returns (script_content, approx_line_offset) pairs.
fn extract_inline_scripts(html: &str) -> Vec<(String, usize)> {
    let mut scripts = Vec::new();
    let mut i = 0;

    while i < html.len() {
        if let Some(pos) = html[i..].find("<script") {
            let start = i + pos;
            if let Some(tag_end) = html[start..].find('>') {
                let tag_end = start + tag_end;
                let tag_content = &html[start..tag_end];

                if tag_content.contains("src=") {
                    i = tag_end + 1;
                    continue;
                }

                let body_start = tag_end + 1;
                if let Some(end) = html[body_start..].find("</script>") {
                    let body_end = body_start + end;
                    let script_body = &html[body_start..body_end];

                    if !script_body.trim().is_empty() {
                        let line_offset = html[..body_start].lines().count();
                        scripts.push((script_body.to_string(), line_offset));
                    }
                    i = body_end + 9;
                    continue;
                }
            }
        }
        break;
    }
    scripts
}

/// Replace template variables with JS-safe placeholders so node can parse.
/// `{{ var }}` → `null` (or `[]` for known array vars)
/// `{% if x %}` / `{% endif %}` etc → removed (commented out)
fn preprocess_template_vars(script: &str) -> String {
    let mut result = script.to_string();

    // Replace {{ ... }} with null (safe JS value)
    // Handle common array cases
    while let Some(start) = result.find("{{") {
        if let Some(end) = result[start..].find("}}") {
            let end = start + end + 2;
            let var_content = &result[start..end];
            // Arrays get [], objects get {}, else null
            let replacement = if var_content.contains("json")
                || var_content.contains("images")
                || var_content.contains("citations")
            {
                "[]"
            } else if var_content.contains("count") || var_content.contains("num") {
                "0"
            } else {
                "null"
            };
            result.replace_range(start..end, replacement);
        } else {
            break;
        }
    }

    // Replace {% ... %} with empty string (control flow, not valid JS)
    while let Some(start) = result.find("{%") {
        if let Some(end) = result[start..].find("%}") {
            let end = start + end + 2;
            result.replace_range(start..end, "");
        } else {
            break;
        }
    }

    result
}

/// 拿 node 的版本；拿不到就 **fail closed**（#59：依赖缺失不许报 ok）。
fn require_node() -> String {
    match Command::new("node").arg("--version").output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            panic!(
                "node 存在但 `node --version` 失败（stderr 首行：{}）。                 这条守卫无法工作，请安装可用的 node，或在跑它的 CI job 里 setup-node。",
                err.lines().next().unwrap_or("<empty>")
            );
        }
        Err(e) => panic!(
            "找不到 node（{e}）。模板内联 JS 语法守卫 **无法工作**，不能算通过。             请安装 node，或在跑这条测试的 CI job 里加 actions/setup-node@v4。"
        ),
    }
}

/// 把 node 的 stderr 变成可定位的消息：**模板内行号 + SyntaxError 原文**，
/// 绝不把临时文件路径/临时行号当诊断（临时文件测完就删，那串信息无法复现，见 #59）。
fn describe_node_error(stderr: &str, line_offset: usize) -> String {
    let mut temp_line: Option<usize> = None;
    let mut syntax: Option<String> = None;
    for line in stderr.lines() {
        let t = line.trim();
        if syntax.is_none() && (t.starts_with("SyntaxError") || t.starts_with("Error")) {
            syntax = Some(t.to_string());
            continue;
        }
        // node 的定位行形如 "/tmp/xxx.js:6" 或 "D:\Temp\abc.js:6"
        if temp_line.is_none() {
            if let Some((_, n)) = t.rsplit_once(':') {
                if let Ok(v) = n.trim().parse::<usize>() {
                    if t.contains(".js:") || t.contains(".mjs:") {
                        temp_line = Some(v);
                    }
                }
            }
        }
    }
    let at = temp_line
        .map(|n| format!("templates 内第 {} 行附近", line_offset + n))
        .unwrap_or_else(|| format!("inline script（起始于第 {} 行）", line_offset));
    format!(
        "{at}：{}",
        syntax.unwrap_or_else(|| "<node 未给出错误行>".to_string())
    )
}

fn check_script_with_node(script: &str, line_offset: usize) -> Result<(), String> {
    let preprocessed = preprocess_template_vars(script);

    let temp = tempfile::Builder::new()
        .suffix(".js")
        .tempfile()
        .map_err(|e| format!("tempfile error: {e}"))?;
    fs::write(temp.path(), &preprocessed).map_err(|e| format!("write error: {e}"))?;

    let output = Command::new("node")
        .arg("--check")
        .arg(temp.path())
        .output()
        .map_err(|e| format!("node spawn error: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(describe_node_error(&stderr, line_offset))
    }
}

fn collect_templates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(entries) = fs::read_dir("templates") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "html") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths
}

#[test]
fn all_inline_scripts_parse_as_valid_js() {
    // 依赖缺失 = 守卫无法工作 = 失败（#59）。这里打印版本，便于在 CI 日志里确认真的跑了。
    let node_version = require_node();
    eprintln!("inline_scripts_parse: node {node_version}");

    let templates = collect_templates();
    assert!(!templates.is_empty(), "no templates found");

    let mut failures = Vec::new();

    for path in &templates {
        let html = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let scripts = extract_inline_scripts(&html);

        for (script, line_offset) in scripts {
            if let Err(err) = check_script_with_node(&script, line_offset) {
                failures.push(format!("{}: {}", path.display(), err));
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "Inline script parse failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

#[test]
fn no_non_async_function_uses_await() {
    // Text-level guard: scan for `function name(` (not `async function`)
    // that contains `await ` in its body. This is a fast pre-check that
    // doesn't need node, and gives a clearer error message than node's
    // "Unexpected identifier".
    let templates = collect_templates();
    let mut violations = Vec::new();

    for path in &templates {
        let html = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let lines: Vec<&str> = html.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim_start();

            if trimmed.starts_with("function ") && trimmed.contains('(') {
                let rest = &trimmed[9..];
                let name = rest.split('(').next().unwrap_or("?").trim();

                let mut depth = 0i32;
                let mut found_open = false;
                let start = i;
                let mut matched = false;
                for j in start..lines.len() {
                    for ch in lines[j].chars() {
                        if ch == '{' {
                            depth += 1;
                            found_open = true;
                        } else if ch == '}' {
                            depth -= 1;
                        }
                    }
                    if found_open && depth == 0 {
                        let body: String = lines[start..=j].join("\n");
                        if body.contains("await ") {
                            violations.push(format!(
                                "{}:{}: function `{}` is not async but uses await",
                                path.display(),
                                start + 1,
                                name
                            ));
                        }
                        i = j + 1;
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    i += 1;
                }
                continue;
            }
            i += 1;
        }
    }

    if !violations.is_empty() {
        panic!(
            "Non-async functions using await ({}):\n{}",
            violations.len(),
            violations.join("\n")
        );
    }
}
