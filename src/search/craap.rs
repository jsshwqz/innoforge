//! 模块职责：CRAAP 五维来源可信度评分与多源三角互证引擎（v0.9.67）。
//! 上游调用者：`src/routes/search.rs`（本地/向量/在线/单源检索结果卡片增强）。
//! 模块专属禁区：全部基于文献真实字段确定性计算，严禁编造引用数或伪造多源互证。

use crate::types::search::PatentSummary;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// 单篇文献的 CRAAP 五维可信度评分 + 多源三角互证结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CraapScore {
    /// 总分（0 ~ 100）
    pub total: u8,
    /// 等级：`"A"` (≥80) / `"B"` (65..79) / `"C"` (50..64) / `"D"` (<50)
    pub grade: String,
    /// C - 时效性 Currency (0~20)
    pub currency: u8,
    /// R - 相关性 Relevance (0~20)
    pub relevance: u8,
    /// A1 - 权威度 Authority (0~20)
    pub authority: u8,
    /// A2 - 实体核验准确度 Accuracy (0~20)
    pub accuracy: u8,
    /// P - 文献类型客观度 Purpose (0~20)
    pub purpose: u8,
    /// 多源三角互证命中源列表（len >= 2 表示被多个独立检索源交叉印证）
    pub corroborated_sources: Vec<String>,
    /// 简短中文说明（用于卡片悬浮提示）
    pub summary_zh: String,
}

/// 抽取 4 位年份（1950..=2035）。
fn extract_year(date_str: &str) -> Option<i32> {
    let bytes = date_str.as_bytes();
    if bytes.len() < 4 {
        return None;
    }
    for i in 0..=bytes.len() - 4 {
        if bytes[i..i + 4].iter().all(|b| b.is_ascii_digit()) {
            if let Ok(y) = date_str[i..i + 4].parse::<i32>() {
                if (1950..=2035).contains(&y) {
                    return Some(y);
                }
            }
        }
    }
    None
}

/// 识别文献来源类型与专利授权/学术期刊属性。
fn classify_doc_authority_and_purpose(item: &PatentSummary, source_hint: &str) -> (u8, u8) {
    let pnum = item.patent_number.trim().to_ascii_uppercase();
    let src = source_hint.to_ascii_lowercase();
    let is_academic = pnum.starts_with("10.")
        || pnum.starts_with("PMID:")
        || pnum.starts_with("S2:")
        || matches!(src.as_str(), "openalex" | "semantic_scholar" | "pubmed");

    if is_academic {
        let has_venue = !item.applicant.trim().is_empty();
        let has_doi = pnum.starts_with("10.") || item.abstract_text.contains("DOI:");
        let high_cite = item.abstract_text.contains("高影响力")
            || item.abstract_text.contains("引用 ")
            || src == "pubmed";
        let authority = match (has_venue, has_doi, high_cite) {
            (true, true, true) => 19,
            (true, true, false) => 17,
            (true, false, _) => 15,
            _ => 12,
        };
        let purpose = if src == "pubmed" || has_venue { 18 } else { 15 };
        return (authority, purpose);
    }

    // 专利文献：授权公告 (B/B1/B2/U) > 公开申请 (A/A1) > 无号草稿
    let is_granted =
        pnum.ends_with('B') || pnum.ends_with("B1") || pnum.ends_with("B2") || pnum.ends_with('U');
    let is_major_office = pnum.starts_with("CN")
        || pnum.starts_with("US")
        || pnum.starts_with("EP")
        || pnum.starts_with("WO")
        || pnum.starts_with("JP");

    let authority = match (is_granted, is_major_office) {
        (true, true) => 19,
        (true, false) => 17,
        (false, true) => 15,
        (false, false) => {
            if pnum.is_empty() {
                8
            } else {
                12
            }
        }
    };
    let purpose = if is_granted {
        19
    } else if is_major_office {
        16
    } else {
        12
    };
    (authority, purpose)
}

/// 计算单篇文献的 CRAAP 五维评分。
pub fn score_patent_summary(
    item: &PatentSummary,
    source_hint: &str,
    corroborated_sources: Vec<String>,
    reference_year: i32,
) -> CraapScore {
    // 1. C (Currency 时效性): 0~20
    let currency: u8 = match extract_year(&item.filing_date) {
        Some(y) => {
            let age = (reference_year - y).max(0);
            match age {
                0..=3 => 20,
                4..=6 => 17,
                7..=10 => 14,
                11..=15 => 11,
                _ => 8,
            }
        }
        None => 6,
    };

    // 2. R (Relevance 相关性): 0~20
    let relevance: u8 = match item.relevance_score {
        Some(s) if s > 1.0 => ((s / 100.0).clamp(0.0, 1.0) * 20.0).round() as u8,
        Some(s) => (s.clamp(0.0, 1.0) * 20.0).round() as u8,
        None => 12,
    };

    // 3. A1 (Authority) & 5. P (Purpose): 0~20
    let (mut authority, purpose) = classify_doc_authority_and_purpose(item, source_hint);
    if corroborated_sources.len() >= 2 {
        authority = (authority + 1).min(20);
    }

    // 4. A2 (Accuracy 实体核验准确度): 0~20
    let mut accuracy: u8 = 4;
    if !item.patent_number.trim().is_empty() {
        accuracy += 5;
    }
    if item.title.chars().count() >= 6 {
        accuracy += 3;
    }
    if item.abstract_text.chars().count() >= 40 {
        accuracy += 4;
    }
    if !item.applicant.trim().is_empty() {
        accuracy += 2;
    }
    if !item.inventor.trim().is_empty() {
        accuracy += 2;
    }
    accuracy = accuracy.min(20);

    let total = currency
        .saturating_add(relevance)
        .saturating_add(authority)
        .saturating_add(accuracy)
        .saturating_add(purpose)
        .min(100);

    let grade = match total {
        80..=100 => "A",
        65..=79 => "B",
        50..=64 => "C",
        _ => "D",
    }
    .to_string();

    let summary_zh = format!(
        "时效{}/20 · 相关{}/20 · 权威{}/20 · 核验{}/20 · 客观{}/20{}",
        currency,
        relevance,
        authority,
        accuracy,
        purpose,
        if corroborated_sources.len() >= 2 {
            format!(" · 多源互证({})", corroborated_sources.join("+"))
        } else {
            String::new()
        }
    );

    CraapScore {
        total,
        grade,
        currency,
        relevance,
        authority,
        accuracy,
        purpose,
        corroborated_sources,
        summary_zh,
    }
}

/// 规范化标题/编号键，用于跨源三角互证比对。
pub fn canonical_corroboration_key(patent_number: &str, title: &str) -> String {
    let pn: String = patent_number
        .trim()
        .to_ascii_uppercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '/')
        .collect();
    if pn.len() >= 6 {
        return pn;
    }
    title
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation())
        .take(32)
        .collect()
}

/// 批量计算 CRAAP 五维评分，并结合跨源候选集合标记多源三角互证。
///
/// `extra_source_keys`: `(source_label, canonical_key)` 列表（如本地库命中或并行其他源命中），
/// 当同一篇文献在 ≥2 个独立源中出现时，`corroborated_sources` 将如实列出全部印证源。
pub fn score_batch_with_corroboration(
    items: &[PatentSummary],
    primary_source: &str,
    extra_source_keys: &[(String, String)],
    reference_year: i32,
) -> HashMap<String, CraapScore> {
    let mut key_to_sources: HashMap<String, HashSet<String>> = HashMap::new();
    for item in items {
        let key = canonical_corroboration_key(&item.patent_number, &item.title);
        if !key.is_empty() {
            key_to_sources
                .entry(key)
                .or_default()
                .insert(primary_source.to_string());
        }
    }
    for (src, key) in extra_source_keys {
        if !key.is_empty() {
            key_to_sources
                .entry(key.clone())
                .or_default()
                .insert(src.clone());
        }
    }

    let mut out = HashMap::new();
    for item in items {
        let corr_key = canonical_corroboration_key(&item.patent_number, &item.title);
        let mut sources: Vec<String> = key_to_sources
            .get(&corr_key)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_else(|| vec![primary_source.to_string()]);
        sources.sort();
        let score = score_patent_summary(item, primary_source, sources, reference_year);
        let map_key = if !item.id.trim().is_empty() {
            item.id.clone()
        } else {
            item.patent_number.clone()
        };
        out.insert(map_key, score);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_summary(num: &str, date: &str, abs: &str, rel: Option<f64>) -> PatentSummary {
        PatentSummary {
            id: format!("id-{num}"),
            patent_number: num.to_string(),
            title: "一种高效电机浸没冷却散热系统".to_string(),
            abstract_text: abs.to_string(),
            applicant: "某创新机电研究所".to_string(),
            inventor: "张某; 李某".to_string(),
            filing_date: date.to_string(),
            country: "CN".to_string(),
            relevance_score: rel,
            score_source: None,
            matched_fields: None,
        }
    }

    #[test]
    fn granted_recent_complete_patent_scores_grade_a_and_beats_incomplete_old_record() {
        let strong = sample_summary(
            "CN999000123B",
            "2025-04-10",
            "本发明公开一种用于高功率密度驱动电机的浸没式介电液直接冷却结构，采用微负压循环防止轴承外泄。",
            Some(0.92),
        );
        let weak = PatentSummary {
            id: "weak-1".into(),
            patent_number: "".into(),
            title: "测试".into(),
            abstract_text: "".into(),
            applicant: "".into(),
            inventor: "".into(),
            filing_date: "1998-01-01".into(),
            country: "".into(),
            relevance_score: Some(0.2),
            score_source: None,
            matched_fields: None,
        };

        let s_strong = score_patent_summary(&strong, "serpapi", vec!["serpapi".into()], 2026);
        let s_weak = score_patent_summary(&weak, "local", vec!["local".into()], 2026);

        assert!(
            s_strong.total >= 80 && s_strong.grade == "A",
            "{:?}",
            s_strong
        );
        assert!(s_weak.total < 50 && s_weak.grade == "D", "{:?}", s_weak);
        assert!(s_strong.total > s_weak.total + 40);
    }

    /// 负面反作弊测试：单源结果不得伪造多源互证；仅当独立第二源出现同规范键时才产出多源互证
    #[test]
    fn anti_cheat_corroboration_requires_distinct_second_source() {
        let item = sample_summary(
            "10.1000/fake-doi-1",
            "2024-06-01",
            "High efficiency immersion cooling study for traction motor windings with thermal validation.",
            Some(0.85),
        );
        let single =
            score_batch_with_corroboration(std::slice::from_ref(&item), "openalex", &[], 2026);
        let sc_single = &single["id-10.1000/fake-doi-1"];
        assert_eq!(sc_single.corroborated_sources.len(), 1);
        assert!(!sc_single.summary_zh.contains("多源互证"));

        let key = canonical_corroboration_key(&item.patent_number, &item.title);
        let multi = score_batch_with_corroboration(
            &[item],
            "openalex",
            &[("semantic_scholar".to_string(), key)],
            2026,
        );
        let sc_multi = &multi["id-10.1000/fake-doi-1"];
        assert_eq!(
            sc_multi.corroborated_sources,
            vec!["openalex".to_string(), "semantic_scholar".to_string()]
        );
        assert!(sc_multi.summary_zh.contains("多源互证"));
    }
}
