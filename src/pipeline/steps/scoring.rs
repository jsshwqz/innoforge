//! Step 10: ScoreNovelty — 基于结构化数据计算新颖性评分
//!
//! 类型：CODE（不依赖 LLM，完全可复现）
//!
//! 评分公式：
//! novelty = 100 - max_sim_penalty - avg_penalty + contradiction_bonus + gap_bonus
//! 其中代码计算，不是 AI 猜测

use crate::pipeline::context::{PipelineContext, ScoreBreakdown};
use anyhow::Result;

/// 执行 Step 10
pub async fn execute(ctx: &mut PipelineContext) -> Result<()> {
    let scores = &ctx.similarity_scores;

    // 最高相似度
    let max_similarity = scores.first().map(|s| s.combined_score).unwrap_or(0.0);

    // Top5 平均相似度
    let top5: Vec<f64> = scores.iter().take(5).map(|s| s.combined_score).collect();
    let avg_top5 = if top5.is_empty() {
        0.0
    } else {
        top5.iter().sum::<f64>() / top5.len() as f64
    };

    // 矛盾信号加分：每个矛盾 +3 分，最多 +10 分
    let contradiction_bonus = (ctx.contradictions.len() as f64 * 3.0).min(10.0);

    // 覆盖缺口加分：多样性越低，说明搜索覆盖不全，可能存在未发现的空白
    let coverage_gap_bonus = if ctx.diversity_score < 0.5 {
        (1.0 - ctx.diversity_score) * 8.0
    } else {
        0.0
    };

    // 最终评分
    // 最高相似度惩罚（权重 60%）：max_similarity 越高，新颖性越低
    let max_sim_penalty = max_similarity * 60.0;
    // 平均相似度惩罚（权重 20%）
    let avg_penalty = avg_top5 * 20.0;

    let raw_score =
        100.0 - max_sim_penalty - avg_penalty + contradiction_bonus + coverage_gap_bonus;

    // 限制在 0-100 范围
    let final_score = raw_score.clamp(0.0, 100.0);

    ctx.score_breakdown = ScoreBreakdown {
        max_similarity,
        avg_top5_similarity: avg_top5,
        contradiction_bonus,
        coverage_gap_bonus,
        final_score,
    };
    ctx.novelty_score = final_score;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::context::PipelineContext;

    #[tokio::test]
    async fn test_no_results_means_high_novelty() {
        let mut ctx = PipelineContext::new(
            "test",
            "新型量子纠缠传感器",
            "利用量子纠缠效应进行超灵敏传感",
        );
        execute(&mut ctx).await.unwrap();
        // 没有搜索结果 → 相似度为 0 → 新颖性接近 100
        assert!(ctx.novelty_score > 90.0);
    }
}
