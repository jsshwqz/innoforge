//! Regression for issue #68: first-run fake green light.
//!
//! 全新 INNOFORGE_HOME + 清空 AI_API_KEY* 环境变量启动 ⇒
//! - /api/settings 的 ai_api_key_configured == false
//! - 首页 HTML 包含「未配置」
//!
//! 负控说明：如果把 src/routes/mod.rs 中 is_placeholder_api_key 的 `"ollama"` 分支
//! 删掉（即占位符判定短路），默认 key "ollama" 会重新通过 !is_empty() 检查，
//! ai_api_key_configured 变回 true，本测试的 assert_eq!(configured, false) 会红。
//! 跑完负控后改回，git diff 应为空。

mod common;
use common::{free_port, spawn_test_server};

/// 正向：全新目录 + 清空环境变量 ⇒ ai_api_key_configured == false + 首页含「未配置」
#[test]
fn first_run_must_not_show_fake_green() {
    let port = free_port();

    // 不传任何 AI_API_KEY* 环境变量 —— 全新空库、未配置
    // spawn_test_server already sets INNOFORGE_PORT / INNOFORGE_NO_OPEN / INNOFORGE_HOME
    let server = spawn_test_server(port, &[]);

    let client = reqwest::blocking::Client::new();

    // 1) /api/settings — ai_api_key_configured must be false
    let settings_resp = client
        .get(format!("http://127.0.0.1:{}/api/settings", port))
        .send()
        .expect("settings must respond");
    assert!(settings_resp.status().is_success(), "settings status");
    let settings_json: serde_json::Value = settings_resp.json().expect("settings json");

    let configured = settings_json["ai_api_key_configured"]
        .as_bool()
        .expect("ai_api_key_configured must be a bool");
    assert!(
        !configured,
        "#68: 全新目录启动，ai_api_key_configured 必须为 false（不能假绿灯）"
    );

    // 2) 首页 HTML 必须包含「未配置」
    let index_resp = client
        .get(format!("http://127.0.0.1:{}/", port))
        .send()
        .expect("index must respond");
    assert!(index_resp.status().is_success(), "index status");
    let index_html = index_resp.text().expect("index text");
    assert!(
        index_html.contains("未配置"),
        "#68: 首页必须包含「未配置」文案，实际不含。首页前 500 字：{}",
        &index_html[..index_html.len().min(500)]
    );

    // 3) /api/ai/models 的 Ollama provider status 应为 error（服务不可达），
    //    与首页「未配置」/「未检测到服务」一致，不再矛盾
    let models_resp = client
        .get(format!("http://127.0.0.1:{}/api/ai/models", port))
        .send()
        .expect("models must respond");
    assert!(models_resp.status().is_success(), "models status");
    let models_json: serde_json::Value = models_resp.json().expect("models json");
    let providers = models_json["providers"]
        .as_array()
        .expect("providers must be an array");
    // Ollama 应在列表中且 status != "ok"（本地无 Ollama 服务）
    let ollama = providers
        .iter()
        .find(|p| p["name"].as_str() == Some("Ollama"));
    assert!(
        ollama.is_some(),
        "#68: /api/ai/models 应包含 Ollama provider"
    );
    if let Some(oll) = ollama {
        let status = oll["status"].as_str().unwrap_or("");
        assert_ne!(
            status, "ok",
            "#68: 本地无 Ollama 服务时 status 不应为 ok（否则与首页矛盾）"
        );
    }

    // server killed on drop
    drop(server);
}

/// 负控：把默认 key 设为非占位符的真实 key ⇒ ai_api_key_configured == true。
/// 这证明 is_placeholder_api_key 确实在起作用 —— 如果短路掉占位符判定，
/// 正向测试的 assert_eq!(configured, false) 就会红（因为 "ollama" 不再被排除）。
#[test]
fn real_key_must_show_configured() {
    let port = free_port();

    // 设一个真实的非占位符 key + 非 Ollama base_url
    let server = spawn_test_server(
        port,
        &[
            ("AI_API_KEY", "sk-real-key-for-negative-control-test-only"),
            ("AI_BASE_URL", "https://api.deepseek.com/v1"),
        ],
    );

    let client = reqwest::blocking::Client::new();
    let settings_resp = client
        .get(format!("http://127.0.0.1:{}/api/settings", port))
        .send()
        .expect("settings must respond");
    let settings_json: serde_json::Value = settings_resp.json().expect("settings json");
    let configured = settings_json["ai_api_key_configured"]
        .as_bool()
        .expect("ai_api_key_configured must be a bool");
    assert!(
        configured,
        "#68 负控：真实 key + 非 Ollama URL ⇒ configured 必须为 true"
    );

    drop(server);
}

/// Ollama URL + 默认 key "ollama" ⇒ ai_api_key_configured == false（本地服务不算"已配置"）
#[test]
fn ollama_default_must_not_show_configured() {
    let port = free_port();

    // 默认 base_url 就是 Ollama，默认 key 就是 "ollama" —— 不传任何 env
    let server = spawn_test_server(port, &[]);

    let client = reqwest::blocking::Client::new();
    let settings_resp = client
        .get(format!("http://127.0.0.1:{}/api/settings", port))
        .send()
        .expect("settings must respond");
    let settings_json: serde_json::Value = settings_resp.json().expect("settings json");
    let configured = settings_json["ai_api_key_configured"]
        .as_bool()
        .expect("ai_api_key_configured must be a bool");
    let is_ollama = settings_json["is_ollama"]
        .as_bool()
        .expect("is_ollama must be a bool");
    assert!(is_ollama, "#68: 默认 base_url 应识别为 Ollama");
    assert!(
        !configured,
        "#68: Ollama + 默认 key ⇒ configured 必须为 false"
    );

    drop(server);
}
