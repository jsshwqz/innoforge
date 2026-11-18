//! Semantic Chunker

/// Split text into semantic chunks.
///
/// Operates on `Vec<char>` to guarantee char-boundary safety. The previous
/// byte-index approach (`text[start..end]`) could split multi-byte characters
/// and produce invalid UTF-8 or panic on Chinese text.
pub fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }
    let chars: Vec<char> = text.chars().collect();
    let total = chars.len();
    let window = chunk_size.saturating_sub(overlap);
    if window == 0 {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < total {
        let end = (start + chunk_size).min(total);
        let chunk: String = chars[start..end].iter().collect();
        chunks.push(chunk);
        if end >= total {
            break;
        }
        let search_start = end.saturating_sub(overlap);
        let mut boundary = end;
        for i in (search_start..end).rev() {
            let c = chars[i];
            let is_boundary = c == '。' || c == ';' || c == '.';
            let is_newline = c == '\n';
            if (is_boundary || is_newline) && i > start + window / 2 {
                boundary = i;
                break;
            }
        }
        start = boundary + 1;
    }
    let mut deduped = Vec::new();
    for chunk in chunks {
        if deduped.is_empty() || *deduped.last().unwrap_or(&"".to_string()) != chunk {
            deduped.push(chunk);
        }
    }
    deduped
}

/// 从字符流里识别「权利要求序号」起点（Batch 1 任务 1.2）。
///
/// 有状态扫描：首个命中的序号 `n`（1..=50，数字后紧跟 `.、．` 且再往后不是数字——
/// 防「1.5 mm」这类小数误切）确定列表起点，之后要求序号严格等于 `expected+1` 递进，
/// 「权利要求1所述」「图1.」等不会误触发（不等于当前期望序号就跳过）。
/// 返回按出现顺序的 `(claim_number, 字符下标)` 列表。
fn find_claim_starts(chars: &[char]) -> Vec<(u32, usize)> {
    let mut starts = Vec::new();
    let mut expected: u32 = 0; // 0 = 尚未找到列表起点
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            let mut n: u32 = 0;
            let mut overflow = false;
            while i < chars.len() && chars[i].is_ascii_digit() {
                match n
                    .checked_mul(10)
                    .and_then(|v| v.checked_add(u32::from(chars[i] as u8 - b'0')))
                {
                    Some(v) => n = v,
                    None => overflow = true,
                }
                i += 1;
            }
            if i < chars.len()
                && matches!(chars[i], '.' | '、' | '．')
                && i + 1 < chars.len()
                && !chars[i + 1].is_ascii_digit()
                && !overflow
            {
                let accepted = if expected == 0 {
                    if (1..=50).contains(&n) {
                        expected = n;
                        true
                    } else {
                        false
                    }
                } else {
                    n == expected.saturating_add(1)
                };
                if accepted {
                    starts.push((n, start));
                    expected = n;
                }
            }
        } else {
            i += 1;
        }
    }
    starts
}

/// 说明书段落号 `[NNNN]` 起始位置（仅行首或行首空白后，正文引用不算）。
fn find_paragraph_marks(chars: &[char]) -> Vec<usize> {
    let mut marks = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '[' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 && j < chars.len() && chars[j] == ']' {
                let at_line_start = i == 0 || chars[i - 1] == '\n' || chars[i - 1].is_whitespace();
                if at_line_start {
                    marks.push(i);
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    marks
}

/// 中文专利说明书的法定章节标题（结构化分块的标题锚点）。
const DESCRIPTION_HEADINGS: [&str; 5] = [
    "技术领域",
    "背景技术",
    "发明内容",
    "附图说明",
    "具体实施方式",
];

/// 在字符流中找章节标题的行首位置。要求整行除标题词外只有标点/空白
/// （防正文里「本发明属于…技术领域」误判），且标题词不与更长的标题前缀重叠。
fn find_heading_starts(chars: &[char]) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    let mut line_start = 0usize;
    for i in 0..=chars.len() {
        let at_end = i == chars.len();
        if at_end || chars[i] == '\n' {
            let mut k = line_start;
            while k < i && chars[k].is_whitespace() {
                k += 1;
            }
            for h in DESCRIPTION_HEADINGS {
                let hc: Vec<char> = h.chars().collect();
                if k + hc.len() <= i {
                    let matched = (0..hc.len()).all(|m| chars[k + m] == hc[m]);
                    if matched {
                        let rest_ok = chars[k + hc.len()..i]
                            .iter()
                            .all(|c| c.is_whitespace() || matches!(c, '：' | ':' | '，' | ','));
                        if rest_ok {
                            found.push((line_start, h));
                        }
                    }
                }
            }
            line_start = i + 1;
        }
    }
    found
}

/// 结构化专利分块（Batch 1 任务 1.2）。
///
/// 返回 `(section_tag, chunk_text)` 列表：
/// - `source_type == "claim"`：按权利要求序号打 `Claim:N`（有状态扫描，见
///   [`find_claim_starts`]），序号列表前的引导文字标 `Claim:Intro`；
///   识别不到序号时回退 [`chunk_text`] 窗口切分，标 `Claim:Window`。
/// - `source_type == "description"`：优先按 `[NNNN]` 段落号打 `Desc:[NNNN]`
///   （首段前的前言标 `Desc:Intro`）；无段落号但含章节标题时按标题打
///   `Desc:技术领域` 等；两者都没有则回退窗口切分，标 `Desc:Window`。
/// - 其他（abstract 等）：标 `Abstract`。
///
/// 超过 `chunk_size` 的片段用 [`chunk_text`] 细分，保留同一 section_tag。
/// 全程按 `Vec<char>` 操作，无 `&str` 字节切片（AGENTS.md 2.7 / #73 防线）。
pub fn chunk_structured_patent(
    source_type: &str,
    text: &str,
    chunk_size: usize,
    overlap: usize,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    if text.trim().is_empty() {
        return out;
    }
    let chars: Vec<char> = text.chars().collect();

    fn emit(
        out: &mut Vec<(String, String)>,
        tag: &str,
        seg: &[char],
        chunk_size: usize,
        overlap: usize,
    ) {
        if seg.is_empty() {
            return;
        }
        let text_seg: String = seg.iter().collect();
        for piece in chunk_text(&text_seg, chunk_size, overlap) {
            out.push((tag.to_string(), piece));
        }
    }

    match source_type {
        "claim" => {
            let starts = find_claim_starts(&chars);
            if starts.is_empty() {
                emit(&mut out, "Claim:Window", &chars, chunk_size, overlap);
            } else {
                if starts[0].1 > 0 {
                    emit(
                        &mut out,
                        "Claim:Intro",
                        &chars[..starts[0].1],
                        chunk_size,
                        overlap,
                    );
                }
                for (idx, (num, s)) in starts.iter().enumerate() {
                    let end = starts
                        .get(idx + 1)
                        .map(|(_, s2)| *s2)
                        .unwrap_or(chars.len());
                    emit(
                        &mut out,
                        &format!("Claim:{num}"),
                        &chars[*s..end],
                        chunk_size,
                        overlap,
                    );
                }
            }
        }
        "description" => {
            let marks = find_paragraph_marks(&chars);
            let headings = find_heading_starts(&chars);
            if marks.len() >= 2 {
                // 段落号优先：首个 [NNNN] 之前的前言（如「说明书」标题行）标 Desc:Intro
                if marks[0] > 0 {
                    emit(
                        &mut out,
                        "Desc:Intro",
                        &chars[..marks[0]],
                        chunk_size,
                        overlap,
                    );
                }
                for (idx, s) in marks.iter().enumerate() {
                    let end = marks.get(idx + 1).copied().unwrap_or(chars.len());
                    let tag = extract_paragraph_tag(&chars[*s..end]);
                    emit(&mut out, &tag, &chars[*s..end], chunk_size, overlap);
                }
            } else if !headings.is_empty() {
                for (idx, (s, name)) in headings.iter().enumerate() {
                    let end = headings
                        .get(idx + 1)
                        .map(|(s2, _)| *s2)
                        .unwrap_or(chars.len());
                    emit(
                        &mut out,
                        &format!("Desc:{name}"),
                        &chars[*s..end],
                        chunk_size,
                        overlap,
                    );
                }
            } else {
                emit(&mut out, "Desc:Window", &chars, chunk_size, overlap);
            }
        }
        _ => {
            emit(&mut out, "Abstract", &chars, chunk_size, overlap);
        }
    }
    out
}

/// 从 `[NNNN]` 段落片段开头提取完整段号标签（`[0001]正文…` → `Desc:[0001]` 前缀由
/// 调用方拼接；本函数只取 `[` 到 `]` 的段号体，异常形态截到 32 字符防失控）。
fn extract_paragraph_tag(seg: &[char]) -> String {
    let mut tag = String::from("Desc:");
    for c in seg.iter().take(32) {
        tag.push(*c);
        if *c == ']' {
            break;
        }
    }
    tag
}

pub fn chunk_summary(chunk: &str) -> String {
    let preview: String = chunk.chars().take(60).collect();
    if preview.len() < chunk.len() {
        format!("{}...", preview)
    } else {
        preview
    }
}

/// Compute TF-IDF embedding for a chunk of text.
///
/// Delegates to [`crate::db::vector::compute_tfidf_embedding`] — the single
/// canonical implementation. The previous inline copy had the same
/// byte-slicing bug as the other two copies (panicked on Chinese text).
pub fn compute_chunk_embedding(text: &str) -> Vec<f32> {
    crate::db::vector::compute_tfidf_embedding(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_text_empty() {
        assert!(chunk_text("", 100, 20).is_empty());
    }

    #[test]
    fn test_chunk_text_short_text_single_chunk() {
        let result = chunk_text("hello world", 100, 20);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "hello world");
    }

    #[test]
    fn test_chunk_text_chinese_no_panic() {
        // Multi-byte Chinese text must not panic
        let text = "这是一段中文测试文本，用于验证切片器在多字节字符上的安全性。".repeat(10);
        let result = chunk_text(&text, 50, 10);
        assert!(!result.is_empty());
        // Every chunk must be valid UTF-8 (implicit — String is always valid UTF-8)
        for chunk in &result {
            assert!(!chunk.is_empty());
        }
    }

    #[test]
    fn test_chunk_text_overlap_produces_multiple_chunks() {
        let text = "Sentence one. Sentence two. Sentence three. Sentence four. Sentence five.";
        let result = chunk_text(text, 30, 10);
        assert!(
            result.len() >= 2,
            "Long text should produce multiple chunks"
        );
    }

    #[test]
    fn test_chunk_summary_short() {
        let result = chunk_summary("short");
        assert_eq!(result, "short");
    }

    #[test]
    fn test_chunk_summary_long_truncates() {
        let long = "a".repeat(100);
        let result = chunk_summary(&long);
        assert!(result.ends_with("..."));
        assert!(result.len() < long.len());
    }

    #[test]
    fn test_compute_chunk_embedding_returns_vector() {
        let emb = compute_chunk_embedding("test text");
        assert!(!emb.is_empty());
    }

    // ── 结构化分块（Batch 1 任务 1.2） ───────────────────────────────────────────

    #[test]
    fn structured_claims_numbered_inline_and_multiline() {
        // 行内编号（PDF 提取常见形态）+ 换行编号两种形态都要能切
        let text = "1.一种固态电池，包括正极与负极。2.根据权利要求1所述的固态电池，其特征在于还包括隔膜。\n3.如权利要求2所述，还包括外壳。";
        let out = chunk_structured_patent("claim", text, 800, 100);
        let tags: Vec<&str> = out.iter().map(|(t, _)| t.as_str()).collect();
        assert!(tags.contains(&"Claim:1"), "{tags:?}");
        assert!(tags.contains(&"Claim:2"), "{tags:?}");
        assert!(tags.contains(&"Claim:3"), "{tags:?}");
        // 内容按序号归属正确
        let c1 = out
            .iter()
            .find(|(t, _)| t == "Claim:1")
            .expect("Claim:1")
            .1
            .clone();
        assert!(c1.starts_with("1.一种固态电池"), "{c1}");
        assert!(
            !c1.contains("隔膜"),
            "Claim:1 不得吞掉 Claim:2 的内容: {c1}"
        );
    }

    #[test]
    fn structured_claims_without_numbers_fall_back_to_window() {
        let out =
            chunk_structured_patent("claim", "没有编号的权利要求文本，整体作为窗口。", 800, 100);
        assert_eq!(1, out.len());
        assert_eq!("Claim:Window", out[0].0);
    }

    #[test]
    fn structured_description_paragraph_marks_win() {
        let text = "说明书\n[0001]\n本发明涉及电池技术领域。\n[0002]\n一种固态电池，包括电解质层。\n[0003]\n所述电解质层为硫化物。";
        let out = chunk_structured_patent("description", text, 800, 100);
        let tags: Vec<&str> = out.iter().map(|(t, _)| t.as_str()).collect();
        assert!(tags.contains(&"Desc:Intro"), "{tags:?}");
        assert!(tags.contains(&"Desc:[0001]"), "{tags:?}");
        assert!(tags.contains(&"Desc:[0002]"), "{tags:?}");
        let p1 = out
            .iter()
            .find(|(t, _)| t == "Desc:[0001]")
            .expect("Desc:[0001]")
            .1
            .clone();
        assert!(p1.contains("电池技术领域"), "{p1}");
        assert!(!p1.contains("[0002]"), "段落 0001 不得吞掉 0002: {p1}");
    }

    #[test]
    fn structured_description_headings_when_no_marks() {
        let text = "技术领域：\n本发明涉及电池。\n背景技术：\n现有技术存在问题。\n具体实施方式：\n下面结合实施例说明。";
        let out = chunk_structured_patent("description", text, 800, 100);
        let tags: Vec<&str> = out.iter().map(|(t, _)| t.as_str()).collect();
        assert!(tags.contains(&"Desc:技术领域"), "{tags:?}");
        assert!(tags.contains(&"Desc:背景技术"), "{tags:?}");
        assert!(tags.contains(&"Desc:具体实施方式"), "{tags:?}");
    }

    #[test]
    fn structured_description_plain_text_falls_back_to_window() {
        let out = chunk_structured_patent(
            "description",
            "一段没有任何结构标记的说明书文字。",
            800,
            100,
        );
        assert_eq!(1, out.len());
        assert_eq!("Desc:Window", out[0].0);
    }

    #[test]
    fn structured_abstract_tagged_and_empty_input_empty() {
        let out = chunk_structured_patent("abstract", "摘要内容。", 800, 100);
        assert_eq!("Abstract", out[0].0);
        assert!(chunk_structured_patent("abstract", "", 800, 100).is_empty());
        assert!(chunk_structured_patent("description", "   ", 800, 100).is_empty());
    }

    #[test]
    fn structured_long_claim_segment_subdivided_with_same_tag() {
        let body = "一种固态电池及其制备方法，包括多个长句内容。".repeat(30);
        let text = format!("1. {body} 2. 根据权利要求1所述的固态电池。");
        let out = chunk_structured_patent("claim", &text, 100, 20);
        let claim1_chunks: Vec<&(String, String)> =
            out.iter().filter(|(t, _)| t == "Claim:1").collect();
        assert!(
            claim1_chunks.len() >= 2,
            "超长权利要求应被细分且保留 Claim:1 标签: {:?}",
            out.iter().map(|(t, _)| t).collect::<Vec<_>>()
        );
        // 总内容不丢（拼接后含首尾标记）
        let joined: String = claim1_chunks.iter().map(|(_, c)| c.as_str()).collect();
        assert!(joined.contains("制备方法"));
        assert!(
            !joined.contains("2. 根据权利要求1"),
            "Claim:1 不得吞掉 Claim:2"
        );
        let has_claim2 = out.iter().any(|(t, _)| t == "Claim:2");
        assert!(has_claim2, "Claim:2 必须独立成段");
    }

    /// 中文正文全程无 panic（#73 防线——多字节输入在结构化分块里安全）。
    #[test]
    fn structured_chunking_no_panic_on_chinese() {
        let text = "1.一种多字节中文的权利要求，长度跨越边界。2.根据权利要求1所述。".repeat(5);
        let out = chunk_structured_patent("claim", &text, 60, 10);
        assert!(!out.is_empty());
        for (tag, piece) in &out {
            assert!(!tag.is_empty());
            assert!(!piece.is_empty());
        }
    }

    /// 正文里的小数/引用编号不得误切成权利要求起点（有状态扫描的负控）。
    #[test]
    fn structured_claims_ignore_decimals_and_forward_references() {
        let text = "1.一种装置，精度为1.5毫米，参照图1.所示。2.根据权利要求1所述的装置。";
        let out = chunk_structured_patent("claim", text, 800, 100);
        let tags: Vec<&str> = out.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(vec!["Claim:1", "Claim:2"], tags, "{tags:?}");
    }
}
