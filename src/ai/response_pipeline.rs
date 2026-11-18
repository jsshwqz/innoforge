//! 通用 OA→答复书管线。
//!
//! 五阶段确定性管线，取代 patent.rs 旧单案预置文本。
//! AI 只做语义判断（抽特征、填协同段、对比效果），结构决策全部确定性。
//!
//! ```text
//! CaseInput
//!   ├─ Stage A: OA区别特征抽取 → Vec<DistinguishingFeature>
//!   ├─ Stage B: 定性核词计数 → Vec<TermCountCheck> + Vec<UnsupportedClaim>
//!   ├─ Stage C: 逐特征三段式反驳 → Vec<FeatureRebuttal>
//!   ├─ Stage D: 作用相同断言对比 → Vec<EffectMismatch>
//!   ├─ Stage E: 程序性固定件组装 → ResponseLetter
//!   └─ Guard: 输出前校验
//! ```

use super::case_input::*;
use super::client::AiClient;
use super::extraction::{count_term_in_text, normalize_text};
use super::fact_check::{check_oa_analysis, format_report, FactCheckReport};
use super::output_guard::run_guard;

// ── 管线入口 ───────────────────────────────────────────────────────────

/// 生成完整答复书。
///
/// `ai_client` 为 None 时，Stage A/D 降级为空（不抽特征/不对比效果），
/// 仅 Stage B 确定性计数 + Stage E 拼接运行。用于测试和无AI环境。
pub async fn generate_response_letter_pipeline(
    input: &CaseInput,
    ai_client: Option<&AiClient>,
    ai_client_fast: Option<&AiClient>,
) -> ResponseLetter {
    let (features, dropped_features) = stage_a_extract_features(input, ai_client_fast).await;
    let (term_checks, unsupported_claims) = stage_b_count_terms(input, &features);
    let rebuttals = stage_c_rebut_features(input, &features).await;
    let effect_mismatches = stage_d_compare_effects(input, ai_client).await;
    let raw_text = stage_e_assemble(
        input,
        &features,
        &term_checks,
        &unsupported_claims,
        &rebuttals,
        &effect_mismatches,
        &dropped_features,
    );

    // R6c: 对生成文本运行事实校验，将报告附加为末尾固定段
    let references_text: String = input
        .reference_documents
        .iter()
        .map(|r| format!("[{}] {}\n{}", r.label, r.publication_number, r.full_text))
        .collect();
    let my_patent_text = input
        .spec_full_text
        .as_deref()
        .unwrap_or(&input.claims_under_review);
    let fact_report = check_oa_analysis(&raw_text, &references_text, my_patent_text);
    let fact_section = format!(
        "\n\n## AI事实核查（请人工复核）\n\n{}",
        format_report(&fact_report)
    );
    let banner = fact_check_banner(&fact_report);
    let full_text = format!("{}{}{}", banner, raw_text, fact_section);

    let guard_result = run_guard(&full_text, input);
    ResponseLetter {
        full_text,
        guard_result,
    }
}

/// 事实核查顶部横幅：零举证不再是文末一条警告，必须在正文最前面可见。
fn fact_check_banner(report: &FactCheckReport) -> String {
    if report.warnings.is_empty() {
        return String::new();
    }
    let zero_evidence = report
        .warnings
        .iter()
        .any(|w| w.category.contains("零举证") || w.description.contains("零举证"));
    if zero_evidence {
        format!(
            "【不可直接提交：存在零举证断言（{} 条警告），见文末事实核查】\n\n",
            report.warnings.len()
        )
    } else {
        format!(
            "【需人工复核：事实核查有 {} 条警告，见文末】\n\n",
            report.warnings.len()
        )
    }
}

/// 材料完整性提示：输入缺口必须显式可见，不能默认自己拿到了全文。
fn material_warnings(input: &CaseInput) -> Vec<String> {
    let mut out = Vec::new();
    let oa_len = input.oa_full_text.chars().count();
    if oa_len < 1000 {
        out.push(format!("驳回决定文本仅 {oa_len} 字，可能为残片或 OCR 片段"));
    }
    if !input.oa_full_text.contains("对比文件") {
        out.push("驳回决定中未出现「对比文件」；特征级比对只能依据审查员转述".into());
    }
    if input.reference_documents.is_empty() {
        out.push("未提供对比文件全文；本稿不能声称已核对对比文件原文".into());
    }
    if input.claims_under_review.trim().is_empty() {
        out.push("未提供待审权利要求原文".into());
    }
    out
}

/// 被丢弃特征的产物内显式提示（errors.md #72 预防②：响亮失败优于静默缺失）。
///
/// Stage A 的 verbatim 校验丢弃一条特征，意味着答复书**缺少**对应论证段——
/// 若被丢的恰是审查员实际认定的区别特征，产物就是结构性缺失。只留日志时
/// 用户无从察觉（#205 缺陷曾因此潜伏）。无丢弃返回 None（不占产物版面）。
pub(crate) fn format_dropped_features_note(dropped: &[String]) -> Option<String> {
    if dropped.is_empty() {
        return None;
    }
    let listed = dropped
        .iter()
        .map(|f| {
            let preview: String = f.chars().take(60).collect();
            if f.chars().count() > 60 {
                format!("{preview}…")
            } else {
                preview
            }
        })
        .collect::<Vec<_>>()
        .join("；");
    Some(format!(
        "【特征抽取丢弃提示：另有 {} 条被审查员提及但未能在权利要求/说明书中逐字定位的表述未进入逐项论证：{}。\
         若其中含实际认定的区别特征，本稿缺少对应论证段，请人工核对并补写】",
        dropped.len(),
        listed
    ))
}

/// 中文数字（0–99）→ 阿拉伯数字；已是阿拉伯数字或识别不了则原样返回。
fn cn_number_to_digits(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() || chars.iter().all(|c| c.is_ascii_digit()) {
        return s.to_string();
    }
    let digit = |c: char| -> Option<u32> {
        Some(match c {
            '零' | '〇' => 0,
            '一' => 1,
            '二' | '两' => 2,
            '三' => 3,
            '四' => 4,
            '五' => 5,
            '六' => 6,
            '七' => 7,
            '八' => 8,
            '九' => 9,
            _ => return None,
        })
    };
    if let Some(pos) = chars.iter().position(|c| *c == '十') {
        let tens = if pos == 0 {
            1
        } else {
            digit(chars[pos - 1]).unwrap_or(1)
        };
        let ones = if pos + 1 < chars.len() {
            digit(chars[pos + 1]).unwrap_or(0)
        } else {
            0
        };
        return (tens * 10 + ones).to_string();
    }
    if chars.len() == 1 {
        if let Some(d) = digit(chars[0]) {
            return d.to_string();
        }
    }
    s.to_string()
}

/// 法条引用的去重键：中文数字归一为阿拉伯数字，使「第二十二条第三款」与「第22条第3款」
/// 视为同一条款（同一依据不得在答复书里列两遍）。「专利法」与「专利法实施细则」前缀不同，
/// 不会被误并。
fn law_citation_key(c: &str) -> String {
    let Ok(re) = regex::Regex::new(r"第([一二三四五六七八九十百零〇0-9]+)(条|款)")
    else {
        return c.to_string();
    };
    let mut out = String::new();
    let mut last = 0usize;
    for cap in re.captures_iter(c) {
        let m = match cap.get(0) {
            Some(m) => m,
            None => continue,
        };
        out.push_str(&c[last..m.start()]);
        out.push('第');
        out.push_str(&cn_number_to_digits(&cap[1]));
        out.push_str(&cap[2]);
        last = m.end();
    }
    out.push_str(&c[last..]);
    out
}

/// 「本案适用」语句的判据标记：只收决定主文与复审救济告知（实体认定由 cites_specific_claim 判）。
const APPLIED_GROUND_MARKERS: &[&str] = &[
    "驳回上述专利申请",
    "决定驳回",
    "作出本驳回决定",
    "请求复审",
    "复审请求",
];

/// 同句是否引用了具体权利要求编号（「权利要求1-4」「权利要求1~3」「权利要求1」）。
///
/// 勾选表模板行有三种写法（2026-11-06 真实案件实测逐轮暴露）：
/// 1. 「申请不符合专利法第X条的规定」——排除「申请不符合」即可；
/// 2. 「权利要求不符合专利法第X条的规定」——按「权利要求」四字放行会把这条也收进来；
/// 3. 「权利要求不具备专利法第22条第2款规定的新颖性」——按「不具备」放行又会收进来。
///
/// 三种模板行的共同点是**泛指、无具体编号**；真实认定必然点名具体权利要求
/// （「指出权利要求1-4不具备专利法第22条第3款规定的创造性」）。故判据取「有编号」，
/// 或「属决定主文/救济告知」（见 APPLIED_GROUND_MARKERS）。
///
/// 代价与兜底：若理由正文只用「该权利要求不具备……」而不带编号，该条会被漏收——
/// 但漏收会触发答复书里的显式告警「实体条款未在现有文本中识别，需人工核对」，
/// 属响亮失败；相比旧行为（把 15 条从未适用的依据当真依据列给用户），
/// 漏收+显式告警是可接受的一侧。
fn cites_specific_claim(sent: &str) -> bool {
    let Ok(re) = regex::Regex::new(r"权利要求\s*[0-9一二三四五六七八九十]") else {
        return false;
    };
    re.is_match(sent)
}

/// 按句切分（`。`/`；`/`;`/换行），供法条判据限定在**同句**作用域内——跨句牵连会把
/// 表格空白项与正文认定混为一谈。
fn split_law_sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (idx, ch) in text.char_indices() {
        if matches!(ch, '。' | '；' | ';' | '\n') {
            out.push(&text[start..idx]);
            start = idx + ch.len_utf8();
        }
    }
    out.push(&text[start..]);
    out
}

/// 从驳回决定原文提取**本案实际适用**的法条依据（确定性，不猜造）。
///
/// 2026-11-06 缺陷修复（真实案件实测）：驳回决定首页是一张候选法条勾选表，每一项都印着
/// 「申请不符合专利法第X条的规定」，**未勾选的项同样留在 OCR 文本里**。旧实现对全文所有
/// 法条样式一律收走，导致答复书把 15 条从未适用的依据一并列给用户（实际实体依据只有
/// 第22条第3款一条）。现只认「本案适用」语句形态（见 [`APPLIED_GROUND_MARKERS`]），
/// 表格式裸条目一律丢弃；同一法条的中文/阿拉伯数字写法归一后去重。
fn extract_legal_basis(oa_text: &str) -> Vec<String> {
    let Ok(re) = regex::Regex::new(
        r"专利法(?:实施条例|实施细则)?第[一二三四五六七八九十百千零〇0-9]+条(?:第[一二三四五六七八九十0-9]+款)?",
    ) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    let mut seen_keys: Vec<String> = Vec::new();
    for sent in split_law_sentences(oa_text) {
        if !APPLIED_GROUND_MARKERS.iter().any(|m| sent.contains(m)) && !cites_specific_claim(sent) {
            continue;
        }
        for m in re.find_iter(sent) {
            let s = m.as_str().to_string();
            let key = law_citation_key(&s);
            if !seen_keys.contains(&key) {
                seen_keys.push(key);
                out.push(s);
            }
        }
    }
    out
}

/// 区别特征 → 驳回决定原文出处对照表（确定性；缺摘录必须显式标出）。
fn evidence_table(features: &[DistinguishingFeature]) -> String {
    let mut text = String::new();
    text.push_str("| 序号 | 区别特征 | 驳回标签 | 审查员归因 | 驳回决定原文摘录 |\n");
    text.push_str("|------|---------|---------|-----------|------------------|\n");
    for f in features {
        let excerpt = if f.oa_excerpt.trim().is_empty() {
            "【缺原文摘录】".to_string()
        } else {
            truncate_chars(&sanitize_cell(&f.oa_excerpt), 120)
        };
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            f.ordinal,
            truncate_chars(&sanitize_cell(&f.feature_name), 80),
            f.oa_tag.as_chinese(),
            f.attributed_ref.as_deref().unwrap_or("未归因"),
            excerpt
        ));
    }
    text
}

fn sanitize_cell(s: &str) -> String {
    s.replace(['\n', '\r', '|', '｜'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate_chars(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
}

// ── Stage A：OA区别特征抽取 ─────────────────────────────────────────────

/// 从驳回决定全文中抽出审查员认定的区别特征、泛化标签、所引对比文件。
///
/// 方法：AI单次调用（structured output）。
/// 硬约束（裁决①）：每个 feature_name 必须是 claims_under_review 或 spec_full_text
/// 的 verbatim 子串，否则丢弃。
/// 返回：(通过校验的特征, 被丢弃的特征名列表)。丢弃必须让调用方知情——
/// 丢弃导致产物结构性缺失时只留日志是静默失败（errors.md #72 预防②），
/// 由 stage_e 在产物内挂显式提示（响亮失败优于静默缺失）。
async fn stage_a_extract_features(
    input: &CaseInput,
    ai_client: Option<&AiClient>,
) -> (Vec<DistinguishingFeature>, Vec<String>) {
    let ai = match ai_client {
        Some(c) => c,
        None => return (Vec::new(), Vec::new()),
    };

    let system_prompt = "\
你是一位资深专利代理师。任务：从驳回决定中提取审查员认定的区别特征。\n\
\n\
输入：驳回决定全文 + 待审权利要求。\n\
输出：JSON数组，每项包含：\n\
- ordinal: 序号（从1开始）\n\
- feature_name: 特征名称（必须是权利要求原文的verbatim子串）\n\
- oa_tag: 审查员给的泛化标签，必须是以下之一：容易想到/常规设计/常规选择/常规调整/常规设置/惯用手段\n\
- attributed_ref: 审查员归因的对比文件标签（如D1/D2/D3），未归因则为null\n\
- oa_excerpt: OA中该特征的转述原文片段\n\
\n\
严格约束：\n\
1. feature_name 必须是权利要求原文中连续出现的子串，不得改写、缩写或意译\n\
2. 只从驳回决定中提取审查员明确认定的区别特征，不得自行推断\n\
3. 输出纯JSON数组，不要markdown代码块标记";

    let user_prompt = format!(
        "## 驳回决定全文\n{}\n\n## 待审权利要求\n{}\n\n请提取区别特征，输出JSON数组。",
        input.oa_full_text, input.claims_under_review
    );

    let raw = match ai.chat_with_system(system_prompt, &user_prompt, 1.0).await {
        Ok(text) => text,
        Err(e) => {
            tracing::warn!("Stage A AI调用失败: {}", e);
            return (Vec::new(), Vec::new());
        }
    };

    let json_str = extract_json_array(&raw);
    let parsed: Vec<serde_json::Value> = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                "Stage A JSON解析失败: {} | raw[:200]={:?} | json_str[:200]={:?}",
                e,
                raw.chars().take(200).collect::<String>(),
                json_str.chars().take(200).collect::<String>()
            );
            // 回退：正则提取特征字段（处理AI输出的未转义引号）
            match parse_features_regex(json_str) {
                Ok(v) if !v.is_empty() => {
                    tracing::info!("Stage A 正则回退解析成功：提取{}个特征", v.len());
                    v
                }
                _ => return (Vec::new(), Vec::new()),
            }
        }
    };

    let claims = &input.claims_under_review;
    let spec = input.spec_full_text.as_deref().unwrap_or("");
    let mut features = Vec::new();
    let mut dropped = Vec::new();

    for v in parsed {
        let feature_name = v
            .get("feature_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if feature_name.is_empty() {
            continue;
        }

        // 裁决①：verbatim 子串校验
        if !claims.contains(&feature_name) && !spec.contains(&feature_name) {
            tracing::warn!(
                "Stage A 丢弃非verbatim特征: '{}' 不在权利要求或说明书中",
                feature_name
            );
            dropped.push(feature_name);
            continue;
        }

        let ordinal = v
            .get("ordinal")
            .and_then(|v| v.as_u64())
            .unwrap_or(features.len() as u64 + 1) as usize;

        let tag_str = v.get("oa_tag").and_then(|v| v.as_str()).unwrap_or("其他");
        let oa_tag = OaTag::from_chinese(tag_str);

        let attributed_ref = v
            .get("attributed_ref")
            .and_then(|v| v.as_str())
            .filter(|s| !s.eq_ignore_ascii_case("null"))
            .map(|s| s.to_string());

        let oa_excerpt = v
            .get("oa_excerpt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        features.push(DistinguishingFeature {
            ordinal,
            feature_name,
            oa_tag,
            attributed_ref,
            oa_excerpt,
        });
    }

    (features, dropped)
}

/// 从AI输出中提取JSON数组（去除markdown代码块标记）。
fn extract_json_array(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(start) = trimmed.find("```") {
        let after_fence = &trimmed[start + 3..];
        let after_lang = after_fence.strip_prefix("json").unwrap_or(after_fence);
        if let Some(arr_start) = after_lang.find('[') {
            if let Some(arr_end) = after_lang.rfind(']') {
                return &after_lang[arr_start..=arr_end];
            }
        }
    }
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            return &trimmed[start..=end];
        }
    }
    trimmed
}

// ── Stage B：定性核词计数 ──────────────────────────────────────────────

/// 对OA中审查员使用的定性术语，在其归因对比文件原文中自动计数。
/// count==0 → 生成"定性无支撑"反驳段 + 计数表。
///
/// 方法：纯确定性，复用 extraction.rs::count_term_in_text。
/// 正则回退解析：从含未转义引号的 JSON-like 文本中提取特征字段。
/// 逐对象扫描 ordinal/feature_name/oa_tag/attributed_ref/oa_excerpt。
fn parse_features_regex(text: &str) -> Result<Vec<serde_json::Value>, ()> {
    use regex::Regex;
    let mut results = Vec::new();

    // 匹配 "feature_name": "..." — 取引号后到下一个 "ordinal" 或 "oa_tag" 或 } 之前的内容
    let feature_re = Regex::new(r#""feature_name"\s*:\s*"(.*?)""#).unwrap();
    let ordinal_re = Regex::new(r#""ordinal"\s*:\s*(\d+)"#).unwrap();
    let tag_re = Regex::new(r#""oa_tag"\s*:\s*"(.*?)""#).unwrap();
    let ref_re = Regex::new(r#""attributed_ref"\s*:\s*"(.*?)""#).unwrap();
    // oa_excerpt 的值可能含未转义引号，取 "oa_excerpt": " 到下一个 \n  + "  或 } 之前
    let excerpt_re = Regex::new(r#""oa_excerpt"\s*:\s*"(.*?)"\s*[,}\n]"#).unwrap();

    // 按 } 分割为对象块
    for block in text.split('}') {
        let feature_name = match feature_re.captures(block) {
            Some(c) => c.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
            None => continue,
        };
        if feature_name.is_empty() {
            continue;
        }

        let ordinal = ordinal_re
            .captures(block)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse::<u64>().ok())
            .unwrap_or(0);

        let oa_tag = tag_re
            .captures(block)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "其他".to_string());

        let attributed_ref = ref_re
            .captures(block)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string());

        let oa_excerpt = excerpt_re
            .captures(block)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();

        let mut obj = serde_json::Map::new();
        obj.insert(
            "feature_name".to_string(),
            serde_json::Value::String(feature_name),
        );
        obj.insert(
            "ordinal".to_string(),
            serde_json::Value::Number(serde_json::Number::from(ordinal)),
        );
        obj.insert("oa_tag".to_string(), serde_json::Value::String(oa_tag));
        if let Some(r) = attributed_ref {
            obj.insert("attributed_ref".to_string(), serde_json::Value::String(r));
        }
        obj.insert(
            "oa_excerpt".to_string(),
            serde_json::Value::String(oa_excerpt),
        );
        results.push(serde_json::Value::Object(obj));
    }

    if results.is_empty() {
        Err(())
    } else {
        Ok(results)
    }
}

fn stage_b_count_terms(
    input: &CaseInput,
    features: &[DistinguishingFeature],
) -> (Vec<TermCountCheck>, Vec<UnsupportedClaim>) {
    let mut term_checks = Vec::new();
    let mut unsupported_claims = Vec::new();
    // 修复②a：按 (term, ref_label) 去重，避免多特征提取相同术语产生重复段
    let mut seen_term_ref: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();

    let oa_normalized = normalize_text(&input.oa_full_text);

    for feature in features {
        let terms = extract_key_terms(&feature.oa_excerpt, &input.custom_stopwords);
        let ref_label = match &feature.attributed_ref {
            Some(label) => label.clone(),
            None => continue,
        };

        let ref_doc = match input
            .reference_documents
            .iter()
            .find(|r| r.label == ref_label)
        {
            Some(doc) => doc,
            None => continue,
        };

        let ref_normalized = normalize_text(&ref_doc.full_text);

        for term in &terms {
            let oa_count = count_term_in_text(&oa_normalized, term);
            let ref_count = count_term_in_text(&ref_normalized, term);

            // 去重仅限对照表：(断言摘要+来源文件)整串精确匹配
            let table_key = (term.clone(), ref_label.clone());
            if !seen_term_ref.insert(table_key) {
                // 对照表已含此条，跳过重复行；正文论证段照常生成
            } else {
                term_checks.push(TermCountCheck {
                    term: term.clone(),
                    oa_count,
                    ref_label: ref_label.clone(),
                    ref_pub_num: ref_doc.publication_number.clone(),
                    ref_count,
                });
            }

            // count==0 且 OA中存在该术语 → 定性无支撑（正文段不剪枝）
            if ref_count == 0 && oa_count > 0 {
                let rebuttal_text = format!(
                    "审查员对{}的\"{}\"定性在{}原文无支撑，属最接近现有技术认定偏差\n\n\
                     - \"{}\"在{}原文出现次数：0次\n\
                     - 驳回决定中\"{}\"出现{}次\n\n\
                     【依据：C0实测计数；{}原文全文提取】",
                    ref_label,
                    term,
                    ref_doc.publication_number,
                    term,
                    ref_label,
                    term,
                    oa_count,
                    ref_label
                );

                unsupported_claims.push(UnsupportedClaim {
                    rebuttal_text,
                    counts: vec![TermCountCheck {
                        term: term.clone(),
                        oa_count,
                        ref_label: ref_label.clone(),
                        ref_pub_num: ref_doc.publication_number.clone(),
                        ref_count,
                    }],
                });
            }
        }
    }

    (term_checks, unsupported_claims)
}

/// CJK语法虚词/连接词，用作分词分隔符。
const CJK_DELIMITERS: &[&str] = &[
    "是", "的", "了", "通过", "进行", "实现", "能够", "可以", "应该", "需要", "并且", "因此",
    "从而", "进而", "即", "指",
];

/// 从文本中提取关键术语（2-6字中文技术名词）。
///
/// 分词策略：按非CJK字符和CJK虚词切分，保留2-6字的中文片段。
fn extract_key_terms(text: &str, custom_stopwords: &[String]) -> Vec<String> {
    // 先用虚词切分
    let mut segments: Vec<String> = vec![text.to_string()];
    for delim in CJK_DELIMITERS {
        let mut next = Vec::new();
        for seg in &segments {
            for part in seg.split(delim) {
                next.push(part.to_string());
            }
        }
        segments = next;
    }

    let mut terms = Vec::new();
    for seg in &segments {
        // 再按非CJK字符切分
        let mut current = String::new();
        for c in seg.chars() {
            if is_cjk_char(c) {
                current.push(c);
            } else {
                let len = current.chars().count();
                if (2..=6).contains(&len) && !terms.contains(&current) {
                    terms.push(current.clone());
                }
                current.clear();
            }
        }
        let len = current.chars().count();
        if (2..=6).contains(&len) && !terms.contains(&current) {
            terms.push(current);
        }
    }

    const STOPWORDS: &[&str] = &[
        // 原有专利术语停用词
        "对比文件",
        "区别特征",
        "权利要求",
        "技术方案",
        "技术特征",
        "技术效果",
        "技术问题",
        "现有技术",
        "发明创造",
        "实施方式",
        "具体实施",
        "本领域",
        "技术人",
        "方案",
        "特征",
        "效果",
        "常规设计",
        "常规选择",
        "常规调整",
        "常规设置",
        "容易想到",
        "惯用手段",
        // 缺陷2修复：通用语法/连接词，纯语法功能，不含案特有技术词
        "公开",
        "本发明",
        "说明书",
        "技术",
        "装置",
        "方法",
        "所述",
        "包括",
        "包含",
        "具有",
        "设有",
        "用于",
        "其中",
        "上述",
        "该",
        "其",
        "及",
        "和",
        "与",
        "或",
        "在",
        "为",
        "由",
        "从",
        "向",
        "上",
        "下",
        "内",
        "外",
        "前",
        "后",
        "一种",
        "多个",
        "若干",
        "上述",
        "本案",
        "本申请",
        "对比",
        "文件",
        "审查",
        "员",
        "决定",
        "驳回",
        "创造",
        "性",
        "不具备",
        "符合",
        "规定",
        "条款",
        "专利",
        "法",
        "申请",
        "人",
        "发明",
        "名称",
        "号",
        "日",
        "提交",
        "原文",
        "全文",
        "已被对比文件",
        "被对比文件",
        "对比文件1",
        "对比文件2",
        "对比文件3",
        "文件1",
        "文件2",
        "文件3",
    ];
    terms.retain(|t| !STOPWORDS.contains(&t.as_str()));
    terms.retain(|t| !custom_stopwords.iter().any(|s| s == t));
    terms
}

/// 判断字符是否为CJK汉字。
fn is_cjk_char(c: char) -> bool {
    let cp = c as u32;
    (0x4E00..=0x9FFF).contains(&cp) || (0x3400..=0x4DBF).contains(&cp)
}

// ── Stage C：逐特征三段式反驳 ──────────────────────────────────────────

/// 对每项区别特征，生成"割裂论断→整体协同→举证不足"三段式反驳。
///
/// 三段式结构：
/// - severance（割裂论断）：审查员将本特征从整体方案中割裂单独评价，
///   忽略其与其他特征的协同关系
/// - synergy（整体协同）：基于共现依赖图，列举与本特征共现的其他特征，
///   说明它们如何协同工作（纯确定性，不调AI）
/// - insufficient（举证不足）：审查员仅用泛化标签否定，未提供具体证据
///
/// 特征依赖图：共现建边（裁决③），不用AI补边。
fn stage_c_rebut_features(
    input: &CaseInput,
    features: &[DistinguishingFeature],
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<FeatureRebuttal>> + Send>> {
    let claims = input.claims_under_review.clone();
    let features_vec = features.to_vec();
    Box::pin(async move {
        let _graph = build_dependency_graph_with_features(&claims, &features_vec);
        let _rebuttals: Vec<FeatureRebuttal> = Vec::new();
        let graph = build_dependency_graph_with_features(&claims, &features_vec);
        let mut rebuttals: Vec<FeatureRebuttal> = Vec::new();

        for feature in &features_vec {
            let ref_str = feature.attributed_ref.as_deref().unwrap_or("对比文件");

            // 定性失真段（仲裁优先级1：主攻）
            let qualitative = format!(
                "审查员以\u{201c}{}\u{201d}这一上位泛化标签概括区别特征\u{201c}{}\u{201d}，\
                 未分析该特征的技术实质和在该技术方案中的具体作用。\
                 上位概括掩盖了该特征相对于对比文件的实质区别，\
                 不满足创造性判断中\u{201c}区别特征技术实质\u{201d}的分析要求。",
                feature.oa_tag.as_chinese(),
                feature.feature_name
            );

            // 举证不足段（仲裁优先级2：主攻）
            let insufficient = format!(
                "驳回决定仅以\u{201c}{}\u{201d}这一泛化标签否定\u{201c}{}\u{201d}的创造性，\
                 未具体说明{}中何种现有技术给出了采用\u{201c}{}\u{201d}的技术启示，\
                 也未论证本领域技术人员为何有动机将{}的方案与\u{201c}{}\u{201d}结合。\
                 根据《专利审查指南》关于创造性判断的规定，\
                 仅有泛化标签而无具体技术启示证据，不满足创造性否定\
                 的举证要求。",
                feature.oa_tag.as_chinese(),
                feature.feature_name,
                ref_str,
                feature.feature_name,
                ref_str,
                feature.feature_name
            );

            // 割裂论断段（仲裁优先级5：防守，≤3句）
            let severance = format!(
                "驳回决定将\u{201c}{}\u{201d}从权利要求的整体技术方案中割裂出来单独评价，\
                 认为其相对于{}属于\u{201c}{}\u{201d}。然而，这种割裂评价方法违背了\
                 创造性判断的整体性原则——应当将区别特征置于其所在的整体\
                 技术方案中考察，而非脱离其他技术特征孤立评判。",
                feature.feature_name,
                ref_str,
                feature.oa_tag.as_chinese()
            );

            // 整体协同段（基于共现依赖图，纯确定性）
            let deps = graph
                .edges
                .iter()
                .find(|(name, _)| name == &feature.feature_name)
                .map(|(_, deps)| deps.clone())
                .unwrap_or_default();

            let synergy = if deps.is_empty() {
                format!(
                    "\u{201c}{}\u{201d}作为独立技术特征，其技术贡献应当结合说明书\
                     记载的技术效果综合评价，而非仅以泛化标签否定。",
                    feature.feature_name
                )
            } else {
                let dep_list = deps.join("、");
                format!(
                    "\u{201c}{}\u{201d}与权利要求中的{}存在协同关系：\
                     这些特征在权利要求原文中共现，共同构成不可分割的\
                     技术整体。审查员未考虑上述特征间的协同作用，\
                     仅对\u{201c}{}\u{201d}单独贴标签，遗漏了整体技术方案\
                     相对于{}的协同技术贡献。",
                    feature.feature_name, dep_list, feature.feature_name, ref_str
                )
            };

            // 仲裁决定段落顺序：定性失真(主攻) → 零举证(主攻) → 割裂式(防守) → 协同
            let arbitrated_text = format!(
                "{}\n\n{}\n\n{}\n\n{}",
                qualitative, insufficient, severance, synergy
            );

            rebuttals.push(FeatureRebuttal {
                ordinal: feature.ordinal,
                feature_name: feature.feature_name.clone(),
                oa_tag: feature.oa_tag.clone(),
                severance: arbitrated_text,
                synergy: String::new(),
                insufficient: String::new(),
            });
        }

        rebuttals
    })
}

/// 从权利要求文本构建特征依赖图（共现建边）。
///
/// 裁决③：只统计特征词在权利要求原文的共现，不许AI凭空加边。
/// 两个特征共现 = 它们在权利要求文本的同一从句（顿号/逗号分隔段）中同时出现。
#[allow(dead_code)] // 占位实现（边表恒空）：依赖图裁决尚未接线，先保留形状不删
fn build_dependency_graph(claims_text: &str) -> DependencyGraph {
    // 无特征列表时无法建边，返回空图
    let _ = claims_text;
    DependencyGraph { edges: Vec::new() }
}

/// 从权利要求文本 + 特征列表构建共现依赖图。
///
/// 算法：
/// 1. 将权利要求按顿号、逗号、分号切分为从句片段
/// 2. 对每对特征 (A, B)，若存在同一片段同时包含 A 和 B，则建边 A→B 和 B→A
/// 3. 不调AI，纯确定性共现统计
fn build_dependency_graph_with_features(
    claims_text: &str,
    features: &[DistinguishingFeature],
) -> DependencyGraph {
    if features.len() < 2 {
        return DependencyGraph { edges: Vec::new() };
    }

    // 按逗号、分号、句号切分从句（不按顿号切分——顿号连接的是同组特征）
    let clauses: Vec<&str> = claims_text.split(['，', '；', '。', ',']).collect();

    let mut edges = Vec::new();

    for (i, fa) in features.iter().enumerate() {
        let mut deps = Vec::new();
        for (j, fb) in features.iter().enumerate() {
            if i == j {
                continue;
            }
            // 检查是否存在某个从句同时包含两个特征
            let co_occur = clauses.iter().any(|clause| {
                clause.contains(&fa.feature_name) && clause.contains(&fb.feature_name)
            });
            if co_occur && !deps.contains(&fb.feature_name) {
                deps.push(fb.feature_name.clone());
            }
        }
        if !deps.is_empty() {
            edges.push((fa.feature_name.clone(), deps));
        }
    }

    DependencyGraph { edges }
}

#[path = "response_pipeline_stage_d.rs"]
mod stage_d;
use stage_d::stage_d_compare_effects;
#[cfg(test)]
use stage_d::{
    auto_extract_effect_statements, extend_window_to_sentence_boundary, find_ref_label_for_claim,
    scan_effect_claims,
};

// ── Stage E：程序性固定件组装 ──────────────────────────────────────────

/// 合并连续空白字符：将2个以上连续空格/制表符压缩为1个空格。
///
/// 缺陷修复：Stage C 模板中的行连接（\）在 format! 展开后可能残留多余空格，
/// 此函数在 Stage E 最终输出前调用，确保无连续2个以上空白字符。
fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_was_space = false;
    for ch in s.chars() {
        if ch == ' ' || ch == '\t' {
            if !prev_was_space {
                result.push(' ');
                prev_was_space = true;
            }
        } else {
            result.push(ch);
            prev_was_space = false;
        }
    }
    result
}

fn stage_e_assemble(
    input: &CaseInput,
    features: &[DistinguishingFeature],
    term_checks: &[TermCountCheck],
    unsupported_claims: &[UnsupportedClaim],
    rebuttals: &[FeatureRebuttal],
    effect_mismatches: &[EffectMismatch],
    dropped_features: &[String],
) -> String {
    let meta = &input.case_meta;
    let mut text = String::new();
    // 缺陷3修复：按实际生成段落动态编号，连续无跳
    let cn_nums = ["一", "二", "三", "四", "五", "六", "七", "八", "九", "十"];
    let mut section = 0usize;

    text.push_str("复审请求书\n\n");
    section += 1;
    text.push_str(&format!("{}、复审请求范围\n\n", cn_nums[section - 1]));
    text.push_str(&format!("申请号：{}\n", meta.application_number));
    text.push_str(&format!("发明名称：{}\n", meta.invention_title));
    text.push_str(&format!("申请人：{}\n", meta.applicant));
    if let Some(date) = &meta.oa_date {
        text.push_str(&format!("驳回决定发文日：{}\n", date));
    }
    text.push_str("\n请求撤销驳回决定，并发回原审查部门继续审查。\n\n");

    let warnings = material_warnings(input);
    if !warnings.is_empty() {
        text.push_str("【材料完整性提示：以下缺口会影响结论可靠性，补齐前不可直接提交】\n");
        for w in &warnings {
            text.push_str(&format!("- {w}\n"));
        }
        text.push('\n');
    }
    if let Some(note) = format_dropped_features_note(dropped_features) {
        text.push_str(&note);
        text.push_str("\n\n");
    }

    let mut legal_bases = extract_legal_basis(&input.oa_full_text);
    if input.case_meta.oa_type == "reject_review"
        && !legal_bases.iter().any(|b| b.contains("第41条"))
    {
        legal_bases.push("专利法第41条（复审请求的程序依据）".into());
    }
    section += 1;
    text.push_str(&format!("{}、法律依据\n\n", cn_nums[section - 1]));
    if legal_bases.is_empty() {
        text.push_str("【系统警告：未从驳回决定中识别到法条依据，请人工核对】\n\n");
    } else {
        for b in &legal_bases {
            text.push_str(&format!("- {b}\n"));
        }
        if !legal_bases.iter().any(|b| !b.contains("第41条")) {
            text.push_str(
                "\n【提示：驳回决定的实体条款未在现有文本中识别，需人工核对驳回决定原文】\n\n",
            );
        } else {
            text.push_str("\n以上为驳回决定引用的法律依据，下文逐项回应。\n\n");
        }
    }

    section += 1;
    text.push_str(&format!(
        "{}、针对驳回决定各项理由的逐项消除缺陷论证\n\n",
        cn_nums[section - 1]
    ));
    // 空正文报错：禁止空正文，打印根因
    if unsupported_claims.is_empty() && rebuttals.is_empty() && effect_mismatches.is_empty() {
        text.push_str("【系统警告：第二节正文为空】\n\n");
        text.push_str(&format!(
            "根因诊断：features={} 个, unsupported_claims=0, rebuttals=0, effect_mismatches=0\n",
            features.len()
        ));
        if features.is_empty() {
            text.push_str("根因：Stage A 特征提取返回空（AI 调用失败或 JSON 解析失败），导致 Stage B/C/D 无输入。\n");
            text.push_str("建议：检查 AI 服务连通性、模型名称、oa_full_text 是否非空。\n");
        } else {
            text.push_str("根因：Stage A 提取到特征但 Stage B/C/D 均未生成论证段，检查 reference_documents 是否匹配。\n");
        }
        text.push('\n');
    }
    for uc in unsupported_claims {
        text.push_str(&uc.rebuttal_text);
        text.push_str("\n\n");
    }
    for r in rebuttals {
        text.push_str(&format!(
            "{}{}——驳回决定标签\"{}\"。\n",
            ordinal_circled(r.ordinal),
            r.feature_name,
            r.oa_tag.as_chinese()
        ));
        text.push_str(&r.severance);
        if !r.synergy.is_empty() {
            text.push('\n');
            text.push_str(&r.synergy);
        }
        if !r.insufficient.is_empty() {
            text.push('\n');
            text.push_str(&r.insufficient);
        }
        if let Some(f) = features.iter().find(|f| f.ordinal == r.ordinal) {
            if !f.oa_excerpt.trim().is_empty() {
                text.push_str(&format!(
                    "\n根据驳回决定原文摘录（见证据表第{}行）。",
                    r.ordinal
                ));
            }
        }
        text.push_str("\n\n");
    }
    for em in effect_mismatches {
        text.push_str(&em.rebuttal_text);
        text.push_str("\n\n");
    }

    let arbitration =
        crate::ai::strategy_arbitration::arbitrate(input, features, rebuttals, effect_mismatches);
    for item in &arbitration.items {
        let is_academic_doi = item.line
            == crate::ai::strategy_arbitration::AttackLine::ZeroEvidence
            && (item.text.contains("DOI") || item.text.contains("10."));
        let is_topology_diff = item.line
            == crate::ai::strategy_arbitration::AttackLine::MechanismEffectDiff
            && !effect_mismatches
                .iter()
                .any(|em| em.rebuttal_text == item.text);
        if is_academic_doi || is_topology_diff {
            text.push_str(&item.text);
            text.push_str("\n\n");
        }
    }

    if !features.is_empty() {
        section += 1;
        text.push_str(&format!(
            "{}、区别特征与驳回决定出处对照表\n\n",
            cn_nums[section - 1]
        ));
        text.push_str(&evidence_table(features));
        text.push_str("\n本表为驳回决定转述的出处对照，不替代对比文件原文核验。\n\n");
    }

    section += 1;
    text.push_str(&format!("{}、请求事项\n\n", cn_nums[section - 1]));
    text.push_str("1. 请求撤销驳回决定，并发回原审查部门继续审查。\n\n");

    if !term_checks.is_empty() {
        section += 1;
        text.push_str(&format!(
            "{}、C0实测计数出处对照表\n\n",
            cn_nums[section - 1]
        ));
        text.push_str("| 序号 | 断言摘要 | 来源文件 | 实测计数 |\n");
        text.push_str("|------|---------|---------|---------|\n");
        for (i, tc) in term_checks.iter().enumerate() {
            text.push_str(&format!(
                "| {} | {} | {} | {}={} |\n",
                i + 1,
                tc.term,
                tc.ref_pub_num,
                tc.term,
                tc.ref_count
            ));
        }
    }

    // UA2 接线（2026-11-06 缺陷修复）：复审请求书文末固定追加「提交前清单」——
    // 该文案早已定稿并写了常量存在性测试，但从未拼进产物（`allow(dead_code)` 挂了很久），
    // 属于「文案到位、动作缺位」的假交付。清单是静态确定性内容，不经 AI 生成。
    if input.case_meta.oa_type == "reject_review" {
        text.push_str(AiClient::REEXAM_PRE_SUBMIT_CHECKLIST);
    }

    collapse_whitespace(&text)
}

/// 序号转带圈数字。
fn ordinal_circled(n: usize) -> String {
    const CIRCLED: &[&str] = &["", "①", "②", "③", "④", "⑤", "⑥", "⑦", "⑧", "⑨", "⑩"];
    if n < CIRCLED.len() {
        CIRCLED[n].to_string()
    } else {
        format!("({})", n)
    }
}

#[cfg(test)]
#[path = "response_pipeline_tests.rs"]
mod tests;
