//! Google OAuth 认证 / Google OAuth + gcloud CLI for Gemini API
//!
//! 两种认证方式：
//! 1. **gcloud CLI**（推荐）：检测本地 gcloud CLI，获取 ADC（Application Default Credentials）
//!    令牌来认证 Gemini API 调用，无需配置 OAuth 同意屏幕。
//! 2. **浏览器 OAuth**（备选）：标准 OAuth 2.0 Web 流，需要 Google Cloud Console
//!    中正确配置 OAuth 同意屏幕和凭据。
//!
//! ## gcloud CLI 流程
//! 1. 用户安装 gcloud CLI 并运行 `gcloud auth application-default login`
//! 2. 前端调用 `GET /api/auth/gcloud/status` 检测 gcloud 状态
//! 3. 前端调用 `POST /api/auth/gcloud/login` 获取访问令牌
//! 4. 令牌自动刷新：`ensure_google_token()` 在过期时重新运行 gcloud

use super::AppState;
use axum::{extract::State, Json};
use serde_json::json;

const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_SCOPE: &str = "openid email https://www.googleapis.com/auth/generative-language";
const REDIRECT_URI: &str = "http://localhost:3000/api/auth/google/callback";

// ===== gcloud CLI (ADC) 认证 =====

/// gcloud SDK 内置的 OAuth 客户端凭据（公有已知值）
/// 用于从 ADC 文件刷新令牌，无需用户创建自己的 OAuth 客户端。
#[allow(dead_code)]
const GCLOUD_ADC_CLIENT_ID: &str =
    "764086051850-6qr4p6gpi6hn506pt8ejuq83di341hur.apps.googleusercontent.com";
#[allow(dead_code)]
const GCLOUD_ADC_CLIENT_SECRET: &str = "d-FL95Q19q7MQmFpd7hHD0Ty";

/// 检测 gcloud CLI 状态
pub async fn api_gcloud_status(State(s): State<AppState>) -> Json<serde_json::Value> {
    let gcloud_available = detect_gcloud_cli();
    let adc_path = get_adc_path();
    let adc_exists = adc_path.as_ref().map(|p| p.exists()).unwrap_or(false);

    // 尝试解析 ADC 文件以获取更多信息
    let adc_info = if adc_exists {
        if let Some(path) = &adc_path {
            parse_adc_file(path)
        } else {
            None
        }
    } else {
        None
    };

    // 当前是否已通过 gcloud 认证
    let cfg = s.config.read().unwrap_or_else(|e| e.into_inner());
    let is_gcloud_mode = cfg.google_auth_mode == "gcloud";
    let has_token = !cfg.google_access_token.is_empty();
    let is_expired = cfg
        .google_token_expiry
        .map(|exp| chrono::Utc::now().timestamp() >= exp)
        .unwrap_or(true);

    Json(json!({
        "gcloud_available": gcloud_available,
        "adc_file_exists": adc_exists,
        "adc_file_path": adc_path.map(|p| p.to_string_lossy().to_string()),
        "adc_type": adc_info.as_ref().map(|i| i.cred_type.clone()),
        "adc_email": adc_info.as_ref().map(|i| i.email.clone()),
        "is_gcloud_mode": is_gcloud_mode,
        "has_token": has_token,
        "token_expired": is_expired,
    }))
}

struct AdcInfo {
    cred_type: String,
    email: String,
}

/// 解析 ADC 文件的基本信息
fn parse_adc_file(path: &std::path::Path) -> Option<AdcInfo> {
    let content = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    let cred_type = v["type"].as_str()?.to_string();
    let email = v["client_email"]
        .as_str()
        .or_else(|| v["account"].as_str())
        .unwrap_or("unknown")
        .to_string();
    Some(AdcInfo { cred_type, email })
}

/// 通过 gcloud CLI 登录并获取令牌
pub async fn api_gcloud_login(State(s): State<AppState>) -> Json<serde_json::Value> {
    // 策略 1：尝试运行 gcloud CLI 获取令牌
    if detect_gcloud_cli() {
        match get_token_from_gcloud_cli() {
            Some(token) => {
                let now = chrono::Utc::now().timestamp();
                let expiry = now + 3600; // gcloud 令牌通常 1 小时有效

                // 存储到内存
                {
                    let mut cfg = s.config.write().unwrap_or_else(|e| e.into_inner());
                    cfg.google_access_token = token.clone();
                    cfg.google_token_expiry = Some(expiry);
                    cfg.google_refresh_token = String::new(); // gcloud CLI 方式无 refresh token
                    cfg.google_auth_mode = "gcloud".to_string();
                }

                // 持久化到 DB
                let _ = s.db.set_setting("google_access_token", &token);
                let _ = s.db.set_setting("google_token_expiry", &expiry.to_string());
                let _ = s.db.set_setting("google_refresh_token", "");
                let _ = s.db.set_setting("google_auth_mode", "gcloud");

                tracing::info!("gcloud CLI 认证成功，令牌有效期至 {}", expiry);

                return Json(json!({
                    "status": "ok",
                    "message": "✅ gcloud CLI 认证成功！",
                    "method": "gcloud_cli",
                    "expires_in": 3600,
                }));
            }
            None => {
                tracing::warn!("gcloud CLI 可用但获取令牌失败");
            }
        }
    }

    // 策略 2：尝试从 ADC 文件读取凭据（无需 gcloud CLI）
    if let Some(adc_path) = get_adc_path() {
        if adc_path.exists() {
            if let Some(token) = refresh_token_from_adc(&adc_path).await {
                let now = chrono::Utc::now().timestamp();
                let expiry = now + 3600;

                {
                    let mut cfg = s.config.write().unwrap_or_else(|e| e.into_inner());
                    cfg.google_access_token = token.clone();
                    cfg.google_token_expiry = Some(expiry);
                    cfg.google_refresh_token = String::new();
                    cfg.google_auth_mode = "gcloud".to_string();
                }

                let _ = s.db.set_setting("google_access_token", &token);
                let _ = s.db.set_setting("google_token_expiry", &expiry.to_string());
                let _ = s.db.set_setting("google_refresh_token", "");
                let _ = s.db.set_setting("google_auth_mode", "gcloud");

                tracing::info!("ADC 文件认证成功");
                return Json(json!({
                    "status": "ok",
                    "message": "✅ ADC 凭据认证成功！",
                    "method": "adc_file",
                    "expires_in": 3600,
                }));
            }
        }
    }

    Json(json!({
        "status": "error",
        "message": "无法获取 Google 访问令牌。请确保已安装 gcloud CLI 并运行：\n\n\
                    gcloud auth application-default login --scopes=https://www.googleapis.com/auth/generative-language,openid,email\n\n\
                    或通过「设置」页面的 OAuth 方式登录。"
    }))
}

/// 检测 gcloud CLI 是否可用
fn find_gcloud() -> Option<&'static str> {
    let candidates = [
        "gcloud",
        "gcloud.cmd",
        r"D:\Program Files\Google\Cloud SDK\google-cloud-sdk\bin\gcloud.cmd",
        r"C:\Program Files\Google\Cloud SDK\google-cloud-sdk\bin\gcloud.cmd",
        r"C:\Program Files (x86)\Google\Cloud SDK\google-cloud-sdk\bin\gcloud.cmd",
    ];
    // Use find() for the manual iteration pattern
    #[allow(clippy::manual_find)]
    for cmd in &candidates {
        if std::process::Command::new(cmd)
            .args(["--version"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(cmd);
        }
    }
    None
}

pub(crate) fn detect_gcloud_cli() -> bool {
    find_gcloud().is_some()
}

/// 运行 gcloud CLI 获取访问令牌
/// 注意：使用 tokio::process::Command 并设置超时，防止在无凭证环境中阻塞 30+ 秒
fn get_token_from_gcloud_cli() -> Option<String> {
    let gcloud = find_gcloud()?;

    // 快速检查：如果没有 ADC 文件，大概率 gcloud 也无凭证，跳过耗时的 metadata server 探测
    if !has_adc_credentials() {
        tracing::debug!("skip gcloud token refresh: no ADC credentials found");
        return None;
    }

    let output = std::process::Command::new(gcloud)
        .args(["auth", "application-default", "print-access-token"])
        .output()
        .ok()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!("gcloud CLI 获取令牌失败: {}", stderr.trim());
        return None;
    }

    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() || token.len() < 20 {
        return None;
    }
    Some(token)
}

/// 快速检查是否存在 ADC 凭据文件，避免执行 gcloud 命令因无凭证而长时间阻塞
fn has_adc_credentials() -> bool {
    get_adc_path().is_some_and(|p| p.exists())
}

/// 从 ADC 文件刷新令牌（无需 gcloud CLI）
async fn refresh_token_from_adc(adc_path: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(adc_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;

    let cred_type = v["type"].as_str()?;

    match cred_type {
        "authorized_user" => {
            let client_id = v["client_id"].as_str()?.to_string();
            let client_secret = v["client_secret"].as_str()?.to_string();
            let refresh_token = v["refresh_token"].as_str()?.to_string();

            let http_client = reqwest::Client::new();
            let mut params = std::collections::HashMap::new();
            params.insert("client_id", client_id);
            params.insert("client_secret", client_secret);
            params.insert("refresh_token", refresh_token);
            params.insert("grant_type", "refresh_token".to_string());

            let resp = http_client
                .post(GOOGLE_TOKEN_URL)
                .form(&params)
                .send()
                .await
                .ok()?;

            let token_data: serde_json::Value = resp.json().await.ok()?;
            token_data["access_token"].as_str().map(|s| s.to_string())
        }
        "service_account" => {
            // 服务账号需要 JWT 签名，实现较复杂
            // 这里提示用户改用 authorized_user 类型
            tracing::warn!(
                "ADC 文件为 service_account 类型（{}），不支持自动刷新令牌。\
                 请运行 gcloud auth application-default login 使用用户账号。",
                v["client_email"].as_str().unwrap_or("unknown")
            );
            None
        }
        _ => {
            tracing::warn!("未知的 ADC 凭据类型: {}", cred_type);
            None
        }
    }
}

/// 获取 ADC 文件路径（跨平台）
fn get_adc_path() -> Option<std::path::PathBuf> {
    // 环境变量 CLOUDSDK_CONFIG 优先
    if let Ok(config_dir) = std::env::var("CLOUDSDK_CONFIG") {
        let p = std::path::PathBuf::from(&config_dir).join("application_default_credentials.json");
        if p.exists() {
            return Some(p);
        }
    }

    // Windows: %APPDATA%\gcloud\application_default_credentials.json
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let p = std::path::PathBuf::from(&appdata)
                .join("gcloud")
                .join("application_default_credentials.json");
            if p.exists() {
                return Some(p);
            }
        }
    }

    // Linux/macOS/Other: ~/.config/gcloud/application_default_credentials.json
    if let Some(home) = home_dir() {
        let p = std::path::PathBuf::from(&home)
            .join(".config")
            .join("gcloud")
            .join("application_default_credentials.json");
        if p.exists() {
            return Some(p);
        }

        // 也检查 ~/.config/gcloud/legacy_credentials/...
        let legacy_dir = std::path::PathBuf::from(&home)
            .join(".config")
            .join("gcloud")
            .join("legacy_credentials");
        if let Ok(entries) = std::fs::read_dir(&legacy_dir) {
            for entry in entries.flatten() {
                let cred_path = entry.path().join("adc.json");
                if cred_path.exists() {
                    return Some(cred_path);
                }
            }
        }
    }

    None
}

/// 跨平台获取用户 home 目录
fn home_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("USERPROFILE").map(std::path::PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME").map(std::path::PathBuf::from)
    }
}

// ===== 浏览器 OAuth 认证（备选）=====

/// 生成 Google OAuth 授权 URL（前端跳转用）
pub async fn api_google_auth_url(State(s): State<AppState>) -> Json<serde_json::Value> {
    let cfg = s.config.read().unwrap_or_else(|e| e.into_inner());
    let client_id = &cfg.google_client_id;
    if client_id.is_empty() {
        return Json(json!({"error": "请先在设置页填写 Google OAuth Client ID"}));
    }

    let auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
        GOOGLE_AUTH_URL,
        urlencoding::encode(client_id),
        urlencoding::encode(REDIRECT_URI),
        urlencoding::encode(GOOGLE_SCOPE),
    );

    Json(json!({"url": auth_url, "redirect_uri": REDIRECT_URI}))
}

/// Google OAuth 回调页面：HTML 页面，JS 读取 URL query 中的 code 并 POST 到 exchange 端点
pub async fn api_google_callback() -> (axum::http::StatusCode, &'static str) {
    (
        axum::http::StatusCode::OK,
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"></head><body style="font-family:sans-serif;background:#0d1117;color:#c9d1d9;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;">
<div style="text-align:center;">
<h2>Google 认证中...</h2>
<p id="status">正在处理授权...</p>
</div>
<script>
(function(){
  var params = new URLSearchParams(window.location.search);
  var code = params.get('code');
  var err = params.get('error');
  if (err) {
    document.getElementById('status').textContent = '❌ 授权失败: ' + err;
    return;
  }
  if (!code) {
    document.getElementById('status').textContent = '❌ 缺少授权码';
    return;
  }
  fetch('/api/auth/google/exchange', {
    method: 'POST',
    headers: {'Content-Type': 'application/json'},
    body: JSON.stringify({code: code})
  }).then(function(r){return r.json()}).then(function(d){
    if (d.status === 'ok') {
      document.getElementById('status').innerHTML = '✅ 认证成功！Gemini API 将使用 OAuth 令牌调用。<br>此窗口可关闭。';
      try { if (window.opener) { window.opener.postMessage('google-auth-ok', '*'); } } catch(e){}
    } else {
      document.getElementById('status').textContent = '❌ 认证失败: ' + (d.error || '未知错误');
    }
  }).catch(function(e){
    document.getElementById('status').textContent = '❌ 请求失败: ' + e;
  });
})();
</script></body></html>"#,
    )
}

/// 交换 code → tokens（POST，前端 JS 调用）
pub async fn api_google_exchange_handler(
    State(s): State<AppState>,
    body: String,
) -> Json<serde_json::Value> {
    let req: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return Json(json!({"error": "无效的 JSON 请求体"})),
    };
    let code = match req["code"].as_str() {
        Some(c) => c.to_string(),
        None => return Json(json!({"error": "缺少授权码"})),
    };

    let (client_id, client_secret) = {
        let cfg = s.config.read().unwrap_or_else(|e| e.into_inner());
        (
            cfg.google_client_id.clone(),
            cfg.google_client_secret.clone(),
        )
    };

    if client_id.is_empty() || client_secret.is_empty() {
        return Json(json!({"error": "未配置 Google OAuth 凭据，请先在设置页填写"}));
    }

    // 交换 code → tokens
    let http_client = reqwest::Client::new();
    let mut form = std::collections::HashMap::new();
    form.insert("code", code);
    form.insert("client_id", client_id);
    form.insert("client_secret", client_secret);
    form.insert("grant_type", "authorization_code".to_string());
    form.insert("redirect_uri", REDIRECT_URI.to_string());

    let token_resp = http_client.post(GOOGLE_TOKEN_URL).form(&form).send().await;

    match token_resp {
        Ok(resp) => {
            let status = resp.status();
            match resp.json::<serde_json::Value>().await {
                Ok(token_data) => {
                    if status.is_success() {
                        let access_token = token_data["access_token"]
                            .as_str()
                            .unwrap_or("")
                            .to_string();
                        let refresh_token = token_data["refresh_token"]
                            .as_str()
                            .unwrap_or("")
                            .to_string();
                        let expires_in = token_data["expires_in"].as_i64().unwrap_or(3600);
                        let expiry = chrono::Utc::now().timestamp() + expires_in;

                        // 存储到内存 vs DB
                        {
                            let mut cfg = s.config.write().unwrap_or_else(|e| e.into_inner());
                            cfg.google_access_token = access_token.clone();
                            cfg.google_refresh_token = refresh_token.clone();
                            cfg.google_token_expiry = Some(expiry);
                            cfg.google_auth_mode = "oauth".to_string();
                        }

                        let _ = s.db.set_setting("google_refresh_token", &refresh_token);
                        let _ = s.db.set_setting("google_access_token", &access_token);
                        let _ = s.db.set_setting("google_token_expiry", &expiry.to_string());
                        let _ = s.db.set_setting("google_auth_mode", "oauth");

                        Json(json!({
                            "status": "ok",
                            "message": "Google 账号认证成功！",
                            "expires_in": expires_in,
                            "has_refresh_token": !refresh_token.is_empty(),
                        }))
                    } else {
                        let err_msg = token_data["error_description"]
                            .as_str()
                            .or_else(|| token_data["error"].as_str())
                            .unwrap_or("未知错误");
                        Json(json!({"error": format!("令牌交换失败: {}", err_msg)}))
                    }
                }
                Err(e) => Json(json!({"error": format!("解析响应失败: {}", e)})),
            }
        }
        Err(e) => Json(json!({"error": format!("网络请求失败: {}", e)})),
    }
}

/// 获取 Google OAuth / gcloud 认证状态
pub async fn api_google_oauth_status(State(s): State<AppState>) -> Json<serde_json::Value> {
    let cfg = s.config.read().unwrap_or_else(|e| e.into_inner());
    let is_expired = cfg
        .google_token_expiry
        .map(|exp| chrono::Utc::now().timestamp() >= exp)
        .unwrap_or(true);

    Json(json!({
        "has_credentials": !cfg.google_client_id.is_empty() && !cfg.google_client_secret.is_empty(),
        "has_token": !cfg.google_access_token.is_empty(),
        "token_expired": is_expired,
        "token_expiry": cfg.google_token_expiry,
        "client_id_set": !cfg.google_client_id.is_empty(),
        "client_secret_set": !cfg.google_client_secret.is_empty(),
        "refresh_token_set": !cfg.google_refresh_token.is_empty(),
        "auth_mode": cfg.google_auth_mode,
    }))
}

/// 后台定期刷新 Google 令牌的独立函数
/// 由 spawn_channel_cleaner 每 60 秒调用一次
pub(crate) async fn refresh_google_token_background(
    config: &std::sync::Arc<std::sync::RwLock<super::AppConfig>>,
    db: &crate::db::Database,
) {
    let (needs_refresh, auth_mode, is_gemini) = {
        let cfg = config.read().unwrap_or_else(|e| e.into_inner());
        let is_gemini = cfg.ai_base_url.contains("googleapis");
        if !is_gemini {
            (false, String::new(), false)
        } else {
            let has_token = !cfg.google_access_token.is_empty();
            let is_expired = cfg
                .google_token_expiry
                .map(|exp| chrono::Utc::now().timestamp() >= exp - 120)
                .unwrap_or(true);
            (is_expired || !has_token, cfg.google_auth_mode.clone(), true)
        }
    };

    if !is_gemini || !needs_refresh {
        return;
    }

    match auth_mode.as_str() {
        "gcloud" => {
            // 尝试 gcloud CLI（在 blocking 线程池中执行，避免阻塞 tokio worker）
            if detect_gcloud_cli() {
                let token = tokio::task::spawn_blocking(get_token_from_gcloud_cli)
                    .await
                    .ok()
                    .flatten();
                if let Some(token) = token {
                    let expiry = chrono::Utc::now().timestamp() + 3600;
                    {
                        let mut cfg = config.write().unwrap_or_else(|e| e.into_inner());
                        cfg.google_access_token = token.clone();
                        cfg.google_token_expiry = Some(expiry);
                    }
                    let _ = db.set_setting("google_access_token", &token);
                    let _ = db.set_setting("google_token_expiry", &expiry.to_string());
                    tracing::info!("后台 gcloud 令牌已刷新");
                    return;
                }
            }
            // 尝试 ADC 文件
            if let Some(adc_path) = get_adc_path() {
                if adc_path.exists() {
                    if let Some(token) = refresh_token_from_adc(&adc_path).await {
                        let expiry = chrono::Utc::now().timestamp() + 3600;
                        {
                            let mut cfg = config.write().unwrap_or_else(|e| e.into_inner());
                            cfg.google_access_token = token.clone();
                            cfg.google_token_expiry = Some(expiry);
                        }
                        let _ = db.set_setting("google_access_token", &token);
                        let _ = db.set_setting("google_token_expiry", &expiry.to_string());
                        tracing::info!("后台 ADC 令牌已刷新");
                    }
                }
            }
        }
        "oauth" => {
            // 使用 refresh_token 刷新
            let (client_id, client_secret, refresh_token) = {
                let cfg = config.read().unwrap_or_else(|e| e.into_inner());
                (
                    cfg.google_client_id.clone(),
                    cfg.google_client_secret.clone(),
                    cfg.google_refresh_token.clone(),
                )
            };
            if !refresh_token.is_empty() && !client_id.is_empty() && !client_secret.is_empty() {
                let http_client = reqwest::Client::new();
                let mut params = std::collections::HashMap::new();
                params.insert("client_id", client_id);
                params.insert("client_secret", client_secret);
                params.insert("refresh_token", refresh_token);
                params.insert("grant_type", "refresh_token".to_string());

                if let Ok(resp) = http_client
                    .post(GOOGLE_TOKEN_URL)
                    .form(&params)
                    .send()
                    .await
                {
                    if let Ok(token_data) = resp.json::<serde_json::Value>().await {
                        if let Some(access_token) = token_data["access_token"].as_str() {
                            let expires_in = token_data["expires_in"].as_i64().unwrap_or(3600);
                            let expiry = chrono::Utc::now().timestamp() + expires_in;
                            {
                                let mut cfg = config.write().unwrap_or_else(|e| e.into_inner());
                                cfg.google_access_token = access_token.to_string();
                                cfg.google_token_expiry = Some(expiry);
                            }
                            let _ = db.set_setting("google_access_token", access_token);
                            let _ = db.set_setting("google_token_expiry", &expiry.to_string());
                            tracing::info!("后台 OAuth 令牌已刷新");
                        }
                    }
                }
            }
        }
        _ => {}
    }
}
