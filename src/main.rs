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

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use innoforge::common::build_router_with_token;

fn resolve_bind_ip() -> IpAddr {
    let raw = std::env::var("INNOFORGE_BIND_HOST")
        .ok()
        .or_else(|| std::env::var("HOST").ok());
    match raw.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => s
            .parse::<IpAddr>()
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        None => IpAddr::V4(Ipv4Addr::LOCALHOST),
    }
}

fn resolve_access_token(bind_ip: IpAddr) -> Option<String> {
    if let Ok(tok) = std::env::var("INNOFORGE_ACCESS_TOKEN") {
        let trimmed = tok.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if !bind_ip.is_loopback() {
        return Some(uuid::Uuid::new_v4().simple().to_string());
    }
    None
}

fn bind_candidate_ports(configured_port: Option<u16>) -> Vec<u16> {
    match configured_port {
        Some(port) => vec![port],
        None => vec![3000, 3921, 3100],
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv_override();
    tracing_subscriber::fmt::init();

    // Resolve database path following priority order (issue #14)
    let db_path_buf = innoforge::common::resolve_db_path()?;
    let db_path = db_path_buf.to_string_lossy().to_string();

    // Print absolute path to stdout and log
    let db_abs = db_path_buf
        .canonicalize()
        .unwrap_or_else(|_| db_path_buf.clone());
    println!("Database path: {}", db_abs.display());
    tracing::info!("Database path: {}", db_abs.display());

    let mut state = innoforge::common::init_app_state(&db_path)?;

    let bind_ip = resolve_bind_ip();
    let configured_port = std::env::var("INNOFORGE_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok());
    let mut server = None;
    let mut last_bind_error = None;
    for candidate in bind_candidate_ports(configured_port) {
        let candidate_addr = SocketAddr::new(bind_ip, candidate);
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

    // Unify port source: callback follows actual bound port (issue #33)
    state.listen_port = port;

    // Startup self-check: print effective callback address (issue #33)
    let callback_url = innoforge::common::oauth_callback_url(port);
    let has_oauth = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        !cfg.google_client_id.is_empty()
    };
    if has_oauth {
        println!("OAuth callback URL: {}", callback_url);
        // Warn if explicit callback URL port differs from listen port
        if let Ok(explicit) = std::env::var("INNOFORGE_OAUTH_CALLBACK_URL") {
            if !explicit.is_empty() && !explicit.contains(&format!(":{port}")) {
                eprintln!(
                    "⚠️  INNOFORGE_OAUTH_CALLBACK_URL='{}' does not contain listen port {} — \
                     OAuth redirect may fail",
                    explicit, port
                );
            }
        }
    }

    let access_token = resolve_access_token(bind_ip);
    if let Some(ref token) = access_token {
        println!(
            "🔒 Access token auth enabled (non-loopback or INNOFORGE_ACCESS_TOKEN set). Pass Header `Authorization: Bearer {token}` or `X-InnoForge-Token: {token}`."
        );
    }

    let app = build_router_with_token(state, access_token.clone());

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

    // 显示局域网 IP（方便手机 / WSL2 宿主机一键访问）
    if let Ok(local_ip) = local_ip_address::local_ip() {
        if let Some(ref token) = access_token {
            println!("Mobile access: http://{}:{port}/?token={token}", local_ip);
        } else {
            println!("Mobile access: http://{}:{port}", local_ip);
        }
    }

    server.serve(app.into_make_service()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::bind_candidate_ports;

    #[test]
    fn default_port_has_fallback_candidates() {
        assert_eq!(bind_candidate_ports(None), vec![3000, 3921, 3100]);
    }

    #[test]
    fn explicit_port_does_not_fallback_to_other_ports() {
        assert_eq!(bind_candidate_ports(Some(4567)), vec![4567]);
    }
}
