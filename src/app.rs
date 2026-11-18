//! # 应用入口装配、绑定策略与同源鉴权 / Application Entry, Bind Policy & Auth Guard
//!
//! 统一提供：
//! - `resolve_bind_ip`：默认回环绑定 `127.0.0.1`（可通过 `INNOFORGE_BIND_HOST` 覆盖）
//! - `AuthGuardConfig`：非回环监听或显式配置令牌时自动开启同源 Cookie / Bearer 鉴权
//! - `api_healthz` / `api_auth_unlock`：健康检查与会话解锁端点
//! - `build_app_router`：装配全量路由与鉴权中间件
//! - `verify_json_masks_secrets`：校验响应 JSON 不含未脱敏明文密钥

use std::net::{IpAddr, Ipv4Addr};

use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// 解析服务监听 IP。
/// 未设置或为空时默认绑定回环地址 `127.0.0.1`，防止局域网裸奔。
pub fn resolve_bind_ip() -> IpAddr {
    match std::env::var("INNOFORGE_BIND_HOST") {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                IpAddr::V4(Ipv4Addr::LOCALHOST)
            } else {
                match trimmed.parse::<IpAddr>() {
                    Ok(ip) => ip,
                    Err(e) => {
                        tracing::warn!(
                            "Invalid INNOFORGE_BIND_HOST='{}' ({}), falling back to 127.0.0.1",
                            trimmed,
                            e
                        );
                        IpAddr::V4(Ipv4Addr::LOCALHOST)
                    }
                }
            }
        }
        Err(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
    }
}

/// 同源会话 / Bearer 令牌鉴权配置。
#[derive(Debug, Clone)]
pub struct AuthGuardConfig {
    pub enforce_auth: bool,
    pub access_token: String,
}

impl AuthGuardConfig {
    /// 根据绑定 IP 与 `INNOFORGE_ACCESS_TOKEN` 环境变量构造鉴权配置。
    ///
    /// - 当 `!bind_ip.is_loopback()` 或 `INNOFORGE_ACCESS_TOKEN` 非空时启用鉴权；
    /// - 若未预设 `INNOFORGE_ACCESS_TOKEN`，自动生成 32 位随机令牌；
    /// - 当 `bind_ip.is_loopback()` 且未设置令牌时，关闭鉴权以保持本机零打扰。
    pub fn from_bind_and_env(bind_ip: IpAddr) -> Self {
        let env_token = std::env::var("INNOFORGE_ACCESS_TOKEN")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());

        if !bind_ip.is_loopback() || env_token.is_some() {
            let access_token =
                env_token.unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
            Self {
                enforce_auth: true,
                access_token,
            }
        } else {
            Self {
                enforce_auth: false,
                access_token: String::new(),
            }
        }
    }
}

/// 解锁请求体 / Session unlock request payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUnlockRequest {
    pub access_code: String,
}

/// 定长时间逐字节异或比较，防止时序侧信道。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let max_len = a.len().max(b.len());
    let mut diff: u8 = if a.len() == b.len() { 0 } else { 1 };
    for i in 0..max_len {
        let ba = a.get(i).copied().unwrap_or(0);
        let bb = b.get(i).copied().unwrap_or(0);
        diff |= ba ^ bb;
    }
    diff == 0
}

fn is_whitelisted_path(method: &Method, path: &str) -> bool {
    if method == Method::GET && path == "/api/healthz" {
        return true;
    }
    if method == Method::POST && path == "/api/auth/unlock" {
        return true;
    }
    path.starts_with("/static/")
}

fn extract_bearer_token(req: &Request) -> Option<&str> {
    let raw = req.headers().get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = raw.strip_prefix("Bearer ")?.trim();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

fn extract_cookie_token(req: &Request) -> Option<&str> {
    for val in req.headers().get_all(header::COOKIE) {
        if let Ok(cookie_str) = val.to_str() {
            for part in cookie_str.split(';') {
                let trimmed = part.trim();
                if let Some(token) = trimmed.strip_prefix("innoforge_session=") {
                    let token = token.trim();
                    if !token.is_empty() {
                        return Some(token);
                    }
                }
            }
        }
    }
    None
}

async fn auth_guard_middleware(
    State(auth): State<AuthGuardConfig>,
    req: Request,
    next: Next,
) -> Response {
    if !auth.enforce_auth || is_whitelisted_path(req.method(), req.uri().path()) {
        return next.run(req).await;
    }

    let expected = auth.access_token.as_bytes();
    let bearer_ok = extract_bearer_token(&req)
        .map(|t| constant_time_eq(t.as_bytes(), expected))
        .unwrap_or(false);
    let cookie_ok = extract_cookie_token(&req)
        .map(|t| constant_time_eq(t.as_bytes(), expected))
        .unwrap_or(false);

    if bearer_ok || cookie_ok {
        return next.run(req).await;
    }

    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "status": "error",
            "code": "auth_required",
            "message": "非回环监听已强制启用同源会话/令牌鉴权 / Authentication required"
        })),
    )
        .into_response()
}

/// 健康检查端点 / Health check endpoint.
pub async fn api_healthz() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "innoforge",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// 同源会话解锁端点：校验 `access_code` 后下发 `HttpOnly; SameSite=Strict` Cookie。
pub async fn api_auth_unlock(
    Extension(auth): Extension<AuthGuardConfig>,
    Json(req): Json<AuthUnlockRequest>,
) -> Response {
    if !auth.enforce_auth {
        return (
            StatusCode::OK,
            Json(json!({
                "status": "ok",
                "authenticated": true,
            })),
        )
            .into_response();
    }

    if constant_time_eq(
        req.access_code.trim().as_bytes(),
        auth.access_token.as_bytes(),
    ) {
        let cookie = format!(
            "innoforge_session={}; HttpOnly; SameSite=Strict; Path=/",
            auth.access_token
        );
        match HeaderValue::from_str(&cookie) {
            Ok(cookie_hdr) => (
                StatusCode::OK,
                [(header::SET_COOKIE, cookie_hdr)],
                Json(json!({
                    "status": "ok",
                    "authenticated": true,
                })),
            )
                .into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "status": "error",
                    "code": "cookie_encode_error",
                    "message": "Failed to encode session cookie"
                })),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "status": "error",
                "code": "invalid_access_code",
                "message": "访问令牌无效 / Invalid access code"
            })),
        )
            .into_response()
    }
}

/// 构建带同源鉴权中间件的应用 Router。
pub fn build_app_router(state: crate::routes::AppState, auth: AuthGuardConfig) -> Router {
    crate::common::build_router_inner(state)
        .layer(Extension(auth.clone()))
        .layer(middleware::from_fn_with_state(auth, auth_guard_middleware))
}

/// 递归校验 JSON 载荷中不包含任何长度 ≥ 8 的明文密钥。
pub fn verify_json_masks_secrets(payload: &Value, raw_secrets: &[&str]) -> bool {
    match payload {
        Value::String(s) => {
            for secret in raw_secrets {
                if secret.len() >= 8 && s.contains(secret) {
                    return false;
                }
            }
            true
        }
        Value::Array(items) => items
            .iter()
            .all(|item| verify_json_masks_secrets(item, raw_secrets)),
        Value::Object(map) => map
            .values()
            .all(|val| verify_json_masks_secrets(val, raw_secrets)),
        _ => true,
    }
}
