use super::{find_gemini_cli, AppState};
use axum::{extract::State, Json};
use serde_json::json;

/// 根据 base_url 返回该服务商对应的 DB/.env Key 名称
fn provider_db_key(base_url: &str) -> &'static str {
    if base_url.contains("deepseek") {
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

    // 仅保留 SerpAPI + AI 配置，其他搜索源（Firecrawl/Bing/Lens/CNIPR）和备用 AI 已屏蔽

    Json(json!({
        "serpapi_keys": serpapi_keys,
        "serpapi_key_configured": config.has_serpapi(),
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
    }))
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

pub async fn api_save_serpapi(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    // 读取当前内存中的实际 Key（用于反查掩码对应的原始值）
    let current_keys: Vec<String> = {
        let config = s.config.read().unwrap_or_else(|e| e.into_inner());
        config.serpapi_keys.clone()
    };

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
