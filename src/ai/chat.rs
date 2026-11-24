//! 聊天接口 / Chat interface methods

use super::client::{
    build_ai_http_client, normalize_model_for_provider, safe_truncate, AiClient, AiProvider,
    AiProviderMode, Message,
};
use anyhow::Result;
use std::time::Duration;

enum StreamAttempt {
    Completed,
    FailedBeforeOutput(String),
    FailedMidStream(String),
}

async fn stream_single_provider(
    client: &reqwest::Client,
    provider: &AiProvider,
    messages: &[Message],
    temperature: f32,
    tx: &tokio::sync::mpsc::Sender<String>,
) -> StreamAttempt {
    let model = normalize_model_for_provider(&provider.base_url, &provider.model);
    let is_anthropic = provider.base_url.contains("anthropic");
    let ft_secs = crate::ai::client::stream_first_token_timeout_secs();

    if is_anthropic {
        let mut system_prompt = String::new();
        let mut chat_messages: Vec<serde_json::Value> = Vec::new();
        for msg in messages {
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
            "model": model,
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
            Err(e) => return StreamAttempt::FailedBeforeOutput(format!("{}", e)),
        };
        if !resp.status().is_success() {
            let status = resp.status();
            let b = resp.text().await.unwrap_or_default();
            return StreamAttempt::FailedBeforeOutput(format!(
                "Anthropic HTTP {}: {}",
                status,
                safe_truncate(&b, 200)
            ));
        }

        let mut buf = String::new();
        let ft_deadline = tokio::time::Instant::now() + Duration::from_secs(ft_secs);
        let mut sent_any = false;
        loop {
            let next = if sent_any {
                resp.chunk().await
            } else {
                match tokio::time::timeout_at(ft_deadline, resp.chunk()).await {
                    Ok(c) => c,
                    Err(_) => {
                        return StreamAttempt::FailedBeforeOutput(format!(
                            "首个内容超时（{}s）——模型长时间无可见输出；可调 INNOFORGE_AI_FIRST_TOKEN_TIMEOUT_SECS",
                            ft_secs
                        ));
                    }
                }
            };
            match next {
                Ok(Some(chunk)) => {
                    buf.push_str(&String::from_utf8_lossy(&chunk));
                    while let Some((raw_line, rest)) = buf.split_once('\n') {
                        let line = raw_line.trim().to_string();
                        buf = rest.to_string();
                        if let Some(json_str) = line.strip_prefix("data: ") {
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                                match val["type"].as_str() {
                                    Some("content_block_delta") => {
                                        if let Some(text) = val["delta"]["text"].as_str() {
                                            if !text.is_empty() {
                                                let sanitized = text.replace(['\r', '\n'], " ");
                                                sent_any = true;
                                                if tx.send(sanitized).await.is_err() {
                                                    return StreamAttempt::Completed;
                                                }
                                            }
                                        }
                                    }
                                    Some("message_stop") => {
                                        if !sent_any {
                                            return StreamAttempt::FailedBeforeOutput(
                                                "流结束但未输出任何内容（message_stop 前零内容）"
                                                    .to_string(),
                                            );
                                        }
                                        return StreamAttempt::Completed;
                                    }
                                    Some("error") => {
                                        let msg =
                                            val["error"]["message"].as_str().unwrap_or("unknown");
                                        let err = format!("Anthropic: {}", msg);
                                        return if sent_any {
                                            StreamAttempt::FailedMidStream(err)
                                        } else {
                                            StreamAttempt::FailedBeforeOutput(err)
                                        };
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                Ok(None) => {
                    let rest = buf.trim().to_string();
                    if let Some(json_str) = rest.strip_prefix("data: ") {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                            if let Some(text) =
                                val["delta"]["text"].as_str().filter(|t| !t.is_empty())
                            {
                                let sanitized = text.replace(['\r', '\n'], " ");
                                sent_any = true;
                                let _ = tx.send(sanitized).await;
                            }
                        }
                    }
                    if sent_any {
                        tracing::warn!(
                            "[AI STREAM] anthropic stream closed without message_stop, treating as complete"
                        );
                        return StreamAttempt::Completed;
                    }
                    return StreamAttempt::FailedBeforeOutput("流结束但未输出任何内容".to_string());
                }
                Err(e) => {
                    let err = format!("流中断: {}", e);
                    return if sent_any {
                        StreamAttempt::FailedMidStream(err)
                    } else {
                        StreamAttempt::FailedBeforeOutput(err)
                    };
                }
            }
        }
    } else {
        let request_body = serde_json::json!({
            "model": model,
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
            model,
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
            Err(e) => return StreamAttempt::FailedBeforeOutput(format!("{}", e)),
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return StreamAttempt::FailedBeforeOutput(format!(
                "HTTP {} — {}",
                status,
                safe_truncate(&body, 200)
            ));
        }

        let mut buf = String::new();
        let mut total_chars: usize = 0;
        let ft_deadline = tokio::time::Instant::now() + Duration::from_secs(ft_secs);
        let mut sent_any = false;

        loop {
            let next = if sent_any {
                resp.chunk().await
            } else {
                match tokio::time::timeout_at(ft_deadline, resp.chunk()).await {
                    Ok(c) => c,
                    Err(_) => {
                        return StreamAttempt::FailedBeforeOutput(format!(
                            "首个内容超时（{}s）——模型长时间无可见输出；可调 INNOFORGE_AI_FIRST_TOKEN_TIMEOUT_SECS",
                            ft_secs
                        ));
                    }
                }
            };
            match next {
                Ok(Some(chunk)) => {
                    let chunk_str = String::from_utf8_lossy(&chunk);
                    let trimmed_chunk = chunk_str.trim();
                    if total_chars == 0 && trimmed_chunk.len() > 10 {
                        tracing::info!(
                            "[AI STREAM] first chunk raw preview: {:?} | decoded: {:?}",
                            chunk.get(..chunk.len().min(60)).unwrap_or(chunk.as_ref()),
                            safe_truncate(trimmed_chunk, 80)
                        );
                    }
                    if total_chars == 0 && trimmed_chunk.len() <= 10 {
                        tracing::warn!(
                            "[AI STREAM] first chunk suspiciously short: raw={:?} decoded={:?}",
                            chunk.get(..chunk.len().min(40)).unwrap_or(chunk.as_ref()),
                            safe_truncate(trimmed_chunk, 40)
                        );
                    }
                    total_chars += trimmed_chunk.len();
                    buf.push_str(&chunk_str);

                    while let Some((raw_line, rest)) = buf.split_once('\n') {
                        let line = raw_line.trim().to_string();
                        buf = rest.to_string();

                        if line == "data: [DONE]" {
                            if !sent_any {
                                return StreamAttempt::FailedBeforeOutput(
                                    "流结束但未输出任何内容（[DONE] 前零内容）".to_string(),
                                );
                            }
                            return StreamAttempt::Completed;
                        }
                        if let Some(json_str) = line.strip_prefix("data: ") {
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                                let delta = &val["choices"][0]["delta"];
                                let content = delta["content"].as_str().filter(|c| !c.is_empty());

                                if let Some(content) = content {
                                    let sanitized = content.replace(['\r', '\n'], " ");
                                    sent_any = true;
                                    if tx.send(sanitized).await.is_err() {
                                        return StreamAttempt::Completed;
                                    }
                                } else if let Some(delta_obj) = delta.as_object() {
                                    let fallback = delta_obj.iter().find_map(|(k, v)| {
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
                                            "[AI STREAM] content & reasoning_content both empty, captured '{}' field with {} chars",
                                            field_name,
                                            text.len()
                                        );
                                        let sanitized = text.replace(['\n', '\r'], " ");
                                        sent_any = true;
                                        if tx.send(sanitized).await.is_err() {
                                            return StreamAttempt::Completed;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(None) => {
                    let rest = buf.trim().to_string();
                    if rest == "data: [DONE]" {
                        if !sent_any {
                            return StreamAttempt::FailedBeforeOutput(
                                "流结束但未输出任何内容".to_string(),
                            );
                        }
                        tracing::warn!(
                            "[AI STREAM] stream closed after [DONE] without trailing newline"
                        );
                        return StreamAttempt::Completed;
                    }
                    if let Some(json_str) = rest.strip_prefix("data: ") {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
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
                    if !sent_any {
                        return StreamAttempt::FailedBeforeOutput(
                            "流结束但未输出任何内容".to_string(),
                        );
                    }
                    tracing::warn!(
                        "[AI STREAM] stream closed without [DONE], treating as complete (chars={})",
                        total_chars
                    );
                    return StreamAttempt::Completed;
                }
                Err(e) => {
                    let err = format!("流中断: {}", e);
                    return if sent_any {
                        StreamAttempt::FailedMidStream(err)
                    } else {
                        StreamAttempt::FailedBeforeOutput(err)
                    };
                }
            }
        }
    }
}

impl AiClient {
    /// 多模态看图对话接口（OpenAI Vision 兼容格式，支持主备通道容灾）。
    pub async fn chat_with_images(
        &self,
        system: &str,
        prompt: &str,
        image_base64_urls: &[String],
        temperature: f32,
    ) -> Result<String> {
        if image_base64_urls.is_empty() {
            return Err(anyhow::anyhow!(
                "未提供有效附图（image_base64_urls 为空），无法执行视觉分析。"
            ));
        }

        let mut failed_chain: Vec<String> = Vec::new();
        match self
            .try_provider_vision(
                &self.primary,
                system,
                prompt,
                image_base64_urls,
                temperature,
            )
            .await
        {
            Ok(content) => return Ok(content),
            Err(e) => {
                failed_chain.push(format!("{}: {}", self.primary.name, e));
                if self.fallbacks.is_empty() {
                    return Err(e);
                }
                tracing::warn!(
                    "[{}] vision failed: {}, trying fallbacks...",
                    self.primary.name,
                    e
                );
            }
        }

        for fallback in &self.fallbacks {
            match self
                .try_provider_vision(fallback, system, prompt, image_base64_urls, temperature)
                .await
            {
                Ok(content) => return Ok(content),
                Err(e) => {
                    failed_chain.push(format!("{}: {}", fallback.name, e));
                }
            }
        }

        Err(anyhow::anyhow!(
            "视觉模型通道均失败或不支持看图（{}）。",
            failed_chain.join(" | ")
        ))
    }

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

    /// 通用流式消息发送：返回 SSE chunk 接收端（支持系统代理感知与主备通道自动容灾）
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

        let mut providers = Vec::with_capacity(1 + self.fallbacks.len());
        providers.push(self.primary.clone());
        providers.extend(self.fallbacks.iter().cloned());

        tokio::spawn(async move {
            let global_timeout = Duration::from_secs(AiClient::global_timeout_secs());
            let mut failed_chain: Vec<String> = Vec::new();
            let total_providers = providers.len();

            for (idx, provider) in providers.iter().enumerate() {
                let client = build_ai_http_client(&provider.base_url, global_timeout, false);
                match stream_single_provider(&client, provider, &messages, temperature, &tx).await {
                    StreamAttempt::Completed => return,
                    StreamAttempt::FailedMidStream(err) => {
                        let _ = tx.send(format!("[ERROR] {}", err)).await;
                        return;
                    }
                    StreamAttempt::FailedBeforeOutput(err) => {
                        if idx + 1 < total_providers {
                            tracing::warn!(
                                "[AI STREAM] provider={} ({}) failed before output: {}; failing over to next provider...",
                                provider.name,
                                provider.model,
                                err
                            );
                            failed_chain.push(format!("{}: {}", provider.name, err));
                        } else if total_providers == 1 {
                            let _ = tx.send(format!("[ERROR] {}", err)).await;
                            return;
                        } else {
                            failed_chain.push(format!("{}: {}", provider.name, err));
                            let _ = tx
                                .send(format!(
                                    "[ERROR] 所有流式通道均失败（{}）",
                                    failed_chain.join(" | ")
                                ))
                                .await;
                            return;
                        }
                    }
                }
            }
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

#[cfg(test)]
mod stream_failover_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn send_chat_stream_fails_over_on_primary_http_503() {
        let primary_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind primary");
        let primary_addr = primary_listener.local_addr().expect("primary addr");
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = primary_listener.accept().await {
                let mut buf = [0u8; 2048];
                let _ = sock.read(&mut buf).await;
                let resp = "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nContent-Length: 27\r\nConnection: close\r\n\r\n{\"error\":\"overloaded 503\"}";
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });

        let fb_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fallback");
        let fb_addr = fb_listener.local_addr().expect("fallback addr");
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = fb_listener.accept().await {
                let mut buf = [0u8; 2048];
                let _ = sock.read(&mut buf).await;
                let body = "data: {\"choices\":[{\"delta\":{\"content\":\"热备通道\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"恢复成功\"}}]}\n\ndata: [DONE]\n\n";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });

        let mut client = AiClient::with_config(
            &format!("http://{}", primary_addr),
            "k-primary",
            "primary-model",
        );
        client.add_fallback(
            &format!("http://{}", fb_addr),
            "k-fallback",
            "fallback-model",
            "fallback_1",
        );

        let mut rx = client.send_chat_stream(
            vec![Message {
                role: "user".into(),
                content: "ping".into(),
            }],
            0.2,
        );
        let mut out = String::new();
        while let Some(chunk) = rx.recv().await {
            out.push_str(&chunk);
        }
        assert_eq!(out, "热备通道恢复成功");
    }
}
