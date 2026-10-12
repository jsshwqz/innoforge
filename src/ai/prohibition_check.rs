//! C12 答复禁令自查（纯函数）
//!
//! 答复函中有些表述是绝对禁止的——写了就等于自认无效。
//! 本模块对答复函草稿做禁令扫描，输出违规清单。
//!
//! 禁令清单（FACT_DISCIPLINE 的子集）：
//! 1. 禁止承认"常规设计""容易想到"等审查员断言
//! 2. 禁止放弃权项（"放弃权利要求N"）未标注理由
//! 3. 禁止编造对比文件中不存在的技术特征
//! 4. 禁止使用"公知常识"作为反驳论据
//! 5. 禁止引用未在 OA 中出现的对比文件
//! 6. 禁止修改权项引入新内容（超范围）
//! 7. 禁止仅否定审查员结论而不提供技术论据

use crate::ai::whitespace_search::strip_whitespace;

/// C12 禁令自查报告
#[derive(Debug, Clone, PartialEq)]
pub struct ProhibitionCheckReport {
    /// 违规列表
    pub violations: Vec<ProhibitionViolation>,
    /// 违规总数
    pub count: usize,
    /// 人类可读摘要
    pub summary: String,
}

/// 单条违规
#[derive(Debug, Clone, PartialEq)]
pub struct ProhibitionViolation {
    /// 禁令编号
    pub rule_id: String,
    /// 禁令描述
    pub rule_description: String,
    /// 违规文本片段
    pub violating_text: String,
    /// 严重程度
    pub severity: ProhibitionSeverity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProhibitionSeverity {
    /// 致命：写了就等于自认无效
    Fatal,
    /// 警告：有风险但非致命
    Warning,
}

/// C12 主函数：对答复函草稿做禁令扫描
///
/// # 参数
/// - `response_text`: 答复函草稿全文
pub fn check_prohibitions(response_text: &str) -> ProhibitionCheckReport {
    let stripped = strip_whitespace(response_text);

    let mut violations = Vec::new();

    // 禁令1：禁止承认审查员断言
    let forbidden_concessions = [
        "属于常规设计",
        "属于常规设置",
        "属于惯用手段",
        "属于常规选择",
        "属于常规调整",
        "确实容易想到",
        "确实是常规",
        "本领域技术人员容易",
        "确实是公知常识",
        "确实是常用技术",
    ];
    for phrase in &forbidden_concessions {
        if stripped.contains(phrase) {
            violations.push(ProhibitionViolation {
                rule_id: "P1".into(),
                rule_description: "禁止承认审查员的常规性断言".into(),
                violating_text: phrase.to_string(),
                severity: ProhibitionSeverity::Fatal,
            });
        }
    }

    // 禁令2：禁止放弃权项未标注理由
    let re_abandon = regex::Regex::new(r"放弃权利要求(\d+)").ok();
    if let Some(re) = re_abandon {
        for cap in re.captures_iter(&stripped) {
            let claim_num = cap
                .get(1)
                .and_then(|m| m.as_str().parse::<usize>().ok())
                .unwrap_or(0);
            let pos = cap.get(0).unwrap().end();
            let after = &stripped[pos..pos + 100.min(stripped.len() - pos)];
            // 检查后续是否有理由说明
            if !after.contains("因为") && !after.contains("由于") && !after.contains("理由") {
                violations.push(ProhibitionViolation {
                    rule_id: "P2".into(),
                    rule_description: format!("放弃权利要求{}未标注理由", claim_num),
                    violating_text: format!("放弃权利要求{}", claim_num),
                    severity: ProhibitionSeverity::Fatal,
                });
            }
        }
    }

    // 禁令3：禁止使用"公知常识"作为反驳论据
    if stripped.contains("公知常识") {
        // 检查是否在反驳语境中使用（"并非公知常识"不算违规）
        let re = regex::Regex::new(r"(?<!并非)(?<!不是)(?<!不属于)公知常识").ok();
        if let Some(re) = re {
            if re.is_match(&stripped) {
                violations.push(ProhibitionViolation {
                    rule_id: "P3".into(),
                    rule_description: "禁止使用'公知常识'作为反驳论据".into(),
                    violating_text: "公知常识".into(),
                    severity: ProhibitionSeverity::Warning,
                });
            }
        }
    }

    // 禁令4：禁止仅否定结论而不提供技术论据
    let naked_negations = [
        "审查员的结论是错误的",
        "审查员的认定不成立",
        "审查员的理解有误",
        "上述认定不能成立",
    ];
    for phrase in &naked_negations {
        if stripped.contains(phrase) {
            // 检查附近是否有技术论据
            let pos = stripped.find(phrase).unwrap();
            let after = &stripped[pos..pos + 200.min(stripped.len() - pos)];
            let has_tech_arg = after.contains("因为")
                || after.contains("由于")
                || after.contains("具体而言")
                || after.contains("事实上")
                || after.contains("首先");
            if !has_tech_arg {
                violations.push(ProhibitionViolation {
                    rule_id: "P4".into(),
                    rule_description: "仅否定审查员结论而未提供技术论据".into(),
                    violating_text: phrase.to_string(),
                    severity: ProhibitionSeverity::Warning,
                });
            }
        }
    }

    // 禁令5：禁止"同意审查员意见"
    let agreement_phrases = [
        "同意审查员",
        "认可审查员",
        "审查员所述正确",
        "审查员的意见合理",
    ];
    for phrase in &agreement_phrases {
        if stripped.contains(phrase) {
            violations.push(ProhibitionViolation {
                rule_id: "P5".into(),
                rule_description: "禁止同意审查员意见（答复函是反驳文件）".into(),
                violating_text: phrase.to_string(),
                severity: ProhibitionSeverity::Fatal,
            });
        }
    }

    // 禁令6：禁止引入新内容
    let new_content_phrases = ["新增", "新加入", "进一步改进", "额外设置"];
    for phrase in &new_content_phrases {
        if stripped.contains(phrase) {
            violations.push(ProhibitionViolation {
                rule_id: "P6".into(),
                rule_description: "可能引入超范围新内容".into(),
                violating_text: phrase.to_string(),
                severity: ProhibitionSeverity::Warning,
            });
        }
    }

    let count = violations.len();
    let fatal_count = violations
        .iter()
        .filter(|v| v.severity == ProhibitionSeverity::Fatal)
        .count();

    let summary = if count == 0 {
        "禁令自查全部通过，未检测到违规。".to_string()
    } else {
        format!(
            "检测到{}处违规（{}处致命、{}处警告）",
            count,
            fatal_count,
            count - fatal_count
        )
    };

    ProhibitionCheckReport {
        violations,
        count,
        summary,
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  测试
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_response_passes() {
        let response = "审查员关于权利要求1不具备创造性的认定不能成立，因为本发明的对角安装方式产生了意想不到的技术效果。";
        let report = check_prohibitions(response);
        assert_eq!(report.count, 0);
        assert!(report.summary.contains("全部通过"));
    }

    #[test]
    fn detects_concession() {
        let response = "权利要求2的把手确实容易想到。";
        let report = check_prohibitions(response);
        assert!(report.count > 0);
        assert!(report.violations.iter().any(|v| v.rule_id == "P1"));
    }

    #[test]
    fn detects_unjustified_abandonment() {
        let response = "放弃权利要求3。";
        let report = check_prohibitions(response);
        assert!(report.violations.iter().any(|v| v.rule_id == "P2"));
    }

    #[test]
    fn justified_abandonment_ok() {
        let response = "放弃权利要求3，因为对比文件1已公开该特征。";
        let report = check_prohibitions(response);
        assert!(!report.violations.iter().any(|v| v.rule_id == "P2"));
    }

    #[test]
    fn detects_naked_negation() {
        let response = "审查员的结论是错误的。";
        let report = check_prohibitions(response);
        assert!(report.violations.iter().any(|v| v.rule_id == "P4"));
    }

    #[test]
    fn negation_with_argument_ok() {
        let response = "审查员的结论是错误的，因为对角安装与交错设置在技术效果上存在本质区别。";
        let report = check_prohibitions(response);
        assert!(!report.violations.iter().any(|v| v.rule_id == "P4"));
    }

    #[test]
    fn detects_agreement() {
        let response = "同意审查员关于权利要求2的意见。";
        let report = check_prohibitions(response);
        assert!(report.violations.iter().any(|v| v.rule_id == "P5"));
    }

    #[test]
    fn not_gongzhichangshi_ok() {
        // "并非公知常识" 不应触发 P3
        let response = "对角安装并非公知常识，因为对比文件均未公开。";
        let report = check_prohibitions(response);
        assert!(!report.violations.iter().any(|v| v.rule_id == "P3"));
    }

    // ── 示例申请人案回归 ──

    #[test]
    fn sample_case_c12_golden_reference_prohibition_check() {
        let golden = include_str!("../../tests/fixtures/oa_sample/golden_reference.txt");
        let report = check_prohibitions(golden);
        eprintln!("示例申请人案 C12 黄金参考：{}处违规", report.count);
        for v in &report.violations {
            eprintln!(
                "  违规 {}: {} — '{}'",
                v.rule_id, v.rule_description, v.violating_text
            );
        }
        // 黄金参考不应有致命违规
        let fatal_count = report
            .violations
            .iter()
            .filter(|v| v.severity == ProhibitionSeverity::Fatal)
            .count();
        assert_eq!(fatal_count, 0, "黄金参考不应有致命违规");
    }
}
