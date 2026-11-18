//! T-103 集成测试：回环免鉴权、非回环强制同源 Cookie/Bearer 鉴权与密钥脱敏校验。

use std::net::{IpAddr, Ipv4Addr};

use innoforge::app::{
    build_app_router, resolve_bind_ip, verify_json_masks_secrets, AuthGuardConfig,
};
use innoforge::common::init_app_state;
use reqwest::{header, Client, StatusCode};
use serde_json::{json, Value};

async fn spawn_router(router: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    format!("http://{}", addr)
}

#[test]
fn bind_ip_defaults_to_loopback_and_non_loopback_enforces_auth() {
    std::env::remove_var("INNOFORGE_BIND_HOST");
    std::env::remove_var("INNOFORGE_ACCESS_TOKEN");

    let default_ip = resolve_bind_ip();
    assert!(
        default_ip.is_loopback(),
        "default bind IP must be loopback, got {default_ip}"
    );

    let loopback_auth = AuthGuardConfig::from_bind_and_env(IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert!(!loopback_auth.enforce_auth);

    let non_loopback_auth = AuthGuardConfig::from_bind_and_env(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    assert!(non_loopback_auth.enforce_auth);
    assert_eq!(non_loopback_auth.access_token.len(), 32);
}

#[tokio::test]
async fn auth_guard_enforces_cookie_and_bearer_and_masks_secrets() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db_path = temp.path().join("auth_guard.db");
    let state = init_app_state(&db_path.to_string_lossy()).expect("init_app_state");

    let ai_secret = "sk-test-ai-secret-12345678";
    let serp_secret = "serp-test-secret-87654321";
    let epo_secret = "epo-test-secret-abcdefgh";
    {
        let mut cfg = state.config.write().unwrap_or_else(|e| e.into_inner());
        cfg.ai_base_url = "https://custom.example.com/v1".to_string();
        cfg.ai_api_key = ai_secret.to_string();
        cfg.serpapi_keys = vec![serp_secret.to_string()];
        cfg.epo_secret = epo_secret.to_string();
    }

    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("reqwest client");

    // 1. 回环模式免鉴权直通
    let loopback_url = spawn_router(build_app_router(
        state.clone(),
        AuthGuardConfig {
            enforce_auth: false,
            access_token: String::new(),
        },
    ))
    .await;
    let resp = client
        .get(format!("{}/api/settings", loopback_url))
        .send()
        .await
        .expect("loopback settings request");
    assert_eq!(resp.status(), StatusCode::OK);

    // 2. 非回环 / enforce_auth=true 模式
    let token = "test-token-99887766";
    let guarded_url = spawn_router(build_app_router(
        state,
        AuthGuardConfig {
            enforce_auth: true,
            access_token: token.to_string(),
        },
    ))
    .await;

    // 2a. 未带凭证访问 /api/settings 必拦 401
    let unauth_resp = client
        .get(format!("{}/api/settings", guarded_url))
        .send()
        .await
        .expect("unauth settings request");
    assert_eq!(unauth_resp.status(), StatusCode::UNAUTHORIZED);
    let unauth_json: Value = unauth_resp.json().await.expect("unauth json");
    assert_eq!(unauth_json["code"], "auth_required");

    // 2b. /api/healthz 免鉴权直通 200
    let health_resp = client
        .get(format!("{}/api/healthz", guarded_url))
        .send()
        .await
        .expect("healthz request");
    assert_eq!(health_resp.status(), StatusCode::OK);
    let health_json: Value = health_resp.json().await.expect("healthz json");
    assert_eq!(health_json["status"], "ok");

    // 2c. POST /api/auth/unlock 错误码返回 401
    let bad_unlock = client
        .post(format!("{}/api/auth/unlock", guarded_url))
        .json(&json!({ "access_code": "wrong-code" }))
        .send()
        .await
        .expect("bad unlock request");
    assert_eq!(bad_unlock.status(), StatusCode::UNAUTHORIZED);

    // 2d. POST /api/auth/unlock 正确码返回 200 且带 HttpOnly; SameSite=Strict Cookie
    let good_unlock = client
        .post(format!("{}/api/auth/unlock", guarded_url))
        .json(&json!({ "access_code": token }))
        .send()
        .await
        .expect("good unlock request");
    assert_eq!(good_unlock.status(), StatusCode::OK);
    let set_cookie = good_unlock
        .headers()
        .get(header::SET_COOKIE)
        .expect("Set-Cookie header")
        .to_str()
        .expect("Set-Cookie utf8")
        .to_string();
    assert!(set_cookie.contains(&format!("innoforge_session={token}")));
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Strict"));

    // 2e. 持 Cookie 访问 /api/settings 返回 200 且零明文密钥泄露
    let cookie_resp = client
        .get(format!("{}/api/settings", guarded_url))
        .header(header::COOKIE, format!("innoforge_session={token}"))
        .send()
        .await
        .expect("cookie settings request");
    assert_eq!(cookie_resp.status(), StatusCode::OK);
    let cookie_json: Value = cookie_resp.json().await.expect("cookie settings json");
    assert!(
        verify_json_masks_secrets(&cookie_json, &[ai_secret, serp_secret, epo_secret]),
        "cookie /api/settings leaked plaintext secret: {cookie_json}"
    );

    // 2f. 持 Bearer 访问 /api/settings 返回 200 且零明文密钥泄露
    let bearer_resp = client
        .get(format!("{}/api/settings", guarded_url))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .expect("bearer settings request");
    assert_eq!(bearer_resp.status(), StatusCode::OK);
    let bearer_json: Value = bearer_resp.json().await.expect("bearer settings json");
    assert!(
        verify_json_masks_secrets(&bearer_json, &[ai_secret, serp_secret, epo_secret]),
        "bearer /api/settings leaked plaintext secret: {bearer_json}"
    );
}
