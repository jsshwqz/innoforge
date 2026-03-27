//! Step 2: ExpandQuery — AI 扩展搜索查询词
//!
//! 类型：LLM

use crate::ai::AiClient;
use crate::pipeline::context::PipelineContext;
use anyhow::Result;

/// 执行 Step 2
pub async fn execute(ctx: &mut PipelineContext, ai: &AiClient) -> Result<()> {
    let prompt = format!(
        "基于以下创意，生成 6-8 个搜索查询词，用于检索相关的现有技术和专利。\n\n\
         创意标题：{}\n创意描述：{}\n提取的关键词：{}\n技术领域：{}\n\n\
         要求：\n\
         - 包含中文和英文查询词\n\
         - 包含同义词、上位概念、下位概念\n\
         - 每行一个查询词，不要编号\n\
         - 只输出查询词，不要其他说明文字",
        ctx.title,
        ctx.description,
        ctx.keywords.join("、"),
        ctx.technical_domain,
    );

    match ai.chat(&prompt, None).await {
        Ok(response) => {
            let queries: Vec<String> = response
                .lines()
                .map(|l| l.trim().trim_start_matches(|c: char| c.is_numeric() || c == '.' || c == '、' || c == '-'))
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty() && l.len() >= 2)
                .take(8)
                .collect();

            if queries.is_empty() {
                // AI 失败时，用关键词作为备选查询
                ctx.expanded_queries = ctx.keywords.iter().take(5).cloned().collect();
            } else {
                ctx.expanded_queries = queries;
            }
        }
        Err(_) => {
            // AI 不可用时降级：直接用提取的关键词
            ctx.expanded_queries = ctx.keywords.iter().take(5).cloned().collect();
        }
    }

    // 确保至少有原始标题作为查询
    if !ctx.expanded_queries.contains(&ctx.title) {
        ctx.expanded_queries.insert(0, ctx.title.clone());
    }

    Ok(())
}
