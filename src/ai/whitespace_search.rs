//! 去空格文本搜索工具 — C1/C2/C9/C11/C12 纯函数层的基础设施
//!
//! 所有专利 OA 文本经 OCR 提取，含大量空格、换行、全角空格等噪声。
//! 去空格后逐字计数是可复现的确定性操作，不依赖 AI。

/// 去除全部空白字符：空格、制表符、换行、回车、全角空格（U+3000）、
/// 不换行空格（U+00A0）、BOM（U+FEFF）。
pub fn strip_whitespace(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{3000}' && *c != '\u{00A0}' && *c != '\u{FEFF}')
        .collect()
}

/// 将字节索引回退到最近的字符边界，避免 UTF-8 切割 panic。
pub fn floor_char_boundary(s: &str, idx: usize) -> &str {
    if idx >= s.len() {
        return s;
    }
    let mut i = idx;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    &s[..i]
}

/// 安全切片：取 `[start, end)` 并保证落在字符边界上。
pub fn safe_slice(s: &str, start: usize, end: usize) -> &str {
    let lo = if start >= s.len() {
        return "";
    } else {
        start
    };
    let hi = end.min(s.len());
    let mut lo = lo;
    let mut hi = hi;
    while lo > 0 && !s.is_char_boundary(lo) {
        lo -= 1;
    }
    while hi > lo && !s.is_char_boundary(hi) {
        hi -= 1;
    }
    &s[lo..hi]
}

/// 在**已去空格**文本中计数 term 出现次数（非重叠匹配）。
///
/// 调用方应先 `strip_whitespace` 再传入，避免重复去空格。
/// term 本身也会被去空格后再搜索，确保 "对 角" 能命中 "对角"。
pub fn count_term(stripped_text: &str, term: &str) -> usize {
    let needle = strip_whitespace(term);
    if needle.is_empty() {
        return 0;
    }
    let mut count = 0;
    let mut idx = 0;
    while let Some(pos) = stripped_text[idx..].find(&needle) {
        count += 1;
        idx += pos + needle.len();
    }
    count
}

/// 批量计数：对同一去空格文本同时计数多个术语，返回 `[(term, count)]`。
pub fn count_terms<'a>(stripped_text: &str, terms: &[&'a str]) -> Vec<(&'a str, usize)> {
    terms
        .iter()
        .map(|&term| (term, count_term(stripped_text, term)))
        .collect()
}

/// 双向比对报告：同一组术语在两个文本中的去空格计数差异。
#[derive(Debug, Clone, PartialEq)]
pub struct BidirectionalReport {
    /// 每个术语在文本 A（通常是驳回决定）中的计数
    pub a_counts: Vec<(&'static str, usize)>,
    /// 每个术语在文本 B（通常是对比文件原文）中的计数
    pub b_counts: Vec<(&'static str, usize)>,
    /// 失真项：术语在 A 中出现但在 B 中为 0，或反之
    pub distortions: Vec<Distortion>,
}

/// 单个术语的双向失真
#[derive(Debug, Clone, PartialEq)]
pub struct Distortion {
    pub term: String,
    pub a_count: usize,
    pub b_count: usize,
    /// 失真类型
    pub kind: DistortionKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DistortionKind {
    /// A 有 B 无：A 中使用了该术语，但 B 原文中不存在
    AOnly,
    /// B 有 A 无：B 原文中有该术语，但 A 中未提及
    BOnly,
}

/// 双向比对：在 `text_a` 和 `text_b` 中分别去空格计数 `terms`，
/// 输出差异报告。
///
/// 典型用法：`text_a` = 驳回决定，`text_b` = 对比文件原文，
/// `terms` = ["对角", "交错", "内侧面", "长方形"]。
pub fn bidirectional_compare(
    text_a: &str,
    text_b: &str,
    terms: &[&'static str],
) -> BidirectionalReport {
    let a_stripped = strip_whitespace(text_a);
    let b_stripped = strip_whitespace(text_b);

    let a_counts: Vec<(&'static str, usize)> = count_terms(&a_stripped, terms);
    let b_counts: Vec<(&'static str, usize)> = count_terms(&b_stripped, terms);

    let mut distortions = Vec::new();
    for (i, &term) in terms.iter().enumerate() {
        let a = a_counts[i].1;
        let b = b_counts[i].1;
        if a > 0 && b == 0 {
            distortions.push(Distortion {
                term: term.to_string(),
                a_count: a,
                b_count: b,
                kind: DistortionKind::AOnly,
            });
        } else if a == 0 && b > 0 {
            distortions.push(Distortion {
                term: term.to_string(),
                a_count: a,
                b_count: b,
                kind: DistortionKind::BOnly,
            });
        }
    }

    BidirectionalReport {
        a_counts,
        b_counts,
        distortions,
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  测试
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_removes_all_whitespace_types() {
        let input = "对 角\t安装\n的\r\n电极\u{3000}发生器\u{00A0}";
        let stripped = strip_whitespace(input);
        assert_eq!(stripped, "对角安装的电极发生器");
    }

    #[test]
    fn count_term_basic() {
        let text = "对称分布的传感元件和辅助信号传感元件，对称布置";
        let stripped = strip_whitespace(text);
        assert_eq!(count_term(&stripped, "对称"), 2);
        assert_eq!(count_term(&stripped, "传感"), 2);
        assert_eq!(count_term(&stripped, "交错"), 0);
    }

    #[test]
    fn count_term_handles_whitespace_in_term() {
        let text = strip_whitespace("反应腔侧壁设有对称分布的传感元件");
        // term 内含空格也应去空格后匹配
        assert_eq!(count_term(&text, "对 称"), 1);
    }

    #[test]
    fn count_term_empty_returns_zero() {
        let text = strip_whitespace("任意文本");
        assert_eq!(count_term(&text, ""), 0);
        assert_eq!(count_term(&text, "   "), 0);
    }

    #[test]
    fn count_terms_batch() {
        let text = strip_whitespace("对角安装交错设置内侧面长方形");
        let counts = count_terms(&text, &["对角", "交错", "内侧面", "长方形", "不存在"]);
        assert_eq!(
            counts,
            vec![
                ("对角", 1),
                ("交错", 1),
                ("内侧面", 1),
                ("长方形", 1),
                ("不存在", 0),
            ]
        );
    }

    #[test]
    fn bidirectional_detects_a_only() {
        let oa = "驳回决定说对角安装";
        let d1 = "D1原文说交错设置";
        let report = bidirectional_compare(oa, d1, &["对角", "交错"]);

        assert_eq!(report.a_counts, vec![("对角", 1), ("交错", 0)]);
        assert_eq!(report.b_counts, vec![("对角", 0), ("交错", 1)]);
        assert_eq!(report.distortions.len(), 2);
        assert!(report
            .distortions
            .iter()
            .any(|d| d.term == "对角" && d.kind == DistortionKind::AOnly));
        assert!(report
            .distortions
            .iter()
            .any(|d| d.term == "交错" && d.kind == DistortionKind::BOnly));
    }

    #[test]
    fn safe_slice_handles_multibyte() {
        let s = "对角安装的电极";
        // 在多字节字符中间切割不应 panic
        let _ = safe_slice(s, 1, 7);
        let _ = safe_slice(s, 2, 5);
        let _ = floor_char_boundary(s, 4);
    }

    // ── 示例申请人案回归数据 ──

    #[test]
    fn sample_case_oa_rejection_duojiao_count() {
        let oa = include_str!("../../tests/fixtures/oa_sample/oa_rejection.txt");
        let stripped = strip_whitespace(oa);
        // 驳回决定中 "对称" 出现 1 次
        assert_eq!(count_term(&stripped, "对称"), 1);
        // 驳回决定中 "交错" 出现 0 次
        assert_eq!(count_term(&stripped, "交错"), 0);
        // 驳回决定中 "内侧面" 出现 0 次
        assert_eq!(count_term(&stripped, "内侧面"), 0);
        // 驳回决定中 "长方形" 出现 0 次
        assert_eq!(count_term(&stripped, "长方形"), 0);
    }

    #[test]
    fn sample_case_submitted_claims_has_duojiao() {
        // 关键发现：递交权项1自身就写了"对称分布"——发明自己的用语
        let claims = include_str!("../../tests/fixtures/oa_sample/submitted_claims.txt");
        let stripped = strip_whitespace(claims);
        assert!(count_term(&stripped, "对称") >= 1, "递交权项应含'对称'");
        // 确认递交权项无机理句
        assert_eq!(count_term(&stripped, "电场"), 0, "递交权项不应含'电场'");
        assert_eq!(count_term(&stripped, "交流"), 0, "递交权项不应含'交流'");
        assert_eq!(
            count_term(&stripped, "接通电源"),
            0,
            "递交权项不应含'接通电源'"
        );
        assert_eq!(
            count_term(&stripped, "辅助电场"),
            0,
            "递交权项不应含'辅助电场'"
        );
        assert_eq!(count_term(&stripped, "形变"), 0, "递交权项不应含'形变'");
        assert_eq!(count_term(&stripped, "湍流"), 0, "递交权项不应含'湍流'");
        assert_eq!(count_term(&stripped, "迁移"), 0, "递交权项不应含'迁移'");
        assert_eq!(count_term(&stripped, "往返"), 0, "递交权项不应含'往返'");
        assert_eq!(count_term(&stripped, "周期性"), 0, "递交权项不应含'周期性'");
        assert_eq!(
            count_term(&stripped, "带电粒子"),
            0,
            "递交权项不应含'带电粒子'"
        );
        // 确认结构特征存在
        assert!(count_term(&stripped, "网状") >= 4, "递交权项应含'网状'>=4");
        assert_eq!(count_term(&stripped, "提手"), 1);
        assert_eq!(count_term(&stripped, "调节阀"), 1);
        assert_eq!(count_term(&stripped, "氧化铝"), 1);
        assert_eq!(count_term(&stripped, "弹性件"), 1);
        assert_eq!(count_term(&stripped, "固定销"), 2);
        assert_eq!(count_term(&stripped, "滑动件"), 3);
    }
}
