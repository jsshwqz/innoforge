//! 五维评分（grounding / coverage / legal_reasoning / clarity / consistency）。
//!
//! 评分哲学（ADR，见设计文档 §4）：grounding 是乘法门——「看起来好」与「有证据」的区分
//! 由数值结构保证：overall = grounding × (coverage/legal/clarity/consistency 加权)，
//! 因此 grounding=0（空证据硬门 I3）时无论其余维度多“漂亮”，overall 恒 0。

use crate::evidence_kernel::model::{
    ClaimKind, ClaimRecord, GroundingMetrics, KernelStatus, RubricScore, VerifyStatus,
};
use std::collections::HashSet;

/// 从核验后的 claim 记录计算 grounding 计量（#79 分母规则：分母 = 可解析锚点）。
pub fn compute_metrics(records: &[ClaimRecord]) -> GroundingMetrics {
    let mut m = GroundingMetrics::default();
    let mut parseable = 0usize;
    let mut verified_weight_sum = 0.0f64;
    let mut fact_claims = 0usize;
    for r in records {
        if r.kind == ClaimKind::FactFromSource || r.kind == ClaimKind::LegalArgument {
            fact_claims += 1;
            let mut claim_weight = 0.0f64;
            for (ev, status) in &r.verifications {
                match status {
                    VerifyStatus::VerifiedExact { .. } => m.verified_exact += 1,
                    VerifyStatus::VerifiedApprox { .. } => m.verified_approx += 1,
                    VerifyStatus::AnchorOnly { .. } => m.anchor_only += 1,
                    VerifyStatus::AnchorNotFound => m.anchor_not_found += 1,
                    VerifyStatus::QuoteMismatch { .. } => m.quote_mismatch += 1,
                    VerifyStatus::UnparsedAnchor => m.unparsed_anchor += 1,
                }
                if ev.anchor.is_some() {
                    parseable += 1;
                }
                if let Some(w) = status.verified_weight() {
                    claim_weight = claim_weight.max(w);
                }
            }
            if claim_weight > 0.0 && !r.downgraded {
                verified_weight_sum += claim_weight;
            }
        }
    }
    m.fact_claims = fact_claims;
    m.grounding = if fact_claims == 0 {
        None
    } else {
        Some(verified_weight_sum / fact_claims as f64)
    };
    m.hallucination_rate = if parseable == 0 {
        None
    } else {
        Some((m.anchor_not_found + m.quote_mismatch) as f64 / parseable as f64)
    };
    m
}

/// 全维评分。status 影响 grounding（InsufficientEvidence → 0，硬门 I3）。
pub fn score(
    records: &[ClaimRecord],
    metrics: &GroundingMetrics,
    total_sections: usize,
    status: KernelStatus,
    text: &str,
) -> RubricScore {
    // 无事实句（纯推论/纯建议文本）不因 grounding 虚降；空证据场景由 InsufficientEvidence 硬门处理
    let grounding = match status {
        KernelStatus::InsufficientEvidence => 0.0,
        _ => metrics.grounding.unwrap_or(1.0).clamp(0.0, 1.0),
    };

    // coverage：被引用（含仅锚点）过的不同 section 数 / 语料 section 总数
    let cited: HashSet<&String> = records
        .iter()
        .flat_map(|r| r.verifications.iter())
        .filter_map(|(_, s)| match s {
            VerifyStatus::VerifiedExact { section_id }
            | VerifyStatus::VerifiedApprox { section_id, .. }
            | VerifyStatus::AnchorOnly { section_id } => Some(section_id),
            _ => None,
        })
        .collect();
    let coverage = if total_sections == 0 {
        0.0
    } else {
        (cited.len() as f64 / total_sections as f64).clamp(0.0, 1.0)
    };

    // legal_reasoning：法律论证句中带可核验引用的比例（无法律句则中性 1.0，不虚增不虚减）
    let legal: Vec<&ClaimRecord> = records
        .iter()
        .filter(|r| r.kind == ClaimKind::LegalArgument)
        .collect();
    let legal_reasoning = if legal.is_empty() {
        1.0
    } else {
        let anchored = legal
            .iter()
            .filter(|r| {
                r.verifications
                    .iter()
                    .any(|(_, s)| s.verified_weight().is_some())
            })
            .count();
        (anchored as f64 / legal.len() as f64).clamp(0.0, 1.0)
    };

    // clarity：结构代理——标题存在 + 平均句长 8..=120 字 + 无 >300 字巨句
    let sentences = crate::evidence_kernel::classifier::split_sentences(text);
    let avg_len = if sentences.is_empty() {
        0.0
    } else {
        sentences
            .iter()
            .map(|(s, _)| s.chars().count())
            .sum::<usize>() as f64
            / sentences.len() as f64
    };
    let has_header = text.lines().any(|l| l.starts_with('#'));
    let has_giant = sentences.iter().any(|(s, _)| s.chars().count() > 300);
    let clarity = {
        let base: f64 = if avg_len == 0.0 {
            0.5
        } else if (8.0..=120.0).contains(&avg_len) {
            1.0
        } else {
            0.7
        };
        let header_factor: f64 = if has_header { 1.0 } else { 0.9 };
        let giant_factor: f64 = if has_giant { 0.8 } else { 1.0 };
        (base * header_factor * giant_factor).clamp(0.0, 1.0)
    };

    let consistency = detect_negation_conflicts(records);

    let mix = 0.35 * coverage + 0.20 * legal_reasoning + 0.25 * clarity + 0.20 * consistency;
    let overall = (grounding * mix).clamp(0.0, 1.0);
    RubricScore {
        grounding,
        coverage,
        legal_reasoning,
        clarity,
        consistency,
        overall,
    }
}

/// 检测成对矛盾：同一 doc_id 上，一句断言「公开了 X」而另一句断言「未公开 X」，
/// 且两句各有 ≥1 个已核验锚点。返回一致性分（无矛盾 1.0；每对 -0.5，下限 0）。
fn detect_negation_conflicts(records: &[ClaimRecord]) -> f64 {
    fn is_pos(s: &str) -> bool {
        s.contains("公开了") || s.contains("记载了") || s.contains("披露了")
    }
    fn is_neg(s: &str) -> bool {
        s.contains("未公开")
            || s.contains("没有公开")
            || s.contains("并未记载")
            || s.contains("未记载")
    }
    fn anchored_docs(r: &ClaimRecord) -> HashSet<String> {
        // 矛盾检测只问「锚点是否真实命中该 doc」（existence 语义）——含仅锚点引用；
        // 引文对不上、伪造锚点、无法解析不算（与判定原文「各有 ≥1 个已核验锚点」的
        // 命中面一致，verified_weight() 已收紧为逐字核验，这里不随之收窄）。
        r.verifications
            .iter()
            .filter(|(_, s)| {
                matches!(
                    s,
                    VerifyStatus::VerifiedExact { .. }
                        | VerifyStatus::VerifiedApprox { .. }
                        | VerifyStatus::AnchorOnly { .. }
                )
            })
            .filter_map(|(e, _)| e.anchor.as_ref().map(|a| a.doc_id.clone()))
            .collect()
    }
    let facts: Vec<&ClaimRecord> = records
        .iter()
        .filter(|r| r.kind == ClaimKind::FactFromSource && !r.downgraded)
        .collect();
    let mut pairs = 0usize;
    for i in 0..facts.len() {
        for j in (i + 1)..facts.len() {
            let (a, b) = (facts[i], facts[j]);
            let shares_doc = !anchored_docs(a).is_disjoint(&anchored_docs(b));
            if shares_doc
                && ((is_pos(&a.text) && is_neg(&b.text)) || (is_pos(&b.text) && is_neg(&a.text)))
            {
                pairs += 1;
            }
        }
    }
    (1.0 - 0.5 * pairs as f64).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grounding_zero_when_insufficient() {
        let metrics = GroundingMetrics {
            grounding: Some(0.9),
            ..Default::default()
        };
        let r = score(
            &[],
            &metrics,
            10,
            KernelStatus::InsufficientEvidence,
            "文本",
        );
        assert_eq!(r.grounding, 0.0);
        assert_eq!(
            r.overall, 0.0,
            "空证据门：overall 必须为 0（负控 NC3 断言形态）"
        );
    }

    #[test]
    fn grounding_caps_overall() {
        let metrics = GroundingMetrics {
            grounding: Some(0.2),
            ..Default::default()
        };
        let r = score(&[], &metrics, 0, KernelStatus::Ok, "# 标题\n正文。");
        assert!(r.overall <= 0.2 + 1e-9, "overall={}", r.overall);
    }

    #[test]
    fn hallucination_rate_denominator_is_parseable_only() {
        use crate::evidence_kernel::model::*;
        let records = vec![ClaimRecord {
            claim_id: "C1".into(),
            sentence_index: 0,
            text: "x".into(),
            kind: ClaimKind::FactFromSource,
            downgraded: false,
            verifications: vec![
                (
                    EvidenceRef {
                        raw: "[D1:第9999段]".into(),
                        anchor: Some(Anchor {
                            doc_id: "D1".into(),
                            kind: LocatorKind::Paragraph,
                            number: 9999,
                        }),
                        quote: "q".into(),
                    },
                    VerifyStatus::AnchorNotFound,
                ),
                (
                    EvidenceRef {
                        raw: "乱格式".into(),
                        anchor: None,
                        quote: String::new(),
                    },
                    VerifyStatus::UnparsedAnchor,
                ),
            ],
        }];
        let m = compute_metrics(&records);
        // 分母 = 1（可解析），unparsed 不进分母（#79）
        assert_eq!(m.hallucination_rate, Some(1.0));
    }
}
