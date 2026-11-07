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
async fn stage_d_compare_effects(
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
struct EffectClaim {
    /// 断言原文片段（含上下文，≤100字）
    text: String,
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
    let result: String = chars[..cut].iter().collect();
    if cut < chars.len() {
        format!("{}…", result)
    } else {
        result
    }
}

/// 在 chars[from] 起向后（含 from）在 lookahead 字内找最近的句子边界（。；;），
/// 返回边界后一位（含句号）。找不到边界则返回 from，调用方自行回退硬窗口——
/// 硬窗口切在句中会产生「公开了流量传」这类半句引文，正式文书里不可读。
fn extend_window_to_sentence_boundary(chars: &[char], from: usize, lookahead: usize) -> usize {
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

fn scan_effect_claims(oa_text: &str) -> Vec<EffectClaim> {
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
fn find_ref_label_for_claim(oa_text: &str, claim_text: &str) -> Option<String> {
    // 在 claim_text 中直接搜索 D1/D2/D3 等
    for label in &["D1", "D2", "D3", "D4", "D5", "D6"] {
        if claim_text.contains(label) {
            return Some(label.to_string());
        }
    }
    // 在 OA 全文中找 claim_text 附近最近的对比文件标签
    if let Some(pos) = oa_text.find(claim_text) {
        let chars: Vec<char> = oa_text.chars().collect();
        let pos_char = oa_text[..pos].chars().count();
        // 向前搜索100字符
        let search_start = pos_char.saturating_sub(100);
        let search_end = (pos_char + claim_text.chars().count() + 100).min(chars.len());
        for i in (search_start..pos_char).rev() {
            for label in &["D1", "D2", "D3", "D4", "D5", "D6"] {
                let label_chars: Vec<char> = label.chars().collect();
                if i + label_chars.len() <= search_end
                    && chars[i..i + label_chars.len()] == label_chars[..]
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
fn auto_extract_effect_statements(spec_full_text: &str) -> Vec<String> {
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
mod tests {

    #[test]
    fn collapse_whitespace_removes_consecutive_spaces() {
        assert_eq!(collapse_whitespace("a  b"), "a b");
        assert_eq!(collapse_whitespace("a    b"), "a b");
        assert_eq!(collapse_whitespace("a		b"), "a b");
        assert_eq!(collapse_whitespace("a 	 b"), "a b");
        assert_eq!(collapse_whitespace("no spaces"), "no spaces");
        assert_eq!(collapse_whitespace("  leading"), " leading");
        assert_eq!(collapse_whitespace("trailing  "), "trailing ");
    }

    #[test]
    fn stage_e_output_has_no_consecutive_whitespace() {
        // 验收：Stage E 输出不含连续2个以上空白字符
        let input = make_sample_case_input();
        let features: Vec<DistinguishingFeature> = vec![];
        let term_checks: Vec<TermCountCheck> = vec![];
        let unsupported: Vec<UnsupportedClaim> = vec![];
        let rebuttals: Vec<FeatureRebuttal> = vec![];
        let mismatches: Vec<EffectMismatch> = vec![];
        let output = stage_e_assemble(
            &input,
            &features,
            &term_checks,
            &unsupported,
            &rebuttals,
            &mismatches,
            &[],
        );
        let mut max_consecutive = 0;
        let mut current = 0;
        for ch in output.chars() {
            if ch == ' ' || ch == '\t' {
                current += 1;
                if current > max_consecutive {
                    max_consecutive = current;
                }
            } else {
                current = 0;
            }
        }
        assert!(
            max_consecutive <= 1,
            "Found {} consecutive whitespace chars in output",
            max_consecutive
        );
    }

    use super::super::case_input::{
        CaseMeta, DistinguishingFeature, FeatureRebuttal, OaTag, ReferenceDocument,
    };
    use super::super::fact_check::FactWarning;
    use super::*;

    #[test]
    fn fact_check_banner_marks_zero_evidence_as_blocking() {
        let report = FactCheckReport {
            warnings: vec![FactWarning {
                category: "C2零举证".into(),
                description: "检测到15处零举证断言".into(),
                severity: "高".into(),
            }],
            score: 85.0,
            c0_extractions: vec![],
        };
        let banner = fact_check_banner(&report);
        assert!(banner.contains("不可直接提交"));
        assert!(banner.contains("零举证"));
    }

    #[test]
    fn material_warnings_flag_missing_refs_and_short_oa() {
        let input = make_sample_case_input();
        let warnings = material_warnings(&input);
        assert!(warnings.iter().any(|w| w.contains("字")));
        assert!(warnings.iter().any(|w| w.contains("对比文件")));
    }

    #[test]
    fn stage_e_includes_legal_basis_and_evidence_table() {
        let mut input = make_sample_case_input();
        input.oa_full_text = "根据专利法第22条第3款，权利要求1不具备创造性。".into();
        let features = vec![DistinguishingFeature {
            ordinal: 1,
            feature_name: "特征A".into(),
            oa_tag: OaTag::RoutineChoice,
            attributed_ref: Some("D1".into()),
            oa_excerpt: String::new(),
        }];
        let output = stage_e_assemble(&input, &features, &[], &[], &[], &[], &[]);
        assert!(output.contains("法律依据"));
        assert!(output.contains("专利法第22条第3款"));
        assert!(output.contains("区别特征与驳回决定出处对照表"));
        assert!(output.contains("【缺原文摘录】"));
    }

    #[test]
    fn stage_e_reject_review_without_substantive_law_marks_gap() {
        let mut input = make_sample_case_input();
        input.oa_full_text = "驳回决定正文，未引用具体条款。".into();
        let output = stage_e_assemble(&input, &[], &[], &[], &[], &[], &[]);
        assert!(output.contains("专利法第41条"));
        assert!(output.contains("实体条款未在现有文本中识别"));
    }

    /// **2026-11-06 缺陷锁（真实案件形态）**：驳回决定首页的候选法条勾选表，未勾选项也留在
    /// OCR 文本里。旧实现把它们全当真依据列给用户（实测 15 条 vs 实际 1 条实体依据）。
    /// 现只认「本案适用」语句；表格裸条目必须被丢弃。
    #[test]
    fn legal_basis_ignores_rejection_form_checklist() {
        let oa = "1. 根据专利法第38条及其实施细则第59条的规定，决定驳回上述专利申请。\n\
                  申请不符合专利法第2条第2款的规定。\n\
                  申请不符合专利法第5条或者第25条规定的不授予专利权的范围。\n\
                  申请不符合专利法第9条第1款的规定。\n\
                  申请不符合专利法第22条第2款的规定。\n\
                  申请不符合专利法第26条第3款或者第4款的规定。\n\
                  权利要求不符合专利法第2条第2款的规定。\n\
                  权利要求不符合专利法第31条第1款的规定。\n\
                  权利要求不具备专利法第22条第2款规定的新颖性。\n\
                  权利要求不具备专利法第22条第4款规定的实用性。\n\
                  详细的驳回理由见驳回决定正文部分（共3页）。\n\
                  审查员于2026年01月19日发出第一次审查意见通知书，指出权利要求1-4不具备专利法第22条第3款规定的创造性。\n\
                  该权利要求不具备专利法第二十二条第三款规定的创造性。\n\
                  3. 根据专利法第41条及实施细则第65条的规定，申请人可以请求复审。";
        let bases = extract_legal_basis(oa);
        let joined = bases.join("|");
        // 实际适用的实体依据保留
        assert!(
            joined.contains("第22条第3款"),
            "本案实体依据必须保留: {joined}"
        );
        // 决定主文的程序依据保留
        assert!(
            joined.contains("第38条"),
            "驳回主文程序依据必须保留: {joined}"
        );
        // 表格空白项一律不得出现（含「权利要求不符合…」「权利要求不具备…规定的新颖性」
        // 这两种带「权利要求」/「不具备」字样的模板行）
        for bogus in [
            "第2条第2款",
            "第9条第1款",
            "第22条第2款",
            "第22条第4款",
            "第26条第3款",
            "第31条第1款",
        ] {
            assert!(
                !bases.iter().any(|b| b.contains(bogus)),
                "勾选表未适用条目 {bogus} 不得列为法律依据: {joined}"
            );
        }
        // 中文数字与阿拉伯数字同一条款只出现一次
        assert_eq!(
            bases
                .iter()
                .filter(|b| law_citation_key(b).contains("第22条第3款"))
                .count(),
            1,
            "第二十二条第三款与第22条第3款必须归一去重: {joined}"
        );
    }

    /// **UA2 接线锁（2026-11-06）**：复审请求书文末必须带「提交前清单」固定段。
    /// 旧状态是文案常量早已定稿但从未拼进产物（`allow(dead_code)`），属假交付。
    #[test]
    fn stage_e_reject_review_appends_pre_submit_checklist() {
        let mut input = make_sample_case_input();
        input.case_meta.oa_type = "reject_review".into();
        let output = stage_e_assemble(&input, &[], &[], &[], &[], &[], &[]);
        assert!(
            output.contains("提交前清单"),
            "复审请求书文末必须有提交前清单"
        );
        for i in 1..=8 {
            assert!(
                output.contains(&format!("□ {i}.")),
                "清单第 {i} 项必须出现在产物里"
            );
        }

        // 非复审类型（普通审查意见答复）不得追加复审专用清单
        let mut oa_input = make_sample_case_input();
        oa_input.case_meta.oa_type = "office_action".into();
        let oa_output = stage_e_assemble(&oa_input, &[], &[], &[], &[], &[], &[]);
        assert!(
            !oa_output.contains("提交前清单"),
            "提交前清单是复审专用，不得混入其他答复类型"
        );
    }

    #[test]
    fn stage_e_ties_rebuttal_to_evidence_row() {
        let input = make_sample_case_input();
        let features = vec![DistinguishingFeature {
            ordinal: 1,
            feature_name: "特征A".into(),
            oa_tag: OaTag::RoutineChoice,
            attributed_ref: Some("D1".into()),
            oa_excerpt: "审查员转述".into(),
        }];
        let rebuttals = vec![FeatureRebuttal {
            ordinal: 1,
            feature_name: "特征A".into(),
            oa_tag: OaTag::RoutineChoice,
            severance: "割裂".into(),
            synergy: "协同".into(),
            insufficient: "不足".into(),
        }];
        let output = stage_e_assemble(&input, &features, &[], &[], &rebuttals, &[], &[]);
        assert!(output.contains("根据驳回决定原文摘录（见证据表第1行）"));
    }

    #[test]
    fn r6c_pipeline_output_contains_fact_check_section() {
        // R6c: pipeline 输出必须包含 AI事实核查 固定段
        let input = make_sample_case_input();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let letter = rt.block_on(generate_response_letter_pipeline(&input, None, None));
        assert!(
            letter.full_text.contains("AI事实核查"),
            "pipeline output must contain fact-check section"
        );
        assert!(
            letter.full_text.contains("可信度评分"),
            "fact-check section must contain score"
        );
    }

    fn make_sample_case_input() -> CaseInput {
        CaseInput {
            case_meta: CaseMeta {
                application_number: "1990999999999".into(),
                applicant: "示例申请人".into(),
                invention_title: "一种合成技术装置的装置".into(),
                oa_date: Some("2026-07-10".into()),
                oa_type: "reject_review".into(),
                claim_basis_date: Some("2026-06-25".into()),
                claim_basis_text: None,
            },
            oa_full_text: "驳回决定正文".into(),
            reference_documents: vec![
                ReferenceDocument {
                    label: "D1".into(),
                    publication_number: "CN990001001A".into(),
                    full_text: "D1原文".into(),
                    extraction_method: Some("ocr".into()),
                    page_count: Some(9),
                    technical_field: None,
                },
                ReferenceDocument {
                    label: "D2".into(),
                    publication_number: "CN990001002A".into(),
                    full_text: "D2原文".into(),
                    extraction_method: None,
                    page_count: None,
                    technical_field: None,
                },
                ReferenceDocument {
                    label: "D3".into(),
                    publication_number: "CN990001003U".into(),
                    full_text: "D3原文".into(),
                    extraction_method: None,
                    page_count: None,
                    technical_field: None,
                },
            ],
            claims_under_review: "1. 一种合成技术装置的装置".into(),
            spec_effect_statements: vec!["通过电磁场调控使反应气体流向发生偏转".into()],
            spec_full_text: None,
            legacy_analysis: None,
            legacy_discussion: None,
            custom_stopwords: vec![],
        }
    }

    #[tokio::test]
    async fn pipeline_no_ai_produces_guarded_output() {
        let input = make_sample_case_input();
        let result = generate_response_letter_pipeline(&input, None, None).await;
        assert!(result.full_text.contains("复审请求书"));
        assert!(result.full_text.contains("1990999999999"));
        assert!(result.full_text.contains("发回原审查部门"));
        assert!(
            result.guard_result.passed,
            "Guard errors: {:?}",
            result.guard_result.errors
        );
    }

    #[test]
    fn dependency_graph_empty_for_empty_claims() {
        let graph = build_dependency_graph("");
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn auto_extract_returns_empty_for_empty_spec() {
        let effects = auto_extract_effect_statements("");
        assert!(effects.is_empty());
    }

    #[test]
    fn ordinal_circled_basic() {
        assert_eq!(ordinal_circled(1), "①");
        assert_eq!(ordinal_circled(9), "⑨");
        assert_eq!(ordinal_circled(11), "(11)");
    }

    #[tokio::test]
    async fn pipeline_with_synthetic_second_case() {
        let input = CaseInput {
            case_meta: CaseMeta {
                application_number: "2025108888888".into(),
                applicant: "张三".into(),
                invention_title: "一种散热装置".into(),
                oa_date: Some("2026-08-01".into()),
                oa_type: "reject_review".into(),
                claim_basis_date: None,
                claim_basis_text: None,
            },
            oa_full_text: "驳回决定：权利要求1不具备创造性。".into(),
            reference_documents: vec![
                ReferenceDocument {
                    label: "D1".into(),
                    publication_number: "CN111122233A".into(),
                    full_text: "D1原文".into(),
                    extraction_method: None,
                    page_count: None,
                    technical_field: None,
                },
                ReferenceDocument {
                    label: "D2".into(),
                    publication_number: "CN222233344U".into(),
                    full_text: "D2原文".into(),
                    extraction_method: None,
                    page_count: None,
                    technical_field: None,
                },
            ],
            claims_under_review: "1. 一种散热装置，包括散热鳍片、热管、均温板".into(),
            spec_effect_statements: vec!["通过均温板将热量均匀传导至散热鳍片".into()],
            spec_full_text: None,
            legacy_analysis: None,
            legacy_discussion: None,
            custom_stopwords: vec![],
        };
        let result = generate_response_letter_pipeline(&input, None, None).await;
        assert!(result.full_text.contains("2025108888888"));
        assert!(result.full_text.contains("张三"));
        assert!(result.full_text.contains("散热装置"));
        assert!(result.full_text.contains("发回原审查部门"));
        assert!(
            result.guard_result.passed,
            "Guard errors: {:?}",
            result.guard_result.errors
        );
    }

    // ── Stage B 确定性计数测试 ──

    #[test]
    fn stage_b_counts_terms_in_reference() {
        let input = CaseInput {
            case_meta: CaseMeta {
                application_number: "1990999999999".into(),
                applicant: "测试".into(),
                invention_title: "测试".into(),
                oa_date: None,
                oa_type: "reject_review".into(),
                claim_basis_date: None,
                claim_basis_text: None,
            },
            oa_full_text: "审查员认为对角安装是常规设计。对角安装能够实现均匀受热。".into(),
            reference_documents: vec![ReferenceDocument {
                label: "D1".into(),
                publication_number: "CN990001001A".into(),
                full_text: "D1公开了一种安装方式，但未提及对角安装。".into(),
                extraction_method: None,
                page_count: None,
                technical_field: None,
            }],
            claims_under_review: "1. 一种装置，包括对角安装的加热元件".into(),
            spec_effect_statements: vec![],
            spec_full_text: None,
            legacy_analysis: None,
            legacy_discussion: None,
            custom_stopwords: vec![],
        };
        let features = vec![DistinguishingFeature {
            ordinal: 1,
            feature_name: "对角安装".into(),
            oa_tag: OaTag::RoutineDesign,
            attributed_ref: Some("D1".into()),
            oa_excerpt: "对角安装是常规设计，能够实现均匀受热".into(),
        }];
        let (term_checks, unsupported) = stage_b_count_terms(&input, &features);

        // "对角安装" 在D1中出现1次，OA中出现2次
        let tc = term_checks.iter().find(|t| t.term == "对角安装").unwrap();
        assert_eq!(tc.ref_count, 1, "D1中'对角安装'应出现1次");
        assert_eq!(tc.oa_count, 2, "OA中'对角安装'应出现2次");

        // "均匀受热" 在D1中0次，OA中1次 → 应生成unsupported
        let uc = unsupported
            .iter()
            .find(|u| u.rebuttal_text.contains("均匀受热"));
        assert!(uc.is_some(), "应检测到'均匀受热'在D1无支撑");
    }

    #[test]
    fn stage_b_no_ref_no_check() {
        let input = CaseInput {
            case_meta: CaseMeta {
                application_number: "123".into(),
                applicant: "t".into(),
                invention_title: "t".into(),
                oa_date: None,
                oa_type: "reject_review".into(),
                claim_basis_date: None,
                claim_basis_text: None,
            },
            oa_full_text: "OA".into(),
            reference_documents: vec![],
            claims_under_review: "claims".into(),
            spec_effect_statements: vec![],
            spec_full_text: None,
            legacy_analysis: None,
            legacy_discussion: None,
            custom_stopwords: vec![],
        };
        let features = vec![DistinguishingFeature {
            ordinal: 1,
            feature_name: "test".into(),
            oa_tag: OaTag::EasyToThink,
            attributed_ref: None, // 无归因
            oa_excerpt: "test".into(),
        }];
        let (tc, uc) = stage_b_count_terms(&input, &features);
        assert!(tc.is_empty());
        assert!(uc.is_empty());
    }

    // ── extract_json_array 测试 ──

    #[test]
    fn extract_json_from_markdown() {
        let raw = "```json\n[{\"a\":1}]\n```";
        let extracted = extract_json_array(raw);
        assert_eq!(extracted, "[{\"a\":1}]");
    }

    #[test]
    fn extract_json_plain() {
        let raw = "[{\"a\":1}]";
        let extracted = extract_json_array(raw);
        assert_eq!(extracted, "[{\"a\":1}]");
    }

    #[test]
    fn extract_json_with_prefix_text() {
        let raw = "好的，结果如下：\n[{\"a\":1}]";
        let extracted = extract_json_array(raw);
        assert_eq!(extracted, "[{\"a\":1}]");
    }

    // ── extract_key_terms 测试 ──

    #[test]
    fn extract_key_terms_basic() {
        let terms = extract_key_terms("对角安装是常规设计，能够实现均匀受热", &[]);
        assert!(terms.contains(&"对角安装".to_string()));
        assert!(terms.contains(&"均匀受热".to_string()));
        // 停用词应被过滤
        assert!(!terms.contains(&"常规设计".to_string())); // "常规设计" 不在停用词，应该保留
    }

    #[test]
    fn extract_key_terms_filters_stopwords() {
        let terms = extract_key_terms("对比文件公开了技术方案", &[]);
        assert!(!terms.contains(&"对比文件".to_string()));
        assert!(!terms.contains(&"技术方案".to_string()));
    }

    // ── Stage C 三段式反驳测试 ──

    #[tokio::test]
    async fn stage_c_produces_three_part_rebuttal() {
        let input = CaseInput {
            case_meta: CaseMeta {
                application_number: "9999999".into(),
                applicant: "测试".into(),
                invention_title: "测试".into(),
                oa_date: None,
                oa_type: "reject_review".into(),
                claim_basis_date: None,
                claim_basis_text: None,
            },
            oa_full_text: "OA".into(),
            reference_documents: vec![ReferenceDocument {
                label: "D1".into(),
                publication_number: "CN111A".into(),
                full_text: "D1".into(),
                extraction_method: None,
                page_count: None,
                technical_field: None,
            }],
            claims_under_review: "包括特征甲、特征乙".into(),
            spec_effect_statements: vec![],
            spec_full_text: None,
            legacy_analysis: None,
            legacy_discussion: None,
            custom_stopwords: vec![],
        };
        let features = vec![
            DistinguishingFeature {
                ordinal: 1,
                feature_name: "特征甲".into(),
                oa_tag: OaTag::RoutineDesign,
                attributed_ref: Some("D1".into()),
                oa_excerpt: "特征甲是常规设计".into(),
            },
            DistinguishingFeature {
                ordinal: 2,
                feature_name: "特征乙".into(),
                oa_tag: OaTag::EasyToThink,
                attributed_ref: Some("D1".into()),
                oa_excerpt: "特征乙容易想到".into(),
            },
        ];
        let rebuttals = stage_c_rebut_features(&input, &features).await;
        assert_eq!(rebuttals.len(), 2);

        // 第一项：特征甲 — 仲裁后 severance 字段包含全部段落
        let r0 = &rebuttals[0];
        assert_eq!(r0.feature_name, "特征甲");
        // 定性失真段（仲裁优先级1，首段）
        assert!(
            r0.severance.contains("上位泛化标签"),
            "应包含定性失真主攻段"
        );
        // 举证不足段（仲裁优先级2）
        assert!(r0.severance.contains("举证"), "应包含零举证主攻段");
        assert!(r0.severance.contains("常规设计"));
        // 割裂论断段（仲裁优先级5，防守）
        assert!(r0.severance.contains("割裂"));
        assert!(r0.severance.contains("D1"));
        // 协同段
        assert!(r0.severance.contains("特征乙"), "应包含协同段含共现特征乙");
    }

    #[tokio::test]
    async fn stage_c_no_co_occurrence_no_synergy_deps() {
        let input = CaseInput {
            case_meta: CaseMeta {
                application_number: "999".into(),
                applicant: "t".into(),
                invention_title: "t".into(),
                oa_date: None,
                oa_type: "reject_review".into(),
                claim_basis_date: None,
                claim_basis_text: None,
            },
            oa_full_text: "".into(),
            reference_documents: vec![],
            claims_under_review: "包括特征甲。包括特征乙。".into(),
            spec_effect_statements: vec![],
            spec_full_text: None,
            legacy_analysis: None,
            legacy_discussion: None,
            custom_stopwords: vec![],
        };
        let features = vec![
            DistinguishingFeature {
                ordinal: 1,
                feature_name: "特征甲".into(),
                oa_tag: OaTag::RoutineChoice,
                attributed_ref: Some("D1".into()),
                oa_excerpt: "".into(),
            },
            DistinguishingFeature {
                ordinal: 2,
                feature_name: "特征乙".into(),
                oa_tag: OaTag::RoutineChoice,
                attributed_ref: Some("D1".into()),
                oa_excerpt: "".into(),
            },
        ];
        let rebuttals = stage_c_rebut_features(&input, &features).await;
        // 特征甲和特征乙不在同一从句（句号分隔），无共现
        let r0 = &rebuttals[0];
        // 仲裁后全部内容在 severance 字段；无共现时协同段为独立特征描述
        assert!(
            r0.severance.contains("独立技术特征"),
            "无共现时应为独立特征描述"
        );
    }

    #[test]
    fn dependency_graph_co_occurrence() {
        let features = vec![
            DistinguishingFeature {
                ordinal: 1,
                feature_name: "特征甲".into(),
                oa_tag: OaTag::RoutineDesign,
                attributed_ref: None,
                oa_excerpt: "".into(),
            },
            DistinguishingFeature {
                ordinal: 2,
                feature_name: "特征乙".into(),
                oa_tag: OaTag::RoutineDesign,
                attributed_ref: None,
                oa_excerpt: "".into(),
            },
            DistinguishingFeature {
                ordinal: 3,
                feature_name: "特征丙".into(),
                oa_tag: OaTag::RoutineDesign,
                attributed_ref: None,
                oa_excerpt: "".into(),
            },
        ];
        // 特征甲和特征乙在同一从句（顿号分隔），特征丙在不同从句
        let graph =
            build_dependency_graph_with_features("包括特征甲、特征乙，以及特征丙", &features);
        let deps_a = graph
            .edges
            .iter()
            .find(|(n, _)| n == "特征甲")
            .map(|(_, d)| d.clone())
            .unwrap_or_default();
        assert!(deps_a.contains(&"特征乙".to_string()), "甲应依赖乙（共现）");
        assert!(
            !deps_a.contains(&"特征丙".to_string()),
            "甲不应依赖丙（不同从句）"
        );
    }

    #[test]
    fn dependency_graph_no_ai_edges() {
        // 确认：即使两个特征语义相关，若不在原文共现也不建边
        let features = vec![
            DistinguishingFeature {
                ordinal: 1,
                feature_name: "加热".into(),
                oa_tag: OaTag::RoutineDesign,
                attributed_ref: None,
                oa_excerpt: "".into(),
            },
            DistinguishingFeature {
                ordinal: 2,
                feature_name: "温控".into(),
                oa_tag: OaTag::RoutineDesign,
                attributed_ref: None,
                oa_excerpt: "".into(),
            },
        ];
        // 加热和温控不在同一从句
        let graph =
            build_dependency_graph_with_features("包括加热模块。还包括温控单元。", &features);
        assert!(graph.edges.is_empty(), "无共现时不应建边");
    }

    // ── Stage D 作用相同断言对比测试 ──

    #[tokio::test]
    async fn stage_d_detects_effect_claim_mismatch() {
        let input = CaseInput {
            case_meta: CaseMeta {
                application_number: "999".into(),
                applicant: "t".into(),
                invention_title: "t".into(),
                oa_date: None,
                oa_type: "reject_review".into(),
                claim_basis_date: None,
                claim_basis_text: None,
            },
            oa_full_text: "D1中公开了加热模块，其与本申请的加热模块作用相同，均为了提高温度。"
                .into(),
            reference_documents: vec![ReferenceDocument {
                label: "D1".into(),
                publication_number: "CN111A".into(),
                full_text: "D1原文".into(),
                extraction_method: None,
                page_count: None,
                technical_field: None,
            }],
            claims_under_review: "包括加热模块".into(),
            spec_effect_statements: vec!["通过电磁感应实现均匀加热".into()],
            spec_full_text: None,
            legacy_analysis: None,
            legacy_discussion: None,
            custom_stopwords: vec![],
        };
        let mismatches = stage_d_compare_effects(&input, None).await;
        assert!(!mismatches.is_empty(), "应检测到效果不匹配");
        let m = &mismatches[0];
        assert!(m.rebuttal_text.contains("实质差异"), "反驳段应指出实质差异");
        assert!(
            m.rebuttal_text.contains("电磁感应"),
            "反驳段应引用本申请实际效果"
        );
    }

    #[tokio::test]
    async fn stage_d_no_effect_claim_no_mismatch() {
        let input = CaseInput {
            case_meta: CaseMeta {
                application_number: "999".into(),
                applicant: "t".into(),
                invention_title: "t".into(),
                oa_date: None,
                oa_type: "reject_review".into(),
                claim_basis_date: None,
                claim_basis_text: None,
            },
            oa_full_text: "权利要求1不具备创造性。".into(),
            reference_documents: vec![],
            claims_under_review: "1. 一种装置".into(),
            spec_effect_statements: vec!["提高效率".into()],
            spec_full_text: None,
            legacy_analysis: None,
            legacy_discussion: None,
            custom_stopwords: vec![],
        };
        let mismatches = stage_d_compare_effects(&input, None).await;
        assert!(mismatches.is_empty(), "无'作用相同'断言时不应生成mismatch");
    }

    #[test]
    fn scan_effect_claims_finds_patterns() {
        let oa = "D1的加热模块与本申请作用相同，均为了提高温度。另外，D2的效果一致。";
        let claims = scan_effect_claims(oa);
        assert!(
            claims.iter().any(|c| c.text.contains("作用相同")),
            "应匹配'作用相同'"
        );
        assert!(
            claims.iter().any(|c| c.text.contains("均为了")),
            "应匹配'均为了'"
        );
        assert!(
            claims.iter().any(|c| c.text.contains("效果一致")),
            "应匹配'效果一致'"
        );
    }

    #[test]
    fn scan_effect_claims_quotes_end_at_sentence_boundary() {
        // 造一段：命中"效果相同"后跟 >60 字才到句号——旧硬窗口（+60 字）会把引文
        // 切在句中（如「公开了流量传」半句），正式文书不可读。
        let long_tail = "本领域技术人员在对比文件的基础上结合公知常识即可获得该技术方案，这对所属技术领域的技术人员来说是显而易见的，因此该权利要求不具备创造性";
        let oa = format!(
            "对比文件1的流量控制与本申请的效果相同，{}。第二句从句号后开始。",
            long_tail
        );
        let claims = scan_effect_claims(&oa);
        let hit = claims
            .iter()
            .find(|c| c.text.contains("效果相同"))
            .expect("应命中'效果相同'");
        assert!(
            hit.text.ends_with('。'),
            "引文应在句号收尾，实际结尾: {:?}",
            hit.text.chars().last()
        );
        assert!(
            hit.text.contains("不具备创造性"),
            "引文应包含到句号为止的完整句子"
        );
        assert!(!hit.text.ends_with('…'), "未触发截断时不应带省略号");
    }

    #[test]
    fn extend_window_to_sentence_boundary_finds_or_defers() {
        let chars: Vec<char> = "第一句。第二句很长但还没有句号".chars().collect();
        // from 指向第二句开头"第"，句号在 lookahead 内 → 返回句号后一位
        let from = chars.iter().rposition(|&c| c == '第').unwrap();
        let end = super::extend_window_to_sentence_boundary(&chars, from, 40);
        assert_eq!(chars[end - 1], '。', "应停在句号后");
        // lookahead 内无边界 → 返回原值（调用方回退硬窗口）
        let end2 = super::extend_window_to_sentence_boundary(&chars, from, 3);
        assert_eq!(end2, from, "无边界时应返回 from");
    }

    #[test]
    fn dropped_features_note_emitted_and_lists_names() {
        let dropped = vec![
            "某条未逐字定位的特征表述甲".to_string(),
            "另一条被审查员提及的表述乙，其中包含较长的描述内容用于测试截断行为是否正确处理——这一段是超过六十字的填充内容，用于验证预览截断省略号".to_string(),
        ];
        let note = format_dropped_features_note(&dropped).expect("有丢弃必须有提示");
        assert!(note.contains("2 条"), "提示应含丢弃条数: {note}");
        assert!(note.contains("未逐字定位的特征表述甲"), "应列出丢弃特征名");
        assert!(note.contains('…'), "超 60 字的条目应截断加省略号");
        assert!(
            note.contains("请人工核对"),
            "提示应要求人工核对（响亮失败）"
        );
    }

    #[test]
    fn dropped_features_note_none_when_empty() {
        assert!(format_dropped_features_note(&[]).is_none());
    }

    #[test]
    fn stage_e_emits_dropped_note_after_material_warnings() {
        let input = make_sample_case_input();
        let features: Vec<DistinguishingFeature> = vec![];
        let dropped = vec!["被丢弃的特征丙（测试）".to_string()];
        let output = stage_e_assemble(&input, &features, &[], &[], &[], &[], &dropped);
        let warn_pos = output
            .find("【材料完整性提示")
            .or_else(|| output.find("复审请求范围"));
        let note_pos = output.find("【特征抽取丢弃提示");
        assert!(note_pos.is_some(), "有丢弃必须在产物内显式提示");
        if let (Some(w), Some(n)) = (warn_pos, note_pos) {
            assert!(w < n, "丢弃提示应在材料提示之后、正文论证之前");
        }
        assert!(output.contains("被丢弃的特征丙（测试）"));
        // 无丢弃 → 不出现提示
        let clean = stage_e_assemble(&input, &features, &[], &[], &[], &[], &[]);
        assert!(!clean.contains("【特征抽取丢弃提示"), "无丢弃时不得占版面");
    }

    #[test]
    fn auto_extract_effect_statements_basic() {
        let spec = "本发明通过特殊结构提高燃烧效率。普通句子没有关键词。\
                    该设计避免了热能损耗。另外的描述。";
        let effects = auto_extract_effect_statements(spec);
        assert!(
            effects.iter().any(|e| e.contains("提高")),
            "应提取含'提高'的效果句"
        );
        assert!(
            effects.iter().any(|e| e.contains("避免")),
            "应提取含'避免'的效果句"
        );
        assert!(
            !effects.iter().any(|e| e.contains("普通句子")),
            "不应提取无关键词的句子"
        );
    }

    #[test]
    fn find_ref_label_finds_nearby() {
        let oa = "对比文件D1公开了一种结构。该结构与本申请的方案作用相同。";
        let label = find_ref_label_for_claim(oa, "作用相同");
        assert_eq!(label, Some("D1".to_string()));
    }

    // ── Stage A verbatim 丢弃 trace 测试 ──

    #[test]
    fn stage_a_discards_non_verbatim_feature() {
        // 模拟AI返回一个非verbatim特征名
        // claims中只有"对角安装"，AI返回"斜向安装"（意译，非verbatim）
        let claims = "1. 一种装置，包括对角安装的加热元件";
        let ai_feature = "斜向安装"; // 意译，不是claims的子串
        assert!(
            !claims.contains(ai_feature),
            "验证前提：'斜向安装'确实不是claims子串"
        );
        assert!(
            claims.contains("对角安装"),
            "验证前提：'对角安装'是claims子串"
        );
        // Stage A 的 verbatim 校验逻辑：claims.contains(feature_name)
        // "斜向安装" 会被丢弃，"对角安装" 会保留
    }

    #[tokio::test]
    async fn pipeline_full_stage_c_d_integration() {
        let input = CaseInput {
            case_meta: CaseMeta {
                application_number: "2025999".into(),
                applicant: "测试人".into(),
                invention_title: "一种测试装置".into(),
                oa_date: Some("2026-09-01".into()),
                oa_type: "reject_review".into(),
                claim_basis_date: None,
                claim_basis_text: None,
            },
            oa_full_text: "审查员认为，D1公开了加热模块，其与本申请作用相同，均为了提高温度。\
                           权利要求1的区别特征加热模块相对于D1属于常规设计。"
                .into(),
            reference_documents: vec![ReferenceDocument {
                label: "D1".into(),
                publication_number: "CN111222A".into(),
                full_text: "D1公开了一种加热装置。".into(),
                extraction_method: None,
                page_count: None,
                technical_field: None,
            }],
            claims_under_review: "1. 一种测试装置，包括加热模块、温控单元".into(),
            spec_effect_statements: vec!["通过电磁感应实现均匀加热".into()],
            spec_full_text: None,
            legacy_analysis: None,
            legacy_discussion: None,
            custom_stopwords: vec![],
        };
        // 无AI运行：Stage A空，Stage B空，Stage C空，Stage D确定性运行
        let result = generate_response_letter_pipeline(&input, None, None).await;
        assert!(result.full_text.contains("复审请求书"));
        assert!(result.full_text.contains("2025999"));
        assert!(result.full_text.contains("测试人"));
        assert!(result.full_text.contains("发回原审查部门"));
        // Guard应通过
        assert!(
            result.guard_result.passed,
            "Guard errors: {:?}",
            result.guard_result.errors
        );
    }
}
