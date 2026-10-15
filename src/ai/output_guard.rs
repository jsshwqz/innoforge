//! 输出 Guard：答复书生成后、返回调用方之前的最终校验。
//!
//! 三项校验全过才输出，否则拒绝并报错：
//! 1. 引文白名单：输出中所有公开号必须在 reference_documents 白名单内
//! 2. 计数一致性：对每个声明"X出现N次"的断言，重新 count 验证
//! 3. 内部路径/OCR残留：grep data/uploads / tesseract / OcrFallback / TextLayer / normalize / uuid
//! 4. R6b 引文风格：英文缩写 vs/VS
//! 5. R6b 引文风格："等"字结尾模糊列举接肯定断言

use super::case_input::{CaseInput, GuardResult};

/// 拋留模式列表（grep 出现即拒绝）。
const RESIDUE_PATTERNS: &[&str] = &[
    "data/uploads",
    "tesseract",
    "OcrFallback",
    "TextLayer",
    "TEXT_LAYER",
    "OCR_FALLBACK",
    "normalize",
    "A9.1",
];

/// uuid 正则前缀（8-4-4-4-12 格式）。
fn contains_uuid(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 36 <= len {
        if is_uuid_at(bytes, i) {
            return Some(text[i..i + 36].to_string());
        }
        i += 1;
    }
    None
}

fn is_uuid_at(bytes: &[u8], pos: usize) -> bool {
    let layout = [8, 4, 4, 4, 12];
    let mut idx = pos;
    for (gi, &group_len) in layout.iter().enumerate() {
        for _ in 0..group_len {
            if idx >= bytes.len() || !bytes[idx].is_ascii_hexdigit() {
                return false;
            }
            idx += 1;
        }
        if gi < layout.len() - 1 {
            if idx >= bytes.len() || bytes[idx] != b'-' {
                return false;
            }
            idx += 1;
        }
    }
    true
}

/// 提取文本中所有类似公开号的字符串（CN/US/EP/JP + 数字+字母）。
fn extract_publication_numbers(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let prefixes = ["CN", "US", "EP", "JP", "WO", "KR", "DE", "FR", "GB"];

    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for &(ci, _c) in &chars {
        let two_char: String = text[ci..].chars().take(2).collect();
        if prefixes.contains(&two_char.as_str()) {
            let rest: String = text[ci + 2..]
                .chars()
                .take_while(|&c| c.is_ascii_alphanumeric())
                .collect();
            if rest.len() >= 7 && rest.len() <= 12 {
                let pub_num = format!("{}{}", two_char, rest);
                if !result.contains(&pub_num) {
                    result.push(pub_num);
                }
            }
        }
    }
    result
}

/// 运行全部校验。
pub fn run_guard(output_text: &str, input: &CaseInput) -> GuardResult {
    let mut errors = Vec::new();

    // 1. 引文白名单
    let whitelist: Vec<String> = input
        .reference_documents
        .iter()
        .map(|r| r.publication_number.clone())
        .collect();
    let case_num = &input.case_meta.application_number;
    let pub_nums_in_output = extract_publication_numbers(output_text);
    for pn in &pub_nums_in_output {
        if !whitelist.contains(pn) && pn != case_num {
            errors.push(format!(
                "引文白名单校验失败：输出含公开号 {} 但不在 reference_documents 白名单中",
                pn
            ));
        }
    }

    // 2. 计数一致性（在 Phase 2 实现，此处跳过）

    // 3. 内部路径/OCR残留
    for pattern in RESIDUE_PATTERNS {
        if output_text.contains(pattern) {
            errors.push(format!("内部残留校验失败：输出含禁止模式 '{}'", pattern));
        }
    }
    if let Some(uuid) = contains_uuid(output_text) {
        errors.push(format!("UUID残留校验失败：输出含UUID '{}'", uuid));
    }

    // 4. R6b 引文风格守卫：英文缩写 vs/VS
    for finding in detect_english_abbreviations(output_text) {
        errors.push(finding);
    }

    // 5. R6b 引文风格守卫："等"字结尾模糊列举接肯定断言
    for finding in detect_fuzzy_enumeration_assertion(output_text) {
        errors.push(finding);
    }

    GuardResult {
        passed: errors.is_empty(),
        errors,
    }
}

/// R6b 规则1：检测正式文书中引号外单独出现的英文缩写 vs/VS
///
/// 返回错误消息含替换建议：vs → "，而非"
fn detect_english_abbreviations(text: &str) -> Vec<String> {
    let mut findings = Vec::new();
    let lower = text.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let byte_offsets: Vec<usize> = lower.char_indices().map(|(i, _)| i).collect();

    for i in 0..chars.len().saturating_sub(1) {
        if chars[i] == 'v' && chars[i + 1] == 's' {
            let prev_ok = i == 0 || !chars[i - 1].is_ascii_alphabetic();
            let next_ok = i + 2 >= chars.len() || !chars[i + 2].is_ascii_alphabetic();
            if prev_ok && next_ok {
                let start = byte_offsets[i];
                let end = if i + 2 < byte_offsets.len() {
                    byte_offsets[i + 2]
                } else {
                    text.len()
                };
                let original = &text[start..end];
                findings.push(format!(
                    "引文风格守卫：正式文书中引号外禁止英文缩写 '{}'，建议替换为'，而非'",
                    original
                ));
            }
        }
    }

    findings
}

/// R6b 规则2：检测"等"字结尾的模糊列举后接肯定断言
///
/// 模式："...等" + 逗号/句号 + 肯定断言（如"可以""能够""应当""显然"）
/// 这种写法用模糊列举支撑肯定结论，在正式文书中应避免。
fn detect_fuzzy_enumeration_assertion(text: &str) -> Vec<String> {
    let mut findings = Vec::new();

    for (idx, _) in text.match_indices('等') {
        let after_start = idx + '等'.len_utf8();
        let after_end = (after_start + 40).min(text.len());
        let after = &text[after_start..after_end];

        // 去除引号/括号/空格/逗号/句号/顿号前缀
        let stripped = after.trim_start_matches(|c: char| {
            c == '"'
                || c == '\u{201d}'
                || c == ')'
                || c == ' '
                || c == '\u{ff0c}'
                || c == '\u{3002}'
                || c == '\u{3001}'
                || c == '\u{ff1b}'
        });

        let assertion_patterns = ["可以", "能够", "应当", "显然", "必然", "确定"];
        let has_assertion = assertion_patterns.iter().any(|p| stripped.starts_with(p));

        if has_assertion {
            let before_start = idx.saturating_sub(30);
            let before = &text[before_start..idx];
            // "等"前面有顿号、逗号等列举标志
            let has_list_marker =
                before.contains('、') || before.contains('，') || before.contains('；');
            if has_list_marker {
                findings.push(format!(
                    "引文风格守卫：'等'字结尾的模糊列举后接肯定断言（位置{}附近），建议改为明确列举或删除肯定断言",
                    idx
                ));
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::super::case_input::{CaseMeta, ReferenceDocument};
    use super::*;

    fn make_input() -> CaseInput {
        CaseInput {
            case_meta: CaseMeta {
                application_number: "1990999999999".into(),
                applicant: "测试".into(),
                invention_title: "测试发明".into(),
                oa_date: None,
                oa_type: "reject_review".into(),
                claim_basis_date: None,
                claim_basis_text: None,
            },
            oa_full_text: "OA".into(),
            reference_documents: vec![ReferenceDocument {
                label: "D1".into(),
                publication_number: "CN990001001A".into(),
                full_text: "D1原文".into(),
                extraction_method: None,
                page_count: None,
                technical_field: None,
            }],
            claims_under_review: "权利要求".into(),
            spec_effect_statements: vec![],
            spec_full_text: None,
            legacy_analysis: None,
            legacy_discussion: None,
            custom_stopwords: vec![],
        }
    }

    #[test]
    fn clean_output_passes() {
        let input = make_input();
        let output = "复审请求书\n申请号：1990999999999\nD1（CN990001001A）公开了某结构。";
        let result = run_guard(output, &input);
        assert!(result.passed, "errors: {:?}", result.errors);
    }

    #[test]
    fn detects_residue_pattern() {
        let input = make_input();
        let output = "正文包含 data/uploads/xxx.pdf 路径";
        let result = run_guard(output, &input);
        assert!(!result.passed);
        assert!(result.errors.iter().any(|e| e.contains("data/uploads")));
    }

    #[test]
    fn detects_uuid() {
        let input = make_input();
        let output = "正文含 d38ccf3f-2227-43f4-bdc1-7ae12b582c71 拋回件";
        let result = run_guard(output, &input);
        assert!(!result.passed);
        assert!(result.errors.iter().any(|e| e.contains("UUID")));
    }

    #[test]
    fn detects_unknown_pub_num() {
        let input = make_input();
        let output = "正文引用了 CN999999999A 但不在白名单";
        let result = run_guard(output, &input);
        assert!(!result.passed);
        assert!(result.errors.iter().any(|e| e.contains("CN999999999A")));
    }

    #[test]
    fn allows_whitelisted_pub_num() {
        let input = make_input();
        let output = "D1（CN990001001A）公开了某结构。申请号1990999999999。";
        let result = run_guard(output, &input);
        assert!(result.passed, "errors: {:?}", result.errors);
    }

    // ── R6b 新增测试 ──────────────────────────────────────────────

    #[test]
    fn detects_english_vs_abbreviation() {
        let input = make_input();
        let output = "D1的技术方案 vs D2的技术方案存在区别。";
        let result = run_guard(output, &input);
        assert!(!result.passed);
        assert!(result
            .errors
            .iter()
            .any(|e| e.contains("英文缩写") && e.contains("vs")));
    }

    #[test]
    fn detects_english_vs_uppercase() {
        let input = make_input();
        let output = "D1的技术方案 VS D2的技术方案。";
        let result = run_guard(output, &input);
        assert!(!result.passed);
        assert!(result.errors.iter().any(|e| e.contains("英文缩写")));
    }

    #[test]
    fn no_false_positive_vs_in_word() {
        let input = make_input();
        // "devices" contains "vs" but should not trigger
        let output = "D1（CN990001001A）公开了devices结构。";
        let result = run_guard(output, &input);
        assert!(
            !result.errors.iter().any(|e| e.contains("英文缩写")),
            "vs in devices should not trigger"
        );
    }

    #[test]
    fn detects_fuzzy_enumeration_with_assertion() {
        let input = make_input();
        let output = "D1、D2等，可以确定技术方案相同。";
        let result = run_guard(output, &input);
        assert!(!result.passed);
        assert!(result.errors.iter().any(|e| e.contains("模糊列举")));
    }

    #[test]
    fn no_false_positive_fuzzy_without_assertion() {
        let input = make_input();
        let output = "D1、D2等对比文件。";
        let result = run_guard(output, &input);
        assert!(
            !result.errors.iter().any(|e| e.contains("模糊列举")),
            "等 without assertion should not trigger"
        );
    }
}
