//! 双栖结构化切片器与纯 Rust 中英文分词器（`src/core/vault/chunker.rs`）
//!
//! 严守 `AGENTS.md` §三.1：
//! - 零 C 依赖纯 Rust 分词（移动端 Android/iOS 与桌面端共用）；
//! - 全程使用 `.chars()` 迭代器，严禁任何 `&s[..n]` 裸字节切片；
//! - 同一 `doc_id` 下生成的 `section_tag` 严格唯一。

use std::collections::HashSet;

use regex::Regex;

use super::models::{DocChunk, DocKind, FigureNumeral, KnowledgeDoc, SectionKind};

fn is_cjk(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c)
        || ('\u{3400}'..='\u{4DBF}').contains(&c)
        || ('\u{F900}'..='\u{FAFF}').contains(&c)
}

fn flush_ascii(buf: &mut String, out: &mut Vec<String>) {
    if !buf.is_empty() {
        out.push(std::mem::take(buf));
    }
}

fn flush_cjk(buf: &mut Vec<char>, out: &mut Vec<String>) {
    if buf.is_empty() {
        return;
    }
    for &c in buf.iter() {
        out.push(c.to_string());
    }
    for pair in buf.windows(2) {
        if let [a, b] = pair {
            let mut bigram = String::with_capacity(8);
            bigram.push(*a);
            bigram.push(*b);
            out.push(bigram);
        }
    }
    buf.clear();
}

/// 纯 Rust 中英文混合分词器（移动端零 C 依赖）
///
/// - 连续 ASCII 字母数字（如 `0024`、`H01L`）转小写原样保留；
/// - 连续中文字符同时输出单字与相邻双字（bigram），以空格分隔；
/// - 确保单字与双字专业术语在 `chunks_fts` (`unicode61`) 中 100% 精准命中。
pub fn build_fts_tokens(text: &str) -> String {
    let mut tokens = Vec::new();
    let mut ascii_buf = String::new();
    let mut cjk_buf: Vec<char> = Vec::new();

    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            flush_cjk(&mut cjk_buf, &mut tokens);
            ascii_buf.push(c.to_ascii_lowercase());
        } else if is_cjk(c) {
            flush_ascii(&mut ascii_buf, &mut tokens);
            cjk_buf.push(c);
        } else {
            flush_ascii(&mut ascii_buf, &mut tokens);
            flush_cjk(&mut cjk_buf, &mut tokens);
        }
    }
    flush_ascii(&mut ascii_buf, &mut tokens);
    flush_cjk(&mut cjk_buf, &mut tokens);

    tokens.join(" ")
}

fn ensure_unique_tag(base: String, used: &mut HashSet<String>) -> String {
    if used.insert(base.clone()) {
        return base;
    }
    let mut idx = 2usize;
    loop {
        let candidate = format!("{base}#{idx}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        idx += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn make_chunk(
    doc_id: &str,
    section_kind: SectionKind,
    section_tag: String,
    claim_no: Option<i32>,
    parent_claim_no: Option<i32>,
    para_start: Option<i32>,
    para_end: Option<i32>,
    embodiment_group: Option<String>,
    raw_content: String,
) -> DocChunk {
    let char_count = raw_content.chars().count();
    let fts_tokens = build_fts_tokens(&raw_content);
    DocChunk {
        chunk_id: 0,
        doc_id: doc_id.to_string(),
        section_kind,
        section_tag,
        claim_no,
        parent_claim_no,
        para_start,
        para_end,
        embodiment_group,
        raw_content,
        fts_tokens,
        embedding_blob: None,
        char_count,
    }
}

fn split_by_char_limit(text: &str, max_chars: usize) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if max_chars == 0 {
        return vec![trimmed.to_string()];
    }
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut count = 0usize;

    for line in trimmed.lines() {
        let line_len = line.chars().count();
        if count > 0 && count + 1 + line_len > max_chars {
            let chunk = current.trim().to_string();
            if !chunk.is_empty() {
                parts.push(chunk);
            }
            current.clear();
            count = 0;
        }
        if line_len > max_chars {
            if !current.is_empty() {
                let chunk = current.trim().to_string();
                if !chunk.is_empty() {
                    parts.push(chunk);
                }
                current.clear();
                count = 0;
            }
            let mut sub = String::new();
            let mut sub_count = 0usize;
            for ch in line.chars() {
                sub.push(ch);
                sub_count += 1;
                if sub_count >= max_chars {
                    let s = sub.trim().to_string();
                    if !s.is_empty() {
                        parts.push(s);
                    }
                    sub.clear();
                    sub_count = 0;
                }
            }
            if !sub.trim().is_empty() {
                current.push_str(sub.trim());
                count = current.chars().count();
            }
        } else {
            if !current.is_empty() {
                current.push('\n');
                count += 1;
            }
            current.push_str(line);
            count += line_len;
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

fn parse_claims_list(claims_raw: &str) -> Vec<String> {
    let trimmed = claims_raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if let Ok(arr) = serde_json::from_str::<Vec<String>>(trimmed) {
        let cleaned: Vec<String> = arr
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !cleaned.is_empty() {
            return cleaned;
        }
    }
    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(trimmed) {
        let cleaned: Vec<String> = arr
            .into_iter()
            .filter_map(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .or_else(|| {
                        v.get("text")
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string())
                    })
                    .or_else(|| {
                        v.get("content")
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string())
                    })
            })
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !cleaned.is_empty() {
            return cleaned;
        }
    }

    // 纯文本按行首 `1.` / `1、` / `权利要求1` / `Claim 1` 切分
    let Ok(split_re) = Regex::new(r"(?m)^(?:\s*(?:权利要求\s*|Claim\s*)?\d+[.、:：\s])")
    else {
        return vec![trimmed.to_string()];
    };
    let mut starts: Vec<usize> = split_re.find_iter(trimmed).map(|m| m.start()).collect();
    if starts.is_empty() {
        return vec![trimmed.to_string()];
    }
    if starts.first().copied() != Some(0) {
        starts.insert(0, 0);
    }
    let mut out = Vec::new();
    for (idx, &start) in starts.iter().enumerate() {
        let end = starts.get(idx + 1).copied().unwrap_or(trimmed.len());
        if let Some(slice) = trimmed.get(start..end) {
            let s = slice.trim().to_string();
            if !s.is_empty() {
                out.push(s);
            }
        }
    }
    out
}

fn detect_patent_section_kind(line: &str) -> Option<SectionKind> {
    let t = line.trim();
    if t.chars().count() > 60 {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    if t.contains("技术领域") || lower.contains("technical field") || lower.contains("field of the")
    {
        return Some(SectionKind::TechField);
    }
    if t.contains("背景技术") || lower.contains("background") {
        return Some(SectionKind::Background);
    }
    if t.contains("发明内容")
        || t.contains("实用新型内容")
        || lower.contains("summary of the invention")
        || lower == "summary"
    {
        return Some(SectionKind::Summary);
    }
    if t.contains("附图说明") || lower.contains("brief description of the drawings") {
        return Some(SectionKind::Drawings);
    }
    if t.contains("具体实施方式")
        || t.contains("实施例")
        || lower.contains("detailed description")
        || lower.contains("embodiment")
    {
        return Some(SectionKind::Embodiment);
    }
    None
}

fn detect_embodiment_group(text: &str) -> Option<String> {
    if let Ok(re) = Regex::new(r"(实施例[一二三四五六七八九十\d]+|(?i)Embodiment\s*\d+)")
    {
        if let Some(m) = re.find(text) {
            return Some(m.as_str().trim().to_string());
        }
    }
    None
}

fn extract_figure_numerals(doc_id: &str, chunks: &[DocChunk]) -> Vec<FigureNumeral> {
    let mut out = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();

    let re_code_first =
        Regex::new(r"(\d+[a-zA-Z]?)\s*[、\-—:：]\s*([\p{Han}A-Za-z0-9_]{2,20})").ok();
    let re_paren = Regex::new(r"([\p{Han}A-Za-z0-9_]{2,20})\s*[（(](\d+[a-zA-Z]?)[)）]").ok();
    let re_inline = Regex::new(r"([\p{Han}]{2,12})(\d+[a-zA-Z]?)").ok();

    for chunk in chunks {
        if !matches!(
            chunk.section_kind,
            SectionKind::Drawings
                | SectionKind::Embodiment
                | SectionKind::Fallback
                | SectionKind::Summary
        ) {
            continue;
        }
        let text = &chunk.raw_content;
        if let Some(re) = &re_code_first {
            for cap in re.captures_iter(text) {
                if let (Some(code), Some(part)) = (cap.get(1), cap.get(2)) {
                    let c = code.as_str().trim().to_string();
                    let p = part.as_str().trim().to_string();
                    if seen.insert((c.clone(), p.clone())) {
                        out.push(FigureNumeral {
                            id: 0,
                            doc_id: doc_id.to_string(),
                            figure_label: String::new(),
                            numeral_code: c,
                            part_name: p,
                            embodiment_group: chunk.embodiment_group.clone(),
                            anchor_tag: chunk.section_tag.clone(),
                        });
                    }
                }
            }
        }
        if let Some(re) = &re_paren {
            for cap in re.captures_iter(text) {
                if let (Some(part), Some(code)) = (cap.get(1), cap.get(2)) {
                    let c = code.as_str().trim().to_string();
                    let p = part.as_str().trim().to_string();
                    if seen.insert((c.clone(), p.clone())) {
                        out.push(FigureNumeral {
                            id: 0,
                            doc_id: doc_id.to_string(),
                            figure_label: String::new(),
                            numeral_code: c,
                            part_name: p,
                            embodiment_group: chunk.embodiment_group.clone(),
                            anchor_tag: chunk.section_tag.clone(),
                        });
                    }
                }
            }
        }
        if let Some(re) = &re_inline {
            for cap in re.captures_iter(text) {
                if let (Some(part), Some(code)) = (cap.get(1), cap.get(2)) {
                    let c = code.as_str().trim().to_string();
                    let p = part.as_str().trim().to_string();
                    // 过滤如“实施例1”“第2”等非部件标号
                    if p.starts_with("实施例") || p.ends_with('第') || p == "步骤" || p == "附图"
                    {
                        continue;
                    }
                    if seen.insert((c.clone(), p.clone())) {
                        out.push(FigureNumeral {
                            id: 0,
                            doc_id: doc_id.to_string(),
                            figure_label: String::new(),
                            numeral_code: c,
                            part_name: p,
                            embodiment_group: chunk.embodiment_group.clone(),
                            anchor_tag: chunk.section_tag.clone(),
                        });
                    }
                }
            }
        }
    }
    out
}

fn chunk_patent(doc: &KnowledgeDoc) -> (Vec<DocChunk>, Vec<FigureNumeral>) {
    let mut chunks = Vec::new();
    let mut used_tags = HashSet::new();

    // 1. 摘要
    if !doc.abstract_text.trim().is_empty() {
        let tag = ensure_unique_tag("Desc:Abstract".to_string(), &mut used_tags);
        chunks.push(make_chunk(
            &doc.doc_id,
            SectionKind::Abstract,
            tag,
            None,
            None,
            None,
            None,
            None,
            doc.abstract_text.trim().to_string(),
        ));
    }

    // 2. 权利要求
    let claims = parse_claims_list(&doc.claims_json);
    let re_lead_no = Regex::new(r"^\s*(?:权利要求\s*|(?i)claim\s*)?(\d+)").ok();
    let re_dep_zh = Regex::new(r"(?:如|根据|所述)权利要求\s*(\d+)").ok();
    let re_dep_en = Regex::new(r"(?i)(?:according\s+to|of|as\s+claimed\s+in)\s+claim\s*(\d+)").ok();

    for (idx, claim_text) in claims.into_iter().enumerate() {
        let claim_no = re_lead_no
            .as_ref()
            .and_then(|re| re.captures(&claim_text))
            .and_then(|cap| cap.get(1))
            .and_then(|m| m.as_str().parse::<i32>().ok())
            .unwrap_or((idx + 1) as i32);

        let mut parent_claim_no = re_dep_zh
            .as_ref()
            .and_then(|re| re.captures(&claim_text))
            .and_then(|cap| cap.get(1))
            .and_then(|m| m.as_str().parse::<i32>().ok());

        if parent_claim_no.is_none() {
            parent_claim_no = re_dep_en
                .as_ref()
                .and_then(|re| re.captures(&claim_text))
                .and_then(|cap| cap.get(1))
                .and_then(|m| m.as_str().parse::<i32>().ok());
        }

        if parent_claim_no == Some(claim_no) {
            parent_claim_no = None;
        }

        let section_kind = if parent_claim_no.is_some() {
            SectionKind::ClaimDep
        } else {
            SectionKind::ClaimIndep
        };
        let tag = ensure_unique_tag(format!("Claim:{claim_no}"), &mut used_tags);
        chunks.push(make_chunk(
            &doc.doc_id,
            section_kind,
            tag,
            Some(claim_no),
            parent_claim_no,
            None,
            None,
            None,
            claim_text,
        ));
    }

    // 3. 说明书全文
    let fulltext = doc.fulltext_raw.trim();
    if !fulltext.is_empty() {
        let re_para = Regex::new(r"(?:\[(\d{1,5})\]|【(\d{1,5})】)").ok();
        let para_matches: Vec<(usize, i32)> = re_para
            .as_ref()
            .map(|re| {
                re.captures_iter(fulltext)
                    .filter_map(|cap| {
                        let m = cap.get(0)?;
                        let num_str = cap.get(1).or_else(|| cap.get(2))?.as_str();
                        let num = num_str.parse::<i32>().ok()?;
                        Some((m.start(), num))
                    })
                    .collect()
            })
            .unwrap_or_default();

        if !para_matches.is_empty() {
            let mut current_kind = SectionKind::Fallback;
            let mut current_embodiment: Option<String> = None;

            // 处理首个段号之前的前置章节头
            if let Some(&(first_start, _)) = para_matches.first() {
                if first_start > 0 {
                    if let Some(prefix) = fulltext.get(0..first_start) {
                        for line in prefix.lines() {
                            if let Some(k) = detect_patent_section_kind(line) {
                                current_kind = k;
                            }
                            if let Some(emb) = detect_embodiment_group(line) {
                                current_embodiment = Some(emb);
                            }
                        }
                    }
                }
            }

            for (i, &(start, para_no)) in para_matches.iter().enumerate() {
                let end = para_matches
                    .get(i + 1)
                    .map(|&(s, _)| s)
                    .unwrap_or(fulltext.len());
                let Some(raw_seg) = fulltext.get(start..end) else {
                    continue;
                };
                let seg = raw_seg.trim();
                if seg.is_empty() {
                    continue;
                }
                for line in seg.lines() {
                    if let Some(k) = detect_patent_section_kind(line) {
                        current_kind = k;
                    }
                }
                if let Some(emb) = detect_embodiment_group(seg) {
                    current_embodiment = Some(emb);
                    if current_kind == SectionKind::Fallback {
                        current_kind = SectionKind::Embodiment;
                    }
                }
                let tag = ensure_unique_tag(format!("Desc:[{:04}]", para_no), &mut used_tags);
                chunks.push(make_chunk(
                    &doc.doc_id,
                    current_kind,
                    tag,
                    None,
                    None,
                    Some(para_no),
                    Some(para_no),
                    current_embodiment.clone(),
                    seg.to_string(),
                ));
            }
        } else {
            // 无段号：按章节与 800 字符安全切分
            let mut current_kind = SectionKind::Fallback;
            let mut current_embodiment: Option<String> = None;
            let mut buf = String::new();
            let mut seg_idx = 1usize;

            let flush_buf = |buf: &mut String,
                             kind: SectionKind,
                             emb: &Option<String>,
                             seg_idx: &mut usize,
                             chunks: &mut Vec<DocChunk>,
                             used_tags: &mut HashSet<String>| {
                for part in split_by_char_limit(buf, 800) {
                    let tag = ensure_unique_tag(format!("Desc:Seg{}", *seg_idx), used_tags);
                    *seg_idx += 1;
                    chunks.push(make_chunk(
                        &doc.doc_id,
                        kind,
                        tag,
                        None,
                        None,
                        None,
                        None,
                        emb.clone(),
                        part,
                    ));
                }
                buf.clear();
            };

            for line in fulltext.lines() {
                if let Some(next_kind) = detect_patent_section_kind(line) {
                    if !buf.trim().is_empty() {
                        flush_buf(
                            &mut buf,
                            current_kind,
                            &current_embodiment,
                            &mut seg_idx,
                            &mut chunks,
                            &mut used_tags,
                        );
                    }
                    current_kind = next_kind;
                }
                if let Some(emb) = detect_embodiment_group(line) {
                    current_embodiment = Some(emb);
                }
                buf.push_str(line);
                buf.push('\n');
            }
            if !buf.trim().is_empty() {
                flush_buf(
                    &mut buf,
                    current_kind,
                    &current_embodiment,
                    &mut seg_idx,
                    &mut chunks,
                    &mut used_tags,
                );
            }
        }
    }

    let numerals = extract_figure_numerals(&doc.doc_id, &chunks);
    (chunks, numerals)
}

fn detect_paper_section_kind(heading: &str) -> SectionKind {
    let lower = heading.trim().to_ascii_lowercase();
    if lower.contains("table")
        || lower.contains("figure")
        || heading.contains("表")
        || heading.contains("图")
    {
        return SectionKind::PaperTableFig;
    }
    if lower.contains("limitation")
        || lower.contains("conclusion")
        || lower.contains("future work")
        || heading.contains("局限")
        || heading.contains("结论")
    {
        return SectionKind::PaperLimitation;
    }
    if lower.contains("experiment")
        || lower.contains("evaluation")
        || lower.contains("ablation")
        || lower.contains("result")
        || heading.contains("实验")
        || heading.contains("评估")
    {
        return SectionKind::PaperExperiment;
    }
    if lower.contains("method")
        || lower.contains("approach")
        || lower.contains("architecture")
        || lower.contains("framework")
        || heading.contains("方法")
        || heading.contains("原理")
    {
        return SectionKind::PaperMethod;
    }
    if lower.contains("intro")
        || lower.contains("related work")
        || lower.contains("background")
        || heading.contains("引言")
        || heading.contains("绪论")
    {
        return SectionKind::PaperIntro;
    }
    SectionKind::Fallback
}

fn is_paper_heading(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() || t.chars().count() > 100 {
        return false;
    }
    if t.starts_with('#') {
        return true;
    }
    if let Ok(re) = Regex::new(
        r"^(?:\d+(?:\.\d+)*\.?\s+\S+|(?i)(?:Introduction|Method|Methodology|Experiments?|Evaluation|Results?|Conclusion|Limitations?|Table\s+\d+|Figure\s+\d+)\b)",
    ) {
        return re.is_match(t);
    }
    false
}

fn chunk_paper_or_other(doc: &KnowledgeDoc) -> (Vec<DocChunk>, Vec<FigureNumeral>) {
    let mut chunks = Vec::new();
    let mut used_tags = HashSet::new();

    if !doc.abstract_text.trim().is_empty() {
        let tag = ensure_unique_tag("Paper:Abstract".to_string(), &mut used_tags);
        chunks.push(make_chunk(
            &doc.doc_id,
            SectionKind::Abstract,
            tag,
            None,
            None,
            None,
            None,
            None,
            doc.abstract_text.trim().to_string(),
        ));
    }

    let fulltext = doc.fulltext_raw.trim();
    if !fulltext.is_empty() {
        let mut current_kind = SectionKind::PaperIntro;
        let mut buf = String::new();
        let mut sec_idx = 1usize;
        let mut table_idx = 1usize;

        let flush_paper_buf = |buf: &mut String,
                               kind: SectionKind,
                               sec_idx: &mut usize,
                               table_idx: &mut usize,
                               chunks: &mut Vec<DocChunk>,
                               used_tags: &mut HashSet<String>| {
            for part in split_by_char_limit(buf, 1200) {
                let raw_tag = if kind == SectionKind::PaperTableFig {
                    let t = format!("Paper:Table{}", *table_idx);
                    *table_idx += 1;
                    t
                } else {
                    let t = format!("Paper:Sec{}", *sec_idx);
                    *sec_idx += 1;
                    t
                };
                let tag = ensure_unique_tag(raw_tag, used_tags);
                chunks.push(make_chunk(
                    &doc.doc_id,
                    kind,
                    tag,
                    None,
                    None,
                    None,
                    None,
                    None,
                    part,
                ));
            }
            buf.clear();
        };

        for line in fulltext.lines() {
            if is_paper_heading(line) {
                if !buf.trim().is_empty() {
                    flush_paper_buf(
                        &mut buf,
                        current_kind,
                        &mut sec_idx,
                        &mut table_idx,
                        &mut chunks,
                        &mut used_tags,
                    );
                }
                current_kind = detect_paper_section_kind(line);
            }
            buf.push_str(line);
            buf.push('\n');
        }
        if !buf.trim().is_empty() {
            flush_paper_buf(
                &mut buf,
                current_kind,
                &mut sec_idx,
                &mut table_idx,
                &mut chunks,
                &mut used_tags,
            );
        }
    }

    (chunks, Vec::new())
}

/// 将统一文献资产切分为结构化 `DocChunk` 列表与附图部件标号词典
pub fn chunk_document(doc: &KnowledgeDoc) -> (Vec<DocChunk>, Vec<FigureNumeral>) {
    match doc.doc_kind {
        DocKind::Patent => chunk_patent(doc),
        _ => chunk_paper_or_other(doc),
    }
}
