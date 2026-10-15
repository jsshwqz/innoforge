//! C10 区别特征双向归属检测（纯函数）
//!
//! 审查员在 OA 中声称某区别特征来自对比文件 D1，但实际上该特征可能
//! 在 D1 中并不存在，而是在 D2 或 D3 中。本模块检测这种错误归属。
//!
//! 检测逻辑：
//! 1. 从 OA 的"区别在于"段提取区别特征
//! 2. 对每个特征，在所有对比文件中做去空格搜索
//! 3. 检查审查员归属（"对比文件N公开了特征X"）是否与实际命中一致

use crate::ai::whitespace_search::{count_term, safe_slice, strip_whitespace};

/// C10 区别特征归属报告
#[derive(Debug, Clone, PartialEq)]
pub struct FeatureAttributionReport {
    /// 各特征的归属检测结果
    pub features: Vec<FeatureAttribution>,
    /// 错误归属数
    pub misattribution_count: usize,
    /// 人类可读摘要
    pub summary: String,
}

/// 单个区别特征的归属检测
#[derive(Debug, Clone, PartialEq)]
pub struct FeatureAttribution {
    /// 特征术语
    pub feature: String,
    /// 审查员声称的来源（如 "对比文件1"）
    pub claimed_source: String,
    /// 实际命中的对比文件列表
    pub actual_sources: Vec<String>,
    /// 归属是否正确
    pub is_correct: bool,
}

/// C10 主函数：检测区别特征的双向归属
///
/// # 参数
/// - `oa_text`: OA/驳回决定全文
/// - `features`: 区别特征术语列表
/// - `documents`: 对比文件列表，`(doc_id, original_text)` 对
pub fn check_feature_attribution(
    oa_text: &str,
    features: &[&str],
    documents: &[(&str, &str)],
) -> FeatureAttributionReport {
    let stripped = strip_whitespace(oa_text);

    let mut feature_results = Vec::new();
    let mut misattribution_count = 0;

    for &feature in features {
        // 查找审查员声称的来源
        let claimed_source = find_claimed_source(&stripped, feature);

        // 在所有对比文件中搜索
        let actual_sources: Vec<String> = documents
            .iter()
            .filter(|(_, text)| {
                let orig_stripped = strip_whitespace(text);
                count_term(&orig_stripped, feature) > 0
            })
            .map(|(id, _)| id.to_string())
            .collect();

        // 判断归属是否正确
        let is_correct = if claimed_source.is_empty() {
            // 审查员未明确归属，不算错误
            true
        } else if actual_sources.is_empty() {
            // 审查员归属了某文件，但所有文件中都没有此特征
            false
        } else {
            // 审查员归属的文件应在实际命中列表中
            actual_sources.iter().any(|s| s == &claimed_source)
        };

        if !is_correct {
            misattribution_count += 1;
        }

        feature_results.push(FeatureAttribution {
            feature: feature.to_string(),
            claimed_source,
            actual_sources,
            is_correct,
        });
    }

    let summary = if misattribution_count == 0 {
        format!("共检测{}个区别特征归属，全部正确。", features.len())
    } else {
        let misattributed: Vec<_> = feature_results
            .iter()
            .filter(|f| !f.is_correct)
            .map(|f| f.feature.clone())
            .collect();
        format!(
            "共检测{}个区别特征归属，{}个归属错误：{}",
            features.len(),
            misattribution_count,
            misattributed.join("、")
        )
    };

    FeatureAttributionReport {
        features: feature_results,
        misattribution_count,
        summary,
    }
}

/// 从 OA 文本中查找审查员对某特征的声称来源
///
/// 匹配 "对比文件N...特征X" 或 "特征X...对比文件N"
fn find_claimed_source(stripped: &str, feature: &str) -> String {
    let feature_stripped = strip_whitespace(feature);

    // 方式1："对比文件N...公开了...特征X"
    let re1 = Some(
        regex::Regex::new(r"对比文件(\d+)[^。]*?(?:公开|披露|记载|描述)[^。]*?")
            .expect("feature_attribution: 对比文件特征归属正则1"),
    );
    if let Some(re) = re1 {
        for cap in re.captures_iter(stripped) {
            let match_end = cap.get(0).unwrap().end();
            let after = safe_slice(stripped, match_end, match_end + 200);
            if after.contains(&feature_stripped) {
                let n = cap
                    .get(1)
                    .and_then(|m| m.as_str().parse::<usize>().ok())
                    .unwrap_or(0);
                return format!("对比文件{}", n);
            }
        }
    }

    // 方式2：特征出现在"对比文件N"附近（±100字符）
    if let Some(feat_pos) = stripped.find(&feature_stripped) {
        let ctx_start = feat_pos.saturating_sub(100);
        let ctx_end = (feat_pos + feature_stripped.len() + 100).min(stripped.len());

        // 在上下文中找最近的"对比文件N"
        let ctx = safe_slice(stripped, ctx_start, ctx_end);
        let re2 = Some(
            regex::Regex::new(r"对比文件(\d+)")
                .expect("feature_attribution: 对比文件编号提取正则2"),
        );
        if let Some(re) = re2 {
            if let Some(cap) = re.captures(ctx) {
                let n = cap
                    .get(1)
                    .and_then(|m| m.as_str().parse::<usize>().ok())
                    .unwrap_or(0);
                return format!("对比文件{}", n);
            }
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
    fn correct_attribution() {
        let oa = "对比文件1公开了对角安装的电极。区别在于对角安装。";
        let d1 = "对角安装的电极发生器";
        let d2 = "交错设置的电极";
        let report =
            check_feature_attribution(oa, &["对角"], &[("对比文件1", d1), ("对比文件2", d2)]);
        assert_eq!(report.misattribution_count, 0);
        assert!(report.summary.contains("全部正确"));
    }

    #[test]
    fn misattribution_detected() {
        // 审查员说"对比文件1公开了交错"，但"交错"只在 D2 中
        let oa = "对比文件1公开了交错的电极。区别在于交错。";
        let d1 = "对角安装的电极";
        let d2 = "交错设置的电极";
        let report =
            check_feature_attribution(oa, &["交错"], &[("对比文件1", d1), ("对比文件2", d2)]);
        assert_eq!(report.misattribution_count, 1);
        assert!(report.summary.contains("归属错误"));
    }

    #[test]
    fn feature_in_multiple_docs() {
        let oa = "对比文件1公开了电极。";
        let d1 = "对角的电极";
        let d2 = "交错的电极";
        let report =
            check_feature_attribution(oa, &["电极"], &[("对比文件1", d1), ("对比文件2", d2)]);
        // "电极"在两个文件中都有，审查员归属 D1 是正确的（D1 确实有）
        assert_eq!(report.misattribution_count, 0);
    }

    #[test]
    fn no_claimed_source() {
        let oa = "区别在于对角安装。";
        let d1 = "对角安装";
        let report = check_feature_attribution(oa, &["对角"], &[("对比文件1", d1)]);
        // 审查员未明确归属，不算错误
        assert_eq!(report.misattribution_count, 0);
    }

    // ── 示例申请人案回归 ──

    #[test]
    fn sample_case_c10_feature_attribution() {
        let oa = include_str!("../../tests/fixtures/oa_sample/oa_rejection.txt");
        let d1 = include_str!("../../tests/fixtures/oa_sample/d1_sample.txt");
        let d2 = include_str!("../../tests/fixtures/oa_sample/d2_sample.txt");
        let d3 = include_str!("../../tests/fixtures/oa_sample/d3_sample.txt");

        let report = check_feature_attribution(
            oa,
            &[
                "对角",
                "网状",
                "把手",
                "控制阀",
                "弹簧",
                "定位杆",
                "滑块",
                "陶瓷",
            ],
            &[("对比文件1", d1), ("对比文件2", d2), ("对比文件3", d3)],
        );

        eprintln!(
            "示例申请人案 C10：{}个特征，{}个归属错误",
            report.features.len(),
            report.misattribution_count
        );
        for f in &report.features {
            eprintln!(
                "  特征 '{}'：声称来源={}，实际命中={:?}，正确={}",
                f.feature, f.claimed_source, f.actual_sources, f.is_correct
            );
        }
    }
}
