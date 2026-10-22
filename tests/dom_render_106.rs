//! #106 DOM-level assertions (Rust wrapper).
//!
//! Runs `tests/dom_render_106.mjs` which extracts the actual rendering code
//! from `templates/office_action_response.html`, executes it with a mock DOM,
//! and asserts on the rendered HTML output (not API JSON).
//!
//! This complements `tests/issue_106_score_contract.rs` (which tests the
//! API response shape) by testing what the user actually sees in the DOM.
//!
//! Covers:
//! - **Failure path** (`parsed=false` + non-empty `error`): rendered HTML
//!   must contain `oa.quality.parseFailed` bilingual text, must NOT trigger
//!   `alert`, must NOT contain `undefined`/`NaN`/`0/10`.
//! - **Success path** (`parsed=true`): rendered HTML must contain score
//!   value, `/10` (block 1), dimension names, grade; no `undefined`/`NaN`.
//! - **Regression guard**: `parsed=false` + non-empty `error` must not
//!   trigger `alert` (proves the failure path is not swallowed by the
//!   old `data.error && data.parsed !== true` guard).

use std::process::Command;

#[test]
fn dom_render_106_quality_score() {
    // Skip gracefully if node is not installed.
    let node_check = Command::new("node").arg("--version").output();
    if node_check
        .as_ref()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        eprintln!("node not available, skipping DOM render test for #106");
        return;
    }

    let output = Command::new("node")
        .arg("tests/dom_render_106.mjs")
        .output()
        .expect("failed to spawn node tests/dom_render_106.mjs");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse JSON-lines and tally pass/fail.
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut failures: Vec<String> = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match v.get("kind").and_then(|k| k.as_str()) {
            Some("pass") => passed += 1,
            Some("fail") => {
                failed += 1;
                let name = v.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                let reason = v.get("reason").and_then(|r| r.as_str()).unwrap_or("");
                failures.push(format!("  FAIL {name}: {reason}"));
            }
            Some("summary") => {
                let s_pass = v.get("passed").and_then(|p| p.as_u64()).unwrap_or(0);
                let s_fail = v.get("failed").and_then(|p| p.as_u64()).unwrap_or(0);
                if s_pass != passed as u64 || s_fail != failed as u64 {
                    panic!(
                        "summary mismatch: script says {s_pass} passed / {s_fail} failed, \
                         but we counted {passed} passed / {failed} failed"
                    );
                }
            }
            _ => {}
        }
    }

    if failed > 0 {
        panic!(
            "DOM render test for #106 had {failed} failure(s) ({passed} passed):\n{}\n\
             stderr:\n{stderr}",
            failures.join("\n")
        );
    }
    assert!(
        passed >= 20,
        "DOM render test only had {passed} passed assertions — \
         script may not have executed correctly\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
