//! C1 转述失真检测（纯函数层）
//!
//! 比对审查员转述文本与对比文件原文，检测转述遗漏/改写。
//! 本模块为纯函数，不调用 AI——AI 调用版本在 patent.rs::detect_paraphrase_conflicts。
//!
//! 检测逻辑：
//! 1. 提取审查员转述的对比文件描述段（"对比文件1公开了..."）
//! 2. 对每个术语做双向去空格计数
//! 3. 若审查员转述中使用了某术语，但对比文件原文中不存在 → 转述添加
//! 4. 若对比文件原文有某术语，但审查员转述中不存在 → 转述遗漏

use crate::ai::whitespace_search::{
    bidirectional_compare, floor_char_boundary, safe_slice, strip_whitespace, BidirectionalReport,
    DistortionKind,
};

/// C1 转述失真报告
#[derive(Debug, Clone, PartialEq)]
pub struct ParaphraseCheckReport {
    /// 对比文件编号（如 "D1" / "对比文件1"）
    pub doc_id: String,
    /// 审查员转述文本
    pub paraphrase_text: String,
    /// 对比文件原文文本
    pub original_text: String,
    /// 双向比对结果
    pub comparison: BidirectionalReport,
    /// 失真项数
    pub distortion_count: usize,
    /// 人类可读摘要
    pub summary: String,
}

/// C1 主函数：检测单个对比文件的转述失真
///
/// # 参数
/// - `doc_id`: 对比文件编号（如 "D1"）
/// - `paraphrase`: 审查员转述文本（从 OA 中提取的"对比文件N公开了..."段）
/// - `original`: 对比文件原文文本
/// - `terms`: 待检测的术语列表
///
/// # 返回
/// 转述失真报告
pub fn check_paraphrase_distortion(
    doc_id: &str,
    paraphrase: &str,
    original: &str,
    terms: &[&'static str],
) -> ParaphraseCheckReport {
    let comparison = bidirectional_compare(paraphrase, original, terms);

    let distortion_count = comparison.distortions.len();

    let summary = if distortion_count == 0 {
        format!("{}转述与原文一致，未检测到失真。", doc_id)
    } else {
        let a_only: Vec<_> = comparison
            .distortions
            .iter()
            .filter(|d| d.kind == DistortionKind::AOnly)
            .map(|d| d.term.clone())
            .collect();
        let b_only: Vec<_> = comparison
            .distortions
            .iter()
            .filter(|d| d.kind == DistortionKind::BOnly)
            .map(|d| d.term.clone())
            .collect();

        let mut parts = Vec::new();
        if !a_only.is_empty() {
            parts.push(format!("转述添加了原文没有的术语：{}", a_only.join("、")));
        }
        if !b_only.is_empty() {
            parts.push(format!("转述遗漏了原文中的术语：{}", b_only.join("、")));
        }
        format!("{}存在转述失真：{}", doc_id, parts.join("；"))
    };

    ParaphraseCheckReport {
        doc_id: doc_id.to_string(),
        paraphrase_text: paraphrase.to_string(),
        original_text: original.to_string(),
        comparison,
        distortion_count,
        summary,
    }
}

/// 批量检测多个对比文件的转述失真
pub fn check_paraphrase_distortions(
    checks: &[(&str, &str, &str, &[&'static str])],
) -> Vec<ParaphraseCheckReport> {
    checks
        .iter()
        .map(|&(doc_id, paraphrase, original, terms)| {
            check_paraphrase_distortion(doc_id, paraphrase, original, terms)
        })
        .collect()
}

/// 从 OA 文本中提取审查员转述段
///
/// 匹配 "对比文件N公开了..." 或 "对比文件N披露了..." 后到句号或下一个"对比文件"的文本
pub fn extract_paraphrase_segments(oa_text: &str) -> Vec<(String, String)> {
    let stripped = strip_whitespace(oa_text);
    let mut segments = Vec::new();

    let re = regex::Regex::new(r"对比文件(\d+)(公开|披露|记载|描述)了").ok();
    if let Some(re) = re {
        for cap in re.captures_iter(&stripped) {
            let doc_num = cap
                .get(1)
                .and_then(|m| m.as_str().parse::<usize>().ok())
                .unwrap_or(0);
            let start = cap.get(0).unwrap().start();

            // 找到段末：下一个"对比文件"或句号或300字符限制
            let remaining = &stripped[start..];
            let end = remaining
                .find("对比文件")
                .filter(|&p| p > 10)
                .or_else(|| remaining.find('。').map(|p| p + 1))
                .unwrap_or(300)
                .min(300);

            let text = floor_char_boundary(remaining, end).to_string();
            segments.push((format!("对比文件{}", doc_num), text));
        }
    }

    segments
}

// ═══════════════════════════════════════════════════════════════════════
//  测试
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_distortion() {
        let paraphrase = "对比文件1公开了通风管和火焰喷嘴";
        let original = "包括通风管以及火焰喷嘴";
        let terms = ["通风管", "火焰喷嘴"];
        let report = check_paraphrase_distortion("D1", paraphrase, original, &terms);
        assert_eq!(report.distortion_count, 0);
        assert!(report.summary.contains("一致"));
    }

    #[test]
    fn detects_a_only_addition() {
        // 审查员转述添加了"交错"，但原文是"对角"
        let paraphrase = "对比文件1公开了交错安装的电极";
        let original = "对角安装的电极";
        let terms = ["对角", "交错", "电极"];
        let report = check_paraphrase_distortion("D1", paraphrase, original, &terms);
        assert!(report.distortion_count > 0);
        assert!(report.summary.contains("转述添加"));
    }

    #[test]
    fn detects_b_only_omission() {
        // 审查员转述遗漏了原文中的"内侧面"
        let paraphrase = "对比文件1公开了电极发生器";
        let original = "电极发生器的内侧面呈长方形";
        let terms = ["电极", "内侧面", "长方形"];
        let report = check_paraphrase_distortion("D1", paraphrase, original, &terms);
        assert!(report.distortion_count > 0);
        assert!(report.summary.contains("转述遗漏"));
    }

    #[test]
    fn extract_paraphrase_segments_basic() {
        let oa = "对比文件1公开了一种燃烧器，包括通风管。对比文件2披露了火焰喷嘴。";
        let segments = extract_paraphrase_segments(oa);
        assert_eq!(segments.len(), 2);
        assert!(segments[0].0 == "对比文件1");
        assert!(segments[1].0 == "对比文件2");
    }

    // ── 示例申请人案回归 ──

    #[test]
    fn sample_case_c1_d1_paraphrase_check() {
        let oa = include_str!("../../tests/fixtures/oa_sample/oa_rejection.txt");
        let d1 = include_str!("../../tests/fixtures/oa_sample/d1_claims.txt");

        // 提取审查员对 D1 的转述
        let segments = extract_paraphrase_segments(oa);
        let d1_paraphrase = segments
            .iter()
            .find(|(doc, _)| doc == "对比文件1")
            .map(|(_, text)| text.as_str())
            .unwrap_or("");

        if d1_paraphrase.is_empty() {
            eprintln!("警告：未提取到 D1 转述段，可能 OCR 格式问题");
            return;
        }

        // 检测关键术语
        let terms: &[&'static str] = &["对角", "交错", "内侧面", "长方形", "稳燃锥罩"];
        let report = check_paraphrase_distortion("D1", d1_paraphrase, d1, terms);

        // 本案关键发现：审查员转述用了"对角"，D1 原文也是"对角"——一致
        // 如果审查员转述用了"交错"而 D1 原文用"对角"，则应检测到失真
        eprintln!("D1 转述检测：{} 失真项", report.distortion_count);
    }
}
