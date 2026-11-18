//! 卡片 ⑥ HTTP 路由处理器（`src/cards/deliverable_forge/routes.rs`）
//!
//! - `POST /api/deliverable-forge/priority-check`
//! - `POST /api/deliverable-forge/compile`
//! - `POST /api/deliverable-forge/export`

use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::cards::deliverable_forge::service::{
    CompilePreviewInput, DeliverableForgeService, ExportDeliverableInput,
};
use crate::core::vault::models::{DeliverableFormat, DeliverableGenre};
use crate::routes::AppState;

/// 公开时序冲突守卫检查请求体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgePriorityCheckReq {
    #[serde(default)]
    pub related_doc_ids: Vec<String>,
    pub target_genre: DeliverableGenre,
    #[serde(default)]
    pub knowhow_markers: Vec<String>,
    #[serde(default)]
    pub draft_text: String,
}

/// 一源四出分节编译预览请求体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeCompileReq {
    pub title: String,
    pub genre: DeliverableGenre,
    #[serde(default)]
    pub related_doc_ids: Vec<String>,
    #[serde(default)]
    pub reference_doc_ids: Vec<String>,
    #[serde(default)]
    pub experiment_ids: Vec<String>,
    #[serde(default)]
    pub knowhow_markers: Vec<String>,
    #[serde(default)]
    pub custom_notes: String,
    #[serde(default)]
    pub use_ai: bool,
}

/// 一源四出成品导出请求体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeExportReq {
    pub title: String,
    pub genre: DeliverableGenre,
    pub format: DeliverableFormat,
    #[serde(default)]
    pub sections: Vec<(String, String)>,
    #[serde(default)]
    pub original_text_for_diff: Option<String>,
    #[serde(default)]
    pub amended_text_for_diff: Option<String>,
    #[serde(default)]
    pub related_doc_ids: Vec<String>,
    #[serde(default)]
    pub reference_doc_ids: Vec<String>,
    #[serde(default)]
    pub knowhow_markers: Vec<String>,
    #[serde(default)]
    pub acknowledge_priority_warning: bool,
    #[serde(default)]
    pub bundle_zip: bool,
}

fn err_json(
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

/// `POST /api/deliverable-forge/priority-check`：检查「先专利申请日后公开论文/开源」时序冲突并做 Know-how 脱敏预览
pub async fn api_forge_priority_check(
    State(s): State<AppState>,
    Json(req): Json<ForgePriorityCheckReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let conn = s.db.conn();
    let (priority_check, redacted_text) = DeliverableForgeService::check_priority(
        &conn,
        &req.related_doc_ids,
        req.target_genre,
        &req.knowhow_markers,
        &req.draft_text,
    )
    .map_err(|e| {
        err_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "priority_check_failed",
            format!("公开时序守卫检测失败: {e}"),
        )
    })?;

    Ok(Json(json!({
        "status": "ok",
        "priority_check": priority_check,
        "redacted_preview": redacted_text,
    })))
}

/// `POST /api/deliverable-forge/compile`：编译四体裁分节可核验预览并计算五维门禁评分
pub async fn api_forge_compile(
    State(s): State<AppState>,
    Json(req): Json<ForgeCompileReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if req.title.trim().is_empty() {
        return Err(err_json(
            StatusCode::BAD_REQUEST,
            "empty_title",
            "成果标题不能为空 / Deliverable title is required",
        ));
    }

    let cfg = s.config.read().map(|g| g.clone()).unwrap_or_default();
    let input = CompilePreviewInput {
        title: req.title.trim().to_string(),
        genre: req.genre,
        related_doc_ids: req.related_doc_ids,
        reference_doc_ids: req.reference_doc_ids,
        experiment_ids: req.experiment_ids,
        knowhow_markers: req.knowhow_markers,
        custom_notes: req.custom_notes,
        use_ai: req.use_ai,
    };

    let output = DeliverableForgeService::compile_preview(&s.db, &cfg, &input)
        .await
        .map_err(|e| {
            err_json(
                StatusCode::UNPROCESSABLE_ENTITY,
                "compile_failed",
                format!("成果编译预览失败: {e}"),
            )
        })?;

    Ok(Json(json!({
        "status": "ok",
        "preview": output,
    })))
}

/// `POST /api/deliverable-forge/export`：执行门禁与时序守卫后导出 `.docx` / `.md` / `.tex`（或含 `.bib` 的 `.zip`）/ `.pdf`
pub async fn api_forge_export(
    State(s): State<AppState>,
    Json(req): Json<ForgeExportReq>,
) -> Response {
    if req.title.trim().is_empty() {
        return err_json(
            StatusCode::BAD_REQUEST,
            "empty_title",
            "成果标题不能为空 / Deliverable title is required",
        )
        .into_response();
    }
    if req.sections.is_empty() {
        return err_json(
            StatusCode::BAD_REQUEST,
            "empty_sections",
            "请先编译或提供至少一个章节正文 / At least one section is required",
        )
        .into_response();
    }

    let input = ExportDeliverableInput {
        title: req.title.trim().to_string(),
        genre: req.genre,
        format: req.format,
        sections: req.sections,
        original_text_for_diff: req.original_text_for_diff,
        amended_text_for_diff: req.amended_text_for_diff,
        related_doc_ids: req.related_doc_ids,
        reference_doc_ids: req.reference_doc_ids,
        knowhow_markers: req.knowhow_markers,
        acknowledge_priority_warning: req.acknowledge_priority_warning,
        bundle_zip: req.bundle_zip,
    };

    let compiled = {
        let conn = s.db.conn();
        DeliverableForgeService::export_deliverable(&conn, &input)
    };

    match compiled {
        Ok(out) => {
            let safe_name = out.filename.replace(['"', '\\', '\r', '\n'], "_");
            let disposition = format!("attachment; filename=\"{}\"", safe_name);
            let mut resp = (StatusCode::OK, out.bytes).into_response();
            if let Ok(ct) = HeaderValue::from_str(&out.mime_type) {
                resp.headers_mut().insert(header::CONTENT_TYPE, ct);
            }
            if let Ok(cd) = HeaderValue::from_str(&disposition) {
                resp.headers_mut().insert(header::CONTENT_DISPOSITION, cd);
            }
            resp
        }
        Err(e) => err_json(
            StatusCode::UNPROCESSABLE_ENTITY,
            "export_blocked",
            format!("成果导出被拦截或失败: {e}"),
        )
        .into_response(),
    }
}
