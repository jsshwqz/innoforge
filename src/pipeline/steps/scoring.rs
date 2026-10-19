//! Step 10: ScoreNovelty — 基于结构化数据计算新颖性评分
//!
//! 类型：CODE（不依赖 LLM，完全可复现）
//!
//! 评分公式：
//! novelty = 100 - max_sim_penalty - avg_penalty + contradiction_bonus + gap_bonus
//! 其中代码计算，不是 AI 猜测
//!
//! 证据语义（#70）：「没查到任何对比文献」与「查了且不相似」是两种不同状态。
//! 前者必须不给数值分（novelty_status = InsufficientEvidence，novelty_score 不参与报告），
//! 后者才进加减公式。diversity_score 为 None（DiversityGate 被跳过）时覆盖缺口加分恒为 0，
//! 禁止用 0.0 哨兵值同时表达「实测多样性 0」和「没测」。

use crate::pipeline::context::{Evidence, NoveltyStatus, PipelineContext, ScoreBreakdown};
use anyhow::Result;

/// 执行 Step 10
pub async fn execute(ctx: &mut PipelineContext) -> Result<()> {
    let scores = &ctx.similarity_scores;

    // 根因 1（#70）：空检索必须走「证据不足」路径，不进加减公式。
    // 旧实现 `scores.first().map(...).unwrap_or(0.0)` 把「没查」当成「查了且完全不相似」，
    // 再叠加根因 2 的恒 +8 覆盖缺口加分，空证据 raw=108 被 clamp 压成 100，输出满分假结论。
    if scores.is_empty() {
        ctx.novelty_status = NoveltyStatus::InsufficientEvidence;
        ctx.novelty_score = 0.0;
        ctx.score_breakdown = ScoreBreakdown::default();
        ctx.research_state
            .verified_claims
            .push("新颖性无法评估：未获得任何对比文献（证据不足）".to_string());
        ctx.evidence_chain.push(Evidence {
            id: uuid::Uuid::new_v4().to_string(),
            idea_id: ctx.idea_id.clone(),
            claim: "未获得任何对比文献，无法评估新颖性".to_string(),
            source_type: "scoring".to_string(),
            source_id: "novelty_score".to_string(),
            source_title: "算法评分".to_string(),
            source_url: String::new(),
            claim_number: None,
            excerpt: "evidence_status=insufficient_evidence, similarity_scores=0".to_string(),
            relation: "neutral".to_string(),
            confidence: 1.0,
            produced_by: "ScoreNovelty".to_string(),
            created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        });
        return Ok(());
    }

    // 证据充分：scores 非空，进加减公式。
    ctx.novelty_status = NoveltyStatus::Sufficient;

    // 最高相似度（scores 非空，first 必存在）
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

    // 根因 2（#70）：diversity_score 为 Option<f64>。
    // None（DiversityGate 被跳过，如 quick 模式）→ 覆盖缺口加分恒为 0，
    // 禁止把「没测多样性」当成「实测多样性 0」而 +8。
    // Some(d) → 仅当实测多样性 < 0.5 才给缺口加分。
    let coverage_gap_bonus = match ctx.diversity_score {
        Some(d) if d < 0.5 => (1.0 - d) * 8.0,
        Some(_) => 0.0,
        None => 0.0,
    };

    // 最终评分
    // 最高相似度惩罚（权重 60%）：max_similarity 越高，新颖性越低
    let max_sim_penalty = max_similarity * 60.0;
    // 平均相似度惩罚（权重 20%）
    let avg_penalty = avg_top5 * 20.0;

    let raw_score =
        100.0 - max_sim_penalty - avg_penalty + contradiction_bonus + coverage_gap_bonus;

    // 根因 3（#70）：raw_score > 100 意味着分数靠加分凑出来，是构造错误信号。
    // debug_assert 让越界在开发期炸出，不许静默 clamp 成满分掩盖。
    debug_assert!(
        raw_score <= 100.0,
        "raw_score={raw_score} 超 100：加分口径异常（contradiction_bonus={contradiction_bonus}, coverage_gap_bonus={coverage_gap_bonus}）"
    );

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

    // 更新研发状态机：记录验证结论
    ctx.research_state.verified_claims.push(format!(
        "新颖性评分 {:.1}/100（最高相似度 {:.2}）",
        final_score, max_similarity
    ));

    // 生成评分汇总证据 / Generate scoring summary evidence
    let level = if final_score >= 70.0 {
        "高新颖性"
    } else if final_score >= 40.0 {
        "中等新颖性"
    } else {
        "低新颖性"
    };
    let relation = if final_score >= 60.0 {
        "supports"
    } else {
        "contradicts"
    };
    ctx.evidence_chain.push(Evidence {
        id: uuid::Uuid::new_v4().to_string(),
        idea_id: ctx.idea_id.clone(),
        claim: format!("新颖性评分 {:.1}/100（{}）", final_score, level),
        source_type: "scoring".to_string(),
        source_id: "novelty_score".to_string(),
        source_title: "算法评分".to_string(),
        source_url: String::new(),
        claim_number: None,
        excerpt: format!(
            "最高相似度:{:.2}, Top5均值:{:.2}, 矛盾加分:{:.1}, 覆盖缺口:{:.1}, 最终:{:.1}",
            max_similarity, avg_top5, contradiction_bonus, coverage_gap_bonus, final_score
        ),
        relation: relation.to_string(),
        confidence: 1.0, // 确定性算法计算
        produced_by: "ScoreNovelty".to_string(),
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::context::{NoveltyStatus, PipelineContext, SimilarityEntry};

    /// #70 反转锁绿断言：空证据 ⇒ 绝不给数值新颖性分。
    ///
    /// 旧名 `test_no_results_means_high_novelty` 断言 `novelty_score > 90.0`，
    /// 把「无证据＝高分」这一缺陷行为固化成不变量，谁修好缺陷谁就被它打红，
    /// 属于 #36「永远不触发也能过」的升级版（触发了、还断言了错误不变量）。
    /// 现反转为：空证据 ⇒ novelty_status = InsufficientEvidence，novelty_score 不代表评分。
    #[tokio::test]
    async fn test_no_results_must_not_claim_novelty() {
        let mut ctx = PipelineContext::new(
            "test",
            "新型量子纠缠传感器",
            "利用量子纠缠效应进行超灵敏传感",
        );
        execute(&mut ctx).await.unwrap();
        // 没有搜索结果 ⇒ 证据不足 ⇒ 不许给出数值新颖性分
        assert_eq!(
            ctx.novelty_status,
            NoveltyStatus::InsufficientEvidence,
            "空证据必须标 InsufficientEvidence，不得给数值分"
        );
        // 双重保险：即便有人误读 novelty_score，也不应是接近满分的高分
        assert!(
            ctx.novelty_score <= 90.0,
            "空证据下 novelty_score 不应是高分，当前={}",
            ctx.novelty_score
        );
    }

    /// 正向 B（#70 验收·不能一刀切）：喂入若干低相似度但非空的结果 ⇒ 仍给数值分数且 < 100。
    /// 防止把所有分数都改成 null 混过验收。
    #[tokio::test]
    async fn test_nonempty_low_similarity_still_scores_below_100() {
        let mut ctx = PipelineContext::new("test", "t", "d");
        // quick 模式下 diversity_score 为 None ⇒ coverage_gap_bonus = 0
        ctx.similarity_scores = vec![SimilarityEntry {
            source_id: "x".into(),
            source_title: "x".into(),
            source_type: "web".into(),
            tfidf_score: 0.1,
            jaccard_score: 0.1,
            combined_score: 0.1,
        }];
        execute(&mut ctx).await.unwrap();
        assert_eq!(ctx.novelty_status, NoveltyStatus::Sufficient);
        assert!(ctx.novelty_score < 100.0, "got {}", ctx.novelty_score);
        assert!(ctx.novelty_score > 0.0, "got {}", ctx.novelty_score);
    }

    /// 正向 B（#70 验收·3 条非空低相似度）：喂 3 条 combined_score=0.1 的结果，
    /// 断言 novelty_status=Sufficient 且 0<novelty_score<100。
    /// 期望 raw=100-6-2+0+0=92.0（max_sim=0.1→penalty6, avg_top5=0.1→penalty2, 无矛盾, None 多样性→bonus0）。
    #[tokio::test]
    async fn test_positive_b_three_low_similarity_entries_sufficient() {
        let mut ctx = PipelineContext::new("test", "t", "d");
        ctx.similarity_scores = (0..3)
            .map(|i| SimilarityEntry {
                source_id: format!("s{i}"),
                source_title: format!("s{i}"),
                source_type: "web".into(),
                tfidf_score: 0.1,
                jaccard_score: 0.1,
                combined_score: 0.1,
            })
            .collect();
        execute(&mut ctx).await.unwrap();
        assert_eq!(
            ctx.novelty_status,
            NoveltyStatus::Sufficient,
            "3 条非空低相似度结果必须标 Sufficient"
        );
        assert!(
            ctx.novelty_score > 0.0 && ctx.novelty_score < 100.0,
            "3 条非空结果应给 0<score<100，实际={}",
            ctx.novelty_score
        );
    }

    /// 负控 1（#70 验收）：若把 None 时 coverage_gap_bonus 重新赋 8，
    /// 本测试构造的低相似度+None 多样性场景会因 +8 越界而被 debug_assert 捕获或分数偏高。
    /// 这里断言 None 时 bonus 真为 0：raw = 100 - 6 - 2 + 0 + 0 = 92。
    #[tokio::test]
    async fn test_none_diversity_means_zero_coverage_gap_bonus() {
        let mut ctx = PipelineContext::new("test", "t", "d");
        ctx.similarity_scores = vec![SimilarityEntry {
            source_id: "x".into(),
            source_title: "x".into(),
            source_type: "web".into(),
            tfidf_score: 0.1,
            jaccard_score: 0.1,
            combined_score: 0.1,
        }];
        // diversity_score 保持默认 None（quick 模式）
        assert!(ctx.diversity_score.is_none());
        execute(&mut ctx).await.unwrap();
        // max_sim=0.1 ⇒ penalty=6; avg_top5=0.1 ⇒ penalty=2; bonus=0 ⇒ raw=92
        assert_eq!(ctx.score_breakdown.coverage_gap_bonus, 0.0);
        assert!(
            (ctx.novelty_score - 92.0).abs() < 1e-6,
            "got {}",
            ctx.novelty_score
        );
    }
}
