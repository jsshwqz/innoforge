//! 结果排序与去重 / Ranking & merge（MA1 抽出，MA2 扩多源）
//!
//! `sort_by_relevance` / `dedup_patent_summaries` 逐字迁自 `src/routes/search.rs`，
//! 去重键沿用 `crate::patent::canonical_patent_key`（规范化 publication_number，
//! 空号退化为 `TITLE::<大写标题>`），与 spec §6「合并去重键 = 规范化 publication_number」一致。

use crate::patent::canonical_patent_key;
use crate::search::model::{MergedPatent, SourceKind};
use crate::types::search::PatentSummary;

pub fn sort_by_relevance(patents: &mut [PatentSummary]) {
    patents.sort_by(|a, b| {
        let sa = a.relevance_score.unwrap_or(0.0);
        let sb = b.relevance_score.unwrap_or(0.0);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
}

pub fn dedup_patent_summaries(items: Vec<PatentSummary>) -> Vec<PatentSummary> {
    let mut best_by_key: std::collections::HashMap<String, PatentSummary> =
        std::collections::HashMap::new();
    for item in items {
        let key = canonical_patent_key(&item.patent_number);
        let dedup_key = if key.is_empty() {
            format!("TITLE::{}", item.title.trim().to_uppercase())
        } else {
            key
        };
        match best_by_key.get_mut(&dedup_key) {
            None => {
                best_by_key.insert(dedup_key, item);
            }
            Some(existing) => {
                let old_score = existing.relevance_score.unwrap_or(0.0);
                let new_score = item.relevance_score.unwrap_or(0.0);
                let old_info = existing.title.len()
                    + existing.abstract_text.len()
                    + existing.applicant.len()
                    + existing.inventor.len();
                let new_info = item.title.len()
                    + item.abstract_text.len()
                    + item.applicant.len()
                    + item.inventor.len();
                if new_score > old_score || (new_score == old_score && new_info > old_info) {
                    *existing = item;
                }
            }
        }
    }
    let mut out: Vec<PatentSummary> = best_by_key.into_values().collect();
    sort_by_relevance(&mut out);
    out
}

/// 把单源结果包成 [`MergedPatent`]（MA1 只有一个在线源，故等价于打去重键 + 标源）。
pub fn merged_from(source: SourceKind, items: Vec<PatentSummary>) -> Vec<MergedPatent> {
    items
        .into_iter()
        .map(|summary| MergedPatent {
            key: canonical_patent_key(&summary.patent_number),
            sources: vec![source],
            summary,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(
        patent_number: &str,
        title: &str,
        score: Option<f64>,
        abstract_len: usize,
    ) -> PatentSummary {
        PatentSummary {
            id: format!("id-{patent_number}-{title}"),
            patent_number: patent_number.to_string(),
            title: title.to_string(),
            abstract_text: "x".repeat(abstract_len),
            applicant: String::new(),
            inventor: String::new(),
            filing_date: String::new(),
            country: "CN".to_string(),
            relevance_score: score,
            score_source: None,
        }
    }

    /// 同键（规范化后相同）取分高者；同分取信息量大者——旧行为。
    #[test]
    fn dedup_keeps_best_scoring_variant() {
        let items = vec![
            summary("CN2024101234A", "低分", Some(50.0), 3),
            summary("CN2024101234A", "高分", Some(90.0), 3),
        ];
        let out = dedup_patent_summaries(items);
        assert_eq!(1, out.len());
        assert_eq!("高分", out[0].title);
    }

    #[test]
    fn dedup_prefers_richer_info_on_tie() {
        let items = vec![
            summary("CN2024101234A", "薄的", Some(90.0), 1),
            summary("CN2024101234A", "厚的", Some(90.0), 50),
        ];
        let out = dedup_patent_summaries(items);
        assert_eq!(1, out.len());
        assert_eq!("厚的", out[0].title);
    }

    /// 无公开号时退化为标题键（大小写不敏感、去首尾空格），旧行为。
    #[test]
    fn dedup_falls_back_to_title_key() {
        let items = vec![
            summary("", "Alpha Widget", Some(60.0), 1),
            summary("", "  alpha widget  ", Some(55.0), 1),
        ];
        let out = dedup_patent_summaries(items);
        assert_eq!(1, out.len());
        assert_eq!("Alpha Widget", out[0].title);
    }

    /// 不同号必须都留下，并按分数降序（PRD N3 召回保障的最小说明）。
    #[test]
    fn dedup_sorts_by_score_descending() {
        let items = vec![
            summary("CN1", "c", Some(10.0), 1),
            summary("CN2", "b", Some(80.0), 1),
            summary("CN3", "a", Some(40.0), 1),
        ];
        let out = dedup_patent_summaries(items);
        assert_eq!(3, out.len());
        let scores: Vec<f64> = out.iter().filter_map(|p| p.relevance_score).collect();
        assert_eq!(vec![80.0, 40.0, 10.0], scores);
    }

    #[test]
    fn merged_entries_carry_key_and_source() {
        let merged = merged_from(
            SourceKind::SerpApi,
            vec![summary("cn 2024 101234 A", "t", Some(1.0), 1)],
        );
        assert_eq!(1, merged.len());
        let first = &merged[0];
        assert_eq!(vec![SourceKind::SerpApi], first.sources);
        assert!(!first.key.is_empty());
        assert_eq!("t", first.summary.title);
    }
}
