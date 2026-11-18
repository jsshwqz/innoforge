//! 特征卡片 API 路由 / Feature cards API routes
//!
//! CRUD 操作 + 卡片间最小差异比较

use super::AppState;
use crate::patent::{CreateFeatureCardRequest, FeatureCard};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;

/// 获取指定创意的所有特征卡片 / Get all feature cards for an idea
pub async fn api_get_feature_cards(
    State(s): State<AppState>,
    Path(idea_id): Path<String>,
) -> impl IntoResponse {
    // 校验创意存在 / Verify the idea exists
    match s.db.get_idea(&idea_id) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"status": "error", "message": "idea not found"})),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status": "error", "message": e.to_string()})),
            )
                .into_response();
        }
    }

    match s.db.get_feature_cards_by_idea(&idea_id) {
        Ok(cards) => Json(json!({"status": "ok", "cards": cards})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"status": "error", "message": e.to_string()})),
        )
            .into_response(),
    }
}

/// 创建特征卡片 / Create a feature card for an idea
pub async fn api_create_feature_card(
    State(s): State<AppState>,
    Path(idea_id): Path<String>,
    Json(req): Json<CreateFeatureCardRequest>,
) -> impl IntoResponse {
    // 输入校验 / Validate input
    let title = req.title.trim().to_string();
    if title.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"status": "error", "message": "title is required"})),
        )
            .into_response();
    }
    if title.chars().count() > 500 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"status": "error", "message": "title too long (max 500 chars)"})),
        )
            .into_response();
    }

    // 校验创意存在 / Verify the idea exists
    match s.db.get_idea(&idea_id) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"status": "error", "message": "idea not found"})),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status": "error", "message": e.to_string()})),
            )
                .into_response();
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let card = FeatureCard {
        id: id.clone(),
        idea_id: idea_id.clone(),
        title,
        description: req.description.trim().to_string(),
        novelty_score: req.novelty_score,
        created_at: now,
        technical_problem: req.technical_problem.trim().to_string(),
        core_structure: req.core_structure.trim().to_string(),
        key_relations: req.key_relations.trim().to_string(),
        process_steps: req.process_steps.trim().to_string(),
        application_scenarios: req.application_scenarios.trim().to_string(),
    };

    match s.db.insert_feature_card(&card) {
        Ok(()) => Json(json!({"status": "ok", "card": card})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"status": "error", "message": e.to_string()})),
        )
            .into_response(),
    }
}

/// 特征卡差异对比查询参数 / Query parameters for feature card diff
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FeatureCardDiffQuery {
    #[serde(default)]
    pub a: Option<String>,
    #[serde(default)]
    pub b: Option<String>,
    #[serde(default)]
    pub semantic: Option<bool>,
}

/// GET /api/feature-cards/diff?a=ID_A&b=ID_B
/// 最小差异定位：对比两张特征卡片的标题和描述
/// Minimal diff: compare two feature cards' title and description
pub async fn api_feature_card_diff(
    State(s): State<AppState>,
    Query(params): Query<FeatureCardDiffQuery>,
) -> impl IntoResponse {
    let id_a = match params.a.as_deref() {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"status": "error", "message": "缺少参数 a"})),
            )
                .into_response();
        }
    };
    let id_b = match params.b.as_deref() {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"status": "error", "message": "缺少参数 b"})),
            )
                .into_response();
        }
    };

    let card_a = match s.db.get_feature_card(id_a) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"status": "error", "message": format!("卡片 {} 不存在", id_a)})),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status": "error", "message": e.to_string()})),
            )
                .into_response();
        }
    };

    let card_b = match s.db.get_feature_card(id_b) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"status": "error", "message": format!("卡片 {} 不存在", id_b)})),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status": "error", "message": e.to_string()})),
            )
                .into_response();
        }
    };

    let diff = compute_minimal_diff(&card_a, &card_b);
    if params.semantic == Some(true) {
        let fast_client = {
            let cfg = s.config.read().unwrap_or_else(|e| e.into_inner());
            if cfg.ai_fast_channel_ready() {
                Some(cfg.ai_client_fast())
            } else {
                None
            }
        };
        let verdict = classify_diff_semantic(fast_client.as_ref(), &card_a, &card_b).await;
        return Json(json!({
            "status": "ok",
            "card_a": { "id": card_a.id, "title": card_a.title },
            "card_b": { "id": card_b.id, "title": card_b.title },
            "diff": diff,
            "diff_type": verdict.diff_type,
            "novelty_significance": verdict.novelty_significance,
            "novelty_reason": verdict.novelty_reason,
            "semantically_equivalent": verdict.semantically_equivalent,
            "fallback_used": verdict.fallback_used,
        }))
        .into_response();
    }

    let (diff_type, novelty_significance, novelty_reason) = classify_diff(&card_a, &card_b);
    Json(json!({
        "status": "ok",
        "card_a": { "id": card_a.id, "title": card_a.title },
        "card_b": { "id": card_b.id, "title": card_b.title },
        "diff": diff,
        "diff_type": diff_type,
        "novelty_significance": novelty_significance,
        "novelty_reason": novelty_reason,
    }))
    .into_response()
}

/// 计算两张卡片的最小差异 / Compute minimal diff between two cards
///
/// 使用字符级 LCS 定位差异段落，返回结构化差异列表
fn compute_minimal_diff(a: &FeatureCard, b: &FeatureCard) -> serde_json::Value {
    let title_diff = diff_strings(&a.title, &b.title);
    let desc_diff = diff_strings(&a.description, &b.description);
    let score_diff = match (a.novelty_score, b.novelty_score) {
        (Some(sa), Some(sb)) => json!({
            "a": format!("{:.1}", sa),
            "b": format!("{:.1}", sb),
            "delta": format!("{:+.1}", sb - sa),
        }),
        _ => json!(null),
    };
    let tp_diff = diff_strings(&a.technical_problem, &b.technical_problem);
    let cs_diff = diff_strings(&a.core_structure, &b.core_structure);
    let kr_diff = diff_strings(&a.key_relations, &b.key_relations);
    let ps_diff = diff_strings(&a.process_steps, &b.process_steps);
    let as_diff = diff_strings(&a.application_scenarios, &b.application_scenarios);

    json!({
        "title": title_diff,
        "description": desc_diff,
        "novelty_score": score_diff,
        "technical_problem": tp_diff,
        "core_structure": cs_diff,
        "key_relations": kr_diff,
        "process_steps": ps_diff,
        "application_scenarios": as_diff,
    })
}

/// 差异类型分类 + 新颖性判断
fn classify_diff(a: &FeatureCard, b: &FeatureCard) -> (String, bool, String) {
    let structure_changed = a.core_structure != b.core_structure;
    let method_changed = a.process_steps != b.process_steps;
    let problem_changed = a.technical_problem != b.technical_problem;
    let relations_changed = a.key_relations != b.key_relations;
    let scenarios_changed = a.application_scenarios != b.application_scenarios;

    let diff_type = if structure_changed {
        "structure"
    } else if method_changed {
        "method"
    } else if problem_changed || relations_changed || scenarios_changed {
        "parameter"
    } else {
        "none"
    };

    let novelty_significance = structure_changed || method_changed;

    let novelty_reason = if structure_changed {
        "核心结构采用了不同的技术方案".to_string()
    } else if method_changed {
        "工艺/实施步骤存在差异".to_string()
    } else if problem_changed || relations_changed || scenarios_changed {
        "参数级差异，新颖性意义较低".to_string()
    } else {
        "无显著差异".to_string()
    };

    (diff_type.to_string(), novelty_significance, novelty_reason)
}

/// 滚动数组计算 LCS DP 的最后一行 / Final LCS DP row via a two-row rolling array
///
/// 只保留上一行与当前行两行（各 n+1 格），空间 O(n)。
fn lcs_last_row(a: &[char], b: &[char]) -> Vec<u32> {
    let n = b.len();
    let mut prev = vec![0u32; n + 1];
    let mut cur = vec![0u32; n + 1];
    for &ca in a {
        for j in 1..=n {
            cur[j] = if ca == b[j - 1] {
                prev[j - 1] + 1
            } else {
                prev[j].max(cur[j - 1])
            };
        }
        std::mem::swap(&mut prev, &mut cur);
        // 交换后 cur 持有旧行，下一轮每个格子都会被整行覆写（cur[0] 恒为 0），无需清零
    }
    prev
}

/// Hirschberg 分治：线性空间重建 LCS 编辑序列 / Linear-space LCS reconstruction
///
/// 输出顺序与旧全表回溯一致（从左到右）：'='（保留）/'-'（删除）/'+'（新增）。
/// 平手时取最小分割点，保持稳定的输出取向。
fn lcs_diff_ops(a: &[char], b: &[char], ops: &mut Vec<(char, char)>) {
    if b.is_empty() {
        for &ch in a {
            ops.push(('-', ch));
        }
        return;
    }
    if a.is_empty() {
        for &ch in b {
            ops.push(('+', ch));
        }
        return;
    }
    if a.len() == 1 {
        // 单字符基准：在 b 中找首个匹配，命中则 add+equal+add，否则全增再删
        match b.iter().position(|&c| c == a[0]) {
            Some(j) => {
                for &c in &b[..j] {
                    ops.push(('+', c));
                }
                ops.push(('=', a[0]));
                for &c in &b[j + 1..] {
                    ops.push(('+', c));
                }
            }
            None => {
                for &c in b {
                    ops.push(('+', c));
                }
                ops.push(('-', a[0]));
            }
        }
        return;
    }
    let mid = a.len() / 2;
    let n = b.len();
    let left = lcs_last_row(&a[..mid], b);
    let a_tail_rev: Vec<char> = a[mid..].iter().rev().copied().collect();
    let b_rev: Vec<char> = b.iter().rev().copied().collect();
    let right = lcs_last_row(&a_tail_rev, &b_rev);
    // 选分割点 k：最大化 left[k] + right[n-k]（LCS 分治的最优分割）
    let mut best_k = 0usize;
    let mut best_score = left[0] + right[n];
    for k in 1..=n {
        let s = left[k] + right[n - k];
        if s > best_score {
            best_score = s;
            best_k = k;
        }
    }
    lcs_diff_ops(&a[..mid], &b[..best_k], ops);
    lcs_diff_ops(&a[mid..], &b[best_k..], ops);
}

/// 字符级差异提取 / Character-level diff extraction
///
/// 基于最长公共子序列（LCS），输出增删操作列表。
///
/// 空间优化（滚动数组）：DP 只保留两行，差异重建用 Hirschberg 分治线性空间完成，
/// 整体空间 O(n)，不再分配完整 (m+1)×(n+1) 表——5000 字上限下旧实现约 100MB，
/// 现在约 40KB。时间仍为 O(m·n)（分治带来约 2 倍常数）。
fn diff_strings(a: &str, b: &str) -> Vec<serde_json::Value> {
    if a == b {
        return vec![json!({"type": "equal", "text": a})];
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    // 对超长文本短路为整段替换，避免 O(m*n) 时间爆量（空间已是线性，仍按旧口径防御）
    if m > 5000 || n > 5000 {
        return vec![
            json!({"type": "remove", "text": a}),
            json!({"type": "add", "text": b}),
        ];
    }

    // Hirschberg 分治重建差异：顺序产出 '='（保留）/'-'（删除）/'+'（新增）
    let mut raw_ops: Vec<(char, char)> = Vec::new();
    lcs_diff_ops(&a_chars, &b_chars, &mut raw_ops);

    // 合并连续的同类操作
    let mut ops: Vec<serde_json::Value> = Vec::new();
    let mut buf_eq = String::new();
    let mut buf_rm = String::new();
    let mut buf_add = String::new();
    let flush = |ops: &mut Vec<serde_json::Value>,
                 buf_eq: &mut String,
                 buf_rm: &mut String,
                 buf_add: &mut String| {
        if !buf_rm.is_empty() {
            ops.push(json!({"type": "remove", "text": buf_rm.clone()}));
            buf_rm.clear();
        }
        if !buf_add.is_empty() {
            ops.push(json!({"type": "add", "text": buf_add.clone()}));
            buf_add.clear();
        }
        if !buf_eq.is_empty() {
            ops.push(json!({"type": "equal", "text": buf_eq.clone()}));
            buf_eq.clear();
        }
    };

    for (op, ch) in raw_ops {
        match op {
            '=' => {
                // 先刷新增删
                if !buf_rm.is_empty() {
                    ops.push(json!({"type": "remove", "text": buf_rm.clone()}));
                    buf_rm.clear();
                }
                if !buf_add.is_empty() {
                    ops.push(json!({"type": "add", "text": buf_add.clone()}));
                    buf_add.clear();
                }
                buf_eq.push(ch);
            }
            '-' => {
                if !buf_eq.is_empty() {
                    ops.push(json!({"type": "equal", "text": buf_eq.clone()}));
                    buf_eq.clear();
                }
                buf_rm.push(ch);
            }
            '+' => {
                if !buf_eq.is_empty() {
                    ops.push(json!({"type": "equal", "text": buf_eq.clone()}));
                    buf_eq.clear();
                }
                buf_add.push(ch);
            }
            _ => {}
        }
    }
    // 最后刷新
    flush(&mut ops, &mut buf_eq, &mut buf_rm, &mut buf_add);

    ops
}

/// 语义级特征卡对比判定结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SemanticDiffVerdict {
    pub semantically_equivalent: bool,
    pub diff_type: String,
    pub novelty_significance: bool,
    pub novelty_reason: String,
    pub fallback_used: bool,
}

fn extract_context_hints(
    db: &crate::db::Database,
    idea_id: &str,
    fallback_analysis: &str,
) -> (String, Vec<String>) {
    let ctx_json_opt = db
        .load_pipeline_snapshot(idea_id)
        .ok()
        .flatten()
        .map(|(ctx_json, _)| ctx_json)
        .or_else(|| {
            db.get_latest_version(idea_id, "main")
                .ok()
                .flatten()
                .map(|v| v.context_json)
        });

    if let Some(ctx_json) = ctx_json_opt {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&ctx_json) {
            let analysis = val["ai_analysis"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(fallback_analysis)
                .to_string();
            let keywords = val["keywords"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::trim))
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            return (analysis, keywords);
        }
    }

    (fallback_analysis.to_string(), Vec::new())
}

fn build_deterministic_feature_cards(
    idea_id: &str,
    idea_title: &str,
    idea_description: &str,
    analysis_text: &str,
    keywords: &[String],
) -> Vec<FeatureCard> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let segments: Vec<&str> = idea_description
        .split(['。', '；', '\n', '.', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .take(3)
        .collect();

    let key_rel = if keywords.is_empty() {
        crate::ai::safe_truncate_chars(idea_description.trim(), 120).to_string()
    } else {
        keywords
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("、")
    };
    let steps = if analysis_text.trim().is_empty() {
        "待补充实施步骤".to_string()
    } else {
        crate::ai::safe_truncate_chars(analysis_text.trim(), 160).to_string()
    };
    let problem = crate::ai::safe_truncate_chars(idea_title.trim(), 120).to_string();
    let scenario = crate::ai::safe_truncate_chars(idea_title.trim(), 80).to_string();

    if segments.is_empty() {
        return vec![FeatureCard {
            id: uuid::Uuid::new_v4().to_string(),
            idea_id: idea_id.to_string(),
            title: crate::ai::safe_truncate_chars(idea_title.trim(), 40).to_string(),
            description: idea_description.trim().to_string(),
            novelty_score: None,
            created_at: now,
            technical_problem: problem,
            core_structure: crate::ai::safe_truncate_chars(idea_description.trim(), 200)
                .to_string(),
            key_relations: key_rel,
            process_steps: steps,
            application_scenarios: scenario,
        }];
    }

    segments
        .into_iter()
        .map(|seg| FeatureCard {
            id: uuid::Uuid::new_v4().to_string(),
            idea_id: idea_id.to_string(),
            title: crate::ai::safe_truncate_chars(seg, 40).to_string(),
            description: seg.to_string(),
            novelty_score: None,
            created_at: now.clone(),
            technical_problem: problem.clone(),
            core_structure: crate::ai::safe_truncate_chars(seg, 200).to_string(),
            key_relations: key_rel.clone(),
            process_steps: steps.clone(),
            application_scenarios: scenario.clone(),
        })
        .collect()
}

/// 为指定创意自动生成 5 维特征卡（优先调 ai_client_fast 60s 超时解析，无 Key/失败回退规则提取并入库）
pub async fn api_generate_feature_cards(
    State(s): State<AppState>,
    Path(idea_id): Path<String>,
) -> impl IntoResponse {
    let idea = match s.db.get_idea(&idea_id) {
        Ok(Some(i)) => i,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"status": "error", "message": "idea not found"})),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status": "error", "message": e.to_string()})),
            )
                .into_response();
        }
    };

    let (analysis_text, keywords) = extract_context_hints(&s.db, &idea_id, &idea.analysis);
    let config = s.config.read().unwrap_or_else(|e| e.into_inner()).clone();

    if config.ai_fast_channel_ready() {
        const SYSTEM_PROMPT: &str =
            "你是专利技术特征提取助手。请严格只输出合法的 JSON 数组，不要输出任何解释文字或 markdown 标记。";
        let prompt = format!(
            "从以下创意描述与分析上下文中提取 5 维技术特征卡片（最多 5 张）。\n\
             输出 JSON 数组，每个元素包含字段：\n\
             title, technical_problem, core_structure, key_relations, process_steps, application_scenarios。\n\n\
             创意标题：\n<user_input>\n{}\n</user_input>\n\n\
             创意描述：\n<user_input>\n{}\n</user_input>\n\n\
             分析摘要：\n<user_input>\n{}\n</user_input>",
            idea.title,
            idea.description,
            crate::ai::truncate_for_ai(&analysis_text, 2000)
        );
        let fast_ai = config
            .ai_client_fast()
            .clone_with_timeout(std::time::Duration::from_secs(60));
        if let Ok(raw) = fast_ai.chat_with_system(SYSTEM_PROMPT, &prompt, 0.2).await {
            let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
            let cards =
                crate::pipeline::steps::analysis::parse_ai_feature_cards_json(&idea_id, &raw, &now);
            if !cards.is_empty() {
                for card in &cards {
                    if let Err(e) = s.db.insert_feature_card(card) {
                        tracing::warn!("自动生成特征卡入库失败: {}", e);
                    }
                }
                let cad_prompts: Vec<String> =
                    cards.iter().map(build_cad_prompt_from_card).collect();
                return Json(json!({
                    "status": "ok",
                    "cards": cards,
                    "cad_prompts": cad_prompts,
                    "source": "ai_fast"
                }))
                .into_response();
            }
        }
    }

    let cards = build_deterministic_feature_cards(
        &idea_id,
        &idea.title,
        &idea.description,
        &analysis_text,
        &keywords,
    );
    for card in &cards {
        if let Err(e) = s.db.insert_feature_card(card) {
            tracing::warn!("保底特征卡入库失败: {}", e);
        }
    }
    let cad_prompts: Vec<String> = cards.iter().map(build_cad_prompt_from_card).collect();
    Json(json!({
        "status": "ok",
        "cards": cards,
        "cad_prompts": cad_prompts,
        "source": "deterministic_fallback"
    }))
    .into_response()
}

fn deterministic_semantic_verdict(a: &FeatureCard, b: &FeatureCard) -> SemanticDiffVerdict {
    let (diff_type, novelty_significance, novelty_reason) = classify_diff(a, b);
    SemanticDiffVerdict {
        semantically_equivalent: diff_type == "none",
        diff_type,
        novelty_significance,
        novelty_reason,
        fallback_used: true,
    }
}

fn parse_semantic_diff_json(
    raw: &str,
    fallback: &SemanticDiffVerdict,
) -> Option<SemanticDiffVerdict> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if start > end {
        return None;
    }
    let slice = raw.get(start..=end)?;
    let val = serde_json::from_str::<serde_json::Value>(slice).ok()?;
    let semantically_equivalent = val["semantically_equivalent"].as_bool()?;
    let diff_type = match val["diff_type"].as_str() {
        Some(t @ ("none" | "structure" | "method" | "parameter")) => t.to_string(),
        _ => fallback.diff_type.clone(),
    };
    let novelty_significance = val["novelty_significance"]
        .as_bool()
        .unwrap_or(!semantically_equivalent);
    let novelty_reason = val["novelty_reason"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| fallback.novelty_reason.clone());

    Some(SemanticDiffVerdict {
        semantically_equivalent,
        diff_type,
        novelty_significance,
        novelty_reason,
        fallback_used: false,
    })
}

/// 语义级特征卡对比（ai_fast 为 None 或调用/JSON 解析失败时，100% 回退确定性 classify_diff）
pub async fn classify_diff_semantic(
    ai_fast: Option<&crate::ai::AiClient>,
    a: &FeatureCard,
    b: &FeatureCard,
) -> SemanticDiffVerdict {
    let fallback = deterministic_semantic_verdict(a, b);
    let Some(ai) = ai_fast else {
        return fallback;
    };

    const SYSTEM_PROMPT: &str =
        "你是专利技术特征语义对比助手。请严格只输出一个合法的 JSON 对象，包含字段：semantically_equivalent (bool), diff_type (\"none\"|\"structure\"|\"method\"|\"parameter\"), novelty_significance (bool), novelty_reason (string)。";
    let user_prompt = format!(
        "请判断以下两张技术特征卡是否构成实质性语义等价。\n\n\
         卡片 A：\n<user_input>\n标题：{}\n技术问题：{}\n核心结构：{}\n关键关系：{}\n工艺步骤：{}\n应用场景：{}\n</user_input>\n\n\
         卡片 B：\n<user_input>\n标题：{}\n技术问题：{}\n核心结构：{}\n关键关系：{}\n工艺步骤：{}\n应用场景：{}\n</user_input>",
        a.title,
        a.technical_problem,
        a.core_structure,
        a.key_relations,
        a.process_steps,
        a.application_scenarios,
        b.title,
        b.technical_problem,
        b.core_structure,
        b.key_relations,
        b.process_steps,
        b.application_scenarios,
    );

    let fast_ai = ai.clone_with_timeout(std::time::Duration::from_secs(60));
    match fast_ai
        .chat_with_system(SYSTEM_PROMPT, &user_prompt, 0.1)
        .await
    {
        Ok(raw) => parse_semantic_diff_json(&raw, &fallback).unwrap_or(fallback),
        Err(_) => fallback,
    }
}

/// 从单张 5 维特征卡组装结构化 FreeCAD 绘图指令（供一键转入 /api/cad/draw）
pub fn build_cad_prompt_from_card(card: &FeatureCard) -> String {
    let structure = if card.core_structure.trim().is_empty() {
        card.description.trim()
    } else {
        card.core_structure.trim()
    };
    let mut parts = vec![
        format!("特征名称：{}", card.title.trim()),
        format!("核心技术结构：{}", structure),
    ];
    if !card.technical_problem.trim().is_empty() {
        parts.push(format!("解决的技术问题：{}", card.technical_problem.trim()));
    }
    if !card.key_relations.trim().is_empty() {
        parts.push(format!("关键配合与连接关系：{}", card.key_relations.trim()));
    }
    if !card.process_steps.trim().is_empty() {
        parts.push(format!("工艺/装配步骤：{}", card.process_steps.trim()));
    }
    if !card.application_scenarios.trim().is_empty() {
        parts.push(format!("应用场景：{}", card.application_scenarios.trim()));
    }
    format!(
        "请基于以下专利技术特征生成参数化 FreeCAD 3D 建模脚本与关键几何约束：\n{}",
        parts.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patent::FeatureCard;

    /// 构造测试用特征卡片
    fn sample_card(id: &str, title: &str) -> FeatureCard {
        FeatureCard {
            id: id.to_string(),
            idea_id: "idea-1".to_string(),
            title: title.to_string(),
            description: format!("Description for {}", title),
            novelty_score: Some(0.85),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            technical_problem: "technical problem".to_string(),
            core_structure: "core structure".to_string(),
            key_relations: "key relations".to_string(),
            process_steps: "process steps".to_string(),
            application_scenarios: "application scenarios".to_string(),
        }
    }

    // ── diff_strings 测试 ──

    #[test]
    fn test_diff_strings_identical() {
        let ops = diff_strings("hello", "hello");
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0]["type"], "equal");
        assert_eq!(ops[0]["text"], "hello");
    }

    #[test]
    fn test_diff_strings_empty_vs_nonempty() {
        let ops = diff_strings("", "hello");
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0]["type"], "add");
        assert_eq!(ops[0]["text"], "hello");
    }

    #[test]
    fn test_diff_strings_nonempty_vs_empty() {
        let ops = diff_strings("hello", "");
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0]["type"], "remove");
        assert_eq!(ops[0]["text"], "hello");
    }

    #[test]
    fn test_diff_strings_simple_replacement() {
        // "abc" vs "adc" — 'b' 移除, 'd' 添加
        let ops = diff_strings("abc", "adc");
        let types: Vec<&str> = ops.iter().map(|o| o["type"].as_str().unwrap()).collect();
        assert!(types.contains(&"remove"), "应有 remove 操作");
        assert!(types.contains(&"add"), "应有 add 操作");
    }

    #[test]
    fn test_diff_strings_completely_different() {
        // "abc" vs "xyz" — 全部替换
        let ops = diff_strings("abc", "xyz");
        let has_remove = ops.iter().any(|o| o["type"] == "remove");
        let has_add = ops.iter().any(|o| o["type"] == "add");
        assert!(has_remove, "完全不同时应包含 remove");
        assert!(has_add, "完全不同时应包含 add");
    }

    #[test]
    fn test_diff_strings_long_text_truncation() {
        // 超过 5000 字符时直接返回 remove + add（避免 O(m*n) 爆时间）
        let long_a = "A".repeat(5001);
        let long_b = "B".repeat(5001);
        let ops = diff_strings(&long_a, &long_b);
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0]["type"], "remove");
        assert_eq!(ops[1]["type"], "add");
    }

    #[test]
    fn test_diff_strings_lcs_length_classic() {
        // 经典教材例：LCS("ABCBDAB", "BDCABA") = 4（如 BCBA / BDAB）。
        // equal 段总字符数必须恰为 LCS 长度，锁定 Hirschberg 重建的最优性。
        let ops = diff_strings("ABCBDAB", "BDCABA");
        let equal_chars: usize = ops
            .iter()
            .filter(|o| o["type"] == "equal")
            .map(|o| o["text"].as_str().unwrap().chars().count())
            .sum();
        assert_eq!(equal_chars, 4, "equal 段总字符数必须等于 LCS 长度");
    }

    #[test]
    fn test_diff_strings_near_cap_no_shortcut() {
        // 5000 字上限内不做整段替换捷径：微差长文本应产出 equal 主体 + 局部增删。
        // 滚动数组后空间线性（旧全表实现此处约 100MB），长文本可安全计算。
        let long_a = format!("X{}Y", "A".repeat(4998));
        let long_b = format!("X{}Z", "A".repeat(4998));
        assert_eq!(long_a.chars().count(), 5000);
        let ops = diff_strings(&long_a, &long_b);
        assert!(
            ops.iter()
                .any(|o| o["type"] == "remove" && o["text"] == "Y"),
            "应包含 remove Y"
        );
        assert!(
            ops.iter().any(|o| o["type"] == "add" && o["text"] == "Z"),
            "应包含 add Z"
        );
        let equal_chars: usize = ops
            .iter()
            .filter(|o| o["type"] == "equal")
            .map(|o| o["text"].as_str().unwrap().chars().count())
            .sum();
        assert_eq!(equal_chars, 4999, "X + 4998 个 A 应全部保留");
    }

    // ── classify_diff 测试 ──

    #[test]
    fn test_classify_diff_no_difference() {
        let card = sample_card("c1", "Same Title");
        let (diff_type, significance, reason) = classify_diff(&card, &card);
        assert_eq!(diff_type, "none");
        assert!(!significance);
        assert_eq!(reason, "无显著差异");
    }

    #[test]
    fn test_classify_diff_structure_changed() {
        let a = sample_card("a1", "Card A");
        let mut b = sample_card("b1", "Card B");
        b.core_structure = "different structure".to_string();
        let (diff_type, significance, reason) = classify_diff(&a, &b);
        assert_eq!(diff_type, "structure");
        assert!(significance);
        assert_eq!(reason, "核心结构采用了不同的技术方案");
    }

    #[test]
    fn test_classify_diff_method_changed() {
        let a = sample_card("a2", "Card A");
        let mut b = sample_card("b2", "Card B");
        b.process_steps = "different steps".to_string();
        let (diff_type, significance, reason) = classify_diff(&a, &b);
        assert_eq!(diff_type, "method");
        assert!(significance);
        assert_eq!(reason, "工艺/实施步骤存在差异");
    }

    #[test]
    fn test_classify_diff_parameter_changed() {
        let a = sample_card("a3", "Card A");
        let mut b = sample_card("b3", "Card B");
        // core_structure 和 process_steps 相同，仅 technical_problem 不同
        b.technical_problem = "different problem".to_string();
        let (diff_type, significance, reason) = classify_diff(&a, &b);
        assert_eq!(diff_type, "parameter");
        assert!(!significance);
        assert_eq!(reason, "参数级差异，新颖性意义较低");
    }

    #[test]
    fn test_classify_diff_structure_takes_priority_over_method() {
        // 同时变更 core_structure 和 process_steps，应归类为 structure
        let a = sample_card("a4", "Card A");
        let mut b = sample_card("b4", "Card B");
        b.core_structure = "new structure".to_string();
        b.process_steps = "new steps".to_string();
        let (diff_type, significance, _) = classify_diff(&a, &b);
        assert_eq!(diff_type, "structure");
        assert!(significance);
    }

    // ── compute_minimal_diff 测试 ──

    #[test]
    fn test_compute_minimal_diff_identical_cards() {
        let card = sample_card("x1", "Same Title");
        let diff = compute_minimal_diff(&card, &card);
        // title diff 应为单个 equal 操作
        assert_eq!(diff["title"].as_array().unwrap().len(), 1);
        assert_eq!(diff["title"][0]["type"], "equal");
    }

    #[test]
    fn test_compute_minimal_diff_title_difference() {
        let a = sample_card("t1", "方案A");
        let b = sample_card("t2", "方案B");
        let diff = compute_minimal_diff(&a, &b);
        let title_ops = diff["title"].as_array().unwrap();
        let types: Vec<&str> = title_ops
            .iter()
            .map(|o| o["type"].as_str().unwrap())
            .collect();
        assert!(
            types.contains(&"remove") || types.contains(&"add"),
            "标题不同时应有差异操作"
        );
    }

    #[test]
    fn test_compute_minimal_diff_novelty_score_delta() {
        let a = sample_card("s1", "Card A");
        let mut b = sample_card("s2", "Card B");
        b.novelty_score = Some(0.6);
        let diff = compute_minimal_diff(&a, &b);
        let score = &diff["novelty_score"];
        assert!(score.is_object());
        assert_eq!(score["a"], "0.8"); // format!("{:.1}", 0.85) = "0.8"
        assert_eq!(score["b"], "0.6");
        assert_eq!(score["delta"], "-0.2");
    }

    #[test]
    fn test_compute_minimal_diff_no_novelty_score() {
        let mut a = sample_card("n1", "Card A");
        a.novelty_score = None;
        let mut b = sample_card("n2", "Card B");
        b.novelty_score = None;
        let diff = compute_minimal_diff(&a, &b);
        assert!(diff["novelty_score"].is_null());
    }

    #[tokio::test]
    async fn test_classify_diff_semantic_fallback_when_no_ai() {
        let a = sample_card("c1", "Card A");
        let mut b = sample_card("c2", "Card B");
        b.core_structure = "行星齿轮减速机构".to_string();

        let verdict = classify_diff_semantic(None, &a, &b).await;
        assert!(verdict.fallback_used);
        assert!(!verdict.semantically_equivalent);
        assert_eq!(verdict.diff_type, "structure");
        assert!(verdict.novelty_significance);

        let same = classify_diff_semantic(None, &a, &a).await;
        assert!(same.fallback_used);
        assert!(same.semantically_equivalent);
        assert_eq!(same.diff_type, "none");
    }

    #[test]
    fn test_parse_semantic_diff_json_valid_and_invalid() {
        let a = sample_card("c1", "Card A");
        let fallback = deterministic_semantic_verdict(&a, &a);

        let wrapped = "结果如下：\n```json\n{\"semantically_equivalent\":true,\"diff_type\":\"none\",\"novelty_significance\":false,\"novelty_reason\":\"同义表述\"}\n```";
        let parsed = parse_semantic_diff_json(wrapped, &fallback).expect("valid json");
        assert!(!parsed.fallback_used);
        assert!(parsed.semantically_equivalent);
        assert_eq!(parsed.diff_type, "none");
        assert_eq!(parsed.novelty_reason, "同义表述");

        assert!(parse_semantic_diff_json("not json", &fallback).is_none());
        assert!(parse_semantic_diff_json("}{", &fallback).is_none());
    }

    #[test]
    fn test_build_cad_prompt_from_card_contains_structured_fields() {
        let card = sample_card("cad-1", "双联液压阀体");
        let prompt = build_cad_prompt_from_card(&card);
        assert!(prompt.contains("FreeCAD"));
        assert!(prompt.contains("双联液压阀体"));
        assert!(prompt.contains("core structure"));
        assert!(prompt.contains("key relations"));
        assert!(prompt.contains("technical problem"));
    }

    #[test]
    fn test_build_deterministic_feature_cards_splits_and_persists() {
        let db = crate::db::Database::init(":memory:").expect("in-memory db");
        let idea_id = "idea-det-1";
        let idea = crate::patent::Idea {
            id: idea_id.to_string(),
            title: "液压阀".to_string(),
            description: "第一句：阀体带阶梯孔。第二句：阀芯轴向滑动。".to_string(),
            input_type: "text".to_string(),
            status: "draft".to_string(),
            analysis: String::new(),
            web_results: String::new(),
            patent_results: String::new(),
            novelty_score: None,
            created_at: "2026-11-18T00:00:00Z".to_string(),
            updated_at: "2026-11-18T00:00:00Z".to_string(),
            discussion_summary: String::new(),
        };
        db.insert_idea(&idea).expect("insert idea");
        let (analysis, keywords) = extract_context_hints(&db, idea_id, "初步分析");
        assert_eq!(analysis, "初步分析");
        assert!(keywords.is_empty());

        let cards = build_deterministic_feature_cards(
            idea_id,
            &idea.title,
            &idea.description,
            &analysis,
            &["液压阀".to_string(), "阶梯孔".to_string()],
        );
        assert_eq!(cards.len(), 2);
        for card in &cards {
            db.insert_feature_card(card).expect("insert card");
        }
        let stored = db.get_feature_cards_by_idea(idea_id).expect("query cards");
        assert_eq!(stored.len(), 2);
    }

    fn make_test_state() -> AppState {
        let db = crate::db::Database::init(":memory:").expect("in-memory db");
        let cad_root = std::env::temp_dir().join(format!(
            "innoforge-feature-cards-test-{}",
            uuid::Uuid::new_v4()
        ));
        let cad = crate::cad::CadService::new(cad_root, None).expect("cad service");
        AppState {
            db: std::sync::Arc::new(db),
            cad: std::sync::Arc::new(cad),
            config: std::sync::Arc::new(
                std::sync::RwLock::new(crate::routes::AppConfig::default()),
            ),
            pipeline_channels: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            db_path: ":memory:".to_string(),
            listen_port: 3000,
        }
    }

    async fn response_json(resp: axum::response::Response) -> serde_json::Value {
        use axum::body::HttpBody as _;
        let mut body = resp.into_body();
        let mut bytes = Vec::new();
        while let Some(chunk) = body.data().await {
            bytes.extend_from_slice(&chunk.expect("read body chunk"));
        }
        serde_json::from_slice(&bytes).expect("valid json response")
    }

    #[tokio::test]
    async fn test_api_feature_card_diff_semantic_and_generate_cad_prompts() {
        let state = make_test_state();
        let idea_id = "idea-route-1";
        let idea = crate::patent::Idea {
            id: idea_id.to_string(),
            title: "双联液压阀".to_string(),
            description: "阀体设置阶梯内孔。阀芯采用双密封环结构。".to_string(),
            input_type: "text".to_string(),
            status: "draft".to_string(),
            analysis: String::new(),
            web_results: String::new(),
            patent_results: String::new(),
            novelty_score: None,
            created_at: "2026-11-18T00:00:00Z".to_string(),
            updated_at: "2026-11-18T00:00:00Z".to_string(),
            discussion_summary: String::new(),
        };
        state.db.insert_idea(&idea).expect("insert idea");

        let gen_resp = api_generate_feature_cards(State(state.clone()), Path(idea_id.to_string()))
            .await
            .into_response();
        let gen_json = response_json(gen_resp).await;
        assert_eq!(gen_json["status"], "ok");
        let cards = gen_json["cards"].as_array().expect("cards array");
        let cad_prompts = gen_json["cad_prompts"]
            .as_array()
            .expect("cad_prompts array");
        assert_eq!(cards.len(), 2);
        assert_eq!(cad_prompts.len(), cards.len());
        assert!(cad_prompts[0]
            .as_str()
            .expect("prompt string")
            .contains("FreeCAD"));

        let id_a = cards[0]["id"].as_str().expect("card a id").to_string();
        let id_b = cards[1]["id"].as_str().expect("card b id").to_string();

        // 默认分支（semantic=None）保持向后兼容，不返回 semantic 扩展字段
        let det_resp = api_feature_card_diff(
            State(state.clone()),
            Query(FeatureCardDiffQuery {
                a: Some(id_a.clone()),
                b: Some(id_b.clone()),
                semantic: None,
            }),
        )
        .await
        .into_response();
        let det_json = response_json(det_resp).await;
        assert_eq!(det_json["status"], "ok");
        assert!(det_json.get("semantically_equivalent").is_none());
        assert!(det_json.get("fallback_used").is_none());

        // semantic=true 分支（无 fast key 时回退确定性语义判定并返回扩展字段）
        let sem_resp = api_feature_card_diff(
            State(state),
            Query(FeatureCardDiffQuery {
                a: Some(id_a),
                b: Some(id_b),
                semantic: Some(true),
            }),
        )
        .await
        .into_response();
        let sem_json = response_json(sem_resp).await;
        assert_eq!(sem_json["status"], "ok");
        assert_eq!(sem_json["fallback_used"], true);
        assert!(sem_json["semantically_equivalent"].is_boolean());
        assert!(sem_json["diff_type"].is_string());
    }
}
