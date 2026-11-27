//! Step 2: ExpandQuery — AI 扩展搜索查询词
//!
//! 类型：LLM

use crate::ai::AiClient;
use crate::pipeline::context::PipelineContext;
use anyhow::Result;

/// 执行 Step 2
pub async fn execute(ctx: &mut PipelineContext, ai: &AiClient) -> Result<()> {
    let excluded_block =
        crate::orchestrator::constraints::format_excluded_paths_prompt_block(&ctx.research_state);
    let prompt = format!(
        "基于以下创意，生成 6-8 个搜索查询词，用于检索相关的现有技术和专利。\n\n\
         创意标题：{}\n创意描述：{}\n提取的关键词：{}\n技术领域：{}\n{}\n\
         要求：\n\
         - 包含中文和英文查询词\n\
         - 包含同义词、上位概念、下位概念\n\
         - 主动避开已排除的失败路径，向未证伪的替代方向拓展\n\
         - 每行一个查询词，不要编号\n\
         - 只输出查询词，不要其他说明文字",
        ctx.title,
        ctx.description,
        ctx.keywords.join("、"),
        ctx.technical_domain,
        excluded_block,
    );

    match ai.chat(&prompt, None).await {
        Ok(response) => {
            let mut queries: Vec<String> = Vec::new();
            for line in response.lines() {
                let raw = line
                    .trim()
                    .trim_start_matches(|c: char| {
                        c.is_numeric() || c == '.' || c == '、' || c == '-' || c == '*'
                    })
                    .trim();
                let cleaned = super::parse::sanitize_search_query(raw);
                if cleaned.chars().count() >= 2 && !queries.contains(&cleaned) {
                    queries.push(cleaned);
                }
                if queries.len() >= 8 {
                    break;
                }
            }

            if queries.is_empty() {
                // AI 失败时，用清洗后的关键词作为备选查询
                ctx.expanded_queries = ctx
                    .keywords
                    .iter()
                    .map(|k| super::parse::sanitize_search_query(k))
                    .filter(|k| k.chars().count() >= 2)
                    .take(5)
                    .collect();
            } else {
                ctx.expanded_queries = queries;
            }
        }
        Err(_) => {
            // AI 不可用时降级：直接用清洗后的关键词
            ctx.expanded_queries = ctx
                .keywords
                .iter()
                .map(|k| super::parse::sanitize_search_query(k))
                .filter(|k| k.chars().count() >= 2)
                .take(5)
                .collect();
        }
    }

    // 补充清洗后的核心标题短语（绝不把 28 字口语调研整句强塞到第 0 位挤占专利检索配额）
    let sanitized_title = super::parse::sanitize_search_query(&ctx.title);
    if sanitized_title.chars().count() >= 2 && !ctx.expanded_queries.contains(&sanitized_title) {
        if ctx.expanded_queries.is_empty() {
            ctx.expanded_queries.push(sanitized_title);
        } else {
            ctx.expanded_queries.insert(0, sanitized_title);
        }
    }

    Ok(())
}
