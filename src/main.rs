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

mod ai;
mod docx_export;
pub mod common;
mod db;
mod error;
mod experiment;
mod orchestrator;
mod patent;
pub mod pipeline;
mod routes;

use std::net::SocketAddr;

use common::build_router;

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

    // 统一初始化 / Unified initialization (see common.rs)
    let db_path = if cfg!(target_os = "android") {
        let data_dir = std::env::var("HOME")
            .or_else(|_| std::env::var("TMPDIR"))
            .unwrap_or_else(|_| "/data/local/tmp".to_string());
        format!("{}/innoforge.db", data_dir)
    } else {
        // 兼容旧版数据库文件名
        if std::path::Path::new("patent_hub.db").exists()
            && !std::path::Path::new("innoforge.db").exists()
        {
            let _ = std::fs::rename("patent_hub.db", "innoforge.db");
        }
        "innoforge.db".to_string()
    };

    let state = common::init_app_state(&db_path)?;
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
    println!("创研台 InnoForge running at http://{addr}");
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
