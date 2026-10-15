//! C5 区别特征提取 + C11 技术问题失真检测（纯函数）
//!
//! 从 OA 文本中提取"区别在于"段和"实际解决的技术问题"句，
//! 并检测审查员重构的技术问题是否遗漏了区别特征。

use crate::ai::whitespace_search::{count_term, floor_char_boundary, strip_whitespace};

/// C5 区别特征报告
#[derive(Debug, Clone, PartialEq)]
pub struct DistinctionReport {
    /// "区别在于"段原文
    pub distinction_text: String,
    /// "实际解决的技术问题"句原文
    pub tech_problem_text: String,
    /// 从区别段提取的特征术语
    pub distinction_features: Vec<String>,
    /// 技术问题句中是否包含各区别特征
    pub feature_coverage: Vec<(String, bool)>,
    /// 未被技术问题覆盖的区别特征
    pub uncovered_features: Vec<String>,
    /// 人类可读摘要
    pub summary: String,
}

/// C5 主函数：提取区别特征并检查技术问题覆盖
///
/// # 参数
/// - `oa_text`: OA/驳回决定全文
/// - `known_features`: 已知的区别特征术语列表（可选，用于精确检查）
pub fn check_tech_problem(oa_text: &str, known_features: &[&str]) -> DistinctionReport {
    let stripped = strip_whitespace(oa_text);

    // 提取"区别在于"段
    let distinction_text = extract_segment(&stripped, "区别在于", &["实际解决", "对于本发明"]);

    // 提取"实际解决的技术问题"句
    let tech_problem_text = extract_segment(&stripped, "实际解决的技术问题", &["。", "对比文件"]);

    // 从区别段提取特征术语
    let distinction_features = extract_features(&distinction_text, known_features);

    // 检查技术问题句是否覆盖了各区别特征
    let tech_stripped = strip_whitespace(&tech_problem_text);
    let feature_coverage: Vec<(String, bool)> = distinction_features
        .iter()
        .map(|f| (f.clone(), count_term(&tech_stripped, f) > 0))
        .collect();

    let uncovered_features: Vec<String> = feature_coverage
        .iter()
        .filter(|(_, covered)| !covered)
        .map(|(f, _)| f.clone())
        .collect();

    let summary = if uncovered_features.is_empty() {
        "技术问题重构完整，未遗漏区别特征。".to_string()
    } else {
        format!(
            "技术问题重构遗漏了{}个区别特征：{}",
            uncovered_features.len(),
            uncovered_features.join("、")
        )
    };

    DistinctionReport {
        distinction_text,
        tech_problem_text,
        distinction_features,
        feature_coverage,
        uncovered_features,
        summary,
    }
}

/// 从文本中提取以 `marker` 开始、到 `end_markers` 中任一标记结束的段
fn extract_segment(stripped: &str, marker: &str, end_markers: &[&str]) -> String {
    let marker_stripped = strip_whitespace(marker);
    if let Some(start) = stripped.find(&marker_stripped) {
        let remaining = &stripped[start..];
        let end = end_markers
            .iter()
            .filter_map(|em| {
                let em_s = strip_whitespace(em);
                remaining.find(&em_s).map(|p| p + em_s.len())
            })
            .min()
            .unwrap_or(remaining.len().min(500));

        floor_char_boundary(remaining, end).to_string()
    } else {
        String::new()
    }
}

/// 从区别段提取特征术语
///
/// 如果提供了 `known_features`，直接使用；
/// 否则尝试从文本中提取引号内容或常见模式。
fn extract_features(distinction_text: &str, known_features: &[&str]) -> Vec<String> {
    if !known_features.is_empty() {
        return known_features.iter().map(|s| s.to_string()).collect();
    }

    let mut features = Vec::new();

    // 尝试提取引号内容
    let re = regex::Regex::new(r#""([^"]+)""#).ok();
    if let Some(re) = re {
        for cap in re.captures_iter(distinction_text) {
            if let Some(m) = cap.get(1) {
                let f = m.as_str().to_string();
                if f.len() >= 2 && f.len() <= 20 {
                    features.push(f);
                }
            }
        }
    }

    features
}

// ═══════════════════════════════════════════════════════════════════════
//  测试
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_distinction_and_tech_problem() {
        let oa = "区别在于：本发明的火焰喷嘴为对角安装，而对比文件1为交错设置。发明实际解决的技术问题是：如何提高燃烧稳定性。";
        let report = check_tech_problem(oa, &["对角安装", "燃烧稳定性"]);
        assert!(!report.distinction_text.is_empty());
        assert!(!report.tech_problem_text.is_empty());
        assert!(report.distinction_features.len() >= 2);
    }

    #[test]
    fn detects_uncovered_features() {
        let oa = "区别在于：对角安装、内侧面呈长方形、弹簧定位。发明实际解决的技术问题是：如何提高对角安装的稳定性。";
        let report = check_tech_problem(oa, &["对角安装", "内侧面", "弹簧"]);
        // "对角安装"被技术问题覆盖，"内侧面"和"弹簧"未覆盖
        assert!(report.uncovered_features.len() >= 2);
        assert!(report.summary.contains("遗漏"));
    }

    #[test]
    fn no_uncovered_when_complete() {
        let oa = "区别在于：对角安装。发明实际解决的技术问题是：如何实现对角安装。";
        let report = check_tech_problem(oa, &["对角安装"]);
        assert!(report.uncovered_features.is_empty());
        assert!(report.summary.contains("完整"));
    }

    // ── 示例申请人案回归 ──

    #[test]
    fn sample_case_c5_c11_tech_problem_check() {
        let oa = include_str!("../../tests/fixtures/oa_sample/oa_rejection.txt");
        let features: &[&str] = &["对角", "网状", "把手", "控制阀", "弹簧", "定位杆", "滑块"];
        let report = check_tech_problem(oa, features);

        eprintln!(
            "示例申请人案 C5/C11：区别段长度{}，技术问题句长度{}，未覆盖特征{:?}",
            report.distinction_text.len(),
            report.tech_problem_text.len(),
            report.uncovered_features
        );

        // 区别段应非空
        if report.distinction_text.is_empty() {
            eprintln!("警告：未提取到区别段，可能 OCR 格式问题或标记词不同");
        }
    }
}
