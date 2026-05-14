//! 聊天记录跨设备同步 API / Chat Records Cross-Device Sync API
//!
//! 替代前端 localStorage，通过后端 SQLite 持久化实现跨设备共享。
//! Replaces frontend localStorage with backend SQLite persistence for cross-device sharing.

use super::AppState;
use axum::{extract::Path, extract::State, Json};
use serde_json::json;

/// 获取指定会话的所有消息（自动限制最近 200 条）
pub async fn api_chat_get_messages(
    Path(session_key): Path<String>,
    State(s): State<AppState>,
) -> Json<serde_json::Value> {
    // 安全检查：防止注入过长 session_key
    if session_key.len() > 255 || session_key.is_empty() {
        return Json(json!({"status": "error", "message": "无效的 session_key"}));
    }

    match s.db.get_chat_messages(&session_key) {
        Ok(messages) => Json(json!({
            "status": "ok",
            "messages": messages,
        })),
        Err(e) => Json(json!({
            "status": "error",
            "message": format!("查询聊天记录失败: {}", e),
        })),
    }
}

/// 保存一条聊天消息
pub async fn api_chat_save_message(
    Path(session_key): Path<String>,
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    if session_key.len() > 255 || session_key.is_empty() {
        return Json(json!({"status": "error", "message": "无效的 session_key"}));
    }

    let role = req["role"].as_str().unwrap_or("").trim();
    let content = req["content"].as_str().unwrap_or("").trim();

    if role.is_empty() || content.is_empty() {
        return Json(json!({"status": "error", "message": "role 和 content 不能为空"}));
    }

    if !["user", "assistant", "system"].contains(&role) {
        return Json(json!({"status": "error", "message": "role 必须是 user/assistant/system"}));
    }

    if content.len() > 50000 {
        return Json(json!({"status": "error", "message": "content 超过最大长度（50000 字符）"}));
    }

    match s.db.save_chat_message(&session_key, role, content) {
        Ok(id) => Json(json!({
            "status": "ok",
            "message_id": id,
        })),
        Err(e) => Json(json!({
            "status": "error",
            "message": format!("保存聊天记录失败: {}", e),
        })),
    }
}

/// 删除指定会话的所有消息
pub async fn api_chat_delete_messages(
    Path(session_key): Path<String>,
    State(s): State<AppState>,
) -> Json<serde_json::Value> {
    if session_key.len() > 255 || session_key.is_empty() {
        return Json(json!({"status": "error", "message": "无效的 session_key"}));
    }

    match s.db.delete_chat_messages(&session_key) {
        Ok(count) => Json(json!({
            "status": "ok",
            "deleted": count,
        })),
        Err(e) => Json(json!({
            "status": "error",
            "message": format!("删除聊天记录失败: {}", e),
        })),
    }
}
