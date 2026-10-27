//! 自我批判与修复循环（ADR-6）：核验 → 未支撑句清单 → 修复或降级。
//!
//! 修复是注入式的（`RepairFn`），v1 生产用 `NoRepair`（直接降级标注），测试可注入 fake repair；
//! AI 修复轮在后续版本接入，产物仍须过同一核验器（ADR-1：核验永远不吃 AI 的输出当判据）。
//! 降级 = 在句前插入 `【未证实】` 标注并保留原句——绝不静默删除（用户可见性优先）。

use crate::evidence_kernel::model::{ClaimKind, ClaimRecord, Finding, VerifyStatus};

/// 修复函数：输入 (原文, 未支撑清单)，返回改写后的文本。
pub type RepairFn<'a> = Box<dyn Fn(&str, &[Finding]) -> String + 'a>;

/// 不修复：直接对未支撑句做降级标注。
pub fn no_repair() -> RepairFn<'static> {
    Box::new(|text: &str, findings: &[Finding]| apply_downgrades(text, findings))
}

/// 在每个未支撑句前插入降级标注（按句子文本定位，重复句全部标注——保守策略）。
pub fn apply_downgrades(text: &str, findings: &[Finding]) -> String {
    let targets: Vec<&str> = findings.iter().map(|f| f.text.as_str()).collect();
    if targets.is_empty() {
        return text.to_string();
    }
    let mut out = text.to_string();
    for t in targets {
        if !t.is_empty() && out.contains(t) {
            out = out.replace(t, &format!("【未证实】{t}"));
        }
    }
    out
}

/// 从核验记录中挑出需要进批判清单的 claim：
/// FactFromSource / LegalArgument 句，且无任何已核验证据。
pub fn collect_findings(records: &[ClaimRecord]) -> Vec<Finding> {
    records
        .iter()
        .filter(|r| {
            (r.kind == ClaimKind::FactFromSource || r.kind == ClaimKind::LegalArgument)
                && !r.downgraded
                && r.verifications
                    .iter()
                    .all(|(_, s)| s.verified_weight().is_none())
        })
        .map(|r| {
            let has_unparsed = r
                .verifications
                .iter()
                .any(|(_, s)| matches!(s, VerifyStatus::UnparsedAnchor));
            let anchor_only_only = !r.verifications.is_empty()
                && r.verifications
                    .iter()
                    .all(|(_, s)| matches!(s, VerifyStatus::AnchorOnly { .. }));
            let reason = if r.verifications.is_empty() {
                "事实断言无任何引用".to_string()
            } else if has_unparsed {
                "引用格式无法解析为锚点".to_string()
            } else if anchor_only_only {
                "仅有锚点命中、无逐字引文支撑（existence ≠ support）".to_string()
            } else {
                "引用未通过核验（锚点或引文不成立）".to_string()
            };
            Finding {
                claim_id: r.claim_id.clone(),
                sentence_index: r.sentence_index,
                text: r.text.clone(),
                reason,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence_kernel::model::*;

    fn record(id: &str, text: &str, kind: ClaimKind, statuses: Vec<VerifyStatus>) -> ClaimRecord {
        ClaimRecord {
            claim_id: id.into(),
            sentence_index: 0,
            text: text.into(),
            kind,
            downgraded: false,
            verifications: statuses
                .into_iter()
                .map(|s| {
                    (
                        EvidenceRef {
                            raw: "[D1:第0001段]".into(),
                            anchor: Some(Anchor {
                                doc_id: "D1".into(),
                                kind: LocatorKind::Paragraph,
                                number: 1,
                            }),
                            quote: String::new(),
                        },
                        s,
                    )
                })
                .collect(),
        }
    }

    /// NC4 修复后的语义：仅锚点命中（AnchorOnly）不算有支撑——它与锚点未命中
    /// 一样必须进批判清单，且 reason 各自点名（existence ≠ support）。
    #[test]
    fn unsupported_fact_claims_become_findings() {
        let records = vec![
            record(
                "C1",
                "仅锚点句。",
                ClaimKind::FactFromSource,
                vec![VerifyStatus::AnchorOnly {
                    section_id: "D1:para:1".into(),
                }],
            ),
            record(
                "C2",
                "无证据句。",
                ClaimKind::FactFromSource,
                vec![VerifyStatus::AnchorNotFound],
            ),
            record("C3", "推论句。", ClaimKind::AnalysisInference, vec![]),
        ];
        let findings = collect_findings(&records);
        assert_eq!(
            findings.len(),
            2,
            "仅锚点与未命中都不算支撑: {:?}",
            findings
        );
        let c1 = findings.iter().find(|f| f.claim_id == "C1").expect("C1");
        assert!(
            c1.reason.contains("仅有锚点"),
            "C1 应点名仅锚点形态: {}",
            c1.reason
        );
        let c2 = findings.iter().find(|f| f.claim_id == "C2").expect("C2");
        assert!(c2.reason.contains("未通过核验"), "C2 reason: {}", c2.reason);
    }

    #[test]
    fn no_ref_fact_claim_reported_as_such() {
        let records = vec![record("C1", "裸断言。", ClaimKind::FactFromSource, vec![])];
        let findings = collect_findings(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].reason.contains("无任何引用"));
    }

    #[test]
    fn downgraded_claims_not_refound() {
        let mut r = record(
            "C1",
            "无证据句。",
            ClaimKind::FactFromSource,
            vec![VerifyStatus::AnchorNotFound],
        );
        r.downgraded = true;
        assert!(collect_findings(&[r]).is_empty(), "已降级句不重复进清单");
    }

    #[test]
    fn downgrade_marks_text_in_place() {
        let text = "有据句。无据断言句内容较长一些。结尾。";
        let findings = vec![Finding {
            claim_id: "C2".into(),
            sentence_index: 1,
            text: "无据断言句内容较长一些。".into(),
            reason: "x".into(),
        }];
        let out = apply_downgrades(text, &findings);
        assert!(
            out.contains("【未证实】无据断言句内容较长一些。"),
            "out={out}"
        );
        assert!(out.contains("有据句。"));
        assert_eq!(out.matches("【未证实】").count(), 1);
    }
}
