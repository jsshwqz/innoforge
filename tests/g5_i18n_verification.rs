//! G5: Verify 8 hardcoded English strings that bypass i18n (issue #34).
//!
//! This test documents 8 hardcoded English strings found in the codebase
//! that are NOT routed through the i18n system. When the UI is in Chinese
//! mode (lang="zh-CN"), these strings still appear in English.
//!
//! ## Findings
//!
//! | # | String | Location | User-facing? | Goes through i18n? |
//! |---|--------|----------|-------------|-------------------|
//! | 1 | `Loading...` | templates/idea.html:460,496,539,572,1617 | Yes (innerHTML) | NO |
//! | 2 | `Patent not found` | src/routes/ai.rs:634 | Yes (AI chat content) | NO |
//! | 3 | `Patent not found` | src/routes/patent.rs:40,263,335,576 | Yes (API error) | NO |
//! | 4 | `Use millimetres` | src/routes/cad.rs:353 | Yes (CAD instruction) | NO |
//! | 5 | `PDF store failed` | templates/office_action_response.html:330 | Yes (alert) | NO |
//! | 6 | `Missing required field: patent_id` | src/routes/oa_rounds.rs:50 | Yes (API error) | NO |
//! | 7 | `Missing required field: round_type` | src/routes/oa_rounds.rs:60 | Yes (API error) | NO |
//! | 8 | `No rows updated` | src/routes/oa_rounds.rs:204 | Yes (API error) | NO |
//!
//! ## Strings that DO go through i18n (for contrast)
//!
//! | String | Location | Mechanism |
//! |--------|----------|-----------|
//! | `No ideas yet` / `暂无创意记录` | templates/idea.html:364 | `i18nLang === 'en' ? ... : ...` |
//! | `No discussion yet` / `暂无讨论记录` | templates/idea.html:627 | `i18nLang === 'en' ? ... : ...` |
//! | `No idea selected` / `请先选择一个创意` | templates/idea.html:696 | `i18nLang === 'en' ? ... : ...` |
//! | `Failed to load history` / `加载历史失败` | templates/idea.html:431 | `i18nLang === 'en' ? ... : ...` |
//! | `Search` / `搜索` | templates/search.html (i18n.js) | `t('btn.search')` |
//! | `Please enter a query` / `请输入搜索内容` | templates/search.html (i18n.js) | `t('alert.emptyQuery')` |
//!
//! ## Conclusion
//!
//! 8 hardcoded English strings bypass i18n. The most impactful is `Loading...`
//! in idea.html (5 occurrences, directly visible to users). The API error
//! messages (`Patent not found`, `Missing required field`, `No rows updated`)
//! are returned as JSON and typically shown in alerts or error divs.
//!
//! Fix recommendation: replace each with `i18nLang === 'en' ? 'English' : '中文'`
//! (frontend) or return localized messages based on Accept-Language header (backend).

use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn g5_verify_8_hardcoded_english_strings_exist() {
    // This test verifies that the 8 documented hardcoded English strings
    // are present in the source code at the expected locations.
    //
    // If any string is removed or localized, this test should be updated
    // to reflect the new state.

    let idea_html = include_str!("../templates/idea.html");
    let oar_html = include_str!("../templates/office_action_response.html");
    let ai_rs = include_str!("../src/routes/ai.rs");
    let patent_rs = include_str!("../src/routes/patent.rs");
    let cad_rs = include_str!("../src/routes/cad.rs");
    let oa_rounds_rs = include_str!("../src/routes/oa_rounds.rs");

    // 1. Loading... in idea.html (5 occurrences)
    let loading_count = idea_html.matches("Loading...").count();
    assert!(
        loading_count >= 5,
        "Expected ≥5 'Loading...' in idea.html, found {}",
        loading_count
    );

    // 2. Patent not found in ai.rs
    assert!(
        ai_rs.contains("\"Patent not found\""),
        "Expected 'Patent not found' in ai.rs"
    );

    // 3. Patent not found in patent.rs
    assert!(
        patent_rs.contains("\"Patent not found\"") || patent_rs.contains("Patent not found"),
        "Expected 'Patent not found' in patent.rs"
    );

    // 4. Use millimetres in cad.rs
    assert!(
        cad_rs.contains("\"Use millimetres\""),
        "Expected 'Use millimetres' in cad.rs"
    );

    // 5. PDF store failed in office_action_response.html
    assert!(
        oar_html.contains("'PDF store failed'"),
        "Expected 'PDF store failed' in office_action_response.html"
    );

    // 6. Missing required field: patent_id in oa_rounds.rs
    assert!(
        oa_rounds_rs.contains("Missing required field: patent_id"),
        "Expected 'Missing required field: patent_id' in oa_rounds.rs"
    );

    // 7. Missing required field: round_type in oa_rounds.rs
    assert!(
        oa_rounds_rs.contains("Missing required field: round_type"),
        "Expected 'Missing required field: round_type' in oa_rounds.rs"
    );

    // 8. No rows updated in oa_rounds.rs
    assert!(
        oa_rounds_rs.contains("\"No rows updated\""),
        "Expected 'No rows updated' in oa_rounds.rs"
    );
}

#[test]
fn g5_verify_i18n_handled_strings_have_chinese() {
    // Verify that strings which DO go through i18n have Chinese translations.
    // This proves the i18n system is working for those strings.

    let idea_html = include_str!("../templates/idea.html");
    let search_html = include_str!("../templates/search.html");

    // idea.html inline i18n: should have both English and Chinese
    assert!(
        idea_html.contains("No ideas yet") && idea_html.contains("暂无创意记录"),
        "idea.html should have both English and Chinese for 'No ideas yet'"
    );

    assert!(
        idea_html.contains("No discussion yet") && idea_html.contains("暂无讨论记录"),
        "idea.html should have both English and Chinese for 'No discussion yet'"
    );

    assert!(
        idea_html.contains("No idea selected") && idea_html.contains("请先选择一个创意"),
        "idea.html should have both English and Chinese for 'No idea selected'"
    );

    // i18n/search.html should have Chinese translations for search page
    assert!(
        search_html.contains("请输入搜索内容"),
        "i18n/search.html should have Chinese for 'Please enter a query'"
    );

    assert!(
        search_html.contains("搜索失败"),
        "i18n/search.html should have Chinese for 'Search failed'"
    );
}

#[test]
fn g5_loading_string_bypasses_i18n() {
    // Negative verification: 'Loading...' in idea.html is NOT wrapped in
    // i18nLang check — it's always English regardless of language setting.
    //
    // This is the most impactful hardcoded English string because it's
    // directly set as innerHTML and visible to users on every page load.

    let idea_html = include_str!("../templates/idea.html");

    // Find all lines with 'Loading...'
    let loading_lines: Vec<&str> = idea_html
        .lines()
        .filter(|l| l.contains("Loading..."))
        .collect();

    assert!(
        !loading_lines.is_empty(),
        "Should find 'Loading...' in idea.html"
    );

    // Verify none of them use i18nLang or t()
    for (i, line) in loading_lines.iter().enumerate() {
        let uses_i18n = line.contains("i18nLang") || line.contains("t('") || line.contains("t(\"");
        assert!(
            !uses_i18n,
            "Line with 'Loading...' #{} should NOT use i18n (but does): {}",
            i,
            line.trim()
        );
    }
}
