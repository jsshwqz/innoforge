//! 证据可核验的专利分析内核 v2（Evidence Kernel v2）。
//!
//! 设计文档：`docs/design/evidence-kernel-v2.md`。核心承诺：
//! - 结论可追溯：逐句 claim 绑定 evidence id（ADR-3 句粒度）；
//! - 可反驳：核验是确定性代码（ADR-1，零 AI），锚点三元组精确匹配、引文归一化匹配，
//!   失败即 FAIL（#79：禁止子串包含判据）；
//! - 可评分：五维 rubric，grounding 为乘法门——空证据（I3）与未支撑断言在数值上不可能高分。
//!
//! 垂直切片接缝：`pipeline/steps/finalize.rs`（详见设计文档 §3）。

pub mod anchor;
pub mod classifier;
pub mod critic;
pub mod model;
pub mod report;
pub mod rubric;
pub mod verifier;

use crate::pipeline::context::ReferenceChunk;
use model::{ClaimKind, ClaimRecord, Finding, KernelReport, KernelStatus, SourceCorpus};

/// 决策门阈值：grounding 低于此值时决策建议必须被抑制（设计文档 §3-4，可在 PR 复核调整）。
pub const DECISION_GATE_GROUNDING: f64 = 0.5;

/// 从 pipeline 的 RAG 切片构造证据语料。
/// doc_id 与 deep_reasoning 注入 prompt 的 `[引用N]` 编号严格一致（1 基、同顺序），
/// 因此 AI 写出的 `[引用N]` 可以被可信地还原到源切片（解 EK-P7 的编号漂移）。
pub fn corpus_from_chunks(chunks: &[ReferenceChunk]) -> SourceCorpus {
    SourceCorpus {
        docs: chunks
            .iter()
            .enumerate()
            .map(|(i, c)| model::SourceDoc {
                doc_id: format!("引用{}", i + 1),
                kind: format!("patent_chunk:{}", c.source_type),
                title: c.patent_id.clone(),
                sections: vec![model::SourceSection {
                    section_id: format!("引用{}:chunk:0", i + 1),
                    kind: model::LocatorKind::Chunk,
                    number: 0,
                    text: c.content.clone(),
                }],
            })
            .collect(),
    }
}

/// 运行内核（零修复轮：未支撑句直接降级标注——v1 生产路径，ADR-6）。
/// 返回 (报告, 处理后的文本)。文本可能带 `【未证实】` 标注，调用方应回写。
pub fn run_kernel(text: &str, corpus: &SourceCorpus) -> (KernelReport, String) {
    run_kernel_with_repair(text, corpus, critic::no_repair(), 0)
}

/// 运行内核（可注入修复循环，测试用；生产 v1 用 [`run_kernel`]）。
/// 不变量 I3：空语料 → InsufficientEvidence，不产出事实断言，grounding=0。
pub fn run_kernel_with_repair(
    text: &str,
    corpus: &SourceCorpus,
    repair: critic::RepairFn<'_>,
    max_repair_rounds: u32,
) -> (KernelReport, String) {
    if corpus.is_empty() {
        return (
            KernelReport {
                status: KernelStatus::InsufficientEvidence,
                claims: Vec::new(),
                metrics: rubric::compute_metrics(&[]),
                rubric: model::RubricScore::default(),
                critique: Vec::new(),
                repair_rounds: 0,
                insufficient_reason: "证据语料为空（检索结果为空或未取得全文切片）".to_string(),
            },
            text.to_string(),
        );
    }
    if text.trim().is_empty() {
        return (
            KernelReport {
                status: KernelStatus::InsufficientEvidence,
                claims: Vec::new(),
                metrics: rubric::compute_metrics(&[]),
                rubric: model::RubricScore::default(),
                critique: Vec::new(),
                repair_rounds: 0,
                insufficient_reason: "分析文本为空".to_string(),
            },
            text.to_string(),
        );
    }

    let index = verifier::CorpusIndex::build(corpus);
    let mut current_text = text.to_string();
    let mut rounds = 0u32;

    // 首轮核验
    let mut records = verify_text(&current_text, &index);
    let mut findings = critic::collect_findings(&records);

    while !findings.is_empty() && rounds <= max_repair_rounds {
        current_text = repair(&current_text, &findings);
        rounds += 1;
        records = verify_text(&current_text, &index);
        // 降级标注以【未证实】前缀落在句首；标记后 collect_findings 不再重复报告
        for r in records.iter_mut() {
            if r.text.starts_with("【未证实】") {
                r.downgraded = true;
            }
        }
        findings = critic::collect_findings(&records);
    }

    let metrics = rubric::compute_metrics(&records);
    // 降级过的句（即使修复轮后 findings 已空）也必须保持 Degraded——
    // 「修复完就当 Ok」会把降级史洗白（golden 测试曾抓到这个错）
    let has_downgraded = records.iter().any(|r| r.downgraded);
    let status = if findings.is_empty() && !has_downgraded {
        KernelStatus::Ok
    } else {
        KernelStatus::Degraded
    };
    // 自我批判清单 = 已降级句（保留降级史，可追溯）+ 仍未处理句
    let mut critique: Vec<Finding> = records
        .iter()
        .filter(|r| r.downgraded)
        .map(|r| Finding {
            claim_id: r.claim_id.clone(),
            sentence_index: r.sentence_index,
            text: r.text.clone(),
            reason: "未通过证据核验，已降级标注【未证实】".to_string(),
        })
        .collect();
    critique.extend(findings);
    let rubric = rubric::score(
        &records,
        &metrics,
        index.section_count(),
        status,
        &current_text,
    );
    (
        KernelReport {
            status,
            claims: records,
            metrics,
            rubric,
            critique,
            repair_rounds: rounds,
            insufficient_reason: String::new(),
        },
        current_text,
    )
}

fn verify_text(text: &str, index: &verifier::CorpusIndex) -> Vec<ClaimRecord> {
    classifier::extract_claims(text)
        .into_iter()
        .map(|c| {
            let verifications = c
                .evidence
                .iter()
                .map(|ev| (ev.clone(), verifier::verify_ref(ev, index)))
                .collect::<Vec<_>>();
            ClaimRecord {
                claim_id: c.id,
                sentence_index: c.sentence_index,
                text: c.text,
                kind: c.kind,
                verifications,
                downgraded: false,
            }
        })
        .collect()
}

/// 决策门：内核结果是否要求抑制决策建议（申请/放弃/转向）。
/// 触发条件：空证据硬门，或实证度低于阈值（未支撑的事实断言占比过高，
/// 此时「建议申请/放弃」没有证据地基）。
pub fn should_suppress_decision(report: &KernelReport) -> bool {
    report.status == KernelStatus::InsufficientEvidence
        || report.rubric.grounding < DECISION_GATE_GROUNDING
}

/// 报告是否包含事实断言（供渲染层区分「证据不足」与「正常无事实句」）。
pub fn has_factual_claims(report: &KernelReport) -> bool {
    report
        .claims
        .iter()
        .any(|c| c.kind == ClaimKind::FactFromSource || c.kind == ClaimKind::LegalArgument)
}
