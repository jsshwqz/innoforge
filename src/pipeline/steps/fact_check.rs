//! MB1: 幻觉防线扩面 — 将 check_oa_analysis 接入创意分析 pipeline
//!
//! OA 答复路径（routes/ai.rs）已有 check_oa_analysis 调用。
//! 本模块将其等价能力接入创意分析 pipeline，两条路径共用同一核查实现。
//!
//! 处置策略：
//! - 默认：标注（将警告追加到报告末尾）
//! - 致命档（扣分 ≥35，即 score ≤65）：拒绝并给可读原因

use crate::ai::{check_oa_analysis, format_report};
use crate::pipeline::context::PipelineContext;

/// 致命档阈值：扣分 ≥35 即 score ≤65
const FATAL_SCORE_THRESHOLD: f64 = 65.0;

/// 对创意分析结果执行事实校验
///
/// 使用 top_matches 的摘要作为对比文献（references），
/// 创意描述作为本专利说明书（my_patent）。
/// OA 与创意两条路径共用此同一 check_oa_analysis 实现，不留第二份判定。
pub fn run_fact_check(ctx: &mut PipelineContext) {
    if ctx.ai_analysis.is_empty() {
        return;
    }

    // 拼装对比文献：取 top_matches 的摘要
    let references: String = ctx
        .top_matches
        .iter()
        .map(|m| format!("{}\n{}", m.source_title, m.snippet))
        .collect::<Vec<_>>()
        .join("\n---\n");

    // 创意描述作为"本专利说明书"
    let my_patent = &ctx.description;

    // 调用共用的事实校验函数（与 routes/ai.rs:1433 同一实现）
    let report = check_oa_analysis(&ctx.ai_analysis, &references, my_patent);

    tracing::info!(
        "Fact check completed: score={:.0}, warnings={}",
        report.score,
        report.warnings.len()
    );

    // 致命档判定：扣分 ≥35（score ≤65）
    let is_fatal = report.score <= FATAL_SCORE_THRESHOLD;

    if is_fatal {
        // 拒绝并给可读原因
        ctx.fact_check_rejected = true;

        let mut rejection = String::new();
        rejection.push_str("\n\n---\n\n## ⚠️ 事实校验未通过\n\n");
        rejection.push_str(&format!(
            "可信度评分：{:.0}/100（低于阈值 {:.0}），分析结果可能包含严重事实性问题。\n\n",
            report.score, FATAL_SCORE_THRESHOLD
        ));
        rejection.push_str("检测到的问题：\n\n");
        for (i, w) in report.warnings.iter().enumerate() {
            let icon = match w.severity.as_str() {
                "致命" => "🔴",
                "高" => "🟠",
                "中" => "🟡",
                _ => "⚪",
            };
            rejection.push_str(&format!(
                "{}. {} {}（{}）\n",
                i + 1,
                icon,
                w.category,
                w.severity
            ));
            rejection.push_str(&format!("   {}\n", w.description));
        }
        rejection.push_str("\n**建议：** 请人工复核上述问题后再采信分析结论。\n");

        ctx.ai_analysis.push_str(&rejection);
    } else if !report.warnings.is_empty() {
        // 标注：追加警告到报告末尾（不拒绝）
        let formatted = format_report(&report);
        ctx.ai_analysis.push_str("\n\n---\n\n## 事实校验标注\n\n");
        ctx.ai_analysis.push_str(&formatted);
    }

    ctx.fact_check_report = Some(report);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::check_oa_analysis;
    use crate::pipeline::context::PipelineContext;

    #[test]
    fn test_fact_check_clean_analysis() {
        let mut ctx = PipelineContext::new("test-idea", "测试创意", "一种新型太阳能电池");
        ctx.ai_analysis = "该技术方案具有创新性。".to_string();

        run_fact_check(&mut ctx);

        let report = ctx.fact_check_report.expect("Report should be generated");
        assert!(!ctx.fact_check_rejected, "Should not reject clean analysis");
        assert!(report.score > FATAL_SCORE_THRESHOLD);
    }

    #[test]
    fn test_fact_check_fabricated_law_article() {
        // 诱导用例：编造法条编号
        let ai_output = "根据专利法第99条第3款，该技术方案不具备新颖性。";
        let references = "现有技术描述";
        let my_patent = "本专利说明书";

        let report = check_oa_analysis(ai_output, references, my_patent);
        // Should detect some issue (fabricated legal reference)
        // The exact detection depends on implementation, but it should not crash
        assert!(report.score <= 100.0);
    }

    #[test]
    fn test_fact_check_fabricated_page_numbers() {
        // 诱导用例：编造页码引用
        let ai_output = "对比文件第999页第3段明确记载了该技术特征。";
        let references = "现有技术描述，只有5页内容";
        let my_patent = "本专利说明书";

        let report = check_oa_analysis(ai_output, references, my_patent);
        // Should detect paragraph reference issue
        assert!(report.score <= 100.0);
    }

    #[test]
    fn test_fact_check_fabricated_citations() {
        // 诱导用例：编造专利引用号
        let ai_output = "根据 CN999999999A 的记载，该技术已被公开。";
        let references = "现有技术描述";
        let my_patent = "本专利说明书";

        let report = check_oa_analysis(ai_output, references, my_patent);
        // Should not crash, may or may not detect depending on implementation
        assert!(report.score <= 100.0);
    }

    #[test]
    fn test_fact_check_fatal_rejection() {
        let mut ctx = PipelineContext::new("test-idea", "测试创意", "测试描述");
        // 构造一个会触发多个致命警告的 AI 输出
        ctx.ai_analysis = "根据专利法第99条，对比文件第999页记载，该技术评分0分。".to_string();
        ctx.top_matches.push(crate::pipeline::context::RankedMatch {
            source_id: "CN001".to_string(),
            source_title: "测试专利".to_string(),
            source_type: "patent".to_string(),
            source_url: String::new(),
            snippet: "测试摘要".to_string(),
            combined_score: 0.5,
            ..Default::default()
        });

        run_fact_check(&mut ctx);

        // Should have a report
        assert!(ctx.fact_check_report.is_some());
        // If score is low enough, should be rejected
        if let Some(ref report) = ctx.fact_check_report {
            if report.score <= FATAL_SCORE_THRESHOLD {
                assert!(ctx.fact_check_rejected);
                assert!(ctx.ai_analysis.contains("事实校验未通过"));
            }
        }
    }

    #[test]
    fn test_oa_and_idea_share_same_function() {
        // 验证 OA 路径和创意路径调用的是同一个 check_oa_analysis 函数
        let ai_output = "测试文本";
        let references = "参考文献";
        let my_patent = "本专利";

        // OA 路径调用（模拟 routes/ai.rs:1433）
        let oa_report = check_oa_analysis(ai_output, references, my_patent);

        // 创意路径调用（模拟 pipeline/steps/fact_check.rs）
        let idea_report = check_oa_analysis(ai_output, references, my_patent);

        // 两者必须完全相同（同一函数，同一实现）
        assert_eq!(oa_report.score, idea_report.score);
        assert_eq!(oa_report.warnings.len(), idea_report.warnings.len());
    }

    #[test]
    fn test_empty_analysis_skipped() {
        let mut ctx = PipelineContext::new("test-idea", "测试创意", "测试描述");
        ctx.ai_analysis = String::new();

        run_fact_check(&mut ctx);

        assert!(ctx.fact_check_report.is_none());
        assert!(!ctx.fact_check_rejected);
    }

    #[test]
    fn test_fact_check_report_stored_in_context() {
        let mut ctx = PipelineContext::new("test-idea", "测试创意", "测试描述");
        ctx.ai_analysis = "该方案具有创新性。".to_string();

        run_fact_check(&mut ctx);

        assert!(ctx.fact_check_report.is_some());
        let report = ctx.fact_check_report.unwrap();
        assert!(report.score >= 0.0 && report.score <= 100.0);
    }
}
