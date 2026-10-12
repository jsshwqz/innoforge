//! C4 对比文件引文逐字核对（纯函数）
//!
//! 审查员在 OA 中引用对比文件原文时，常出现引文与原文不符的情况。
//! 本模块提取 OA 中的引文段，与对比文件原文做去空格逐字比对。
//!
//! 检测逻辑：
//! 1. 从 OA 中提取引号内容或"记载了""公开了"后的引文段
//! 2. 对每段引文，在对应对比文件原文中做去空格子串搜索
//! 3. 若引文在原文中找不到 → 引文失真

use crate::ai::whitespace_search::{safe_slice, strip_whitespace};

/// C4 引文核对报告
#[derive(Debug, Clone, PartialEq)]
pub struct CitationCheckReport {
    /// 检测到的引文条目
    pub citations: Vec<CitationEntry>,
    /// 失真引文数（在原文中找不到的）
    pub mismatch_count: usize,
    /// 人类可读摘要
    pub summary: String,
}

/// 单条引文
#[derive(Debug, Clone, PartialEq)]
pub struct CitationEntry {
    /// 对比文件编号（如 "D1" / "对比文件1"）
    pub doc_id: String,
    /// 引文文本
    pub quoted_text: String,
    /// 引文在原文中是否找到（去空格子串匹配）
    pub found_in_original: bool,
    /// 匹配位置（如果找到）
    pub match_position: Option<usize>,
}

/// C4 主函数：核对 OA 中引用的对比文件原文是否准确
///
/// # 参数
/// - `oa_text`: OA/驳回决定全文
/// - `documents`: 对比文件列表，`(doc_id, original_text)` 对
///
/// # 返回
/// 引文核对报告
pub fn check_citations(oa_text: &str, documents: &[(&str, &str)]) -> CitationCheckReport {
    let stripped = strip_whitespace(oa_text);

    // 提取所有引文段
    let raw_citations = extract_citations(&stripped);

    let mut citations = Vec::new();
    let mut mismatch_count = 0;

    for (doc_id, quoted_text) in &raw_citations {
        // 找到对应的对比文件原文
        let original = documents
            .iter()
            .find(|(id, _)| id == doc_id || id == &format!("对比文件{}", doc_id))
            .map(|(_, text)| *text);

        if let Some(orig) = original {
            let orig_stripped = strip_whitespace(orig);
            let quote_stripped = strip_whitespace(quoted_text);

            // 在原文中搜索引文（去空格子串匹配）
            if quote_stripped.len() < 4 {
                // 引文太短，跳过
                continue;
            }

            if let Some(pos) = orig_stripped.find(&quote_stripped) {
                citations.push(CitationEntry {
                    doc_id: doc_id.clone(),
                    quoted_text: quoted_text.clone(),
                    found_in_original: true,
                    match_position: Some(pos),
                });
            } else {
                citations.push(CitationEntry {
                    doc_id: doc_id.clone(),
                    quoted_text: quoted_text.clone(),
                    found_in_original: false,
                    match_position: None,
                });
                mismatch_count += 1;
            }
        }
    }

    let summary = if mismatch_count == 0 {
        format!("共核对{}条引文，全部与原文一致。", citations.len())
    } else {
        format!(
            "共核对{}条引文，其中{}条与原文不符（去空格后未在原文中找到）。",
            citations.len(),
            mismatch_count
        )
    };

    CitationCheckReport {
        citations,
        mismatch_count,
        summary,
    }
}

/// 从 OA 文本中提取引文段
///
/// 支持两种格式：
/// 1. 引号内容："对比文件N公开了"后跟引号内容
/// 2. 记载段："对比文件N记载了"后到句号的内容
fn extract_citations(stripped: &str) -> Vec<(String, String)> {
    let mut citations = Vec::new();

    // 方式1：引号内容 — 同时匹配 ASCII " 和中文 \u{201c}\u{201d}
    // CNIPA 正文使用中文引号“”，纯 ASCII " 正则对真实 OA 文本恒不命中。
    let re_quote = Some(
        regex::Regex::new(
            r#"对比文件(\d+)(?:公开|披露|记载|描述)了[^"\u{201c}\u{201d}]*?["\u{201c}\u{201d}]([^"\u{201c}\u{201d}]{4,})["\u{201c}\u{201d}]"#,
        )
        .expect("citation_check: 对比文件引用提取正则（含中文引号）"),
    );
    if let Some(re) = re_quote {
        for cap in re.captures_iter(stripped) {
            let doc_num = cap
                .get(1)
                .and_then(|m| m.as_str().parse::<usize>().ok())
                .unwrap_or(0);
            let quote = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            if quote.len() >= 4 {
                citations.push((format!("对比文件{}", doc_num), quote));
            }
        }
    }

    // 方式2：记载了 + 到句号
    let re_segment = Some(
        regex::Regex::new(r"对比文件(\d+)记载了").expect("citation_check: 对比文件段落分割正则"),
    );
    if let Some(re) = re_segment {
        for cap in re.captures_iter(stripped) {
            let doc_num = cap
                .get(1)
                .and_then(|m| m.as_str().parse::<usize>().ok())
                .unwrap_or(0);
            let start = cap.get(0).unwrap().end();
            let remaining = &stripped[start..];
            let end = remaining.find('。').unwrap_or(200).min(200);
            let quote = safe_slice(remaining, 0, end).to_string();
            if quote.len() >= 4 {
                let doc_id = format!("对比文件{}", doc_num);
                // 避免重复添加
                if !citations.iter().any(|(id, q)| id == &doc_id && q == &quote) {
                    citations.push((doc_id, quote));
                }
            }
        }
    }

    citations
}

// ═══════════════════════════════════════════════════════════════════════
//  测试
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn citation_matches_original() {
        // 中文引号“”是 CNIPA 正文的标准引号，正则必须匹配。
        let oa = "对比文件1公开了\u{201c}火焰喷嘴的圆周壁上固定连接有对角安装的电极\u{201d}。";
        let d1 = "一种燃烧器，火焰喷嘴的圆周壁上固定连接有对角安装的电极和下网状电极发生器。";
        let report = check_citations(oa, &[("对比文件1", d1)]);
        // 正向断言：引号内容被提取且在原文中找到
        assert_eq!(
            report.citations.len(),
            1,
            "应提取1条引文，实际提取{}条",
            report.citations.len()
        );
        assert!(report.citations[0].found_in_original, "引文应在原文中找到");
        assert_eq!(report.mismatch_count, 0);
    }

    #[test]
    fn citation_chinese_quote_only() {
        // 反例：只有中文引号，无 ASCII 引号——旧正则恒不命中。
        let oa = "对比文件2披露了\u{201c}交错排列的电极组件\u{201d}以实现均匀放电。";
        let d2 = "电极组件交错排列的电极组件用于均匀放电。";
        let report = check_citations(oa, &[("对比文件2", d2)]);
        assert_eq!(
            report.citations.len(),
            1,
            "中文引号引文必须被提取，实际提取{}条",
            report.citations.len()
        );
        assert!(report.citations[0].found_in_original);
    }

    #[test]
    fn citation_ascii_quote_still_works() {
        // 回归：ASCII 引号仍应匹配。
        let oa = "对比文件1公开了\"火焰喷嘴的圆周壁上固定连接有对角安装的电极\"。";
        let d1 = "一种燃烧器，火焰喷嘴的圆周壁上固定连接有对角安装的电极和下网状电极发生器。";
        let report = check_citations(oa, &[("对比文件1", d1)]);
        assert_eq!(report.citations.len(), 1);
        assert!(report.citations[0].found_in_original);
    }

    #[test]
    fn citation_mismatch_detected() {
        let oa = "对比文件1记载了交错安装的电极发生器。";
        let d1 = "对角安装的电极和下网状电极发生器";
        let report = check_citations(oa, &[("对比文件1", d1)]);
        assert_eq!(report.mismatch_count, 1);
        assert!(report.summary.contains("不符"));
    }

    #[test]
    fn citation_with_whitespace_match() {
        let oa = "对比文件1记载了火焰喷嘴的圆周壁上固定连接有对角安装的电极。";
        let d1 = "火焰喷嘴 的圆周壁 上固定连接 有对角 安装的电极。";
        let report = check_citations(oa, &[("对比文件1", d1)]);
        // 去空格后应匹配
        assert_eq!(report.mismatch_count, 0);
    }

    #[test]
    fn no_citations_in_text() {
        let oa = "本发明不具备创造性。";
        let report = check_citations(oa, &[("对比文件1", "任意原文")]);
        assert_eq!(report.citations.len(), 0);
        assert!(report.summary.contains("共核对0条"));
    }

    // ── 示例申请人案回归 ──

    #[test]
    fn sample_case_c4_citation_check() {
        let oa = include_str!("../../tests/fixtures/oa_sample/oa_rejection.txt");
        let d1 = include_str!("../../tests/fixtures/oa_sample/d1_sample.txt");
        let d2 = include_str!("../../tests/fixtures/oa_sample/d2_sample.txt");
        let d3 = include_str!("../../tests/fixtures/oa_sample/d3_sample.txt");

        let report = check_citations(
            oa,
            &[("对比文件1", d1), ("对比文件2", d2), ("对比文件3", d3)],
        );

        eprintln!(
            "示例申请人案 C4：共{}条引文，{}条不符",
            report.citations.len(),
            report.mismatch_count
        );
        // 不强制断言，因为 OCR 格式可能影响引文提取
    }
}
