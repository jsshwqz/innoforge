//! R6a: 递交前表单校验器
//!
//! 输入 CaseInput + 目标系统类型（如 CNIPA 复审网页表单），
//! 输出"递交前检查清单"：每项给通过条件与当前值。
//!
//! 硬约束：不硬编码任何案号/人名/特征词。所有判断基于 CaseInput 数据。

use serde::{Deserialize, Serialize};

use super::case_input::CaseInput;

/// 目标递交系统类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TargetSystem {
    /// CNIPA 复审网页表单
    CnipaReexam,
    /// CNIPA 一通答复网页表单
    CnipaFirstOfficeAction,
}

/// 单项检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreCheckItem {
    /// 检查项名称
    pub name: String,
    /// 通过条件描述
    pub pass_condition: String,
    /// 当前值（字符串化）
    pub current_value: String,
    /// 是否通过
    pub passed: bool,
    /// 未通过时的修复建议
    pub fix_hint: Option<String>,
}

/// 完整检查清单
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreSubmissionChecklist {
    /// 目标系统
    pub target: TargetSystem,
    /// 检查项列表
    pub items: Vec<PreCheckItem>,
    /// 总体是否通过
    pub all_passed: bool,
}

/// 运行递交前检查，返回完整清单
pub fn run_pre_submission_check(input: &CaseInput, target: TargetSystem) -> PreSubmissionChecklist {
    let mut items = Vec::new();

    // 1. 申请人类型/证件类型
    items.push(check_applicant_type(input));

    // 2. 地址省市重复
    items.push(check_address_dedup(input));

    // 3. 驳回决定发文日
    items.push(check_oa_date(input));

    // 4. 意见书正文非空
    items.push(check_opinion_text(input));

    // 5. 附件清单
    items.push(check_attachment_list(input));

    // CNIPA 复审额外检查
    if matches!(target, TargetSystem::CnipaReexam) {
        items.push(check_reexam_fee(input));
    }

    let all_passed = items.iter().all(|item| item.passed);

    PreSubmissionChecklist {
        target,
        items,
        all_passed,
    }
}

/// 检查1：申请人类型/证件类型
///
/// 通过条件：legacy_analysis 或 legacy_discussion 中包含"申请人类型"且非空，
/// 或 CaseMeta.applicant 非空（至少有申请人姓名）。
fn check_applicant_type(input: &CaseInput) -> PreCheckItem {
    let applicant = &input.case_meta.applicant;

    // 从 legacy_analysis 或 legacy_discussion 中提取申请人类型
    let legacy = format!(
        "{} {}",
        input.legacy_analysis.as_deref().unwrap_or(""),
        input.legacy_discussion.as_deref().unwrap_or("")
    );

    let has_type = legacy.contains("申请人类型")
        || legacy.contains("个人")
        || legacy.contains("企业")
        || legacy.contains("事业单位")
        || legacy.contains("社会团体");

    let has_id_type = legacy.contains("证件类型")
        || legacy.contains("身份证")
        || legacy.contains("统一社会信用代码")
        || legacy.contains("组织机构代码");

    let passed = !applicant.is_empty() && (has_type || has_id_type);

    let current_value = if applicant.is_empty() {
        "申请人姓名缺失".to_string()
    } else if !has_type && !has_id_type {
        format!("姓名={}(类型/证件类型缺失)", applicant)
    } else {
        format!(
            "姓名={}({})",
            applicant,
            if has_type {
                "类型已填"
            } else {
                "证件类型已填"
            }
        )
    };

    PreCheckItem {
        name: "申请人类型/证件类型".to_string(),
        pass_condition: "申请人姓名非空且包含申请人类型或证件类型信息".to_string(),
        current_value,
        passed,
        fix_hint: if passed {
            None
        } else if applicant.is_empty() {
            Some("请填写申请人姓名".to_string())
        } else {
            Some("请在 legacy_analysis 或 legacy_discussion 中补充申请人类型(个人/企业/事业单位)和证件类型(身份证/统一社会信用代码)".to_string())
        },
    }
}

/// 检查2：地址省市重复
///
/// 通过条件：地址中不出现"XX省XX市XX省"或"XX市XX市"等重复模式。
/// 地址从 legacy_analysis 或 legacy_discussion 中提取。
fn check_address_dedup(input: &CaseInput) -> PreCheckItem {
    let legacy = format!(
        "{} {}",
        input.legacy_analysis.as_deref().unwrap_or(""),
        input.legacy_discussion.as_deref().unwrap_or("")
    );

    // 检测省市重复模式
    let has_dup = detect_address_duplication(&legacy);

    let current_value = if legacy.is_empty() {
        "无地址信息".to_string()
    } else if has_dup {
        "地址含省市重复".to_string()
    } else {
        "地址无重复".to_string()
    };

    PreCheckItem {
        name: "地址省市去重".to_string(),
        pass_condition: "地址中不出现省市重复(如'XX省XX市XX省'或'XX市XX市')".to_string(),
        current_value,
        passed: !has_dup,
        fix_hint: if has_dup {
            Some("请去除地址中的重复省市字段".to_string())
        } else {
            None
        },
    }
}

/// 检测地址中省市重复
fn detect_address_duplication(text: &str) -> bool {
    // 检测 "XX省XX市XX省" 或 "XX市XX市" 模式
    // 简化实现：查找连续出现的省或市
    let province_positions: Vec<usize> = text.match_indices('省').map(|(i, _)| i).collect();
    let city_positions: Vec<usize> = text.match_indices('市').map(|(i, _)| i).collect();

    // 如果有2个以上"省"或2个以上"市"，可能存在重复
    if province_positions.len() >= 2 {
        for i in 0..province_positions.len() - 1 {
            let dist = province_positions[i + 1] - province_positions[i];
            // "省X市X省" 模式（省之间有市）或 "省X省" 近距离重复（如"江苏省江苏省"）
            if text[province_positions[i]..province_positions[i + 1]].contains('市') || dist < 10 {
                return true;
            }
        }
    }

    if city_positions.len() >= 2 {
        // 检查两个"市"之间距离是否很短（<20字符），可能是"XX市XX市"重复
        for i in 0..city_positions.len() - 1 {
            let dist = city_positions[i + 1] - city_positions[i];
            if dist < 20 {
                return true;
            }
        }
    }

    false
}

/// 检查3：驳回决定发文日
fn check_oa_date(input: &CaseInput) -> PreCheckItem {
    let date = &input.case_meta.oa_date;
    let passed = date.is_some();

    PreCheckItem {
        name: "驳回决定发文日".to_string(),
        pass_condition: "oa_date 非空".to_string(),
        current_value: date.clone().unwrap_or_else(|| "缺失".to_string()),
        passed,
        fix_hint: if passed {
            None
        } else {
            Some("请填写驳回决定发文日".to_string())
        },
    }
}

/// 检查4：意见书正文非空
///
/// 通过条件：claims_under_review 或 oa_full_text 非空（至少有内容可写答复）
fn check_opinion_text(input: &CaseInput) -> PreCheckItem {
    let has_content = !input.claims_under_review.is_empty() || !input.oa_full_text.is_empty();

    let current_value = if input.claims_under_review.is_empty() {
        if input.oa_full_text.is_empty() {
            "权利要求和OA全文均为空".to_string()
        } else {
            "权利要求为空(有OA全文)".to_string()
        }
    } else {
        format!("权利要求{}字符", input.claims_under_review.len())
    };

    PreCheckItem {
        name: "意见书正文非空".to_string(),
        pass_condition: "claims_under_review 或 oa_full_text 至少一个非空".to_string(),
        current_value,
        passed: has_content,
        fix_hint: if has_content {
            None
        } else {
            Some("请填写权利要求或OA全文".to_string())
        },
    }
}

/// 检查5：附件清单
///
/// 通过条件：reference_documents 非空（至少有对比文件作为附件）
fn check_attachment_list(input: &CaseInput) -> PreCheckItem {
    let count = input.reference_documents.len();
    let passed = count > 0;

    PreCheckItem {
        name: "附件清单".to_string(),
        pass_condition: "reference_documents 至少1条(对比文件作为附件)".to_string(),
        current_value: format!("{}个对比文件", count),
        passed,
        fix_hint: if passed {
            None
        } else {
            Some("请添加至少1个对比文件".to_string())
        },
    }
}

/// 检查6（CNIPA复审专用）：复审费缴纳信息
fn check_reexam_fee(input: &CaseInput) -> PreCheckItem {
    let legacy = format!(
        "{} {}",
        input.legacy_analysis.as_deref().unwrap_or(""),
        input.legacy_discussion.as_deref().unwrap_or("")
    );

    let has_fee_info = legacy.contains("复审费")
        || legacy.contains("恢复费")
        || legacy.contains("已缴纳")
        || legacy.contains("缴费");

    PreCheckItem {
        name: "复审费缴纳".to_string(),
        pass_condition: "legacy_analysis 或 legacy_discussion 含复审费缴纳信息".to_string(),
        current_value: if has_fee_info {
            "已提供缴费信息".to_string()
        } else {
            "未提供缴费信息".to_string()
        },
        passed: has_fee_info,
        fix_hint: if has_fee_info {
            None
        } else {
            Some("请在 legacy_analysis 中补充复审费缴纳信息".to_string())
        },
    }
}

/// 将检查清单格式化为可读文本
pub fn format_checklist(checklist: &PreSubmissionChecklist) -> String {
    let mut text = String::new();
    let target_name = match checklist.target {
        TargetSystem::CnipaReexam => "CNIPA复审网页表单",
        TargetSystem::CnipaFirstOfficeAction => "CNIPA一通答复网页表单",
    };

    text.push_str(&format!("递交前检查清单（目标：{}）\n", target_name));
    text.push_str(&format!(
        "总体：{}\n\n",
        if checklist.all_passed {
            "✓ 全部通过"
        } else {
            "✗ 存在未通过项"
        }
    ));

    for (i, item) in checklist.items.iter().enumerate() {
        let status = if item.passed { "✓" } else { "✗" };
        text.push_str(&format!("{}. {} [{}]\n", i + 1, item.name, status));
        text.push_str(&format!("   通过条件：{}\n", item.pass_condition));
        text.push_str(&format!("   当前值：{}\n", item.current_value));
        if let Some(hint) = &item.fix_hint {
            text.push_str(&format!("   修复建议：{}\n", hint));
        }
        text.push('\n');
    }

    text
}

#[cfg(test)]
mod tests {
    use super::super::case_input::{CaseMeta, ReferenceDocument};
    use super::*;

    fn make_base_input() -> CaseInput {
        CaseInput {
            case_meta: CaseMeta {
                application_number: "TEST".to_string(),
                applicant: "测试申请人".to_string(),
                invention_title: "测试发明".to_string(),
                oa_date: Some("2026-07-10".to_string()),
                oa_type: "reject_review".to_string(),
                claim_basis_date: None,
                claim_basis_text: None,
            },
            oa_full_text: "OA正文".to_string(),
            reference_documents: vec![ReferenceDocument {
                label: "D1".to_string(),
                publication_number: "CN123456789A".to_string(),
                full_text: "D1原文".to_string(),
                extraction_method: None,
                page_count: None,
                technical_field: None,
            }],
            claims_under_review: "权利要求1".to_string(),
            spec_effect_statements: vec![],
            spec_full_text: None,
            legacy_analysis: None,
            legacy_discussion: None,
            custom_stopwords: vec![],
        }
    }

    #[test]
    fn all_pass_for_complete_input() {
        let mut input = make_base_input();
        input.legacy_analysis =
            Some("申请人类型：企业。证件类型：统一社会信用代码。复审费已缴纳。".to_string());
        let checklist = run_pre_submission_check(&input, TargetSystem::CnipaReexam);
        assert!(
            checklist.all_passed,
            "Failed items: {:?}",
            checklist
                .items
                .iter()
                .filter(|i| !i.passed)
                .collect::<Vec<_>>()
        );
    }

    // JY9060015: 地址重复必须被预检命中
    #[test]
    fn jy9060015_address_duplication_detected() {
        let mut input = make_base_input();
        input.case_meta.application_number = "JY9060015".to_string();
        // 地址含省市重复："广东省深圳市广东省"
        input.legacy_analysis = Some(
            "申请人类型：个人。证件类型：身份证。地址：广东省深圳市广东省广州市天河区。复审费已缴纳。".to_string()
        );
        let checklist = run_pre_submission_check(&input, TargetSystem::CnipaReexam);
        let addr_check = checklist
            .items
            .iter()
            .find(|i| i.name == "地址省市去重")
            .expect("地址检查项应存在");
        assert!(
            !addr_check.passed,
            "JY9060015 地址含省市重复，必须被预检命中"
        );
        assert!(addr_check.fix_hint.is_some(), "应提供修复建议");
    }

    // JY9060018: 申请人类型缺失必须被预检命中
    #[test]
    fn jy9060018_applicant_type_missing_detected() {
        let mut input = make_base_input();
        input.case_meta.application_number = "JY9060018".to_string();
        // 有申请人姓名但无类型/证件类型信息
        input.legacy_analysis = Some("地址：北京市海淀区。复审费已缴纳。".to_string());
        let checklist = run_pre_submission_check(&input, TargetSystem::CnipaReexam);
        let type_check = checklist
            .items
            .iter()
            .find(|i| i.name == "申请人类型/证件类型")
            .expect("申请人类型检查项应存在");
        assert!(
            !type_check.passed,
            "JY9060018 申请人类型缺失，必须被预检命中"
        );
        assert!(type_check.fix_hint.is_some(), "应提供修复建议");
    }

    #[test]
    fn empty_oa_date_detected() {
        let mut input = make_base_input();
        input.case_meta.oa_date = None;
        let checklist = run_pre_submission_check(&input, TargetSystem::CnipaReexam);
        let date_check = checklist
            .items
            .iter()
            .find(|i| i.name == "驳回决定发文日")
            .expect("发文日检查项应存在");
        assert!(!date_check.passed);
    }

    #[test]
    fn empty_attachments_detected() {
        let mut input = make_base_input();
        input.reference_documents = vec![];
        let checklist = run_pre_submission_check(&input, TargetSystem::CnipaReexam);
        let attach_check = checklist
            .items
            .iter()
            .find(|i| i.name == "附件清单")
            .expect("附件清单检查项应存在");
        assert!(!attach_check.passed);
    }

    #[test]
    fn format_checklist_readable() {
        let input = make_base_input();
        let checklist = run_pre_submission_check(&input, TargetSystem::CnipaReexam);
        let text = format_checklist(&checklist);
        assert!(text.contains("递交前检查清单"));
        assert!(text.contains("CNIPA复审网页表单"));
        // 不硬编码案号
        assert!(!text.contains("JY9060015"));
        assert!(!text.contains("JY9060018"));
    }

    #[test]
    fn no_hardcoded_case_numbers_in_check() {
        // 校验：检查模块本身不硬编码任何案号
        let input = make_base_input();
        let checklist = run_pre_submission_check(&input, TargetSystem::CnipaReexam);
        let text = format_checklist(&checklist);
        // 案号 TEST 不应出现在检查项名称或通过条件中
        for item in &checklist.items {
            assert!(!item.name.contains("TEST"), "检查项名称不应硬编码案号");
            assert!(
                !item.pass_condition.contains("TEST"),
                "通过条件不应硬编码案号"
            );
        }
        let _ = text; // suppress unused warning
    }
}
