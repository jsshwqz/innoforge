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
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// SerpAPI multi-key support (round-robin) — 最多 5 个 Key，自动轮询
    pub serpapi_keys: Vec<String>,
    pub ai_base_url: String,
    pub ai_api_key: String,
    pub ai_model: String,
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

        Self {
            serpapi_keys,
            ai_base_url: get("AI_BASE_URL", "http://localhost:11434/v1"),
            ai_api_key: get("AI_API_KEY", "ollama"),
            ai_model: get("AI_MODEL", "qwen2.5:7b"),
        }
    }

    /// Build an AiClient from the current config.
    /// 仅使用 DeepSeek，无备用 AI 服务商。
    pub fn ai_client(&self) -> AiClient {
        AiClient::with_config(&self.ai_base_url, &self.ai_api_key, &self.ai_model)
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
        tokio::spawn(async move {
            let stale_threshold = std::time::Duration::from_secs(300); // 5 分钟
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;

                // 清理超时的管道通道
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
                drop(ch);

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
                // Bare digits — Google Patents finds this via application_number field
                core
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
}
