//! 公理 6：先锁专利申请日、后公开论文/开源时序守卫与商业秘密脱敏裁剪（`src/core/evidence_rebuttal/priority_guard.rs`）

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

use crate::core::vault::models::{AssetRole, DeliverableGenre, FilingStatus, PriorityCheckResult};

const REDACTED_PLACEHOLDER: &str = "[商业秘密脱敏保留 / Know-how Redacted]";

fn is_public_disclosure_genre(genre: DeliverableGenre) -> bool {
    matches!(
        genre,
        DeliverableGenre::AcademicPaper | DeliverableGenre::PaperRebuttalPackage
    )
}

/// 检查关联自有方案的专利递交状态，并在导出文本中对商业秘密敏感词条执行自动脱敏裁剪
pub fn check_disclosure_priority(
    conn: &Connection,
    related_doc_ids: &[String],
    target_genre: DeliverableGenre,
    knowhow_markers: &[String],
    draft_text: &str,
) -> Result<(PriorityCheckResult, String)> {
    let mut conflicting_doc_ids = Vec::new();

    for doc_id in related_doc_ids {
        let row: Option<(String, String)> = conn
            .query_row(
                "SELECT asset_role, filing_status
                 FROM knowledge_docs
                 WHERE doc_id = ?1
                 LIMIT 1",
                params![doc_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;

        if let Some((role_str, status_str)) = row {
            let role = AssetRole::parse(&role_str);
            let status = FilingStatus::parse(&status_str);
            if role == AssetRole::OwnAsset && status == FilingStatus::UnfiledSecret {
                conflicting_doc_ids.push(doc_id.clone());
            }
        }
    }

    // 商业秘密参数脱敏裁剪
    let mut redacted_text = draft_text.to_string();
    let mut stripped_knowhow_fields = Vec::new();
    for marker in knowhow_markers {
        let trimmed = marker.trim();
        if trimmed.is_empty() {
            continue;
        }
        if redacted_text.contains(trimmed) {
            redacted_text = redacted_text.replace(trimmed, REDACTED_PLACEHOLDER);
            stripped_knowhow_fields.push(trimmed.to_string());
        }
    }

    let is_public = is_public_disclosure_genre(target_genre);
    let (lamp, has_conflict, advisory_zh, advisory_en) = if is_public
        && !conflicting_doc_ids.is_empty()
    {
        (
            "red".to_string(),
            true,
            format!(
                "【公开时序高危阻断】关联我方自有技术方案（{}）仍处于「未递交专利保密（unfiled_secret）」状态，若此时导出公开发表体裁将导致现有技术自毁新颖性！请先完成专利申请日锁定（patent_filed）。",
                conflicting_doc_ids.join(", ")
            ),
            format!(
                "[Priority Conflict Blocked] Own technical assets ({}) are still in 'unfiled_secret' status. Publicly disclosing them via {:?} will destroy patent novelty! Lock a patent filing date first.",
                conflicting_doc_ids.join(", "),
                target_genre
            ),
        )
    } else if !conflicting_doc_ids.is_empty() {
        (
            "yellow".to_string(),
            false,
            format!(
                "【内部保密提醒】关联我方自有方案（{}）尚未完成专利递交，当前体裁仅限内部研发评审，严禁对外公开传播。",
                conflicting_doc_ids.join(", ")
            ),
            format!(
                "[Internal Confidentiality Notice] Related own assets ({}) are not yet patent-filed; restrict distribution to internal review only.",
                conflicting_doc_ids.join(", ")
            ),
        )
    } else {
        (
            "green".to_string(),
            false,
            "公开时序合规：关联自有技术资产已锁定专利申请日或已合法公开。".to_string(),
            "Priority check passed: related own assets have a locked patent filing date or are already disclosed.".to_string(),
        )
    };

    Ok((
        PriorityCheckResult {
            lamp,
            has_conflict,
            conflicting_doc_ids,
            advisory_zh,
            advisory_en,
            stripped_knowhow_fields,
        },
        redacted_text,
    ))
}
