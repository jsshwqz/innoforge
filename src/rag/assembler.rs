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
        return format!(
            "{}

<user_query>
{}
</user_query>",
            system_prompt, query
        );
    }

    let mut context = String::new();
    context.push_str(&format!(
        "以下是检索到的相关文档片段（共 {} 段），请按顺序引用：

",
        chunks.len()
    ));

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
                i + 1,
                chunk.source_type,
                chunk.relevance_score,
                truncated
            ));
            break;
        }
        context.push_str(&format!(
            "### [引用 {}]（来源: {}, 相似度: {:.2}）
{}

",
            i + 1,
            chunk.source_type,
            chunk.relevance_score,
            chunk.content
        ));
        remaining -= chunk_len;
    }

    let query_section = format!(
        "<user_query>
{}
</user_query>",
        query
    );
    let ref_instruction = format!(
        "

请基于以上文档片段回答问题。引用格式：[引用N] 表示第N个片段。
{}",
        query_section
    );

    format!(
        "{}
{}{}",
        system_prompt, context, ref_instruction
    )
}

/// Build citation list from referenced chunks.
pub fn build_citations(chunks: &[ReferenceChunk]) -> String {
    if chunks.is_empty() {
        return "无引用".to_string();
    }
    let refs: Vec<String> = chunks
        .iter()
        .take(5)
        .enumerate()
        .map(|(i, c)| {
            let preview: String = c.content.chars().take(50).collect();
            format!("[{}] {} ({}): {}", i + 1, c.id, c.source_type, preview)
        })
        .collect();
    refs.join(
        "
",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::context::ReferenceChunk;

    fn make_chunk(id: &str, content: &str, score: f32) -> ReferenceChunk {
        ReferenceChunk {
            id: id.to_string(),
            patent_id: "test-patent".to_string(),
            chunk_index: 0,
            source_type: "abstract".to_string(),
            content: content.to_string(),
            relevance_score: score,
        }
    }

    #[test]
    fn test_assemble_no_chunks_returns_plain_prompt() {
        let result = assemble_rag_prompt("query", &[], "system", 1000);
        assert!(result.contains("system"));
        assert!(result.contains("query"));
        assert!(!result.contains("引用"));
    }

    #[test]
    fn test_assemble_with_chunks_includes_citations() {
        let chunks = vec![
            make_chunk("c1", "Content one", 0.9),
            make_chunk("c2", "Content two", 0.8),
        ];
        let result = assemble_rag_prompt("query", &chunks, "system", 1000);
        assert!(result.contains("[引用 1]"));
        assert!(result.contains("[引用 2]"));
        assert!(result.contains("Content one"));
        assert!(result.contains("Content two"));
    }

    #[test]
    fn test_assemble_respects_max_tokens() {
        let long_content = "x".repeat(500);
        let chunks = vec![make_chunk("c1", &long_content, 0.9)];
        let result = assemble_rag_prompt("query", &chunks, "system", 200);
        // The content should be truncated to fit max_tokens
        assert!(result.contains("[引用 1]"));
        // Should not contain the full 500-char content
        assert!(!result.contains(&long_content));
    }

    #[test]
    fn test_build_citations_empty() {
        assert_eq!(build_citations(&[]), "无引用");
    }

    #[test]
    fn test_build_citations_with_chunks() {
        let chunks = vec![
            make_chunk("c1", "First chunk content", 0.9),
            make_chunk("c2", "Second chunk content", 0.8),
        ];
        let result = build_citations(&chunks);
        assert!(result.contains("[1]"));
        assert!(result.contains("[2]"));
        assert!(result.contains("c1"));
        assert!(result.contains("c2"));
    }
}
