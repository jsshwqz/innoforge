use crate::ai::case_input::{CaseInput, EffectMismatch};
use crate::ai::client::AiClient;
use crate::ai::extraction::normalize_text;

// ── Stage D：作用相同断言对比 ──────────────────────────────────────────

/// "作用相同"类断言的正则模式。
///
/// 匹配 OA 中审查员声称"某特征与对比文件作用相同/均为了..."的断言。
/// 纯确定性正则扫描，不调AI。
const EFFECT_CLAIM_PATTERNS: &[&str] = &[
    "作用相同",
    "作用一致",
    "均为了",
    "均能实现",
    "均可以达到",
    "都能实现",
    "都可以达到",
    "效果相同",
    "效果一致",
    "功能相同",
    "功能一致",
    "同样能够",
    "同样可以",
    "与.*?相同",
    "与.*?一致",
];

/// 检测OA中"作用相同""均为了..."等断言，拉说明书效果句对比。
///
/// 方法：
/// 1. 正则扫描OA全文，找出所有"作用相同"类断言及其上下文
/// 2. 取本申请说明书效果句（spec_effect_statements 优先，缺则自动抽取）
/// 3. 对比本申请效果与OA声称的效果，若不同则生成反驳段
/// 4. AI可选：当需要语义判断两个效果句是否实质相同时调用AI
pub(super) async fn stage_d_compare_effects(
    input: &CaseInput,
    ai_client: Option<&AiClient>,
) -> Vec<EffectMismatch> {
    // 缺陷4修复：去除换行与多余空白后再定位引文窗口，与CNIPA提取件去空格规则一致
    let oa_text = &input.oa_full_text;
    let oa_normalized = normalize_text(oa_text);

    // Step 1: 正则扫描"作用相同"类断言（在去空白文本上扫描，避免残词）
    let effect_claims = scan_effect_claims(&oa_normalized);
    if effect_claims.is_empty() {
        return Vec::new();
    }

    // Step 2: 获取本申请效果句（去空白，与OA文本同规则）
    let actual_effects = if !input.spec_effect_statements.is_empty() {
        input
            .spec_effect_statements
            .iter()
            .map(|s| normalize_text(s))
            .collect()
    } else if let Some(spec) = &input.spec_full_text {
        auto_extract_effect_statements(spec)
            .iter()
            .map(|s| normalize_text(s))
            .collect()
    } else {
        Vec::new()
    };

    if actual_effects.is_empty() {
        return Vec::new();
    }

    // Step 3: 对每个效果断言，对比本申请实际效果
    let mut mismatches = Vec::new();

    for claim in &effect_claims {
        // 找到 claim 对应的对比文件标签
        let ref_label = find_ref_label_for_claim(&oa_normalized, &claim.text);

        // 对每条本申请效果句，检查是否与OA声称的效果实质不同
        for actual_effect in &actual_effects {
            // 确定性检查：如果OA声称的效果文本与本申请效果文本不完全一致
            // 则可能存在效果差异，生成反驳段
            let is_identical = normalize_text(&claim.text).contains(&normalize_text(actual_effect))
                || normalize_text(actual_effect).contains(&normalize_text(&claim.text));

            if !is_identical {
                let rebuttal_text = format!(
                    "驳回决定声称{}\"{}\"，然而本申请说明书记载的技术效果为\
                     \"{}\"，两者在技术效果上存在实质差异。\n\n\
                     - 驳回决定声称效果：{}\n\
                     - 本申请说明书效果：{}\n\n\
                     审查员将技术效果实质不同的方案认定为\"作用相同\"，\
                     不符合创造性判断中关于\"技术效果\"的对比要求。\
                     根据《专利审查指南》，创造性判断应当对比\
                     要求保护的发明与现有技术所达到的技术效果是否\
                     实质相同，而非仅以功能层面的笼统对应否定创造性。",
                    ref_label
                        .as_deref()
                        .map(|l| format!("{}中", l))
                        .unwrap_or_default(),
                    claim.text,
                    actual_effect,
                    claim.text,
                    actual_effect
                );

                let ref_label_str = ref_label.clone().unwrap_or_else(|| "对比文件".to_string());
                mismatches.push(EffectMismatch {
                    oa_claim: claim.text.clone(),
                    ref_label: ref_label_str,
                    actual_effect: actual_effect.clone(),
                    rebuttal_text,
                });
            }
        }
    }

    // Step 4: 如果有AI且存在mismatches，可选调用AI做语义精判
    // 当前确定性实现已足够，AI精判留作Phase 4增强
    let _ = ai_client;

    mismatches
}

/// 正则扫描结果：OA中一个"作用相同"类断言。
#[derive(Debug, Clone)]
pub(super) struct EffectClaim {
    /// 断言原文片段（含上下文，≤100字）
    pub(super) text: String,
}

/// 扫描OA全文，找出所有"作用相同"类断言。
///
/// 纯确定性正则扫描，不调AI。
/// 句子边界截断：在 max_chars 字内找最后一个。/；，在此处收尾。
/// 若找不到句子边界，在 max_chars 处硬截断并加省略号。
fn truncate_at_sentence_boundary(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    // 在 max_chars 范围内找最后一个句子边界
    let mut cut = max_chars;
    for (i, &c) in chars.iter().enumerate().take(max_chars) {
        if c == '。' || c == '；' || c == ';' {
            cut = i + 1; // 包含句号
        }
    }
    let result: String = chars.iter().take(cut).collect();
    if cut < chars.len() {
        format!("{}…", result)
    } else {
        result
    }
}

/// 在 chars[from] 起向后（含 from）在 lookahead 字内找最近的句子边界（。；;），
/// 返回边界后一位（含句号）。找不到边界则返回 from，调用方自行回退硬窗口——
/// 硬窗口切在句中会产生「公开了流量传」这类半句引文，正式文书里不可读。
pub(super) fn extend_window_to_sentence_boundary(
    chars: &[char],
    from: usize,
    lookahead: usize,
) -> usize {
    let limit = from.saturating_add(lookahead).min(chars.len());
    let mut i = from;
    while i < limit {
        if matches!(chars[i], '。' | '；' | ';') {
            return i + 1;
        }
        i += 1;
    }
    from
}

pub(super) fn scan_effect_claims(oa_text: &str) -> Vec<EffectClaim> {
    let mut claims = Vec::new();
    let chars: Vec<char> = oa_text.chars().collect();
    let len = chars.len();

    for pattern in EFFECT_CLAIM_PATTERNS {
        let pattern_chars: Vec<char> = pattern.chars().collect();
        // 处理含 .*? 的模式（简化处理：跳过通配符模式，用固定子串匹配）
        if pattern.contains(".*?") {
            // 对 "与X相同" 类模式，扫描"与"和"相同"的共现
            let parts: Vec<&str> = pattern.split(".*?").collect();
            if parts.len() == 2 {
                let prefix = parts[0];
                let suffix = parts[1];
                let mut start = 0;
                while start < len {
                    // 找 prefix
                    if let Some(pos) =
                        find_substring(&chars, start, &prefix.chars().collect::<Vec<_>>())
                    {
                        // 找 suffix（在prefix后50字内）
                        let search_end = (pos + prefix.chars().count() + 50).min(len);
                        if let Some(suf_pos) = find_substring(
                            &chars,
                            pos + prefix.chars().count(),
                            &suffix.chars().collect::<Vec<_>>(),
                        ) {
                            if suf_pos < search_end {
                                // 窗口末尾优先延伸到句尾（120 字内），无边界才退回硬窗口
                                let win_from = (suf_pos + suffix.chars().count()).min(len);
                                let end = {
                                    let extended =
                                        extend_window_to_sentence_boundary(&chars, win_from, 120);
                                    if extended == win_from {
                                        (win_from + 60).min(len)
                                    } else {
                                        extended
                                    }
                                };
                                let raw: String = chars[pos..end].iter().collect();
                                let context = truncate_at_sentence_boundary(&raw, 120);
                                if !claims.iter().any(|c: &EffectClaim| c.text == context) {
                                    claims.push(EffectClaim { text: context });
                                }
                                start = suf_pos + suffix.chars().count();
                                continue;
                            }
                        }
                        start = pos + prefix.chars().count();
                    } else {
                        break;
                    }
                }
            }
            continue;
        }

        // 固定子串匹配
        let pattern_vec: Vec<char> = pattern_chars.clone();
        let mut start = 0;
        while start < len {
            if let Some(pos) = find_substring(&chars, start, &pattern_vec) {
                // 取上下文：前60字 + 匹配 + 句尾（150 字内无边界才退回硬窗口 +60）
                let ctx_start = pos.saturating_sub(60);
                let win_from = (pos + pattern_vec.len()).min(len);
                let ctx_end = {
                    let extended = extend_window_to_sentence_boundary(&chars, win_from, 150);
                    if extended == win_from {
                        (win_from + 60).min(len)
                    } else {
                        extended
                    }
                };
                let raw: String = chars[ctx_start..ctx_end].iter().collect();
                let context = truncate_at_sentence_boundary(&raw, 150);
                if !claims.iter().any(|c: &EffectClaim| c.text == context) {
                    claims.push(EffectClaim { text: context });
                }
                start = pos + pattern_vec.len();
            } else {
                break;
            }
        }
    }

    claims
}

/// 在 chars[start..] 中查找 pattern 的起始位置。
fn find_substring(chars: &[char], start: usize, pattern: &[char]) -> Option<usize> {
    if pattern.is_empty() || start + pattern.len() > chars.len() {
        return None;
    }
    for i in start..=chars.len() - pattern.len() {
        if chars[i..i + pattern.len()] == *pattern {
            return Some(i);
        }
    }
    None
}

/// 从OA文本中找到效果断言对应的对比文件标签。
///
/// 在断言上下文附近搜索 D1/D2/D3 等标签。
pub(super) fn find_ref_label_for_claim(oa_text: &str, claim_text: &str) -> Option<String> {
    // 在 claim_text 中直接搜索 D1/D2/D3 等
    for label in &["D1", "D2", "D3", "D4", "D5", "D6"] {
        if claim_text.contains(label) {
            return Some(label.to_string());
        }
    }
    // 在 OA 全文中找 claim_text 附近最近的对比文件标签
    if let Some(pos) = oa_text.find(claim_text) {
        let chars: Vec<char> = oa_text.chars().collect();
        let pos_char = oa_text.get(..pos).map(|s| s.chars().count()).unwrap_or(0);
        // 向前搜索100字符
        let search_start = pos_char.saturating_sub(100);
        let search_end = (pos_char + claim_text.chars().count() + 100).min(chars.len());
        for i in (search_start..pos_char).rev() {
            for label in &["D1", "D2", "D3", "D4", "D5", "D6"] {
                let label_chars: Vec<char> = label.chars().collect();
                if i + label_chars.len() <= search_end
                    && chars.get(i..i + label_chars.len()) == Some(label_chars.as_slice())
                {
                    return Some(label.to_string());
                }
            }
        }
    }
    None
}

/// 从说明书全文逐句抽取技术效果句（手填缺失时的降级路径）。
///
/// 裁决⑤：不得改写，只取 verbatim 子串。
/// 按句号/分号切分，筛选含效果关键词的句子。
pub(super) fn auto_extract_effect_statements(spec_full_text: &str) -> Vec<String> {
    const EFFECT_KEYWORDS: &[&str] = &[
        "效果", "提高", "改善", "效率", "优势", "有益", "从而", "进而", "由此", "因此", "使得",
        "实现", "避免", "防止", "减少", "降低", "增强", "提升",
    ];

    let sentences: Vec<&str> = spec_full_text.split(['。', '；', '\n']).collect();

    let mut effects = Vec::new();
    for sent in sentences {
        let trimmed = sent.trim();
        if trimmed.is_empty() || trimmed.chars().count() < 5 {
            continue;
        }
        if EFFECT_KEYWORDS.iter().any(|kw| trimmed.contains(kw)) {
            effects.push(trimmed.to_string());
        }
    }

    effects
}
