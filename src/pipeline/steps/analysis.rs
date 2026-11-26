//! Step 11-12: AiDeepAnalysis + AiActionPlan
//!
//! 类型：LLM
//!
//! 关键设计：AI 接收的是结构化计算结果，而非让 AI 自己猜测

use crate::ai::AiClient;
use crate::db::Database;
use crate::patent::FeatureCard;
use crate::pipeline::context::{PipelineContext, PipelineProgress};
use anyhow::Result;

/// 执行 Step 11: AI 深度分析（多维推演引擎）
///
/// MB2: 在推演开始前，先对 top-N 专利做免费全文富化（共用冷却表）。
/// 富化结果存入 `ctx.enrichment_results`，供 `build_user_context` 使用全文替代摘要。
pub async fn deep_analysis(
    ctx: &mut PipelineContext,
    ai: &AiClient,
    db: &Database,
    progress_tx: &Option<tokio::sync::broadcast::Sender<PipelineProgress>>,
) -> Result<()> {
    // MB2: 富化 top-N 专利的全文（无付费 Key 可达，冷却表共用）
    enrich_top_n(ctx, db).await;

    // MB3: 检索专利全文切片，供深度分析引用（无切片时降级为摘要档）
    retrieve_rag_chunks(ctx, db);

    // 运行多维深度推演引擎
    let result = super::deep_reasoning::run_deep_reasoning(ctx, ai, progress_tx).await?;

    // 格式化为 Markdown 报告（兼容旧的 ai_analysis 字段）
    ctx.ai_analysis = super::deep_reasoning::format_report(&result);
    ctx.deep_reasoning = result;

    // 更新研发状态机：低新颖性时记录排除路径
    if ctx.novelty_score < 30.0 {
        ctx.research_state
            .excluded_paths
            .push("该方向与现有技术高度重叠，建议调整技术路线".to_string());
    }

    Ok(())
}

/// 执行 Step 11 的降级版本：单次全文档深度 AI 调用（用于快速模式或推演引擎失败时）
pub async fn deep_analysis_simple(ctx: &mut PipelineContext, ai: &AiClient) -> Result<()> {
    // v0.9.67：告别 150 字裸截断，保留结构化全文摘录并预挖定量参数/拓扑/隐藏部件
    let mut combined_claims = String::new();
    let mut combined_desc = String::new();
    for m in ctx.top_matches.iter().take(5) {
        if let Some((desc_part, claims_part)) = m.snippet.split_once("## 权利要求") {
            combined_desc.push_str(desc_part);
            combined_desc.push('\n');
            combined_claims.push_str("## 权利要求");
            combined_claims.push_str(claims_part);
            combined_claims.push('\n');
        } else {
            combined_desc.push_str(&m.snippet);
            combined_desc.push('\n');
        }
    }
    let signals =
        crate::ai::patent::extract_whole_document_signals(&combined_claims, &combined_desc);
    let pre_mined = signals.format_prompt_block();

    let top_matches_summary: String = ctx
        .top_matches
        .iter()
        .take(8)
        .enumerate()
        .map(|(idx, m)| {
            let budget = if idx < 4 { 1200 } else { 500 };
            format!(
                "- [{}] {} (相似度: {:.1}%)\n  {}\n  来源: {}",
                m.source_type,
                m.source_title,
                m.combined_score * 100.0,
                crate::ai::patent::structured_patent_excerpt(&m.snippet, budget),
                m.source_url,
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let contradictions_summary = if ctx.contradictions.is_empty() {
        "未检测到明显的技术路线矛盾。".to_string()
    } else {
        ctx.contradictions
            .iter()
            .map(|c| {
                format!(
                    "- 矛盾维度: {}\n  信号强度: {:.0}%\n  机会: {}",
                    c.dimension,
                    c.signal_strength * 100.0,
                    c.opportunity,
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };

    let excluded_block =
        crate::orchestrator::constraints::format_excluded_paths_prompt_block(&ctx.research_state);
    let is_survey =
        ctx.query_intent == crate::pipeline::context::QueryIntent::MarketLandscapeSurvey;
    let prompt = if is_survey {
        format!(
            "请以「资深产业技术分析师 + 首席研发总工」双重视角，针对用户的开放式行业与产品谱系调研提问，严格基于以下真实检索材料输出结构化调研推演报告（严禁编造输入证据之外的任何专利公开号或 DOI，原文未披露的数据必须明确标注「原文未披露」）：\n\n\
             ## 用户调研主题\n\
             **主题：** {title}\n\
             **具体问题：** {description}\n\
             **技术领域：** {domain}\n\
             {excluded_block}\n\
             ## 程序预挖现有技术全文信号\n{pre_mined}\n\n\
             ## 检索到的代表性专利与文献证据（共 {hit_count} 条核心匹配）\n\n\
             {matches}\n\n\
             ## 路线差异与创新机会信号\n\n\
             {contradictions}\n\n\
             请用 Markdown 标题与表格严格按以下 5 节输出：\n\n\
             ### 1. 物理工作区间、标准边界与合规/安全红线\n\
             - 明确该品类主流工程参数区间、民用/专业或法规准入边界及核心安全风险。\n\n\
             ### 2. 市面主流产品分类与优缺点全景对照表\n\
             - 用 Markdown 表格横向对比各类主流产品形态（结构原理、代表真实专利/厂商、核心优点、核心缺点与痛点、适用场景）。\n\n\
             ### 3. 行业共性技术痛点与近年专利演进趋势\n\
             - 结合输入中的真实专利/文献剖析当前竞争壁垒与技术演进方向。\n\n\
             ### 4. 差异化产品定义与高价值研发/专利切入建议\n\
             - 给出 ≥3 条避开红海内卷的可落地切入路线及对应技术突破口。\n\n\
             ### 5. 立项决策与实验室验证优先清单\n\
             - 给出清晰的产品选型建议、成本/合规权衡与优先验证指标。",
            title = ctx.title,
            description = ctx.description,
            domain = ctx.technical_domain,
            hit_count = ctx.top_matches.len(),
            pre_mined = pre_mined,
            excluded_block = excluded_block,
            matches = top_matches_summary,
            contradictions = contradictions_summary,
        )
    } else {
        format!(
            "请以「首席研发总工 + 资深专利代理师」双重视角，基于以下全文档材料做深度研发与专利布局剖析（严禁浮于表面，原文未披露数据标注「原文未披露」，绝不编造）：\n\n\
             ## 用户的创意\n\
             **标题：** {title}\n\
             **描述：** {description}\n\
             **技术领域：** {domain}\n\
             {excluded_block}\n\
             ## 代码计算结果（已验证）\n\
             **新颖性评分：{score:.0}/100**（最高相似度 {max_sim:.1}% / Top5 均值 {avg_sim:.1}% / 矛盾加分 +{contra_bonus:.0} / 缺口加分 +{gap_bonus:.0} / 多样性 {diversity:.0}%）\n\n\
             ## 程序预挖现有技术全文信号\n{pre_mined}\n\n\
             ## 最相关的现有技术（含富化全文选段）\n\n\
             {matches}\n\n\
             ## 矛盾信号（不同技术路线 = 创新空间）\n\n\
             {contradictions}\n\n\
             请用 Markdown 标题与表格严格按以下 5 节输出：\n\n\
             ### 1. 核心物理因果链与现有方案失效边界\n\
             - 拆解最相关现有技术的核心机理、实测参数窗口与隐含工程前提/失效边界。\n\n\
             ### 2. 说明书隐藏宝藏与跨文档创新空白点\n\
             - 指出可免费借鉴的未权要细节与跨方案嫁接切入点（同步指出新引入的耦合风险）。\n\n\
             ### 3. 抓准真重点：独立权利要求最优抽象层与规避靶点\n\
             - 明确哪些核心原理必须卡死进独权、哪些单一原型细枝末节必须踢出独权；给出 ≥2 条可落地规避设计路径。\n\n\
             ### 4. 面向未来 3~5 年的演进卡位（不拘一格 × 沉住气求真）\n\
             - 给出 2 个跨代材料/架构/控制演进方向及可提前铺设的替代实施例，并列明必须先在实验室验证的真实风险红线。\n\n\
             ### 5. 知识产权与落地可行性总评\n\
             - 给出清晰的新颖性/创造性判断、主要技术壁垒与优先行动建议。",
            title = ctx.title,
            description = ctx.description,
            domain = ctx.technical_domain,
            score = ctx.novelty_score,
            max_sim = ctx.score_breakdown.max_similarity * 100.0,
            avg_sim = ctx.score_breakdown.avg_top5_similarity * 100.0,
            contra_bonus = ctx.score_breakdown.contradiction_bonus,
            gap_bonus = ctx.score_breakdown.coverage_gap_bonus,
            diversity = ctx.diversity_score.unwrap_or(0.0) * 100.0,
            pre_mined = pre_mined,
            excluded_block = excluded_block,
            matches = top_matches_summary,
            contradictions = contradictions_summary,
        )
    };

    match ai.chat_expert(&prompt, None).await {
        Ok(analysis) => ctx.ai_analysis = analysis,
        Err(e) => {
            ctx.ai_analysis = format!(
                "AI 分析暂不可用（{}）。\n\n\
                 基于代码计算的结果：新颖性评分 {:.0}/100，\
                 找到 {} 个相关现有技术，{} 个矛盾信号。\n\
                 请参考上方的量化数据进行判断。",
                e,
                ctx.novelty_score,
                ctx.top_matches.len(),
                ctx.contradictions.len(),
            );
        }
    }

    Ok(())
}

/// 执行 Step 12: AI 生成行动方案
pub async fn action_plan(ctx: &mut PipelineContext, ai: &AiClient) -> Result<()> {
    let analysis_excerpt = crate::ai::patent::structured_patent_excerpt(&ctx.ai_analysis, 3500);
    let excluded_block =
        crate::orchestrator::constraints::format_excluded_paths_prompt_block(&ctx.research_state);
    let prompt = format!(
        "基于以下全文档深度分析结果，生成可拿回实验室与专利撰写台直接执行的行动方案：\n\n\
         **创意：** {}\n\
         **新颖性评分：** {:.0}/100\n\
         **技术领域：** {}\n\
         **矛盾信号数量：** {}\n{}\n\
         **深度分析核心摘录：**\n{}\n\n\
         请按以下 4 节输出（务实求真、不画大饼）：\n\n\
         ### 1. 专利撰写与保护宽度防踩坑清单（抓真重点）\n\
         - 独立权利要求必须卡死的核心物理特征 vs 必须踢出的非必要原型细节；\n\
         - 建议同步铺设的 2~3 个面向未来 3~5 年演进的替代实施例。\n\n\
         ### 2. 台架/实验室优先验证清单（沉住气求真）\n\
         - 列出 3 项最先必须跑通的定量验证实验（写清测试指标、合格红线与失败止损条件）。\n\n\
         ### 3. 阶段性推进路线\n\
         - 短期（1~3 个月）：原型验证与核心专利交底卡位；\n\
         - 中期（3~12 个月）：工程可靠性验证与外围组合布局。\n\n\
         ### 4. 潜在对标机构与关键标准/文献线索\n\
         - 建议重点跟踪的对标团队、行业测试标准或核心期刊方向。",
        ctx.title,
        ctx.novelty_score,
        ctx.technical_domain,
        ctx.contradictions.len(),
        excluded_block,
        analysis_excerpt,
    );

    match ai.chat_expert(&prompt, None).await {
        Ok(plan) => ctx.action_plan = plan,
        Err(e) => {
            ctx.action_plan = format!("行动方案生成暂不可用：{}", e);
        }
    }

    Ok(())
}

/// 从 AI 分析结果和 top_matches 自动提取特征卡片并存入数据库
/// Extract feature cards from AI analysis + ranked matches, persist to DB
pub async fn extract_feature_cards(ctx: &PipelineContext, db: &Database) -> Result<()> {
    let idea_id = &ctx.idea_id;
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // 从 top_matches 提取：每个高相似度匹配 → 一张特征卡（含 5 维字段）
    // 反转 combined_score 为 novelty_score：相似度越高 → 新颖性越低
    for (i, m) in ctx.top_matches.iter().take(5).enumerate() {
        let novelty = ((1.0 - m.combined_score) * 100.0).clamp(0.0, 100.0);
        let description = crate::ai::truncate_for_ai(&m.snippet, 300);

        let card = FeatureCard {
            id: format!("{}-fc-{}", idea_id, i + 1),
            idea_id: idea_id.clone(),
            title: format!("[{}] {}", m.source_type, m.source_title),
            description,
            novelty_score: Some(novelty),
            created_at: now.clone(),
            technical_problem: format!("与「{}」相关的技术问题", ctx.title),
            core_structure: crate::ai::truncate_for_ai(&m.snippet, 200),
            key_relations: m.tokens.join(", "),
            process_steps: String::new(),
            application_scenarios: ctx.technical_domain.clone(),
        };

        if let Err(e) = db.insert_feature_card(&card) {
            tracing::warn!("特征卡片存储失败: {} — {}", card.title, e);
        }
    }

    // 从矛盾信号提取：每个矛盾 → 一张「创新机会」特征卡（含 5 维字段）
    for (i, c) in ctx.contradictions.iter().enumerate() {
        let card = FeatureCard {
            id: format!("{}-fc-opp-{}", idea_id, i + 1),
            idea_id: idea_id.clone(),
            title: format!("创新机会: {}", c.dimension),
            description: c.opportunity.clone(),
            novelty_score: Some(c.signal_strength * 100.0),
            created_at: now.clone(),
            technical_problem: format!("{}与{}在{}维度的矛盾", c.source_a, c.source_b, c.dimension),
            core_structure: c.opportunity.clone(),
            key_relations: format!("{} ↔ {}", c.source_a, c.source_b),
            process_steps: String::new(),
            application_scenarios: ctx.technical_domain.clone(),
        };

        if let Err(e) = db.insert_feature_card(&card) {
            tracing::warn!("创新机会卡片存储失败: {} — {}", card.title, e);
        }
    }

    let total = ctx.top_matches.len().min(5) + ctx.contradictions.len();
    tracing::info!("已自动提取 {} 张特征卡片 (idea: {})", total, idea_id);
    Ok(())
}

/// 从 AI 返回文本中安全解析 5 维特征卡片 JSON 数组（最多 5 张）。
/// 兼容裸 JSON 数组、markdown 代码块包裹及前后夹杂解释文字；非法 JSON 降级返回空列表。
pub fn parse_ai_feature_cards_json(idea_id: &str, raw: &str, created_at: &str) -> Vec<FeatureCard> {
    let Some(start) = raw.find('[') else {
        return Vec::new();
    };
    let Some(end) = raw.rfind(']') else {
        return Vec::new();
    };
    if start > end {
        return Vec::new();
    }
    let Some(json_slice) = raw.get(start..=end) else {
        return Vec::new();
    };
    let Ok(cards) = serde_json::from_str::<Vec<serde_json::Value>>(json_slice) else {
        return Vec::new();
    };

    cards
        .iter()
        .take(5)
        .map(|card_json| FeatureCard {
            id: uuid::Uuid::new_v4().to_string(),
            idea_id: idea_id.to_string(),
            title: card_json["title"]
                .as_str()
                .unwrap_or("AI提取特征")
                .to_string(),
            description: String::new(),
            novelty_score: None,
            created_at: created_at.to_string(),
            technical_problem: card_json["technical_problem"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            core_structure: card_json["core_structure"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            key_relations: card_json["key_relations"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            process_steps: card_json["process_steps"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            application_scenarios: card_json["application_scenarios"]
                .as_str()
                .unwrap_or("")
                .to_string(),
        })
        .collect()
}

/// AI 驱动的 5 维特征提取 — 在 deep_analysis 完成后调用
/// 向 AI 发送结构化 prompt，解析返回的 JSON 数组，创建带 5 维字段的 FeatureCard
pub async fn extract_feature_cards_ai(
    ctx: &PipelineContext,
    ai: &AiClient,
    db: &Database,
) -> Result<()> {
    const SYSTEM_PROMPT: &str =
        "你是专利技术特征提取助手。请严格只输出合法的 JSON 数组，不要输出任何解释文字、散文结构或 markdown 标记。";
    let prompt = format!(
        "从以下创意描述和分析结果中提取技术特征卡片。\n\
         请输出 JSON 数组，每个元素包含以下字段：\n\
         - title: 特征标题\n\
         - technical_problem: 解决什么技术问题\n\
         - core_structure: 核心技术结构/方案\n\
         - key_relations: 关键技术关系/连接\n\
         - process_steps: 工艺/实施步骤\n\
         - application_scenarios: 应用场景/领域\n\n\
         创意标题：\n<user_input>\n{}\n</user_input>\n\n\
         创意描述：\n<user_input>\n{}\n</user_input>\n\n\
         分析结果：\n<user_input>\n{}\n</user_input>\n\n\
         请直接输出 JSON 数组，不要包含 markdown 标记。",
        ctx.title,
        ctx.description,
        crate::ai::truncate_for_ai(&ctx.ai_analysis, 2000)
    );

    let fast_ai = ai.clone_with_timeout(std::time::Duration::from_secs(60));
    match fast_ai.chat_with_system(SYSTEM_PROMPT, &prompt, 0.2).await {
        Ok(response) => {
            let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let parsed_cards = parse_ai_feature_cards_json(&ctx.idea_id, &response, &now);
            if parsed_cards.is_empty() {
                tracing::warn!("AI 特征提取返回非法或空 JSON，跳过");
            } else {
                let count = parsed_cards.len();
                for card in &parsed_cards {
                    if let Err(e) = db.insert_feature_card(card) {
                        tracing::warn!("AI 特征卡片存储失败: {}", e);
                    }
                }
                tracing::info!(
                    "AI 自动提取 {} 张 5 维特征卡片 (idea: {})",
                    count,
                    ctx.idea_id
                );
            }
        }
        Err(e) => {
            tracing::warn!("AI 特征卡片提取失败: {}", e);
        }
    }

    Ok(())
}

/// MB2: 对 top-N 专利做免费全文富化。
///
/// - N = [`crate::search::enrichment::DEFAULT_ENRICH_TOP_N`]（默认 5）
/// - 从 `top_matches` 提取专利号，调 [`enrich_patent_free`]
/// - 结果存入 `ctx.enrichment_results`（结构化，供响应侧使用）
/// - 若富化成功且专利有全文，更新 `RankedMatch.snippet` 为 description+claims
/// - 冷却命中时零出网，如实记录原因
async fn enrich_top_n(ctx: &mut PipelineContext, db: &Database) {
    use crate::search::enrichment::{
        enrich_patent_free, enrich_result_to_status, extract_patent_number, EnrichResult,
        DEFAULT_ENRICH_TOP_N,
    };

    let n = DEFAULT_ENRICH_TOP_N;
    let matches: Vec<_> = ctx.top_matches.iter().take(n).cloned().collect();
    let mut statuses = Vec::with_capacity(matches.len());

    for m in &matches {
        let pn = extract_patent_number(&m.source_id, &m.source_url);
        let pn_str = pn.as_deref().unwrap_or("");
        if pn_str.is_empty() {
            statuses.push(enrich_result_to_status(
                &m.source_id,
                "",
                &EnrichResult::NotFound,
            ));
            continue;
        }

        tracing::info!("[MB2] Enriching patent {} ({})", pn_str, m.source_title);

        let result = enrich_patent_free(db, pn_str).await;
        let status = enrich_result_to_status(&m.source_id, pn_str, &result);
        statuses.push(status);
    }

    // 更新 snippet：对富化成功的专利，用全文替代摘要
    for m in &matches {
        let pn = extract_patent_number(&m.source_id, &m.source_url);
        let pn_str = pn.as_deref().unwrap_or("");
        if pn_str.is_empty() {
            continue;
        }
        if let Ok(Some(p)) = db.get_patent(pn_str) {
            let full_text = if !p.description.is_empty() && !p.claims.is_empty() {
                format!("{}\n\n## 权利要求\n\n{}", p.description, p.claims)
            } else if !p.description.is_empty() {
                p.description.clone()
            } else if !p.claims.is_empty() {
                format!("## 权利要求\n\n{}", p.claims)
            } else {
                continue;
            };
            // 只在全文比摘要更长时替换（避免用短全文替换长摘要）
            if full_text.len() > m.snippet.len() {
                if let Some(target) = ctx
                    .top_matches
                    .iter_mut()
                    .find(|rm| rm.source_id == m.source_id)
                {
                    target.snippet = full_text;
                }
            }
        }
    }

    ctx.enrichment_results = statuses;
}

/// MB3: 检索 top-N 专利的全文切片，存入 `ctx.rag_chunks` 供深度分析引用。
///
/// 在 `enrich_top_n` 之后调用——此时专利全文已切片入库。
/// 用用户创意（title + description）作为检索 query，
/// 对每个专利取 top-K 切片，合并后按专利顺序编号。
/// 无切片时静默降级（`rag_chunks` 为空，`build_user_context` 走摘要档）。
fn retrieve_rag_chunks(ctx: &mut PipelineContext, db: &Database) {
    use crate::rag::retriever::retrieve_chunks;
    use crate::search::enrichment::{extract_patent_number, DEFAULT_ENRICH_TOP_N};

    let query = format!("{} {}", ctx.title, ctx.description);
    let n = DEFAULT_ENRICH_TOP_N;
    let top_k = 3; // 每篇专利取 3 个最相关切片

    let mut all_chunks = Vec::new();

    for m in ctx.top_matches.iter().take(n) {
        let pn = extract_patent_number(&m.source_id, &m.source_url);
        let pn_str = pn.as_deref().unwrap_or("");
        if pn_str.is_empty() {
            continue;
        }

        match retrieve_chunks(db, pn_str, &query, top_k) {
            Ok(chunks) => {
                if !chunks.is_empty() {
                    tracing::info!(
                        "[MB3] Retrieved {} chunks for patent {}",
                        chunks.len(),
                        pn_str
                    );
                    all_chunks.extend(chunks);
                }
            }
            Err(e) => {
                tracing::warn!("[MB3] Chunk retrieval failed for {}: {}", pn_str, e);
            }
        }
    }

    if !all_chunks.is_empty() {
        tracing::info!(
            "[MB3] Total {} RAG chunks retrieved for deep analysis",
            all_chunks.len()
        );
    }

    ctx.rag_chunks = all_chunks;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::context::RankedMatch;

    #[test]
    fn test_extract_patent_number_from_ranked_match() {
        use crate::search::enrichment::extract_patent_number;
        // Local patent
        let pn = extract_patent_number("patent_local_CN990001012A", "");
        assert_eq!(pn, Some("CN990001012A".to_string()));
        // From URL
        let pn = extract_patent_number(
            "patent_online_0",
            "https://patents.google.com/patent/US12345678B2/en",
        );
        assert_eq!(pn, Some("US12345678B2".to_string()));
    }

    #[test]
    fn test_default_enrich_top_n_is_5() {
        assert_eq!(crate::search::enrichment::DEFAULT_ENRICH_TOP_N, 5);
    }

    #[tokio::test]
    async fn test_enrich_top_n_with_empty_matches() {
        let db = crate::db::Database::init(":memory:").unwrap();
        let mut ctx = PipelineContext::new("test", "title", "desc");
        enrich_top_n(&mut ctx, &db).await;
        assert!(ctx.enrichment_results.is_empty());
    }

    #[tokio::test]
    async fn test_enrich_top_n_respects_limit() {
        let db = crate::db::Database::init(":memory:").unwrap();
        let mut ctx = PipelineContext::new("test", "title", "desc");
        // Create 10 matches — only 5 should be processed
        for i in 0..10 {
            ctx.top_matches.push(RankedMatch {
                rank: i + 1,
                source_id: format!("patent_online_{i}"),
                source_title: format!("Patent {i}"),
                source_type: "patent".to_string(),
                source_url: String::new(),
                snippet: "abstract".to_string(),
                combined_score: 0.5,
                tokens: Vec::new(),
            });
        }
        enrich_top_n(&mut ctx, &db).await;
        // Should have exactly 5 results (top-5), all NotFound since no patents in DB
        assert_eq!(ctx.enrichment_results.len(), 5);
        for s in &ctx.enrichment_results {
            assert_eq!(s.reason, "not_found");
        }
    }

    // MB3 tests

    #[test]
    fn test_rag_chunks_field_starts_empty() {
        let ctx = PipelineContext::new("test", "title", "desc");
        assert!(ctx.rag_chunks.is_empty());
    }

    #[test]
    fn test_retrieve_rag_chunks_with_empty_matches() {
        let db = crate::db::Database::init(":memory:").unwrap();
        let mut ctx = PipelineContext::new("test", "title", "desc");
        retrieve_rag_chunks(&mut ctx, &db);
        assert!(ctx.rag_chunks.is_empty());
    }

    #[test]
    fn test_retrieve_rag_chunks_with_no_chunks_in_db() {
        let db = crate::db::Database::init(":memory:").unwrap();
        let mut ctx = PipelineContext::new("test", "title", "desc");
        // Add a match with a patent number, but no chunks in DB
        ctx.top_matches.push(RankedMatch {
            rank: 1,
            source_id: "patent_local_CN123456A".to_string(),
            source_title: "Test Patent".to_string(),
            source_type: "patent".to_string(),
            source_url: String::new(),
            snippet: "abstract".to_string(),
            combined_score: 0.5,
            tokens: Vec::new(),
        });
        retrieve_rag_chunks(&mut ctx, &db);
        // No chunks in DB → rag_chunks should be empty (graceful degradation)
        assert!(ctx.rag_chunks.is_empty());
    }

    #[test]
    fn test_build_chunks_from_patent_produces_chunks() {
        let chunks = crate::rag::build_chunks_from_patent(
            "test-patent-1",
            "Test Patent Title",
            "This is the abstract text for testing.",
            "Claim 1: A method comprising steps A and B.",
            "The description provides detailed technical content for the patent.",
        );
        // Should produce chunks for abstract, claims, and description
        assert!(
            !chunks.is_empty(),
            "Should produce chunks from non-empty text"
        );
        // All chunks should have the correct patent_id
        for c in &chunks {
            assert_eq!(c.patent_id, "test-patent-1");
        }
        // Should have at least 3 different source_types
        let source_types: std::collections::HashSet<_> =
            chunks.iter().map(|c| c.source_type.as_str()).collect();
        assert!(
            source_types.contains("abstract"),
            "Should have abstract chunks"
        );
        assert!(source_types.contains("claim"), "Should have claim chunks");
        assert!(
            source_types.contains("description"),
            "Should have description chunks"
        );
    }

    #[test]
    fn test_build_chunks_from_empty_text() {
        let chunks = crate::rag::build_chunks_from_patent("test-patent-2", "", "", "", "");
        assert!(chunks.is_empty(), "Empty text should produce no chunks");
    }

    #[test]
    fn test_save_and_retrieve_patent_chunks() {
        let db = crate::db::Database::init(":memory:").unwrap();
        // Must insert a patent first (FK constraint on patent_chunks)
        let patent = crate::types::patent::Patent {
            id: String::new(),
            patent_number: "CN000000003A".to_string(),
            title: "Title".to_string(),
            abstract_text: "Abstract text here.".to_string(),
            description: String::new(),
            claims: String::new(),
            applicant: String::new(),
            inventor: String::new(),
            filing_date: String::new(),
            publication_date: String::new(),
            grant_date: None,
            ipc_codes: String::new(),
            cpc_codes: String::new(),
            priority_date: String::new(),
            country: "CN".to_string(),
            kind_code: "A".to_string(),
            family_id: None,
            legal_status: String::new(),
            citations: "[]".to_string(),
            cited_by: "[]".to_string(),
            source: String::new(),
            raw_json: String::new(),
            created_at: String::new(),
            images: String::new(),
            pdf_url: String::new(),
        };
        let patent_id = db.insert_patent(&patent).unwrap();
        let chunks = crate::rag::build_chunks_from_patent(
            &patent_id,
            "Title",
            "Abstract text here.",
            "Claims text here.",
            "Description text here.",
        );
        assert!(!chunks.is_empty());
        let count = db
            .save_patent_chunks(&patent_id, &chunks, "char-tfidf-v1")
            .unwrap();
        assert_eq!(count, chunks.len());
        // Verify count_chunks
        let db_count = db.count_chunks(&patent_id).unwrap();
        assert_eq!(db_count as usize, chunks.len());
    }

    #[test]
    fn test_retrieve_chunks_by_keyword_not_empty_placeholder() {
        // Verify the function is not just returning empty vec
        let db = crate::db::Database::init(":memory:").unwrap();
        // Must insert a patent first (FK constraint)
        let patent = crate::types::patent::Patent {
            id: String::new(),
            patent_number: "CN000000004A".to_string(),
            title: "Title".to_string(),
            abstract_text: "machine learning neural network".to_string(),
            description: String::new(),
            claims: String::new(),
            applicant: String::new(),
            inventor: String::new(),
            filing_date: String::new(),
            publication_date: String::new(),
            grant_date: None,
            ipc_codes: String::new(),
            cpc_codes: String::new(),
            priority_date: String::new(),
            country: "CN".to_string(),
            kind_code: "A".to_string(),
            family_id: None,
            legal_status: String::new(),
            citations: "[]".to_string(),
            cited_by: "[]".to_string(),
            source: String::new(),
            raw_json: String::new(),
            created_at: String::new(),
            images: String::new(),
            pdf_url: String::new(),
        };
        let patent_id = db.insert_patent(&patent).unwrap();
        // Save some chunks
        let chunks = crate::rag::build_chunks_from_patent(
            &patent_id,
            "Title",
            "machine learning neural network",
            "",
            "",
        );
        db.save_patent_chunks(&patent_id, &chunks, "char-tfidf-v1")
            .unwrap();
        // Search for a keyword that exists in the chunks
        let result =
            crate::rag::retriever::retrieve_chunks_by_keyword(&db, &patent_id, "machine", 10);
        assert!(result.is_ok());
        assert!(
            !result.unwrap().is_empty(),
            "Keyword search should find matching chunks"
        );
    }

    #[test]
    fn test_truncate_for_ai_used_in_all_8_sites() {
        // Verify that none of the 4 target files contain chars().take in AI prompt paths.
        // This is a regression guard: if someone re-introduces chars().take,
        // this test will catch it.
        let files = [
            "src/pipeline/steps/analysis.rs",
            "src/pipeline/steps/deep_reasoning.rs",
            "src/pipeline/steps/oa_response.rs",
            "src/pipeline/steps/claim_tree.rs",
        ];
        for file in files {
            let content = std::fs::read_to_string(file).unwrap_or_default();
            // Skip everything after #[cfg(test)] — only check production code
            let prod_code = content.split("#[cfg(test)]").next().unwrap_or("");
            // Allow chars().take in comments, but not in .collect() patterns
            let bad_count = prod_code
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .filter(|l| l.contains("chars().take(") && l.contains(".collect"))
                .count();
            assert_eq!(
                bad_count, 0,
                "{} still has chars().take().collect() in production code",
                file
            );
        }
    }

    #[test]
    fn parse_ai_feature_cards_json_handles_raw_markdown_prose_and_cap() {
        let raw = r#"[{"title":"特征A","technical_problem":"问题A","core_structure":"结构A","key_relations":"关系A","process_steps":"步骤A","application_scenarios":"场景A"}]"#;
        let cards = super::parse_ai_feature_cards_json("idea-1", raw, "2026-11-18 00:00:00");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].idea_id, "idea-1");
        assert_eq!(cards[0].title, "特征A");
        assert_eq!(cards[0].technical_problem, "问题A");
        assert_eq!(cards[0].core_structure, "结构A");

        let wrapped = "以下是提取结果：\n```json\n[{\"title\":\"特征B\",\"core_structure\":\"结构B\"}]\n```\n请查收。";
        let cards = super::parse_ai_feature_cards_json("idea-2", wrapped, "2026-11-18 00:00:00");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].title, "特征B");
        assert_eq!(cards[0].core_structure, "结构B");

        let six_items = r#"[
            {"title":"1"},{"title":"2"},{"title":"3"},{"title":"4"},{"title":"5"},{"title":"6"}
        ]"#;
        let cards = super::parse_ai_feature_cards_json("idea-3", six_items, "2026-11-18 00:00:00");
        assert_eq!(cards.len(), 5);
    }

    #[test]
    fn parse_ai_feature_cards_json_returns_empty_on_invalid_json() {
        assert!(super::parse_ai_feature_cards_json("idea-1", "无 JSON 内容", "t").is_empty());
        assert!(super::parse_ai_feature_cards_json("idea-1", "][", "t").is_empty());
        assert!(super::parse_ai_feature_cards_json("idea-1", "[{bad json}]", "t").is_empty());
        assert!(
            super::parse_ai_feature_cards_json("idea-1", "{\"title\":\"not array\"}", "t")
                .is_empty()
        );
    }
}
