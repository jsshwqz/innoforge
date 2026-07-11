//! OA 分析事实校验层 / OA Analysis Fact-Checking Layer
//!
//! 对 AI 生成的 OA 答复分析做事实性后处理校验，检测四类常见 AI 错误：
//! 1. 对比文件技术领域识别错误（幻觉）
//! 2. 段落引用不存在（编造出处）
//! 3. 无来源的定量数据（编造数据）
//! 4. 修改建议违反 A33（引入说明书未记载的术语）
//!
//! 设计原则：
//! - 纯函数，不依赖网络/大模型，可独立单元测试
//! - 只生成警告，不阻断 AI 输出（不破坏现有功能）
//! - 适配流式场景：输入文本可能已被 sanitize（换行→空格）
//!
//! 注：本模块为 OA 分析增强功能的预留层，当前未接入主流程，已标记 #[allow(dead_code)]

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// 单条校验警告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactWarning {
    /// 警告类别："技术领域错误" | "段落引用不存在" | "无来源数据" | "A33风险"
    pub category: String,
    /// 人类可读的警告描述
    pub description: String,
    /// 严重程度："致命" | "高" | "中"
    pub severity: String,
}

/// 完整校验报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactCheckReport {
    pub warnings: Vec<FactWarning>,
    /// 可信度评分 0-100（警告越多越低）
    pub score: f64,
}

impl Default for FactCheckReport {
    fn default() -> Self {
        Self {
            warnings: Vec::new(),
            score: 100.0,
        }
    }
}

/// 通用停用词（在技术领域提取时过滤，避免"流量""控制"等通用词造成误判）
const STOP_WORDS: &[&str] = &[
    "系统",
    "装置",
    "方法",
    "控制",
    "调节",
    "流量",
    "装置的",
    "一种",
    "所述",
    "本发明",
    "本实用新型",
    "包括",
    "设置",
    "连接",
    "以及",
    "通过",
    "用于",
    "其特征",
    "进一步",
];

// ═══════════════════════════════════════════════════════════════════════
//  主入口
// ═══════════════════════════════════════════════════════════════════════

/// 对 OA 分析文本执行全量事实校验
///
/// 参数：
/// - `ai_output`: AI 生成的分析文本
/// - `references`: 对比文献原文（用于校验技术领域、段落引用）
/// - `my_patent`: 本专利说明书原文（用于 A33 校验）
pub fn check_oa_analysis(ai_output: &str, references: &str, my_patent: &str) -> FactCheckReport {
    let mut report = FactCheckReport::default();

    // ① 技术领域一致性校验
    let domain_report = check_tech_domain(ai_output, references);
    report.warnings.extend(domain_report.warnings);

    // ② 段落引用存在性校验
    let ref_report = check_paragraph_refs(ai_output, references);
    report.warnings.extend(ref_report.warnings);

    // ③ 无来源数据检测
    let data_report = check_fabricated_data(ai_output);
    report.warnings.extend(data_report.warnings);

    // ④ A33 合规性风险检测
    let a33_report = check_a33_risk(ai_output, my_patent);
    report.warnings.extend(a33_report.warnings);

    // 计算综合评分
    report.score = compute_score(&report.warnings);

    report
}

/// 根据警告列表计算可信度评分
fn compute_score(warnings: &[FactWarning]) -> f64 {
    let mut penalty = 0.0;
    for w in warnings {
        penalty += match w.severity.as_str() {
            "致命" => 35.0,
            "高" => 15.0,
            "中" => 5.0,
            _ => 2.0,
        };
    }
    (100.0_f64 - penalty).max(0.0)
}

/// 将校验报告格式化为人类可读的 Markdown 文本（追加到分析结果末尾）
pub fn format_report(report: &FactCheckReport) -> String {
    if report.warnings.is_empty() {
        return format!(
            "可信度评分：{:.0}/100。未检测到明显事实性问题。",
            report.score
        );
    }

    let mut out = String::new();
    out.push_str(&format!("可信度评分：{:.0}/100。\n\n", report.score));
    out.push_str(&format!("检测到 {} 个可疑点：\n\n", report.warnings.len()));

    for (i, w) in report.warnings.iter().enumerate() {
        let icon = match w.severity.as_str() {
            "致命" => "[致命]",
            "高" => "[高]",
            "中" => "[中]",
            _ => "[?]",
        };
        out.push_str(&format!(
            "{}. {}（{}）{}\n",
            i + 1,
            icon,
            w.category,
            w.description
        ));
    }

    out.push_str("\n请人工核实上述问题后再用于正式答复。");
    out
}

// ═══════════════════════════════════════════════════════════════════════
//  校验①：技术领域一致性
// ═══════════════════════════════════════════════════════════════════════

/// 提取文本中的技术领域关键词（从"技术领域""背景技术"等标志性段落）
///
/// 返回去停用词后的领域特有名词集合
fn extract_domain_keywords(text: &str) -> Vec<String> {
    let mut keywords = Vec::new();

    // 定位技术领域相关段落
    let markers = [
        "技术领域",
        "背景技术",
        "本发明涉及",
        "本实用新型涉及",
        "属于",
    ];

    let mut domain_text = String::new();
    // 找到标志词后截取其后 500 字符作为领域描述
    for marker in &markers {
        if let Some(pos) = text.find(marker) {
            let start = pos;
            let end = (start + 500).min(text.len());
            // 确保 UTF-8 边界安全
            let safe_end = text[..end]
                .char_indices()
                .last()
                .map(|(i, _)| i + text[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1))
                .unwrap_or(end);
            domain_text.push_str(&text[start..safe_end.min(text.len())]);
        }
    }

    // 如果没找到标志段落，用全文前 1000 字符
    if domain_text.is_empty() {
        let end = text
            .char_indices()
            .take(1000)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(text.len());
        domain_text.push_str(&text[..end]);
    }

    // 提取 2-6 字的中文词组（连续的中文字符块）
    for chunk in domain_text.split(|c: char| !('\u{4e00}'..='\u{9fff}').contains(&c)) {
        let chars: Vec<char> = chunk.chars().collect();
        if chars.len() >= 2 && chars.len() <= 6 {
            let word: String = chars.iter().collect();
            // 过滤停用词
            if !STOP_WORDS.contains(&word.as_str()) {
                keywords.push(word);
            }
        }
    }

    keywords.sort();
    keywords.dedup();
    keywords
}

/// 从 AI 输出中提取描述对比文献技术领域的语句
///
/// 查找含 "D1""对比文件""对比文献" 且含 "领域/用于/涉及/属于" 的句子
fn extract_ai_domain_claims(ai_output: &str) -> Vec<String> {
    let mut claims = Vec::new();

    // 按句号/换行/空格（sanitized后） 分句
    let sentences: Vec<&str> = ai_output.split(['。', '.', '\n', ' ', '；', ';']).collect();

    let domain_indicators = ["领域", "用于", "涉及", "属于", "应用场景", "工况"];

    for sentence in &sentences {
        let has_ref = sentence.contains("D1")
            || sentence.contains("对比文件")
            || sentence.contains("对比文献")
            || sentence.contains("D2");
        let has_domain = domain_indicators.iter().any(|ind| sentence.contains(ind));

        if has_ref && has_domain {
            claims.push(sentence.trim().to_string());
        }
    }

    claims
}

/// 技术领域一致性校验
///
/// 比较 AI 对对比文献领域的描述与对比文献原文的领域关键词
pub fn check_tech_domain(ai_output: &str, references: &str) -> FactCheckReport {
    let mut report = FactCheckReport::default();

    if references.trim().is_empty() {
        return report; // 无对比文献则跳过
    }

    let ref_keywords = extract_domain_keywords(references);
    if ref_keywords.is_empty() {
        return report;
    }

    let ai_claims = extract_ai_domain_claims(ai_output);

    for claim in &ai_claims {
        // 检查 AI 描述中是否包含对比文献的领域关键词
        let mut match_count = 0;
        for kw in &ref_keywords {
            if claim.contains(kw) {
                match_count += 1;
            }
        }

        // 关键词重合度为 0，且 AI 描述中有其他领域名词 → 强烈怀疑幻觉
        if match_count == 0 {
            // 检查 AI 是否用了对比文献中不存在的领域词（如"消防""供水"对比"供热"）
            let suspect_domains = [
                "消防",
                "供水",
                "排水",
                "消防车",
                "农业",
                "医疗",
                "航空",
                "汽车",
                "船舶",
                "食品",
                "纺织",
                "造纸",
                "矿山",
            ];
            let hit_suspect = suspect_domains.iter().find(|d| claim.contains(*d));

            if let Some(suspect) = hit_suspect {
                // 确认对比文献中确实没有这个词
                if !references.contains(suspect) {
                    report.warnings.push(FactWarning {
                        category: "技术领域错误".to_string(),
                        description: format!(
                            "AI 称对比文献涉及「{}」，但对比文献原文未出现该词，原文领域关键词为：{}。疑似技术领域识别错误（幻觉）。",
                            suspect,
                            ref_keywords.iter().take(5).cloned().collect::<Vec<_>>().join("、")
                        ),
                        severity: "致命".to_string(),
                    });
                }
            }
        }
    }

    report.score = compute_score(&report.warnings);
    report
}

// ═══════════════════════════════════════════════════════════════════════
//  校验②：段落引用存在性
// ═══════════════════════════════════════════════════════════════════════

/// 提取 AI 输出中引用的段落号
///
/// 匹配模式：第[00XX]段、第XXXX段、说明书第X段、[00XX]段
fn extract_paragraph_refs(text: &str) -> Vec<String> {
    let mut refs = Vec::new();

    // 匹配 [00XX] 格式
    if let Ok(re) = regex::Regex::new(r"\[(\d{3,4})\]") {
        for cap in re.captures_iter(text) {
            if let Some(num) = cap.get(1) {
                refs.push(num.as_str().to_string());
            }
        }
    }

    // 匹配 第00XX段 / 第XXXX段 格式
    if let Ok(re) = regex::Regex::new(r"第\s*\[?(\d{3,4})\]?\s*段") {
        for cap in re.captures_iter(text) {
            if let Some(num) = cap.get(1) {
                let n = num.as_str().to_string();
                if !refs.contains(&n) {
                    refs.push(n);
                }
            }
        }
    }

    refs.sort();
    refs.dedup();
    refs
}

/// 检查对比文献中是否存在某段落号
fn paragraph_exists_in(ref_num: &str, references: &str) -> bool {
    // 检查 [00XX] 格式
    if references.contains(&format!("[{}]", ref_num)) {
        return true;
    }
    // 检查不带括号的格式
    if references.contains(&format!("{}]", ref_num)) {
        return true;
    }
    // 补零处理：如果 AI 写了 "5" 但原文是 "[0005]"
    let zero_padded = format!("{:0>4}", ref_num);
    if references.contains(&format!("[{}]", zero_padded)) {
        return true;
    }
    false
}

/// 段落引用存在性校验
pub fn check_paragraph_refs(ai_output: &str, references: &str) -> FactCheckReport {
    let mut report = FactCheckReport::default();

    if references.trim().is_empty() {
        return report;
    }

    let refs = extract_paragraph_refs(ai_output);
    if refs.is_empty() {
        return report;
    }

    let mut missing: Vec<String> = Vec::new();
    for r in &refs {
        if !paragraph_exists_in(r, references) {
            missing.push(r.clone());
        }
    }

    if !missing.is_empty() {
        report.warnings.push(FactWarning {
            category: "段落引用不存在".to_string(),
            description: format!(
                "AI 引用了段落号 [{}]，但对比文献原文中未找到对应段落。可能为编造的出处。",
                missing.join("]、[")
            ),
            severity: if missing.len() >= 3 {
                "高".to_string()
            } else {
                "中".to_string()
            },
        });
    }

    report.score = compute_score(&report.warnings);
    report
}

// ═══════════════════════════════════════════════════════════════════════
//  校验③：无来源数据检测
// ═══════════════════════════════════════════════════════════════════════

/// 检测 AI 输出中的定量数据，并验证是否有来源标注
pub fn check_fabricated_data(ai_output: &str) -> FactCheckReport {
    let mut report = FactCheckReport::default();

    // 定量数据模式：数字 + 单位
    let data_patterns = [
        (r"\d+(?:\.\d+)?\s*%", "百分比"),
        (r"\d+(?:\.\d+)?\s*毫秒", "时间"),
        (r"\d+(?:\.\d+)?\s*摄氏度", "温度"),
        (r"\d+(?:\.\d+)?\s*℃", "温度"),
        (r"\d+(?:\.\d+)?\s*度", "温度"),
        (r"\d+(?:\.\d+)?\s*MPa", "压力"),
        (r"\d+(?:\.\d+)?\s*秒", "时间"),
    ];

    // 来源标注关键词
    let source_markers = [
        "参见说明书",
        "说明书第",
        "说明书记载",
        "对比文件",
        "对比文献",
        "D1第",
        "D2第",
        "根据",
        "原文",
        "背景技术",
        "有益效果",
        "公知常识",
        "本领域",
        "现有技术",
    ];

    let mut unsourced_data: Vec<String> = Vec::new();

    // 按句子分割（适配 sanitized 文本：换行已变空格）
    let sentences: Vec<&str> = ai_output.split(['。', '\n']).collect();

    for sentence in &sentences {
        for (pattern, unit_label) in &data_patterns {
            let re = match regex::Regex::new(pattern) {
                Ok(r) => r,
                Err(_) => continue,
            };

            if re.is_match(sentence) {
                // 检查该句附近是否有来源标注
                let has_source = source_markers.iter().any(|m| sentence.contains(m));

                if !has_source {
                    // 提取匹配到的具体数据
                    if let Some(m) = re.find(sentence) {
                        let data_str = m.as_str().to_string();
                        if !unsourced_data.contains(&data_str) {
                            unsourced_data.push(data_str);
                        }
                    }
                    let _ = unit_label; // 标记使用
                }
            }
        }
    }

    if !unsourced_data.is_empty() {
        report.warnings.push(FactWarning {
            category: "无来源数据".to_string(),
            description: format!(
                "AI 使用了定量数据「{}」但未标注来源（说明书段落号/对比文件出处）。可能为编造的数据，请人工核实。",
                unsourced_data.join("」「")
            ),
            severity: "中".to_string(),
        });
    }

    report.score = compute_score(&report.warnings);
    report
}

// ═══════════════════════════════════════════════════════════════════════
//  校验④：A33 合规性风险
// ═══════════════════════════════════════════════════════════════════════

/// 一组高风险术语——这些通常是 AI 编造的、在说明书中不太可能出现的专业术语
///
/// 当这些术语出现在"修改建议"中但不在说明书原文中时，强烈提示 A33 风险
const HIGH_RISK_AMENDMENT_TERMS: &[&str] = &[
    "以太网",
    "Profinet",
    "EtherCAT",
    "Modbus",
    "总线",
    "确定性",
    "实时以太网",
    "工业总线",
    "毫秒级",
    "亚毫秒",
    "微秒",
    "OTA升级",
    "云端协同",
    "预测性维护",
    "边缘计算",
    "数字孪生",
    "区块链",
];

/// 从 AI 输出中提取权利要求修改建议段落
fn extract_amendment_section(ai_output: &str) -> String {
    let mut result = String::new();

    // 定位"修改方案""修改后的权利要求""建议修改"等段落
    let markers = [
        "修改后的权利要求",
        "建议修改",
        "修改方案",
        "层级C",
        "权利要求修改",
        "修改为",
    ];

    for marker in &markers {
        if let Some(pos) = ai_output.find(marker) {
            // 截取标志词后 800 字符
            let start = pos;
            let remaining = &ai_output[start..];
            let end = remaining
                .char_indices()
                .take(800)
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(remaining.len());
            result.push_str(&remaining[..end]);
            result.push('\n');
        }
    }

    result
}

/// A33 合规性风险检测
///
/// 检查 AI 建议的权利要求修改是否引入了说明书中不存在的术语
pub fn check_a33_risk(ai_output: &str, my_patent: &str) -> FactCheckReport {
    let mut report = FactCheckReport::default();

    if my_patent.trim().is_empty() {
        return report;
    }

    let amendment_text = extract_amendment_section(ai_output);
    if amendment_text.is_empty() {
        return report;
    }

    let mut risky_terms: Vec<String> = Vec::new();

    for term in HIGH_RISK_AMENDMENT_TERMS {
        // 术语出现在修改建议中
        if amendment_text.contains(term) {
            // 但不在说明书原文中
            if !my_patent.contains(term) {
                risky_terms.push(term.to_string());
            }
        }
    }

    // 额外检测：提取"修改为："后的引号内容，检查是否在原文中
    if let Ok(re) = regex::Regex::new(r#"修改为[：:]\s*[「"]([^」"]+)[」"]"#) {
        for cap in re.captures_iter(&amendment_text) {
            if let Some(amended) = cap.get(1) {
                let text = amended.as_str();
                // 检查这段修改内容中的关键术语是否在原文中
                // 提取 3 字以上的中文术语
                for chunk in text.split(|c: char| !('\u{4e00}'..='\u{9fff}').contains(&c)) {
                    let chars: Vec<char> = chunk.chars().collect();
                    if chars.len() >= 3 && chars.len() <= 8 {
                        let word: String = chars.iter().collect();
                        if !my_patent.contains(&word)
                            && !STOP_WORDS.contains(&word.as_str())
                            && !risky_terms.contains(&word)
                        {
                            // 只报告看起来像专业术语的词（避免过多误报）
                            let likely_term = HIGH_RISK_AMENDMENT_TERMS
                                .iter()
                                .any(|t| word.contains(t))
                            // 包含高风险关键词的变体
                            || word.contains("协议")
                            || word.contains("算法")
                            || word.contains("架构");
                            if likely_term {
                                risky_terms.push(word);
                            }
                        }
                    }
                }
            }
        }
    }

    if !risky_terms.is_empty() {
        report.warnings.push(FactWarning {
            category: "A33风险".to_string(),
            description: format!(
                "AI 修改建议中使用了术语「{}」，但本专利说明书原文中未出现。写入权利要求可能违反专利法第33条（修改超范围）。必须人工核实或删除。",
                risky_terms.join("」「")
            ),
            severity: "高".to_string(),
        });
    }

    report.score = compute_score(&report.warnings);
    report
}

// ═══════════════════════════════════════════════════════════════════════
//  测试
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── 技术领域校验测试 ──

    #[test]
    fn test_tech_domain_catches_hallucination() {
        // 模拟真实案例：AI 说 D1 是"消防车"，但 D1 原文是"供热管网"
        let ai_output = "D1涉及水陆消防车及二次供水设备领域，属于通用流体输送。";
        let ref_text = "背景技术：目前，在城市集中供热管网系统中普遍存在水力失衡情况出现。本实用新型旨在解决供热管网水力失衡问题。";

        let report = check_tech_domain(ai_output, ref_text);

        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.severity == "致命" && w.category == "技术领域错误"),
            "应检测到技术领域幻觉，实际警告：{:?}",
            report.warnings
        );
    }

    #[test]
    fn test_tech_domain_passes_correct_description() {
        // AI 正确描述了 D1 的技术领域
        let ai_output = "D1涉及城市集中供热管网系统的水力平衡调节领域。";
        let ref_text = "背景技术：目前，在城市集中供热管网系统中普遍存在水力失衡情况。";

        let report = check_tech_domain(ai_output, ref_text);
        assert!(
            report.warnings.is_empty(),
            "正确描述不应触发警告，实际：{:?}",
            report.warnings
        );
    }

    #[test]
    fn test_tech_domain_empty_references_skipped() {
        let report = check_tech_domain("D1涉及消防领域", "");
        assert!(report.warnings.is_empty());
    }

    // ── 段落引用校验测试 ──

    #[test]
    fn test_paragraph_ref_catches_missing() {
        let ai_output = "对比文件1第[0005]段公开了电缸结构。D1第[0099]段也有记载。";
        let ref_text = "[0001] 技术领域。\n[0003] 背景技术。\n[0007] 具体实施方式。";

        let report = check_paragraph_refs(ai_output, ref_text);

        assert!(!report.warnings.is_empty(), "应检测到不存在的段落号");
        // [0005] 和 [0099] 都不存在
        assert!(
            report.warnings[0].description.contains("0005")
                || report.warnings[0].description.contains("0099"),
            "警告应包含不存在的段落号"
        );
    }

    #[test]
    fn test_paragraph_ref_passes_existing() {
        let ai_output = "D1第[0003]段公开了背景技术。";
        let ref_text = "[0001] 技术领域。\n[0003] 背景技术。\n[0007] 具体实施方式。";

        let report = check_paragraph_refs(ai_output, ref_text);
        assert!(report.warnings.is_empty(), "存在的段落号不应触发警告");
    }

    #[test]
    fn test_paragraph_ref_handles_zero_padding() {
        // AI 写 "第5段"，原文是 "[0005]"
        let ai_output = "说明书第5段记载了控制逻辑。";
        let ref_text = "[0005] 计算机控制器内设有PLC程序。";

        let report = check_paragraph_refs(ai_output, ref_text);
        // 补零后应能匹配
        assert!(report.warnings.is_empty(), "补零匹配应通过");
    }

    // ── 无来源数据检测测试 ──

    #[test]
    fn test_fabricated_data_catches_unsourced() {
        let ai_output =
            "燃烧器火焰控制中，流量变化1%即可导致火焰温度波动数十摄氏度。要求毫秒级响应。";

        let report = check_fabricated_data(ai_output);
        assert!(!report.warnings.is_empty(), "应检测到无来源数据");
    }

    #[test]
    fn test_fabricated_data_passes_sourced() {
        let ai_output =
            "参见说明书有益效果第1项，流量稳定性提高了5%。根据D1第[0017]段，控制精度为2%。";

        let report = check_fabricated_data(ai_output);
        assert!(report.warnings.is_empty(), "有来源标注的数据不应触发警告");
    }

    // ── A33 风险检测测试 ──

    #[test]
    fn test_a33_catches_unsupported_terms() {
        let ai_output = "层级C—权利要求修改方案：建议将权利要求1修改为：所述计算机控制器为工业PLC，通过工业实时以太网总线电性连接，以进行确定性闭环控制。";

        let patent_text = "说明书：计算机控制器内设有PLC程序。电性连接。闭环控制。";

        let report = check_a33_risk(ai_output, patent_text);
        assert!(
            report.warnings.iter().any(|w| w.category == "A33风险"),
            "应检测到 A33 风险术语（以太网总线/确定性），实际：{:?}",
            report.warnings
        );
    }

    #[test]
    fn test_a33_passes_supported_terms() {
        let ai_output = "建议修改为：所述电动执行器为电缸，电缸的伸缩轴与阀芯连接。";
        let patent_text = "说明书：电动执行器优选为电缸，电缸的伸缩轴与阀芯连接。";

        let report = check_a33_risk(ai_output, patent_text);
        assert!(report.warnings.is_empty(), "说明书已支持的术语不应触发警告");
    }

    #[test]
    fn test_a33_empty_amendment_skipped() {
        let report = check_a33_risk("这是一段普通分析，没有修改建议。", "说明书内容");
        assert!(report.warnings.is_empty());
    }

    // ── 综合校验测试 ──

    #[test]
    fn test_full_check_combines_all() {
        let ai_output = "D1涉及消防车领域。对比文件第[0099]段公开了电缸。流量变化1%导致温度波动50度。建议修改为：通过以太网总线连接。";
        let references = "背景技术：城市集中供热管网。说明书第[0001]段。";
        let patent = "说明书：电性连接，闭环控制。";

        let report = check_oa_analysis(ai_output, references, patent);

        // 应触发至少 3 类警告
        let categories: Vec<&str> = report
            .warnings
            .iter()
            .map(|w| w.category.as_str())
            .collect();
        assert!(categories.contains(&"技术领域错误"), "应包含技术领域错误");
        assert!(categories.contains(&"段落引用不存在"), "应包含段落引用错误");
        assert!(report.score < 100.0, "有警告时评分应低于100");
    }

    #[test]
    fn test_clean_output_scores_high() {
        let ai_output =
            "权利要求1与对比文件1相比，区别特征在于电缸直行程调节。参见说明书有益效果第1项。";
        let references = "背景技术：供热管网。[0001] 技术领域。";
        let patent = "说明书：电缸直行程调节。有益效果。";

        let report = check_oa_analysis(ai_output, references, patent);
        assert!(
            report.score >= 80.0,
            "干净输出评分应较高，实际：{}",
            report.score
        );
    }

    // ── 格式化测试 ──

    #[test]
    fn test_format_report_with_warnings() {
        let report = FactCheckReport {
            warnings: vec![FactWarning {
                category: "技术领域错误".to_string(),
                description: "测试描述".to_string(),
                severity: "致命".to_string(),
            }],
            score: 65.0,
        };
        let formatted = format_report(&report);
        assert!(formatted.contains("65"));
        assert!(formatted.contains("致命"));
        assert!(formatted.contains("技术领域错误"));
    }

    #[test]
    fn test_format_report_empty() {
        let report = FactCheckReport::default();
        let formatted = format_report(&report);
        assert!(formatted.contains("100"));
    }
}
