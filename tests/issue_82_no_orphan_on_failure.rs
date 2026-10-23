//! #82 — e2e 失败路径不回收子进程（实测孤儿 innoforge-server，端口 7631）
//!
//! `spawn_test_server_with_timeout` 在 ready 失败时必须 kill 子进程。
//! 若不 kill，孤儿子进程会继续启动并最终 bind 目标端口。
//!
//! 设计（per brief）：用 0s 超时触发即时 ready 失败。home TempDir 在函数内
//! 被 mem::forget，故 panic 后仍然存在——未 kill 的孤儿能继续启动并 bind 端口。
//! 修复态（kill 后）端口始终无人 bind ⇒ 断言通过；泄漏态（不 kill）孤儿会
//! bind ⇒ 用例红。

mod common;

use common::{free_port, fresh_db_dir, spawn_test_server_with_timeout, wait_for_server};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

/// 正向：0s 超时 → spawn 必 panic → 子进程被 kill → 端口在 15s 内始终不可连。
///
/// 负-A：删除 `spawn_test_server_with_timeout` 里的 `child.kill()` + `child.wait()`
/// （只 panic 不回收）⇒ 孤儿存活并最终 bind 端口 ⇒ `wait_for_server` 返回 true
/// ⇒ panic!("orphan server detected") ⇒ 用例红。
#[test]
fn no_orphan_when_server_fails_to_start() {
    let port = free_port();
    let home = fresh_db_dir();

    // 0s 超时：wait_for_server 立即返回 false。
    let result = catch_unwind(AssertUnwindSafe(|| {
        spawn_test_server_with_timeout(port, 0, home, &[]);
    }));

    assert!(result.is_err(), "spawn must panic with 0s timeout");

    // 轮询端口 15s。修复态（child killed）端口始终无人 bind。
    // 泄漏态（orphan alive）服务器最终启动并 bind 端口 ⇒ 用例红。
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if wait_for_server(port, 1) {
            panic!(
                "orphan server detected on port {} — spawn_test_server_with_timeout \
                 failed to kill the child process on readiness failure (#82)",
                port
            );
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    // 端口在 15s 内始终未 bind → child 已被 kill → 修复生效。
}

/// 确保成功路径仍然正常工作：正常超时下 spawn 返回 TestServer，
/// Drop 时 kill 子进程。
#[test]
fn successful_spawn_still_works() {
    let port = free_port();
    let home = fresh_db_dir();

    let server = spawn_test_server_with_timeout(port, 60, home, &[]);
    assert_eq!(port, server.port);
    // Drop 会 kill 子进程。
    drop(server);
    // 端口应很快不可连（Drop kill 了子进程）。
    assert!(
        !wait_for_server(port, 2),
        "server should be killed after drop"
    );
}
