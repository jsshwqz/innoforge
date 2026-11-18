//! 知识金库单事务批处理仓储层（`src/core/vault/repo.rs`）
//!
//! 严守 `AGENTS.md` §三.1 与 §三.3：
//! - 零 `unwrap()` / `expect()`；
//! - 单事务批处理（`unchecked_transaction` + 预编译语句复用），零循环 N+1；
//! - `get_chunk_by_anchor` 走 `idx_doc_chunks_doc_tag` 唯一索引 O(1) 精确回查。

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension, Row};

use super::chunker::chunk_document;
use super::models::{
    AssetRole, DagNode, DagRun, DagRunStatus, DocChunk, DocKind, ExperimentRecord,
    ExperimentStatus, FeatureAtom, FigureNumeral, FilingStatus, KnowledgeDoc, SectionKind,
};

fn row_to_doc(row: &Row<'_>) -> rusqlite::Result<KnowledgeDoc> {
    let doc_kind_str: String = row.get(1)?;
    let asset_role_str: String = row.get(2)?;
    let filing_status_str: String = row.get(9)?;
    let doi_verified_int: i64 = row.get(10)?;
    Ok(KnowledgeDoc {
        doc_id: row.get(0)?,
        doc_kind: DocKind::parse(&doc_kind_str),
        asset_role: AssetRole::parse(&asset_role_str),
        external_id: row.get(3)?,
        title_zh: row.get(4)?,
        title_en: row.get(5)?,
        authors_or_assignees: row.get(6)?,
        pub_or_filing_date: row.get(7)?,
        venue_or_authority: row.get(8)?,
        filing_status: FilingStatus::parse(&filing_status_str),
        doi_verified: doi_verified_int != 0,
        bibtex_raw: row.get(11)?,
        abstract_text: row.get(12)?,
        claims_json: row.get(13)?,
        fulltext_raw: row.get(14)?,
        fulltext_status: row.get(15)?,
        chunk_ver: row.get(16)?,
    })
}

pub(crate) fn row_to_chunk(row: &Row<'_>) -> rusqlite::Result<DocChunk> {
    let kind_str: String = row.get(2)?;
    let char_count_i64: i64 = row.get(12)?;
    Ok(DocChunk {
        chunk_id: row.get(0)?,
        doc_id: row.get(1)?,
        section_kind: SectionKind::parse(&kind_str),
        section_tag: row.get(3)?,
        claim_no: row.get(4)?,
        parent_claim_no: row.get(5)?,
        para_start: row.get(6)?,
        para_end: row.get(7)?,
        embodiment_group: row.get(8)?,
        raw_content: row.get(9)?,
        fts_tokens: row.get(10)?,
        embedding_blob: row.get(11)?,
        char_count: usize::try_from(char_count_i64).unwrap_or(0),
    })
}

fn row_to_dag_run(row: &Row<'_>) -> rusqlite::Result<DagRun> {
    let mounted_raw: String = row.get(6)?;
    let mounted_own_doc_ids: Vec<String> = serde_json::from_str(&mounted_raw).unwrap_or_default();
    let status_str: String = row.get(7)?;
    Ok(DagRun {
        run_id: row.get(0)?,
        card_kind: row.get(1)?,
        parent_run_id: row.get(2)?,
        fork_from_step: row.get(3)?,
        tier_mode: row.get(4)?,
        scheme_baseline_json: row.get(5)?,
        mounted_own_doc_ids,
        status: DagRunStatus::parse(&status_str),
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

/// 写入或更新文献并单事务重建结构化切片与附图部件词典（`chunk_ver = 2`）
pub fn upsert_document_with_chunks(conn: &Connection, doc: &KnowledgeDoc) -> Result<()> {
    let (chunks, numerals) = chunk_document(doc);
    let chunk_ver = doc.chunk_ver.max(2);

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO knowledge_docs (
            doc_id, doc_kind, asset_role, external_id, title_zh, title_en,
            authors_or_assignees, pub_or_filing_date, venue_or_authority,
            filing_status, doi_verified, bibtex_raw, abstract_text,
            claims_json, fulltext_raw, fulltext_status, chunk_ver
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
        ON CONFLICT(doc_id) DO UPDATE SET
            doc_kind = excluded.doc_kind,
            asset_role = excluded.asset_role,
            external_id = excluded.external_id,
            title_zh = excluded.title_zh,
            title_en = excluded.title_en,
            authors_or_assignees = excluded.authors_or_assignees,
            pub_or_filing_date = excluded.pub_or_filing_date,
            venue_or_authority = excluded.venue_or_authority,
            filing_status = excluded.filing_status,
            doi_verified = excluded.doi_verified,
            bibtex_raw = excluded.bibtex_raw,
            abstract_text = excluded.abstract_text,
            claims_json = excluded.claims_json,
            fulltext_raw = excluded.fulltext_raw,
            fulltext_status = excluded.fulltext_status,
            chunk_ver = excluded.chunk_ver",
        params![
            doc.doc_id,
            doc.doc_kind.as_str(),
            doc.asset_role.as_str(),
            doc.external_id,
            doc.title_zh,
            doc.title_en,
            doc.authors_or_assignees,
            doc.pub_or_filing_date,
            doc.venue_or_authority,
            doc.filing_status.as_str(),
            if doc.doi_verified { 1i64 } else { 0i64 },
            doc.bibtex_raw,
            doc.abstract_text,
            doc.claims_json,
            doc.fulltext_raw,
            doc.fulltext_status,
            chunk_ver,
        ],
    )?;

    tx.execute(
        "DELETE FROM doc_chunks WHERE doc_id = ?1",
        params![doc.doc_id],
    )?;
    tx.execute(
        "DELETE FROM figure_numerals WHERE doc_id = ?1",
        params![doc.doc_id],
    )?;

    if !chunks.is_empty() {
        let mut stmt = tx.prepare(
            "INSERT INTO doc_chunks (
                doc_id, section_kind, section_tag, claim_no, parent_claim_no,
                para_start, para_end, embodiment_group, raw_content, fts_tokens,
                embedding_blob, char_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )?;
        for c in &chunks {
            stmt.execute(params![
                c.doc_id,
                c.section_kind.as_str(),
                c.section_tag,
                c.claim_no,
                c.parent_claim_no,
                c.para_start,
                c.para_end,
                c.embodiment_group,
                c.raw_content,
                c.fts_tokens,
                c.embedding_blob,
                i64::try_from(c.char_count).unwrap_or(i64::MAX),
            ])?;
        }
    }

    if !numerals.is_empty() {
        let mut stmt = tx.prepare(
            "INSERT INTO figure_numerals (
                doc_id, figure_label, numeral_code, part_name, embodiment_group, anchor_tag
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for n in &numerals {
            stmt.execute(params![
                n.doc_id,
                n.figure_label,
                n.numeral_code,
                n.part_name,
                n.embodiment_group,
                n.anchor_tag,
            ])?;
        }
    }

    tx.commit()?;
    Ok(())
}

pub fn get_document(conn: &Connection, doc_id: &str) -> Result<Option<KnowledgeDoc>> {
    let doc = conn
        .query_row(
            "SELECT doc_id, doc_kind, asset_role, external_id, title_zh, title_en,
                    authors_or_assignees, pub_or_filing_date, venue_or_authority,
                    filing_status, doi_verified, bibtex_raw, abstract_text,
                    claims_json, fulltext_raw, fulltext_status, chunk_ver
             FROM knowledge_docs
             WHERE doc_id = ?1",
            params![doc_id],
            row_to_doc,
        )
        .optional()?;
    Ok(doc)
}

pub fn list_documents(
    conn: &Connection,
    role_filter: Option<AssetRole>,
    kind_filter: Option<DocKind>,
    limit: usize,
    offset: usize,
) -> Result<Vec<KnowledgeDoc>> {
    let role_str = role_filter.map(|r| r.as_str().to_string());
    let kind_str = kind_filter.map(|k| k.as_str().to_string());
    let limit_i64 = i64::try_from(limit).unwrap_or(100);
    let offset_i64 = i64::try_from(offset).unwrap_or(0);

    let mut stmt = conn.prepare(
        "SELECT doc_id, doc_kind, asset_role, external_id, title_zh, title_en,
                authors_or_assignees, pub_or_filing_date, venue_or_authority,
                filing_status, doi_verified, bibtex_raw, abstract_text,
                claims_json, fulltext_raw, fulltext_status, chunk_ver
         FROM knowledge_docs
         WHERE (?1 IS NULL OR asset_role = ?1)
           AND (?2 IS NULL OR doc_kind = ?2)
         ORDER BY pub_or_filing_date DESC, doc_id ASC
         LIMIT ?3 OFFSET ?4",
    )?;
    let rows = stmt.query_map(
        params![role_str, kind_str, limit_i64, offset_i64],
        row_to_doc,
    )?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn update_doc_role_and_filing(
    conn: &Connection,
    doc_id: &str,
    asset_role: AssetRole,
    filing_status: FilingStatus,
) -> Result<bool> {
    let affected = conn.execute(
        "UPDATE knowledge_docs
         SET asset_role = ?2, filing_status = ?3
         WHERE doc_id = ?1",
        params![doc_id, asset_role.as_str(), filing_status.as_str()],
    )?;
    Ok(affected > 0)
}

pub fn delete_document(conn: &Connection, doc_id: &str) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM figure_numerals WHERE doc_id = ?1",
        params![doc_id],
    )?;
    tx.execute(
        "DELETE FROM feature_atoms WHERE doc_id = ?1",
        params![doc_id],
    )?;
    let affected = tx.execute(
        "DELETE FROM knowledge_docs WHERE doc_id = ?1",
        params![doc_id],
    )?;
    tx.commit()?;
    Ok(affected > 0)
}

/// 走 `idx_doc_chunks_doc_tag` 唯一索引 O(1) 精确回查原文切片
pub fn get_chunk_by_anchor(
    conn: &Connection,
    doc_id: &str,
    section_tag: &str,
) -> Result<Option<DocChunk>> {
    let chunk = conn
        .query_row(
            "SELECT chunk_id, doc_id, section_kind, section_tag, claim_no, parent_claim_no,
                    para_start, para_end, embodiment_group, raw_content, fts_tokens,
                    embedding_blob, char_count
             FROM doc_chunks
             WHERE doc_id = ?1 AND section_tag = ?2",
            params![doc_id, section_tag],
            row_to_chunk,
        )
        .optional()?;
    Ok(chunk)
}

pub(crate) fn get_chunk_by_claim_no(
    conn: &Connection,
    doc_id: &str,
    claim_no: i32,
) -> Result<Option<DocChunk>> {
    let chunk = conn
        .query_row(
            "SELECT chunk_id, doc_id, section_kind, section_tag, claim_no, parent_claim_no,
                    para_start, para_end, embodiment_group, raw_content, fts_tokens,
                    embedding_blob, char_count
             FROM doc_chunks
             WHERE doc_id = ?1 AND claim_no = ?2
             LIMIT 1",
            params![doc_id, claim_no],
            row_to_chunk,
        )
        .optional()?;
    Ok(chunk)
}

pub(crate) fn get_adjacent_para_chunks(
    conn: &Connection,
    doc_id: &str,
    min_para: i32,
    max_para: i32,
) -> Result<Vec<DocChunk>> {
    let mut stmt = conn.prepare(
        "SELECT chunk_id, doc_id, section_kind, section_tag, claim_no, parent_claim_no,
                para_start, para_end, embodiment_group, raw_content, fts_tokens,
                embedding_blob, char_count
         FROM doc_chunks
         WHERE doc_id = ?1 AND para_start IS NOT NULL AND para_start BETWEEN ?2 AND ?3
         ORDER BY para_start ASC",
    )?;
    let rows = stmt.query_map(params![doc_id, min_para, max_para], row_to_chunk)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn get_figure_numerals(conn: &Connection, doc_id: &str) -> Result<Vec<FigureNumeral>> {
    let mut stmt = conn.prepare(
        "SELECT id, doc_id, figure_label, numeral_code, part_name, embodiment_group, anchor_tag
         FROM figure_numerals
         WHERE doc_id = ?1
         ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![doc_id], |row| {
        Ok(FigureNumeral {
            id: row.get(0)?,
            doc_id: row.get(1)?,
            figure_label: row.get(2)?,
            numeral_code: row.get(3)?,
            part_name: row.get(4)?,
            embodiment_group: row.get(5)?,
            anchor_tag: row.get(6)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn save_feature_atoms(conn: &Connection, atoms: &[FeatureAtom]) -> Result<()> {
    if atoms.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO feature_atoms (
                atom_id, doc_id, anchor_tag, tech_problem, tech_means, tech_effect, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(atom_id) DO UPDATE SET
                doc_id = excluded.doc_id,
                anchor_tag = excluded.anchor_tag,
                tech_problem = excluded.tech_problem,
                tech_means = excluded.tech_means,
                tech_effect = excluded.tech_effect,
                created_at = excluded.created_at",
        )?;
        for a in atoms {
            stmt.execute(params![
                a.atom_id,
                a.doc_id,
                a.anchor_tag,
                a.tech_problem,
                a.tech_means,
                a.tech_effect,
                a.created_at,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn list_feature_atoms(conn: &Connection, doc_id: &str) -> Result<Vec<FeatureAtom>> {
    let mut stmt = conn.prepare(
        "SELECT atom_id, doc_id, anchor_tag, tech_problem, tech_means, tech_effect, created_at
         FROM feature_atoms
         WHERE doc_id = ?1
         ORDER BY created_at ASC, atom_id ASC",
    )?;
    let rows = stmt.query_map(params![doc_id], |row| {
        Ok(FeatureAtom {
            atom_id: row.get(0)?,
            doc_id: row.get(1)?,
            anchor_tag: row.get(2)?,
            tech_problem: row.get(3)?,
            tech_means: row.get(4)?,
            tech_effect: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 落库对照实验记录，并在 `RealDataFilled` / `Verified` 时返回铁证锚点 `Some("Exp:{exp_id}")`
pub fn save_and_verify_experiment(
    conn: &Connection,
    exp: &ExperimentRecord,
) -> Result<Option<String>> {
    conn.execute(
        "INSERT INTO experiment_records (
            exp_id, run_id, title, hypothesis, control_group_spec, test_group_spec,
            sandbox_output_json, measured_data_json, status
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ON CONFLICT(exp_id) DO UPDATE SET
            run_id = excluded.run_id,
            title = excluded.title,
            hypothesis = excluded.hypothesis,
            control_group_spec = excluded.control_group_spec,
            test_group_spec = excluded.test_group_spec,
            sandbox_output_json = excluded.sandbox_output_json,
            measured_data_json = excluded.measured_data_json,
            status = excluded.status",
        params![
            exp.exp_id,
            exp.run_id,
            exp.title,
            exp.hypothesis,
            exp.control_group_spec,
            exp.test_group_spec,
            exp.sandbox_output_json,
            exp.measured_data_json,
            exp.status.as_str(),
        ],
    )?;
    Ok(exp.evidence_anchor())
}

pub fn get_experiment(conn: &Connection, exp_id: &str) -> Result<Option<ExperimentRecord>> {
    let rec = conn
        .query_row(
            "SELECT exp_id, run_id, title, hypothesis, control_group_spec, test_group_spec,
                    sandbox_output_json, measured_data_json, status
             FROM experiment_records
             WHERE exp_id = ?1",
            params![exp_id],
            |row| {
                let status_str: String = row.get(8)?;
                Ok(ExperimentRecord {
                    exp_id: row.get(0)?,
                    run_id: row.get(1)?,
                    title: row.get(2)?,
                    hypothesis: row.get(3)?,
                    control_group_spec: row.get(4)?,
                    test_group_spec: row.get(5)?,
                    sandbox_output_json: row.get(6)?,
                    measured_data_json: row.get(7)?,
                    status: ExperimentStatus::parse(&status_str),
                })
            },
        )
        .optional()?;
    Ok(rec)
}

pub fn upsert_dag_run(conn: &Connection, run: &DagRun) -> Result<()> {
    let mounted_json = serde_json::to_string(&run.mounted_own_doc_ids)?;
    conn.execute(
        "INSERT INTO dag_runs (
            run_id, card_kind, parent_run_id, fork_from_step, tier_mode,
            scheme_baseline_json, mounted_own_doc_ids, status, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT(run_id) DO UPDATE SET
            card_kind = excluded.card_kind,
            parent_run_id = excluded.parent_run_id,
            fork_from_step = excluded.fork_from_step,
            tier_mode = excluded.tier_mode,
            scheme_baseline_json = excluded.scheme_baseline_json,
            mounted_own_doc_ids = excluded.mounted_own_doc_ids,
            status = excluded.status,
            updated_at = excluded.updated_at",
        params![
            run.run_id,
            run.card_kind,
            run.parent_run_id,
            run.fork_from_step,
            run.tier_mode,
            run.scheme_baseline_json,
            mounted_json,
            run.status.as_str(),
            run.created_at,
            run.updated_at,
        ],
    )?;
    Ok(())
}

pub fn get_dag_run(conn: &Connection, run_id: &str) -> Result<Option<DagRun>> {
    let rec = conn
        .query_row(
            "SELECT run_id, card_kind, parent_run_id, fork_from_step, tier_mode,
                    scheme_baseline_json, mounted_own_doc_ids, status, created_at, updated_at
             FROM dag_runs
             WHERE run_id = ?1",
            params![run_id],
            row_to_dag_run,
        )
        .optional()?;
    Ok(rec)
}

pub fn list_child_forks(conn: &Connection, parent_run_id: &str) -> Result<Vec<DagRun>> {
    let mut stmt = conn.prepare(
        "SELECT run_id, card_kind, parent_run_id, fork_from_step, tier_mode,
                scheme_baseline_json, mounted_own_doc_ids, status, created_at, updated_at
         FROM dag_runs
         WHERE parent_run_id = ?1
         ORDER BY created_at ASC, run_id ASC",
    )?;
    let rows = stmt.query_map(params![parent_run_id], row_to_dag_run)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn upsert_dag_node(conn: &Connection, node: &DagNode) -> Result<()> {
    conn.execute(
        "INSERT INTO dag_nodes (
            node_id, run_id, step_key, step_index, status,
            input_json, output_json, checkpoint_payload_json, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ON CONFLICT(node_id) DO UPDATE SET
            run_id = excluded.run_id,
            step_key = excluded.step_key,
            step_index = excluded.step_index,
            status = excluded.status,
            input_json = excluded.input_json,
            output_json = excluded.output_json,
            checkpoint_payload_json = excluded.checkpoint_payload_json,
            updated_at = excluded.updated_at",
        params![
            node.node_id,
            node.run_id,
            node.step_key,
            node.step_index,
            node.status,
            node.input_json,
            node.output_json,
            node.checkpoint_payload_json,
            node.updated_at,
        ],
    )?;
    Ok(())
}

pub fn list_dag_nodes(conn: &Connection, run_id: &str) -> Result<Vec<DagNode>> {
    let mut stmt = conn.prepare(
        "SELECT node_id, run_id, step_key, step_index, status,
                input_json, output_json, checkpoint_payload_json, updated_at
         FROM dag_nodes
         WHERE run_id = ?1
         ORDER BY step_index ASC, node_id ASC",
    )?;
    let rows = stmt.query_map(params![run_id], |row| {
        Ok(DagNode {
            node_id: row.get(0)?,
            run_id: row.get(1)?,
            step_key: row.get(2)?,
            step_index: row.get(3)?,
            status: row.get(4)?,
            input_json: row.get(5)?,
            output_json: row.get(6)?,
            checkpoint_payload_json: row.get(7)?,
            updated_at: row.get(8)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}
