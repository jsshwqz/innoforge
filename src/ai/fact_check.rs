//! OA 分析事实校验层 / OA Analysis Fact-Checking Layer
//!
//! 对 AI 生成的 OA 答复分析做事实性后处理校验，检测四类常见 AI 错误：
//! 1. 对比文件技术领域识别错误（幻觉）
//! 2. 段落引用不存在（编造出处）
//! 3. 无来源的定量数据（编造数据）
//! 4. 修改建议违反 A33（修改稿特征句回原始申请文本逐字/滑窗零命中）
//!
//! 设计原则：
//! - 纯函数，不依赖网络/大模型，可独立单元测试
//! - 只生成警告，不阻断 AI 输出（不破坏现有功能）
//! - 适配流式场景：输入文本可能已被 sanitize（换行→空格）
//!
//! 注：本模块为 OA 分析与创意分析共用的事实校验层，已接入 OA 答复和创意分析两条路径

use serde::{Deserialize, Serialize};

/// 单条校验警告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactWarning {
    /// 警告类别："技术领域错误" | "段落引用不存在" | "无来源数据" | "A33逐字核验" | "A33断言降级"
    pub category: String,
    /// 人类可读的警告描述
    pub description: String,
    /// 严重程度："致命" | "高" | "中"
    pub severity: String,
}

/// C0 提取元信息：记录每个 PDF 的提取方法、实测计数、来源
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct C0ExtractionInfo {
    /// PDF 文件标识（公开号或文件名）
    pub source_id: String,
    /// 提取方法：TextLayer / OcrFallback / ImageOnlyEmpty
    pub method: String,
    /// 页数
    pub n_pages: usize,
    /// 原始字符数（去空白）
    pub n_chars: usize,
    /// 关键术语实测计数 { term: count }
    pub term_counts: Vec<(String, usize)>,
    /// 来源注释（公开号 + 来源 + 下载日期）
    pub source_note: String,
}

/// 完整校验报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactCheckReport {
    pub warnings: Vec<FactWarning>,
    /// 可信度评分 0-100（警告越多越低）
    pub score: f64,
    /// C0 提取元信息列表（每个 PDF 一条）
    #[serde(default)]
    pub c0_extractions: Vec<C0ExtractionInfo>,
}

impl Default for FactCheckReport {
    fn default() -> Self {
        Self {
            warnings: Vec::new(),
            score: 100.0,
            c0_extractions: Vec::new(),
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

    // M2-B（基线失败 #2）：空/纯空白输入必须显式失败。旧行为是走完全部检测器、
    // warnings 为空 → compute_score([])=100（"未检测到明显事实性问题"）——空输入默认
    // 通过，与 #79 否决的假绿同族。M1 实测：主分析静默失败后空文本拿了 100/100。
    // M2 审查 P1-7：is_blank 覆盖 Unicode 空白与零宽字符（trim 不算零宽）。
    if is_blank_incl_zero_width(ai_output) {
        report.warnings.push(FactWarning {
            category: "无内容可核查".to_string(),
            description: "分析文本为空（或仅空白）：事实核查无对象，评分 0——请先确认主分析已产出。"
                .to_string(),
            severity: "致命".to_string(),
        });
        report.score = 0.0;
        return report;
    }

    // ① 技术领域一致性校验
    let domain_report = check_tech_domain(ai_output, references);
    report.warnings.extend(domain_report.warnings);

    // ② 段落引用存在性校验
    let ref_report = check_paragraph_refs(ai_output, references);
    report.warnings.extend(ref_report.warnings);

    // ③ 无来源数据检测
    let data_report = check_fabricated_data(ai_output);
    report.warnings.extend(data_report.warnings);

    // ④ A33 合规性核验（UA3 v2：修改稿特征句逐字回原文，判据见
    //    evidence_kernel::amendment_gate；旧 17 词表判据 check_a33_risk 已删除）
    let a33_report = check_a33_verbatim(ai_output, my_patent);
    report.warnings.extend(a33_report.warnings);

    // ⑤ C1-C13 纯函数检测层
    let c_warnings = run_c_module_checks(ai_output);
    report.warnings.extend(c_warnings);

    // 计算综合评分
    report.score = compute_score(&report.warnings);

    report
}

/// 从 PDF 文件执行事实校验：先 extract_pdf 提取文本，再 check_oa_analysis
///
/// 对每个 PDF 调用 extract_pdf，记录提取方法 + 实测计数 + 来源注释，
/// 然后将提取后的文本传入 check_oa_analysis 执行 C1-C13 检测。
///
/// 参数：
/// - `ai_output`: AI 生成的分析文本
/// - `reference_pdfs`: 对比文献 PDF 路径列表，每条含 (路径, 公开号, 关键术语列表, 来源注释)
/// - `my_patent_pdf`: 本专利 PDF 路径（可选，None 则跳过 A33 校验）
/// - `my_patent_terms`: 本专利关键术语列表（用于 C0 计数记录）
#[allow(dead_code)] // 入口尚未接线（PDF 直读事实核验路径），保留定义以免能力丢失
pub fn check_oa_analysis_from_pdfs(
    ai_output: &str,
    reference_pdfs: &[(&std::path::Path, &str, &[&str], &str)],
    my_patent_pdf: Option<(&std::path::Path, &str, &[&str], &str)>,
) -> FactCheckReport {
    use crate::ai::extraction::extract_pdf;

    let mut c0_extractions: Vec<C0ExtractionInfo> = Vec::new();
    let mut references_text = String::new();
    let mut my_patent_text = String::new();

    // 提取对比文献 PDF
    for (path, pub_number, terms, source_note) in reference_pdfs {
        match extract_pdf(path) {
            Ok(extracted) => {
                let counts = extracted.count_terms(terms);
                let info = C0ExtractionInfo {
                    source_id: pub_number.to_string(),
                    method: extracted.method.as_str().to_string(),
                    n_pages: extracted.n_pages,
                    n_chars: extracted.n_chars_normalized,
                    term_counts: counts,
                    source_note: source_note.to_string(),
                };
                c0_extractions.push(info);
                references_text.push_str(&format!(
                    "\n\n=== {} ({}) ===\n{}",
                    pub_number,
                    extracted.method.as_str(),
                    extracted.normalized()
                ));
            }
            Err(e) => {
                c0_extractions.push(C0ExtractionInfo {
                    source_id: pub_number.to_string(),
                    method: "ExtractionError".to_string(),
                    n_pages: 0,
                    n_chars: 0,
                    term_counts: Vec::new(),
                    source_note: format!("提取失败: {}", e),
                });
            }
        }
    }

    // 提取本专利 PDF
    if let Some((path, pub_number, terms, source_note)) = my_patent_pdf {
        match extract_pdf(path) {
            Ok(extracted) => {
                let counts = extracted.count_terms(terms);
                let info = C0ExtractionInfo {
                    source_id: pub_number.to_string(),
                    method: extracted.method.as_str().to_string(),
                    n_pages: extracted.n_pages,
                    n_chars: extracted.n_chars_normalized,
                    term_counts: counts,
                    source_note: source_note.to_string(),
                };
                c0_extractions.push(info);
                my_patent_text = extracted.normalized().to_string();
            }
            Err(e) => {
                c0_extractions.push(C0ExtractionInfo {
                    source_id: pub_number.to_string(),
                    method: "ExtractionError".to_string(),
                    n_pages: 0,
                    n_chars: 0,
                    term_counts: Vec::new(),
                    source_note: format!("提取失败: {}", e),
                });
            }
        }
    }

    // 执行 C1-C13 事实校验
    let mut report = check_oa_analysis(ai_output, &references_text, &my_patent_text);
    report.c0_extractions = c0_extractions;
    report
}

/// C2/C12 检测前剥离"非作者主张"文本，消除两类结构性误伤（M3-20261025 实测发现）：
///
/// 1. **尾部自报告段**：`check_oa_analysis` 的报告会追加到分析文本末尾，答复书门禁
///    （MC2）对同一文本复检时，报告内的违规示例文本（如「属于常规设置」）会再次
///    触发 C12——检查器被自己的报告自触发。
/// 2. **引号段**：驳斥式分析的正当结构是"先复述审查员/对比文件原文再驳斥"，引述
///    使用 「…」/“…”/"…"（ASCII）。审查员的核心主张往往正是"公知常识/常规选择"，
///    不剥离引号则任何认真复述审查观点的分析都无法通过答复书门禁（阈值 65）。
///    M3-20261028 复核加固：只剥离**明确属于引述**的引号段（前文 30 字内出现引述
///    标记，或引号段自身以引述主语开头）；无引述判据的引号段（我方强调/自认）保留
///    参与检查，杜绝借引号绕过 C2/C12。
///
/// 只影响 C2（零举证计数）与 C12（答复禁令）两个 needle 型检查；C1 引用真实性等
/// 需要原文对照的检查不受影响。
fn strip_non_authorial_for_c_checks(text: &str) -> String {
    // 1) 删除尾部自报告段：锚点多种写法取最后一个命中，且仅当命中位于文档末尾
    //    20% 区域才截断——生成器把核查报告拼在文末，中部出现同词属正文引用，
    //    截断会腰斩其后正常答复正文（漏检），故明确放弃。
    const SELF_REPORT_ANCHORS: [&str; 4] =
        ["AI 事实核查", "事实核查报告", "一致性核查", "AI核查报告"];
    let total_chars = text.chars().count();
    let mut last_hit: Option<usize> = None; // 字节偏移
    for anchor in SELF_REPORT_ANCHORS {
        let mut from = 0;
        while let Some(pos) = text.get(from..).and_then(|s| s.find(anchor)) {
            let abs = from + pos;
            last_hit = Some(match last_hit {
                Some(p) if p > abs => p,
                _ => abs,
            });
            from = abs + anchor.len();
        }
    }
    let body: &str = match last_hit {
        Some(idx) => {
            // idx 来自 find()，必落字符边界；get() 形态兜底：万一非边界返回 None 走不截断分支
            let chars_before = text
                .get(..idx)
                .map(|s| s.chars().count())
                .unwrap_or(total_chars);
            if total_chars > 0 && chars_before * 5 >= total_chars * 4 {
                tracing::debug!(
                    hit_chars = chars_before,
                    total_chars,
                    "自报告锚点命中于文档末尾 20% 区域，截断其后全文"
                );
                text.get(..idx).unwrap_or(text)
            } else {
                tracing::debug!(
                    hit_chars = chars_before,
                    total_chars,
                    "自报告锚点命中于文档中部，不截断（防腰斩正常正文）"
                );
                text
            }
        }
        None => {
            tracing::debug!("未命中自报告锚点，全文参与 C2/C12 检查");
            text
        }
    };

    // 2) 删除引号段：仅剥离「明确属于引述」的段——引号前文 30 字内出现引述标记
    //    （审查员/审查意见/通知书/对比文件/申请人主张），或引号段自身以引述主语
    //    （审查员/对比文件）开头；无引述判据的引号段（如我方用引号强调或自认）
    //    原样保留，继续参与 C2/C12。成对上限 300 字防远距/未闭合引号吞正文。
    const QUOTE_CONTEXT_MARKERS: [&str; 5] =
        ["审查员", "审查意见", "通知书", "对比文件", "申请人主张"];
    const QUOTE_SUBJECT_PREFIXES: [&str; 2] = ["审查员", "对比文件"];
    let mut out = String::with_capacity(body.len());
    let chars: Vec<char> = body.chars().collect();
    let openers = ['「', '“', '"'];
    let closers = ['」', '”', '"'];
    let mut i = 0;
    while i < chars.len() {
        if openers.contains(&chars[i]) {
            let close = closers[openers.iter().position(|&o| o == chars[i]).unwrap()];
            let mut j = i + 1;
            while j < chars.len() && chars[j] != close && j - i <= 300 {
                j += 1;
            }
            if j < chars.len() && chars[j] == close {
                let quoted: String = chars[i + 1..j].iter().collect();
                // ctx_start 来自 char_indices()，必落字符边界；get() 形态兜底防 panic
                let context = out
                    .char_indices()
                    .rev()
                    .nth(29)
                    .map(|(b, _)| b)
                    .and_then(|b| out.get(b..))
                    .unwrap_or(out.as_str());
                let context_marked = QUOTE_CONTEXT_MARKERS.iter().any(|m| context.contains(m));
                let subject_prefixed = QUOTE_SUBJECT_PREFIXES
                    .iter()
                    .any(|p| quoted.trim_start().starts_with(p));
                if context_marked || subject_prefixed {
                    // 明确引述 → 成对引号段整体剥离
                    i = j + 1;
                    continue;
                }
                // 无引述判据：引号与其内容保留，参与 C2/C12
                out.push(chars[i]);
                i += 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// C1-C13 纯函数检测层：调用各检测模块，将结果转为 FactWarning 列表
///
/// 此函数是 fact_check 与 C1-C13 纯函数模块之间的适配层。
/// 它不改变原有 ①-④ 检测逻辑，只追加 C 系列检测结果。
fn run_c_module_checks(ai_output: &str) -> Vec<FactWarning> {
    let mut warnings = Vec::new();

    // 引述/自报告剥离后再做 needle 型检查（见函数文档）
    let authorial = strip_non_authorial_for_c_checks(ai_output);

    // C2: 零举证检测
    let zero_ev = crate::ai::zero_evidence::check_zero_evidence(&authorial);
    if zero_ev.count > 0 {
        warnings.push(FactWarning {
            category: "C2零举证".to_string(),
            severity: "高".to_string(),
            description: zero_ev.summary,
        });
    }

    // C12: 答复禁令自查
    let prohibitions = crate::ai::prohibition_check::check_prohibitions(&authorial);
    for v in &prohibitions.violations {
        let severity = if v.severity == crate::ai::prohibition_check::ProhibitionSeverity::Fatal {
            "致命"
        } else {
            "中"
        };
        warnings.push(FactWarning {
            category: format!("C12禁令-{}", v.rule_id),
            severity: severity.to_string(),
            description: format!("{}：违规文本「{}」", v.rule_description, v.violating_text),
        });
    }

    warnings
}

/// 根据警告列表计算可信度评分
/// M2 审查 P1-7：空白判定覆盖零宽字符（U+200B/200C/200D/FEFF）与 Unicode 空白。
fn is_blank_incl_zero_width(s: &str) -> bool {
    s.chars().all(|c| {
        c.is_whitespace() || matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}')
    })
}

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
    // M2-B 双保险：零警告但零分 = 无内容场景，绝不能写成"未检测到问题"
    if report.warnings.is_empty() {
        if report.score <= 0.0 {
            return "无内容可核查（分析文本为空），评分 0。".to_string();
        }
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
            // 确保 UTF-8 边界安全：回退到最近的 char boundary
            let safe_end = text
                .char_indices()
                .take_while(|(i, _)| *i <= end)
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(0);
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
//  校验④：A33 合规性核验（UA3 v2，判据在 `evidence_kernel::amendment_gate`）
// ═══════════════════════════════════════════════════════════════════════
//
// 旧实现 `check_a33_risk`（17 个硬编码高风险术语 + 「修改为：「…」」正则 + 3–8 字
// 中文块包含匹配）已整体删除，不留 `#[allow(dead_code)]` 封存：它的判据不可证伪
// （措辞不在词表里就放过），且唯一调用点是它自己的单测——保留即给弱判据留一条
// 重新接线的路。生产入口只有 `check_a33_verbatim`（槽位④）。

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

    // ── UA3 v2: A33 特征句逐字核验（判据在 evidence_kernel::amendment_gate）──
    // 旧 17 词表判据的用例随实现一并删除；五类豁免与负控的逐条用例在
    // `src/evidence_kernel/amendment_gate.rs` 单测与 `tests/amendment_verbatim_gate.rs`。

    // ── 综合校验测试 ──

    // M3-20261025：C2/C12 对引述与自报告的剥离行为（strip_non_authorial_for_c_checks）
    // M3-20261028 复核加固：引号段剥离加语境判据（只剥「明确属于引述」）、自报告锚点
    // 多写法 + 末尾 20% 区域限定；各用例注释标明所锁语义。

    /// 构造长正文：使后续追加的自报告锚点落在文档末尾 20% 区域（trailing 前提）。
    fn long_clean_body() -> String {
        "权利要求1与对比文件1相比，区别特征在于采用电缸直行程驱动调节阀芯行程，".repeat(12)
    }

    #[test]
    fn c12_quoted_examiner_assertion_does_not_fire() {
        // 锁的语义（负向+正向）：①引号段前文 30 字内有引述标记（「审查员引用」）→
        // 该引号段按引述剥离，引号内的禁令短语不触发 P1；②剥离只作用于该引号段——
        // 同文本中剥离后仍存在的己方口吻让步必须触发 P1（防剥离逻辑错吞全文的假绿）。
        let quoted_only = "审查员引用的关键原文：「区别仅在于……属于常规设置」。申请人不同意该认定：对比文件1未公开电缸直行程驱动，参见说明书第[0002]段。";
        let with_own_voice = quoted_only.to_string() + "且该区别特征属于常规设置，无需创造性劳动。";
        assert!(
            !run_c_module_checks(quoted_only)
                .iter()
                .any(|w| w.category.contains("C12禁令-P1")),
            "引号内的审查员断言不应触发 P1"
        );
        assert!(
            run_c_module_checks(&with_own_voice)
                .iter()
                .any(|w| w.category.contains("C12禁令-P1")),
            "剥离后仍存在的己方口吻段必须触发 P1"
        );
    }

    #[test]
    fn c12_own_voice_assertion_still_fires() {
        // 锁的语义：己方口吻（无引号）的常规性让步必须继续触发 P1。
        let text = "该区别特征属于常规设置，无需创造性劳动。";
        let warnings = run_c_module_checks(text);
        assert!(
            warnings.iter().any(|w| w.category.contains("C12禁令-P1")),
            "己方让步表述应触发 P1，实际：{:?}",
            warnings
        );
    }

    #[test]
    fn c12_own_voice_in_quotes_without_marker_still_fires() {
        // 锁的语义（复核问题 1 核心，先红后绿）：我方用引号强调/自认（前文 30 字内
        // 无引述标记、引号段不以引述主语开头）不得被剥离——「该特征属于常规选择」
        // 必须继续触发 C12-P1（修复前成对引号一律无条件剥离 = 假绿）。
        let text = "综上所述，申请人认为「该特征属于常规选择」的认定有误，本申请不应被据此驳回。参见说明书第[0002]段。";
        let warnings = run_c_module_checks(text);
        assert!(
            warnings.iter().any(|w| w.category.contains("C12禁令-P1")),
            "无引述标记的己方引号自认必须触发 P1，实际：{:?}",
            warnings
        );
    }

    #[test]
    fn c12_quoted_examiner_subject_prefix_still_stripped() {
        // 锁的语义（防过修，修复前后均绿）：引号段自身以引述主语「审查员」开头 →
        // 即使前文无引述标记也按引述剥离，「审查员认为该特征属于常规选择」不触发 P1。
        let text = "对于创造性的认定「审查员认为该特征属于常规选择」的主张，本申请不予认同，理由如下。参见说明书第[0002]段。";
        let warnings = run_c_module_checks(text);
        assert!(
            !warnings.iter().any(|w| w.category.contains("C12禁令-P1")),
            "引号段以引述主语开头应剥离，不应触发 P1，实际：{:?}",
            warnings
        );
    }

    #[test]
    fn trailing_self_report_section_ignored() {
        // 锁的语义：文档末尾 20% 区域的自报告段（锚点写法一「AI 事实核查」）被截断，
        // 段内违规示例文本不再触发 C2/C12（门禁复检自触发回归）。
        let body = long_clean_body();
        let self_report = "\n\n## AI 事实核查（请人工复核）\n可信度评分：40/100。\n检测到违规文本「属于常规设置」、公知常识。";
        let warnings = run_c_module_checks(&(body + self_report));
        assert!(
            warnings.is_empty(),
            "自报告段不应触发 C2/C12，实际：{:?}",
            warnings
        );
    }

    #[test]
    fn trailing_self_report_marker_variant_still_ignored() {
        // 锁的语义（先红后绿）：自报告段换措辞（锚点写法二「事实核查报告」）也须识别——
        // 末尾 20% 区域内命中第二写法 → 截断尾部（修复前只认「AI 事实核查」= 漏截自触发）。
        let body = long_clean_body();
        let self_report =
            "\n\n## 事实核查报告\n可信度评分：40/100。\n检测到违规文本「属于常规设置」、公知常识。";
        let warnings = run_c_module_checks(&(body + self_report));
        assert!(
            warnings.is_empty(),
            "换措辞的自报告段仍应被截断，实际：{:?}",
            warnings
        );
    }

    #[test]
    fn mid_text_self_report_marker_does_not_truncate() {
        // 锁的语义（先红后绿）：自报告锚点出现在文档中部（非末尾 20% 区域）→ 不截断，
        // 其后的正常答复正文仍参与 C2/C12（修复前 find 首个命中腰斩其后全文 = 漏检）。
        let text = "答复正文不应出现「AI 事实核查」字样。该区别特征属于常规设置，无需创造性劳动。参见说明书第[0002]段。";
        let warnings = run_c_module_checks(text);
        assert!(
            warnings.iter().any(|w| w.category.contains("C12禁令-P1")),
            "中部锚点不得腰斩其后正文，其后违规必须触发 P1，实际：{:?}",
            warnings
        );
    }

    #[test]
    fn long_distance_quote_pair_not_stripped() {
        // 锁的语义（复核问题 3①）：opener 与 close 相距超过 300 字的成对引号与未闭合
        // 引号一样**整体不剥离**（0 吞噬，比"最多吞 300 字"更保守）——引号内与引号外
        // 的正文全部参与 C2/C12，远距引号不得吞掉其后正文（正向断言：保留段仍参与检查）。
        let mut text = String::from("审查员提出如下认定「");
        text.push_str(&"此处为审查意见的展开论述内容。".repeat(30));
        text.push_str(
            "属于常规设置」该区别特征属于常规设置，无需创造性劳动。参见说明书第[0002]段。",
        );
        let warnings = run_c_module_checks(&text);
        assert!(
            warnings.iter().any(|w| w.category.contains("C12禁令-P1")),
            "跨距>300 的成对引号不得吞掉其后正文，实际：{:?}",
            warnings
        );
    }

    #[test]
    fn unclosed_quote_does_not_swallow_body() {
        // 锁的语义：未闭合引号不得吞掉后续正文（防剥离过宽导致漏检）——
        // 扫描在 300 字上限内找不到 close 时 opener 按普通字符保留，正文全量参与检查。
        let text = "审查员称「如下：区别特征属于常规设置，且公知常识结合显而易见。后续正文本领域技术人员容易想到。";
        let warnings = run_c_module_checks(text);
        assert!(
            warnings.iter().any(|w| w.category.contains("C2零举证")),
            "未闭合引号后的正文仍应被检查，实际：{:?}",
            warnings
        );
    }

    #[test]
    fn full_check_combines_all() {
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
            c0_extractions: Vec::new(),
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

    // ── UA3: A33 结构化对照自检测试 ──

    #[test]
    fn ua3_a33_structured_detects_high_risk() {
        let ai_output = "建议修改：权项1修改为：「区块链存储装置」";
        let my_patent = "一种分布式存储装置，包括存储节点和控制器。";
        let result = check_a33_structured(ai_output, my_patent);
        assert_eq!(result.status, "risk");
        assert!(result.items.iter().any(|i| i.risk_level == "high"));
        assert!(result
            .items
            .iter()
            .any(|i| i.introduced_terms.contains(&"区块链存储装置".to_string())));
    }

    #[test]
    fn ua3_a33_structured_passes_when_no_modification() {
        let ai_output = "建议维持权利要求1不变。";
        let my_patent = "一种存储装置。";
        let result = check_a33_structured(ai_output, my_patent);
        assert_eq!(result.status, "pass");
        assert!(result.items.is_empty());
    }

    #[test]
    fn ua3_a33_structured_passes_when_term_in_spec() {
        let ai_output = "权项1修改为：「一种分布式存储装置及其控制方法」";
        let my_patent = "一种分布式存储装置，包括存储节点和控制器。控制方法包括步骤A。";
        let result = check_a33_structured(ai_output, my_patent);
        assert_eq!(result.status, "pass");
    }

    // ── UA6: 出处锚点纪律检测测试 ──

    #[test]
    fn ua6_missing_source_detects_unanchored_fact() {
        let ai_output = "对比文件D1公开了一种热交换装置。本申请的区别特征在于效率更高。";
        let report = check_missing_sources(ai_output);
        assert!(report
            .warnings
            .iter()
            .any(|w| w.category == "missing_source"));
    }

    #[test]
    fn ua6_missing_source_passes_anchored_fact() {
        let ai_output = "对比文件D1公开了一种热交换装置【依据：对比文件：D1第4段】。";
        let report = check_missing_sources(ai_output);
        assert!(!report
            .warnings
            .iter()
            .any(|w| w.category == "missing_source"));
    }

    #[test]
    fn ua6_missing_source_ignores_non_fact_sentences() {
        let ai_output = "建议修改权利要求1。可以考虑增加从属权利要求。";
        let report = check_missing_sources(ai_output);
        assert!(report.warnings.is_empty());
    }

    use std::path::PathBuf;

    fn project_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    // C0→C1-C13 集成测试：用真实 PDF 验证 check_oa_analysis_from_pdfs
    // 真实源: Google Patents + CNIPA, 下载日 2026-10-05
    // 驳回件: data/uploads/d99e9a01-b6f3-4408-8ad4-5ef64dd80ff9.pdf (CNIPA, TextLayer, 911204B)
    // D1: data/uploads/d38ccf3f-2227-43f4-bdc1-7ae12b582c71.pdf (Google Patents, OcrFallback, 833087B)
    // D3: data/uploads/bb2e12a0-47ac-4595-97ec-ab0c830ce3d6.pdf (Google Patents, TextLayer, 625673B)

    #[test]
    #[ignore]
    fn test_check_oa_from_pdfs_extraction_metadata() {
        let d1_path = project_root().join("data/uploads/d38ccf3f-2227-43f4-bdc1-7ae12b582c71.pdf");
        let d3_path = project_root().join("data/uploads/bb2e12a0-47ac-4595-97ec-ab0c830ce3d6.pdf");
        assert!(d1_path.exists(), "D1 PDF missing");
        assert!(d3_path.exists(), "D3 PDF missing");

        let d1_terms: &[&str] = &["交错", "内侧面", "长方形", "对角"];
        let d3_terms: &[&str] = &["固定螺栓", "螺纹", "通孔"];

        let refs: &[(&std::path::Path, &str, &[&str], &str)] = &[
            (
                &d1_path,
                "CN990001001A",
                d1_terms,
                "合成对比文件D1, 合成来源, 合成日期, sha256前缀 9f8e7d6c",
            ),
            (
                &d3_path,
                "CN990001003U",
                d3_terms,
                "合成对比文件D3, 合成来源, 合成日期, sha256前缀 0a0b0c0d",
            ),
        ];

        let ai_output = "D1公开了交错设置的电板，D3公开了固定螺栓和螺纹结构。";
        let report = check_oa_analysis_from_pdfs(ai_output, refs, None);

        // 验证 C0 提取元信息
        assert_eq!(report.c0_extractions.len(), 2, "应有2个PDF的提取信息");

        // D1: OcrFallback
        let d1_info = &report.c0_extractions[0];
        assert_eq!(d1_info.source_id, "CN990001001A");
        assert_eq!(d1_info.method, "OCR_FALLBACK", "D1应为OcrFallback");
        assert_eq!(d1_info.n_pages, 9, "D1应9页");
        println!("=== D1 C0 提取信息 ===");
        println!("  method = {}", d1_info.method);
        println!("  n_pages = {}", d1_info.n_pages);
        println!("  n_chars = {}", d1_info.n_chars);
        for (t, c) in &d1_info.term_counts {
            println!("  {} = {}", t, c);
        }

        // D3: TextLayer
        let d3_info = &report.c0_extractions[1];
        assert_eq!(d3_info.source_id, "CN990001003U");
        assert_eq!(d3_info.method, "TEXT_LAYER", "D3应为TextLayer");
        assert_eq!(d3_info.n_pages, 8, "D3应8页");
        println!("=== D3 C0 提取信息 ===");
        println!("  method = {}", d3_info.method);
        println!("  n_pages = {}", d3_info.n_pages);
        println!("  n_chars = {}", d3_info.n_chars);
        for (t, c) in &d3_info.term_counts {
            println!("  {} = {}", t, c);
        }

        // 验证实测计数与 C0 一致
        let get_count = |info: &C0ExtractionInfo, term: &str| -> usize {
            info.term_counts
                .iter()
                .find(|(k, _)| k == term)
                .map(|(_, v)| *v)
                .unwrap_or(0)
        };
        // D1 OCR 实测 (tesseract chi_sim, 2026-10-05)
        assert_eq!(get_count(d1_info, "交错"), 5, "D1交错=5(OCR)");
        assert_eq!(get_count(d1_info, "对角"), 0, "D1对角=0");
        // D3 TextLayer 实测 (normalize_text, 2026-10-05)
        assert_eq!(get_count(d3_info, "固定螺栓"), 13, "D3固定螺栓=13");
        assert_eq!(get_count(d3_info, "螺纹"), 29, "D3螺纹=29(normalize后)");
        assert_eq!(get_count(d3_info, "通孔"), 14, "D3通孔=14(normalize后)");
    }

    #[test]
    #[ignore]
    fn test_check_oa_from_pdfs_with_rejection_pdf() {
        let rejection_path =
            project_root().join("data/uploads/d99e9a01-b6f3-4408-8ad4-5ef64dd80ff9.pdf");
        assert!(rejection_path.exists(), "驳回件PDF missing");

        let rejection_terms: &[&str] = &["容易想到", "惯用手段", "对角", "对角安装"];
        let refs: &[(&std::path::Path, &str, &[&str], &str)] = &[(
            &rejection_path,
            "CN900000001A-OA",
            rejection_terms,
            "CNIPA驳回决定, 文本层, 下载日 2026-10-05, 911204B, sha256前缀 e1ed4375",
        )];

        let ai_output = "审查员认为本申请容易想到，属于惯用手段。";
        let report = check_oa_analysis_from_pdfs(ai_output, refs, None);

        assert_eq!(report.c0_extractions.len(), 1);
        let oa_info = &report.c0_extractions[0];
        assert_eq!(oa_info.method, "TEXT_LAYER", "驳回件应为TextLayer");
        assert_eq!(oa_info.n_pages, 4, "驳回件应4页");

        let get_count = |info: &C0ExtractionInfo, term: &str| -> usize {
            info.term_counts
                .iter()
                .find(|(k, _)| k == term)
                .map(|(_, v)| *v)
                .unwrap_or(0)
        };
        // 驳回件 TextLayer 实测 (2026-10-05)
        assert_eq!(get_count(oa_info, "容易想到"), 5, "驳回件容易想到=5");
        assert!(get_count(oa_info, "对角") >= 1, "驳回件对角>=1");

        println!("=== 驳回件 C0 提取信息 ===");
        println!("  method = {}", oa_info.method);
        println!("  n_pages = {}", oa_info.n_pages);
        for (t, c) in &oa_info.term_counts {
            println!("  {} = {}", t, c);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  UA3: A33 结构化对照自检
// ═══════════════════════════════════════════════════════════════════════

/// A33 单项检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A33CheckItem {
    pub claim_num: String,
    pub modified_text: String,
    /// 零命中特征句原文（字段名沿用旧 UA3 便于快照/前端兼容，语义＝回原文零命中的措辞）
    pub introduced_terms: Vec<String>,
    /// 命中出处（`本申请:para:N` / `本申请:claim:N`）或「零命中」
    pub original_spec_coverage: String,
    pub risk_level: String, // "high"（零命中）| "none"
    pub detail: String,
}

/// A33 结构化检查结果（UA3 v2：修改稿特征句逐字核验门）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A33CheckResult {
    /// "pass" | "risk"（warning 档在新判据下不存在：要么逐字命中，要么零命中）
    pub status: String,
    pub items: Vec<A33CheckItem>,
    /// 零命中特征句（含权项号与 critic 判据理由）
    pub unsupported: Vec<crate::evidence_kernel::amendment_gate::UnsupportedFeature>,
    /// 零命中特征句数＝ meta 事件的 amendment_unsupported
    pub unsupported_count: usize,
    /// 被替换为降级标注的「明确记载 / 来源于原说明书」断言条数
    pub assertions_downgraded: usize,
    /// 就地标注后的文本（答复书网关据此下发；分析流仅消费计数）
    pub gated_text: String,
    /// 提交度维度：零命中为 0（压死规则）
    pub submission_ready: bool,
    /// 门状态：no_amendment / skipped_no_original / verified / degraded
    pub gate_status: String,
}

impl Default for A33CheckResult {
    fn default() -> Self {
        Self {
            status: "pass".to_string(),
            items: Vec::new(),
            unsupported: Vec::new(),
            unsupported_count: 0,
            assertions_downgraded: 0,
            gated_text: String::new(),
            submission_ready: true,
            gate_status: "no_amendment".to_string(),
        }
    }
}

/// UA3 v2: 修改后权利要求 vs 原始申请文本（说明书 + 权利要求书）特征句逐字核验。
///
/// 判据、归一化豁免（五类合法撰写变换）、检索范围（本申请权利要求 + 具体实施方式
/// 段落三元组）与支撑判定（严格 `Relation::Support`，AnchorOnly 不算支撑）全部在
/// `evidence_kernel::amendment_gate` 内，本函数只做适配——**不在这里另写包含匹配**。
pub fn check_a33_structured(ai_output: &str, my_patent: &str) -> A33CheckResult {
    use crate::evidence_kernel::amendment_gate;

    let gate = amendment_gate::gate_amendment_text(ai_output, my_patent);

    // 按权项聚合零命中特征句（一个权项一条 item）
    let mut by_claim: Vec<(String, Vec<String>)> = Vec::new();
    for u in &gate.unsupported {
        if let Some(slot) = by_claim.iter_mut().find(|(c, _)| *c == u.claim_no) {
            slot.1.push(u.text.clone());
        } else {
            by_claim.push((u.claim_no.clone(), vec![u.text.clone()]));
        }
    }

    let items: Vec<A33CheckItem> = by_claim
        .into_iter()
        .map(|(claim_no, terms)| A33CheckItem {
            detail: format!(
                "修改后权利要求{} 有 {} 处特征句在原始申请文本（说明书 + 权利要求书）零命中，已就地降级标注【未证实】，违反专利法第33条风险",
                claim_no,
                terms.len()
            ),
            original_spec_coverage: "零命中（本申请权利要求 + 具体实施方式段落均无逐字/滑窗命中）"
                .to_string(),
            modified_text: terms.join("，"),
            introduced_terms: terms,
            claim_num: claim_no,
            risk_level: "high".to_string(),
        })
        .collect();

    let status = if gate.unsupported_count > 0 {
        "risk"
    } else {
        "pass"
    };
    A33CheckResult {
        status: status.to_string(),
        items,
        unsupported: gate.unsupported,
        unsupported_count: gate.unsupported_count,
        assertions_downgraded: gate.assertions_downgraded,
        gated_text: gate.gated_text,
        submission_ready: gate.submission_ready,
        gate_status: gate.status.as_str().to_string(),
    }
}

/// ④ A33 合规性核验的生产入口：把 UA3 v2 的门结果转成事实核查警告。
///
/// 零命中＝致命档（100 - 35 = 65，答复书网关阈值 65 因此拒绝放行未经核验的修改稿）；
/// 断言降级单独记中档，保留可见性但不重复计分。
pub fn check_a33_verbatim(ai_output: &str, my_patent: &str) -> FactCheckReport {
    let mut report = FactCheckReport::default();
    let result = check_a33_structured(ai_output, my_patent);

    if result.unsupported_count > 0 {
        let terms: Vec<String> = result
            .unsupported
            .iter()
            .map(|u| format!("「{}」（权项{}）", u.text, u.claim_no))
            .collect();
        report.warnings.push(FactWarning {
            category: "A33逐字核验".to_string(),
            severity: "致命".to_string(),
            description: format!(
                "修改后权利要求有 {} 处特征句在原始申请文本（说明书 + 权利要求书）零命中：{}。已就地标注【未证实：该措辞在原始文本零命中】，提交度维度置零——提交前必须逐处核对专利法第33条。",
                result.unsupported_count,
                terms.join("；")
            ),
        });
    }

    if result.assertions_downgraded > 0 {
        report.warnings.push(FactWarning {
            category: "A33断言降级".to_string(),
            severity: "中".to_string(),
            description: format!(
                "有 {} 处「明确记载 / 来源于原说明书」类断言在零命中情况下被替换为降级标注。",
                result.assertions_downgraded
            ),
        });
    }

    report.score = compute_score(&report.warnings);
    report
}

// ═══════════════════════════════════════════════════════════════════════
//  UA6: 出处锚点纪律检测
// ═══════════════════════════════════════════════════════════════════════

/// 事实动词列表——含这些动词的句子是事实断言，须带出处锚点
#[allow(dead_code)]
const FACT_VERBS: &[&str] = &[
    "公开了",
    "记载了",
    "显示了",
    "指出",
    "披露了",
    "揭示了",
    "描述了",
    "说明了",
    "提到了",
];

/// UA6: 检测输出中无出处锚点的事实断言。
///
/// 扫描含事实动词且未跟 `【依据：…】` 的句子，标记为 `missing_source` 警告。
/// 返回的 FactCheckReport 中 warnings 的 category 为 "missing_source"。
#[allow(dead_code)]
pub fn check_missing_sources(ai_output: &str) -> FactCheckReport {
    let mut report = FactCheckReport::default();

    // 按句子分割（中文句号、问号、叹号、换行）
    let sentences: Vec<&str> = ai_output
        .split(['。', '？', '！', '\n'])
        .filter(|s| !s.trim().is_empty())
        .collect();

    for sentence in &sentences {
        let has_fact_verb = FACT_VERBS.iter().any(|v| sentence.contains(v));
        if !has_fact_verb {
            continue;
        }

        // 检查句子中或紧随其后是否有出处锚点
        let has_anchor = sentence.contains("【依据：") || sentence.contains("【依据:");

        if !has_anchor {
            // 提取事实动词用于描述
            let verb = FACT_VERBS
                .iter()
                .find(|v| sentence.contains(*v))
                .unwrap_or(&"断言");

            // 截取句子前 60 字用于描述
            let preview: String = sentence.trim().chars().take(60).collect();

            report.warnings.push(FactWarning {
                category: "missing_source".to_string(),
                description: format!(
                    "事实断言（含「{verb}」）未带出处锚点：「{preview}…」——须补充【依据：出处类型：定位信息】或降级为【需申请人确认】",
                ),
                severity: "中".to_string(),
            });
        }
    }

    report.score = compute_score(&report.warnings);
    report
}
