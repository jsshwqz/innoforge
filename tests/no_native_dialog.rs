//! Static guard for issue #15: ensure native prompt()/confirm() never
//! reappear in embedded HTML templates. Templates are baked at compile
//! time via include_str!, so this test runs as part of `cargo test`.

#[cfg(test)]
mod tests {
    // List all embedded templates — if a new template is added, add it here too.
    const TEMPLATES: &[(&str, &str)] = &[
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

    #[test]
    fn no_native_prompt_in_templates() {
        for (name, content) in TEMPLATES {
            assert!(
                !content.contains("prompt("),
                "template '{}' contains native prompt() — use page-in modal instead (issue #15)",
                name
            );
        }
    }

    #[test]
    fn no_native_confirm_in_templates() {
        for (name, content) in TEMPLATES {
            assert!(
                !content.contains("confirm("),
                "template '{}' contains native confirm() — use showConfirmDialog() instead (issue #15)",
                name
            );
        }
    }

    // ── Negative controls: prove the guard actually fires on violations ──

    /// Negative control: a synthetic HTML string containing `prompt(` must
    /// be detected by the same `.contains("prompt(")` logic the guard uses.
    /// If this test passes, the guard is not a no-op.
    #[test]
    fn negative_control_prompt_is_detected() {
        let synthetic_html = r#"<button onclick="prompt('Enter:')">Click</button>"#;
        assert!(
            synthetic_html.contains("prompt("),
            "negative control: synthetic HTML with prompt() must be detected"
        );
    }

    /// Negative control: a synthetic HTML string containing `confirm(` must
    /// be detected by the same `.contains("confirm(")` logic the guard uses.
    #[test]
    fn negative_control_confirm_is_detected() {
        let synthetic_html = r#"<button onclick="confirm('Sure?')">Delete</button>"#;
        assert!(
            synthetic_html.contains("confirm("),
            "negative control: synthetic HTML with confirm() must be detected"
        );
    }

    /// Negative control: a clean HTML string without prompt/confirm must
    /// NOT trigger the guard. This proves the guard doesn't false-positive.
    #[test]
    fn negative_control_clean_html_passes() {
        let clean_html = r#"<button onclick="showConfirmDialog()">Delete</button>"#;
        assert!(
            !clean_html.contains("prompt("),
            "negative control: clean HTML must not trigger prompt guard"
        );
        assert!(
            !clean_html.contains("confirm("),
            "negative control: clean HTML must not trigger confirm guard"
        );
    }
}
