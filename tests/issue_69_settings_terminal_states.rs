//! Issue #69: Settings page four terminal states — static guards + integration.
//!
//! ① No hardcoded provider names in `status.push('…')` string literals.
//! ③ `ai_api_key` echo is empty for every placeholder shape; `ai_api_key_configured` is false.
//! ④ FreeCAD probe has an upper-bound timeout and a clear terminal state on failure.
//!
//! All test names carry `issue_69` for discoverability.

mod common;
use common::{free_port, spawn_test_server};

const SETTINGS_HTML: &str = include_str!("../templates/settings.html");
const I18N_JS: &str = include_str!("../static/i18n.js");
const SETTINGS_RS: &str = include_str!("../src/routes/settings.rs");

// ── ① Static guard: no hardcoded AI provider name in status.push() literals ──

/// Provider display names that must NOT appear as hardcoded string literals
/// inside `status.push('…')` calls **in the AI service status block**.
/// These are the same names that appear in the `AI_PROVIDERS` table (which is
/// a data source and intentionally excluded from this scan).
const AI_PROVIDER_NAMES: &[&str] = &[
    "DeepSeek",
    "Ollama",
    "Moonshot",
    "OpenAI",
    "Anthropic",
    "Qwen",
    "Gemini",
    "MiMo",
    "SenseNova",
    "OpenRouter",
    "GLM",
];

/// Extract the argument of `status.push(…)` on a single line.
/// Returns `Some(arg)` if the argument is a **single string literal** (no `+`
/// concatenation), `None` otherwise (dynamic expression — allowed).
fn extract_status_push_literal(line: &str) -> Option<String> {
    let push = line.find("status.push(")?;
    let after = &line[push + "status.push(".len()..];
    if !after.starts_with('\'') && !after.starts_with('"') {
        return None;
    }
    if after.contains(" + ") {
        return None;
    }
    Some(after.to_string())
}

/// Find the line range of the AI service status block — the
/// `if (config.ai_api_key_configured)` branch and its `else`, up to (but not
/// including) the next top-level config check (e.g.
/// `if (config.google_oauth_connected)`).  Only this block is scanned for
/// hardcoded provider names, so legitimate mentions like "Gemini 高级额度" in
/// the Google OAuth status are not false positives.
fn ai_status_block_range(html: &str) -> Option<(usize, usize)> {
    let lines: Vec<&str> = html.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.contains("config.ai_api_key_configured"))?;
    let end = lines.iter().skip(start + 1).position(|l| {
        let t = l.trim();
        t.starts_with("if (config.") && !t.contains("ai_api_key")
    })? + start
        + 1;
    Some((start, end))
}

#[test]
fn issue_69_no_hardcoded_provider_in_status_push() {
    let (start, end) = ai_status_block_range(SETTINGS_HTML)
        .expect("AI service status block must exist in settings.html");

    for idx in start..end {
        let line = SETTINGS_HTML.lines().nth(idx).unwrap_or("");
        let literal = match extract_status_push_literal(line) {
            Some(l) => l,
            None => continue,
        };
        for &provider in AI_PROVIDER_NAMES {
            assert!(
                !literal.contains(provider),
                "issue #69: templates/settings.html:{} — status.push() \
                 contains hardcoded provider name (category: provider-literal)",
                idx + 1,
            );
        }
    }
}

// ── ③ Backend: placeholder keys ⇒ empty echo + configured=false ──

/// Verify that `mask_api_key` in settings.rs delegates to
/// `is_placeholder_api_key` (single source of truth) rather than
/// checking only a subset of placeholders inline.
#[test]
fn issue_69_mask_api_key_uses_is_placeholder_api_key() {
    assert!(
        SETTINGS_RS.contains("super::is_placeholder_api_key"),
        "issue #69: mask_api_key must delegate to is_placeholder_api_key"
    );
    assert!(
        !SETTINGS_RS.contains(r#"key == "your-serpapi-key-here""#),
        "issue #69: mask_api_key must not inline-check individual placeholders"
    );
}

/// Integration: for every known placeholder shape, /api/settings returns
/// `ai_api_key == ""` and `ai_api_key_configured == false`.
#[test]
fn issue_69_placeholder_keys_echo_empty_and_unconfigured() {
    let placeholders: &[&str] = &["", "ollama", "sk-xxx", "your-serpapi-key-here"];

    for &placeholder in placeholders {
        let port = free_port();
        let env_key = if placeholder.is_empty() {
            vec![("AI_BASE_URL", "https://api.deepseek.com/v1")]
        } else {
            vec![
                ("AI_BASE_URL", "https://api.deepseek.com/v1"),
                ("AI_API_KEY", placeholder),
            ]
        };

        let server = spawn_test_server(port, &env_key);
        let client = reqwest::blocking::Client::new();

        let resp = client
            .get(format!("http://127.0.0.1:{}/api/settings", port))
            .send()
            .expect("settings must respond");
        assert!(
            resp.status().is_success(),
            "settings status for placeholder {:?}",
            placeholder
        );

        let json: serde_json::Value = resp.json().expect("settings json");
        let key_echo = json["ai_api_key"].as_str().unwrap_or("");
        let configured = json["ai_api_key_configured"].as_bool().unwrap_or(true);

        assert!(
            key_echo.is_empty(),
            "issue #69: placeholder {:?} must echo empty ai_api_key, got {:?} \
             (category: mask-leak)",
            placeholder,
            key_echo,
        );
        assert!(
            !configured,
            "issue #69: placeholder {:?} must have ai_api_key_configured=false \
             (category: false-configured)",
            placeholder,
        );

        drop(server);
    }
}

/// Negative control: a real (non-placeholder) key must show as configured=true
/// and ai_api_key must be masked (non-empty, containing ****).
/// This proves the placeholder check is actually discriminating — if
/// is_placeholder_api_key were short-circuited to always-true, this test
/// would fail.
#[test]
fn issue_69_real_key_shows_configured_and_masked() {
    let port = free_port();
    let server = spawn_test_server(
        port,
        &[
            ("AI_BASE_URL", "https://api.deepseek.com/v1"),
            ("AI_API_KEY", "sk-real-key-for-negative-control-69"),
        ],
    );

    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(format!("http://127.0.0.1:{}/api/settings", port))
        .send()
        .expect("settings must respond");
    let json: serde_json::Value = resp.json().expect("settings json");

    let configured = json["ai_api_key_configured"].as_bool().unwrap_or(false);
    assert!(
        configured,
        "issue #69 negative control: real key must have configured=true"
    );

    let key_echo = json["ai_api_key"].as_str().unwrap_or("");
    assert!(
        !key_echo.is_empty() && key_echo.contains("****"),
        "issue #69 negative control: real key must be masked (non-empty + ****), got {:?}",
        key_echo,
    );

    drop(server);
}

// ── ③ Frontend: empty echo must not fill **** ──

/// Verify the frontend uses `config.ai_api_key || ''` (or equivalent)
/// so that an empty backend echo results in an empty input value,
/// NOT a **** fallback.
#[test]
fn issue_69_frontend_uses_empty_fallback_not_stars() {
    assert!(
        SETTINGS_HTML.contains("config.ai_api_key || ''"),
        "issue #69: frontend must use config.ai_api_key || '' for the key input"
    );
    let key_load_line = SETTINGS_HTML
        .lines()
        .find(|l| l.contains("ai-api-key") && l.contains(".value"))
        .expect("ai-api-key value assignment must exist");
    assert!(
        !key_load_line.contains("****"),
        "issue #69: frontend must not fill **** for the API key input (category: star-fallback)"
    );
}

// ── ④ FreeCAD: timeout + terminal state ──

/// Verify refreshFreeCadStatus has an upper-bound timeout (AbortController).
#[test]
fn issue_69_freecad_probe_has_timeout() {
    assert!(
        SETTINGS_HTML.contains("AbortController"),
        "issue #69: refreshFreeCadStatus must use AbortController for timeout"
    );
    assert!(
        SETTINGS_HTML.contains("setTimeout"),
        "issue #69: FreeCAD probe must use setTimeout for timeout bound"
    );
    assert!(
        SETTINGS_HTML.contains("clearTimeout"),
        "issue #69: FreeCAD probe must clear timeout on success"
    );
}

/// Verify the FreeCAD failure path shows a terminal state (not "checking").
#[test]
fn issue_69_freecad_failure_shows_terminal_state() {
    assert!(
        SETTINGS_HTML.contains("cad.checkTimeout"),
        "issue #69: FreeCAD timeout must show cad.checkTimeout terminal state"
    );
    assert!(
        SETTINGS_HTML.contains("cad.notDetected"),
        "issue #69: FreeCAD error must show cad.notDetected terminal state"
    );
    assert!(
        SETTINGS_HTML.contains("checkAndStartFreeCad"),
        "issue #69: FreeCAD must have a retry entry point"
    );
}

/// Verify the i18n strings for the new terminal states exist in both languages.
#[test]
fn issue_69_freecad_i18n_strings_exist() {
    assert!(
        I18N_JS.contains("'cad.notDetected'"),
        "issue #69: i18n must define cad.notDetected"
    );
    assert!(
        I18N_JS.contains("'cad.checkTimeout'"),
        "issue #69: i18n must define cad.checkTimeout"
    );
    let count = I18N_JS.matches("'cad.notDetected'").count();
    assert!(
        count >= 2,
        "issue #69: cad.notDetected must exist in both zh and en sections, found {}",
        count
    );
    let count = I18N_JS.matches("'cad.checkTimeout'").count();
    assert!(
        count >= 2,
        "issue #69: cad.checkTimeout must exist in both zh and en sections, found {}",
        count
    );
}

/// Verify the backend WARN is a single-line format ({e} not {e:?}).
#[test]
fn issue_69_cad_warn_is_single_line() {
    let cad_rs = include_str!("../src/cad.rs");
    assert!(
        !cad_rs.contains("CAD check_ready failed: {e:?}"),
        "issue #69: CAD WARN must use {{e}} (single-line) not {{e:?}} (debug multi-line)"
    );
    assert!(
        cad_rs.contains("CAD check_ready failed: {e}"),
        "issue #69: CAD WARN must be present in single-line format"
    );
}

// ── ② Custom dropdown terminal state (control-flow guard) ──

/// Extract the lines of the `if (selectedProvider === 'custom')` block
/// that appear **before** the top-level `return;`, i.e. the code that
/// runs unconditionally when the provider is `custom` (regardless of
/// `isManual`).  Brace-depth tracking ensures we only stop at a `return`
/// that sits at depth 1 — the top level of the if block — not one nested
/// inside `if (isManual) { … }`.
fn custom_branch_lines_before_return(html: &str) -> Vec<String> {
    let lines: Vec<&str> = html.lines().collect();

    let custom_idx = lines
        .iter()
        .position(|l| {
            let t = l.trim();
            t.starts_with("if") && l.contains("selectedProvider === 'custom'")
        })
        .expect("custom branch if-statement must exist in settings.html");

    let mut depth: i32 = 0;
    let mut collected: Vec<String> = Vec::new();

    for (i, line) in lines.iter().enumerate().skip(custom_idx) {
        let depth_before = depth;
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
            }
        }
        // Block closed — stop without collecting the closing brace line.
        if depth == 0 && i > custom_idx {
            break;
        }
        // Top-level return inside the if block — stop, don't collect it.
        if depth_before == 1 && line.trim().starts_with("return") {
            break;
        }
        // Collect every line inside the block (skip the if-line itself).
        if i > custom_idx && depth > 0 {
            collected.push(line.to_string());
        }
    }
    collected
}

/// Control-flow assertion: the `selectedProvider === 'custom'` branch
/// must assign a terminal state to **both** `ai-model` and
/// `ai-model-expert` via `innerHTML` **before** its `return`, the
/// assigned text must not contain "加载中", and both assignments must
/// go through `DOMPurify.sanitize`.
///
/// This is falsifiable: if the assignments are removed (bare `return`),
/// or if only one dropdown is set, or if the text still says "加载中",
/// the test goes red.
#[test]
fn issue_69_custom_branch_sets_dropdowns_before_return() {
    let lines = custom_branch_lines_before_return(SETTINGS_HTML);
    assert!(
        !lines.is_empty(),
        "issue #69: custom branch must contain code before return, not a bare return"
    );

    let ai_model_set = lines
        .iter()
        .any(|l| l.contains("getElementById('ai-model')") && l.contains(".innerHTML"));
    assert!(
        ai_model_set,
        "issue #69: custom branch must assign ai-model innerHTML before return          (category: missing-terminal-state)"
    );

    let ai_model_expert_set = lines
        .iter()
        .any(|l| l.contains("getElementById('ai-model-expert')") && l.contains(".innerHTML"));
    assert!(
        ai_model_expert_set,
        "issue #69: custom branch must assign ai-model-expert innerHTML before return          (category: missing-terminal-state)"
    );

    let still_loading = lines
        .iter()
        .any(|l| l.contains(".innerHTML") && l.contains("加载中"));
    assert!(
        !still_loading,
        "issue #69: custom branch must not leave dropdowns in '加载中' state          (category: stuck-loading)"
    );

    let ai_model_sanitized = lines.iter().any(|l| {
        l.contains("getElementById('ai-model')")
            && l.contains(".innerHTML")
            && l.contains("DOMPurify.sanitize")
    });
    assert!(
        ai_model_sanitized,
        "issue #69: custom branch ai-model assignment must use DOMPurify.sanitize"
    );

    let ai_model_expert_sanitized = lines.iter().any(|l| {
        l.contains("getElementById('ai-model-expert')")
            && l.contains(".innerHTML")
            && l.contains("DOMPurify.sanitize")
    });
    assert!(
        ai_model_expert_sanitized,
        "issue #69: custom branch ai-model-expert assignment must use DOMPurify.sanitize"
    );
}

/// Sanitize consistency: every line that sets `innerHTML` to a value
/// containing "加载中" (the initial loading placeholder) must use
/// `DOMPurify.sanitize`.  This catches the line-451/452 inconsistency
/// where one dropdown was sanitized and the other was not.
#[test]
fn issue_69_loading_state_lines_use_sanitize_consistently() {
    let loading_lines: Vec<_> = SETTINGS_HTML
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains(".innerHTML") && l.contains("加载中"))
        .collect();

    assert!(
        !loading_lines.is_empty(),
        "issue #69: must have loading-state innerHTML lines to test"
    );

    for (idx, line) in &loading_lines {
        assert!(
            line.contains("DOMPurify.sanitize"),
            "issue #69: settings.html:{} sets innerHTML with '加载中' but does              not use DOMPurify.sanitize (category: sanitize-mismatch)",
            idx + 1,
        );
    }
}

// ── ① Verify the status.push for AI uses dynamic provider title ──

/// Verify the AI status line uses a dynamic provider title (concatenation)
/// rather than a hardcoded string literal.
#[test]
fn issue_69_ai_status_uses_dynamic_provider_title() {
    let configured_line = SETTINGS_HTML
        .lines()
        .find(|l| l.contains("status.push") && l.contains("AI 服务已配置"))
        .expect("AI service configured status line must exist");
    assert!(
        configured_line.contains(" + "),
        "issue #69: AI status push must use dynamic concatenation, not a hardcoded literal"
    );
    assert!(
        configured_line.contains("_pTitle") || configured_line.contains("providerTitle"),
        "issue #69: AI status push must reference a dynamic provider title variable"
    );
}
