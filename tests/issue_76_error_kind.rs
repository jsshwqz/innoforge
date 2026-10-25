//! Regression for issue #76: error_kind classification of provider HTTP errors.
//!
//! 核心变更：429 / 504 / 422 / 503 → RetryableUpstream（非 500，可重试），
//! 500 → Server。之前 client.rs 三条重试循环用 `is_server_error()` 判断，
//! 422（4xx）不被捕获、不重试；Anthropic 路径甚至对所有非 2xx 直接报错不重试。
//!
//! 负控说明（revert => red）：
//! - 若把 `error_kind` 中 `422` 从 `RetryableUpstream` 分支移除（回退到
//!   `is_server_error()` 语义），`error_kind(422)` 返回 `Client`，
//!   `should_retry()` 为 false，`issue_76_422_must_retry` 断言失败（红）。
//! - 若把整个 `error_kind` 函数删掉，编译失败（红）。
//! - 若把 `should_retry` 改为只对 `RetryableUpstream` 返回 true（不含 `Server`），
//!   `issue_76_500_server_should_retry` 断言失败（红）。

use innoforge::error::{error_kind, ErrorKind};

/// 429 Too Many Requests → RetryableUpstream
#[test]
fn issue_76_429_retryable_upstream() {
    assert_eq!(error_kind(429), ErrorKind::RetryableUpstream);
    assert!(error_kind(429).should_retry());
}

/// 503 Service Unavailable → RetryableUpstream
#[test]
fn issue_76_503_retryable_upstream() {
    assert_eq!(error_kind(503), ErrorKind::RetryableUpstream);
    assert!(error_kind(503).should_retry());
}

/// 504 Gateway Timeout → RetryableUpstream
#[test]
fn issue_76_504_retryable_upstream() {
    assert_eq!(error_kind(504), ErrorKind::RetryableUpstream);
    assert!(error_kind(504).should_retry());
}

/// 422 Unprocessable Entity → RetryableUpstream
/// 这是关键变更：之前 422 是 4xx，is_server_error() 不捕获，不重试。
#[test]
fn issue_76_422_must_retry() {
    assert_eq!(
        error_kind(422),
        ErrorKind::RetryableUpstream,
        "422 must be RetryableUpstream; revert to is_server_error() makes this red"
    );
    assert!(
        error_kind(422).should_retry(),
        "422 must be retryable; revert to is_server_error() makes this red"
    );
}

/// 500 Internal Server Error → Server（仍然可重试）
#[test]
fn issue_76_500_server() {
    assert_eq!(error_kind(500), ErrorKind::Server);
}

/// Server errors should still be retryable (preserves existing behavior).
#[test]
fn issue_76_500_server_should_retry() {
    assert!(
        error_kind(500).should_retry(),
        "500 (Server) must still be retryable; removing Server from should_retry makes this red"
    );
}

/// 401/403 auth errors → Client, not retryable.
#[test]
fn issue_76_auth_errors_not_retryable() {
    assert_eq!(error_kind(401), ErrorKind::Client);
    assert_eq!(error_kind(403), ErrorKind::Client);
    assert!(!error_kind(401).should_retry());
    assert!(!error_kind(403).should_retry());
}

/// 200 success → Ok, not retryable.
#[test]
fn issue_76_success_not_retryable() {
    assert_eq!(error_kind(200), ErrorKind::Ok);
    assert!(!error_kind(200).should_retry());
}
