//! C9 区别特征覆盖检测（纯函数）
//!
//! 检查"区别特征清单"是否覆盖了递交的全部权利要求。
//! 审查员可能只评述了权1，遗漏了从属权项——这构成漏项。

use crate::ai::whitespace_search::{count_term, strip_whitespace};

/// C9 权项覆盖报告
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimCoverageReport {
    /// 递交的权项总数
    pub submitted_claim_count: usize,
    /// 审查员评述中涉及的权项号
    pub evaluated_claims: Vec<usize>,
    /// 未被评述的权项号
    pub unevaluated_claims: Vec<usize>,
    /// 是否覆盖完整
    pub is_complete: bool,
    /// 人类可读摘要
    pub summary: String,
}

/// C9 主函数：检查权项覆盖
///
/// # 参数
/// - `oa_text`: OA/驳回决定全文
/// - `submitted_claim_count`: 递交的权项总数
pub fn check_claim_coverage(oa_text: &str, submitted_claim_count: usize) -> ClaimCoverageReport {
    let stripped = strip_whitespace(oa_text);

    // 从 OA 中提取所有被评述的权项号
    let evaluated_claims = extract_evaluated_claims(&stripped);

    // 找出未被评述的权项
    let unevaluated_claims: Vec<usize> = (1..=submitted_claim_count)
        .filter(|n| !evaluated_claims.contains(n))
        .collect();

    let is_complete = unevaluated_claims.is_empty();

    let summary = if is_complete {
        format!("审查员评述覆盖了全部{}项权利要求。", submitted_claim_count)
    } else {
        format!(
            "审查员评述遗漏了{}项权利要求：第{}项",
            unevaluated_claims.len(),
            unevaluated_claims
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join("、")
        )
    };

    ClaimCoverageReport {
        submitted_claim_count,
        evaluated_claims,
        unevaluated_claims,
        is_complete,
        summary,
    }
}

/// 从 OA 文本中提取所有被评述的权项号
fn extract_evaluated_claims(stripped: &str) -> Vec<usize> {
    let mut claims = std::collections::HashSet::new();

    // 匹配 "权利要求N"
    if let Ok(re) = regex::Regex::new(r"权利要求(\d+)") {
        for cap in re.captures_iter(stripped) {
            if let Some(n) = cap.get(1).and_then(|m| m.as_str().parse().ok()) {
                if (1..=30).contains(&n) {
                    claims.insert(n);
                }
            }
        }
    }

    // 匹配 "权N" 简写
    if let Ok(re) = regex::Regex::new(r"权(\d+)") {
        for cap in re.captures_iter(stripped) {
            if let Some(n) = cap.get(1).and_then(|m| m.as_str().parse().ok()) {
                if (1..=30).contains(&n) {
                    claims.insert(n);
                }
            }
        }
    }

    let mut result: Vec<usize> = claims.into_iter().collect();
    result.sort();
    result
}

/// C9 附加功能：检查区别特征清单是否覆盖了递交权项中的结构特征
///
/// # 参数
/// - `oa_text`: OA 全文
/// - `submitted_claims`: 递交的权项文本
/// - `structural_features`: 关键结构特征列表
pub fn check_feature_coverage(
    oa_text: &str,
    submitted_claims: &str,
    structural_features: &[&str],
) -> FeatureCoverageReport {
    let oa_stripped = strip_whitespace(oa_text);
    let claims_stripped = strip_whitespace(submitted_claims);

    let mut covered = Vec::new();
    let mut uncovered = Vec::new();

    for &feature in structural_features {
        let in_claims = count_term(&claims_stripped, feature) > 0;
        let in_oa = count_term(&oa_stripped, feature) > 0;

        if in_claims && !in_oa {
            // 递交权项有此特征，但 OA 未评述
            uncovered.push(feature.to_string());
        } else if in_claims && in_oa {
            covered.push(feature.to_string());
        }
    }

    let summary = if uncovered.is_empty() {
        "区别特征清单覆盖了递交权项的全部结构特征。".to_string()
    } else {
        format!(
            "区别特征清单遗漏了{}个结构特征：{}",
            uncovered.len(),
            uncovered.join("、")
        )
    };

    FeatureCoverageReport {
        covered_features: covered,
        uncovered_features: uncovered,
        summary,
    }
}

/// C9 结构特征覆盖报告
#[derive(Debug, Clone, PartialEq)]
pub struct FeatureCoverageReport {
    /// 已覆盖的结构特征
    pub covered_features: Vec<String>,
    /// 未覆盖的结构特征
    pub uncovered_features: Vec<String>,
    /// 人类可读摘要
    pub summary: String,
}

// ═══════════════════════════════════════════════════════════════════════
//  测试
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_coverage() {
        let oa =
            "权利要求1不具备创造性。权利要求2是常规设置。权利要求3容易想到。权利要求4是惯用手段。";
        let report = check_claim_coverage(oa, 4);
        assert!(report.is_complete);
        assert!(report.summary.contains("覆盖了全部"));
    }

    #[test]
    fn incomplete_coverage() {
        let oa = "权利要求1不具备创造性。权利要求2是常规设置。";
        let report = check_claim_coverage(oa, 6);
        assert!(!report.is_complete);
        assert_eq!(report.unevaluated_claims, vec![3, 4, 5, 6]);
        assert!(report.summary.contains("遗漏"));
    }

    #[test]
    fn feature_coverage_detects_uncovered() {
        let oa = "区别在于对角安装";
        let claims = "对角安装的网状电极，把手，控制阀，弹簧，定位杆，滑块";
        let report = check_feature_coverage(
            oa,
            claims,
            &["对角", "网状", "把手", "控制阀", "弹簧", "定位杆", "滑块"],
        );
        // "把手"等在递交权项中有，但 OA 未评述
        assert!(!report.uncovered_features.is_empty());
        assert!(report.summary.contains("遗漏"));
    }

    // ── 示例申请人案回归 ──

    #[test]
    fn sample_case_c9_claim_coverage() {
        let oa = include_str!("../../tests/fixtures/oa_sample/oa_rejection.txt");
        // 示例申请人案递交了 6 项权利要求
        let report = check_claim_coverage(oa, 6);

        eprintln!(
            "示例申请人案 C9：评述权项{:?}，遗漏权项{:?}",
            report.evaluated_claims, report.unevaluated_claims
        );

        // 驳回决定应评述了至少权1
        assert!(report.evaluated_claims.contains(&1), "应评述了权利要求1");
    }

    #[test]
    fn sample_case_c9_feature_coverage() {
        let oa = include_str!("../../tests/fixtures/oa_sample/oa_rejection.txt");
        let claims = include_str!("../../tests/fixtures/oa_sample/submitted_claims.txt");
        let report = check_feature_coverage(
            oa,
            claims,
            &[
                "对角",
                "网状",
                "把手",
                "控制阀",
                "陶瓷",
                "弹簧",
                "定位杆",
                "滑块",
            ],
        );

        eprintln!(
            "示例申请人案 C9 特征覆盖：已覆盖{:?}，未覆盖{:?}",
            report.covered_features, report.uncovered_features
        );

        // "对角" 应在 OA 和递交权项中都存在
        assert!(
            report.covered_features.contains(&"对角".to_string()),
            "'对角'应被覆盖"
        );
    }
}
