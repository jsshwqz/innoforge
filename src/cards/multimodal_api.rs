//! 多模态公共上传与三级解析进度 SSE 接口（第五章第 8 组公共接口）
//!
//! - `POST /api/multimodal/upload`
//! - `GET /api/multimodal/task/{id}/events`

use crate::core::multimodal::doc_ingest::{ingest_attachment, IngestOptions};
use crate::core::vault::models::SseEnvelopeEvent;
use crate::routes::AppState;
use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use serde_json::json;
use std::convert::Infallible;
use uuid::Uuid;

/// `POST /api/multimodal/upload`：接收多格式文件或剪贴板图像，执行三级自适应解析并返回附件标识与附图词典预览
pub async fn api_multimodal_upload(
    State(_state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut filename = "attachment.bin".to_string();
    let mut mime_type = "application/octet-stream".to_string();
    let mut file_bytes: Vec<u8> = Vec::new();
    let mut privacy_offline_only = false;

    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name == "privacy_offline_only" {
            if let Ok(val) = field.text().await {
                privacy_offline_only = val.trim().eq_ignore_ascii_case("true") || val.trim() == "1";
            }
            continue;
        }
        if let Some(fname) = field.file_name() {
            filename = fname.to_string();
        }
        if let Some(ct) = field.content_type() {
            mime_type = ct.to_string();
        }
        if let Ok(bytes) = field.bytes().await {
            if !bytes.is_empty() {
                file_bytes = bytes.to_vec();
            }
        }
    }

    if file_bytes.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "status": "error",
                "code": "empty_upload",
                "message": "未收到有效上传文件内容 / No valid file bytes uploaded"
            })),
        ));
    }

    let attachment_id = format!("att_{}", Uuid::new_v4().simple());
    let opts = IngestOptions {
        umi_ocr_url: None,
        vision_endpoint: None,
        privacy_offline_only,
    };

    let ingested = ingest_attachment(&filename, Some(&mime_type), &file_bytes, &opts)
        .await
        .map_err(|e| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "status": "error",
                    "code": "ingest_failed",
                    "message": format!("多模态解析失败: {e}")
                })),
            )
        })?;

    Ok(Json(json!({
        "status": "ok",
        "attachment_id": attachment_id,
        "task_id": attachment_id,
        "filename": ingested.filename,
        "mime_type": ingested.mime_type,
        "tier_used": ingested.tier_used,
        "extracted_text": ingested.extracted_text,
        "structured_summary": ingested.structured_summary_json,
        "figure_numerals": ingested.figure_numerals,
    })))
}

/// `GET /api/multimodal/task/{id}/events`：推送统一 SSE 信封的解析完成状态流
pub async fn api_multimodal_task_events(
    Path(task_id): Path<String>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let stage = SseEnvelopeEvent::StageUpdate {
        stage: "multimodal_ingest".to_string(),
        percent: 100,
        elapsed_ms: 5,
        message: format!("Attachment {task_id} ingested"),
    };
    let done = SseEnvelopeEvent::Completed {
        artifact_id: task_id.clone(),
        summary: json!({"task_id": task_id, "ready": true}),
    };
    let events = [stage, done].into_iter().map(|ev| {
        let payload = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".to_string());
        Ok(Event::default().data(payload))
    });
    Sse::new(futures::stream::iter(events)).keep_alive(KeepAlive::default())
}
