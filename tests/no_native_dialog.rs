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
}
