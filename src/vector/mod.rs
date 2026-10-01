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
pub struct CharNGramTokenizer {
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
    ///
    /// Operates on `Vec<char>` to guarantee char-boundary safety regardless
    /// of the input's UTF-8 byte layout. The previous byte-slice approach
    /// (`cleaned[i..i+n]`) panicked on Chinese text because `i` iterated over
    /// byte indices but multi-byte chars make most offsets non-char-boundaries.
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        let cleaned: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
        let mut grams = Vec::new();
        for n in self.min_n..=self.max_n {
            if cleaned.len() >= n {
                for i in 0..=cleaned.len() - n {
                    let gram: String = cleaned[i..i + n].iter().collect();
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
        for count in tf.values_mut() {
            *count = 1.0 + (*count / doc_len).log2();
        }

        // Normalize to unit vector.
        //
        // KNOWN LIMITATION (M-B §0 decision, intentionally not fixed this round):
        //   - No IDF weighting (no corpus statistics); all terms treated equally.
        //   - The sorted-then-padded approach discards the term→dimension mapping,
        //     so query and document vectors are NOT in the same comparable space.
        //     Cosine similarity values do NOT constitute semantic relevance.
        //   - This is acceptable for M-B: we only promise "usable + no panic +
        //     single implementation", not retrieval quality.
        //   - Re-evaluate after MB3 when real chunk corpus is available.
        let norm_sq: f32 = tf.values().map(|v| v * v).sum();
        let norm = if norm_sq > 0.0 { norm_sq.sqrt() } else { 1.0 };

        // Return as sorted values (for consistent embedding length)
        let mut embedding: Vec<f32> = tf.values().map(|v| v / norm).collect();
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
        (dot / (norm_a * norm_b)).clamp(0.0, 1.0)
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
            .map(|(id, emb)| (id.clone(), Self::cosine_similarity(&query_embedding, emb)))
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

/// Convenience: compute a TF-IDF embedding without constructing a VectorIndex.
///
/// Delegates to [`crate::db::vector::compute_tfidf_embedding`] — the single
/// canonical implementation shared across both binary and library targets.
pub fn compute_tfidf_embedding(text: &str) -> Vec<f32> {
    crate::db::vector::compute_tfidf_embedding(text)
}

/// Try to compute and persist an embedding for a patent.
///
/// Delegates to [`crate::db::vector::try_compute_and_save_embedding`].
/// **Failure is non-fatal**: embedding is an optional enhancement, not a
/// prerequisite for patent insertion.
pub fn try_compute_and_save_embedding(db: &Database, patent_id: &str, text: &str) {
    crate::db::vector::try_compute_and_save_embedding(db, patent_id, text)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: pure Chinese text must not panic in tokenize.
    /// Previously, `cleaned[i..i+n]` byte-sliced at arbitrary offsets,
    /// panicking because Chinese chars are 3 bytes in UTF-8.
    #[test]
    fn tokenize_pure_chinese_no_panic() {
        let tok = CharNGramTokenizer::default();
        let grams = tok.tokenize("一种基于深度学习的专利分析方法");
        assert!(!grams.is_empty(), "Chinese text should produce n-grams");
        // 11 chars, n=2..4 → (10+9+8) = 27 grams
        assert_eq!(grams.len(), 39);
    }

    /// Regression: mixed Chinese + English must not panic.
    #[test]
    fn tokenize_mixed_zh_en_no_panic() {
        let tok = CharNGramTokenizer::default();
        let grams = tok.tokenize("专利分析patent analysis方法");
        assert!(!grams.is_empty(), "Mixed text should produce n-grams");
    }

    /// Regression: emoji (4-byte UTF-8) must not panic.
    #[test]
    fn tokenize_emoji_no_panic() {
        let tok = CharNGramTokenizer::default();
        let grams = tok.tokenize("专利🚀分析🚀方法");
        assert!(!grams.is_empty(), "Emoji text should produce n-grams");
    }

    /// Exhaustive: every possible substring start position must not panic.
    /// This catches off-by-one errors that a few hand-picked inputs might miss.
    #[test]
    fn tokenize_all_start_positions_no_panic() {
        let tok = CharNGramTokenizer::default();
        let text = "一种基于深度学习的专利分析方法";
        let chars: Vec<char> = text.chars().collect();
        for start in 0..chars.len() {
            for end in (start + 1)..=chars.len() {
                let substring: String = chars[start..end].iter().collect();
                let _ = tok.tokenize(&substring); // must not panic
            }
        }
    }

    /// The embedding output must always be exactly 512 dimensions.
    #[test]
    fn embedding_fixed_dimension_512() {
        let emb = compute_tfidf_embedding("一种基于深度学习的专利分析方法");
        assert_eq!(emb.len(), 512, "Embedding must be 512-dimensional");
    }

    /// Empty text should not panic and should produce a valid (zero) embedding.
    #[test]
    fn embedding_empty_text_no_panic() {
        let emb = compute_tfidf_embedding("");
        assert_eq!(emb.len(), 512);
        assert!(
            emb.iter().all(|&v| v == 0.0),
            "Empty text → all-zero embedding"
        );
    }

    /// Single character should not panic.
    #[test]
    fn tokenize_single_char_no_panic() {
        let tok = CharNGramTokenizer::default();
        let grams = tok.tokenize("专");
        assert!(grams.is_empty(), "Single char can't form n-grams with n>=2");
    }

    /// Exactly n characters: boundary case.
    #[test]
    fn tokenize_exact_n_chars() {
        let tok = CharNGramTokenizer::default();
        let grams = tok.tokenize("专利"); // 2 chars
        assert_eq!(grams.len(), 1, "2 chars with n=2 → 1 bigram");
        assert_eq!(grams[0], "专利");
    }

    /// Cosine similarity: identical vectors → 1.0, orthogonal → 0.0.
    #[test]
    fn cosine_similarity_basic() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((crate::db::vector::cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        let c = vec![0.0, 1.0, 0.0];
        assert!((crate::db::vector::cosine_similarity(&a, &c) - 0.0).abs() < 1e-6);
    }
}
