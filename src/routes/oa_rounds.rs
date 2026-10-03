//! 多轮答复全流程追踪 / OA Round Workflow Tracking
//!
//! 追踪一件专利从一审→二审→复审→授权/驳回的完整流程。
//! 每轮记录 OA 内容、答复文本、审查结果，形成时间线。

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use super::AppState;

/// GET /api/oa/rounds/:patent_id
///
/// 获取某专利的所有 OA 轮次，按轮次号正序返回。
pub async fn api_oa_rounds_get(
    Path(patent_id): Path<String>,
    State(s): State<AppState>,
) -> Json<Value> {
    match s.db.list_oa_rounds(&patent_id) {
        Ok(rounds) => Json(json!({
            "status": "ok",
            "patent_id": patent_id,
            "rounds": rounds,
            "count": rounds.len()
        })),
        Err(e) => Json(json!({
            "status": "error",
            "message": format!("Failed to list OA rounds: {e}")
        })),
    }
}

/// POST /api/oa/rounds
///
/// 新增一轮 OA。请求体：
/// ```json
/// { "patent_id": "...", "round_type": "first_rejection", "oa_date": "2026-01-01",
///   "oa_text": "...", "notes": "..." }
/// ```
/// round_number 自动计算（当前最大 +1）。
pub async fn api_oa_rounds_create(
    State(s): State<AppState>,
    Json(req): Json<Value>,
) -> Json<Value> {
    let patent_id = match req.get("patent_id").and_then(|v| v.as_str()) {
        Some(v) => v.to_string(),
        None => {
            return Json(json!({
                "status": "error",
                "message": "Missing required field: patent_id"
            }))
        }
    };

    let round_type = match req.get("round_type").and_then(|v| v.as_str()) {
        Some(v) => v.to_string(),
        None => {
            return Json(json!({
                "status": "error",
                "message": "Missing required field: round_type"
            }))
        }
    };

    // 自动计算轮次号
    let round_number = match s.db.next_round_number(&patent_id) {
        Ok(n) => n,
        Err(e) => {
            return Json(json!({
                "status": "error",
                "message": format!("Failed to compute round number: {e}")
            }))
        }
    };

    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();
    let id = format!("oa_round_{}_{}", now.timestamp_millis(), round_number);

    let round = crate::db::oa::OaRound {
        id: id.clone(),
        patent_id: patent_id.clone(),
        round_number,
        round_type,
        oa_date: req
            .get("oa_date")
            .and_then(|v| v.as_str())
            .map(String::from),
        oa_text: req
            .get("oa_text")
            .and_then(|v| v.as_str())
            .map(String::from),
        response_date: req
            .get("response_date")
            .and_then(|v| v.as_str())
            .map(String::from),
        response_text: req
            .get("response_text")
            .and_then(|v| v.as_str())
            .map(String::from),
        result: req.get("result").and_then(|v| v.as_str()).map(String::from),
        result_date: req
            .get("result_date")
            .and_then(|v| v.as_str())
            .map(String::from),
        strategy_used: req
            .get("strategy_used")
            .and_then(|v| v.as_str())
            .map(String::from),
        quality_score: req.get("quality_score").and_then(|v| v.as_f64()),
        notes: req.get("notes").and_then(|v| v.as_str()).map(String::from),
        created_at: now_str.clone(),
        updated_at: now_str,
    };

    match s.db.save_oa_round(&round) {
        Ok(()) => Json(json!({
            "status": "ok",
            "round": round,
            "message": "OA round created"
        })),
        Err(e) => Json(json!({
            "status": "error",
            "message": format!("Failed to save OA round: {e}")
        })),
    }
}

/// PUT /api/oa/rounds/:id
///
/// 更新某轮 OA 的字段。所有字段可选，仅更新提供的字段。
/// ```json
/// { "oa_text": "...", "response_text": "...", "result": "accepted", ... }
/// ```
pub async fn api_oa_rounds_update(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(req): Json<Value>,
) -> Json<Value> {
    // 先确认记录存在
    match s.db.get_oa_round(&id) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Json(json!({
                "status": "error",
                "message": "OA round not found"
            }))
        }
        Err(e) => {
            return Json(json!({
                "status": "error",
                "message": format!("Database error: {e}")
            }))
        }
    }

    let oa_date = req.get("oa_date").and_then(|v| v.as_str());
    let oa_text = req.get("oa_text").and_then(|v| v.as_str());
    let response_date = req.get("response_date").and_then(|v| v.as_str());
    let response_text = req.get("response_text").and_then(|v| v.as_str());
    let result = req.get("result").and_then(|v| v.as_str());
    let result_date = req.get("result_date").and_then(|v| v.as_str());
    let strategy_used = req.get("strategy_used").and_then(|v| v.as_str());
    let quality_score = req.get("quality_score").and_then(|v| v.as_f64());
    let notes = req.get("notes").and_then(|v| v.as_str());

    match s.db.update_oa_round(
        &id,
        oa_date,
        oa_text,
        response_date,
        response_text,
        result,
        result_date,
        strategy_used,
        quality_score,
        notes,
    ) {
        Ok(rows) => {
            if rows == 0 {
                return Json(json!({
                    "status": "error",
                    "message": "No rows updated"
                }));
            }
            // 返回更新后的记录
            match s.db.get_oa_round(&id) {
                Ok(Some(round)) => Json(json!({
                    "status": "ok",
                    "round": round,
                    "message": "OA round updated"
                })),
                _ => Json(json!({
                    "status": "ok",
                    "message": "OA round updated"
                })),
            }
        }
        Err(e) => Json(json!({
            "status": "error",
            "message": format!("Failed to update OA round: {e}")
        })),
    }
}

/// GET /api/oa/timeline/:patent_id
///
/// 获取时间线视图数据。返回所有轮次及其关联的 OA 分析记录，
/// 供前端渲染横向时间线。
pub async fn api_oa_timeline(
    Path(patent_id): Path<String>,
    State(s): State<AppState>,
) -> Json<Value> {
    let rounds = s.db.list_oa_rounds(&patent_id).unwrap_or_default();
    let analyses = s.db.list_oa_analyses(&patent_id).unwrap_or_default();

    // 为每轮附加分析记录
    let timeline: Vec<Value> = rounds
        .iter()
        .map(|round| {
            // 找到该轮次对应的分析记录（按 oa_type 匹配）
            let related_analyses: Vec<&crate::db::oa::OaAnalysis> = analyses
                .iter()
                .filter(|a| a.oa_type == round.round_type)
                .collect();

            json!({
                "round": round,
                "analyses": related_analyses,
                "has_response": round.response_text.is_some(),
                "has_result": round.result.is_some(),
            })
        })
        .collect();

    // 统计摘要
    let total_rounds = rounds.len();
    let completed = rounds.iter().filter(|r| r.result.is_some()).count();
    let pending = total_rounds - completed;

    Json(json!({
        "status": "ok",
        "patent_id": patent_id,
        "timeline": timeline,
        "summary": {
            "total_rounds": total_rounds,
            "completed": completed,
            "pending": pending,
        }
    }))
}
