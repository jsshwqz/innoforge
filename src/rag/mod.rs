//! RAG 管道 / RAG Pipeline
//!
//! 检索增强生成（Retrieval-Augmented Generation）管道：
//! 切片 → 检索 → 组装 → AI 回答（带引用）
//! Chunk → Retrieve → Assemble → Answer with citations

pub mod assembler;
pub mod chunker;
pub mod retriever;

use self::chunker::{chunk_structured_patent, chunk_summary, compute_chunk_embedding};
use crate::ai::AiClient;
use crate::db::Database;
use crate::pipeline::context::ReferenceChunk;

const CHUNK_SIZE: usize = 800;
const CHUNK_OVERLAP: usize = 100;
const TOP_K: usize = 3;

/// Build chunks from patent text fields.
///
/// Batch 1 任务 1.2：切片改走 [`chunker::chunk_structured_patent`] 结构化分块——
/// 权利要求按序号、说明书按段落号/章节标题打 `section_tag`（检索结果可透传展示
/// 「命中在哪一条权利要求/哪个段落」），无结构标记时回退窗口切分（`Desc:Window` /
/// `Claim:Window`），语义与旧实现一致。
pub fn build_chunks_from_patent(
    patent_id: &str,
    _title: &str,
    abstract_text: &str,
    claims: &str,
    description: &str,
) -> Vec<crate::db::rag::PatentChunk> {
    let mut chunks = Vec::new();
    let mut index = 0i32;

    // Abstract chunk（单段常规形态，统一标 Abstract）
    if !abstract_text.is_empty() {
        for (tag, text) in
            chunk_structured_patent("abstract", abstract_text, CHUNK_SIZE, CHUNK_OVERLAP)
        {
            let embedding = compute_chunk_embedding(&text);
            chunks.push(crate::db::rag::PatentChunk {
                id: format!("{}-{}", patent_id, index),
                patent_id: patent_id.to_string(),
                chunk_index: index,
                source_type: "abstract".to_string(),
                section_tag: tag,
                content: text,
                embedding,
            });
            index += 1;
        }
    }

    // Claims chunks（按权利要求序号标 Claim:N）
    if !claims.is_empty() {
        for (tag, text) in chunk_structured_patent("claim", claims, CHUNK_SIZE, CHUNK_OVERLAP) {
            let embedding = compute_chunk_embedding(&text);
            chunks.push(crate::db::rag::PatentChunk {
                id: format!("{}-{}", patent_id, index),
                patent_id: patent_id.to_string(),
                chunk_index: index,
                source_type: "claim".to_string(),
                section_tag: tag,
                content: text,
                embedding,
            });
            index += 1;
        }
    }

    // Description chunks（按 [NNNN] 段落号 / 章节标题打标签，无标记回退窗口）
    if !description.is_empty() {
        for (tag, text) in
            chunk_structured_patent("description", description, CHUNK_SIZE, CHUNK_OVERLAP)
        {
            let embedding = compute_chunk_embedding(&text);
            chunks.push(crate::db::rag::PatentChunk {
                id: format!("{}-{}", patent_id, index),
                patent_id: patent_id.to_string(),
                chunk_index: index,
                source_type: "description".to_string(),
                section_tag: tag,
                content: text,
                embedding,
            });
            index += 1;
        }
    }

    chunks
}

/// RAG search: retrieve chunks, assemble prompt, call AI, return with citations.
pub async fn rag_search(
    ai_client: &AiClient,
    db: &Database,
    patent_id: &str,
    query: &str,
    system_prompt: &str,
    max_tokens: usize,
) -> Result<RagResult, String> {
    // Step 1: Retrieve relevant chunks
    let chunks = match retriever::retrieve_chunks(db, patent_id, query, TOP_K) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("RAG retrieval failed: {}", e);
            Vec::new()
        }
    };

    let chunk_count = chunks.len();

    // Step 2: Assemble prompt with retrieved context
    let prompt = assembler::assemble_rag_prompt(query, &chunks, system_prompt, max_tokens);

    // Step 3: Call AI
    let ai_result = match ai_client.send_rag_chat(&prompt, 0.5).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("RAG AI call failed: {}", e);
            // Graceful degradation: return chunks as text
            return Ok(RagResult {
                answer: format!("检索到 {} 个相关片段，但 AI 调用失败。", chunk_count),
                citations: build_fallback_citations(&chunks),
                chunk_count,
                rag_enabled: true,
                rag_failed: true,
            });
        }
    };

    // Step 4: Build citations
    let citations = build_fallback_citations(&chunks);

    Ok(RagResult {
        answer: ai_result,
        citations,
        chunk_count,
        rag_enabled: true,
        rag_failed: false,
    })
}

fn build_fallback_citations(chunks: &[ReferenceChunk]) -> Vec<String> {
    chunks
        .iter()
        .take(TOP_K)
        .map(|c| {
            format!(
                "[引用{}] ({}): {}",
                c.chunk_index + 1,
                c.source_type,
                chunk_summary(&c.content)
            )
        })
        .collect()
}

/// Result of a RAG search.
#[derive(Debug, Clone)]
pub struct RagResult {
    pub answer: String,
    pub citations: Vec<String>,
    pub chunk_count: usize,
    pub rag_enabled: bool,
    pub rag_failed: bool,
}
