//! # 创研台 InnoForge 移动端 FFI 接口 / Mobile FFI Interface
//!
//! 为 Android/iOS/HarmonyOS 提供 FFI 入口，通过 `uniffi` 导出原生函数。
//! FFI entry point for Android/iOS/HarmonyOS, using `uniffi` to export native functions.
//!
//! 与 `main.rs` 共享初始化逻辑（`common.rs`），消除双入口同步维护风险。
//! Shared initialization with `main.rs` via `common.rs` to eliminate dual-entry sync risk.

mod ai;
pub mod common;
pub mod db;
mod docx_export;
mod error;
mod experiment;
mod orchestrator;
pub mod patent;
pub mod pipeline;
mod routes;

use common::{build_router, init_app_state};

/// 全局服务器句柄（移动端 FFI 单例） / Global server handle (mobile FFI singleton).
static SERVER_HANDLE: std::sync::Mutex<
    Option<(
        std::thread::JoinHandle<()>,
        tokio::sync::oneshot::Sender<()>,
    )>,
> = std::sync::Mutex::new(None);

/// 启动内嵌 axum 服务器（移动端用，与桌面端共享构建逻辑）。
/// Start embedded axum server for mobile, sharing router/init with desktop.
fn start_server(
    db_path: String,
) -> Result<
    (
        std::thread::JoinHandle<()>,
        tokio::sync::oneshot::Sender<()>,
    ),
    Box<dyn std::error::Error>,
> {
    let state = init_app_state(&db_path)?;
    let app = build_router(state);
    let runtime = tokio::runtime::Runtime::new()?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let handle = std::thread::Builder::new()
        .name("innoforge-mobile-server".to_string())
        .spawn(move || {
            runtime.block_on(async move {
                let addr: std::net::SocketAddr = ([127, 0, 0, 1], 3000).into();
                tracing::info!("InnoForge mobile server starting on http://{}", addr);

                let server = axum::Server::bind(&addr).serve(app.into_make_service());

                tokio::select! {
                    _ = shutdown_rx => {
                        tracing::info!("InnoForge mobile server shutting down gracefully");
                    }
                    result = server => {
                        if let Err(e) = result {
                            tracing::error!("InnoForge mobile server error: {}", e);
                        }
                    }
                }
            });
        })?;

    Ok((handle, shutdown_tx))
}

/// 关闭服务器 / Shutdown server.
fn shutdown_server(handle: std::thread::JoinHandle<()>, tx: tokio::sync::oneshot::Sender<()>) {
    let _ = tx.send(());
    let _ = handle.join();
}

// ================================================================
//  FFI 导出函数（Android / iOS 通过 uniffi 调用）
//  FFI export functions (called via uniffi on Android / iOS)
// ================================================================

/// 启动创研台服务器 / Start InnoForge server.
#[no_mangle]
pub extern "C" fn innoforge_start_server() -> i32 {
    let _ = dotenvy::dotenv_override();
    let _ = tracing_subscriber::fmt::try_init();

    let db_path = if cfg!(target_os = "android") {
        let data_dir = std::env::var("HOME").unwrap_or_else(|_| "/data/local/tmp".to_string());
        format!("{}/innoforge.db", data_dir)
    } else {
        "innoforge.db".to_string()
    };

    let mut server_handle = match SERVER_HANDLE.lock() {
        Ok(server_handle) => server_handle,
        Err(_) => {
            eprintln!("Unable to start server because the server state is unavailable");
            return 1;
        }
    };

    if server_handle.is_some() {
        eprintln!("InnoForge server is already running");
        return 1;
    }

    match start_server(db_path) {
        Ok((handle, tx)) => {
            *server_handle = Some((handle, tx));
            0
        }
        Err(e) => {
            eprintln!("Failed to start server: {}", e);
            1
        }
    }
}

/// 关闭创研台服务器 / Shutdown InnoForge server.
#[no_mangle]
pub extern "C" fn innoforge_shutdown_server() -> i32 {
    let server = match SERVER_HANDLE.lock() {
        Ok(mut server_handle) => server_handle.take(),
        Err(_) => {
            eprintln!("Unable to shut down server because the server state is unavailable");
            return 1;
        }
    };

    if let Some((handle, tx)) = server {
        shutdown_server(handle, tx);
        0
    } else {
        eprintln!("No server to shut down");
        1
    }
}

/// 启动专利 Hub 服务器（兼容旧接口）/ Legacy entry point for patent_hub.
#[no_mangle]
pub extern "C" fn patent_hub_start_server() -> i32 {
    innoforge_start_server()
}

#[no_mangle]
pub extern "C" fn patent_hub_shutdown_server() -> i32 {
    innoforge_shutdown_server()
}
