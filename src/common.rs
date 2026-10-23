//! # 创研台 InnoForge 共享初始化模块 / Shared Initialization Module
//!
//! 被 `main.rs`（桌面端）和 `lib.rs`（移动端 FFI）共同使用，
//! 消除双入口的代码重复与同步维护风险。
//!
//! Used by both `main.rs` (desktop) and `lib.rs` (mobile FFI) to
//! eliminate code duplication and sync maintenance risks.
//!
//! ## 设计原则 / Design Principles
//! - 路由注册集中在一处，避免 `main.rs` 和 `lib.rs` 发散
//! - 所有路由变化只需修改本文件
//! - 静态资源嵌入与服务函数统一管理

use axum::{
    body::Body,
    extract::DefaultBodyLimit,
    http::{HeaderValue, Response, StatusCode},
    routing::{delete, get, post, put},
    Router,
};
use rust_embed::Embed;
use std::sync::{Arc, RwLock};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

/// 嵌入的静态资源 / Embedded static assets.
#[derive(Embed)]
#[folder = "static/"]
pub struct StaticAssets;

const DEFAULT_CORS_ORIGINS: [&str; 2] = ["http://127.0.0.1:3000", "http://localhost:3000"];

/// Returns the local defaults plus explicitly configured, syntactically valid origins.
/// Invalid configuration is ignored so it can never broaden the CORS policy.
fn cors_allowed_origins(configured_origins: Option<&str>) -> Vec<HeaderValue> {
    let mut origins = DEFAULT_CORS_ORIGINS
        .iter()
        .map(|origin| HeaderValue::from_static(origin))
        .collect::<Vec<_>>();

    if let Some(configured_origins) = configured_origins {
        for origin in configured_origins.split(',').map(str::trim) {
            if let Some(origin) = parse_cors_origin(origin) {
                origins.push(origin);
            } else if !origin.is_empty() {
                tracing::warn!("Ignoring invalid INNOFORGE_CORS_ORIGINS entry");
            }
        }
    }

    origins
}

fn parse_cors_origin(origin: &str) -> Option<HeaderValue> {
    let uri = origin.parse::<axum::http::Uri>().ok()?;
    let scheme = uri.scheme_str()?;
    let authority = uri.authority()?;
    if !matches!(scheme, "http" | "https")
        || authority.as_str().contains('@')
        || uri.path() != "/"
        || uri.query().is_some()
        || origin.contains('#')
    {
        return None;
    }

    origin.parse::<HeaderValue>().ok()
}

/// 统一的静态文件服务函数 / Unified static file serving function.
/// 从编译进二进制的静态资源中读取并返回。
pub async fn serve_static_embedded(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response<Body> {
    match StaticAssets::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            match Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", mime.as_ref())
                .header("Cache-Control", "public, max-age=3600")
                .body(Body::from(content.data.to_vec()))
            {
                Ok(resp) => resp,
                Err(_) => Response::new(Body::from(content.data.to_vec())),
            }
        }
        None => match Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not found"))
        {
            Ok(resp) => resp,
            Err(_) => Response::new(Body::from("Not found")),
        },
    }
}

/// Resolve the database file path following a strict priority order (issue #14):
///
/// 1. `INNOFORGE_DB` env var (full file path)
/// 2. `INNOFORGE_HOME` env var (directory; db at `<dir>/innoforge.db`)
/// 3. Platform data directory:
///    - Linux:   `~/.local/share/innoforge/innoforge.db`
///    - Windows: `%APPDATA%\\innoforge\\innoforge.db`
///    - macOS:   `~/Library/Application Support/innoforge/innoforge.db`
/// 4. Directory of the running executable
///
/// If the old location (`./innoforge.db` or `./patent_hub.db` in cwd) exists
/// but the new location does not, auto-migrate (copy then verify).  If the
/// copy fails, returns an error — does NOT silently start with an empty db.
pub fn resolve_db_path() -> anyhow::Result<std::path::PathBuf> {
    // 1. INNOFORGE_DB — explicit full file path
    if let Ok(p) = std::env::var("INNOFORGE_DB") {
        let path = std::path::PathBuf::from(&p);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        return Ok(path);
    }

    // 2. INNOFORGE_HOME — directory, db file inside
    if let Ok(dir) = std::env::var("INNOFORGE_HOME") {
        let dir_path = std::path::PathBuf::from(&dir);
        let _ = std::fs::create_dir_all(&dir_path);
        return Ok(dir_path.join("innoforge.db"));
    }

    // 3. Platform data directory
    let platform_dir = platform_data_dir();
    let _ = std::fs::create_dir_all(&platform_dir);
    let new_path = platform_dir.join("innoforge.db");

    // 4. Auto-migrate from old location if new doesn't exist
    auto_migrate_db(&new_path)?;

    Ok(new_path)
}

/// Return the platform-specific data directory for InnoForge.
pub fn platform_data_dir() -> std::path::PathBuf {
    if cfg!(target_os = "android") {
        let base = std::env::var("HOME")
            .or_else(|_| std::env::var("TMPDIR"))
            .unwrap_or_else(|_| "/data/local/tmp".to_string());
        return std::path::PathBuf::from(base);
    }

    if cfg!(target_os = "windows") {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return std::path::PathBuf::from(appdata).join("innoforge");
        }
        return exe_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    }

    if cfg!(target_os = "macos") {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("innoforge");
        }
    }

    // Linux & other Unix: ~/.local/share/innoforge
    if let Ok(home) = std::env::var("HOME") {
        return std::path::PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("innoforge");
    }

    exe_dir().unwrap_or_else(|| std::path::PathBuf::from("."))
}

/// Directory of the currently running executable.
pub fn exe_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
}

/// SQLite magic header: first 16 bytes of any valid SQLite 3 database file.
const SQLITE_MAGIC: &[u8] = b"SQLite format 3\x00";

/// Check whether a file starts with the SQLite 3 magic header (16 bytes).
/// Returns false for missing files, directories, or files too small to read.
pub fn is_sqlite_file(path: &std::path::Path) -> bool {
    use std::io::Read;
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut header = [0u8; 16];
    match file.read_exact(&mut header) {
        Ok(()) => header == SQLITE_MAGIC,
        Err(_) => false,
    }
}

/// If old db file exists in cwd but new path doesn't, copy it over.
/// Keeps the original file in place (non-destructive).
///
/// **Validation (issue #19):** Before copying, the source file's first 16
/// bytes are checked against the SQLite magic header.  If the file is not a
/// valid SQLite database, migration is **skipped** (no copy), a warning is
/// logged with both paths, and `Ok(())` is returned so the caller creates a
/// fresh database at the new location.  The user is never locked out.
pub fn auto_migrate_db(new_path: &std::path::Path) -> anyhow::Result<()> {
    if new_path.exists() {
        return Ok(()); // new location already has a db
    }

    let candidates = ["innoforge.db", "patent_hub.db"];
    for old_name in &candidates {
        let old_path = std::path::Path::new(old_name);
        if old_path.exists() && old_path.is_file() {
            // Validate SQLite magic header before copying (issue #19).
            if !is_sqlite_file(old_path) {
                tracing::warn!(
                    "Skipping migration: {} is not a valid SQLite database \
                     (missing magic header). A fresh database will be created at {}.",
                    old_path.display(),
                    new_path.display()
                );
                println!(
                    "Skipping migration: {} is not a valid SQLite database.                      A fresh database will be created at {}.",
                    old_path.display(),
                    new_path.display()
                );
                return Ok(()); // Don't copy garbage; let fresh db be created
            }

            let bytes = std::fs::copy(old_path, new_path).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to migrate database from {}: {}",
                    old_path.display(),
                    e
                )
            })?;

            let old_size = std::fs::metadata(old_path).map(|m| m.len()).unwrap_or(0);
            if bytes != old_size {
                let _ = std::fs::remove_file(new_path);
                return Err(anyhow::anyhow!(
                    "Database migration size mismatch: old={} copied={}",
                    old_size,
                    bytes
                ));
            }

            tracing::info!(
                "Auto-migrated database from {} to {} ({} bytes)",
                old_path.display(),
                new_path.display(),
                bytes
            );
            return Ok(());
        }
    }
    Ok(())
}

/// Strip the Windows verbatim (long-path) prefix `\?\` from a path string.
///
/// On Windows, `std::fs::canonicalize` returns paths with the `\?\` prefix
/// (e.g. `\?\D:\TEMP\innoforge.db`).  This prefix breaks downstream string
/// comparisons and changes directory names to uppercase.  We strip it so
/// `/api/version` returns a clean path on all platforms (issue #19).
pub fn strip_verbatim_prefix(path: &str) -> String {
    // Strip \?\ prefix (Windows long path)
    if let Some(stripped) = path.strip_prefix("\\\\?\\") {
        stripped.to_string()
    } else {
        path.to_string()
    }
}

/// 初始化应用数据库和状态（桌面端与移动端共用）。
///
/// - 解析/创建数据库路径
/// - 迁移数据库
/// - 重置 stale analyzing 状态
/// - 初始化 AppConfig 和 AppState
///
/// # Arguments
/// * `db_path` — SQLite 数据库文件路径
/// * `routes_mod` — 用于访问 `routes::AppConfig::from_db_and_env` 的路径（因为本 crate 可能有不同 module 结构）
///
/// 注意：这里直接调用 `crate::routes::*`，因为 `common` 模块在 crate 内部，
/// 而 `main.rs` 和 `lib.rs` 都以 `mod routes` 暴露了 routes 模块。
pub fn init_app_state(db_path: &str) -> anyhow::Result<crate::routes::AppState> {
    use crate::db;
    use crate::routes::{AppConfig, AppState};

    let db = db::Database::init(db_path)?;

    // 启动时将卡在 analyzing 的创意重置为 error（上次 pipeline 中断）
    match db.reset_stale_analyzing() {
        Ok(n) if n > 0 => tracing::warn!("Reset {} stale analyzing ideas to error", n),
        Ok(_) => {}
        Err(e) => tracing::error!("Failed to reset stale analyzing ideas: {}", e),
    }

    let config = AppConfig::from_db_and_env(Some(&db));
    let workspace = std::env::var_os("INNOFORGE_AIONCAD_WORKSPACE")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            db.get_setting("aioncad_workspace")
                .ok()
                .flatten()
                .map(Into::into)
        });
    let db_path_abs = std::path::Path::new(db_path)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(db_path));
    let db_parent = db_path_abs
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("data"));
    let cad = crate::cad::CadService::new(db_parent.join("cad"), workspace)?;
    let state = AppState {
        db: Arc::new(db),
        cad: Arc::new(cad),
        config: Arc::new(RwLock::new(config)),
        pipeline_channels: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        db_path: strip_verbatim_prefix(db_path_abs.to_string_lossy().as_ref()),
        listen_port: std::env::var("INNOFORGE_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3000),
    };

    // 启动日志：打印当前 AI 服务商和超时配置
    {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        let provider_label = if cfg.ai_base_url.contains("deepseek") {
            "DeepSeek"
        } else if cfg.ai_base_url.contains("googleapis") {
            "Gemini"
        } else if cfg.ai_base_url.contains("bigmodel") {
            "Zhipu"
        } else {
            "Custom"
        };
        let mode = if cfg.ai_base_url.contains("googleapis") && cfg.gemini_cli_enabled {
            "Gemini CLI"
        } else {
            "HTTP API"
        };
        tracing::info!(
            "[CONFIG] AI provider={}, mode={}, timeout={}s, base_url={}, model={}",
            provider_label,
            mode,
            crate::ai::AiClient::global_timeout_secs(),
            cfg.ai_base_url,
            cfg.ai_model,
        );
    }

    // 启动日志：打印数据库路径（与 /api/version 回显的 db_path 同源，issue #14/#16 统一口径）
    tracing::info!("[CONFIG] db_path={}", state.db_path);

    // 创建上传目录 / Create uploads directory
    let _ = std::fs::create_dir_all("data/uploads");

    // 启动管道通道超时清理
    state.spawn_channel_cleaner();

    Ok(state)
}

/// 构建统一的 axum Router（桌面端与移动端共用）。
///
/// 所有路由入口集中在本函数，确保 `main.rs` 和 `lib.rs` 同步。
pub fn build_router(state: crate::routes::AppState) -> Router {
    use crate::routes;

    Router::new()
        // 页面路由 / Page routes
        .route("/", get(routes::index_page))
        .route("/search", get(routes::search_page))
        .route("/patent/:id", get(routes::patent_detail_page))
        .route("/ai", get(routes::ai_page))
        .route("/compare", get(routes::compare_page))
        .route("/idea", get(routes::idea_page))
        .route("/settings", get(routes::settings_page))
        .route("/oa-response", get(routes::office_action_response_page))
        // FreeCAD visual artifact API
        .route("/api/cad/status", get(routes::api_cad_status))
        .route("/api/cad/start", post(routes::api_cad_start))
        .route("/api/cad/draw", post(routes::api_cad_draw))
        .route("/api/cad/artifacts", get(routes::api_cad_artifacts))
        .route(
            "/api/cad/artifacts/:id/preview",
            get(routes::api_cad_preview),
        )
        .route(
            "/api/cad/artifacts/:id/download/:format",
            get(routes::api_cad_download),
        )
        // 设置 API / Settings API
        .route("/api/version", get(routes::api_get_version))
        // OA 期限计算 —— 单一数据源 (src/oa_deadline.rs)
        .route(
            "/api/oa/deadline-table",
            get(crate::oa_deadline::deadline_table),
        )
        .route(
            "/api/oa/compute-deadline",
            get(crate::oa_deadline::compute_deadline_api),
        )
        .route("/api/settings", get(routes::api_get_settings))
        .route("/api/settings/cad", post(routes::api_save_cad_settings))
        .route("/api/settings/serpapi", post(routes::api_save_serpapi))
        .route("/api/settings/ai", post(routes::api_save_ai))
        .route(
            "/api/settings/serpapi/balance",
            get(routes::api_serpapi_balance),
        )
        // 聊天记录 API / Chat Records API
        .route("/api/chat/:session_key", get(routes::api_chat_get_messages))
        .route(
            "/api/chat/:session_key/save",
            post(routes::api_chat_save_message),
        )
        .route(
            "/api/chat/:session_key/delete",
            post(routes::api_chat_delete_messages),
        )
        // 搜索 API / Search API
        .route("/api/search", post(routes::api_search))
        .route("/api/search/stats", post(routes::api_search_stats))
        .route("/api/search/export", post(routes::api_export_csv))
        .route("/api/search/export/xlsx", post(routes::api_export_xlsx))
        .route("/api/search/online", post(routes::api_search_online))
        .route("/api/search/vector", post(routes::api_search_vector))
        .route("/api/search/analyze", post(routes::api_ai_analyze_results))
        // 专利 API / Patent API
        .route("/api/patent/fetch", post(routes::api_fetch_patent))
        .route("/api/patent/enrich/:id", get(routes::api_enrich_patent))
        .route(
            "/api/patent/enrich-free/:id",
            get(routes::api_enrich_patent_free),
        )
        .route("/api/patent/pdf/:id", get(routes::api_patent_pdf))
        .route(
            "/api/patent/pdf/extract-text",
            post(routes::api_patent_pdf_extract_text),
        )
        .route(
            "/api/patent/image-proxy",
            get(routes::api_patent_image_proxy),
        )
        .route("/api/patent/lookup/:number", get(routes::api_patent_lookup))
        .route(
            "/api/patent/lookup-or-fetch",
            post(routes::api_patent_lookup_and_fetch),
        )
        .route(
            "/api/patent/similar/:id",
            get(routes::api_recommend_similar),
        )
        .route(
            "/api/patent/:id/legal-status",
            get(routes::api_patent_legal_status),
        )
        // AI 接口 / AI API
        .route("/api/ai/chat", post(routes::api_ai_chat))
        .route("/api/ai/chat/stream", post(routes::api_ai_chat_stream))
        .route(
            "/api/ai/chat/conclusions",
            post(routes::api_ai_chat_conclusions),
        )
        // Google OAuth
        .route("/api/auth/google/url", get(routes::api_google_auth_url))
        .route(
            "/api/auth/google/callback",
            get(routes::api_google_callback),
        )
        .route(
            "/api/auth/google/exchange",
            post(routes::api_google_exchange_handler),
        )
        .route(
            "/api/auth/google/status",
            get(routes::api_google_oauth_status),
        )
        // gcloud CLI auth
        .route("/api/auth/gcloud/status", get(routes::api_gcloud_status))
        .route("/api/auth/gcloud/login", post(routes::api_gcloud_login))
        .route("/api/ai/summarize", post(routes::api_ai_summarize))
        .route("/api/ai/compare", post(routes::api_ai_compare))
        .route("/api/ai/claims", post(routes::api_ai_claims_analysis))
        .route("/api/ai/risk", post(routes::api_ai_risk_assessment))
        .route(
            "/api/ai/compare-matrix",
            post(routes::api_ai_compare_matrix),
        )
        .route(
            "/api/ai/batch-summarize",
            post(routes::api_ai_batch_summarize),
        )
        .route(
            "/api/ai/inventiveness-analysis",
            post(routes::api_ai_inventiveness_analysis),
        )
        .route(
            "/api/ai/office-action-response",
            post(routes::api_ai_office_action_response),
        )
        .route(
            "/api/ai/office-action-response/stream",
            post(routes::api_ai_office_action_response_stream),
        )
        .route(
            "/api/ai/oa-generate-response-letter",
            post(routes::api_ai_oa_generate_response_letter),
        )
        .route("/api/ai/oa-discuss", post(routes::api_ai_oa_discuss))
        .route("/api/ai/oa-fetch-refs", post(routes::api_ai_oa_fetch_refs))
        .route("/api/ai/oa-panel", post(routes::api_ai_oa_panel))
        .route(
            "/api/ai/oa-history-compare",
            post(routes::api_ai_oa_history_compare),
        )
        .route(
            "/api/ai/oa-examiner-preview",
            post(routes::api_ai_oa_examiner_preview),
        )
        .route(
            "/api/ai/oa-defense-analysis",
            post(routes::api_ai_oa_defense_analysis),
        )
        .route(
            "/api/ai/oa-strategy-recommend",
            post(routes::api_ai_oa_strategy_recommend),
        )
        .route(
            "/api/ai/oa-quality-score",
            post(routes::api_ai_oa_quality_score),
        )
        .route("/api/ai/audit-checks", post(routes::api_ai_audit_checks))
        .route(
            "/api/ai/prohibition-check",
            post(routes::api_ai_prohibition_check),
        )
        .route(
            "/api/ai/oa-claim-simulate",
            post(routes::api_ai_oa_claim_simulate),
        )
        .route(
            "/api/ai/oa-collect-evidence",
            post(routes::api_ai_oa_collect_evidence),
        )
        .route(
            "/api/ai/oa-adversarial",
            post(routes::api_ai_oa_adversarial),
        )
        // OA 分析历史 API / OA History API
        .route("/api/oa/history/all", get(routes::api_oa_history_all))
        .route(
            "/api/oa/history/:patent_number",
            get(routes::api_oa_history),
        )
        .route(
            "/api/oa/history/detail/:id",
            get(routes::api_oa_history_detail),
        )
        .route(
            "/api/oa/history/:id/delete",
            post(routes::api_oa_history_delete),
        )
        // OA 一审-二审对比分析 / OA First-Second Examination Diff
        .route(
            "/api/oa/history/:patent_number/diff",
            get(routes::api_oa_history_diff),
        )
        // OA 讨论会话列表与详情 / OA Discussion Sessions API
        .route(
            "/api/oa/discussions/:patent_number",
            get(routes::api_oa_discussion_list),
        )
        .route(
            "/api/oa/discussions/:patent_number/:discussion_id",
            get(routes::api_oa_discussion_get),
        )
        .route(
            "/api/oa/discussions/import",
            post(routes::api_oa_discussion_import),
        )
        // OA 答复书 docx 导出
        .route("/api/oa/export-docx", post(routes::api_oa_export_docx))
        // OA 分析结果服务端导出 (R10-A1)
        .route(
            "/api/oa/export",
            post(routes::oa_export::api_oa_export_post),
        )
        .route(
            "/api/oa/export/:filename",
            get(routes::oa_export::api_oa_export_get),
        )
        // OA 轮次追踪 / OA Round Workflow Tracking
        .route("/api/oa/rounds/:patent_id", get(routes::api_oa_rounds_get))
        .route("/api/oa/rounds", post(routes::api_oa_rounds_create))
        .route(
            "/api/oa/rounds/update/:id",
            put(routes::api_oa_rounds_update),
        )
        .route("/api/oa/timeline/:patent_id", get(routes::api_oa_timeline))
        .route(
            "/api/ai/check-amendments",
            post(routes::api_ai_check_amendments),
        )
        // 专利威胁评估 / Patent Threat Assessment
        .route(
            "/api/ai/threat-assessment",
            post(routes::api_ai_threat_assessment),
        )
        .route("/api/ai/claim-chart", post(routes::api_ai_claim_chart))
        .route("/api/ai/cost", get(routes::api_ai_cost_summary))
        .route("/api/ai/cost/records", get(routes::api_ai_cost_records))
        .route("/api/ai/cost/record", post(routes::api_ai_cost_save))
        // 创意验证 API / Idea API
        .route("/api/idea/submit", post(routes::api_idea_submit))
        .route("/api/idea/analyze", post(routes::api_idea_analyze))
        .route("/api/idea/pipeline", post(routes::api_idea_pipeline))
        .route(
            "/api/ideas/batch-compare",
            post(routes::api_ideas_batch_compare),
        )
        .route("/api/idea/list", get(routes::api_idea_list))
        .route("/api/idea/:id", get(routes::api_idea_get))
        .route("/api/idea/:id/delete", post(routes::api_idea_delete))
        .route("/api/idea/:id/progress", get(routes::api_idea_progress))
        .route("/api/idea/:id/resume", post(routes::api_idea_resume))
        .route(
            "/api/idea/:id/research-state",
            get(routes::api_idea_research_state).post(routes::api_idea_research_state_update),
        )
        .route("/api/idea/:id/redirect", post(routes::api_idea_redirect))
        .route("/api/idea/:id/report", get(routes::api_idea_report))
        .route(
            "/api/idea/:id/report.html",
            get(routes::api_idea_report_html),
        )
        .route("/api/ai/models", get(routes::list_ai_models))
        .route("/api/idea/:id/chat", post(routes::api_idea_chat))
        .route("/api/idea/:id/evidence", get(routes::api_idea_evidence))
        .route("/api/idea/:id/messages", get(routes::api_idea_messages))
        .route(
            "/api/idea/:id/chat/conclusions",
            get(routes::api_idea_chat_conclusions),
        )
        .route(
            "/api/idea/:id/chat/conversation",
            get(routes::api_idea_chat_conversation),
        )
        .route(
            "/api/idea/:id/summarize",
            post(routes::api_idea_summarize_discussion),
        )
        // 特征卡片 API / Feature cards API
        .route(
            "/api/ideas/:id/feature-cards",
            get(routes::api_get_feature_cards).post(routes::api_create_feature_card),
        )
        .route(
            "/api/feature-cards/diff",
            get(routes::api_feature_card_diff),
        )
        // 版本管理 + 迭代 API
        .route("/api/idea/:id/claim-tree", get(routes::api_idea_claim_tree))
        .route("/api/idea/:id/iterate", post(routes::api_idea_iterate))
        .route("/api/idea/:id/versions", get(routes::api_idea_versions))
        .route("/api/idea/:id/branches", get(routes::api_idea_branches))
        .route("/api/idea/:id/findings", get(routes::api_idea_findings))
        // 记忆 API / Memory API
        .route(
            "/api/idea/:id/memory",
            get(routes::api_idea_memory).post(routes::api_idea_memory_add),
        )
        .route(
            "/api/idea/:id/memory/:entry_id",
            delete(routes::api_idea_memory_delete),
        )
        // IPC 分类 API
        .route("/api/ipc/tree", get(routes::api_ipc_tree))
        .route("/api/ipc/:code/patents", get(routes::api_ipc_patents))
        // 导入 API
        .route("/api/patents/import", post(routes::api_import_patents))
        // 收藏夹 API
        .route(
            "/api/collections",
            get(routes::api_list_collections).post(routes::api_create_collection),
        )
        .route(
            "/api/collections/:id",
            axum::routing::delete(routes::api_delete_collection),
        )
        .route(
            "/api/collections/:id/patents",
            get(routes::api_get_collection_patents),
        )
        .route(
            "/api/collections/:id/add",
            post(routes::api_add_to_collection),
        )
        .route(
            "/api/collections/:id/remove/:patent_id",
            axum::routing::delete(routes::api_remove_from_collection),
        )
        // 标签 API
        .route(
            "/api/patents/:id/tags",
            get(routes::api_get_patent_tags).post(routes::api_add_tag),
        )
        .route(
            "/api/patents/:id/tags/:tag",
            axum::routing::delete(routes::api_remove_tag),
        )
        .route(
            "/api/patents/:id/collections",
            get(routes::api_get_patent_collections),
        )
        .route("/api/tags", get(routes::api_list_all_tags))
        // 文件上传
        .route("/api/upload/compare", post(routes::api_upload_compare))
        .route("/api/upload/extract", post(routes::api_upload_extract))
        .route("/api/upload/pdf-store", post(routes::api_upload_pdf_store))
        // 上传文件静态服务
        .nest_service("/uploads", ServeDir::new("data/uploads"))
        // 静态资源
        .route("/static/*path", get(serve_static_embedded))
        // 中间件层
        .layer(DefaultBodyLimit::max(20 * 1024 * 1024))
        .layer(
            CorsLayer::new()
                .allow_origin(cors_allowed_origins(
                    std::env::var("INNOFORGE_CORS_ORIGINS").ok().as_deref(),
                ))
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .with_state(state)
}

// ============================================================================
// 项目临时文件工具 / Project Temp File Utilities
//
// 按 AGENTS.md 规范：运行时临时文件统一放在 data/runtime-temp 下，
// 使用 UUID 文件名 + create_new 独占创建，避免系统临时目录污染和竞态。
// ============================================================================

/// 获取项目专属运行时临时目录。
/// 按 AGENTS.md 规范，不使用系统 temp 目录。
pub fn project_temp_dir() -> std::path::PathBuf {
    let base = std::path::PathBuf::from("data").join("runtime-temp");
    let _ = std::fs::create_dir_all(&base);
    base
}

/// 创建 UUID 命名的独占临时文件。
/// 使用 create_new 防止竞态（文件已存在则失败）。
pub fn new_temp_file(prefix: &str, ext: &str) -> std::io::Result<std::path::PathBuf> {
    let dir = project_temp_dir();
    let name = format!("{}_{}.{}", prefix, uuid::Uuid::new_v4(), ext);
    let path = dir.join(name);
    std::fs::File::create_new(&path)?;
    Ok(path)
}

#[cfg(test)]
mod temp_file_tests {
    use super::*;

    #[test]
    fn project_temp_dir_exists() {
        let dir = project_temp_dir();
        assert!(dir.exists());
        assert!(dir.to_string_lossy().contains("runtime-temp"));
    }

    #[test]
    fn new_temp_file_unique_and_exclusive() {
        let path1 = new_temp_file("test", "tmp").unwrap();
        let path2 = new_temp_file("test", "tmp").unwrap();
        assert_ne!(path1, path2);
        assert!(path1.exists());
        assert!(path2.exists());
        let _ = std::fs::remove_file(&path1);
        let _ = std::fs::remove_file(&path2);
    }
}

#[cfg(test)]
mod cors_tests {
    use super::*;

    fn origin_strings(origins: Vec<HeaderValue>) -> Vec<String> {
        origins
            .into_iter()
            .filter_map(|origin| origin.to_str().ok().map(str::to_owned))
            .collect()
    }

    #[test]
    fn cors_uses_local_defaults_without_configuration() {
        assert_eq!(
            origin_strings(cors_allowed_origins(None)),
            ["http://127.0.0.1:3000", "http://localhost:3000"]
        );
    }

    #[test]
    fn cors_adds_valid_configured_origins() {
        assert_eq!(
            origin_strings(cors_allowed_origins(Some(
                "https://desktop.innoforge.example, http://192.168.1.10:3000",
            ))),
            [
                "http://127.0.0.1:3000",
                "http://localhost:3000",
                "https://desktop.innoforge.example",
                "http://192.168.1.10:3000",
            ]
        );
    }

    #[test]
    fn cors_ignores_invalid_configured_origins() {
        assert_eq!(
            origin_strings(cors_allowed_origins(Some(
                "*, null, file:///tmp, https://safe.example/path, https://safe.example?query, https://safe.example#fragment, https://safe.example",
            ))),
            [
                "http://127.0.0.1:3000",
                "http://localhost:3000",
                "https://safe.example",
            ]
        );
    }
}

#[cfg(test)]
mod db_path_consistency_tests {
    use super::*;

    /// ③ 启动日志的 db_path 与 /api/version 回显的 db_path 必须是同一个字符串。
    /// 这里直接验证 AppState.db_path 等于 strip_verbatim_prefix(canonicalize(path))。
    #[tokio::test]
    async fn db_path_log_equals_api_echo() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db_path = tmp.path().to_str().unwrap();
        let state = init_app_state(db_path).unwrap();

        // Compute expected: canonicalize then strip verbatim prefix
        let canonical = std::path::Path::new(db_path)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(db_path));
        let expected = strip_verbatim_prefix(canonical.to_string_lossy().as_ref());

        assert_eq!(
            state.db_path, expected,
            "state.db_path must equal strip_verbatim_prefix(canonicalize(db_path)) — \
             this is the same value printed in startup log and returned by /api/version"
        );
    }

    /// ③ strip_verbatim_prefix preserves original case (no lowercasing)
    #[test]
    fn strip_verbatim_prefix_preserves_case() {
        // On Linux paths are case-sensitive; the function must not alter case
        let input = "/tmp/InnoForge/Data/MyDB.sqlite";
        let result = strip_verbatim_prefix(input);
        assert_eq!(result, input, "strip_verbatim_prefix must preserve case");
    }

    /// ③ strip_verbatim_prefix removes \\?\ prefix on Windows-style paths
    #[test]
    fn strip_verbatim_prefix_removes_windows_prefix() {
        let input = r"\\?\C:\Users\test\data\db.sqlite";
        let result = strip_verbatim_prefix(input);
        assert_eq!(
            result, r"C:\Users\test\data\db.sqlite",
            "strip_verbatim_prefix must remove \\\\?\\ prefix"
        );
    }
}

// ============================================================================
// 上游 HTTP 调用超时 / Upstream HTTP call timeout (issue #30)
// ============================================================================

/// 上游 HTTP 调用的超时秒数，可通过环境变量 `INNOFORGE_UPSTREAM_TIMEOUT_SECS` 配置。
/// 默认 30 秒。用于 patent.rs / auth.rs / idea.rs 中对外部 API 的 reqwest 调用。
pub fn upstream_timeout_secs() -> u64 {
    std::env::var("INNOFORGE_UPSTREAM_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&secs: &u64| secs > 0)
        .unwrap_or(30)
}

/// 构建带超时的 reqwest::Client，替代裸 `reqwest::Client::new()`。
/// 超时值由 `upstream_timeout_secs()` 决定（环境变量 `INNOFORGE_UPSTREAM_TIMEOUT_SECS`，默认 30s）。
pub fn build_upstream_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(upstream_timeout_secs()))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

// ============================================================================
// OAuth 回调地址配置 / OAuth callback URL configuration (issue #33)
// ============================================================================

/// OAuth 回调地址，可配置 / Configurable OAuth callback URL.
///
/// 优先级：
/// 1. `INNOFORGE_OAUTH_CALLBACK_URL` 环境变量（非空时直接使用）
/// 2. `INNOFORGE_PORT` 环境变量 → `http://localhost:{port}/api/auth/google/callback`
/// 3. 默认 `http://localhost:3000/api/auth/google/callback`
pub fn oauth_callback_url(listen_port: u16) -> String {
    if let Ok(url) = std::env::var("INNOFORGE_OAUTH_CALLBACK_URL") {
        if !url.is_empty() {
            return url;
        }
    }
    format!("http://localhost:{listen_port}/api/auth/google/callback")
}
