//! 聊天记录持久化 / Chat Record Persistence
//!
//! 支持跨设备同步的聊天消息存储，替代 localStorage。
//! Cross-device chat message storage, replacing localStorage.

use anyhow::Result;
use rusqlite::params;

/// 单条聊天消息 / Single chat message
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

impl super::Database {
    /// 获取指定会话的全部消息（按时间正序）
    pub fn get_chat_messages(&self, session_key: &str) -> Result<Vec<ChatMessage>> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT id, role, content, created_at FROM chat_records \
             WHERE session_key = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![session_key], |r| {
            Ok(ChatMessage {
                id: r.get(0)?,
                role: r.get(1)?,
                content: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?;
        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        Ok(messages)
    }

    /// 保存一条聊天消息
    pub fn save_chat_message(&self, session_key: &str, role: &str, content: &str) -> Result<i64> {
        let c = self.conn();
        c.execute(
            "INSERT INTO chat_records (session_key, role, content) VALUES (?1, ?2, ?3)",
            params![session_key, role, content],
        )?;
        Ok(c.last_insert_rowid())
    }

    /// 删除指定会话的所有消息
    pub fn delete_chat_messages(&self, session_key: &str) -> Result<usize> {
        let c = self.conn();
        let deleted = c.execute(
            "DELETE FROM chat_records WHERE session_key = ?1",
            params![session_key],
        )?;
        Ok(deleted)
    }
}
