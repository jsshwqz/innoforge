//! 卡片 ② 多模态想法共创与可分叉推演卡 HTTP 路由（`src/cards/idea_studio/routes.rs`）
//!
//! 严格对应第五章契约：
//! - `POST /api/idea-studio/session`
//! - `POST /api/idea-studio/session/{id}/turn`
//! - `POST /api/idea-studio/run/start`
//! - `POST /api/idea-studio/run/{id}/fork`
//! - `POST /api/idea-studio/run/{id}/checkpoint`
//! - `POST /api/idea-studio/run/{id}/experiment`
//! - `GET  /api/idea-studio/run/{id}/events`

use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures::stream;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::cards::idea_studio::service::{self, ExperimentSaveInput};
use crate::core::multimodal::{CocreationTurnInput, IngestedAttachment, SchemeBaselineCard};
use crate::core::vault::models::SseEnvelopeEvent;
use crate::routes::AppState;

/// 创建多模态共创会话请求体
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StudioCreateSessionReq {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub mounted_own_doc_ids: Vec<String>,
}

/// 推进一轮多模态共创请求体
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StudioTurnReq {
    #[serde(default)]
    pub user_text: String,
    #[serde(default)]
    pub attachments: Vec<IngestedAttachment>,
    #[serde(default)]
    pub mounted_own_doc_ids: Vec<String>,
    #[serde(default)]
    pub lock_baseline: bool,
}

fn default_tier_mode() -> String {
    "quick_scout".to_string()
}

/// 启动后舱可分叉推演流水线请求体
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StudioRunStartReq {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default = "default_tier_mode")]
    pub tier_mode: String,
    #[serde(default)]
    pub scheme_baseline: Option<SchemeBaselineCard>,
    #[serde(default)]
    pub mounted_own_doc_ids: Vec<String>,
}

/// 从指定步骤分叉对比路线 B 请求体
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StudioRunForkReq {
    pub fork_from_step: String,
    #[serde(default)]
    pub override_baseline: Option<SchemeBaselineCard>,
    #[serde(default)]
    pub route_label: Option<String>,
}

/// 提交人工卡点微调确认请求体
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StudioCheckpointReq {
    #[serde(default)]
    pub step_key: String,
    #[serde(default)]
    pub confirmed_output: serde_json::Value,
}

/// 回填对照实验真实测量数据请求体
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StudioExperimentReq {
    #[serde(default)]
    pub exp_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub hypothesis: String,
    #[serde(default)]
    pub control_group_spec: String,
    #[serde(default)]
    pub test_group_spec: String,
    #[serde(default)]
    pub sandbox_output_json: Option<serde_json::Value>,
    pub measured_data_json: serde_json::Value,
    #[serde(default)]
    pub verified: bool,
}

fn err_resp(
    status: StatusCode,
    code: &str,
    message: String,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(json!({
            "status": "error",
            "code": code,
            "message": message,
        })),
    )
}

fn encode_sse_event(ev: &SseEnvelopeEvent) -> Event {
    let val = serde_json::to_value(ev).unwrap_or_else(|_| json!({}));
    let event_name = val
        .get("event")
        .and_then(|v| v.as_str())
        .unwrap_or("stage_update");
    let data_str = val
        .get("data")
        .and_then(|d| serde_json::to_string(d).ok())
        .unwrap_or_else(|| "{}".to_string());
    Event::default().event(event_name).data(data_str)
}

/// `POST /api/idea-studio/session`：创建多模态共创会话
pub async fn api_studio_create_session(
    State(s): State<AppState>,
    Json(req): Json<StudioCreateSessionReq>,
) -> Json<serde_json::Value> {
    match service::create_session(&s, req.title, req.mounted_own_doc_ids).await {
        Ok(session) => Json(json!({
            "status": "ok",
            "session_id": session.session_id,
            "session": session,
        })),
        Err(e) => Json(json!({
            "status": "error",
            "code": "create_session_failed",
            "message": e.to_string(),
        })),
    }
}

/// `POST /api/idea-studio/session/{id}/turn`：处理图文附件轮次并返回启发式追问与增量更新后的 `SchemeBaselineCard`
pub async fn api_studio_turn(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(req): Json<StudioTurnReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let input = CocreationTurnInput {
        user_text: req.user_text,
        attachments: req.attachments,
        mounted_own_doc_ids: req.mounted_own_doc_ids,
    };

    let (output, session) = service::advance_turn(&s, &id, input, req.lock_baseline)
        .await
        .map_err(|e| err_resp(StatusCode::BAD_REQUEST, "turn_failed", e.to_string()))?;

    Ok(Json(json!({
        "status": "ok",
        "session_id": id,
        "reply_message": output.reply_message,
        "follow_up_questions": output.follow_up_questions,
        "baseline": output.updated_baseline,
        "isolated_prompt": output.isolated_prompt,
        "session": session,
    })))
}

/// `POST /api/idea-studio/run/start`：按 `QuickScout / RdExperiment / FullProtectionRebuttal` 三档与挂载的 `mounted_own_doc_ids` 启动 `DagRunner` 并落库 `dag_runs`
pub async fn api_studio_run_start(
    State(s): State<AppState>,
    Json(req): Json<StudioRunStartReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (run, nodes) = service::start_run(
        &s,
        req.session_id.as_deref(),
        &req.tier_mode,
        req.scheme_baseline,
        req.mounted_own_doc_ids,
    )
    .await
    .map_err(|e| err_resp(StatusCode::BAD_REQUEST, "run_start_failed", e.to_string()))?;

    Ok(Json(json!({
        "status": "ok",
        "run_id": run.run_id,
        "run": run,
        "nodes": nodes,
    })))
}

/// `POST /api/idea-studio/run/{id}/fork`：从任意步骤 `fork_from_step` 分叉对比路线 B，返回父子分叉谱系
pub async fn api_studio_run_fork(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(req): Json<StudioRunForkReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (child_run, nodes, sibling_forks) = service::fork_run(
        &s,
        &id,
        &req.fork_from_step,
        req.override_baseline,
        req.route_label,
    )
    .await
    .map_err(|e| err_resp(StatusCode::BAD_REQUEST, "run_fork_failed", e.to_string()))?;

    Ok(Json(json!({
        "status": "ok",
        "run_id": child_run.run_id,
        "parent_run_id": id,
        "fork_from_step": req.fork_from_step,
        "run": child_run,
        "nodes": nodes,
        "genealogy": {
            "parent_run_id": id,
            "child_forks": sibling_forks,
        }
    })))
}

/// `POST /api/idea-studio/run/{id}/checkpoint`：提交人工卡点微调确认恢复运行
pub async fn api_studio_run_checkpoint(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(req): Json<StudioCheckpointReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (run, nodes) = service::resume_checkpoint(&s, &id, &req.step_key, req.confirmed_output)
        .await
        .map_err(|e| {
            err_resp(
                StatusCode::BAD_REQUEST,
                "checkpoint_resume_failed",
                e.to_string(),
            )
        })?;

    Ok(Json(json!({
        "status": "ok",
        "run_id": run.run_id,
        "run": run,
        "nodes": nodes,
    })))
}

/// `POST /api/idea-studio/run/{id}/experiment`：回填真实测量数据调用 `VaultService::save_and_verify_experiment`，返回 `Exp:{exp_id}` 锚点
pub async fn api_studio_run_experiment(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(req): Json<StudioExperimentReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let input = ExperimentSaveInput {
        exp_id: req.exp_id,
        title: req.title,
        hypothesis: req.hypothesis,
        control_group_spec: req.control_group_spec,
        test_group_spec: req.test_group_spec,
        sandbox_output_json: req.sandbox_output_json,
        measured_data_json: req.measured_data_json,
        verified: req.verified,
    };

    let (experiment, anchor) = service::record_experiment(&s, &id, input)
        .await
        .map_err(|e| {
            err_resp(
                StatusCode::BAD_REQUEST,
                "experiment_save_failed",
                e.to_string(),
            )
        })?;

    Ok(Json(json!({
        "status": "ok",
        "run_id": id,
        "exp_id": experiment.exp_id,
        "evidence_anchor": anchor,
        "experiment": experiment,
    })))
}

/// `GET /api/idea-studio/run/{id}/events`：输出统一 `SseEnvelopeEvent` 信封流
pub async fn api_studio_run_events(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let (initial, rx) = service::subscribe_run_events(&s, &id);

    let initial_stream = stream::iter(
        initial
            .into_iter()
            .map(|ev| Ok::<Event, Infallible>(encode_sse_event(&ev))),
    );

    let live_stream = stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let is_terminal = matches!(
                        ev,
                        SseEnvelopeEvent::Completed { .. } | SseEnvelopeEvent::Error { .. }
                    );
                    let sse_ev = encode_sse_event(&ev);
                    if is_terminal {
                        return Some((Ok::<Event, Infallible>(sse_ev), rx));
                    }
                    return Some((Ok::<Event, Infallible>(sse_ev), rx));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });

    let combined = futures::StreamExt::chain(initial_stream, live_stream);
    Sse::new(combined).keep_alive(KeepAlive::default())
}
