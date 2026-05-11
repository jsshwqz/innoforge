use super::AppState;
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
    }))
}

pub async fn api_save_serpapi(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let keys: Vec<String> = req["api_keys"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    let k = v.as_str()?.trim().to_string();
                    if k.is_empty() || k.len() < 20 || k.len() > 200 {
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

    let mut new_keys: Vec<String> = Vec::new();

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
        new_keys.push(k.clone());
    }

    // 更新内存配置（立即生效）
    {
        let mut config = s.config.write().unwrap_or_else(|e| e.into_inner());
        config.serpapi_keys = new_keys;
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

    if base_url.is_empty() || api_key.is_empty() || model.is_empty() {
        return Json(json!({"status": "error", "message": "All fields are required"}));
    }
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Json(json!({"status": "error", "message": "URL must use HTTP or HTTPS protocol"}));
    }
    if api_key.len() < 8 || api_key.len() > 200 {
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

    // 先更新内存配置（立即生效）
    {
        let mut config = s.config.write().unwrap_or_else(|e| e.into_inner());
        config.ai_base_url = base_url.to_string();
        config.ai_api_key = api_key.to_string();
        config.ai_model = model.to_string();
    }

    // SQLite 持久化（主存储，Android 友好）
    for (k, v) in [
        ("AI_BASE_URL", base_url),
        ("AI_API_KEY", api_key),
        ("AI_MODEL", model),
    ] {
        if let Err(e) = s.db.set_setting(k, v) {
            tracing::warn!("保存设置 {} 到数据库失败: {}", k, e);
        }
    }
    // .env 持久化为可选（桌面端后备）
    let _ = update_env_file("AI_BASE_URL", base_url);
    let _ = update_env_file("AI_API_KEY", api_key);
    let _ = update_env_file("AI_MODEL", model);

    Json(json!({"status": "ok"}))
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

    std::fs::write(env_path, lines.join("\n"))
        .map_err(|e| format!("Failed to write .env file: {}", e))?;

    Ok(())
}
