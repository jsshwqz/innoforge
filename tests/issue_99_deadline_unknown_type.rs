//! Regression for issue #99: unknown `oa_type` silently fell back to 4 months
//! with no warning, returning a deadline that looked legitimate but was
//! potentially a month too late.
//!
//! Fixes:
//! 1. Unknown oa_type now returns `recognized: false` + `warning` field.
//! 2. Fallback period is the SHORTEST valid period (2 months), not 4.
//! 3. Valid types list is a single source of truth (VALID_OA_TYPES).
//!
//! Negative control B: if the fallback months are changed to any other value,
//! the test `issue_99_unknown_type_uses_shortest_period` will fail because it
//! asserts `spec == Months(2)`, proving the assertion pins on "unknown type
//! must be identifiable", not on a specific month count that could be
//! silently changed.

mod common;
use common::{free_port, spawn_test_server};

/// #99 positive: known oa_types produce correct deadlines.
#[test]
fn issue_99_known_types_correct_deadlines() {
    let port = free_port();
    let server = spawn_test_server(port, &[]);
    let client = reqwest::blocking::Client::new();

    // reject_review → +3 months
    let resp: serde_json::Value = client
        .get(format!(
            "http://127.0.0.1:{}/api/oa/compute-deadline?oa_type=reject_review&date=2026-08-11",
            port
        ))
        .send()
        .expect("request must succeed")
        .json()
        .expect("json");
    assert_eq!(resp["deadline"], "2026-11-11");
    assert_eq!(resp["spec"]["kind"], "Months");
    assert_eq!(resp["spec"]["value"], 3);
    assert_eq!(resp["recognized"], true);

    // second_rejection → +2 months
    let resp: serde_json::Value = client
        .get(format!(
            "http://127.0.0.1:{}/api/oa/compute-deadline?oa_type=second_rejection&date=2026-08-11",
            port
        ))
        .send()
        .expect("request must succeed")
        .json()
        .expect("json");
    assert_eq!(resp["deadline"], "2026-10-11");
    assert_eq!(resp["spec"]["value"], 2);
    assert_eq!(resp["recognized"], true);

    // first_exam → +4 months
    let resp: serde_json::Value = client
        .get(format!(
            "http://127.0.0.1:{}/api/oa/compute-deadline?oa_type=first_exam&date=2026-08-11",
            port
        ))
        .send()
        .expect("request must succeed")
        .json()
        .expect("json");
    assert_eq!(resp["deadline"], "2026-12-11");
    assert_eq!(resp["spec"]["value"], 4);
    assert_eq!(resp["recognized"], true);

    drop(server);
}

/// #99 negative control A: unknown oa_type must be marked as unrecognized
/// with a warning.  The response must contain `recognized: false`.
#[test]
fn issue_99_unknown_type_marked_unrecognized() {
    let port = free_port();
    let server = spawn_test_server(port, &[]);
    let client = reqwest::blocking::Client::new();

    let resp: serde_json::Value = client
        .get(format!(
            "http://127.0.0.1:{}/api/oa/compute-deadline?oa_type=typo_xxx&date=2026-08-11",
            port
        ))
        .send()
        .expect("request must succeed")
        .json()
        .expect("json");

    // Must have recognized: false
    let recognized = resp["recognized"].as_bool();
    assert_eq!(
        recognized,
        Some(false),
        "#99 neg-A: unknown oa_type must have recognized=false, got: {resp}"
    );

    // Must have a warning field
    let warning = resp["warning"].as_str();
    assert!(
        warning.is_some(),
        "#99 neg-A: unknown oa_type must have a warning, got: {resp}"
    );
    assert!(
        warning.unwrap().contains("unknown"),
        "#99 neg-A: warning must mention 'unknown', got: {}",
        warning.unwrap()
    );

    drop(server);
}

/// #99 negative control B: the fallback period must be the SHORTEST valid
/// period (2 months), not 4.  This test pins on the specific value to prove
/// that changing the fallback to any other value would be caught.
/// If someone changes the fallback from 2 to 4 (or any other number),
/// this test will fail.
#[test]
fn issue_99_unknown_type_uses_shortest_period() {
    let port = free_port();
    let server = spawn_test_server(port, &[]);
    let client = reqwest::blocking::Client::new();

    let resp: serde_json::Value = client
        .get(format!(
            "http://127.0.0.1:{}/api/oa/compute-deadline?oa_type=nonexistent_type&date=2026-08-11",
            port
        ))
        .send()
        .expect("request must succeed")
        .json()
        .expect("json");

    // Must be unrecognized
    let recognized = resp["recognized"].as_bool();
    assert_eq!(recognized, Some(false), "#99 neg-B: must be unrecognized");

    // Must use 2 months (shortest valid period), NOT 4 months (the old default)
    let spec_value = resp["spec"]["value"].as_u64().expect("spec value");
    assert_eq!(
        spec_value, 2,
        "#99 neg-B: fallback must be 2 months (shortest), not 4.          If this fails, someone changed the fallback period."
    );

    // 2026-08-11 + 2 months = 2026-10-11 (not 2026-12-11 which was the old 4-month default)
    let deadline = resp["deadline"].as_str().expect("deadline");
    assert_eq!(
        deadline, "2026-10-11",
        "#99 neg-B: deadline must be 2026-10-11 (2 months), not 2026-12-11 (4 months)"
    );

    drop(server);
}

/// #99: valid_types in deadline-table response must match the valid types
/// used by deadline_spec (single source of truth).
#[test]
fn issue_99_deadline_table_lists_valid_types() {
    let port = free_port();
    let server = spawn_test_server(port, &[]);
    let client = reqwest::blocking::Client::new();

    let resp: serde_json::Value = client
        .get(format!("http://127.0.0.1:{}/api/oa/deadline-table", port))
        .send()
        .expect("request must succeed")
        .json()
        .expect("json");

    // Must have valid_types array
    let valid_types = resp["valid_types"].as_array().expect("valid_types array");
    assert!(
        valid_types.len() == 7,
        "#99: deadline-table must list 7 valid types, got {}",
        valid_types.len()
    );

    // Each valid type must be in the deadline_table
    let table = resp["deadline_table"]
        .as_object()
        .expect("deadline_table object");
    for vt in valid_types {
        let name = vt.as_str().expect("type name");
        assert!(
            table.contains_key(name),
            "#99: type '{}' must be in deadline_table",
            name
        );
    }

    drop(server);
}
