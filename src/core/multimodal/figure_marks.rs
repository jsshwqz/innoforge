//! 专利附图部件标号提取与正文联动（`src/core/multimodal/figure_marks.rs`）
//!
//! 识别中文/英文专利附图标记说明及正文中的部件标号（如 `10-壳体`、`12a. 第一导流板`、
//! `附图标记：1、主轴；2、轴承座`、`housing (102)`），关联 `figure_label` 与 `embodiment_group`，
//! 并提供正文标号命中对照表供前端悬停高亮。

use crate::core::vault::models::FigureNumeral;
use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

fn fig_label_regex() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(图\s*\d+[A-Za-z]?|Fig(?:ure)?\.?\s*\d+[A-Za-z]?)").ok())
        .as_ref()
}

fn embodiment_regex() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(实施例\s*[一二三四五六七八九十\d]+|Embodiment\s*\d+|Example\s*\d+)").ok()
    })
    .as_ref()
}

fn zh_numeral_pair_regex() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| {
        // 匹配如：10-壳体、10：底座、12a. 第一导流板、1、主轴、10 壳体（限定分隔符或空白）
        Regex::new(r"(?:^|[\s；;，,：:、])(\d{1,4}[a-zA-Z]{0,2}'?)\s*[-—:：\.、]\s*([^\d\s；;，,。\(\)（）\[\]]{1,30}(?:\s+[^\d\s；;，,。\(\)（）\[\]]{1,20}){0,2})").ok()
    })
    .as_ref()
}

fn zh_space_pair_regex() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| {
        // 仅在“附图标记/标号说明”上下文中启用空格分隔：如 `10 壳体`
        Regex::new(r"(?:^|[\s；;，,])(\d{1,4}[a-zA-Z]{0,2}'?)\s+([\p{Han}A-Za-z][\p{Han}A-Za-z0-9_\-]{0,29})").ok()
    })
    .as_ref()
}

fn en_paren_numeral_regex() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| {
        // 匹配英文专利正文如：`housing (102)`、`first baffle plate (12a)`
        Regex::new(r"\b([A-Za-z][A-Za-z\-]{1,25}(?:\s+[A-Za-z][A-Za-z\-]{1,25}){0,3})\s*\(\s*(\d{1,4}[a-zA-Z]{0,2}'?)\s*\)").ok()
    })
    .as_ref()
}

fn para_anchor_regex() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[(\d{4,5})\]").ok())
        .as_ref()
}

fn is_non_part_heading(part_name: &str) -> bool {
    let trimmed = part_name.trim();
    if trimmed.is_empty() {
        return true;
    }
    const STOP_PREFIXES: &[&str] = &[
        "背景技术",
        "发明内容",
        "技术领域",
        "附图说明",
        "具体实施方式",
        "权利要求",
        "摘要",
        "实施例",
        "Background",
        "Summary",
        "Abstract",
        "Claims",
        "Description",
        "Introduction",
        "Method",
        "Embodiment",
        "Figure",
        "Fig",
        "Table",
    ];
    STOP_PREFIXES
        .iter()
        .any(|prefix| trimmed.eq_ignore_ascii_case(prefix) || trimmed.starts_with(prefix))
}

fn is_valid_part_name(name: &str) -> bool {
    let trimmed = name.trim();
    let char_count = trimmed.chars().count();
    if char_count == 0 || char_count > 40 {
        return false;
    }
    if is_non_part_heading(trimmed) {
        return false;
    }
    // 至少包含一个汉字或英文字母，避免误匹配纯数字/日期/版本号
    trimmed
        .chars()
        .any(|c| c.is_ascii_alphabetic() || ('\u{4E00}'..='\u{9FFF}').contains(&c))
}

fn is_en_stop_phrase(phrase: &str) -> bool {
    let lower = phrase.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "fig"
            | "figure"
            | "table"
            | "eq"
            | "equation"
            | "see"
            | "in"
            | "of"
            | "and"
            | "or"
            | "claim"
            | "claims"
            | "step"
            | "page"
            | "year"
    )
}

fn trim_en_leading_noise(raw: &str) -> String {
    const LEADING_STOP: &[&str] = &[
        "illustrates",
        "illustrating",
        "shows",
        "showing",
        "depicts",
        "depicting",
        "includes",
        "including",
        "comprises",
        "comprising",
        "has",
        "having",
        "with",
        "and",
        "or",
        "the",
        "a",
        "an",
        "of",
        "in",
        "on",
        "to",
        "for",
        "from",
        "by",
        "into",
        "at",
    ];
    let mut words: Vec<&str> = raw.split_whitespace().collect();
    while let Some(first) = words.first() {
        let lower = first.to_ascii_lowercase();
        if LEADING_STOP.contains(&lower.as_str()) {
            words.remove(0);
        } else {
            break;
        }
    }
    words.join(" ")
}

fn clean_part_name(raw: &str) -> String {
    raw.trim()
        .trim_matches(|c: char| {
            matches!(
                c,
                '，' | ',' | '；' | ';' | '。' | '.' | '：' | ':' | '、' | ')' | '）'
            )
        })
        .trim()
        .to_string()
}

fn normalize_figure_label(raw: &str) -> String {
    let compact: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.starts_with('图') {
        compact.replace(' ', "")
    } else {
        compact
    }
}

fn normalize_embodiment(raw: &str) -> String {
    let compact: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.starts_with("实施例") {
        compact.replace(' ', "")
    } else {
        compact
    }
}

/// 从专利文本或附图说明中抽取部件标号词典，自动关联所属图号（`figure_label`）与实施例（`embodiment_group`）。
pub fn extract_figure_numerals(doc_id: &str, text: &str) -> Vec<FigureNumeral> {
    let mut results = Vec::new();
    let mut seen: HashSet<(String, String, String)> = HashSet::new();

    let mut current_figure = String::new();
    let mut current_embodiment: Option<String> = None;
    let mut in_legend_block = false;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            in_legend_block = false;
            continue;
        }

        if let Some(re) = fig_label_regex() {
            if let Some(m) = re.find(line) {
                current_figure = normalize_figure_label(m.as_str());
            }
        }

        if let Some(re) = embodiment_regex() {
            if let Some(m) = re.find(line) {
                current_embodiment = Some(normalize_embodiment(m.as_str()));
            }
        }

        if line.contains("附图标记")
            || line.contains("标号说明")
            || line.contains("附图说明")
            || line.to_ascii_lowercase().contains("reference numeral")
        {
            in_legend_block = true;
        }

        let anchor_tag = if let Some(re) = para_anchor_regex() {
            if let Some(m) = re.find(line) {
                format!("Desc:{}", m.as_str())
            } else if !current_figure.is_empty() {
                format!("Fig:{}", current_figure)
            } else {
                "Drawings".to_string()
            }
        } else if !current_figure.is_empty() {
            format!("Fig:{}", current_figure)
        } else {
            "Drawings".to_string()
        };

        let mut line_matched = false;

        if let Some(re) = zh_numeral_pair_regex() {
            for cap in re.captures_iter(line) {
                let code = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                let part = clean_part_name(cap.get(2).map(|m| m.as_str()).unwrap_or(""));
                if !code.is_empty() && is_valid_part_name(&part) {
                    let emb_key = current_embodiment.clone().unwrap_or_default();
                    let dedup_key = (code.to_string(), part.clone(), emb_key);
                    if seen.insert(dedup_key) {
                        results.push(FigureNumeral {
                            id: 0,
                            doc_id: doc_id.to_string(),
                            figure_label: current_figure.clone(),
                            numeral_code: code.to_string(),
                            part_name: part,
                            embodiment_group: current_embodiment.clone(),
                            anchor_tag: anchor_tag.clone(),
                        });
                        line_matched = true;
                    }
                }
            }
        }

        if in_legend_block && !line_matched {
            if let Some(re) = zh_space_pair_regex() {
                for cap in re.captures_iter(line) {
                    let code = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                    let part = clean_part_name(cap.get(2).map(|m| m.as_str()).unwrap_or(""));
                    if !code.is_empty() && is_valid_part_name(&part) {
                        let emb_key = current_embodiment.clone().unwrap_or_default();
                        let dedup_key = (code.to_string(), part.clone(), emb_key);
                        if seen.insert(dedup_key) {
                            results.push(FigureNumeral {
                                id: 0,
                                doc_id: doc_id.to_string(),
                                figure_label: current_figure.clone(),
                                numeral_code: code.to_string(),
                                part_name: part,
                                embodiment_group: current_embodiment.clone(),
                                anchor_tag: anchor_tag.clone(),
                            });
                        }
                    }
                }
            }
        }

        if let Some(re) = en_paren_numeral_regex() {
            for cap in re.captures_iter(line) {
                let part = trim_en_leading_noise(&clean_part_name(
                    cap.get(1).map(|m| m.as_str()).unwrap_or(""),
                ));
                let code = cap.get(2).map(|m| m.as_str().trim()).unwrap_or("");
                if !code.is_empty() && !is_en_stop_phrase(&part) && is_valid_part_name(&part) {
                    let emb_key = current_embodiment.clone().unwrap_or_default();
                    let dedup_key = (code.to_string(), part.clone(), emb_key);
                    if seen.insert(dedup_key) {
                        results.push(FigureNumeral {
                            id: 0,
                            doc_id: doc_id.to_string(),
                            figure_label: current_figure.clone(),
                            numeral_code: code.to_string(),
                            part_name: part,
                            embodiment_group: current_embodiment.clone(),
                            anchor_tag: anchor_tag.clone(),
                        });
                    }
                }
            }
        }
    }

    results
}

/// 扫描正文并返回命中的 `(numeral_code, part_name)` 对照表，供前端悬停高亮。
///
/// 采用 ASCII 字母数字边界检查，防止标号 `"1"` 误命中 `"10"` 或 `"12a"`。
pub fn annotate_text_with_numerals(
    text: &str,
    glossary: &[FigureNumeral],
) -> Vec<(String, String)> {
    let mut hits = Vec::new();
    let mut seen_codes: HashSet<String> = HashSet::new();

    for item in glossary {
        let code = item.numeral_code.trim();
        if code.is_empty() || seen_codes.contains(code) {
            continue;
        }

        let mut matched = false;
        for (idx, _) in text.match_indices(code) {
            let prev_ok = text
                .get(0..idx)
                .and_then(|prefix| prefix.chars().next_back())
                .map(|c| !c.is_ascii_alphanumeric())
                .unwrap_or(true);
            let after_idx = idx + code.len();
            let next_ok = text
                .get(after_idx..)
                .and_then(|suffix| suffix.chars().next())
                .map(|c| !c.is_ascii_alphanumeric())
                .unwrap_or(true);

            if prev_ok && next_ok {
                matched = true;
                break;
            }
        }

        if matched {
            seen_codes.insert(code.to_string());
            hits.push((code.to_string(), item.part_name.clone()));
        }
    }

    hits
}
