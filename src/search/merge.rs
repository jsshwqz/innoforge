//! 结果排序与去重 / Ranking & merge（MA1 抽出，MA2 扩多源）
//!
//! `sort_by_relevance` / `dedup_patent_summaries` 逐字迁自 `src/routes/search.rs`，
//! 去重键沿用 `crate::patent::canonical_patent_key`（规范化 publication_number，
//! 空号退化为 `TITLE::<大写标题>`）。
//!
//! **口径对账（MA6c，2026-09-28）**：spec §6 那句「合并去重键 = 规范化 publication_number」
//! 描述的是 ~~跨源结果合并~~ **同源结果去重**——在线链按登记顺序择单一胜者，跨源拼接经
//! 裁决不做（依据见 [`MergedPatent`](crate::search::model::MergedPatent) 文档与规格书 §6）。
//! 因此本文件的去重能力只有 [`dedup_patent_summaries`] 一处，服务的是本地 `/api/search`
//! 路径（`routes/search.rs:107` 调用），不是在线链的合并器。

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

/// 把单源结果包成 [`MergedPatent`]：只做「标源 + 原样搬运 summary」两件事。
///
/// **MA6c 对账**：本函数曾额外用 `canonical_patent_key` 算一条 `key` 作为跨源合并去重键。
/// 执行链按登记顺序择单一胜者（胜出源整份返回），没有第二个源的输入可拼接，该键从未有
/// 生产消费点，已随 MA6c 裁决删除（依据见 [`MergedPatent`] 文档与规格书 §6）。
/// 本函数**不改写** `summary` 的任何字段（规范化只发生在 [`dedup_patent_summaries`] 的
/// 键计算里，且只在真正需要跨条去重的本地 `/api/search` 路径上跑）。
pub fn merged_from(source: SourceKind, items: Vec<PatentSummary>) -> Vec<MergedPatent> {
    items
        .into_iter()
        .map(|summary| MergedPatent {
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

    /// `merged_from` 的两项职责：**每条都打上来源**（`winning_source` 与面板读它）、
    /// **summary 原样透传**（不做任何规范化改写）。
    ///
    /// MA6c 对账：本用例原有第三条断言 `!first.key.is_empty()`（去重键非空）随 `key` 字段
    /// 按裁决删除一并移除；判据换成「多条输入逐条标源 + patent_number 未被改写」，
    /// 断言覆盖面从 1 条扩到 2 条，强度不降（规范化职责仍由上面四条 dedup 用例锁死）。
    #[test]
    fn merged_entries_carry_source_and_untouched_summary() {
        let items = vec![
            summary("cn 2024 101234 A", "t1", Some(1.0), 1),
            summary("CN2024101235B2", "t2", Some(2.0), 2),
        ];
        let merged = merged_from(SourceKind::SerpApi, items);
        assert_eq!(
            2,
            merged.len(),
            "merged_from 不得丢弃或合并任何一条（择胜/去重都不归它管）"
        );
        for (entry, want_number) in merged.iter().zip(["cn 2024 101234 A", "CN2024101235B2"]) {
            assert_eq!(
                vec![SourceKind::SerpApi],
                entry.sources,
                "每条都必须标上来源（出参 source 字段与诊断面板都读它）"
            );
            assert_eq!(
                want_number, entry.summary.patent_number,
                "公开号必须原样透传，禁止在这里被规范化改写（面板显示的就是它）"
            );
        }
        assert_eq!("t1", merged[0].summary.title);
        assert_eq!(2, merged[1].summary.abstract_text.len());
    }
}
