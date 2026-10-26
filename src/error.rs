//! 统一错误处理 / Unified Error Handling
//!
//! 所有 API 路由共享的错误类型，自动转换为 JSON 响应。
//! Shared error type for all API routes, auto-converts to JSON response.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// 统一应用错误类型 / Unified application error type.
#[derive(Debug)]
#[allow(dead_code)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
            Self::BadRequest(msg) => write!(f, "bad request: {msg}"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };
        (status, Json(json!({"status": "error", "message": message}))).into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(err.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Internal(err.to_string())
    }
}

// ── HTTP 状态码分类 / HTTP status code classification ──────────────────

/// AI 提供商 HTTP 错误的分类 / Classification of AI provider HTTP errors.
///
/// 用于统一判断上游返回的错误是否值得重试，替代散落在 `client.rs`
/// 三条重试循环里的 `status.is_server_error()` / `status.as_u16() == 429`
/// 等临时判断。
///
/// Replaces the ad-hoc `is_server_error()` / `== 429` checks scattered across
/// the three retry loops in `client.rs` with a single classification point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// 成功 (2xx) / Success
    Ok,
    /// 可重试的上游错误 / Retryable upstream error (non-500).
    ///
    /// - **429** Too Many Requests — 频率限制，等一下就好
    /// - **503** Service Unavailable — 上游暂时不可用
    /// - **504** Gateway Timeout — 网关超时，上游可能只是慢
    /// - **422** Unprocessable Entity — 部分提供商对瞬时负载返回 422 而非 503
    RetryableUpstream,
    /// 服务器错误 / Server error (500 and other 5xx not classified above).
    Server,
    /// 不可重试的客户端错误 / Non-retryable client error (400, 401, 403, 404, …).
    Client,
}

impl ErrorKind {
    /// 是否值得重试 / Whether this kind is worth retrying.
    ///
    /// `RetryableUpstream` 和 `Server` 都值得重试——前者是上游临时问题，
    /// 后者是服务器内部错误，重试可能成功。`Client`（认证失败、参数错误等）
    /// 重试无意义。
    pub fn should_retry(self) -> bool {
        matches!(self, Self::RetryableUpstream | Self::Server)
    }
}

/// 根据 HTTP 状态码返回错误分类 / Classify an HTTP status code.
///
/// ```rust,ignore
/// use innoforge::error::{error_kind, ErrorKind};
/// assert_eq!(error_kind(429), ErrorKind::RetryableUpstream);
/// assert_eq!(error_kind(500), ErrorKind::Server);
/// ```
pub fn error_kind(status: u16) -> ErrorKind {
    match status {
        429 | 503 | 504 | 422 => ErrorKind::RetryableUpstream,
        500..=599 => ErrorKind::Server,
        200..=299 => ErrorKind::Ok,
        _ => ErrorKind::Client,
    }
}

#[cfg(test)]
mod tests {
    use super::{error_kind, ErrorKind};

    #[test]
    fn retryable_upstream_codes() {
        // 429 Too Many Requests
        assert_eq!(error_kind(429), ErrorKind::RetryableUpstream);
        // 503 Service Unavailable
        assert_eq!(error_kind(503), ErrorKind::RetryableUpstream);
        // 504 Gateway Timeout
        assert_eq!(error_kind(504), ErrorKind::RetryableUpstream);
        // 422 Unprocessable Entity — 关键变更：之前不重试，现在重试
        assert_eq!(error_kind(422), ErrorKind::RetryableUpstream);
    }

    #[test]
    fn server_error_codes() {
        // 500 Internal Server Error
        assert_eq!(error_kind(500), ErrorKind::Server);
        // 502 Bad Gateway (other 5xx, not specially classified)
        assert_eq!(error_kind(502), ErrorKind::Server);
        // 501 Not Implemented
        assert_eq!(error_kind(501), ErrorKind::Server);
    }

    #[test]
    fn client_error_codes() {
        assert_eq!(error_kind(400), ErrorKind::Client);
        assert_eq!(error_kind(401), ErrorKind::Client);
        assert_eq!(error_kind(403), ErrorKind::Client);
        assert_eq!(error_kind(404), ErrorKind::Client);
    }

    #[test]
    fn success_codes() {
        assert_eq!(error_kind(200), ErrorKind::Ok);
        assert_eq!(error_kind(204), ErrorKind::Ok);
    }

    #[test]
    fn should_retry_semantics() {
        assert!(ErrorKind::RetryableUpstream.should_retry());
        assert!(ErrorKind::Server.should_retry());
        assert!(!ErrorKind::Client.should_retry());
        assert!(!ErrorKind::Ok.should_retry());
    }

    /// 负控：422 必须可重试。若回退到 `is_server_error()` 判断，
    /// 422 是 4xx 不会被捕获，此断言失败（红）。
    #[test]
    fn issue_76_422_must_be_retryable() {
        let kind = error_kind(422);
        assert!(
            kind.should_retry(),
            "422 must be retryable; revert to is_server_error() would make this red"
        );
    }
}
