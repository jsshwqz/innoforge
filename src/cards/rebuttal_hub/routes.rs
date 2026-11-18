//! 卡片 ⑤ 三栖抗辩中心 HTTP 路由处理器（`src/cards/rebuttal_hub/routes.rs`）
//!
//! 严格对应第五章契约：
//! - `POST /api/rebuttal-hub/case`
//! - `POST /api/rebuttal-hub/case/{id}/round`
//! - `POST /api/rebuttal-hub/case/{id}/mock-review`
//! - `POST /api/rebuttal-hub/case/{id}/draft`
//! - `POST /api/rebuttal-hub/case/{id}/verify`
//! - `POST /api/rebuttal-hub/case/{id}/export`

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::routes::AppState;

use super::service::{
    add_round, create_case, run_draft, run_export, run_mock_review, run_verify, AddRoundInput,
    CreateCaseInput,
};

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

/// 创建三栖抗辩案件请求体
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RebuttalCreateCaseReq {
    pub mode: String,
    pub title: String,
    pub own_doc_id: Option<String>,
    pub reference_doc_ids: Vec<String>,
    pub experiment_ids: Vec<String>,
    pub oa_type: Option<String>,
    pub dispatch_date: Option<String>,
    pub original_claim_or_text: String,
    pub amended_claim_or_text: String,
    pub notice_or_review_text: String,
    pub knowhow_markers: Vec<String>,
}

/// 新增抗辩/返修轮次请求体
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RebuttalAddRoundReq {
    pub round_label: Option<String>,
    pub oa_type: Option<String>,
    pub dispatch_date: Option<String>,
    pub notice_text: String,
    pub strategy_used: Option<String>,
    pub draft_text: Option<String>,
    pub notes: Option<String>,
}

/// 模拟审查 / 同行评审请求体
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RebuttalMockReviewReq {
    pub focus_query: String,
}

/// 双轨策略裁决与草稿生成请求体
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RebuttalDraftReq {
    pub preferred_track: String,
    pub user_instruction: String,
}

/// 五维硬门禁核验请求体
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RebuttalVerifyReq {
    pub draft_text: Option<String>,
    pub original_claim_or_text: Option<String>,
    pub amended_claim_or_text: Option<String>,
    pub require_citations: bool,
}

/// 修订留痕 DOCX / 多格式导出请求体
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RebuttalExportReq {
    pub format: Option<String>,
    pub acknowledge_priority_warning: bool,
}

/// `POST /api/rebuttal-hub/case`：创建三栖抗辩案件（含绝限倒计时计算）
pub async fn api_rebuttal_create_case(
    State(s): State<AppState>,
    Json(req): Json<RebuttalCreateCaseReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let input = CreateCaseInput {
        mode: if req.mode.trim().is_empty() {
            "patent_oa".to_string()
        } else {
            req.mode
        },
        title: req.title,
        own_doc_id: req.own_doc_id,
        reference_doc_ids: req.reference_doc_ids,
        experiment_ids: req.experiment_ids,
        oa_type: req.oa_type,
        dispatch_date: req.dispatch_date,
        original_claim_or_text: req.original_claim_or_text,
        amended_claim_or_text: req.amended_claim_or_text,
        notice_or_review_text: req.notice_or_review_text,
        knowhow_markers: req.knowhow_markers,
    };

    let case = create_case(&s, input).map_err(|e| {
        err_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "create_failed",
            e.to_string(),
        )
    })?;

    Ok(Json(json!({
        "status": "ok",
        "case": case,
    })))
}

/// `POST /api/rebuttal-hub/case/{id}/round`：新增一通→二通 / R1→R2 轮次
pub async fn api_rebuttal_add_round(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(req): Json<RebuttalAddRoundReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let input = AddRoundInput {
        round_label: req.round_label,
        oa_type: req.oa_type,
        dispatch_date: req.dispatch_date,
        notice_text: req.notice_text,
        strategy_used: req.strategy_used,
        draft_text: req.draft_text,
        notes: req.notes,
    };

    let (case, round) = add_round(&s, &id, input)
        .map_err(|e| err_json(StatusCode::NOT_FOUND, "round_add_failed", e.to_string()))?;

    Ok(Json(json!({
        "status": "ok",
        "round": round,
        "case": case,
    })))
}

/// `POST /api/rebuttal-hub/case/{id}/mock-review`：生成高强度模拟审查 / 审稿质疑清单
pub async fn api_rebuttal_mock_review(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(req): Json<RebuttalMockReviewReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (case, issues, provider_used, offline_fallback) =
        run_mock_review(&s, &id, &req.focus_query)
            .await
            .map_err(|e| err_json(StatusCode::BAD_REQUEST, "mock_review_failed", e.to_string()))?;

    Ok(Json(json!({
        "status": "ok",
        "case_id": case.case_id,
        "mode": case.mode,
        "issues": issues,
        "provider_used": provider_used,
        "offline_fallback": offline_fallback,
        "deadline_info": case.deadline_info,
    })))
}

/// `POST /api/rebuttal-hub/case/{id}/draft`：双轨策略裁决与绑定原文/实验锚点答复草稿生成
pub async fn api_rebuttal_draft(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(req): Json<RebuttalDraftReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (case, draft) = run_draft(&s, &id, &req.preferred_track, &req.user_instruction)
        .await
        .map_err(|e| err_json(StatusCode::BAD_REQUEST, "draft_failed", e.to_string()))?;

    Ok(Json(json!({
        "status": "ok",
        "case_id": case.case_id,
        "mode": case.mode,
        "draft": draft,
    })))
}

/// `POST /api/rebuttal-hub/case/{id}/verify`：五维硬阻断门禁、第33条/论文一致性与跨实施例拼凑核验
pub async fn api_rebuttal_verify(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(req): Json<RebuttalVerifyReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (case, outcome) = run_verify(
        &s,
        &id,
        req.draft_text.as_deref(),
        req.original_claim_or_text.as_deref(),
        req.amended_claim_or_text.as_deref(),
        req.require_citations,
    )
    .map_err(|e| err_json(StatusCode::BAD_REQUEST, "verify_failed", e.to_string()))?;

    Ok(Json(json!({
        "status": "ok",
        "case_id": case.case_id,
        "mode": outcome.mode,
        "gate_report": outcome.gate_report,
        "scope_report": outcome.scope_report,
        "stitching_alerts": outcome.stitching_alerts,
        "priority_check": outcome.priority_check,
    })))
}

/// `POST /api/rebuttal-hub/case/{id}/export`：导出带 `<w:del>`/`<w:ins>` Word 修订留痕的 `.docx` 答复包
pub async fn api_rebuttal_export(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(req): Json<RebuttalExportReq>,
) -> Response {
    match run_export(
        &s,
        &id,
        req.format.as_deref(),
        req.acknowledge_priority_warning,
    ) {
        Ok(compiled) => {
            let disposition = format!("attachment; filename=\"{}\"", compiled.filename);
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, compiled.mime_type),
                    (header::CONTENT_DISPOSITION, disposition),
                ],
                compiled.bytes,
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "status": "error",
                "code": "export_blocked_or_failed",
                "message": e.to_string(),
            })),
        )
            .into_response(),
    }
}
