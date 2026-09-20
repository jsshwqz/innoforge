//! Step 13: Finalize - aggregate report, persist to database
//! Type: CODE

use crate::db::memory::IdeaMemory;
use crate::db::Database;
use crate::pipeline::context::PipelineContext;
use anyhow::Result;
use std::sync::Arc;

pub async fn execute(ctx: &mut PipelineContext, db: &Arc<Database>) -> Result<()> {
    ctx.novelty_score = ctx.novelty_score.clamp(0.0, 100.0);

    if ctx.ai_analysis.is_empty() {
        ctx.ai_analysis = generate_code_only_report(ctx);
    }

    if !ctx.evidence_chain.is_empty() {
        let _ = db.delete_evidence_by_idea(&ctx.idea_id);
        if let Err(e) = db.insert_evidence_batch(&ctx.evidence_chain) {
            tracing::warn!("Evidence chain persist failed: {}", e);
        } else {
            tracing::info!("Evidence chain saved: {} entries", ctx.evidence_chain.len());
        }
    }

    let memory = extract_memory_entries(ctx);
    ctx.memory_entries = memory.clone();
    if !memory.is_empty() {
        if let Err(e) = db.save_memory_batch(&memory) {
            tracing::warn!("Memory persist failed: {}", e);
        } else {
            tracing::info!("Memory entries saved: {}", memory.len());
        }
    }

    Ok(())
}

fn extract_memory_entries(ctx: &PipelineContext) -> Vec<IdeaMemory> {
    let mut entries = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();
    let idea_id = ctx.idea_id.clone();

    for kw in &ctx.keywords {
        if kw.chars().count() >= 2 {
            entries.push(IdeaMemory {
                id: format!("{}-concept-{}", idea_id, uuid::Uuid::new_v4()),
                idea_id: idea_id.clone(),
                concept_name: kw.clone(),
                concept_type: "domain_concept".to_string(),
                content: format!("Keyword: {}", kw),
                confidence: 0.6,
                source_step: Some("ParseInput".to_string()),
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        }
    }

    if !ctx.technical_domain.is_empty() {
        entries.push(IdeaMemory {
            id: format!("{}-domain-{}", idea_id, uuid::Uuid::new_v4()),
            idea_id: idea_id.clone(),
            concept_name: ctx.technical_domain.clone(),
            concept_type: "domain_concept".to_string(),
            content: format!("Technical domain: {}", ctx.technical_domain),
            confidence: 0.7,
            source_step: Some("ParseInput".to_string()),
            created_at: now.clone(),
            updated_at: now.clone(),
        });
    }

    for c in &ctx.contradictions {
        entries.push(IdeaMemory {
            id: format!("{}-pattern-{}", idea_id, uuid::Uuid::new_v4()),
            idea_id: idea_id.clone(),
            concept_name: c.dimension.clone(),
            concept_type: "pattern".to_string(),
            content: format!(
                "Contradiction in {}: signal={:.2}, opportunity={}",
                c.dimension, c.signal_strength, c.opportunity
            ),
            confidence: c.signal_strength.clamp(0.0, 1.0),
            source_step: Some("DetectContradictions".to_string()),
            created_at: now.clone(),
            updated_at: now.clone(),
        });
    }

    for cluster in &ctx.prior_art_clusters {
        if !cluster.topic.is_empty() {
            entries.push(IdeaMemory {
                id: format!("{}-cluster-{}", idea_id, uuid::Uuid::new_v4()),
                idea_id: idea_id.clone(),
                concept_name: cluster.topic.clone(),
                concept_type: "domain_concept".to_string(),
                content: format!(
                    "Prior art cluster: {}, avg_similarity={:.1}%",
                    cluster.topic,
                    cluster.avg_similarity * 100.0
                ),
                confidence: (1.0 - cluster.avg_similarity).clamp(0.0, 1.0),
                source_step: Some("PriorArtCluster".to_string()),
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        }
    }

    for q in &ctx.research_state.open_questions {
        if !q.is_empty() {
            entries.push(IdeaMemory {
                id: format!("{}-question-{}", idea_id, uuid::Uuid::new_v4()),
                idea_id: idea_id.clone(),
                concept_name: q.chars().take(30).collect(),
                concept_type: "question".to_string(),
                content: q.clone(),
                confidence: 0.5,
                source_step: Some("ResearchState".to_string()),
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        }
    }

    if !ctx.research_state.current_hypothesis.is_empty() {
        entries.push(IdeaMemory {
            id: format!("{}-hypothesis-{}", idea_id, uuid::Uuid::new_v4()),
            idea_id: idea_id.clone(),
            concept_name: ctx
                .research_state
                .current_hypothesis
                .chars()
                .take(30)
                .collect(),
            concept_type: "decision".to_string(),
            content: ctx.research_state.current_hypothesis.clone(),
            confidence: 0.5,
            source_step: Some("ResearchState".to_string()),
            created_at: now.clone(),
            updated_at: now.clone(),
        });
    }

    entries
}

fn generate_code_only_report(ctx: &PipelineContext) -> String {
    let mut report = String::new();
    report.push_str(
        "## 创新验证报告（数据驱动）

",
    );
    report.push_str(&format!(
        "**新颖性评分：{:.0}/100**

",
        ctx.novelty_score
    ));
    let level = match ctx.novelty_score as u32 {
        80..=100 => "高度新颖",
        60..=79 => "较为新颖",
        40..=59 => "中等新颖",
        20..=39 => "新颖性较低",
        _ => "新颖性很低",
    };
    report.push_str(&format!(
        "**评估等级：** {}

",
        level
    ));
    report.push_str(
        "### 评分构成

",
    );
    report.push_str(&format!(
        "| 指标 | 数值 | 说明 |
|------|------|------|
| 最高相似度 | {:.1}% | 与最相似现有技术的匹配度 |
| Top5 平均相似度 | {:.1}% | 前 5 个最相似方案的平均值 |
| 矛盾信号加分 | +{:.0} | 检测到 {} 个技术路线矛盾 |
| 覆盖缺口加分 | +{:.0} | 搜索覆盖度 {:.0}% |

",
        ctx.score_breakdown.max_similarity * 100.0,
        ctx.score_breakdown.avg_top5_similarity * 100.0,
        ctx.score_breakdown.contradiction_bonus,
        ctx.contradictions.len(),
        ctx.score_breakdown.coverage_gap_bonus,
        ctx.diversity_score * 100.0,
    ));
    if !ctx.top_matches.is_empty() {
        report.push_str(
            "### 最相关的现有技术

",
        );
        for m in ctx.top_matches.iter().take(5) {
            report.push_str(&format!(
                "{}. **{}** (相似度 {:.1}%)
   {}

",
                m.rank,
                m.source_title,
                m.combined_score * 100.0,
                if m.snippet.len() > 100 {
                    format!("{}...", m.snippet.chars().take(100).collect::<String>())
                } else {
                    m.snippet.clone()
                },
            ));
        }
    }
    if !ctx.contradictions.is_empty() {
        report.push_str(
            "### 矛盾信号（创新机会）

",
        );
        for c in &ctx.contradictions {
            report.push_str(&format!(
                "- **{}**
  {}

",
                c.dimension, c.opportunity
            ));
        }
    }
    report
}
