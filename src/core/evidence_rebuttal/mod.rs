//! 三栖证据核验、跨实施例识破、时序守卫与五维硬阻断总闸门内核（`src/core/evidence_rebuttal/`）
//!
//! 严格对应 `docs/plans/2026-11-18-v1-overhaul-plan.md` §3.2.4 与 `T-204`：
//! - [`anchor_verifier`]：专利段落/权项、论文真实 DOI/图表、对照实验 O(1) 原文回查
//! - [`embodiment_guard`]：跨实施例强行拼凑特征确定性识破
//! - [`legal_academic_guard`]：专利法第 33 条修改超范围 + 论文返修一致性 + 涉密禁词守卫
//! - [`priority_guard`]：先锁专利申请日、后公开论文/开源时序守卫与商业秘密脱敏裁剪
//! - [`quality_gate`]：五维量化评分与硬阻断总闸门（根治 `ERR-02`）

pub mod anchor_verifier;
pub mod embodiment_guard;
pub mod legal_academic_guard;
pub mod priority_guard;
pub mod quality_gate;

use anyhow::Result;
use rusqlite::Connection;

use crate::core::vault::models::{
    DeliverableGenre, FiveDimGateReport, PriorityCheckResult, StitchingAlert, VerifiedCitation,
};

pub use legal_academic_guard::ScopeConsistencyReport;
pub use quality_gate::GateEvaluationInput;

/// 统一证据核验与三栖抗辩共享内核服务（`EvidenceRebuttalService`）
pub struct EvidenceRebuttalService;

impl EvidenceRebuttalService {
    /// 扫描任意草稿中的四类引证锚点并执行 O(1) SQLite 回查
    pub fn verify_all_citations(
        conn: &Connection,
        draft_text: &str,
        default_doc_id: Option<&str>,
    ) -> Result<Vec<VerifiedCitation>> {
        anchor_verifier::verify_all_citations(conn, draft_text, default_doc_id)
    }

    /// 识破对比文献中的跨实施例强行拼凑组合
    pub fn detect_cross_embodiment_stitching(
        conn: &Connection,
        doc_id: &str,
        cited_anchors_or_numerals: &[String],
    ) -> Result<Option<StitchingAlert>> {
        embodiment_guard::detect_cross_embodiment_stitching(conn, doc_id, cited_anchors_or_numerals)
    }

    /// 专利模式：第 33 条修改超范围与涉密禁词核查
    pub fn check_patent_art33_scope(
        conn: &Connection,
        own_doc_id: &str,
        original_claim: &str,
        amended_claim: &str,
        draft_rebuttal: &str,
    ) -> Result<ScopeConsistencyReport> {
        legal_academic_guard::check_patent_art33_scope(
            conn,
            own_doc_id,
            original_claim,
            amended_claim,
            draft_rebuttal,
        )
    }

    /// 学术论文 Rebuttal 模式：返修回复信与修订稿/实验数据一致性核查
    pub fn check_paper_rebuttal_consistency(
        conn: &Connection,
        manuscript_doc_id: &str,
        response_letter: &str,
    ) -> Result<ScopeConsistencyReport> {
        legal_academic_guard::check_paper_rebuttal_consistency(
            conn,
            manuscript_doc_id,
            response_letter,
        )
    }

    /// 公理 6：先锁专利申请日、后公开论文/开源时序冲突检测与商业秘密脱敏裁剪
    pub fn check_disclosure_priority(
        conn: &Connection,
        related_doc_ids: &[String],
        target_genre: DeliverableGenre,
        knowhow_markers: &[String],
        draft_text: &str,
    ) -> Result<(PriorityCheckResult, String)> {
        priority_guard::check_disclosure_priority(
            conn,
            related_doc_ids,
            target_genre,
            knowhow_markers,
            draft_text,
        )
    }

    /// 五维质量与合规硬阻断总闸门裁决
    pub fn evaluate_submission_gate(
        conn: &Connection,
        input: &GateEvaluationInput,
    ) -> Result<FiveDimGateReport> {
        quality_gate::evaluate_submission_gate(conn, input)
    }
}
