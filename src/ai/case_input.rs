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
}
