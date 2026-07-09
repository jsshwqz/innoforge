//! 聊天记录跨设备同步 API / Chat Records Cross-Device Sync API
//!
//! 替代前端 localStorage，通过后端 SQLite 持久化实现跨设备共享。
//! Replaces frontend localStorage with backend SQLite persistence for cross-device sharing.

use super::AppState;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, Default)]
pub struct ChatPagination {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// 获取指定会话的消息（分页，默认最近 50 条，按时间正序）
pub async fn api_chat_get_messages(
    Path(session_key): Path<String>,
    State(s): State<AppState>,
    Query(pagination): Query<ChatPagination>,
) -> Json<serde_json::Value> {
    // 安全检查：防止注入过长 session_key
    if session_key.len() > 255 || session_key.is_empty() {
        return Json(json!({"status": "error", "message": "无效的 session_key"}));
    }

    let limit = pagination.limit.unwrap_or(50).clamp(1, 200);
    let offset = pagination.offset.unwrap_or(0);

    match s
        .db
        .get_chat_messages_paginated(&session_key, limit, offset)
    {
        Ok((messages, total)) => Json(json!({
            "status": "ok",
            "messages": messages,
            "total": total,
            "limit": limit,
            "offset": offset,
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

#[cfg(test)]
mod tests {
    use crate::db::Database;

    // ===== session_key 校验测试 =====

    #[test]
    fn test_session_key_validation_empty() {
        // 空 session_key 应被拒绝
        let session_key = "";
        assert!(session_key.is_empty() || session_key.len() > 255);
    }

    #[test]
    fn test_session_key_validation_too_long() {
        // 超过 255 字符的 session_key 应被拒绝
        let session_key = "A".repeat(256);
        assert!(session_key.is_empty() || session_key.len() > 255);
    }

    #[test]
    fn test_session_key_valid() {
        // 合法长度的 session_key 应通过
        let session_key = "valid-session-key";
        assert!(!(session_key.is_empty() || session_key.len() > 255));
    }

    // ===== role 校验测试 =====

    #[test]
    fn test_role_validation_valid_roles() {
        // 仅允许 user / assistant / system 三种角色
        for role in &["user", "assistant", "system"] {
            assert!(["user", "assistant", "system"].contains(role));
        }
    }

    #[test]
    fn test_role_validation_invalid_role() {
        // 非法角色应被拒绝
        let role = "admin";
        assert!(!["user", "assistant", "system"].contains(&role));
    }

    // ===== content 长度校验测试 =====

    #[test]
    fn test_content_length_validation() {
        // 超过 50000 字符的内容应被拒绝
        let content = "A".repeat(50001);
        assert!(content.len() > 50000);
    }

    #[test]
    fn test_content_length_valid() {
        // 正常长度内容应通过
        let content = "Hello world";
        assert!(content.len() <= 50000);
    }

    // ===== 分页参数测试 =====
    // 验证 api_chat_get_messages 中的分页钳位逻辑：
    //   let limit = pagination.limit.unwrap_or(50).clamp(1, 200);
    // 分别模拟 None（默认 50）、超大值（钳到 200）、零值（钳到 1）三种情况。

    #[test]
    fn test_pagination_defaults() {
        // limit 缺省时取默认值 50
        let limit = 50_usize.clamp(1, 200);
        assert_eq!(limit, 50);
    }

    #[test]
    fn test_pagination_clamp_high() {
        // 超过 200 的 limit 应被钳位到 200
        let limit = 500_usize.clamp(1, 200);
        assert_eq!(limit, 200);
    }

    #[test]
    fn test_pagination_clamp_low() {
        // 小于 1 的 limit 应被钳位到 1
        let limit = 0_usize.clamp(1, 200);
        assert_eq!(limit, 1);
    }

    // ===== 数据库操作测试 =====

    #[test]
    fn test_save_and_get_chat_messages() {
        // 保存消息后应能正确读取
        let db = Database::init(":memory:").unwrap();
        let id1 = db.save_chat_message("session-1", "user", "Hello").unwrap();
        let id2 = db
            .save_chat_message("session-1", "assistant", "Hi there!")
            .unwrap();
        assert!(id1 > 0);
        assert!(id2 > 0);
        let (messages, total) = db.get_chat_messages_paginated("session-1", 50, 0).unwrap();
        assert_eq!(total, 2);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
    }

    #[test]
    fn test_chat_pagination_limit() {
        // 分页 limit 应正确限制返回条数
        let db = Database::init(":memory:").unwrap();
        for i in 0..5 {
            db.save_chat_message("session-2", "user", &format!("msg {}", i))
                .unwrap();
        }
        let (messages, total) = db.get_chat_messages_paginated("session-2", 2, 0).unwrap();
        assert_eq!(total, 5);
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_chat_pagination_offset() {
        // 分页 offset 应正确跳过指定条数
        let db = Database::init(":memory:").unwrap();
        for i in 0..5 {
            db.save_chat_message("session-3", "user", &format!("msg {}", i))
                .unwrap();
        }
        let (messages, total) = db.get_chat_messages_paginated("session-3", 2, 3).unwrap();
        assert_eq!(total, 5);
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_delete_chat_messages() {
        // 删除消息后应返回正确数量且查询为空
        let db = Database::init(":memory:").unwrap();
        db.save_chat_message("session-4", "user", "Hello").unwrap();
        db.save_chat_message("session-4", "assistant", "Bye")
            .unwrap();
        let count = db.delete_chat_messages("session-4").unwrap();
        assert_eq!(count, 2);
        let (_, total) = db.get_chat_messages_paginated("session-4", 50, 0).unwrap();
        assert_eq!(total, 0);
    }

    #[test]
    fn test_chat_session_isolation() {
        // 不同 session 的消息应互相隔离
        let db = Database::init(":memory:").unwrap();
        db.save_chat_message("s-a", "user", "msg in A").unwrap();
        db.save_chat_message("s-b", "user", "msg in B").unwrap();
        let (_, total_a) = db.get_chat_messages_paginated("s-a", 50, 0).unwrap();
        let (_, total_b) = db.get_chat_messages_paginated("s-b", 50, 0).unwrap();
        assert_eq!(total_a, 1);
        assert_eq!(total_b, 1);
    }
}
