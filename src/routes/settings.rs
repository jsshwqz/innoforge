use super::{find_gemini_cli, AppState};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CadSettingsRequest {
    workspace: String,
    auto_start: bool,
}

/// 根据 base_url 返回该服务商对应的 DB/.env Key 名称
fn provider_db_key(base_url: &str) -> &'static str {
    if base_url.contains("qwen") || base_url.contains("dashscope") {
        "AI_API_KEY_QWEN"
    } else if base_url.contains("api.openai.com") {
        "AI_API_KEY_OPENAI"
    } else if base_url.contains("deepseek") {
        "AI_API_KEY_DEEPSEEK"
    } else if base_url.contains("xiaomimimo") {
        "AI_API_KEY_XIAOMI"
    } else if base_url.contains("sensenova") {
        "AI_API_KEY_SENSENOVA"
    } else if base_url.contains("openrouter") {
        "AI_API_KEY_OPENROUTER"
    } else if base_url.contains("googleapis") {
        "AI_API_KEY_GEMINI"
    } else if base_url.contains("bigmodel") {
        "AI_API_KEY_ZHIPU"
    } else {
        "AI_API_KEY"
    }
}

pub async fn api_get_settings(State(s): State<AppState>) -> Json<serde_json::Value> {
    let config = s.config.read().unwrap_or_else(|e| e.into_inner());

    fn mask_api_key(key: &str) -> String {
        if key.is_empty() || key == "your-serpapi-key-here" {
            String::new()
        } else if key.len() <= 8 {
            "****".to_string()
        } else {
            format!("{}****{}", &key[..4], &key[key.len() - 4..])
        }
    }

    let serpapi_keys: Vec<String> = config
        .serpapi_keys
        .iter()
        .map(|k| mask_api_key(k))
        .collect();

    // 返回当前服务商对应的 Key（而非共享 ai_api_key）
    let current_key = config.api_key_for_provider(&config.ai_base_url);

    // 所有服务商的独立 Key（掩码），供前端切换时使用
    let mask = |k: &str| mask_api_key(k);

    // 从 DB 读取原始存储值（不经过 config 的回退逻辑），
    // 让前端能区分"已配置"和"未配置"，避免回退值误导
    let db_settings = s.db.get_all_settings().ok().unwrap_or_default();
    let raw_key = |db_key: &str| -> String { db_settings.get(db_key).cloned().unwrap_or_default() };
    let aioncad_workspace = raw_key("aioncad_workspace");
    let freecad_auto_start = raw_key("freecad_auto_start") != "false";

    // 仅保留 SerpAPI + AI 配置，其他搜索源（Firecrawl/Bing/Lens/CNIPR）和备用 AI 已屏蔽

    Json(json!({
        "serpapi_keys": serpapi_keys,
        "serpapi_key_configured": config.has_serpapi(),
        // EPO OPS 凭证（MA5a）：读的是内存里的 AppConfig —— 与 `/api/search/online`
        // 登记该源时用的是同一份值（`epo_credentials()`），所以这里显示「已配置」
        // 就等于链上真的会带这个源，设置页不会给出与检索侧不一致的假状态。
        // 回显形态与 SerpAPI 完全同构（`前4****后4`），明文 Key 不出后端。
        "epo_key": mask_api_key(&config.epo_key),
        "epo_secret": mask_api_key(&config.epo_secret),
        "epo_key_configured": !config.epo_key.trim().is_empty(),
        "epo_secret_configured": !config.epo_secret.trim().is_empty(),
        "epo_configured": config.epo_credentials().is_some(),
        "ai_base_url": config.ai_base_url,
        "ai_api_key": mask_api_key(&current_key),
        "ai_api_key_configured": !current_key.is_empty(),
        // 各服务商独立 Key（从 DB 读取原始值，不含回退逻辑）
        "ai_api_key_deepseek": mask(&raw_key("AI_API_KEY_DEEPSEEK")),
        "ai_api_key_anthropic": mask(&raw_key("AI_API_KEY_ANTHROPIC")),
        "ai_api_key_xiaomi": mask(&raw_key("AI_API_KEY_XIAOMI")),
        "ai_api_key_sensetime": mask(&raw_key("AI_API_KEY_SENSENOVA")),
        "ai_api_key_openrouter": mask(&raw_key("AI_API_KEY_OPENROUTER")),
        "ai_api_key_gemini": mask(&raw_key("AI_API_KEY_GEMINI")),
        "ai_api_key_zhipu": mask(&raw_key("AI_API_KEY_ZHIPU")),
        "ai_api_key_qwen": mask(&raw_key("AI_API_KEY_QWEN")),
        "ai_api_key_openai": mask(&raw_key("AI_API_KEY_OPENAI")),
        "ai_model": config.ai_model,
        "ai_model_expert": config.ai_model_expert,
        "google_client_id": config.google_client_id.clone(),
        "google_client_id_set": !config.google_client_id.is_empty(),
        "google_client_secret_set": !config.google_client_secret.is_empty(),
        "google_oauth_connected": !config.google_refresh_token.is_empty() || !config.google_access_token.is_empty(),
        "google_auth_mode": config.google_auth_mode,
        "gemini_cli_enabled": config.gemini_cli_enabled,
        "gemini_cli_available": !config.gemini_cli_path.is_empty(),
        "gemini_cli_path": config.gemini_cli_path,
        "aioncad_workspace": aioncad_workspace,
        "freecad_auto_start": freecad_auto_start,
        "freecad_platform_supported": cfg!(target_os = "windows"),
    }))
}

pub async fn api_save_cad_settings(
    State(s): State<AppState>,
    Json(req): Json<CadSettingsRequest>,
) -> Json<serde_json::Value> {
    let workspace = req.workspace.trim();
    if workspace.chars().count() > 1024 {
        return Json(json!({"status": "error", "code": "workspace_too_long"}));
    }

    let resolved_workspace = if workspace.is_empty() {
        None
    } else {
        let path = PathBuf::from(workspace);
        if !path.is_absolute() {
            return Json(json!({"status": "error", "code": "workspace_not_absolute"}));
        }
        let canonical = match path.canonicalize() {
            Ok(path) => path,
            Err(_) => {
                return Json(json!({"status": "error", "code": "workspace_not_found"}));
            }
        };
        if !canonical.join("bootstrap_bridge.ps1").is_file() {
            return Json(json!({"status": "error", "code": "workspace_invalid"}));
        }
        Some(canonical)
    };

    let workspace_value = if resolved_workspace.is_some() {
        workspace.to_string()
    } else {
        String::new()
    };
    let auto_start_value = if req.auto_start { "true" } else { "false" };
    if let Err(error) = s.db.set_settings_batch(&[
        ("aioncad_workspace", workspace_value.as_str()),
        ("freecad_auto_start", auto_start_value),
    ]) {
        tracing::error!(error = %error, "Failed to save FreeCAD settings");
        return Json(json!({"status": "error", "code": "save_failed"}));
    }

    s.cad.set_workspace(resolved_workspace);
    Json(json!({"status": "ok"}))
}

fn parse_serpapi_keys(
    req: &serde_json::Value,
    current_keys: &[String],
) -> Result<Vec<String>, String> {
    let values = req
        .get("api_keys")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "请提供 SerpAPI Key 数组".to_string())?;

    if values.len() > 5 {
        return Err("最多可保存 5 个 SerpAPI Key".to_string());
    }

    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let key = value
                .as_str()
                .ok_or_else(|| format!("第 {} 个 SerpAPI Key 必须是文本", index + 1))?
                .trim();

            if key.is_empty() {
                return Err(format!("第 {} 个 SerpAPI Key 不能为空", index + 1));
            }
            if key.len() > 200 {
                return Err(format!(
                    "第 {} 个 SerpAPI Key 不能超过 200 个字符",
                    index + 1
                ));
            }

            if key.contains("****") {
                let mut parts = key.split("****");
                let prefix = parts.next().unwrap_or_default();
                let suffix = parts.next().unwrap_or_default();
                if parts.next().is_some() {
                    return Err(format!("第 {} 个 SerpAPI Key 掩码格式无效", index + 1));
                }

                let matches: Vec<&String> = current_keys
                    .iter()
                    .filter(|current| current.starts_with(prefix) && current.ends_with(suffix))
                    .collect();
                return match matches.as_slice() {
                    [current] => Ok((*current).clone()),
                    [] => Err(format!(
                        "第 {} 个 SerpAPI Key 掩码无法匹配当前配置，请输入完整 Key",
                        index + 1
                    )),
                    _ => Err(format!(
                        "第 {} 个 SerpAPI Key 掩码匹配多个当前配置，请输入完整 Key",
                        index + 1
                    )),
                };
            }

            if key.len() < 20 {
                return Err(format!(
                    "第 {} 个 SerpAPI Key 至少需要 20 个字符",
                    index + 1
                ));
            }
            if !key.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
            }) {
                return Err(format!("第 {} 个 SerpAPI Key 包含无效字符", index + 1));
            }

            Ok(key.to_string())
        })
        .collect()
}

/// Build the complete persisted SerpAPI configuration replacement.
///
/// Keeping every legacy and numbered slot in one batch means an explicit key
/// removal cannot leave an older key available after a partial update.
fn serpapi_settings_batch(keys: &[String]) -> Vec<(String, String)> {
    let mut settings = vec![
        ("SERPAPI_KEY".to_string(), String::new()),
        ("SERPAPI_KEY_1".to_string(), String::new()),
        ("SERPAPI_KEY_2".to_string(), String::new()),
        ("SERPAPI_KEY_3".to_string(), String::new()),
        ("SERPAPI_KEY_4".to_string(), String::new()),
        ("SERPAPI_KEY_5".to_string(), String::new()),
    ];

    for (setting, key) in settings.iter_mut().skip(1).zip(keys) {
        setting.1 = key.clone();
    }

    settings
}

/// EPO OPS 凭证在 `app_settings` 里的键名（MA5a）。
///
/// **必须与 `AppConfig::from_db_and_env` 读取的键名逐字一致**（`src/routes/mod.rs`：
/// `get("EPO_KEY", "")` / `get("EPO_SECRET", "")`），否则设置页存进库却永远不生效。
/// 键名收在常量里，由 `epo_settings_batch` 唯一使用，配对的回归测试
/// `epo_settings_batch_is_visible_to_app_config_reader` 直接写库后经 `AppConfig` 读回取证。
const EPO_KEY_SETTING: &str = "EPO_KEY";
const EPO_SECRET_SETTING: &str = "EPO_SECRET";

/// 解析设置页提交的 EPO 凭证半边（`epo_key` / `epo_secret`）。
///
/// 三态语义与设置页既有 Key 的处理口径一致：
/// - **字段缺席** → `Ok(None)`：本轮不动这半凭证（设置页 SerpAPI 保存就不会误清 EPO）；
/// - **空串** → `Ok(Some(""))`：显式清空（用户删掉输入框即撤销该源）；
/// - **掩码回显**（GET `/api/settings` 用 `前4****后4` 形态回显，用户原样提交回来）
///   → 反查成当前真实值，避免把 `abcd****wxyz` 当成 Key 写进库；反查不上就报错要全值，
///   与 `parse_serpapi_keys` 的掩码裁决同一条尺子。
///
/// 明文校验：8～200 个字符、只允许可见 ASCII（EPO 的 Consumer Key/Secret 都是
/// 32 位 ASCII 串；含空格/换行/控制字符的粘贴是最常见的误配置，且 Basic 头带空格
/// 会直接撞 OPS 401，故在入口就拒掉）。
fn parse_epo_credential(
    req: &serde_json::Value,
    field: &str,
    current: &str,
    label: &str,
) -> Result<Option<String>, String> {
    let Some(value) = req.get(field) else {
        return Ok(None);
    };
    let raw = value
        .as_str()
        .ok_or_else(|| format!("{label} 必须是文本"))?
        .trim();

    if raw.is_empty() {
        return Ok(Some(String::new()));
    }

    if raw.contains("****") {
        let mut parts = raw.split("****");
        let prefix = parts.next().unwrap_or_default();
        let suffix = parts.next().unwrap_or_default();
        if parts.next().is_some() {
            return Err(format!("{label} 掩码格式无效，请输入完整值"));
        }
        let current = current.trim();
        if current.is_empty() {
            return Err(format!("{label} 尚未配置过，不能用掩码保存，请输入完整值"));
        }
        if !current.starts_with(prefix) || !current.ends_with(suffix) {
            return Err(format!("{label} 的掩码与当前配置不匹配，请输入完整值"));
        }
        return Ok(Some(current.to_string()));
    }

    if raw.len() < 8 || raw.len() > 200 {
        return Err(format!("{label} 长度需在 8～200 个字符之间"));
    }
    if !raw.chars().all(|c| c.is_ascii_graphic()) {
        return Err(format!("{label} 只能包含不含空格的可见字符"));
    }

    Ok(Some(raw.to_string()))
}

/// EPO 凭证的落库批次：两半**一次事务写完**，这样「只存上一半」在库里不可能出现，
/// 与 `AppConfig::epo_credentials()` 的成对语义对齐。
fn epo_settings_batch<'a>(key: &'a str, secret: &'a str) -> [(&'static str, &'a str); 2] {
    [(EPO_KEY_SETTING, key), (EPO_SECRET_SETTING, secret)]
}

/// `POST /api/settings/serpapi` 的 EPO 分支（MA5a）：先全部校验、后一次写库、
/// 成功后才更新内存配置 —— 与 SerpAPI / AI 两处保存的既有次序保持一致，
/// 失败时库里和内存里都还是原值。
///
/// 与 SerpAPI 不同的是**不回写 `.env`**：`app_settings` 是主存储，环境变量只是
/// 用户在 `.env` 里自带凭证时的后备路径；把新凭证再抄一份进明文文件只会扩大
/// MASTER §5 已登记的「.env 明文密钥」风险面。
fn apply_epo_credentials(
    s: &AppState,
    req: &serde_json::Value,
    current_key: &str,
    current_secret: &str,
) -> Json<serde_json::Value> {
    let parsed_key = match parse_epo_credential(req, "epo_key", current_key, "EPO Consumer Key") {
        Ok(value) => value,
        Err(message) => return Json(json!({"status": "error", "message": message})),
    };
    let parsed_secret =
        match parse_epo_credential(req, "epo_secret", current_secret, "EPO Consumer Secret") {
            Ok(value) => value,
            Err(message) => return Json(json!({"status": "error", "message": message})),
        };

    let next_key = parsed_key.unwrap_or_else(|| current_key.trim().to_string());
    let next_secret = parsed_secret.unwrap_or_else(|| current_secret.trim().to_string());

    let batch = epo_settings_batch(&next_key, &next_secret);
    if let Err(error) = s.db.set_settings_batch(&batch) {
        tracing::error!("保存 EPO OPS 配置到数据库失败: {error}");
        return Json(json!({
            "status": "error",
            "message": "保存 EPO OPS 配置失败，请稍后重试"
        }));
    }

    {
        let mut config = s.config.write().unwrap_or_else(|e| e.into_inner());
        config.epo_key = next_key.clone();
        config.epo_secret = next_secret.clone();
    }

    let message = if !next_key.is_empty() && !next_secret.is_empty() {
        "已保存 EPO OPS 凭证，在线检索链已启用该源"
    } else if next_key.is_empty() && next_secret.is_empty() {
        "已清空 EPO OPS 凭证，在线检索链不再登记该源"
    } else {
        "已保存 EPO OPS 凭证，但 Key 与 Secret 需成对配置才会启用该源"
    };
    Json(json!({
        "status": "ok",
        "message": message,
        "epo_configured": !next_key.is_empty() && !next_secret.is_empty(),
    }))
}

/// 保存**搜索源凭证**。
///
/// 路由单源在 `src/common.rs::build_router`，MA5a 的红线是路由表对 `main` 零 diff
/// （118 条 `.route()` 不动），因此 EPO OPS 的两个配置位复用本端点而不是新开
/// `/api/settings/epo`。请求体按字段分组各管各的源：带 `api_keys` 只动 SerpAPI，
/// 带 `epo_key`/`epo_secret` 只动 EPO，两组同时出现则明确报错（宁可拒也不静默丢 Key）。
pub async fn api_save_serpapi(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    // 读取当前内存中的实际 Key（用于反查掩码对应的原始值）
    let (current_keys, current_epo_key, current_epo_secret) = {
        let config = s.config.read().unwrap_or_else(|e| e.into_inner());
        (
            config.serpapi_keys.clone(),
            config.epo_key.clone(),
            config.epo_secret.clone(),
        )
    };

    if req.get("epo_key").is_some() || req.get("epo_secret").is_some() {
        if req.get("api_keys").is_some() {
            return Json(json!({
                "status": "error",
                "message": "一次请求只能保存一组搜索源凭证：请分别提交 SerpAPI Key 与 EPO OPS 凭证"
            }));
        }
        return apply_epo_credentials(&s, &req, &current_epo_key, &current_epo_secret);
    }

    let keys = match parse_serpapi_keys(&req, &current_keys) {
        Ok(keys) => keys,
        Err(message) => return Json(json!({"status": "error", "message": message})),
    };

    let settings = serpapi_settings_batch(&keys);
    let settings_refs: Vec<(&str, &str)> = settings
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();

    if let Err(error) = s.db.set_settings_batch(&settings_refs) {
        tracing::error!("保存 SerpAPI 配置到数据库失败: {error}");
        return Json(json!({
            "status": "error",
            "message": "保存 SerpAPI 配置失败，请稍后重试"
        }));
    }

    {
        let mut config = s.config.write().unwrap_or_else(|e| e.into_inner());
        config.serpapi_keys = keys.clone();
    }

    // .env is only a desktop backup. Keep every legacy and numbered slot in
    // sync after the primary SQLite transaction has succeeded.
    for (db_key, value) in &settings {
        if let Err(error) = update_env_file(db_key, value) {
            tracing::warn!("保存 SerpAPI 配置备份失败，key: {db_key}，error: {error}");
        }
    }

    Json(json!({
        "status": "ok",
        "message": format!("已保存 {} 个 SerpAPI Key", keys.len())
    }))
}

pub async fn api_save_ai(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let base_url = req["base_url"].as_str().unwrap_or("").trim();
    let api_key = req["api_key"].as_str().unwrap_or("").trim();
    let model = req["model"].as_str().unwrap_or("").trim();
    let model_expert = req["ai_model_expert"].as_str().unwrap_or("").trim();
    let google_client_id = req["google_client_id"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();
    let google_client_secret = req["google_client_secret"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();

    if base_url.is_empty() || api_key.is_empty() || model.is_empty() {
        return Json(json!({"status": "error", "message": "All fields are required"}));
    }
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Json(json!({"status": "error", "message": "URL must use HTTP or HTTPS protocol"}));
    }
    // 掩码检测必须在长度检查之前：如果用户使用 ≤8 字符的短 Key 的掩码值 "****"，应允许通过
    if !api_key.contains("****") && (api_key.len() < 8 || api_key.len() > 200) {
        return Json(
            json!({"status": "error", "message": "API key length must be between 8 and 200 characters"}),
        );
    }
    if model.len() < 2 || model.len() > 100 {
        return Json(
            json!({"status": "error", "message": "Model name must be between 2 and 100 characters"}),
        );
    }
    if !model
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == ':' || c == '/')
    {
        return Json(
            json!({"status": "error", "message": "Model name contains invalid characters"}),
        );
    }
    let model_expert = if model_expert.is_empty() {
        "deepseek-reasoner"
    } else {
        model_expert
    };
    if model_expert.len() > 100
        || !model_expert.chars().all(|c| {
            c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == ':' || c == '/'
        })
    {
        return Json(
            json!({"status": "error", "message": "Expert model name contains invalid characters"}),
        );
    }

    // 检查 API Key 是否为掩码形式（包含 ****），若是则从该服务商独立 Key 字段中读取真实值
    let db_key_name = provider_db_key(base_url);
    let api_key = if api_key.contains("****") {
        // 从当前内存配置中反查该服务商的真实 Key
        let config = s.config.read().unwrap_or_else(|e| e.into_inner());
        let current_key = config.api_key_for_provider(base_url);
        if current_key.is_empty() || current_key == "ollama" {
            return Json(json!({
                "status": "error",
                "message": "该服务商尚未配置过 API Key，请手动输入完整 Key，不能使用掩码值。"
            }));
        }
        current_key
    } else {
        api_key.to_string()
    };

    // Google Client Secret 同理：前端传来空值或掩码时保持原有值
    let google_client_secret = if google_client_secret.is_empty()
        || google_client_secret.contains("****")
    {
        let current = s.config.read().unwrap_or_else(|e| e.into_inner());
        let old = current.google_client_secret.clone();
        if google_client_secret.contains("****") {
            // 掩码情况下检查域名变更（Google Client Secret 仍与服务商绑定）
            let old_domain = extract_domain(&current.ai_base_url);
            let new_domain = extract_domain(base_url);
            if old_domain != new_domain {
                return Json(json!({
                    "status": "error",
                    "message": format!(
                        "检测到切换 AI 服务商（{} → {}），请手动输入新的 Google Client Secret，不能使用旧 Secret 的掩码值。",
                        old_domain.as_deref().unwrap_or("<unknown>"),
                        new_domain.as_deref().unwrap_or("<unknown>")
                    )
                }));
            }
        }
        old
    } else {
        google_client_secret
    };

    // Gemini CLI 模式设置
    let gemini_cli_enabled = req["gemini_cli_enabled"].as_bool().unwrap_or(false);
    let gemini_cli_path = find_gemini_cli().unwrap_or_default();
    let gemini_cli_val = if gemini_cli_enabled { "true" } else { "false" };

    let settings = [
        ("AI_BASE_URL", base_url),
        (db_key_name, api_key.as_str()),
        ("AI_API_KEY", api_key.as_str()),
        ("AI_MODEL", model),
        ("AI_MODEL_EXPERT", model_expert),
        ("GOOGLE_CLIENT_ID", google_client_id.as_str()),
        ("GOOGLE_CLIENT_SECRET", google_client_secret.as_str()),
        ("GEMINI_CLI_ENABLED", gemini_cli_val),
    ];
    if let Err(e) = s.db.set_settings_batch(&settings) {
        tracing::error!("Failed to save AI settings to the database: {}", e);
        return Json(json!({
            "status": "error",
            "message": "保存 AI 配置失败，请稍后重试"
        }));
    }

    // SQLite 主存储成功后，再更新内存配置以保持一致。
    {
        let mut config = s.config.write().unwrap_or_else(|e| e.into_inner());
        config.ai_base_url = base_url.to_string();
        config.ai_api_key = api_key.clone(); // 始终更新通用 Key（向后兼容）
                                             // 同时更新对应服务商的独立 Key
        *match db_key_name {
            "AI_API_KEY_DEEPSEEK" => &mut config.ai_api_key_deepseek,
            "AI_API_KEY_ANTHROPIC" => &mut config.ai_api_key_anthropic,
            "AI_API_KEY_XIAOMI" => &mut config.ai_api_key_xiaomi,
            "AI_API_KEY_SENSENOVA" => &mut config.ai_api_key_sensetime,
            "AI_API_KEY_OPENROUTER" => &mut config.ai_api_key_openrouter,
            "AI_API_KEY_GEMINI" => &mut config.ai_api_key_gemini,
            "AI_API_KEY_ZHIPU" => &mut config.ai_api_key_zhipu,
            "AI_API_KEY_QWEN" => &mut config.ai_api_key_qwen,
            "AI_API_KEY_OPENAI" => &mut config.ai_api_key_openai,
            _ => &mut config.ai_api_key, // custom → 通用 Key
        } = api_key.clone();

        config.ai_model = model.to_string();
        config.ai_model_expert = model_expert.to_string();
        config.google_client_id = google_client_id.clone();
        config.google_client_secret = google_client_secret.clone();

        // 非 Gemini 服务商时，清除 OAuth 令牌（避免旧令牌持久化干扰）
        if !base_url.contains("googleapis") {
            config.google_access_token.clear();
            config.google_token_expiry = None;
            config.google_refresh_token.clear();
            config.google_auth_mode.clear();
        }

        config.gemini_cli_enabled = gemini_cli_enabled;
        config.gemini_cli_path = gemini_cli_path.clone();
    }

    // .env is an optional desktop backup; SQLite remains the source of truth.
    for (key, value) in settings {
        if let Err(e) = update_env_file(key, value) {
            tracing::warn!("Failed to update AI setting .env backup for {}: {}", key, e);
        }
    }

    Json(json!({"status": "ok"}))
}

/// 查询 SerpAPI 账户余额/用量
pub async fn api_serpapi_balance(State(s): State<AppState>) -> Json<serde_json::Value> {
    let config = s.config.read().unwrap_or_else(|e| e.into_inner());
    let keys = config.serpapi_keys.clone();
    drop(config);

    // 使用同步线程 + channel 的方式查询 SerpAPI（避免 async reqwest 的 Send 问题）
    let data = if keys.is_empty() {
        None
    } else {
        let key = keys[0].clone();
        let url = format!("https://serpapi.com/account.json?api_key={}", key);
        let (tx, rx) = std::sync::mpsc::channel::<Result<serde_json::Value, String>>();
        std::thread::spawn(move || {
            let client = reqwest::blocking::Client::new();
            let result = client
                .get(&url)
                .send()
                .map_err(|e| format!("连接 SerpAPI 失败: {}", e))
                .and_then(|r| {
                    r.json::<serde_json::Value>()
                        .map_err(|e| format!("解析 SerpAPI 响应失败: {}", e))
                });
            let _ = tx.send(result);
        });
        match rx.recv_timeout(std::time::Duration::from_secs(30)) {
            Ok(Ok(d)) => Some(d),
            Ok(Err(e)) => return Json(json!({"status": "error", "message": e})),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                return Json(json!({"status": "error", "message": "查询 SerpAPI 余额超时（>30s）"}))
            }
            Err(e) => {
                return Json(
                    json!({"status": "error", "message": format!("接收线程结果失败: {}", e)}),
                )
            }
        }
    };

    let Some(data) = data else {
        return Json(json!({"status": "error", "message": "未配置 SerpAPI Key"}));
    };

    let searches_per_month = data["searches_per_month"].as_i64().unwrap_or(250);
    let this_month_usage = data["this_month_usage"].as_i64().unwrap_or(0);
    let total_usage = data["total_usage"].as_i64().unwrap_or(0);
    let plan_name = data["plan_name"].as_str().unwrap_or("Free").to_string();
    let remaining = (searches_per_month - this_month_usage).max(0);

    Json(json!({
        "status": "ok",
        "plan_name": plan_name,
        "searches_per_month": searches_per_month,
        "this_month_usage": this_month_usage,
        "remaining": remaining,
        "total_usage": total_usage,
        "key_count": keys.len(),
    }))
}

// 以下搜索源保存接口已屏蔽（仅保留 SerpAPI）：
// api_save_firecrawl, api_save_bing, api_save_lens, api_save_cnipr

pub async fn api_import_patents(
    State(s): State<AppState>,
    Json(req): Json<crate::patent::ImportRequest>,
) -> Json<serde_json::Value> {
    let mut n = 0;
    for p in &req.patents {
        if s.db.insert_patent(p).is_ok() {
            n += 1;
        }
    }
    Json(json!({"status":"ok","imported":n}))
}

// api_save_fallbacks 已屏蔽（仅保留 DeepSeek AI，无需备用 AI）

fn update_env_file(key: &str, value: &str) -> Result<(), String> {
    let env_path = ".env";
    let content = std::fs::read_to_string(env_path).unwrap_or_default();
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut found = false;

    for line in &mut lines {
        if line.starts_with(&format!("{}=", key)) {
            *line = format!("{}={}", key, value);
            found = true;
            break;
        }
    }

    if !found {
        lines.push(format!("{}={}", key, value));
    }

    // 原子写入：先写临时文件，再重命名，避免并发写入丢失数据
    let tmp_path = format!("{}.tmp", env_path);
    std::fs::write(&tmp_path, lines.join("\n"))
        .map_err(|e| format!("Failed to write .env.tmp file: {}", e))?;
    std::fs::rename(&tmp_path, env_path)
        .map_err(|e| format!("Failed to rename .env.tmp to .env: {}", e))?;

    Ok(())
}

/// 从 URL 提取域名用于检测是否切换了服务商
fn extract_domain(url: &str) -> Option<String> {
    let url = url.trim();
    let after_protocol = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let domain = after_protocol.split('/').next()?;
    // 检测是否为 IP 地址（如 192.168.1.1），IP 地址返回完整 IP
    let is_ip = domain.chars().all(|c| c.is_ascii_digit() || c == '.');
    if is_ip {
        return Some(domain.to_string());
    }
    // 只取主域名（如 api.deepseek.com → deepseek.com, generativelanguage.googleapis.com → googleapis.com）
    let parts: Vec<&str> = domain.split('.').collect();
    if parts.len() >= 2 {
        Some(format!(
            "{}.{}",
            parts[parts.len() - 2],
            parts[parts.len() - 1]
        ))
    } else {
        Some(domain.to_string())
    }
}

#[cfg(test)]
mod serpapi_key_parsing_tests {
    use super::{parse_serpapi_keys, serpapi_settings_batch};
    use serde_json::json;

    const VALID_KEY: &str = "abcdEFGHijklMNOPqrstUVWX";

    fn request(api_keys: serde_json::Value) -> serde_json::Value {
        json!({"api_keys": api_keys})
    }

    #[test]
    fn empty_array_is_an_explicit_request_to_clear_keys() {
        let parsed = parse_serpapi_keys(&request(json!([])), &[VALID_KEY.to_string()]).unwrap();

        assert!(parsed.is_empty());
    }

    #[test]
    fn invalid_requests_are_rejected_without_partial_results() {
        assert!(parse_serpapi_keys(&request(json!("not-an-array")), &[]).is_err());
        assert!(parse_serpapi_keys(
            &request(json!([
                VALID_KEY, VALID_KEY, VALID_KEY, VALID_KEY, VALID_KEY, VALID_KEY
            ])),
            &[]
        )
        .is_err());
        assert!(parse_serpapi_keys(&request(json!(["too-short"])), &[]).is_err());
        assert!(parse_serpapi_keys(&request(json!(["unknown****mask"])), &[]).is_err());
        assert!(parse_serpapi_keys(
            &request(json!(["abcd****UVWX"])),
            &[
                "abcdEFGHijklMNOPqrstUVWX".to_string(),
                "abcdZYXWvutsRQPOnmlkUVWX".to_string(),
            ]
        )
        .is_err());
    }

    #[test]
    fn valid_keys_and_unique_masks_are_accepted() {
        let current_keys = vec![VALID_KEY.to_string()];

        assert_eq!(
            parse_serpapi_keys(&request(json!([VALID_KEY])), &current_keys).unwrap(),
            current_keys
        );
        assert_eq!(
            parse_serpapi_keys(&request(json!(["abcd****UVWX"])), &current_keys).unwrap(),
            current_keys
        );
    }

    #[test]
    fn persistence_batch_replaces_all_legacy_and_numbered_slots() {
        let keys = vec![
            VALID_KEY.to_string(),
            "ZYXWvutsRQPOnmlkjihgfedc".to_string(),
        ];

        assert_eq!(
            serpapi_settings_batch(&keys),
            vec![
                ("SERPAPI_KEY".to_string(), String::new()),
                ("SERPAPI_KEY_1".to_string(), VALID_KEY.to_string()),
                (
                    "SERPAPI_KEY_2".to_string(),
                    "ZYXWvutsRQPOnmlkjihgfedc".to_string(),
                ),
                ("SERPAPI_KEY_3".to_string(), String::new()),
                ("SERPAPI_KEY_4".to_string(), String::new()),
                ("SERPAPI_KEY_5".to_string(), String::new()),
            ]
        );
    }
}

/// MA5a：设置页 EPO OPS 凭证位的后端侧取证。
///
/// 这里锁死三件事，缺一条配置位就是「看着能存、其实不生效」的假功能：
/// 1. **键名逐字对齐**——写入用的键名必须就是 `AppConfig::from_db_and_env` 读取的那两个，
///    且经过 `AppConfig::epo_credentials()` 的成对裁决；
/// 2. **回显脱敏**——`GET /api/settings` 永远吐掩码，明文不出后端；掩码原样提交回来
///    要能反查成真实值（用户只改 Secret 时 Key 框里就是掩码）；
/// 3. **两个源互不清**——`/api/settings/serpapi` 按字段分组，保存 EPO 不动 SerpAPI，
///    反之亦然（这条最容易在复用端点时写错）。
#[cfg(test)]
mod epo_credential_settings_tests {
    use super::{api_get_settings, api_save_serpapi, epo_settings_batch, parse_epo_credential};
    use crate::cad::CadService;
    use crate::db::Database;
    use crate::routes::{AppConfig, AppState};
    use axum::{extract::State, Json};
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, RwLock};

    /// 30 字符；脱敏回显应为 `epok****FFFF`（手算，不调用生产掩码函数生成期望值）。
    const EPO_KEY_VALUE: &str = "epokeyAAAABBBBCCCCDDDDEEEEFFFF";
    /// 31 字符；脱敏回显应为 `epos****5566`。
    const EPO_SECRET_VALUE: &str = "eposecret1111222233334444555566";
    const EPO_KEY_MASK: &str = "epok****FFFF";
    const SERPAPI_VALUE: &str = "abcdEFGHijklMNOPqrstUVWX";
    const NEW_SERPAPI_KEY: &str = "wxyzABCDefgh0123456789IJKL";

    fn state_with(config: AppConfig) -> AppState {
        let cad_root =
            std::env::temp_dir().join(format!("innoforge-epo-settings-{}", uuid::Uuid::new_v4()));
        AppState {
            db: Arc::new(Database::init(":memory:").expect("in-memory db")),
            cad: Arc::new(CadService::new(cad_root, None).expect("cad service")),
            config: Arc::new(RwLock::new(config)),
            pipeline_channels: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn setting(state: &AppState, key: &str) -> String {
        state
            .db
            .get_setting(key)
            .expect("read setting")
            .unwrap_or_default()
    }

    fn stored_config(state: &AppState) -> AppConfig {
        state
            .config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    #[test]
    fn batch_writes_exactly_the_keys_app_config_reads() {
        // 逐字对照：常量若被改名，下面这条 `AppConfig` 读回断言会立刻红。
        assert_eq!(
            epo_settings_batch("ck", "cs"),
            [("EPO_KEY", "ck"), ("EPO_SECRET", "cs")]
        );

        let db = Database::init(":memory:").expect("in-memory db");
        db.set_settings_batch(&epo_settings_batch("db-ck", "db-cs"))
            .expect("write settings");

        let config = AppConfig::from_db_and_env(Some(&db));
        assert_eq!(
            Some(("db-ck".to_string(), "db-cs".to_string())),
            config.epo_credentials(),
            "设置页落库即生效：读取路径与检索链登记用的是同一对键名"
        );
    }

    #[test]
    fn half_pair_lands_in_db_but_is_not_registered_as_configured() {
        let db = Database::init(":memory:").expect("in-memory db");
        db.set_settings_batch(&epo_settings_batch("only-key", ""))
            .expect("write settings");

        let config = AppConfig::from_db_and_env(Some(&db));
        assert_eq!(None, config.epo_credentials(), "只填一半不算配置");
    }

    #[test]
    fn absent_field_keeps_current_value_and_empty_string_clears_it() {
        assert_eq!(
            None,
            parse_epo_credential(&json!({}), "epo_key", "current", "EPO Consumer Key")
                .expect("字段缺席不是错误"),
            "字段缺席 = 本轮不动这半凭证（SerpAPI 保存因此不会清掉 EPO）"
        );
        assert_eq!(
            Some(String::new()),
            parse_epo_credential(&json!({"epo_key": "  "}), "epo_key", "current", "L")
                .expect("空串是合法的显式清空"),
            "用户删空输入框即撤销该源"
        );
    }

    #[test]
    fn masked_echo_submits_back_as_the_real_value() {
        let req = json!({"epo_key": EPO_KEY_MASK});
        assert_eq!(
            Some(EPO_KEY_VALUE.to_string()),
            parse_epo_credential(&req, "epo_key", EPO_KEY_VALUE, "EPO Consumer Key")
                .expect("掩码可反查"),
            "GET 回显的掩码原样提交回来时必须解析回真实值，否则库里写进的就是掩码"
        );

        // 掩码与当前值不匹配（例如换了 Key 又提交旧掩码）→ 拒绝，不静默保留旧值。
        let stale = json!({"epo_key": "zzzz****0000"});
        assert!(
            parse_epo_credential(&stale, "epo_key", EPO_KEY_VALUE, "EPO Consumer Key").is_err(),
            "对不上的掩码要报错"
        );
        // 从没配过时提交掩码同样报错（否则会把掩码当 Key 存）。
        assert!(
            parse_epo_credential(&req, "epo_key", "", "EPO Consumer Key").is_err(),
            "未配置过就没有可反查的真值"
        );
        // 多段掩码（"a****b****c"）不是本端点认识的形态。
        assert!(parse_epo_credential(
            &json!({"epo_key": "epok****FF****FFFF"}),
            "epo_key",
            EPO_KEY_VALUE,
            "EPO Consumer Key"
        )
        .is_err());
    }

    #[test]
    fn plaintext_validation_rejects_the_common_misconfigurations() {
        let too_long = "x".repeat(201);
        for bad in [
            "short",               // 太短
            "epo key with spaces", // 粘贴带空格
            "epo\nsecret",         // 带换行
            "epo\tsecret",         // 带制表符
            too_long.as_str(),     // 太长
        ] {
            let req = json!({"epo_key": bad});
            assert!(
                parse_epo_credential(&req, "epo_key", "", "EPO Consumer Key").is_err(),
                "{bad:?} 应被拒"
            );
        }
        assert!(
            parse_epo_credential(&json!({"epo_key": 123}), "epo_key", "", "EPO Consumer Key")
                .is_err()
        );
        assert!(parse_epo_credential(
            &json!({"epo_key": EPO_KEY_VALUE}),
            "epo_key",
            "",
            "EPO Consumer Key"
        )
        .expect("合法明文通过")
        .is_some());
    }

    #[tokio::test]
    async fn save_then_get_round_trips_with_masked_echo() {
        let state = state_with(AppConfig::default());
        let saved = api_save_serpapi(
            State(state.clone()),
            Json(json!({"epo_key": EPO_KEY_VALUE, "epo_secret": EPO_SECRET_VALUE})),
        )
        .await;
        assert_eq!("ok", saved.0["status"], "{:?}", saved.0);
        assert!(saved.0["epo_configured"].as_bool().unwrap_or(false));

        // 库里存明文，且键名就是检索链读的那两个
        assert_eq!(EPO_KEY_VALUE, setting(&state, "EPO_KEY"));
        assert_eq!(EPO_SECRET_VALUE, setting(&state, "EPO_SECRET"));
        assert_eq!(
            Some((EPO_KEY_VALUE.to_string(), EPO_SECRET_VALUE.to_string())),
            stored_config(&state).epo_credentials(),
            "保存后内存配置即刻生效，不必重启"
        );

        // GET 只吐掩码，明文不出后端
        let echoed = api_get_settings(State(state.clone())).await;
        let payload = echoed.0.to_string();
        assert_eq!(
            EPO_KEY_MASK,
            echoed.0["epo_key"].as_str().expect("掩码 Key")
        );
        assert_eq!(
            "epos****5566",
            echoed.0["epo_secret"].as_str().expect("掩码 Secret")
        );
        assert!(
            !payload.contains(EPO_KEY_VALUE),
            "明文 Key 不得出现在回显里"
        );
        assert!(
            !payload.contains(EPO_SECRET_VALUE),
            "明文 Secret 不得出现在回显里"
        );
        assert!(echoed.0["epo_configured"].as_bool().unwrap_or(false));
        assert!(echoed.0["epo_key_configured"].as_bool().unwrap_or(false));
        assert!(echoed.0["epo_secret_configured"].as_bool().unwrap_or(false));

        // 用户只改 Secret、Key 框保持回显掩码 → Key 原值保留
        let new_secret = "rotatedsecretvalue0123456789ab";
        let masked_key = echoed.0["epo_key"].as_str().expect("掩码").to_string();
        let saved2 = api_save_serpapi(
            State(state.clone()),
            Json(json!({"epo_key": masked_key, "epo_secret": new_secret})),
        )
        .await;
        assert_eq!("ok", saved2.0["status"], "{:?}", saved2.0);
        assert_eq!(EPO_KEY_VALUE, setting(&state, "EPO_KEY"), "掩码不该写进库");
        assert_eq!(new_secret, setting(&state, "EPO_SECRET"));
    }

    #[tokio::test]
    async fn half_pair_saves_but_reports_not_configured() {
        let state = state_with(AppConfig::default());
        let saved = api_save_serpapi(
            State(state.clone()),
            Json(json!({"epo_key": EPO_KEY_VALUE, "epo_secret": ""})),
        )
        .await;
        // 只填一半**不阻止保存**（发包口径），但如实报告「未生效」
        assert_eq!("ok", saved.0["status"], "{:?}", saved.0);
        assert!(!saved.0["epo_configured"].as_bool().unwrap_or(true));
        assert_eq!(EPO_KEY_VALUE, setting(&state, "EPO_KEY"));
        assert_eq!("", setting(&state, "EPO_SECRET"));

        let echoed = api_get_settings(State(state.clone())).await;
        assert!(!echoed.0["epo_configured"].as_bool().unwrap_or(true));
        assert!(echoed.0["epo_key_configured"].as_bool().unwrap_or(false));
        assert!(!echoed.0["epo_secret_configured"].as_bool().unwrap_or(true));
        assert_eq!(
            None,
            stored_config(&state).epo_credentials(),
            "成对裁决在 AppConfig 侧，设置页与检索链看到的是同一个结论"
        );

        // 清空两半 → 该源撤销
        let cleared = api_save_serpapi(
            State(state.clone()),
            Json(json!({"epo_key": "", "epo_secret": ""})),
        )
        .await;
        assert_eq!("ok", cleared.0["status"], "{:?}", cleared.0);
        assert!(setting(&state, "EPO_KEY").is_empty());
    }

    #[tokio::test]
    async fn saving_one_search_provider_never_touches_the_other() {
        let config = AppConfig {
            serpapi_keys: vec![SERPAPI_VALUE.to_string()],
            epo_key: EPO_KEY_VALUE.to_string(),
            epo_secret: EPO_SECRET_VALUE.to_string(),
            ..AppConfig::default()
        };
        let state = state_with(config);
        state
            .db
            .set_settings_batch(&[("SERPAPI_KEY_1", SERPAPI_VALUE)])
            .expect("seed serpapi");
        state
            .db
            .set_settings_batch(&epo_settings_batch(EPO_KEY_VALUE, EPO_SECRET_VALUE))
            .expect("seed epo");

        // ① 只保存 EPO（设置页新增按钮的请求形状）→ SerpAPI 槽位不动
        let new_key = "newepokey11112222333344445555FF";
        let epo_only = api_save_serpapi(
            State(state.clone()),
            Json(json!({"epo_key": new_key, "epo_secret": EPO_SECRET_VALUE})),
        )
        .await;
        assert_eq!("ok", epo_only.0["status"], "{:?}", epo_only.0);
        assert_eq!(SERPAPI_VALUE, setting(&state, "SERPAPI_KEY_1"));
        assert_eq!(new_key, setting(&state, "EPO_KEY"));

        // ② 只保存 SerpAPI → EPO 两半都不动（SerpAPI 自己被换成新 Key 是本端点的既有语义）
        let serpapi_only = api_save_serpapi(
            State(state.clone()),
            Json(json!({"api_keys": [NEW_SERPAPI_KEY]})),
        )
        .await;
        assert_eq!("ok", serpapi_only.0["status"], "{:?}", serpapi_only.0);
        assert_eq!(NEW_SERPAPI_KEY, setting(&state, "SERPAPI_KEY_1"));
        assert_eq!(new_key, setting(&state, "EPO_KEY"));
        assert_eq!(EPO_SECRET_VALUE, setting(&state, "EPO_SECRET"));

        // ③ 空 body 的旧错误语义保持不变（既不走 EPO 分支，也不清空任何东西）
        let empty = api_save_serpapi(State(state.clone()), Json(json!({}))).await;
        assert_eq!("error", empty.0["status"]);
        assert_eq!(new_key, setting(&state, "EPO_KEY"));
        assert_eq!(NEW_SERPAPI_KEY, setting(&state, "SERPAPI_KEY_1"));
    }

    #[tokio::test]
    async fn mixed_credential_request_is_rejected_without_writing() {
        let state = state_with(AppConfig::default());
        let rejected = api_save_serpapi(
            State(state.clone()),
            Json(json!({"api_keys": [SERPAPI_VALUE], "epo_key": EPO_KEY_VALUE})),
        )
        .await;
        assert_eq!("error", rejected.0["status"], "{:?}", rejected.0);
        assert!(setting(&state, "EPO_KEY").is_empty(), "报错就不能留下半边");
        assert!(
            setting(&state, "SERPAPI_KEY_1").is_empty(),
            "报错就不能顺手写另一个源"
        );
    }
}
