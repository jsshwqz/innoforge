//! Semantic Chunker

/// Split text into semantic chunks.
pub fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }
    let window = chunk_size.saturating_sub(overlap);
    if window == 0 {
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
        let search_start = end - overlap;
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

/// 切片嵌入——MB0 归一后**委托** [`crate::vector::compute_char_tfidf_embedding`]（全仓单一出处）。
///
/// 此处原有第三份 n-gram/TF-IDF 拷贝（按字节索引切 UTF-8，中文必 panic），已删除：
/// 写入侧、查询侧、切片侧共用同一个函数，不留第二套标准。
pub fn compute_chunk_embedding(text: &str) -> Vec<f32> {
    crate::vector::compute_char_tfidf_embedding(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MB0 归一验证：切片侧拷贝已删除，委托后与全仓单一实现共用**同一个函数**，
    /// 且中文（3 字节/字）零 panic（旧拷贝在此必 panic）。
    #[test]
    fn compute_chunk_embedding_delegates_to_single_vector_impl() {
        let text = "本发明公开了一种固态电池及其制备方法，属于新能源技术领域。";
        let via_chunker = compute_chunk_embedding(text);
        let via_vector = crate::vector::compute_char_tfidf_embedding(text);
        assert_eq!(via_chunker.len(), via_vector.len());
        // 浮点求和顺序随 HashMap 迭代抖动（既有行为，见 vector 侧同名注释），锁 1e-6 容差。
        for (x, y) in via_chunker.iter().zip(via_vector.iter()) {
            assert!(
                (x - y).abs() < 1e-6,
                "两处必须共用同一个函数，不允许第二套标准"
            );
        }
        assert_eq!(via_chunker.len(), 512);
    }
}
