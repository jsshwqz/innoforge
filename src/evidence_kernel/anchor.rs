//! 锚点解析：把 AI 文本里的四套遗留引用格式统一为 (doc_id, kind, number) 三元组。
//!
//! 规范语法（ADR-2）：`[D1:第0035段]`、`[D1:权利要求2]`、`[引用3]`。
//! 遗留适配：`D1第[0035]段`、`【依据：对比文件D1第35段】`、`【依据：对比文件D1权利要求2】`。
//! 解析失败一律 `None`（UnparsedAnchor），绝不猜（#79：unparsed 与 unverified 分开统计）。
//!
//! 注意（vacuous-checks 审计教训）：regex crate 不支持 look-around，所有模式只用基础语法；
//! 模式是编译期常量，构建失败直接 panic（不可能静默空转）。

use crate::evidence_kernel::model::{Anchor, EvidenceRef, LocatorKind};
use regex::Regex;
use std::sync::OnceLock;

/// 完整锚点集合正则：规范与遗留格式一次捕获。
static ANCHOR_RE: OnceLock<Regex> = OnceLock::new();

fn anchor_re() -> &'static Regex {
    ANCHOR_RE.get_or_init(|| {
        Regex::new(
            r"(?x)
        \[(?P<bd>[A-Za-z0-9]{1,8}):(?P<bk>权利要求|第)(?P<bn>[0-9]{1,6})(?:段)?\]
      | \[(?P<ref>引用)(?P<refn>[0-9]{1,4})\]
      | (?P<ld>[A-Z][0-9]{1,3})第\[(?P<ln>[0-9]{1,6})\]段
      | 【依据：[^】]*?(?P<yd>[A-Z][0-9]{1,3})(?P<yk>权利要求|第)(?P<yn>[0-9]{1,6})(?:段)?[^】]*?】
        ",
        )
        .unwrap_or_else(|e| panic!("evidence_kernel ANCHOR_RE 是静态常量，编译失败即 bug: {e}"))
    })
}

/// 解析一个原始锚点串为三元组。返回 None = 无法解析（unparsed）。
pub fn parse_anchor(raw: &str) -> Option<Anchor> {
    let s = raw.trim();
    let caps = anchor_re().captures(s)?;
    if let (Some(doc), Some(num)) = (caps.name("bd"), caps.name("bn")) {
        let kind = if caps.name("bk").map(|m| m.as_str()) == Some("权利要求") {
            LocatorKind::Claim
        } else {
            LocatorKind::Paragraph
        };
        return Some(Anchor {
            doc_id: doc.as_str().to_string(),
            kind,
            number: parse_num(num.as_str())?,
        });
    }
    if let Some(num) = caps.name("refn") {
        return Some(Anchor {
            doc_id: format!("引用{}", num.as_str()),
            kind: LocatorKind::Chunk,
            number: 0,
        });
    }
    if let (Some(doc), Some(num)) = (caps.name("ld"), caps.name("ln")) {
        return Some(Anchor {
            doc_id: doc.as_str().to_string(),
            kind: LocatorKind::Paragraph,
            number: parse_num(num.as_str())?,
        });
    }
    if let (Some(doc), Some(num)) = (caps.name("yd"), caps.name("yn")) {
        let kind = if caps.name("yk").map(|m| m.as_str()) == Some("权利要求") {
            LocatorKind::Claim
        } else {
            LocatorKind::Paragraph
        };
        return Some(Anchor {
            doc_id: doc.as_str().to_string(),
            kind,
            number: parse_num(num.as_str())?,
        });
    }
    None
}

/// "0035" 与 "35" 判等：统一转整数（#79 的零填充相等，而非子串包含）。
fn parse_num(s: &str) -> Option<u32> {
    let t = s.trim();
    if t.chars().all(|c| c == '0') {
        return Some(0);
    }
    t.trim_start_matches('0').parse::<u32>().ok()
}

/// 提取文本中全部锚点引用（含引文尝试）。
/// 引文归属：锚点紧前是闭引号（“…”[锚点]，中文常态）→ 取锚点前引文；
/// 否则在锚点后 12 字内找开引号（[锚点]：“…”，少见语序）。
pub fn extract_evidence_refs(text: &str) -> Vec<EvidenceRef> {
    let mut out = Vec::new();
    for caps in anchor_re().captures_iter(text) {
        let m = caps.get(0).unwrap_or_else(|| unreachable!("组 0 恒存在"));
        let whole = m.as_str().to_string();
        let anchor = parse_anchor(&whole);
        let quote = if anchor.is_some() {
            // 索引均来自 regex/find（天然落在字符边界），get() 形态从语法上根除
            // 非 ASCII panic 可能（#73 防线：禁裸字节切片）
            let before = text.get(..m.start()).unwrap_or_default();
            let adjacent_close = before.trim_end().ends_with(['”', '」', '』', '’']);
            if adjacent_close {
                quote_before(before)
            } else {
                quote_after(text.get(m.end()..).unwrap_or_default())
            }
            .unwrap_or_default()
        } else {
            String::new()
        };
        out.push(EvidenceRef {
            raw: whole,
            anchor,
            quote,
        });
    }
    out
}

/// 锚点串之后紧跟的引文（≤120 字），找不到返回 None。
fn quote_after(rest: &str) -> Option<String> {
    let rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '：' || c == ':');
    for (open, close) in [('“', '”'), ('「', '」'), ('《', '》')] {
        if let Some(start) = rest.find(open) {
            if start <= 12 {
                let after = rest.get(start + open.len_utf8()..).unwrap_or_default();
                if let Some(end) = after.find(close) {
                    let q = after.get(..end).unwrap_or_default();
                    if !q.is_empty() && q.chars().count() <= 120 {
                        return Some(q.to_string());
                    }
                }
            }
        }
    }
    None
}

/// 锚点串之前最近的引文。窗口 = 本句内（先截断到最后一个句读符）再取末尾 120 字，
/// 防止抓到上一句的引文。
fn quote_before(before: &str) -> Option<String> {
    // 截断到本句：最后一个句读符之后的内容（find 返回字节索引必在字符边界）
    let this_sentence = match before.rfind(['。', '！', '？', '；', ';', '!', '?', '\n']) {
        Some(pos) => before.get(pos + 1..).unwrap_or_default(),
        None => before,
    };
    let chars: Vec<char> = this_sentence.chars().collect();
    if chars.is_empty() {
        return None;
    }
    let window_start = chars.len().saturating_sub(120);
    let window: String = chars[window_start..].iter().collect();
    for (open, close) in [('“', '”'), ('「', '」'), ('《', '》')] {
        if let Some(open_pos) = window.rfind(open) {
            let inner = window.get(open_pos + open.len_utf8()..).unwrap_or_default();
            if let Some(end) = inner.find(close) {
                let q = inner.get(..end).unwrap_or_default();
                if !q.is_empty() && q.chars().count() <= 120 {
                    return Some(q.to_string());
                }
            }
        }
    }
    None
}

/// 规范锚点格式化（报告/修复轮回写用）。
pub fn format_anchor(a: &Anchor) -> String {
    match a.kind {
        LocatorKind::Claim => format!("[{}:权利要求{}]", a.doc_id, a.number),
        LocatorKind::Paragraph => format!("[{}:第{:04}段]", a.doc_id, a.number),
        LocatorKind::Chunk => format!("[{}]", a.doc_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_formats() {
        let a = parse_anchor("[D1:第0035段]").expect("canonical para");
        assert_eq!(
            a,
            Anchor {
                doc_id: "D1".into(),
                kind: LocatorKind::Paragraph,
                number: 35
            }
        );
        // 零填充等价：0035 与 35 是同一编号（整数判等）
        let b = parse_anchor("[D1:第35段]").expect("no pad");
        assert_eq!(a, b);
        let c = parse_anchor("[D2:权利要求3]").expect("canonical claim");
        assert_eq!(c.kind, LocatorKind::Claim);
        assert_eq!(c.number, 3);
    }

    #[test]
    fn parses_ref_chunk_and_legacy() {
        let a = parse_anchor("[引用3]").expect("ref chunk");
        assert_eq!(a.doc_id, "引用3");
        assert_eq!(a.kind, LocatorKind::Chunk);
        let a = parse_anchor("D1第[0035]段").expect("legacy bracket");
        assert_eq!(a.number, 35);
        assert_eq!(a.kind, LocatorKind::Paragraph);
        let a = parse_anchor("【依据：对比文件D1第35段】").expect("yiju para");
        assert_eq!(a.number, 35);
        assert_eq!(a.kind, LocatorKind::Paragraph);
        let a = parse_anchor("【依据：对比文件D2权利要求1】").expect("yiju claim");
        assert_eq!(a.kind, LocatorKind::Claim);
        assert_eq!(a.number, 1);
    }

    #[test]
    fn unparsed_stays_none() {
        for s in [
            "没有锚点的句子",
            "[第35段]",
            "D9",
            "[D1:摘要]",
            "第35段",
            "[引用]",
            "[D1:第段]",
        ] {
            assert!(parse_anchor(s).is_none(), "不应解析: {s}");
        }
    }

    #[test]
    fn extracts_multiple_anchors_with_quotes() {
        let text = "D1公开了“当温度超过阈值时启动风扇”[D1:第0035段]，且记载“自动调节转速”[D2:权利要求1]，另有[引用3]支持。";
        let refs = extract_evidence_refs(text);
        assert_eq!(refs.len(), 3, "refs: {refs:?}");
        assert_eq!(refs[0].quote, "当温度超过阈值时启动风扇");
        assert_eq!(refs[1].anchor.as_ref().map(|a| a.number), Some(1));
        assert_eq!(
            refs[2].anchor.as_ref().map(|a| a.doc_id.clone()),
            Some("引用3".into())
        );
    }

    #[test]
    fn formats_canonical_anchor() {
        let a = Anchor {
            doc_id: "D1".into(),
            kind: LocatorKind::Paragraph,
            number: 35,
        };
        assert_eq!(format_anchor(&a), "[D1:第0035段]");
    }
}
