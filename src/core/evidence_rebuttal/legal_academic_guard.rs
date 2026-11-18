//! 专利法第 33 条超范围 + 论文返修一致性 + 涉密禁词守卫（`src/core/evidence_rebuttal/legal_academic_guard.rs`）

use std::collections::BTreeSet;
use std::sync::OnceLock;

use anyhow::Result;
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// 第 33 条 / 论文返修一致性与涉密禁词核查报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeConsistencyReport {
    pub mode: String,
    pub passed: bool,
    pub violations: Vec<String>,
    pub warnings: Vec<String>,
    pub new_terms_detected: Vec<String>,
    pub prohibited_hits: Vec<String>,
}

fn compile_regex(slot: &'static OnceLock<Option<Regex>>, pattern: &str) -> Option<&'static Regex> {
    slot.get_or_init(|| Regex::new(pattern).ok()).as_ref()
}

/// 涉密标识与绝对化幻觉禁词表
const PROHIBITED_TERMS: &[&str] = &[
    "绝密",
    "未授权绝密",
    "TOP SECRET",
    "INTERNAL ONLY",
    "100%无懈可击",
    "必然授权",
    "编造数据",
];

fn normalize_text(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join("")
        .to_ascii_lowercase()
}

fn scan_prohibited_terms(text: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let mut hits = Vec::new();
    for &term in PROHIBITED_TERMS {
        if text.contains(term) || lower.contains(&term.to_ascii_lowercase()) {
            hits.push(term.to_string());
        }
    }
    hits
}

/// 从权利要求文本中提取数值限定表达式与技术特征词组
fn extract_feature_tokens(claim_text: &str) -> Vec<String> {
    static RE_NUM_UNIT: OnceLock<Option<Regex>> = OnceLock::new();
    static RE_SPLIT_STOP: OnceLock<Option<Regex>> = OnceLock::new();

    let mut tokens = BTreeSet::new();

    // 1. 提取带单位的数值或数值范围限定（如 850℃、10%-20%、3.5MPa）
    if let Some(re) = compile_regex(
        &RE_NUM_UNIT,
        r"\d+(?:\.\d+)?(?:\s*[-~～]\s*\d+(?:\.\d+)?)?\s*(?:℃|°C|°F|K|%|wt%|vol%|nm|μm|um|mm|cm|m|ms|s|min|h|Hz|kHz|MHz|GHz|Pa|kPa|MPa|GPa|V|mV|A|mA|W|kW|g|kg|mg|mol|rpm)",
    ) {
        for m in re.find_iter(claim_text) {
            let t = m.as_str().trim();
            if !t.is_empty() {
                tokens.insert(t.to_string());
            }
        }
    }

    // 2. 按标点与常见专利连接虚词切分技术特征词组
    if let Some(re_split) = compile_regex(
        &RE_SPLIT_STOP,
        r"(?:[，,；;。\.、\(\)（）\[\]【】\s]+|其特征在于|其中|所述|一种|包括|具有|还包含|还包括|设有|采用|设置|连接|构成|用于|以及|并且|或者|至少|大于|小于|超过|低于|之间|表面|工作温度|温度为|为|与|和|及|的|在|中|至|由|将|对)",
    ) {
        for part in re_split.split(claim_text) {
            let trimmed = part.trim();
            if trimmed.chars().count() >= 2 {
                tokens.insert(trimmed.to_string());
            }
        }
    }

    tokens.into_iter().collect()
}

/// 专利模式：专利法第 33 条修改超范围与涉密禁词核查
///
/// 提取 `amended_claim` 相对 `original_claim` 新增的技术特征词组/数值限定，
/// 在原申请文件（`knowledge_docs` + `doc_chunks`）中做逐词组与数值范围原文字面核查。
pub fn check_patent_art33_scope(
    conn: &Connection,
    own_doc_id: &str,
    original_claim: &str,
    amended_claim: &str,
    draft_rebuttal: &str,
) -> Result<ScopeConsistencyReport> {
    let mut violations = Vec::new();
    let warnings = Vec::new();
    let mut new_terms_detected = Vec::new();

    // 加载原申请文件全文语料
    let doc_row: Option<(String, String, String)> = conn
        .query_row(
            "SELECT abstract_text, claims_json, fulltext_raw
             FROM knowledge_docs
             WHERE doc_id = ?1
             LIMIT 1",
            params![own_doc_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;

    let mut raw_corpus = String::from(original_claim);
    if let Some((abs, claims, full)) = doc_row {
        raw_corpus.push('\n');
        raw_corpus.push_str(&abs);
        raw_corpus.push('\n');
        raw_corpus.push_str(&claims);
        raw_corpus.push('\n');
        raw_corpus.push_str(&full);
    }

    let mut stmt = conn.prepare("SELECT raw_content FROM doc_chunks WHERE doc_id = ?1")?;
    let chunk_rows = stmt.query_map(params![own_doc_id], |r| r.get::<_, String>(0))?;
    for c in chunk_rows {
        raw_corpus.push('\n');
        raw_corpus.push_str(&c?);
    }

    let norm_corpus = normalize_text(&raw_corpus);
    let norm_orig_claim = normalize_text(original_claim);

    let amended_tokens = extract_feature_tokens(amended_claim);
    for token in amended_tokens {
        let norm_tok = normalize_text(&token);
        if norm_tok.is_empty() {
            continue;
        }
        // 仅核查相对原权利要求新增的词组/数值限定
        if !norm_orig_claim.contains(&norm_tok) {
            new_terms_detected.push(token.clone());
            if !norm_corpus.contains(&norm_tok) {
                violations.push(format!(
                    "违反专利法第 33 条（修改超范围）：新增技术特征/数值限定「{}」在原权利要求及原说明书全文中均无字面记载",
                    token
                ));
            }
        }
    }

    // 涉密禁词与幻觉套话检查
    let mut prohibited_hits = scan_prohibited_terms(amended_claim);
    for hit in scan_prohibited_terms(draft_rebuttal) {
        if !prohibited_hits.contains(&hit) {
            prohibited_hits.push(hit);
        }
    }
    for hit in &prohibited_hits {
        violations.push(format!("命中涉密/违规禁词：「{}」", hit));
    }

    let passed = violations.is_empty();
    Ok(ScopeConsistencyReport {
        mode: "patent_art33".to_string(),
        passed,
        violations,
        warnings,
        new_terms_detected,
        prohibited_hits,
    })
}

/// 学术论文 Rebuttal 模式：核查审稿回复信中声称修改的章节/图表与补充实验一致性
pub fn check_paper_rebuttal_consistency(
    conn: &Connection,
    manuscript_doc_id: &str,
    response_letter: &str,
) -> Result<ScopeConsistencyReport> {
    static RE_PAPER_ANCHOR: OnceLock<Option<Regex>> = OnceLock::new();
    static RE_EXP_ANCHOR: OnceLock<Option<Regex>> = OnceLock::new();

    let mut violations = Vec::new();
    let warnings = Vec::new();
    let mut new_terms_detected = Vec::new();

    // 1. 核查回复信中声称的论文章节/图表锚点
    if let Some(re) = compile_regex(&RE_PAPER_ANCHOR, r"\b(Paper:[A-Za-z0-9_.\-]+)") {
        let mut seen = BTreeSet::new();
        for cap in re.captures_iter(response_letter) {
            if let Some(m) = cap.get(1) {
                let tag = m.as_str();
                if seen.insert(tag.to_string()) {
                    new_terms_detected.push(tag.to_string());
                    let exists: Option<i64> = conn
                        .query_row(
                            "SELECT chunk_id FROM doc_chunks WHERE doc_id = ?1 AND section_tag = ?2 LIMIT 1",
                            params![manuscript_doc_id, tag],
                            |r| r.get(0),
                        )
                        .optional()?;
                    if exists.is_none() {
                        violations.push(format!(
                            "论文返修一致性失配：回复信声称修改了「{}」，但修订稿 {} 切片中不存在该章节/图表锚点",
                            tag, manuscript_doc_id
                        ));
                    }
                }
            }
        }
    }

    // 2. 核查回复信中引用的补充实验锚点 Exp:{exp_id}
    if let Some(re) = compile_regex(&RE_EXP_ANCHOR, r"\bExp:([A-Za-z0-9_\-]+)") {
        let mut seen = BTreeSet::new();
        for cap in re.captures_iter(response_letter) {
            if let Some(m) = cap.get(1) {
                let exp_id = m.as_str();
                if seen.insert(exp_id.to_string()) {
                    let tag = format!("Exp:{}", exp_id);
                    new_terms_detected.push(tag);
                    let row: Option<String> = conn
                        .query_row(
                            "SELECT status FROM experiment_records WHERE exp_id = ?1 LIMIT 1",
                            params![exp_id],
                            |r| r.get(0),
                        )
                        .optional()?;
                    match row {
                        None => violations.push(format!(
                            "论文返修实验失配：回复信引用的补充实验 Exp:{} 不存在",
                            exp_id
                        )),
                        Some(st) if st.trim() != "real_data_filled" && st.trim() != "verified" => {
                            violations.push(format!(
                                "论文返修实验未实测：Exp:{} 当前状态为 {}，尚未回填真实测量数据",
                                exp_id, st
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // 3. 涉密禁词检查
    let prohibited_hits = scan_prohibited_terms(response_letter);
    for hit in &prohibited_hits {
        violations.push(format!("命中涉密/违规禁词：「{}」", hit));
    }

    let passed = violations.is_empty();
    Ok(ScopeConsistencyReport {
        mode: "paper_rebuttal".to_string(),
        passed,
        violations,
        warnings,
        new_terms_detected,
        prohibited_hits,
    })
}
