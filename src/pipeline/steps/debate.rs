use crate::pipeline::context::{DebateResult, PipelineContext};

pub fn run_debate(ctx: &PipelineContext) -> DebateResult {
    let mut result = DebateResult {
        conclusion: String::new(),
        perspectives: Vec::new(),
        consensus: Vec::new(),
        divergences: Vec::new(),
        recommendation: String::new(),
    };

    if ctx.contradictions.is_empty() && ctx.reflection_history.is_empty() {
        result.conclusion = "No contradictions or reflections to synthesize".to_string();
        result.recommendation = "Proceed with current analysis".to_string();
        return result;
    }

    for c in &ctx.contradictions {
        result.perspectives.push(format!(
            "Contradiction [{}]: {} vs {} ({:.0}% signal)",
            c.dimension,
            c.source_a,
            c.source_b,
            c.signal_strength * 100.0
        ));
    }

    for r in &ctx.reflection_history {
        if r.needs_retry {
            result.perspectives.push(format!(
                "Reflection [{}]: {} - needs retry",
                r.evaluated_step,
                r.retry_reason.as_deref().unwrap_or("quality too low")
            ));
        }
        for suggestion in &r.improvement_suggestions {
            result
                .perspectives
                .push(format!("Suggestion [{}]: {}", r.evaluated_step, suggestion));
        }
    }

    if ctx.novelty_score > 60.0 && ctx.top_matches.iter().all(|m| m.combined_score < 0.5) {
        result
            .consensus
            .push("All evidence sources agree: this idea has significant novelty".to_string());
    }
    if ctx.novelty_score < 30.0 {
        result.consensus.push(
            "Existing patents show significant overlap - consider differentiation strategy"
                .to_string(),
        );
    }
    if !ctx.contradictions.is_empty() {
        let high = ctx
            .contradictions
            .iter()
            .filter(|c| c.signal_strength > 0.6)
            .count();
        if high > 0 {
            result
                .consensus
                .push(format!("{} high-signal contradictions detected", high));
        }
    }

    for c in &ctx.contradictions {
        if c.signal_strength > 0.4 {
            result.divergences.push(format!(
                "[{}] {} vs {}: {}",
                c.dimension, c.source_a, c.source_b, c.opportunity
            ));
        }
    }

    if !ctx.ai_analysis.is_empty()
        && ctx.novelty_score < 30.0
        && (ctx.ai_analysis.contains("高度新颖") || ctx.ai_analysis.contains("highly novel"))
    {
        result
            .divergences
            .push("AI claims high novelty but scoring is low".to_string());
    }
    if !ctx.ai_analysis.is_empty()
        && ctx.novelty_score > 70.0
        && (ctx.ai_analysis.contains("新颖性很低") || ctx.ai_analysis.contains("very low novelty"))
    {
        result
            .divergences
            .push("AI claims low novelty but scoring is high".to_string());
    }

    result.conclusion = synthesize_conclusion(&result, ctx);
    result.recommendation = make_recommendation(&result, ctx);
    result
}

fn synthesize_conclusion(result: &DebateResult, ctx: &PipelineContext) -> String {
    let mut parts = Vec::new();
    parts.push(format!(
        "综合评估 / Overall: novelty score {:.0}/100",
        ctx.novelty_score
    ));
    if !result.consensus.is_empty() {
        parts.push(format!(
            "共识点 / Consensus: {}",
            result.consensus.join("; ")
        ));
    }
    if !result.divergences.is_empty() {
        parts.push(format!(
            "分歧点 / Divergences: {}",
            result.divergences.join("; ")
        ));
    }
    if !ctx.contradictions.is_empty() {
        parts.push(format!(
            "{} contradictions detected",
            ctx.contradictions.len()
        ));
    }
    if !parts.is_empty() {
        parts.join("\n\n")
    } else {
        "Analysis complete".to_string()
    }
}

fn make_recommendation(result: &DebateResult, ctx: &PipelineContext) -> String {
    let has_div = !result.divergences.is_empty();
    let has_contra = !ctx.contradictions.is_empty();
    let low = ctx.novelty_score < 30.0;
    let high = ctx.novelty_score > 70.0;

    if low && has_contra {
        return "Suggest: redefine from contradiction dimensions".to_string();
    }
    if low {
        return "Suggest: increase differentiation from prior art".to_string();
    }
    if high && has_div {
        return "Suggest: cross-validate key conclusions".to_string();
    }
    if high {
        return "Suggest: proceed to claim tree construction".to_string();
    }
    "Suggest: proceed to next step".to_string()
}

#[cfg(test)]
mod tests {
    use crate::pipeline::context::{Contradiction, PipelineContext};

    #[test]
    fn empty_context_no_debate() {
        let ctx = PipelineContext::new("1", "test", "test");
        let result = super::run_debate(&ctx);
        assert!(result.conclusion.contains("No contradictions"));
    }

    #[test]
    fn contradictions_produce_synthesis() {
        let mut ctx = PipelineContext::new("1", "test", "test");
        ctx.novelty_score = 50.0;
        ctx.contradictions.push(Contradiction {
            source_a: "Method A".to_string(),
            source_b: "Method B".to_string(),
            dimension: "Power Supply".to_string(),
            signal_strength: 0.8,
            opportunity: "Hybrid".to_string(),
        });
        let result = super::run_debate(&ctx);
        assert!(!result.divergences.is_empty());
        assert!(!result.recommendation.is_empty());
    }

    #[test]
    fn high_novelty_gets_good_recommendation() {
        let mut ctx = PipelineContext {
            novelty_score: 80.0,
            contradictions: Vec::new(),
            reflection_history: Vec::new(),
            ..PipelineContext::new("1", "test", "test")
        };
        ctx.contradictions.push(Contradiction {
            source_a: "A".to_string(),
            source_b: "B".to_string(),
            dimension: "Test".to_string(),
            signal_strength: 0.3,
            opportunity: "Minor".to_string(),
        });
        let result = super::run_debate(&ctx);
        assert!(result.recommendation.contains("claim tree"));
    }
}
