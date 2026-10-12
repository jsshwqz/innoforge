//! 一审-二审对比分析 / First-Second Examination Diff
//!
//! 查询同一专利号的历史 OA 记录，生成相邻轮次的差异对比。
//! 差异维度：驳回理由变化、对比文件变化、权利要求变化。

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use super::AppState;

/// GET /api/oa/history/:patent_number/diff
///
/// 返回该专利号所有 OA 轮次的对比分析。
///
/// 逻辑：
/// 1. 查询该专利号所有历史 OA 记录（按版本正序，即从早到晚）
/// 2. >= 2 条则生成相邻轮次 diff
/// 3. diff 内容：OA 类型变化、分析深度变化、分析文本差异摘要
/// 4. < 2 条则返回空 diffs 并提示无足够历史
pub async fn api_oa_history_diff(
    Path(patent_number): Path<String>,
    State(s): State<AppState>,
) -> Json<Value> {
    match s.db.list_oa_analyses(&patent_number) {
        Ok(mut analyses) => {
            if analyses.is_empty() {
                return Json(json!({
                    "status": "ok",
                    "patent_number": patent_number,
                    "total_rounds": 0,
                    "diffs": [],
                    "message": "无历史 OA 记录 / No history OA records"
                }));
            }

            // list_oa_analyses 返回版本倒序（最新在前），我们翻转为正序（最早在前）
            analyses.reverse();

            let total_rounds = analyses.len();

            if total_rounds < 2 {
                // 只有一轮，无法对比
                return Json(json!({
                    "status": "ok",
                    "patent_number": patent_number,
                    "total_rounds": total_rounds,
                    "diffs": [],
                    "analyses": analyses.iter().map(|a| json!({
                        "version": a.version,
                        "oa_type": a.oa_type,
                        "depth": a.depth,
                        "created_at": a.created_at,
                        "patent_title": a.patent_title,
                    })).collect::<Vec<_>>(),
                    "message": "仅有一轮 OA 记录，无法对比 / Only one OA round, cannot compare"
                }));
            }

            // 生成相邻轮次 diff
            let mut diffs = Vec::new();
            for i in 1..analyses.len() {
                let prev = &analyses[i - 1];
                let curr = &analyses[i];

                let type_changed = prev.oa_type != curr.oa_type;
                let depth_changed = prev.depth != curr.depth;

                // 提取驳回理由关键词变化
                let prev_reasons = extract_rejection_reasons(&prev.analysis_text);
                let curr_reasons = extract_rejection_reasons(&curr.analysis_text);
                let new_reasons: Vec<&str> = curr_reasons
                    .iter()
                    .filter(|r| !prev_reasons.contains(r))
                    .map(|s| s.as_str())
                    .collect();
                let overcome_reasons: Vec<&str> = prev_reasons
                    .iter()
                    .filter(|r| !curr_reasons.contains(r))
                    .map(|s| s.as_str())
                    .collect();

                // 提取对比文件变化
                let prev_refs = extract_reference_numbers(&prev.analysis_text);
                let curr_refs = extract_reference_numbers(&curr.analysis_text);
                let added_refs: Vec<&str> = curr_refs
                    .iter()
                    .filter(|r| !prev_refs.contains(r))
                    .map(|s| s.as_str())
                    .collect();
                let removed_refs: Vec<&str> = prev_refs
                    .iter()
                    .filter(|r| !curr_refs.contains(r))
                    .map(|s| s.as_str())
                    .collect();

                // 提取权利要求变化
                let prev_claims = extract_claim_changes(&prev.analysis_text);
                let curr_claims = extract_claim_changes(&curr.analysis_text);

                diffs.push(json!({
                    "from_version": prev.version,
                    "to_version": curr.version,
                    "from_oa_type": prev.oa_type,
                    "to_oa_type": curr.oa_type,
                    "oa_type_changed": type_changed,
                    "from_depth": prev.depth,
                    "to_depth": curr.depth,
                    "depth_changed": depth_changed,
                    "from_date": prev.created_at,
                    "to_date": curr.created_at,
                    "new_reasons": new_reasons,
                    "overcome_reasons": overcome_reasons,
                    "added_refs": added_refs,
                    "removed_refs": removed_refs,
                    "from_claim_changes": prev_claims,
                    "to_claim_changes": curr_claims,
                    "from_analysis_preview": safe_preview(&prev.analysis_text, 500),
                    "to_analysis_preview": safe_preview(&curr.analysis_text, 500),
                    "analysis_diff": {
                        "added": new_reasons,
                        "removed": overcome_reasons,
                    },
                    "ref_diff": {
                        "added": added_refs,
                        "removed": removed_refs,
                    },
                }));
            }

            // 同时返回完整历史摘要（供前端注入 AI prompt）
            let history_summary: Vec<Value> = analyses
                .iter()
                .map(|a| {
                    json!({
                        "version": a.version,
                        "oa_type": a.oa_type,
                        "depth": a.depth,
                        "created_at": a.created_at,
                        "patent_title": a.patent_title,
                        "analysis_preview": safe_preview(&a.analysis_text, 1000),
                        "analysis_full": a.analysis_text,
                    })
                })
                .collect();

            // T3: 提取一审历史上下文（供 AI 分析 prompt 注入）
            let first_exam_context = analyses
                .iter()
                .find(|a| a.oa_type == "first_exam")
                .map(|a| {
                    format!(
                        "一审 OA 类型: {}\n一审分析摘要:\n{}",
                        a.oa_type,
                        safe_preview(&a.analysis_text, 2000)
                    )
                });

            Json(json!({
                "status": "ok",
                "patent_number": patent_number,
                "total_rounds": total_rounds,
                "diffs": diffs,
                "history": history_summary,
                "first_exam_context": first_exam_context,
            }))
        }
        Err(e) => Json(json!({
            "status": "error",
            "message": format!("查询失败: {}", e)
        })),
    }
}

/// 从分析文本中提取驳回理由关键词
fn extract_rejection_reasons(text: &str) -> Vec<String> {
    let keywords = [
        "新颖性",
        "创造性",
        "实用性",
        "充分公开",
        "权利要求不清楚",
        "得不到说明书支持",
        "修改超范围",
        "缺乏技术启示",
        "显而易见",
        "技术矛盾",
        "结合动机",
        "协同效应",
    ];
    keywords
        .iter()
        .filter(|kw| text.contains(*kw))
        .map(|kw| kw.to_string())
        .collect()
}

/// 从分析文本中提取对比文件公开号（CN\d{7,13}[A-Z]?）
fn extract_reference_numbers(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // 查找 "CN" 开头
        if i + 2 <= bytes.len() && bytes[i] == b'C' && bytes[i + 1] == b'N' {
            let start = i;
            i += 2;
            // 收集数字
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            // 可选字母后缀
            if i < bytes.len() && bytes[i].is_ascii_uppercase() {
                i += 1;
            }
            let num_digits = i - start - 2; // 减去 "CN"
            if num_digits >= 7 {
                let s = &text[start..i];
                if !result.contains(&s.to_string()) {
                    result.push(s.to_string());
                }
            }
        } else {
            i += 1;
        }
    }
    result
}

/// 从分析文本中提取权利要求修改相关文本
fn extract_claim_changes(text: &str) -> Vec<String> {
    let markers = [
        "权1",
        "权2",
        "权3",
        "权利要求1",
        "权利要求2",
        "权利要求3",
        "修改为",
        "增加",
        "删除",
    ];
    markers
        .iter()
        .filter(|m| text.contains(*m))
        .map(|m| m.to_string())
        .collect()
}

/// 安全截取预览文本（不破坏数据完整性，仅用于显示）
fn safe_preview(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_chars).collect();
        format!("{}...(已截断)", truncated)
    }
}

// ── 单元测试 ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        extract_claim_changes, extract_reference_numbers, extract_rejection_reasons, safe_preview,
    };
    use crate::cad::CadService;
    use crate::db::Database;
    use crate::routes::{AppConfig, AppState};
    use axum::extract::{Path, State};
    use serde_json::Value;
    use std::sync::{Arc, RwLock};

    /// 构建测试用 AppState（内存数据库）
    fn make_state() -> AppState {
        let db = Database::init(":memory:").expect("in-memory db");
        let cad_root =
            std::env::temp_dir().join(format!("innoforge-oa-diff-test-{}", uuid::Uuid::new_v4()));
        let cad = CadService::new(cad_root, None).expect("cad service");
        let config = AppConfig::default();
        AppState {
            db: Arc::new(db),
            cad: Arc::new(cad),
            config: Arc::new(RwLock::new(config)),
            pipeline_channels: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// 调用 diff API 并返回 JSON 响应
    async fn call_diff(state: &AppState, patent_number: &str) -> Value {
        let result =
            super::api_oa_history_diff(Path(patent_number.to_string()), State(state.clone())).await;
        let axum::Json(val) = result;
        val
    }

    // ── 辅助函数测试 ──────────────────────────────────────────────────────────

    #[test]
    fn test_extract_rejection_reasons() {
        let text = "该申请存在新颖性和创造性问题，且修改超范围。";
        let reasons = extract_rejection_reasons(text);
        assert!(reasons.contains(&"新颖性".to_string()));
        assert!(reasons.contains(&"创造性".to_string()));
        assert!(reasons.contains(&"修改超范围".to_string()));
        assert!(!reasons.contains(&"实用性".to_string()));
    }

    #[test]
    fn test_extract_rejection_reasons_empty() {
        let reasons = extract_rejection_reasons("这是一段普通文本，没有驳回理由。");
        assert!(reasons.is_empty());
    }

    #[test]
    fn test_extract_reference_numbers() {
        let text = "对比文件 CN11111111A 和 CN22222222B 公开了相关技术。";
        let refs = extract_reference_numbers(text);
        assert!(refs.contains(&"CN11111111A".to_string()));
        assert!(refs.contains(&"CN22222222B".to_string()));
    }

    #[test]
    fn test_extract_reference_numbers_dedup() {
        let text = "CN11111111A 公开了技术。再次引用 CN11111111A。";
        let refs = extract_reference_numbers(text);
        assert_eq!(refs.len(), 1, "should deduplicate references");
    }

    #[test]
    fn test_extract_reference_numbers_short_rejected() {
        // CN + 6 digits = too short (< 7 digits), should not match
        let text = "对比文件 CN123456 公开了技术。";
        let refs = extract_reference_numbers(text);
        assert!(refs.is_empty(), "CN123456 (6 digits) should be rejected");
    }

    #[test]
    fn test_extract_claim_changes() {
        let text = "权利要求1 修改为增加技术特征。权2 保持不变。";
        let claims = extract_claim_changes(text);
        assert!(claims.contains(&"权利要求1".to_string()));
        assert!(claims.contains(&"修改为".to_string()));
        assert!(claims.contains(&"增加".to_string()));
        assert!(claims.contains(&"权2".to_string()));
    }

    #[test]
    fn test_safe_preview_no_truncation() {
        let text = "短文本";
        assert_eq!(safe_preview(text, 100), "短文本");
    }

    #[test]
    fn test_safe_preview_truncation() {
        let text = "这是一段很长的文本，需要被截断处理。";
        let preview = safe_preview(text, 5);
        assert!(preview.contains("...(已截断)"));
        let core = preview.strip_suffix("...(已截断)").unwrap();
        assert!(core.chars().count() <= 5);
    }

    // ── API 端点测试 ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_no_history_returns_empty_diffs() {
        let state = make_state();
        let resp = call_diff(&state, "CN99999999A").await;

        assert_eq!(resp["status"], "ok");
        assert_eq!(resp["total_rounds"], 0);
        assert_eq!(resp["diffs"].as_array().unwrap().len(), 0);
        assert!(resp["message"].as_str().unwrap().contains("无历史"));
    }

    #[tokio::test]
    async fn test_single_round_returns_cannot_compare() {
        let state = make_state();

        state
            .db
            .save_oa_analysis(
                "CN12345678A",
                "测试专利",
                "first_exam",
                "deep",
                "一审分析：该申请缺乏新颖性，对比文件 CN11111111A 公开了相同技术方案。",
            )
            .expect("save oa analysis");

        let resp = call_diff(&state, "CN12345678A").await;

        assert_eq!(resp["status"], "ok");
        assert_eq!(resp["total_rounds"], 1);
        assert_eq!(resp["diffs"].as_array().unwrap().len(), 0);
        assert!(resp["message"].as_str().unwrap().contains("仅有一轮"));
        assert!(resp["analyses"].is_array());
        assert_eq!(resp["analyses"][0]["oa_type"], "first_exam");
    }

    #[tokio::test]
    async fn test_two_rounds_generates_diff() {
        let state = make_state();

        state
            .db
            .save_oa_analysis(
                "CN22222222A",
                "火焰燃烧装置",
                "first_exam",
                "deep",
                "一审分析：\n1. 新颖性问题：对比文件 CN11111111A 公开了相同技术方案。\n\
                 权利要求1 不具备新颖性。\n2. 创造性问题：缺乏技术启示。",
            )
            .expect("save first exam");

        state
            .db
            .save_oa_analysis(
                "CN22222222A",
                "火焰燃烧装置",
                "second_rejection",
                "deep",
                "二审驳回：\n1. 创造性问题：对比文件 CN11111111A 和 CN22222222B 结合，\
                 缺乏技术启示。\n2. 修改超范围：权1 修改超出原说明书范围。\n\
                 权利要求1 修改为增加技术特征。",
            )
            .expect("save second rejection");

        let resp = call_diff(&state, "CN22222222A").await;

        assert_eq!(resp["status"], "ok");
        assert_eq!(resp["total_rounds"], 2);

        let diffs = resp["diffs"].as_array().expect("diffs should be array");
        assert_eq!(diffs.len(), 1, "should have 1 diff between 2 rounds");

        let diff = &diffs[0];

        assert_eq!(diff["from_oa_type"], "first_exam");
        assert_eq!(diff["to_oa_type"], "second_rejection");
        assert_eq!(diff["oa_type_changed"], true);
        assert_eq!(diff["depth_changed"], false);

        let new_reasons: Vec<&str> = diff["new_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let overcome_reasons: Vec<&str> = diff["overcome_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();

        assert!(
            new_reasons.contains(&"修改超范围"),
            "new_reasons should contain 修改超范围, got: {:?}",
            new_reasons
        );
        assert!(
            overcome_reasons.contains(&"新颖性"),
            "overcome_reasons should contain 新颖性, got: {:?}",
            overcome_reasons
        );

        let added_refs: Vec<&str> = diff["added_refs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(
            added_refs.iter().any(|r| r.contains("CN2222")),
            "added_refs should contain a CN2222* reference, got: {:?}",
            added_refs
        );
    }

    #[tokio::test]
    async fn test_three_rounds_generates_two_diffs() {
        let state = make_state();

        state
            .db
            .save_oa_analysis(
                "CN33333333A",
                "三轮测试专利",
                "first_exam",
                "shallow",
                "一审：新颖性问题。对比文件 CN44444444A。",
            )
            .expect("save round 1");

        state
            .db
            .save_oa_analysis(
                "CN33333333A",
                "三轮测试专利",
                "first_exam",
                "deep",
                "一审继续：创造性性问题。对比文件 CN44444444A。",
            )
            .expect("save round 2");

        state
            .db
            .save_oa_analysis(
                "CN33333333A",
                "三轮测试专利",
                "second_rejection",
                "deep",
                "二审驳回：创造性性问题和修改超范围。",
            )
            .expect("save round 3");

        let resp = call_diff(&state, "CN33333333A").await;

        assert_eq!(resp["total_rounds"], 3);
        let diffs = resp["diffs"].as_array().unwrap();
        assert_eq!(diffs.len(), 2, "3 rounds should produce 2 diffs");

        // 第一个 diff：shallow → deep（深度变化）
        assert_eq!(diffs[0]["depth_changed"], true);
        // 第二个 diff：first_exam → second_rejection（类型变化）
        assert_eq!(diffs[1]["oa_type_changed"], true);
    }

    #[tokio::test]
    async fn test_first_exam_context_extraction() {
        let state = make_state();

        state
            .db
            .save_oa_analysis(
                "CN55555555A",
                "上下文测试专利",
                "first_exam",
                "deep",
                "一审分析：该申请存在新颖性问题。对比文件 CN66666666A 公开了核心技术特征。",
            )
            .expect("save first exam");

        state
            .db
            .save_oa_analysis(
                "CN55555555A",
                "上下文测试专利",
                "second_rejection",
                "deep",
                "二审驳回：创造性性问题。",
            )
            .expect("save second rejection");

        let resp = call_diff(&state, "CN55555555A").await;

        assert!(
            !resp["first_exam_context"].is_null(),
            "first_exam_context should be extracted when first_exam record exists"
        );
        let ctx = resp["first_exam_context"].as_str().unwrap();
        assert!(
            ctx.contains("一审"),
            "context should mention 一审, got: {}",
            ctx
        );
        assert!(
            ctx.contains("新颖性"),
            "context should contain first exam analysis content, got: {}",
            ctx
        );
    }

    #[tokio::test]
    async fn test_no_first_exam_context_when_absent() {
        let state = make_state();

        state
            .db
            .save_oa_analysis(
                "CN77777777A",
                "无一审测试",
                "second_rejection",
                "deep",
                "二审驳回：创造性性问题。",
            )
            .expect("save second rejection");

        state
            .db
            .save_oa_analysis(
                "CN77777777A",
                "无一审测试",
                "second_rejection",
                "deep",
                "二审继续驳回。",
            )
            .expect("save another second rejection");

        let resp = call_diff(&state, "CN77777777A").await;

        assert!(
            resp["first_exam_context"].is_null(),
            "first_exam_context should be null when no first_exam record exists"
        );
    }

    #[tokio::test]
    async fn test_history_summary_returned() {
        let state = make_state();

        state
            .db
            .save_oa_analysis(
                "CN88888888A",
                "历史摘要测试",
                "first_exam",
                "deep",
                "一审分析内容。",
            )
            .expect("save round 1");

        state
            .db
            .save_oa_analysis(
                "CN88888888A",
                "历史摘要测试",
                "second_rejection",
                "deep",
                "二审驳回内容。",
            )
            .expect("save round 2");

        let resp = call_diff(&state, "CN88888888A").await;

        let history = resp["history"].as_array().expect("history should be array");
        assert_eq!(history.len(), 2, "should have 2 history entries");
        assert_eq!(history[0]["oa_type"], "first_exam");
        assert_eq!(history[1]["oa_type"], "second_rejection");
        assert!(history[0]["analysis_full"].is_string());
    }
}
