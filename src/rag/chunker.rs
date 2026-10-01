//! Semantic Chunker

/// Split text into semantic chunks.
///
/// Operates on `Vec<char>` to guarantee char-boundary safety. The previous
/// byte-index approach (`text[start..end]`) could split multi-byte characters
/// and produce invalid UTF-8 or panic on Chinese text.
pub fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }
    let chars: Vec<char> = text.chars().collect();
    let total = chars.len();
    let window = chunk_size.saturating_sub(overlap);
    if window == 0 {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < total {
        let end = (start + chunk_size).min(total);
        let chunk: String = chars[start..end].iter().collect();
        chunks.push(chunk);
        if end >= total {
            break;
        }
        let search_start = end.saturating_sub(overlap);
        let mut boundary = end;
        for i in (search_start..end).rev() {
            let c = chars[i];
            let is_boundary = c == '。' || c == ';' || c == '.';
            let is_newline = c == '\n';
            if (is_boundary || is_newline) && i > start + window / 2 {
                boundary = i;
                break;
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

/// Compute TF-IDF embedding for a chunk of text.
///
/// Delegates to [`crate::db::vector::compute_tfidf_embedding`] — the single
/// canonical implementation. The previous inline copy had the same
/// byte-slicing bug as the other two copies (panicked on Chinese text).
pub fn compute_chunk_embedding(text: &str) -> Vec<f32> {
    crate::db::vector::compute_tfidf_embedding(text)
}
