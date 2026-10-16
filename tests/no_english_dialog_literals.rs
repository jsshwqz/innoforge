//! Static guard for issue #16: ensure no hardcoded English alert/confirm
//! literals bypass the i18n system. Also verifies t() fallback behavior.
//!
//! The guard counts `alert('…')` / `confirm('…')` calls where the argument
//! is a string literal that contains NO Chinese characters and is NOT
//! wrapped in t(). The baseline after #16 fix is 0.

#[cfg(test)]
mod tests {
    static TEMPLATES: &[(&str, &str)] = &[
        ("ai.html", include_str!("../templates/ai.html")),
        ("compare.html", include_str!("../templates/compare.html")),
        ("idea.html", include_str!("../templates/idea.html")),
        ("index.html", include_str!("../templates/index.html")),
        (
            "office_action_response.html",
            include_str!("../templates/office_action_response.html"),
        ),
        (
            "patent_detail.html",
            include_str!("../templates/patent_detail.html"),
        ),
        ("search.html", include_str!("../templates/search.html")),
        ("settings.html", include_str!("../templates/settings.html")),
    ];

    // Check if a string contains Chinese characters
    fn has_chinese(s: &str) -> bool {
        s.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
    }

    /// Find all alert('...') and confirm('...') calls with literal string
    /// arguments that contain no Chinese and are not wrapped in t().
    fn count_english_literals() -> Vec<String> {
        let mut violations = Vec::new();
        for (name, content) in TEMPLATES {
            // Search for alert(' and confirm(' patterns
            for prefix in &["alert('", "confirm('"] {
                let mut search_start = 0;
                while let Some(idx) = content[search_start..].find(prefix) {
                    let abs_idx = search_start + idx;
                    // Find the closing quote
                    let after_prefix = &content[abs_idx + prefix.len()..];
                    if let Some(end) = after_prefix.find('\'') {
                        let literal = &after_prefix[..end];
                        // Check: no Chinese, not empty, not a t() call
                        if !literal.is_empty() && !has_chinese(literal) {
                            // Check it's not already inside a t() call
                            // by looking at what comes before the alert/confirm
                            let before = &content[..abs_idx];
                            // If the last non-whitespace before this is t(,
                            // it's wrapped in t() — skip
                            let trimmed = before.trim_end();
                            if !trimmed.ends_with("t(") {
                                violations.push(format!(
                                    "{}: {}('{}')",
                                    name,
                                    prefix.trim_end_matches('('),
                                    literal
                                ));
                            }
                        }
                    }
                    search_start = abs_idx + prefix.len();
                }
            }
        }
        violations
    }

    // Baseline: after issue #16 fix, there should be 0 English-only literals
    const BASELINE_ENGLISH_LITERALS: usize = 0;

    #[test]
    fn no_english_only_alert_confirm_literals() {
        let violations = count_english_literals();
        assert!(
            violations.len() == BASELINE_ENGLISH_LITERALS,
            "Found {} hardcoded English alert/confirm literals (baseline {}):\n{}",
            violations.len(),
            BASELINE_ENGLISH_LITERALS,
            violations.join("\n")
        );
    }

    /// Negative test B: verify t() has a fallback so missing keys show
    /// the key string itself, not `undefined`.
    #[test]
    fn t_function_has_fallback_for_missing_keys() {
        let i18n = include_str!("../static/i18n.js");
        // The t() function must have `|| key` fallback logic
        assert!(
            i18n.contains("|| key") || i18n.contains("||key"),
            "t() function must have fallback (|| key) so missing keys show the key, not undefined"
        );
    }

    /// Verify all 8 new i18n keys exist in both zh and en sections
    #[test]
    fn issue16_keys_exist_in_both_languages() {
        let i18n = include_str!("../static/i18n.js");

        let keys = [
            "idea.chatNotFound",
            "idea.pdfExtractFailed",
            "idea.pdfUploadFailed",
            "idea.conceptNameDescRequired",
            "idea.deleteMemoryConfirm",
            "idea.deleteFailed",
            "common.failed",
            "oar.pdfPreviewFailed",
        ];

        for key in &keys {
            let count = i18n.matches(&format!("'{}'", key)).count();
            assert!(
                count >= 2,
                "i18n key '{}' should appear at least twice (zh + en), found {} times",
                key,
                count
            );
        }
    }

    // ── Negative controls: prove the guard logic actually fires ──

    /// Negative control: `alert('Error')` with no Chinese must be detected
    /// as an English-only literal.
    #[test]
    fn negative_control_english_alert_detected() {
        let synthetic = "alert('Error occurred')";
        // Replicate the guard's detection logic
        let prefix = "alert('";
        let idx = synthetic.find(prefix).unwrap();
        let after = &synthetic[idx + prefix.len()..];
        let end = after.find('\'').unwrap();
        let literal = &after[..end];
        assert!(
            !literal.is_empty() && !has_chinese(literal),
            "negative control: English alert literal must be detected as violation"
        );
    }

    /// Negative control: `alert('错误发生')` with Chinese must NOT be
    /// detected as an English-only literal.
    #[test]
    fn negative_control_chinese_alert_passes() {
        let synthetic = "alert('错误发生')";
        let prefix = "alert('";
        let idx = synthetic.find(prefix).unwrap();
        let after = &synthetic[idx + prefix.len()..];
        let end = after.find('\'').unwrap();
        let literal = &after[..end];
        assert!(
            has_chinese(literal),
            "negative control: Chinese alert literal must not be flagged"
        );
    }

    /// Negative control: `t(alert('Error'))` wrapped in t() must NOT be
    /// detected (the guard skips t()-wrapped calls).
    #[test]
    fn negative_control_t_wrapped_passes() {
        let synthetic = "t(alert('Error'))";
        let prefix = "alert('";
        let idx = synthetic.find(prefix).unwrap();
        let before = &synthetic[..idx];
        let trimmed = before.trim_end();
        assert!(
            trimmed.ends_with("t("),
            "negative control: t()-wrapped alert must be recognized as wrapped"
        );
    }
}
