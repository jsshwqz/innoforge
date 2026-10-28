//! Issue #77: SQLite busy_timeout PRAGMA + orchestrator snapshot writes offloaded via spawn_blocking
//!
//! Two concerns:
//! 1. Database::init must set PRAGMA busy_timeout so lock contention retries
//!    instead of immediately returning SQLITE_BUSY.
//! 2. Orchestrator snapshot writes must go through spawn_blocking so they
//!    don't stall the async runtime worker thread.

use innoforge::db::Database;
use innoforge::orchestrator;
use innoforge::patent::Idea;
use innoforge::pipeline::context::PipelineContext;
use innoforge::pipeline::state::PipelineStep;

/// Helper: insert a minimal idea so FK constraints on idea_versions / findings are satisfied.
fn create_test_idea(db: &Database, idea_id: &str) {
    let idea = Idea {
        id: idea_id.to_string(),
        title: "test".to_string(),
        description: "test".to_string(),
        input_type: "text".to_string(),
        status: "pending".to_string(),
        analysis: String::new(),
        web_results: "[]".to_string(),
        patent_results: "[]".to_string(),
        novelty_score: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        discussion_summary: String::new(),
    };
    db.insert_idea(&idea).unwrap();
}

/// Verify that Database::init sets busy_timeout to 5000ms.
/// Without this PRAGMA, SQLite returns SQLITE_BUSY immediately on lock
/// contention instead of retrying.
#[test]
fn issue_77_busy_timeout_pragma_is_set() {
    let db = Database::init(":memory:").unwrap();
    let timeout = db
        .query_busy_timeout()
        .expect("PRAGMA busy_timeout must be queryable");
    assert_eq!(
        timeout, 5000,
        "busy_timeout must be 5000ms (set in Database::init)"
    );
}

/// Verify that the async snapshot wrappers complete without panicking.
/// This exercises the spawn_blocking offload path end-to-end.
#[tokio::test]
async fn issue_77_async_snapshot_wrappers_complete() {
    let db = std::sync::Arc::new(Database::init(":memory:").unwrap());
    create_test_idea(&db, "test-idea-77");

    // Create a minimal pipeline context
    let ctx = PipelineContext::new("test-idea-77", "test idea", "test description");

    // 1. save_pipeline_snapshot_blocking
    let context_json = serde_json::to_string(&ctx).unwrap_or_default();
    orchestrator::save_pipeline_snapshot_blocking(
        db.clone(),
        ctx.idea_id.clone(),
        context_json,
        "ParseInput".to_string(),
    )
    .await;

    // Verify the snapshot was actually written
    let loaded = db.load_pipeline_snapshot(&ctx.idea_id).unwrap();
    assert!(
        loaded.is_some(),
        "snapshot must be persisted after save_pipeline_snapshot_blocking"
    );

    // 2. upsert_research_state_blocking
    orchestrator::upsert_research_state_blocking(
        db.clone(),
        ctx.idea_id.clone(),
        ctx.research_state.clone(),
    )
    .await;

    // 3. save_version_snapshot_blocking
    orchestrator::save_version_snapshot_blocking(db.clone(), ctx.clone(), PipelineStep::ParseInput)
        .await;

    // Verify version was written
    let versions = db.get_idea_versions(&ctx.idea_id).unwrap();
    assert!(
        !versions.is_empty(),
        "version snapshot must be persisted after save_version_snapshot_blocking"
    );

    // 4. record_failure_blocking
    orchestrator::record_failure_blocking(
        db.clone(),
        ctx.clone(),
        PipelineStep::SearchWeb,
        "test failure".to_string(),
    )
    .await;

    // Verify finding was written
    let findings = db.get_findings_by_idea(&ctx.idea_id).unwrap();
    assert!(
        !findings.is_empty(),
        "finding must be persisted after record_failure_blocking"
    );

    // 5. delete_pipeline_snapshot_blocking
    orchestrator::delete_pipeline_snapshot_blocking(db.clone(), ctx.idea_id.clone()).await;

    // Verify snapshot was deleted
    let gone = db.load_pipeline_snapshot(&ctx.idea_id).unwrap();
    assert!(
        gone.is_none(),
        "snapshot must be deleted after delete_pipeline_snapshot_blocking"
    );
}

/// Verify that the old per-call busy_timeout in purge_idea was removed.
/// If it were still present, it would override the global 5000ms to 3000ms
/// on every subsequent call, silently degrading lock contention behavior.
#[test]
fn issue_77_no_per_call_busy_timeout_override() {
    let db = Database::init(":memory:").unwrap();
    create_test_idea(&db, "purge-test-77");

    // Call purge_idea — if it still had busy_timeout(3s), the global
    // setting would be overridden to 3000ms afterward.
    db.purge_idea("purge-test-77").unwrap();

    // Verify busy_timeout is still 5000ms after purge_idea
    let timeout = db
        .query_busy_timeout()
        .expect("PRAGMA busy_timeout must be queryable");
    assert_eq!(
        timeout, 5000,
        "busy_timeout must remain 5000ms after purge_idea (no per-call override)"
    );
}
