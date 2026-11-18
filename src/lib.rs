//! # 创研台 InnoForge 移动端 FFI 接口 / Mobile FFI Interface
//!
//! 为 Android/iOS/HarmonyOS 提供 FFI 入口，通过 `uniffi` 导出原生函数。
//! FFI entry point for Android/iOS/HarmonyOS, using `uniffi` to export native functions.
//!
//! 与 `main.rs` 共享初始化逻辑（`common.rs`），消除双入口同步维护风险。
//! Shared initialization with `main.rs` via `common.rs` to eliminate dual-entry sync risk.

pub mod ai;
pub mod app;
mod cad;
pub mod cards;
pub mod common;
pub(crate) mod context;
pub mod core;
pub mod db;
mod docx_export;
pub mod error;
/// 证据可核验分析内核 v2（设计文档 docs/design/evidence-kernel-v2.md）
pub mod evidence_kernel;
mod experiment;
mod oa_deadline;
pub mod orchestrator;
pub mod patent;
pub mod pipeline;
pub mod rag;
mod routes;
/// 统一检索层（MA1）：`pub` 与 `types` 同理——bin 入口 `main.rs` 也声明本模块，
/// 若判为私有会让仅被 bin 侧不可达的 `pub` 项在 dead_code 上语义不一致。
pub mod search;
pub mod types;
pub mod vector;

use std::net::{IpAddr, Ipv4Addr};

use app::{build_app_router, AuthGuardConfig};
use common::init_app_state;

struct MobileServerHandle {
    join_handle: std::thread::JoinHandle<()>,
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    bound_port: u16,
}

/// 全局服务器句柄（移动端 FFI 单例） / Global server handle (mobile FFI singleton).
static SERVER_HANDLE: std::sync::Mutex<Option<MobileServerHandle>> = std::sync::Mutex::new(None);

fn resolve_mobile_db_path() -> String {
    if let Ok(p) = std::env::var("INNOFORGE_DB") {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if cfg!(target_os = "android") {
        let data_dir = std::env::var("HOME").unwrap_or_else(|_| "/data/local/tmp".to_string());
        format!("{}/innoforge.db", data_dir)
    } else {
        "innoforge.db".to_string()
    }
}

/// 启动内嵌 axum 服务器（移动端用，与桌面端共享构建逻辑）。
/// Start embedded axum server for mobile, sharing router/init with desktop.
fn start_server_on_port(
    db_path: String,
    port: u16,
) -> Result<MobileServerHandle, Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Runtime::new()?;
    let mut state = {
        let _rt_guard = runtime.enter();
        init_app_state(&db_path)?
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u16, String>>();

    let join_handle = std::thread::Builder::new()
        .name("innoforge-mobile-server".to_string())
        .spawn(move || {
            runtime.block_on(async move {
                let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
                let addr: std::net::SocketAddr = (bind_ip, port).into();
                tracing::info!("InnoForge mobile server starting on http://{}", addr);

                let listener = match tokio::net::TcpListener::bind(addr).await {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!("InnoForge mobile server bind error: {}", e);
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };

                let bound_port = match listener.local_addr() {
                    Ok(local_addr) => local_addr.port(),
                    Err(e) => {
                        tracing::error!("InnoForge mobile server local_addr error: {}", e);
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };

                state.listen_port = bound_port;
                let auth = AuthGuardConfig::from_bind_and_env(bind_ip);
                let app = build_app_router(state, auth);

                let _ = ready_tx.send(Ok(bound_port));

                if let Err(e) = axum::serve(listener, app)
                    .with_graceful_shutdown(async move {
                        let _ = shutdown_rx.await;
                        tracing::info!("InnoForge mobile server shutting down gracefully");
                    })
                    .await
                {
                    tracing::error!("InnoForge mobile server error: {}", e);
                }
            });
        })?;

    match ready_rx.recv() {
        Ok(Ok(bound_port)) => Ok(MobileServerHandle {
            join_handle,
            shutdown_tx,
            bound_port,
        }),
        Ok(Err(err_msg)) => {
            let _ = join_handle.join();
            Err(err_msg.into())
        }
        Err(e) => {
            let _ = join_handle.join();
            Err(e.into())
        }
    }
}

/// 关闭服务器 / Shutdown server.
fn shutdown_server(handle: MobileServerHandle) {
    let _ = handle.shutdown_tx.send(());
    let _ = handle.join_handle.join();
}

// ================================================================
//  FFI 导出函数（Android / iOS 通过 uniffi 调用）
//  FFI export functions (called via uniffi on Android / iOS)
// ================================================================

/// 在指定端口启动创研台服务器（传 `0` 自动分配空闲端口）。
/// Start InnoForge server on the specified port (`0` selects an ephemeral port).
#[no_mangle]
pub extern "C" fn innoforge_start_server_on_port(port: u16) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = dotenvy::dotenv_override();
        let _ = tracing_subscriber::fmt::try_init();

        let db_path = resolve_mobile_db_path();

        let mut server_handle = SERVER_HANDLE.lock().unwrap_or_else(|e| e.into_inner());
        if server_handle.is_some() {
            eprintln!("InnoForge server is already running");
            return 1;
        }

        match start_server_on_port(db_path, port) {
            Ok(handle) => {
                *server_handle = Some(handle);
                0
            }
            Err(e) => {
                eprintln!("Failed to start server: {}", e);
                1
            }
        }
    }))
    .unwrap_or(-1)
}

/// 查询当前运行中服务器的实际监听端口；未运行返回 `-1`。
/// Return the active bound port, or `-1` if the server is not running.
#[no_mangle]
pub extern "C" fn innoforge_get_bound_port() -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let server_handle = SERVER_HANDLE.lock().unwrap_or_else(|e| e.into_inner());
        match server_handle.as_ref() {
            Some(handle) => i32::from(handle.bound_port),
            None => -1,
        }
    }))
    .unwrap_or(-1)
}

/// 启动创研台服务器 / Start InnoForge server.
#[no_mangle]
pub extern "C" fn innoforge_start_server() -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let port = std::env::var("INNOFORGE_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(3000);
        innoforge_start_server_on_port(port)
    }))
    .unwrap_or(-1)
}

/// 关闭创研台服务器 / Shutdown InnoForge server.
#[no_mangle]
pub extern "C" fn innoforge_shutdown_server() -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let server = {
            let mut server_handle = SERVER_HANDLE.lock().unwrap_or_else(|e| e.into_inner());
            server_handle.take()
        };

        if let Some(handle) = server {
            shutdown_server(handle);
            0
        } else {
            eprintln!("No server to shut down");
            1
        }
    }))
    .unwrap_or(-1)
}

/// 启动专利 Hub 服务器（兼容旧接口）/ Legacy entry point for patent_hub.
#[no_mangle]
pub extern "C" fn patent_hub_start_server() -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| innoforge_start_server()))
        .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn patent_hub_shutdown_server() -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| innoforge_shutdown_server()))
        .unwrap_or(-1)
}
