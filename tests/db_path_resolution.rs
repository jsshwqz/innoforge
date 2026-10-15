//! Database path resolution integration tests (issue #14).
//!
//! All tests that touch process-global state (env vars, cwd) are serialized
//! via a single Mutex to prevent cross-test pollution.  No test uses
//! conditional assertions — every branch unconditionally asserts.

use innoforge::common::{auto_migrate_db, platform_data_dir, resolve_db_path};
use std::sync::Mutex;

/// Serializes all tests in this file that touch env vars or cwd.
/// Recovers from poison so one test's panic doesn't cascade.
static TEST_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Save and restore an env var around a closure.
fn with_env_var<F: FnOnce()>(key: &str, value: Option<&str>, f: F) {
    let orig = std::env::var_os(key);
    match value {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
    f();
    match orig {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
}

/// RAII guard that restores cwd on drop — even if the test panics.
struct CwdGuard {
    orig: std::path::PathBuf,
}

impl CwdGuard {
    fn enter(dir: &std::path::Path) -> Self {
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir).unwrap();
        CwdGuard { orig }
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.orig);
    }
}

#[test]
fn innoforge_db_overrides_innoforge_home_and_platform() {
    let _lock = lock();

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let tmp_home = tempfile::tempdir().unwrap();

    with_env_var(
        "INNOFORGE_DB",
        Some(tmp_db.path().to_str().unwrap()),
        || {
            with_env_var(
                "INNOFORGE_HOME",
                Some(tmp_home.path().to_str().unwrap()),
                || {
                    let resolved = resolve_db_path().unwrap();
                    assert_eq!(
                        resolved,
                        tmp_db.path(),
                        "INNOFORGE_DB must take priority over INNOFORGE_HOME"
                    );
                },
            );
        },
    );
}

#[test]
fn innoforge_home_used_when_db_not_set() {
    let _lock = lock();

    let tmp_dir = tempfile::tempdir().unwrap();

    with_env_var("INNOFORGE_DB", None, || {
        with_env_var(
            "INNOFORGE_HOME",
            Some(tmp_dir.path().to_str().unwrap()),
            || {
                let resolved = resolve_db_path().unwrap();
                assert_eq!(
                    resolved,
                    tmp_dir.path().join("innoforge.db"),
                    "INNOFORGE_HOME should be used when INNOFORGE_DB is not set"
                );
            },
        );
    });
}

#[test]
fn platform_data_dir_used_when_no_env_vars() {
    let _lock = lock();

    let tmp_home = tempfile::tempdir().unwrap();

    with_env_var("INNOFORGE_DB", None, || {
        with_env_var("INNOFORGE_HOME", None, || {
            with_env_var("HOME", Some(tmp_home.path().to_str().unwrap()), || {
                let resolved = resolve_db_path().unwrap();
                let expected = tmp_home
                    .path()
                    .join(".local")
                    .join("share")
                    .join("innoforge")
                    .join("innoforge.db");
                assert_eq!(
                    resolved, expected,
                    "without env vars, db should be in platform data dir under HOME"
                );
            });
        });
    });
}

#[test]
fn platform_data_dir_is_absolute_and_contains_innoforge() {
    let dir = platform_data_dir();
    assert!(
        dir.is_absolute(),
        "platform_data_dir must be absolute, got {}",
        dir.display()
    );
    assert!(
        dir.to_string_lossy().contains("innoforge"),
        "platform_data_dir must contain 'innoforge': {}",
        dir.display()
    );
}

/// Create a minimal valid SQLite database file at the given path.
fn create_valid_sqlite_file(path: &std::path::Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch("CREATE TABLE IF NOT EXISTS t(x);")
        .unwrap();
    conn.close().unwrap();
}

#[test]
fn auto_migrate_copies_valid_sqlite_from_cwd_to_new_location() {
    let _lock = lock();

    let tmp_cwd = tempfile::tempdir().unwrap();
    let _cwd = CwdGuard::enter(tmp_cwd.path());

    // Create a real SQLite db in cwd
    create_valid_sqlite_file(std::path::Path::new("innoforge.db"));
    let old_content = std::fs::read("innoforge.db").unwrap();

    let tmp_target = tempfile::tempdir().unwrap();
    let new_path = tmp_target.path().join("innoforge.db");

    auto_migrate_db(&new_path).expect("migration should succeed for valid SQLite");

    assert!(new_path.exists(), "new db must exist after migration");
    let copied = std::fs::read(&new_path).unwrap();
    assert_eq!(copied, old_content, "copied content must match original");
    assert!(
        std::path::Path::new("innoforge.db").exists(),
        "old db must still exist after non-destructive migration"
    );
}

#[test]
fn auto_migrate_skips_non_sqlite_file_in_cwd() {
    let _lock = lock();

    let tmp_cwd = tempfile::tempdir().unwrap();
    let _cwd = CwdGuard::enter(tmp_cwd.path());

    // Write a non-SQLite file (plain text) as innoforge.db in cwd
    std::fs::write("innoforge.db", b"this is not a sqlite database").unwrap();

    let tmp_target = tempfile::tempdir().unwrap();
    let new_path = tmp_target.path().join("innoforge.db");

    auto_migrate_db(&new_path).expect("should skip non-SQLite file gracefully");

    // The garbage file must NOT be copied to the new location
    assert!(
        !new_path.exists(),
        "non-SQLite file must not be copied to new location"
    );
    // The original garbage file must still exist (non-destructive)
    assert!(
        std::path::Path::new("innoforge.db").exists(),
        "original file should still exist"
    );
}

#[test]
fn auto_migrate_skips_zero_byte_file_in_cwd() {
    let _lock = lock();

    let tmp_cwd = tempfile::tempdir().unwrap();
    let _cwd = CwdGuard::enter(tmp_cwd.path());

    // Write a 0-byte file as innoforge.db in cwd
    std::fs::write("innoforge.db", b"").unwrap();

    let tmp_target = tempfile::tempdir().unwrap();
    let new_path = tmp_target.path().join("innoforge.db");

    auto_migrate_db(&new_path).expect("should skip 0-byte file gracefully");

    assert!(
        !new_path.exists(),
        "0-byte file must not be copied to new location"
    );
}

#[test]
fn auto_migrate_noop_when_new_path_already_exists() {
    let _lock = lock();

    let tmp_cwd = tempfile::tempdir().unwrap();
    let _cwd = CwdGuard::enter(tmp_cwd.path());

    std::fs::write("innoforge.db", b"old content").unwrap();

    let tmp_target = tempfile::tempdir().unwrap();
    let new_path = tmp_target.path().join("innoforge.db");
    std::fs::write(&new_path, b"existing content").unwrap();

    auto_migrate_db(&new_path).expect("should be a no-op when new path exists");

    let content = std::fs::read_to_string(&new_path).unwrap();
    assert_eq!(
        content, "existing content",
        "existing db must not be overwritten"
    );
}

#[test]
fn auto_migrate_errors_when_write_fails() {
    let _lock = lock();

    let tmp_cwd = tempfile::tempdir().unwrap();
    let _cwd = CwdGuard::enter(tmp_cwd.path());

    create_valid_sqlite_file(std::path::Path::new("innoforge.db"));

    // Target path whose parent directory does NOT exist.
    // std::fs::copy will fail with "No such file or directory" regardless
    // of process privileges — works even as root.
    let tmp_target = tempfile::tempdir().unwrap();
    let new_path = tmp_target
        .path()
        .join("nonexistent_subdir")
        .join("innoforge.db");

    let result = auto_migrate_db(&new_path);

    // Must return an error — NOT silently succeed with an empty db
    assert!(
        result.is_err(),
        "auto_migrate_db must error when write fails, got Ok"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Failed to migrate"),
        "error message should mention migration failure: {}",
        err_msg
    );
}

/// Test that Database::init isolates a corrupt db file and creates a fresh one.
/// This tests issue #19 requirement 2: platform dir has pre-placed bad file
/// => startup succeeds and a *.corrupt-* isolation file appears.
#[test]
fn init_isolates_corrupt_db_and_creates_fresh() {
    use innoforge::db::Database;

    let tmp_dir = tempfile::tempdir().unwrap();
    let db_path = tmp_dir.path().join("innoforge.db");

    // Pre-place a corrupt (non-SQLite) file at the db location
    std::fs::write(&db_path, b"not a sqlite database file").unwrap();

    // init should succeed — it isolates the corrupt file and creates a fresh db
    let db = Database::init(db_path.to_str().unwrap())
        .expect("init should succeed even with corrupt db");

    // A corrupt-<timestamp> isolation file should exist
    let entries = std::fs::read_dir(tmp_dir.path()).unwrap();
    let mut found_corrupt = false;
    for entry in entries {
        let name = entry.unwrap().file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with("innoforge.db.corrupt-") {
            found_corrupt = true;
            break;
        }
    }
    assert!(
        found_corrupt,
        "a .corrupt-<timestamp> isolation file must exist"
    );

    // The fresh db should be usable
    let version = db
        .query_schema_version()
        .expect("fresh db should be queryable");
    assert_eq!(
        version,
        Database::SCHEMA_VERSION,
        "fresh db should be at current schema version"
    );
}

/// Test that Database::init works normally with a valid SQLite file.
#[test]
fn init_succeeds_with_valid_sqlite_file() {
    use innoforge::db::Database;

    let tmp_dir = tempfile::tempdir().unwrap();
    let db_path = tmp_dir.path().join("innoforge.db");

    // No pre-existing file — init should create a fresh db
    let db = Database::init(db_path.to_str().unwrap())
        .expect("init should succeed with no pre-existing file");

    let version = db.query_schema_version().expect("db should be queryable");
    assert_eq!(
        version,
        Database::SCHEMA_VERSION,
        "db should be at current schema version"
    );

    // No corrupt file should exist
    let entries = std::fs::read_dir(tmp_dir.path()).unwrap();
    for entry in entries {
        let name = entry.unwrap().file_name();
        let name_str = name.to_string_lossy();
        assert!(
            !name_str.contains(".corrupt-"),
            "no .corrupt file should exist for valid db, found: {}",
            name_str
        );
    }
}

/// Test that strip_verbatim_prefix removes the Windows \\?\ prefix.
#[test]
fn strip_verbatim_prefix_removes_long_path_prefix() {
    use innoforge::common::strip_verbatim_prefix;

    // Windows long path prefix
    let win_path = r"\\?\D:\TEMP\innoforge\innoforge.db";
    let stripped = strip_verbatim_prefix(win_path);
    assert_eq!(
        stripped, r"D:\TEMP\innoforge\innoforge.db",
        "verbatim prefix must be stripped"
    );

    // Normal path should be unchanged
    let normal = "/home/user/.local/share/innoforge/innoforge.db";
    assert_eq!(strip_verbatim_prefix(normal), normal);
}
