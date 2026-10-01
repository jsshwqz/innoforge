//! MB4: 出处标注体系 — 事实性结论强制附源 + 决策建议段
//!
//! 三阶段：
//! 1. annotate_provenance: 解析 AI 分析报告中的事实性结论，匹配已有证据链，
//!    无源的标 【推测】。
//! 2. generate_decision: 基于新颖性得分、矛盾、证据链生成决策建议（申请/放弃/转向），
//!    每条理由引用已有证据编号。
//! 3. append_decision_section: 将决策建议段追加到报告末尾。

use crate::pipeline::context::{Evidence, PipelineContext};
use crate::types::idea::{DecisionReason, DecisionRecommendation, ProvenanceAnnotation};
use regex::Regex;

/// 从文本中提取专利号 / Extract patent number from text
fn extract_patent_number(text: &str) -> Option<String> {
    let re = Regex::new(r"(?i)\b([A-Z]{2})\s?(\d{6,})\s?([A-Z]\d?)\b").ok()?;
    let caps = re.captures(text)?;
    Some(format!("{}{}{}", &caps[1], &caps[2], &caps[3]))
}

/// 从文本中提取段/权号引用 / Extract section/claim reference from text
fn extract_section_ref(text: &str) -> Option<String> {
    let patterns = [
        (Regex::new(r"(?i)权利要求\s*(\d+)").ok()?, "claim"),
        (Regex::new(r"(?i)claim\s*(\d+)").ok()?, "claim"),
        (Regex::new(r"claims?\[(\d+)\]").ok()?, "claim"),
        (Regex::new(r"段\s*(\d+)").ok()?, "para"),
        (Regex::new(r"para\[(\d+)\]").ok()?, "para"),
        (Regex::new(r"\[引用(\d+)\]").ok()?, "ref"),
    ];
    for (re, prefix) in &patterns {
        if let Some(caps) = re.captures(text) {
            return Some(format!("{}[{}]", prefix, &caps[1]));
        }
    }
    None
}

/// 判断一行是否为事实性结论 / Determine if a line is a factual conclusion
fn is_factual_conclusion(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("---") {
        return false;
    }
    if trimmed.starts_with('-') || trimmed.starts_with('*') || trimmed.starts_with("•") {
        return true;
    }
    let conclusion_markers = [
        "因此",
        "所以",
        "表明",
        "显示",
        "证明",
        "可知",
        "得出",
        "说明",
        "意味着",
        "suggests",
        "indicates",
        "shows",
        "demonstrates",
        "therefore",
        "thus",
        "concludes",
    ];
    conclusion_markers.iter().any(|m| trimmed.contains(m))
}

/// 在证据链中按专利号查找 / Find evidence by patent number in the chain
fn find_evidence_by_patent<'a>(chain: &'a [Evidence], patent_num: &str) -> Option<&'a Evidence> {
    chain
        .iter()
        .find(|e| e.source_id.contains(patent_num) || e.source_title.contains(patent_num))
}

/// 阶段 1: 标注出处 — 解析 AI 分析报告，为每条事实性结论附源或标推测
pub fn annotate_provenance(ctx: &mut PipelineContext) {
    if ctx.ai_analysis.is_empty() {
        return;
    }

    let mut annotations = Vec::new();

    for line in ctx.ai_analysis.lines() {
        if !is_factual_conclusion(line) {
            continue;
        }

        let patent_num = extract_patent_number(line);
        let section_ref = extract_section_ref(line);

        let evidence_id = if let Some(ref num) = patent_num {
            find_evidence_by_patent(&ctx.evidence_chain, num).map(|e| e.id.clone())
        } else {
            None
        };

        let is_speculation = evidence_id.is_none();

        annotations.push(ProvenanceAnnotation {
            conclusion_text: line.trim().to_string(),
            evidence_id,
            is_speculation,
            patent_number: patent_num,
            section_ref,
        });
    }

    ctx.provenance_annotations = annotations;
}

/// 阶段 2: 生成决策建议 — 基于证据链、新颖性得分、矛盾生成申请/放弃/转向
pub fn generate_decision(ctx: &PipelineContext) -> Option<DecisionRecommendation> {
    if ctx.evidence_chain.is_empty() {
        return None;
    }

    let novelty = ctx.novelty_score;
    let has_contradictions = !ctx.contradictions.is_empty();
    let evidence_count = ctx.evidence_chain.len();

    let (recommendation, confidence) = if novelty >= 60.0 && !has_contradictions {
        ("申请", (novelty / 100.0).min(1.0))
    } else if novelty < 30.0 {
        ("放弃", (1.0 - novelty / 100.0).min(1.0))
    } else {
        ("转向", 0.5 + (novelty - 30.0) / 100.0 * 0.3)
    };

    let mut reasons = Vec::new();

    // 理由 1: 新颖性得分
    if let Some(ev) = ctx.evidence_chain.first() {
        reasons.push(DecisionReason {
            evidence_id: ev.id.clone(),
            text: format!(
                "综合新颖性得分 {:.1}/100，{}",
                novelty,
                if novelty >= 60.0 {
                    "具备申请潜力"
                } else if novelty < 30.0 {
                    "创新空间不足"
                } else {
                    "存在改进空间"
                }
            ),
        });
    }

    // 理由 2: 矛盾或证据充分度
    if has_contradictions {
        let ev = ctx
            .evidence_chain
            .iter()
            .find(|e| e.source_type == "contradiction")
            .or_else(|| ctx.evidence_chain.first());
        if let Some(ev) = ev {
            reasons.push(DecisionReason {
                evidence_id: ev.id.clone(),
                text: format!(
                    "检测到技术路线矛盾（{}），建议调整方向",
                    ctx.contradictions
                        .first()
                        .map(|c| format!("{} vs {}", c.source_a, c.source_b))
                        .unwrap_or_default()
                ),
            });
        }
    } else if evidence_count > 1 {
        if let Some(ev) = ctx.evidence_chain.get(1) {
            reasons.push(DecisionReason {
                evidence_id: ev.id.clone(),
                text: format!(
                    "已有 {} 条证据支撑分析结论，证据充分度{}",
                    evidence_count,
                    if evidence_count >= 5 {
                        "较高"
                    } else {
                        "中等"
                    }
                ),
            });
        }
    }

    // 理由 3: 最高置信度证据
    if let Some(ev) = ctx.evidence_chain.iter().max_by(|a, b| {
        a.confidence
            .partial_cmp(&b.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    }) {
        reasons.push(DecisionReason {
            evidence_id: ev.id.clone(),
            text: format!(
                "最高单条证据置信度 {:.0}%，来源「{}」",
                ev.confidence * 100.0,
                ev.source_title
            ),
        });
    }

    // 补足至 ≥3 条理由
    while reasons.len() < 3 && !ctx.evidence_chain.is_empty() {
        let idx = reasons.len() % ctx.evidence_chain.len();
        let ev = &ctx.evidence_chain[idx];
        if reasons.iter().any(|r| r.evidence_id == ev.id) {
            break;
        }
        reasons.push(DecisionReason {
            evidence_id: ev.id.clone(),
            text: format!("辅助证据：{}", ev.claim),
        });
    }

    if reasons.len() < 3 {
        return None;
    }

    Some(DecisionRecommendation {
        recommendation: recommendation.to_string(),
        reasons,
        confidence,
    })
}

/// 阶段 3: 将决策建议段追加到报告末尾
pub fn append_decision_section(ctx: &mut PipelineContext) {
    let decision = match &ctx.decision {
        Some(d) => d,
        None => return,
    };

    let mut section = String::new();
    section.push_str("\n\n---\n\n## 决策建议\n\n");
    section.push_str(&format!("**建议：{}**\n\n", decision.recommendation));
    section.push_str("理由（引用证据编号）：\n\n");
    for (i, reason) in decision.reasons.iter().enumerate() {
        section.push_str(&format!(
            "{}. {}（证据编号：`{}`）\n",
            i + 1,
            reason.text,
            reason.evidence_id
        ));
    }
    section.push_str(&format!(
        "\n综合置信度：{:.0}%\n",
        decision.confidence * 100.0
    ));

    ctx.ai_analysis.push_str(&section);
}

/// 完整执行三阶段 / Run all three phases
pub fn run_provenance_pipeline(ctx: &mut PipelineContext) {
    annotate_provenance(ctx);
    if let Some(decision) = generate_decision(ctx) {
        ctx.decision = Some(decision);
    }
    append_decision_section(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::context::{Contradiction, PipelineContext};
    use crate::types::idea::Evidence;

    fn make_test_evidence(id: &str, patent_num: &str, title: &str, confidence: f64) -> Evidence {
        Evidence {
            id: id.to_string(),
            idea_id: "test-idea".to_string(),
            claim: format!("与{}相关", title),
            source_type: "patent".to_string(),
            source_id: patent_num.to_string(),
            source_title: title.to_string(),
            source_url: String::new(),
            claim_number: None,
            excerpt: String::new(),
            relation: "supports".to_string(),
            confidence,
            produced_by: "test".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_extract_patent_number() {
        assert_eq!(
            extract_patent_number("参考 CN123456A 的设计"),
            Some("CN123456A".to_string())
        );
        assert_eq!(
            extract_patent_number("US12345678B2 描述了方法"),
            Some("US12345678B2".to_string())
        );
        assert_eq!(extract_patent_number("没有专利号的文本"), None);
    }

    #[test]
    fn test_extract_section_ref() {
        assert_eq!(
            extract_section_ref("根据权利要求1"),
            Some("claim[1]".to_string())
        );
        assert_eq!(
            extract_section_ref("see claim 3"),
            Some("claim[3]".to_string())
        );
        assert_eq!(
            extract_section_ref("claims[5] shows"),
            Some("claim[5]".to_string())
        );
        assert_eq!(extract_section_ref("无引用文本"), None);
    }

    #[test]
    fn test_is_factual_conclusion() {
        assert!(is_factual_conclusion("- 因此该方案可行"));
        assert!(is_factual_conclusion("* 这表明技术成熟"));
        assert!(!is_factual_conclusion("# 标题"));
        assert!(!is_factual_conclusion(""));
        assert!(!is_factual_conclusion("---"));
        assert!(is_factual_conclusion("因此可以得出结论"));
    }

    #[test]
    fn test_annotate_provenance_with_patent_source() {
        let mut ctx = PipelineContext::new("test-idea", "测试创意", "测试描述");
        ctx.ai_analysis = "- 因此该方案参考了 CN123456A 的设计\n".to_string();
        ctx.evidence_chain
            .push(make_test_evidence("ev-1", "CN123456A", "测试专利", 0.8));

        annotate_provenance(&mut ctx);

        assert_eq!(ctx.provenance_annotations.len(), 1);
        let ann = &ctx.provenance_annotations[0];
        assert!(!ann.is_speculation);
        assert_eq!(ann.evidence_id, Some("ev-1".to_string()));
        assert_eq!(ann.patent_number, Some("CN123456A".to_string()));
    }

    #[test]
    fn test_annotate_provenance_speculation() {
        let mut ctx = PipelineContext::new("test-idea", "测试创意", "测试描述");
        ctx.ai_analysis = "- 因此该方案具有创新性\n".to_string();
        annotate_provenance(&mut ctx);

        assert_eq!(ctx.provenance_annotations.len(), 1);
        let ann = &ctx.provenance_annotations[0];
        assert!(ann.is_speculation);
        assert!(ann.evidence_id.is_none());
    }

    #[test]
    fn test_generate_decision_high_novelty_apply() {
        let mut ctx = PipelineContext::new("test-idea", "测试创意", "测试描述");
        ctx.novelty_score = 75.0;
        ctx.evidence_chain
            .push(make_test_evidence("ev-1", "CN001", "专利A", 0.9));
        ctx.evidence_chain
            .push(make_test_evidence("ev-2", "CN002", "专利B", 0.7));
        ctx.evidence_chain
            .push(make_test_evidence("ev-3", "CN003", "专利C", 0.5));

        let decision = generate_decision(&ctx).unwrap();
        assert_eq!(decision.recommendation, "申请");
        assert!(decision.reasons.len() >= 3);
        for reason in &decision.reasons {
            assert!(
                ctx.evidence_chain
                    .iter()
                    .any(|e| e.id == reason.evidence_id),
                "Reason evidence_id {} not found in chain",
                reason.evidence_id
            );
        }
    }

    #[test]
    fn test_generate_decision_low_novelty_abandon() {
        let mut ctx = PipelineContext::new("test-idea", "测试创意", "测试描述");
        ctx.novelty_score = 20.0;
        ctx.evidence_chain
            .push(make_test_evidence("ev-1", "CN001", "专利A", 0.9));
        ctx.evidence_chain
            .push(make_test_evidence("ev-2", "CN002", "专利B", 0.7));
        ctx.evidence_chain
            .push(make_test_evidence("ev-3", "CN003", "专利C", 0.5));

        let decision = generate_decision(&ctx).unwrap();
        assert_eq!(decision.recommendation, "放弃");
        assert!(decision.reasons.len() >= 3);
    }

    #[test]
    fn test_generate_decision_with_contradictions_pivot() {
        let mut ctx = PipelineContext::new("test-idea", "测试创意", "测试描述");
        ctx.novelty_score = 50.0;
        ctx.contradictions.push(Contradiction {
            source_a: "路线A".to_string(),
            source_b: "路线B".to_string(),
            dimension: "材料".to_string(),
            signal_strength: 0.8,
            opportunity: "材料冲突".to_string(),
        });
        ctx.evidence_chain
            .push(make_test_evidence("ev-1", "CN001", "专利A", 0.9));
        ctx.evidence_chain
            .push(make_test_evidence("ev-2", "CN002", "专利B", 0.7));
        ctx.evidence_chain
            .push(make_test_evidence("ev-3", "CN003", "专利C", 0.5));

        let decision = generate_decision(&ctx).unwrap();
        assert_eq!(decision.recommendation, "转向");
        assert!(decision.reasons.len() >= 3);
    }

    #[test]
    fn test_generate_decision_no_evidence_returns_none() {
        let ctx = PipelineContext::new("test-idea", "测试创意", "测试描述");
        assert!(generate_decision(&ctx).is_none());
    }

    #[test]
    fn test_append_decision_section() {
        let mut ctx = PipelineContext::new("test-idea", "测试创意", "测试描述");
        ctx.ai_analysis = "分析报告正文\n".to_string();
        ctx.decision = Some(DecisionRecommendation {
            recommendation: "申请".to_string(),
            reasons: vec![
                DecisionReason {
                    evidence_id: "ev-1".to_string(),
                    text: "新颖性高".to_string(),
                },
                DecisionReason {
                    evidence_id: "ev-2".to_string(),
                    text: "无矛盾".to_string(),
                },
                DecisionReason {
                    evidence_id: "ev-3".to_string(),
                    text: "证据充分".to_string(),
                },
            ],
            confidence: 0.85,
        });

        append_decision_section(&mut ctx);

        assert!(ctx.ai_analysis.contains("## 决策建议"));
        assert!(ctx.ai_analysis.contains("**建议：申请**"));
        assert!(ctx.ai_analysis.contains("证据编号：`ev-1`"));
        assert!(ctx.ai_analysis.contains("综合置信度：85%"));
    }

    #[test]
    fn test_run_provenance_pipeline_integration() {
        let mut ctx = PipelineContext::new("test-idea", "测试创意", "测试描述");
        ctx.novelty_score = 80.0;
        ctx.ai_analysis = "- 因此该方案参考了 CN123456A 的设计\n- 该技术具有创新性\n".to_string();
        ctx.evidence_chain
            .push(make_test_evidence("ev-1", "CN123456A", "测试专利", 0.9));
        ctx.evidence_chain
            .push(make_test_evidence("ev-2", "CN002", "专利B", 0.7));
        ctx.evidence_chain
            .push(make_test_evidence("ev-3", "CN003", "专利C", 0.5));

        run_provenance_pipeline(&mut ctx);

        // Provenance annotations should be created
        assert!(!ctx.provenance_annotations.is_empty());
        // Decision should be generated
        let decision = ctx.decision.as_ref().expect("Decision should be generated");
        assert_eq!(decision.recommendation, "申请");
        // Report should contain decision section
        assert!(ctx.ai_analysis.contains("## 决策建议"));
        // Evidence IDs in reasons should be反查able
        for reason in &decision.reasons {
            assert!(
                ctx.evidence_chain
                    .iter()
                    .any(|e| e.id == reason.evidence_id),
                "Reason evidence_id {} not found in chain",
                reason.evidence_id
            );
        }
    }
}
