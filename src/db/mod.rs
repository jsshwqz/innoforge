//! 数据库操作层 / Database Operations
//!
//! 基于 SQLite + FTS5 的本地数据持久化，支持全文搜索、创意管理、收藏夹、标签等。
//! Local data persistence with SQLite + FTS5, supporting full-text search, ideas, collections, tags.

mod cad;
mod chat;
mod collection;
mod cost;
mod evidence;
mod idea;
pub mod memory;
pub(crate) mod migrations;
pub mod oa;
mod patent;
pub mod rag;
pub(crate) mod relevance;
mod research_state;
mod settings;
#[cfg(test)]
mod tests;
pub mod vector;
pub mod version;
pub use oa::OaDiscussion;

use anyhow::Result;
use rusqlite::Connection;
use std::sync::Mutex;

/// 数据库实例 / Database instance
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// 获取数据库连接，自动恢复被污染的互斥锁
    pub(crate) fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("Database mutex was poisoned, recovering");
            poisoned.into_inner()
        })
    }

    /// Current schema version. Increment when adding migrations.
    pub const SCHEMA_VERSION: i32 = 24;

    pub fn init(path: &str) -> Result<Self> {
        let conn = match Self::try_open_valid(path) {
            Ok(c) => c,
            Err(e) => {
                // Database file is corrupt (e.g. not a SQLite database, code 26).
                // Isolate it so the user is not locked out, then create a fresh db.
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let corrupt_path = format!("{}.corrupt-{}", path, timestamp);
                tracing::error!(
                    "Database at '{}' is corrupt ({}). Isolating to '{}'.",
                    path,
                    e,
                    corrupt_path
                );
                std::fs::rename(path, &corrupt_path)
                    .map_err(|re| anyhow::anyhow!("Failed to isolate corrupt db: {}", re))?;
                Connection::open(path)?
            }
        };
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);",
        )?;
        let current_version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        migrations::run(&conn, current_version, Self::SCHEMA_VERSION)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Try to open a database and verify it is a valid SQLite file.
    ///
    /// `Connection::open` is lazy — it succeeds even on a non-SQLite file.
    /// The error surfaces on the first read.  We probe with `SELECT count(*)
    /// FROM sqlite_master` which fails with "file is not a database" (code 26)
    /// if the file content is not a valid SQLite database.
    fn try_open_valid(path: &str) -> Result<Connection> {
        let conn = Connection::open(path)?;
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
            r.get::<_, i64>(0)
        })?;
        Ok(conn)
    }

    /// Read current schema version (for testing/diagnostics).
    #[allow(dead_code)]
    pub fn query_schema_version(&self) -> Result<i32> {
        let c = self.conn();
        let v: i32 = c.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )?;
        Ok(v)
    }
}
