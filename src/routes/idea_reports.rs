use super::*;

/// GET /api/idea/:id/report?type=executive|technical — 分层报告
///
/// - executive: 领导版（1 页摘要）
/// - technical: 研发版（详细技术报告）
/// - 无参数: 原始完整数据（向后兼容）
pub async fn api_idea_report(
    State(s): State<AppState>,
    Path(idea_id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let idea = match s.db.get_idea(&idea_id) {
        Ok(Some(idea)) => idea,
        _ => return Json(json!({"status": "error", "message": "创意不存在"})),
    };

    let report_type = params.get("type").map(|s| s.as_str()).unwrap_or("full");

    // T21: 支持 ?version_id= 从历史版本快照生成报告。
    // EK-P2 修复：novelty_score 为 None（证据不足/未评估）时不再折算 0 分输出"低新颖性"结论，
    // 而是如实输出"证据不足"（设计文档 evidence-kernel-v2.md §3-5）。
    let (score, analysis_text): (Option<f64>, String) = if let Some(vid) = params.get("version_id")
    {
        // 从 idea_versions 读取快照
        match s.db.get_idea_versions(&idea_id) {
            Ok(versions) => {
                if let Some(ver) = versions.iter().find(|v| v.id == *vid) {
                    if let Ok(ctx) = serde_json::from_str::<crate::pipeline::context::PipelineContext>(
                        &ver.context_json,
                    ) {
                        let s = if ctx.novelty_status
                            == crate::pipeline::context::NoveltyStatus::InsufficientEvidence
                        {
                            None
                        } else {
                            Some(ctx.novelty_score)
                        };
                        (s, ctx.ai_analysis)
                    } else {
                        (idea.novelty_score, idea.analysis.clone())
                    }
                } else {
                    return Json(json!({"status": "error", "message": "版本不存在"}));
                }
            }
            Err(_) => (idea.novelty_score, idea.analysis.clone()),
        }
    } else {
        (idea.novelty_score, idea.analysis.clone())
    };

    // 证据不足与数值分档互斥：None 只能出"证据不足"，绝不折算 0 分
    let (level, risk, conclusion, score_line) = match score {
        None => (
            "证据不足（未评估）",
            "—",
            "检索证据不足，无法给出新颖性结论；请补充检索或对比文件后重新分析",
            "—/100".to_string(),
        ),
        Some(s) => (
            if s >= 70.0 {
                "高新颖性"
            } else if s >= 40.0 {
                "中等新颖性"
            } else {
                "低新颖性"
            },
            if s >= 70.0 {
                "低"
            } else if s >= 40.0 {
                "中"
            } else {
                "高"
            },
            if s >= 70.0 {
                "该创意具有较高新颖性，建议推进专利申请"
            } else if s >= 40.0 {
                "该创意有一定新颖性，建议进一步差异化后申请"
            } else {
                "该创意与现有技术重叠较多，建议调整技术路线"
            },
            format!("{:.1}/100", s),
        ),
    };

    match report_type {
        "executive" => {
            // 领导版：1 页精简报告
            let report_md = format!(
                "# 创新验证结论\n\
                 **项目**：{}\n\
                 **新颖性评分**：{}（{}）\n\
                 **核心结论**：{}\n\
                 **建议**：\n\
                 - 1. 对核心技术差异点进行深度论证\n\
                 - 2. 补充实验数据支撑新颖性主张\n\
                 - 3. 关注竞品动态，及时更新技术方案\n\
                 **风险等级**：{}",
                idea.title, score_line, level, conclusion, risk
            );
            Json(json!({
                "status": "ok",
                "report_type": "executive",
                "report": report_md,
            }))
        }
        "technical" => {
            // 研发版：详细技术报告
            let web_results =
                serde_json::from_str::<serde_json::Value>(&idea.web_results).unwrap_or_default();
            let patent_results =
                serde_json::from_str::<serde_json::Value>(&idea.patent_results).unwrap_or_default();
            let report_md = format!(
                "# 技术验证报告\n\n\
                 ## 创意概述\n\
                 **标题**：{}\n\
                 **描述**：{}\n\n\
                 ## 新颖性评分\n\
                 **总分**：{}（{}）\n\n\
                 ## 现有技术对比\n\
                 详见 web_results 和 patent_results 字段。\n\n\
                 ## 技术差异分析\n\
                 {}\n\n\
                 ## 行动方案\n\
                 基于以上分析，建议执行差异化策略。",
                idea.title, idea.description, score_line, level, analysis_text
            );
            Json(json!({
                "status": "ok",
                "report_type": "technical",
                "report": report_md,
                "web_results": web_results,
                "patent_results": patent_results,
            }))
        }
        "bom_pack" | "journal_draft" => {
            Json(build_deliverable_report_json(&s.db, &idea, report_type))
        }
        _ => {
            // 默认：完整原始数据（向后兼容）
            Json(json!({
                "status": "ok",
                "report_type": "full",
                "report": {
                    "id": idea.id,
                    "title": idea.title,
                    "description": idea.description,
                    "status": idea.status,
                    "novelty_score": idea.novelty_score,
                    "analysis": idea.analysis,
                    "web_results": serde_json::from_str::<serde_json::Value>(&idea.web_results).unwrap_or_default(),
                    "patent_results": serde_json::from_str::<serde_json::Value>(&idea.patent_results).unwrap_or_default(),
                    "created_at": idea.created_at,
                }
            }))
        }
    }
}

/// GET /api/idea/:id/report.html — 可打印的 HTML 验证报告（浏览器 Ctrl+P 另存 PDF）
/// Printable HTML validation report (browser Save as PDF)
pub async fn api_idea_report_html(
    State(s): State<AppState>,
    Path(idea_id): Path<String>,
) -> Response<Body> {
    let idea = match s.db.get_idea(&idea_id) {
        Ok(Some(i)) => i,
        _ => {
            return match Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("创意不存在"))
            {
                Ok(resp) => resp,
                Err(_) => Response::new(Body::from("创意不存在")),
            };
        }
    };

    let feature_cards = s.db.get_feature_cards_by_idea(&idea_id).unwrap_or_default();

    let cards_html: String = if feature_cards.is_empty() {
        "<p>暂无特征卡片</p>".to_string()
    } else {
        feature_cards
            .iter()
            .map(|c| {
                format!(
                    "<div class='card'>\
                     <h4>{}</h4>\
                     <p>{}</p>\
                     <span class='score'>新颖性: {}</span>\
                     </div>",
                    html_escape(&c.title),
                    html_escape(&c.description),
                    c.novelty_score
                        .map(|s| format!("{:.0}", s))
                        .unwrap_or_else(|| "N/A".into()),
                )
            })
            .collect()
    };

    let analysis_html = if idea.analysis.is_empty() {
        "分析尚未完成".to_string()
    } else {
        simple_md_to_html(&idea.analysis)
    };
    let score_display = idea
        .novelty_score
        .map(|s| format!("{:.0}/100", s))
        .unwrap_or_else(|| "N/A".into());

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="zh">
<head>
<meta charset="UTF-8">
<title>创新验证报告 — {title}</title>
<style>
  @media print {{ @page {{ margin: 1.5cm; }} }}
  body {{ font-family: -apple-system, "Microsoft YaHei", sans-serif; max-width: 800px; margin: 0 auto; padding: 20px; color: #333; line-height: 1.6; }}
  h1 {{ border-bottom: 2px solid #2563eb; padding-bottom: 8px; }}
  .meta {{ color: #666; margin-bottom: 20px; }}
  .score-box {{ background: #eff6ff; border: 1px solid #bfdbfe; border-radius: 8px; padding: 16px; text-align: center; margin: 20px 0; }}
  .score-box .score {{ font-size: 2em; font-weight: bold; color: #2563eb; }}
  .card {{ border: 1px solid #e5e7eb; border-radius: 6px; padding: 12px; margin: 8px 0; }}
  .card h4 {{ margin: 0 0 4px 0; }}
  .card .score {{ font-size: 0.85em; color: #059669; }}
  .section {{ margin-top: 24px; }}
  .section h2 {{ color: #1e40af; }}
  .analysis {{ background: #f9fafb; padding: 16px; border-radius: 8px; }}
  .print-btn {{ background: #2563eb; color: #fff; border: none; padding: 10px 24px; border-radius: 6px; cursor: pointer; font-size: 16px; }}
  @media print {{ .no-print {{ display: none; }} }}
</style>
</head>
<body>
<div class="no-print" style="text-align:right;margin-bottom:16px;">
  <button class="print-btn" onclick="window.print()">导出 PDF / 打印</button>
</div>
<h1>创新验证报告</h1>
<div class="meta">
  <strong>创意：</strong>{title}<br>
  <strong>描述：</strong>{description}<br>
  <strong>状态：</strong>{status}<br>
  <strong>创建时间：</strong>{created_at}
</div>

<div class="score-box">
  <div>新颖性评分</div>
  <div class="score">{score}</div>
</div>

<div class="section">
  <h2>AI 深度分析</h2>
  <div class="analysis">{analysis}</div>
</div>

<div class="section">
  <h2>特征卡片 ({card_count} 张)</h2>
  {cards}
</div>

<div class="section" style="color:#999;font-size:0.85em;margin-top:40px;border-top:1px solid #eee;padding-top:8px;">
  创研台 InnoForge · 创新推演报告 · {created_at}
</div>
</body>
</html>"#,
        title = html_escape(&idea.title),
        description = html_escape(&idea.description),
        status = html_escape(&idea.status),
        created_at = html_escape(&idea.created_at),
        score = score_display,
        analysis = analysis_html,
        card_count = feature_cards.len(),
        cards = cards_html,
    );

    match Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Body::from(html))
    {
        Ok(resp) => resp,
        Err(_) => Response::new(Body::from("Internal error generating report")),
    }
}

/// 简单 HTML 转义 / Simple HTML escape
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// 简易 Markdown → HTML（用于可打印报告，无需外部依赖）
#[allow(unused_assignments)]
fn simple_md_to_html(md: &str) -> String {
    let mut out = String::with_capacity(md.len() * 2);
    let mut in_table = false;
    let mut table_header_done = false;

    for line in md.lines() {
        let trimmed = line.trim();

        // Table rows
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            // Separator row (|---|---|)
            if trimmed
                .chars()
                .all(|c| c == '|' || c == '-' || c == ':' || c == ' ')
            {
                if !in_table {
                    in_table = true;
                    table_header_done = false;
                }
                table_header_done = true;
                continue;
            }
            if !in_table {
                out.push_str("<table style='border-collapse:collapse;width:100%;margin:12px 0;'>");
                in_table = true;
                table_header_done = false;
            }
            let cells: Vec<&str> = trimmed.split('|').filter(|c| !c.is_empty()).collect();
            let tag = if !table_header_done { "th" } else { "td" };
            let bg = if !table_header_done {
                "background:#eff6ff;"
            } else {
                ""
            };
            out.push_str("<tr>");
            for cell in cells {
                out.push_str(&format!(
                    "<{tag} style='border:1px solid #d1d5db;padding:6px 10px;{bg}'>{}</{tag}>",
                    html_escape(cell.trim())
                ));
            }
            out.push_str("</tr>");
            if !table_header_done {
                table_header_done = true;
            }
            continue;
        }

        // End table if we were in one
        if in_table {
            out.push_str("</table>");
            in_table = false;
            table_header_done = false;
        }

        if trimmed.is_empty() {
            out.push_str("<br>");
        } else if let Some(h) = trimmed.strip_prefix("### ") {
            out.push_str(&format!(
                "<h3 style='color:#1e40af;margin:16px 0 8px;'>{}</h3>",
                html_escape(h)
            ));
        } else if let Some(h) = trimmed.strip_prefix("## ") {
            out.push_str(&format!(
                "<h2 style='color:#1e40af;margin:20px 0 8px;'>{}</h2>",
                html_escape(h)
            ));
        } else if let Some(h) = trimmed.strip_prefix("# ") {
            out.push_str(&format!(
                "<h1 style='color:#1e40af;'>{}</h1>",
                html_escape(h)
            ));
        } else if let Some(item) = trimmed.strip_prefix("- ") {
            out.push_str(&format!(
                "<li style='margin:4px 0;'>{}</li>",
                inline_md(item)
            ));
        } else if trimmed.starts_with("1. ")
            || trimmed.starts_with("2. ")
            || trimmed.starts_with("3. ")
        {
            if let Some(pos) = trimmed.find(". ") {
                let item = &trimmed[pos + 2..];
                out.push_str(&format!(
                    "<li style='margin:4px 0;'>{}</li>",
                    inline_md(item)
                ));
            }
        } else {
            out.push_str(&format!(
                "<p style='margin:6px 0;'>{}</p>",
                inline_md(trimmed)
            ));
        }
    }

    if in_table {
        out.push_str("</table>");
    }
    out
}

/// 行内 Markdown: **bold**, *italic*, `code`
pub fn inline_md(s: &str) -> String {
    use std::sync::OnceLock;

    static RE_BOLD: OnceLock<Result<regex::Regex, regex::Error>> = OnceLock::new();
    static RE_ITALIC: OnceLock<Result<regex::Regex, regex::Error>> = OnceLock::new();
    static RE_CODE: OnceLock<Result<regex::Regex, regex::Error>> = OnceLock::new();

    let escaped = html_escape(s);
    let re_bold = match RE_BOLD
        .get_or_init(|| regex::Regex::new(r"\*\*(.+?)\*\*"))
        .as_ref()
    {
        Ok(regex) => regex,
        Err(error) => {
            tracing::error!(error = %error, "Failed to initialize inline Markdown bold regex");
            return escaped;
        }
    };
    let s = re_bold.replace_all(&escaped, "<strong>$1</strong>");
    let re_italic = match RE_ITALIC
        .get_or_init(|| regex::Regex::new(r"\*(.+?)\*"))
        .as_ref()
    {
        Ok(regex) => regex,
        Err(error) => {
            tracing::error!(error = %error, "Failed to initialize inline Markdown italic regex");
            return escaped;
        }
    };
    let s = re_italic.replace_all(&s, "<em>$1</em>");
    let re_code = match RE_CODE
        .get_or_init(|| regex::Regex::new(r"`([^`]+)`"))
        .as_ref()
    {
        Ok(regex) => regex,
        Err(error) => {
            tracing::error!(error = %error, "Failed to initialize inline Markdown code regex");
            return escaped;
        }
    };
    let s = re_code.replace_all(
        &s,
        "<code style='background:#f3f4f6;padding:1px 4px;border-radius:3px;'>$1</code>",
    );
    s.into_owned()
}

/// 导出创意页讨论的原始聊天记录（供另一个人工智能核对/复核）。
/// 返回干净的 JSON：标题、描述、按时间排序的消息数组（角色 + 内容 + 时间）。
/// 不含 AI 内部推理或图片 base64 等噪音，可直接喂给其它 AI。
pub async fn api_idea_chat_conversation(
    State(s): State<AppState>,
    Path(idea_id): Path<String>,
) -> Json<serde_json::Value> {
    let idea = match s.db.get_idea(&idea_id) {
        Ok(Some(i)) => i,
        _ => return Json(json!({"error": "创意不存在"})),
    };
    let messages = s.db.get_idea_messages(&idea_id).unwrap_or_default();
    let out: Vec<serde_json::Value> = messages.iter().map(|(_id, role, content, created_at)| {
        json!({"role": role, "content": content, "created_at": created_at})
    }).collect();
    Json(json!({
        "status": "ok",
        "idea": { "id": idea.id, "title": idea.title, "description": idea.description },
        "messages": out,
    }))
}

/// Export structured conclusions from the idea discussion.
/// 从讨论中导出结构化结论（已定决策 / 达成的结论 / 待解决问题 / 风险项）。
pub async fn api_idea_chat_conclusions(
    State(s): State<AppState>,
    Path(idea_id): Path<String>,
) -> Json<serde_json::Value> {
    let idea = match s.db.get_idea(&idea_id) {
        Ok(Some(i)) => i,
        _ => return Json(json!({"error": "创意不存在"})),
    };

    let history = s.db.get_idea_messages(&idea_id).unwrap_or_default();
    if history.is_empty() {
        return Json(json!({"error": "还没有讨论记录，请先开始讨论后再导出结论"}));
    }

    let summary = s.db.get_idea_summary(&idea_id).unwrap_or_default();

    // Build conversation context
    let mut conv = format!(
        "创意标题：{}\n创意描述：{}\n\n",
        idea.title, idea.description
    );

    if !summary.is_empty() {
        conv.push_str(&format!("之前的讨论摘要：\n{}\n\n", summary));
    }

    conv.push_str("完整讨论记录：\n");
    for (_id, role, content, _ts) in &history {
        let role_label = if role == "user" { "用户" } else { "AI" };
        let preview = truncate_for_ai(content, 800);
        conv.push_str(&format!("\n【{}】{}\n", role_label, preview));
    }

    let prompt = format!(
        "{}\n\n请基于以上讨论，输出一份结构化结论报告。严格按以下格式输出：\n\n\
         ## 已定决策\n\
         - 列出已确定的技术方案或选择，每条标明理由\n\n\
         ## 达成的结论\n\
         - 列出经过讨论验证的技术判断，每条给出支撑证据\n\n\
         ## 待解决问题\n\
         - 列出尚未得出结论或需要进一步验证的问题\n\n\
         ## 风险项及等级\n\
         - 致命缺陷：列出可能导致方案失败的致命风险\n\
         - 需要验证：列出需要实验或数据验证的中等风险\n\
         - 可接受风险：列出已评估可接受的低风险\n\n\
         要求：结论要具体、有依据，不写泛泛的空话。全文必须用中文撰写（即使输入材料是英文也翻译成中文），\
         专业术语、专利号、化学式可保留原文。",
        truncate_for_ai(&conv, 5_000)
    );

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client();
    match ai.chat(&prompt, None).await {
        Ok(conclusions) => {
            let _ =
                s.db.log_ai_call(&ai, "idea-conclusions", Some(&idea_id), None);
            // Also update the idea summary with conclusions for future reference
            let _ = s.db.update_idea_summary(&idea_id, &conclusions);
            Json(json!({"status": "ok", "conclusions": conclusions}))
        }
        Err(e) => Json(json!({ "error": format!("导出结论失败: {}", e) })),
    }
}

/// Generate a summary of the idea discussion
pub async fn api_idea_summarize_discussion(
    State(s): State<AppState>,
    Path(idea_id): Path<String>,
) -> Json<serde_json::Value> {
    let idea = match s.db.get_idea(&idea_id) {
        Ok(Some(i)) => i,
        _ => return Json(json!({"error": "创意不存在"})),
    };

    let history = s.db.get_idea_messages(&idea_id).unwrap_or_default();
    if history.is_empty() {
        return Json(json!({"error": "没有讨论记录可以总结"}));
    }

    // Build conversation text for summarization
    let mut conversation = format!(
        "创意标题：{}\n创意描述：{}\n\n讨论记录：\n",
        idea.title, idea.description
    );
    for (_id, role, content, _ts) in &history {
        let role_label = if role == "user" { "用户" } else { "AI" };
        let content_preview = truncate_for_ai(content, 500);
        conversation.push_str(&format!("\n【{}】{}\n", role_label, content_preview));
    }

    let prompt = format!(
        "{}\n\n请对以上讨论进行总结，包括：\n\
         1. **核心讨论点**：讨论了哪些关键问题\n\
         2. **达成的共识**：确定了哪些方向或方案\n\
         3. **待解决问题**：还有哪些未决问题\n\
         4. **行动建议**：下一步应该做什么\n\n\
         总结要简洁，重点突出。全文必须用中文撰写（即使输入材料是英文也翻译成中文），\
         专业术语可保留原文。",
        truncate_for_ai(&conversation, 4_000)
    );

    let ai = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client();
    match ai.chat(&prompt, None).await {
        Ok(summary) => {
            let _ =
                s.db.log_ai_call(&ai, "idea-summarize", Some(&idea_id), None);
            // Save summary to DB
            let _ = s.db.update_idea_summary(&idea_id, &summary);
            Json(json!({"status": "ok", "summary": summary}))
        }
        Err(e) => Json(json!({ "error": format!("总结生成失败: {}", e) })),
    }
}

/// POST /api/ideas/batch-compare — 批量创意对比矩阵 / Batch idea comparison matrix
///
/// 请求：{ "idea_ids": ["id1", "id2", ...] }（2-10 个）
/// 返回：每个创意的摘要 + 两两相似度矩阵
pub async fn api_ideas_batch_compare(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let ids: Vec<String> = match req["idea_ids"].as_array() {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        None => return Json(json!({"status": "error", "message": "缺少 idea_ids 数组"})),
    };
    if ids.len() < 2 {
        return Json(json!({"status": "error", "message": "至少需要 2 个创意进行对比"}));
    }
    if ids.len() > 10 {
        return Json(json!({"status": "error", "message": "最多支持 10 个创意同时对比"}));
    }

    // 加载所有创意
    let mut ideas = Vec::new();
    for id in &ids {
        match s.db.get_idea(id) {
            Ok(Some(idea)) => ideas.push(idea),
            _ => return Json(json!({"status": "error", "message": format!("创意 {} 不存在", id)})),
        }
    }

    // 构建摘要列表
    let summaries: Vec<serde_json::Value> = ideas
        .iter()
        .map(|i| {
            json!({
                "id": i.id,
                "title": i.title,
                "description": i.description.chars().take(200).collect::<String>(),
                "novelty_score": i.novelty_score,
                "status": i.status,
            })
        })
        .collect();

    // 计算两两文本相似度矩阵（Jaccard on character trigrams）
    let n = ideas.len();
    let mut matrix = vec![vec![0.0f64; n]; n];
    let trigrams: Vec<std::collections::HashSet<String>> = ideas
        .iter()
        .map(|i| {
            let text = format!("{} {}", i.title, i.description);
            char_trigrams(&text)
        })
        .collect();

    for i in 0..n {
        matrix[i][i] = 1.0;
        for j in (i + 1)..n {
            let sim = jaccard_trigram(&trigrams[i], &trigrams[j]);
            matrix[i][j] = sim;
            matrix[j][i] = sim;
        }
    }

    Json(json!({
        "status": "ok",
        "ideas": summaries,
        "similarity_matrix": matrix,
    }))
}

/// 字符三元组集合 / Character trigram set
fn char_trigrams(text: &str) -> std::collections::HashSet<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut set = std::collections::HashSet::new();
    if chars.len() >= 3 {
        for w in chars.windows(3) {
            set.insert(w.iter().collect::<String>());
        }
    }
    set
}

/// Jaccard 相似度 / Jaccard similarity between two sets
fn jaccard_trigram(
    a: &std::collections::HashSet<String>,
    b: &std::collections::HashSet<String>,
) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let inter = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

// ── 版本管理 + 迭代 API / Version management + iterate API ────────────

/// POST /api/idea/:id/iterate — 持续迭代：基于上一轮结果，用 Orchestrator 跳转重跑
pub async fn api_idea_iterate(
    State(s): State<AppState>,
    Path(idea_id): Path<String>,
) -> Json<serde_json::Value> {
    // 获取最新版本快照
    let latest = match s.db.get_latest_version(&idea_id, "main") {
        Ok(Some(v)) => v,
        Ok(None) => {
            return Json(
                serde_json::json!({"status": "error", "message": "无历史版本，请先运行 Pipeline"}),
            );
        }
        Err(e) => {
            return Json(serde_json::json!({"status": "error", "message": e.to_string()}));
        }
    };

    // 反序列化上一轮 context
    let mut ctx: crate::pipeline::context::PipelineContext = match serde_json::from_str(
        &latest.context_json,
    ) {
        Ok(c) => c,
        Err(e) => {
            return Json(
                serde_json::json!({"status": "error", "message": format!("快照反序列化失败: {}", e)}),
            );
        }
    };

    // 递增迭代计数
    ctx.iteration_count += 1;
    ctx.parent_version_id = latest.id.clone();

    // v0.9.68：加载并合并数据库持久化的 ResearchState（含跨轮次 excluded_paths 负向约束）
    if let Ok(Some(persisted)) = s.db.get_research_state(&idea_id) {
        crate::orchestrator::constraints::merge_persisted_research_state(&mut ctx, persisted);
    }

    // 使用 open_questions 作为额外搜索种子
    if !ctx.research_state.open_questions.is_empty() {
        let extra_queries: Vec<String> = ctx
            .research_state
            .open_questions
            .iter()
            .take(3)
            .map(|q| q.chars().take(50).collect())
            .collect();
        ctx.expanded_queries.extend(extra_queries);
    }

    // 将用户已导出确认的讨论定论作为下一轮研究的种子（讨论->定论->迭代的桥）。
    if let Some(ref idea) = s.db.get_idea(&idea_id).ok().flatten() {
        if !idea.discussion_summary.is_empty() {
            let summary: String = idea.discussion_summary.chars().take(700).collect();
            ctx.expanded_queries.push(format!(
                "讨论已得出定论（请基于此继续深入研究）:\n{}",
                summary
            ));
        }
    }

    let iteration = ctx.iteration_count;

    // 用 Orchestrator 从 SearchWeb 跳转重跑（不是线性续跑）
    let config = s.config.read().unwrap_or_else(|e| e.into_inner()).clone();
    let mut orch = crate::orchestrator::engine::Orchestrator::new(
        config.ai_client(),
        fast_ai_channel(&config),
        s.db.clone(),
        config.next_serpapi_key().unwrap_or_default(),
        false,
    );
    orch.inject_command(crate::orchestrator::OrchestratorCommand::Jump(
        crate::pipeline::state::PipelineStep::SearchWeb,
    ));

    let db = s.db.clone();
    let idea_id_clone = idea_id.clone();
    tokio::spawn(async move {
        match orch.run(ctx, None).await {
            Ok(result_ctx) => {
                tracing::info!(
                    "Iterate completed for {} (iter={})",
                    idea_id_clone,
                    iteration
                );
                if let Ok(Some(mut idea)) = db.get_idea(&idea_id_clone) {
                    idea.novelty_score = if result_ctx.novelty_status
                        == crate::pipeline::context::NoveltyStatus::Sufficient
                    {
                        Some(result_ctx.novelty_score)
                    } else {
                        None
                    };
                    idea.analysis = result_ctx.ai_analysis;
                    idea.status = "done".into();
                    let _ = db.update_idea(&idea);
                }
            }
            Err(e) => {
                tracing::error!("Iterate failed: {}", e);
                if let Ok(Some(mut idea)) = db.get_idea(&idea_id_clone) {
                    idea.analysis = format!("迭代失败: {}", e);
                    idea.status = "error".into();
                    let _ = db.update_idea(&idea);
                }
            }
        }
    });

    Json(serde_json::json!({
        "status": "ok",
        "message": format!("第 {} 轮迭代已启动（Orchestrator Jump→SearchWeb）", iteration),
        "iteration": iteration,
    }))
}

/// GET /api/idea/:id/checkpoints — 列出所有步骤级检查点快照及当前跨轮次 ResearchState（v0.9.68）
pub async fn api_idea_checkpoints(
    State(s): State<AppState>,
    Path(idea_id): Path<String>,
) -> Json<serde_json::Value> {
    let versions = s.db.get_idea_versions(&idea_id).unwrap_or_default();
    let research_state =
        s.db.get_research_state(&idea_id)
            .ok()
            .flatten()
            .unwrap_or_default();

    let checkpoints: Vec<serde_json::Value> = versions
        .into_iter()
        .map(|v| {
            let step_label = crate::pipeline::state::PipelineStep::from_name(&v.current_step)
                .map(|st| st.description().to_string())
                .unwrap_or_else(|| v.current_step.clone());
            let parsed_ctx: Option<crate::pipeline::context::PipelineContext> =
                serde_json::from_str(&v.context_json).ok();
            let auto_refill_rounds = parsed_ctx
                .as_ref()
                .map(|c| c.auto_refill_rounds)
                .unwrap_or(0);
            let excluded_count = parsed_ctx
                .as_ref()
                .map(|c| c.research_state.excluded_paths.len())
                .unwrap_or(0);
            serde_json::json!({
                "version_id": v.id,
                "version_number": v.version_number,
                "step": v.current_step,
                "step_label": step_label,
                "branch_id": v.branch_id,
                "created_at": v.created_at,
                "auto_refill_rounds": auto_refill_rounds,
                "excluded_paths_count": excluded_count,
            })
        })
        .collect();

    Json(serde_json::json!({
        "status": "ok",
        "checkpoints": checkpoints,
        "research_state": research_state,
    }))
}

/// POST /api/idea/:id/checkpoint/rollback — 步骤级检查点回滚并创建新分支重跑（v0.9.68）
pub async fn api_idea_checkpoint_rollback(
    State(s): State<AppState>,
    Path(idea_id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let version_id = req["version_id"].as_str().unwrap_or("").trim();
    if version_id.is_empty() {
        return Json(serde_json::json!({
            "status": "error",
            "message": "缺少 version_id（请选择要回滚的步骤检查点）"
        }));
    }

    let ver = match s.db.get_idea_version_by_id(version_id) {
        Ok(Some(v)) if v.idea_id == idea_id => v,
        Ok(Some(_)) => {
            return Json(serde_json::json!({
                "status": "error",
                "message": "检查点快照不属于当前创意"
            }));
        }
        Ok(None) => {
            return Json(serde_json::json!({
                "status": "error",
                "message": "未找到指定的检查点快照"
            }));
        }
        Err(e) => {
            return Json(serde_json::json!({
                "status": "error",
                "message": format!("读取检查点失败: {e}")
            }));
        }
    };

    let mut ctx: crate::pipeline::context::PipelineContext =
        match serde_json::from_str(&ver.context_json) {
            Ok(c) => c,
            Err(e) => {
                return Json(serde_json::json!({
                    "status": "error",
                    "message": format!("检查点上下文反序列化失败: {e}")
                }));
            }
        };

    // 合并最新持久化的 ResearchState，确保不丢失已记录的负向约束
    if let Ok(Some(persisted)) = s.db.get_research_state(&idea_id) {
        crate::orchestrator::constraints::merge_persisted_research_state(&mut ctx, persisted);
    }

    // 应用用户局部输入微调
    if let Some(desc) = req["override_description"]
        .as_str()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        ctx.description = desc.to_string();
    }
    if let Some(kws) = req["override_keywords"].as_array() {
        let parsed: Vec<String> = kws
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        if !parsed.is_empty() {
            ctx.keywords = parsed;
        }
    }
    if let Some(excl) = req["add_excluded_path"]
        .as_str()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        if !ctx.research_state.excluded_paths.iter().any(|p| p == excl) {
            ctx.research_state.excluded_paths.push(excl.to_string());
        }
        let _ = s.db.upsert_research_state(&idea_id, &ctx.research_state);
    }

    let requested_step = req["target_step"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&ver.current_step);
    let target_step = crate::pipeline::state::PipelineStep::from_name(requested_step)
        .unwrap_or(crate::pipeline::state::PipelineStep::ExpandQuery);

    let branch_name = req["branch_name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("rollback-v{}-{}", ver.version_number, requested_step));

    ctx.parent_version_id = ver.id.clone();
    ctx.iteration_count += 1;
    let branch_id = match crate::orchestrator::create_branch(&s.db, &ctx, &branch_name) {
        Ok(bid) => bid,
        Err(e) => {
            return Json(serde_json::json!({
                "status": "error",
                "message": format!("创建回滚分支失败: {e}")
            }));
        }
    };
    ctx.branch_id = branch_id.clone();

    if let Err(breaker_msg) = s.db.check_ai_budget_circuit_breaker(Some(&idea_id)) {
        return Json(serde_json::json!({
            "status": "error",
            "message": breaker_msg
        }));
    }

    let config = s.config.read().unwrap_or_else(|e| e.into_inner()).clone();
    let mut orch = crate::orchestrator::engine::Orchestrator::new(
        config.ai_client(),
        fast_ai_channel(&config),
        s.db.clone(),
        config.next_serpapi_key().unwrap_or_default(),
        false,
    );
    orch.inject_command(crate::orchestrator::OrchestratorCommand::Jump(target_step));

    let db = s.db.clone();
    let idea_id_clone = idea_id.clone();
    let excluded_count = ctx.research_state.excluded_paths.len();
    tokio::spawn(async move {
        match orch.run(ctx, None).await {
            Ok(result_ctx) => {
                if let Ok(Some(mut idea)) = db.get_idea(&idea_id_clone) {
                    idea.novelty_score = if result_ctx.novelty_status
                        == crate::pipeline::context::NoveltyStatus::Sufficient
                    {
                        Some(result_ctx.novelty_score)
                    } else {
                        None
                    };
                    idea.analysis = result_ctx.ai_analysis;
                    idea.status = "done".into();
                    let _ = db.update_idea(&idea);
                }
            }
            Err(e) => {
                tracing::error!("Checkpoint rollback failed: {}", e);
                if let Ok(Some(mut idea)) = db.get_idea(&idea_id_clone) {
                    idea.analysis = format!("检查点分支重跑失败: {}", e);
                    idea.status = "error".into();
                    let _ = db.update_idea(&idea);
                }
            }
        }
    });

    Json(serde_json::json!({
        "status": "ok",
        "branch_id": branch_id,
        "branch_name": branch_name,
        "target_step": format!("{:?}", target_step),
        "excluded_paths_count": excluded_count,
        "message": format!("已从版本 v{} ({}) 创建新分支「{}」并启动局部重跑", ver.version_number, target_step.description(), branch_name)
    }))
}

fn resolve_pipeline_context_for_deliverables(
    db: &crate::db::Database,
    idea: &Idea,
) -> crate::pipeline::context::PipelineContext {
    if let Ok(Some((ctx_json, _))) = db.load_pipeline_snapshot(&idea.id) {
        if let Ok(ctx) =
            serde_json::from_str::<crate::pipeline::context::PipelineContext>(&ctx_json)
        {
            return ctx;
        }
    }
    let mut ctx =
        crate::pipeline::context::PipelineContext::new(&idea.id, &idea.title, &idea.description);
    ctx.ai_analysis = idea.analysis.clone();
    ctx.web_results = serde_json::from_str(&idea.web_results).unwrap_or_default();
    ctx.patent_results = serde_json::from_str(&idea.patent_results).unwrap_or_default();
    ctx
}

fn build_deliverable_report_json(
    db: &crate::db::Database,
    idea: &Idea,
    report_type: &str,
) -> serde_json::Value {
    let concept_key = if report_type == "bom_pack" {
        "deliverable_bom"
    } else {
        "deliverable_journal_draft"
    };
    let persisted = db
        .get_memory_by_type(&idea.id, "decision")
        .unwrap_or_default()
        .into_iter()
        .find(|m| m.concept_name == concept_key)
        .map(|m| m.content);

    let ctx = resolve_pipeline_context_for_deliverables(db, idea);
    if report_type == "bom_pack" {
        let (report_md, source) = match persisted {
            Some(md) => (md, "persisted_memory"),
            None => {
                let cards = db.get_feature_cards_by_idea(&idea.id).unwrap_or_default();
                let pack =
                    crate::pipeline::steps::deliverables::build_engineering_bom_pack(&ctx, &cards);
                (pack.full_markdown, "generated_on_demand")
            }
        };
        return json!({
            "status": "ok",
            "idea_id": idea.id,
            "title": idea.title,
            "type": "bom_pack",
            "report_type": "bom_pack",
            "source": source,
            "report": report_md,
        });
    }

    let draft = crate::pipeline::steps::deliverables::build_journal_manuscript_draft(&ctx);
    let (report_md, source) = match persisted {
        Some(md) => (md, "persisted_memory"),
        None => (draft.full_markdown, "generated_on_demand"),
    };
    json!({
        "status": "ok",
        "idea_id": idea.id,
        "title": idea.title,
        "type": "journal_draft",
        "report_type": "journal_draft",
        "source": source,
        "novelty_lock_warning": draft.novelty_lock_warning,
        "cited_dois": draft.cited_dois,
        "report": report_md,
    })
}

/// 生成三 Sheet 标准化《工程落地交付包》`.xlsx` 字节流（BOM 清单 + 电气控制 IO 表 + 台架验收大纲）。
pub fn build_delivery_pack_xlsx(db: &crate::db::Database, idea: &Idea) -> Result<Vec<u8>, String> {
    use rust_xlsxwriter::{Color, Format, Workbook};

    let ctx = resolve_pipeline_context_for_deliverables(db, idea);
    let cards = db.get_feature_cards_by_idea(&idea.id).unwrap_or_default();
    let pack = crate::pipeline::steps::deliverables::build_engineering_bom_pack(&ctx, &cards);

    let mut workbook = Workbook::new();
    let header_fmt = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0xE8EEF5));

    // Sheet 1: 工程 BOM 清单
    let ws1 = workbook.add_worksheet();
    ws1.set_name("工程BOM清单").map_err(|e| e.to_string())?;
    ws1.set_freeze_panes(1, 0).map_err(|e| e.to_string())?;
    let s1_headers = [
        "序号",
        "部件标号",
        "名称",
        "类别",
        "关键规格参数",
        "材质/耐温耐压要求",
        "参考数量",
        "备注",
    ];
    for (col, h) in s1_headers.iter().enumerate() {
        ws1.write_string_with_format(0, col as u16, *h, &header_fmt)
            .map_err(|e| e.to_string())?;
    }
    for (idx, item) in pack.bom_items.iter().enumerate() {
        let row = (idx + 1) as u32;
        ws1.write_number(row, 0, item.item_no as f64)
            .map_err(|e| e.to_string())?;
        ws1.write_string(row, 1, format!("P{:02}", item.item_no))
            .map_err(|e| e.to_string())?;
        ws1.write_string(row, 2, &item.part_name)
            .map_err(|e| e.to_string())?;
        ws1.write_string(row, 3, &item.category)
            .map_err(|e| e.to_string())?;
        ws1.write_string(row, 4, &item.spec_summary)
            .map_err(|e| e.to_string())?;
        let env_req = if item.category.contains("自制") {
            "工业级金属/工程塑料（耐压耐温按工况标定）"
        } else {
            "原厂标准工业级规格"
        };
        ws1.write_string(row, 5, env_req)
            .map_err(|e| e.to_string())?;
        ws1.write_number(row, 6, 1.0).map_err(|e| e.to_string())?;
        ws1.write_string(row, 7, &item.source_feature)
            .map_err(|e| e.to_string())?;
    }
    ws1.autofit();

    // Sheet 2: 电气控制 IO 与参数表
    let ws2 = workbook.add_worksheet();
    ws2.set_name("电气控制IO表").map_err(|e| e.to_string())?;
    ws2.set_freeze_panes(1, 0).map_err(|e| e.to_string())?;
    let s2_headers = [
        "通道/点位",
        "信号类型",
        "关联执行器/传感器",
        "量程与控制逻辑说明",
    ];
    for (col, h) in s2_headers.iter().enumerate() {
        ws2.write_string_with_format(0, col as u16, *h, &header_fmt)
            .map_err(|e| e.to_string())?;
    }
    let io_rows = parse_markdown_table_rows(&pack.control_io_spec_md, 4);
    for (idx, cols) in io_rows.iter().enumerate() {
        let row = (idx + 1) as u32;
        for (c_idx, val) in cols.iter().enumerate() {
            ws2.write_string(row, c_idx as u16, val)
                .map_err(|e| e.to_string())?;
        }
    }
    ws2.autofit();

    // Sheet 3: 样机台架测试验收大纲
    let ws3 = workbook.add_worksheet();
    ws3.set_name("样机台架测试验收大纲")
        .map_err(|e| e.to_string())?;
    ws3.set_freeze_panes(1, 0).map_err(|e| e.to_string())?;
    let s3_headers = ["测试项", "输入工况", "目标指标", "判定标准"];
    for (col, h) in s3_headers.iter().enumerate() {
        ws3.write_string_with_format(0, col as u16, *h, &header_fmt)
            .map_err(|e| e.to_string())?;
    }
    let test_rows = parse_markdown_table_rows(&pack.bench_test_outline_md, 4);
    if test_rows.is_empty() {
        ws3.write_string(1, 0, "功能与指标验证")
            .map_err(|e| e.to_string())?;
        ws3.write_string(1, 1, "额定工况")
            .map_err(|e| e.to_string())?;
        let metric = ctx
            .experiment_results
            .first()
            .map(|e| format!("exit_code={}（实测指标见第13步）", e.exit_code))
            .unwrap_or_else(|| "【待台架标定】".to_string());
        ws3.write_string(1, 2, &metric).map_err(|e| e.to_string())?;
        ws3.write_string(1, 3, "满足设计指标且无异常告警")
            .map_err(|e| e.to_string())?;
    } else {
        for (idx, cols) in test_rows.iter().enumerate() {
            let row = (idx + 1) as u32;
            for (c_idx, val) in cols.iter().enumerate() {
                ws3.write_string(row, c_idx as u16, val)
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    ws3.autofit();

    workbook.save_to_buffer().map_err(|e| e.to_string())
}

fn parse_markdown_table_rows(md: &str, expected_cols: usize) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut skipped_header = false;
    for line in md.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
            continue;
        }
        let inner = trimmed.trim_matches('|');
        let cells: Vec<String> = inner.split('|').map(|s| s.trim().to_string()).collect();
        if cells
            .iter()
            .all(|c| c.chars().all(|ch| ch == '-' || ch == ':' || ch == ' '))
        {
            skipped_header = true;
            continue;
        }
        if !skipped_header {
            continue;
        }
        let mut padded = cells;
        padded.resize(expected_cols, String::new());
        rows.push(padded);
    }
    rows
}

/// GET /api/ideas/:id/delivery-pack/xlsx — 一键导出三 Sheet 工程 BOM 交付包 (.xlsx)
pub async fn api_idea_delivery_pack_xlsx(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> axum::response::Response {
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;

    let idea = match s.db.get_idea(&id) {
        Ok(Some(i)) => i,
        _ => {
            return (StatusCode::NOT_FOUND, "Idea not found").into_response();
        }
    };

    match build_delivery_pack_xlsx(&s.db, &idea) {
        Ok(bytes) => {
            let safe_id: String = idea
                .id
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            let filename = format!(
                "delivery-pack-{}.xlsx",
                if safe_id.is_empty() { "bom" } else { &safe_id }
            );
            (
                StatusCode::OK,
                [
                    (
                        header::CONTENT_TYPE,
                        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                            .to_string(),
                    ),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{filename}\""),
                    ),
                ],
                bytes,
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("XLSX build error: {e}"),
        )
            .into_response(),
    }
}

/// POST /api/ideas/:id/control-prototype — 生成 IEC 61131-3 ST 控制原型、多段标定表与闭环阶跃响应曲线
pub async fn api_idea_control_prototype(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let idea = match s.db.get_idea(&id) {
        Ok(Some(i)) => i,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "status": "error", "message": "Idea not found" })),
            )
                .into_response();
        }
    };

    let cards = s.db.get_feature_cards_by_idea(&idea.id).unwrap_or_default();
    let ai_expert = {
        let cfg = s.config.read().unwrap_or_else(|e| e.into_inner());
        if cfg.ai_expert_channel_ready() {
            Some(cfg.ai_client_expert())
        } else {
            None
        }
    };

    let result = crate::pipeline::steps::control_proto::generate_control_prototype(
        &idea.id,
        &idea.title,
        &cards,
        ai_expert.as_ref(),
    )
    .await;

    (StatusCode::OK, Json(json!(result))).into_response()
}

/// POST /api/ideas/:id/fto-scan — 批量 FTO 侵权风险主动扫描与规避设计生成
pub async fn api_idea_fto_scan(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let idea = match s.db.get_idea(&id) {
        Ok(Some(i)) => i,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "status": "error", "message": "Idea not found" })),
            )
                .into_response();
        }
    };

    let cards = s.db.get_feature_cards_by_idea(&idea.id).unwrap_or_default();
    let mut ids: Vec<String> =
        s.db.search_smart(&idea.title, None, None, None, None, 1, 20)
            .map(|(list, _, _)| list.into_iter().map(|p| p.id).collect())
            .unwrap_or_default();
    if ids.is_empty() {
        ids =
            s.db.search_like("", None, None, None, 1, 20)
                .map(|(list, _)| list.into_iter().map(|p| p.id).collect())
                .unwrap_or_default();
    }
    let patents = if ids.is_empty() {
        Vec::new()
    } else {
        s.db.get_patents_by_ids(&ids).unwrap_or_default()
    };

    let solution_desc = format!("{}\n{}", idea.title, idea.description);
    let report =
        crate::ai::fto_scanner::run_batch_fto_scan(&idea.id, &solution_desc, &cards, &patents);

    // 同步沉淀至 idea_memory 供 Batch 10 一键归档使用
    let now = chrono::Utc::now().to_rfc3339();
    let _ = s.db.save_memory(&crate::db::memory::IdeaMemory {
        id: format!("fto-{}", idea.id),
        idea_id: idea.id.clone(),
        concept_name: "fto_report".to_string(),
        concept_type: "decision".to_string(),
        content: serde_json::to_string_pretty(&report).unwrap_or_default(),
        confidence: 0.85,
        source_step: Some("BatchFtoScan".to_string()),
        created_at: now.clone(),
        updated_at: now,
    });

    (StatusCode::OK, Json(json!(report))).into_response()
}

/// 生成全链路 5 目录标准化项目归档 `.zip` 内存字节流。
pub async fn build_project_archive_zip(
    db: &crate::db::Database,
    idea: &Idea,
) -> Result<Vec<u8>, String> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let ctx = resolve_pipeline_context_for_deliverables(db, idea);
    let cards = db.get_feature_cards_by_idea(&idea.id).unwrap_or_default();
    let pack = crate::pipeline::steps::deliverables::build_engineering_bom_pack(&ctx, &cards);
    let journal = crate::pipeline::steps::deliverables::build_journal_manuscript_draft(&ctx);
    let xlsx_bytes = build_delivery_pack_xlsx(db, idea)?;
    let ctrl_proto = crate::pipeline::steps::control_proto::generate_control_prototype(
        &idea.id,
        &idea.title,
        &cards,
        None,
    )
    .await;
    let fallback_sim = crate::pipeline::steps::control_proto::run_closed_loop_step_simulation(
        &crate::pipeline::steps::control_proto::default_calibration_table(),
    );
    let dois = crate::pipeline::steps::deliverables::extract_verified_dois(&ctx);
    let db_patents = db.get_patents_by_numbers(&dois).unwrap_or_default();
    let latex_bundle = crate::pipeline::steps::deliverables::build_paper_latex_bundle(
        &ctx,
        &cards,
        &db_patents,
        Some(&fallback_sim),
    );
    let paper_docx_bytes = build_paper_docx_bytes(db, idea)?;
    let docx_bytes = crate::routes::oa_export::build_oa_docx_bytes(
        &idea.title,
        "",
        &idea.description,
        &idea.id,
        "",
    )?;

    let fto_md = db
        .get_memory_by_type(&idea.id, "decision")
        .unwrap_or_default()
        .into_iter()
        .find(|m| m.concept_name == "fto_report")
        .map(|m| m.content)
        .unwrap_or_else(|| "（尚未执行批量 FTO 扫描，可在项目页点击执行）".to_string());

    let research_md = format!(
        "# 查新调研与 FTO 风险档案\n\n项目：{}\n\n## 绑定真实 DOI 文献\n{}\n\n## FTO 扫描记录\n{}\n",
        idea.title,
        if journal.cited_dois.is_empty() {
            "无".to_string()
        } else {
            journal.cited_dois.join("\n")
        },
        fto_md
    );

    let cad_md = format!(
        "# CAD 与机械结构拓扑说明\n\n项目：{}\n\n## 特征卡片摘要\n{}\n",
        idea.title,
        cards
            .iter()
            .map(|c| format!("- {}: {}", c.title, c.description))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let sim_json = serde_json::to_string_pretty(&ctrl_proto.simulation).unwrap_or_default();

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default();

        let entries: Vec<(&str, Vec<u8>)> = vec![
            (
                "01_Research_and_FTO/research_and_fto.md",
                research_md.into_bytes(),
            ),
            (
                "02_CAD_and_Mechanics/part_topology_and_cad.md",
                cad_md.into_bytes(),
            ),
            (
                "03_Control_and_Simulation/FB_ClosedLoopControl.st",
                ctrl_proto.st_code.into_bytes(),
            ),
            (
                "03_Control_and_Simulation/simulation_metrics.json",
                sim_json.into_bytes(),
            ),
            (
                "04_BOM_and_Delivery/engineering_delivery_pack.xlsx",
                xlsx_bytes,
            ),
            (
                "04_BOM_and_Delivery/bom_and_bench_outline.md",
                pack.full_markdown.into_bytes(),
            ),
            (
                "05_IP_and_Paper/journal_manuscript_draft.md",
                journal.full_markdown.into_bytes(),
            ),
            (
                "05_IP_and_Paper/main.tex",
                latex_bundle.main_tex.into_bytes(),
            ),
            (
                "05_IP_and_Paper/references.bib",
                latex_bundle.bibtex_content.into_bytes(),
            ),
            (
                "05_IP_and_Paper/figures/simulation_curve.svg",
                latex_bundle.simulation_svg.into_bytes(),
            ),
            ("05_IP_and_Paper/paper_manuscript.docx", paper_docx_bytes),
            ("05_IP_and_Paper/oa_response_kit.docx", docx_bytes),
        ];

        for (entry_path, data) in entries {
            if entry_path.contains("..") || entry_path.starts_with('/') {
                return Err(format!("非法归档路径: {entry_path}"));
            }
            zip.start_file(entry_path, opts)
                .map_err(|e| e.to_string())?;
            zip.write_all(&data).map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
    }

    Ok(buf.into_inner())
}

/// GET /api/ideas/:id/archive/zip — 一键导出全项目 5 目录工程归档包 (.zip)
pub async fn api_idea_archive_zip(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> axum::response::Response {
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;

    let idea = match s.db.get_idea(&id) {
        Ok(Some(i)) => i,
        _ => return (StatusCode::NOT_FOUND, "Idea not found").into_response(),
    };

    match build_project_archive_zip(&s.db, &idea).await {
        Ok(bytes) => {
            let safe_id: String = idea
                .id
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            let filename = format!(
                "innoforge-project-{}.zip",
                if safe_id.is_empty() {
                    "archive"
                } else {
                    &safe_id
                }
            );
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "application/zip".to_string()),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{filename}\""),
                    ),
                ],
                bytes,
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("ZIP archive error: {e}"),
        )
            .into_response(),
    }
}

fn sanitize_ascii_id(raw: &str, fallback: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned
    }
}

fn escape_paper_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn docx_heading_p(style: &str, text: &str) -> String {
    format!(
        "<w:p><w:pPr><w:pStyle w:val=\"{}\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        escape_paper_xml(style),
        escape_paper_xml(text)
    )
}

fn docx_text_p(text: &str) -> String {
    format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        escape_paper_xml(text)
    )
}

fn docx_table_xml(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut xml = String::from(
        "<w:tbl><w:tblPr><w:tblBorders>\
         <w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"CBD5E1\"/>\
         <w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"CBD5E1\"/>\
         <w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"CBD5E1\"/>\
         <w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"CBD5E1\"/>\
         <w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"E2E8F0\"/>\
         <w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"E2E8F0\"/>\
         </w:tblBorders></w:tblPr>",
    );
    xml.push_str("<w:tr>");
    for h in headers {
        xml.push_str(&format!(
            "<w:tc><w:tcPr><w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"E8EEF5\"/></w:tcPr>\
             <w:p><w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p></w:tc>",
            escape_paper_xml(h)
        ));
    }
    xml.push_str("</w:tr>");
    for row in rows {
        xml.push_str("<w:tr>");
        for cell in row {
            xml.push_str(&format!(
                "<w:tc><w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p></w:tc>",
                escape_paper_xml(cell)
            ));
        }
        xml.push_str("</w:tr>");
    }
    xml.push_str("</w:tbl>");
    xml
}

/// 按指定模板（`ctexart` | `ieee` | `elsevier`）生成期刊投稿 LaTeX 打包 ZIP
pub fn build_paper_latex_bundle_zip_with_template(
    db: &crate::db::Database,
    idea: &Idea,
    template_param: Option<&str>,
) -> Result<Vec<u8>, String> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let ctx = resolve_pipeline_context_for_deliverables(db, idea);
    let cards = db.get_feature_cards_by_idea(&idea.id).unwrap_or_default();
    let dois = crate::pipeline::steps::deliverables::extract_verified_dois(&ctx);
    let db_patents = db.get_patents_by_numbers(&dois).unwrap_or_default();
    let fallback_sim = crate::pipeline::steps::control_proto::run_closed_loop_step_simulation(
        &crate::pipeline::steps::control_proto::default_calibration_table(),
    );
    let sample = format!("{} {} {}", ctx.title, ctx.description, ctx.ai_analysis);
    let template = crate::pipeline::steps::deliverables::PaperLatexTemplate::from_query_or_content(
        template_param,
        &sample,
    );

    let bundle = crate::pipeline::steps::deliverables::build_paper_latex_bundle_with_template(
        &ctx,
        &cards,
        &db_patents,
        Some(&fallback_sim),
        template,
    );

    let readme = format!(
        "InnoForge Journal Submission LaTeX Bundle\n\
         ========================================\n\
         Project: {}\n\
         Template: {}\n\
         Verified DOI Citations: {}\n\
         Step 13 Numerical Simulation Figure: {}\n\n\
         Files:\n\
         - main.tex\n\
         - references.bib\n\
         - figures/simulation_curve.svg\n\n\
         {}\n",
        idea.title,
        bundle.template.as_str(),
        bundle.bib_entries.len(),
        if bundle.has_step13_simulation {
            "figures/simulation_curve.svg (Step 13 Experiment Validation)"
        } else {
            "figures/simulation_curve.svg (Closed-Loop Control Step Simulation)"
        },
        crate::pipeline::steps::deliverables::NOVELTY_LOCK_TEXT_PLAIN,
    );

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default();

        let entries: [(&str, &[u8]); 4] = [
            ("main.tex", bundle.main_tex.as_bytes()),
            ("references.bib", bundle.bibtex_content.as_bytes()),
            (
                "figures/simulation_curve.svg",
                bundle.simulation_svg.as_bytes(),
            ),
            ("README.txt", readme.as_bytes()),
        ];

        for (entry_path, data) in entries {
            if entry_path.contains("..") || entry_path.starts_with('/') {
                return Err(format!("非法归档路径: {entry_path}"));
            }
            zip.start_file(entry_path, opts)
                .map_err(|e| e.to_string())?;
            zip.write_all(data).map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
    }

    Ok(buf.into_inner())
}

/// 生成学术期刊投稿 Word `.docx` 字节流（置顶防破新颖性安全锁、真实 DOI 文献与仿真数据表）
pub fn build_paper_docx_bytes(db: &crate::db::Database, idea: &Idea) -> Result<Vec<u8>, String> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let ctx = resolve_pipeline_context_for_deliverables(db, idea);
    let cards = db.get_feature_cards_by_idea(&idea.id).unwrap_or_default();
    let dois = crate::pipeline::steps::deliverables::extract_verified_dois(&ctx);
    let db_patents = db.get_patents_by_numbers(&dois).unwrap_or_default();
    let bib_entries =
        crate::pipeline::steps::deliverables::extract_verified_bib_entries(&ctx, &db_patents);
    let fallback_sim = crate::pipeline::steps::control_proto::run_closed_loop_step_simulation(
        &crate::pipeline::steps::control_proto::default_calibration_table(),
    );
    let plot_data = crate::pipeline::steps::deliverables::extract_simulation_plot_data(
        &ctx,
        Some(&fallback_sim),
    );

    let title = if ctx.title.trim().is_empty() {
        "工程系统设计与实验验证手稿"
    } else {
        ctx.title.trim()
    };
    let domain = if ctx.technical_domain.trim().is_empty() {
        "通用工程与机电控制"
    } else {
        ctx.technical_domain.trim()
    };
    let abstract_body = if ctx.description.trim().is_empty() {
        "【待补充方案技术摘要】"
    } else {
        crate::ai::client::safe_truncate_chars(ctx.description.trim(), 500)
    };
    let keywords_line = if ctx.keywords.is_empty() {
        "工程设计；闭环控制；实验验证".to_string()
    } else {
        ctx.keywords.join("；")
    };

    let mut body = String::new();
    // 置顶「防破新颖性安全锁」醒目提示框
    body.push_str(&format!(
        "<w:p><w:pPr><w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"FEF3C7\"/>\
         <w:spacing w:before=\"120\" w:after=\"180\"/></w:pPr>\
         <w:r><w:rPr><w:b/><w:color w:val=\"B45309\"/></w:rPr>\
         <w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        escape_paper_xml(crate::pipeline::steps::deliverables::NOVELTY_LOCK_TEXT_PLAIN)
    ));

    body.push_str(&docx_heading_p("Title", title));
    body.push_str(&docx_text_p(&format!(
        "项目 ID：{} | 技术领域：{}",
        ctx.idea_id, domain
    )));
    body.push_str(&docx_heading_p("Heading1", "摘要 / Abstract"));
    body.push_str(&docx_text_p(abstract_body));
    body.push_str(&docx_text_p(&format!(
        "关键词 / Keywords：{}",
        keywords_line
    )));

    // 1. 引言与文献评述
    body.push_str(&docx_heading_p("Heading1", "1. 引言与文献评述"));
    body.push_str(&docx_text_p(&format!(
        "本研究针对「{}」领域的关键工程瓶颈开展系统建模与验证，共检索核验可追溯 DOI 学术文献 {} 篇。",
        domain,
        bib_entries.len()
    )));
    if bib_entries.is_empty() {
        body.push_str(&docx_text_p(
            "【当前检索结果无 DOI 文献，请补充 OpenAlex / CrossRef 检索后再绑定引用】",
        ));
    } else {
        for (idx, entry) in bib_entries.iter().enumerate() {
            body.push_str(&docx_text_p(&format!(
                "文献 [{}] {}（DOI: {}）对相关方法提供了基础支撑。",
                idx + 1,
                entry.title,
                entry.doi
            )));
        }
    }

    // 2. 系统机理与结构建模
    body.push_str(&docx_heading_p("Heading1", "2. 系统机理与结构建模"));
    if ctx.ai_analysis.trim().is_empty() {
        body.push_str(&docx_text_p(abstract_body));
    } else {
        for para in crate::ai::client::safe_truncate_chars(ctx.ai_analysis.trim(), 1200).lines() {
            let trimmed = para.trim();
            if !trimmed.is_empty() {
                body.push_str(&docx_text_p(trimmed));
            }
        }
    }
    if !cards.is_empty() {
        let rows: Vec<Vec<String>> = cards
            .iter()
            .take(8)
            .map(|c| {
                vec![
                    c.title.clone(),
                    crate::ai::client::safe_truncate_chars(c.core_structure.trim(), 100)
                        .to_string(),
                    crate::ai::client::safe_truncate_chars(c.key_relations.trim(), 100).to_string(),
                ]
            })
            .collect();
        body.push_str(&docx_table_xml(
            &["特征名称", "核心结构", "关键关系"],
            &rows,
        ));
    }

    // 3. 仿真与实验结果分析
    body.push_str(&docx_heading_p("Heading1", "3. 仿真与实验结果分析"));
    let successful_exp: Vec<&crate::pipeline::context::ExperimentResult> = ctx
        .experiment_results
        .iter()
        .filter(|e| e.success)
        .collect();
    if successful_exp.is_empty() {
        body.push_str(&docx_text_p(
            "【待台架标定】第 13 步尚未记录通过的沙箱实验，以下附带确定性闭环阶跃响应仿真参考数据。",
        ));
    } else {
        let exp_rows: Vec<Vec<String>> = successful_exp
            .iter()
            .enumerate()
            .map(|(idx, exp)| {
                vec![
                    format!("实验组 {}", idx + 1),
                    exp.language.clone(),
                    format!("{} ms", exp.duration_ms),
                    crate::ai::client::safe_truncate_chars(&exp.metrics.to_string(), 140)
                        .to_string(),
                ]
            })
            .collect();
        body.push_str(&docx_table_xml(
            &["实验组", "语言", "耗时", "真实指标"],
            &exp_rows,
        ));
    }

    if let Some(plot) = plot_data {
        body.push_str(&docx_text_p(&format!(
            "仿真曲线摘要（{}）：{}",
            plot.title, plot.summary_note
        )));
        let headers_vec: Vec<String> = std::iter::once(plot.x_label.clone())
            .chain(plot.series.iter().map(|s| s.label.clone()))
            .collect();
        let header_refs: Vec<&str> = headers_vec.iter().map(String::as_str).collect();
        let point_count = plot.x_values.len();
        let stride = (point_count / 10).max(1);
        let mut sample_rows = Vec::new();
        for idx in (0..point_count).step_by(stride).take(12) {
            let mut row = vec![format!("{:.2}", plot.x_values[idx])];
            for s in &plot.series {
                let val_str = s
                    .values
                    .get(idx)
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string());
                row.push(val_str);
            }
            sample_rows.push(row);
        }
        if !sample_rows.is_empty() {
            body.push_str(&docx_table_xml(&header_refs, &sample_rows));
        }
    }

    // 4. 结论
    body.push_str(&docx_heading_p("Heading1", "4. 结论"));
    let conclusion_body = if ctx.action_plan.trim().is_empty() {
        format!(
            "围绕「{}」完成了技术方案机理梳理与仿真验证，待专利申请受理后可推进样机测试与论文投稿。",
            title
        )
    } else {
        crate::ai::client::safe_truncate_chars(ctx.action_plan.trim(), 500).to_string()
    };
    body.push_str(&docx_text_p(&conclusion_body));

    // 5. 参考文献（仅绑定真实 DOI）
    body.push_str(&docx_heading_p("Heading1", "5. 参考文献"));
    if bib_entries.is_empty() {
        body.push_str(&docx_text_p(
            "【无已核验 DOI 文献条目；系统严禁编造参考文献】",
        ));
    } else {
        for (idx, entry) in bib_entries.iter().enumerate() {
            let mut citation = format!("[{}] ", idx + 1);
            if !entry.author.is_empty() {
                citation.push_str(&format!("{}. ", entry.author));
            }
            citation.push_str(&entry.title);
            if !entry.journal.is_empty() {
                citation.push_str(&format!(". {}", entry.journal));
            }
            if !entry.year.is_empty() {
                citation.push_str(&format!(" ({})", entry.year));
            }
            citation.push_str(&format!(". DOI: {}", entry.url));
            body.push_str(&docx_text_p(&citation));
        }
    }

    let document_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
         <w:body>{body}<w:sectPr>\
         <w:pgSz w:w=\"11906\" w:h=\"16838\"/>\
         <w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\"/>\
         </w:sectPr></w:body></w:document>"
    );

    let content_types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
        <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-officedocument.relationships+xml\"/>\
        <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
        <Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
        <Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
        </Types>";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
        <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
        </Relationships>";
    let doc_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
        <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
        </Relationships>";
    let styles_xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
        <w:style w:type=\"paragraph\" w:styleId=\"Title\">\
          <w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/>\
          <w:pPr><w:spacing w:before=\"120\" w:after=\"160\"/><w:jc w:val=\"center\"/></w:pPr>\
          <w:rPr><w:rFonts w:ascii=\"Times New Roman\" w:hAnsi=\"Times New Roman\" w:eastAsia=\"SimSun\"/><w:b/><w:sz w:val=\"32\"/></w:rPr>\
        </w:style>\
        <w:style w:type=\"paragraph\" w:styleId=\"Heading1\">\
          <w:name w:val=\"Heading 1\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/>\
          <w:pPr><w:spacing w:before=\"200\" w:after=\"80\"/></w:pPr>\
          <w:rPr><w:rFonts w:ascii=\"Times New Roman\" w:hAnsi=\"Times New Roman\" w:eastAsia=\"SimHei\"/><w:b/><w:sz w:val=\"26\"/><w:color w:val=\"1E3A8A\"/></w:rPr>\
        </w:style>\
        <w:style w:type=\"paragraph\" w:styleId=\"Normal\">\
          <w:name w:val=\"Normal\"/>\
          <w:pPr><w:spacing w:before=\"0\" w:after=\"100\" w:line=\"320\" w:lineRule=\"auto\"/></w:pPr>\
          <w:rPr><w:rFonts w:ascii=\"Times New Roman\" w:hAnsi=\"Times New Roman\" w:eastAsia=\"SimSun\"/><w:sz w:val=\"22\"/></w:rPr>\
        </w:style>\
        </w:styles>";

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default();
        for (path, content) in [
            ("[Content_Types].xml", content_types.as_bytes()),
            ("_rels/.rels", rels.as_bytes()),
            ("word/_rels/document.xml.rels", doc_rels.as_bytes()),
            ("word/styles.xml", styles_xml.as_bytes()),
            ("word/document.xml", document_xml.as_bytes()),
        ] {
            zip.start_file(path, opts).map_err(|e| e.to_string())?;
            zip.write_all(content).map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
    }

    Ok(buf.into_inner())
}

#[derive(Debug, Deserialize, Default)]
pub struct LatexBundleQuery {
    #[serde(default)]
    pub template: Option<String>,
}

/// GET /api/ideas/:id/paper/latex-bundle — 导出期刊投稿 LaTeX 打包文件 (.zip: main.tex + references.bib + SVG 仿真图)
pub async fn api_idea_paper_latex_bundle(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<LatexBundleQuery>,
) -> axum::response::Response {
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;

    let idea = match s.db.get_idea(&id) {
        Ok(Some(i)) => i,
        _ => return (StatusCode::NOT_FOUND, "Idea not found").into_response(),
    };

    match build_paper_latex_bundle_zip_with_template(&s.db, &idea, query.template.as_deref()) {
        Ok(bytes) => {
            let safe_id = sanitize_ascii_id(&idea.id, "paper");
            let filename = format!("paper-latex-{safe_id}.zip");
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "application/zip".to_string()),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{filename}\""),
                    ),
                ],
                bytes,
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("LaTeX bundle build error: {e}"),
        )
            .into_response(),
    }
}

/// GET /api/ideas/:id/paper/docx — 导出学术期刊论文 Word 排版件 (.docx)
pub async fn api_idea_paper_docx(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> axum::response::Response {
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;

    let idea = match s.db.get_idea(&id) {
        Ok(Some(i)) => i,
        _ => return (StatusCode::NOT_FOUND, "Idea not found").into_response(),
    };

    match build_paper_docx_bytes(&s.db, &idea) {
        Ok(bytes) => {
            let safe_id = sanitize_ascii_id(&idea.id, "paper");
            let filename = format!("paper-manuscript-{safe_id}.docx");
            (
                StatusCode::OK,
                [
                    (
                        header::CONTENT_TYPE,
                        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                            .to_string(),
                    ),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{filename}\""),
                    ),
                ],
                bytes,
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Paper DOCX build error: {e}"),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod deliverables_report_tests {
    use super::{build_deliverable_report_json, Idea};
    use crate::db::memory::IdeaMemory;
    use crate::db::Database;

    fn seed_idea(db: &Database, id: &str) -> Idea {
        let now = "2026-11-18T00:00:00Z".to_string();
        let idea = Idea {
            id: id.to_string(),
            title: "磁流变阻尼器".to_string(),
            description: "面向精密隔振平台的磁流变阻尼控制装置".to_string(),
            input_type: "text".to_string(),
            status: "done".to_string(),
            analysis: "采用多级励磁线圈结构降低响应延时。".to_string(),
            web_results: r#"[{"id":"10.1016/j.ymssp.2024.111222","title":"MR damper","snippet":"OpenAlex","link":"https://doi.org/10.1016/j.ymssp.2024.111222","source":"openalex"}]"#.to_string(),
            patent_results: "[]".to_string(),
            novelty_score: Some(82.0),
            created_at: now.clone(),
            updated_at: now,
            discussion_summary: String::new(),
        };
        db.insert_idea(&idea).expect("insert idea");
        idea
    }

    #[test]
    fn bom_pack_and_journal_draft_prefer_persisted_and_fallback_on_demand() {
        let db = Database::init(":memory:").expect("in-memory db");
        let idea = seed_idea(&db, "idea-w3-1");

        let on_demand_bom = build_deliverable_report_json(&db, &idea, "bom_pack");
        assert_eq!(on_demand_bom["status"], "ok");
        assert_eq!(on_demand_bom["type"], "bom_pack");
        assert_eq!(on_demand_bom["source"], "generated_on_demand");
        assert!(on_demand_bom["report"]
            .as_str()
            .unwrap_or("")
            .contains("机械与电控分类 BOM 清单"));

        let on_demand_journal = build_deliverable_report_json(&db, &idea, "journal_draft");
        assert_eq!(on_demand_journal["status"], "ok");
        assert_eq!(on_demand_journal["type"], "journal_draft");
        assert_eq!(on_demand_journal["source"], "generated_on_demand");
        assert!(on_demand_journal["novelty_lock_warning"]
            .as_str()
            .unwrap_or("")
            .contains("防破新颖性安全锁"));
        assert_eq!(
            on_demand_journal["cited_dois"],
            serde_json::json!(["10.1016/j.ymssp.2024.111222"])
        );

        let now = "2026-11-18T00:00:00Z".to_string();
        db.save_memory(&IdeaMemory {
            id: "mem-bom-1".to_string(),
            idea_id: "idea-w3-1".to_string(),
            concept_name: "deliverable_bom".to_string(),
            concept_type: "decision".to_string(),
            content: "# 已落库 BOM".to_string(),
            confidence: 0.9,
            source_step: Some("FinalizeDeliverables".to_string()),
            created_at: now.clone(),
            updated_at: now,
        })
        .expect("save persisted bom");

        let persisted_bom = build_deliverable_report_json(&db, &idea, "bom_pack");
        assert_eq!(persisted_bom["source"], "persisted_memory");
        assert_eq!(persisted_bom["report"], "# 已落库 BOM");
    }

    #[test]
    fn delivery_pack_xlsx_has_valid_zip_and_three_sheets() {
        let db = Database::init(":memory:").expect("open db");
        let idea = seed_idea(&db, "idea-xlsx-1");
        let bytes = super::build_delivery_pack_xlsx(&db, &idea).expect("build xlsx");
        assert!(bytes.len() > 500, "xlsx bytes should be non-empty");
        assert_eq!(&bytes[0..2], b"PK", "xlsx must be a valid ZIP archive");

        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("open zip");
        for sheet in [
            "xl/worksheets/sheet1.xml",
            "xl/worksheets/sheet2.xml",
            "xl/worksheets/sheet3.xml",
        ] {
            let mut f = archive.by_name(sheet).expect("sheet must exist");
            let mut xml = String::new();
            std::io::Read::read_to_string(&mut f, &mut xml).expect("read sheet xml");
            assert!(!xml.is_empty(), "{sheet} should not be empty");
        }
    }

    #[tokio::test]
    async fn project_archive_zip_contains_five_standard_directories() {
        let db = Database::init(":memory:").expect("open db");
        let idea = seed_idea(&db, "idea-zip-1");
        let bytes = super::build_project_archive_zip(&db, &idea)
            .await
            .expect("build zip");
        assert_eq!(&bytes[0..2], b"PK", "archive must be a valid ZIP");

        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("open zip");
        let mut names = Vec::new();
        for i in 0..archive.len() {
            let f = archive.by_index(i).expect("entry");
            let n = f.name().to_string();
            assert!(!n.contains(".."), "no path traversal allowed: {n}");
            names.push(n);
        }
        for prefix in [
            "01_Research_and_FTO/",
            "02_CAD_and_Mechanics/",
            "03_Control_and_Simulation/",
            "04_BOM_and_Delivery/",
            "05_IP_and_Paper/",
        ] {
            assert!(
                names.iter().any(|n| n.starts_with(prefix)),
                "missing directory {prefix} in {names:?}"
            );
        }
        for required_paper_file in [
            "05_IP_and_Paper/main.tex",
            "05_IP_and_Paper/references.bib",
            "05_IP_and_Paper/figures/simulation_curve.svg",
            "05_IP_and_Paper/paper_manuscript.docx",
        ] {
            assert!(
                names.iter().any(|n| n == required_paper_file),
                "missing paper deliverable {required_paper_file} in {names:?}"
            );
        }

        let mut tex = String::new();
        std::io::Read::read_to_string(
            &mut archive
                .by_name("05_IP_and_Paper/main.tex")
                .expect("main.tex"),
            &mut tex,
        )
        .expect("read main.tex");
        assert!(tex.contains(r"\documentclass[UTF8]{ctexart}"));
        assert!(tex.contains("10.1016/j.ymssp.2024.111222"));

        let mut docx_buf = Vec::new();
        std::io::Read::read_to_end(
            &mut archive
                .by_name("05_IP_and_Paper/paper_manuscript.docx")
                .expect("paper_manuscript.docx"),
            &mut docx_buf,
        )
        .expect("read paper_manuscript.docx");
        assert!(
            docx_buf.starts_with(b"PK"),
            "embedded paper docx must be a valid ZIP"
        );
    }

    #[test]
    fn paper_latex_bundle_zip_contains_valid_tex_bib_svg_and_zero_orphan_cites() {
        let db = Database::init(":memory:").expect("open db");
        let idea = seed_idea(&db, "idea-latex-1");

        let mut ctx = crate::pipeline::context::PipelineContext::new(
            &idea.id,
            &idea.title,
            &idea.description,
        );
        ctx.web_results = serde_json::from_str(&idea.web_results).expect("parse web_results");
        ctx.experiment_results
            .push(crate::pipeline::context::ExperimentResult {
                script_path: "sim_step13.py".to_string(),
                language: "python".to_string(),
                exit_code: 0,
                stdout: "EXPERIMENT_DONE".to_string(),
                stderr: String::new(),
                metrics: serde_json::json!({
                    "time_ms": [0.0, 25.0, 50.0, 75.0, 100.0],
                    "setpoint": [50.0, 50.0, 50.0, 50.0, 50.0],
                    "actual_value": [0.0, 21.0, 42.5, 49.0, 50.1]
                }),
                duration_ms: 35,
                success: true,
            });
        let ctx_json = serde_json::to_string(&ctx).expect("serialize ctx");
        db.save_pipeline_snapshot(&idea.id, "Finalize", &ctx_json)
            .expect("save snapshot");

        let bytes = super::build_paper_latex_bundle_zip_with_template(&db, &idea, None)
            .expect("build latex zip");
        assert!(bytes.starts_with(b"PK"), "latex bundle must be a valid ZIP");

        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("open latex zip");

        let mut tex = String::new();
        std::io::Read::read_to_string(
            &mut archive.by_name("main.tex").expect("main.tex must exist"),
            &mut tex,
        )
        .expect("read main.tex");

        let mut bib = String::new();
        std::io::Read::read_to_string(
            &mut archive
                .by_name("references.bib")
                .expect("references.bib must exist"),
            &mut bib,
        )
        .expect("read references.bib");

        let mut svg = String::new();
        std::io::Read::read_to_string(
            &mut archive
                .by_name("figures/simulation_curve.svg")
                .expect("figures/simulation_curve.svg must exist"),
            &mut svg,
        )
        .expect("read svg");

        assert!(tex.contains("专利法》第22条第2款新颖性"));
        assert!(tex.contains("figures/simulation_curve.svg"));
        assert!(bib.contains("10.1016/j.ymssp.2024.111222"));

        let tex_cites = crate::pipeline::steps::deliverables::extract_tex_cite_keys(&tex);
        let bib_keys = crate::pipeline::steps::deliverables::extract_bib_keys(&bib);
        assert!(!tex_cites.is_empty(), "must cite verified DOI entry");
        for key in &tex_cites {
            assert!(
                bib_keys.contains(key),
                "orphan citation {key} not found in references.bib ({bib_keys:?})"
            );
        }

        assert!(svg.contains("<svg"));
        assert!(svg.contains("Time (ms)"));
        assert!(svg.contains("class=\"legend\""));
        assert!(svg.contains("<polyline"));

        // 显式选择 ?template=ieee 应生成 IEEEtran 文档类
        let ieee_bytes =
            super::build_paper_latex_bundle_zip_with_template(&db, &idea, Some("ieee"))
                .expect("build ieee latex zip");
        let mut ieee_archive =
            zip::ZipArchive::new(std::io::Cursor::new(ieee_bytes)).expect("open ieee zip");
        let mut ieee_tex = String::new();
        std::io::Read::read_to_string(
            &mut ieee_archive.by_name("main.tex").expect("ieee main.tex"),
            &mut ieee_tex,
        )
        .expect("read ieee main.tex");
        assert!(ieee_tex.contains(r"\documentclass[journal]{IEEEtran}"));
    }

    #[test]
    fn paper_docx_zip_structure_is_valid_and_preserves_novelty_lock() {
        let db = Database::init(":memory:").expect("open db");
        let idea = seed_idea(&db, "idea-paper-docx-1");
        let bytes = super::build_paper_docx_bytes(&db, &idea).expect("build paper docx");
        assert!(bytes.starts_with(b"PK"), "docx must be a valid ZIP archive");

        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("open docx zip");
        for required in [
            "[Content_Types].xml",
            "_rels/.rels",
            "word/_rels/document.xml.rels",
            "word/styles.xml",
            "word/document.xml",
        ] {
            let mut f = archive.by_name(required).expect("required OOXML part");
            let mut xml = String::new();
            std::io::Read::read_to_string(&mut f, &mut xml).expect("read OOXML part");
            assert!(!xml.is_empty(), "{required} should not be empty");
        }

        let mut doc_xml = String::new();
        std::io::Read::read_to_string(
            &mut archive.by_name("word/document.xml").expect("document.xml"),
            &mut doc_xml,
        )
        .expect("read document.xml");
        assert!(doc_xml.contains("专利法》第22条第2款新颖性"));
        assert!(doc_xml.contains("10.1016/j.ymssp.2024.111222"));
    }
}
