//! Persistent Memory System
//! Stores domain concepts, decisions, patterns, and open questions
//! extracted from completed pipeline runs for cross-session knowledge recall.

use super::Database;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeaMemory {
    pub id: String,
    pub idea_id: String,
    pub concept_name: String,
    pub concept_type: String,
    pub content: String,
    pub confidence: f64,
    pub source_step: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Database {
    pub fn save_memory(&self, entry: &IdeaMemory) -> Result<(), rusqlite::Error> {
        let c = self.conn();
        c.execute(
            "INSERT OR REPLACE INTO idea_memory (id, idea_id, concept_name, concept_type, content, confidence, source_step, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                entry.id, entry.idea_id, entry.concept_name, entry.concept_type,
                entry.content, entry.confidence, entry.source_step.as_deref(),
                entry.created_at, entry.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn save_memory_batch(&self, entries: &[IdeaMemory]) -> Result<(), rusqlite::Error> {
        if entries.is_empty() { return Ok(()); }
        let mut c = self.conn();
        let tx = c.transaction()?;
        for entry in entries {
            tx.execute(
                "INSERT OR REPLACE INTO idea_memory (id, idea_id, concept_name, concept_type, content, confidence, source_step, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    entry.id, entry.idea_id, entry.concept_name, entry.concept_type,
                    entry.content, entry.confidence, entry.source_step.as_deref(),
                    entry.created_at, entry.updated_at,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_memory_entries(&self, idea_id: &str) -> Result<Vec<IdeaMemory>, rusqlite::Error> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT id, idea_id, concept_name, concept_type, content, confidence, source_step, created_at, updated_at
             FROM idea_memory WHERE idea_id = ?1
             ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map(rusqlite::params![idea_id], |row: &rusqlite::Row| {
            Ok(IdeaMemory {
                id: row.get(0)?, idea_id: row.get(1)?, concept_name: row.get(2)?,
                concept_type: row.get(3)?, content: row.get(4)?, confidence: row.get(5)?,
                source_step: row.get(6)?, created_at: row.get(7)?, updated_at: row.get(8)?,
            })
        })?;
        let mut entries = Vec::new();
        for row in rows { entries.push(row?); }
        Ok(entries)
    }

    pub fn get_memory_by_type(&self, idea_id: &str, concept_type: &str) -> Result<Vec<IdeaMemory>, rusqlite::Error> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT id, idea_id, concept_name, concept_type, content, confidence, source_step, created_at, updated_at
             FROM idea_memory WHERE idea_id = ?1 AND concept_type = ?2
             ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map(rusqlite::params![idea_id, concept_type], |row: &rusqlite::Row| {
            Ok(IdeaMemory {
                id: row.get(0)?, idea_id: row.get(1)?, concept_name: row.get(2)?,
                concept_type: row.get(3)?, content: row.get(4)?, confidence: row.get(5)?,
                source_step: row.get(6)?, created_at: row.get(7)?, updated_at: row.get(8)?,
            })
        })?;
        let mut entries = Vec::new();
        for row in rows { entries.push(row?); }
        Ok(entries)
    }

    pub fn delete_memory(&self, idea_id: &str, entry_id: &str) -> Result<(), rusqlite::Error> {
        let c = self.conn();
        c.execute("DELETE FROM idea_memory WHERE idea_id = ?1 AND id = ?2", rusqlite::params![idea_id, entry_id])?;
        Ok(())
    }

    pub fn get_memory_counts(&self, idea_id: &str) -> Result<serde_json::Value, rusqlite::Error> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT concept_type, COUNT(*) as cnt FROM idea_memory
             WHERE idea_id = ?1 GROUP BY concept_type",
        )?;
        let rows = stmt.query_map(rusqlite::params![idea_id], |row: &rusqlite::Row| {
            let t: String = row.get(0)?;
            let cnt: i64 = row.get(1)?;
            Ok((t, cnt))
        })?;
        let mut map = serde_json::Map::new();
        for row in rows {
            if let Ok((t, cnt)) = row { map.insert(t, serde_json::json!(cnt)); }
        }
        let total: i64 = map.values().map(|v| v.as_i64().unwrap_or(0)).sum();
        Ok(serde_json::json!({ "total": total, "by_type": serde_json::Value::Object(map) }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_entry(id: &str) -> IdeaMemory {
        IdeaMemory {
            id: id.to_string(), idea_id: "idea-1".to_string(),
            concept_name: "固态电解质".to_string(), concept_type: "domain_concept".to_string(),
            content: "硫化物基固态电解质具有高离子电导率".to_string(),
            confidence: 0.85, source_step: Some("AiDeepAnalysis".to_string()),
            created_at: "2026-01-01".to_string(), updated_at: "2026-01-01".to_string(),
        }
    }
    #[test]
    fn memory_crud_roundtrip() {
        let db = Database::init(":memory:").expect("init");
        // Insert idea to satisfy FK constraint
        let _ = db.conn().execute(
            "INSERT INTO ideas (id, title, description, created_at, updated_at) VALUES ('idea-1', 'test', 'test', datetime('now'), datetime('now'))",
            rusqlite::params![],
        );
        let entry = make_entry("mem-1");
        db.save_memory(&entry).expect("save");
        let entries = db.get_memory_entries("idea-1").expect("get");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "mem-1");
        db.delete_memory("idea-1", "mem-1").expect("delete");
        let entries = db.get_memory_entries("idea-1").expect("get");
        assert_eq!(entries.len(), 0);
    }
    #[test]
    fn memory_batch_save_and_filter() {
        let db = Database::init(":memory:").expect("init");
        let _ = db.conn().execute(
            "INSERT INTO ideas (id, title, description, created_at, updated_at) VALUES ('idea-1', 'test', 'test', datetime('now'), datetime('now'))",
            rusqlite::params![],
        );
        let mut entries = vec![make_entry("mem-1"), make_entry("mem-2")];
        entries[1].concept_type = "decision".to_string();
        entries[1].concept_name = "选择硫化物路线".to_string();
        db.save_memory_batch(&entries).expect("save");
        let all = db.get_memory_entries("idea-1").expect("get");
        assert_eq!(all.len(), 2);
        let concepts = db.get_memory_by_type("idea-1", "domain_concept").expect("filter");
        assert_eq!(concepts.len(), 1);
    }
}
