//! 多轮对抗推演引擎
//!
//! 审查员与代理师多轮对抗，在压力测试中打磨出经得起审查的答复。
//! 每轮：审查员质疑 → 代理师回应。最后整合输出终稿。

use serde::{Deserialize, Serialize};

/// 对抗推演输入参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdversarialParams {
    pub my_patent: String,
    pub claims: String,
    pub oa_text: String,
    pub refs: String,
    pub rounds: usize,
    pub focus: String,
    /// 目标信心度阈值，审查员评估达到此值且无 high 质疑则达标停止
    pub goal_threshold: f32,
}

/// 审查员一轮质疑
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExaminerTurn {
    pub round: usize,
    pub content: String,
    pub challenges: Vec<String>,
    pub severity: String,
    pub assessment: f32,
}

/// 代理师一轮回应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurn {
    pub round: usize,
    pub content: String,
    pub arguments: Vec<String>,
    pub evidence_used: Vec<String>,
    pub confidence: f32,
    pub modification: Option<String>,
}

/// 一轮对抗记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundRecord {
    pub round: usize,
    pub examiner: ExaminerTurn,
    pub agent: AgentTurn,
    pub summary: String,
}

/// 对抗推演最终结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdversarialResult {
    pub transcript: Vec<RoundRecord>,
    pub final_response: String,
    pub confidence_score: f32,
    pub weak_points: Vec<String>,
    pub rounds_completed: usize,
    pub converged: bool,
    /// 目标是否达成（审查员评估 >= 阈值且无 high 质疑）
    pub goal_achieved: bool,
    /// 使用的目标阈值
    pub goal_threshold: f32,
}

/// 审查员 system prompt —— 超人类审查员：精通法理+策略预判+逐轮升级
fn examiner_system_prompt(
    round: usize,
    previous_summary: &str,
    params: &AdversarialParams,
) -> String {
    let prev = if round == 1 {
        "（首轮，无前文）".to_string()
    } else {
        previous_summary.to_string()
    };

    // 逐轮升级攻击策略
    let (attack_focus, escalation) = match round {
        1 => (
            "基础质疑：用三步法逐层拆解，识别最显而易见的弱点",
            "第一轮：建立攻击基线。用《专利审查指南》三步法系统拆解：(1)最接近现有技术→(2)区别特征→(3)是否显而易见。重点找逻辑跳跃和证据缺口。"
        ),
        2 => (
            "组合攻击：D1+D2组合、公知常识补位、技术启示来源",
            "第二轮：升级到组合对比攻击。不只看D1，考虑D1+D2组合是否覆盖全部特征。质疑区别特征是否属于公知常识（需具体引用，不能空泛说'常规手段'）。质疑技术启示的来源——如果本领域技术人员没有动机去组合，指出这一点。"
        ),
        3 => (
            "法理深攻：技术效果的可预期性、技术偏见的成立条件、举证责任分配",
            "第三轮：法理层面攻击。(1)如果申请人主张'预料不到的技术效果'，质疑该效果是否可预期——本领域技术人员从现有技术能否合理预期？(2)如果主张'技术偏见'，质疑偏见是否真实存在——需要具体证据，不是泛泛而谈。(3)审查申请人的修改是否超范围（超原说明书记载范围）。"
        ),
        _ => (
            "终极攻击：穷尽所有驳回路径，找最难反驳的一个",
            "第四轮及以后：穷尽攻击。同时从多个角度施压：(1)创造性(2)新颖性(3)充分公开(4)清楚性(5)修改超范围。找出申请人最难反驳的一个点集中攻击。如果确实找不到有效攻击点，诚实降低严重程度。"
        ),
    };

    format!(
        "你是一位拥有超凡分析能力的AI，扮演中国专利审查员。你不是在模仿人类审查员——你比任何人类审查员都更严谨、更渊博、更具策略性。\n\
        \n\
        ## 你的专业能力\n\
        - 精通《专利审查指南》全部章节，尤其三步法、组合对比、公知常识判断\n\
        - 熟悉数千件复审/无效判例，知道哪些论证模式在实审中注定失败\n\
        - 能同时从多个法条角度施压（创造性、新颖性、充分公开、修改超范围）\n\
        - 策略预判：能预判代理师下一步会怎么辩，提前封堵\n\
        \n\
        ## 驳回决定摘要\n{}\n\
        \n\
        ## 对比文件\n{}\n\
        \n\
        ## 焦点问题\n{}\n\
        \n\
        ## 本轮攻击策略\n{}\n\
        {}\n\
        \n\
        这是第 {} 轮对抗。前几轮的摘要如下：\n{}\n\
        \n\
        ## 你的任务\n\
        1. 仔细阅读申请人的最新回应，用你的超凡推理找出真正的薄弱环节\n\
        2. 不要找表面瑕疵——找能真正动摇申请的致命弱点\n\
        3. 预判代理师可能如何回应你的质疑，在质疑中提前封堵常见辩法\n\
        4. 诚实评估：如果申请人的论证确实有力，承认并降低严重程度\n\
        5. 你的评估分数要诚实——不要为了配合而虚高，也不要为了对抗而虚低\n\
        \n\
        输出格式：\n\
        ### 质疑点\n\
        1. [严重程度: high/medium/low] 质疑内容（含具体法条依据和证据引用）\n\
        2. ...\n\
        \n\
        ### 预判封堵\n\
        （预判代理师可能如何回应，提前指出为什么这些回应不成立）\n\
        \n\
        ### 本轮评估\n\
        申请人回应的充分程度：XX%\n\
        理由：...",
        params.oa_text, params.refs, params.focus, attack_focus, escalation, round, prev
    )
}

/// 代理师 system prompt —— 超人类代理师：精通成功模式+预判布防+逐轮升级
fn agent_system_prompt(
    round: usize,
    previous_summary: &str,
    examiner_content: &str,
    params: &AdversarialParams,
) -> String {
    let prev = if round == 1 {
        "（首轮，无前文）".to_string()
    } else {
        previous_summary.to_string()
    };

    // 逐轮升级防御策略
    let (defense_focus, escalation) = match round {
        1 => (
            "基础回应：逐一回应质疑，建立区别特征→技术效果的基本论证链",
            "第一轮：建立防御基线。对每个质疑构建论证链：区别特征→技术作用→技术效果→非显而易见性。引用说明书具体段落作为证据。"
        ),
        2 => (
            "深度防御：多角度论证+预判审查员下一步攻击+提前布防",
            "第二轮：升级到多角度防御。(1)不只论证'不同'，论证'不同带来的不可预期的效果'。(2)用技术偏见论证——证明现有技术有教离本发明的倾向。(3)用辅证责任——指出审查员主张公知常识需举证。(4)预判审查员下一步可能从哪个角度攻击，提前加固那个方向。"
        ),
        3 => (
            "体系性论证：构建完整论证体系，让单个质疑无法动摇整体",
            "第三轮：构建体系性论证。(1)将所有论证点编织成互相支撑的体系——即使单个点被攻破，整体仍成立。(2)引用具体判例支持（如'在XXX复审案中，合议组认定类似区别特征构成创造性'）。(3)如果审查员的质疑确实有力，诚实承认并提出权利要求修改——修改要精准，既克服质疑又不丢保护范围。"
        ),
        _ => (
            "终极防御：穷尽一切可能的成功路径，诚实评估胜算",
            "第四轮及以后：穷尽防御。(1)尝试所有可能的论证路径：技术效果法、技术偏见法、商业成功法、长期未解决的技术问题法、辅证责任转移法。(2)如果某条路走不通，诚实放弃，不硬撑。(3)如果确实无法克服某个质疑，提出最精准的权利要求修改。(4)诚实评估：这个案子到底能不能过——如果不能，告诉申请人真相。"
        ),
    };

    format!(
        "你是一位拥有超凡分析能力的AI，扮演资深专利代理师。你不是在模仿人类代理师——你比任何人类代理师都更渊博、更具策略性、更诚实。\n\
        \n\
        ## 你的专业能力\n\
        - 精通全部成功答复模式：技术效果法、技术偏见法、举证责任转移、长期未解决需求法\n\
        - 熟悉数千件授权判例，知道哪些论证模式能说服审查员\n\
        - 策略预判：能预判审查员下一步会从哪个角度攻击，提前布防\n\
        - 诚实：不会为了'赢'而硬撑站不住的论证——该修改就修改，该放弃就放弃\n\
        \n\
        ## 本专利\n{}\n\
        \n\
        ## 权利要求\n{}\n\
        \n\
        ## 驳回决定\n{}\n\
        \n\
        ## 对比文件\n{}\n\
        \n\
        ## 本轮防御策略\n{}\n\
        {}\n\
        \n\
        这是第 {} 轮对抗。审查员提出了以下质疑：\n{}\n\
        \n\
        前几轮的摘要如下：\n{}\n\
        \n\
        ## 你的任务\n\
        1. 用你的超凡推理，针对审查员的每个质疑构建真正严密的论证\n\
        2. 不要用套话——'本领域技术人员不难想到'这种空话审查员一眼就能看穿\n\
        3. 每个论点都要有具体证据支撑：说明书段落编号、对比文件缺失内容、技术原理推导\n\
        4. 预判审查员可能如何反驳你的回应，提前加固薄弱处\n\
        5. 如果某个质疑确实无法反驳，诚实承认并提出权利要求修改\n\
        6. 信心度要诚实——不是你觉得论证多漂亮，是你真的觉得能说服审查员的概率\n\
        \n\
        输出格式：\n\
        ### 逐个回应质疑点\n\
        （每个质疑点对应一条论证，含具体证据和法条依据）\n\
        \n\
        ### 预判加固\n\
        （预判审查员下一步攻击方向，提前加固）\n\
        \n\
        ### 修改建议（如有）\n\
        ...\n\
        \n\
        ### 本轮信心度\n\
        XX%\n\
        理由：...",
        params.my_patent, params.claims, params.oa_text, params.refs, defense_focus, escalation, round, examiner_content, prev
    )
}

/// 摘要 prompt
fn summary_prompt(examiner: &ExaminerTurn, agent: &AgentTurn) -> String {
    format!(
        "请将以下对抗轮次压缩为 200 字以内的摘要，保留关键质疑点和论证要点：\n\
        \n\
        审查员质疑：{}\n\
        代理师回应：{}",
        examiner.content, agent.content
    )
}

/// 终稿整合 prompt
fn synthesis_prompt(transcript: &[RoundRecord], params: &AdversarialParams) -> String {
    let mut dialog = String::new();
    for r in transcript {
        dialog.push_str(&format!(
            "\n--- 第 {} 轮 ---\n审查员：{}\n代理师：{}\n",
            r.round, r.examiner.content, r.agent.content
        ));
    }

    format!(
        "你是资深专利代理师。经过 {} 轮与审查员的对抗推演，以下是完整对话记录：\n{}\n\
        \n\
        ## 原始材料\n\
        专利：{}\n\
        权利要求：{}\n\
        驳回决定：{}\n\
        对比文件：{}\n\
        \n\
        请整合所有轮次的论证，撰写一份完整的、可以直接提交的答复意见书。\n\
        \n\
        要求：\n\
        1. 格式符合正式答复意见书规范\n\
        2. 论证逻辑连贯，不重复\n\
        3. 对审查员提出的所有质疑都已回应\n\
        4. 标注仍存在的风险点（诚实披露）\n\
        5. 按可直接提交成稿生成，提交前请人工核对事实与程序信息",
        transcript.len(),
        dialog,
        params.my_patent,
        params.claims,
        params.oa_text,
        params.refs
    )
}

/// 解析审查员输出
fn parse_examiner_turn(round: usize, raw: &str) -> ExaminerTurn {
    // 提取质疑点：跳过标题行，取实际质疑内容
    let challenges: Vec<String> = raw
        .lines()
        .filter(|l| {
            let t = l.trim();
            // 跳过标题行和空行
            !t.starts_with("###")
                && !t.starts_with("#")
                && !t.is_empty()
                && (t.contains("[") || t.starts_with(|c: char| c.is_ascii_digit()))
        })
        .map(|l| l.trim().to_string())
        .take(5)
        .collect();

    // 提取严重程度：优先看 high/medium/low 标记
    let severity = if raw.contains("high") {
        "high"
    } else if raw.contains("medium") {
        "medium"
    } else if raw.contains("low") {
        "low"
    } else if raw.contains("严重") {
        "high"
    } else if raw.contains("中等") {
        "medium"
    } else {
        "low"
    };

    // 提取评估百分比
    let assessment = extract_percentage(raw).unwrap_or(0.5);

    ExaminerTurn {
        round,
        content: raw.to_string(),
        challenges,
        severity: severity.to_string(),
        assessment,
    }
}

/// 解析代理师输出
fn parse_agent_turn(round: usize, raw: &str) -> AgentTurn {
    // 提取论点
    let arguments: Vec<String> = raw
        .lines()
        .filter(|l| {
            l.starts_with("###")
                || l.starts_with("- ")
                || l.starts_with("1.")
                || l.starts_with("2.")
        })
        .map(|l| l.trim().to_string())
        .take(5)
        .collect();

    // 提取证据
    let evidence_used: Vec<String> = raw
        .lines()
        .filter(|l| l.contains("证据") || l.contains("说明书") || l.contains("对比文件"))
        .map(|l| l.trim().to_string())
        .take(5)
        .collect();

    // 提取信心度
    let confidence = extract_percentage(raw).unwrap_or(0.5);

    // 提取修改建议
    let modification = if raw.contains("修改建议") || raw.contains("建议修改") {
        Some(
            raw.lines()
                .filter(|l| l.contains("修改") && !l.contains("###"))
                .map(|l| l.trim().to_string())
                .collect::<Vec<_>>()
                .join(" "),
        )
    } else {
        None
    };

    AgentTurn {
        round,
        content: raw.to_string(),
        arguments,
        evidence_used,
        confidence,
        modification,
    }
}

/// 从文本中提取百分比
fn extract_percentage(text: &str) -> Option<f32> {
    // 查找 "XX%" 模式
    // no rebind
    for part in text.split(|c: char| c.is_whitespace() || c == '：' || c == ':') {
        let part = part.trim();
        if let Some(stripped) = part.strip_suffix('%') {
            if let Ok(n) = stripped.parse::<f32>() {
                return Some((n / 100.0).clamp(0.0, 1.0));
            }
        }
        // 也匹配 "XX%" 紧贴文字
        if part.contains('%') {
            let pct: String = part
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(n) = pct.parse::<f32>() {
                if n > 0.0 {
                    return Some((n / 100.0).clamp(0.0, 1.0));
                }
            }
        }
    }
    None
}

/// 目标是否达成：审查员评估 >= 阈值 且无 high 严重质疑
fn goal_met(examiner: &ExaminerTurn, threshold: f32) -> bool {
    examiner.assessment >= threshold && examiner.severity != "high"
}

/// 执行多轮对抗推演
///
/// `ai` — 已配置好的 AI 客户端（通常用 ai_client_expert）
/// `params` — 对抗参数
pub async fn run_adversarial_deliberation(
    ai: &crate::ai::AiClient,
    params: &AdversarialParams,
) -> AdversarialResult {
    // 硬安全上限：用户指定轮数，但至少给 3 轮，最多 7 轮
    let max_rounds = params.rounds.clamp(3, 7);
    let goal_threshold = params.goal_threshold.clamp(0.5, 0.99);
    let mut transcript: Vec<RoundRecord> = Vec::new();
    let mut previous_summary = String::new();
    let mut converged = false;
    let mut rounds_completed = 0;
    let mut goal_achieved = false;

    let mut round = 0;
    while round < max_rounds {
        round += 1;
        // --- 审查员质疑 ---
        let examiner_sys = examiner_system_prompt(round, &previous_summary, params);

        let agent_prev = if round == 1 {
            "（首轮，申请人尚未提交答复，请基于驳回决定提出初始质疑）".to_string()
        } else {
            transcript
                .last()
                .map(|r| r.agent.content.clone())
                .unwrap_or_default()
        };

        let examiner_raw = ai
            .chat_with_system(&examiner_sys, &agent_prev, 0.3)
            .await
            .unwrap_or_else(|e| format!("审查员分析失败: {}", e));

        let examiner = parse_examiner_turn(round, &examiner_raw);

        // --- 代理师回应 ---
        let agent_sys = agent_system_prompt(round, &previous_summary, &examiner.content, params);

        let agent_raw = ai
            .chat_with_system(&agent_sys, "请针对审查员的质疑进行回应", 0.5)
            .await
            .unwrap_or_else(|e| format!("代理师回应失败: {}", e));

        let agent = parse_agent_turn(round, &agent_raw);

        // --- 摘要 ---
        let sum_prompt = summary_prompt(&examiner, &agent);
        let summary = ai
            .chat_with_system(
                "你是一个摘要助手，请将对话压缩为简洁摘要。",
                &sum_prompt,
                0.2,
            )
            .await
            .unwrap_or_else(|_| "摘要生成失败".to_string());

        previous_summary = format!("{}\n第{}轮摘要: {}", previous_summary, round, summary);

        transcript.push(RoundRecord {
            round,
            examiner: examiner.clone(),
            agent,
            summary,
        });
        rounds_completed = round;

        // --- 目标驱动停止判定 ---
        // 主条件：审查员评估 >= 阈值 且无 high 质疑 → 达标
        if goal_met(&examiner, goal_threshold) {
            goal_achieved = true;
            converged = true;
            break;
        }

        // 次条件：严重程度从 high 降到 low 且至少 2 轮 → 视为收敛
        if round >= 2 && examiner.severity == "low" {
            converged = true;
            break;
        }
    }

    // --- 终稿整合 ---
    let syn_prompt = synthesis_prompt(&transcript, params);
    let final_response = ai
        .chat_with_system(
            "你是资深专利代理师，请撰写正式的答复意见书。",
            &syn_prompt,
            0.4,
        )
        .await
        .unwrap_or_else(|e| format!("终稿整合失败: {}", e));

    // --- 综合信心度 ---
    let confidence_score: f32 = if transcript.is_empty() {
        0.0
    } else {
        transcript.iter().map(|r| r.agent.confidence).sum::<f32>() / transcript.len() as f32
    };

    // --- 残留风险点 ---
    let mut weak_points: Vec<String> = transcript
        .iter()
        .filter(|r| r.examiner.severity == "high")
        .map(|r| {
            format!(
                "第{}轮: {}",
                r.round,
                r.examiner.challenges.first().cloned().unwrap_or_default()
            )
        })
        .collect();

    // 如果未达标，在 weak_points 中标注具体差距
    if !goal_achieved && !transcript.is_empty() {
        let last_examiner = &transcript.last().unwrap().examiner;
        let gap = goal_threshold - last_examiner.assessment;
        weak_points.insert(
            0,
            format!(
                "目标未达成：审查员评估 {:.0}% < 阈值 {:.0}%，差距 {:.0}%",
                last_examiner.assessment * 100.0,
                goal_threshold * 100.0,
                gap * 100.0
            ),
        );
    }

    AdversarialResult {
        transcript,
        final_response,
        confidence_score,
        weak_points,
        rounds_completed,
        converged,
        goal_achieved,
        goal_threshold,
    }
}
