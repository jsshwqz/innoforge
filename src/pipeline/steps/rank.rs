//! Step 7: RankAndFilter — 按相似度排序，去重，取 Top-N
//!
//! 类型：CODE

use crate::pipeline::context::{PipelineContext, RankedMatch};
use crate::pipeline::steps::parse::tokenize;
use anyhow::Result;
use std::collections::HashSet;

const MAX_TOP_MATCHES: usize = 15;

/// 执行 Step 7
pub async fn execute(ctx: &mut PipelineContext) -> Result<()> {
    let mut seen_titles: HashSet<String> = HashSet::new();
    let mut ranked = Vec::new();

    // 合并 web + patent 结果的 URL 映射
    let url_map: std::collections::HashMap<String, (String, String)> = ctx
        .web_results
        .iter()
        .map(|r| (r.id.clone(), (r.link.clone(), r.snippet.clone())))
        .chain(
            ctx.patent_results
                .iter()
                .map(|r| (r.id.clone(), (r.link.clone(), r.snippet.clone()))),
        )
        .collect();

    for entry in &ctx.similarity_scores {
        // 去重：标题相似的只保留第一个
        let title_key = entry
            .source_title
            .chars()
            .take(30)
            .collect::<String>()
            .to_lowercase();
        if seen_titles.contains(&title_key) {
            continue;
        }
        seen_titles.insert(title_key);

        let (url, snippet) = url_map.get(&entry.source_id).cloned().unwrap_or_default();

        let tokens = tokenize(&format!("{} {}", entry.source_title, snippet));

        ranked.push(RankedMatch {
            rank: ranked.len() + 1,
            source_id: entry.source_id.clone(),
            source_title: entry.source_title.clone(),
            source_type: entry.source_type.clone(),
            source_url: url,
            snippet,
            combined_score: entry.combined_score,
            tokens,
        });

        if ranked.len() >= MAX_TOP_MATCHES {
            break;
        }
    }

    ctx.top_matches = ranked;
    Ok(())
}
