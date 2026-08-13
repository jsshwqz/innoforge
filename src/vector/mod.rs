//! 向量嵌入与混合搜索 / Vector Embedding & Hybrid Search
//!
//! 使用字符 n-gram + TF-IDF 实现轻量级本地嵌入，无需外部模型文件。
//! Lightweight local embedding using character n-gram TF-IDF, no external model files needed.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::db::Database;
use serde::{Deserialize, Serialize};

/// Character n-gram tokenizer for Chinese text.
#[derive(Debug, Clone)]
struct CharNGramTokenizer {
    min_n: usize,
    max_n: usize,
}

impl Default for CharNGramTokenizer {
    fn default() -> Self {
        Self { min_n: 2, max_n: 4 }
    }
}

impl CharNGramTokenizer {
    /// Tokenize text into character n-grams.
    fn tokenize(&self, text: &str) -> Vec<String> {
        let cleaned: String = text
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let mut grams = Vec::new();
        for n in self.min_n..=self.max_n {
            if cleaned.len() >= n {
                for i in 0..=cleaned.len() - n {
                    let gram: String = cleaned[i..i + n].chars().collect();
                    grams.push(gram);
                }
            }
        }
        grams
    }
}

/// Lazy-initialized TF-IDF vocabulary and document matrix.
/// For large corpora, only the vocabulary is cached; embeddings are computed on demand.
pub struct VectorIndex {
    tokenizer: CharNGramTokenizer,
    doc_embeddings: RwLock<HashMap<String, Vec<f32>>>,
}

impl Default for VectorIndex {
    fn default() -> Self {
        Self {
            tokenizer: CharNGramTokenizer::default(),
            doc_embeddings: RwLock::new(HashMap::new()),
        }
    }
}

impl VectorIndex {
    /// Compute TF-IDF embedding for a single text document.
    /// Returns a fixed-size vector (top-k features).
    pub fn compute_embedding(&self, text: &str) -> Vec<f32> {
        let tokens = self.tokenizer.tokenize(text);
        if tokens.is_empty() {
            return vec![0.0f32];
        }

        // Compute term frequency
        let mut tf: HashMap<String, f32> = HashMap::new();
        for token in &tokens {
            *tf.entry(token.clone()).or_insert(0.0) += 1.0;
        }
        let doc_len = tokens.len() as f32;
        for (_, count) in tf.iter_mut() {
            *count = 1.0 + (*count / doc_len).log2();
        }

        // Normalize to unit vector (simplified: no IDF without corpus)
        let norm_sq: f32 = tf.values().map(|v| v * v).sum();
        let norm = if norm_sq > 0.0 { norm_sq.sqrt() } else { 1.0 };

        // Return as sorted values (for consistent embedding length)
        let mut embedding: Vec<f32> = tf
            .values()
            .map(|v| v / norm)
            .collect();
        embedding.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

        // Pad/truncate to a fixed size for storage
        const FIXED_SIZE: usize = 512;
        if embedding.len() < FIXED_SIZE {
            embedding.resize(FIXED_SIZE, 0.0);
        } else {
            embedding.truncate(FIXED_SIZE);
        }

        embedding
    }

    /// Compute cosine similarity between two normalized vectors.
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let len = a.len().min(b.len());
        if len == 0 {
            return 0.0;
        }
        let dot: f32 = (0..len).map(|i| a[i] * b[i]).sum();
        let norm_a: f32 = (0..len).map(|i| a[i] * a[i]).sum::<f32>().sqrt();
        let norm_b: f32 = (0..len).map(|i| b[i] * b[i]).sum::<f32>().sqrt();
        if norm_a < 1e-8 || norm_b < 1e-8 {
            return 0.0;
        }
        (dot / (norm_a * norm_b)).max(0.0).min(1.0)
    }

    /// Search for similar documents using cosine similarity.
    pub fn search(
        &self,
        query_text: &str,
        db: &Database,
        limit: usize,
    ) -> Result<Vec<(String, f32)>, String> {
        let query_embedding = self.compute_embedding(query_text);

        let embeddings = self.doc_embeddings.read().map_err(|e| e.to_string())?;

        let mut scores: Vec<(String, f32)> = embeddings
            .iter()
            .map(|(id, emb)| {
                (id.clone(), Self::cosine_similarity(&query_embedding, emb))
            })
            .filter(|(_, score)| *score > 0.1) // threshold
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(limit);

        // Drop db reference before return
        let _ = db;
        Ok(scores)
    }

    /// Build or rebuild the vector index from database.
    pub fn build_index(&self, db: &Database) -> Result<usize, String> {
        let mut count = 0;

        // Get patents needing embedding
        let ids = db
            .list_patents_needing_embedding()
            .map_err(|e| e.to_string())?;

        // We need the full patent module - delegate to caller
        // This method is a skeleton; actual indexing done via pipeline

        let mut emb_map = self.doc_embeddings.write().map_err(|e| e.to_string())?;
        for id in &ids {
            emb_map.insert(id.clone(), vec![0.0f32]);
            count += 1;
        }

        Ok(count)
    }
}

/// Compute TF-IDF embedding for a single text and persist to DB.
pub fn compute_and_save_embedding(
    index: &VectorIndex,
    db: &Database,
    patent_id: &str,
    text: &str,
) -> Result<(), String> {
    let embedding = index.compute_embedding(text);
    db.save_patent_embedding(patent_id, &embedding, "char-tfidf-v1")
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// RRF (Reciprocal Rank Fusion) — fuse BM25 and vector results.
/// rank_fusion_score = sum(1 / (k + rank)) for each result set.
pub fn rrf_fuse(
    bm25_ranked: &[(String, f64)],
    vector_ranked: &[(String, f32)],
    k: usize,
) -> Vec<(String, f64)> {
    let mut scores: HashMap<String, f64> = HashMap::new();

    for (i, (id, _score)) in bm25_ranked.iter().enumerate() {
        let rank = i + 1;
        *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (k as f64 + rank as f64);
    }

    for (i, (id, _score)) in vector_ranked.iter().enumerate() {
        let rank = i + 1;
        *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (k as f64 + rank as f64);
    }

    let mut sorted: Vec<(String, f64)> = scores.into_iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    sorted
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    pub patent_id: String,
    pub patent_number: String,
    pub title: String,
    pub bm25_score: f64,
    pub vector_score: f32,
    pub fused_score: f64,
    pub bm25_rank: usize,
    pub vector_rank: usize,
}