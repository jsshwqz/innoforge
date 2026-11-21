//! Step 13: Finalize - aggregate report, persist to database
//! Type: CODE

use crate::db::memory::IdeaMemory;
use crate::db::Database;
use crate::pipeline::context::PipelineContext;
use anyhow::Result;
use std::sync::Arc;

pub async fn execute(ctx: &mut PipelineContext, db: &Arc<Database>) -> Result<()> {
    if ctx.novelty_status == crate::pipeline::context::NoveltyStatus::Sufficient {
        ctx.novelty_score = ctx.novelty_score.clamp(0.0, 100.0);
    }

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

    // MB1: 幻觉防线——事实校验（在 ai_analysis 确定后、provenance 之前执行）
    super::fact_check::run_fact_check(ctx);
    if ctx.fact_check_rejected {
        let fc_score = ctx
            .fact_check_report
            .as_ref()
            .map(|r| r.score)
            .unwrap_or(0.0);
        tracing::warn!(
            "Fact check rejected: score={:.1} below threshold — suppressing decision output",
            fc_score
        );
        ctx.decision = None;
        let block_banner = format!(
            "> ⚠️ **【证据与事实核验门禁拦截声明】**：本轮分析未通过确定性事实核验（核验得分：{:.0}/100），\
             系统已硬性阻断正面立项/专利申请决策建议。请重点核查下方标记为 `【未证实】`、`【无原文支撑】` 或 `【修改超范围】` 的条目并补充真实对比文献原文。\n\n",
            fc_score
        );
        if !ctx.ai_analysis.contains("【证据与事实核验门禁拦截声明】") {
            ctx.ai_analysis = format!("{block_banner}{}", ctx.ai_analysis);
        }
    }

    // EK-v2: 证据核验内核（docs/design/evidence-kernel-v2.md）。
    // 逐句绑定证据：未支撑断言降级标注【未证实】；报告追加到 ai_analysis 尾部；
    // 决策门（grounding < 0.5 或证据不足或事实核验被拒）抑制申请/放弃/转向建议。
    let corpus = crate::evidence_kernel::corpus_from_chunks(&ctx.rag_chunks);
    let (kernel_report, kernel_text) =
        crate::evidence_kernel::run_kernel(&ctx.ai_analysis, &corpus);
    if kernel_text != ctx.ai_analysis {
        ctx.ai_analysis = kernel_text;
    }
    ctx.kernel_report = serde_json::to_string(&kernel_report).ok();
    let ek_suppressed = crate::evidence_kernel::should_suppress_decision(&kernel_report);
    let allow_decision = !ctx.fact_check_rejected && !ek_suppressed;
    if !allow_decision {
        ctx.decision = None;
        tracing::warn!(
            "Decision gate blocked: fact_check_rejected={} ek_status={:?} grounding={:.2} — 决策建议被抑制",
            ctx.fact_check_rejected,
            kernel_report.status,
            kernel_report.rubric.grounding
        );
    }
    ctx.ai_analysis
        .push_str(&crate::evidence_kernel::report::render_markdown(
            &kernel_report,
        ));

    // MB4: 出处标注 + 决策建议段（在事实校验之后、记忆提取之前执行）
    super::provenance::run_provenance_pipeline(ctx, allow_decision);
    if !ctx.provenance_annotations.is_empty() {
        let speculation_count = ctx
            .provenance_annotations
            .iter()
            .filter(|a| a.is_speculation)
            .count();
        tracing::info!(
            "Provenance annotated: {} conclusions ({} speculation)",
            ctx.provenance_annotations.len(),
            speculation_count
        );
    }

    let mut memory = extract_memory_entries(ctx);
    let feature_cards = db
        .get_feature_cards_by_idea(&ctx.idea_id)
        .unwrap_or_default();
    let bom_pack = super::deliverables::build_engineering_bom_pack(ctx, &feature_cards);
    let journal_draft = super::deliverables::build_journal_manuscript_draft(ctx);
    let verified_dois = super::deliverables::extract_verified_dois(ctx);
    let db_patents = db
        .get_patents_by_numbers(&verified_dois)
        .unwrap_or_default();
    let bib_entries = super::deliverables::extract_verified_bib_entries(ctx, &db_patents);
    let bibtex_content = super::deliverables::format_bibtex(&bib_entries);
    let step13_svg = super::deliverables::extract_simulation_plot_data(ctx, None)
        .map(|plot| super::deliverables::render_simulation_svg(&plot));
    let now = chrono::Utc::now().to_rfc3339();
    memory.push(IdeaMemory {
        id: format!("{}-deliverable-bom-{}", ctx.idea_id, uuid::Uuid::new_v4()),
        idea_id: ctx.idea_id.clone(),
        concept_name: "deliverable_bom".to_string(),
        concept_type: "decision".to_string(),
        content: bom_pack.full_markdown,
        confidence: 0.9,
        source_step: Some("FinalizeDeliverables".to_string()),
        created_at: now.clone(),
        updated_at: now.clone(),
    });
    memory.push(IdeaMemory {
        id: format!(
            "{}-deliverable-journal-{}",
            ctx.idea_id,
            uuid::Uuid::new_v4()
        ),
        idea_id: ctx.idea_id.clone(),
        concept_name: "deliverable_journal_draft".to_string(),
        concept_type: "decision".to_string(),
        content: journal_draft.full_markdown,
        confidence: 0.9,
        source_step: Some("FinalizeDeliverables".to_string()),
        created_at: now.clone(),
        updated_at: now.clone(),
    });
    if !bib_entries.is_empty() {
        memory.push(IdeaMemory {
            id: format!("{}-deliverable-bib-{}", ctx.idea_id, uuid::Uuid::new_v4()),
            idea_id: ctx.idea_id.clone(),
            concept_name: "deliverable_journal_bibtex".to_string(),
            concept_type: "decision".to_string(),
            content: bibtex_content,
            confidence: 0.9,
            source_step: Some("FinalizeDeliverables".to_string()),
            created_at: now.clone(),
            updated_at: now.clone(),
        });
    }
    if let Some(svg) = step13_svg {
        memory.push(IdeaMemory {
            id: format!("{}-deliverable-svg-{}", ctx.idea_id, uuid::Uuid::new_v4()),
            idea_id: ctx.idea_id.clone(),
            concept_name: "deliverable_simulation_svg".to_string(),
            concept_type: "decision".to_string(),
            content: svg,
            confidence: 0.9,
            source_step: Some("FinalizeDeliverables".to_string()),
            created_at: now.clone(),
            updated_at: now,
        });
    }
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
    // #70：证据不足时不给数值分
    if ctx.novelty_status != crate::pipeline::context::NoveltyStatus::Sufficient {
        return format!(
            "## 创新验证报告（数据驱动）\n\n\
             **新颖性评分：无法评估（证据不足）**\n\n\
             未获得任何对比文献：本地库 0 条 / 在线搜索未配置，无法评估新颖性。\n\n\
             ### 可操作出路\n\
             - 导入文献到本地库后再推演；\n\
             - 配置 SerpAPI/EPO 在线搜索后重新推演。\n\n\
             ### 搜索结果\n\
             - 网络结果：{} 条\n\
             - 专利结果：{} 条\n",
            ctx.web_results.len(),
            ctx.patent_results.len(),
        );
    }
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
        ctx.diversity_score.unwrap_or(0.0) * 100.0,
    ));
    if !ctx.top_matches.is_empty() {
        report.push_str(
            "### 最相关的现有技术

",
        );
        for m in ctx.top_matches.iter().take(5) {
            // MC1: 引用编号对齐 MB3 的 {ref_no, patent_id} 形状
            let ref_no = format!("[引用{}]", m.rank);
            let patent_ref = if m.source_id.is_empty() {
                String::new()
            } else {
                format!(" (patent_id={})", m.source_id)
            };
            report.push_str(&format!(
                "{}. **{}** (相似度 {:.1}%) {}{}
   {}

",
                m.rank,
                m.source_title,
                m.combined_score * 100.0,
                ref_no,
                patent_ref,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patent::{FeatureCard, Idea};

    #[tokio::test]
    async fn finalize_persists_deliverables_without_altering_ek_grounding() {
        let db = Arc::new(Database::init(":memory:").expect("in-memory db"));
        let now = "2026-11-18T00:00:00Z".to_string();
        db.insert_idea(&Idea {
            id: "idea-fin-1".to_string(),
            title: "微流控比例阀".to_string(),
            description: "采用压电叠堆驱动的微流控比例调节阀".to_string(),
            input_type: "text".to_string(),
            status: "analyzing".to_string(),
            analysis: String::new(),
            web_results: String::new(),
            patent_results: String::new(),
            novelty_score: None,
            created_at: now.clone(),
            updated_at: now.clone(),
            discussion_summary: String::new(),
        })
        .expect("insert idea");
        db.insert_feature_card(&FeatureCard {
            id: "fc-fin-1".to_string(),
            idea_id: "idea-fin-1".to_string(),
            title: "压电驱动微阀芯".to_string(),
            description: "高频响微流量控制".to_string(),
            novelty_score: Some(0.8),
            created_at: now,
            technical_problem: "传统电磁阀响应慢".to_string(),
            core_structure: "压电叠堆致动器与弹性膜片阀座".to_string(),
            key_relations: "位移放大机构连接阀针".to_string(),
            process_steps: "流量传感器闭环调节驱动电压".to_string(),
            application_scenarios: "生物芯片进样".to_string(),
        })
        .expect("insert feature card");

        let mut ctx = PipelineContext::new(
            "idea-fin-1",
            "微流控比例阀",
            "采用压电叠堆驱动的微流控比例调节阀",
        );
        let initial_analysis = "本方案通过压电叠堆致动器实现微流量调节。".to_string();
        ctx.ai_analysis = initial_analysis.clone();

        let corpus = crate::evidence_kernel::corpus_from_chunks(&ctx.rag_chunks);
        let (expected_kernel, _) = crate::evidence_kernel::run_kernel(&initial_analysis, &corpus);

        execute(&mut ctx, &db).await.expect("finalize execute");

        let kernel_json = ctx.kernel_report.as_deref().expect("kernel report present");
        let parsed: serde_json::Value =
            serde_json::from_str(kernel_json).expect("valid kernel json");
        let actual_grounding = parsed
            .get("rubric")
            .and_then(|r| r.get("grounding"))
            .and_then(|g| g.as_f64())
            .expect("grounding score");
        assert!((actual_grounding - expected_kernel.rubric.grounding).abs() < 1e-9);

        let saved = db
            .get_memory_entries("idea-fin-1")
            .expect("query memory entries");
        let bom_entry = saved
            .iter()
            .find(|m| m.concept_name == "deliverable_bom")
            .expect("deliverable_bom persisted");
        assert_eq!(bom_entry.concept_type, "decision");
        assert_eq!(
            bom_entry.source_step.as_deref(),
            Some("FinalizeDeliverables")
        );
        assert!(bom_entry.content.contains("机械与电控分类 BOM 清单"));
        assert!(bom_entry.content.contains("压电叠堆致动器与弹性膜片阀座"));

        let journal_entry = saved
            .iter()
            .find(|m| m.concept_name == "deliverable_journal_draft")
            .expect("deliverable_journal_draft persisted");
        assert_eq!(journal_entry.concept_type, "decision");
        assert_eq!(
            journal_entry.source_step.as_deref(),
            Some("FinalizeDeliverables")
        );
        assert!(journal_entry.content.contains("防破新颖性安全锁"));
    }

    #[tokio::test]
    async fn finalize_blocks_decision_and_injects_banner_when_fact_check_rejected() {
        let db = Arc::new(Database::init(":memory:").expect("in-memory db"));
        let mut ctx = PipelineContext::new(
            "idea-fin-reject",
            "固态电池隔热片",
            "采用气凝胶与相变层复合隔热",
        );
        // 构造触发 C12 致命禁令（"属于常规设计"）与虚构段落引用的分析文本，必然触发 fact_check_rejected = true
        ctx.top_matches.push(crate::pipeline::context::RankedMatch {
            source_id: "CN001".to_string(),
            source_title: "对比文件1".to_string(),
            source_type: "patent".to_string(),
            source_url: String::new(),
            snippet: "[0001] 简短现有技术摘要。".to_string(),
            combined_score: 0.8,
            ..Default::default()
        });
        ctx.ai_analysis =
            "该区别特征属于常规设计，且对比文件1第[9999]段、第[8888]段记载效率提升99.9%。"
                .to_string();

        execute(&mut ctx, &db).await.expect("finalize execute");

        assert!(
            ctx.fact_check_rejected,
            "Fabricated citations without backing evidence must trigger fact_check_rejected"
        );
        assert!(
            ctx.decision.is_none(),
            "Decision recommendation must be suppressed (None) when fact_check_rejected is true"
        );
        assert!(
            ctx.ai_analysis.contains("【证据与事实核验门禁拦截声明】"),
            "Blocking banner must be injected into ai_analysis when fact check fails"
        );
        assert!(
            ctx.ai_analysis.contains("本次不输出申请/放弃/转向建议"),
            "Decision section must explicitly state that recommendations are suppressed"
        );
    }
}
