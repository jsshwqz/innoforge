//! CaseInput：通用 OA→答复书引擎的输入数据结构。
//!
//! 取代 patent.rs 旧单案预置文本。
//! 任意专利的驳回件 + 对比文件 + 权利要求 + 说明书效果句 → 引擎自动写结构正确答复。

use serde::{Deserialize, Serialize};

/// 引擎顶层输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseInput {
    /// 案件元数据（案号、人名、日期等）
    pub case_meta: CaseMeta,
    /// 驳回决定 / 审查意见通知书全文
    pub oa_full_text: String,
    /// 对比文件数组（D1/D2/D3...，每项含公开号+原文）
    pub reference_documents: Vec<ReferenceDocument>,
    /// 待审权利要求全文
    pub claims_under_review: String,
    /// 说明书技术效果句（手填优先；缺则管线从说明书自动逐句抽取，不得改写）
    #[serde(default)]
    pub spec_effect_statements: Vec<String>,
    /// 说明书全文（可选；用于自动提取效果句 + verbatim校验）
    #[serde(default)]
    pub spec_full_text: Option<String>,
    /// 旧 analysis 字段（向后兼容，当额外hint拼接，缺失不报错）
    #[serde(default)]
    pub legacy_analysis: Option<String>,
    /// 旧 discussion 字段（向后兼容，当额外hint拼接，缺失不报错）
    #[serde(default)]
    pub legacy_discussion: Option<String>,
    /// 自定义停用词（追加到内置通用停用词表，过滤 Stage B 的术语计数）
    #[serde(default)]
    pub custom_stopwords: Vec<String>,
}

/// 案件元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseMeta {
    /// 申请号，如 "9999001.1"（主体加点加一位校验位）
    pub application_number: String,
    /// 申请人姓名
    pub applicant: String,
    /// 发明名称
    pub invention_title: String,
    /// 驳回决定发文日，如 "2026-07-10"
    #[serde(default)]
    pub oa_date: Option<String>,
    /// OA 类型，复用现有枚举：reject_review / first_exam / second_rejection / ...
    pub oa_type: String,
    /// 审查基准文本提交日（可选）
    #[serde(default)]
    pub claim_basis_date: Option<String>,
    /// 审查基准权利要求文本（可选）
    #[serde(default)]
    pub claim_basis_text: Option<String>,
}

/// 对比文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceDocument {
    /// 标签：D1 / D2 / D3 ...
    pub label: String,
    /// 公开号，如 "CN900000001A"
    pub publication_number: String,
    /// 原文全文
    pub full_text: String,
    /// 提取方法：ocr / text_layer（可选，用于可靠性标记）
    #[serde(default)]
    pub extraction_method: Option<String>,
    /// 页数（可选）
    #[serde(default)]
    pub page_count: Option<usize>,
    /// 技术领域（可选，用于载体不兼容检测）
    #[serde(default)]
    pub technical_field: Option<String>,
}

// ── Stage 输出类型 ─────────────────────────────────────────────────────

/// Stage A 输出：区别特征。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistinguishingFeature {
    /// 序号（1-based）
    pub ordinal: usize,
    /// 特征名称（必须是 claims_under_review 或 spec_full_text 的 verbatim 子串）
    pub feature_name: String,
    /// 审查员给的泛化标签
    pub oa_tag: OaTag,
    /// 审查员归因的对比文件标签（D1/D2/D3...），None 表示未归因
    pub attributed_ref: Option<String>,
    /// OA 中该特征的转述原文片段
    pub oa_excerpt: String,
}

/// 泛化标签枚举（复用 zero_evidence.rs 的标签集）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OaTag {
    EasyToThink,    // 容易想到
    RoutineDesign,  // 常规设计
    RoutineChoice,  // 常规选择
    RoutineAdjust,  // 常规调整
    RoutineSetup,   // 常规设置
    CustomaryMeans, // 惯用手段
    Other(String),  // 其他
}

impl OaTag {
    /// 中文显示名。
    pub fn as_chinese(&self) -> String {
        match self {
            OaTag::EasyToThink => "容易想到".into(),
            OaTag::RoutineDesign => "常规设计".into(),
            OaTag::RoutineChoice => "常规选择".into(),
            OaTag::RoutineAdjust => "常规调整".into(),
            OaTag::RoutineSetup => "常规设置".into(),
            OaTag::CustomaryMeans => "惯用手段".into(),
            OaTag::Other(s) => s.clone(),
        }
    }

    /// 从中文标签解析。
    pub fn from_chinese(s: &str) -> OaTag {
        match s {
            "容易想到" => OaTag::EasyToThink,
            "常规设计" => OaTag::RoutineDesign,
            "常规选择" => OaTag::RoutineChoice,
            "常规调整" => OaTag::RoutineAdjust,
            "常规设置" => OaTag::RoutineSetup,
            "惯用手段" => OaTag::CustomaryMeans,
            other => OaTag::Other(other.into()),
        }
    }
}

/// Stage B 输出：术语计数检查。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermCountCheck {
    /// 被检查的术语
    pub term: String,
    /// OA 中的出现次数
    pub oa_count: usize,
    /// 对比文件标签
    pub ref_label: String,
    /// 对比文件公开号
    pub ref_pub_num: String,
    /// 对比文件原文中的出现次数
    pub ref_count: usize,
}

/// Stage B 输出：定性无支撑反驳段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsupportedClaim {
    /// 反驳段正文
    pub rebuttal_text: String,
    /// 关联的计数检查
    pub counts: Vec<TermCountCheck>,
}

/// Stage C 输出：逐特征三段式反驳。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureRebuttal {
    /// 序号（对应 DistinguishingFeature.ordinal）
    pub ordinal: usize,
    /// 特征名称
    pub feature_name: String,
    /// OA 标签
    pub oa_tag: OaTag,
    /// 割裂论断段
    pub severance: String,
    /// 整体协同段
    pub synergy: String,
    /// 举证不足段
    pub insufficient: String,
}

/// Stage D 输出：效果不匹配。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectMismatch {
    /// OA 中声称的效果原文
    pub oa_claim: String,
    /// 归因对比文件标签
    pub ref_label: String,
    /// 本申请实际效果（来自 spec_effect_statements）
    pub actual_effect: String,
    /// 反驳段正文
    pub rebuttal_text: String,
}

/// Stage E 输出：完整答复书。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseLetter {
    /// 完整正文
    pub full_text: String,
    /// Guard 校验结果
    pub guard_result: GuardResult,
}

/// Guard 校验结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardResult {
    /// 是否通过
    pub passed: bool,
    /// 不通过时的错误列表
    pub errors: Vec<String>,
}

/// 特征依赖图（共现建边，确定性）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraph {
    /// 邻接表：feature_name → 依赖的 feature_name 列表
    pub edges: Vec<(String, Vec<String>)>,
}

/// 用请求 / 分析 / 材料中的显式字段补齐空白 CaseMeta。
/// 只填空，不覆盖已有值；识别不到就保持为空，不猜造。
pub fn fill_case_meta(
    meta: &mut CaseMeta,
    request_json: Option<&serde_json::Value>,
    analysis_text: &str,
    oa_text: &str,
    my_patent_text: &str,
) {
    if let Some(rj) = request_json {
        if let Some(cm) = rj.get("case_meta").and_then(|v| v.as_object()) {
            fill_meta_from_object(meta, cm);
        }
        if let Some(mp) = rj.get("my_patent") {
            if let Some(obj) = mp.as_object() {
                fill_meta_from_object(meta, obj);
            } else if let Some(s) = mp.as_str() {
                fill_meta_from_labels(meta, s);
            }
        }
        if meta.application_number.is_empty() {
            if let Some(id) = rj.get("my_patent_id").and_then(|v| v.as_str()) {
                if !id.trim().is_empty() {
                    meta.application_number = id.trim().to_string();
                }
            }
        }
    }
    fill_meta_from_labels(meta, analysis_text);
    fill_meta_from_labels(meta, oa_text);
    fill_meta_from_labels(meta, my_patent_text);
    if meta.application_number.is_empty() {
        if let Some(n) = extract_patent_number(my_patent_text) {
            meta.application_number = n;
        }
    }
}

fn fill_meta_from_object(meta: &mut CaseMeta, o: &serde_json::Map<String, serde_json::Value>) {
    let get = |k: &str| o.get(k).and_then(|v| v.as_str()).unwrap_or("").trim();
    if meta.application_number.is_empty() {
        for k in ["application_number", "patent_number", "app_no"] {
            let v = get(k);
            if !v.is_empty() {
                meta.application_number = v.to_string();
                break;
            }
        }
    }
    if meta.invention_title.is_empty() {
        for k in ["invention_title", "title"] {
            let v = get(k);
            if !v.is_empty() && !is_generic_title(v) && !is_placeholder_value(v) {
                meta.invention_title = v.to_string();
                break;
            }
        }
    }
    if meta.applicant.is_empty() {
        let v = get("applicant");
        if !v.is_empty() && !is_placeholder_value(v) && !is_section_like(v) {
            meta.applicant = v.to_string();
        }
    }
}

fn fill_meta_from_labels(meta: &mut CaseMeta, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let head: String = text.chars().take(6000).collect();
    if meta.application_number.is_empty() {
        if let Some(v) = label_value(&head, &["申请号", "申请案号"]) {
            meta.application_number = v;
        }
    }
    if meta.invention_title.is_empty() {
        if let Some(v) = label_value(&head, &["发明名称", "发明创造名称"]) {
            meta.invention_title = v;
        }
    }
    if meta.applicant.is_empty() {
        if let Some(v) = label_value(&head, &["申请人", "专利权人"]) {
            if !is_section_like(&v) {
                meta.applicant = v;
            }
        }
    }
}

fn label_value(text: &str, labels: &[&str]) -> Option<String> {
    for line in text.lines().take(120) {
        let head = line.trim_start_matches(|c: char| {
            c == '|' || c == '*' || c == '#' || c == '-' || c.is_whitespace()
        });
        for label in labels {
            if !head.starts_with(label) {
                continue;
            }
            let rest = &head[label.len()..];
            let first = rest.chars().next();
            if !matches!(
                first,
                Some(':') | Some('：') | Some('*') | Some('|') | Some(' ') | Some('\t')
            ) {
                continue;
            }
            let rest = rest.trim_start_matches(|c: char| {
                c == ':' || c == '：' || c == '*' || c == '|' || c == '｜' || c.is_whitespace()
            });
            let value = rest
                .split(['|', '｜', '\n'])
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches(|c: char| c == '*' || c == ' ');
            if !value.is_empty() && !is_placeholder_value(value) && value.chars().count() <= 80 {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn is_generic_title(title: &str) -> bool {
    matches!(
        title.trim(),
        "本案" | "本专利" | "我的专利" | "专利" | "未命名"
    )
}

fn is_section_like(value: &str) -> bool {
    [
        "要点",
        "概述",
        "分析",
        "论证",
        "策略",
        "意见陈述",
        "结论",
        "请求",
        "目录",
        "摘要",
        "正文",
        "说明",
        "核对",
        "回应",
    ]
    .iter()
    .any(|k| value.contains(k))
}

fn is_placeholder_value(value: &str) -> bool {
    let v = value.trim();
    matches!(
        v,
        "核对原文"
            | "待核对"
            | "待补充"
            | "未提供"
            | "不详"
            | "未知"
            | "无"
            | "-"
            | "—"
            | "N/A"
            | "n/a"
    ) || v.starts_with("待")
        || v.starts_with("核对")
}

fn extract_patent_number(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?:CN|US|EP|JP|KR|WO)\s*\d{7,12}(?:\.\d)?[A-Z]?").ok()?;
    re.find(text).map(|m| m.as_str().replace(' ', ""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_input_deserializes() {
        let json = r#"{
            "case_meta": {
                "application_number": "9999001.1",
                "applicant": "示例申请人",
                "invention_title": "一种合成技术装置的装置",
                "oa_type": "reject_review"
            },
            "oa_full_text": "驳回决定正文",
            "reference_documents": [
                {"label": "D1", "publication_number": "CN990001001A", "full_text": "D1原文"}
            ],
            "claims_under_review": "1. 一种合成技术装置的装置",
            "spec_effect_statements": ["提高火焰燃烧速度"]
        }"#;
        let input: CaseInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.case_meta.application_number, "9999001.1");
        assert_eq!(input.reference_documents.len(), 1);
        assert_eq!(input.spec_effect_statements.len(), 1);
    }

    #[test]
    fn oa_tag_roundtrip() {
        for tag in [
            OaTag::EasyToThink,
            OaTag::RoutineDesign,
            OaTag::RoutineChoice,
            OaTag::RoutineAdjust,
            OaTag::RoutineSetup,
            OaTag::CustomaryMeans,
        ] {
            let cn = tag.as_chinese();
            assert_eq!(OaTag::from_chinese(&cn), tag);
        }
    }

    #[test]
    fn case_input_with_legacy_fields() {
        let json = r#"{
            "case_meta": {
                "application_number": "2025108888888",
                "applicant": "张三",
                "invention_title": "一种散热装置",
                "oa_type": "reject_review"
            },
            "oa_full_text": "驳回决定",
            "reference_documents": [],
            "claims_under_review": "1. 一种散热装置",
            "legacy_analysis": "旧分析结果",
            "legacy_discussion": "[]"
        }"#;
        let input: CaseInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.legacy_analysis.as_deref(), Some("旧分析结果"));
        assert!(input.spec_effect_statements.is_empty());
    }

    fn empty_meta() -> CaseMeta {
        CaseMeta {
            application_number: String::new(),
            applicant: String::new(),
            invention_title: String::new(),
            oa_date: None,
            oa_type: "reject_review".into(),
            claim_basis_date: None,
            claim_basis_text: None,
        }
    }

    #[test]
    fn fill_case_meta_uses_request_then_labels_without_fabricating() {
        let mut meta = empty_meta();
        let req = serde_json::json!({
            "my_patent": {"title": "本案", "content": ""},
            "my_patent_id": "CN990001001A"
        });
        fill_case_meta(
            &mut meta,
            Some(&req),
            "**申请号**:9999999999999\n**发明名称**:一种示例装置\n**申请人**:示例申请人",
            "",
            "",
        );
        assert_eq!(meta.application_number, "CN990001001A");
        assert_eq!(meta.invention_title, "一种示例装置");
        assert_eq!(meta.applicant, "示例申请人");
    }

    #[test]
    fn fill_case_meta_leaves_unknown_fields_empty() {
        let mut meta = empty_meta();
        fill_case_meta(&mut meta, None, "没有案号字段", "", "");
        assert!(meta.application_number.is_empty());
        assert!(meta.invention_title.is_empty());
        assert!(meta.applicant.is_empty());
    }

    #[test]
    fn fill_case_meta_does_not_override_existing_values() {
        let mut meta = empty_meta();
        meta.application_number = "EXISTING".into();
        meta.applicant = "原申请人".into();
        meta.invention_title = "原名称".into();
        let req = serde_json::json!({
            "case_meta": {
                "application_number": "NEW",
                "invention_title": "新名称",
                "applicant": "新申请人"
            }
        });
        fill_case_meta(
            &mut meta,
            Some(&req),
            "申请号：X\n发明名称：Y\n申请人：Z",
            "",
            "",
        );
        assert_eq!(meta.application_number, "EXISTING");
        assert_eq!(meta.invention_title, "原名称");
        assert_eq!(meta.applicant, "原申请人");
    }

    #[test]
    fn fill_case_meta_reads_markdown_table_row() {
        let mut meta = empty_meta();
        fill_case_meta(
            &mut meta,
            None,
            "| **申请号** | 9999999999999 |\n| **发明名称** | 一种示例装置 |",
            "",
            "",
        );
        assert_eq!(meta.application_number, "9999999999999");
        assert_eq!(meta.invention_title, "一种示例装置");
    }

    #[test]
    fn fill_case_meta_skips_placeholder_values() {
        let mut meta = empty_meta();
        fill_case_meta(
            &mut meta,
            None,
            "**申请号**:999999999999.9\n**发明名称**:一种示例装置\n**申请人**:核对原文",
            "",
            "",
        );
        assert_eq!(meta.application_number, "999999999999.9");
        assert_eq!(meta.invention_title, "一种示例装置");
        assert!(
            meta.applicant.is_empty(),
            "placeholder applicant must be skipped"
        );
    }

    #[test]
    fn fill_case_meta_rejects_section_like_applicant() {
        let mut meta = empty_meta();
        fill_case_meta(
            &mut meta,
            None,
            "**申请人**:意见陈述的回应要点\n申请人认为该特征应当整体考虑。",
            "",
            "",
        );
        assert!(
            meta.applicant.is_empty(),
            "section-like applicant must be skipped"
        );
    }
}
