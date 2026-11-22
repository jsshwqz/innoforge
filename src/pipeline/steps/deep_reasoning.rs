//! 模块职责：多维全文档深度研发推演引擎（v0.9.67 重构：首席总工 × 资深专利代理师 × 未来 3~5 年演进卡位）。
//! 上游调用者：`src/pipeline/steps/analysis.rs::deep_analysis`。
//! 模块专属禁区：严禁退回空泛哲学角色或对富化全文做 120 字裸截断；必须严格保持 `[引用N]` 1 基编号与 `evidence_kernel` 对齐；禁 `chars().take().collect()`。

use crate::ai::patent::{extract_whole_document_signals, structured_patent_excerpt};
use crate::ai::AiClient;
use crate::pipeline::context::{
    DeepReasoningResult, DimensionInsight, PipelineContext, PipelineProgress, StepStatus,
};
use crate::pipeline::state::PipelineStep;
use anyhow::Result;

/// 维度定义
struct Dimension {
    key: &'static str,
    label: &'static str,
    system_prompt: &'static str,
    temperature: f32,
}

const DIMENSIONS: &[Dimension] = &[
    // ── 维度 1: 物理机理与定量极限剖析 ──
    Dimension {
        key: "physics_bottleneck",
        label: "物理机理与定量极限",
        system_prompt:
            "你是拥有 20 年一线工程经验的首席研发总工兼物理机理专家。\n\
             你必须扎根于所提供专利全文与实验参数的真实物理规律，绝不空谈概念、绝不编造原文未披露的数字。\n\n\
             你的分析任务：\n\
             1. 识别支配本创意与现有方案的核心物理/化学/热力学/控制因果链（痛点机理 → 关键结构/工艺 → 物理作用链）；\n\
             2. 汇总上下文中已披露的真实定量参数窗口（温度/压力/效率/尺寸/材料），原文缺失处明确标注「原文未披露」；\n\
             3. 指出当前行业实践距离物理理论上限的真实瓶颈在哪里，以及最值得攻克的 1~2 个决定性“胜负手”变量。\n\n\
             约束：凡引用现有专利切片必须标注 [引用N]；每条判断必须给出可验证的物理依据。",
        temperature: 0.3,
    },
    // ── 维度 2: 现有专利全文解剖与失效边界 ──
    Dimension {
        key: "prior_art_anatomy",
        label: "现有技术全文解剖与失效边界",
        system_prompt:
            "你是专攻工程失效分析（FMEA）与竞品拆解的资深总工。\n\
             你的任务是穿透现有专利的表面优点，从全文实施例字里行间挖出它们未明说的工程前提与踩坑死角。\n\n\
             你的分析任务：\n\
             1. 逐篇剖析最相关现有技术默认成立却未明说的 2~3 个隐含工程前提（工况载荷、公差、材料相容性、密封寿命、工艺一致性等）；\n\
             2. 指出在什么边界条件下现有方案会率先失效或性价比崩塌；\n\
             3. 提炼出所有对比方案共同存在的行业共性物理矛盾（说明为何局部微调无法根治）。\n\n\
             约束：必须基于真实上下文材料，标明 [引用N] 出处，绝不泛泛而谈“成本高、结构复杂”。",
        temperature: 0.45,
    },
    // ── 维度 3: 说明书隐藏宝藏与跨文档嫁接点 ──
    Dimension {
        key: "unclaimed_gems",
        label: "说明书隐藏宝藏与跨文档嫁接",
        system_prompt:
            "你是擅长跨文档知识挖掘的专利情报首席分析师。\n\
             你的任务是发现「单看摘要根本看不到、必须通读多篇说明书与预挖部件清单才能挖出的新知识」。\n\n\
             你的分析任务：\n\
             1. 结合程序预挖的“说明书披露但未写入权利要求的隐藏部件/细节”，区分哪些可直接免费借鉴，哪些适合二次改进补强；\n\
             2. 发掘至少 2 个跨专利嫁接空白点：「提取方案 A 的关键结构/机理 + 结合方案 B 的材料/控制手段」解决共性瓶颈；\n\
             3. 诚实预判跨方案嫁接会引入什么新的工程耦合副作用及对策。\n\n\
             约束：所有部件与方案依据必须可追溯至上下文与 [引用N]，严禁无中生有。",
        temperature: 0.55,
    },
    // ── 维度 4: 权要核心抽象层与工程规避靶点 ──
    Dimension {
        key: "claim_barrier_evasion",
        label: "抓真重点与权要攻防壁垒",
        system_prompt:
            "你是兼具 15 年无效诉讼经验与发明人辅导经验的资深专利代理师。\n\
             许多发明人回头看自己早年专利常后悔“没抓住真重点、被原型细枝末节锁死保护范围”。你的核心使命就是帮发明人彻底避开这个坑。\n\n\
             你的分析任务：\n\
             1. **抓真重点（最优抽象层）**：剔除具体原型中的非必要结构/位置/连接细枝末节，提炼出独立权利要求真正应该卡死的核心发明本质；\n\
             2. **现有专利规避靶点**：指出高相似竞品独立权利要求中最窄化的多余限定特征，并给出至少 2 条不落入字面/等同侵权的可落地工程替换方案；\n\
             3. **授权稳固性预警**：提前标出可能触发专利法第 26.4 条（说明书支撑不足）或第 22.3 条（创造性弱）的撰写雷区。\n\n\
             约束：明确给出“哪些必须写进独权、哪些应踢出独权下沉为从权”。",
        temperature: 0.35,
    },
    // ── 维度 5: 不拘一格的跨界迁移与上游减法 ──
    Dimension {
        key: "cross_domain_synthesis",
        label: "跨界架构迁移与上游减法",
        system_prompt:
            "你是敢于打破学科壁垒的创新系统架构师，遵循「不拘一格的研发想象 + 沉住气的工程可实现性」。\n\n\
             你的分析任务：\n\
             1. **上游减法（消除问题根源）**：现有方案的哪些复杂部件只是在“补救上游缺陷”？若改变上游物理条件，哪 1~2 个传统组件可以直接删掉且系统更可靠？\n\
             2. **跨界机理迁移**：在航空航天、半导体热管理、医工装备、化工过程强化或电力电子等相邻工业领域中，找出 ≥2 个已验证的成熟技术机制，说明如何具体适配迁移到本课题；\n\
             3. 明确指出迁移后的关键工程接口改造点，而非停留在抽象类比。\n\n\
             约束：禁止空洞口号，每个跨界方案必须说清具体结构/材料/控制落点。",
        temperature: 0.65,
    },
    // ── 维度 6: 面向未来 3~5 年演进卡位与真实实验边界 ──
    Dimension {
        key: "future_evolution_3_5y",
        label: "未来3~5年演进卡位与求真验证",
        system_prompt:
            "你是着眼 3~5 年技术代际跃迁的 CTO 兼务实实验室负责人。\n\
             写研发方案与专利绝不能只盯着眼前一代原型，必须提前卡住未来替代路线；但同时也必须沉住气、讲真话，绝不画大饼。\n\n\
             你的分析任务：\n\
             1. **面向未来 3~5 年的演进路线图**：推演本技术未来在跨代材料、系统级集成、智能前馈/预测控制或制造工艺装备化上的 2~3 个高概率演进方向；\n\
             2. **替代实施例包裹建议**：指导发明人在当前申请中提前铺设哪些可实现的替代实施例，防止未来 3 年内被竞品用下一代介质/构型轻松绕开；\n\
             3. **沉住气的求真实验清单**：列出落地前必须在台架/实验室优先验证的 3 项最致命风险（如长期相容性、热疲劳、良率与成本阈值），明确失败红线。\n\n\
             约束：方向要前瞻、验证要冷酷真实，不编造未经实测的提升百分比。",
        temperature: 0.5,
    },
];

/// 合成轮的 system prompt
const SYNTHESIS_SYSTEM: &str = "你是首席研发总工兼资深专利战略顾问。你收到了 6 个硬核工程与专利维度的全文档深度剖析。\n\
     你的任务不是复述各维度，而是整合出一份能让一线研发拍板立项、能让发明人写出十年不过时高价值专利的**决策级合成报告**。\n\n\
     请严格按以下 4 节输出（使用 Markdown 标题与表格，凡涉及未实测指标明确标明需实验验证，绝不画大饼）：\n\n\
     ### 收敛信号\n\
     - 多个维度独立印证的核心物理胜负手、行业共性瓶颈与必须卡死的独权核心抽象层（明确指出哪些非必要原型细节应踢出独权）。\n\n\
     ### 突破性方向（Top 3）\n\
     - 给出 3 个既不局限眼前原型、又经得起物理与量产检验的创新/演进方向（覆盖眼前可落地优化 + 未来 3~5 年架构卡位）；\n\
     - 每条必须写清：「改动部位/跨界来源 + 物理机理 + 专利布局卡位点 + 必须优先验证的真实风险边界」。\n\n\
     ### 盲区\n\
     - 当前检索材料与已有方案尚未覆盖的关键数据缺口、长周期失效风险或待补充检索的对比证据（诚实指出风险，不报喜不报忧）。\n\n\
     ### 发明人防踩坑撰写自检清单\n\
     - 3 条直击要害的撰写提醒，确保这次方案抓准真重点、保护宽度兼顾当下与未来 3~5 年演进。";

/// 从富化后的 `snippet`（`description\n\n## 权利要求\n\nclaims`）拆分说明书与权利要求供信号预挖。
fn split_enriched_snippet(snippet: &str) -> (&str, &str) {
    if let Some((desc, claims)) = snippet.split_once("## 权利要求") {
        (claims.trim(), desc.trim())
    } else {
        ("", snippet.trim())
    }
}

/// 构建用户上下文（所有维度共用的输入数据）
pub(crate) fn build_user_context(ctx: &PipelineContext) -> String {
    // v0.9.67：彻底废除 120 字裸截断！充分消费 `enrich_top_n` 写入的真实专利全文，
    // 并先用确定性全文档信号预挖引擎提取定量参数、部件拓扑与未写入权要的说明书隐藏部件。
    let mut combined_claims = String::new();
    let mut combined_desc = String::new();
    for m in ctx.top_matches.iter().take(5) {
        let (claims_part, desc_part) = split_enriched_snippet(&m.snippet);
        if !claims_part.is_empty() {
            combined_claims.push_str(claims_part);
            combined_claims.push('\n');
        }
        if !desc_part.is_empty() {
            combined_desc.push_str(desc_part);
            combined_desc.push('\n');
        }
    }
    let signals = extract_whole_document_signals(&combined_claims, &combined_desc);
    let pre_mined_block = signals.format_prompt_block();

    let top_matches: String = ctx
        .top_matches
        .iter()
        .take(8)
        .enumerate()
        .map(|(idx, m)| {
            let budget = if idx < 5 { 1600 } else { 700 };
            format!(
                "- [{}] {} (相似度: {:.0}%)\n  {}",
                m.source_type,
                m.source_title,
                m.combined_score * 100.0,
                structured_patent_excerpt(&m.snippet, budget)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let contradictions: String = if ctx.contradictions.is_empty() {
        "无".to_string()
    } else {
        ctx.contradictions
            .iter()
            .map(|c| {
                format!(
                    "- {}: {} (强度 {:.0}%)",
                    c.dimension,
                    c.opportunity,
                    c.signal_strength * 100.0
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    // MB3: RAG 全文切片引用上下文（严格保持 1 基 [引用N] 编号与 evidence_kernel 对齐）
    let rag_context = if ctx.rag_chunks.is_empty() {
        // 无切片时如实标注（不得静默把「没取到」变成「没有相关内容」）
        "\n## 专利全文切片\n（暂无全文切片，以下分析基于摘要/富化片段档。）\n".to_string()
    } else {
        let mut rag_ctx = String::from("\n## 专利全文切片（可回溯引用）\n");
        for (i, chunk) in ctx.rag_chunks.iter().enumerate() {
            rag_ctx.push_str(&format!(
                "### [引用{}]（专利: {}, 来源: {}, 相似度: {:.2})\n{}\n\n",
                i + 1,
                chunk.patent_id,
                chunk.source_type,
                chunk.relevance_score,
                chunk.content
            ));
        }
        rag_ctx
    };

    let excluded_block =
        crate::orchestrator::constraints::format_excluded_paths_prompt_block(&ctx.research_state);

    format!(
        "## 待分析的创意\n\
         **标题：** {title}\n\
         **描述：** {desc}\n\
         **技术领域：** {domain}\n\
         **新颖性评分：** {score:.0}/100\n\
         {excluded_block}\n\
         ## 程序预挖现有技术全文信号\n{pre_mined}\n\n\
         ## 已发现的相关现有技术（含富化全文选段）\n{matches}\n\n\
         {rag}\n\
         ## 已检测到的技术路线矛盾\n{contras}\n\n\
         基于以上真实文档材料与预挖信号，按你的专业视角进行全文档深度知识发掘。\
         如引用了全文切片，请在相关结论后标注 [引用N]；原文未披露的具体数值必须标明「原文未披露」，严禁编造。",
        title = ctx.title,
        desc = ctx.description,
        domain = ctx.technical_domain,
        score = ctx.novelty_score,
        excluded_block = excluded_block,
        pre_mined = pre_mined_block,
        matches = top_matches,
        rag = rag_context,
        contras = contradictions,
    )
}

/// 从 AI 输出中提取核心洞察（第一段非空内容）
fn extract_key_insight(text: &str) -> String {
    text.lines()
        .filter(|l| {
            let trimmed = l.trim();
            !trimmed.is_empty()
                && !trimmed.starts_with('#')
                && !trimmed.starts_with("---")
                && trimmed.len() > 10
        })
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(200)
        .collect()
}

/// 从合成结果中提取 novel_directions 和 blind_spots
fn parse_synthesis(text: &str) -> (Vec<String>, Vec<String>) {
    let mut novel = Vec::new();
    let mut blinds = Vec::new();
    let mut section = "";

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.contains("突破性方向") || trimmed.contains("Top 3") {
            section = "novel";
            continue;
        }
        if trimmed.contains("盲区") || trimmed.contains("盲点") {
            section = "blind";
            continue;
        }
        if trimmed.contains("收敛信号") {
            section = "converge";
            continue;
        }
        if trimmed.starts_with("###") {
            section = "";
            continue;
        }

        let content = trimmed.trim_start_matches(|c: char| {
            c == '-' || c == '*' || c.is_ascii_digit() || c == '.' || c == ' '
        });
        if content.len() > 10 {
            match section {
                "novel" => novel.push(content.to_string()),
                "blind" => blinds.push(content.to_string()),
                _ => {}
            }
        }
    }

    (novel, blinds)
}

/// 执行多维深度推演
pub async fn run_deep_reasoning(
    ctx: &mut PipelineContext,
    ai: &AiClient,
    progress_tx: &Option<tokio::sync::broadcast::Sender<PipelineProgress>>,
) -> Result<DeepReasoningResult> {
    let user_context = build_user_context(ctx);
    let total_rounds = DIMENSIONS.len() + 1; // 6 维度 + 1 合成
    let mut result = DeepReasoningResult::default();

    // ── 6 轮维度推演 ──
    for (i, dim) in DIMENSIONS.iter().enumerate() {
        tracing::info!(
            "Deep reasoning: dimension {}/{} [{}]",
            i + 1,
            total_rounds,
            dim.key
        );

        // 发送子进度
        if let Some(tx) = progress_tx {
            let _ = tx.send(PipelineProgress {
                step: PipelineStep::AiDeepAnalysis,
                step_index: PipelineStep::AiDeepAnalysis.index(),
                total_steps: PipelineStep::TOTAL_STEPS,
                status: StepStatus::Running,
                message: format!("多维推演：{} ({}/{})", dim.label, i + 1, total_rounds),
                sub_step: Some(dim.key.to_string()),
                sub_progress: Some(i as f64 / total_rounds as f64),
            });
        }

        let reasoning = match ai
            .chat_with_system(dim.system_prompt, &user_context, dim.temperature)
            .await
        {
            Ok(text) => text,
            Err(e) => {
                tracing::warn!("维度 [{}] 推演失败: {}", dim.key, e);
                format!("（此维度推演未完成：{}）", e)
            }
        };

        let key_insight = extract_key_insight(&reasoning);

        result.dimensions.push(DimensionInsight {
            dimension: dim.key.to_string(),
            label: dim.label.to_string(),
            reasoning,
            key_insight,
        });
    }

    // ── 第 7 轮：跨维度合成 ──
    tracing::info!(
        "Deep reasoning: synthesis round ({}/{})",
        total_rounds,
        total_rounds
    );

    if let Some(tx) = progress_tx {
        let _ = tx.send(PipelineProgress {
            step: PipelineStep::AiDeepAnalysis,
            step_index: PipelineStep::AiDeepAnalysis.index(),
            total_steps: PipelineStep::TOTAL_STEPS,
            status: StepStatus::Running,
            message: format!("跨维度合成 ({}/{})", total_rounds, total_rounds),
            sub_step: Some("synthesis".to_string()),
            sub_progress: Some(6.0 / total_rounds as f64),
        });
    }

    // 构建合成输入：每个维度的核心洞察
    let synthesis_input = result
        .dimensions
        .iter()
        .map(|d| {
            format!(
                "## [{label}] 维度分析\n\n{reasoning}\n",
                label = d.label,
                reasoning = d.reasoning,
            )
        })
        .collect::<Vec<_>>()
        .join("\n---\n\n");

    let synthesis_prompt = format!(
        "以下是 6 位不同思维模式专家对同一个创意的分析：\n\n\
         **创意：** {title}\n\
         **描述：** {desc}\n\n\
         ---\n\n\
         {inputs}\n\n\
         ---\n\n\
         请进行跨维度合成分析。",
        title = ctx.title,
        desc = ctx.description,
        inputs = synthesis_input,
    );

    match ai
        .chat_with_system(SYNTHESIS_SYSTEM, &synthesis_prompt, 0.5)
        .await
    {
        Ok(text) => {
            let (novel, blinds) = parse_synthesis(&text);
            result.synthesis = text;
            result.novel_directions = novel;
            result.blind_spots = blinds;
        }
        Err(e) => {
            tracing::warn!("合成推演失败: {}", e);
            result.synthesis = format!("（跨维度合成未完成：{}）", e);
        }
    }

    Ok(result)
}

/// 将推演结果格式化为 Markdown 报告（兼容现有 ai_analysis 字段）
pub fn format_report(result: &DeepReasoningResult) -> String {
    let mut report = String::from("# 多维深度推演报告\n\n");

    // 合成结论放最前面
    if !result.synthesis.is_empty() && !result.synthesis.starts_with('（') {
        report.push_str("## 🔮 跨维度合成\n\n");
        report.push_str(&result.synthesis);
        report.push_str("\n\n---\n\n");
    }

    // 各维度详细分析
    let icons = ["🔬", "🧱", "💎", "🛡️", "🔀", "🚀"];
    for (i, dim) in result.dimensions.iter().enumerate() {
        let icon = icons.get(i).unwrap_or(&"📌");
        report.push_str(&format!(
            "## {} {} ({})\n\n{}\n\n---\n\n",
            icon, dim.label, dim.dimension, dim.reasoning
        ));
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::context::{PipelineContext, RankedMatch};

    /// 反作弊单测：确认深推演上下文不再用 120 字裸截断丢弃富化全文，
    /// 必须保留 120 字之后的定量参数与权利要求，并注入程序预挖的隐藏部件与拓扑信号。
    #[test]
    fn build_user_context_preserves_enriched_fulltext_and_premined_signals() {
        let mut ctx = PipelineContext::new(
            "idea-test",
            "高功率密度电机浸没冷却优化",
            "采用微负压介电液循环冷却绕组并结合预测控制",
        );
        ctx.technical_domain = "电机热管理".into();
        let long_prefix = "背景技术说明：现有密封流道散热方案接触面积有限且加工复杂，".repeat(8);
        let enriched = format!(
            "{long_prefix}具体实施方式：油泵(3)在0.45MPa压力下循环冷却液，使绕组峰值温度从148℃降至81℃。\
             附图标记说明：1-电机机体；2-散热器；3-油泵；4-导流挡板。\n\n\
             ## 权利要求\n\n\
             1. 一种电机冷却系统，包括电机机体(1)、散热器(2)和油泵(3)，油泵(3)与散热器(2)固定连接。"
        );
        ctx.top_matches.push(RankedMatch {
            rank: 1,
            source_id: "CN999000123B".into(),
            source_title: "一种电机冷却装置".into(),
            source_type: "patent".into(),
            source_url: String::new(),
            snippet: enriched,
            combined_score: 0.87,
            tokens: vec!["电机".into(), "冷却".into()],
        });

        let user_ctx = build_user_context(&ctx);
        assert!(
            user_ctx.contains("0.45MPa") && user_ctx.contains("148℃"),
            "必须保留并预挖 120 字之后的定量参数: {user_ctx}"
        );
        assert!(
            user_ctx.contains("导流挡板"),
            "必须预挖仅说明书披露而未写入权要的隐藏部件: {user_ctx}"
        );
        assert!(
            user_ctx.contains("电机机体(1)"),
            "必须保留富化后的权利要求书正文: {user_ctx}"
        );
    }
}
