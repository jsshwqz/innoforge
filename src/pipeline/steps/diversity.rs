//! Step 5: DiversityGate — 检查搜索结果的多样性
//!
//! 类型：CODE

use crate::pipeline::context::PipelineContext;
use anyhow::Result;
use std::collections::HashSet;

/// 执行 Step 5
pub async fn execute(ctx: &mut PipelineContext) -> Result<()> {
    let mut dimensions: HashSet<&str> = HashSet::new();
    let total_results = ctx.web_results.len() + ctx.patent_results.len();

    // 维度 1：结果数量充足
    if total_results >= 5 {
        dimensions.insert("result_count");
    }

    // 维度 2：来源多样性（不同域名/来源）
    let sources: HashSet<String> = ctx
        .web_results
        .iter()
        .filter_map(|r| {
            // 简单提取域名
            r.link.split('/').nth(2).map(|h| h.to_string())
        })
        .chain(ctx.patent_results.iter().map(|r| r.source.clone()))
        .collect();
    if sources.len() >= 3 {
        dimensions.insert("source_diversity");
    }

    // 维度 3：同时有 web 和 patent 结果
    if !ctx.web_results.is_empty() && !ctx.patent_results.is_empty() {
        dimensions.insert("type_diversity");
    }

    // 维度 4：结果标题中包含多种语言
    let has_chinese = ctx
        .web_results
        .iter()
        .chain(ctx.patent_results.iter())
        .any(|r| {
            r.title
                .chars()
                .any(|c| (c as u32) >= 0x4E00 && (c as u32) <= 0x9FFF)
        });
    let has_english = ctx
        .web_results
        .iter()
        .chain(ctx.patent_results.iter())
        .any(|r| r.title.chars().any(|c| c.is_ascii_alphabetic()));
    if has_chinese && has_english {
        dimensions.insert("language_diversity");
    }

    // 维度 5：内容多样性（标题关键词不过度集中）
    let all_titles: String = ctx
        .web_results
        .iter()
        .chain(ctx.patent_results.iter())
        .map(|r| r.title.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let unique_words: HashSet<&str> = all_titles.split_whitespace().collect();
    if unique_words.len() >= total_results {
        dimensions.insert("content_diversity");
    }

    let score = dimensions.len() as f64 / 5.0;
    ctx.diversity_score = score;
    ctx.coverage_dimensions = dimensions.iter().map(|s| s.to_string()).collect();

    Ok(())
}
