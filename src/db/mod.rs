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
pub mod migrations;
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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// 批量写库 RAII 守卫：持有期间标记 `Database::is_bulk_write_active() == true`，
/// 使在线搜索缓存主动避让写锁，避免并发 `SQLITE_BUSY` 拖慢检索响应。
pub(crate) struct BulkWriteGuard<'a>(&'a AtomicUsize);

impl Drop for BulkWriteGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}

/// 数据库实例 / Database instance
pub struct Database {
    conn: Mutex<Connection>,
    /// 主库文件路径（`:memory:` 内存库为 `None`），供独立 WAL 批量写入连接与磁盘守卫无锁读取。
    db_path: Option<std::path::PathBuf>,
    /// 正在进行的批量导库写任务计数（>0 时前台在线检索跳过同步落库缓存，零锁竞争）。
    bulk_write_depth: AtomicUsize,
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
    pub const SCHEMA_VERSION: i32 = 27;

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
        // busy_timeout：并发启动/并行读写时不要立刻 SQLITE_BUSY（#77）；
        // synchronous=NORMAL：WAL 下足以保证崩溃一致性，同时少一次 fsync；
        // journal_size_limit=32MB + wal_autocheckpoint=1000：硬限制 WAL 文件体积上限，杜绝 GB 级批量导入后 WAL 膨胀至数百 MB 拖慢读写与重启恢复；
        // cache_size=-16000 / temp_store=MEMORY：给覆盖索引与排序常驻 16MB 内存缓存，避免主表重复扫盘。
        conn.execute_batch(
            "PRAGMA busy_timeout=5000; PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA wal_autocheckpoint=1000; PRAGMA journal_size_limit=33554432; PRAGMA temp_store=MEMORY; PRAGMA cache_size=-16000;",
        )?;
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

        let db_path = {
            let trimmed = path.trim();
            if trimmed.is_empty() || trimmed == ":memory:" || trimmed.starts_with("file::memory:") {
                None
            } else {
                Some(std::path::PathBuf::from(trimmed))
            }
        };

        // 启动自愈：若上次异常退出或大规模导入留下 >32MB 未截断 WAL，启动时自动 TRUNCATE 回收磁盘并避免后续每读一次都扫数万 WAL 帧
        if let Some(ref p) = db_path {
            let wal_path = std::path::PathBuf::from(format!("{}-wal", p.display()));
            if let Ok(meta) = std::fs::metadata(&wal_path) {
                if meta.len() > 32 * 1024 * 1024 {
                    tracing::info!(
                        "Detected large leftover WAL ({:.1} MB), running startup TRUNCATE checkpoint...",
                        meta.len() as f64 / (1024.0 * 1024.0)
                    );
                    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
                }
            }
        }

        Ok(Self {
            conn: Mutex::new(conn),
            db_path,
            bulk_write_depth: AtomicUsize::new(0),
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

    /// Read current busy_timeout in milliseconds (for testing/diagnostics).
    #[allow(dead_code)]
    pub fn query_busy_timeout(&self) -> Result<i64> {
        let c = self.conn();
        let v: i64 = c.query_row("PRAGMA busy_timeout", [], |r| r.get(0))?;
        Ok(v)
    }

    /// 为大库高频只读查询（检索、IPC 聚合等）打开独立 SQLite WAL 只读连接：
    /// 在 WAL 模式下读者之间、读者与写者之间 100% 并发不互斥，彻底避免慢查询占死全局 `self.conn` 互斥锁卡死全站。
    /// 内存库（`:memory:`）返回 `None`，自动回退主连接。
    pub(crate) fn open_concurrent_wal_reader(&self) -> Option<Connection> {
        let path = self.db_path.as_deref()?;
        let flags =
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(path, flags).ok()?;
        let _ = conn.execute_batch(
            "PRAGMA query_only=ON; PRAGMA busy_timeout=15000; PRAGMA temp_store=MEMORY; PRAGMA cache_size=-16000;",
        );
        Some(conn)
    }

    /// 标记进入批量写库区间（返回 RAII 守卫，退出或 panic 时自动清除）。
    pub(crate) fn begin_bulk_write(&self) -> BulkWriteGuard<'_> {
        self.bulk_write_depth.fetch_add(1, Ordering::Acquire);
        BulkWriteGuard(&self.bulk_write_depth)
    }

    /// 当前是否有后台大批量专利导入正在占用 SQLite 写锁。
    pub fn is_bulk_write_active(&self) -> bool {
        self.bulk_write_depth.load(Ordering::Acquire) > 0
    }

    /// 在文件库上优先走独立 WAL 并发只读连接执行只读闭包，内存库自动回退 `self.conn()`。
    pub(crate) fn with_read_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        if let Some(conn) = self.open_concurrent_wal_reader() {
            f(&conn)
        } else {
            let c = self.conn();
            f(&c)
        }
    }

    /// 使用 SQLite 原生 `VACUUM INTO` 生成一致热备份快照文件，返回文件大小（字节）。
    pub fn vacuum_into_backup(&self, dest_path: &std::path::Path) -> Result<u64> {
        for comp in dest_path.components() {
            if matches!(comp, std::path::Component::ParentDir) {
                anyhow::bail!("备份路径禁止包含 '..'");
            }
        }
        if let Some(parent) = dest_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        if dest_path.exists() {
            std::fs::remove_file(dest_path)?;
        }
        let path_str = dest_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("备份路径包含非 UTF-8 字符"))?;
        {
            let c = self.conn();
            c.execute("VACUUM INTO ?1", rusqlite::params![path_str])?;
        }
        let meta = std::fs::metadata(dest_path)?;
        Ok(meta.len())
    }

    /// 对导出的备份快照执行敏感密钥脱敏（默认清空 API Key / Secret / Token 等敏感项）。
    pub fn scrub_backup_secrets(backup_path: &std::path::Path) -> Result<usize> {
        for comp in backup_path.components() {
            if matches!(comp, std::path::Component::ParentDir) {
                anyhow::bail!("备份路径禁止包含 '..'");
            }
        }
        let conn = rusqlite::Connection::open(backup_path)?;
        let affected = conn.execute(
            "UPDATE app_settings
             SET value = ''
             WHERE LOWER(key) LIKE '%key%'
                OR LOWER(key) LIKE '%secret%'
                OR LOWER(key) LIKE '%token%'
                OR LOWER(key) LIKE '%password%'
                OR LOWER(key) = 'fallback_providers'",
            [],
        )?;
        Ok(affected)
    }

    /// 校验备份文件头（SQLite format 3）及 `schema_version` 完整性，返回 `(schema_version, file_size)`。
    pub fn verify_sqlite_backup(backup_path: &std::path::Path) -> Result<(i32, u64)> {
        use std::io::Read;
        for comp in backup_path.components() {
            if matches!(comp, std::path::Component::ParentDir) {
                anyhow::bail!("校验路径禁止包含 '..'");
            }
        }
        let mut file = std::fs::File::open(backup_path)?;
        let size = file.metadata()?.len();
        let mut header = [0u8; 16];
        file.read_exact(&mut header)?;
        if &header != b"SQLite format 3\0" {
            anyhow::bail!("非有效 SQLite 3 数据库文件头");
        }
        let conn =
            Connection::open_with_flags(backup_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let ver: i32 = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )?;
        Ok((ver, size))
    }
}

#[cfg(test)]
mod backup_tests {
    use super::Database;

    #[test]
    fn vacuum_into_backup_produces_valid_reopenable_sqlite() {
        let dir = tempfile::tempdir().expect("tempdir");
        let main_path = dir.path().join("main.db");
        let backup_path = dir.path().join("snap.db");
        let db = Database::init(main_path.to_str().expect("utf8")).expect("init db");

        let size = db.vacuum_into_backup(&backup_path).expect("vacuum into");
        assert!(size > 4096, "backup db should be non-empty");

        let (ver, verified_size) =
            Database::verify_sqlite_backup(&backup_path).expect("verify backup");
        assert_eq!(size, verified_size);
        assert!(ver >= 25, "expected schema_version >= 25, got {ver}");
    }

    #[test]
    fn scrub_backup_secrets_redacts_api_keys_and_preserves_other_settings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let main_path = dir.path().join("main_sec.db");
        let backup_path = dir.path().join("snap_sec.db");
        let db = Database::init(main_path.to_str().expect("utf8")).expect("init db");
        db.set_setting("ai_api_key", "sk-secret-123456")
            .expect("set key");
        db.set_setting("ai_model", "deepseek-chat")
            .expect("set model");

        db.vacuum_into_backup(&backup_path).expect("vacuum into");
        let scrubbed = Database::scrub_backup_secrets(&backup_path).expect("scrub secrets");
        assert!(scrubbed >= 1);

        let snap_db = Database::init(backup_path.to_str().expect("utf8")).expect("reopen backup");
        assert_eq!(
            snap_db
                .get_setting("ai_api_key")
                .unwrap_or(None)
                .unwrap_or_default(),
            ""
        );
        assert_eq!(
            snap_db
                .get_setting("ai_model")
                .unwrap_or(None)
                .unwrap_or_default(),
            "deepseek-chat"
        );
    }
}
