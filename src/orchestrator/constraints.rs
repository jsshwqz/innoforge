//! 模块职责：ResearchState 跨轮次负向约束闭环与限次自动补搜决策引擎（v0.9.68）。
//! 上游调用者：`src/orchestrator/engine.rs`、`src/pipeline/steps/{expand,deep_reasoning,analysis}.rs`。
//! 模块专属禁区：自动补搜严格硬控上限 `MAX_AUTO_REFILL_ROUNDS = 2` 并受预算熔断保护；负向约束违规必须产生实质拦截警示，严禁仅打 warn 日志。

use crate::pipeline::context::{PipelineContext, ResearchState};
use crate::pipeline::state::PipelineStep;

/// 限次自动补搜硬上限（v0.9.68 规划红线：最多 2 轮，严禁无限循环）。
pub const MAX_AUTO_REFILL_ROUNDS: u32 = 2;

/// 合并数据库持久化的 `ResearchState` 到当前 `PipelineContext`，彻底打通“只写不读”断链。
pub fn merge_persisted_research_state(ctx: &mut PipelineContext, persisted: ResearchState) {
    if ctx.research_state.current_hypothesis.trim().is_empty()
        && !persisted.current_hypothesis.trim().is_empty()
    {
        ctx.research_state.current_hypothesis = persisted.current_hypothesis;
    }
    for path in persisted.excluded_paths {
        let p = path.trim().to_string();
        if !p.is_empty() && !ctx.research_state.excluded_paths.contains(&p) {
            ctx.research_state.excluded_paths.push(p);
        }
    }
    for q in persisted.open_questions {
        let q = q.trim().to_string();
        if !q.is_empty() && !ctx.research_state.open_questions.contains(&q) {
            ctx.research_state.open_questions.push(q);
        }
    }
    for claim in persisted.verified_claims {
        let c = claim.trim().to_string();
        if !c.is_empty() && !ctx.research_state.verified_claims.contains(&c) {
            ctx.research_state.verified_claims.push(c);
        }
    }
}

/// 构造注入 LLM Prompt 的跨轮次负向硬约束段落（无已排除路径时返回空串）。
pub fn format_excluded_paths_prompt_block(state: &ResearchState) -> String {
    let valid_paths: Vec<&str> = state
        .excluded_paths
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if valid_paths.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\n## ⛔ 跨轮次负向硬约束：已证伪/排除的技术路径（严禁重走）\n\
         以下路线已在前期检索、实验或发明人评审中被排除，本轮禁止再次推荐、扩展或作为新方案提出：\n",
    );
    for (i, p) in valid_paths.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", i + 1, p));
    }
    out
}

/// 从已排除路径条目中抽取用于后置校验的显著核心词元（去除纯虚词与极短字符）。
fn extract_path_signature_tokens(path: &str) -> Vec<String> {
    let stopwords = [
        "该方向",
        "技术路线",
        "已有技术",
        "现有技术",
        "高度重叠",
        "建议调整",
        "已经排除",
        "方案",
        "结构",
        "装置",
        "系统",
        "方法",
        "采用",
        "使用",
    ];
    let mut tokens: Vec<String> = path
        .split(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    '，' | '。'
                        | '；'
                        | '：'
                        | '、'
                        | ','
                        | ';'
                        | ':'
                        | '('
                        | ')'
                        | '（'
                        | '）'
                        | '-'
                        | '/'
                )
        })
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| t.chars().count() >= 3 && !stopwords.contains(&t.as_str()))
        .collect();
    tokens.sort();
    tokens.dedup();
    tokens
}

/// 确定性后置校验器：检测 AI 输出是否正面复用了已排除的技术路径（排除明确写着“避免/放弃/排除/切勿”的否定语境）。
pub fn detect_excluded_path_violations(
    excluded_paths: &[String],
    output_text: &str,
) -> Vec<String> {
    if excluded_paths.is_empty() || output_text.trim().is_empty() {
        return Vec::new();
    }
    let negation_cues = [
        "避免",
        "排除",
        "放弃",
        "不再采用",
        "切勿",
        "禁止",
        "已证伪",
        "不可行",
        "淘汰",
        "弃用",
        "avoid",
        "exclude",
        "deprecated",
        "rejected",
    ];
    let sentences: Vec<&str> = output_text
        .split(['\n', '。', '；', '!', '！'])
        .map(str::trim)
        .filter(|s| s.chars().count() >= 4)
        .collect();

    let mut violations = Vec::new();
    for path in excluded_paths {
        let p_trim = path.trim();
        if p_trim.chars().count() < 3 {
            continue;
        }
        let tokens = extract_path_signature_tokens(p_trim);
        if tokens.is_empty() {
            continue;
        }
        let violated = sentences.iter().any(|sent| {
            let s_lower = sent.to_ascii_lowercase();
            // 若该句本身处于明确否定/排斥语境，或属于本拦截横幅自身，不判违规
            if sent.contains("负向约束拦截警示")
                || negation_cues.iter().any(|cue| s_lower.contains(cue))
            {
                return false;
            }
            if s_lower.contains(&p_trim.to_ascii_lowercase()) {
                return true;
            }
            // 若路径包含多个关键技术词元，至少命中 >=2 个或全部词元才算复发
            let required = if tokens.len() >= 2 { 2 } else { 1 };
            let hits = tokens
                .iter()
                .filter(|t| s_lower.contains(t.as_str()))
                .count();
            hits >= required
        });
        if violated {
            violations.push(p_trim.to_string());
        }
    }
    violations
}

/// 对 `ctx.ai_analysis` 与 `ctx.action_plan` 执行实质性负向约束拦截（绝不仅仅打 warn 日志）。
pub fn enforce_excluded_path_guard(ctx: &mut PipelineContext) -> Vec<String> {
    let mut combined_violations =
        detect_excluded_path_violations(&ctx.research_state.excluded_paths, &ctx.ai_analysis);
    for v in detect_excluded_path_violations(&ctx.research_state.excluded_paths, &ctx.action_plan) {
        if !combined_violations.contains(&v) {
            combined_violations.push(v);
        }
    }
    if combined_violations.is_empty() {
        return combined_violations;
    }

    let banner = format!(
        "> ⛔ **【跨轮次负向约束拦截警示】**：检测到本轮分析/行动方案重新提及了已排除的技术路径（{}），\
         系统已强制标注拦截，请剔除相关旧方案并转向未证伪路线。\n\n",
        combined_violations.join("；")
    );
    if !ctx.ai_analysis.is_empty() && !ctx.ai_analysis.contains("跨轮次负向约束拦截警示")
    {
        ctx.ai_analysis = format!("{banner}{}", ctx.ai_analysis);
    }
    if !ctx.action_plan.is_empty() && !ctx.action_plan.contains("跨轮次负向约束拦截警示")
    {
        ctx.action_plan = format!("{banner}{}", ctx.action_plan);
    }
    let q = format!(
        "请验证替代路线以彻底规避已排除路径：{}",
        combined_violations.join("、")
    );
    if !ctx.research_state.open_questions.contains(&q) {
        ctx.research_state.open_questions.push(q);
    }
    combined_violations
}

/// 从 `open_questions` 与 `deep_reasoning.blind_spots` 中派生针对性补搜子查询（去重并排除已搜查询）。
pub fn derive_open_question_refill_queries(ctx: &PipelineContext) -> Vec<String> {
    let existing_lower: Vec<String> = ctx
        .expanded_queries
        .iter()
        .map(|q| q.trim().to_ascii_lowercase())
        .collect();
    let mut candidates: Vec<String> = Vec::new();

    for q in &ctx.research_state.open_questions {
        let cleaned = q
            .trim()
            .trim_start_matches(['-', '*', '•', ' '])
            .chars()
            .take(64)
            .collect::<String>();
        if cleaned.chars().count() >= 4 {
            candidates.push(cleaned);
        }
    }

    for b in &ctx.deep_reasoning.blind_spots {
        let cleaned = b
            .trim()
            .trim_start_matches(['-', '*', '•', ' '])
            .chars()
            .take(64)
            .collect::<String>();
        if cleaned.chars().count() >= 6 {
            candidates.push(cleaned);
        }
    }

    let mut out = Vec::new();
    for cand in candidates {
        let low = cand.to_ascii_lowercase();
        if existing_lower.iter().any(|e| e == &low || e.contains(&low)) {
            continue;
        }
        if !out.iter().any(|x: &String| x.to_ascii_lowercase() == low) {
            out.push(cand);
            if out.len() >= 3 {
                break;
            }
        }
    }
    out
}

/// 限次自动补搜触发判据（纯函数，离线可测）：
/// - 仅在 `AiDeepAnalysis` 完成后、非快速模式下生效；
/// - 严格硬控 `auto_refill_rounds < max_rounds`（默认上限 2 轮）；
/// - 必须通过预算硬熔断校验（`budget_ok == true`）；
/// - 必须存在非空的新派生补搜子查询（避免原地空转）。
pub fn should_trigger_auto_refill(
    quick_mode: bool,
    current: PipelineStep,
    auto_refill_rounds: u32,
    max_rounds: u32,
    budget_ok: bool,
    derived_queries: &[String],
) -> bool {
    !quick_mode
        && current == PipelineStep::AiDeepAnalysis
        && auto_refill_rounds < max_rounds
        && budget_ok
        && !derived_queries.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_persisted_state_hydrates_excluded_paths_without_duplicates() {
        let mut ctx = PipelineContext::new("i1", "t", "d");
        ctx.research_state
            .excluded_paths
            .push("传统风冷翅片".into());
        let persisted = ResearchState {
            current_hypothesis: "微负压浸没冷却".into(),
            excluded_paths: vec!["传统风冷翅片".into(), "密封水套流道".into()],
            open_questions: vec!["硅油与绕组漆膜1000h相容性".into()],
            verified_claims: vec!["复用原润滑油口".into()],
        };
        merge_persisted_research_state(&mut ctx, persisted);
        assert_eq!(ctx.research_state.excluded_paths.len(), 2);
        assert!(ctx
            .research_state
            .excluded_paths
            .contains(&"密封水套流道".to_string()));
        assert_eq!(ctx.research_state.current_hypothesis, "微负压浸没冷却");
        let block = format_excluded_paths_prompt_block(&ctx.research_state);
        assert!(block.contains("密封水套流道") && block.contains("严禁重走"));
    }

    /// 负面反作弊测试：当 AI 重新推荐已被排除的路径时，后置拦截器必须实质注入警示横幅；
    /// 而当文本是在明确拒绝该路径时不得误报。
    #[test]
    fn anti_cheat_excluded_path_guard_intercepts_recycled_dead_end_and_allows_negation() {
        let mut ctx = PipelineContext::new("i2", "电机冷却", "描述");
        ctx.research_state
            .excluded_paths
            .push("外部密封水套流道".into());

        // 明确否定语境 → 不误报
        ctx.ai_analysis = "本方案明确放弃外部密封水套流道，改用直接浸没换热。".into();
        let clean = enforce_excluded_path_guard(&mut ctx);
        assert!(clean.is_empty(), "否定语境不应误判违规: {:?}", clean);

        // 正面复发推荐已排除路径 → 必须实质拦截并置顶警示横幅
        ctx.ai_analysis = "建议在电机壳体增加外部密封水套流道以提升换热面积。".into();
        let violated = enforce_excluded_path_guard(&mut ctx);
        assert_eq!(violated, vec!["外部密封水套流道".to_string()]);
        assert!(
            ctx.ai_analysis.contains("跨轮次负向约束拦截警示"),
            "必须实质写入拦截横幅"
        );
    }

    /// 负面反作弊测试：限次自动补搜在达到上限 2 轮或预算熔断时必须硬停止，绝不无限循环。
    #[test]
    fn anti_cheat_auto_refill_hard_stops_at_max_2_rounds_or_budget_exhaustion() {
        let q = vec!["硅油 绝缘漆 相容性 测试".to_string()];
        // 第 0 轮 & 第 1 轮 允许触发
        assert!(should_trigger_auto_refill(
            false,
            PipelineStep::AiDeepAnalysis,
            0,
            MAX_AUTO_REFILL_ROUNDS,
            true,
            &q
        ));
        assert!(should_trigger_auto_refill(
            false,
            PipelineStep::AiDeepAnalysis,
            1,
            MAX_AUTO_REFILL_ROUNDS,
            true,
            &q
        ));
        // 第 2 轮（已达上限 2）→ 必须拦截！
        assert!(!should_trigger_auto_refill(
            false,
            PipelineStep::AiDeepAnalysis,
            2,
            MAX_AUTO_REFILL_ROUNDS,
            true,
            &q
        ));
        // 预算熔断 → 必须拦截！
        assert!(!should_trigger_auto_refill(
            false,
            PipelineStep::AiDeepAnalysis,
            0,
            MAX_AUTO_REFILL_ROUNDS,
            false,
            &q
        ));
        // 快速模式 → 必须拦截！
        assert!(!should_trigger_auto_refill(
            true,
            PipelineStep::AiDeepAnalysis,
            0,
            MAX_AUTO_REFILL_ROUNDS,
            true,
            &q
        ));
    }
}
