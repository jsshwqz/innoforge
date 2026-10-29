//! 证据表条目的**段落级锚点**：定位 → 校验 → 回写，全部由 Rust 做，不信模型自报。
//!
//! # 缺口
//! `POST /api/ai/oa-collect-evidence` 的 `source` 字段一直是模型自己写的字符串
//! （`src/ai/evidence.rs` 里 `item["source"].as_str().unwrap_or("未知来源")`），
//! Rust 从未校验。v6 真跑 17 条证据里**带段落号锚点的是 0 条**：模型写的是
//! "说明书"/"权利要求书"这种文档级措辞，复核人要按一条证据去原文里找位置，
//! 10 秒内根本找不到——而证据表的价值恰恰是"可翻回原文"。
//!
//! # 本层做法
//! 1. [`build_index`]：把本专利（说明书 / 权利要求）、对比文件、审查意见切成
//!    **带编号的段落**，只按输入文本自身的 `[00xx]` 标记或空行分段，不内置任何案件词。
//! 2. [`ParagraphIndex::locate`]：拿证据正文（去空格后）在段落里做**整串包含**判定；
//!    模型爱用省略号拼接引文，允许"各片段都落在同一段"这种放宽，
//!    但**不做前缀/模糊匹配**——那等于凭空造锚点。
//! 3. [`anchor_items`]：定位成功就把 `source` 改写成 `evidence_kernel::anchor::format_anchor`
//!    的规范形态（`[说明书:第0027段]`）；定位失败**保留模型原话并前置** [`UNLOCATED`]，
//!    让它成为一种可数、可见的失败，而不是伪装成"来源已填"。
//! 4. 模型自己报的锚点与定位结果不一致时按定位结果改写，并单独计 `corrected`——
//!    "报了段号但那段里没有这句话"是最危险的一种假证据，必须和"没报段号"分开统计。
//!
//! 锚点三元组复用 `evidence_kernel::model::Anchor`，不另造一套锚点词汇；
//! 解析失败与校验失败分开计数（沿用 #79 的口径）。

use crate::ai::evidence::Evidence;
use crate::ai::whitespace_search::strip_whitespace;
use crate::evidence_kernel::anchor::format_anchor;
use crate::evidence_kernel::model::{Anchor, LocatorKind};
use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

/// 定位失败的前置标记：与"模型写了个来源但没人核对过"区分开。
pub const UNLOCATED: &str = "【未定位】";

/// 短于该长度的证据正文不做定位——两个字的短语在全文里到处都能"命中"，
/// 那种命中是噪声，写进锚点列等于骗人。
const MIN_LOCATABLE_CHARS: usize = 8;

/// 说明书段落号：`[0027]`、`【0027】`、`〔0027〕`（1~4 位，允许前导零）。
fn para_marker_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[\[【〔](0*[1-9][0-9]{0,3})[\]】〕]").expect("evidence_anchor: 段落号正则")
    })
}

/// 权利要求项号：行首 `1.`、`2、`、`权利要求3`。
fn claim_head_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)^\s*(?:权利要求\s*)?([1-9][0-9]{0,2})\s*[.、．]\s*")
            .expect("evidence_anchor: 权项号正则")
    })
}

/// 章节标题行：官方文书的分节名，全是通用词，不含任何案件专有信息。
///
/// 为什么需要它：`[00xx]` 段落只覆盖说明书正文，**最后一个段号的正文不能一路吃到文本末尾**——
/// 实测把 `权利要求书` 整节吞进 `[0003]` 段之后，跨段拼接的引文会被"定位"到那一段上，
/// 权项原文也会被判成说明书段落（两个单测当场把这条打死）。
///
/// 只用基础语法：`regex` crate 不支持 look-around（见 `evidence_kernel::anchor` 的同一教训）。
/// 行边界由 `^\s*…\s*$` 保证——正文句子"说明书…如何如何"因为后面还有字，不会命中。
fn section_heading_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?m)^\s*(?:权利要求书|说明书|具体实施方式|技术领域|背景技术|附图说明|发明内容|摘要)\s*$",
        )
        .expect("evidence_anchor: 章节标题正则")
    })
}

/// 从 `from` 之后出现的第一个章节标题位置（没有就返回文本末尾）。
fn next_heading_after(text: &str, from: usize) -> usize {
    section_heading_re()
        .find_iter(text)
        .map(|m| m.start())
        .find(|p| *p >= from)
        .unwrap_or(text.len())
}

/// 「权利要求书」这一节的字节区间。没有这一节标题就退回整篇——宁可多扫一点，
/// 也不要因为标题写法不同就一条权项都不索引。
fn claims_section(text: &str) -> (usize, usize) {
    let Some(hit) = section_heading_re()
        .find_iter(text)
        .find(|m| m.as_str().contains("权利要求书"))
    else {
        return (0, text.len());
    };
    let start = hit.end();
    (start, next_heading_after(text, start))
}

/// 一个可定位的段落。
#[derive(Debug, Clone)]
struct Entry {
    anchor: Anchor,
    /// 去空格后的段文本，匹配用它，避免提取件里的排版空格干扰
    normalized: String,
}

/// 段落索引（说明书 / 权利要求 / 每份对比文件 / 审查意见）。
#[derive(Debug, Clone, Default)]
pub struct ParagraphIndex {
    entries: Vec<Entry>,
}

impl ParagraphIndex {
    /// 段落总数（报告与测试用）。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 索引是否为空（三份材料都没内容时）。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 把证据正文定位到一个段落。返回该段落的锚点。
    ///
    /// 判定口径见 [`content_matches`]。命不中就是命不中，**不猜**。
    pub fn locate(&self, content: &str) -> Option<&Anchor> {
        self.entries
            .iter()
            .find(|e| content_matches(&e.normalized, content))
            .map(|e| &e.anchor)
    }

    /// 模型自报的锚点段落里**是否确有**这段正文（校验用，与定位共用同一口径）。
    pub fn contains(&self, anchor: &Anchor, content: &str) -> bool {
        self.entries
            .iter()
            .any(|e| &e.anchor == anchor && content_matches(&e.normalized, content))
    }
}

/// 正文与段落的匹配口径（唯一实现，`locate` 与 `contains` 共用，防止两处判定漂移）：
/// 1. 正文去空格后完整包含于段落 ⇒ 命中；
/// 2. 正文含省略号（`…` / `...` / `——`）时，切出的**每个 ≥8 字片段**都落在该段 ⇒ 命中
///    （模型爱把两处原文用省略号连成一条"引文"，这仍然是可翻回原文的）；
/// 3. 正文不足 8 字 ⇒ 不判命中（两三个字的短语全文到处都在，那种命中是噪声）。
/// **不做前缀匹配、不做相似度匹配**——锚点的意义就是能翻回原文。
fn content_matches(normalized_entry: &str, content: &str) -> bool {
    let Some(needle) = normalize(content) else {
        return false;
    };
    if normalized_entry.contains(&needle) {
        return true;
    }
    let fragments = split_quotation_fragments(&needle);
    fragments.len() >= 2 && fragments.iter().all(|f| normalized_entry.contains(f))
}

fn normalize(text: &str) -> Option<String> {
    let n = strip_whitespace(text).to_lowercase();
    if n.chars().count() < MIN_LOCATABLE_CHARS {
        None
    } else {
        Some(n)
    }
}

/// 省略号切分后的可用片段（≥ 最小可定位长度）。
fn split_quotation_fragments(hay: &str) -> Vec<String> {
    // 按 `…`、`...`、`——` 三种模型常用省略写法切
    let mut parts: Vec<&str> = vec![hay];
    for sep in ["...", "——"] {
        let mut next = Vec::new();
        for p in &parts {
            next.extend(p.split(sep));
        }
        parts = next;
    }
    let mut out = Vec::new();
    for p in parts.iter().flat_map(|s| s.split('…')) {
        let t = p.trim_matches(|c: char| c == '，' || c == ',' || c.is_whitespace());
        if t.chars().count() >= MIN_LOCATABLE_CHARS {
            out.push(t.to_string());
        }
    }
    out
}

/// 说明书/对比文件 doc_id：本专利固定叫 `说明书`，对比文件用**输入里自带的**标记
/// （`D1` 或公开号），绝不从外部拼案件值。
const SPEC_DOC: &str = "说明书";
const CLAIMS_DOC: &str = "本申请";
const OA_DOC: &str = "审查意见";

/// 把一份长文本切成带锚点的段落。
///
/// 分段只看版式，两种形态的**编号来源不同**，所以直接在 doc_id 上标出来：
/// - 文本里有 `[00xx]` 段号标记 ⇒ 按标记切，doc_id 原样（`说明书` / `D1`），编号就是官方段号；
/// - 没有标记 ⇒ 按空行切，doc_id 变成 `说明书(按空行分段)`，编号是本工具的序数，
///   读者据此知道要按"第几个空行块"去找，而不是去翻通知书记载的段号。
///
/// 至少要出现 2 个段号标记才认定"这是分段结构"：只命中 1 个通常是正文里的引用。
fn push_paragraphs(entries: &mut Vec<Entry>, doc_id: &str, text: &str) {
    let marks: Vec<(usize, u32, usize)> = para_marker_re()
        .captures_iter(text)
        .filter_map(|caps| {
            let full = caps.get(0)?;
            let number = caps.get(1)?.as_str().parse::<u32>().ok()?;
            Some((full.start(), number, full.end()))
        })
        .collect();
    if marks.len() >= 2 {
        for i in 0..marks.len() {
            let next_mark = marks.get(i + 1).map(|m| m.0).unwrap_or(text.len());
            // 段号段的正文止于"下一个段号"与"下一个章节标题"中更靠前者
            let body_end = next_mark.min(next_heading_after(text, marks[i].2));
            if body_end <= marks[i].2 {
                continue;
            }
            if let Some(normalized) = normalize(&text[marks[i].2..body_end]) {
                entries.push(Entry {
                    anchor: Anchor {
                        doc_id: doc_id.to_string(),
                        kind: LocatorKind::Paragraph,
                        number: marks[i].1,
                    },
                    normalized,
                });
            }
        }
        return;
    }
    for (i, block) in text.split("\n\n").enumerate() {
        if let Some(normalized) = normalize(block) {
            entries.push(Entry {
                anchor: Anchor {
                    doc_id: format!("{doc_id}(按空行分段)"),
                    kind: LocatorKind::Paragraph,
                    number: (i + 1) as u32,
                },
                normalized,
            });
        }
    }
}

/// 权利要求：按行首项号切块，锚点 `[本申请:权利要求N]`。
///
/// 只在"权利要求书"这一节里找项号。整篇扫描会把说明书里 `1.` `2.` 这类编号步骤
/// 也当成权项，锚点就会指到一段根本不是权利要求书的文字上。
fn push_claims(entries: &mut Vec<Entry>, text: &str) {
    let (lo, hi) = claims_section(text);
    let region = &text[lo..hi];
    let heads: Vec<(usize, u32)> = claim_head_re()
        .captures_iter(region)
        .filter_map(|caps| {
            let m = caps.get(1)?;
            let number = m.as_str().parse::<u32>().ok()?;
            Some((lo + m.start(), number))
        })
        .collect();
    for i in 0..heads.len() {
        let end = if i + 1 < heads.len() {
            heads[i + 1].0
        } else {
            hi
        };
        if let Some(normalized) = normalize(&text[heads[i].0..end]) {
            entries.push(Entry {
                anchor: Anchor {
                    doc_id: CLAIMS_DOC.to_string(),
                    kind: LocatorKind::Claim,
                    number: heads[i].1,
                },
                normalized,
            });
        }
    }
}

/// 对比文件切块：只认 JSON（本应用自己导出的 `refs` 形态），认不出就整块作为一份
/// 未标注来源的对比文件。
///
/// **不按正文里的 `D1` 字样猜文档边界**：猜出来的边界会把锚点指到另一份文件上，
/// 那比"定位不到"更糟——读者照着翻到的是无关文本。
fn split_ref_documents(refs: &str) -> Vec<(String, String)> {
    let value: serde_json::Value = match serde_json::from_str(refs) {
        Ok(v) => v,
        Err(_) => return vec![("对比文件".to_string(), refs.to_string())],
    };
    let mut arrays: Vec<&Vec<serde_json::Value>> = Vec::new();
    match &value {
        serde_json::Value::Array(arr) => arrays.push(arr),
        other => {
            for key in ["reference_documents", "refs"] {
                if let Some(arr) = other.get(key).and_then(|v| v.as_array()) {
                    arrays.push(arr);
                }
            }
        }
    }
    let mut docs = Vec::new();
    for arr in arrays {
        for doc in arr {
            let label = doc["label"]
                .as_str()
                .or_else(|| doc["publication_number"].as_str())
                .unwrap_or("对比文件")
                .trim()
                .to_string();
            let body = doc["full_text"].as_str().unwrap_or("").to_string();
            if !body.trim().is_empty() {
                docs.push((label, body));
            }
        }
    }
    if docs.is_empty() {
        docs.push(("对比文件".to_string(), refs.to_string()));
    }
    docs
}

/// 建立三份材料（本专利 / 对比文件 / 审查意见）的段落索引。
pub fn build_index(patent: &str, refs: &str, oa: &str) -> ParagraphIndex {
    let mut entries = Vec::new();
    if !patent.trim().is_empty() {
        // 本应用自己导出的 JSON 形态先摊平，否则说明书与权利要求会被当成一串键名
        let flat = crate::evidence_kernel::amendment_gate::flatten_json_application(patent)
            .unwrap_or_else(|| patent.to_string());
        push_paragraphs(&mut entries, SPEC_DOC, &flat);
        push_claims(&mut entries, &flat);
    }
    if !refs.trim().is_empty() {
        for (label, body) in split_ref_documents(refs) {
            push_paragraphs(&mut entries, &label, &body);
        }
    }
    if !oa.trim().is_empty() {
        push_paragraphs(&mut entries, OA_DOC, oa);
    }
    ParagraphIndex { entries }
}

/// 锚点化结果：四个可对账的计数，`total = located + unlocated`，
/// 其中 `corrected`/`verified` 是对 `located` 的进一步分类（不与 total 相加）。
#[derive(Debug, Clone, Default, Serialize)]
pub struct AnchorReport {
    /// 证据条目总数
    pub total: usize,
    /// 定位成功、`source` 已写成规范锚点的条数
    pub located: usize,
    /// 定位失败、`source` 前置 [`UNLOCATED`] 的条数
    pub unlocated: usize,
    /// 模型自报了段落号但那段里没有这句话 ⇒ 按定位结果改写的条数
    pub corrected: usize,
    /// 模型自报的锚点与定位结果一致（真正"验证过"）的条数
    pub verified: usize,
}

impl AnchorReport {
    /// 一句话结论，进 `summary` 让读者在表格之外也能看到锚点完成度。
    pub fn as_line(&self) -> String {
        format!(
            "锚点核对：{}/{} 条已定位到段落（其中模型自报锚点被验证 {} 条、纠正 {} 条），\
             {} 条在原文里定位不到，已标{}，不得当成已核实来源使用。",
            self.located, self.total, self.verified, self.corrected, self.unlocated, UNLOCATED
        )
    }
}

/// 就地把每条证据的 `source` 换成**经核对的**段落锚点。
///
/// `blank_line_numbering_note` 只影响无段号材料的 doc_id 后缀说明，见 [`push_paragraphs`]。
pub fn anchor_items(items: &mut [Evidence], index: &ParagraphIndex) -> AnchorReport {
    let mut report = AnchorReport {
        total: items.len(),
        ..Default::default()
    };
    for item in items.iter_mut() {
        let claimed = claimed_anchor(&item.source);
        match index.locate(&item.content) {
            Some(hit) => {
                let hit = hit.clone();
                if let Some(c) = &claimed {
                    if c == &hit {
                        report.verified += 1;
                    } else {
                        report.corrected += 1;
                    }
                }
                item.source = format_anchor(&hit);
                report.located += 1;
            }
            None => {
                // 定位不到：模型自报的锚点如果**确有**这段（索引口径与它一致时才会走到这里），
                // 那已经被上一条分支接住；到这里就是"没人能翻回原文"，前置可见标记。
                report.unlocated += 1;
                let kept = claimed
                    .as_ref()
                    .map(|c| format_anchor(c))
                    .unwrap_or_else(|| item.source.clone());
                item.source = format!("{UNLOCATED}{kept}");
            }
        }
    }
    report
}

/// 宽松读一遍模型自报的来源，只为"报了却没验证"这条统计服务，不作为定位依据。
///
/// 认：`[说明书:第0027段]`、`D1第[0035]段`、`说明书第27段`、`第27段`、`[0027]`、
/// `权利要求3`。认不出返回 `None`（模型多数时候写的是"说明书"这种文档级措辞）。
pub fn claimed_anchor(source: &str) -> Option<Anchor> {
    if let Some(a) = crate::evidence_kernel::anchor::parse_anchor(source) {
        return Some(a);
    }
    let re = paragraph_claim_re();
    if let Some(caps) = re.captures(source) {
        // 两种写法都要接住：`第0027段`（组1）与 `[0027]`（组2）
        let digits = caps.get(1).or_else(|| caps.get(2))?;
        let number = digits.as_str().parse::<u32>().ok()?;
        let doc_id = bracket_doc_id(source).unwrap_or_else(|| SPEC_DOC.to_string());
        return Some(Anchor {
            doc_id,
            kind: LocatorKind::Paragraph,
            number,
        });
    }
    let re = claim_claim_re();
    if let Some(caps) = re.captures(source) {
        let number = caps.get(1)?.as_str().parse::<u32>().ok()?;
        let doc_id = bracket_doc_id(source).unwrap_or_else(|| CLAIMS_DOC.to_string());
        return Some(Anchor {
            doc_id,
            kind: LocatorKind::Claim,
            number,
        });
    }
    None
}

/// 读 `[说明书:第0027段]` 这种自家规范形态里的 doc 段，让本层输出能被第二轮认出来。
/// （`parse_anchor` 的 doc 模式只认 ASCII，中文 doc_id 由这里兜住。）
fn bracket_doc_id(source: &str) -> Option<String> {
    let re = doc_id_re();
    let caps = re.captures(source)?;
    caps.get(1).map(|m| m.as_str().to_string())
}

fn doc_id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\[([^\]:：]{1,20})[:：]").expect("evidence_anchor: 括号 doc 正则")
    })
}

fn paragraph_claim_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(?:第\s*(0*[1-9][0-9]{0,3})\s*段|\[(0*[1-9][0-9]{0,3})\])")
            .expect("evidence_anchor: 模型自报段号正则")
    })
}

fn claim_claim_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"权利要求\s*([1-9][0-9]{0,2})").expect("evidence_anchor: 模型自报权项号正则")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::evidence::{Evidence, EvidenceType};

    /// 合成说明书：通用技术词 + 官方段号，无任何案件专有信息。
    const SPEC: &str = "\
说明书
技术领域
[0001] 本发明涉及一种物料输送装置。
具体实施方式
[0002] 输送带上方设有导料板，导料板与输送带之间的间距为20～30厘米。
[0003] 驱动电机的输出轴与输送带的主动辊通过链条连接，链条外侧设有张紧轮。
";

    const CLAIMS: &str = "\
权利要求书
1. 一种物料输送装置，包括机架和输送带。
2. 根据权利要求1所述的装置，其特征在于，输送带上方设有导料板。
";

    fn item(source: &str, content: &str) -> Evidence {
        Evidence {
            evidence_type: EvidenceType::SpecSupport,
            source: source.to_string(),
            content: content.to_string(),
            relevance: 0.8,
            used_in_response: false,
        }
    }

    fn material() -> String {
        format!("{SPEC}\n{CLAIMS}")
    }

    #[test]
    fn anchor_comes_from_the_material_not_from_what_the_model_claimed() {
        let index = build_index(&material(), "", "");
        let mut items = vec![item(
            "说明书",
            "输送带上方设有导料板，导料板与输送带之间的间距为20～30厘米。",
        )];
        let report = anchor_items(&mut items, &index);
        assert_eq!(
            items[0].source, "[说明书:第0002段]",
            "实际产物：{}",
            items[0].source
        );
        assert_eq!(report.located, 1);
        assert_eq!(report.unlocated, 0);
        assert_eq!(
            report.total,
            report.located + report.unlocated,
            "计数必须闭合"
        );
    }

    #[test]
    fn content_absent_from_the_material_is_marked_and_never_given_an_anchor() {
        // 负控：模型编出来的"证据"不能被配上真锚点，否则假证据变得比真证据更好看
        let index = build_index(&material(), "", "");
        let mut items = vec![item(
            "[说明书:第0009段]",
            "隔离罩与伸缩轴之间的预设差值用于抑制粉尘外溢现象。",
        )];
        let report = anchor_items(&mut items, &index);
        assert_eq!(report.located, 0);
        assert_eq!(report.unlocated, 1);
        assert!(
            items[0].source.starts_with(UNLOCATED),
            "定位不到必须可见：{}",
            items[0].source
        );
        assert!(
            items[0].source.contains("[说明书:第0009段]"),
            "模型自报的锚点要保留下来供核对：{}",
            items[0].source
        );
    }

    #[test]
    fn too_short_content_is_not_located_because_any_match_is_noise() {
        let index = build_index(&material(), "", "");
        let mut items = vec![item("说明书", "差值")];
        let report = anchor_items(&mut items, &index);
        assert_eq!(
            report.unlocated, 1,
            "两三个字的短语全文到处都在，不能算定位"
        );
    }

    #[test]
    fn wrong_self_reported_paragraph_is_corrected_and_counted() {
        let index = build_index(&material(), "", "");
        let mut items = vec![item(
            "说明书第[0001]段",
            "输送带上方设有导料板，导料板与输送带之间的间距为20～30厘米。",
        )];
        let report = anchor_items(&mut items, &index);
        assert_eq!(items[0].source, "[说明书:第0002段]");
        assert_eq!(
            report.corrected, 1,
            "报了段号但那段里没有这句话 ⇒ 必须单独计数"
        );
        assert_eq!(report.verified, 0);
    }

    #[test]
    fn right_self_reported_paragraph_is_counted_as_verified() {
        let index = build_index(&material(), "", "");
        let mut items = vec![item(
            "[说明书:第0003段]",
            "驱动电机的输出轴与输送带的主动辊通过链条连接，链条外侧设有张紧轮。",
        )];
        let report = anchor_items(&mut items, &index);
        assert_eq!(items[0].source, "[说明书:第0003段]");
        assert_eq!(report.verified, 1);
        assert_eq!(report.corrected, 0);
    }

    #[test]
    fn ellipsis_joining_two_quotes_in_the_same_paragraph_still_locates() {
        let index = build_index(&material(), "", "");
        let mut items = vec![item("说明书", "输送带上方设有导料板…间距为20～30厘米")];
        let report = anchor_items(&mut items, &index);
        assert_eq!(
            report.located, 1,
            "省略号引文仍是可翻回原文的：{:?}",
            items[0].source
        );
        assert_eq!(items[0].source, "[说明书:第0002段]");
    }

    #[test]
    fn ellipsis_spanning_different_paragraphs_is_not_located() {
        // 跨段拼接不是"同一段的引文"，不能被配成单段锚点
        let index = build_index(&material(), "", "");
        let mut items = vec![item("说明书", "输送带上方设有导料板…链条外侧设有张紧轮")];
        let report = anchor_items(&mut items, &index);
        assert_eq!(report.located, 0);
        assert_eq!(report.unlocated, 1);
    }

    #[test]
    fn claims_are_anchored_by_claim_number() {
        let index = build_index(&material(), "", "");
        let mut items = vec![item(
            "权利要求",
            "根据权利要求1所述的装置，其特征在于，输送带上方设有导料板。",
        )];
        let report = anchor_items(&mut items, &index);
        assert_eq!(report.located, 1);
        assert_eq!(
            items[0].source, "[本申请:权利要求2]",
            "实际：{}",
            items[0].source
        );
    }

    #[test]
    fn text_without_paragraph_markers_falls_back_to_blank_lines_and_says_so() {
        let index = build_index(
            "",
            "",
            "第一段内容属于审查意见的开头部分说明。\n\n第二段内容说明驳回理由与创造性判断。\n",
        );
        let mut items = vec![item("审查意见", "第二段内容说明驳回理由与创造性判断。")];
        let report = anchor_items(&mut items, &index);
        assert_eq!(report.located, 1);
        assert!(
            items[0].source.contains("(按空行分段)"),
            "无官方段号时必须标出编号来源：{}",
            items[0].source
        );
    }

    #[test]
    fn refs_json_labels_become_the_doc_id() {
        let refs = serde_json::json!([
            {
                "label": "D9",
                "publication_number": "CN990009001A",
                "full_text": "[0001] 一种输送设备，包括机架与输送带组件。\n[0002] 输送带组件上方设置清扫刷，清扫刷与输送带接触。"
            }
        ])
        .to_string();
        let index = build_index("", &refs, "");
        let mut items = vec![item("D9", "输送带组件上方设置清扫刷，清扫刷与输送带接触。")];
        let report = anchor_items(&mut items, &index);
        assert_eq!(report.located, 1);
        assert_eq!(
            items[0].source, "[D9:第0002段]",
            "实际：{}",
            items[0].source
        );
    }

    #[test]
    fn refs_are_not_split_by_guessing_d_labels_in_plain_text() {
        // 已知边界：非 JSON 的 refs 整块当成一份未标注来源的对比文件，
        // 不能靠正文里出现的 "D1" 字样猜文档边界（猜错=锚点指向另一份文件）
        let index = build_index(
            "",
            "这里提到 D1 但没有任何结构化边界，只在一段里叙述相关内容说明。",
            "",
        );
        let found = index.entries.iter().any(|e| e.anchor.doc_id == "D1");
        assert!(!found, "不许按 D1 字样猜边界");
    }

    #[test]
    fn own_json_application_shape_is_flattened_before_indexing() {
        let patent = serde_json::json!({
            "claims": "1. 一种物料输送装置，包括机架和输送带。",
            "description": "[0001] 本发明涉及一种物料输送装置。\n[0002] 输送带上方设有导料板，间距为20厘米。"
        })
        .to_string();
        let index = build_index(&patent, "", "");
        let mut items = vec![item("说明书", "输送带上方设有导料板，间距为20厘米。")];
        let report = anchor_items(&mut items, &index);
        assert_eq!(
            report.located, 1,
            "自家 JSON 形态必须进索引：{:?}",
            items[0].source
        );
    }

    #[test]
    fn anchoring_is_idempotent() {
        let index = build_index(&material(), "", "");
        let mut items = vec![item(
            "说明书",
            "输送带上方设有导料板，导料板与输送带之间的间距为20～30厘米。",
        )];
        let once = anchor_items(&mut items, &index);
        let before = items[0].source.clone();
        let twice = anchor_items(&mut items, &index);
        assert_eq!(items[0].source, before, "第二次跑不得改写已经核过的锚点");
        assert_eq!(once.located, twice.located);
        assert_eq!(
            twice.verified, 1,
            "第二轮里自报锚点等于定位结果 ⇒ 记为已验证"
        );
    }

    #[test]
    fn report_line_states_the_numbers_it_claims() {
        let index = build_index(&material(), "", "");
        let mut items = vec![
            item(
                "说明书",
                "输送带上方设有导料板，导料板与输送带之间的间距为20～30厘米。",
            ),
            item(
                "说明书",
                "编造出来的特征描述句，材料里根本没有出现过这句话。",
            ),
        ];
        let report = anchor_items(&mut items, &index);
        let line = report.as_line();
        assert!(line.contains("1/2 条已定位到段落"), "line={line}");
        assert!(line.contains("1 条在原文里定位不到"), "line={line}");
        assert!(line.contains(UNLOCATED), "line={line}");
    }
}
