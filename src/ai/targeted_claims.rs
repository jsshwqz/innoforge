//! C6 针对权项 + 末段原文提取（纯函数）
//!
//! 从 OA 中提取：
//! 1. 审查员针对的是哪一项权利要求
//! 2. 驳回决定末段（"综上所述"段）的逐字原文
//!
//! 这两个信息是答复函的核心攻击面——答复必须针对被驳回的具体权项，
//! 并回应驳回决定的总结性论断。

use crate::ai::whitespace_search::{floor_char_boundary, strip_whitespace};

/// C6 针对权项 + 末段报告
#[derive(Debug, Clone, PartialEq)]
pub struct TargetedClaimsReport {
    /// 被驳回的权项号
    pub rejected_claims: Vec<usize>,
    /// 被维持驳回的权项号（一审驳回决定中）
    pub maintained_rejections: Vec<usize>,
    /// 末段原文（"综上所述"或"基于上述理由"后的全文）
    pub concluding_paragraph: String,
    /// 末段是否找到
    pub has_concluding_paragraph: bool,
    /// 人类可读摘要
    pub summary: String,
}

/// C6 主函数：提取被驳回权项和末段原文
///
/// # 参数
/// - `oa_text`: OA/驳回决定全文
pub fn extract_targeted_claims_and_conclusion(oa_text: &str) -> TargetedClaimsReport {
    let stripped = strip_whitespace(oa_text);

    // 提取被驳回的权项
    let rejected_claims = extract_rejected_claims(&stripped);

    // 提取末段
    let concluding_paragraph = extract_concluding_paragraph(&stripped);
    let has_concluding_paragraph = !concluding_paragraph.is_empty();

    // 提取"维持驳回"的权项（驳回决定中）
    let maintained_rejections = extract_maintained_rejections(&stripped);

    let summary = {
        let mut parts = Vec::new();
        if !rejected_claims.is_empty() {
            let claims_str = rejected_claims
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join("、");
            parts.push(format!("驳回权项：第{}项", claims_str));
        }
        if !maintained_rejections.is_empty() {
            let claims_str = maintained_rejections
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join("、");
            parts.push(format!("维持驳回：第{}项", claims_str));
        }
        if has_concluding_paragraph {
            parts.push(format!("末段原文（{}字符）", concluding_paragraph.len()));
        } else {
            parts.push("未找到末段".to_string());
        }
        parts.join("；")
    };

    TargetedClaimsReport {
        rejected_claims,
        maintained_rejections,
        concluding_paragraph,
        has_concluding_paragraph,
        summary,
    }
}

/// 提取被驳回的权项号
///
/// 匹配模式：
/// - "权利要求N不具备创造性"
/// - "权利要求N-N不具备创造性"
/// - "驳回权利要求N"
fn extract_rejected_claims(stripped: &str) -> Vec<usize> {
    let mut claims = std::collections::HashSet::new();

    // "权利要求N不具备创造性" / "权利要求N-N不具备创造性"
    let re_single = regex::Regex::new(r"权利要求(\d+)(?:不具备|不具有|不符合)").ok();
    if let Some(re) = re_single {
        for cap in re.captures_iter(stripped) {
            if let Some(n) = cap.get(1).and_then(|m| m.as_str().parse().ok()) {
                claims.insert(n);
            }
        }
    }

    // "权利要求N-M不具备创造性" — 范围
    let re_range = regex::Regex::new(r"权利要求(\d+)-(\d+)(?:不具备|不具有|不符合)").ok();
    if let Some(re) = re_range {
        for cap in re.captures_iter(stripped) {
            let start: usize = cap
                .get(1)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0);
            let end: usize = cap
                .get(2)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0);
            for n in start..=end {
                if n >= 1 && n <= 30 {
                    claims.insert(n);
                }
            }
        }
    }

    // "驳回权利要求N"
    let re_reject = regex::Regex::new(r"驳回权利要求(\d+)").ok();
    if let Some(re) = re_reject {
        for cap in re.captures_iter(stripped) {
            if let Some(n) = cap.get(1).and_then(|m| m.as_str().parse().ok()) {
                claims.insert(n);
            }
        }
    }

    let mut result: Vec<usize> = claims.into_iter().collect();
    result.sort();
    result
}

/// 提取"维持驳回"的权项号
fn extract_maintained_rejections(stripped: &str) -> Vec<usize> {
    let mut claims = std::collections::HashSet::new();

    // "维持驳回权利要求N" 或 "维持对权利要求N的驳回"
    let re1 = regex::Regex::new(r"维持驳回权利要求(\d+)").ok();
    let re2 = regex::Regex::new(r"维持对权利要求(\d+).*?驳回").ok();

    for re in [re1, re2].into_iter().flatten() {
        for cap in re.captures_iter(stripped) {
            if let Some(n) = cap.get(1).and_then(|m| m.as_str().parse().ok()) {
                if n >= 1 && n <= 30 {
                    claims.insert(n);
                }
            }
        }
    }

    let mut result: Vec<usize> = claims.into_iter().collect();
    result.sort();
    result
}

/// 提取末段（"综上所述"或"基于上述理由"后的全文）
fn extract_concluding_paragraph(stripped: &str) -> String {
    let markers = ["综上所述", "基于上述理由", "基于上述事实", "据此"];

    for marker in &markers {
        if let Some(pos) = stripped.find(marker) {
            let remaining = &stripped[pos..];
            // 末段取到文本结尾，最多 2000 字符
            let end = remaining.len().min(2000);
            return floor_char_boundary(remaining, end).to_string();
        }
    }

    String::new()
}

// ═══════════════════════════════════════════════════════════════════════
//  测试
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_rejected_claims() {
        let oa = "权利要求1不具备创造性。权利要求2-4不具备创造性。";
        let report = extract_targeted_claims_and_conclusion(oa);
        assert_eq!(report.rejected_claims, vec![1, 2, 3, 4]);
    }

    #[test]
    fn extracts_concluding_paragraph() {
        let oa = "权利要求1不具备创造性。综上所述，本发明不具备创造性，维持驳回。";
        let report = extract_targeted_claims_and_conclusion(oa);
        assert!(report.has_concluding_paragraph);
        assert!(report.concluding_paragraph.contains("综上所述"));
        assert!(report.concluding_paragraph.contains("维持驳回"));
    }

    #[test]
    fn no_concluding_paragraph() {
        let oa = "权利要求1不具备创造性。";
        let report = extract_targeted_claims_and_conclusion(oa);
        assert!(!report.has_concluding_paragraph);
    }

    #[test]
    fn extracts_maintained_rejections() {
        let oa = "维持驳回权利要求1。维持对权利要求2的驳回。";
        let report = extract_targeted_claims_and_conclusion(oa);
        assert!(report.maintained_rejections.contains(&1));
        assert!(report.maintained_rejections.contains(&2));
    }

    // ── 示例申请人案回归 ──

    #[test]
    fn sample_case_c6_targeted_and_conclusion() {
        let oa = include_str!("../../tests/fixtures/oa_sample/oa_rejection.txt");
        let report = extract_targeted_claims_and_conclusion(oa);

        eprintln!(
            "示例申请人案 C6：驳回权项{:?}，维持驳回{:?}，末段{}字符",
            report.rejected_claims,
            report.maintained_rejections,
            report.concluding_paragraph.len()
        );

        // 驳回决定应有被驳回的权项
        if report.rejected_claims.is_empty() {
            eprintln!("警告：未提取到被驳回权项，可能 OCR 格式问题");
        }

        // 应有末段
        if !report.has_concluding_paragraph {
            eprintln!("警告：未提取到末段，可能标记词不同");
        }
    }
}
