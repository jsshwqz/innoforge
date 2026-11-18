use super::*;

/// T1: 对比文献自动获取与全文分析
/// POST /api/ai/oa-fetch-refs  { oa_text: String }
/// 从 OA 文本提取公开号 -> 查本地库 -> 返回全文
pub async fn api_ai_oa_fetch_refs(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let oa_text = req["oa_text"].as_str().unwrap_or("");
    if oa_text.is_empty() {
        return Json(json!({ "status": "error", "message": "OA 文本为空 / OA text is empty" }));
    }

    // 1. 提取公开号（复用已有函数）
    let pub_numbers = crate::ai::patent::extract_publication_numbers(oa_text);
    if pub_numbers.is_empty() {
        return Json(
            json!({ "status": "ok", "refs": [], "message": "未检测到对比文件公开号 / No reference publication numbers detected" }),
        );
    }

    // 2. 逐个查本地库
    let mut refs = Vec::new();
    for (context, pub_num) in &pub_numbers {
        let found = match s.db.get_patent(pub_num) {
            Ok(Some(p)) => {
                refs.push(json!({
                    "pub_number": pub_num,
                    "context": context,
                    "title": p.title,
                    "abstract": p.abstract_text,
                    "full_text": format!("{}\n\n{}", p.title, p.abstract_text),
                    "found": true,
                    "source": "local"
                }));
                true
            }
            Ok(None) => false,
            Err(e) => {
                tracing::warn!("查询专利 {} 失败: {}", pub_num, e);
                false
            }
        };
        if !found {
            // 本地未找到，尝试用 search_smart_exact 搜索
            match s.db.search_smart_exact(
                pub_num,
                Some(&crate::types::search::SearchType::PatentNumber),
                None,
                None,
                None,
                0,
                1,
                false,
            ) {
                Ok((patents, total, _)) if total > 0 && !patents.is_empty() => {
                    let p = &patents[0];
                    refs.push(json!({
                        "pub_number": pub_num,
                        "context": context,
                        "title": p.title.clone(),
                        "abstract": p.abstract_text.clone(),
                        "full_text": format!("{}\n\n{}", p.title, p.abstract_text.clone()),
                        "found": true,
                        "source": "search"
                    }));
                }
                _ => {
                    // 本地未找到，尝试在线搜索链（SerpAPI → Google Patents → EPO OPS）
                    let online_ref =
                        try_online_search(s.db.clone(), s.config.clone(), pub_num, context).await;
                    refs.push(online_ref);
                }
            }
        }
    }

    let found_count = refs
        .iter()
        .filter(|r| r["found"].as_bool().unwrap_or(false))
        .count();
    Json(json!({
        "status": "ok",
        "refs": refs,
        "total": pub_numbers.len(),
        "found_count": found_count
    }))
}

/// P2-T1: 多角色 AI 会诊面板
/// POST /api/ai/oa-panel  { my_patent, oa_text, refs }
/// 并行调用 4 个角色 + 1 个综合报告
pub async fn api_ai_oa_panel(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let my_patent = req["my_patent"].as_str().unwrap_or("");
    let oa_text = req["oa_text"].as_str().unwrap_or("");
    let refs = req["refs"].as_str().unwrap_or("");

    if my_patent.is_empty() || oa_text.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "缺少专利文本或 OA 文本 / Missing patent or OA text" })),
        );
    }

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_expert();

    // ── 截断阈值常量 ──
    // 模型：deepseek-reasoner（专家模型），上下文窗口 64K tokens。
    // 中文字符与 token 比约 1.5:1，故 64K tokens ≈ 96K 字符容量。
    // 扣除系统提示（~2K tokens）和输出预留（max_tokens=16384），可用输入约 46K tokens ≈ 69K 字符。
    //
    // 第一轮：用户上传材料，用严格变体 truncate_for_ai。50K 字符是安全阀
    // （第一轮原本全量不截断，此阈值仅防极端超长输入导致 422）。
    // 本轮目标不是第一轮——第一轮全量送入是正确行为。
    const ROUND1_SAFETY_VALVE: usize = 50_000;
    // 第二轮：角色间互驳，每角色看其余 3 方立论。12K 字符 × 3 = 36K 字符/次。
    // 36K 字符 ≈ 24K tokens，加系统提示 ~2K tokens，输入约 26K tokens，远在 46K 限制内。
    const ROUND2_MAX_CHARS: usize = 12_000;
    // 第三轮：中立裁判，看 4 方立论 + 4 方反驳共 8 份。8K 字符 × 8 = 64K 字符。
    // 64K 字符 ≈ 43K tokens，加系统提示 ~1K tokens，输入约 44K tokens，在 46K 限制内。
    // 选 8K/份而非预算制 7.5K/份：8K 是整数且 64K 字符仍有余量，无需引入额外预算函数。
    const ROUND3_MAX_CHARS: usize = 8_000;

    let user_msg = format!(
        "## 我的专利\n{}\n\n## 审查意见\n{}\n\n## 对比文献\n{}",
        truncate_for_ai(my_patent, ROUND1_SAFETY_VALVE),
        truncate_for_ai(oa_text, ROUND1_SAFETY_VALVE),
        truncate_for_ai(refs, ROUND1_SAFETY_VALVE)
    );

    // ── 第 1 轮：立论 ──
    // 4 个角色并行，各自输出编号论点 A1/A2/A3
    let examiner_sys = "你是一位中国专利审查员，刚刚发出了这份驳回决定。申请人即将提交答复。\n        请从审查员视角分析，并按以下编号格式输出论点：\n        A1: （一句主张）—— （一句依据）\n        A2: （一句主张）—— （一句依据）\n        A3: （一句主张）—— （一句依据）\n        要求：每个论点必须有明确的主张和对应的审查依据。\n        禁止假设材料中未披露的技术特征。\n        请用中文回答，态度专业客观。";

    let agent_sys = "你是一位有 20 年经验的资深中国专利代理师。\n        请从代理师视角分析，并按以下编号格式输出论点：\n        A1: （一句主张）—— （一句依据）\n        A2: （一句主张）—— （一句依据）\n        A3: （一句主张）—— （一句依据）\n        要求：论点应涵盖翻案难度、修改路径、风险点。\n        禁止假设材料中未披露的技术特征。\n        请用中文回答，给出具体可操作的建议。";

    let expert_sys = "你是一位技术专家，精通物理、化学、工程领域。\n        请从技术视角分析，并按以下编号格式输出论点：\n        A1: （一句主张）—— （一句依据）\n        A2: （一句主张）—— （一句依据）\n        A3: （一句主张）—— （一句依据）\n        要求：论点应涵盖技术实质、对比文件差异、技术贡献。\n        对比文件中披露的技术方案与本申请的技术方案在物理/化学/工程原理上是否实质等同？\n        材料未披露的技术特征不得假设。\n        请用中文回答，深入到技术原理层面。";

    let opponent_sys = "你是一位专利诉讼律师，代表对方当事人。你的目标是在专利授权后无效它。\n        请从对方视角分析，并按以下编号格式输出论点：\n        A1: （一句主张）—— （一句依据）\n        A2: （一句主张）—— （一句依据）\n        A3: （一句主张）—— （一句依据）\n        要求：论点应涵盖无效宣告路径、权利要求漏洞、对方最有利策略。\n        禁止假设材料中未披露的技术特征。\n        请用中文回答，从对抗视角出发。";

    let (examiner_res, agent_res, expert_res, opponent_res) = tokio::join!(
        ai.chat_with_system(examiner_sys, &user_msg, 0.7),
        ai.chat_with_system(agent_sys, &user_msg, 0.7),
        ai.chat_with_system(expert_sys, &user_msg, 0.7),
        ai.chat_with_system(opponent_sys, &user_msg, 0.7),
    );

    let examiner = examiner_res.unwrap_or_else(|e| format!("立论失败: {}", e));
    let agent = agent_res.unwrap_or_else(|e| format!("立论失败: {}", e));
    let expert = expert_res.unwrap_or_else(|e| format!("立论失败: {}", e));
    let opponent = opponent_res.unwrap_or_else(|e| format!("立论失败: {}", e));

    // ── 第 2 轮：交叉反驳 ──
    // 每个角色拿到其余三方的立论（truncate_for_ai_internal 12K 字），逐条点名反驳
    let rebuttal_sys_prefix = "你参与了刚才的专利 OA 答复策略讨论。现在你看到其他角色的立论。\n        请对每一方逐条回应：\n        1. 写「反驳 <角色名> <编号>：……」对你不认同的论点逐条点名反驳\n        2. 写「接受 <角色名> <编号>：……」承认被说服的点（如无，明确写「无」）\n        3. 抛出一个决定胜负的关键一问\n        禁止假设材料中未披露的技术特征。请用中文回答。\n\n";

    let _examiner_others = format!(
        "## 审查员的立论\n{}\n\n## 代理师的立论\n{}\n\n## 技术专家的立论\n{}\n\n## 对方律师的立论\n{}",
        truncate_for_ai_internal(&examiner, ROUND2_MAX_CHARS, "审查员立论"),
        truncate_for_ai_internal(&agent, ROUND2_MAX_CHARS, "代理师立论"),
        truncate_for_ai_internal(&expert, ROUND2_MAX_CHARS, "技术专家立论"),
        truncate_for_ai_internal(&opponent, ROUND2_MAX_CHARS, "对方律师立论")
    );
    // 审查员不需要看自己的立论，需要看其他三方
    let examiner_rebuttal_input = format!(
        "## 代理师的立论\n{}\n\n## 技术专家的立论\n{}\n\n## 对方律师的立论\n{}",
        truncate_for_ai_internal(&agent, ROUND2_MAX_CHARS, "代理师立论"),
        truncate_for_ai_internal(&expert, ROUND2_MAX_CHARS, "技术专家立论"),
        truncate_for_ai_internal(&opponent, ROUND2_MAX_CHARS, "对方律师立论")
    );
    let agent_rebuttal_input = format!(
        "## 审查员的立论\n{}\n\n## 技术专家的立论\n{}\n\n## 对方律师的立论\n{}",
        truncate_for_ai_internal(&examiner, ROUND2_MAX_CHARS, "审查员立论"),
        truncate_for_ai_internal(&expert, ROUND2_MAX_CHARS, "技术专家立论"),
        truncate_for_ai_internal(&opponent, ROUND2_MAX_CHARS, "对方律师立论")
    );
    let expert_rebuttal_input = format!(
        "## 审查员的立论\n{}\n\n## 代理师的立论\n{}\n\n## 对方律师的立论\n{}",
        truncate_for_ai_internal(&examiner, ROUND2_MAX_CHARS, "审查员立论"),
        truncate_for_ai_internal(&agent, ROUND2_MAX_CHARS, "代理师立论"),
        truncate_for_ai_internal(&opponent, ROUND2_MAX_CHARS, "对方律师立论")
    );
    let opponent_rebuttal_input = format!(
        "## 审查员的立论\n{}\n\n## 代理师的立论\n{}\n\n## 技术专家的立论\n{}",
        truncate_for_ai_internal(&examiner, ROUND2_MAX_CHARS, "审查员立论"),
        truncate_for_ai_internal(&agent, ROUND2_MAX_CHARS, "代理师立论"),
        truncate_for_ai_internal(&expert, ROUND2_MAX_CHARS, "技术专家立论")
    );

    let examiner_rebuttal_sys = format!(
        "{}你是审查员，请针对其他三方的论点进行反驳。",
        rebuttal_sys_prefix
    );
    let agent_rebuttal_sys = format!(
        "{}你是代理师，请针对其他三方的论点进行反驳。",
        rebuttal_sys_prefix
    );
    let expert_rebuttal_sys = format!(
        "{}你是技术专家，请针对其他三方的论点进行反驳。",
        rebuttal_sys_prefix
    );
    let opponent_rebuttal_sys = format!(
        "{}你是对方律师，请针对其他三方的论点进行反驳。",
        rebuttal_sys_prefix
    );

    let (ex_rebuttal, ag_rebuttal, expt_rebuttal, opp_rebuttal) = tokio::join!(
        ai.chat_with_system(&examiner_rebuttal_sys, &examiner_rebuttal_input, 0.7),
        ai.chat_with_system(&agent_rebuttal_sys, &agent_rebuttal_input, 0.7),
        ai.chat_with_system(&expert_rebuttal_sys, &expert_rebuttal_input, 0.7),
        ai.chat_with_system(&opponent_rebuttal_sys, &opponent_rebuttal_input, 0.7),
    );

    let examiner_rebuttal = ex_rebuttal.unwrap_or_else(|e| format!("反驳失败: {}", e));
    let agent_rebuttal = ag_rebuttal.unwrap_or_else(|e| format!("反驳失败: {}", e));
    let expert_rebuttal = expt_rebuttal.unwrap_or_else(|e| format!("反驳失败: {}", e));
    let opponent_rebuttal = opp_rebuttal.unwrap_or_else(|e| format!("反驳失败: {}", e));

    // ── 第 3 轮：中立裁判 ──
    let chair_sys = "你是一位中立裁判，负责裁决专利 OA 答复策略辩论。\n        你不替任何一方说话。请按以下结构输出：\n\n        ## 最终结论\n        （开门见山：本案应如何答复？翻盘把握 1-10 分，给出评分和理由）\n\n        ## 争议焦点裁决\n        （逐条裁决：正方编号/反方编号/谁赢/落到答复书上怎么写）\n\n        ## 共识\n        （四方一致同意的结论）\n\n        ## 未决事项\n        （无法定论的问题）\n\n        ## 答复要点\n        （落到答复书上的具体内容）\n\n        ## 行动清单\n        （具体可操作的下一步行动）\n\n        要求：结论先行，先给最终结论再展开分析。请用中文回答。";

    let chair_input = format!(
        "## 审查员立论\n{}\n\n## 代理师立论\n{}\n\n## 技术专家立论\n{}\n\n## 对方律师立论\n{}\n\n        ## 审查员反驳\n{}\n\n## 代理师反驳\n{}\n\n## 技术专家反驳\n{}\n\n## 对方律师反驳\n{}",
        truncate_for_ai_internal(&examiner, ROUND3_MAX_CHARS, "审查员立论"),
        truncate_for_ai_internal(&agent, ROUND3_MAX_CHARS, "代理师立论"),
        truncate_for_ai_internal(&expert, ROUND3_MAX_CHARS, "技术专家立论"),
        truncate_for_ai_internal(&opponent, ROUND3_MAX_CHARS, "对方律师立论"),
        truncate_for_ai_internal(&examiner_rebuttal, ROUND3_MAX_CHARS, "审查员反驳"),
        truncate_for_ai_internal(&agent_rebuttal, ROUND3_MAX_CHARS, "代理师反驳"),
        truncate_for_ai_internal(&expert_rebuttal, ROUND3_MAX_CHARS, "技术专家反驳"),
        truncate_for_ai_internal(&opponent_rebuttal, ROUND3_MAX_CHARS, "对方律师反驳")
    );

    let chair = ai
        .chat_with_system(chair_sys, &chair_input, 0.7)
        .await
        .unwrap_or_else(|e| format!("裁判失败: {}", e));

    // synthesis = chair for backward compatibility
    let synthesis = chair.clone();

    (
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "roles": {
                "examiner": examiner,
                "agent": agent,
                "expert": expert,
                "opponent": opponent
            },
            "rebuttals": {
                "examiner": examiner_rebuttal,
                "agent": agent_rebuttal,
                "expert": expert_rebuttal,
                "opponent": opponent_rebuttal
            },
            "chair": chair,
            "synthesis": synthesis
        })),
    )
}

/// P1-T3: 一审-二审对比分析
/// POST /api/ai/oa-history-compare  { patent_id, current_oa_type, current_oa_text }
/// 按专利号关联历史 OA 记录，生成 diff
pub async fn api_ai_oa_history_compare(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let patent_id = req["patent_id"].as_str().unwrap_or("");
    let current_oa_type = req["current_oa_type"]
        .as_str()
        .unwrap_or("second_rejection");
    let current_oa_text = req["current_oa_text"].as_str().unwrap_or("");

    if patent_id.is_empty() {
        return Json(json!({ "error": "缺少专利ID / Missing patent_id" }));
    }

    // 查询该专利的历史 OA 记录
    let history = match s.db.list_oa_analyses(patent_id) {
        Ok(h) => h,
        Err(e) => return Json(json!({ "error": format!("查询历史失败: {}", e) })),
    };

    if history.is_empty() {
        return Json(json!({ "status": "no_history", "message": "无历史OA记录" }));
    }

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_expert();

    let sys = "你是一位资深专利代理师，正在对比分析同一专利的多轮审查意见。\n\
        请对比一审和二审驳回决定，分析：\n\
        1. **新增驳回理由**：二审比一审新增了哪些理由？\n\
        2. **变更理由**：同样的理由，论据有什么变化？\n\
        3. **维持理由**：一审和二审都坚持的理由\n\
        4. **放弃理由**：一审有但二审没提的理由\n\
        5. **对比文献变化**：新增/删除了哪些对比文件？\n\
        6. **策略建议**：基于变化，答复策略应如何调整？\n\
        请用中文回答。";

    let mut history_text = String::new();
    for (i, h) in history.iter().enumerate() {
        history_text.push_str(&format!(
            "### 第{}轮（类型：{}，日期：{}）\n{}\n\n",
            i + 1,
            h.oa_type,
            h.created_at,
            h.analysis_text
        ));
    }

    let user_msg = format!(
        "## 历史审查意见\n{history_text}\n## 当前审查意见（类型：{current_oa_type}）\n{current_oa_text}"
    );

    let result = ai
        .chat_with_system(sys, &user_msg, 0.7)
        .await
        .unwrap_or_else(|e| format!("分析失败: {}", e));

    Json(json!({
        "status": "ok",
        "history_count": history.len(),
        "analysis": result
    }))
}
/// P2-T2: 模拟审查员预判
/// POST /api/ai/oa-examiner-preview  { my_patent, oa_text, response_text }
pub async fn api_ai_oa_examiner_preview(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let my_patent = req["my_patent"].as_str().unwrap_or("");
    let oa_text = req["oa_text"].as_str().unwrap_or("");
    let response_text = req["response_text"].as_str().unwrap_or("");

    if response_text.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "缺少答复文本 / Missing response text" })),
        );
    }

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_expert();

    let sys = "你是一位中国专利审查员，刚刚收到了申请人对你发出的驳回决定的答复。\n\
        请以审查员视角预判：\n\
        1. **可能接受的论点**（标绿）：申请人说的哪些你有倾向接受？为什么？\n\
        2. **可能反驳的论点**（标红）：哪些论点你仍要反驳？给出你的反驳理由。\n\
        3. **下一轮可能新驳回理由**：如果进入下一轮，你可能提出什么新理由？\n\
        4. **总体预判**：可能授权 / 可能部分授权 / 可能维持驳回\n\
        5. **建议修改**：如果要授权，你建议申请人怎么修改？\n\
        请用中文回答，态度专业客观。";

    let user_msg = format!(
        "## 我的专利\n{my_patent}\n\n## 我发出的驳回决定\n{oa_text}\n\n## 申请人提交的答复\n{response_text}"
    );

    let result = ai
        .chat_with_system(sys, &user_msg, 0.7)
        .await
        .unwrap_or_else(|e| format!("预判失败: {}", e));

    (
        StatusCode::OK,
        Json(json!({ "status": "ok", "preview": result })),
    )
}

/// P2-T3: 对方视角防御分析
/// POST /api/ai/oa-defense-analysis  { my_patent, claims }
pub async fn api_ai_oa_defense_analysis(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let my_patent = req["my_patent"].as_str().unwrap_or("");
    let claims = req["claims"].as_str().unwrap_or("");

    if my_patent.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "缺少专利文本 / Missing patent text" })),
        );
    }

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_expert();

    let sys = "你是一位专利诉讼律师，代表对方当事人。你的目标是在专利授权后无效它。\n\
        请从对方视角分析三个维度：\n\n\
        ## 一、无效宣告预判\n\
        1. 权利要求有哪些潜在漏洞可以利用？\n\
        2. 最有可能的无效理由是什么（新颖性/创造性/说明书充分公开/权利要求清楚）？\n\
        3. 需要准备什么证据？\n\n\
        ## 二、侵权可执行性\n\
        1. 授权后是否容易检测侵权？\n\
        2. 等同侵权范围有多大？\n\
        3. 是否有规避设计空间？\n\n\
        ## 三、商业价值评估\n\
        1. 保护范围是否太窄无商业价值？\n\
        2. 如果修改缩范围，哪些场景不受保护？\n\
        3. 对竞争对手的实际威慑力如何？\n\n\
        请用中文回答，从对抗视角出发。";

    let user_msg = if claims.is_empty() {
        format!("## 专利文本\n{my_patent}")
    } else {
        format!("## 专利文本\n{my_patent}\n\n## 权利要求\n{claims}")
    };

    let result = ai
        .chat_with_system(sys, &user_msg, 0.7)
        .await
        .unwrap_or_else(|e| format!("分析失败: {}", e));

    (
        StatusCode::OK,
        Json(json!({ "status": "ok", "analysis": result })),
    )
}

/// P3-T1: 答复策略智能推荐
/// POST /api/ai/oa-strategy-recommend  { oa_type, my_patent, refs, oa_text }
pub async fn api_ai_oa_strategy_recommend(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let oa_type = req["oa_type"].as_str().unwrap_or("second_rejection");
    let my_patent = req["my_patent"].as_str().unwrap_or("");
    let refs = req["refs"].as_str().unwrap_or("");
    let oa_text = req["oa_text"].as_str().unwrap_or("");

    if my_patent.is_empty() || oa_text.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "缺少专利文本或OA文本 / Missing patent or OA text" })),
        );
    }

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_expert();

    let (sys, user_msg) =
        crate::ai::AiClient::build_strategy_recommend_prompt(oa_type, my_patent, refs, oa_text);

    let result = ai
        .chat_with_system(&sys, &user_msg, 0.7)
        .await
        .unwrap_or_else(|e| format!("策略推荐失败: {}", e));

    (
        StatusCode::OK,
        Json(json!({ "status": "ok", "strategies": result })),
    )
}

/// POST /api/ai/oa-quality-score — P3-T3: 答复质量量化评估
/// 对生成的答复文本进行多维度量化评分（逻辑严密性、证据引用、区别特征论证、修改合理性、措辞专业度、回复完整性）
pub async fn api_ai_oa_quality_score(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let response_text = req["response_text"].as_str().unwrap_or("");
    let oa_text = req["oa_text"].as_str().unwrap_or("");
    let my_patent = req["my_patent"].as_str().unwrap_or("");

    if response_text.is_empty() || oa_text.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "缺少答复文本或OA文本 / Missing response or OA text" })),
        );
    }

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_expert();

    let sys = "你是一位专利答复质量评审专家，精通中国专利法及审查指南。\n        请对以下答复文本进行多维度量化评分（每项 0-10 分，保留一位小数）：\n\n        评分维度：\n        1. 逻辑严密性（logic）：论证链条是否完整，有无逻辑跳跃，因果关系是否成立\n        2. 证据引用充分性（evidence）：是否引用了具体段落、附图标记、对比文件编号\n        3. 区别技术特征论证（distinction）：是否清晰区分了本申请与对比文件的技术特征\n        4. 修改合理性（modification）：权利要求修改是否恰当，有无超出原说明书范围\n        5. 措辞专业度（tone）：用语是否符合专利答复规范，有无口语化或情绪化表达\n        6. 回复完整性（completeness）：是否逐一回应了所有驳回理由\n\n        输出格式（严格按此 JSON 结构，不要加 markdown 代码块标记）：\n        {\n          \"scores\": {\n            \"logic\": 8.5,\n            \"evidence\": 7.0,\n            \"distinction\": 9.0,\n            \"modification\": 8.0,\n            \"tone\": 7.5,\n            \"completeness\": 8.0\n          },\n          \"total\": 8.0,\n          \"grade\": \"B\",\n          \"weaknesses\": \"薄弱点分析...\",\n          \"suggestions\": \"改进建议...\",\n          \"detail\": \"详细评分说明...\",\n          \"not_applicable\": []\n        }\n\n        评分标准：\n        - 9-10: 优秀，该维度无明显缺陷\n        - 7-8: 良好，有小瑕疵但不影响整体\n        - 5-6: 一般，存在明显不足\n        - 3-4: 较差，严重缺失或错误\n        - 0-2: 极差，完全缺失\n\n        total = 加权平均（logic 20%, evidence 15%, distinction 25%, modification 15%, tone 10%, completeness 15%）\n        grade: A(≥9), B(≥7), C(≥5), D(<5)\n        not_applicable: 列出不适用的维度名称（如无权利要求修改则 modification 不适用）\n\n        请严格按 JSON 格式输出，不要加任何其他文字。";

    let user_msg = format!(
        "## 我的专利\n{my_patent}\n\n## 审查意见\n{oa_text}\n\n## 答复文本\n{response_text}"
    );

    let result = ai
        .chat_with_system(sys, &user_msg, 0.3)
        .await
        .unwrap_or_else(|e| format!("AI call failed: {e}"));

    // #100: parse the model output into a structured result.
    // Previously, the raw text was wrapped in {"raw": ...} and returned as
    // {"status":"ok","score":{"raw":"..."}} — making it impossible for the UI
    // to extract a numeric score.
    let score_result = parse_quality_score(&result);
    (StatusCode::OK, Json(score_result))
}

/// #100: Parse the model's quality-score output into a structured result.
///
/// Expected model output (JSON):
/// ```json
/// { "scores": {"logic": 8.5, ...}, "total": 8.0, "grade": "B", ... }
/// ```
///
/// Returns a JSON object with:
/// - `status`: "ok" or "error"
/// - `parsed`: true if the model output was successfully parsed
/// - `score`: numeric score (f64) — only present if parsed
/// - `max`: maximum possible score (10.0)
/// - `dimensions`: array of {name, score, comment} — only present if parsed
/// - `raw`: the original model output (always present)
/// - `error`: error message if parsing failed or score out of bounds
fn parse_quality_score(raw: &str) -> serde_json::Value {
    // Strip markdown code fences if present (```json ... ```)
    let trimmed = raw.trim();
    let json_str = if trimmed.starts_with("```") {
        let inner = trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```"))
            .unwrap_or(trimmed);
        inner
            .rsplit_once("```")
            .map(|(s, _)| s)
            .unwrap_or(inner)
            .trim()
    } else {
        trimmed
    };

    let parsed_json: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => {
            // #100: parsing failed — must NOT wrap raw text as if it were a
            // successful score.  Return parsed:false with the reason.
            return json!({
                "status": "ok",
                "parsed": false,
                "error": format!("Failed to parse model output as JSON: {e}"),
                "raw": raw
            });
        }
    };

    // Check for error in model output
    if let Some(err) = parsed_json["error"].as_str() {
        return json!({
            "status": "error",
            "parsed": false,
            "error": err,
            "raw": raw
        });
    }

    // Extract the total score as a number
    let score = parsed_json["total"].as_f64();
    let score = match score {
        Some(s) => s,
        None => {
            // Try "score" field as fallback
            match parsed_json["score"].as_f64() {
                Some(s) => s,
                None => {
                    return json!({
                        "status": "ok",
                        "parsed": false,
                        "error": "Model output does not contain a numeric 'total' or 'score' field",
                        "raw": raw
                    });
                }
            }
        }
    };

    // #100: check for out-of-bounds score (expected range: 0-10)
    const SCORE_MIN: f64 = 0.0;
    const SCORE_MAX: f64 = 10.0;
    let out_of_bounds = score < SCORE_MIN || score > SCORE_MAX;

    // Build dimensions array from the scores object
    let dimensions: Vec<serde_json::Value> =
        if let Some(scores_obj) = parsed_json["scores"].as_object() {
            scores_obj
                .iter()
                .map(|(name, val)| {
                    let dim_score = val.as_f64().unwrap_or(0.0);
                    json!({
                        "name": name,
                        "score": dim_score,
                        "comment": ""
                    })
                })
                .collect()
        } else {
            Vec::new()
        };

    let mut result = json!({
        "status": "ok",
        "parsed": true,
        "score": score,
        "max": SCORE_MAX,
        "dimensions": dimensions,
        "raw": raw,
        "grade": parsed_json["grade"].clone(),
        "weaknesses": parsed_json["weaknesses"].clone(),
        "suggestions": parsed_json["suggestions"].clone(),
    });

    if out_of_bounds {
        result["out_of_bounds"] = json!(true);
        result["warning"] = json!(format!("Score {} is outside expected range [0, 10]", score));
    }

    result
}

/// POST /api/ai/oa-claim-simulate — P3-T2: 权利要求修改模拟器
/// 用户输入修改后的权利要求，AI 模拟审查员审查，判断是否可能授权
pub async fn api_ai_oa_claim_simulate(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let original_claims = req["original_claims"].as_str().unwrap_or("");
    let modified_claims = req["modified_claims"].as_str().unwrap_or("");
    let refs = req["refs"].as_str().unwrap_or("");
    let oa_text = req["oa_text"].as_str().unwrap_or("");

    if modified_claims.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "缺少修改后权利要求 / Missing modified claims" })),
        );
    }

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_expert();

    let sys = "你是一位专利审查员，申请人提交了修改后的权利要求。\n        请逐项检查并给出明确结论：\n\n        检查项目：\n        1. 说明书支持（support_check）：修改后的权项是否能在说明书中找到依据？有无超范围修改？\n        2. 新颖性（novelty_check）：对比各对比文件 D1/D2/...，修改后是否新颖？\n        3. 创造性（inventiveness_check）：对比文件组合能否显而易见得到修改后的方案？\n        4. 保护范围（scope_check）：修改后范围是否合理？是否过度缩小导致保护价值丧失？\n        5. 总体判断（overall）：可能授权 / 需要进一步修改 / 仍有问题\n        6. 修改建议（suggestions）：如果需要进一步修改，给出具体建议\n\n        输出格式（严格按此 JSON 结构，不要加 markdown 代码块标记）：\n        {\n          \"support_check\": \"通过/不通过：具体分析...\",\n          \"novelty_check\": \"通过/不通过：具体分析...\",\n          \"inventiveness_check\": \"通过/不通过：具体分析...\",\n          \"scope_check\": \"合理/过窄/过宽：具体分析...\",\n          \"overall\": \"可能授权/需要进一步修改/仍有问题\",\n          \"suggestions\": \"具体修改建议...\",\n          \"risk_level\": \"低/中/高\"\n        }\n\n        请严格按 JSON 格式输出，不要加任何其他文字。";

    let user_msg = format!(
        "## 原始权利要求\n{original_claims}\n\n## 修改后权利要求\n{modified_claims}\n\n## 审查意见\n{oa_text}\n\n## 对比文献\n{refs}"
    );

    let result = ai
        .chat_with_system(sys, &user_msg, 0.3)
        .await
        .unwrap_or_else(|e| format!("{{\"error\": \"模拟审查失败: {e}\"}}"));

    let parsed: serde_json::Value =
        serde_json::from_str(&result).unwrap_or_else(|_| json!({ "raw": result }));

    (
        StatusCode::OK,
        Json(json!({ "status": "ok", "simulation": parsed })),
    )
}

/// T1: 在线搜索链 fallback — 当本地库未找到对比文献时，尝试在线搜索
async fn try_online_search(
    db: std::sync::Arc<crate::db::Database>,
    config: std::sync::Arc<std::sync::RwLock<crate::routes::AppConfig>>,
    pub_num: &str,
    context: &str,
) -> serde_json::Value {
    let (serpapi_key, epo_credentials) = {
        let cfg = config.read().unwrap_or_else(|e| e.into_inner());
        let serpapi_key = cfg.next_serpapi_key();
        let epo_credentials = {
            let epo_key = cfg.epo_key.trim();
            let epo_secret = cfg.epo_secret.trim();
            if !epo_key.is_empty() && !epo_secret.is_empty() {
                Some((epo_key.to_string(), epo_secret.to_string()))
            } else {
                None
            }
        };
        (serpapi_key, epo_credentials)
    };

    // 构建在线搜索链（SerpAPI → Google Patents → EPO OPS）
    let mut providers: Vec<std::sync::Arc<dyn SearchProvider>> = Vec::new();
    if let Some(api_key) = serpapi_key {
        providers.push(std::sync::Arc::new(SerpApiProvider::new(
            api_key,
            db.clone(),
        )));
    }
    providers.push(std::sync::Arc::new(GooglePatentsXhrProvider::new(
        db.clone(),
    )));
    if let Some((epo_key, epo_secret)) = epo_credentials {
        providers.push(std::sync::Arc::new(EpoOpsProvider::new(
            epo_key,
            epo_secret,
            db.clone(),
        )));
    }

    if providers.is_empty() {
        return json!({
            "pub_number": pub_num,
            "context": context,
            "title": "",
            "abstract": "",
            "full_text": "",
            "found": false,
            "source": ""
        });
    }

    let chain = SourceChain::new(providers);
    let query = SearchQuery {
        keyword: pub_num.to_string(),
        country: None,
        language: Some(Lang::English),
        assignee: None,
        exact_assignee: false,
        date_from: None,
        date_to: None,
        limit: 1,
        page: 0,
        sort_by: None,
        search_type: Some(crate::types::search::SearchType::PatentNumber),
    };

    // 超时保护：单次在线搜索最多 10 秒
    match tokio::time::timeout(std::time::Duration::from_secs(10), chain.run(query)).await {
        Ok(outcome) if !outcome.results.is_empty() => {
            let m = &outcome.results[0];
            let title = m.summary.title.clone();
            let abstract_text = m.summary.abstract_text.clone();
            let full_text = if title.is_empty() && abstract_text.is_empty() {
                String::new()
            } else {
                format!("{}\n\n{}", title, abstract_text)
            };
            json!({
                "pub_number": pub_num,
                "context": context,
                "title": title,
                "abstract": abstract_text,
                "full_text": full_text,
                "found": true,
                "source": "online"
            })
        }
        _ => {
            // 在线搜索也未找到或超时
            json!({
                "pub_number": pub_num,
                "context": context,
                "title": "",
                "abstract": "",
                "full_text": "",
                "found": false,
                "source": ""
            })
        }
    }
}

/// POST /api/ai/oa-collect-evidence — P4-T1: 证据自动收集与整理
///
/// 从专利全文、对比文献、OA 文本中自动提取证据段落，按类型分类整理。
/// 请求: { my_patent, refs, oa_text }
/// 响应: { status: "ok", evidence: [Evidence], summary: String, stats: {...} }
pub async fn api_ai_oa_collect_evidence(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let my_patent = req["my_patent"].as_str().unwrap_or("");
    let refs = req["refs"].as_str().unwrap_or("");
    let oa_text = req["oa_text"].as_str().unwrap_or("");

    if my_patent.is_empty() || oa_text.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "status": "error",
                "message": "缺少专利文本或OA文本 / Missing patent or OA text"
            })),
        );
    }

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_expert();

    match crate::ai::collect_evidence(&ai, my_patent, refs, oa_text).await {
        Ok((evidence, summary, anchor_report)) => {
            // 统计各类型数量
            let mut stats = serde_json::Map::new();
            let mut total_relevance = 0.0f32;
            for e in &evidence {
                let key = e.evidence_type.as_key();
                let count = stats.get(key).and_then(|v| v.as_u64()).unwrap_or(0);
                stats.insert(key.to_string(), json!(count + 1));
                total_relevance += e.relevance;
            }
            let avg_relevance = if evidence.is_empty() {
                0.0
            } else {
                total_relevance / evidence.len() as f32
            };

            (
                StatusCode::OK,
                Json(json!({
                    "status": "ok",
                    "evidence": evidence,
                    "summary": summary,
                    "stats": {
                        "type_counts": stats,
                        "total": evidence.len(),
                        "avg_relevance": avg_relevance,
                        // 锚点核对结果：total = located + unlocated，模型自报的 source 一律不采信
                        "anchor": serde_json::to_value(&anchor_report)
                            .unwrap_or(serde_json::Value::Null)
                    }
                })),
            )
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "status": "error",
                "message": format!("证据收集失败: {e}")
            })),
        ),
    }
}

/// POST /api/ai/oa-adversarial — 多轮对抗推演
///
/// 审查员与代理师多轮对抗（默认3轮），输出经压力测试的终稿答复书。
/// 每轮：审查员质疑 → 代理师回应。最后整合输出终稿 + 信心度 + 风险点。
pub async fn api_ai_oa_adversarial(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let my_patent = req["my_patent"].as_str().unwrap_or("");
    let claims = req["claims"].as_str().unwrap_or("");
    let oa_text = req["oa_text"].as_str().unwrap_or("");
    let refs = req["refs"].as_str().unwrap_or("");
    let rounds = req["rounds"].as_u64().unwrap_or(5) as usize;
    let focus = req["focus"].as_str().unwrap_or("创造性");
    let goal_threshold = req["goal_threshold"].as_f64().unwrap_or(0.85) as f32;

    if my_patent.is_empty() || oa_text.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "缺少专利文本或OA文本 / Missing patent or OA text" })),
        );
    }

    let params = crate::ai::AdversarialParams {
        my_patent: my_patent.to_string(),
        claims: claims.to_string(),
        oa_text: oa_text.to_string(),
        refs: refs.to_string(),
        rounds,
        focus: focus.to_string(),
        goal_threshold,
    };

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_expert();

    let result = crate::ai::run_adversarial_deliberation(&ai, &params).await;

    (
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "transcript": result.transcript,
            "final_response": result.final_response,
            "confidence_score": result.confidence_score,
            "weak_points": result.weak_points,
            "rounds_completed": result.rounds_completed,
            "converged": result.converged,
            "goal_achieved": result.goal_achieved,
            "goal_threshold": result.goal_threshold
        })),
    )
}

/// Batch 5: 专利附图多模态看图联动与标号视觉核对
/// POST /api/ai/vision-figure-check
/// 接收 { patent_id?, image_base64_urls?, spec_text? } -> 返回 FigureAnalysisResult JSON
pub async fn api_ai_vision_figure_check(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let patent_id = req["patent_id"].as_str().unwrap_or("").trim();
    let mut spec_text = req["spec_text"].as_str().unwrap_or("").trim().to_string();

    if spec_text.is_empty() && !patent_id.is_empty() {
        if let Ok(Some(p)) = s.db.get_patent(patent_id) {
            spec_text = format!(
                "{}\n\n{}\n\n{}\n\n{}",
                p.title, p.abstract_text, p.claims, p.description
            );
        }
    }

    let images: Vec<String> = req["image_base64_urls"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::trim))
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();

    let vision_opt = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_vision();

    let (client_ref, model_label) = match vision_opt.as_ref() {
        Some((c, m)) => (Some(c), m.as_str()),
        None => (None, "none"),
    };

    let result = crate::ai::vision_topology::analyze_figure_with_vision(
        client_ref,
        &images,
        &spec_text,
        model_label,
    )
    .await;

    (StatusCode::OK, Json(json!(result)))
}
