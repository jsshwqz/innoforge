//! RAG Prompt 组装器 / RAG Prompt Assembler

use crate::pipeline::context::ReferenceChunk;

/// Assemble RAG context into a formatted prompt string.
pub fn assemble_rag_prompt(
    query: &str,
    chunks: &[ReferenceChunk],
    system_prompt: &str,
    max_tokens: usize,
) -> String {
    if chunks.is_empty() {
        return format!("{}

<user_query>
{}
</user_query>", system_prompt, query);
    }

    let mut context = String::new();
    context.push_str(&format!("以下是检索到的相关文档片段（共 {} 段），请按顺序引用：

", chunks.len()));

    let mut remaining = max_tokens;
    for (i, chunk) in chunks.iter().enumerate() {
        if remaining < 100 {
            break;
        }
        let chunk_len = chunk.content.len();
        if chunk_len > remaining {
            let truncated: String = chunk.content.chars().take(remaining).collect();
            context.push_str(&format!(
                "### [引用 {}]（来源: {}, 相似度: {:.2}）
{}

",
                i + 1, chunk.source_type, chunk.relevance_score, truncated
            ));
            break;
        }
        context.push_str(&format!(
            "### [引用 {}]（来源: {}, 相似度: {:.2}）
{}

",
            i + 1, chunk.source_type, chunk.relevance_score, chunk.content
        ));
        remaining -= chunk_len;
    }

    let query_section = format!("<user_query>
{}
</user_query>", query);
    let ref_instruction = format!("

请基于以上文档片段回答问题。引用格式：[引用N] 表示第N个片段。
{}", query_section);

    format!("{}
{}{}", system_prompt, context, ref_instruction)
}

/// Build citation list from referenced chunks.
pub fn build_citations(chunks: &[ReferenceChunk]) -> String {
    if chunks.is_empty() {
        return "无引用".to_string();
    }
    let refs: Vec<String> = chunks.iter().take(5).enumerate().map(|(i, c)| {
        let preview: String = c.content.chars().take(50).collect();
        format!("[{}] {} ({}): {}", i + 1, c.id, c.source_type, preview)
    }).collect();
    refs.join("
")
}
