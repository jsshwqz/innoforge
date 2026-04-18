//! Step 6: ComputeSimilarity — TF-IDF + Jaccard 混合相似度计算
//!
//! 类型：CODE（纯算法，不调用 LLM）

use crate::pipeline::context::{PipelineContext, SimilarityEntry};
use crate::pipeline::steps::parse::tokenize;
use anyhow::Result;
use std::collections::{HashMap, HashSet};

/// 计算 Jaccard 相似系数
fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

/// 计算 IDF（逆文档频率）
fn compute_idf(documents: &[Vec<String>]) -> HashMap<String, f64> {
    let n = documents.len() as f64;
    let mut doc_freq: HashMap<String, usize> = HashMap::new();

    for doc_tokens in documents {
        let unique: HashSet<_> = doc_tokens.iter().cloned().collect();
        for token in unique {
            *doc_freq.entry(token).or_default() += 1;
        }
    }

    doc_freq
        .into_iter()
        .map(|(token, df)| {
            let idf = (n / (df as f64 + 1.0)).ln() + 1.0;
            (token, idf)
        })
        .collect()
}

/// 计算 TF-IDF 余弦相似度
fn tfidf_cosine(query_tokens: &[String], doc_tokens: &[String], idf: &HashMap<String, f64>) -> f64 {
    // 计算 TF
    let query_tf = term_frequency(query_tokens);
    let doc_tf = term_frequency(doc_tokens);

    // TF-IDF 向量
    let all_terms: HashSet<_> = query_tf.keys().chain(doc_tf.keys()).cloned().collect();

    let mut dot_product = 0.0;
    let mut query_norm = 0.0;
    let mut doc_norm = 0.0;

    for term in &all_terms {
        let q_tfidf = query_tf.get(term).unwrap_or(&0.0) * idf.get(term).unwrap_or(&1.0);
        let d_tfidf = doc_tf.get(term).unwrap_or(&0.0) * idf.get(term).unwrap_or(&1.0);
        dot_product += q_tfidf * d_tfidf;
        query_norm += q_tfidf * q_tfidf;
        doc_norm += d_tfidf * d_tfidf;
    }

    let denom = query_norm.sqrt() * doc_norm.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot_product / denom
    }
}

/// 计算词频（归一化）
fn term_frequency(tokens: &[String]) -> HashMap<String, f64> {
    let mut freq: HashMap<String, f64> = HashMap::new();
    let total = tokens.len() as f64;
    if total == 0.0 {
        return freq;
    }
    for token in tokens {
        *freq.entry(token.clone()).or_default() += 1.0;
    }
    for v in freq.values_mut() {
        *v /= total;
    }
    freq
}

/// 执行 Step 6：计算用户创意与所有搜索结果的相似度
pub async fn execute(ctx: &mut PipelineContext) -> Result<()> {
    let query_text = format!("{} {}", ctx.title, ctx.description);
    let query_tokens = tokenize(&query_text);
    let query_token_set: HashSet<String> = query_tokens.iter().cloned().collect();

    // 收集所有文档的 tokens
    let mut all_doc_tokens: Vec<Vec<String>> = Vec::new();
    let mut doc_metadata: Vec<(String, String, String)> = Vec::new(); // (id, title, type)

    for result in &ctx.web_results {
        let tokens = tokenize(&format!("{} {}", result.title, result.snippet));
        all_doc_tokens.push(tokens);
        doc_metadata.push((result.id.clone(), result.title.clone(), "web".into()));
    }

    for result in &ctx.patent_results {
        let tokens = tokenize(&format!("{} {}", result.title, result.snippet));
        all_doc_tokens.push(tokens);
        doc_metadata.push((result.id.clone(), result.title.clone(), "patent".into()));
    }

    if all_doc_tokens.is_empty() {
        return Ok(());
    }

    // 计算全局 IDF（包含 query）
    let mut all_docs_for_idf = all_doc_tokens.clone();
    all_docs_for_idf.push(query_tokens.clone());
    let idf = compute_idf(&all_docs_for_idf);

    // 计算每个文档与 query 的相似度
    let mut scores = Vec::new();
    for (i, doc_tokens) in all_doc_tokens.iter().enumerate() {
        let doc_token_set: HashSet<String> = doc_tokens.iter().cloned().collect();
        let j = jaccard(&query_token_set, &doc_token_set);
        let t = tfidf_cosine(&query_tokens, doc_tokens, &idf);
        let combined = 0.6 * t + 0.4 * j;

        let (ref id, ref title, ref source_type) = doc_metadata[i];
        scores.push(SimilarityEntry {
            source_id: id.clone(),
            source_title: title.clone(),
            source_type: source_type.clone(),
            tfidf_score: t,
            jaccard_score: j,
            combined_score: combined,
        });
    }

    // 按 combined_score 降序排序
    scores.sort_by(|a, b| {
        b.combined_score
            .partial_cmp(&a.combined_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    ctx.similarity_scores = scores;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jaccard_identical() {
        let a: HashSet<String> = ["foo", "bar"].iter().map(|s| s.to_string()).collect();
        assert!((jaccard(&a, &a) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_jaccard_disjoint() {
        let a: HashSet<String> = ["foo"].iter().map(|s| s.to_string()).collect();
        let b: HashSet<String> = ["bar"].iter().map(|s| s.to_string()).collect();
        assert!((jaccard(&a, &b)).abs() < 0.001);
    }

    #[test]
    fn test_tfidf_same_doc() {
        let tokens = vec!["hello".into(), "world".into()];
        let idf = compute_idf(std::slice::from_ref(&tokens));
        let score = tfidf_cosine(&tokens, &tokens, &idf);
        assert!(score > 0.99);
    }
}
