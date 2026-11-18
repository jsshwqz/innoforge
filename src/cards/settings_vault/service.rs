//! 卡片 ⑦：系统配置与知识金库核心服务（`src/cards/settings_vault/service.rs`）
//!
//! 严守工程红线：
//! - 零 `unwrap()` / `expect()`；
//! - 零 `reqwest::Client::new()`（统一使用 `Client::builder()` 配显式超时）；
//! - 零裸字节切片（全部使用 `.chars()` / `.get()` 等 UTF-8 安全方法）；
//! - 密钥回显一律经字符级掩码处理，严禁回显明文 Key。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::app::{resolve_bind_ip, AuthGuardConfig};
use crate::core::runtime::llm_router::{
    builtin_nine_providers, ollama_local_provider, LlmProviderSpec,
};
use crate::core::vault::models::{AssetRole, DocKind, FilingStatus, KnowledgeDoc};
use crate::core::vault::VaultService;
use crate::routes::{is_placeholder_api_key, AppConfig, AppState};

use super::prompts::{
    FILING_STATUS_POLICY_NOTE, HEALTH_DIAGNOSTIC_GUIDE, PACK_FIELD_MAPPING_GUIDE,
};

// ── 请求 / 响应结构体（统一派生 Debug, Clone, Serialize, Deserialize） ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderKeyUpdate {
    pub id: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsConfigUpdateReq {
    #[serde(default)]
    pub active_provider: Option<String>,
    #[serde(default)]
    pub ai_base_url: Option<String>,
    #[serde(default)]
    pub ai_model: Option<String>,
    #[serde(default)]
    pub ai_model_expert: Option<String>,
    #[serde(default)]
    pub ai_model_fast: Option<String>,
    #[serde(default)]
    pub ai_api_key: Option<String>,
    #[serde(default)]
    pub provider_keys: Option<Vec<ProviderKeyUpdate>>,
    #[serde(default)]
    pub failover_order: Option<Vec<String>>,
    #[serde(default)]
    pub vision_provider_id: Option<String>,
    #[serde(default)]
    pub daily_budget: Option<usize>,
    #[serde(default)]
    pub privacy_offline_only: Option<bool>,
    #[serde(default)]
    pub umi_ocr_url: Option<String>,
    #[serde(default)]
    pub serpapi_keys: Option<Vec<String>>,
    #[serde(default)]
    pub epo_key: Option<String>,
    #[serde(default)]
    pub epo_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeProviderReq {
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultImportPackReq {
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub default_doc_kind: Option<String>,
    #[serde(default)]
    pub default_asset_role: Option<String>,
    #[serde(default)]
    pub default_filing_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultDocsQuery {
    #[serde(default)]
    pub asset_role: Option<String>,
    #[serde(default)]
    pub doc_kind: Option<String>,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultPatchDocReq {
    pub doc_id: String,
    pub asset_role: String,
    pub filing_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultDeleteDocQuery {
    pub doc_id: String,
}

// ── UTF-8 安全掩码与服务商映射辅助 ──

/// 按字符数安全掩码密钥，绝不在字节边界切片
pub fn mask_api_key(key: &str) -> String {
    let trimmed = key.trim();
    if is_placeholder_api_key(trimmed) {
        return String::new();
    }
    let count = trimmed.chars().count();
    if count <= 8 {
        return "****".to_string();
    }
    let prefix: String = trimmed.chars().take(4).collect();
    let suffix: String = trimmed.chars().skip(count - 4).collect();
    format!("{prefix}****{suffix}")
}

fn provider_db_key_for_id(id: &str) -> Option<&'static str> {
    match id.trim().to_ascii_lowercase().as_str() {
        "deepseek" => Some("AI_API_KEY_DEEPSEEK"),
        "sensenova" | "sensetime" => Some("AI_API_KEY_SENSENOVA"),
        "zhipu" => Some("AI_API_KEY_ZHIPU"),
        "qwen" => Some("AI_API_KEY_QWEN"),
        "openai" => Some("AI_API_KEY_OPENAI"),
        "anthropic" => Some("AI_API_KEY_ANTHROPIC"),
        "gemini" => Some("AI_API_KEY_GEMINI"),
        "openrouter" => Some("AI_API_KEY_OPENROUTER"),
        "xiaomi" => Some("AI_API_KEY_XIAOMI"),
        "custom" => Some("AI_API_KEY"),
        _ => None,
    }
}

fn config_key_for_provider_id(cfg: &AppConfig, id: &str) -> String {
    match id.trim().to_ascii_lowercase().as_str() {
        "deepseek" => cfg.ai_api_key_deepseek.clone(),
        "sensenova" | "sensetime" => cfg.ai_api_key_sensetime.clone(),
        "zhipu" => cfg.ai_api_key_zhipu.clone(),
        "qwen" => cfg.ai_api_key_qwen.clone(),
        "openai" => cfg.ai_api_key_openai.clone(),
        "anthropic" => cfg.ai_api_key_anthropic.clone(),
        "gemini" => cfg.ai_api_key_gemini.clone(),
        "openrouter" => cfg.ai_api_key_openrouter.clone(),
        "xiaomi" => cfg.ai_api_key_xiaomi.clone(),
        _ => cfg.ai_api_key.clone(),
    }
}

fn infer_provider_id_from_url(base_url: &str) -> String {
    let lower = base_url.trim().to_ascii_lowercase();
    if lower.contains("deepseek") {
        "deepseek".to_string()
    } else if lower.contains("sensenova") {
        "sensenova".to_string()
    } else if lower.contains("bigmodel") {
        "zhipu".to_string()
    } else if lower.contains("dashscope") || lower.contains("qwen") {
        "qwen".to_string()
    } else if lower.contains("api.openai.com") {
        "openai".to_string()
    } else if lower.contains("anthropic") {
        "anthropic".to_string()
    } else if lower.contains("googleapis") {
        "gemini".to_string()
    } else if lower.contains("openrouter") {
        "openrouter".to_string()
    } else if lower.contains("xiaomimimo") {
        "xiaomi".to_string()
    } else if lower.contains("11434") || lower.contains("127.0.0.1") || lower.contains("localhost")
    {
        "ollama".to_string()
    } else {
        "custom".to_string()
    }
}

fn build_provider_specs_with_keys(
    cfg: &AppConfig,
    db_settings: &HashMap<String, String>,
) -> Vec<LlmProviderSpec> {
    let mut specs: Vec<LlmProviderSpec> = builtin_nine_providers()
        .into_iter()
        .map(|mut spec| {
            let db_k = provider_db_key_for_id(&spec.id)
                .and_then(|k| db_settings.get(k))
                .cloned()
                .unwrap_or_default();
            let raw = if !db_k.trim().is_empty() {
                db_k
            } else {
                config_key_for_provider_id(cfg, &spec.id)
            };
            spec.api_key = if is_placeholder_api_key(&raw) {
                String::new()
            } else {
                raw
            };
            spec
        })
        .collect();

    let active_id = infer_provider_id_from_url(&cfg.ai_base_url);
    let ollama_model = if active_id == "ollama" && !cfg.ai_model.trim().is_empty() {
        Some(cfg.ai_model.as_str())
    } else {
        None
    };
    specs.push(ollama_local_provider(ollama_model, false));
    specs
}

fn is_valid_ascii_key(key: &str) -> bool {
    key.chars()
        .all(|c| c.is_ascii() && !c.is_ascii_control() && !c.is_whitespace())
}

// ── 1. 首启四维环境健康快速自检（纯本机无阻塞毫秒级） ──

pub fn check_environment_health(state: &AppState) -> Value {
    let start = Instant::now();

    // ① SQLite WAL 与 Schema v26 状态
    let (wal_mode, busy_timeout_ms, schema_version, doc_count, exp_count, atom_count) = {
        let conn = state.db.conn();
        let wal: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap_or_else(|_| "unknown".to_string());
        let busy: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
            .unwrap_or(0);
        let ver: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let docs: i64 = conn
            .query_row("SELECT COUNT(*) FROM knowledge_docs", [], |r| r.get(0))
            .unwrap_or(0);
        let exps: i64 = conn
            .query_row("SELECT COUNT(*) FROM experiment_records", [], |r| r.get(0))
            .unwrap_or(0);
        let atoms: i64 = conn
            .query_row("SELECT COUNT(*) FROM feature_atoms", [], |r| r.get(0))
            .unwrap_or(0);
        (wal, busy, ver, docs, exps, atoms)
    };
    let sqlite_healthy = wal_mode.eq_ignore_ascii_case("wal") && schema_version >= 26;

    // ② 网络绑定安全状态
    let bind_ip = resolve_bind_ip();
    let auth = AuthGuardConfig::from_bind_and_env(bind_ip);
    let network_healthy = bind_ip.is_loopback() || auth.enforce_auth;

    // ③ 9 家 AI 服务商容灾链与看图模型就绪度
    let cfg = state
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let db_settings = state.db.get_all_settings().unwrap_or_default();
    let providers = build_provider_specs_with_keys(&cfg, &db_settings);

    let remote_usable_count = providers
        .iter()
        .filter(|p| p.id != "ollama" && p.is_usable())
        .count();
    let vision_ready_count = providers
        .iter()
        .filter(|p| p.is_usable() && p.supports_vision)
        .count();
    let active_provider = infer_provider_id_from_url(&cfg.ai_base_url);
    let ai_healthy = remote_usable_count > 0 || active_provider == "ollama";

    // ④ 四维检索凭据与三级 OCR 状态
    let privacy_offline_only = db_settings
        .get("PRIVACY_OFFLINE_ONLY")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);
    let umi_ocr_url = db_settings
        .get("UMI_OCR_URL")
        .cloned()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| std::env::var("UMI_OCR_URL").ok())
        .unwrap_or_else(|| "http://127.0.0.1:1224".to_string());
    let epo_configured = !cfg.epo_key.trim().is_empty() && !cfg.epo_secret.trim().is_empty();
    let serpapi_count = cfg.serpapi_keys.len();

    let overall_healthy = sqlite_healthy && network_healthy && ai_healthy;
    let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(0);

    json!({
        "status": "ok",
        "overall_healthy": overall_healthy,
        "overall_level": if overall_healthy { "green" } else if sqlite_healthy && network_healthy { "yellow" } else { "red" },
        "elapsed_ms": elapsed_ms,
        "guide": HEALTH_DIAGNOSTIC_GUIDE,
        "dimensions": {
            "sqlite": {
                "healthy": sqlite_healthy,
                "journal_mode": wal_mode,
                "busy_timeout_ms": busy_timeout_ms,
                "schema_version": schema_version,
                "expected_version": 26,
                "knowledge_docs_count": doc_count,
                "experiment_records_count": exp_count,
                "feature_atoms_count": atom_count
            },
            "network_security": {
                "healthy": network_healthy,
                "bind_ip": bind_ip.to_string(),
                "listen_port": state.listen_port,
                "is_loopback": bind_ip.is_loopback(),
                "auth_enforced": auth.enforce_auth,
                "label": if bind_ip.is_loopback() {
                    "本机回环安全监听 (127.0.0.1)"
                } else if auth.enforce_auth {
                    "非回环监听已强制开启同源/Token鉴权"
                } else {
                    "非回环监听未开启鉴权（高危）"
                }
            },
            "ai_failover_vision": {
                "healthy": ai_healthy,
                "active_provider": active_provider,
                "remote_usable_count": remote_usable_count,
                "total_builtin_providers": 9,
                "vision_ready_count": vision_ready_count,
                "vision_ready": vision_ready_count > 0,
                "local_ollama_configured": active_provider == "ollama"
            },
            "omni_search_and_ocr": {
                "healthy": true,
                "free_scholar_sources": ["OpenAlex", "Crossref", "arXiv", "Semantic Scholar"],
                "free_scholar_ready": true,
                "serpapi_key_count": serpapi_count,
                "epo_configured": epo_configured,
                "ocr_tier1_native_pdf": true,
                "ocr_tier2_umi_url": umi_ocr_url,
                "ocr_tier3_vision_fallback": vision_ready_count > 0 && !privacy_offline_only,
                "privacy_offline_only": privacy_offline_only
            }
        }
    })
}

// ── 2. 配置脱敏快照与保存 ──

pub fn get_masked_config(state: &AppState) -> Value {
    let cfg = state
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let db_settings = state.db.get_all_settings().unwrap_or_default();
    let specs = build_provider_specs_with_keys(&cfg, &db_settings);

    let providers_json: Vec<Value> = specs
        .iter()
        .map(|p| {
            let masked = mask_api_key(&p.api_key);
            json!({
                "id": p.id,
                "display_name": p.display_name,
                "base_url": p.base_url,
                "default_model": p.default_model,
                "expert_model": p.expert_model,
                "vision_model": p.vision_model,
                "supports_vision": p.supports_vision,
                "max_context_tokens": p.max_context_tokens,
                "api_key_masked": masked,
                "configured": p.is_usable()
            })
        })
        .collect();

    let default_order: Vec<String> = builtin_nine_providers().into_iter().map(|p| p.id).collect();
    let failover_order: Vec<String> = db_settings
        .get("LLM_FAILOVER_ORDER")
        .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
        .filter(|v| !v.is_empty())
        .unwrap_or(default_order);

    let daily_budget = db_settings
        .get("AI_DAILY_BUDGET")
        .or_else(|| db_settings.get("SEARCH_LLM_DAILY_BUDGET"))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(200);

    let privacy_offline_only = db_settings
        .get("PRIVACY_OFFLINE_ONLY")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);

    let umi_ocr_url = db_settings
        .get("UMI_OCR_URL")
        .cloned()
        .unwrap_or_else(|| "http://127.0.0.1:1224".to_string());

    let vision_provider_id = db_settings
        .get("LLM_VISION_PROVIDER")
        .cloned()
        .unwrap_or_default();

    let serpapi_keys_masked: Vec<String> =
        cfg.serpapi_keys.iter().map(|k| mask_api_key(k)).collect();

    json!({
        "status": "ok",
        "active_provider": infer_provider_id_from_url(&cfg.ai_base_url),
        "ai_base_url": cfg.ai_base_url,
        "ai_model": cfg.ai_model,
        "ai_model_expert": cfg.ai_model_expert,
        "ai_model_fast": cfg.ai_model_fast,
        "vision_provider_id": vision_provider_id,
        "failover_order": failover_order,
        "daily_budget": daily_budget,
        "privacy_offline_only": privacy_offline_only,
        "umi_ocr_url": umi_ocr_url,
        "serpapi_keys_masked": serpapi_keys_masked,
        "epo_key_masked": mask_api_key(&cfg.epo_key),
        "epo_secret_configured": !cfg.epo_secret.trim().is_empty(),
        "providers": providers_json
    })
}

pub fn save_config(state: &AppState, req: &SettingsConfigUpdateReq) -> Result<Value> {
    let mut pairs: Vec<(String, String)> = Vec::new();

    if let Some(ref base_url) = req.ai_base_url {
        let trimmed = base_url.trim();
        if !trimmed.is_empty() {
            pairs.push(("AI_BASE_URL".to_string(), trimmed.to_string()));
        }
    } else if let Some(ref active) = req.active_provider {
        if let Some(found) = builtin_nine_providers()
            .into_iter()
            .find(|p| p.id == active.trim())
        {
            pairs.push(("AI_BASE_URL".to_string(), found.base_url));
        } else if active.trim() == "ollama" {
            pairs.push((
                "AI_BASE_URL".to_string(),
                "http://127.0.0.1:11434/v1".to_string(),
            ));
        }
    }

    if let Some(ref model) = req.ai_model {
        if !model.trim().is_empty() {
            pairs.push(("AI_MODEL".to_string(), model.trim().to_string()));
        }
    }
    if let Some(ref expert) = req.ai_model_expert {
        if !expert.trim().is_empty() {
            pairs.push(("AI_MODEL_EXPERT".to_string(), expert.trim().to_string()));
        }
    }
    if let Some(ref fast) = req.ai_model_fast {
        if !fast.trim().is_empty() {
            pairs.push(("AI_MODEL_FAST".to_string(), fast.trim().to_string()));
        }
    }

    if let Some(ref raw_key) = req.ai_api_key {
        let trimmed = raw_key.trim();
        if !trimmed.is_empty() && !trimmed.contains("****") {
            if !is_valid_ascii_key(trimmed) {
                return Err(anyhow!(
                    "AI API Key 包含非法空白或非 ASCII 字符 / Invalid characters in API Key"
                ));
            }
            pairs.push(("AI_API_KEY".to_string(), trimmed.to_string()));
        }
    }

    if let Some(ref pkeys) = req.provider_keys {
        for item in pkeys {
            if let Some(db_key) = provider_db_key_for_id(&item.id) {
                if let Some(ref raw_val) = item.api_key {
                    let trimmed = raw_val.trim();
                    if trimmed.contains("****") {
                        continue;
                    }
                    if !trimmed.is_empty() && !is_valid_ascii_key(trimmed) {
                        return Err(anyhow!(
                            "服务商 {} 的 API Key 含非法字符 / Invalid key for provider {}",
                            item.id,
                            item.id
                        ));
                    }
                    pairs.push((db_key.to_string(), trimmed.to_string()));
                }
            }
        }
    }

    if let Some(ref order) = req.failover_order {
        let cleaned: Vec<String> = order
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !cleaned.is_empty() {
            pairs.push((
                "LLM_FAILOVER_ORDER".to_string(),
                serde_json::to_string(&cleaned)?,
            ));
        }
    }

    if let Some(ref vp) = req.vision_provider_id {
        pairs.push(("LLM_VISION_PROVIDER".to_string(), vp.trim().to_string()));
    }

    if let Some(budget) = req.daily_budget {
        if !(1..=10_000).contains(&budget) {
            return Err(anyhow!(
                "AI 日预算须在 1~10000 之间 / Daily budget must be between 1 and 10000"
            ));
        }
        pairs.push(("AI_DAILY_BUDGET".to_string(), budget.to_string()));
        pairs.push(("SEARCH_LLM_DAILY_BUDGET".to_string(), budget.to_string()));
    }

    if let Some(offline) = req.privacy_offline_only {
        pairs.push((
            "PRIVACY_OFFLINE_ONLY".to_string(),
            if offline { "true" } else { "false" }.to_string(),
        ));
    }

    if let Some(ref umi) = req.umi_ocr_url {
        pairs.push(("UMI_OCR_URL".to_string(), umi.trim().to_string()));
    }

    if let Some(ref serp_keys) = req.serpapi_keys {
        for idx in 1..=5 {
            let db_k = format!("SERPAPI_KEY_{idx}");
            if let Some(raw) = serp_keys.get(idx - 1) {
                let trimmed = raw.trim();
                if trimmed.contains("****") {
                    continue;
                }
                if !trimmed.is_empty() && !is_valid_ascii_key(trimmed) {
                    return Err(anyhow!("SerpAPI Key {idx} 含非法字符"));
                }
                pairs.push((db_k, trimmed.to_string()));
            } else {
                pairs.push((db_k, String::new()));
            }
        }
    }

    if let Some(ref epo_k) = req.epo_key {
        let trimmed = epo_k.trim();
        if !trimmed.contains("****") {
            pairs.push(("EPO_KEY".to_string(), trimmed.to_string()));
        }
    }
    if let Some(ref epo_s) = req.epo_secret {
        let trimmed = epo_s.trim();
        if !trimmed.contains("****") && !trimmed.is_empty() {
            pairs.push(("EPO_SECRET".to_string(), trimmed.to_string()));
        }
    }

    if !pairs.is_empty() {
        let refs: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        state
            .db
            .set_settings_batch(&refs)
            .context("保存配置到 SQLite 失败 / Failed to persist settings")?;
    }

    // 刷新内存配置快照
    let reloaded = AppConfig::from_db_and_env(Some(&state.db));
    {
        let mut guard = state.config.write().unwrap_or_else(|e| e.into_inner());
        *guard = reloaded;
    }

    Ok(get_masked_config(state))
}

// ── 3. 服务商探活（3 秒短超时异步探测） ──

pub async fn probe_provider(state: &AppState, req: &ProbeProviderReq) -> Value {
    let start = Instant::now();
    let cfg = state
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let db_settings = state.db.get_all_settings().unwrap_or_default();
    let specs = build_provider_specs_with_keys(&cfg, &db_settings);

    let req_id = req
        .provider_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let matched_spec = req_id.and_then(|id| specs.iter().find(|p| p.id == id));

    let base_url = req
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| matched_spec.map(|s| s.base_url.clone()))
        .unwrap_or_else(|| cfg.ai_base_url.clone());

    let api_key = req
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.contains("****"))
        .map(str::to_string)
        .or_else(|| matched_spec.map(|s| s.api_key.clone()))
        .unwrap_or_else(|| cfg.api_key_for_provider(&base_url));

    let provider_id = req_id
        .map(str::to_string)
        .unwrap_or_else(|| infer_provider_id_from_url(&base_url));

    if provider_id != "ollama" && is_placeholder_api_key(&api_key) {
        return json!({
            "status": "ok",
            "reachable": false,
            "provider_id": provider_id,
            "latency_ms": 0,
            "models": [],
            "message": "未配置有效 API Key / API key not configured"
        });
    }

    let client = match reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(800))
        .timeout(Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return json!({
                "status": "error",
                "reachable": false,
                "provider_id": provider_id,
                "message": format!("HTTP 客户端初始化失败: {e}")
            });
        }
    };

    let endpoint = format!("{}/models", base_url.trim_end_matches('/'));
    let mut request = client.get(&endpoint);
    if provider_id == "anthropic" {
        request = request
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01");
    } else if !api_key.is_empty() {
        request = request.bearer_auth(&api_key);
    }

    match request.send().await {
        Ok(resp) => {
            let code = resp.status().as_u16();
            let ok = resp.status().is_success();
            let mut models: Vec<String> = Vec::new();
            if ok {
                if let Ok(body) = resp.json::<Value>().await {
                    if let Some(arr) = body.get("data").and_then(Value::as_array) {
                        for item in arr.iter().take(30) {
                            if let Some(id) = item.get("id").and_then(Value::as_str) {
                                models.push(id.to_string());
                            }
                        }
                    }
                }
            }
            let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(0);
            json!({
                "status": "ok",
                "reachable": ok,
                "http_status": code,
                "provider_id": provider_id,
                "latency_ms": latency_ms,
                "models": models,
                "message": if ok {
                    format!("探活成功 ({latency_ms} ms)")
                } else {
                    format!("上游返回 HTTP {code}")
                }
            })
        }
        Err(e) => {
            let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(0);
            json!({
                "status": "ok",
                "reachable": false,
                "http_status": 0,
                "provider_id": provider_id,
                "latency_ms": latency_ms,
                "models": [],
                "message": format!("连接失败或超时 (3s): {e}")
            })
        }
    }
}

// ── 4. 免 Python 原生公开文献/专利数据包流式灌库（闭环 #67） ──

fn detect_pack_format(hint: Option<&str>, filename: Option<&str>, content: &str) -> &'static str {
    if let Some(h) = hint.map(|s| s.trim().to_ascii_lowercase()) {
        match h.as_str() {
            "json" | "json_array" => return "json_array",
            "jsonl" | "ndjson" => return "jsonl",
            "csv" => return "csv",
            "bib" | "bibtex" => return "bibtex",
            _ => {}
        }
    }
    if let Some(fname) = filename.map(|s| s.trim().to_ascii_lowercase()) {
        if fname.ends_with(".jsonl") || fname.ends_with(".ndjson") {
            return "jsonl";
        }
        if fname.ends_with(".json") {
            return "json_array";
        }
        if fname.ends_with(".csv") {
            return "csv";
        }
        if fname.ends_with(".bib") {
            return "bibtex";
        }
    }
    let trimmed = content.trim_start();
    if trimmed.starts_with('[') {
        "json_array"
    } else if trimmed.starts_with('@') {
        "bibtex"
    } else if trimmed.starts_with('{') {
        // Distinguish single JSON wrapper object vs JSONL
        if serde_json::from_str::<Value>(trimmed).is_ok() {
            "json_array"
        } else {
            "jsonl"
        }
    } else {
        "csv"
    }
}

fn parse_csv_row(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cur.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == ',' {
            fields.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    fields.push(cur.trim().to_string());
    fields
}

fn parse_csv_records(content: &str) -> Vec<HashMap<String, String>> {
    let mut lines = content.lines().filter(|l| !l.trim().is_empty());
    let Some(header_line) = lines.next() else {
        return Vec::new();
    };
    let headers: Vec<String> = parse_csv_row(header_line)
        .into_iter()
        .map(|h| h.trim_start_matches('\u{feff}').to_ascii_lowercase())
        .collect();
    let mut out = Vec::new();
    for line in lines {
        let cols = parse_csv_row(line);
        let mut map = HashMap::new();
        for (i, h) in headers.iter().enumerate() {
            if let Some(val) = cols.get(i) {
                if !val.is_empty() {
                    map.insert(h.clone(), val.clone());
                }
            }
        }
        if !map.is_empty() {
            out.push(map);
        }
    }
    out
}

fn parse_bibtex_records(content: &str) -> Vec<HashMap<String, String>> {
    let mut entries = Vec::new();
    let mut current_raw = String::new();
    let mut brace_depth: i32 = 0;
    let mut in_entry = false;

    for ch in content.chars() {
        if !in_entry {
            if ch == '@' {
                in_entry = true;
                brace_depth = 0;
                current_raw.clear();
                current_raw.push(ch);
            }
        } else {
            current_raw.push(ch);
            if ch == '{' {
                brace_depth += 1;
            } else if ch == '}' {
                brace_depth -= 1;
                if brace_depth <= 0 {
                    if let Some(parsed) = parse_single_bibtex_entry(&current_raw) {
                        entries.push(parsed);
                    }
                    in_entry = false;
                }
            }
        }
    }
    entries
}

fn parse_single_bibtex_entry(raw: &str) -> Option<HashMap<String, String>> {
    let open_idx = raw.find('{')?;
    let entry_type = raw.get(1..open_idx)?.trim().to_ascii_lowercase();
    let inner = raw.get(open_idx + 1..raw.len().saturating_sub(1))?;
    let comma_idx = inner.find(',')?;
    let cite_key = inner.get(0..comma_idx)?.trim().to_string();
    let body = inner.get(comma_idx + 1..)?;

    let mut map = HashMap::new();
    map.insert("bibtex_raw".to_string(), raw.trim().to_string());
    if !cite_key.is_empty() {
        map.insert("cite_key".to_string(), cite_key);
    }
    let kind = match entry_type.as_str() {
        "inproceedings" | "conference" => "conf_paper",
        "misc" | "techreport" => "preprint",
        "patent" => "patent",
        _ => "journal_paper",
    };
    map.insert("doc_kind".to_string(), kind.to_string());

    // Parse key = {value} or key = "value" or key = 2024
    let mut rest = body.trim();
    while !rest.is_empty() {
        let Some(eq_idx) = rest.find('=') else {
            break;
        };
        let field_name = rest
            .get(0..eq_idx)
            .unwrap_or("")
            .trim()
            .trim_start_matches(',')
            .trim()
            .to_ascii_lowercase();
        rest = rest.get(eq_idx + 1..).unwrap_or("").trim_start();
        let (val, consumed) = extract_bibtex_value(rest);
        if !field_name.is_empty() && !val.is_empty() {
            map.insert(field_name, val);
        }
        rest = rest.get(consumed..).unwrap_or("").trim_start();
        if rest.starts_with(',') {
            rest = rest.get(1..).unwrap_or("").trim_start();
        }
    }
    Some(map)
}

fn extract_bibtex_value(s: &str) -> (String, usize) {
    let mut chars = s.char_indices();
    let Some((_, first)) = chars.next() else {
        return (String::new(), 0);
    };
    if first == '{' {
        let mut depth = 1i32;
        for (idx, c) in chars {
            if c == '{' {
                depth += 1;
            } else if c == '}' {
                depth -= 1;
                if depth == 0 {
                    let val = s.get(1..idx).unwrap_or("").trim().to_string();
                    return (val, idx + c.len_utf8());
                }
            }
        }
        (s.trim().to_string(), s.len())
    } else if first == '"' {
        for (idx, c) in chars {
            if c == '"' {
                let val = s.get(1..idx).unwrap_or("").trim().to_string();
                return (val, idx + c.len_utf8());
            }
        }
        (s.trim().to_string(), s.len())
    } else {
        let end = s.find(',').unwrap_or(s.len());
        let val = s.get(0..end).unwrap_or("").trim().to_string();
        (val, end)
    }
}

fn pick_str(map: &HashMap<String, String>, keys: &[&str]) -> String {
    for k in keys {
        if let Some(v) = map.get(*k) {
            let t = v.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
    }
    String::new()
}

fn json_obj_to_map(obj: &serde_json::Map<String, Value>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (k, v) in obj {
        let key = k.trim().to_ascii_lowercase();
        let val = match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Array(_) | Value::Object(_) => serde_json::to_string(v).unwrap_or_default(),
            Value::Null => String::new(),
        };
        if !val.trim().is_empty() {
            map.insert(key, val);
        }
    }
    map
}

fn map_to_knowledge_doc(
    map: &HashMap<String, String>,
    default_kind: DocKind,
    default_role: AssetRole,
    default_filing: FilingStatus,
    idx: usize,
) -> KnowledgeDoc {
    let external_id = pick_str(
        map,
        &[
            "external_id",
            "patent_number",
            "publication_number",
            "doi",
            "arxiv_id",
            "cite_key",
            "id",
        ],
    );
    let raw_doc_id = pick_str(map, &["doc_id"]);
    let doc_id = if !raw_doc_id.is_empty() {
        raw_doc_id
    } else if !external_id.is_empty() {
        format!(
            "pack_{}",
            external_id
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect::<String>()
        )
    } else {
        format!("pack_{}_{idx}", uuid::Uuid::new_v4().simple())
    };

    let doc_kind = map
        .get("doc_kind")
        .map(|s| DocKind::parse(s))
        .unwrap_or(default_kind);
    let asset_role = map
        .get("asset_role")
        .map(|s| AssetRole::parse(s))
        .unwrap_or(default_role);
    let filing_status = map
        .get("filing_status")
        .map(|s| FilingStatus::parse(s))
        .unwrap_or(default_filing);

    let title_zh = pick_str(map, &["title_zh", "title", "patent_title"]);
    let title_en = pick_str(map, &["title_en"]);
    let authors_or_assignees = pick_str(
        map,
        &[
            "authors_or_assignees",
            "authors",
            "author",
            "inventors",
            "assignee",
            "applicant",
        ],
    );
    let pub_or_filing_date = pick_str(
        map,
        &[
            "pub_or_filing_date",
            "publication_date",
            "filing_date",
            "date",
            "year",
        ],
    );
    let venue_or_authority = pick_str(
        map,
        &[
            "venue_or_authority",
            "journal",
            "booktitle",
            "authority",
            "venue",
            "publisher",
        ],
    );
    let abstract_text = pick_str(map, &["abstract_text", "abstract", "summary"]);
    let claims_json = pick_str(map, &["claims_json", "claims"]);
    let fulltext_raw = pick_str(map, &["fulltext_raw", "fulltext", "description", "body"]);
    let bibtex_raw = pick_str(map, &["bibtex_raw"]);
    let doi_verified = map
        .get("doi_verified")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(!pick_str(map, &["doi"]).is_empty());

    let fulltext_status = if !fulltext_raw.is_empty() || !claims_json.is_empty() {
        "fulltext_ready".to_string()
    } else if !abstract_text.is_empty() {
        "abstract_only".to_string()
    } else {
        "metadata_only".to_string()
    };

    KnowledgeDoc {
        doc_id,
        doc_kind,
        asset_role,
        external_id,
        title_zh,
        title_en,
        authors_or_assignees,
        pub_or_filing_date,
        venue_or_authority,
        filing_status,
        doi_verified,
        bibtex_raw,
        abstract_text,
        claims_json,
        fulltext_raw,
        fulltext_status,
        chunk_ver: 2,
    }
}

pub fn import_data_pack(state: &AppState, req: &VaultImportPackReq) -> Result<Value> {
    let start = Instant::now();
    let trimmed = req.content.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("导入数据包内容为空 / Pack content is empty"));
    }

    let format = detect_pack_format(req.format.as_deref(), req.filename.as_deref(), trimmed);

    let default_kind = req
        .default_doc_kind
        .as_deref()
        .map(DocKind::parse)
        .unwrap_or(DocKind::Patent);
    let default_role = req
        .default_asset_role
        .as_deref()
        .map(AssetRole::parse)
        .unwrap_or(AssetRole::GeneralRef);
    let default_filing = req
        .default_filing_status
        .as_deref()
        .map(FilingStatus::parse)
        .unwrap_or(FilingStatus::PublicDisclosed);

    let mut maps: Vec<HashMap<String, String>> = Vec::new();
    let mut parse_errors: Vec<String> = Vec::new();

    match format {
        "json_array" => {
            let val: Value = serde_json::from_str(trimmed)
                .context("JSON 数据包解析失败 / Failed to parse JSON pack")?;
            let arr = if let Some(a) = val.as_array() {
                a.clone()
            } else if let Some(a) = val.get("records").and_then(Value::as_array) {
                a.clone()
            } else if let Some(a) = val.get("docs").and_then(Value::as_array) {
                a.clone()
            } else if val.is_object() {
                vec![val]
            } else {
                Vec::new()
            };
            for item in arr {
                if let Some(obj) = item.as_object() {
                    maps.push(json_obj_to_map(obj));
                }
            }
        }
        "jsonl" => {
            for (line_no, line) in trimmed.lines().enumerate() {
                let l = line.trim();
                if l.is_empty() {
                    continue;
                }
                match serde_json::from_str::<Value>(l) {
                    Ok(Value::Object(obj)) => maps.push(json_obj_to_map(&obj)),
                    Ok(_) => {
                        parse_errors.push(format!("Line {}: expected JSON object", line_no + 1))
                    }
                    Err(e) => parse_errors.push(format!("Line {}: {e}", line_no + 1)),
                }
            }
        }
        "csv" => {
            maps = parse_csv_records(trimmed);
        }
        "bibtex" => {
            maps = parse_bibtex_records(trimmed);
        }
        _ => {}
    }

    if maps.is_empty() {
        return Err(anyhow!(
            "未能从 {} 格式数据包中解析出有效文献记录 / No valid records parsed from {} pack",
            format,
            format
        ));
    }

    let mut imported_count = 0usize;
    let mut skipped_count = 0usize;
    let mut sample_doc_ids: Vec<String> = Vec::new();

    {
        let conn = state.db.conn();
        VaultService::ensure_tables(&conn)?;
        for (idx, map) in maps.iter().enumerate() {
            let doc = map_to_knowledge_doc(map, default_kind, default_role, default_filing, idx);
            if doc.title_zh.is_empty()
                && doc.title_en.is_empty()
                && doc.abstract_text.is_empty()
                && doc.fulltext_raw.is_empty()
            {
                skipped_count += 1;
                continue;
            }
            match VaultService::upsert_document_with_chunks(&conn, &doc) {
                Ok(()) => {
                    imported_count += 1;
                    if sample_doc_ids.len() < 10 {
                        sample_doc_ids.push(doc.doc_id);
                    }
                }
                Err(e) => {
                    skipped_count += 1;
                    if parse_errors.len() < 10 {
                        parse_errors.push(format!("Record {}: {e}", idx + 1));
                    }
                }
            }
        }
    }

    let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(0);
    Ok(json!({
        "status": "ok",
        "format_detected": format,
        "imported_count": imported_count,
        "skipped_count": skipped_count,
        "total_parsed": maps.len(),
        "sample_doc_ids": sample_doc_ids,
        "errors": parse_errors,
        "elapsed_ms": elapsed_ms,
        "field_mapping_guide": PACK_FIELD_MAPPING_GUIDE
    }))
}

// ── 5. 本机知识金库总表管理 ──

pub fn list_vault_docs(state: &AppState, q: &VaultDocsQuery) -> Value {
    let role_filter = q
        .asset_role
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "all")
        .map(AssetRole::parse);
    let kind_filter = q
        .doc_kind
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "all")
        .map(DocKind::parse);
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0);
    let keyword =
        q.q.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_ascii_lowercase);

    let conn = state.db.conn();
    let _ = VaultService::ensure_tables(&conn);

    let docs = VaultService::list_documents(
        &conn,
        role_filter,
        kind_filter,
        if keyword.is_some() { 200 } else { limit },
        if keyword.is_some() { 0 } else { offset },
    )
    .unwrap_or_default();

    let filtered_docs: Vec<KnowledgeDoc> = if let Some(ref kw) = keyword {
        docs.into_iter()
            .filter(|d| {
                d.doc_id.to_ascii_lowercase().contains(kw)
                    || d.external_id.to_ascii_lowercase().contains(kw)
                    || d.title_zh.to_ascii_lowercase().contains(kw)
                    || d.title_en.to_ascii_lowercase().contains(kw)
                    || d.authors_or_assignees.to_ascii_lowercase().contains(kw)
            })
            .skip(offset)
            .take(limit)
            .collect()
    } else {
        docs
    };

    let total_docs: i64 = conn
        .query_row("SELECT COUNT(*) FROM knowledge_docs", [], |r| r.get(0))
        .unwrap_or(0);
    let own_asset_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_docs WHERE asset_role = 'own_asset'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let experiment_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM experiment_records", [], |r| r.get(0))
        .unwrap_or(0);
    let verified_experiment_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM experiment_records WHERE status IN ('real_data_filled', 'verified')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let feature_atom_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM feature_atoms", [], |r| r.get(0))
        .unwrap_or(0);

    json!({
        "status": "ok",
        "docs": filtered_docs,
        "stats": {
            "total_docs": total_docs,
            "own_asset_count": own_asset_count,
            "experiment_count": experiment_count,
            "verified_experiment_count": verified_experiment_count,
            "feature_atom_count": feature_atom_count
        },
        "policy_note": FILING_STATUS_POLICY_NOTE
    })
}

pub fn patch_vault_doc(state: &AppState, req: &VaultPatchDocReq) -> Result<Value> {
    let doc_id = req.doc_id.trim();
    if doc_id.is_empty() {
        return Err(anyhow!("doc_id 不能为空 / doc_id is required"));
    }
    let role = AssetRole::parse(&req.asset_role);
    let filing = FilingStatus::parse(&req.filing_status);

    let conn = state.db.conn();
    let updated = VaultService::update_doc_role_and_filing(&conn, doc_id, role, filing)?;
    if !updated {
        return Err(anyhow!("未找到文献 {doc_id} / Document not found"));
    }

    Ok(json!({
        "status": "ok",
        "doc_id": doc_id,
        "asset_role": role.as_str(),
        "filing_status": filing.as_str()
    }))
}

pub fn delete_vault_doc(state: &AppState, doc_id: &str) -> Result<Value> {
    let trimmed = doc_id.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("doc_id 不能为空 / doc_id is required"));
    }
    let conn = state.db.conn();
    let deleted = VaultService::delete_document(&conn, trimmed)?;
    if !deleted {
        return Err(anyhow!("未找到文献 {trimmed} / Document not found"));
    }
    Ok(json!({
        "status": "ok",
        "deleted": true,
        "doc_id": trimmed
    }))
}
