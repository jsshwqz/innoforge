//! 引用核验器：锚点三元组精确匹配 + 引文归一化匹配（ADR-1/ADR-4）。
//!
//! 两条铁律（不变量 I1，负控 NC1 盯守）：
//! 1. 锚点 = (doc_id, kind, number) 三元组在索引中**精确相等**（整数判等，零填充自然等价）；
//!    全链禁止对编号用子串包含判据（issue #79 否决的“数字包含即通过”假绿机器）。
//! 2. 引文 = 归一化后逐字子串优先，3-gram Jaccard ≥ 0.85 近似兜底（分数明示）；
//!    两者皆败 = QuoteMismatch，绝不放行。
//!
//! 纯函数、零 AI、零网络（I5）。

use crate::evidence_kernel::model::{
    Anchor, EvidenceRef, LocatorKind, SourceCorpus, SourceSection, VerifyStatus,
};
use std::collections::{HashMap, HashSet};

/// 语料索引：doc_id → kind → number → section。锚点判等 = 三级键精确查找（O(1)）。
pub struct CorpusIndex {
    map: HashMap<(String, LocatorKind), HashMap<u32, SourceSection>>,
    section_count: usize,
}

impl CorpusIndex {
    pub fn build(corpus: &SourceCorpus) -> Self {
        let mut map: HashMap<(String, LocatorKind), HashMap<u32, SourceSection>> = HashMap::new();
        let mut count = 0usize;
        for doc in &corpus.docs {
            for sec in &doc.sections {
                map.entry((doc.doc_id.clone(), sec.kind))
                    .or_default()
                    .insert(sec.number, sec.clone());
                count += 1;
            }
        }
        Self {
            map,
            section_count: count,
        }
    }

    pub fn section_count(&self) -> usize {
        self.section_count
    }

    /// 锚点精确匹配。注意：这是 HashMap 键查找，不是文本 contains。
    fn resolve(&self, anchor: &Anchor) -> Option<&SourceSection> {
        self.map
            .get(&(anchor.doc_id.clone(), anchor.kind))
            .and_then(|m| m.get(&anchor.number))
    }

    /// 已被引用过的 section_id 集合（coverage 用）。
    pub fn all_section_ids(&self) -> Vec<String> {
        self.map
            .values()
            .flat_map(|m| m.values().map(|s| s.section_id.clone()))
            .collect()
    }
}

/// 核验单条证据引用。
pub fn verify_ref(ev: &EvidenceRef, index: &CorpusIndex) -> VerifyStatus {
    let Some(anchor) = &ev.anchor else {
        return VerifyStatus::UnparsedAnchor;
    };
    let Some(section) = index.resolve(anchor) else {
        return VerifyStatus::AnchorNotFound;
    };
    if ev.quote.trim().is_empty() {
        return VerifyStatus::AnchorOnly {
            section_id: section.section_id.clone(),
        };
    }
    let norm_quote = normalize(&ev.quote);
    let norm_text = normalize(&section.text);
    if norm_text.contains(&norm_quote) {
        return VerifyStatus::VerifiedExact {
            section_id: section.section_id.clone(),
        };
    }
    // 近似匹配：先做「去标点宽松归一化」（排版/标点差异不是内容差异），
    // 再滑窗找原文中与引文最像的等长跨度——整段 Jaccard 会因段落更长而系统性低估
    let loose_quote = normalize_loose(&norm_quote);
    let loose_text = normalize_loose(&norm_text);
    let score = best_window_jaccard(&loose_quote, &loose_text);
    if !loose_quote.is_empty() && loose_text.contains(&loose_quote) {
        return VerifyStatus::VerifiedApprox {
            section_id: section.section_id.clone(),
            score: score.max(0.99),
        };
    }
    if score >= APPROX_THRESHOLD {
        return VerifyStatus::VerifiedApprox {
            section_id: section.section_id.clone(),
            score,
        };
    }
    VerifyStatus::QuoteMismatch { best_score: score }
}

/// 宽松归一化：在 normalize 基础上去掉全部标点（只留字母/数字/CJK）。
/// 用于近似层——"引文与原文差一个逗号"应算排版差异而非内容差异。
pub fn normalize_loose(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(c))
        .collect()
}

/// 近似引文匹配阈值（ADR-4：复用 golden_regression 的 3-gram 家族，分数必须明示）。
pub const APPROX_THRESHOLD: f64 = 0.85;

/// 文本归一化：全/半角统一 + 去全部空白 + 引号/常用标点统一 + ASCII 小写。
/// 目标：让“引文逐字存在于原文”的判定对排版差异鲁棒，但不改变字符内容本身。
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        let ch = match ch {
            // 全角 ASCII → 半角
            '０'..='９' => {
                out.push((ch as u32 - '０' as u32 + '0' as u32) as u8 as char);
                continue;
            }
            'Ａ'..='Ｚ' => {
                out.push((ch as u32 - 'Ａ' as u32 + 'A' as u32) as u8 as char);
                continue;
            }
            'ａ'..='ｚ' => {
                out.push((ch as u32 - 'ａ' as u32 + 'a' as u32) as u8 as char);
                continue;
            }
            // 引号统一为直引号
            '“' | '”' | '「' | '」' | '『' | '』' | '‘' | '’' | '"' => {
                out.push('"');
                continue;
            }
            // 常用中文标点 → 西文等价
            '，' => {
                out.push(',');
                continue;
            }
            '。' => {
                out.push('.');
                continue;
            }
            '；' => {
                out.push(';');
                continue;
            }
            '：' => {
                out.push(':');
                continue;
            }
            '（' | '）' => {
                out.push(if ch == '（' { '(' } else { ')' });
                continue;
            }
            '【' | '】' => {
                out.push(if ch == '【' { '[' } else { ']' });
                continue;
            }
            '　' => continue, // 全角空格
            _ => ch,
        };
        if ch.is_whitespace() {
            continue;
        }
        for lower in ch.to_lowercase() {
            out.push(lower);
        }
    }
    out
}

/// 字符 3-gram Jaccard（整串对整串，回归工具用途）。
pub fn trigram_jaccard(a: &str, b: &str) -> f64 {
    ngram_jaccard(a, b, 3)
}

/// 滑窗最佳跨度匹配：在 `haystack` 中滑动与 `needle` 等长（±20%）的窗口，
/// 返回最佳窗口的 3-gram Jaccard。引文"近似存在于段落某处"的正确语义。
fn best_window_jaccard(needle: &str, haystack: &str) -> f64 {
    let n_chars: Vec<char> = needle.chars().collect();
    let h_chars: Vec<char> = haystack.chars().collect();
    if n_chars.len() < 2 || h_chars.len() < 2 {
        return 0.0;
    }
    let n_grams: HashSet<Vec<char>> = n_chars
        .windows(3.min(n_chars.len()))
        .map(<[char]>::to_vec)
        .collect();
    let mut best = 0.0f64;
    let win_min = (n_chars.len() as f64 * 0.8).floor() as usize;
    let win_max = (n_chars.len() as f64 * 1.2).ceil() as usize;
    for win in win_min.max(2)..=win_max.min(h_chars.len()) {
        for start in 0..=(h_chars.len() - win) {
            let window: Vec<char> = h_chars[start..start + win].to_vec();
            let w_grams: HashSet<Vec<char>> =
                window.windows(3.min(win)).map(<[char]>::to_vec).collect();
            let inter = n_grams.intersection(&w_grams).count() as f64;
            let union = n_grams.union(&w_grams).count() as f64;
            if union > 0.0 {
                best = best.max(inter / union);
            }
        }
    }
    best
}

fn ngram_jaccard(a: &str, b: &str, n: usize) -> f64 {
    let grams = |s: &str| -> std::collections::HashSet<Vec<char>> {
        let chars: Vec<char> = s.chars().collect();
        if chars.len() < 2 {
            return HashSet::new();
        }
        let size = n.min(chars.len());
        chars.windows(size).map(<[char]>::to_vec).collect()
    };
    let ga = grams(a);
    let gb = grams(b);
    if ga.is_empty() || gb.is_empty() {
        return 0.0;
    }
    let inter = ga.intersection(&gb).count() as f64;
    let union = ga.union(&gb).count() as f64;
    inter / union
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus() -> SourceCorpus {
        SourceCorpus {
            docs: vec![crate::evidence_kernel::model::SourceDoc {
                doc_id: "D1".into(),
                kind: "patent".into(),
                title: "对比文件1".into(),
                sections: vec![
                    SourceSection {
                        section_id: "D1:para:35".into(),
                        kind: LocatorKind::Paragraph,
                        number: 35,
                        text: "当温度超过阈值时，控制器自动启动风扇并调节转速。".into(),
                    },
                    SourceSection {
                        section_id: "D1:claim:2".into(),
                        kind: LocatorKind::Claim,
                        number: 2,
                        text: "根据权利要求1所述的装置，其特征在于，风扇转速可自动调节。".into(),
                    },
                ],
            }],
        }
    }

    fn ev(raw: &str, quote: &str) -> EvidenceRef {
        EvidenceRef {
            raw: raw.into(),
            anchor: parse(raw),
            quote: quote.into(),
        }
    }

    fn parse(raw: &str) -> Option<Anchor> {
        crate::evidence_kernel::anchor::parse_anchor(raw)
    }

    #[test]
    fn exact_quote_verifies() {
        let idx = CorpusIndex::build(&corpus());
        let s = verify_ref(
            &ev("[D1:第0035段]", "当温度超过阈值时，控制器自动启动风扇"),
            &idx,
        );
        assert!(
            matches!(s, VerifyStatus::VerifiedExact { .. }),
            "应逐字通过: {s:?}"
        );
    }

    #[test]
    fn formatting_noise_still_exact() {
        // 引文带全角空格/换行/全角标点，归一化后仍逐字命中
        let idx = CorpusIndex::build(&corpus());
        let s = verify_ref(
            &ev("[D1:第0035段]", "当温度超过阈值时 ，控制器自动\n启动风扇"),
            &idx,
        );
        assert!(matches!(s, VerifyStatus::VerifiedExact { .. }), "got {s:?}");
    }

    #[test]
    fn fabricated_anchor_fails() {
        let idx = CorpusIndex::build(&corpus());
        let s = verify_ref(&ev("[D1:第9999段]", "任意编造引文"), &idx);
        assert!(matches!(s, VerifyStatus::AnchorNotFound), "got {s:?}");
    }

    #[test]
    fn fabricated_quote_fails_even_with_wrong_doc_attribution() {
        // 锚点命中 D1:claim:2，但引文其实来自别的文档 → QuoteMismatch
        let idx = CorpusIndex::build(&corpus());
        let s = verify_ref(&ev("[D2:权利要求9]", "完全无关的引文内容"), &idx);
        assert!(matches!(s, VerifyStatus::AnchorNotFound));
        let s = verify_ref(
            &ev("[D1:权利要求2]", "本申请解决了散热效率低的技术问题"),
            &idx,
        );
        assert!(matches!(s, VerifyStatus::QuoteMismatch { .. }), "got {s:?}");
    }

    /// 负控 NC1 的核心断言形态：数字包含绝不放行。
    /// 引文 "35" 不得因原文含 "35" 字样而通过；段落 35 的引文不得在段落 99 命中。
    #[test]
    fn digit_substring_never_passes() {
        let idx = CorpusIndex::build(&corpus());
        // 引文只是一个数字片段
        let s = verify_ref(&ev("[D1:第0035段]", "0035"), &idx);
        assert!(
            !s.verified_weight().is_some_and(|w| w >= 1.0),
            "数字包含不得计为逐字通过: {s:?}"
        );
        // 锚点 350 不存在：不能因 35 存在而子串放行（整数键查找天然拒绝）
        let s = verify_ref(&ev("[D1:第350段]", "当温度超过阈值时"), &idx);
        assert!(matches!(s, VerifyStatus::AnchorNotFound), "got {s:?}");
    }

    #[test]
    fn approx_quote_with_threshold() {
        let idx = CorpusIndex::build(&corpus());
        // 轻微改写（去几个词）——近似分数应达标或明确 mismatch，不允许模棱两可
        let s = verify_ref(
            &ev(
                "[D1:第0035段]",
                "当温度超过阈值时控制器自动启动风扇并调节转速",
            ),
            &idx,
        );
        match s {
            VerifyStatus::VerifiedApprox { score, .. } => assert!(score >= APPROX_THRESHOLD),
            VerifyStatus::VerifiedExact { .. } => {}
            other => panic!("近似改写应通过或明确失败: {other:?}"),
        }
    }

    #[test]
    fn anchor_only_when_no_quote() {
        let idx = CorpusIndex::build(&corpus());
        let s = verify_ref(&ev("[D1:第0035段]", ""), &idx);
        assert!(
            matches!(s, VerifyStatus::AnchorOnly { .. }),
            "无引文时降级为仅锚点: {s:?}"
        );
    }

    #[test]
    fn unparsed_anchor_reported() {
        let idx = CorpusIndex::build(&corpus());
        let s = verify_ref(&ev("不存在的格式", "x"), &idx);
        assert!(matches!(s, VerifyStatus::UnparsedAnchor));
    }

    #[test]
    fn zero_padding_equivalence_in_index() {
        let idx = CorpusIndex::build(&corpus());
        // 0035 与 35 是同一键
        let a1 = Anchor {
            doc_id: "D1".into(),
            kind: LocatorKind::Paragraph,
            number: 35,
        };
        assert!(idx.resolve(&a1).is_some());
    }
}
