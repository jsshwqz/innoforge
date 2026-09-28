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
    ///
    /// MB0 修复：按**字符窗口**（`Vec<char>::windows`）切片。旧实现用 `cleaned[i..i+n]`
    /// 按**字节索引**切 String，而 `cleaned.len()` 是字节长度，中文（3 字节/字）必然
    /// 从字符中间切开并 panic（`byte index ... is not a char boundary`）。本实现的
    /// 滑窗语义与旧版 2..=4 gram 完全一致：ASCII（1 字节 == 1 字符）下新旧逐点同值。
    fn tokenize(&self, text: &str) -> Vec<String> {
        let cleaned: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
        let mut grams = Vec::new();
        for n in self.min_n..=self.max_n {
            if cleaned.len() >= n {
                for window in cleaned.windows(n) {
                    grams.push(window.iter().collect::<String>());
                }
            }
        }
        grams
    }
}

/// 全仓**唯一**的字符 n-gram TF 嵌入实现（MB0 归一后的单一出处）。
///
/// 写入侧（`Database::insert_patent` 新入库顺手算，经
/// [`compute_and_save_embedding`]）与查询侧（`/api/search/vector`）**必须调用同一个
/// 本函数**，禁止再出现第二套标准。
///
/// **已知设计局限（本轮有意不修，规格书 §0 决策）**：
/// - 这里产出的「向量」是把各 term 的 TF 分数**排序后**填进 512 维，**丢弃了
///   term→维度映射**——同一维度在不同文档里对应不同的 term，查询向量与文档向量
///   **不在同一可比空间**，相似度数值只是「排序后分数序列的接近程度」，
///   **不构成语义相关性**，不是可对外承诺的语义检索能力。
/// - 没有 IDF 权重（"no IDF without corpus"），且 `1 + log2(count/doc_len)` 公式对
///   低频项出**负值**——均为既有打分行为，MB0 只修切法、**不改打分公式**。
/// - 本轮禁止引入任何嵌入模型 / 新 crate 依赖；禁止用归一化、加权、伪造 IDF 等
///   手段让相似度「看起来更准」。
pub fn compute_char_tfidf_embedding(text: &str) -> Vec<f32> {
    let tokens = CharNGramTokenizer::default().tokenize(text);
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

    // Normalize to unit vector (simplified: no IDF without corpus — 局限见函数头注释)
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

/// Lazy-initialized TF-IDF vocabulary and document matrix.
/// For large corpora, only the vocabulary is cached; embeddings are computed on demand.
pub struct VectorIndex {
    doc_embeddings: RwLock<HashMap<String, Vec<f32>>>,
}

impl Default for VectorIndex {
    fn default() -> Self {
        Self {
            doc_embeddings: RwLock::new(HashMap::new()),
        }
    }
}

impl VectorIndex {
    /// Compute TF-IDF embedding for a single text document.
    /// Delegates to [`compute_char_tfidf_embedding`] — the single source of truth.
    pub fn compute_embedding(&self, text: &str) -> Vec<f32> {
        compute_char_tfidf_embedding(text)
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
///
/// MB0 写入链接通后有了真实调用点：[`crate::db::Database::insert_patent`]（全仓唯一挂点，
/// 新入库顺手算；此前长期零调用，`patents_embedding` 生产恒 0 行）。
/// 局限声明见 [`compute_char_tfidf_embedding`]——相似度不构成语义能力，本轮有意不修。
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 纯中文（UTF-8 每字 3 字节）长文本——旧实现按字节索引切片，必踩非字符边界。
    fn chinese_text() -> String {
        "本发明公开了一种固态电池及其制备方法，属于新能源技术领域。该电池采用硫化物电解质层，\
         通过界面修饰工艺显著降低了界面阻抗，提升了循环寿命与倍率性能。实施方式中，正极材料\
         选自磷酸铁锂、三元材料或富锂锰基化合物，负极材料为硅碳复合负极。"
            .to_string()
    }

    #[test]
    fn tokenize_pure_chinese_hits_no_char_boundary_panic() {
        // 红→绿锚：旧实现（cleaned[i..i+n] 按字节切片）在此 panic：
        // "byte index ... is not a char boundary"。新实现必须零 panic 且产出 n-gram。
        let tok = CharNGramTokenizer::default();
        let grams = tok.tokenize(&chinese_text());
        assert!(!grams.is_empty(), "中文文本必须产出 n-gram");
        // 每个 gram 都必须是合法字符窗口（长度按字符计，不是字节）。
        for g in &grams {
            assert!(!g.is_empty());
        }
        assert!(
            grams.contains(&"固态电池".to_string()),
            "字符级 4-gram 窗口必须存在"
        );
        assert!(
            grams.contains(&"电池".to_string()),
            "字符级 2-gram 窗口必须存在"
        );
    }

    #[test]
    fn tokenize_mixed_cjk_ascii_emoji_hits_no_panic() {
        let tok = CharNGramTokenizer::default();
        let mixed = "一种 device 装置🔋电池🔋测试";
        let grams = tok.tokenize(mixed);
        let cleaned: Vec<char> = mixed.chars().filter(|c| !c.is_whitespace()).collect();
        let expected: usize = (2..=4)
            .map(|n| cleaned.len().saturating_sub(n) + 1)
            .sum::<usize>()
            - (2..=4).filter(|n| cleaned.len() < *n).count();
        assert_eq!(
            grams.len(),
            expected,
            "窗口数以字符计：len(char)={}",
            cleaned.len()
        );
        // emoji（4 字节）不得被从中间切开——每个 gram 重新拼回后必须仍是原串的字符子串。
        let joined: String = cleaned.iter().collect::<String>();
        for g in &grams {
            assert!(
                joined.contains(g.as_str()),
                "gram {g:?} 必须是清理后文本的连续字符子串"
            );
        }
    }

    #[test]
    fn tokenize_windows_cover_every_char_start() {
        // 对 0..字符数 的每个起点都切片：旧实现从字节起点 i 切 i..i+n，
        // 中文 3 字节 ⇒ 起点/终点大量落在字符中间，必红。
        let tok = CharNGramTokenizer::default();
        let text = "测试文本包含中文与english混排以及🔋emoji符号用于边界覆盖验证";
        let cleaned: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
        let grams = tok.tokenize(text);
        for n in 2..=4usize {
            if cleaned.len() >= n {
                let expect_n = cleaned.len() - n + 1;
                let got_n = grams.iter().filter(|g| g.chars().count() == n).count();
                assert_eq!(
                    got_n, expect_n,
                    "n={n} 的窗口必须覆盖每个字符起点（滑窗语义不变）"
                );
            }
        }
    }

    #[test]
    fn ascii_semantics_of_scoring_formula_unchanged() {
        // 只修切法、不改打分公式：纯 ASCII（1 字节 == 1 字符）下新旧行为必须逐点一致。
        // 手工推导 "aabb"：2-gram {aa,ab,bb} + 3-gram {aab,abb} + 4-gram {aabb}
        // = 6 个 term 各计数 1，doc_len=6 ⇒ tf = 1 + log2(1/6) = -0.585…（公式对低频项
        // 出负值是既有行为，本包不改公式，故一并锁死），L2 归一后每项 -1/sqrt(6)。
        let emb = compute_char_tfidf_embedding("aabb");
        assert_eq!(emb.len(), 512);
        let expect = -(1.0f32 / 6.0f32.sqrt());
        for v in &emb[..6] {
            assert!(
                (v - expect).abs() < 1e-6,
                "归一化 tf 值应逐字保持旧公式: got {v}"
            );
        }
        assert!(emb[6..].iter().all(|v| *v == 0.0), "512 维补零不变");
    }

    #[test]
    fn embedding_is_deterministic_and_fixed_size() {
        let a = compute_char_tfidf_embedding(&chinese_text());
        let b = compute_char_tfidf_embedding(&chinese_text());
        // 既有事实（非本包引入、打分公式不许改动故不修）：归一项 `norm_sq` 是对 HashMap
        // 迭代序求和，浮点加法顺序随 RandomState 抖动，两次计算允许 **1 ulp 级**逐位差；
        // 这里锁「反复计算零 panic + 数值等价（1e-6 容差）+ 形状恒定」，不虚报位级确定性。
        assert_eq!(a.len(), 512);
        assert_eq!(b.len(), 512);
        for (x, y) in a.iter().zip(b.iter()) {
            assert!(
                (x - y).abs() < 1e-6,
                "同一文本重复计算必须数值等价，got {x} vs {y}"
            );
        }
        // 空文本降级为单元素零向量（与三份旧拷贝的既有形状一致）。
        assert_eq!(compute_char_tfidf_embedding("   "), vec![0.0f32]);
        // VectorIndex 门面与自由函数必须同实现（单一出处，不留第二套标准）。
        let c = VectorIndex::default().compute_embedding(&chinese_text());
        assert_eq!(c.len(), a.len());
        for (x, y) in a.iter().zip(c.iter()) {
            assert!((x - y).abs() < 1e-6);
        }
    }
}
