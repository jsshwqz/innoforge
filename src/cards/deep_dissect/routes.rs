//! 卡片 ③ 单篇文献扒皮与深析 HTTP 路由处理器（`src/cards/deep_dissect/routes.rs`）
//!
//! 对接第五章卡片 ③ API 契约：
//! - `POST /api/deep-dissect/ingest` -> [`api_dissect_ingest`]
//! - `POST /api/deep-dissect/analyze` -> [`api_dissect_analyze`]
//! - `POST /api/deep-dissect/ask` -> [`api_dissect_ask`]

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::routes::{AppConfig, AppState};

use super::service::{
    commit_ingest_document, finalize_analyze_document, finalize_ask_document, load_analyze_context,
    load_ask_context, prepare_ingest_attachment, run_analyze_sections, run_ask_llm,
    DissectAnalyzeInput, DissectAskInput, DissectIngestInput,
};

/// `POST /api/deep-dissect/ingest` 请求体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissectIngestReq {
    pub doc_id: Option<String>,
    pub doc_kind: Option<String>,
    pub asset_role: Option<String>,
    pub filing_status: Option<String>,
    pub external_id: Option<String>,
    pub title: Option<String>,
    pub authors_or_assignees: Option<String>,
    pub pub_or_filing_date: Option<String>,
    pub venue_or_authority: Option<String>,
    pub abstract_text: Option<String>,
    pub claims_json: Option<String>,
    pub fulltext_raw: Option<String>,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub file_base64: Option<String>,
    #[serde(default)]
    pub privacy_offline_only: bool,
}

impl From<DissectIngestReq> for DissectIngestInput {
    fn from(r: DissectIngestReq) -> Self {
        Self {
            doc_id: r.doc_id,
            doc_kind: r.doc_kind,
            asset_role: r.asset_role,
            filing_status: r.filing_status,
            external_id: r.external_id,
            title: r.title,
            authors_or_assignees: r.authors_or_assignees,
            pub_or_filing_date: r.pub_or_filing_date,
            venue_or_authority: r.venue_or_authority,
            abstract_text: r.abstract_text,
            claims_json: r.claims_json,
            fulltext_raw: r.fulltext_raw,
            filename: r.filename,
            mime_type: r.mime_type,
            file_base64: r.file_base64,
            privacy_offline_only: r.privacy_offline_only,
        }
    }
}

/// `POST /api/deep-dissect/analyze` 请求体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissectAnalyzeReq {
    pub doc_id: String,
    pub mode: Option<String>,
    #[serde(default)]
    pub sub_types: Vec<String>,
    #[serde(default)]
    pub privacy_offline_only: bool,
}

impl From<DissectAnalyzeReq> for DissectAnalyzeInput {
    fn from(r: DissectAnalyzeReq) -> Self {
        Self {
            doc_id: r.doc_id,
            mode: r.mode,
            sub_types: r.sub_types,
            privacy_offline_only: r.privacy_offline_only,
        }
    }
}

/// `POST /api/deep-dissect/ask` 请求体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissectAskReq {
    pub doc_id: String,
    pub question: String,
    pub selected_excerpt: Option<String>,
    pub selected_anchor: Option<String>,
    pub figure_numeral: Option<String>,
    #[serde(default)]
    pub image_data_urls: Vec<String>,
    #[serde(default)]
    pub privacy_offline_only: bool,
}

impl From<DissectAskReq> for DissectAskInput {
    fn from(r: DissectAskReq) -> Self {
        Self {
            doc_id: r.doc_id,
            question: r.question,
            selected_excerpt: r.selected_excerpt,
            selected_anchor: r.selected_anchor,
            figure_numeral: r.figure_numeral,
            image_data_urls: r.image_data_urls,
            privacy_offline_only: r.privacy_offline_only,
        }
    }
}

fn clone_app_config(s: &AppState) -> AppConfig {
    s.config.read().map(|g| g.clone()).unwrap_or_default()
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

/// `POST /api/deep-dissect/ingest`：无截断入库或从本机金库加载单篇文献，返回结构化切片与附图部件词典
pub async fn api_dissect_ingest(
    State(s): State<AppState>,
    Json(req): Json<DissectIngestReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let cfg = clone_app_config(&s);
    let input: DissectIngestInput = req.into();

    let prepared = prepare_ingest_attachment(&cfg, &input).await.map_err(|e| {
        err_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "ingest_parse_failed",
            e.to_string(),
        )
    })?;

    let result = {
        let conn = s.db.conn();
        commit_ingest_document(&conn, input, prepared)
    }
    .map_err(|e| {
        err_response(
            StatusCode::BAD_REQUEST,
            "ingest_commit_failed",
            e.to_string(),
        )
    })?;

    Ok(Json(json!({
        "status": "ok",
        "doc": result.doc,
        "chunks": result.chunks,
        "figure_numerals": result.figure_numerals,
        "existing_feature_atoms": result.existing_feature_atoms,
        "ingest_tier_used": result.ingest_tier_used,
        "warnings": result.warnings,
    })))
}

/// `POST /api/deep-dissect/analyze`：执行专利/论文双模式结构化深析、O(1) 锚点核验与特征原子自动沉淀
pub async fn api_dissect_analyze(
    State(s): State<AppState>,
    Json(req): Json<DissectAnalyzeReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let cfg = clone_app_config(&s);
    let input: DissectAnalyzeInput = req.into();

    let ctx = {
        let conn = s.db.conn();
        load_analyze_context(&conn, &input)
    }
    .map_err(|e| err_response(StatusCode::NOT_FOUND, "doc_not_found", e.to_string()))?;

    let sections = run_analyze_sections(&cfg, &ctx).await;

    let result = {
        let conn = s.db.conn();
        finalize_analyze_document(&conn, ctx, sections)
    }
    .map_err(|e| {
        err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "analyze_failed",
            e.to_string(),
        )
    })?;

    Ok(Json(json!({
        "status": "ok",
        "doc_id": result.doc_id,
        "detected_mode": result.detected_mode,
        "sections": result.sections,
        "report_markdown": result.report_markdown,
        "verified_citations": result.verified_citations,
        "saved_feature_atoms": result.saved_feature_atoms,
        "figure_numerals": result.figure_numerals,
    })))
}

/// `POST /api/deep-dissect/ask`：基于 `hybrid_search_chunks` + 附图词典的多轮带图局部追问
pub async fn api_dissect_ask(
    State(s): State<AppState>,
    Json(req): Json<DissectAskReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let cfg = clone_app_config(&s);
    let input: DissectAskInput = req.into();

    let ctx = {
        let conn = s.db.conn();
        load_ask_context(&conn, &input)
    }
    .map_err(|e| err_response(StatusCode::BAD_REQUEST, "ask_context_failed", e.to_string()))?;

    let (answer_markdown, answer_source, provider_id) = run_ask_llm(&cfg, &ctx).await;

    let result = {
        let conn = s.db.conn();
        finalize_ask_document(&conn, ctx, answer_markdown, answer_source, provider_id)
    }
    .map_err(|e| {
        err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "ask_finalize_failed",
            e.to_string(),
        )
    })?;

    Ok(Json(json!({
        "status": "ok",
        "doc_id": result.doc_id,
        "answer_markdown": result.answer_markdown,
        "verified_citations": result.verified_citations,
        "matched_chunks": result.matched_chunks,
        "matched_numerals": result.matched_numerals,
        "answer_source": result.answer_source,
        "provider_id": result.provider_id,
    })))
}
