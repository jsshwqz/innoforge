//! 双栖混合检索内核（`src/core/vault/local_rag.rs`）
//!
//! 结合 FTS5 (`chunks_fts` BM25) 与本地向量余弦相似度，执行章节语义加权 RRF 融合（`k = 60.0`），
//! 并实现从属权利要求父链自动前置、说明书邻接段上下文拼装与 `char_budget` 严格控长（根治 `ERR-03`）。
//! 全程零 `unwrap()` / `expect()`，零 `[..` 裸字节切片。

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use rusqlite::{params_from_iter, types::Value, Connection};

use super::chunker::build_fts_tokens;
use super::models::{
    AssetRole, DocChunk, FigureNumeral, HybridChunkHit, HybridEvidenceBundle, SearchSceneMode,
    SectionKind,
};
use super::repo::{
    get_adjacent_para_chunks, get_chunk_by_anchor, get_chunk_by_claim_no, get_figure_numerals,
    row_to_chunk,
};

const RRF_K: f64 = 60.0;
const CANDIDATE_LIMIT: usize = 40;

fn build_fts_or_query(query: &str) -> String {
    let tokens_str = build_fts_tokens(query);
    let mut seen = HashSet::new();
    let mut parts = Vec::new();
    for tok in tokens_str.split_whitespace() {
        if tok.is_empty() || !seen.insert(tok.to_string()) {
            continue;
        }
        let escaped = tok.replace('"', "\"\"");
        parts.push(format!("\"{escaped}\""));
        if parts.len() >= 24 {
            break;
        }
    }
    parts.join(" OR ")
}

fn decode_f32_le(blob: &[u8]) -> Vec<f32> {
    blob.as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect()
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for (&x, &y) in a.iter().zip(b.iter()) {
        let xf = f64::from(x);
        let yf = f64::from(y);
        dot += xf * yf;
        norm_a += xf * xf;
        norm_b += yf * yf;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom <= 1e-12 {
        0.0
    } else {
        dot / denom
    }
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut out = String::new();
    for (idx, ch) in s.chars().enumerate() {
        if idx >= max_chars {
            break;
        }
        out.push(ch);
    }
    out
}

fn append_doc_filter_sql(
    sql: &mut String,
    params: &mut Vec<Value>,
    doc_ids: Option<&[String]>,
    role_filter: Option<AssetRole>,
) {
    if let Some(role) = role_filter {
        let idx = params.len() + 1;
        sql.push_str(&format!(" AND d.asset_role = ?{idx}"));
        params.push(Value::Text(role.as_str().to_string()));
    }
    if let Some(ids) = doc_ids {
        if !ids.is_empty() {
            let mut placeholders = Vec::with_capacity(ids.len());
            for id in ids {
                let idx = params.len() + 1;
                placeholders.push(format!("?{idx}"));
                params.push(Value::Text(id.clone()));
            }
            sql.push_str(&format!(" AND c.doc_id IN ({})", placeholders.join(", ")));
        }
    }
}

fn search_exact_anchors(
    conn: &Connection,
    anchor_tag: &str,
    doc_ids: Option<&[String]>,
    role_filter: Option<AssetRole>,
) -> Result<Vec<DocChunk>> {
    let tag = anchor_tag.trim();
    if tag.is_empty() {
        return Ok(Vec::new());
    }
    if let (Some(ids), None) = (doc_ids, role_filter) {
        if !ids.is_empty() {
            let mut out = Vec::new();
            for id in ids {
                if let Some(c) = get_chunk_by_anchor(conn, id, tag)? {
                    out.push(c);
                }
            }
            return Ok(out);
        }
    }

    let mut sql = String::from(
        "SELECT c.chunk_id, c.doc_id, c.section_kind, c.section_tag, c.claim_no,
                c.parent_claim_no, c.para_start, c.para_end, c.embodiment_group,
                c.raw_content, c.fts_tokens, c.embedding_blob, c.char_count
         FROM doc_chunks c
         JOIN knowledge_docs d ON d.doc_id = c.doc_id
         WHERE c.section_tag = ?1",
    );
    let mut params = vec![Value::Text(tag.to_string())];
    append_doc_filter_sql(&mut sql, &mut params, doc_ids, role_filter);
    sql.push_str(" ORDER BY c.chunk_id ASC LIMIT 40");

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(params.iter()), row_to_chunk)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

fn search_fts_candidates(
    conn: &Connection,
    fts_query: &str,
    doc_ids: Option<&[String]>,
    role_filter: Option<AssetRole>,
) -> Result<Vec<DocChunk>> {
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }
    let mut sql = String::from(
        "SELECT c.chunk_id, c.doc_id, c.section_kind, c.section_tag, c.claim_no,
                c.parent_claim_no, c.para_start, c.para_end, c.embodiment_group,
                c.raw_content, c.fts_tokens, c.embedding_blob, c.char_count
         FROM chunks_fts
         JOIN doc_chunks c ON c.chunk_id = chunks_fts.rowid
         JOIN knowledge_docs d ON d.doc_id = c.doc_id
         WHERE chunks_fts MATCH ?1",
    );
    let mut params = vec![Value::Text(fts_query.to_string())];
    append_doc_filter_sql(&mut sql, &mut params, doc_ids, role_filter);
    sql.push_str(&format!(
        " ORDER BY bm25(chunks_fts) ASC LIMIT {CANDIDATE_LIMIT}"
    ));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(params.iter()), row_to_chunk)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

fn search_vector_candidates(
    conn: &Connection,
    query_embedding: &[f32],
    doc_ids: Option<&[String]>,
    role_filter: Option<AssetRole>,
) -> Result<Vec<DocChunk>> {
    if query_embedding.is_empty() {
        return Ok(Vec::new());
    }
    let mut sql = String::from(
        "SELECT c.chunk_id, c.doc_id, c.section_kind, c.section_tag, c.claim_no,
                c.parent_claim_no, c.para_start, c.para_end, c.embodiment_group,
                c.raw_content, c.fts_tokens, c.embedding_blob, c.char_count
         FROM doc_chunks c
         JOIN knowledge_docs d ON d.doc_id = c.doc_id
         WHERE c.embedding_blob IS NOT NULL",
    );
    let mut params: Vec<Value> = Vec::new();
    append_doc_filter_sql(&mut sql, &mut params, doc_ids, role_filter);

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(params.iter()), row_to_chunk)?;
    let mut scored: Vec<(f64, DocChunk)> = Vec::new();
    for r in rows {
        let chunk = r?;
        if let Some(blob) = &chunk.embedding_blob {
            let vec = decode_f32_le(blob);
            let sim = cosine_similarity(query_embedding, &vec);
            if sim > 0.0 {
                scored.push((sim, chunk));
            }
        }
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored.truncate(CANDIDATE_LIMIT);
    Ok(scored.into_iter().map(|(_, c)| c).collect())
}

fn assemble_chunk_context(conn: &Connection, chunk: &DocChunk) -> Result<String> {
    if chunk.section_kind == SectionKind::ClaimDep && chunk.parent_claim_no.is_some() {
        let mut chain: Vec<DocChunk> = Vec::new();
        let mut visited: HashSet<i32> = HashSet::new();
        if let Some(cno) = chunk.claim_no {
            visited.insert(cno);
        }
        let mut cur_parent = chunk.parent_claim_no;
        let mut depth = 0usize;
        while let Some(p_no) = cur_parent {
            if depth >= 20 || !visited.insert(p_no) {
                break;
            }
            depth += 1;
            if let Some(parent_chunk) = get_chunk_by_claim_no(conn, &chunk.doc_id, p_no)? {
                cur_parent = parent_chunk.parent_claim_no;
                chain.push(parent_chunk);
            } else {
                break;
            }
        }
        chain.reverse();
        let mut assembled = String::new();
        for anc in &chain {
            assembled.push_str(&format!("[{}] {}\n\n", anc.section_tag, anc.raw_content));
        }
        assembled.push_str(&format!("[{}] {}", chunk.section_tag, chunk.raw_content));
        return Ok(assembled);
    }

    if let (Some(p_start), Some(p_end)) = (chunk.para_start, chunk.para_end) {
        let min_p = p_start.saturating_sub(1);
        let max_p = p_end.saturating_add(1);
        let neighbors = get_adjacent_para_chunks(conn, &chunk.doc_id, min_p, max_p)?;
        if neighbors.len() > 1 {
            let mut parts = Vec::new();
            for n in neighbors {
                if n.chunk_id == chunk.chunk_id {
                    parts.push(format!("[{}] {}", n.section_tag, n.raw_content));
                } else {
                    parts.push(format!(
                        "[邻接 {}] {}",
                        n.section_tag,
                        truncate_chars(&n.raw_content, 240)
                    ));
                }
            }
            return Ok(parts.join("\n"));
        }
    }

    Ok(format!("[{}] {}", chunk.section_tag, chunk.raw_content))
}

/// 混合检索主入口：支持锚点精确回查、BM25 + 向量加权 RRF 融合、从属权项链自动回溯与 `char_budget` 控长
#[allow(clippy::too_many_arguments)]
pub fn hybrid_search_chunks(
    conn: &Connection,
    query: &str,
    mode: SearchSceneMode,
    doc_ids: Option<&[String]>,
    role_filter: Option<AssetRole>,
    query_embedding: Option<&[f32]>,
    top_k: usize,
    char_budget: usize,
) -> Result<HybridEvidenceBundle> {
    let effective_top_k = top_k.max(1);
    let effective_budget = if char_budget == 0 { 12000 } else { char_budget };

    // 1. AnchorExact 优先 O(1) 锚点直击
    if mode == SearchSceneMode::AnchorExact {
        let exact_chunks = search_exact_anchors(conn, query, doc_ids, role_filter)?;
        if !exact_chunks.is_empty() {
            return finalize_bundle(
                conn,
                query,
                mode,
                exact_chunks.into_iter().map(|c| (c, 1.0)).collect(),
                effective_top_k,
                effective_budget,
            );
        }
    }

    // 2. 词法路（FTS5 BM25）
    let fts_query = build_fts_or_query(query);
    let fts_hits = search_fts_candidates(conn, &fts_query, doc_ids, role_filter)?;

    // 3. 向量路（可选）
    let vec_hits = match query_embedding {
        Some(emb) if !emb.is_empty() => search_vector_candidates(conn, emb, doc_ids, role_filter)?,
        _ => Vec::new(),
    };

    // 4. 加权 RRF 融合（k = 60.0）
    let mut by_id: HashMap<i64, (DocChunk, Option<usize>, Option<usize>)> = HashMap::new();
    for (idx, chunk) in fts_hits.into_iter().enumerate() {
        let rank = idx + 1;
        by_id
            .entry(chunk.chunk_id)
            .or_insert_with(|| (chunk, None, None))
            .1 = Some(rank);
    }
    for (idx, chunk) in vec_hits.into_iter().enumerate() {
        let rank = idx + 1;
        by_id
            .entry(chunk.chunk_id)
            .or_insert_with(|| (chunk, None, None))
            .2 = Some(rank);
    }

    let mut fused: Vec<(DocChunk, f64)> = by_id
        .into_values()
        .map(|(chunk, r_bm25, r_vec)| {
            let b_term = r_bm25.map(|r| 1.0 / (RRF_K + r as f64)).unwrap_or(0.0);
            let v_term = r_vec.map(|r| 1.0 / (RRF_K + r as f64)).unwrap_or(0.0);
            let score = chunk.section_kind.rrf_weight() * (b_term + v_term);
            (chunk, score)
        })
        .collect();

    fused.sort_by(|a, b| b.1.total_cmp(&a.1));

    finalize_bundle(conn, query, mode, fused, effective_top_k, effective_budget)
}

fn finalize_bundle(
    conn: &Connection,
    query: &str,
    mode: SearchSceneMode,
    candidates: Vec<(DocChunk, f64)>,
    top_k: usize,
    char_budget: usize,
) -> Result<HybridEvidenceBundle> {
    let mut hits = Vec::new();
    let mut total_chars = 0usize;
    let mut hit_doc_ids = Vec::new();
    let mut seen_docs = HashSet::new();

    for (chunk, rrf_score) in candidates.into_iter().take(top_k) {
        if total_chars >= char_budget {
            break;
        }
        let assembled_full = assemble_chunk_context(conn, &chunk)?;
        let remaining = char_budget.saturating_sub(total_chars);
        let assembled_context = if assembled_full.chars().count() > remaining {
            truncate_chars(&assembled_full, remaining)
        } else {
            assembled_full
        };
        let added = assembled_context.chars().count();
        if added == 0 {
            continue;
        }
        total_chars += added;
        if seen_docs.insert(chunk.doc_id.clone()) {
            hit_doc_ids.push(chunk.doc_id.clone());
        }
        hits.push(HybridChunkHit {
            chunk,
            rrf_score,
            assembled_context,
        });
    }

    let mut figure_glossary: Vec<FigureNumeral> = Vec::new();
    for doc_id in &hit_doc_ids {
        let mut nums = get_figure_numerals(conn, doc_id)?;
        figure_glossary.append(&mut nums);
    }

    Ok(HybridEvidenceBundle {
        query: query.to_string(),
        mode,
        hits,
        total_chars,
        figure_glossary,
    })
}
