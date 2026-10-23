//! 结论句切分与分类（ADR-3：句粒度 + 确定性启发式）。
//!
//! 分类代价不对称：把推论误判为事实 → 进核验 → 无引用 → 降级（偏保守，可接受）；
//! 把事实误判为推论 → 漏网（用高召回的事实模式表缓解，失败模式 F5）。

use crate::evidence_kernel::anchor;
use crate::evidence_kernel::model::{Claim, ClaimKind};

/// 事实断言的强信号（来源内容断言 / 区别-对比断言）。
const FACT_PATTERNS: &[&str] = &[
    "公开了",
    "公开有",
    "记载了",
    "记载有",
    "披露了",
    "描述了",
    "揭示了",
    "提出了",
    "区别在于",
    "区别技术特征",
    "特征在于",
    "不同于",
    "相比",
    "相对于现有技术",
    "相对于对比文件",
    "属于公知",
    "属于常规",
    "惯用手段",
    "已经存在",
    "已被",
    "解决了",
    "实现了",
    "达到了",
];

/// 法律论证句信号。
const LEGAL_PATTERNS: &[&str] = &[
    "不具备",
    "具有创造性",
    "有创造性",
    "无创造性",
    "新颖性",
    "显而易见",
    "非显而易见",
    "技术启示",
    "结合启示",
    "专利法",
    "审查指南",
    "应当",
    "不应被授予",
    "驳回",
];

/// 豁免句信号（元信息/建议/已降级标注）。
const META_PATTERNS: &[&str] = &[
    "建议",
    "下一步",
    "后续可以",
    "请注意",
    "待核实",
    "【未证实】",
    "本报告",
    "以下为",
];

/// 切分句子（CJK 句读 + 西文句读 + 行边界），返回 (句子, 在原文中的起始字节)。
/// 行也参与切分：Markdown 标题/列表行必须独立成句，不能与正文黏连。
pub fn split_sentences(text: &str) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut start = 0usize;
    for (idx, ch) in text.char_indices() {
        let is_terminator = matches!(ch, '。' | '！' | '？' | '!' | '?' | '；' | ';' | '\n');
        if is_terminator {
            // char_indices 的 idx 与 start 均在字符边界；get() 形态杜绝裸字节切片（#73）
            let seg = text.get(start..idx + ch.len_utf8()).unwrap_or_default();
            if seg.trim().chars().any(|c| !c.is_whitespace()) {
                out.push((seg.trim().to_string(), start));
            }
            start = idx + ch.len_utf8();
        }
    }
    let tail = text.get(start..).unwrap_or("");
    if tail.trim().chars().any(|c| !c.is_whitespace()) {
        out.push((tail.trim().to_string(), start));
    }
    // 不做短句合并：长度不是句界语义。短句（“见图1。”）由分类器的 <8 字 → Meta 豁免兜底，
    // 合并反而会吞掉合法短句（“第一句。第二句！”被并成一句的事故）。
    out
}

/// 句子分类（确定性）。
pub fn classify_sentence(sentence: &str) -> ClaimKind {
    let s = sentence.trim();
    let chars = s.chars().count();
    // 标题/极短句/纯符号 → 豁免
    if chars < 8 || s.starts_with('#') || s.starts_with('|') || s.starts_with("--") {
        return ClaimKind::Meta;
    }
    // 【待核实】是作者自认未核 → 豁免；【未证实】是内核降级标注，保持原分类
    // （核验计数必须继续计入——否则降级会把幻觉率洗成 0）
    if s.contains("【待核实】") {
        return ClaimKind::Meta;
    }
    // 建议类元信息 → 豁免
    if META_PATTERNS.iter().any(|p| s.contains(p)) && !FACT_PATTERNS.iter().any(|p| s.contains(p)) {
        return ClaimKind::Meta;
    }
    // 含锚点或专利号 → 至少是事实句（引用即事实主张）
    let has_evidence_token =
        !anchor::extract_evidence_refs(s).is_empty() || contains_patent_number(s);
    if has_evidence_token || FACT_PATTERNS.iter().any(|p| s.contains(p)) {
        // 但若同时是强法律判断句，归法律论证（同样需要证据，进入 grounding 分母）
        if LEGAL_PATTERNS.iter().any(|p| s.contains(p)) && !has_evidence_token {
            return ClaimKind::LegalArgument;
        }
        return ClaimKind::FactFromSource;
    }
    if LEGAL_PATTERNS.iter().any(|p| s.contains(p)) {
        return ClaimKind::LegalArgument;
    }
    ClaimKind::AnalysisInference
}

/// 专利号形状：CN12345678A/B/U、US1234567、EP1234567、WO2024/12345。
fn contains_patent_number(s: &str) -> bool {
    let re = crate::evidence_kernel::classifier::patent_number_re();
    re.is_match(s)
}

fn patent_number_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(?:CN\d{7,10}[A-Z]?|US\d{7,8}|EP\d{6,7}|WO\d{4}/\d{4,6})")
            .unwrap_or_else(|e| panic!("patent_number_re 静态常量失败即 bug: {e}"))
    })
}

/// 把一段文本切成 claim 列表（句子 → claim，锚点附到所在句）。
pub fn extract_claims(text: &str) -> Vec<Claim> {
    let mut claims = Vec::new();
    for (idx, (sentence, _pos)) in split_sentences(text).into_iter().enumerate() {
        let kind = classify_sentence(&sentence);
        let evidence = if kind == ClaimKind::Meta {
            Vec::new()
        } else {
            anchor::extract_evidence_refs(&sentence)
        };
        claims.push(Claim {
            id: format!("C{}", idx + 1),
            sentence_index: idx,
            text: sentence,
            kind,
            evidence,
        });
    }
    claims
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_cjk_and_ascii_terminators() {
        let text = "第一句。第二句！第三句?第四句;还有尾巴";
        let ss = split_sentences(text);
        assert_eq!(ss.len(), 5);
        assert!(ss[0].0.starts_with("第一句"));
        assert!(ss[4].0.contains("尾巴"));
    }

    #[test]
    fn tiny_fragments_stay_separate_but_classified_meta() {
        // 短句不吞并：独立成句，但由 <8 字规则归 Meta 豁免（不进 grounding 分母）
        let text = "见图1。其特征在于温控风扇结构。";
        let ss = split_sentences(text);
        assert_eq!(ss.len(), 2, "短句应独立: {ss:?}");
        assert_eq!(classify_sentence(&ss[0].0), ClaimKind::Meta);
        assert_eq!(classify_sentence(&ss[1].0), ClaimKind::FactFromSource);
    }

    #[test]
    fn classifies_fact_vs_inference_vs_meta() {
        assert_eq!(
            classify_sentence("D1公开了温控风扇的结构与控制逻辑。"),
            ClaimKind::FactFromSource
        );
        assert_eq!(
            classify_sentence("该方案相比现有技术区别在于散热路径重新布局。"),
            ClaimKind::FactFromSource
        );
        // 建议句（无事实模式）→ Meta
        assert_eq!(
            classify_sentence("建议后续补充实验数据以支持创造性主张。"),
            ClaimKind::Meta
        );
        // 纯推论（无事实模式、无锚点、无法律信号）
        assert_eq!(
            classify_sentence("从工程视角看这一路线仍具备相当的探索价值。"),
            ClaimKind::AnalysisInference
        );
        // 法律论证
        assert_eq!(
            classify_sentence("本领域技术人员在此基础上得到该方案是显而易见的。"),
            ClaimKind::LegalArgument
        );
        // 标题豁免
        assert_eq!(classify_sentence("## 分析结论"), ClaimKind::Meta);
    }

    #[test]
    fn patent_number_token_counts_as_fact() {
        assert_eq!(
            classify_sentence("该结构在 CN12345678A 中已有明确记载与说明。"),
            ClaimKind::FactFromSource
        );
    }

    #[test]
    fn anchors_attach_to_their_sentence() {
        let text = "D1公开了风扇结构 [D1:第0035段]。这是推论句，没有引用。D2 记载了转速逻辑 [D2:权利要求1]。";
        let claims = extract_claims(text);
        assert_eq!(claims.len(), 3);
        assert_eq!(claims[0].evidence.len(), 1);
        assert_eq!(claims[1].evidence.len(), 0);
        assert_eq!(claims[2].evidence.len(), 1);
        assert_eq!(claims[1].kind, ClaimKind::AnalysisInference);
    }
}
