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
