//! 跨实施例强行拼凑特征确定性识破模块（`src/core/evidence_rebuttal/embodiment_guard.rs`）
//!
//! 查询被引用锚点（`doc_chunks.embodiment_group`）与部件标号（`figure_numerals.embodiment_group`）
//! 在对比文献中所属的实施例集合；若多个被拼凑特征分别仅出现在互不重叠的不同专有实施例
//! （没有任何共同实施例同时包含二者），确定性输出 `Some(StitchingAlert)` 及反驳话术。

use std::collections::BTreeSet;

use anyhow::Result;
use rusqlite::{params, Connection};

use crate::core::vault::models::StitchingAlert;

/// 规范化输入引用项，剥离 `[doc_id#...]` 外壳以提取 `section_tag` 或部件标号
fn normalize_feature_key(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = trimmed.trim_start_matches('[').trim_end_matches(']');
        if let Some((_, rhs)) = inner.split_once('#') {
            return rhs.trim().to_string();
        }
    }
    trimmed.to_string()
}

/// 查询单个锚点或部件标号在目标文献 `doc_id` 中的全部非空 `embodiment_group`
fn query_feature_embodiments(
    conn: &Connection,
    doc_id: &str,
    feature_key: &str,
) -> Result<BTreeSet<String>> {
    let mut groups = BTreeSet::new();

    // 1. 查询 doc_chunks.embodiment_group（精确命中 section_tag）
    let mut stmt_chunk = conn.prepare(
        "SELECT DISTINCT embodiment_group
         FROM doc_chunks
         WHERE doc_id = ?1
           AND section_tag = ?2
           AND embodiment_group IS NOT NULL
           AND TRIM(embodiment_group) != ''",
    )?;
    let chunk_rows =
        stmt_chunk.query_map(params![doc_id, feature_key], |r| r.get::<_, String>(0))?;
    for g in chunk_rows {
        let val = g?;
        if !val.trim().is_empty() {
            groups.insert(val.trim().to_string());
        }
    }

    // 2. 查询 figure_numerals.embodiment_group（匹配 numeral_code 或 anchor_tag 或 part_name）
    let mut stmt_fig = conn.prepare(
        "SELECT DISTINCT embodiment_group
         FROM figure_numerals
         WHERE doc_id = ?1
           AND (numeral_code = ?2 OR anchor_tag = ?2 OR part_name = ?2)
           AND embodiment_group IS NOT NULL
           AND TRIM(embodiment_group) != ''",
    )?;
    let fig_rows = stmt_fig.query_map(params![doc_id, feature_key], |r| r.get::<_, String>(0))?;
    for g in fig_rows {
        let val = g?;
        if !val.trim().is_empty() {
            groups.insert(val.trim().to_string());
        }
    }

    Ok(groups)
}

/// 检测对同一对比文献的多个引用特征/部件标号是否存在跨实施例强行拼凑
pub fn detect_cross_embodiment_stitching(
    conn: &Connection,
    doc_id: &str,
    cited_anchors_or_numerals: &[String],
) -> Result<Option<StitchingAlert>> {
    if cited_anchors_or_numerals.len() < 2 {
        return Ok(None);
    }

    let mut feature_groups: Vec<(String, BTreeSet<String>)> = Vec::new();
    for raw in cited_anchors_or_numerals {
        let key = normalize_feature_key(raw);
        if key.is_empty() {
            continue;
        }
        let groups = query_feature_embodiments(conn, doc_id, &key)?;
        if !groups.is_empty() {
            feature_groups.push((raw.clone(), groups));
        }
    }

    // 至少需要 2 个带有明确实施例归属的特征，才能判定是否跨实施例拼凑
    if feature_groups.len() < 2 {
        return Ok(None);
    }

    // 计算所有带实施例归属的特征的交集
    let mut intersection: BTreeSet<String> = match feature_groups.first() {
        Some((_, first_set)) => first_set.clone(),
        None => return Ok(None),
    };
    for (_, set) in feature_groups.iter().skip(1) {
        intersection = intersection.intersection(set).cloned().collect();
    }

    // 若存在至少一个共同实施例同时包含全部特征，则未发生跨实施例拼凑
    if !intersection.is_empty() {
        return Ok(None);
    }

    let cited_features: Vec<String> = feature_groups.iter().map(|(f, _)| f.clone()).collect();
    let mut all_groups: BTreeSet<String> = BTreeSet::new();
    let mut detail_parts: Vec<String> = Vec::new();
    for (feat, set) in &feature_groups {
        let joined: Vec<String> = set.iter().cloned().collect();
        detail_parts.push(format!("「{}」仅记载于 {}", feat, joined.join("/")));
        for g in set {
            all_groups.insert(g.clone());
        }
    }
    let conflicting_embodiment_groups: Vec<String> = all_groups.into_iter().collect();

    let rebuttal_rationale = format!(
        "【跨实施例拼凑抗辩】对比文献 {} 中，{}。上述技术特征分属互不重叠的独立实施例（{}），原文未给出将不同实施例的技术特征相互结合的任何技术启示或替代教导；强行拼凑属于典型的“事后诸葛亮”式组合，违反专利创造性三步法/学术比对的整体性原则。",
        doc_id,
        detail_parts.join("；"),
        conflicting_embodiment_groups.join(" vs ")
    );

    Ok(Some(StitchingAlert {
        doc_id: doc_id.to_string(),
        cited_features,
        conflicting_embodiment_groups,
        rebuttal_rationale,
    }))
}
