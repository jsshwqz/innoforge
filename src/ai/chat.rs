//! 聊天接口 / Chat interface methods

use super::client::{safe_truncate, AiClient, AiProviderMode, Message};
use anyhow::Result;
use std::time::Duration;

impl AiClient {
    /// 专家模式：用于创新推演、专利深分析等高推理任务。
    /// 使用 AiClient 创建时配置的模型（ai_client_expert → ai_model_expert，ai_client → ai_model）。
    pub async fn chat_expert(&self, user_msg: &str, context: Option<&str>) -> Result<String> {
        let mut messages = vec![Message {
            role: "system".into(),
            content: "你是资深专利与研发战略专家。请使用\u{201C}结论\u{2192}证据\u{2192}反证\u{2192}风险\u{2192}下一步\u{201D}的结构回答，\
                      并明确给出置信度（高/中/低）与依据。"
                .into(),
        }];

        if let Some(ctx) = context {
            messages.push(Message {
                role: "system".into(),
                content: format!("以下是相关专利信息供参考：\n{ctx}"),
            });
        }

        messages.push(Message {
            role: "user".into(),
            content: user_msg.to_string(),
        });

        self.send_chat_expert(messages, 0.5).await
    }

    /// 带完整消息历史的聊天（用于多轮讨论保持上下文）
    pub async fn chat_with_history(
        &self,
        system_prompt: &str,
        history: Vec<(String, String)>,
        temperature: f32,
    ) -> Result<String> {
        let mut messages = vec![Message {
            role: "system".into(),
            content: system_prompt.to_string(),
        }];
        for (role, content) in history {
            messages.push(Message { role, content });
        }
        self.send_chat(messages, temperature).await
    }

    pub async fn chat_with_history_expert(
        &self,
        system_prompt: &str,
        history: Vec<(String, String)>,
        temperature: f32,
    ) -> Result<String> {
        let mut messages = vec![Message {
            role: "system".into(),
            content: system_prompt.to_string(),
        }];
        for (role, content) in history {
            messages.push(Message { role, content });
        }
        self.send_chat_expert(messages, temperature).await
    }

    /// 自定义 system prompt + temperature 的聊天（用于多维推演引擎）
    pub async fn chat_with_system(
        &self,
        system_prompt: &str,
        user_msg: &str,
        temperature: f32,
    ) -> Result<String> {
        let messages = vec![
            Message {
                role: "system".into(),
                content: system_prompt.to_string(),
            },
            Message {
                role: "user".into(),
                content: user_msg.to_string(),
            },
        ];
        self.send_chat_expert(messages, temperature).await
    }

    /// 使用指定模型发送 system+user 消息（用于结构化输出任务）。
    pub async fn chat_with_system_model(
        &self,
        system_prompt: &str,
        user_msg: &str,
        temperature: f32,
        model_override: &str,
    ) -> Result<String> {
        let messages = vec![
            Message {
                role: "system".into(),
                content: system_prompt.to_string(),
            },
            Message {
                role: "user".into(),
                content: user_msg.to_string(),
            },
        ];
        self.send_chat_expert_with_model(messages, temperature, Some(model_override))
            .await
    }

    pub async fn chat(&self, user_msg: &str, context: Option<&str>) -> Result<String> {
        let mut messages = vec![Message {
            role: "system".into(),
            content: "你是一个专利分析助手。你擅长分析专利文献、解读权利要求、评估专利价值、\
                         进行技术趋势分析。请用中文回答，专业术语可以保留英文。"
                .into(),
        }];

        if let Some(ctx) = context {
            messages.push(Message {
                role: "system".into(),
                content: format!("以下是相关专利信息供参考：\n{ctx}"),
            });
        }

        messages.push(Message {
            role: "user".into(),
            content: user_msg.to_string(),
        });

        self.send_chat(messages, 0.7).await
    }

    /// 通用流式消息发送：返回 SSE chunk 接收端 / Generic streaming: send messages, get chunk Receiver
    ///
    /// OA 讨论等大上下文场景使用 ANALYSIS 超时（180s），全局守卫 300 秒。
    /// Uses the 180-second ANALYSIS timeout and the global 300-second guard.
    pub fn send_chat_stream(
        &self,
        messages: Vec<Message>,
        temperature: f32,
    ) -> tokio::sync::mpsc::Receiver<String> {
        let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);

        if self.provider_mode == AiProviderMode::GeminiCli {
            let _ = tx.try_send(
                "[ERROR] Gemini CLI 模式暂不支持流式输出，请使用普通聊天模式。".to_string(),
            );
            return rx;
        }

        let provider = self.primary.clone();
        // M2-A（基线失败 #1）：不再给流式请求挂 180s 总超时——thinking 模型首 token
        // 可能 >180s，总超时会把"慢思考"当"挂了"掐死且静默无错。改为：
        //   - 传输层总兜底 = 全局超时（global_timeout_secs，env 可覆盖）；
        //   - 首 token 用 stream_first_token_timeout_secs（默认 300s，env 可覆盖）；
        //   - 任何异常终止（首 token 超时/中段错误/零内容结束）必须发 [ERROR]。
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(AiClient::global_timeout_secs()))
            .no_proxy()
            .build()
            .unwrap_or_else(|_| self.client.clone());
        let is_anthropic = provider.base_url.contains("anthropic");

        tokio::spawn(async move {
            // M2 审查 P1-5/P0-2：总 deadline 只保留一个——reqwest client 的
            // .timeout(global_timeout_secs())（上面构建时已设）。外层 tokio::time::timeout
            // 包装删除：其结果此前被 `let _ =` 丢弃，全局超时会静默终止且部分内容
            // 被当作完整保存（P0-2）。现在总超时表现为 chunk() Err → [ERROR] 流中断。
            if is_anthropic {
                // ── Anthropic Messages API streaming ──
                let mut system_prompt = String::new();
                let mut chat_messages: Vec<serde_json::Value> = Vec::new();
                for msg in &messages {
                    if msg.role == "system" {
                        if !system_prompt.is_empty() {
                            system_prompt.push('\n');
                        }
                        system_prompt.push_str(&msg.content);
                    } else {
                        chat_messages.push(serde_json::json!({
                            "role": msg.role,
                            "content": msg.content
                        }));
                    }
                }
                let mut body = serde_json::json!({
                    "model": provider.model,
                    "max_tokens": 32768,
                    "messages": chat_messages,
                    "temperature": temperature,
                    "stream": true,
                });
                if !system_prompt.is_empty() {
                    body["system"] = serde_json::json!(system_prompt);
                }

                let mut resp = match client
                    .post(format!("{}/messages", provider.base_url))
                    .header("x-api-key", &provider.api_key)
                    .header("anthropic-version", "2023-06-01")
                    .json(&body)
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = tx.send(format!("[ERROR] {}", e)).await;
                        return;
                    }
                };
                if !resp.status().is_success() {
                    let status = resp.status();
                    let b = resp.text().await.unwrap_or_default();
                    let _ = tx
                        .send(format!(
                            "[ERROR] Anthropic HTTP {}: {}",
                            status,
                            safe_truncate(&b, 200)
                        ))
                        .await;
                    return;
                }

                let mut buf = String::new();
                // M2-A：首内容截止期 + 异常终止可见 + 零内容显式报错。
                // 截止期从流开始计时到「首个可见 content」——空 role 帧等无内容
                // 帧不能重置/满足它（M2 独立审计：不能被空帧骗过）。
                let ft_secs = crate::ai::client::stream_first_token_timeout_secs();
                let ft_deadline = tokio::time::Instant::now() + Duration::from_secs(ft_secs);
                let mut sent_any = false;
                loop {
                    let next = if sent_any {
                        resp.chunk().await
                    } else {
                        match tokio::time::timeout_at(ft_deadline, resp.chunk()).await {
                            Ok(c) => c,
                            Err(_) => {
                                let _ = tx
                                            .send(format!(
                                                "[ERROR] 首个内容超时（{}s）——模型长时间无可见输出；可调 INNOFORGE_AI_FIRST_TOKEN_TIMEOUT_SECS",
                                                ft_secs
                                            ))
                                            .await;
                                return;
                            }
                        }
                    };
                    match next {
                        Ok(Some(chunk)) => {
                            buf.push_str(&String::from_utf8_lossy(&chunk));
                            while let Some(pos) = buf.find('\n') {
                                let line = buf[..pos].trim().to_string();
                                buf = buf[pos + 1..].to_string();
                                if let Some(json_str) = line.strip_prefix("data: ") {
                                    if let Ok(val) =
                                        serde_json::from_str::<serde_json::Value>(json_str)
                                    {
                                        match val["type"].as_str() {
                                            Some("content_block_delta") => {
                                                if let Some(text) = val["delta"]["text"].as_str() {
                                                    if !text.is_empty() {
                                                        // SSE safety: sanitize \n/\r to prevent protocol breakage
                                                        let sanitized =
                                                            text.replace(['\r', '\n'], " ");
                                                        sent_any = true;
                                                        if tx.send(sanitized).await.is_err() {
                                                            return;
                                                        }
                                                    }
                                                }
                                            }
                                            Some("message_stop") => {
                                                if !sent_any {
                                                    let _ = tx
                                                                .send("[ERROR] 流结束但未输出任何内容（message_stop 前零内容）".to_string())
                                                                .await;
                                                }
                                                return;
                                            }
                                            Some("error") => {
                                                let msg = val["error"]["message"]
                                                    .as_str()
                                                    .unwrap_or("unknown");
                                                let _ = tx
                                                    .send(format!("[ERROR] Anthropic: {}", msg))
                                                    .await;
                                                return;
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                        Ok(None) => {
                            // M2 审查 P1-6：flush 残余 buf（末行无换行不丢尾字）
                            let rest = buf.trim().to_string();
                            if let Some(json_str) = rest.strip_prefix("data: ") {
                                if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str)
                                {
                                    if let Some(text) =
                                        val["delta"]["text"].as_str().filter(|t| !t.is_empty())
                                    {
                                        let sanitized = text.replace(['\r', '\n'], " ");
                                        sent_any = true;
                                        let _ = tx.send(sanitized).await;
                                    }
                                }
                            }
                            // M2 审查 P0-3：已有内容视为完成（网关可不发
                            // message_stop）；零内容才报错
                            if sent_any {
                                tracing::warn!(
                                            "[AI STREAM] anthropic stream closed without message_stop, treating as complete"
                                        );
                                return;
                            }
                            let _ = tx.send("[ERROR] 流结束但未输出任何内容".to_string()).await;
                            return;
                        }
                        Err(e) => {
                            let _ = tx.send(format!("[ERROR] 流中断: {}", e)).await;
                            return;
                        }
                    }
                }
                // 注：本分支所有出口均 return——message_stop（零内容时已报错）、
                // Ok(None)（有内容视为完成 / 零内容已报错）、Err（已报错）、首内容超时。
            } else {
                // ── OpenAI-compatible streaming (DeepSeek, OpenAI, etc.) ──
                let request_body = serde_json::json!({
                    "model": provider.model,
                    "messages": messages.iter().map(|m| serde_json::json!({
                        "role": m.role,
                        "content": m.content
                    })).collect::<Vec<_>>(),
                    "temperature": temperature,
                    "max_tokens": 32768,
                    "stream": true,
                });

                let body_json = serde_json::to_string(&request_body).unwrap_or_default();
                tracing::info!(
                    "[AI STREAM REQUEST] provider={} model={} body_len={}B body_preview={}",
                    provider.name,
                    provider.model,
                    body_json.len(),
                    safe_truncate(&body_json, 200)
                );

                let mut resp = match client
                    .post(format!("{}/chat/completions", provider.base_url))
                    .header("Authorization", format!("Bearer {}", provider.api_key))
                    .json(&request_body)
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = tx.send(format!("[ERROR] {}", e)).await;
                        return;
                    }
                };

                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    let _ = tx
                        .send(format!(
                            "[ERROR] HTTP {} — {}",
                            status,
                            safe_truncate(&body, 200)
                        ))
                        .await;
                    return;
                }

                let mut buf = String::new();
                let mut total_chars: usize = 0;
                // M2-A：首内容截止期（空 role 帧不能骗过）+ 异常终止可见 + 零内容显式报错
                let ft_secs = crate::ai::client::stream_first_token_timeout_secs();
                let ft_deadline = tokio::time::Instant::now() + Duration::from_secs(ft_secs);
                let mut sent_any = false;
                // （[DONE] 命中即 return；Ok(None) 必然无 [DONE] = 截断）

                loop {
                    let next = if sent_any {
                        resp.chunk().await
                    } else {
                        match tokio::time::timeout_at(ft_deadline, resp.chunk()).await {
                            Ok(c) => c,
                            Err(_) => {
                                let _ = tx
                                            .send(format!(
                                                "[ERROR] 首个内容超时（{}s）——模型长时间无可见输出；可调 INNOFORGE_AI_FIRST_TOKEN_TIMEOUT_SECS",
                                                ft_secs
                                            ))
                                            .await;
                                return;
                            }
                        }
                    };
                    match next {
                        Ok(Some(chunk)) => {
                            let chunk_str = String::from_utf8_lossy(&chunk);
                            if total_chars == 0 && chunk_str.trim().len() > 10 {
                                tracing::info!(
                                    "[AI STREAM] first chunk raw preview: {:?} | decoded: {:?}",
                                    &chunk[..chunk.len().min(60)],
                                    &chunk_str[..chunk_str.trim().len().min(80)]
                                );
                            }
                            if total_chars == 0 && chunk_str.trim().len() <= 10 {
                                tracing::warn!(
                                            "[AI STREAM] first chunk suspiciously short: raw={:?} decoded={:?}",
                                            &chunk[..chunk.len().min(40)],
                                            &chunk_str[..chunk_str.trim().len().min(40)]
                                        );
                            }
                            total_chars += chunk_str.trim().len();
                            buf.push_str(&chunk_str);

                            while let Some(pos) = buf.find('\n') {
                                let line = buf[..pos].trim().to_string();
                                buf = buf[pos + 1..].to_string();

                                if line == "data: [DONE]" {
                                    if !sent_any {
                                        let _ = tx
                                            .send(
                                                "[ERROR] 流结束但未输出任何内容（[DONE] 前零内容）"
                                                    .to_string(),
                                            )
                                            .await;
                                    }
                                    return;
                                }
                                if let Some(json_str) = line.strip_prefix("data: ") {
                                    if let Ok(val) =
                                        serde_json::from_str::<serde_json::Value>(json_str)
                                    {
                                        let delta = &val["choices"][0]["delta"];

                                        // glm-5.2 等推理模型：reasoning_content 是推理过程，content 是最终回复
                                        // 只取 content，不回退 reasoning_content（避免把推理过程当回复）
                                        let content =
                                            delta["content"].as_str().filter(|c| !c.is_empty());

                                        if let Some(content) = content {
                                            if !content.is_empty() {
                                                // SSE safety: sanitize \n/\r to prevent protocol breakage
                                                let sanitized = content.replace(['\r', '\n'], " ");
                                                sent_any = true;
                                                if tx.send(sanitized).await.is_err() {
                                                    return;
                                                }
                                            }
                                        } else if let Some(delta_obj) = delta.as_object() {
                                            // 兜底：content 和 reasoning_content 都为空
                                            // 但 delta 有其他字段 → 尝试捕获任何非空字符串
                                            // Fallback: catch any non-empty string field
                                            // (future-proof against new Provider formats)
                                            let fallback = delta_obj.iter().find_map(|(k, v)| {
                                                // Skip metadata fields and reasoning_content (推理过程不是最终回复)
                                                if k == "role"
                                                    || k == "finish_reason"
                                                    || k == "index"
                                                    || k == "reasoning_content"
                                                {
                                                    return None;
                                                }
                                                v.as_str().filter(|s| !s.is_empty()).map(|s| (k, s))
                                            });
                                            if let Some((field_name, text)) = fallback {
                                                tracing::warn!(
                                                            "[AI STREAM] content & reasoning_content both empty, \
                                                             captured '{}' field with {} chars",
                                                            field_name,
                                                            text.len()
                                                        );
                                                // SSE safety: sanitize \n/\r to prevent protocol breakage
                                                let sanitized = text.replace(['\n', '\r'], " ");
                                                sent_any = true;
                                                if tx.send(sanitized).await.is_err() {
                                                    return;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Ok(None) => {
                            // M2 审查 P1-6：flush 残余 buf（末行无换行不丢尾字）
                            let rest = buf.trim().to_string();
                            if rest == "data: [DONE]" {
                                if !sent_any {
                                    let _ =
                                        tx.send("[ERROR] 流结束但未输出任何内容".to_string()).await;
                                    return;
                                }
                                tracing::warn!(
                                            "[AI STREAM] stream closed after [DONE] without trailing newline"
                                        );
                                return;
                            }
                            if let Some(json_str) = rest.strip_prefix("data: ") {
                                if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str)
                                {
                                    if let Some(content) = val["choices"][0]["delta"]["content"]
                                        .as_str()
                                        .filter(|c| !c.is_empty())
                                    {
                                        let sanitized = content.replace(['\r', '\n'], " ");
                                        sent_any = true;
                                        let _ = tx.send(sanitized).await;
                                    }
                                }
                            }
                            // M2 审查 P0-3：网关可能不发 [DONE]——已有内容视为
                            // 完成，不报截断；零内容才报错
                            if !sent_any {
                                let _ = tx.send("[ERROR] 流结束但未输出任何内容".to_string()).await;
                                return;
                            }
                            tracing::warn!(
                                        "[AI STREAM] stream closed without [DONE], treating as complete (chars={})",
                                        total_chars
                                    );
                            return;
                        }
                        Err(e) => {
                            let _ = tx.send(format!("[ERROR] 流中断: {}", e)).await;
                            return;
                        }
                    }
                }
                // 注：本分支所有出口均 return——[DONE]（零内容时已报错）、
                // Ok(None)（有内容视为完成 / 零内容已报错）、Err（已报错）、首内容超时。
            } // end OpenAI-compatible branch
        });

        rx
    }

    /// 流式聊天：返回 SSE chunk 接收端
    pub fn chat_stream(
        &self,
        user_msg: &str,
        context: Option<&str>,
    ) -> tokio::sync::mpsc::Receiver<String> {
        let mut messages = vec![Message {
            role: "system".into(),
            content: "你是一个专利分析助手。你擅长分析专利文献、解读权利要求、评估专利价值、\
                         进行技术趋势分析。请用中文回答，专业术语可以保留英文。"
                .into(),
        }];
        if let Some(ctx) = context {
            messages.push(Message {
                role: "system".into(),
                content: format!("以下是相关专利信息供参考：\n{ctx}"),
            });
        }
        messages.push(Message {
            role: "user".into(),
            content: user_msg.to_string(),
        });

        self.send_chat_stream(messages, 0.7)
    }
}
