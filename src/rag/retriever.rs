//! RAG 检索器 / RAG Retriever

use super::chunker;
use crate::db::Database;
use crate::pipeline::context::ReferenceChunk;

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

/// Keyword-based fallback retrieval using SQL LIKE matching.
///
/// 当向量检索不可用（embedding 全零或表空）时，用关键词 LIKE 检索切片。
/// 返回按 `chunk_index` 排序的前 `top_k` 条匹配。
pub fn retrieve_chunks_by_keyword(
    db: &Database,
    patent_id: &str,
    query: &str,
    top_k: usize,
) -> Result<Vec<ReferenceChunk>, String> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }

    let conn = db.conn();
    let pattern = format!("%{}%", query.trim());
    let mut stmt = match conn.prepare(
        "SELECT id, patent_id, chunk_index, source_type, content
         FROM patent_chunks
         WHERE patent_id = ?1 AND content LIKE ?2
         ORDER BY chunk_index
         LIMIT ?3",
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Keyword chunk search failed: {}", e);
            return Ok(vec![]);
        }
    };

    let rows = stmt.query_map(
        rusqlite::params![patent_id, pattern, top_k as i64],
        |row: &rusqlite::Row| {
            Ok(ReferenceChunk {
                id: row.get(0)?,
                patent_id: row.get(1)?,
                chunk_index: row.get(2)?,
                source_type: row.get(3)?,
                content: row.get(4)?,
                relevance_score: 0.0, // keyword match, no vector score
            })
        },
    );

    match rows {
        Ok(r) => Ok(r.flatten().collect()),
        Err(e) => {
            tracing::warn!("Keyword chunk query execution failed: {}", e);
            Ok(vec![])
        }
    }
}
