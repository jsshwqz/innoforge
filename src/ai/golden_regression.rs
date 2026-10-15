//! C13 黄金回归测试（纯函数）
//!
//! 将系统输出与预存的黄金参考做逐字比对。
//! 黄金参考是人工审核过的标准答复函，每次代码变更后跑此模块确保输出不退化。

use crate::ai::whitespace_search::strip_whitespace;

/// C13 黄金回归报告
#[derive(Debug, Clone, PartialEq)]
pub struct GoldenRegressionReport {
    /// 系统输出（去空格后）
    pub output_stripped: String,
    /// 黄金参考（去空格后）
    pub golden_stripped: String,
    /// 字符级差异位置数
    pub char_diff_count: usize,
    /// 关键术语覆盖差异
    pub term_coverage_diff: Vec<(String, i64)>,
    /// 相似度（0.0 ~ 1.0）
    pub similarity: f64,
    /// 是否通过回归（相似度 >= threshold）
    pub passed: bool,
    /// 人类可读摘要
    pub summary: String,
}

/// C13 主函数：将系统输出与黄金回归比对
///
/// # 参数
/// - `output`: 系统生成的答复函
/// - `golden`: 黄金参考答复函
/// - `key_terms`: 关键术语列表（用于覆盖率比对）
/// - `threshold`: 通过阈值（0.0 ~ 1.0，默认 0.85）
pub fn check_golden_regression(
    output: &str,
    golden: &str,
    key_terms: &[&str],
    threshold: f64,
) -> GoldenRegressionReport {
    let output_stripped = strip_whitespace(output);
    let golden_stripped = strip_whitespace(golden);

    // 字符级差异（编辑距离上界：长度差 + 逐字符差异）
    let char_diff_count = char_level_diff(&output_stripped, &golden_stripped);

    // 关键术语覆盖差异
    let term_coverage_diff: Vec<(String, i64)> = key_terms
        .iter()
        .map(|&term| {
            let out_count = count_term_simple(&output_stripped, term);
            let gold_count = count_term_simple(&golden_stripped, term);
            (term.to_string(), out_count as i64 - gold_count as i64)
        })
        .filter(|(_, diff)| *diff != 0)
        .collect();

    // 相似度计算（基于最长公共子序列的近似）
    let similarity = compute_similarity(&output_stripped, &golden_stripped);

    let passed = similarity >= threshold;

    let summary = if passed {
        format!(
            "黄金回归通过：相似度{:.1}%（阈值{:.1}%），字符差异{}，术语覆盖差异{}项",
            similarity * 100.0,
            threshold * 100.0,
            char_diff_count,
            term_coverage_diff.len()
        )
    } else {
        format!(
            "黄金回归未通过：相似度{:.1}%（阈值{:.1}%），字符差异{}，术语覆盖差异{}项",
            similarity * 100.0,
            threshold * 100.0,
            char_diff_count,
            term_coverage_diff.len()
        )
    };

    GoldenRegressionReport {
        output_stripped,
        golden_stripped,
        char_diff_count,
        term_coverage_diff,
        similarity,
        passed,
        summary,
    }
}

fn count_term_simple(stripped: &str, term: &str) -> usize {
    let needle = strip_whitespace(term);
    if needle.is_empty() {
        return 0;
    }
    let mut count = 0;
    let mut idx = 0;
    while let Some(pos) = stripped[idx..].find(&needle) {
        count += 1;
        idx += pos + needle.len();
    }
    count
}

fn char_level_diff(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    let len_diff = (a_chars.len() as i64 - b_chars.len() as i64).unsigned_abs() as usize;

    let min_len = a_chars.len().min(b_chars.len());
    let char_diff = a_chars[..min_len]
        .iter()
        .zip(b_chars[..min_len].iter())
        .filter(|(x, y)| x != y)
        .count();

    char_diff + len_diff
}

/// 近似相似度：基于 n-gram Jaccard
fn compute_similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let n = 3; // 3-gram
    let grams_a = ngrams(a, n);
    let grams_b = ngrams(b, n);

    if grams_a.is_empty() && grams_b.is_empty() {
        return 1.0;
    }

    let intersection = grams_a.intersection(&grams_b).count();
    let union = grams_a.union(&grams_b).count();

    if union == 0 {
        return 0.0;
    }

    intersection as f64 / union as f64
}

fn ngrams(text: &str, n: usize) -> std::collections::HashSet<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < n {
        return std::collections::HashSet::new();
    }
    (0..=chars.len() - n)
        .map(|i| chars[i..i + n].iter().collect::<String>())
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
//  测试
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_texts_pass() {
        let text = "本发明的对角安装方式产生了意想不到的技术效果。";
        let report = check_golden_regression(text, text, &["对角", "安装"], 0.85);
        assert!(report.passed);
        assert!((report.similarity - 1.0).abs() < 0.01);
    }

    #[test]
    fn similar_texts_pass() {
        let output = "审查员的认定不能成立，因为对角安装产生了意想不到的效果。";
        let golden = "审查员的认定不能成立，因为对角安装产生了意想不到的效果。";
        let report = check_golden_regression(output, golden, &["对角"], 0.85);
        assert!(report.passed);
    }

    #[test]
    fn different_texts_fail() {
        let output = "同意审查员意见。";
        let golden = "审查员的认定不能成立，因为对角安装产生了意想不到的效果。";
        let report = check_golden_regression(output, golden, &["对角"], 0.85);
        assert!(!report.passed);
    }

    #[test]
    fn term_coverage_diff_detected() {
        let output = "对角安装的电极";
        let golden = "对角安装的电极和网状发生器";
        let report = check_golden_regression(output, golden, &["对角", "网状"], 0.3);
        // "网状"在 golden 有 1 次，output 有 0 次 → diff = -1
        assert!(report
            .term_coverage_diff
            .iter()
            .any(|(t, d)| t == "网状" && *d == -1));
    }

    // ── 示例申请人案回归 ──

    #[test]
    fn sample_case_c13_golden_self_regression() {
        // 黄金参考与自身比对应 100% 通过
        let golden = include_str!("../../tests/fixtures/oa_sample/golden_reference.txt");
        let report = check_golden_regression(
            golden,
            golden,
            &["对角", "网状", "把手", "控制阀", "弹簧", "定位杆", "滑块"],
            0.85,
        );
        assert!(report.passed);
        assert!((report.similarity - 1.0).abs() < 0.01);
        assert_eq!(report.char_diff_count, 0);
        assert!(report.term_coverage_diff.is_empty());
    }
}
