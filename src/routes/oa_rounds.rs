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

    // Validate round_type against allowed enum values (issue #15)
    const ALLOWED_ROUND_TYPES: &[&str] = &[
        "first_rejection",
        "second_rejection",
        "reexamination",
        "office_action",
        "first_exam",
        "abnormal",
        "reject_review",
    ];
    if !ALLOWED_ROUND_TYPES.contains(&round_type.as_str()) {
        return Json(json!({
            "status": "error",
            "message": format!(
                "Invalid round_type: '{}'. Allowed values: {}",
                round_type,
                ALLOWED_ROUND_TYPES.join(", ")
            )
        }));
    }

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

/// GET /api/oa/:case_id/rounds/diff
///
/// 跨轮次 OA 视图漂移与禁止反言一致性比对：
/// 读取同一案件的所有 `oa_rounds`，对比前后轮次的对比文件（如新增 D3/D4）、
/// 驳回理由演化、审查员已认可特征，以及不可推翻的禁止反言红线。
pub async fn api_oa_rounds_diff(
    Path(case_id): Path<String>,
    State(s): State<AppState>,
) -> Json<Value> {
    match crate::ai::oa_drift::analyze_case_drift(&s.db, &case_id) {
        Ok(report) => Json(json!({
            "status": "ok",
            "case_id": report.case_id,
            "total_rounds": report.total_rounds,
            "timeline": report.timeline,
            "transitions": report.transitions,
            "accepted_features": report.accepted_features,
            "newly_introduced_prior_art": report.newly_introduced_prior_art,
            "new_rejection_grounds": report.new_rejection_grounds,
            "consistency_redlines": report.consistency_redlines,
            "prompt_constraints_block": report.prompt_constraints_block,
            "summary": report.summary,
        })),
        Err(e) => Json(json!({
            "status": "error",
            "message": format!("Failed to compute OA rounds diff: {e}")
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::init_app_state;
    use axum::extract::State;
    use serde_json::json;

    /// #15: 提交非法 round_type 必须被拒绝，返回可读错误信息
    #[tokio::test]
    async fn reject_invalid_round_type() {
        let state = init_app_state(":memory:").unwrap();
        let req = json!({
            "patent_id": "test_patent_1",
            "round_type": "INVALID_TYPE",
            "oa_date": "2026-01-01"
        });
        let resp = api_oa_rounds_create(State(state), Json(req)).await;
        let resp_obj = resp.as_object().unwrap();
        assert_eq!(resp_obj["status"], "error");
        let msg = resp_obj["message"].as_str().unwrap();
        assert!(
            msg.contains("Invalid round_type"),
            "message should mention invalid type: {}",
            msg
        );
        assert!(
            msg.contains("first_rejection"),
            "message should list allowed values: {}",
            msg
        );
    }

    /// #15: 合法 round_type 应被接受（不会因验证失败返回 error）
    #[tokio::test]
    async fn accept_valid_round_type() {
        let state = init_app_state(":memory:").unwrap();
        let req = json!({
            "patent_id": "test_patent_2",
            "round_type": "first_rejection",
            "oa_date": "2026-01-01"
        });
        let resp = api_oa_rounds_create(State(state), Json(req)).await;
        let resp_obj = resp.as_object().unwrap();
        // Should not be a validation error (might be ok or a db error, but not "Invalid round_type")
        if resp_obj["status"] == "error" {
            let msg = resp_obj["message"].as_str().unwrap_or("");
            assert!(
                !msg.contains("Invalid round_type"),
                "valid type should not be rejected: {}",
                msg
            );
        }
    }

    #[tokio::test]
    async fn rounds_diff_detects_new_prior_art_and_consistency_redlines() {
        let state = init_app_state(":memory:").unwrap();
        let case_id = "MOCK-CASE-DRIFT-001";

        let r1 = json!({
            "patent_id": case_id,
            "round_type": "first_rejection",
            "oa_date": "2026-01-10",
            "oa_text": "一通：权利要求1和权利要求2相对于对比文件1（CN101010101A）和D2（CN102020202A）不具备创造性，且权利要求1不具备新颖性。",
            "response_text": "第1轮答复：承认D1公开了主轴外壳。我们将权利要求1修改为增加双级磁流体密封环限定。技术效果在于消除高速泄漏。"
        });
        let resp1 = api_oa_rounds_create(State(state.clone()), Json(r1)).await;
        assert_eq!(resp1.0["status"], "ok");

        let r2 = json!({
            "patent_id": case_id,
            "round_type": "second_rejection",
            "oa_date": "2026-04-20",
            "oa_text": "二通：权利要求2具备新颖性。补充对比文件3（CN103030303A）和D4（US8877665B2），认为权利要求1相对于D1、D3和D4不具备创造性，并指出修改超范围（专利法第33条）。",
            "response_text": "第2轮答复：核心区别特征在于双级磁流体密封环与螺旋导流槽的协同效应。"
        });
        let resp2 = api_oa_rounds_create(State(state.clone()), Json(r2)).await;
        assert_eq!(resp2.0["status"], "ok");

        let r3 = json!({
            "patent_id": case_id,
            "round_type": "reexamination",
            "oa_date": "2026-08-12",
            "oa_text": "驳回决定：维持驳回权利要求1相对于D1+D3+D4不具备创造性。"
        });
        let resp3 = api_oa_rounds_create(State(state.clone()), Json(r3)).await;
        assert_eq!(resp3.0["status"], "ok");

        let diff_resp = api_oa_rounds_diff(Path(case_id.to_string()), State(state)).await;
        let obj = diff_resp.0.as_object().unwrap();
        assert_eq!(obj["status"], "ok");
        assert_eq!(obj["total_rounds"], 3);

        let timeline = obj["timeline"].as_array().unwrap();
        assert_eq!(timeline.len(), 3);
        assert!(timeline[0]["stage_label"]
            .as_str()
            .unwrap()
            .contains("第1轮"));
        assert!(timeline[2]["stage_label"]
            .as_str()
            .unwrap()
            .contains("驳回/复审"));

        let new_pa = obj["newly_introduced_prior_art"].as_array().unwrap();
        assert!(
            new_pa.iter().any(|p| p["label"] == "D3"),
            "should detect newly introduced D3: {new_pa:?}"
        );
        assert!(
            new_pa.iter().any(|p| p["label"] == "D4"),
            "should detect newly introduced D4: {new_pa:?}"
        );

        let accepted = obj["accepted_features"].as_array().unwrap();
        assert!(
            !accepted.is_empty(),
            "should detect explicit or implicit accepted features"
        );

        let redlines = obj["consistency_redlines"].as_array().unwrap();
        assert!(
            redlines
                .iter()
                .any(|r| r["category"] == "estoppel_amendment"),
            "should capture prior round amendment estoppel redline: {redlines:?}"
        );
        assert!(
            redlines
                .iter()
                .any(|r| r["category"] == "estoppel_admission"),
            "should capture prior round admission estoppel redline: {redlines:?}"
        );

        let block = obj["prompt_constraints_block"].as_str().unwrap();
        assert!(block.contains("禁止反言"));
        assert!(block.contains("D3"));
    }

    #[tokio::test]
    async fn rounds_diff_empty_case_returns_zero_rounds() {
        let state = init_app_state(":memory:").unwrap();
        let diff_resp = api_oa_rounds_diff(Path("MOCK-EMPTY-CASE".to_string()), State(state)).await;
        assert_eq!(diff_resp.0["status"], "ok");
        assert_eq!(diff_resp.0["total_rounds"], 0);
        assert_eq!(diff_resp.0["transitions"].as_array().unwrap().len(), 0);
    }
}
