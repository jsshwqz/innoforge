use super::Database;
use anyhow::Result;

impl Database {
    /// Save a patent embedding (INSERT OR REPLACE).
    pub fn save_patent_embedding(
        &self,
        patent_id: &str,
        embedding: &[f32],
        model_name: &str,
    ) -> Result<(), rusqlite::Error> {
        let c = self.conn();
        let bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        let hash: u64 = embedding
            .iter()
            .fold(0u64, |acc, f| acc.wrapping_add(f.to_bits() as u64) ^ 0x9E3779B9u64);
        let hash_str = hash.to_string();

        c.execute(
            "INSERT OR REPLACE INTO patents_embedding (patent_id, embedding, model_name, text_hash, updated_at) VALUES (?1, ?2, ?3, ?4, datetime('now'))",
            rusqlite::params![patent_id, rusqlite::types::Value::Blob(bytes), model_name, hash_str],
        )?;
        Ok(())
    }

    /// Get a patent embedding by ID.
    pub fn get_patent_embedding(
        &self,
        patent_id: &str,
    ) -> Result<Option<Vec<f32>>, rusqlite::Error> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT embedding FROM patents_embedding WHERE patent_id = ?1"
        )?;
        let rows = stmt.query_map(rusqlite::params![patent_id], |row: &rusqlite::Row| {
            let blob: rusqlite::types::Value = row.get(0)?;
            match blob {
                rusqlite::types::Value::Blob(bytes) => {
                    let mut embedding = Vec::with_capacity(bytes.len() / 4);
                    for i in (0..bytes.len()).step_by(4) {
                        if i + 3 < bytes.len() {
                            let bytes4 = [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]];
                            embedding.push(f32::from_le_bytes(bytes4));
                        }
                    }
                    Ok(embedding)
                }
                _ => Ok(vec![]),
            }
        })?;

        for row in rows {
            if let Ok(emb) = row {
                return Ok(Some(emb));
            }
        }
        Ok(None)
    }

    /// List patents that still need embedding.
    pub fn list_patents_needing_embedding(&self) -> Result<Vec<String>, rusqlite::Error> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT p.id FROM patents p
             LEFT JOIN patents_embedding pe ON p.id = pe.patent_id
             WHERE pe.patent_id IS NULL
             AND (p.description IS NOT NULL OR p.abstract_text IS NOT NULL)
             LIMIT 500"
        )?;
        let rows = stmt.query_map(rusqlite::params![], |row: &rusqlite::Row| {
            let id: String = row.get(0)?;
            Ok(id)
        })?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row?);
        }
        Ok(ids)
    }

    /// Count how many patents have embeddings.
    pub fn count_embeddings(&self) -> Result<i64, rusqlite::Error> {
        let c = self.conn();
        let count: i64 = c.query_row(
            "SELECT COUNT(*) FROM patents_embedding",
            rusqlite::params![],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}
