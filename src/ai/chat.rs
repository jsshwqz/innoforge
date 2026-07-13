//! 聊天接口 / Chat interface methods

use super::client::{safe_truncate, timeouts, AiClient, AiProviderMode, Message};
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
        // OA 讨论和分析使用 ANALYSIS HTTP 超时（180s）；异步任务由全局 300 秒守卫包裹。
        let client = reqwest::Client::builder()
            .timeout(timeouts::ANALYSIS)
            .no_proxy()
            .build()
            .unwrap_or_else(|_| self.client.clone());
        let is_anthropic = provider.base_url.contains("anthropic");

        tokio::spawn(async move {
            let _ = tokio::time::timeout(
                Duration::from_secs(AiClient::GLOBAL_TIMEOUT_SECS),
                async {
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
                            "max_tokens": 16384,
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
                        loop {
                            match resp.chunk().await {
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
                                                        if let Some(text) =
                                                            val["delta"]["text"].as_str()
                                                        {
                                                            if !text.is_empty() {
                                                                // SSE safety: sanitize \n/\r to prevent protocol breakage
                                                                let sanitized = text.replace(['\r', '\n'], " ");
                                                                if tx.send(sanitized).await.is_err() {
                                                                    return;
                                                                }
                                                            }
                                                        }
                                                    }
                                                    Some("message_stop") => return,
                                                    Some("error") => {
                                                        let msg = val["error"]["message"]
                                                            .as_str()
                                                            .unwrap_or("unknown");
                                                        let _ = tx
                                                            .send(format!(
                                                                "[ERROR] Anthropic: {}",
                                                                msg
                                                            ))
                                                            .await;
                                                        return;
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                }
                                Ok(None) => break,
                                Err(_) => break,
                            }
                        }
                    } else {
                        // ── OpenAI-compatible streaming (DeepSeek, OpenAI, etc.) ──
                        let request_body = serde_json::json!({
                            "model": provider.model,
                            "messages": messages.iter().map(|m| serde_json::json!({
                                "role": m.role,
                                "content": m.content
                            })).collect::<Vec<_>>(),
                            "temperature": temperature,
                            "max_tokens": 16384,
                            "stream": true,
                        });

                        let body_json = serde_json::to_string(&request_body).unwrap_or_default();
                        tracing::info!(
                            "[AI STREAM REQUEST] provider={} model={} body_len={}B body_preview={}",
                            provider.name, provider.model, body_json.len(),
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

                        loop {
                            match resp.chunk().await {
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
                                            return;
                                        }
                                        if let Some(json_str) = line.strip_prefix("data: ") {
                                            if let Ok(val) =
                                                serde_json::from_str::<serde_json::Value>(json_str)
                                            {
                                                let delta = &val["choices"][0]["delta"];

                                                // DeepSeek v4-flash: 可见文本在 reasoning_content 字段
                                                // 优先 content，回退 reasoning_content
                                                let content = delta["content"]
                                                    .as_str()
                                                    .or_else(|| {
                                                        delta["reasoning_content"].as_str()
                                                    });

                                                if let Some(content) = content {
                                                    if !content.is_empty() {
                                                        // SSE safety: sanitize \n/\r to prevent protocol breakage
                                                        let sanitized = content.replace(['\r', '\n'], " ");
                                                        if tx.send(sanitized).await.is_err() {
                                                            return;
                                                        }
                                                    }
                                                } else if let Some(delta_obj) = delta.as_object()
                                                {
                                                    // 兜底：content 和 reasoning_content 都为空
                                                    // 但 delta 有其他字段 → 尝试捕获任何非空字符串
                                                    // Fallback: catch any non-empty string field
                                                    // (future-proof against new Provider formats)
                                                    let fallback = delta_obj.iter().find_map(
                                                        |(k, v)| {
                                                            v.as_str()
                                                                .filter(|s| !s.is_empty())
                                                                .map(|s| (k, s))
                                                        },
                                                    );
                                                    if let Some((field_name, text)) = fallback {
                                                        tracing::warn!(
                                                            "[AI STREAM] content & reasoning_content both empty, \
                                                             captured '{}' field with {} chars",
                                                            field_name,
                                                            text.len()
                                                        );
                                                        // SSE safety: sanitize \n/\r to prevent protocol breakage
                                                        let sanitized = text.replace(['\n', '\r'], " ");
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
                                    tracing::info!(
                                        "[AI STREAM] stream complete, total chars: {}",
                                        total_chars
                                    );
                                    break;
                                }
                                Err(_) => break,
                            }
                        }
                    } // end OpenAI-compatible branch
                },
            )
            .await;
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
