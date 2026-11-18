//! 卡片 ① 全域情报检索 HTTP 路由处理器（`src/cards/omni_search/routes.rs`）
//!
//! 严格对齐第五章卡片 ① 接口契约与 `gen_api_contracts.js` 静态契约抽取规范：
//! - `POST /api/omni-search/query`
//! - `POST /api/omni-search/chunks`
//! - `GET /api/omni-search/doc/{id}`
//! - `POST /api/omni-search/export`

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::core::vault::models::{
    AssetRole, KnowledgeDoc, OmniSearchHit, SearchRealm, SearchSceneMode,
};
use crate::routes::AppState;

use super::service;

/// `POST /api/omni-search/query` 请求体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmniSearchQueryReq {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub realms: Vec<SearchRealm>,
    #[serde(default)]
    pub asset_role: Option<AssetRole>,
    #[serde(default)]
    pub enable_ai_rewrite: bool,
    #[serde(default)]
    pub save_to_vault: bool,
    #[serde(default)]
    pub max_per_source: Option<usize>,
    /// 可选：单篇命中结果一键打标入库（如 `[我方已有成果]`）
    #[serde(default)]
    pub save_doc: Option<KnowledgeDoc>,
}

/// `POST /api/omni-search/chunks` 请求体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmniSearchChunksReq {
    pub query: String,
    #[serde(default)]
    pub mode: Option<SearchSceneMode>,
    #[serde(default)]
    pub doc_ids: Option<Vec<String>>,
    #[serde(default)]
    pub asset_role: Option<AssetRole>,
    #[serde(default)]
    pub top_k: Option<usize>,
    #[serde(default)]
    pub char_budget: Option<usize>,
}

/// `POST /api/omni-search/export` 请求体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmniSearchExportReq {
    #[serde(default)]
    pub query: String,
    #[serde(default = "default_export_format")]
    pub format: String,
    #[serde(default)]
    pub realms: Vec<SearchRealm>,
    #[serde(default)]
    pub asset_role: Option<AssetRole>,
    #[serde(default)]
    pub hits: Vec<OmniSearchHit>,
}

fn default_export_format() -> String {
    "csv".to_string()
}

fn err_response(
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(json!({
            "status": "error",
            "code": code,
            "message": message.into(),
        })),
    )
}

/// `POST /api/omni-search/query`：四维全域情报检索（本地金库 + 在线四维源 + 可选入库）
pub async fn api_omni_search_query(
    State(s): State<AppState>,
    Json(req): Json<OmniSearchQueryReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if let Some(doc) = req.save_doc {
        let role = req.asset_role.unwrap_or(AssetRole::OwnAsset);
        let saved = service::save_single_doc_to_vault(&s, doc, role).map_err(|e| {
            err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "vault_save_failed",
                format!("入库打标失败: {e}"),
            )
        })?;
        return Ok(Json(json!({
            "status": "ok",
            "saved_count": 1,
            "saved_doc": saved,
        })));
    }

    if req.query.trim().is_empty() {
        return Err(err_response(
            StatusCode::BAD_REQUEST,
            "empty_query",
            "检索关键词不能为空 / Query cannot be empty",
        ));
    }

    let max_per_source = req.max_per_source.unwrap_or(10);
    let outcome = service::execute_omni_search(
        &s,
        &req.query,
        &req.realms,
        req.asset_role,
        req.enable_ai_rewrite,
        req.save_to_vault,
        max_per_source,
    )
    .await
    .map_err(|e| {
        err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "omni_search_failed",
            format!("全域检索执行失败: {e}"),
        )
    })?;

    Ok(Json(json!({
        "status": "ok",
        "plan": outcome.plan,
        "hits": outcome.hits,
        "diagnostics": outcome.diagnostics,
        "total_before_dedup": outcome.total_before_dedup,
        "saved_count": outcome.saved_count,
    })))
}

/// `POST /api/omni-search/chunks`：调用 `VaultService::hybrid_search_chunks` 返回 RRF 段落命中与父权项链上下文
pub async fn api_omni_search_chunks(
    State(s): State<AppState>,
    Json(req): Json<OmniSearchChunksReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if req.query.trim().is_empty() {
        return Err(err_response(
            StatusCode::BAD_REQUEST,
            "empty_query",
            "段落检索词不能为空 / Chunk query cannot be empty",
        ));
    }

    let mode = req.mode.unwrap_or(SearchSceneMode::PatentCompare);
    let top_k = req.top_k.unwrap_or(10);
    let char_budget = req.char_budget.unwrap_or(8000);

    let bundle = service::search_hybrid_chunks(
        &s,
        &req.query,
        mode,
        req.doc_ids.as_deref(),
        req.asset_role,
        top_k,
        char_budget,
    )
    .map_err(|e| {
        err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "hybrid_chunk_search_failed",
            format!("混合段落召回失败: {e}"),
        )
    })?;

    Ok(Json(json!({
        "status": "ok",
        "bundle": bundle,
    })))
}

/// `GET /api/omni-search/doc/{id}`：返回完整 `KnowledgeDoc` + `doc_chunks` 权项树/段落 + `figure_numerals` 附图部件词典
pub async fn api_omni_search_doc(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let payload = service::get_doc_with_structure(&s, &id).map_err(|e| {
        err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "doc_load_failed",
            format!("读取文献结构失败: {e}"),
        )
    })?;

    let Some(data) = payload else {
        return Err(err_response(
            StatusCode::NOT_FOUND,
            "doc_not_found",
            format!("未找到文献 {id} / Document not found"),
        ));
    };

    Ok(Json(json!({
        "status": "ok",
        "doc": data.doc,
        "doc_chunks": data.doc_chunks,
        "figure_numerals": data.figure_numerals,
    })))
}

/// `POST /api/omni-search/export`：导出 `csv` / `xlsx` / `pdf` 文件流
pub async fn api_omni_search_export(
    State(s): State<AppState>,
    Json(req): Json<OmniSearchExportReq>,
) -> Response {
    match service::export_omni_hits(
        &s,
        &req.query,
        &req.format,
        &req.realms,
        req.asset_role,
        &req.hits,
    )
    .await
    {
        Ok(file) => {
            let mut resp = Response::new(Body::from(file.bytes));
            *resp.status_mut() = StatusCode::OK;
            if let Ok(ct) = HeaderValue::from_str(&file.content_type) {
                resp.headers_mut().insert(header::CONTENT_TYPE, ct);
            }
            if let Ok(cd) =
                HeaderValue::from_str(&format!("attachment; filename=\"{}\"", file.filename))
            {
                resp.headers_mut().insert(header::CONTENT_DISPOSITION, cd);
            }
            resp
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "status": "error",
                "code": "export_failed",
                "message": format!("导出失败: {err}"),
            })),
        )
            .into_response(),
    }
}
