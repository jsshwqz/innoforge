//! 多轮审查意见视图漂移检测与禁止反言一致性守卫。
//!
//! 读取同一案件（`case_id` / `patent_id`）的多轮 `oa_rounds` 历史记录，对比相邻轮次的
//! 审查意见（`oa_text`）、申请人答复（`response_text`）与结论（`result`），识别：
//! 1. **审查员隐含/明确认可的特征或权利要求**（提示不得误改或放弃已获认可的技术方案）；
//! 2. **本轮新增对比文件（如 D3/D4 及新公开号）与新增驳回理由**（聚焦攻防漂移）；
//! 3. **禁止反言（Estoppel）与历史口径一致性红线**（防止新答复与前轮限定、自认或技术效果主张矛盾）。
//!
//! 产出结构化 [`OaDriftReport`]，供 `GET /api/oa/:case_id/rounds/diff` 展示时间线与差异面板，
//! 并自动生成可注入 AI 提示词的历史一致性硬约束块。

use crate::db::oa::OaRound;
use crate::db::Database;
use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::sync::OnceLock;

/// 时间线单轮节点摘要（供前端渲染「第1轮 → 第2轮 → 驳回/复审」演化路径）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoundStageSummary {
    pub round_id: String,
    pub round_number: i32,
    pub round_type: String,
    pub stage_label: String,
    pub oa_date: Option<String>,
    pub result: Option<String>,
    pub prior_art_refs: Vec<String>,
    pub rejection_grounds: Vec<String>,
    pub claims_mentioned: Vec<String>,
    pub has_response: bool,
}

/// 后续轮次新引入的对比文件（例如二审或复审阶段新增 D3 / D4）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PriorArtDelta {
    /// 标准化标签，如 `"D3"`、`"D4"` 或公开号
    pub label: String,
    /// 关联专利公开号或标识
    pub doc_ref: String,
    /// 首次引入该对比文件的轮次号
    pub introduced_in_round: i32,
    /// 该对比文件在通知书中的原文摘录
    pub evidence_snippet: String,
}

/// 审查员明确或隐含认可的特征 / 权利要求。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptedFeature {
    /// 被认可的特征或权利要求描述
    pub feature: String,
    /// 涉及权利要求引用（如 `"权利要求3"`）
    pub claim_ref: String,
    /// `"explicit"`（通知书明确认可）| `"implicit"`（前轮驳回而本轮未再否定/已克服）
    pub basis: String,
    /// 检出轮次号
    pub round_number: i32,
    /// 通知书/对比依据摘录
    pub excerpt: String,
}

/// 跨轮次驳回理由漂移记录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RejectionGroundDrift {
    pub ground: String,
    /// `"new"` | `"persisting"` | `"overcome"`
    pub status: String,
    pub from_round: Option<i32>,
    pub to_round: i32,
}

/// 禁止反言与历史一致性红线。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsistencyRedline {
    pub redline_id: String,
    pub source_round: i32,
    /// `"estoppel_amendment"` | `"estoppel_admission"` | `"estoppel_distinction"` | `"estoppel_effect"`
    pub category: String,
    /// 前轮答复中的关键陈述摘录
    pub statement_excerpt: String,
    /// 面向代理人/AI 的强约束说明
    pub constraint_rule: String,
    /// 严禁与之矛盾的禁止事项
    pub prohibited_contradiction: String,
}

/// 相邻两轮 OA 之间的详细演化差异。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoundTransitionDiff {
    pub from_round: i32,
    pub to_round: i32,
    pub from_round_type: String,
    pub to_round_type: String,
    pub newly_introduced_prior_art: Vec<PriorArtDelta>,
    pub persisting_prior_art: Vec<String>,
    pub new_rejection_grounds: Vec<String>,
    pub overcome_rejection_grounds: Vec<String>,
    pub persisting_rejection_grounds: Vec<String>,
    pub claim_scope_changes: Vec<String>,
    pub examiner_accepted_features: Vec<AcceptedFeature>,
}

/// 跨轮次 OA 视图漂移与一致性守卫总报告。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OaDriftReport {
    pub case_id: String,
    pub total_rounds: usize,
    pub timeline: Vec<RoundStageSummary>,
    pub transitions: Vec<RoundTransitionDiff>,
    pub accepted_features: Vec<AcceptedFeature>,
    pub newly_introduced_prior_art: Vec<PriorArtDelta>,
    pub new_rejection_grounds: Vec<RejectionGroundDrift>,
    pub consistency_redlines: Vec<ConsistencyRedline>,
    pub prompt_constraints_block: String,
    pub summary: String,
}

/// 从数据库读取指定案件的所有 `oa_rounds` 并生成视图漂移报告。
pub fn analyze_case_drift(db: &Database, case_id: &str) -> Result<OaDriftReport> {
    let rounds = db.list_oa_rounds(case_id)?;
    Ok(build_round_drift_report(case_id, &rounds))
}

/// 基于有序 `oa_rounds` 列表构建跨轮次 OA 视图漂移与一致性守卫报告（纯确定性函数）。
pub fn build_round_drift_report(case_id: &str, rounds: &[OaRound]) -> OaDriftReport {
    let mut sorted: Vec<OaRound> = rounds.to_vec();
    sorted.sort_by_key(|r| r.round_number);

    if sorted.is_empty() {
        return OaDriftReport {
            case_id: case_id.to_string(),
            total_rounds: 0,
            timeline: Vec::new(),
            transitions: Vec::new(),
            accepted_features: Vec::new(),
            newly_introduced_prior_art: Vec::new(),
            new_rejection_grounds: Vec::new(),
            consistency_redlines: Vec::new(),
            prompt_constraints_block: String::new(),
            summary: "暂无多轮 OA 轮次记录".to_string(),
        };
    }

    let mut timeline = Vec::with_capacity(sorted.len());
    let mut round_refs: Vec<Vec<(String, String, String)>> = Vec::with_capacity(sorted.len());
    let mut round_grounds: Vec<Vec<String>> = Vec::with_capacity(sorted.len());
    let mut round_rejected_claims: Vec<Vec<String>> = Vec::with_capacity(sorted.len());
    let mut all_accepted: Vec<AcceptedFeature> = Vec::new();
    let mut all_redlines: Vec<ConsistencyRedline> = Vec::new();

    for round in &sorted {
        let oa_text = round.oa_text.as_deref().unwrap_or("");
        let resp_text = round.response_text.as_deref().unwrap_or("");
        let refs = extract_prior_art_refs(oa_text);
        let grounds = extract_rejection_grounds(oa_text);
        let claims = extract_claims_mentioned(oa_text);
        let rejected_claims = extract_rejected_claims(oa_text);

        let explicit_accepted = extract_explicit_accepted_features(oa_text, round.round_number);
        for item in explicit_accepted {
            push_unique_accepted(&mut all_accepted, item);
        }

        if let Some(res) = round.result.as_deref() {
            if res == "accepted" || res == "partially_accepted" {
                let basis_label = if res == "accepted" {
                    "该轮结果已标记为接受/授权"
                } else {
                    "该轮结果已标记为部分接受，部分权利要求/特征已获认可"
                };
                push_unique_accepted(
                    &mut all_accepted,
                    AcceptedFeature {
                        feature: basis_label.to_string(),
                        claim_ref: if claims.is_empty() {
                            "相关权项".to_string()
                        } else {
                            claims.join("、")
                        },
                        basis: "explicit".to_string(),
                        round_number: round.round_number,
                        excerpt: round
                            .notes
                            .clone()
                            .unwrap_or_else(|| basis_label.to_string()),
                    },
                );
            }
        }

        let redlines = extract_estoppel_redlines(resp_text, round.round_number);
        for rl in redlines {
            if !all_redlines
                .iter()
                .any(|existing| existing.statement_excerpt == rl.statement_excerpt)
            {
                all_redlines.push(rl);
            }
        }

        let ref_labels: Vec<String> = refs.iter().map(|(label, _, _)| label.clone()).collect();
        timeline.push(RoundStageSummary {
            round_id: round.id.clone(),
            round_number: round.round_number,
            round_type: round.round_type.clone(),
            stage_label: format_stage_label(round.round_number, &round.round_type),
            oa_date: round.oa_date.clone(),
            result: round.result.clone(),
            prior_art_refs: ref_labels,
            rejection_grounds: grounds.clone(),
            claims_mentioned: claims,
            has_response: !resp_text.trim().is_empty(),
        });

        round_refs.push(refs);
        round_grounds.push(grounds);
        round_rejected_claims.push(rejected_claims);
    }

    let mut seen_prior_art_keys: HashSet<String> = round_refs
        .first()
        .map(|items| {
            items
                .iter()
                .flat_map(|(label, doc_ref, _)| vec![label.clone(), doc_ref.clone()])
                .collect()
        })
        .unwrap_or_default();

    let mut transitions = Vec::new();
    let mut all_new_prior_art = Vec::new();
    let mut all_ground_drifts = Vec::new();

    for idx in 1..sorted.len() {
        let prev = &sorted[idx - 1];
        let curr = &sorted[idx];

        let prev_ref_keys: HashSet<String> = round_refs[idx - 1]
            .iter()
            .flat_map(|(label, doc_ref, _)| vec![label.clone(), doc_ref.clone()])
            .collect();

        let mut newly_introduced = Vec::new();
        let mut persisting_refs = Vec::new();
        for (label, doc_ref, snippet) in &round_refs[idx] {
            let is_known_in_prev = prev_ref_keys.contains(label) || prev_ref_keys.contains(doc_ref);
            let is_known_cumulatively =
                seen_prior_art_keys.contains(label) || seen_prior_art_keys.contains(doc_ref);
            if !is_known_in_prev && !is_known_cumulatively {
                let delta = PriorArtDelta {
                    label: label.clone(),
                    doc_ref: doc_ref.clone(),
                    introduced_in_round: curr.round_number,
                    evidence_snippet: snippet.clone(),
                };
                newly_introduced.push(delta.clone());
                if !all_new_prior_art
                    .iter()
                    .any(|p: &PriorArtDelta| p.label == delta.label && p.doc_ref == delta.doc_ref)
                {
                    all_new_prior_art.push(delta);
                }
            } else if !persisting_refs.contains(label) {
                persisting_refs.push(label.clone());
            }
        }
        for (label, doc_ref, _) in &round_refs[idx] {
            seen_prior_art_keys.insert(label.clone());
            seen_prior_art_keys.insert(doc_ref.clone());
        }

        let prev_g = &round_grounds[idx - 1];
        let curr_g = &round_grounds[idx];

        let new_grounds: Vec<String> = curr_g
            .iter()
            .filter(|g| !prev_g.contains(g))
            .cloned()
            .collect();
        let overcome_grounds: Vec<String> = prev_g
            .iter()
            .filter(|g| !curr_g.contains(g))
            .cloned()
            .collect();
        let persisting_grounds: Vec<String> = curr_g
            .iter()
            .filter(|g| prev_g.contains(g))
            .cloned()
            .collect();

        for g in &new_grounds {
            all_ground_drifts.push(RejectionGroundDrift {
                ground: g.clone(),
                status: "new".to_string(),
                from_round: Some(prev.round_number),
                to_round: curr.round_number,
            });
        }
        for g in &overcome_grounds {
            all_ground_drifts.push(RejectionGroundDrift {
                ground: g.clone(),
                status: "overcome".to_string(),
                from_round: Some(prev.round_number),
                to_round: curr.round_number,
            });
        }

        // 隐含认可推断：前轮被驳回的独立/从属权项若本轮不再被驳回（且本轮仍有其他驳回），
        // 或前轮驳回理由在本轮被克服，标注为隐含认可
        let mut transition_accepted = Vec::new();
        let curr_oa = curr.oa_text.as_deref().unwrap_or("");
        if !curr_oa.trim().is_empty() {
            for prev_claim in &round_rejected_claims[idx - 1] {
                if !round_rejected_claims[idx].contains(prev_claim)
                    && !round_rejected_claims[idx].is_empty()
                {
                    let af = AcceptedFeature {
                        feature: format!(
                            "{prev_claim} 在第{}轮未再被维持驳回（隐含认可其限缩或抗辩成立）",
                            curr.round_number
                        ),
                        claim_ref: prev_claim.clone(),
                        basis: "implicit".to_string(),
                        round_number: curr.round_number,
                        excerpt: format!(
                            "第{}轮曾驳回{prev_claim}，第{}轮仅针对 {:?} 提出异议",
                            prev.round_number, curr.round_number, round_rejected_claims[idx]
                        ),
                    };
                    push_unique_accepted(&mut all_accepted, af.clone());
                    transition_accepted.push(af);
                }
            }
            for og in &overcome_grounds {
                let af = AcceptedFeature {
                    feature: format!(
                        "原「{og}」缺陷已克服，审查员在第{}轮未再就此提出驳回",
                        curr.round_number
                    ),
                    claim_ref: "已克服理由".to_string(),
                    basis: "implicit".to_string(),
                    round_number: curr.round_number,
                    excerpt: format!(
                        "第{}轮存在「{og}」异议，第{}轮通知书中已消失",
                        prev.round_number, curr.round_number
                    ),
                };
                push_unique_accepted(&mut all_accepted, af.clone());
                transition_accepted.push(af);
            }
        }

        let claim_scope_changes =
            extract_claim_scope_changes(prev.response_text.as_deref().unwrap_or(""), curr_oa);

        transitions.push(RoundTransitionDiff {
            from_round: prev.round_number,
            to_round: curr.round_number,
            from_round_type: prev.round_type.clone(),
            to_round_type: curr.round_type.clone(),
            newly_introduced_prior_art: newly_introduced,
            persisting_prior_art: persisting_refs,
            new_rejection_grounds: new_grounds,
            overcome_rejection_grounds: overcome_grounds,
            persisting_rejection_grounds: persisting_grounds,
            claim_scope_changes,
            examiner_accepted_features: transition_accepted,
        });
    }

    let mut report = OaDriftReport {
        case_id: case_id.to_string(),
        total_rounds: sorted.len(),
        timeline,
        transitions,
        accepted_features: all_accepted,
        newly_introduced_prior_art: all_new_prior_art,
        new_rejection_grounds: all_ground_drifts,
        consistency_redlines: all_redlines,
        prompt_constraints_block: String::new(),
        summary: String::new(),
    };

    report.summary = format!(
        "共 {} 轮审查记录：检出 {} 项已认可特征、{} 篇新增对比文件、{} 项驳回理由漂移、{} 条禁止反言红线",
        report.total_rounds,
        report.accepted_features.len(),
        report.newly_introduced_prior_art.len(),
        report
            .new_rejection_grounds
            .iter()
            .filter(|g| g.status == "new")
            .count(),
        report.consistency_redlines.len()
    );
    report.prompt_constraints_block = format_prompt_constraints(&report);
    report
}

/// 将 [`OaDriftReport`] 格式化为可直接注入 AI 提示词的历史一致性硬约束文本。
pub fn format_prompt_constraints(report: &OaDriftReport) -> String {
    if report.total_rounds == 0 {
        return String::new();
    }
    if report.accepted_features.is_empty()
        && report.newly_introduced_prior_art.is_empty()
        && report.new_rejection_grounds.is_empty()
        && report.consistency_redlines.is_empty()
    {
        return String::new();
    }

    let mut out = String::new();
    out.push_str("## 多轮 OA 视图漂移与禁止反言约束（系统确定性校验）\n\n");

    let path_str: Vec<String> = report
        .timeline
        .iter()
        .map(|s| s.stage_label.clone())
        .collect();
    if !path_str.is_empty() {
        out.push_str(&format!("- 审查演化路径：{}\n", path_str.join(" → ")));
    }

    if !report.accepted_features.is_empty() {
        out.push_str("\n### 1. 审查员已隐含/明确认可的权利要求或特征（不得误改或削弱）\n");
        for item in report.accepted_features.iter().take(8) {
            out.push_str(&format!(
                "- [第{}轮·{}] {}（{}）\n",
                item.round_number,
                if item.basis == "explicit" {
                    "明确认可"
                } else {
                    "隐含认可"
                },
                item.feature,
                item.claim_ref
            ));
        }
    }

    let new_grounds: Vec<&RejectionGroundDrift> = report
        .new_rejection_grounds
        .iter()
        .filter(|g| g.status == "new")
        .collect();
    if !report.newly_introduced_prior_art.is_empty() || !new_grounds.is_empty() {
        out.push_str("\n### 2. 本轮新增对比文件与新增驳回理由（必须正面拆解回应）\n");
        for pa in report.newly_introduced_prior_art.iter().take(8) {
            out.push_str(&format!(
                "- [第{}轮新增对比文件] {}（{}）：{}\n",
                pa.introduced_in_round,
                pa.label,
                pa.doc_ref,
                truncate_chars(&pa.evidence_snippet, 100)
            ));
        }
        for g in new_grounds.iter().take(6) {
            out.push_str(&format!(
                "- [第{}轮新增驳回理由] {}\n",
                g.to_round, g.ground
            ));
        }
    }

    if !report.consistency_redlines.is_empty() {
        out.push_str("\n### 3. 禁止反言与历史口径一致性红线（严禁推翻前轮陈述）\n");
        for rl in report.consistency_redlines.iter().take(10) {
            out.push_str(&format!(
                "- [{}·第{}轮] {}（禁止事项：{}）\n",
                rl.redline_id, rl.source_round, rl.constraint_rule, rl.prohibited_contradiction
            ));
        }
    }

    truncate_chars(&out, 3600)
}

fn format_stage_label(round_number: i32, round_type: &str) -> String {
    let type_cn = match round_type {
        "first_rejection" | "first_exam" => "一审审查意见",
        "second_rejection" => "二审/驳回决定",
        "reexamination" | "reject_review" | "reexamination_request" => "驳回/复审",
        "reexamination_decision" => "复审决定",
        "abnormal" => "非正常答辩",
        "office_action" => "审查意见",
        other => other,
    };
    format!("第{round_number}轮·{type_cn}")
}

fn push_unique_accepted(list: &mut Vec<AcceptedFeature>, item: AcceptedFeature) {
    if !list
        .iter()
        .any(|x| x.feature == item.feature && x.claim_ref == item.claim_ref)
    {
        list.push(item);
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let prefix: String = text.chars().take(max_chars).collect();
        format!("{prefix}…")
    }
}

fn split_sentences(text: &str) -> Vec<String> {
    text.split(['\n', '。', '；', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn d_ref_regex() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(?:对比文件\s*(\d+)|(D\d{1,2}))").ok())
        .as_ref()
}

fn pub_num_regex() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b((?:CN|US|EP|JP|WO|KR)\d{6,13}[A-Z]?\d?)\b").ok())
        .as_ref()
}

fn claim_num_regex() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(?:权利要求|权|claim)\s*(\d+)").ok())
        .as_ref()
}

/// 从文本中提取对比文件标识（标准化 `D1`/`D2`/`D3`... 与公开号），附带所在句摘录。
pub fn extract_prior_art_refs(text: &str) -> Vec<(String, String, String)> {
    let mut items: Vec<(String, String, String)> = Vec::new();
    let mut seen_labels: HashSet<String> = HashSet::new();

    for sentence in split_sentences(text) {
        let mut pubs_in_sentence: Vec<String> = Vec::new();
        if let Some(re_pub) = pub_num_regex() {
            for cap in re_pub.captures_iter(&sentence) {
                if let Some(m) = cap.get(1) {
                    let p = m.as_str().to_uppercase();
                    if !pubs_in_sentence.contains(&p) {
                        pubs_in_sentence.push(p);
                    }
                }
            }
        }

        let mut d_labels_in_sentence: Vec<String> = Vec::new();
        if let Some(re_d) = d_ref_regex() {
            for cap in re_d.captures_iter(&sentence) {
                let normalized = if let Some(num) = cap.get(1) {
                    format!("D{}", num.as_str())
                } else if let Some(d) = cap.get(2) {
                    let prev_ok = sentence
                        .get(..d.start())
                        .and_then(|s| s.chars().next_back())
                        .is_none_or(|c| !c.is_ascii_alphanumeric());
                    let next_ok = sentence
                        .get(d.end()..)
                        .and_then(|s| s.chars().next())
                        .is_none_or(|c| !c.is_ascii_alphanumeric());
                    if !prev_ok || !next_ok {
                        continue;
                    }
                    d.as_str().to_uppercase()
                } else {
                    continue;
                };
                if !d_labels_in_sentence.contains(&normalized) {
                    d_labels_in_sentence.push(normalized);
                }
            }
        }

        for (idx, d_label) in d_labels_in_sentence.iter().enumerate() {
            let doc_ref = pubs_in_sentence
                .get(idx)
                .cloned()
                .or_else(|| pubs_in_sentence.first().cloned())
                .unwrap_or_else(|| d_label.clone());
            if seen_labels.insert(d_label.clone()) {
                items.push((
                    d_label.clone(),
                    doc_ref.clone(),
                    truncate_chars(&sentence, 140),
                ));
            }
        }

        for pub_num in pubs_in_sentence {
            let already_paired = items.iter().any(|(_, doc_ref, _)| doc_ref == &pub_num);
            if !already_paired && seen_labels.insert(pub_num.clone()) {
                items.push((pub_num.clone(), pub_num, truncate_chars(&sentence, 140)));
            }
        }
    }

    items
}

/// 从 OA 文本中提取法条/驳回理由。
pub fn extract_rejection_grounds(text: &str) -> Vec<String> {
    let rules: &[(&str, &[&str])] = &[
        (
            "新颖性(A22.2)",
            &["新颖性", "第22条第2款", "A22.2", "novelty"],
        ),
        (
            "创造性(A22.3)",
            &[
                "创造性",
                "第22条第3款",
                "A22.3",
                "突出的实质性特点",
                "显著的进步",
                "inventive step",
                "obvious",
            ],
        ),
        (
            "修改超范围(A33)",
            &[
                "修改超范围",
                "第33条",
                "A33",
                "超出原说明书",
                "added matter",
            ],
        ),
        (
            "权利要求不清楚(A26.4)",
            &["不清楚", "保护范围不清楚", "第26条第4款", "A26.4"],
        ),
        (
            "得不到说明书支持(A26.4)",
            &["得不到说明书支持", "得不到说明书的支持", "以说明书为依据"],
        ),
        (
            "公开不充分(A26.3)",
            &["公开不充分", "未充分公开", "第26条第3款", "A26.3"],
        ),
        (
            "缺乏必要技术特征(R20.2)",
            &["必要技术特征", "第20条第2款", "R20.2", "细则第23条"],
        ),
        ("单一性(A31.1)", &["单一性", "第31条", "A31.1"]),
        ("实用性(A22.4)", &["实用性", "第22条第4款", "A22.4"]),
    ];

    let mut found = Vec::new();
    for sentence in split_sentences(text) {
        let is_purely_positive = (sentence.contains("具备新颖性")
            || sentence.contains("具有新颖性")
            || sentence.contains("具备创造性")
            || sentence.contains("具有创造性")
            || sentence.contains("予以接受"))
            && !sentence.contains("不具备")
            && !sentence.contains("不具有")
            && !sentence.contains("缺乏")
            && !sentence.contains("无新颖性")
            && !sentence.contains("无创造性");
        if is_purely_positive {
            continue;
        }
        let lower = sentence.to_lowercase();
        for (label, markers) in rules {
            if markers
                .iter()
                .any(|m| sentence.contains(m) || lower.contains(&m.to_lowercase()))
                && !found.iter().any(|x| x == label)
            {
                found.push((*label).to_string());
            }
        }
    }
    found
}

fn extract_claims_mentioned(text: &str) -> Vec<String> {
    let mut set = BTreeSet::new();
    if let Some(re) = claim_num_regex() {
        for cap in re.captures_iter(text) {
            if let Some(m) = cap.get(1) {
                if let Ok(n) = m.as_str().parse::<u32>() {
                    set.insert(n);
                }
            }
        }
    }
    set.into_iter().map(|n| format!("权利要求{n}")).collect()
}

fn extract_rejected_claims(text: &str) -> Vec<String> {
    let negative_markers = [
        "不具备新颖性",
        "不具备创造性",
        "不具有新颖性",
        "不具有创造性",
        "不符合",
        "驳回",
        "不清楚",
        "得不到",
        "超范围",
        "rejected",
        "lacks novelty",
        "lacks inventive",
    ];
    let positive_markers = [
        "具备新颖性",
        "具备创造性",
        "具有新颖性",
        "具有创造性",
        "未指出",
        "予以接受",
        "可以接受",
        "allowable",
    ];

    let mut rejected = BTreeSet::new();
    for sentence in split_sentences(text) {
        let has_neg = negative_markers.iter().any(|m| sentence.contains(m));
        let has_pos = positive_markers.iter().any(|m| sentence.contains(m));
        if has_neg && !has_pos {
            if let Some(re) = claim_num_regex() {
                for cap in re.captures_iter(&sentence) {
                    if let Some(m) = cap.get(1) {
                        if let Ok(n) = m.as_str().parse::<u32>() {
                            rejected.insert(n);
                        }
                    }
                }
            }
        }
    }
    rejected
        .into_iter()
        .map(|n| format!("权利要求{n}"))
        .collect()
}

fn extract_explicit_accepted_features(oa_text: &str, round_number: i32) -> Vec<AcceptedFeature> {
    let accept_markers = [
        "具备新颖性",
        "具备创造性",
        "具有新颖性",
        "具有创造性",
        "未指出",
        "未被公开",
        "未公开该特征",
        "未被对比文件公开",
        "均未公开",
        "予以认可",
        "予以接受",
        "可以接受",
        "符合专利法",
        "allowable",
        "novel and inventive",
    ];
    let negate_markers = ["不具备", "不具有", "不符合", "并未认可", "不能认为具备"];

    let mut out = Vec::new();
    for sentence in split_sentences(oa_text) {
        let has_accept = accept_markers.iter().any(|m| sentence.contains(m));
        let has_negate = negate_markers.iter().any(|m| sentence.contains(m));
        if !has_accept || has_negate {
            continue;
        }
        let claims = extract_claims_mentioned(&sentence);
        let claim_ref = if claims.is_empty() {
            "技术特征".to_string()
        } else {
            claims.join("、")
        };
        out.push(AcceptedFeature {
            feature: truncate_chars(&sentence, 120),
            claim_ref,
            basis: "explicit".to_string(),
            round_number,
            excerpt: truncate_chars(&sentence, 180),
        });
    }
    out
}

/// 从前轮申请人答复文本中提取禁止反言与一致性红线。
pub fn extract_estoppel_redlines(
    response_text: &str,
    round_number: i32,
) -> Vec<ConsistencyRedline> {
    if response_text.trim().is_empty() {
        return Vec::new();
    }

    let amendment_markers = [
        "修改为",
        "增加特征",
        "引入",
        "限定为",
        "并入权利要求",
        "加入",
        "删除了",
        "amended to",
        "limited to",
    ];
    let admission_markers = [
        "承认",
        "不否认",
        "确实公开了",
        "公开了特征",
        "不再争辩",
        "放弃",
        "concede",
        "admit",
    ];
    let distinction_markers = [
        "核心区别特征",
        "区别特征在于",
        "区别在于",
        "协同效应",
        "技术效果在于",
        "实质区别",
        "非显而易见",
        "actual technical problem",
        "technical effect",
    ];

    let mut out = Vec::new();
    let mut seq = 1usize;

    for sentence in split_sentences(response_text) {
        if sentence.chars().count() < 8 {
            continue;
        }
        let excerpt = truncate_chars(&sentence, 120);
        if amendment_markers.iter().any(|m| sentence.contains(m)) {
            out.push(ConsistencyRedline {
                redline_id: format!("R{round_number}-E{seq}"),
                source_round: round_number,
                category: "estoppel_amendment".to_string(),
                statement_excerpt: excerpt.clone(),
                constraint_rule: format!(
                    "第{round_number}轮已作权利要求限缩/修改：「{excerpt}」，本轮不得回退删除该限定或扩大保护范围"
                ),
                prohibited_contradiction: format!(
                    "严禁删除、宽化或否定第{round_number}轮已承诺引入的限定特征"
                ),
            });
            seq += 1;
        } else if admission_markers.iter().any(|m| sentence.contains(m)) {
            out.push(ConsistencyRedline {
                redline_id: format!("R{round_number}-E{seq}"),
                source_round: round_number,
                category: "estoppel_admission".to_string(),
                statement_excerpt: excerpt.clone(),
                constraint_rule: format!(
                    "第{round_number}轮已对现有技术公开事实作出自认：「{excerpt}」，后续轮次不得推翻"
                ),
                prohibited_contradiction: format!(
                    "严禁在后续陈述中否认第{round_number}轮已承认公开的对比文件特征"
                ),
            });
            seq += 1;
        } else if distinction_markers.iter().any(|m| sentence.contains(m)) {
            out.push(ConsistencyRedline {
                redline_id: format!("R{round_number}-E{seq}"),
                source_round: round_number,
                category: "estoppel_distinction".to_string(),
                statement_excerpt: excerpt.clone(),
                constraint_rule: format!(
                    "第{round_number}轮已锚定核心区别特征/技术效果口径：「{excerpt}」，本轮论证须保持技术机理一致"
                ),
                prohibited_contradiction: format!(
                    "严禁提出与第{round_number}轮所述区别特征定义或技术效果相矛盾的解释"
                ),
            });
            seq += 1;
        }
        if seq > 8 {
            break;
        }
    }

    out
}

fn extract_claim_scope_changes(prev_response: &str, curr_oa: &str) -> Vec<String> {
    let mut changes = Vec::new();
    let markers = ["修改为", "并入", "增加", "删除", "限缩", "重编", "修改后"];
    for sentence in split_sentences(prev_response) {
        if markers.iter().any(|m| sentence.contains(m)) {
            changes.push(format!("前轮答复修改：{}", truncate_chars(&sentence, 100)));
            if changes.len() >= 4 {
                break;
            }
        }
    }
    for sentence in split_sentences(curr_oa) {
        if (sentence.contains("修改") || sentence.contains("权利要求"))
            && (sentence.contains("超范围")
                || sentence.contains("基于")
                || sentence.contains("针对"))
        {
            changes.push(format!("本轮审查基准：{}", truncate_chars(&sentence, 100)));
            if changes.len() >= 7 {
                break;
            }
        }
    }
    changes
}

/// 拟提交答复草稿与前轮禁止反言红线的冲突项。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EstoppelConflict {
    pub redline_id: String,
    pub source_round: i32,
    pub category: String,
    pub prior_statement: String,
    pub draft_excerpt: String,
    pub reason: String,
}

fn split_clauses(text: &str) -> Vec<String> {
    text.split(['\n', '。', '；', ';', '，', ','])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn extract_d_labels(text: &str) -> HashSet<String> {
    let mut labels = HashSet::new();
    if let Some(re) = d_ref_regex() {
        for cap in re.captures_iter(text) {
            if let Some(num) = cap.get(1) {
                labels.insert(format!("D{}", num.as_str()));
            } else if let Some(d) = cap.get(2) {
                let prev_ok = text
                    .get(..d.start())
                    .and_then(|s| s.chars().next_back())
                    .is_none_or(|c| !c.is_ascii_alphanumeric());
                let next_ok = text
                    .get(d.end()..)
                    .and_then(|s| s.chars().next())
                    .is_none_or(|c| !c.is_ascii_alphanumeric());
                if prev_ok && next_ok {
                    labels.insert(d.as_str().to_uppercase());
                }
            }
        }
    }
    labels
}

fn extract_redline_anchor(redline: &ConsistencyRedline) -> String {
    let stmt = redline.statement_excerpt.as_str();
    let markers: &[&str] = match redline.category.as_str() {
        "estoppel_admission" => &[
            "确实公开了",
            "公开了特征",
            "公开了",
            "不否认",
            "承认",
            "不再争辩",
            "放弃",
            "concede",
            "admit",
        ],
        "estoppel_amendment" => &[
            "增加特征",
            "修改为",
            "限定为",
            "并入权利要求",
            "加入",
            "增加",
            "引入",
            "删除了",
            "amended to",
            "limited to",
        ],
        _ => &[
            "核心区别特征在于",
            "区别特征在于",
            "区别在于",
            "技术效果在于",
            "协同效应",
            "实质区别",
            "在于",
        ],
    };

    let mut tail = stmt;
    for m in markers {
        if let Some(pos) = stmt.find(m) {
            if let Some(after) = stmt.get(pos + m.len()..) {
                let trimmed = after.trim();
                if !trimmed.is_empty() {
                    tail = trimmed;
                    break;
                }
            }
        }
    }

    let mut cleaned = tail.to_string();
    for noise in [
        "的限定",
        "限定特征",
        "特征",
        "限定",
        "结构",
        "所述",
        "采用",
        "了",
    ] {
        cleaned = cleaned.replace(noise, "");
    }
    // 去掉 D1/对比文件1 前缀，保留核心技术特征词
    if let Some(re) = d_ref_regex() {
        cleaned = re.replace_all(&cleaned, "").to_string();
    }
    if let Some(re_c) = claim_num_regex() {
        cleaned = re_c.replace_all(&cleaned, "").to_string();
    }
    cleaned.trim().to_string()
}

fn anchor_matches_clause(anchor: &str, clause: &str) -> bool {
    let norm_anchor: String = anchor
        .chars()
        .filter(|c| !c.is_whitespace() && !['，', '。', '、', ',', '.', '：', ':'].contains(c))
        .collect();
    let norm_clause: String = clause
        .chars()
        .filter(|c| !c.is_whitespace() && !['，', '。', '、', ',', '.', '：', ':'].contains(c))
        .collect();
    if norm_anchor.chars().count() < 2 || norm_clause.chars().count() < 2 {
        return false;
    }
    let anchor_lower = norm_anchor.to_ascii_lowercase();
    let clause_lower = norm_clause.to_ascii_lowercase();
    if clause_lower.contains(&anchor_lower) {
        return true;
    }

    let a_chars: Vec<char> = anchor_lower.chars().collect();
    let a_bigrams: HashSet<String> = a_chars.windows(2).map(|w| w.iter().collect()).collect();
    if a_bigrams.is_empty() {
        return false;
    }
    let c_chars: Vec<char> = clause_lower.chars().collect();
    let c_bigrams: HashSet<String> = c_chars.windows(2).map(|w| w.iter().collect()).collect();
    let shared = a_bigrams.intersection(&c_bigrams).count();
    let ratio = (shared as f64) / (a_bigrams.len() as f64);
    shared >= 2 && ratio >= 0.30
}

/// 确定性检测新答复草稿是否与历史轮次 `consistency_redlines`（自认/限缩/区别特征口径）相矛盾。
pub fn detect_estoppel_conflicts(
    response_draft: &str,
    drift_report: &OaDriftReport,
) -> Vec<EstoppelConflict> {
    if response_draft.trim().is_empty() || drift_report.consistency_redlines.is_empty() {
        return Vec::new();
    }

    const ADMISSION_NEGATIONS: &[&str] = &[
        "未公开",
        "没有公开",
        "并未公开",
        "未披露",
        "并未披露",
        "不存在",
        "均未公开",
        "均未披露",
        "不承认",
        "不再承认",
        "否认",
        "未记载",
        "not disclose",
        "does not disclose",
        "did not disclose",
    ];
    const AMENDMENT_ROLLBACKS: &[&str] = &[
        "删除",
        "取消",
        "删去",
        "去掉",
        "不再限定",
        "取消限定",
        "删除限定",
        "放弃限定",
        "恢复原",
        "扩大保护范围",
        "撤回修改",
        "remove",
        "delete",
    ];
    const DISTINCTION_NEGATIONS: &[&str] = &[
        "并非区别",
        "不是区别",
        "不在于",
        "并无协同效应",
        "无协同效应",
        "并不能",
        "无法抑制",
        "不具有该效果",
        "并非采用",
    ];

    let clauses = split_clauses(response_draft);
    let mut conflicts = Vec::new();
    let mut seen_ids = HashSet::new();

    for rl in &drift_report.consistency_redlines {
        let anchor = extract_redline_anchor(rl);
        let prior_d_labels = extract_d_labels(&rl.statement_excerpt);

        for clause in &clauses {
            let clause_lower = clause.to_ascii_lowercase();
            let matched = match rl.category.as_str() {
                "estoppel_admission" => {
                    let has_neg = ADMISSION_NEGATIONS
                        .iter()
                        .any(|m| clause.contains(m) || clause_lower.contains(m));
                    if !has_neg || !anchor_matches_clause(&anchor, clause) {
                        false
                    } else if prior_d_labels.is_empty() {
                        true
                    } else {
                        let clause_d = extract_d_labels(clause);
                        clause_d.is_empty() || prior_d_labels.iter().any(|dl| clause_d.contains(dl))
                    }
                }
                "estoppel_amendment" => {
                    let has_rollback = AMENDMENT_ROLLBACKS
                        .iter()
                        .any(|m| clause.contains(m) || clause_lower.contains(m));
                    has_rollback && anchor_matches_clause(&anchor, clause)
                }
                "estoppel_distinction" => {
                    let has_neg = DISTINCTION_NEGATIONS
                        .iter()
                        .any(|m| clause.contains(m) || clause_lower.contains(m));
                    has_neg && anchor_matches_clause(&anchor, clause)
                }
                _ => false,
            };

            if matched && seen_ids.insert(rl.redline_id.clone()) {
                conflicts.push(EstoppelConflict {
                    redline_id: rl.redline_id.clone(),
                    source_round: rl.source_round,
                    category: rl.category.clone(),
                    prior_statement: rl.statement_excerpt.clone(),
                    draft_excerpt: truncate_chars(clause, 100),
                    reason: rl.constraint_rule.clone(),
                });
                break;
            }
        }
    }

    conflicts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_round(
        round_number: i32,
        round_type: &str,
        oa_date: &str,
        oa_text: &str,
        response_text: Option<&str>,
        result: Option<&str>,
    ) -> OaRound {
        OaRound {
            id: format!("round_{round_number}"),
            patent_id: "CN202400000001A".to_string(),
            round_number,
            round_type: round_type.to_string(),
            oa_date: Some(oa_date.to_string()),
            oa_text: Some(oa_text.to_string()),
            response_date: response_text.map(|_| oa_date.to_string()),
            response_text: response_text.map(String::from),
            result: result.map(String::from),
            result_date: None,
            strategy_used: None,
            quality_score: None,
            notes: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn detects_newly_introduced_prior_art_d3_d4_and_new_grounds() {
        let r1 = mock_round(
            1,
            "first_rejection",
            "2026-01-10",
            "一通：权利要求1和权利要求2相对于对比文件1（CN101111111A）和D2（CN102222222A）不具备创造性，专利法第22条第3款。\n权利要求1不具备新颖性。",
            Some(
                "第1轮答复：承认D1公开了基础壳体结构。我们将权利要求1修改为限定双层隔热腔体结构，并将权利要求2的密封阀特征并入权利要求1。",
            ),
            Some("partially_accepted"),
        );
        let r2 = mock_round(
            2,
            "second_rejection",
            "2026-04-15",
            "二通：经审查，权利要求2具备新颖性。但审查员补充检索到对比文件3（CN103333333A）和D4（US9988776B2），认为权利要求1相对于D1、D3和D4的结合仍不具备创造性，同时认为修改超范围（专利法第33条）。",
            None,
            None,
        );

        let report = build_round_drift_report("CN202400000001A", &[r1, r2]);
        assert_eq!(report.total_rounds, 2);
        assert_eq!(report.timeline.len(), 2);
        assert_eq!(report.transitions.len(), 1);

        let new_labels: Vec<&str> = report
            .newly_introduced_prior_art
            .iter()
            .map(|p| p.label.as_str())
            .collect();
        assert!(
            new_labels.contains(&"D3"),
            "should detect D3: {new_labels:?}"
        );
        assert!(
            new_labels.contains(&"D4"),
            "should detect D4: {new_labels:?}"
        );

        let new_docs: Vec<&str> = report
            .newly_introduced_prior_art
            .iter()
            .map(|p| p.doc_ref.as_str())
            .collect();
        assert!(new_docs.contains(&"CN103333333A"));
        assert!(new_docs.contains(&"US9988776B2"));

        let tr = &report.transitions[0];
        assert!(tr
            .new_rejection_grounds
            .contains(&"修改超范围(A33)".to_string()));
        assert!(tr
            .overcome_rejection_grounds
            .contains(&"新颖性(A22.2)".to_string()));
        assert!(tr.persisting_prior_art.contains(&"D1".to_string()));
    }

    #[test]
    fn detects_accepted_features_and_estoppel_redlines() {
        let r1 = mock_round(
            1,
            "first_rejection",
            "2026-02-01",
            "一审：权利要求1、权利要求3不具备创造性（D1）。",
            Some(
                "一审意见陈述：申请人承认D1公开了传动轴。核心区别特征在于采用非对称阻尼槽。我们将权利要求1修改为增加非对称阻尼槽宽度比0.3-0.5限定。技术效果在于抑制高频共振。",
            ),
            None,
        );
        let r2 = mock_round(
            2,
            "reexamination",
            "2026-06-01",
            "驳回决定：权利要求3具备创造性，予以接受；权利要求1仍相对于D1+D3不具备创造性。",
            None,
            None,
        );

        let report = build_round_drift_report("CASE-MOCK-002", &[r1, r2]);
        assert!(
            report
                .accepted_features
                .iter()
                .any(|f| f.basis == "explicit" && f.claim_ref.contains("权利要求3")),
            "should detect explicit acceptance of claim 3: {:?}",
            report.accepted_features
        );
        assert!(
            report
                .consistency_redlines
                .iter()
                .any(|r| r.category == "estoppel_admission"
                    && r.statement_excerpt.contains("传动轴")),
            "should extract estoppel admission redline"
        );
        assert!(
            report
                .consistency_redlines
                .iter()
                .any(|r| r.category == "estoppel_amendment"
                    && r.statement_excerpt.contains("0.3-0.5")),
            "should extract estoppel amendment redline"
        );
        assert!(
            report
                .prompt_constraints_block
                .contains("禁止反言与历史口径一致性红线"),
            "prompt constraint block should contain redline section"
        );
        assert!(report.prompt_constraints_block.contains("D3"));
    }

    #[test]
    fn analyzes_case_drift_from_sqlite_oa_rounds() {
        let db = Database::init(":memory:").expect("in-memory db");
        let r1 = mock_round(
            1,
            "first_rejection",
            "2026-01-01",
            "一通：权利要求1相对于D1（CN100000001A）不具备新颖性。",
            Some("答复：将权利要求1修改为加入陶瓷隔膜涂层。"),
            None,
        );
        let r2 = mock_round(
            2,
            "second_rejection",
            "2026-03-01",
            "二通：补充对比文件3（CN100000003A），认为权利要求1相对于D1和D3不具备创造性。",
            None,
            None,
        );
        db.save_oa_round(&r1).expect("save round 1");
        db.save_oa_round(&r2).expect("save round 2");

        let report = analyze_case_drift(&db, "CN202400000001A").expect("analyze drift");
        assert_eq!(report.total_rounds, 2);
        assert!(report
            .newly_introduced_prior_art
            .iter()
            .any(|p| p.label == "D3" && p.doc_ref == "CN100000003A"));
        assert!(!report.consistency_redlines.is_empty());
    }

    #[test]
    fn detects_estoppel_contradictions_in_new_response_draft() {
        let r1 = mock_round(
            1,
            "first_rejection",
            "2026-02-01",
            "一审：权利要求1不具备创造性（D1）。",
            Some(
                "一审意见陈述：申请人承认D1公开了传动轴。我们将权利要求1修改为增加非对称阻尼槽宽度比0.3-0.5限定。",
            ),
            None,
        );
        let report = build_round_drift_report("CASE-ESTOPPEL-1", &[r1]);

        let contradicting_draft = "本轮答复：经核查，D1并未公开传动轴。同时现删除非对称阻尼槽宽度比0.3-0.5的限定，恢复原保护范围。";
        let conflicts = detect_estoppel_conflicts(contradicting_draft, &report);
        assert_eq!(
            conflicts.len(),
            2,
            "应同时检出自认反悔与修改回退矛盾: {conflicts:?}"
        );
        assert!(conflicts.iter().any(|c| c.category == "estoppel_admission"));
        assert!(conflicts.iter().any(|c| c.category == "estoppel_amendment"));

        let consistent_draft =
            "本轮答复：虽然D1公开了传动轴，但D1与D3均未公开非对称阻尼槽宽度比0.3-0.5的限定特征。";
        let no_conflicts = detect_estoppel_conflicts(consistent_draft, &report);
        assert!(
            no_conflicts.is_empty(),
            "口径一致的答复草稿不应误报禁止反言冲突: {no_conflicts:?}"
        );
    }
}
