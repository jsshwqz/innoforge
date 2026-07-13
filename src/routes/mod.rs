//! API 路由层 / API Routes
//!
//! 所有 HTTP 端点的实现，按功能模块拆分。
//! All HTTP endpoint implementations, organized by feature module.
//!
//! - [`ai`] — AI 聊天、摘要、对比 / AI chat, summary, comparison
//! - [`search`] — 专利搜索 / Patent search
//! - [`idea`] — 创意验证 + 多轮对话 / Idea validation + multi-round chat
//! - [`patent`] — 专利详情 / Patent details
//! - [`collections`] — 收藏夹管理 / Collections management
//! - [`settings`] — 系统设置 / System settings
//! - [`ipc`] — IPC 分类 / IPC classification
//! - [`upload`] — 文件上传 / File upload
//! - [`pages`] — 页面渲染 / Page rendering

mod ai;
mod auth;
mod chat;
mod collections;
mod feature_cards;
mod idea;
mod ipc;
mod pages;
mod patent;
mod search;
mod settings;
mod upload;

pub use ai::*;
pub use auth::*;
pub use chat::*;
pub use collections::*;
pub use feature_cards::*;
pub use idea::*;
pub use ipc::*;
pub use pages::*;
pub use patent::*;
pub use search::*;
pub use settings::*;
pub use upload::*;

use crate::{ai::AiClient, db::Database, pipeline::context::PipelineProgress};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use tokio::sync::broadcast;

/// Round-robin counter for SerpAPI multi-key rotation.
static SERPAPI_KEY_INDEX: AtomicUsize = AtomicUsize::new(0);

/// 管道通道条目，附带创建时间用于超时清理
/// Pipeline channel entry with creation timestamp for stale cleanup
pub struct PipelineChannelEntry {
    pub sender: broadcast::Sender<PipelineProgress>,
    pub created_at: Instant,
}

/// Shared application configuration — 仅保留 SerpAPI + DeepSeek AI，其它搜索源和 AI 已屏蔽
#[derive(Debug, Clone, Default)]
pub struct AppConfig {
    /// SerpAPI multi-key support (round-robin) — 最多 5 个 Key，自动轮询
    pub serpapi_keys: Vec<String>,

    pub ai_base_url: String,
    /// 通用 AI API Key（自定义服务商使用 + 向后兼容）
    pub ai_api_key: String,
    /// 各服务商独立 Key：切换服务商不会丢失
    pub ai_api_key_deepseek: String,
    pub ai_api_key_anthropic: String,
    pub ai_api_key_xiaomi: String,
    pub ai_api_key_sensetime: String,
    pub ai_api_key_openrouter: String,
    pub ai_api_key_gemini: String,
    pub ai_api_key_zhipu: String,
    pub ai_model: String,
    /// 专家模型（用于创新推演、深分析等高推理任务，默认 deepseek-reasoner）
    pub ai_model_expert: String,

    // ── Google OAuth for Gemini API ──
    pub google_client_id: String,
    pub google_client_secret: String,
    pub google_refresh_token: String,
    pub google_access_token: String,
    pub google_token_expiry: Option<i64>,
    /// 认证模式："oauth"（浏览器 OAuth）或 "gcloud"（gcloud CLI/ADC）
    /// 空字符串表示未配置 Google 认证
    pub google_auth_mode: String,

    // ── Gemini CLI 子进程模式 ──
    /// 是否启用 Gemini CLI 子进程模式（替代 HTTP API Key 方式）
    pub gemini_cli_enabled: bool,
    /// Gemini CLI 可执行文件路径（自动检测）
    pub gemini_cli_path: String,
}

impl AppConfig {
    /// Load from environment only (without DB). Kept for tests/fallback.
    #[allow(dead_code)]
    pub fn from_env() -> Self {
        Self::from_db_and_env(None)
    }

    /// 从 SQLite 数据库加载设置，环境变量作为后备。
    /// SQLite 是主存储（Android 友好），.env 是次要存储（桌面端后备）。
    pub fn from_db_and_env(db: Option<&Database>) -> Self {
        // 先从数据库加载所有设置
        let db_settings = db
            .and_then(|d| d.get_all_settings().ok())
            .unwrap_or_default();

        // 辅助：优先取 DB 值，其次取环境变量，最后用默认值
        let get = |key: &str, default: &str| -> String {
            if let Some(v) = db_settings.get(key) {
                if !v.is_empty() {
                    return v.clone();
                }
            }
            std::env::var(key).unwrap_or_else(|_| default.to_string())
        };

        // Load SerpAPI multi-key (SERPAPI_KEY_1 ~ SERPAPI_KEY_5)
        let mut serpapi_keys: Vec<String> = Vec::new();

        for i in 1..=5 {
            let k = get(&format!("SERPAPI_KEY_{}", i), "");
            if !k.is_empty() && k != "your-serpapi-key-here" {
                serpapi_keys.push(k);
            }
        }
        // Backward compatibility: read old SERPAPI_KEY if no multi-key configured
        if serpapi_keys.is_empty() {
            let old = get("SERPAPI_KEY", "");
            if !old.is_empty() && old != "your-serpapi-key-here" {
                serpapi_keys.push(old);
            }
        }

        // 解析 google_token_expiry（存储在 DB 中的字符串），
        // 环境变量 GOOGLE_TOKEN_EXPIRY 作为后备
        let google_token_expiry = db_settings
            .get("google_token_expiry")
            .and_then(|v| v.parse::<i64>().ok())
            .or_else(|| {
                std::env::var("GOOGLE_TOKEN_EXPIRY")
                    .ok()
                    .and_then(|v| v.parse::<i64>().ok())
            });

        // 加载各服务商独立 Key，优先使用独立 Key，其次回退到通用 AI_API_KEY
        let load_provider_key = |specific_key: &str| -> String {
            let v = get(specific_key, "");
            if v.is_empty() {
                get("AI_API_KEY", "") // 回退到通用 Key
            } else {
                v
            }
        };

        Self {
            serpapi_keys,

            ai_base_url: get("AI_BASE_URL", "http://localhost:11434/v1"),
            ai_api_key: get("AI_API_KEY", "ollama"),
            ai_api_key_deepseek: load_provider_key("AI_API_KEY_DEEPSEEK"),
            ai_api_key_anthropic: load_provider_key("AI_API_KEY_ANTHROPIC"),
            ai_api_key_xiaomi: load_provider_key("AI_API_KEY_XIAOMI"),
            ai_api_key_sensetime: load_provider_key("AI_API_KEY_SENSENOVA"),
            ai_api_key_openrouter: load_provider_key("AI_API_KEY_OPENROUTER"),
            ai_api_key_gemini: load_provider_key("AI_API_KEY_GEMINI"),
            ai_api_key_zhipu: load_provider_key("AI_API_KEY_ZHIPU"),
            ai_model: get("AI_MODEL", "qwen2.5:7b"),
            ai_model_expert: get("AI_MODEL_EXPERT", "deepseek-reasoner"),

            google_client_id: get("GOOGLE_CLIENT_ID", ""),
            google_client_secret: get("GOOGLE_CLIENT_SECRET", ""),
            google_refresh_token: get("google_refresh_token", ""),
            google_access_token: get("google_access_token", ""),
            google_token_expiry,
            google_auth_mode: get("google_auth_mode", ""),

            gemini_cli_enabled: get("GEMINI_CLI_ENABLED", "") == "true",
            gemini_cli_path: find_gemini_cli().unwrap_or_default(),
        }
    }

    /// Build an AiClient from the current config.
    /// 如果配置了 Google OAuth 且当前是 Gemini 端点，自动使用 OAuth access_token。
    /// 如果启用了 Gemini CLI 模式，使用子进程调用 Gemini CLI。
    /// 如果基础 URL 是 Google API，自动添加模型降级兜底（quota 不足时 fallback 到更便宜的模型）。
    pub fn ai_client(&self) -> AiClient {
        let api_key = self.effective_api_key();
        let mut client = AiClient::with_config(&self.ai_base_url, &api_key, &self.ai_model);
        self.add_gemini_model_fallback(&mut client, &self.ai_model);
        let is_gemini = self.ai_base_url.contains("googleapis");
        if is_gemini && self.gemini_cli_enabled && !self.gemini_cli_path.is_empty() {
            client.set_gemini_cli(&self.gemini_cli_path);
        }
        client
    }

    /// Build an AiClient using the expert model for deep analysis tasks.
    /// 同样自动添加模型降级兜底。
    pub fn ai_client_expert(&self) -> AiClient {
        let api_key = self.effective_api_key();
        let mut client = AiClient::with_config(&self.ai_base_url, &api_key, &self.ai_model_expert);
        self.add_gemini_model_fallback(&mut client, &self.ai_model_expert);
        let is_gemini = self.ai_base_url.contains("googleapis");
        if is_gemini && self.gemini_cli_enabled && !self.gemini_cli_path.is_empty() {
            client.set_gemini_cli(&self.gemini_cli_path);
        }
        client
    }

    /// 为 Gemini 端点添加模型降级兜底：从高到低，主模型 quota 用完后尝试更便宜的模型。
    /// 非 Gemini 端点不添加降级。
    fn add_gemini_model_fallback(&self, client: &mut AiClient, primary_model: &str) {
        let is_gemini = self.ai_base_url.contains("googleapis");
        if !is_gemini || self.gemini_cli_enabled {
            return;
        }
        let api_key = self.effective_api_key();
        // 降级优先级：从最新到最稳定（越靠前越新、quota 可能越紧）
        let fallback_chain = ["gemini-2.5-flash", "gemini-2.0-flash", "gemini-1.5-flash"];
        for fb_model in &fallback_chain {
            if fb_model == &primary_model {
                continue;
            }
            client.add_fallback(&self.ai_base_url, &api_key, fb_model, fb_model);
        }
    }

    /// 根据 base_url 检测当前 AI 服务商、返回对应的 API Key。
    /// custom/未识别服务商返回通用 `ai_api_key`（向后兼容）。
    pub fn api_key_for_provider(&self, base_url: &str) -> String {
        let key = if base_url.contains("deepseek") {
            &self.ai_api_key_deepseek
        } else if base_url.contains("anthropic") {
            &self.ai_api_key_anthropic
        } else if base_url.contains("xiaomimimo") {
            &self.ai_api_key_xiaomi
        } else if base_url.contains("sensenova") {
            &self.ai_api_key_sensetime
        } else if base_url.contains("openrouter") {
            &self.ai_api_key_openrouter
        } else if base_url.contains("googleapis") {
            &self.ai_api_key_gemini
        } else if base_url.contains("bigmodel") {
            &self.ai_api_key_zhipu
        } else {
            &self.ai_api_key
        };
        if key.is_empty() {
            self.ai_api_key.clone()
        } else {
            key.clone()
        }
    }

    /// 返回有效的 API Key：
    /// - 先按服务商获取独立 Key
    /// - 如果使用 Gemini 且配置了有效（未过期）的 OAuth token，优先返回 access_token
    /// - 如果 OAuth token 已过期，回退到 Gemini 独立 Key
    fn effective_api_key(&self) -> String {
        let is_gemini = self.ai_base_url.contains("googleapis");
        if is_gemini && !self.google_access_token.is_empty() {
            let is_expired = self
                .google_token_expiry
                .map(|exp| {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    now >= exp
                })
                .unwrap_or(true);
            if !is_expired {
                return self.google_access_token.clone();
            }
            tracing::warn!(
                "Google OAuth token expired (expiry={:?}), falling back to API Key",
                self.google_token_expiry
            );
        }
        self.api_key_for_provider(&self.ai_base_url)
    }

    /// Whether at least one SerpAPI key is configured.
    pub fn has_serpapi(&self) -> bool {
        !self.serpapi_keys.is_empty()
    }

    /// Round-robin: pick the next SerpAPI key.
    pub fn next_serpapi_key(&self) -> Option<String> {
        if self.serpapi_keys.is_empty() {
            return None;
        }
        let idx = SERPAPI_KEY_INDEX.fetch_add(1, Ordering::Relaxed) % self.serpapi_keys.len();
        Some(self.serpapi_keys[idx].clone())
    }
}

/// 检测 Gemini CLI 是否可用 / Detect Gemini CLI installation.
/// 返回可执行文件的完整路径，如果未安装则返回 None。
pub(crate) fn find_gemini_cli() -> Option<String> {
    // 检查常见安装路径
    let candidates = [
        // Windows npm global
        r"C:\Users\Administrator\AppData\Roaming\npm\gemini.cmd",
        // Windows npm global (alternative)
        r"C:\Program Files\nodejs\gemini.cmd",
        r"C:\Program Files (x86)\nodejs\gemini.cmd",
        // PATH lookups
        "gemini.cmd",
        "gemini",
    ];

    // Check PATH candidates first
    let found = candidates.iter().find(|cmd| {
        std::process::Command::new(cmd)
            .args(["--version"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    });
    if let Some(cmd) = found {
        return Some(cmd.to_string());
    }

    // On Windows, also check USERPROFILE
    if let Some(userprofile) = std::env::var_os("USERPROFILE") {
        let npm_path = std::path::PathBuf::from(&userprofile)
            .join("AppData")
            .join("Roaming")
            .join("npm")
            .join("gemini.cmd");
        if npm_path.exists() {
            return Some(npm_path.to_string_lossy().to_string());
        }
    }

    None
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub config: Arc<RwLock<AppConfig>>,
    /// 管道进度通道（SSE 推送），附带超时清理 / Pipeline progress channels with stale cleanup
    pub pipeline_channels: Arc<Mutex<HashMap<String, PipelineChannelEntry>>>,
}

impl AppState {
    /// 启动后台定时清理，移除超过 5 分钟的管道通道（防止 panic 导致泄漏）
    /// Spawn background task to remove pipeline channels older than 5 minutes
    pub fn spawn_channel_cleaner(&self) {
        let channels = self.pipeline_channels.clone();
        let db = self.db.clone();
        let config = self.config.clone();
        tokio::spawn(async move {
            let stale_threshold = std::time::Duration::from_secs(300); // 5 分钟
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;

                // 清理超时的管道通道
                {
                    let mut ch = channels.lock().unwrap_or_else(|e| e.into_inner());
                    let before = ch.len();
                    ch.retain(|id, entry| {
                        let stale = entry.created_at.elapsed() > stale_threshold;
                        if stale {
                            tracing::info!(
                                "清理超时管道通道: {} (已存在 {:?})",
                                id,
                                entry.created_at.elapsed()
                            );
                        }
                        !stale
                    });
                    if ch.len() < before {
                        tracing::info!("管道通道清理完成: 移除 {} 个", before - ch.len());
                    }
                } // ch dropped here

                // 自动刷新 Google OAuth / gcloud 令牌（如需要）
                refresh_google_token_background(&config, &db).await;

                // 重置卡住超过 10 分钟的 analyzing 创意为 error
                match db.reset_stuck_analyzing(10) {
                    Ok(n) if n > 0 => {
                        tracing::warn!("自动恢复: 重置 {} 个卡住的 analyzing 创意为 error", n)
                    }
                    Ok(_) => {}
                    Err(e) => tracing::error!("自动恢复检查失败: {}", e),
                }

                // 清理超过 24 小时的上传文件
                if let Ok(entries) = std::fs::read_dir("data/uploads") {
                    let threshold = std::time::Duration::from_secs(24 * 3600);
                    for entry in entries.flatten() {
                        if let Ok(meta) = entry.metadata() {
                            let age = meta
                                .modified()
                                .ok()
                                .and_then(|t| t.elapsed().ok())
                                .unwrap_or_default();
                            if age > threshold {
                                let _ = std::fs::remove_file(entry.path());
                                tracing::info!("清理过期上传文件: {:?}", entry.file_name());
                            }
                        }
                    }
                }
            }
        });
    }
}

use crate::patent::SearchType;

pub(crate) fn parse_search_type(search_type: Option<&str>) -> Option<SearchType> {
    search_type.map(|t| match t {
        "applicant" => SearchType::Applicant,
        "inventor" => SearchType::Inventor,
        "patent_number" => SearchType::PatentNumber,
        "keyword" => SearchType::Keyword,
        _ => SearchType::Mixed,
    })
}

pub(crate) fn build_online_query(
    query: &str,
    search_type: Option<&SearchType>,
    date_from: Option<&str>,
    date_to: Option<&str>,
) -> String {
    let q = query.trim().replace('"', "");
    let mut search_query = match search_type {
        Some(SearchType::Applicant) => format!("assignee:\"{}\"", q),
        Some(SearchType::Inventor) => format!("inventor:\"{}\"", q),
        Some(SearchType::PatentNumber) => {
            // For Chinese application numbers (e.g. "CN202420009882.7" or "202210835143.9"),
            // Google Patents indexes by PUBLICATION number, not application number.
            // CN application number format: YYYYMMNNNNNN.X (12 digits + check digit)
            let digits: String = q.chars().filter(|c| c.is_ascii_digit()).collect();
            let has_dot = q.contains('.');
            let is_cn_app = digits.len() >= 10
                && digits.len() <= 15
                && (q.chars().all(|c| c.is_ascii_digit() || c == '.')
                    || (q.starts_with("CN") && q.contains('.')));
            if is_cn_app {
                // If the original query has a dot (e.g. "202210835143.9"),
                // strip the check digit after dot → use 12-digit core number.
                // If no dot but 13 digits, the last digit is likely the check digit.
                let core = if has_dot {
                    // Take only digits before the dot position
                    let dot_pos = q.find('.').unwrap_or(q.len());
                    let pre_dot: String = q[..dot_pos]
                        .chars()
                        .filter(|c| c.is_ascii_digit())
                        .collect();
                    pre_dot
                } else if digits.len() == 13 {
                    // 13 digits without dot: last digit is check digit
                    digits[..12].to_string()
                } else {
                    digits
                };
                // 同时保留原始申请号与核心位数，提升 SerpAPI 在不同索引形态下的命中率
                format!("\"{}\" OR \"{}\"", q, core)
            } else {
                format!("\"{}\"", q)
            }
        }
        _ => q,
    };
    if let Some(from) = date_from {
        if !from.is_empty() {
            search_query.push_str(&format!(" after:{from}"));
        }
    }
    if let Some(to) = date_to {
        if !to.is_empty() {
            search_query.push_str(&format!(" before:{to}"));
        }
    }
    search_query
}

/// HTML-escape to prevent XSS in template interpolation.
pub(crate) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Escape a CSV field.
pub(crate) fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Recursively extract a named field from a JSON value (for EPO responses).
pub(crate) fn efld(json: &serde_json::Value, field: &str) -> String {
    if let Some(obj) = json.as_object() {
        for (k, v) in obj {
            if k == field {
                return match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Array(a) => a
                        .iter()
                        .filter_map(|x| x.as_str().or_else(|| x["$"].as_str()))
                        .collect::<Vec<_>>()
                        .join(", "),
                    _ => v.to_string(),
                };
            }
            let r = efld(v, field);
            if !r.is_empty() {
                return r;
            }
        }
    } else if let Some(arr) = json.as_array() {
        for v in arr {
            let r = efld(v, field);
            if !r.is_empty() {
                return r;
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::build_online_query;
    use crate::patent::SearchType;
    use crate::routes::AppConfig;

    #[test]
    fn online_query_uses_applicant_scope() {
        let q = build_online_query("Alice Zhang", Some(&SearchType::Applicant), None, None);
        assert_eq!(q, "assignee:\"Alice Zhang\"");
    }

    #[test]
    fn online_query_uses_inventor_scope_and_dates() {
        let q = build_online_query(
            "Alice Zhang",
            Some(&SearchType::Inventor),
            Some("2024-01-01"),
            Some("2024-12-31"),
        );
        assert_eq!(
            q,
            "inventor:\"Alice Zhang\" after:2024-01-01 before:2024-12-31"
        );
    }

    /// 验证非 Google 服务商不会启用 Gemini CLI 模式
    /// 这是对赌的核心保护——防止未来代码修改 reintroduce 此 bug
    #[test]
    fn non_gemini_provider_does_not_enable_gemini_cli() {
        // 模拟 DeepSeek 配置（仅设置关键字段，其余用默认值）
        let config = AppConfig {
            ai_base_url: "https://api.deepseek.com/v1".into(),
            ai_model: "deepseek-chat".into(),
            ai_api_key_deepseek: "test-key".into(),
            gemini_cli_enabled: true, // 即使 .env 中有此标记…
            gemini_cli_path: "gemini.cmd".into(),
            ..AppConfig::default()
        };

        let client = config.ai_client();
        // 断言：client 的 provider_mode 是 Http，不是 GeminiCli
        assert!(
            !client.is_gemini_cli_mode(),
            "DeepSeek provider should NOT enable Gemini CLI mode, even when gemini_cli_enabled=true"
        );
    }

    /// 验证 Gemini 服务商 + CLI 标记时确实启用 Gemini CLI 模式（正常功能不受影响）
    #[test]
    fn gemini_provider_with_cli_enables_gemini_cli() {
        let config = AppConfig {
            ai_base_url: "https://generativelanguage.googleapis.com/v1beta/openai/".into(),
            ai_model: "gemini-2.0-flash".into(),
            ai_api_key_gemini: "test-key".into(),
            gemini_cli_enabled: true,
            gemini_cli_path: "gemini.cmd".into(),
            ..AppConfig::default()
        };

        let client = config.ai_client();
        assert!(
            client.is_gemini_cli_mode(),
            "Gemini provider with gemini_cli_enabled=true SHOULD enable CLI mode"
        );
    }

    /// 验证单次 AI 调用的全局超时上限为 60 秒。
    #[test]
    fn global_timeout_is_60_seconds() {
        assert_eq!(
            crate::ai::AiClient::GLOBAL_TIMEOUT_SECS,
            60,
            "Global timeout must cap every single AI call at 60 seconds"
        );
    }
}
