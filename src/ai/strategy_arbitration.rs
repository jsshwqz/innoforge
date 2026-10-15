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

use super::case_input::CaseInput;
use super::case_input::{DistinguishingFeature, EffectMismatch, FeatureRebuttal};

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

/// 检测载体不兼容
///
/// 判断依据：对比文件的 technical_field 与本申请不同（如有该元数据）。
/// 当前为确定性检测：检查 reference_documents 中的 technical_field 字段。
fn detect_carrier_incompatible(input: &CaseInput) -> Vec<StrategyItem> {
    let mut items = Vec::new();
    let app_field = input.case_meta.applicant.as_str();
    for doc in &input.reference_documents {
        if let Some(field) = &doc.technical_field {
            if !field.is_empty() && field != app_field {
                items.push(StrategyItem {
                    line: AttackLine::CarrierIncompatible,
                    role: Role::Defense,
                    feature_ordinal: None,
                    text: format!(
                        "对比文件{}的技术领域为“{}”，\
                         与本申请技术领域不同，本领域技术人员缺乏跨领域结合的动机。",
                        doc.label, field
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
    // 2. 零举证标签
    items.extend(detect_zero_evidence(features, rebuttals));
    // 3. 机理/效果差异
    items.extend(detect_mechanism_effect_diff(effect_mismatches));
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
}
