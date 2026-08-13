//! Semantic Chunker

use std::collections::HashMap;

/// Split text into semantic chunks.
pub fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }
    let window = chunk_size - overlap;
    if window <= 0 || chunk_size <= 0 {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let end = (start + chunk_size).min(text.len());
        chunks.push(text[start..end].to_string());
        if end >= text.len() {
            break;
        }
        let search_start = (end - overlap).max(0);
        let mut boundary = end;
        for i in (search_start..end).rev() {
            if i < text.len() {
                if let Some(c) = text.chars().nth(i) {
                    let is_boundary = c == '。' || c == ';' || c == '.';
                    let byte_val = text.as_bytes().get(i).copied();
                    let is_newline = byte_val == Some(10);
                    if (is_boundary || is_newline) && i > start + window / 2 {
                        boundary = i;
                        break;
                    }
                }
            }
        }
        start = boundary + 1;
    }
    let mut deduped = Vec::new();
    for chunk in chunks {
        if deduped.is_empty() || *deduped.last().unwrap_or(&"".to_string()) != chunk {
            deduped.push(chunk);
        }
    }
    deduped
}

pub fn chunk_summary(chunk: &str) -> String {
    let preview: String = chunk.chars().take(60).collect();
    if preview.len() < chunk.len() {
        format!("{}...", preview)
    } else {
        preview
    }
}

pub fn compute_chunk_embedding(text: &str) -> Vec<f32> {
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let mut tf: HashMap<String, f32> = HashMap::new();
    for n in 2..=4 {
        if cleaned.len() >= n {
            for i in 0..=cleaned.len() - n {
                let gram: String = cleaned[i..i + n].chars().collect();
                *tf.entry(gram).or_insert(0.0) += 1.0;
            }
        }
    }
    if tf.is_empty() {
        return vec![0.0f32];
    }
    let doc_len = (tf.values().sum::<f32>()) as f32;
    for (_, count) in tf.iter_mut() {
        *count = 1.0 + (*count / doc_len).log2();
    }
    let norm_sq: f32 = tf.values().map(|v| v * v).sum();
    let norm = if norm_sq > 0.0 { norm_sq.sqrt() } else { 1.0 };
    let mut emb: Vec<f32> = tf.values().map(|v| v / norm).collect();
    emb.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    const FIXED: usize = 512;
    if emb.len() < FIXED {
        emb.resize(FIXED, 0.0);
    } else {
        emb.truncate(FIXED);
    }
    emb
}
