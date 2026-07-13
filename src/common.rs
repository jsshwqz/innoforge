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
    routing::{get, post},
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
    let state = AppState {
        db: Arc::new(db),
        config: Arc::new(RwLock::new(config)),
        pipeline_channels: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
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
            crate::ai::AiClient::GLOBAL_TIMEOUT_SECS,
            cfg.ai_base_url,
            cfg.ai_model,
        );
    }

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
        // 设置 API / Settings API
        .route("/api/settings", get(routes::api_get_settings))
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
        // OA 分析历史 API / OA History API
        .route("/api/oa/history/all", get(routes::api_oa_history_all))
        .route(
            "/api/oa/history/:patent_number",
            get(routes::api_oa_history),
        )
        .route(
            "/api/oa/history/detail/:id",
            get(routes::api_oa_history_get),
        )
        .route(
            "/api/oa/history/:id/delete",
            post(routes::api_oa_history_delete),
        )
        // OA 答复书 docx 导出
        .route("/api/oa/export-docx", post(routes::api_oa_export_docx))
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
        .route("/api/idea/:id/evidence", get(routes::api_idea_evidence))
        .route("/api/idea/:id/chat", post(routes::api_idea_chat))
        .route("/api/idea/:id/messages", get(routes::api_idea_messages))
        .route(
            "/api/idea/:id/chat/conclusions",
            get(routes::api_idea_chat_conclusions),
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
