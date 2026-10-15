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

fn has_node() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn check_script_with_node(script: &str) -> Result<(), String> {
    let preprocessed = preprocess_template_vars(script);

    let temp = tempfile::NamedTempFile::new().map_err(|e| format!("tempfile error: {e}"))?;
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
        Err(stderr.to_string())
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
    if !has_node() {
        eprintln!("node not found — skipping inline script parse check");
        return;
    }

    let templates = collect_templates();
    assert!(!templates.is_empty(), "no templates found");

    let mut failures = Vec::new();

    for path in &templates {
        let html = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let scripts = extract_inline_scripts(&html);

        for (script, line_offset) in scripts {
            if let Err(err) = check_script_with_node(&script) {
                failures.push(format!(
                    "{} (inline script at ~line {}): {}",
                    path.display(),
                    line_offset,
                    err.lines().next().unwrap_or("unknown error")
                ));
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
