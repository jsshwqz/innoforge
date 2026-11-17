//! 反驳策略仲裁模块（需求5）
//!
//! 确定攻击线优先级和角色长度约束。所有数据来自 CaseInput，不硬编码任何案号/人名/特征词。
//!
//! ## 攻击线优先级（从高到低）
//! 1. 定性失真（上位概括差距）：审查员用上位概念概括区别特征，遗漏技术实质
//! 2. 零举证标签：审查员仅贴泛化标签未给出具体技术启示证据
//! 3. 机理/效果差异：本申请技术机理/效果与对比文件实质不同
//! 4. 载体不兼容：对比文件技术领域/载体与本申请不兼容
//! 5. 割裂式判断：审查员将特征从整体方案中割裂单独评价
//!
//! ## 角色长度约束
//! - 主攻段（attack）：无长度限制，充分展开
//! - 纯防守段（defense）：≤3句，简洁回应

#[path = "part_topology.rs"]
pub mod part_topology;

use super::case_input::{
    CaseInput, DistinguishingFeature, EffectMismatch, FeatureRebuttal, ReferenceDocument,
};

/// 攻击线类型，按优先级从高到低排列
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AttackLine {
    /// 定性失真（上位概括差距）
    QualitativeDistortion,
    /// 零举证标签
    ZeroEvidence,
    /// 机理/效果差异
    MechanismEffectDiff,
    /// 载体不兼容
    CarrierIncompatible,
    /// 割裂式判断
    SeveranceJudgment,
}

impl AttackLine {
    pub fn chinese_name(self) -> &'static str {
        match self {
            AttackLine::QualitativeDistortion => "定性失真（上位概括差距）",
            AttackLine::ZeroEvidence => "零举证标签",
            AttackLine::MechanismEffectDiff => "机理/效果差异",
            AttackLine::CarrierIncompatible => "载体不兼容",
            AttackLine::SeveranceJudgment => "割裂式判断",
        }
    }

    pub fn priority_rank(self) -> usize {
        match self {
            AttackLine::QualitativeDistortion => 1,
            AttackLine::ZeroEvidence => 2,
            AttackLine::MechanismEffectDiff => 3,
            AttackLine::CarrierIncompatible => 4,
            AttackLine::SeveranceJudgment => 5,
        }
    }
}

/// 角色类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// 主攻段——无长度限制
    Attack,
    /// 纯防守段——≤3句
    Defense,
}

impl Role {
    /// 返回该角色的最大句数限制，None 表示无限制
    pub fn max_sentences(self) -> Option<usize> {
        match self {
            Role::Attack => None,
            Role::Defense => Some(3),
        }
    }
}

/// 单条反驳策略项
#[derive(Debug, Clone)]
pub struct StrategyItem {
    /// 攻击线类型
    pub line: AttackLine,
    /// 角色
    pub role: Role,
    /// 对应的特征序号（如有）
    pub feature_ordinal: Option<usize>,
    /// 策略文本
    pub text: String,
}

/// 仲裁结果：按优先级排序的策略项列表
#[derive(Debug, Clone)]
pub struct ArbitrationResult {
    pub items: Vec<StrategyItem>,
}

/// 检测定性失真（上位概括差距）
///
/// 判断依据：OA 中的 oa_tag（泛化标签）与 feature_name（具体技术特征）之间存在
/// 上位概括关系——审查员用"容易想到"/"常规设计"等泛化标签概括了具体技术特征，
/// 未分析该特征的技术实质。
fn detect_qualitative_distortion(features: &[DistinguishingFeature]) -> Vec<StrategyItem> {
    features
        .iter()
        .map(|f| StrategyItem {
            line: AttackLine::QualitativeDistortion,
            role: Role::Attack,
            feature_ordinal: Some(f.ordinal),
            text: format!(
                "审查员以“{}”这一上位泛化标签概括区别特征“{}”，\
             未分析该特征的技术实质和在该技术方案中的具体作用。\
             上位概括掩盖了该特征相对于对比文件的实质区别，\
             不满足创造性判断中“区别特征技术实质”的分析要求。",
                f.oa_tag.as_chinese(),
                f.feature_name
            ),
        })
        .collect()
}

/// 检测零举证标签
///
/// 判断依据：OA 仅给出泛化标签（容易想到/常规设计等），未引用具体对比文件段落
/// 或技术启示证据。FeatureRebuttal 的 insufficient 段即对应此攻击线。
fn detect_zero_evidence(
    _features: &[DistinguishingFeature],
    rebuttals: &[FeatureRebuttal],
) -> Vec<StrategyItem> {
    rebuttals
        .iter()
        .map(|r| StrategyItem {
            line: AttackLine::ZeroEvidence,
            role: Role::Attack,
            feature_ordinal: Some(r.ordinal),
            text: r.insufficient.clone(),
        })
        .collect()
}

/// 检测机理/效果差异
///
/// 判断依据：EffectMismatch 列表非空——Stage D 已检测到 OA 声称的技术效果
/// 与本申请说明书效果存在实质差异。
fn detect_mechanism_effect_diff(effect_mismatches: &[EffectMismatch]) -> Vec<StrategyItem> {
    effect_mismatches
        .iter()
        .map(|em| StrategyItem {
            line: AttackLine::MechanismEffectDiff,
            role: Role::Attack,
            feature_ordinal: None,
            text: em.rebuttal_text.clone(),
        })
        .collect()
}

/// 判断对比文件是否为学术 DOI 文献（如 OpenAlex 检索结果）
pub fn is_academic_doi_doc(doc: &ReferenceDocument) -> bool {
    doc.publication_number.trim().starts_with("10.")
        || doc.extraction_method.as_deref() == Some("openalex")
}

/// 从本申请说明书（优先“涉及…技术领域 / 属于…领域”）或发明名称提取本案技术领域
fn extract_app_technical_field(input: &CaseInput) -> String {
    if let Some(spec) = &input.spec_full_text {
        if let Ok(re) = regex::Regex::new(r"(?:涉及|属于)([^，。；\n]{2,30}?(?:技术领域|领域))")
        {
            if let Some(cap) = re.captures(spec) {
                if let Some(m) = cap.get(1) {
                    let extracted = m.as_str().trim();
                    if !extracted.is_empty() {
                        return extracted.to_string();
                    }
                }
            }
        }
    }
    input.case_meta.invention_title.trim().to_string()
}

/// 检测学术 DOI 文献反驳“公知常识”泛化认定
///
/// 当 `reference_documents` 含学术 DOI 文献，且 OA 含“公知常识”或存在泛化标签特征时，
/// 生成引用该文献 `label`/`publication_number` 的 `AttackLine::ZeroEvidence` 攻击项。
fn detect_academic_common_knowledge_rebuttal(
    input: &CaseInput,
    features: &[DistinguishingFeature],
) -> Vec<StrategyItem> {
    let has_trigger = input.oa_full_text.contains("公知常识") || !features.is_empty();
    if !has_trigger {
        return Vec::new();
    }

    let mut items = Vec::new();
    for doc in &input.reference_documents {
        if !is_academic_doi_doc(doc) {
            continue;
        }
        let pub_num = doc.publication_number.trim();
        let citation = if pub_num.is_empty() {
            doc.label.clone()
        } else {
            format!("{}（DOI: {}）", doc.label, pub_num)
        };
        items.push(StrategyItem {
            line: AttackLine::ZeroEvidence,
            role: Role::Attack,
            feature_ordinal: None,
            text: format!(
                "审查员将相关技术特征认定为公知常识，但并未提供公知常识性证据。\
                 学术文献{}表明该领域存在特定的技术机理与工程约束，\
                 不能仅以“公知常识”这一泛化定性替代具体的技术启示举证。",
                citation
            ),
        });
    }
    items
}

/// 检测部件标号拓扑连接关系差异
///
/// 遍历非学术对比文件，若有正文则调 `part_topology::diff_topology_rebuttals`
/// 生成 `AttackLine::MechanismEffectDiff` 攻击项。
fn detect_part_topology_diff(input: &CaseInput) -> Vec<StrategyItem> {
    let app_text = match &input.spec_full_text {
        Some(spec) if !spec.trim().is_empty() => {
            format!("{}\n{}", input.claims_under_review, spec)
        }
        _ => input.claims_under_review.clone(),
    };
    if app_text.trim().is_empty() {
        return Vec::new();
    }

    let mut items = Vec::new();
    for doc in &input.reference_documents {
        if is_academic_doi_doc(doc) || doc.full_text.trim().is_empty() {
            continue;
        }
        for rebuttal in
            part_topology::diff_topology_rebuttals(&app_text, &doc.label, &doc.full_text)
        {
            items.push(StrategyItem {
                line: AttackLine::MechanismEffectDiff,
                role: Role::Attack,
                feature_ordinal: None,
                text: rebuttal,
            });
        }
    }
    items
}

/// 检测载体不兼容
///
/// 判断依据：对比文件的 technical_field 与本申请不同（优先从说明书提取技术领域，
/// 缺失时回退至发明名称）。
fn detect_carrier_incompatible(input: &CaseInput) -> Vec<StrategyItem> {
    let mut items = Vec::new();
    let app_field = extract_app_technical_field(input);
    for doc in &input.reference_documents {
        if let Some(field) = &doc.technical_field {
            let field_trimmed = field.trim();
            if field_trimmed.is_empty() {
                continue;
            }
            let is_incompatible = if app_field.is_empty() {
                true
            } else {
                !field_trimmed.contains(&app_field) && !app_field.contains(field_trimmed)
            };
            if is_incompatible {
                items.push(StrategyItem {
                    line: AttackLine::CarrierIncompatible,
                    role: Role::Defense,
                    feature_ordinal: None,
                    text: format!(
                        "对比文件{}的技术领域为“{}”，\
                         与本申请技术领域不同，本领域技术人员缺乏跨领域结合的动机。",
                        doc.label, field_trimmed
                    ),
                });
            }
        }
    }
    items
}

/// 检测割裂式判断
///
/// 判断依据：FeatureRebuttal 的 severance 段——审查员将特征从整体方案中
/// 割裂单独评价。
fn detect_severance_judgment(rebuttals: &[FeatureRebuttal]) -> Vec<StrategyItem> {
    rebuttals
        .iter()
        .map(|r| StrategyItem {
            line: AttackLine::SeveranceJudgment,
            role: Role::Defense,
            feature_ordinal: Some(r.ordinal),
            text: r.severance.clone(),
        })
        .collect()
}

/// 主仲裁入口：按优先级排列攻击线
///
/// 所有数据来自 CaseInput 和管线中间产物，不硬编码任何案号/人名/特征词。
pub fn arbitrate(
    input: &CaseInput,
    features: &[DistinguishingFeature],
    rebuttals: &[FeatureRebuttal],
    effect_mismatches: &[EffectMismatch],
) -> ArbitrationResult {
    let mut items = Vec::new();

    // 1. 定性失真（最高优先级）
    items.extend(detect_qualitative_distortion(features));
    // 2. 零举证标签（含学术 DOI 反驳“公知常识”）
    items.extend(detect_zero_evidence(features, rebuttals));
    items.extend(detect_academic_common_knowledge_rebuttal(input, features));
    // 3. 机理/效果差异（含部件标号拓扑连接关系差异）
    items.extend(detect_mechanism_effect_diff(effect_mismatches));
    items.extend(detect_part_topology_diff(input));
    // 4. 载体不兼容
    items.extend(detect_carrier_incompatible(input));
    // 5. 割裂式判断（最低优先级）
    items.extend(detect_severance_judgment(rebuttals));

    // 按优先级排序（priority_rank 越小优先级越高）
    items.sort_by_key(|item| item.line.priority_rank());

    ArbitrationResult { items }
}

/// 长度约束校验：检查防守段是否≤3句
///
/// 返回违规项列表（空表示全部合规）
pub fn validate_length_constraints(result: &ArbitrationResult) -> Vec<(usize, usize)> {
    let mut violations = Vec::new();
    for (i, item) in result.items.iter().enumerate() {
        if let Some(max) = item.role.max_sentences() {
            let sentence_count = item
                .text
                .chars()
                .filter(|c| matches!(c, '。' | '？' | '！'))
                .count();
            if sentence_count > max {
                violations.push((i, sentence_count));
            }
        }
    }
    violations
}

/// 将文本截断至最多 n 句（按句号分句）
fn truncate_to_sentences(text: &str, max_sentences: usize) -> String {
    let mut result = String::new();
    let mut count = 0;
    for ch in text.chars() {
        result.push(ch);
        if matches!(ch, '。' | '？' | '！') {
            count += 1;
            if count >= max_sentences {
                break;
            }
        }
    }
    result
}

/// 将仲裁结果格式化为文本段（用于嵌入 Stage E 输出）
///
/// 按优先级排列，每条策略项标注攻击线类型和角色。
/// 主攻段完整输出，防守段截断至≤3句。
pub fn format_arbitration(result: &ArbitrationResult) -> String {
    let mut text = String::new();
    let mut current_line: Option<AttackLine> = None;

    for item in &result.items {
        if current_line != Some(item.line) {
            if current_line.is_some() {
                text.push('\n');
            }
            text.push_str(&format!(
                "【攻击线{}：{}】\n",
                item.line.priority_rank(),
                item.line.chinese_name()
            ));
            current_line = Some(item.line);
        }

        let role_tag = match item.role {
            Role::Attack => "主攻",
            Role::Defense => "防守",
        };

        let content = if let Some(max) = item.role.max_sentences() {
            truncate_to_sentences(&item.text, max)
        } else {
            item.text.clone()
        };

        if let Some(ord) = item.feature_ordinal {
            text.push_str(&format!("[{}·特征{}] {}\n", role_tag, ord, content));
        } else {
            text.push_str(&format!("[{}] {}\n", role_tag, content));
        }
    }

    text
}

#[cfg(test)]
mod tests {
    use super::super::case_input::OaTag;
    use super::super::case_input::{CaseMeta, ReferenceDocument};
    use super::*;

    fn make_test_input() -> CaseInput {
        CaseInput {
            case_meta: CaseMeta {
                application_number: "TEST001".to_string(),
                invention_title: "测试发明".to_string(),
                applicant: "测试申请人".to_string(),
                oa_date: Some("2026-01-01".to_string()),
                claim_basis_text: None,
                oa_type: "reject_review".to_string(),
                claim_basis_date: None,
            },
            oa_full_text: "测试OA文本".to_string(),
            reference_documents: vec![],
            claims_under_review: "测试权利要求".to_string(),
            spec_effect_statements: vec![],
            spec_full_text: None,
            legacy_analysis: None,
            legacy_discussion: None,
            custom_stopwords: vec![],
        }
    }

    fn make_test_feature(
        ordinal: usize,
        name: &str,
        tag: OaTag,
        ref_label: Option<&str>,
    ) -> DistinguishingFeature {
        DistinguishingFeature {
            ordinal,
            feature_name: name.to_string(),
            oa_tag: tag,
            attributed_ref: ref_label.map(|s| s.to_string()),
            oa_excerpt: "测试转述".to_string(),
        }
    }

    fn make_test_rebuttal(ordinal: usize, name: &str) -> FeatureRebuttal {
        FeatureRebuttal {
            ordinal,
            feature_name: name.to_string(),
            oa_tag: OaTag::EasyToThink,
            severance: "割裂段。".to_string(),
            synergy: "协同段。".to_string(),
            insufficient: "举证段。".to_string(),
        }
    }

    #[test]
    fn attack_line_priority_order() {
        assert!(
            AttackLine::QualitativeDistortion.priority_rank()
                < AttackLine::ZeroEvidence.priority_rank()
        );
        assert!(
            AttackLine::ZeroEvidence.priority_rank()
                < AttackLine::MechanismEffectDiff.priority_rank()
        );
        assert!(
            AttackLine::MechanismEffectDiff.priority_rank()
                < AttackLine::CarrierIncompatible.priority_rank()
        );
        assert!(
            AttackLine::CarrierIncompatible.priority_rank()
                < AttackLine::SeveranceJudgment.priority_rank()
        );
    }

    #[test]
    fn role_length_constraints() {
        assert_eq!(Role::Attack.max_sentences(), None);
        assert_eq!(Role::Defense.max_sentences(), Some(3));
    }

    #[test]
    fn arbitrate_returns_sorted_by_priority() {
        let input = make_test_input();
        let features = vec![make_test_feature(
            1,
            "特征A",
            OaTag::EasyToThink,
            Some("D1"),
        )];
        let rebuttals = vec![make_test_rebuttal(1, "特征A")];
        let effect_mismatches = vec![EffectMismatch {
            oa_claim: "OA声称".to_string(),
            ref_label: "D1".to_string(),
            actual_effect: "本申请效果".to_string(),
            rebuttal_text: "效果差异段。".to_string(),
        }];

        let result = arbitrate(&input, &features, &rebuttals, &effect_mismatches);

        for i in 1..result.items.len() {
            assert!(
                result.items[i - 1].line.priority_rank() <= result.items[i].line.priority_rank()
            );
        }
    }

    #[test]
    fn arbitrate_no_hardcoded_case_numbers() {
        let input = make_test_input();
        let features = vec![make_test_feature(
            1,
            "特征A",
            OaTag::EasyToThink,
            Some("D1"),
        )];
        let rebuttals = vec![make_test_rebuttal(1, "特征A")];

        let result = arbitrate(&input, &features, &rebuttals, &[]);
        let formatted = format_arbitration(&result);

        assert!(!formatted.contains("1990999999999"));
        assert!(!formatted.contains("示例申请人"));
    }

    #[test]
    fn validate_defense_length_constraint() {
        let input = make_test_input();
        let features = vec![make_test_feature(
            1,
            "特征A",
            OaTag::EasyToThink,
            Some("D1"),
        )];
        let rebuttals = vec![FeatureRebuttal {
            ordinal: 1,
            feature_name: "特征A".to_string(),
            oa_tag: OaTag::EasyToThink,
            severance: "第一句。第二句。第三句。第四句。".to_string(),
            synergy: "协同段。".to_string(),
            insufficient: "举证段。".to_string(),
        }];

        let result = arbitrate(&input, &features, &rebuttals, &[]);
        let violations = validate_length_constraints(&result);
        assert!(!violations.is_empty());
    }

    #[test]
    fn truncate_to_sentences_respects_limit() {
        let text = "第一句。第二句。第三句。第四句。第五句。";
        let truncated = truncate_to_sentences(text, 3);
        let count = truncated.chars().filter(|c| *c == '。').count();
        assert_eq!(count, 3);
    }

    #[test]
    fn format_arbitration_includes_line_labels() {
        let input = make_test_input();
        let features = vec![make_test_feature(
            1,
            "特征A",
            OaTag::EasyToThink,
            Some("D1"),
        )];
        let rebuttals = vec![make_test_rebuttal(1, "特征A")];

        let result = arbitrate(&input, &features, &rebuttals, &[]);
        let formatted = format_arbitration(&result);

        assert!(formatted.contains("【攻击线"));
        assert!(formatted.contains("主攻"));
        assert!(formatted.contains("防守"));
    }

    #[test]
    fn carrier_incompatible_from_reference_docs() {
        let mut input = make_test_input();
        input.reference_documents = vec![ReferenceDocument {
            label: "D1".to_string(),
            publication_number: "CN001".to_string(),
            full_text: "D1全文".to_string(),
            extraction_method: None,
            page_count: None,
            technical_field: Some("机械领域".to_string()),
        }];
        let result = arbitrate(&input, &[], &[], &[]);
        let has_carrier = result
            .items
            .iter()
            .any(|item| item.line == AttackLine::CarrierIncompatible);
        assert!(has_carrier);
    }

    #[test]
    fn empty_input_produces_empty_result() {
        let input = make_test_input();
        let result = arbitrate(&input, &[], &[], &[]);
        assert!(result.items.is_empty());
    }

    #[test]
    fn qualitative_distortion_is_highest_priority() {
        let input = make_test_input();
        let features = vec![make_test_feature(
            1,
            "特征A",
            OaTag::EasyToThink,
            Some("D1"),
        )];
        let rebuttals = vec![make_test_rebuttal(1, "特征A")];
        let em = vec![EffectMismatch {
            oa_claim: "c".to_string(),
            ref_label: "D1".to_string(),
            actual_effect: "e".to_string(),
            rebuttal_text: "t。".to_string(),
        }];

        let result = arbitrate(&input, &features, &rebuttals, &em);
        assert!(!result.items.is_empty());
        assert_eq!(result.items[0].line, AttackLine::QualitativeDistortion);
    }

    #[test]
    fn defense_items_truncated_in_format() {
        let input = make_test_input();
        let features = vec![make_test_feature(
            1,
            "特征A",
            OaTag::EasyToThink,
            Some("D1"),
        )];
        let rebuttals = vec![FeatureRebuttal {
            ordinal: 1,
            feature_name: "特征A".to_string(),
            oa_tag: OaTag::EasyToThink,
            severance: "一。二。三。四。五。六。".to_string(),
            synergy: "s。".to_string(),
            insufficient: "i。".to_string(),
        }];

        let result = arbitrate(&input, &features, &rebuttals, &[]);
        let formatted = format_arbitration(&result);

        // 防守段应被截断至3句
        let defense_section = formatted
            .lines()
            .find(|l| l.contains("防守") && l.contains("割裂"))
            .unwrap_or("");
        let sentence_count = defense_section.chars().filter(|c| *c == '。').count();
        assert!(sentence_count <= 3);
    }

    #[test]
    fn carrier_incompatible_uses_spec_field_not_applicant() {
        let mut input = make_test_input();
        // 即使申请人姓名恰好与对比文件领域相同，也不应误判为兼容
        input.case_meta.applicant = "机械制造".to_string();
        input.case_meta.invention_title = "生物医药制剂".to_string();
        input.reference_documents = vec![ReferenceDocument {
            label: "D1".to_string(),
            publication_number: "CN001".to_string(),
            full_text: "D1全文".to_string(),
            extraction_method: None,
            page_count: None,
            technical_field: Some("机械制造".to_string()),
        }];
        let res1 = arbitrate(&input, &[], &[], &[]);
        assert!(
            res1.items
                .iter()
                .any(|item| item.line == AttackLine::CarrierIncompatible),
            "applicant 相同但技术领域不同时应触发 CarrierIncompatible"
        );

        // 当说明书声明同一技术领域时，不应触发 CarrierIncompatible
        input.spec_full_text =
            Some("本发明涉及机械制造技术领域，具体公开了一种传动装置。".to_string());
        let res2 = arbitrate(&input, &[], &[], &[]);
        assert!(
            !res2
                .items
                .iter()
                .any(|item| item.line == AttackLine::CarrierIncompatible),
            "说明书领域与对比文件一致时不应触发 CarrierIncompatible"
        );
    }

    #[test]
    fn academic_doi_doc_detection_and_common_knowledge_rebuttal() {
        let doi_doc = ReferenceDocument {
            label: "NPL1".to_string(),
            publication_number: "10.1016/j.test.2026.001".to_string(),
            full_text: "学术摘要".to_string(),
            extraction_method: Some("openalex".to_string()),
            page_count: None,
            technical_field: None,
        };
        let patent_doc = ReferenceDocument {
            label: "D1".to_string(),
            publication_number: "CN001".to_string(),
            full_text: "D1全文".to_string(),
            extraction_method: None,
            page_count: None,
            technical_field: None,
        };
        assert!(is_academic_doi_doc(&doi_doc));
        assert!(!is_academic_doi_doc(&patent_doc));

        let mut input = make_test_input();
        input.oa_full_text = "审查员认为上述特征属于本领域的公知常识。".to_string();
        input.reference_documents = vec![doi_doc];

        let result = arbitrate(&input, &[], &[], &[]);
        assert!(result.items.iter().any(|item| {
            item.line == AttackLine::ZeroEvidence
                && item.text.contains("NPL1")
                && item.text.contains("10.1016/j.test.2026.001")
        }));
    }

    #[test]
    fn part_topology_diff_generates_mechanism_effect_diff() {
        let mut input = make_test_input();
        input.claims_under_review =
            "一种控制阀，包括1-机架、2-阀体和3-阀芯，阀芯3与阀体2滑动配合，阀体2固定连接在机架1上。"
                .to_string();
        input.reference_documents = vec![ReferenceDocument {
            label: "D1".to_string(),
            publication_number: "CN002".to_string(),
            full_text: "对比文件1公开了一种机架和阀体，但未公开阀芯及其滑动配合结构。".to_string(),
            extraction_method: None,
            page_count: None,
            technical_field: None,
        }];

        let result = arbitrate(&input, &[], &[], &[]);
        assert!(result.items.iter().any(|item| {
            item.line == AttackLine::MechanismEffectDiff
                && item.text.contains("D1")
                && item.text.contains("阀芯")
        }));
    }

    #[test]
    fn part_topology_extracts_legend_and_inline_filters_non_parts() {
        let text = "如图1所示，实施例1及权利要求1记载：1-机架，2、电缸，10：阀体；\
                    阀芯3与第一壳体12滑动配合，对比文件1和步骤1应被过滤。";
        let parts = part_topology::extract_part_designators(text);
        assert!(parts.iter().any(|p| p.number == "1" && p.name == "机架"));
        assert!(parts.iter().any(|p| p.number == "2" && p.name == "电缸"));
        assert!(parts.iter().any(|p| p.number == "10" && p.name == "阀体"));
        assert!(parts.iter().any(|p| p.number == "3" && p.name == "阀芯"));
        assert!(parts
            .iter()
            .any(|p| p.number == "12" && p.name == "第一壳体"));
        assert!(!parts
            .iter()
            .any(|p| p.name.contains("图") || p.name.contains("步骤")));
    }

    #[test]
    fn arbitration_output_passes_output_guard() {
        let mut input = make_test_input();
        input.oa_full_text = "审查员认为该结构属于公知常识。".to_string();
        input.spec_full_text =
            Some("本发明涉及流体控制技术领域。1-机架，2-阀体，3-阀芯。".to_string());
        input.claims_under_review = "阀芯3与阀体2滑动配合。".to_string();
        input.reference_documents = vec![
            ReferenceDocument {
                label: "D1".to_string(),
                publication_number: "CN001".to_string(),
                full_text: "对比文件1公开了一种机架与阀体。".to_string(),
                extraction_method: None,
                page_count: None,
                technical_field: Some("电磁感应技术领域".to_string()),
            },
            ReferenceDocument {
                label: "NPL1".to_string(),
                publication_number: "10.1016/j.test.2026.002".to_string(),
                full_text: "Academic abstract.".to_string(),
                extraction_method: Some("openalex".to_string()),
                page_count: None,
                technical_field: None,
            },
        ];

        let features = vec![make_test_feature(
            1,
            "滑动配合",
            OaTag::RoutineDesign,
            Some("D1"),
        )];
        let rebuttals = vec![make_test_rebuttal(1, "滑动配合")];
        let result = arbitrate(&input, &features, &rebuttals, &[]);
        let formatted = format_arbitration(&result);

        let guard = crate::ai::output_guard::run_guard(&formatted, &input);
        assert!(
            guard.passed,
            "output_guard 应放行仲裁输出，实际错误: {:?}",
            guard.errors
        );
    }
}
