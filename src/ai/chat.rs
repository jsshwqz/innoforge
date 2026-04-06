//! 聊天接口 / Chat interface methods

use super::client::{safe_truncate, AiClient, Message};
use anyhow::Result;
use std::time::Duration;

impl AiClient {
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
        self.send_chat(messages, temperature).await
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

    /// 流式聊天：返回 SSE chunk 接收端
    pub fn chat_stream(
        &self,
        user_msg: &str,
        context: Option<&str>,
    ) -> tokio::sync::mpsc::Receiver<String> {
        let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);

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

        let provider = self.primary.clone();
        let client = self.client.clone();

        tokio::spawn(async move {
            let _ = tokio::time::timeout(
                Duration::from_secs(AiClient::GLOBAL_TIMEOUT_SECS),
                async {
                    let request_body = serde_json::json!({
                        "model": provider.model,
                        "messages": messages.iter().map(|m| serde_json::json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
                        "temperature": 0.7,
                        "stream": true,
                    });

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
                        let _ = tx.send(format!("[ERROR] HTTP {} — {}", status, safe_truncate(&body, 200))).await;
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

                                    if line == "data: [DONE]" {
                                        return;
                                    }
                                    if let Some(json_str) = line.strip_prefix("data: ") {
                                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                                            if let Some(content) = val["choices"][0]["delta"]["content"].as_str() {
                                                if !content.is_empty()
                                                    && tx.send(content.to_string()).await.is_err()
                                                {
                                                    return;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Ok(None) => break,
                            Err(_) => break,
                        }
                    }
                },
            )
            .await;
        });

        rx
    }
}
