//! 内核报告的 Markdown 渲染：逐句 evidence id + 证据表 + 核验表 + 评分表。
//!
//! 渲染追加到 ai_analysis 尾部（前端 Markdown 渲染器直接可用，无需改模板）。

use crate::evidence_kernel::model::{KernelReport, KernelStatus, Relation, VerifyStatus};

/// 证据 id（E#）= 该 claim 在证据表中的序号（1 起）。
pub fn render_markdown(report: &KernelReport) -> String {
    let mut out = String::new();
    out.push_str("\n\n---\n\n## 证据核验报告（Evidence Kernel v2）\n\n");
    match report.status {
        KernelStatus::InsufficientEvidence => {
            out.push_str(&format!(
                "**状态：证据不足（InsufficientEvidence）——本次分析没有可支撑结论的证据，以下内容不构成新颖性/风险判断。**\n\n原因：{}\n\n",
                report.insufficient_reason
            ));
            out.push_str("内核按硬门规则未产出任何事实断言评分；请先补充检索或上传对比文件后再生成结论性分析。\n");
            return out;
        }
        KernelStatus::Degraded => {
            let downgraded = report.claims.iter().filter(|c| c.downgraded).count();
            let pending = report.critique.len();
            out.push_str(&format!(
                "**状态：降级（Degraded）——{} 条事实断言未通过证据核验（已用【未证实】标注保留原文{}）。**\n\n",
                downgraded + pending,
                if pending > 0 { format!("，另有 {pending} 条待处理") } else { String::new() }
            ));
        }
        KernelStatus::Ok => {
            out.push_str("**状态：通过（Ok）——全部事实断言均有已核验证据支撑。**\n\n");
        }
    }

    // 全局证据编号：逐句清单与核验表使用同一套 E#（按 claims 顺序展开）
    let evidence_ids: Vec<(String, &str)> = report
        .claims
        .iter()
        .flat_map(|c| {
            c.verifications
                .iter()
                .map(|_| c.claim_id.as_str())
                .collect::<Vec<_>>()
        })
        .enumerate()
        .map(|(i, cid)| (format!("E{}", i + 1), cid))
        .collect();
    let ids_for_claim = |cid: &str| -> Vec<String> {
        evidence_ids
            .iter()
            .filter(|(_, c)| *c == cid)
            .map(|(eid, _)| eid.clone())
            .collect()
    };

    // 逐句清单
    out.push_str("### 逐句结论与证据\n\n");
    for c in &report.claims {
        let kind_label = match c.kind {
            crate::evidence_kernel::model::ClaimKind::FactFromSource => "事实",
            crate::evidence_kernel::model::ClaimKind::AnalysisInference => "推论",
            crate::evidence_kernel::model::ClaimKind::LegalArgument => "法律论证",
            crate::evidence_kernel::model::ClaimKind::Meta => "元信息",
        };
        let rel = match c.relation() {
            Relation::Support => "✅ 支撑",
            Relation::Contradict => "⚔️ 矛盾",
            Relation::Insufficient => {
                if c.kind == crate::evidence_kernel::model::ClaimKind::Meta
                    || c.kind == crate::evidence_kernel::model::ClaimKind::AnalysisInference
                {
                    "— 豁免/推论"
                } else if c.downgraded {
                    "⚠️ 未证实（已降级）"
                } else {
                    "❌ 证据不足"
                }
            }
        };
        let ids = ids_for_claim(&c.claim_id);
        out.push_str(&format!(
            "- **{}**（{}，句#{}）{} {}{}\n",
            c.claim_id,
            kind_label,
            c.sentence_index,
            short(&c.text, 80),
            rel,
            if ids.is_empty() {
                String::new()
            } else {
                format!(" — {}", ids.join(", "))
            }
        ));
    }

    // 证据核验表（与逐句清单同一套全局 E# 编号）
    out.push_str(
        "\n### 证据核验表\n\n| 证据 | 断言 | 锚点 | 引文 | 核验结果 |\n|---|---|---|---|---|\n",
    );
    let mut seq = 0usize;
    let mut wrote = false;
    for c in &report.claims {
        for (ev, status) in &c.verifications {
            seq += 1;
            wrote = true;
            out.push_str(&format!(
                "| E{seq} | {} | `{}` | {} | {} |\n",
                c.claim_id,
                ev.raw.replace('|', "\\|"),
                if ev.quote.is_empty() {
                    "—".into()
                } else {
                    short(&ev.quote, 40)
                },
                status_label(status)
            ));
        }
    }
    if !wrote {
        out.push_str("| — | 无证据引用 | — | — | — |\n");
    }

    // 计量与评分
    let m = &report.metrics;
    out.push_str(&format!(
        "\n### 计量\n\n- 事实/法律断言句：{}；逐字通过 {}，近似通过 {}，仅锚点 {}；锚点未命中 {}，引文不符 {}，无法解析 {}\n- 幻觉率（未命中+引文不符)/可解析锚点）：{}\n- grounding（已核验断言占比）：{}\n",
        m.fact_claims,
        m.verified_exact,
        m.verified_approx,
        m.anchor_only,
        m.anchor_not_found,
        m.quote_mismatch,
        m.unparsed_anchor,
        match m.hallucination_rate { Some(v) => format!("{:.0}", v * 100.0) + "%", None => "—（无可解析锚点）".into() },
        match m.grounding { Some(v) => format!("{:.0}", v * 100.0) + "%", None => "—（无事实句）".into() },
    ));

    let r = &report.rubric;
    out.push_str(&format!(
        "\n### 评分（grounding 为乘法门：overall = grounding × 其余四维加权）\n\n| 维度 | 得分 |\n|---|---|\n| grounding 实证度 | {:.2} |\n| coverage 证据覆盖 | {:.2} |\n| legal_reasoning 论证有据 | {:.2} |\n| clarity 结构清晰 | {:.2} |\n| consistency 自洽 | {:.2} |\n| **overall 总分** | **{:.2}** |\n",
        r.grounding, r.coverage, r.legal_reasoning, r.clarity, r.consistency, r.overall
    ));

    if !report.critique.is_empty() {
        out.push_str("\n### 未支撑断言清单（自我批判）\n\n");
        for f in &report.critique {
            out.push_str(&format!(
                "- {}（句#{}）：{} —— {}\n",
                f.claim_id,
                f.sentence_index,
                short(&f.text, 60),
                f.reason
            ));
        }
    }

    out.push_str("\n> 核验语义说明：锚点=存在性精确匹配（doc×类型×编号），引文=归一化逐字/近似匹配；「通过」表示证据真实存在于源文并指向该处，不构成语义支持性论证（v3 方向）。\n");
    out
}

fn status_label(s: &VerifyStatus) -> String {
    match s {
        VerifyStatus::VerifiedExact { section_id } => format!("✅ 逐字命中 {section_id}"),
        VerifyStatus::VerifiedApprox { section_id, score } => {
            format!("🟡 近似命中 {section_id}（{:.2}）", score)
        }
        VerifyStatus::AnchorOnly { section_id } => format!("🔵 仅锚点 {section_id}"),
        VerifyStatus::AnchorNotFound => "❌ 锚点未命中（源文无此编号）".into(),
        VerifyStatus::QuoteMismatch { best_score } => {
            format!("❌ 引文不符（最佳 {score:.2}）", score = best_score)
        }
        VerifyStatus::UnparsedAnchor => "⚠️ 无法解析的引用格式".into(),
    }
}

fn short(s: &str, max: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= max {
        t.replace('\n', " ")
    } else {
        let cut: String = t.chars().take(max).collect();
        format!("{}…", cut.replace('\n', " "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence_kernel::model::*;

    #[test]
    fn insufficient_report_renders_hard_gate() {
        let report = KernelReport {
            status: KernelStatus::InsufficientEvidence,
            claims: vec![],
            metrics: GroundingMetrics::default(),
            rubric: RubricScore::default(),
            critique: vec![],
            repair_rounds: 0,
            insufficient_reason: "检索结果为空".into(),
        };
        let md = render_markdown(&report);
        assert!(md.contains("InsufficientEvidence"), "{md}");
        assert!(md.contains("检索结果为空"));
        assert!(!md.contains("### 评分"), "空证据不渲染评分表: {md}");
    }

    #[test]
    fn ok_report_lists_claims_with_evidence_ids() {
        let report = KernelReport {
            status: KernelStatus::Ok,
            claims: vec![ClaimRecord {
                claim_id: "C1".into(),
                sentence_index: 0,
                text: "D1公开了温控风扇。".into(),
                kind: ClaimKind::FactFromSource,
                downgraded: false,
                verifications: vec![(
                    EvidenceRef {
                        raw: "[D1:第0035段]".into(),
                        anchor: Some(Anchor {
                            doc_id: "D1".into(),
                            kind: LocatorKind::Paragraph,
                            number: 35,
                        }),
                        quote: "温控风扇".into(),
                    },
                    VerifyStatus::VerifiedExact {
                        section_id: "D1:para:35".into(),
                    },
                )],
            }],
            metrics: GroundingMetrics {
                fact_claims: 1,
                verified_exact: 1,
                grounding: Some(1.0),
                ..Default::default()
            },
            rubric: RubricScore {
                grounding: 1.0,
                overall: 0.8,
                ..Default::default()
            },
            critique: vec![],
            repair_rounds: 0,
            insufficient_reason: String::new(),
        };
        let md = render_markdown(&report);
        assert!(md.contains("C1"), "{md}");
        assert!(md.contains("E1"), "证据 id 必须出现: {md}");
        assert!(md.contains("D1:para:35"));
        assert!(md.contains("| **overall 总分** | **0.80** |"));
    }
}
