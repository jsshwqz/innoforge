//! 卡片 ④ 多方案对决与矩阵卡 HTTP 路由处理器（`src/cards/scheme_matrix/routes.rs`）
//!
//! 端点契约：
//! - `POST /api/scheme-matrix/compare`
//! - `GET /api/scheme-matrix/{id}`
//! - `POST /api/scheme-matrix/{id}/export`

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::cards::scheme_matrix::service::{self, SchemeSourceInput};
use crate::routes::AppState;

/// `POST /api/scheme-matrix/compare` 请求体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemeMatrixCompareReq {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    pub sources: Vec<SchemeSourceInput>,
    #[serde(default)]
    pub include_ai_synthesis: Option<bool>,
}

/// `POST /api/scheme-matrix/{id}/export` 请求体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemeMatrixExportReq {
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub acknowledge_priority_warning: Option<bool>,
}

/// `POST /api/scheme-matrix/compare`：解析 2～6 个方案，生成三重视角矩阵、Hirschberg 差异与拼凑预警并持久化
pub async fn api_matrix_compare(
    State(s): State<AppState>,
    Json(req): Json<SchemeMatrixCompareReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let include_ai = req.include_ai_synthesis.unwrap_or(false);
    let result = service::compare_schemes(&s, req.title, req.mode, req.sources, include_ai)
        .await
        .map_err(|err| {
            let msg = err.to_string();
            let status = if msg.contains("2 至 6") || msg.contains("缺少有效内容") {
                StatusCode::BAD_REQUEST
            } else if msg.contains("未在知识金库中找到") || msg.contains("未找到推演运行记录")
            {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::UNPROCESSABLE_ENTITY
            };
            (
                status,
                Json(json!({
                    "status": "error",
                    "code": "matrix_compare_failed",
                    "message": msg
                })),
            )
        })?;

    Ok(Json(json!({
        "status": "ok",
        "matrix_id": result.matrix_id,
        "matrix": result
    })))
}

/// `GET /api/scheme-matrix/{id}`：读取已持久化的多方案对决矩阵
pub async fn api_matrix_get(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let matrix = service::get_matrix(&s, &id).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "status": "error",
                "code": "matrix_load_failed",
                "message": err.to_string()
            })),
        )
    })?;

    match matrix {
        Some(m) => Ok(Json(json!({
            "status": "ok",
            "matrix_id": m.matrix_id,
            "matrix": m
        }))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "status": "error",
                "code": "matrix_not_found",
                "message": format!("未找到矩阵记录: {id}")
            })),
        )),
    }
}

/// `POST /api/scheme-matrix/{id}/export`：调用 `ForgeExporter` 导出 `.xlsx` 或 `.pdf` 文件流
pub async fn api_matrix_export(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(req): Json<SchemeMatrixExportReq>,
) -> Response {
    let ack = req.acknowledge_priority_warning.unwrap_or(false);
    match service::export_matrix(&s, &id, req.format.as_deref(), req.mode.as_deref(), ack) {
        Ok(compiled) => {
            let disposition = format!("attachment; filename=\"{}\"", compiled.filename);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, compiled.mime_type)
                .header(header::CONTENT_DISPOSITION, disposition)
                .body(Body::from(compiled.bytes))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(err) => {
            let msg = err.to_string();
            let status = if msg.contains("未找到矩阵记录") {
                StatusCode::NOT_FOUND
            } else if msg.contains("硬门禁拦截") {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(json!({
                    "status": "error",
                    "code": "matrix_export_failed",
                    "message": msg
                })),
            )
                .into_response()
        }
    }
}
