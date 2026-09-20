/// Reflection Agent - deterministic quality scorer
use crate::pipeline::context::{AgentOutput, PipelineContext, ReflectionResult};

const RETRY_THRESHOLD: f64 = 0.5;
const MAX_RETRIES: u32 = 2;

pub fn evaluate(ctx: &PipelineContext) -> ReflectionResult {
    let mut result = ReflectionResult {
        quality_score: 0.5,
        completeness_score: 0.5,
        consistency_score: 0.5,
        evidence_score: 0.5,
        needs_retry: false,
        retry_reason: None,
        improvement_suggestions: Vec::new(),
        evaluated_step: String::new(),
    };

    let completeness = score_completeness(ctx);
    result.completeness_score = completeness;
    if completeness < 0.4 {
        result
            .improvement_suggestions
            .push("Analysis lacks key dimensions - consider more diverse sources".to_string());
    }

    let consistency = score_consistency(ctx);
    result.consistency_score = consistency;
    if consistency < 0.4 {
        result
            .improvement_suggestions
            .push("AI analysis contains contradictions - cross-validate claims".to_string());
    }

    let evidence = score_evidence(ctx);
    result.evidence_score = evidence;
    if evidence < 0.4 {
        result
            .improvement_suggestions
            .push("Insufficient evidence support - add more patent/web references".to_string());
    }

    result.quality_score =
        (completeness * 0.35 + consistency * 0.35 + evidence * 0.30).clamp(0.0, 1.0);

    result.needs_retry = result.quality_score < RETRY_THRESHOLD;
    if result.needs_retry {
        result.retry_reason = Some(format!(
            "Quality score {:.2} below threshold {:.2} (completeness={:.2}, consistency={:.2}, evidence={:.2})",
            result.quality_score, RETRY_THRESHOLD, completeness, consistency, evidence
        ));
    }
    result
}

fn score_completeness(ctx: &PipelineContext) -> f64 {
    let mut filled = 0.0;
    let mut total = 0.0;

    if !ctx.ai_analysis.is_empty() && ctx.ai_analysis.chars().count() > 50 {
        filled += 1.0;
    }
    total += 1.0;

    if !ctx.deep_reasoning.dimensions.is_empty() {
        filled += (ctx.deep_reasoning.dimensions.len() as f64 / 6.0).clamp(0.0, 1.0);
    }
    total += 1.0;

    if !ctx.action_plan.is_empty() && ctx.action_plan.chars().count() > 30 {
        filled += 1.0;
    }
    total += 1.0;

    if ctx.novelty_score > 0.0 && ctx.novelty_score < 100.0 {
        filled += 0.5;
    }
    total += 0.5;

    if !ctx.contradictions.is_empty() {
        filled += 0.5;
    }
    total += 0.5;

    if total == 0.0 {
        return 0.0;
    }
    (filled / total).clamp(0.0, 1.0)
}

fn score_consistency(ctx: &PipelineContext) -> f64 {
    let mut score = 1.0;

    for c in &ctx.contradictions {
        if c.signal_strength > 0.7 {
            score -= 0.2;
        } else if c.signal_strength > 0.3 {
            score -= 0.1;
        }
    }

    if ctx.novelty_score < 20.0
        && !ctx.ai_analysis.is_empty()
        && (ctx.ai_analysis.contains("高度新颖") || ctx.ai_analysis.contains("highly novel"))
    {
        score -= 0.3;
    }
    if ctx.novelty_score > 80.0
        && !ctx.ai_analysis.is_empty()
        && (ctx.ai_analysis.contains("新颖性很低") || ctx.ai_analysis.contains("very low novelty"))
    {
        score -= 0.3;
    }

    for ao in &ctx.agent_outputs {
        if ao.uncertainty.len() > 5 {
            score -= 0.1 * (ao.uncertainty.len() as f64 - 5.0);
        }
    }
    score.max(0.0)
}

fn score_evidence(ctx: &PipelineContext) -> f64 {
    let mut score = 0.0;
    let mut total = 1.0;

    if !ctx.evidence_chain.is_empty() {
        let avg = ctx.evidence_chain.iter().map(|e| e.confidence).sum::<f64>()
            / ctx.evidence_chain.len() as f64;
        score += avg.min(1.0);
    }
    total += 1.0;

    if !ctx.top_matches.is_empty() {
        score += (ctx.top_matches.len() as f64 / 10.0).min(1.0) * 0.8;
    }
    total += 0.8;

    if !ctx.prior_art_clusters.is_empty() {
        score += 0.5;
    }
    total += 0.5;

    score += ctx.diversity_score.min(1.0) * 0.5;
    total += 0.5;

    if total == 0.0 {
        return 0.0;
    }
    (score / total).clamp(0.0, 1.0)
}

pub fn should_retry(ctx: &PipelineContext, step: &crate::pipeline::state::PipelineStep) -> bool {
    if ctx.retry_count >= MAX_RETRIES {
        return false;
    }
    if step.step_type() != crate::pipeline::state::StepType::Llm {
        return false;
    }
    if let Some(refr) = ctx.reflection_history.last() {
        if refr.evaluated_step == format!("{:?}", step) {
            return refr.needs_retry;
        }
    }
    false
}

pub fn record_agent_output(
    ctx: &mut PipelineContext,
    step: &crate::pipeline::state::PipelineStep,
    content: &str,
) -> AgentOutput {
    let output = AgentOutput {
        content: content.to_string(),
        confidence: estimate_confidence(ctx, content),
        evidence_refs: extract_evidence_refs(ctx),
        uncertainty: detect_uncertainty(content),
        self_reflection: None,
        duration_ms: 0,
        tokens_used: 0,
        source_step: format!("{:?}", step),
    };
    ctx.agent_outputs.push(output.clone());
    output
}

fn estimate_confidence(ctx: &PipelineContext, content: &str) -> f64 {
    let mut conf = 0.5;
    conf += (content.chars().count() as f64 / 500.0).min(0.3);
    if !ctx.top_matches.is_empty() {
        conf += 0.1;
    }
    if content.chars().any(|c| c.is_ascii_digit()) {
        conf += 0.05;
    }
    if content.contains("可能") || content.contains("也许") || content.contains("might") {
        conf -= 0.1;
    }
    conf.clamp(0.1, 0.95)
}

fn extract_evidence_refs(ctx: &PipelineContext) -> Vec<String> {
    let mut refs = Vec::new();
    for e in &ctx.evidence_chain {
        refs.push(e.id.clone());
    }
    refs.truncate(20);
    refs
}

fn detect_uncertainty(content: &str) -> Vec<String> {
    let mut uncertainties = Vec::new();
    let markers = [
        "可能",
        "也许",
        "尚不确定",
        "需要进一步验证",
        "有待商榷",
        "不确定",
        "perhaps",
        "uncertain",
    ];
    for marker in &markers {
        if content.contains(marker) {
            uncertainties.push(format!("Hedging: '{}' ", marker));
            break;
        }
    }
    if content.contains("局限性") || content.contains("limitations") {
        uncertainties.push("Limitations mentioned".to_string());
    }
    if content.contains("数据不足")
        || content.contains("证据不足")
        || content.contains("insufficient data")
    {
        uncertainties.push("Insufficient data warning".to_string());
    }
    uncertainties
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::context::Contradiction;
    use crate::pipeline::state::PipelineStep;

    #[test]
    fn empty_context_low_score() {
        let ctx = PipelineContext::new("1", "test", "test");
        assert!((evaluate(&ctx).quality_score) < 0.7);
    }

    #[test]
    fn filled_context_higher_score() {
        let mut ctx = PipelineContext::new("1", "test", "test");
        ctx.ai_analysis =
            "This is a comprehensive analysis covering all aspects of the technology".to_string();
        ctx.action_plan = "Implement in phases".to_string();
        ctx.novelty_score = 75.0;
        ctx.diversity_score = 0.8;
        assert!((evaluate(&ctx).quality_score) > 0.5);
    }

    #[test]
    fn contradictions_lower_consistency() {
        let mut ctx = PipelineContext::new("1", "test", "test");
        ctx.contradictions.push(Contradiction {
            source_a: "A".into(),
            source_b: "B".into(),
            dimension: "Test".into(),
            signal_strength: 0.9,
            opportunity: "High".into(),
        });
        assert!((evaluate(&ctx).consistency_score) < 1.0);
    }

    #[test]
    fn should_retry_max_retries() {
        let mut ctx = PipelineContext::new("1", "test", "test");
        ctx.retry_count = MAX_RETRIES;
        assert!(!should_retry(&ctx, &PipelineStep::AiDeepAnalysis));
    }

    #[test]
    fn record_output_detects_uncertainty() {
        let mut ctx = PipelineContext::new("1", "test", "test");
        let content = "结果可能还需要进一步验证，数据不足的情况下结论有待商榷。".to_string();
        let output = record_agent_output(&mut ctx, &PipelineStep::AiDeepAnalysis, &content);
        assert!(!output.uncertainty.is_empty());
    }
}
