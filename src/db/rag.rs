use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatentChunk {
    pub id: String,
    pub patent_id: String,
    pub chunk_index: i32,
    pub source_type: String,
    pub content: String,
    pub embedding: Vec<f32>,
}

use super::Database;



impl Database {
    /// Save all chunks for a patent.
    pub fn save_patent_chunks(
        &self,
        patent_id: &str,
        chunks: &[PatentChunk],
        model_name: &str,
    ) -> Result<usize, rusqlite::Error> {
        let mut c = self.conn();
        let tx = c.transaction()?;

        // Delete old chunks for this patent
        tx.execute(
            "DELETE FROM patent_chunks WHERE patent_id = ?1",
            rusqlite::params![patent_id],
        )?;

        let mut count = 0;
        for chunk in chunks {
            let bytes: Vec<u8> = chunk
                .embedding
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect();
            tx.execute(
                "INSERT INTO patent_chunks (id, patent_id, chunk_index, source_type, content, embedding, model_name) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    chunk.id, patent_id, chunk.chunk_index,
                    chunk.source_type, chunk.content,
                    rusqlite::types::Value::Blob(bytes), model_name
                ],
            )?;
            count += 1;
        }

        tx.commit()?;
        Ok(count)
    }

    /// Search chunks for a patent by query text with cosine similarity.
    pub fn search_chunks(
        &self,
        patent_id: &str,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<(String, f32)>, rusqlite::Error> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT id, content FROM patent_chunks WHERE patent_id = ?1"
        )?;
        let rows = stmt.query_map(rusqlite::params![patent_id], |row: &rusqlite::Row| {
            let id: String = row.get(0)?;
            let content: String = row.get(1)?;
            let blob_val: rusqlite::types::Value = row.get(2)?;
            match blob_val {
                rusqlite::types::Value::Blob(bytes) => {
                    let mut emb = Vec::with_capacity(bytes.len() / 4);
                    for i in (0..bytes.len()).step_by(4) {
                        if i + 3 < bytes.len() {
                            let b = [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]];
                            emb.push(f32::from_le_bytes(b));
                        }
                    }
                    Ok((id, content, emb))
                }
                _ => Ok((id, content, vec![])),
            }
        })?;

        let mut scores: Vec<(String, f32)> = Vec::new();
        for row in rows {
            if let Ok((id, _content, emb)) = row {
                let sim = cosine_similarity(query, &emb);
                if sim > 0.05 {
                    scores.push((id, sim));
                }
            }
        }

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(limit);
        Ok(scores)
    }

    /// Get chunk by ID.
    pub fn get_chunk(&self, chunk_id: &str) -> Result<Option<PatentChunk>, rusqlite::Error> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT patent_id, chunk_index, source_type, content, embedding FROM patent_chunks WHERE id = ?1"
        )?;
        let rows = stmt.query_map(rusqlite::params![chunk_id], |row: &rusqlite::Row| {
            let patent_id: String = row.get(0)?;
            let chunk_index: i32 = row.get(1)?;
            let source_type: String = row.get(2)?;
            let content: String = row.get(3)?;
            let blob_val: rusqlite::types::Value = row.get(4)?;
            let embedding = match blob_val {
                rusqlite::types::Value::Blob(bytes) => {
                    let mut emb = Vec::with_capacity(bytes.len() / 4);
                    for i in (0..bytes.len()).step_by(4) {
                        if i + 3 < bytes.len() {
                            let b = [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]];
                            emb.push(f32::from_le_bytes(b));
                        }
                    }
                    emb
                }
                _ => vec![],
            };
            Ok(PatentChunk {
                id: chunk_id.to_string(),
                patent_id,
                chunk_index,
                source_type,
                content,
                embedding,
            })
        })?;

        for row in rows {
            if let Ok(chunk) = row {
                return Ok(Some(chunk));
            }
        }
        Ok(None)
    }

    /// Count chunks for a patent.
    pub fn count_chunks(&self, patent_id: &str) -> Result<i64, rusqlite::Error> {
        let c = self.conn();
        let count: i64 = c.query_row(
            "SELECT COUNT(*) FROM patent_chunks WHERE patent_id = ?1",
            rusqlite::params![patent_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }
    let dot: f32 = (0..len).map(|i| a[i] * b[i]).sum();
    let na: f32 = (0..len).map(|i| a[i] * a[i]).sum::<f32>().sqrt();
    let nb: f32 = (0..len).map(|i| b[i] * b[i]).sum::<f32>().sqrt();
    if na < 1e-8 || nb < 1e-8 {
        return 0.0;
    }
    (dot / (na * nb)).max(0.0).min(1.0)
}