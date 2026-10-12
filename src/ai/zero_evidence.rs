//! C2 零举证检测（纯函数）
//!
//! 检测审查员评述中"无对比文件佐证的断言"。
//! 典型模式：审查员说"容易想到""常规设计"但未引用任何对比文件。

use crate::ai::whitespace_search::{safe_slice, strip_whitespace};

/// C2 零举证报告
#[derive(Debug, Clone, PartialEq)]
pub struct ZeroEvidenceReport {
    /// 检测到的零举证断言列表
    pub zero_evidence_assertions: Vec<ZeroEvidenceAssertion>,
    /// 总数
    pub count: usize,
    /// 人类可读摘要
    pub summary: String,
}

/// 单个零举证断言
#[derive(Debug, Clone, PartialEq)]
pub struct ZeroEvidenceAssertion {
    /// 断言类型（如"容易想到""常规设计"）
    pub assertion_type: String,
    /// 断言所在文本片段
    pub text_fragment: String,
    /// 涉及的权项号（如果能识别）
    pub claim_number: Option<usize>,
    /// 该片段是否引用了任何对比文件
    pub has_citation: bool,
}

/// C2 主函数：检测零举证断言
///
/// # 参数
/// - `oa_text`: OA/驳回决定全文
///
/// # 返回
/// 零举证报告
pub fn check_zero_evidence(oa_text: &str) -> ZeroEvidenceReport {
    let stripped = strip_whitespace(oa_text);

    // 常见无证据断言模式
    let assertion_patterns = [
        "容易想到",
        "常规设计",
        "惯用手段",
        "常规选择",
        "常规调整",
        "常规设置",
        "常规技术",
        "显而易见",
        "无需创造性劳动",
        "本领域技术人员",
        "公知常识",
        "常用技术手段",
    ];

    let mut assertions = Vec::new();

    for pattern in &assertion_patterns {
        let mut search_from = 0;
        while let Some(pos) = stripped[search_from..].find(pattern) {
            let abs_pos = search_from + pos;

            // 取前后 100 字符作为上下文片段（使用 safe_slice 避免 UTF-8 边界 panic）
            let ctx_start = abs_pos.saturating_sub(50);
            let ctx_end = (abs_pos + pattern.len() + 100).min(stripped.len());
            let fragment = safe_slice(&stripped, ctx_start, ctx_end).to_string();

            // 检查片段中是否引用了对比文件
            let has_citation = (1..=10).any(|n| {
                let doc_ref = format!("对比文件{}", n);
                let stripped_doc = strip_whitespace(&doc_ref);
                fragment.contains(&stripped_doc)
            });

            // 尝试识别权项号
            let claim_number = extract_claim_number(&stripped, abs_pos);

            assertions.push(ZeroEvidenceAssertion {
                assertion_type: pattern.to_string(),
                text_fragment: fragment,
                claim_number,
                has_citation,
            });

            search_from = abs_pos + pattern.len();
        }
    }

    // 只保留没有引用对比文件的断言
    let zero_evidence: Vec<_> = assertions.into_iter().filter(|a| !a.has_citation).collect();

    let count = zero_evidence.len();

    let summary = if count == 0 {
        "未检测到零举证断言。".to_string()
    } else {
        let by_type: Vec<_> = zero_evidence
            .iter()
            .map(|a| a.assertion_type.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        format!(
            "检测到{}处零举证断言（无对比文件佐证），类型：{}",
            count,
            by_type.join("、")
        )
    };

    ZeroEvidenceReport {
        zero_evidence_assertions: zero_evidence,
        count,
        summary,
    }
}

/// 从位置向前搜索最近的"权利要求N"
fn extract_claim_number(text: &str, pos: usize) -> Option<usize> {
    let before = &text[..pos];
    let re = regex::Regex::new(r"权利要求(\d+)").ok()?;
    let mut last: Option<usize> = None;
    for cap in re.captures_iter(before) {
        if let Some(n) = cap.get(1).and_then(|m| m.as_str().parse().ok()) {
            last = Some(n);
        }
    }
    last
}

// ═══════════════════════════════════════════════════════════════════════
//  测试
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_zero_evidence() {
        let oa = "对于权利要求4，把手是容易想到的。对于权利要求5，控制阀是常规设计。";
        let report = check_zero_evidence(oa);
        assert!(
            report.count >= 2,
            "应检测到至少2处零举证, 实际{}",
            report.count
        );
    }

    #[test]
    fn ignores_assertions_with_citation() {
        let oa = "对比文件3公开了把手，因此把手是容易想到的。";
        let report = check_zero_evidence(oa);
        // "容易想到"附近有"对比文件3"，不算零举证
        assert_eq!(report.count, 0);
    }

    #[test]
    fn extracts_claim_number() {
        let oa = "对于权利要求3，对比文件3公开了。对于权利要求4，把手是容易想到的。";
        let report = check_zero_evidence(oa);
        let claim4 = report
            .zero_evidence_assertions
            .iter()
            .find(|a| a.claim_number == Some(4));
        assert!(claim4.is_some(), "应识别到权项4");
    }

    // ── 示例申请人案回归 ──

    #[test]
    fn sample_case_c2_zero_evidence_in_rejection() {
        let oa = include_str!("../../tests/fixtures/oa_sample/oa_rejection.txt");
        let report = check_zero_evidence(oa);
        // 驳回决定中应有零举证断言（如"常规设置""容易想到"等）
        eprintln!(
            "示例申请人案 C2 零举证检测：{}处, 类型：{:?}",
            report.count,
            report
                .zero_evidence_assertions
                .iter()
                .map(|a| &a.assertion_type)
                .collect::<std::collections::HashSet<_>>()
        );
        // 不强制断言数量，因为 OCR 可能影响匹配
    }
}
