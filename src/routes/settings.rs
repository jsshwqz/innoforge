use super::{find_gemini_cli, AppState};
use axum::{extract::State, Json};
use serde_json::json;

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

    // 仅保留 SerpAPI + AI 配置，其他搜索源（Firecrawl/Bing/Lens/CNIPR）和备用 AI 已屏蔽

    Json(json!({
        "serpapi_keys": serpapi_keys,
        "serpapi_key_configured": config.has_serpapi(),
        "ai_base_url": config.ai_base_url,
        "ai_api_key": mask_api_key(&config.ai_api_key),
        "ai_api_key_configured": !config.ai_api_key.is_empty(),
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

pub async fn api_save_serpapi(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    // 读取当前内存中的实际 Key（用于反查掩码对应的原始值）
    let current_keys: Vec<String> = {
        let config = s.config.read().unwrap_or_else(|e| e.into_inner());
        config.serpapi_keys.clone()
    };

    let keys: Vec<String> = req["api_keys"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    let k = v.as_str()?.trim().to_string();
                    if k.is_empty() || k.len() > 200 {
                        return None;
                    }
                    // 如果是掩码值（前端 GET 后再 PUT），反查原始 Key
                    if k.contains("****") {
                        // 去掉 **** 后取前后缀，匹配当前有效的 Key
                        let parts: Vec<&str> = k.split("****").collect();
                        if parts.len() == 2 {
                            let prefix = parts[0];
                            let suffix = parts[1];
                            for current in &current_keys {
                                if current.starts_with(prefix) && current.ends_with(suffix) {
                                    return Some(current.clone());
                                }
                            }
                        }
                        // 未匹配到原始 Key，跳过
                        return None;
                    }
                    // 非掩码 Key：正常校验
                    if k.len() < 20 {
                        return None;
                    }
                    if !k
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
                    {
                        return None;
                    }
                    Some(k)
                })
                .collect()
        })
        .unwrap_or_default();

    // 先清除 DB 和 .env 中旧的单 key 和多 key 记录
    for suffix in ["", "_1", "_2", "_3", "_4", "_5"] {
        let db_key = format!("SERPAPI_KEY{}", suffix);
        let _ = s.db.set_setting(&db_key, "");
        let _ = update_env_file(&db_key, "");
    }

    for (i, k) in keys.iter().enumerate() {
        let idx = i + 1;
        let db_key = format!("SERPAPI_KEY_{}", idx);
        if let Err(e) = s.db.set_setting(&db_key, k) {
            tracing::warn!("保存设置 {} 到数据库失败: {}", db_key, e);
        }
        let _ = update_env_file(&db_key, k);
    }

    // 更新内存配置（立即生效）
    {
        let mut config = s.config.write().unwrap_or_else(|e| e.into_inner());
        config.serpapi_keys = keys.clone();
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

    // 检查 API Key 是否为掩码形式（包含 ****），若是则保持原有值不变
    // 但如果 base_url 域名变了（切换服务商），则拒绝保存并提示用户手动输入新 Key
    let api_key = if api_key.contains("****") {
        let current = s.config.read().unwrap_or_else(|e| e.into_inner());
        let old_domain = extract_domain(&current.ai_base_url);
        let new_domain = extract_domain(base_url);
        if old_domain != new_domain {
            return Json(json!({
                "status": "error",
                "message": format!(
                    "检测到切换 AI 服务商（{} → {}），请手动输入新的 API Key，不能使用旧 Key 的掩码值。",
                    old_domain.as_deref().unwrap_or("<unknown>"),
                    new_domain.as_deref().unwrap_or("<unknown>")
                )
            }));
        }
        current.ai_api_key.clone()
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
            // 掩码情况下额外检查是否切换了服务商（域名变更）
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

    // 先更新内存配置（立即生效）
    {
        let mut config = s.config.write().unwrap_or_else(|e| e.into_inner());
        config.ai_base_url = base_url.to_string();
        config.ai_api_key = api_key.clone();
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

    // SQLite 持久化（主存储，Android 友好）
    for (k, v) in [
        ("AI_BASE_URL", base_url),
        ("AI_API_KEY", &api_key),
        ("AI_MODEL", model),
        ("AI_MODEL_EXPERT", model_expert),
        ("GOOGLE_CLIENT_ID", &google_client_id),
        ("GOOGLE_CLIENT_SECRET", &google_client_secret),
    ] {
        if let Err(e) = s.db.set_setting(k, v) {
            tracing::warn!("保存设置 {} 到数据库失败: {}", k, e);
        }
    }
    // Gemini CLI 设置持久化
    let gemini_cli_val = if gemini_cli_enabled { "true" } else { "false" };
    let _ = s.db.set_setting("GEMINI_CLI_ENABLED", gemini_cli_val);
    let _ = update_env_file("GEMINI_CLI_ENABLED", gemini_cli_val);
    // .env 持久化为可选（桌面端后备）
    let _ = update_env_file("AI_BASE_URL", base_url);
    let _ = update_env_file("AI_API_KEY", &api_key);
    let _ = update_env_file("AI_MODEL", model);
    let _ = update_env_file("AI_MODEL_EXPERT", model_expert);
    let _ = update_env_file("GOOGLE_CLIENT_ID", &google_client_id);
    let _ = update_env_file("GOOGLE_CLIENT_SECRET", &google_client_secret);

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
