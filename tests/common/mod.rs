//! Shared test helpers for all integration tests that start the server.
//!
//! Provides a pre-migrated DB template (created once, copied per test),
//! readiness polling (every 200ms, 60s cap), and a `spawn_test_server`
//! entry point that every test file should use instead of rolling
//! its own `start_server` / `wait_for_server` / `kill_server`.
//!
//! Usage:
//! ```ignore
//! mod common;
//! use common::spawn_test_server;
//!
//! #[test]
//! fn my_test() {
//!     let port = common::free_port();
//!     let ai_port = common::free_port();
//!     // start stub on ai_port ...
//!     let server = spawn_test_server(port, &[
//!         ("AI_BASE_URL", &format!("http://127.0.0.1:{}", ai_port)),
//!         ("AI_API_KEY", "test-key-not-real"),
//!     ]);
//!     // server is killed automatically when `server` is dropped
//! }
//! ```

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

// ── Port allocation ──

/// Bind to 127.0.0.1:0, extract the OS-assigned port, then drop the listener.
pub fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    listener.local_addr().unwrap().port()
}

// ── Pre-migrated DB template ──

static DB_TEMPLATE: OnceLock<PathBuf> = OnceLock::new();

/// Ensure a pre-migrated database template exists. On first call, starts a
/// server with a fresh INNOFORGE_HOME, waits for /api/version (which triggers
/// all migrations), kills the server, and caches the DB path. Subsequent
/// calls return the cached path instantly.
fn ensure_db_template() -> PathBuf {
    DB_TEMPLATE
        .get_or_init(|| {
            let template_dir = tempfile::tempdir().expect("create template dir");
            let template_port = free_port();
            // #82: spawn() 成功后任何路径都必须回收子进程。
            // 若 wait_for_server 失败（模板服务器未能在 60s 内就绪），
            // 先 kill+wait 再 panic，避免孤儿进程残留。
            let mut child = Command::new(env!("CARGO_BIN_EXE_innoforge-server"))
                .env("INNOFORGE_PORT", template_port.to_string())
                .env("INNOFORGE_NO_OPEN", "1")
                .env("INNOFORGE_HOME", template_dir.path().to_str().unwrap())
                .spawn()
                .expect("spawn server for template");
            if !wait_for_server(template_port, 60) {
                let _ = child.kill();
                let _ = child.wait();
                panic!("template server must start within 60s");
            }
            kill_server(child);
            let db_path = template_dir.path().join("innoforge.db");
            assert!(db_path.exists(), "template db must exist after migration");
            // Leak the tempdir so the DB file survives — template lives for process lifetime.
            std::mem::forget(template_dir);
            db_path
        })
        .clone()
}

/// Copy the pre-migrated template DB to a fresh temp dir. The returned
/// TempDir must be kept alive for the server's lifetime.
pub fn fresh_db_dir() -> tempfile::TempDir {
    let template = ensure_db_template();
    let dir = tempfile::tempdir().expect("create fresh db dir");
    std::fs::copy(&template, dir.path().join("innoforge.db")).expect("copy template db");
    dir
}

// ── Readiness polling ──

/// Poll /api/version every 200ms until it returns 200, or timeout.
/// No fixed sleep — the 200ms sleep is the poll interval, not a startup guess.
pub fn wait_for_server(port: u16, timeout_secs: u64) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < timeout_secs {
        if let Ok(resp) = reqwest::blocking::get(format!("http://127.0.0.1:{}/api/version", port)) {
            if resp.status().is_success() {
                return true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    false
}

// ── Server lifecycle ──

/// Kill and reap a child process.
pub fn kill_server(mut child: std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// A running test server with its own isolated DB. Killed automatically on drop.
#[allow(dead_code)]
pub struct TestServer {
    /// The server child process.
    pub child: std::process::Child,
    /// The port the server is listening on.
    pub port: u16,
    /// The INNOFORGE_HOME temp dir — must stay alive for the server's lifetime.
    pub home: Option<tempfile::TempDir>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Build the base `Command` for spawning a test server with full env isolation.
///
/// Shared by [`spawn_test_server`] and [`spawn_test_server_with_timeout`] so
/// both get the same `env_clear` + PATH passthrough + cwd isolation.
fn build_server_cmd(home_path: &std::path::Path, port: u16, extra_env: &[(&str, &str)]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_innoforge-server"));
    // 隔离 1（#68 验收时发现）：cwd 指到临时 home，避免 dotenvy 从仓库根读到
    // 开发者本地的 .env（`dotenvy::dotenv_override()` 会覆盖进程环境）。
    cmd.current_dir(home_path);
    // 隔离 2：清空继承环境，避免开发机上的 AI_* / SERPAPI_* / EPO_* 泄漏进被测进程，
    // 造成"CI 绿、本机红"的不可复现结果。
    cmd.env_clear();
    for key in [
        "PATH",
        "SystemRoot",
        "TEMP",
        "TMP",
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
    ] {
        if let Ok(v) = std::env::var(key) {
            cmd.env(key, v);
        }
    }
    cmd.env("INNOFORGE_PORT", port.to_string())
        .env("INNOFORGE_NO_OPEN", "1")
        .env("INNOFORGE_HOME", home_path.to_str().unwrap());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd
}

/// Spawn a test server with a configurable readiness timeout.
///
/// Takes `home` by value. On **failure** (server not ready within
/// `timeout_secs`): kills the child, then `mem::forget`s `home` so the
/// temp dir survives the panic (needed by orphan-detection tests that
/// require the home to persist so an un-killed orphan can actually start),
/// then panics. On **success**: moves `home` into the returned `TestServer`.
///
/// Most callers should use [`spawn_test_server`] (60s timeout, fresh DB).
/// Tests that need a short timeout or a persistent home use this directly.
pub fn spawn_test_server_with_timeout(
    port: u16,
    timeout_secs: u64,
    home: tempfile::TempDir,
    extra_env: &[(&str, &str)],
) -> TestServer {
    let home_path = home.path().to_path_buf();
    let mut cmd = build_server_cmd(&home_path, port, extra_env);
    // #82: spawn() 成功后任何路径都必须回收子进程。
    // 若 wait_for_server 失败，先 kill+wait 再 panic，避免孤儿进程残留。
    // mem::forget(home) 确保临时目录在 panic unwind 中不被删除——
    // 孤儿进程检测测试需要 home 持续存在，否则未 kill 的孤儿也会因 home
    // 被删而自崩，导致测试恒真（上一轮踩坑）。
    let mut child = cmd.spawn().expect("failed to spawn server");
    if !wait_for_server(port, timeout_secs) {
        let _ = child.kill();
        let _ = child.wait();
        std::mem::forget(home);
        panic!(
            "server must start on port {} within {}s",
            port, timeout_secs
        );
    }
    TestServer {
        child,
        port,
        home: Some(home),
    }
}

/// Spawn a test server with a pre-migrated DB copy and wait for readiness.
///
/// `extra_env` is applied on top of the defaults
/// (`INNOFORGE_PORT`, `INNOFORGE_NO_OPEN=1`, `INNOFORGE_HOME`).
/// Typically you pass `AI_BASE_URL` and `AI_API_KEY` here.
///
/// Panics if the server does not become ready within 60s.
#[allow(dead_code)]
pub fn spawn_test_server(port: u16, extra_env: &[(&str, &str)]) -> TestServer {
    let home = fresh_db_dir();
    spawn_test_server_with_timeout(port, 60, home, extra_env)
}
