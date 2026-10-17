//! G8: Strengthened Doc Staleness CI job — negative verification.
//!
//! The strengthened check requires NEW entry lines (lines starting with
//! `## ` or `### `) in docs/errors.md or docs/plans/STATUS.md, not just
//! any change to existing text.
//!
//! This test simulates both scenarios:
//! 1. GREEN: A diff with new `###` entry lines → should pass
//! 2. RED: A diff that only modifies existing text (no new ## or ###) → should fail

use std::fs;

/// Simulates the strengthened doc-staleness check.
/// Returns true if the check would PASS (new entries found), false if it would FAIL.
fn check_new_entries(base_content: &str, head_content: &str) -> bool {
    // Simulate git diff: find lines starting with '+' that begin with '## ' or '### '
    let base_lines: Vec<&str> = base_content.lines().collect();
    let head_lines: Vec<&str> = head_content.lines().collect();

    // Simple diff: any line in head that starts with "## " or "### "
    // and is NOT in base (i.e., it's a new entry line)
    let mut new_entries = 0;
    for line in &head_lines {
        if (line.starts_with("## ") || line.starts_with("### ")) && !base_lines.contains(line) {
            new_entries += 1;
        }
    }

    new_entries > 0
}

#[test]
fn g8_green_new_entry_lines_pass() {
    // Scenario: STATUS.md gets a new `###` entry — check should PASS
    let base = "# Status\n\n## Existing Section\nSome existing content.\n";
    let head = "# Status\n\n## Existing Section\nSome existing content.\n\n### New Entry — G8 Test\nNew content.\n";

    assert!(
        check_new_entries(base, head),
        "A diff with new '###' entry lines should PASS the strengthened check"
    );
}

#[test]
fn g8_red_only_modifies_existing_text_fails() {
    // Scenario: STATUS.md is modified but only existing text is changed —
    // no new ## or ### lines — check should FAIL
    let base = "# Status\n\n## Existing Section\nSome existing content.\n";
    let head = "# Status\n\n## Existing Section\nModified existing content.\n";

    assert!(
        !check_new_entries(base, head),
        "A diff that only modifies existing text (no new ## or ###) should FAIL the strengthened check"
    );
}

#[test]
fn g8_green_new_h2_entry_passes() {
    // Scenario: errors.md gets a new `##` entry — check should PASS
    let base = "# Errors\n\n## Existing Error\nDescription.\n";
    let head =
        "# Errors\n\n## Existing Error\nDescription.\n\n## New Error Category\nNew description.\n";

    assert!(
        check_new_entries(base, head),
        "A diff with new '##' entry lines should PASS the strengthened check"
    );
}

#[test]
fn g8_red_no_doc_change_fails() {
    // Scenario: No change to docs at all — check should FAIL
    let base = "# Status\n\n## Existing Section\nContent.\n";
    let head = base; // identical

    assert!(
        !check_new_entries(base, head),
        "No doc change should FAIL the strengthened check"
    );
}

#[test]
fn g8_ci_yaml_contains_strengthened_check() {
    // Verify the CI YAML contains the strengthened check logic
    let ci = fs::read_to_string(".github/workflows/ci.yml").expect("ci.yml must exist");

    // The strengthened check must look for new entry lines
    assert!(
        ci.contains("NEW_ENTRIES"),
        "CI must track NEW_ENTRIES count"
    );
    assert!(
        ci.contains("##|###"),
        "CI must grep for new ## or ### lines"
    );
    assert!(
        ci.contains("new entry line"),
        "CI must report new entry lines"
    );
    // The old weak check (just DOCS_CHANGED=1 → pass) should be gone
    assert!(
        !ci.contains("Retrospective docs updated — check passes"),
        "Old weak check 'docs updated — check passes' must be removed"
    );
}

#[test]
fn g8_ci_yaml_failure_message_distinguishes_cases() {
    let ci = fs::read_to_string(".github/workflows/ci.yml").unwrap();

    // The failure message should distinguish between:
    // 1. Docs changed but no new entries
    // 2. Docs not changed at all
    assert!(
        ci.contains("no new entry lines"),
        "Failure message must mention 'no new entry lines' for the docs-changed-but-no-new-entries case"
    );
    assert!(
        ci.contains("not updated"),
        "Failure message must mention 'not updated' for the docs-not-changed case"
    );
}
