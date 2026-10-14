//! # 创研台 InnoForge 主程序 / Main Application
//!
//! Axum Web 服务器入口，注册所有路由并启动 HTTP 服务。
//! Axum web server entry point, registers all routes and starts HTTP service.
//!
//! 默认监听 `0.0.0.0:3000`（可通过 `INNOFORGE_PORT` 覆盖），自动打开浏览器。
//! Listens on `0.0.0.0:3000` by default (override via `INNOFORGE_PORT`), auto-opens browser.
//!
//! ## 双入口架构 / Dual-entry Architecture
//!
//! 本文件（桌面端）与 `lib.rs`（移动端 FFI）共享初始化逻辑，
//! 统一收拢在 `common.rs` 模块中，消除双入口同步维护风险。
//! See `common.rs` for shared initialization, route registration, and asset serving.
//!
//! ## F1: 消除 lib/bin 双份编译
//!
//! 之前 `main.rs` 与 `lib.rs` 各自声明整棵 `mod` 树，同一份源码编译两遍；
//! bin 那份没被触达的 `pub` 项即被判 `dead_code`，需要 13 条 `#[allow]` 止血。
//! 现在 `main.rs` 只 `use innoforge::common`，所有 `crate::` 路径解析到 lib crate，
//! 源码只编译一次，`#[allow(dead_code)]` 可逐条消除。

use std::net::SocketAddr;

use innoforge::common::build_router;

fn bind_candidate_ports(configured_port: Option<u16>) -> Vec<u16> {
    match configured_port {
        Some(port) => vec![port],
        None => vec![3000, 3921, 3100],
    }
}

/// Resolve the database file path following a strict priority order:
///
/// 1. `INNOFORGE_DB` env var (full file path)
/// 2. `INNOFORGE_HOME` env var (directory; db at `<dir>/innoforge.db`)
/// 3. Platform data directory:
///    - Linux:   `~/.local/share/innoforge/innoforge.db`
///    - Windows: `%APPDATA%\innoforge\innoforge.db`
///    - macOS:   `~/Library/Application Support/innoforge/innoforge.db`
/// 4. Directory of the running executable
///
/// If the old location (`./innoforge.db` or `./patent_hub.db` in cwd) exists
/// but the new location does not, auto-migrate (copy then verify).
fn resolve_db_path() -> anyhow::Result<std::path::PathBuf> {
    // 1. INNOFORGE_DB — explicit full file path
    if let Ok(p) = std::env::var("INNOFORGE_DB") {
        let path = std::path::PathBuf::from(&p);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        return Ok(path);
    }

    // 2. INNOFORGE_HOME — directory, db file inside
    if let Ok(dir) = std::env::var("INNOFORGE_HOME") {
        let dir_path = std::path::PathBuf::from(&dir);
        let _ = std::fs::create_dir_all(&dir_path);
        return Ok(dir_path.join("innoforge.db"));
    }

    // 3. Platform data directory
    let platform_dir = platform_data_dir();
    let _ = std::fs::create_dir_all(&platform_dir);
    let new_path = platform_dir.join("innoforge.db");

    // 4. Auto-migrate from old location if new doesn't exist
    auto_migrate_db(&new_path);

    Ok(new_path)
}

/// Return the platform-specific data directory for InnoForge.
fn platform_data_dir() -> std::path::PathBuf {
    if cfg!(target_os = "android") {
        let base = std::env::var("HOME")
            .or_else(|_| std::env::var("TMPDIR"))
            .unwrap_or_else(|_| "/data/local/tmp".to_string());
        return std::path::PathBuf::from(base);
    }

    if cfg!(target_os = "windows") {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return std::path::PathBuf::from(appdata).join("innoforge");
        }
        // Fallback: executable dir
        return exe_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    }

    if cfg!(target_os = "macos") {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("innoforge");
        }
    }

    // Linux & other Unix: ~/.local/share/innoforge
    if let Ok(home) = std::env::var("HOME") {
        return std::path::PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("innoforge");
    }

    // Last resort: executable directory
    exe_dir().unwrap_or_else(|| std::path::PathBuf::from("."))
}

/// Directory of the currently running executable.
fn exe_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
}

/// If old db file exists in cwd but new path doesn't, copy it over.
/// Keeps the original file in place (non-destructive).  On copy failure, logs
/// a warning but does not abort — the app will start with a fresh database.
fn auto_migrate_db(new_path: &std::path::Path) {
    if new_path.exists() {
        return; // new location already has a db, nothing to migrate
    }

    // Check for old `innoforge.db` or `patent_hub.db` in cwd
    let candidates = ["innoforge.db", "patent_hub.db"];
    for old_name in &candidates {
        let old_path = std::path::Path::new(old_name);
        if old_path.exists() && old_path.is_file() {
            match std::fs::copy(old_path, new_path) {
                Ok(bytes) => {
                    // Verify copy by comparing file sizes
                    let old_size = std::fs::metadata(old_path).map(|m| m.len()).unwrap_or(0);
                    if bytes == old_size {
                        tracing::info!(
                            "Auto-migrated database from {} to {} ({} bytes)",
                            old_path.display(),
                            new_path.display(),
                            bytes
                        );
                        println!(
                            "Database auto-migrated: {} -> {}",
                            old_path.display(),
                            new_path.display()
                        );
                    } else {
                        tracing::error!(
                            "Database migration size mismatch: old={} copied={}",
                            old_size,
                            bytes
                        );
                        // Remove potentially corrupt copy
                        let _ = std::fs::remove_file(new_path);
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to migrate database from {}: {}",
                        old_path.display(),
                        e
                    );
                }
            }
            return; // Only migrate from the first match
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv_override();
    tracing_subscriber::fmt::init();

    // Resolve database path following priority order (issue #14)
    let db_path_buf = resolve_db_path()?;
    let db_path = db_path_buf.to_string_lossy().to_string();

    // Print absolute path to stdout and log
    let db_abs = db_path_buf
        .canonicalize()
        .unwrap_or_else(|_| db_path_buf.clone());
    println!("Database path: {}", db_abs.display());
    tracing::info!("Database path: {}", db_abs.display());

    let state = innoforge::common::init_app_state(&db_path)?;
    let app = build_router(state);

    let configured_port = std::env::var("INNOFORGE_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok());
    let mut server = None;
    let mut last_bind_error = None;
    for candidate in bind_candidate_ports(configured_port) {
        let candidate_addr = SocketAddr::from(([0, 0, 0, 0], candidate));
        match axum::Server::try_bind(&candidate_addr) {
            Ok(bound_server) => {
                server = Some((candidate, candidate_addr, bound_server));
                break;
            }
            Err(e) => {
                if configured_port.is_some() {
                    return Err(e.into());
                }
                eprintln!(
                    "Port {} unavailable ({}), trying next fallback...",
                    candidate, e
                );
                last_bind_error = Some(e);
            }
        }
    }
    let (port, addr, server) = match server {
        Some(server) => server,
        None => {
            if let Some(e) = last_bind_error {
                return Err(e.into());
            }
            return Err(anyhow::anyhow!("no bind candidate ports configured"));
        }
    };
    println!(
        "创研台 InnoForge v{} running at http://{addr}",
        env!("CARGO_PKG_VERSION")
    );
    println!("Local access: http://127.0.0.1:{port}");

    // 自动打开浏览器（设置 INNOFORGE_NO_OPEN 可禁用）
    if std::env::var("INNOFORGE_NO_OPEN").is_err() {
        let url = format!("http://127.0.0.1:{port}/");
        if let Err(e) = open::that(&url) {
            println!("Could not open browser: {}", e);
            println!("Please visit: {}", url);
        }
    }

    // 显示局域网 IP（方便手机访问）
    if let Ok(local_ip) = local_ip_address::local_ip() {
        println!("Mobile access: http://{}:{port}", local_ip);
    }

    server.serve(app.into_make_service()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_port_has_fallback_candidates() {
        assert_eq!(bind_candidate_ports(None), vec![3000, 3921, 3100]);
    }

    #[test]
    fn explicit_port_does_not_fallback_to_other_ports() {
        assert_eq!(bind_candidate_ports(Some(4567)), vec![4567]);
    }

    #[test]
    fn resolve_db_path_prefers_innoforge_db_env() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        std::env::set_var("INNOFORGE_DB", &path);
        let resolved = resolve_db_path().unwrap();
        std::env::remove_var("INNOFORGE_DB");
        assert_eq!(resolved, std::path::PathBuf::from(&path));
    }

    #[test]
    fn resolve_db_path_prefers_innoforge_home_env() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();
        std::env::set_var("INNOFORGE_HOME", &dir_path);
        std::env::remove_var("INNOFORGE_DB");
        let resolved = resolve_db_path().unwrap();
        std::env::remove_var("INNOFORGE_HOME");
        assert_eq!(resolved, dir.path().join("innoforge.db"));
    }

    #[test]
    fn platform_data_dir_is_absolute_on_linux() {
        std::env::remove_var("INNOFORGE_DB");
        std::env::remove_var("INNOFORGE_HOME");
        let dir = platform_data_dir();
        // On Linux with HOME set, should be under ~/.local/share/innoforge
        if cfg!(target_os = "linux") && std::env::var("HOME").is_ok() {
            assert!(dir.is_absolute(), "platform_data_dir should be absolute, got {}", dir.display());
            assert!(
                dir.to_string_lossy().contains("innoforge"),
                "should contain 'innoforge' in path"
            );
        }
    }

    #[test]
    fn auto_migrate_db_copies_from_cwd() {
        let tmp = tempfile::tempdir().unwrap();
        let new_path = tmp.path().join("innoforge.db");
        // Create a fake old db in cwd
        let old_path = std::path::Path::new("innoforge.db");
        std::fs::write(old_path, b"test db content").unwrap();
        auto_migrate_db(&new_path);
        assert!(new_path.exists(), "new db should exist after migration");
        let content = std::fs::read_to_string(&new_path).unwrap();
        assert_eq!(content, "test db content");
        // Old file should still exist (non-destructive)
        assert!(old_path.exists(), "old db should still exist");
        let _ = std::fs::remove_file(old_path);
    }
}
