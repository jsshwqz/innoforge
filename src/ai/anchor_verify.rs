//! 锚点核验（纯函数）：答复书里的 `[D1:权利要求2]` / `[D1:第0035段]` 是否**精确**指向
//! 检索到的分块元数据（issue #79）。
//!
//! 为什么不能"取锚点里的数字、看语料里有没有这个数字"：
//! 那是**假绿机器**——`[D1:第2段]` 会命中任何含字符 `2` 的语料。本模块的三条硬约束：
//!
//! 1. **三元组精确相等**：`(doc_id, kind, number)` 三者必须同时命中，不做任何子串包含；
//! 2. **数字按数值比较**：`第0035段` 与 `第35段` 等价，但 `第35段` ≠ `第003段`；
//! 3. **"解析不出来"与"解析出来但匹配不上"分开统计**：`unparsed` 不进
//!    `hallucination_rate` 的分母（分母只算可解析的），也不许用"没有可匹配对象"默认通过。
//!
//! 生产接线依赖检索分块的 `section_tag`（issue #78）；本模块只做判据本身，输入是可注入的
//! [`ChunkIndex`]，因此可以在测试里用合成分块自证"抓得到"。

use std::collections::HashSet;

/// 锚点指向的层级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnchorKind {
    /// `权利要求N`
    Claim,
    /// `第N段`
    Paragraph,
}

/// 一个锚点：`(doc_id, kind, number)`。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Anchor {
    pub doc_id: String,
    pub kind: AnchorKind,
    pub number: u32,
}

impl Anchor {
    pub fn claim(doc_id: &str, n: u32) -> Self {
        Self {
            doc_id: doc_id.to_string(),
            kind: AnchorKind::Claim,
            number: n,
        }
    }
    pub fn paragraph(doc_id: &str, n: u32) -> Self {
        Self {
            doc_id: doc_id.to_string(),
            kind: AnchorKind::Paragraph,
            number: n,
        }
    }
}

/// 可匹配的分块元数据集合（检索结果的 section_tag 归一化后的形态）。
#[derive(Debug, Clone, Default)]
pub struct ChunkIndex {
    entries: HashSet<(String, AnchorKind, u32)>,
}

impl ChunkIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, anchor: Anchor) {
        self.entries
            .insert((anchor.doc_id, anchor.kind, anchor.number));
    }

    pub fn from_chunks(chunks: impl IntoIterator<Item = Anchor>) -> Self {
        let mut idx = Self::new();
        for c in chunks {
            idx.insert(c);
        }
        idx
    }

    /// 精确命中：doc_id + kind + number 三者都相等。
    pub fn contains_exact(&self, anchor: &Anchor) -> bool {
        self.entries
            .contains(&(anchor.doc_id.clone(), anchor.kind, anchor.number))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// 核验报告。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnchorReport {
    /// 精确命中的锚点
    pub verified: Vec<Anchor>,
    /// 能解析、但精确匹配失败的锚点（**这才是幻觉候选**）
    pub unverified: Vec<Anchor>,
    /// 解析不出来的锚点原文（不进 hallucination_rate 分母）
    pub unparsed: Vec<String>,
}

impl AnchorReport {
    /// 可解析锚点里"匹配不上"的比例；没有可解析锚点时返回 0（并**不表示通过**——
    /// 通过与否要看 [`AnchorReport::has_unverified`]，别拿这个数当绿灯）。
    pub fn hallucination_rate(&self) -> f64 {
        let denom = self.verified.len() + self.unverified.len();
        if denom == 0 {
            0.0
        } else {
            self.unverified.len() as f64 / denom as f64
        }
    }

    pub fn has_unverified(&self) -> bool {
        !self.unverified.is_empty()
    }

    /// 一句话口径（进日志/meta）：三个数分开报，不合并。
    pub fn summary(&self) -> String {
        format!(
            "锚点核验：verified={} unverified={} unparsed={} hallucination_rate={:.3}",
            self.verified.len(),
            self.unverified.len(),
            self.unparsed.len(),
            self.hallucination_rate()
        )
    }
}

/// 从文本里抽出 `[doc:主张]` 形态的锚点。
///
/// 返回 `(可解析锚点, 不可解析锚点原文)`。
pub fn parse_anchors(text: &str) -> (Vec<Anchor>, Vec<String>) {
    let mut parsed = Vec::new();
    let mut unparsed = Vec::new();
    for raw in anchor_bodies(text) {
        match parse_one(&raw) {
            Some(a) => parsed.push(a),
            None => unparsed.push(raw),
        }
    }
    (parsed, unparsed)
}

/// 核验：解析 → 逐条精确匹配。
///
/// `index` 为空时，所有**可解析**锚点都会落进 `unverified`——不允许"没有可匹配对象"
/// 默认通过（issue #79 的补充条款）。
pub fn verify_anchors(text: &str, index: &ChunkIndex) -> AnchorReport {
    let (parsed, unparsed) = parse_anchors(text);
    let mut report = AnchorReport {
        unparsed,
        ..AnchorReport::default()
    };
    for anchor in parsed {
        if index.contains_exact(&anchor) {
            report.verified.push(anchor);
        } else {
            report.unverified.push(anchor);
        }
    }
    report
}

/// 取出所有 `[...]` 的内容（不含方括号）。嵌套/未闭合的一律忽略——调用方拿不到
/// 完整括号就不该猜。
fn anchor_bodies(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        let after = &rest[start + 1..];
        let Some(end) = after.find(']') else {
            break;
        };
        out.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    out
}

/// 解析单条锚点内容：`doc_id:权利要求N` / `doc_id:第N段`。
///
/// 只认这两种形态；`doc_id` 里不允许出现冒号（避免 `[见:某处:第2段]` 这种含糊写法被猜成 D1）。
fn parse_one(body: &str) -> Option<Anchor> {
    let (doc, target) = body.split_once(':').or_else(|| body.split_once('：'))?;
    let doc_id = doc.trim();
    if doc_id.is_empty() || doc_id.contains(':') || doc_id.contains('：') {
        return None;
    }
    let target = target.trim();
    if let Some(rest) = target.strip_prefix("权利要求") {
        let n = parse_number(rest)?;
        return Some(Anchor::claim(doc_id, n));
    }
    if let Some(rest) = target.strip_prefix('第') {
        let n = parse_number(rest.strip_suffix('段')?)?;
        return Some(Anchor::paragraph(doc_id, n));
    }
    None
}

/// 数字：允许前导零（`0035` → 35），但不允许夹杂其它字符。
fn parse_number(s: &str) -> Option<u32> {
    let t = s.trim();
    if t.is_empty() || !t.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    t.parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(chunks: &[Anchor]) -> ChunkIndex {
        ChunkIndex::from_chunks(chunks.iter().cloned())
    }

    #[test]
    fn real_anchor_verifies() {
        let idx = index(&[
            Anchor::claim("D1", 2),
            Anchor::paragraph("D1", 35),
            Anchor::paragraph("D2", 7),
        ]);
        let text = "参见 [D1:权利要求2] 与 [D1:第35段]，另见 [D2:第7段]。";
        let r = verify_anchors(text, &idx);
        assert_eq!(r.verified.len(), 3, "{:?}", r);
        assert!(r.unverified.is_empty());
        assert!(r.unparsed.is_empty());
        assert_eq!(r.hallucination_rate(), 0.0);
    }

    #[test]
    fn fabricated_paragraph_is_unverified() {
        let idx = index(&[Anchor::paragraph("D1", 35)]);
        let r = verify_anchors("参见 [D1:第9999段]。", &idx);
        assert_eq!(
            r.unverified.len(),
            1,
            "编造的段落号必须报 unverified：{:?}",
            r
        );
        assert!(r.verified.is_empty());
        assert_eq!(r.hallucination_rate(), 1.0);
    }

    #[test]
    fn zero_padded_number_matches_by_value() {
        let idx = index(&[Anchor::paragraph("D1", 35)]);
        let r = verify_anchors("[D1:第0035段]", &idx);
        assert_eq!(r.verified.len(), 1, "前导零应按数值比较：{:?}", r);
    }

    #[test]
    fn kind_mismatch_is_unverified() {
        // 语料里只有"权利要求2"，没有"第2段" —— 层级不同不能互相顶替。
        let idx = index(&[Anchor::claim("D1", 2)]);
        let r = verify_anchors("[D1:第2段]", &idx);
        assert_eq!(r.unverified.len(), 1, "层级不同必须判不匹配：{:?}", r);
    }

    #[test]
    fn doc_mismatch_is_unverified() {
        let idx = index(&[Anchor::paragraph("D2", 2)]);
        let r = verify_anchors("[D1:第2段]", &idx);
        assert_eq!(r.unverified.len(), 1, "文档号不同必须判不匹配：{:?}", r);
    }

    /// 回归：记录"数字包含"那套设计为什么被拒——它就是一台假绿机器。
    #[test]
    fn digit_inclusion_would_be_a_false_green() {
        let idx = index(&[Anchor::claim("D1", 2)]);
        let text = "[D1:第2段]";
        // 朴素实现：把语料拼成字符串，锚点里的数字出现在里面就算过。
        let corpus_text = "D1 权利要求2"; // 语料里**没有**第 2 段
        let naive = corpus_text.contains(&extract_number_string(text));
        assert!(
            naive,
            "本用例的前提：朴素'数字包含'实现会判过（若这里 false，说明用例失效）"
        );
        // 精确实现：三元组不同 ⇒ 不匹配
        let r = verify_anchors(text, &idx);
        assert_eq!(r.unverified.len(), 1, "精确实现必须拒绝：{:?}", r);
    }

    #[test]
    fn empty_index_reports_unverified_not_pass() {
        let idx = ChunkIndex::new();
        let r = verify_anchors("参见 [D1:第2段] 与 [D1:权利要求1]。", &idx);
        assert_eq!(
            r.unverified.len(),
            2,
            "没有可匹配对象时不许默认通过：{:?}",
            r
        );
        assert_eq!(r.hallucination_rate(), 1.0);
        assert!(r.has_unverified());
    }

    #[test]
    fn unparsable_anchor_is_counted_separately() {
        let idx = index(&[Anchor::paragraph("D1", 2)]);
        let r = verify_anchors("[D1:见附件] 与 [D1:第2段]", &idx);
        assert_eq!(r.unparsed.len(), 1, "{:?}", r);
        assert_eq!(r.verified.len(), 1);
        assert_eq!(r.unverified.len(), 0);
        // 分母只算可解析的：1 verified ⇒ 0.0，而不是把 unparsed 混进来
        assert_eq!(r.hallucination_rate(), 0.0);
    }

    #[test]
    fn no_anchor_text_is_empty_report() {
        let r = verify_anchors("这段话里没有任何锚点。", &ChunkIndex::new());
        assert_eq!(r.verified.len() + r.unverified.len() + r.unparsed.len(), 0);
        assert!(!r.has_unverified());
    }

    /// 朴素实现会这样取数字：只看冒号之后的"目标"部分（第2段 → "2"），
    /// 然后问"语料里有没有这个数字"。`D1:权利要求2` 里的 2 会让 `[D1:第2段]` 蒙混过关。
    fn extract_number_string(text: &str) -> String {
        let target = text.split_once(':').map(|(_, t)| t).unwrap_or(text);
        target.chars().filter(|c| c.is_ascii_digit()).collect()
    }
}
