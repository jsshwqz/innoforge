//! Issue #88: /api/oa/history/detail/:id must return a single record by id,
//! not the full list.
//!
//! Positive: requesting an existing id returns only that record;
//!           requesting a non-existent id returns a not-found result
//!           whose body does NOT contain the full analyses list.
//! Negative A: if the route still pointed at api_oa_history_all,
//!             the existing-id test would fail (returns all, not one).
//! Negative B: if the handler returned the full list for unknown ids,
//!             the non-existent-id test would fail (body contains "analyses" array).

mod common;

use common::{free_port, spawn_test_server};
use rusqlite::params;

/// Insert a test OA analysis record directly into the server's SQLite DB.
/// Returns the auto-incremented row id.
fn insert_test_oa_analysis(db_path: &str) -> i64 {
    let conn = rusqlite::Connection::open(db_path).expect("open db");
    conn.execute(
        "INSERT INTO oa_analyses \
         (patent_number, patent_title, oa_type, depth, analysis_text, created_at, version) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "TEST-PATENT-001",
            "Test Patent Title",
            "first",
            "standard",
            "Test analysis text content",
            "2026-10-23T00:00:00Z",
            1,
        ],
    )
    .expect("insert oa_analyses");
    conn.last_insert_rowid()
}

#[test]
fn detail_returns_single_record_for_existing_id() {
    let server_port = free_port();
    let ai_port = free_port();
    let ai_url = format!("http://127.0.0.1:{}", ai_port);

    let server = spawn_test_server(
        server_port,
        &[
            ("AI_BASE_URL", &ai_url),
            ("AI_API_KEY", "test-key-not-real"),
        ],
    );

    // Insert a record directly into the server's DB
    let db_path = server
        .home
        .as_ref()
        .unwrap()
        .path()
        .join("innoforge.db")
        .to_str()
        .unwrap()
        .to_string();
    let record_id = insert_test_oa_analysis(&db_path);

    // Request the detail endpoint for the existing id
    let url = format!(
        "http://127.0.0.1:{}/api/oa/history/detail/{}",
        server_port, record_id
    );
    let resp = reqwest::blocking::get(&url).expect("GET detail");
    assert!(
        resp.status().is_success(),
        "detail endpoint should return 2xx for existing id"
    );
    let body: serde_json::Value = serde_json::from_str(&resp.text().unwrap()).unwrap();

    // Must return status ok with a single "analysis" object
    assert_eq!(
        body["status"].as_str(),
        Some("ok"),
        "should return status ok for existing id"
    );
    let analysis = &body["analysis"];
    assert!(
        analysis.is_object(),
        "response must contain a single 'analysis' object, got: {}",
        analysis
    );
    assert_eq!(
        analysis["id"].as_i64(),
        Some(record_id),
        "returned record id must match requested id"
    );
    assert_eq!(
        analysis["patent_number"].as_str(),
        Some("TEST-PATENT-001"),
        "returned patent_number must match"
    );

    // Must NOT contain the full-list "analyses" array
    assert!(
        body["analyses"].is_null(),
        "response must not contain 'analyses' array (that would mean it returned the full list)"
    );
}

#[test]
fn detail_returns_not_found_for_nonexistent_id() {
    let server_port = free_port();
    let ai_port = free_port();
    let ai_url = format!("http://127.0.0.1:{}", ai_port);

    let server = spawn_test_server(
        server_port,
        &[
            ("AI_BASE_URL", &ai_url),
            ("AI_API_KEY", "test-key-not-real"),
        ],
    );

    // Insert one record so the DB is not empty — the negative test must still
    // NOT return this record (or any list) for a non-existent id.
    let db_path = server
        .home
        .as_ref()
        .unwrap()
        .path()
        .join("innoforge.db")
        .to_str()
        .unwrap()
        .to_string();
    let _existing_id = insert_test_oa_analysis(&db_path);

    // Request a non-existent id
    let nonexistent_id = 999_999_999_i64;
    let url = format!(
        "http://127.0.0.1:{}/api/oa/history/detail/{}",
        server_port, nonexistent_id
    );
    let resp = reqwest::blocking::get(&url).expect("GET detail nonexistent");
    let body_text = resp.text().unwrap();
    let body: serde_json::Value = serde_json::from_str(&body_text).unwrap();

    // Must return an error status, not ok
    assert_eq!(
        body["status"].as_str(),
        Some("error"),
        "non-existent id must return status error, not ok"
    );

    // Must NOT contain the full-list "analyses" array — this is the core
    // regression guard: the old handler returned ALL records regardless of id.
    assert!(
        body["analyses"].is_null(),
        "non-existent id response must NOT contain 'analyses' array \
         (old bug: returned full list for any id)"
    );

    // Must NOT contain a single "analysis" object either
    assert!(
        body["analysis"].is_null(),
        "non-existent id response must NOT contain 'analysis' object"
    );
}
