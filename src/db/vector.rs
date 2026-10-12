use super::Database;
use anyhow::Result;
use std::collections::HashMap;

// ─── Free functions: char-n-gram TF-IDF embedding (single canonical impl) ───
//
// These functions live in `db::vector` (not `vector::mod`) because `db` is
// declared in BOTH `main.rs` and `lib.rs`, while `vector` is only in `lib.rs`.
// Putting them here makes them accessible from the binary target
// (`crate::db::vector::compute_tfidf_embedding`) without touching red-line
// files (`main.rs` / `lib.rs`).
//
// KNOWN LIMITATION (M-B §0 decision, intentionally not fixed this round):
//   - No IDF weighting (no corpus statistics); all terms treated equally.
//   - The sorted-then-padded approach discards the term→dimension mapping,
//     so query and document vectors are NOT in the same comparable space.
//     Cosine similarity values do NOT constitute semantic relevance.
//   - This is acceptable for M-B: we only promise "usable + no panic +
//     single implementation", not retrieval quality.
//   - Re-evaluate after MB3 when real chunk corpus is available.

const EMBEDDING_DIM: usize = 512;

/// Compute a char-n-gram TF-IDF embedding for the given text.
///
/// Uses `Vec<char>` windowing to guarantee char-boundary safety regardless
/// of the input's UTF-8 byte layout. The previous byte-slice approach
/// (`cleaned[i..i+n]`) panicked on Chinese text because `i` iterated over
/// byte indices but multi-byte chars make most offsets non-char-boundaries.
///
/// Output is always exactly `EMBEDDING_DIM` (512) dimensions.
pub fn compute_tfidf_embedding(text: &str) -> Vec<f32> {
    let cleaned: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    let mut tf: HashMap<String, f32> = HashMap::new();
    for n in 2..=4 {
        if cleaned.len() >= n {
            for i in 0..=cleaned.len() - n {
                let gram: String = cleaned[i..i + n].iter().collect();
                *tf.entry(gram).or_insert(0.0) += 1.0;
            }
        }
    }
    if tf.is_empty() {
        return vec![0.0f32; EMBEDDING_DIM];
    }
    let doc_len = tf.values().sum::<f32>();
    for count in tf.values_mut() {
        *count = 1.0 + (*count / doc_len).log2();
    }
    let norm_sq: f32 = tf.values().map(|v| v * v).sum();
    let norm = if norm_sq > 0.0 { norm_sq.sqrt() } else { 1.0 };
    let mut emb: Vec<f32> = tf.values().map(|v| v / norm).collect();
    emb.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    if emb.len() < EMBEDDING_DIM {
        emb.resize(EMBEDDING_DIM, 0.0);
    } else {
        emb.truncate(EMBEDDING_DIM);
    }
    emb
}

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// Try to compute and persist an embedding for a patent.
///
/// **Failure is non-fatal**: embedding is an optional enhancement, not a
/// prerequisite for patent insertion. Errors are logged via `tracing::warn!`
/// and never propagated — the caller's `insert_patent` result is unaffected.
pub fn try_compute_and_save_embedding(db: &Database, patent_id: &str, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let embedding = compute_tfidf_embedding(text);
    if let Err(e) = db.save_patent_embedding(patent_id, &embedding, "char-tfidf-v1") {
        tracing::warn!(
            "Embedding computation failed for patent {}: {}",
            patent_id,
            e
        );
    }
}

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
        let hash: u64 = embedding.iter().fold(0u64, |acc, f| {
            acc.wrapping_add(f.to_bits() as u64) ^ 0x9E3779B9u64
        });
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
        let mut stmt = c.prepare("SELECT embedding FROM patents_embedding WHERE patent_id = ?1")?;
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

        if let Some(emb) = rows.flatten().next() {
            return Ok(Some(emb));
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
             LIMIT 500",
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
