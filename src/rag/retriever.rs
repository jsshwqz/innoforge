//! RAG 检索器 / RAG Retriever

use crate::db::Database;
use crate::pipeline::context::ReferenceChunk;
use super::chunker;

/// Retrieve relevant chunks from a patent for a given query.
pub fn retrieve_chunks(
    db: &Database,
    patent_id: &str,
    query: &str,
    top_k: usize,
) -> Result<Vec<ReferenceChunk>, String> {
    let query_embedding = chunker::compute_chunk_embedding(query);

    let scores = match db.search_chunks(patent_id, &query_embedding, top_k * 2) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Failed to search chunks: {}", e);
            return Ok(vec![]);
        }
    };

    let mut chunks = Vec::new();
    for (chunk_id, score) in scores.into_iter().take(top_k) {
        let chunk = match db.get_chunk(&chunk_id) {
            Ok(Some(c)) => c,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!("Failed to get chunk {}: {}", chunk_id, e);
                continue;
            }
        };

        chunks.push(ReferenceChunk {
            id: chunk.id,
            patent_id: chunk.patent_id,
            chunk_index: chunk.chunk_index,
            source_type: chunk.source_type,
            content: chunk.content,
            relevance_score: score,
        });
    }

    Ok(chunks)
}

/// Keyword-based fallback (placeholder for future use).
pub fn retrieve_chunks_by_keyword(
    _db: &Database,
    _patent_id: &str,
    _query: &str,
    _top_k: usize,
) -> Result<Vec<ReferenceChunk>, String> {
    Ok(vec![])
}
