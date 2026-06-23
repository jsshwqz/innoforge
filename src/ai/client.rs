//! AI 多模型容灾客户端核心 / Core AI client with multi-provider failover

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

const PROVIDER_HTTP_TIMEOUT_SECS: u64 = 180;
const PROVIDER_MAX_RETRIES: usize = 3;

/// AI 提供者模式 / AI provider operation mode.
#[derive(Debug, Clone, PartialEq)]
pub enum AiProviderMode {
    /// Standard HTTP-based AI provider (e.g. DeepSeek, OpenAI)
    Http,
    /// Use Gemini CLI subprocess (bypasses API key / scope issues)
    GeminiCli,
}

/// 单个 AI 服务商端点 / A single AI provider endpoint.
#[derive(Clone)]
pub(super) struct AiProvider {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

/// AI client with automatic failover across multiple providers.
#[derive(Clone)]
pub struct AiClient {
    pub(super) client: Client,
    pub(super) primary: AiProvider,
    pub(super) fallbacks: Vec<AiProvider>,
    /// Provider mode: HTTP (default) or Gemini CLI subprocess
    pub provider_mode: AiProviderMode,
    /// Path to the Gemini CLI executable (e.g. gemini.cmd)
    pub gemini_cli_path: Option<String>,
}

#[derive(Serialize)]
pub(super) struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}

// Flexible response parser for different AI providers
#[derive(Deserialize)]
struct ChatResponse {
    choices: Option<Vec<Choice>>,
    data: Option<Value>,    // Zhipu format
    result: Option<String>, // Some providers use this
    error: Option<Value>,   // Provider error object (e.g. {"message": "..."})
}

#[derive(Deserialize)]
struct Choice {
    message: Option<ResponseMessage>,
    delta: Option<ResponseMessage>, // Streaming format
}

pub(crate) fn extract_chat_content(raw_text: &str) -> String {
    if let Ok(resp) = serde_json::from_str::<ChatResponse>(raw_text) {
        // 先从 choices 中提取正常内容
        if let Some(choices) = resp.choices {
            let mut all_empty = true;
            for choice in &choices {
                if let Some(content) = choice
                    .message
                    .as_ref()
                    .and_then(|message| message.content.as_ref())
                    .or_else(|| {
                        choice
                            .delta
                            .as_ref()
                            .and_then(|delta| delta.content.as_ref())
                    })
                {
                    all_empty = false;
                    if !content.is_empty() {
                        return content.clone();
                    }
                }
            }
            // choices 存在但全为空 → 尝试提取 error 中的错误信息
            if all_empty && !choices.is_empty() {
                if let Some(err) = &resp.error {
                    if let Some(msg) = err["message"].as_str() {
                        return format!("AI 错误：{}", msg);
                    }
                }
            }
        }

        if let Some(data) = resp.data {
            if let Some(content) = extract_content_from_data(&data) {
                return content;
            }
        }

        if let Some(result) = resp.result {
            return result;
        }

        // ChatResponse 解析成功但无内容 → 检查 error 字段
        if let Some(err) = &resp.error {
            if let Some(msg) = err["message"].as_str() {
                return format!("AI 错误：{}", msg);
            }
        }
    }

    if let Ok(json) = serde_json::from_str::<Value>(raw_text) {
        if let Some(content) = extract_content_from_data(&json) {
            return content;
        }
        if let Some(err) = json["error"]["message"].as_str() {
            return format!("AI 错误：{}", err);
        }
        if let Some(msg) = json["msg"].as_str() {
            return format!("AI 错误：{}", msg);
        }
        // Handle array response: [{"error": {"message": "...", ...}}]
        if let Some(arr) = json.as_array() {
            for item in arr {
                if let Some(err) = item["error"]["message"].as_str() {
                    return format!("AI 错误：{}", err);
                }
            }
        }
    }

    format!(
        "AI 响应解析失败，原始响应：{}",
        raw_text.chars().take(200).collect::<String>()
    )
}

/// Safely truncate a UTF-8 string to at most `max_bytes` bytes
/// without splitting multi-byte characters.
pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    &s[..end]
}

fn extract_content_from_data(data: &Value) -> Option<String> {
    if let Some(choices) = data["choices"].as_array() {
        for choice in choices {
            if let Some(content) = choice["message"]["content"].as_str() {
                return Some(content.to_string());
            }
            if let Some(content) = choice["delta"]["content"].as_str() {
                return Some(content.to_string());
            }
        }
    }

    if let Some(content) = data["data"]["content"].as_str() {
        return Some(content.to_string());
    }

    if let Some(content) = data["data"]["choices"][0]["message"]["content"].as_str() {
        return Some(content.to_string());
    }

    None
}

impl AiClient {
    fn apply_model_override(provider: &AiProvider, model_override: Option<&str>) -> AiProvider {
        if let Some(model) = model_override {
            let m = model.trim();
            if !m.is_empty() {
                let mut p = provider.clone();
                p.model = m.to_string();
                return p;
            }
        }
        provider.clone()
    }

    /// Create from explicit config values (preferred).
    pub fn with_config(base_url: &str, api_key: &str, model: &str) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(PROVIDER_HTTP_TIMEOUT_SECS))
                .no_proxy()
                .build()
                .unwrap_or_else(|_| {
                    Client::builder()
                        .no_proxy()
                        .build()
                        .unwrap_or_else(|_| Client::new())
                }),
            primary: AiProvider {
                name: "primary".to_string(),
                base_url: base_url.to_string(),
                api_key: api_key.to_string(),
                model: model.to_string(),
            },
            fallbacks: Vec::new(),
            provider_mode: AiProviderMode::Http,
            gemini_cli_path: None,
        }
    }

    /// Switch to Gemini CLI subprocess mode.
    pub fn set_gemini_cli(&mut self, path: &str) {
        self.provider_mode = AiProviderMode::GeminiCli;
        self.gemini_cli_path = Some(path.to_string());
    }

    /// Check if this client is in Gemini CLI subprocess mode.
    pub fn is_gemini_cli_mode(&self) -> bool {
        self.provider_mode == AiProviderMode::GeminiCli
    }

    /// Add a fallback AI provider.
    pub fn add_fallback(&mut self, base_url: &str, api_key: &str, model: &str, name: &str) {
        self.fallbacks.push(AiProvider {
            name: name.to_string(),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
        });
    }

    /// Try a single provider with retries.
    pub(super) async fn try_provider(
        &self,
        provider: &AiProvider,
        messages: &[Message],
        temperature: f32,
    ) -> Result<String> {
        let request_body = ChatRequest {
            model: provider.model.clone(),
            messages: messages.to_vec(),
            temperature,
        };

        let max_retries = PROVIDER_MAX_RETRIES;
        let mut last_err = None;
        for attempt in 0..max_retries {
            if attempt > 0 {
                let delay = Duration::from_secs(2);
                tokio::time::sleep(delay).await;
            }

            let req_url = format!(
                "{}/chat/completions",
                provider.base_url.trim_end_matches('/')
            );
            tracing::debug!(
                "[{}] POST {} model={} messages={}",
                provider.name,
                req_url,
                provider.model,
                messages.len()
            );
            match self
                .client
                .post(&req_url)
                .header("Authorization", format!("Bearer {}", provider.api_key))
                .json(&request_body)
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();

                    if status.as_u16() == 429 {
                        let retry_after = resp
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok());
                        let raw_text = resp.text().await.unwrap_or_default();
                        tracing::warn!(
                            "[{}] rate limited (429): {}",
                            provider.name,
                            safe_truncate(&raw_text, 100)
                        );
                        if attempt < max_retries - 1 {
                            let delay = retry_after.map(Duration::from_secs).unwrap_or_else(|| {
                                Duration::from_secs(2u64.pow(attempt as u32 + 1))
                            });
                            tracing::info!(
                                "[{}] retrying after {}s due to rate limit...",
                                provider.name,
                                delay.as_secs()
                            );
                            tokio::time::sleep(delay).await;
                            last_err = Some(anyhow::anyhow!("AI 频率限制，请稍后再试"));
                            continue;
                        }
                        return Err(anyhow::anyhow!("AI 频率限制，请稍后再试"));
                    }

                    if status.as_u16() == 401 || status.as_u16() == 403 {
                        let raw_text = resp.text().await.unwrap_or_default();
                        tracing::warn!(
                            "[{}] auth error ({}): {}",
                            provider.name,
                            status.as_u16(),
                            safe_truncate(&raw_text, 200)
                        );
                        return Err(anyhow::anyhow!(
                            "AI API Key 无效或已过期。请到「设置」页面检查 API Key 配置。"
                        ));
                    }

                    let raw_text = match resp.text().await {
                        Ok(text) => text,
                        Err(e) => {
                            if attempt < max_retries - 1 {
                                last_err = Some(anyhow::anyhow!("AI 响应读取中断: {}", e));
                                continue;
                            }
                            return Err(anyhow::anyhow!(
                                "AI 响应读取失败（连接中断）。可能原因：\n\
                                 1. API Key 无效或余额不足\n\
                                 2. 网络不稳定\n\
                                 3. AI 服务暂时不可用\n\
                                 请到「设置」检查 AI 配置。"
                            ));
                        }
                    };
                    tracing::debug!(
                        "[{}] status={} body_len={} body_preview={}",
                        provider.name,
                        status.as_u16(),
                        raw_text.len(),
                        safe_truncate(&raw_text, 120)
                    );
                    let content = extract_chat_content(&raw_text);

                    if status.is_server_error() && attempt < max_retries - 1 {
                        last_err = Some(anyhow::anyhow!("Server error {}", status));
                        continue;
                    }

                    if content.starts_with("AI 错误") && attempt < max_retries - 1 {
                        last_err = Some(anyhow::anyhow!("{}", content));
                        continue;
                    }

                    return Ok(content);
                }
                Err(e) => {
                    if e.is_connect() && provider.base_url.contains("localhost") {
                        return Err(anyhow::anyhow!(
                            "AI 未配置。请打开「设置」页面，配置云端 AI 服务（如智谱 GLM、OpenRouter 等）。\
                             当前默认连接本地 Ollama (localhost:11434)，手机端不可用。"
                        ));
                    }
                    if attempt < max_retries - 1 && (e.is_timeout() || e.is_connect()) {
                        last_err = Some(e.into());
                        continue;
                    }
                    return Err(e.into());
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Provider {} failed", provider.name)))
    }

    /// 全局超时上限
    pub(crate) const GLOBAL_TIMEOUT_SECS: u64 = 300;

    /// Get the current model name.
    pub fn model_name(&self) -> &str {
        &self.primary.model
    }

    /// Send a raw JSON body to the AI provider (used for multimodal/vision requests).
    pub async fn send_json_body(&self, body: serde_json::Value) -> Result<String> {
        match tokio::time::timeout(
            Duration::from_secs(Self::GLOBAL_TIMEOUT_SECS),
            self.send_json_body_inner(body),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(anyhow::anyhow!(
                "AI 调用超时（全局上限 {}s）。请检查网络或更换 AI 服务商。",
                Self::GLOBAL_TIMEOUT_SECS
            )),
        }
    }

    async fn send_json_body_inner(&self, body: serde_json::Value) -> Result<String> {
        let mut last_err = anyhow::anyhow!("All providers failed");
        let mut failed_chain: Vec<String> = Vec::new();
        // Try primary
        match self.try_provider_with_body(&self.primary, &body).await {
            Ok(content) => return Ok(content),
            Err(e) => {
                failed_chain.push(format!("{}: {}", self.primary.name, e));
                if self.fallbacks.is_empty() {
                    return Err(e);
                }
                tracing::warn!("[{}] failed: {}, trying fallbacks...", self.primary.name, e);
                last_err = e;
            }
        }
        for fallback in &self.fallbacks {
            match self.try_provider_with_body(fallback, &body).await {
                Ok(content) => return Ok(content),
                Err(e) => {
                    failed_chain.push(format!("{}: {}", fallback.name, e));
                    last_err = e;
                }
            }
        }
        if failed_chain.is_empty() {
            Err(last_err)
        } else {
            Err(anyhow::anyhow!(
                "AI 多通道均失败（{}）。请检查对应服务商 Key/额度/网络状态。",
                failed_chain.join(" | ")
            ))
        }
    }

    /// Try a single provider with a raw JSON body (for multimodal etc.)
    async fn try_provider_with_body(
        &self,
        provider: &AiProvider,
        body: &serde_json::Value,
    ) -> Result<String> {
        let max_retries = PROVIDER_MAX_RETRIES;
        let mut last_err = None;
        for attempt in 0..max_retries {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            match self
                .client
                .post(format!(
                    "{}/chat/completions",
                    provider.base_url.trim_end_matches('/')
                ))
                .header("Authorization", format!("Bearer {}", provider.api_key))
                .json(body)
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.as_u16() == 429 {
                        let retry_after = resp
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok());
                        let raw_text = resp.text().await.unwrap_or_default();
                        tracing::warn!(
                            "[{}] rate limited (429): {}",
                            provider.name,
                            safe_truncate(&raw_text, 100)
                        );
                        if attempt < max_retries - 1 {
                            let delay = retry_after.map(Duration::from_secs).unwrap_or_else(|| {
                                Duration::from_secs(2u64.pow(attempt as u32 + 1))
                            });
                            tracing::info!(
                                "[{}] retrying after {}s due to rate limit...",
                                provider.name,
                                delay.as_secs()
                            );
                            tokio::time::sleep(delay).await;
                            last_err = Some(anyhow::anyhow!("AI 频率限制，请稍后再试"));
                            continue;
                        }
                        return Err(anyhow::anyhow!("AI 频率限制，请稍后再试"));
                    }
                    if status.as_u16() == 401 || status.as_u16() == 403 {
                        let raw_text = resp.text().await.unwrap_or_default();
                        tracing::warn!(
                            "[{}] auth error ({}): {}",
                            provider.name,
                            status.as_u16(),
                            safe_truncate(&raw_text, 200)
                        );
                        return Err(anyhow::anyhow!(
                            "AI API Key 无效或已过期。请到「设置」页面检查 API Key 配置。"
                        ));
                    }
                    let raw_text = match resp.text().await {
                        Ok(text) => text,
                        Err(e) => {
                            if attempt < max_retries - 1 {
                                last_err = Some(anyhow::anyhow!("AI 响应读取中断: {}", e));
                                continue;
                            }
                            return Err(anyhow::anyhow!("AI 响应读取失败（连接中断）。可能原因：\n 1. API Key 无效或余额不足\n 2. 网络不稳定\n 3. AI 服务暂时不可用\n 请到「设置」检查 AI 配置。"));
                        }
                    };
                    let content = extract_chat_content(&raw_text);
                    if status.is_server_error() && attempt < max_retries - 1 {
                        last_err = Some(anyhow::anyhow!("Server error {}", status));
                        continue;
                    }
                    if content.starts_with("AI 错误") && attempt < max_retries - 1 {
                        last_err = Some(anyhow::anyhow!("{}", content));
                        continue;
                    }
                    return Ok(content);
                }
                Err(e) => {
                    if e.is_connect() && provider.base_url.contains("localhost") {
                        return Err(anyhow::anyhow!(
                            "AI 未配置。请打开「设置」页面，配置云端 AI 服务（如智谱 GLM、OpenRouter 等）。\
                             当前默认连接本地 Ollama (localhost:11434)，手机端不可用。"
                        ));
                    }
                    if attempt < max_retries - 1 && (e.is_timeout() || e.is_connect()) {
                        last_err = Some(e.into());
                        continue;
                    }
                    return Err(e.into());
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Provider {} failed", provider.name)))
    }

    /// 带全局超时的 AI 调用入口
    pub(super) async fn send_chat(
        &self,
        messages: Vec<Message>,
        temperature: f32,
    ) -> Result<String> {
        match tokio::time::timeout(
            Duration::from_secs(Self::GLOBAL_TIMEOUT_SECS),
            self.send_chat_inner(messages, temperature),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(anyhow::anyhow!(
                "AI 调用超时（全局上限 {}s）。请检查网络或更换 AI 服务商。",
                Self::GLOBAL_TIMEOUT_SECS
            )),
        }
    }

    /// 调用 Gemini CLI 子进程进行 AI 推理 / Call Gemini CLI subprocess for AI inference.
    async fn call_gemini_cli(messages: &[Message], gemini_path: Option<&str>) -> Result<String> {
        let gemini_path = gemini_path.unwrap_or("gemini.cmd");

        // Build the conversation text from messages
        let mut prompt = String::new();
        for msg in messages {
            let label = match msg.role.as_str() {
                "system" => "System",
                "user" => "User",
                "assistant" => "Assistant",
                _ => &msg.role,
            };
            prompt.push_str(&format!("{}: {}\n", label, msg.content));
        }

        tracing::info!(
            "Gemini CLI subprocess: path={} prompt_len={}",
            gemini_path,
            prompt.len()
        );

        // Spawn Gemini CLI subprocess
        // On Windows, .cmd batch files are just wrappers for node scripts.
        // We resolve the actual JS file and run `node` directly to avoid
        // cmd.exe argument forwarding issues.
        let (program, extra_args) = if gemini_path.ends_with(".cmd") {
            // gemini.cmd resolves to:
            //   %dp0%\node_modules\@google\gemini-cli\bundle\gemini.js
            let dir = std::path::Path::new(gemini_path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."));
            let js_path = dir
                .join("node_modules")
                .join("@google")
                .join("gemini-cli")
                .join("bundle")
                .join("gemini.js");
            (
                "node".to_string(),
                vec![js_path.to_string_lossy().to_string()],
            )
        } else {
            (gemini_path.to_string(), vec![])
        };
        let output = tokio::process::Command::new(&program)
            .env("GEMINI_CLI_TRUST_WORKSPACE", "true")
            .args(&extra_args)
            .args(["-p", &prompt, "--skip-trust", "--output-format", "json"])
            .output()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Gemini CLI 启动失败（{}）。请确认已安装 Gemini CLI（npm install -g @google/gemini-cli）。",
                    e
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_trimmed: String = stderr.chars().take(500).collect();
            return Err(anyhow::anyhow!(
                "Gemini CLI 执行失败（exit code: {}）: {}",
                output.status.code().unwrap_or(-1),
                stderr_trimmed
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Find JSON object in stdout (may have warnings/logs before it)
        let json_start = stdout
            .find('{')
            .ok_or_else(|| anyhow::anyhow!("Gemini CLI 输出中没有找到 JSON 响应"))?;
        let json_str = &stdout[json_start..];

        // Find matching closing brace
        let mut depth = 0;
        let json_end = json_str
            .char_indices()
            .find(|&(_, c)| {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
                depth == 0 && c == '}'
            })
            .map(|(i, _)| i + 1)
            .unwrap_or(json_str.len());
        let json_str = &json_str[..json_end];

        let json: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
            anyhow::anyhow!(
                "Gemini CLI 响应解析失败: {} (preview: {})",
                e,
                safe_truncate(json_str, 200)
            )
        })?;

        json["response"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Gemini CLI 响应中缺少 response 字段"))
    }

    /// 内部实现：Gemini CLI 模式或依次尝试 primary + fallbacks
    async fn send_chat_inner(&self, messages: Vec<Message>, temperature: f32) -> Result<String> {
        if self.provider_mode == AiProviderMode::GeminiCli {
            return Self::call_gemini_cli(&messages, self.gemini_cli_path.as_deref()).await;
        }

        let mut failed_chain: Vec<String> = Vec::new();
        match self
            .try_provider(&self.primary, &messages, temperature)
            .await
        {
            Ok(content) => return Ok(content),
            Err(e) => {
                failed_chain.push(format!("{}: {}", self.primary.name, e));
                if self.fallbacks.is_empty() {
                    return Err(e);
                }
                tracing::warn!("[{}] failed: {}, trying fallbacks...", self.primary.name, e);
            }
        }

        let mut last_err = anyhow::anyhow!("All providers failed");
        for fallback in &self.fallbacks {
            tracing::info!("[failover] trying {}...", fallback.name);
            match self.try_provider(fallback, &messages, temperature).await {
                Ok(content) => {
                    tracing::info!("[failover] {} succeeded", fallback.name);
                    return Ok(content);
                }
                Err(e) => {
                    tracing::warn!("[failover] {} failed: {}", fallback.name, e);
                    failed_chain.push(format!("{}: {}", fallback.name, e));
                    last_err = e;
                }
            }
        }

        if failed_chain.is_empty() {
            Err(last_err)
        } else {
            Err(anyhow::anyhow!(
                "AI 多通道均失败（{}）。请检查对应服务商 Key/额度/网络状态。",
                failed_chain.join(" | ")
            ))
        }
    }

    async fn send_chat_inner_with_model(
        &self,
        messages: Vec<Message>,
        temperature: f32,
        model_override: Option<&str>,
    ) -> Result<String> {
        if self.provider_mode == AiProviderMode::GeminiCli {
            return Self::call_gemini_cli(&messages, self.gemini_cli_path.as_deref()).await;
        }

        let mut failed_chain: Vec<String> = Vec::new();
        let primary = Self::apply_model_override(&self.primary, model_override);
        match self.try_provider(&primary, &messages, temperature).await {
            Ok(content) => return Ok(content),
            Err(e) => {
                failed_chain.push(format!("{}: {}", primary.name, e));
                if self.fallbacks.is_empty() {
                    return Err(e);
                }
                tracing::warn!("[{}] failed: {}, trying fallbacks...", primary.name, e);
            }
        }

        let mut last_err = anyhow::anyhow!("All providers failed");
        for fallback in &self.fallbacks {
            let fb = Self::apply_model_override(fallback, model_override);
            tracing::info!("[failover] trying {}...", fb.name);
            match self.try_provider(&fb, &messages, temperature).await {
                Ok(content) => {
                    tracing::info!("[failover] {} succeeded", fb.name);
                    return Ok(content);
                }
                Err(e) => {
                    tracing::warn!("[failover] {} failed: {}", fb.name, e);
                    failed_chain.push(format!("{}: {}", fb.name, e));
                    last_err = e;
                }
            }
        }

        if failed_chain.is_empty() {
            Err(last_err)
        } else {
            Err(anyhow::anyhow!(
                "AI 多通道均失败（{}）。请检查对应服务商 Key/额度/网络状态。",
                failed_chain.join(" | ")
            ))
        }
    }

    pub(super) async fn send_chat_expert(
        &self,
        messages: Vec<Message>,
        temperature: f32,
    ) -> Result<String> {
        // 使用 AiClient 已配置的模型（由 ai_client_expert 或 ai_client 决定），
        // 不再通过 env var 覆盖，确保设置页的模型选择生效。
        match tokio::time::timeout(
            Duration::from_secs(Self::GLOBAL_TIMEOUT_SECS),
            self.send_chat_inner_with_model(messages, temperature, None),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(anyhow::anyhow!(
                "AI 调用超时（全局上限 {}s）。请检查网络或更换 AI 服务商。",
                Self::GLOBAL_TIMEOUT_SECS
            )),
        }
    }
}
