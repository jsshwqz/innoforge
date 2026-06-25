//! Step 16: GenerateOaResponse — 基于流水线分析结果生成 OA 答复辅助分析
//!
//! 类型：LLM
//!
//! 整合前面步骤的产出（权利要求树、现有技术聚类、AI 深度分析、新颖性评分等），
//! 调用 AI 生成 OA 答复辅助分析，包括：权利要求逐项审查、特征对比、三步法分析、答复策略建议。

use crate::ai::AiClient;
use crate::pipeline::context::PipelineContext;
use anyhow::Result;

/// 执行 Step 16（基于流水线结果生成 OA 答复辅助分析）
pub async fn execute(ctx: &mut PipelineContext, ai_client: &AiClient) -> Result<()> {
    // 从流水线上下文收集有用信息
    let title = &ctx.title;
    let description = &ctx.description;
    let technical_domain = &ctx.technical_domain;
    let novelty_score = ctx.novelty_score;
    let ai_analysis = &ctx.ai_analysis;
    let action_plan = &ctx.action_plan;

    // 构建权利要求信息（从 AI 分析中提取）
    let claims_info = if ai_analysis.len() > 200 {
        format!(
            "（基于 AI 分析中的权利要求信息）\n{}",
            truncate(ai_analysis, 2000)
        )
    } else {
        "（流水线未生成权利要求分析数据）".to_string()
    };

    // 构建现有技术信息
    let prior_art_summary = build_prior_art_summary(ctx);

    // 构建深度推演信息
    let reasoning_info = if !ctx.deep_reasoning.synthesis.is_empty() {
        format!(
            "多维推演综合：{}\n\n新颖方向：{}\n\n盲点：{}",
            ctx.deep_reasoning.synthesis,
            ctx.deep_reasoning.novel_directions.join("；"),
            ctx.deep_reasoning.blind_spots.join("；"),
        )
    } else {
        String::new()
    };

    let prompt = format!(
        "## 发明名称\n{title}\n\n\
         ## 技术方案描述\n{description}\n\n\
         ## 技术领域\n{technical_domain}\n\n\
         ## 权利要求信息\n{claims}\n\n\
         ## 现有技术对比总结\n{prior_art}\n\n\
         ## 新颖性评分\n{score:.0}/100\n\n\
         ## AI 深度分析摘要\n{analysis}\n\n\
         ## AI 行动方案\n{action_plan}\n\n\
         ## 多维推演结果\n{reasoning}\n\n\
         ---\n\n\
         请基于以上流水线分析结果，生成一份**OA 答复辅助分析报告**，按以下结构输出：\n\n\
         === 第一部分：权利要求可授权性评估 ===\n\n\
         逐项评估每条独立权利要求在现有技术下的可授权前景：\n\
         - 权利要求概述\n\
         - 现有技术覆盖情况（哪些特征已被公开）\n\
         - 真正区别特征\n\
         - 可授权前景（高/中/低）\n\
         - 如前景低，建议的修改方向\n\n\
         === 第二部分：现有技术威胁等级分类 ===\n\n\
         列出最具威胁的现有技术（最多 5 篇），按 X/Y/A 分类：\n\
         - X 类：单独影响新颖性/创造性的文件\n\
         - Y 类：与其他文件结合影响创造性的文件\n\
         - A 类：背景技术文件\n\n\
         === 第三部分：三步法创造性分析 ===\n\n\
         对最相关的对比文件执行三步法：\n\
         - 步骤1：确定最接近的现有技术\n\
         - 步骤2：确定区别技术特征和实际解决的技术问题\n\
         - 步骤3：判断是否显而易见\n\
         - 综合创造性结论\n\n\
         === 第四部分：答复策略建议 ===\n\n\
         - 总体策略（争辩/修改/两者结合）\n\
         - 核心论点（最多 5 条）\n\
         - 每项论点的有力程度（强/中/弱）\n\
         - 权利要求修改建议（如需要）\n\
         - 需要补充的实验数据或证据（如需要）\n\n\
         === 第五部分：授权前景预判 ===\n\n\
         - 不修改权利要求情况下的授权概率\n\
         - 采纳修改建议后的授权概率\n\
         - 主要风险点\n\
         - 建议的下一步行动",
        claims = claims_info,
        prior_art = prior_art_summary,
        score = novelty_score,
        analysis = truncate(ai_analysis, 2000),
        action_plan = truncate(action_plan, 1000),
        reasoning = truncate(&reasoning_info, 1000),
    );

    let result = ai_client.chat(&prompt, None).await?;
    ctx.oa_response = result;

    tracing::info!(
        "OA 答复辅助分析完成（新颖性评分 {:.0}，现有技术聚类 {} 组）",
        novelty_score,
        ctx.prior_art_clusters.len(),
    );

    Ok(())
}

/// 构建现有技术对比总结
fn build_prior_art_summary(ctx: &PipelineContext) -> String {
    let mut summary = String::new();

    // 聚类信息
    if !ctx.prior_art_clusters.is_empty() {
        summary.push_str("现有技术聚类：\n");
        for (i, cluster) in ctx.prior_art_clusters.iter().enumerate() {
            summary.push_str(&format!(
                "  {}. {}（{} 篇专利，平均相似度 {:.0}%）\n",
                i + 1,
                cluster.topic,
                cluster.patent_indices.len(),
                cluster.avg_similarity * 100.0,
            ));
        }
        summary.push('\n');
    }

    // Top matches
    if !ctx.top_matches.is_empty() {
        summary.push_str("最相关的现有技术（Top 5）：\n");
        for m in ctx.top_matches.iter().take(5) {
            summary.push_str(&format!(
                "  {}. {}（{:.0}%）- {}\n",
                m.rank,
                m.source_title,
                m.combined_score * 100.0,
                if m.snippet.len() > 80 {
                    format!("{}...", m.snippet.chars().take(80).collect::<String>())
                } else {
                    m.snippet.clone()
                },
            ));
        }
        summary.push('\n');
    }

    // 矛盾信号
    if !ctx.contradictions.is_empty() {
        summary.push_str("检测到的矛盾信号（创新机会）：\n");
        for c in &ctx.contradictions {
            summary.push_str(&format!("  - {}：{}\n", c.dimension, c.opportunity));
        }
    }

    if summary.is_empty() {
        summary = "（流水线未产生现有技术对比数据）".to_string();
    }

    summary
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}
