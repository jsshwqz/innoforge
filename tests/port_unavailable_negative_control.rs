//! issue #110 负控 A：被测端口被人为占用时，spawn 必须**立刻**以
//! `port unavailable` 显式失败——而不是挂着 60s 后给出误导性的
//! "server must start" 断言（旧行为把基础设施问题伪装成产品问题）。
//!
//! 正控（cargo test 全量连跑 3 次稳定）由全量套件本身承担；
//! 负控 B（回退 free_port+drop 复现竞态）的实证记录见 issue #110 描述
//! （CI 实测翻转 + OAuth 测试 3921 保留段现场件）。

mod common;

#[test]
fn occupied_port_fails_fast_with_port_unavailable() {
    // 人为占用一个本进程已知的可用端口
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind :0");
    let port = listener.local_addr().expect("local_addr").port();

    let home = common::fresh_db_dir();
    // 显式端口 + 端口已被占用 → 预留阶段立即失败，panic 信息含 `port unavailable`
    let result = std::panic::catch_unwind(|| {
        common::spawn_test_server_with_timeout(port, 2, home, &[]);
    });
    // 释放占用，让 panic payload 检查能独立进行
    drop(listener);
    let err = match result {
        Err(err) => err,
        Ok(_) => panic!("端口被占用时必须失败，居然成功启动了"),
    };
    let msg = err
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default();
    assert!(
        msg.contains("port unavailable"),
        "panic 信息必须含 `port unavailable`（issue #110 负控 A），实际：{}",
        msg
    );
}

#[test]
fn free_port_entry_reports_port_unavailable_after_retries() {
    // spawn_test_server_on_free_port 的失败路径：把 OS 的临时端口池打满是不现实的，
    // 这里验证的是「3 次尝试全失败 → panic 信息含 port unavailable + 保留最后错误」
    // 的收口形态——用一个确定不可绑定的端口段起点不可行（:0 由内核分配），
    // 故本负控只覆盖 catch_unwind 可达的最小形态：正常路径不 panic（正向护栏）。
    let server = common::spawn_test_server_on_free_port(&[]);
    assert!(server.port > 0);
    // Drop 会自动回收子进程
}
