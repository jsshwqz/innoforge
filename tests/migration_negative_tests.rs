//! Negative verification tests for issue #13 (migration atomicity) and
//! issue #19 (migration file validation).
//!
//! ## Issue #13 — Migration atomicity
//!
//! Each migration is wrapped in `run_migration_tx()` which uses
//! `conn.unchecked_transaction()` → DDL → version update → `commit()`.
//! If the process is killed mid-migration, the entire transaction rolls back.
//! Additionally, `column_exists()` guards make ALTER TABLE idempotent.
//!
//! **Negative verification cycle (red → green):**
//! 1. [green] Run these tests on current code → all pass.
//! 2. [red]  Remove the `column_exists` guards in `run_migration_tx` callers
//!    (replace `if !column_exists(...)` with unconditional ALTER TABLE),
//!    then set schema_version back to a lower value on a fully-migrated
//!    db → `Db::init()` fails with "duplicate column name".
//! 3. [green] Restore the guards → tests pass again.
//!
//! ## Issue #19 — Migration file validation
//!
//! `auto_migrate_db()` checks the SQLite magic header before copying.
//! `Db::init()` isolates corrupt databases to `*.corrupt-<timestamp>`.
//!
//! **Negative verification cycle (red → green):**
//! 1. [green] Run these tests on current code → all pass.
//! 2. [red]  Bypass `is_sqlite_file()` in `auto_migrate_db()` (remove the
//!    check) → garbage file gets copied → test fails.
//! 3. [green] Restore the check → tests pass again.

use rusqlite::Connection;
use std::io::Write;
use tempfile::TempDir;

/// SQLite magic header: first 16 bytes of any valid SQLite database file.
const SQLITE_MAGIC: [u8; 16] = *b"SQLite format 3\0";

// =========================================================================
// Issue #13: Migration atomicity
// =========================================================================

#[test]
fn migration_atomicity_full_migration_reaches_target_version() {
    // Positive assertion: a fresh db initializes to SCHEMA_VERSION (24).
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let db = innoforge::db::Database::init(db_path.to_str().unwrap())
        .expect("init should succeed on fresh db");
    let version = db.query_schema_version().expect("query version");
    assert_eq!(
        version,
        innoforge::db::Database::SCHEMA_VERSION,
        "fresh db should be at target schema version"
    );
}

#[test]
fn migration_atomicity_interrupted_migration_self_heals() {
    // Simulate an interrupted migration: the db is fully migrated (all tables/
    // columns exist) but schema_version was set back to a lower value (as if
    // the process was killed between DDL and version update).
    //
    // With idempotency guards (column_exists checks), re-running migrations
    // should succeed — the ALTER TABLE statements are skipped because the
    // columns already exist. This is the self-healing behavior required by #13.

    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Step 1: Fully migrate the database.
    {
        let db = innoforge::db::Database::init(db_path.to_str().unwrap())
            .expect("init should succeed on fresh db");
        assert_eq!(
            db.query_schema_version().unwrap(),
            innoforge::db::Database::SCHEMA_VERSION,
            "db should be fully migrated"
        );
    }

    // Step 2: Roll back schema_version to simulate interrupted migration.
    // This is exactly the scenario described in issue #13: tables/columns
    // exist but version number is stale.
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch("DELETE FROM schema_version;")
            .expect("clear version");
        // Set to version 8 — before the feature_cards 5-dim columns migration (v9).
        conn.execute_batch("INSERT INTO schema_version (version) VALUES (8);")
            .expect("set stale version");
    }

    // Step 3: Re-init — should self-heal, not crash with "duplicate column name".
    let db = innoforge::db::Database::init(db_path.to_str().unwrap())
        .expect("re-init should self-heal after interrupted migration");
    let version = db.query_schema_version().expect("query version");
    assert_eq!(
        version,
        innoforge::db::Database::SCHEMA_VERSION,
        "self-healed db should be at target schema version"
    );
}

#[test]
fn migration_atomicity_rolled_back_migration_leaves_no_partial_state() {
    // Verify that a failed migration (invalid SQL) does not leave partial
    // state. This test calls the PRODUCTION run_migration_tx() directly —
    // if that function is non-atomic (e.g. plain conn.execute_batch without
    // a transaction), the CREATE TABLE succeeds and is left behind even
    // though the overall migration fails, causing this test to FAIL.
    //
    // With proper transaction wrapping (unchecked_transaction + commit),
    // the entire migration rolls back on error: no partial table, no
    // version update.

    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test_tx.db");
    let conn = Connection::open(&db_path).expect("open db");

    // Set up schema_version table (required by run_migration_tx).
    conn.execute_batch("CREATE TABLE schema_version (version INTEGER NOT NULL);")
        .expect("create schema_version");
    conn.execute_batch("INSERT INTO schema_version (version) VALUES (0);")
        .expect("insert version 0");

    // Call the production function with SQL that has a deliberate error.
    // The CREATE TABLE would succeed on its own, but the invalid SQL after
    // it should cause the entire migration to fail and roll back.
    let result = innoforge::db::migrations::run_migration_tx(
        &conn,
        1,
        "CREATE TABLE test_partial (id INTEGER PRIMARY KEY); THIS IS NOT VALID SQL;",
    );

    // The migration must fail.
    assert!(
        result.is_err(),
        "run_migration_tx with invalid SQL must return Err, got: {:?}",
        result
    );

    // Verify: the table should NOT exist (transaction rolled back).
    // If run_migration_tx is non-atomic, the CREATE TABLE will have been
    // committed and this assertion will FAIL — that's the detection.
    let table_exists: bool = conn
        .query_row(
            "SELECT count(*) > 0 FROM sqlite_master WHERE type='table' AND name='test_partial'",
            [],
            |r| r.get(0),
        )
        .expect("query table existence");
    assert!(
        !table_exists,
        "partial table should not exist after rolled-back migration — \
         if this fails, run_migration_tx is not using a transaction"
    );

    // Verify: schema_version should still be 0 (not updated).
    let version: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )
        .expect("query version");
    assert_eq!(
        version, 0,
        "schema_version should be unchanged after rolled-back migration"
    );
}

#[test]
fn migration_atomicity_multiple_stale_versions_all_self_heal() {
    // Test self-healing from several different stale versions, not just one.
    // This covers migrations that use column_exists guards (v3, v4, v9, v15, v17, v23).
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test_multi.db");

    // Fully migrate first.
    {
        let _db =
            innoforge::db::Database::init(db_path.to_str().unwrap()).expect("init should succeed");
    }

    // Test self-healing from several stale versions.
    for stale_version in [1, 5, 8, 12, 15, 20] {
        // Reset to stale version.
        {
            let conn = Connection::open(&db_path).expect("open db");
            conn.execute_batch("DELETE FROM schema_version;")
                .expect("clear version");
            conn.execute_batch(&format!(
                "INSERT INTO schema_version (version) VALUES ({});",
                stale_version
            ))
            .expect("set stale version");
        }

        // Re-init should self-heal.
        let db = innoforge::db::Database::init(db_path.to_str().unwrap()).unwrap_or_else(|e| {
            panic!(
                "re-init from stale version {} should self-heal, but failed: {}",
                stale_version, e
            )
        });
        let version = db.query_schema_version().expect("query version");
        assert_eq!(
            version,
            innoforge::db::Database::SCHEMA_VERSION,
            "self-healed db from stale version {} should be at target version",
            stale_version
        );
    }
}

#[test]
fn migration_atomicity_idempotent_when_columns_exist_but_version_unrecorded() {
    // Issue #13 idempotency: all columns exist (fully migrated) but
    // schema_version has no record (e.g. the DELETE succeeded but the
    // INSERT was lost due to a crash). Re-running migrations must succeed
    // without "duplicate column name" errors, thanks to column_exists guards.

    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test_idem.db");

    // Step 1: Fully migrate the database.
    {
        let db = innoforge::db::Database::init(db_path.to_str().unwrap())
            .expect("init should succeed on fresh db");
        assert_eq!(
            db.query_schema_version().unwrap(),
            innoforge::db::Database::SCHEMA_VERSION,
            "db should be fully migrated"
        );
    }

    // Step 2: Delete ALL schema_version records — simulate the case where
    // columns exist but the version was never recorded.
    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch("DELETE FROM schema_version;")
            .expect("delete all version records");
    }

    // Step 3: Re-init must succeed (idempotent), not fail with "duplicate column".
    let db = innoforge::db::Database::init(db_path.to_str().unwrap()).unwrap_or_else(|e| {
        panic!(
            "re-init with unrecorded version should succeed idempotently, but failed: {}",
            e
        )
    });
    let version = db.query_schema_version().expect("query version");
    assert_eq!(
        version,
        innoforge::db::Database::SCHEMA_VERSION,
        "idempotent re-init should reach target schema version"
    );
}

// =========================================================================
// Issue #19: Migration file validation
// =========================================================================

#[test]
fn migration_validation_non_sqlite_file_not_copied() {
    // A non-SQLite file named innoforge.db in cwd should NOT be copied to
    // the new location. auto_migrate_db should return Ok (so fresh db is created).

    let dir = TempDir::new().expect("temp dir");
    let old_path = dir.path().join("innoforge.db");
    let new_path = dir.path().join("platform_dir").join("innoforge.db");
    std::fs::create_dir_all(new_path.parent().unwrap()).expect("create platform dir");

    // Write a non-SQLite file (garbage content).
    {
        let mut f = std::fs::File::create(&old_path).expect("create garbage file");
        f.write_all(b"THIS IS NOT A SQLITE DATABASE FILE")
            .expect("write garbage");
    }

    // Change to the temp dir so auto_migrate_db can find "innoforge.db" in cwd.
    let original_cwd = std::env::current_dir().expect("get cwd");
    std::env::set_current_dir(dir.path()).expect("set cwd");

    let result = innoforge::common::auto_migrate_db(&new_path);

    // Restore cwd.
    std::env::set_current_dir(&original_cwd).expect("restore cwd");

    // auto_migrate_db should return Ok (skip migration, let fresh db be created).
    assert!(
        result.is_ok(),
        "auto_migrate_db should return Ok for non-SQLite source, got: {:?}",
        result
    );

    // The new file should NOT exist (garbage was not copied).
    assert!(
        !new_path.exists(),
        "non-SQLite file should NOT be copied to new location"
    );

    // The old garbage file should still exist (non-destructive).
    assert!(
        old_path.exists(),
        "original garbage file should still exist (non-destructive)"
    );
}

#[test]
fn migration_validation_valid_sqlite_file_is_copied() {
    // A valid SQLite file should be migrated (copied) to the new location.
    // This is the positive regression test.

    let dir = TempDir::new().expect("temp dir");
    let old_path = dir.path().join("innoforge.db");
    let new_path = dir.path().join("platform_dir").join("innoforge.db");
    std::fs::create_dir_all(new_path.parent().unwrap()).expect("create platform dir");

    // Create a valid SQLite database file.
    {
        let conn = Connection::open(&old_path).expect("create sqlite db");
        conn.execute_batch("CREATE TABLE test (id INTEGER PRIMARY KEY);")
            .expect("create table");
        conn.execute_batch("INSERT INTO test (id) VALUES (42);")
            .expect("insert data");
    }

    // Verify it has the SQLite magic header.
    assert!(
        innoforge::common::is_sqlite_file(&old_path),
        "valid SQLite file should pass is_sqlite_file check"
    );

    let original_cwd = std::env::current_dir().expect("get cwd");
    std::env::set_current_dir(dir.path()).expect("set cwd");

    let result = innoforge::common::auto_migrate_db(&new_path);

    std::env::set_current_dir(&original_cwd).expect("restore cwd");

    assert!(
        result.is_ok(),
        "auto_migrate_db should succeed for valid SQLite source, got: {:?}",
        result
    );

    // The new file should exist (valid db was copied).
    assert!(
        new_path.exists(),
        "valid SQLite file should be copied to new location"
    );

    // Verify the copied data is intact.
    {
        let conn = Connection::open(&new_path).expect("open migrated db");
        let id: i32 = conn
            .query_row("SELECT id FROM test WHERE id = 42", [], |r| r.get(0))
            .expect("query data");
        assert_eq!(id, 42, "migrated data should be intact");
    }
}

#[test]
fn migration_validation_is_sqlite_file_rejects_garbage() {
    // Direct test of is_sqlite_file: garbage content → false.
    let dir = TempDir::new().expect("temp dir");
    let garbage_path = dir.path().join("garbage.db");

    {
        let mut f = std::fs::File::create(&garbage_path).expect("create file");
        f.write_all(b"NOT SQLITE").expect("write garbage");
    }

    assert!(
        !innoforge::common::is_sqlite_file(&garbage_path),
        "garbage file should fail is_sqlite_file check"
    );
}

#[test]
fn migration_validation_is_sqlite_file_accepts_valid_header() {
    // Direct test of is_sqlite_file: valid SQLite header → true.
    let dir = TempDir::new().expect("temp dir");
    let sqlite_path = dir.path().join("valid.db");

    {
        let conn = Connection::open(&sqlite_path).expect("create sqlite db");
        conn.execute_batch("CREATE TABLE x (id INTEGER);")
            .expect("create table");
    }

    assert!(
        innoforge::common::is_sqlite_file(&sqlite_path),
        "valid SQLite file should pass is_sqlite_file check"
    );

    // Also verify the magic header matches what we expect.
    let mut file = std::fs::File::open(&sqlite_path).expect("open file");
    let mut header = [0u8; 16];
    std::io::Read::read_exact(&mut file, &mut header).expect("read header");
    assert_eq!(
        header, SQLITE_MAGIC,
        "SQLite file header should match magic bytes"
    );
}

#[test]
fn migration_validation_empty_file_rejected() {
    // An empty file (0 bytes) should not be treated as a SQLite database.
    // This covers the "previous crash left 0-byte file" scenario from #19.
    let dir = TempDir::new().expect("temp dir");
    let empty_path = dir.path().join("innoforge.db");
    let new_path = dir.path().join("platform_dir").join("innoforge.db");
    std::fs::create_dir_all(new_path.parent().unwrap()).expect("create platform dir");

    // Create an empty file.
    std::fs::File::create(&empty_path).expect("create empty file");

    assert!(
        !innoforge::common::is_sqlite_file(&empty_path),
        "empty file should fail is_sqlite_file check"
    );

    let original_cwd = std::env::current_dir().expect("get cwd");
    std::env::set_current_dir(dir.path()).expect("set cwd");

    let result = innoforge::common::auto_migrate_db(&new_path);

    std::env::set_current_dir(&original_cwd).expect("restore cwd");

    assert!(
        result.is_ok(),
        "auto_migrate_db should return Ok for empty source file"
    );
    assert!(
        !new_path.exists(),
        "empty file should NOT be copied to new location"
    );
}

#[test]
fn migration_validation_corrupt_db_is_isolated() {
    // If the platform directory already has a corrupt (non-SQLite) db file,
    // Db::init() should isolate it (rename to *.corrupt-<timestamp>) and
    // create a fresh db. This is the self-healing path in try_open_valid().

    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("corrupt.db");

    // Write a file with invalid content (not SQLite).
    {
        let mut f = std::fs::File::create(&db_path).expect("create corrupt file");
        f.write_all(b"CORRUPT DATABASE CONTENT NOT SQLITE")
            .expect("write corrupt");
    }

    // Db::init should succeed by isolating the corrupt file.
    let db = innoforge::db::Database::init(db_path.to_str().unwrap())
        .expect("init should succeed by isolating corrupt db");

    // The db should be at target version (fresh db created after isolation).
    let version = db.query_schema_version().expect("query version");
    assert_eq!(
        version,
        innoforge::db::Database::SCHEMA_VERSION,
        "fresh db (after isolating corrupt file) should be at target version"
    );

    // The corrupt file should have been renamed (isolated).
    // Look for a file matching the pattern corrupt.db.corrupt-*
    let mut found_isolated = false;
    for entry in std::fs::read_dir(dir.path()).expect("read dir") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with("corrupt.db.corrupt-") {
            found_isolated = true;
            // Verify the isolated file still has the corrupt content.
            let content = std::fs::read(entry.path()).expect("read isolated file");
            assert!(
                content.starts_with(b"CORRUPT DATABASE"),
                "isolated file should contain original corrupt content"
            );
        }
    }
    assert!(
        found_isolated,
        "corrupt db should have been isolated to a .corrupt-* file"
    );
}
