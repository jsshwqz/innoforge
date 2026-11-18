//! 五维量化评分与硬阻断总闸门（`src/core/evidence_rebuttal/quality_gate.rs` · 根治 `ERR-02`）
//!
//! 门禁裁决铁律：
//! 1. 存在第 33 条修改超范围、论文返修数据失配、涉密禁词、或公开时序红灯冲突 → `GateDecision::HardBlocked`
//! 2. 存在任何 `CitationVerdict::Unverified`、`Summary`、跨实施例拼凑预警、或对比事实零锚点 → `GateDecision::DowngradeNeedsEvidence`
//! 3. 仅当五维全过且全部关键引证均为 `Verified` 时 → `GateDecision::PassSubmittable`

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::core::vault::models::{
    CitationVerdict, FiveDimGateReport, GateDecision, PriorityCheckResult, StitchingAlert,
};

use super::anchor_verifier;
use super::legal_academic_guard::ScopeConsistencyReport;

/// 五维门禁评估入参
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GateEvaluationInput {
    pub draft_text: String,
    pub default_doc_id: Option<String>,
    /// 若为 true，草稿中未包含任何已核验锚点时强制降级为 `DowngradeNeedsEvidence`
    pub require_citations: bool,
    pub stitching_alerts: Vec<StitchingAlert>,
    pub scope_report: Option<ScopeConsistencyReport>,
    pub priority_check: Option<PriorityCheckResult>,
}

fn contains_comparative_claim(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    text.contains("对比")
        || text.contains("现有技术")
        || text.contains("相比")
        || text.contains("相对于")
        || lower.contains("prior art")
        || lower.contains("compared with")
}

/// 汇总五维得分并执行硬阻断门禁裁决
pub fn evaluate_submission_gate(
    conn: &Connection,
    input: &GateEvaluationInput,
) -> Result<FiveDimGateReport> {
    let verified_citations = anchor_verifier::verify_all_citations(
        conn,
        &input.draft_text,
        input.default_doc_id.as_deref(),
    )?;

    let total_cites = verified_citations.len();
    let verified_count = verified_citations
        .iter()
        .filter(|c| c.verdict == CitationVerdict::Verified)
        .count();
    let summary_count = verified_citations
        .iter()
        .filter(|c| c.verdict == CitationVerdict::Summary)
        .count();
    let unverified_count = verified_citations
        .iter()
        .filter(|c| c.verdict == CitationVerdict::Unverified)
        .count();

    let mut blocking_reasons = Vec::new();
    let mut downgrade_warnings = Vec::new();

    // 维度 1：引证锚点真实率
    let needs_cite_guard = input.require_citations || contains_comparative_claim(&input.draft_text);
    let anchor_accuracy_score = if total_cites == 0 {
        if needs_cite_guard {
            downgrade_warnings.push(
                "正文中包含对比/事实断言但零可核验原文/DOI/实验锚点，强制降级为「待人工补证」"
                    .to_string(),
            );
            40.0
        } else {
            100.0
        }
    } else {
        let weighted = (verified_count as f64) * 100.0 + (summary_count as f64) * 60.0;
        (weighted / (total_cites as f64)).clamp(0.0, 100.0)
    };

    for cite in &verified_citations {
        match cite.verdict {
            CitationVerdict::Unverified => {
                downgrade_warnings.push(format!(
                    "待人工补证：引证 {} 未通过核验 — {}",
                    cite.raw_mention, cite.reason
                ));
            }
            CitationVerdict::Summary => {
                downgrade_warnings.push(format!(
                    "待人工补证：引证 {} 仅命中摘要级证据（缺少全文切片）",
                    cite.raw_mention
                ));
            }
            CitationVerdict::Verified => {}
        }
    }

    // 维度 2：第 33 条 / 论文返修一致性合规度
    let scope_compliance_score = match &input.scope_report {
        Some(rep) if !rep.passed => {
            for v in &rep.violations {
                blocking_reasons.push(v.clone());
            }
            (100.0 - (rep.violations.len() as f64) * 35.0).max(0.0)
        }
        Some(_) | None => 100.0,
    };

    // 维度 3：逻辑链完整度
    let logic_completeness_score = if input.draft_text.trim().is_empty() {
        downgrade_warnings.push("草稿正文为空，缺少可核验技术论证逻辑链".to_string());
        0.0
    } else if unverified_count > 0 {
        (100.0 - (unverified_count as f64) * 20.0).max(20.0)
    } else {
        100.0
    };

    // 维度 4：实施例自洽度
    let embodiment_consistency_score = if input.stitching_alerts.is_empty() {
        100.0
    } else {
        for alert in &input.stitching_alerts {
            downgrade_warnings.push(format!(
                "跨实施例拼凑预警（{}）：特征 {:?} 分属不同实施例 {}",
                alert.doc_id,
                alert.cited_features,
                alert.conflicting_embodiment_groups.join(" vs ")
            ));
        }
        (100.0 - (input.stitching_alerts.len() as f64) * 40.0).max(0.0)
    };

    // 维度 5：公开时序与隐私合规度
    let priority_privacy_score = match &input.priority_check {
        Some(p) => {
            if p.has_conflict || p.lamp == "red" {
                blocking_reasons.push(p.advisory_zh.clone());
                0.0
            } else if p.lamp == "yellow" {
                80.0
            } else {
                100.0
            }
        }
        None => 100.0,
    };

    let overall_score = (anchor_accuracy_score
        + scope_compliance_score
        + logic_completeness_score
        + embodiment_consistency_score
        + priority_privacy_score)
        / 5.0;

    let decision = if !blocking_reasons.is_empty() {
        GateDecision::HardBlocked
    } else if !downgrade_warnings.is_empty() {
        GateDecision::DowngradeNeedsEvidence
    } else {
        GateDecision::PassSubmittable
    };

    Ok(FiveDimGateReport {
        decision,
        anchor_accuracy_score,
        scope_compliance_score,
        logic_completeness_score,
        embodiment_consistency_score,
        priority_privacy_score,
        overall_score,
        blocking_reasons,
        downgrade_warnings,
        verified_citations,
    })
}
