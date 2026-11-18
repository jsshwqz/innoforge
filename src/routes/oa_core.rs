use super::*;

/// 从请求体或备选案号中解析 `case_id` 并提取多轮 OA 视图漂移与禁止反言约束。
fn resolve_oa_drift_report(
    db: &crate::db::Database,
    req: &serde_json::Value,
    fallback_case_id: Option<&str>,
) -> Option<crate::ai::oa_drift::OaDriftReport> {
    let candidates = [
        req.get("case_id").and_then(|v| v.as_str()),
        req.get("patent_id").and_then(|v| v.as_str()),
        req.get("my_patent_id").and_then(|v| v.as_str()),
        fallback_case_id,
    ];
    for cid in candidates.into_iter().flatten() {
        let trimmed = cid.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(report) = crate::ai::oa_drift::analyze_case_drift(db, trimmed) {
            if !report.prompt_constraints_block.is_empty() {
                return Some(report);
            }
        }
    }
    None
}

/// 将策略提示与多轮 OA 一致性红线注入专利背景上下文，供各类型 OA 分析复用。
fn prepend_strategy_and_drift_constraints(
    my_info: &str,
    strategy_hint: Option<&str>,
    drift_report: Option<&crate::ai::oa_drift::OaDriftReport>,
    client_constraints: Option<&str>,
) -> String {
    let mut prefix = String::new();
    if let Some(hint) = strategy_hint.map(str::trim).filter(|s| !s.is_empty()) {
        prefix.push_str(&format!("【用户选择的答复策略：{hint}】\n\n"));
    }
    if let Some(report) = drift_report {
        if !report.prompt_constraints_block.is_empty() {
            prefix.push_str(&report.prompt_constraints_block);
            prefix.push_str("\n\n");
        }
    } else if let Some(hc) = client_constraints.map(str::trim).filter(|s| !s.is_empty()) {
        prefix.push_str(hc);
        prefix.push_str("\n\n");
    }
    if prefix.is_empty() {
        my_info.to_string()
    } else {
        format!("{prefix}{my_info}")
    }
}

/// Office action response analysis: deep analysis for responding to examination opinions
pub async fn api_ai_office_action_response(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    // my_patent: text object (uploaded file with content) or patent ID string
    tracing::info!(
        "[OA-ANALYZE] 收到请求, body keys={:?}",
        req.as_object().map(|o| o.keys().collect::<Vec<_>>())
    );
    // 日志在 AI 调用前后各一条，方便定位
    let my_info = if let Some(obj) = req["my_patent"].as_object() {
        if obj
            .get("content")
            .and_then(|v| v.as_str())
            .is_some_and(|c| c.len() > 10)
        {
            let title = obj
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("我的专利");
            let content = obj.get("content").and_then(|v| v.as_str()).unwrap_or("");
            format!("标题：{}\n\n{}", title, content)
        } else {
            match resolve_my_patent(&s.db, &req) {
                Ok(info) => info,
                Err(e) => return Json(json!({ "error": e })),
            }
        }
    } else {
        match resolve_my_patent(&s.db, &req) {
            Ok(info) => info,
            Err(e) => return Json(json!({ "error": e })),
        }
    };

    // office_action: text object or plain string
    let oa_text = if let Some(obj) = req["office_action"].as_object() {
        obj.get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        req["office_action"].as_str().unwrap_or("").to_string()
    };
    if oa_text.trim().len() < 10 {
        return Json(json!({"error": "请输入审查意见通知书内容"}));
    }

    // P0-4: 提取 oa_date 结构化字段，注入 OA 文本头部，防止 LLM 从正文中抓错日期
    let oa_date = req
        .get("oa_date")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    let oa_text = if let Some(date) = oa_date {
        format!(
            "【驳回决定发文日（结构化字段，以此为准）：{}】\n\n             注意：OA 正文中可能提及多个日期（如申请人提交答复的日期），不得混淆。\n             上述发文日期是驳回决定的发出日期，用于期限计算等程序事项。\n\n\n{}",
            date, oa_text
        )
    } else {
        oa_text
    };

    // references: array of text objects or patent ID strings
    let refs = req["references"].as_array();
    let refs_info = match refs {
        Some(arr) if !arr.is_empty() => {
            let mut info = String::new();
            for (i, item) in arr.iter().enumerate() {
                if let Some(obj) = item.as_object() {
                    if obj.get("type").and_then(|v| v.as_str()) == Some("text") {
                        let title = obj
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("对比文献");
                        let content = obj.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        info.push_str(&format!(
                            "### 对比文献 {} — {}\n{}\n\n",
                            i + 1,
                            title,
                            content
                        ));
                        continue;
                    }
                }
                let id = item.as_str().unwrap_or("");
                if !id.is_empty() {
                    if let Ok(Some(p)) = s.db.get_patent(id) {
                        info.push_str(&format!(
                            "### 对比文献 {} — {}\n专利号：{}\n标题：{}\n摘要：{}\n权利要求：\n{}\n说明书（前部分）：\n{}\n\n",
                            i + 1, p.patent_number, p.patent_number, p.title,
                            p.abstract_text, p.claims,
                            truncate_for_ai(&p.description, 200_000)
                        ));
                    }
                }
            }
            info
        }
        _ => String::new(),
    };

    // MB5: OA 容量统一为「报错不截断」——非流式路径
    for (field, value, max_chars) in [
        ("my_patent", &my_info, OA_RESPONSE_ANALYSIS_MAX_CHARS),
        ("office_action", &oa_text, OA_RESPONSE_OA_MAX_CHARS),
        ("references", &refs_info, OA_RESPONSE_DISCUSSION_MAX_CHARS),
    ] {
        if let Some(error) = oa_capacity_error(field, value, max_chars) {
            return Json(json!({ "error": error }));
        }
    }

    // 从请求中提取专利号，用于缓存命中判断
    let patent_number = req
        .get("my_patent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let force_reanalyze = req.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

    // 缓存命中：同一专利号 + 同一 OA 类型 + 同深度 → 直接返回
    if !patent_number.is_empty() && !force_reanalyze {
        let oa_type_hint = req
            .get("oa_type")
            .and_then(|v| v.as_str())
            .filter(|&v| {
                v == "abnormal"
                    || v == "reject_review"
                    || v == "first_exam"
                    || v == "second_rejection"
                    || v == "reexamination_request"
                    || v == "reexamination_decision"
                    || v == "admin_lawsuit"
            })
            .unwrap_or("first_exam");
        let depth_hint = req
            .get("depth")
            .and_then(|v| v.as_str())
            .filter(|&v| v == "shallow" || v == "deep")
            .unwrap_or("deep");
        let depth_key = if depth_hint == "shallow" {
            "shallow"
        } else {
            "deep"
        };
        if let Ok(existing) = s.db.list_oa_analyses(patent_number) {
            if let Some(cached) = existing
                .iter()
                .find(|a| a.oa_type == oa_type_hint && a.depth == depth_key)
            {
                let patent_title = req
                    .get("my_patent")
                    .and_then(|v| v.as_object())
                    .and_then(|o| o.get("title"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(patent_number);
                return Json(json!({
                    "status": "ok",
                    "analysis": cached.analysis_text,
                    "cached": true,
                    "version": cached.version,
                    "patent_number": patent_number,
                    "patent_title": patent_title
                }));
            }
        }
    }

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_expert();
    let oa_type = req
        .get("oa_type")
        .and_then(|v| v.as_str())
        .filter(|&v| {
            v == "abnormal"
                || v == "reject_review"
                || v == "first_exam"
                || v == "second_rejection"
                || v == "reexamination_request"
                || v == "reexamination_decision"
                || v == "admin_lawsuit"
        })
        .unwrap_or("first_exam");
    let depth = req
        .get("depth")
        .and_then(|v| v.as_str())
        .filter(|&v| v == "shallow" || v == "deep")
        .unwrap_or("medium");
    let discuss = req
        .get("discuss")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // T3: 提取一审历史上下文（可选）
    let first_exam_context = req
        .get("first_exam_context")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // P3-T1: 提取用户选择的策略提示（可选）
    let strategy_hint = req
        .get("strategy_hint")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let drift_report = resolve_oa_drift_report(
        &s.db,
        &req,
        if patent_number.is_empty() {
            None
        } else {
            Some(patent_number)
        },
    );
    let client_constraints = req.get("historical_constraints").and_then(|v| v.as_str());
    let my_info_with_strategy = prepend_strategy_and_drift_constraints(
        &my_info,
        strategy_hint.as_deref(),
        drift_report.as_ref(),
        client_constraints,
    );
    tracing::info!(
        "[OA-ANALYZE] 开始调用AI, my_info_len={}, oa_text_len={}, refs_len={}, oa_type={}",
        my_info_with_strategy.len(),
        oa_text.len(),
        refs_info.len(),
        oa_type
    );
    match ai
        .office_action_response(
            &my_info_with_strategy,
            &oa_text,
            &refs_info,
            oa_type,
            depth,
            discuss,
            first_exam_context.as_deref(),
        )
        .await
    {
        Ok(content) => {
            tracing::info!("[OA-ANALYZE] AI返回成功, content_len={}", content.len());
            let _ = s.db.log_ai_call(&ai, "ai-oa-response", None, None);
            // 自动保存到历史
            let patent_title = req
                .get("my_patent")
                .and_then(|v| v.as_object())
                .and_then(|o| o.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or(patent_number);
            // Only cache non-empty analysis results — caching empty text would
            // cause subsequent requests to return an empty cached response.
            if !patent_number.is_empty() && content.trim().len() > 10 {
                let _ =
                    s.db.save_oa_analysis(patent_number, patent_title, oa_type, depth, &content);
            }
            // If AI returned empty or near-empty content, return an error so
            // the frontend shows a clear message instead of a blank result.
            if content.trim().len() < 10 {
                Json(
                    json!({"error": format!("AI 返回内容为空或过短（{}字符），可能 AI 服务异常，请重试或检查 AI 配置", content.len())}),
                )
            } else {
                // P0-3c: 转述冲突检测 — refs 有全文(>500字)时自动触发
                let mut final_content = content.clone();
                if refs_info.len() > 500 {
                    tracing::info!(
                        "[OA-CONFLICT] refs_info_len={}, 触发转述冲突检测",
                        refs_info.len()
                    );
                    match ai.detect_paraphrase_conflicts(&oa_text, &refs_info).await {
                        Ok(conflict_report) if conflict_report.trim().len() > 20 => {
                            final_content
                                .push_str("\n\n---\n\n## 转述冲突检测（可主张审查员认定错误）\n\n");
                            final_content.push_str(&conflict_report);
                            tracing::info!(
                                "[OA-CONFLICT] 检测完成, report_len={}",
                                conflict_report.len()
                            );
                        }
                        Ok(_) => {
                            tracing::info!("[OA-CONFLICT] 检测完成，无冲突");
                        }
                        Err(e) => {
                            tracing::warn!("[OA-CONFLICT] 检测失败: {}", e);
                        }
                    }
                }
                Json(json!({"status": "ok", "analysis": final_content}))
            }
        }
        Err(e) => Json(json!({ "error": format!("分析失败: {}", e) })),
    }
}

/// SSE 流式 OA 分析 / Streaming OA analysis with SSE
pub async fn api_ai_office_action_response_stream(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Sse<Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>> {
    // Helper: wrap a single error into an SSE stream
    fn error_sse(msg: String) -> Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> {
        Box::pin(futures::stream::once(async move {
            Ok(Event::default().event("error").data(msg))
        }))
    }

    // Extract my_patent info
    let my_info = match req["my_patent"].as_object() {
        Some(obj)
            if obj
                .get("content")
                .and_then(|v| v.as_str())
                .is_some_and(|c| c.len() > 10) =>
        {
            let title = obj
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("我的专利");
            let content = obj.get("content").and_then(|v| v.as_str()).unwrap_or("");
            format!("标题：{}\n\n{}", title, content)
        }
        _ => match resolve_my_patent(&s.db, &req) {
            Ok(info) => info,
            Err(e) => return Sse::new(error_sse(format!("[ERROR] {}", e))),
        },
    };

    // Extract office_action text
    let oa_text = match req["office_action"].as_object() {
        Some(obj) => obj
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        None => req["office_action"].as_str().unwrap_or("").to_string(),
    };
    if oa_text.trim().len() < 10 {
        return Sse::new(error_sse("[ERROR] 请输入审查意见通知书内容".into()));
    }

    // P0-4: 提取 oa_date 结构化字段，注入 OA 文本头部，防止 LLM 从正文中抓错日期
    let oa_date = req
        .get("oa_date")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    let oa_text = if let Some(date) = oa_date {
        format!(
            "【驳回决定发文日（结构化字段，以此为准）：{}】\n\n             注意：OA 正文中可能提及多个日期（如申请人提交答复的日期），不得混淆。\n             上述发文日期是驳回决定的发出日期，用于期限计算等程序事项。\n\n\n{}",
            date, oa_text
        )
    } else {
        oa_text
    };

    // Extract references info
    let refs_info = {
        let mut info = String::new();
        if let Some(arr) = req["references"].as_array() {
            for (i, item) in arr.iter().enumerate() {
                if let Some(obj) = item.as_object() {
                    if obj.get("type").and_then(|v| v.as_str()) == Some("text") {
                        let title = obj
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("对比文献");
                        let content = obj.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        info.push_str(&format!(
                            "### 对比文献 {} — {}\n{}\n\n",
                            i + 1,
                            title,
                            content
                        ));
                        continue;
                    }
                }
                if let Some(id) = item.as_str() {
                    if !id.is_empty() {
                        if let Ok(Some(p)) = s.db.get_patent(id) {
                            info.push_str(&format!(
                                "### 对比文献 {} — {}\n专利号：{}\n标题：{}\n摘要：{}\n权利要求：\n{}\n说明书（前部分）：\n{}\n\n",
                                i + 1, p.patent_number, p.patent_number, p.title,
                                p.abstract_text, p.claims,
                                truncate_for_ai(&p.description, 200_000)
                            ));
                        }
                    }
                }
            }
        }
        info
    };

    // UA1: 自动抽取 OA 文本中的对比文件公开号，补充用户未手贴的对比文献
    let refs_info = {
        let auto_pubs = extract_publication_numbers(&oa_text);
        if auto_pubs.is_empty() {
            refs_info
        } else {
            // 实际上需要检查 refs_info 中已有的公开号
            let mut supplemented = refs_info.clone();
            let mut idx = 1;
            for (pub_num, label) in &auto_pubs {
                // 如果用户已手贴该公开号，跳过（手贴优先级更高）
                if refs_info.contains(pub_num) {
                    continue;
                }
                // 尝试从数据库获取
                match s.db.get_patent(pub_num) {
                    Ok(Some(p)) => {
                        supplemented.push_str(&format!(
                            "### 自动抓取对比文献 D{idx} — {pub_num}（{label}）\n                             专利号：{}\n标题：{}\n摘要：{}\n权利要求：\n{}\n说明书（前部分）：\n{}\n\n",
                            p.patent_number, p.title, p.abstract_text, p.claims,
                            truncate_for_ai(&p.description, 200_000)
                        ));
                        idx += 1;
                    }
                    Ok(None) => {
                        supplemented.push_str(&format!(
                            "### 自动抓取对比文献 D{idx} — {pub_num}（{label}）\n                             [reason_code: not_found] 未在数据库中找到该公开号，请手贴全文\n\n"
                        ));
                        idx += 1;
                    }
                    Err(_) => {
                        supplemented.push_str(&format!(
                            "### 自动抓取对比文献 D{idx} — {pub_num}（{label}）\n                             [reason_code: network_error] 数据库查询失败，请手贴全文\n\n"
                        ));
                        idx += 1;
                    }
                }
            }
            supplemented
        }
    };

    // MB5: OA 容量统一为「报错不截断」——流式路径
    for (field, value, max_chars) in [
        ("my_patent", &my_info, OA_RESPONSE_ANALYSIS_MAX_CHARS),
        ("office_action", &oa_text, OA_RESPONSE_OA_MAX_CHARS),
        ("references", &refs_info, OA_RESPONSE_DISCUSSION_MAX_CHARS),
    ] {
        if let Some(error) = oa_capacity_error(field, value, max_chars) {
            return Sse::new(error_sse(error));
        }
    }

    let oa_type = req
        .get("oa_type")
        .and_then(|v| v.as_str())
        .filter(|&v| {
            v == "abnormal"
                || v == "reject_review"
                || v == "first_exam"
                || v == "second_rejection"
                || v == "reexamination_request"
                || v == "reexamination_decision"
                || v == "admin_lawsuit"
        })
        .or_else(|| {
            // Phase 4: CaseInput JSON 格式从 case_meta.oa_type 提取
            req.get("case_meta")
                .and_then(|m| m.get("oa_type"))
                .and_then(|v| v.as_str())
                .filter(|&v| {
                    v == "reject_review"
                        || v == "reexamination_request"
                        || v == "reexamination_decision"
                })
        })
        .unwrap_or("first_exam")
        .to_string();

    let depth = req
        .get("depth")
        .and_then(|v| v.as_str())
        .filter(|&v| v == "shallow" || v == "deep")
        .unwrap_or("medium")
        .to_string();
    let discuss = req
        .get("discuss")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_expert();
    // T3: 提取一审历史上下文（可选）
    let first_exam_context = req
        .get("first_exam_context")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // P3-T1: 提取用户选择的策略提示（可选）
    let strategy_hint = req
        .get("strategy_hint")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let drift_report = resolve_oa_drift_report(&s.db, &req, None);
    let client_constraints = req.get("historical_constraints").and_then(|v| v.as_str());
    let my_info_with_strategy = prepend_strategy_and_drift_constraints(
        &my_info,
        strategy_hint.as_deref(),
        drift_report.as_ref(),
        client_constraints,
    );

    let mut rx = ai.office_action_response_stream(
        &my_info_with_strategy,
        &oa_text,
        &refs_info,
        &oa_type,
        &depth,
        discuss,
        first_exam_context.as_deref(),
    );

    // Capture values for auto-save (must be before consuming s)
    let patent_number = req
        .get("my_patent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let patent_title = req
        .get("my_patent")
        .and_then(|v| v.as_object())
        .and_then(|o| o.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or(&patent_number)
        .to_string();
    let db = s.db.clone();
    let fact_references = refs_info.clone();
    let fact_my_patent = my_info.clone();
    // P0-3c: 捕获供转述冲突检测使用
    let conflict_oa_text = oa_text.clone();
    let conflict_refs_info = refs_info.clone();
    let conflict_ai = ai.clone();

    let stream = async_stream::stream! {
        let mut full_text = String::new();
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(5));
        heartbeat.tick().await; // skip first immediate tick
        loop {
            tokio::select! {
                chunk = rx.recv() => {
                    match chunk {
                        Some(chunk) => {
                            if chunk.starts_with("[ERROR]") {
                                yield Ok(Event::default().event("error").data(chunk));
                                return;
                            }
                            // EK-P6 修复：累积原文（核验/落库需要句边界），
                            // SSE 载荷用 JSON 编码保单行（前端 sseDecode 还原）
                            full_text.push_str(&chunk);
                            yield Ok(Event::default().data(sse_encode(&chunk)));
                        }
                        None => {
                            // M2-A：通道关闭而全文为空 = 静默失败，必须以 error 事件可见
                            if full_text.trim().is_empty() {
                                yield Ok(Event::default().event("error").data(
                                    "[ERROR] 主分析流结束但无任何内容——请检查模型通道或超时配置",
                                ));
                                return;
                            }
                            break;
                        }
                    }
                }
                _ = heartbeat.tick() => {
                    yield Ok(Event::default().event("heartbeat").data(""));
                }
            }
        }
        // M2-C（审查 P1-9）：M2-A 保证空主分析在 worker 层已发 [ERROR]（路由层
        // None 分支同样兜底），走到这里 full_text 必有主内容——来源恒为
        // main_analysis，conflict_report_only 死分支删除。
        let fact_report = check_oa_analysis(&full_text, &fact_references, &fact_my_patent);
        let fact_text = format!("\n\n## AI 事实核查（请人工复核）\n{}", format_report(&fact_report));
        full_text.push_str(&fact_text);
        yield Ok(Event::default().data(sse_encode(&fact_text)));

        // P0-3c: 转述冲突检测 — refs 有全文(>500字)时自动触发
        if conflict_refs_info.len() > 500 {
            tracing::info!("[OA-CONFLICT-STREAM] refs_info_len={}, 触发转述冲突检测", conflict_refs_info.len());
            match conflict_ai.detect_paraphrase_conflicts(&conflict_oa_text, &conflict_refs_info).await {
                Ok(conflict_report) if conflict_report.trim().len() > 20 => {
                    let conflict_text = format!("\n\n---\n\n## 转述冲突检测（可主张审查员认定错误）\n\n{}", conflict_report);
                    full_text.push_str(&conflict_text);
                    yield Ok(Event::default().data(sse_encode(&conflict_text)));
                    tracing::info!("[OA-CONFLICT-STREAM] 检测完成, report_len={}", conflict_report.len());
                }
                Ok(_) => {
                    tracing::info!("[OA-CONFLICT-STREAM] 检测完成，无冲突");
                }
                Err(e) => {
                    tracing::warn!("[OA-CONFLICT-STREAM] 检测失败: {}", e);
                }
            }
        }

        // M2-C：结构化来源事件（前端按 event 分派，不进正文/导出——P0-1）
        yield Ok(Event::default()
            .event("meta")
            .data("{\"source\":\"main_analysis\"}"));
        yield Ok(Event::default().event("done").data("[DONE]"));

        if !patent_number.is_empty() && !full_text.is_empty() {
            let _ = db.save_oa_analysis(&patent_number, &patent_title, &oa_type, &depth, &full_text);
        }
    };

    Sse::new(Box::pin(stream))
}

/// 在讨论确认后，生成正式答复书（第五部分）
pub async fn api_ai_oa_generate_response_letter(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Sse<Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>> {
    fn error_sse(msg: String) -> Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> {
        Box::pin(futures::stream::once(async move {
            Ok(Event::default().event("error").data(msg))
        }))
    }

    let mut analysis_text = req
        .get("analysis")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut discussion = req
        .get("discussion")
        .and_then(|v| v.as_str())
        .unwrap_or("[]")
        .to_string();
    let mut office_action = req
        .get("office_action")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    // Phase 4: 支持 CaseInput JSON 格式（oa_full_text 字段）
    if office_action.is_empty() {
        if let Some(oa) = req.get("oa_full_text").and_then(|v| v.as_str()) {
            office_action = oa.to_string();
        }
    }
    // P0-4: 提取 oa_date 结构化字段，注入 OA 文本头部，防止 LLM 从正文中抓错日期
    let oa_date = req
        .get("oa_date")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    if let Some(date) = oa_date {
        office_action = format!(
            "【驳回决定发文日（结构化字段，以此为准）：{}】\n\n             注意：OA 正文中可能提及多个日期（如申请人提交答复的日期），不得混淆。\n             上述发文日期是驳回决定的发出日期，用于期限计算等程序事项。\n\n\n{}",
            date, office_action
        );
    }
    let oa_type = req
        .get("oa_type")
        .and_then(|v| v.as_str())
        .filter(|&v| {
            v == "abnormal"
                || v == "reject_review"
                || v == "first_exam"
                || v == "second_rejection"
                || v == "reexamination_request"
                || v == "reexamination_decision"
                || v == "admin_lawsuit"
        })
        .or_else(|| {
            req.get("case_meta")
                .and_then(|m| m.get("oa_type"))
                .and_then(|v| v.as_str())
                .filter(|&v| {
                    v == "reject_review"
                        || v == "reexamination_request"
                        || v == "reexamination_decision"
                })
        })
        .unwrap_or("first_exam")
        .to_string();

    let discussion_id = req
        .get("discussion_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !discussion_id.is_empty() {
        match s.db.get_oa_discussion(discussion_id) {
            Ok(Some(saved)) => {
                if analysis_text.trim().is_empty() {
                    if let Some(snapshot) = saved.analysis_snapshot {
                        if !snapshot.trim().is_empty() {
                            analysis_text = snapshot;
                        }
                    }
                }
                if discussion.trim().is_empty() || discussion == "[]" {
                    discussion = saved.discussion_history;
                }
                if office_action.trim().is_empty() {
                    office_action = saved.oa_text;
                }
            }
            Ok(None) => tracing::warn!(
                discussion_id,
                "OA discussion was not found while generating response letter"
            ),
            Err(error) => {
                tracing::warn!(%error, discussion_id, "Failed to restore OA discussion while generating response letter")
            }
        }
    }

    // 无分析结果时，用 OA 文本 + 讨论历史作为上下文（支持导入后直接生成）
    let is_import_flow = analysis_text.trim().is_empty();
    // M2-C（审查 P1-8）：来源三态标记。优先采用调用方显式声明的 `analysis_source`
    // （前端/驱动在拿到分析流 meta 后回传，杜绝启发式误判）；未提供时才回退到
    // 启发式——且"仅事实核查段"（含 AI 事实核查/可信度评分而无主分析结构）必须
    // 判为非主分析（M1 实况：主分析静默失败后核查段/冲突报告顶位）。
    const VALID_SOURCES: [&str; 3] = ["main_analysis", "conflict_report_only", "oa_text_fallback"];
    let explicit: Option<String> = req
        .get("analysis_source")
        .and_then(|v| v.as_str())
        .filter(|s| VALID_SOURCES.contains(s))
        .map(|s| s.to_string());
    let analysis_source: String = explicit.unwrap_or_else(|| {
        if analysis_text.trim().len() >= 50 {
            let main_markers = ["第一部分", "审查意见概述", "答复思路", "策略分析", "对比表"];
            let non_main_markers = ["转述冲突检测", "AI 事实核查", "可信度评分"];
            let has_main = main_markers.iter().any(|m| analysis_text.contains(m));
            let has_non_main = non_main_markers.iter().any(|m| analysis_text.contains(m));
            if !has_main && has_non_main {
                "conflict_report_only".to_string()
            } else {
                "main_analysis".to_string()
            }
        } else {
            "oa_text_fallback".to_string()
        }
    });
    let final_analysis = if analysis_text.trim().len() >= 50 {
        analysis_text
    } else if !office_action.is_empty() {
        // 降级：用 OA 文本作为伪分析上下文，AI 仍可基于 OA + 讨论生成答复书
        format!("[从审查意见通知书提取]\n{}", office_action)
    } else {
        return Sse::new(error_sse(
            "[ERROR] 缺少分析结果和 OA 文本，请先上传 OA 或完成分析后再生成答复书".into(),
        ));
    };
    // M2-C：非主分析来源 → 答复书正文首部必须有可见提示（随首个 data 帧下发，确定性前置）
    let source_banner = if analysis_source == "main_analysis" {
        String::new()
    } else {
        let basis = if analysis_source == "conflict_report_only" {
            "主分析缺失，本稿基于转述冲突检测报告生成"
        } else {
            "主分析缺失，本稿基于审查意见通知书原文生成"
        };
        format!(
            "> ⚠️ 来源提示（{}）：{}，请人工核验后再使用。\n\n",
            analysis_source, basis
        )
    };

    let config_guard = s.config.read().unwrap_or_else(|e| e.into_inner());
    let ai = config_guard.ai_client_expert();
    let ai_fast = config_guard.ai_client_fast();
    drop(config_guard);
    // MC2: 生成答复书前做事实核查——致命档拒绝生成
    // Phase 4: reject_review 走通用管线，管线自带 OutputGuard，跳过旧事实核查
    if oa_type != "reject_review" {
        let pre_check = check_oa_analysis(&final_analysis, &office_action, &final_analysis);
        if pre_check.score <= 65.0 {
            let reason = format_report(&pre_check);
            return Sse::new(error_sse(format!(
                "[ERROR] 事实核查未通过（得分 {}/100），拒绝生成含编造内容的答复书。\n\n{}",
                pre_check.score, reason
            )));
        }
    }

    // 主 UI 只传 analysis/discussion/office_action；答复管线的 Stage A 需要本专利原文
    // 做 verbatim 校验，否则所有特征被丢弃、产出空答复。
    let my_patent_text = match req.get("my_patent") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Object(o)) => o
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    };

    // 信头五项（专利号/发明名称/申请人/发文序号/发文日）按通用标签抽取一次，
    // 既供提示词注入，也供成文后的确定性回填——两条生成链路都从这里出去，
    // 所以回填放在路由层能同时覆盖模板路径与通用管线路径。
    let oa_header = crate::ai::letter_header::LetterHeader::extract(
        &office_action,
        &my_patent_text,
        Some(&req),
    );

    // 审查所针对的那版权项（可选）：有它才能查"删限定＝扩大保护范围"（issue #159）。
    let current_claims = req
        .get("current_claims")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 注入多轮 OA 视图漂移与禁止反言历史约束（如 DB 中存在 oa_rounds 记录或前端透传）
    let drift_report =
        resolve_oa_drift_report(&s.db, &req, oa_header.application_number.as_deref());
    let drift_redlines_count = drift_report
        .as_ref()
        .map(|r| r.consistency_redlines.len())
        .unwrap_or(0);
    let mut enriched_req = req.clone();
    let merged_constraints = drift_report
        .as_ref()
        .map(|r| r.prompt_constraints_block.clone())
        .or_else(|| {
            req.get("historical_constraints")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
        });
    let drift_constraints_applied = merged_constraints.is_some();
    if let (Some(constraints), Some(obj)) = (merged_constraints, enriched_req.as_object_mut()) {
        obj.insert(
            "historical_constraints".to_string(),
            serde_json::Value::String(constraints),
        );
    }

    let mut rx = ai.generate_response_letter_stream(
        &final_analysis,
        &discussion,
        &office_action,
        &oa_type,
        is_import_flow,
        Some(&enriched_req),
        &my_patent_text,
        Some(&ai_fast),
    );

    let stream = async_stream::stream! {
        let mut raw_text = String::new();
        // M2-C：结构化来源字段走 meta 事件（前端解析器只认 error/done/data，未知事件安全跳过）
        yield Ok(Event::default()
            .event("meta")
            .data(format!("{{\"source\":\"{analysis_source}\"}}")));
        while let Some(chunk) = rx.recv().await {
            if chunk.starts_with("[ERROR]") {
                yield Ok(Event::default().event("error").data(chunk));
                return;
            }
            // EK-P6：累积原文（句边界完整），SSE 载荷 JSON 编码
            raw_text.push_str(&chunk);
        }
        // ── 信头回填（先于逐字核验门）──
        // 顺序理由：门是"就地标注 + 计数"，它看到什么就该被读者看到什么；
        // 若先跑门再改信头，标注的字节偏移与新文本不再对应。信头行也不在修改稿段内，
        // 反过来先回填不会给门引入新的命中。产物里不允许残留裸【待填写】。
        let header_fill = oa_header.backfill(&raw_text);
        // ── UA3 v2 修改稿特征逐字核验门（M2 第 2 步）──
        // 必须在整篇产出后跑：①阻断横幅要落在正文首部；②零命中特征句要**就地**标注，
        // 流式转发无法回填已下发的字符。两条生成链路本就整篇单帧返回（非逐 token），
        // 因此缓冲不改变可见时延，只把"先发后检"换成"检完再发"。
        let gate = crate::evidence_kernel::amendment_gate::gate_amendment_text_with_current_claims(
            &header_fill.letter,
            &my_patent_text,
            &current_claims,
        );
        // 锚点核验（#79）：本路由拿不到检索分块元数据（依赖 #78 的 section_tag），
        // 因此只做"引用了哪些锚点、能不能精确对上"，对不上就如实报，不假装核验过。
        let anchor_report =
            crate::ai::anchor_verify::verify_anchors(&header_fill.letter, &crate::ai::anchor_verify::ChunkIndex::new());
        let mut body = String::new();
        if !source_banner.is_empty() {
            body.push_str(&source_banner);
        }
        if gate.unsupported_count > 0 {
            let banner = gate.banner();
            body.push_str(&banner);
        }
        // 门未核验（未定位到修改稿 / 缺原文）或**语料缺权项**时都要挂说明——
        // 后者即使有零命中也必须挂，否则读者会把"只比对了说明书"当成"已核验修改稿"（#140）。
        let note = gate.skip_note();
        if !note.is_empty() {
            body.push_str(&note);
        }
        // 扩范围方向：不通过 / 未核验都要可见（#159）
        let direction = gate.direction_note();
        if !direction.is_empty() {
            body.push_str(&direction);
        }
        if anchor_report.has_unverified() {
            body.push_str(&format!(
                "> ℹ️ 锚点核验：本稿引用了 {} 处形如 [D1:第N段] / [D1:权利要求N] 的锚点，本次未提供对应文档的分块元数据，无法逐条核对；请人工确认这些段落号/权项号确实存在于被引文件。

",
                anchor_report.unverified.len()
            ));
        }
        body.push_str(&gate.gated_text);
        for frame in split_sse_frames(&body, 240) {
            yield Ok(Event::default().data(sse_encode(&frame)));
        }
        // 结构化维度：amendment_unsupported=N 与提交度维度（N>0 即 False）
        yield Ok(Event::default().event("meta").data(format!(
            "{{\"source\":\"{analysis_source}\",\"amendment_unsupported\":{},\"amendment_marked\":{},\"amendment_merged\":{},\"amendment_unlocated\":{},\"broadening_count\":{},\"broadening_checked\":{},\"amendment_assertions_downgraded\":{},\"amendment_claims\":{},\"amendment_features\":{},\"corpus_claims\":{},\"amendment_gate\":\"{}\",\"anchor_verified\":{},\"anchor_unverified\":{},\"anchor_unparsed\":{},\"anchor_hallucination_rate\":{:.3},\"submission_ready\":{},\"header_known\":{},\"header_filled\":{},\"header_unresolved_count\":{},\"header_unresolved\":\"{}\",\"header_unresolved_marks\":{},\"drift_constraints_applied\":{},\"drift_redlines_count\":{}}}",
            gate.unsupported_count,
            gate.marked_count,
            gate.merged_count,
            gate.unlocated_count,
            gate.broadened.len(),
            gate.current_claims_checked,
            gate.assertions_downgraded,
            gate.amended_claim_count,
            gate.checked_feature_count,
            gate.corpus_claims,
            gate.status.as_str(),
            anchor_report.verified.len(),
            anchor_report.unverified.len(),
            anchor_report.unparsed.len(),
            anchor_report.hallucination_rate(),
            gate.submission_ready,
            oa_header.known_count(),
            header_fill.filled,
            header_fill.unresolved_fields.len(),
            header_fill.unresolved_fields.join("|"),
            header_fill.unresolved_marks,
            drift_constraints_applied,
            drift_redlines_count
        )));
        // R6c: 事实核查已由 pipeline 内嵌，SSE 层不再重复追加
        yield Ok(Event::default().event("done").data("[DONE]"));
    };

    Sse::new(Box::pin(stream))
}

/// OA 讨论消息：在分析完成后，用户与 AI 就分析内容进行深度讨论。
/// AI 会主动评估用户意见、给出具体修改方案、并在必要时输出修改后的段落。
pub async fn api_ai_oa_discuss(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Sse<Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>> {
    fn error_sse(msg: String) -> Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> {
        Box::pin(futures::stream::once(async move {
            Ok(Event::default().event("error").data(msg))
        }))
    }

    let analysis_text = req
        .get("analysis")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let discussion_json = req
        .get("discussion")
        .and_then(|v| v.as_str())
        .unwrap_or("[]")
        .to_string();
    let message = req
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    // P0-2/P0-3: 新增讨论会话元数据参数
    let discussion_id = req
        .get("discussion_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let patent_number = req
        .get("patent_number")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let applicant_name = req
        .get("applicant_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let oa_type = req
        .get("oa_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let oa_text = req
        .get("oa_text")
        .or_else(|| req.get("office_action"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    // 文件上传字段
    let file_name = req
        .get("file_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let file_type = req
        .get("file_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let file_base64 = req
        .get("file_base64")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if message.trim().is_empty() && file_base64.is_empty() {
        return Sse::new(error_sse("[ERROR] 请输入讨论内容".into()));
    }

    // Explicit capacity guard: preserve the complete input and fail visibly when
    // the provider-safe budget is exceeded. No OA data is silently truncated.
    for (field, value, max_chars) in [
        (
            "analysis",
            analysis_text.as_str(),
            OA_DISCUSSION_ANALYSIS_MAX_CHARS,
        ),
        (
            "discussion",
            discussion_json.as_str(),
            OA_DISCUSSION_HISTORY_MAX_CHARS,
        ),
        (
            "office_action",
            req.get("office_action")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            OA_DISCUSSION_OA_MAX_CHARS,
        ),
    ] {
        if let Some(error) = oa_capacity_error(field, value, max_chars) {
            return Sse::new(error_sse(error));
        }
    }

    let analysis = analysis_text.as_str();
    let disc_raw = discussion_json.to_string();

    // Format discussion history as readable dialogue (input is JSON array)
    // 数据库保存不受限（2M）；发给 AI 时从数组层面移除最老消息，保留最新的讨论
    // 上限 120K 字符（接近主流 AI context window），避免超出限制
    const MAX_DISCUSSION_FOR_AI: usize = 120_000;

    let formatted_discussion: String;
    if disc_raw.len() > 10 && disc_raw != "[]" {
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&disc_raw) {
            // 计算每则消息的字符量，从最老的开始移除，直到总长度 <= MAX_DISCUSSION_FOR_AI
            let trimmed: Vec<serde_json::Value> = arr.into_iter().collect();
            let total_chars = trimmed.iter().map(|m| m.to_string().len()).sum::<usize>();
            // 如果只剩一条消息且还是超限，截断该消息本身
            let final_messages = if trimmed.len() == 1 && total_chars > MAX_DISCUSSION_FOR_AI {
                let mut msg = trimmed[0].clone();
                if let Some(content) = msg["content"].as_str() {
                    let max_content = MAX_DISCUSSION_FOR_AI - msg["role"].to_string().len() - 20;
                    if content.len() > max_content {
                        msg["content"] = serde_json::Value::String(
                            truncate_for_ai(content, max_content)
                                + "...\n[讨论内容已被截断以适配 AI 上下文窗口]",
                        );
                    }
                }
                vec![msg]
            } else {
                trimmed
            };
            let mut text = String::new();
            for item in &final_messages {
                let role = item["role"].as_str().unwrap_or("unknown");
                let content = item["content"].as_str().unwrap_or("");
                let label = match role {
                    "user" => "发明人",
                    "assistant" => "AI（代理师）",
                    _ => "未知角色",
                };
                text.push_str(&format!("【{label}】{content}\n\n"));
            }
            formatted_discussion = text;
        } else {
            formatted_discussion = if disc_raw.len() > MAX_DISCUSSION_FOR_AI {
                truncate_for_ai(&disc_raw, MAX_DISCUSSION_FOR_AI) + "\n\n[讨论内容已被截断]"
            } else {
                disc_raw.clone()
            };
        }
    } else {
        formatted_discussion = String::new();
    }

    let oa_snippet = req
        .get("office_action")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let system_prompt = format!(
        "你是一位资深中国专利代理师（执业20年+），精通中国专利法、审查指南（2023修订版）及复审无效程序。\n\n\
         你正在与发明人就一份审查意见通知书答复方案进行深度讨论。你的任务是帮助发明人完善答复方案，\
         确保最终的意见陈述书逻辑严密、论证充分、经得起审查员的推敲。\n\n\
         ## 讨论规则\n\n\
         1. **主动评估**：对发明人的每个意见，先评估其合理性（合理/部分合理/不合理），再给出具体建议。\n\
         2. **具体修改**：如果发明人的意见合理，你必须给出修改后的具体段落文字，\
         标注【建议修改】并用 > 引用原文、>>> 标明修改后文本，让发明人可以直接对比。\n\
         3. **反驳论证**：如果发明人的意见不合理（例如误解了法条、遗漏了审查员的论点），\
         用专业但尊重的语气解释为什么，并给出更好的替代方案。\n\
         4. **主动引导**：如果分析中有些地方可能有争议或薄弱，主动指出并询问发明人的意见。\n\
         5. **法条引用**：每处法律论断必须引用具体法条（A22.2/A22.3/A26.3/A33 等），不可空泛。\n\
         6. **技术依据**：每处技术论断必须引用分析中的具体特征或对比文献段落。\n\n\
         ## 事实纪律（最高优先级，优先于一切）\n\n\
         1. 你只能依据上方提供的 OA 分析结果和原始审查意见通知书作答，禁止引入材料之外的信息。\n\
         2. 材料中没有的内容，必须明确回答「材料中未提供」，禁止推测、补全或编造。\n\
         3. 引用法条时，只允许引用你确定存在的法条条号（如 A22.3、A26.4、A33）；\n\
         不确定条号时写「相关法律条文」，禁止编造条号。\n\
         4. 引用审查意见或对比文献时，只能引用上方材料中出现的原文段落，禁止自行扩写。\n\
         5. 任何数字、日期、百分比必须来自材料原文，否则标注「材料未给出」。\n\
         6. 发明人引用的法条或事实与材料矛盾时，明确指出矛盾，不要附和错误观点。\n\n\
         ## 已有的 OA 分析结果\n{}\n\n\
         ## 原始审查意见通知书（供参考）\n{}\n\n\
         请严格遵守以上规则，用专业但易懂的语言与发明人沟通。\
         不要简单地说「好的」或「明白了」——你需要像一个真正的专利代理师那样，\
         给出有实质内容的、能推动答复方案前进的专业意见。",
        bounded_reference_material("已有 OA 分析结果", analysis),
        bounded_reference_material("原始审查意见通知书", oa_snippet),
    );

    let discussion_context = if !formatted_discussion.is_empty() {
        format!(
            "\n\n## 之前的讨论记录\n{}\n## 发明人的最新意见\n{}",
            bounded_reference_material("之前的讨论记录", &formatted_discussion),
            bounded_reference_material("发明人的最新意见", &message)
        )
    } else {
        format!(
            "\n## 发明人的意见\n{}",
            bounded_reference_material("发明人的最新意见", &message)
        )
    };

    let user_prompt = format!(
        "请针对发明人的意见给出专业回应。记住：\n\
         - 先评估合理性\n\
         - 如合理，给出具体修改方案（含修改前后对比）\n\
         - 如不合理，用专业理由说服，并给出替代方案\n\
         - 引用具体法条和技术特征\n\
         {discussion_context}"
    );

    // 处理上传的文件
    let file_attachment = if !file_base64.is_empty() && !file_name.is_empty() {
        if file_type.starts_with("image/") {
            format!(
                "\n\n[附件] 用户上传了一张图片：{}，请参考图片内容进行分析。",
                file_name
            )
        } else {
            // 尝试 base64 decode 文本文件
            match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &file_base64) {
                Ok(bytes) => {
                    if let Ok(text) = String::from_utf8(bytes) {
                        if text.len() < 20000 {
                            format!(
                                "\n\n[附件] 用户上传了文件：{}\n内容：\n```\n{}\n```",
                                file_name, text
                            )
                        } else {
                            format!(
                                "\n\n[附件] 用户上传了文件：{}（文件过大，仅显示前20000字符）",
                                file_name
                            )
                        }
                    } else {
                        format!("\n\n[附件] 用户上传了文件：{}（二进制文件）", file_name)
                    }
                }
                Err(_) => format!("\n\n[附件] 用户上传了文件：{}（无法解析）", file_name),
            }
        }
    } else {
        String::new()
    };

    let final_user_prompt = format!("{}{}", user_prompt, file_attachment);

    let messages = vec![
        Message {
            role: "system".into(),
            content: system_prompt,
        },
        Message {
            role: "user".into(),
            content: final_user_prompt,
        },
    ];

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_expert();

    // 事实类讨论：低温抑制幻觉，同时保留条理清晰的专业表达
    let mut rx = ai.send_chat_stream(messages, 0.35);

    // P0-2: 持久化讨论会话
    let db = s.db.clone();
    let discussion_id_final = if discussion_id.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        discussion_id.clone()
    };
    let mut accumulated_response = String::new();

    // P0-3: 预构建完整历史（用于持久化），包含之前的讨论 + 本轮用户消息 + AI回复
    // Parse past discussion as JSON array
    let past_messages: Vec<serde_json::Value> = if !disc_raw.is_empty() && disc_raw != "[]" {
        serde_json::from_str(&disc_raw).unwrap_or_default()
    } else {
        Vec::new()
    };

    let stream = async_stream::stream! {
        while let Some(chunk) = rx.recv().await {
            if chunk.starts_with("[ERROR]") {
                yield Ok(Event::default().event("error").data(chunk));
                return;
            }
            // EK-P6 修复：累积原文，SSE 载荷 JSON 编码（前端 sseDecode 还原）
            accumulated_response.push_str(&chunk);
            yield Ok(Event::default().data(sse_encode(&chunk)));
        }

        // MC2: 讨论回复事实核查——附到讨论消息后
        let disc_fact_report = check_oa_analysis(&accumulated_response, &analysis_text, &accumulated_response);
        if !disc_fact_report.warnings.is_empty() {
            let fact_text = format!("\n\n## AI 事实核查（请人工复核）\n{}", format_report(&disc_fact_report));
            yield Ok(Event::default().data(sse_encode(&fact_text)));
        }

        // P0: done 事件携带 discussion_id，确保前端可捕获
        yield Ok(Event::default().event("done").data(format!("[DONE] discussion_id:{} ", discussion_id_final)));

        // P0-2: 流结束后持久化完整讨论历史
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 构建完整 JSON 历史：过去讨论 + 本轮用户消息 + AI 回复
        let mut full_history = past_messages;
        full_history.push(serde_json::json!({"role": "user", "content": message.clone()}));
        full_history.push(serde_json::json!({"role": "assistant", "content": accumulated_response.clone()}));
        let history_json = serde_json::to_string(&full_history).unwrap_or_else(|_| "[]".to_string());

        let _ = db.save_oa_discussion(&crate::db::OaDiscussion {
            id: discussion_id_final,
            patent_number: patent_number.clone(),
            applicant_name: if applicant_name.is_empty() { None } else { Some(applicant_name.clone()) },
            oa_type: if oa_type.is_empty() { None } else { Some(oa_type.clone()) },
            analysis_snapshot: if analysis_text.is_empty() { None } else { Some(analysis_text.clone()) },
            oa_text: oa_text.clone(),
            discussion_history: history_json, // 完整 JSON 历史，不再只是 AI 回复
            created_at: now.clone(),
            updated_at: now,
        });
    };

    Sse::new(Box::pin(stream))
}

/// 获取某专利的 OA 分析历史
pub async fn api_oa_history(
    Path(patent_number): Path<String>,
    State(s): State<AppState>,
) -> Json<serde_json::Value> {
    match s.db.list_oa_analyses(&patent_number) {
        Ok(list) => Json(json!({"status": "ok", "analyses": list})),
        Err(e) => Json(json!({"status": "error", "message": format!("查询失败: {}", e)})),
    }
}

/// 获取所有 OA 分析历史（按时间倒序）
pub async fn api_oa_history_all(State(s): State<AppState>) -> Json<serde_json::Value> {
    match s.db.list_all_oa_analyses(100) {
        Ok(list) => Json(json!({"status": "ok", "analyses": list})),
        Err(e) => Json(json!({"status": "error", "message": format!("查询失败: {}", e)})),
    }
}

/// 按 ID 获取单条 OA 分析记录 / Get a single OA analysis record by ID
///
/// 与 api_oa_history_all 不同，本 handler 只返回指定 id 的单条记录。
/// 未命中时返回 {"status":"error","message":"..."}，绝不回落为全量列表。
pub async fn api_oa_history_detail(
    Path(id): Path<i64>,
    State(s): State<AppState>,
) -> Json<serde_json::Value> {
    match s.db.get_oa_analysis(id) {
        Ok(Some(rec)) => Json(json!({"status": "ok", "analysis": rec})),
        Ok(None) => Json(json!({"status": "error", "message": "记录不存在 / Record not found."})),
        Err(e) => Json(json!({"status": "error", "message": format!("查询失败: {}", e)})),
    }
}

/// 列出某专利的所有 OA 讨论会话
pub async fn api_oa_discussion_list(
    Path(patent_number): Path<String>,
    State(s): State<AppState>,
) -> Json<serde_json::Value> {
    match s.db.list_oa_discussions(&patent_number) {
        Ok(list) => Json(json!({"status": "ok", "discussions": list})),
        Err(e) => Json(json!({"status": "error", "message": format!("查询失败: {}", e)})),
    }
}

/// 获取单个 OA 讨论会话
pub async fn api_oa_discussion_get(
    Path((patent_number, discussion_id)): Path<(String, String)>,
    State(s): State<AppState>,
) -> Json<serde_json::Value> {
    match s.db.get_oa_discussion(&discussion_id) {
        Ok(Some(d)) => {
            // 所有权校验：讨论必须属于请求的专利
            if d.patent_number != patent_number {
                return Json(
                    json!({"status": "error", "message": "讨论不属于该专利 / Discussion does not belong to this patent."}),
                );
            }
            Json(json!({"status": "ok", "discussion": d}))
        }
        Ok(None) => {
            Json(json!({"status": "error", "message": "讨论不存在 / Discussion not found."}))
        }
        Err(e) => Json(json!({"status": "error", "message": format!("查询失败: {}", e)})),
    }
}

/// 导入讨论记录（POST /api/oa/discussions/import）
pub async fn api_oa_discussion_import(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let patent_number = match req["patent_number"].as_str() {
        Some(pn) => pn.to_string(),
        None => return Json(json!({"status": "error", "message": "patent_number 字段缺失"})),
    };

    let oa_text = req["oa_text"].as_str().unwrap_or("").to_string();

    // MB5: OA 讨论导入容量校验（原零容量校验）
    if let Some(error) = oa_capacity_error("oa_text", &oa_text, OA_DISCUSSION_OA_MAX_CHARS) {
        return Json(json!({"status": "error", "message": error}));
    }

    let messages = match req["messages"].as_array() {
        Some(arr) => arr
            .iter()
            .map(|m| {
                let role = m["role"].as_str().unwrap_or("user");
                let content = m["content"].as_str().unwrap_or("");
                serde_json::json!({"role": role, "content": content})
            })
            .collect::<Vec<_>>(),
        None => return Json(json!({"status": "error", "message": "messages 字段缺失或格式错误"})),
    };

    if messages.is_empty() {
        return Json(json!({"status": "error", "message": "messages 不能为空"}));
    }

    let history_json = serde_json::to_string(&messages).unwrap_or_else(|_| "[]".to_string());
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let discussion_id = format!(
        "disc-{}-import",
        chrono::Local::now().timestamp_millis() % 1_000_000
    );

    let discussion = crate::db::OaDiscussion {
        id: discussion_id.clone(),
        patent_number,
        oa_type: None,
        applicant_name: None,
        analysis_snapshot: None,
        oa_text,
        discussion_history: history_json,
        created_at: now.clone(),
        updated_at: now,
    };

    match s.db.save_oa_discussion(&discussion) {
        Ok(_) => Json(json!({
            "status": "ok",
            "ok": true,
            "discussion_id": discussion_id,
            "message": "导入成功 / Import successful"
        })),
        Err(e) => Json(json!({"status": "error", "message": format!("保存失败: {}", e)})),
    }
}

/// 删除 OA 分析记录
pub async fn api_oa_history_delete(
    Path(id): Path<i64>,
    State(s): State<AppState>,
) -> Json<serde_json::Value> {
    match s.db.delete_oa_analysis(id) {
        Ok(_) => Json(json!({"status": "ok"})),
        Err(e) => Json(json!({"status": "error", "message": format!("删除失败: {}", e)})),
    }
}

/// AI 审查权利要求修改方案
pub async fn api_ai_check_amendments(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let original = req["original_claims"].as_str().unwrap_or("");
    let amended = req["amended_claims"].as_str().unwrap_or("");
    let oa = req["office_action"].as_str().unwrap_or("");

    if original.is_empty() || amended.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "原始权利要求和修改后权利要求不能为空"})),
        );
    }

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_expert();

    match ai.check_claim_amendments(original, amended, oa).await {
        Ok(content) => {
            let _ = s.db.log_ai_call(&ai, "ai-claim-amendments", None, None);
            (
                StatusCode::OK,
                Json(json!({"status": "ok", "analysis": content})),
            )
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("审查失败: {}", e) })),
        ),
    }
}

/// POST /api/ai/threat-assessment — Multi-patent threat assessment (X/Y/A classification)
pub async fn api_ai_threat_assessment(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let my_claims = req["my_claims"].as_str().unwrap_or("");
    let patents = req["patents"].as_array();

    if my_claims.is_empty() {
        return Json(json!({"error": "本申请权利要求不能为空"}));
    }
    let patents_json = match patents {
        Some(arr) => serde_json::to_string(arr).unwrap_or_default(),
        None => return Json(json!({"error": "对比专利列表不能为空"})),
    };

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_expert();

    match ai.threat_assessment(&patents_json, my_claims).await {
        Ok(content) => {
            let _ = s.db.log_ai_call(&ai, "ai-threat-assessment", None, None);
            Json(json!({"status": "ok", "analysis": content}))
        }
        Err(e) => Json(json!({ "error": format!("威胁评估失败: {}", e) })),
    }
}

/// POST /api/ai/claim-chart — Generate claim chart mapping elements to prior art
pub async fn api_ai_claim_chart(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let my_claims = req["my_claims"].as_str().unwrap_or("");
    let prior_art = req["prior_art"].as_str().unwrap_or("");

    if my_claims.is_empty() || prior_art.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "权利要求和对比文件内容不能为空"})),
        );
    }

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_expert();

    match ai.claim_chart(my_claims, prior_art).await {
        Ok(content) => {
            let _ = s.db.log_ai_call(&ai, "ai-claim-chart", None, None);
            (
                StatusCode::OK,
                Json(json!({"status": "ok", "analysis": content})),
            )
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": format!("权利要求对照表生成失败: {}", e)
            })),
        ),
    }
}

/// POST /api/oa/export-docx — 导出意见陈述书为 docx 文件
pub async fn api_oa_export_docx(
    State(_s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let response_text = req["response_text"].as_str().unwrap_or("");
    let patent_number = req["patent_number"].as_str().unwrap_or("");
    let applicant = req["applicant"].as_str().unwrap_or("");
    let oa_type = req["oa_type"].as_str().unwrap_or("");

    if response_text.is_empty() {
        return Json(json!({"error": "答复内容不能为空"}));
    }

    if response_text.len() > 50000 {
        return Json(json!({"error": "答复内容过长，请分段导出"}));
    }

    let params = crate::docx_export::ExportParams {
        response_text: response_text.to_string(),
        patent_number: patent_number.to_string(),
        applicant: applicant.to_string(),
        oa_type: oa_type.to_string(),
    };

    match crate::docx_export::generate_docx(&params) {
        Ok(docx_bytes) => {
            let base64_docx = base64::engine::general_purpose::STANDARD.encode(docx_bytes);
            Json(json!({"status": "ok", "docx": base64_docx}))
        }
        Err(error) => {
            tracing::error!(error = %error, "OA DOCX export failed");
            Json(json!({"error": "DOCX 导出失败，请稍后重试"}))
        }
    }
}

/// GET /api/ai/cost — AI 调用成本摘要
pub async fn api_ai_cost_summary(State(s): State<AppState>) -> Json<serde_json::Value> {
    let days = 30i64;
    match s.db.get_cost_summary(days) {
        Ok(summary) => Json(summary),
        Err(e) => Json(json!({
            "error": format!("Failed to get cost summary: {}", e)
        })),
    }
}

/// GET /api/ai/cost/records — AI 调用成本记录列表
pub async fn api_ai_cost_records(State(s): State<AppState>) -> Json<serde_json::Value> {
    let limit = 100i64;
    match s.db.get_recent_cost_records(limit) {
        Ok(records) => Json(json!({
            "status": "ok",
            "count": records.len(),
            "records": records,
        })),
        Err(e) => Json(json!({
            "error": format!("Failed to get cost records: {}", e)
        })),
    }
}

/// POST /api/ai/cost/record — 保存单条 AI 调用成本记录
pub async fn api_ai_cost_save(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    use crate::pipeline::context::AiCostRecord;
    use uuid::Uuid;

    let model = req["model"].as_str().unwrap_or("unknown").to_string();
    let provider = req["provider"].as_str().unwrap_or("unknown").to_string();
    let pipeline_run_id = req["pipeline_run_id"].as_str().unwrap_or("").to_string();
    let step = req["step"].as_str().unwrap_or("unknown").to_string();
    let input_tokens = req["input_tokens"].as_i64().unwrap_or(0);
    let output_tokens = req["output_tokens"].as_i64().unwrap_or(0);
    let estimated_cost_cents = req["estimated_cost_cents"].as_f64().unwrap_or(0.0);
    let duration_ms = req["duration_ms"].as_i64().unwrap_or(0);

    let record = AiCostRecord {
        id: Uuid::new_v4().to_string(),
        pipeline_run_id,
        step,
        model,
        provider,
        timestamp: chrono::Utc::now().to_rfc3339(),
        input_tokens,
        output_tokens,
        estimated_cost_cents,
        duration_ms,
        idea_id: None,
        session_id: None,
    };

    match s.db.save_cost_record(&record) {
        Ok(_) => Json(json!({"status": "ok", "id": record.id})),
        Err(e) => Json(json!({
            "error": format!("Failed to save cost record: {}", e)
        })),
    }
}
