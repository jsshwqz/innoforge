//! 数据库版本化平滑迁移模块（`src/core/vault/schema_mig.rs` · `v24 → v26` 无损演进）
//!
//! 严守 `AGENTS.md` §三.3 与重构总书 §3.1：
//! - 仅执行毫秒级幂等建表、加列、索引与 FTS5 触发器注册；
//! - 严禁启动时全表阻塞扫描，存量 `patents` 表记录通过后台小批次增量函数无损映射至 `knowledge_docs`。

use anyhow::Result;
use rusqlite::Connection;

/// `v25` 迁移 DDL：建立统一文献资产表 `knowledge_docs`、双栖结构化切片表 `doc_chunks` 与外部内容表 `chunks_fts`
pub const MIGRATION_V25_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS knowledge_docs (
    doc_id TEXT PRIMARY KEY,
    doc_kind TEXT NOT NULL DEFAULT 'patent',
    asset_role TEXT NOT NULL DEFAULT 'general_ref',
    external_id TEXT DEFAULT '',
    title_zh TEXT DEFAULT '',
    title_en TEXT DEFAULT '',
    authors_or_assignees TEXT DEFAULT '',
    pub_or_filing_date TEXT DEFAULT '',
    venue_or_authority TEXT DEFAULT '',
    filing_status TEXT NOT NULL DEFAULT 'public_disclosed',
    doi_verified INTEGER NOT NULL DEFAULT 0,
    bibtex_raw TEXT DEFAULT '',
    abstract_text TEXT DEFAULT '',
    claims_json TEXT DEFAULT '',
    fulltext_raw TEXT DEFAULT '',
    fulltext_status TEXT NOT NULL DEFAULT 'summary_only',
    chunk_ver INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_knowledge_docs_kind_extid
    ON knowledge_docs(doc_kind, external_id)
    WHERE external_id IS NOT NULL AND external_id != '';
CREATE INDEX IF NOT EXISTS idx_knowledge_docs_role_date
    ON knowledge_docs(asset_role, pub_or_filing_date);

CREATE TABLE IF NOT EXISTS doc_chunks (
    chunk_id INTEGER PRIMARY KEY AUTOINCREMENT,
    doc_id TEXT NOT NULL,
    section_kind TEXT NOT NULL DEFAULT 'fallback',
    section_tag TEXT NOT NULL,
    claim_no INTEGER NULL,
    parent_claim_no INTEGER NULL,
    para_start INTEGER NULL,
    para_end INTEGER NULL,
    embodiment_group TEXT NULL,
    raw_content TEXT NOT NULL,
    fts_tokens TEXT NOT NULL,
    embedding_blob BLOB NULL,
    char_count INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY(doc_id) REFERENCES knowledge_docs(doc_id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_doc_chunks_doc_tag
    ON doc_chunks(doc_id, section_tag);
CREATE INDEX IF NOT EXISTS idx_doc_chunks_doc_claim
    ON doc_chunks(doc_id, claim_no);
CREATE INDEX IF NOT EXISTS idx_doc_chunks_doc_kind
    ON doc_chunks(doc_id, section_kind);

CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    section_tag,
    fts_tokens,
    doc_id UNINDEXED,
    section_kind UNINDEXED,
    content='doc_chunks',
    content_rowid='chunk_id',
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER IF NOT EXISTS trg_doc_chunks_ai AFTER INSERT ON doc_chunks BEGIN
    INSERT INTO chunks_fts(rowid, section_tag, fts_tokens, doc_id, section_kind)
    VALUES (new.chunk_id, new.section_tag, new.fts_tokens, new.doc_id, new.section_kind);
END;

CREATE TRIGGER IF NOT EXISTS trg_doc_chunks_ad AFTER DELETE ON doc_chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, section_tag, fts_tokens, doc_id, section_kind)
    VALUES ('delete', old.chunk_id, old.section_tag, old.fts_tokens, old.doc_id, old.section_kind);
END;

CREATE TRIGGER IF NOT EXISTS trg_doc_chunks_au AFTER UPDATE ON doc_chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, section_tag, fts_tokens, doc_id, section_kind)
    VALUES ('delete', old.chunk_id, old.section_tag, old.fts_tokens, old.doc_id, old.section_kind);
    INSERT INTO chunks_fts(rowid, section_tag, fts_tokens, doc_id, section_kind)
    VALUES (new.chunk_id, new.section_tag, new.fts_tokens, new.doc_id, new.section_kind);
END;
"#;

/// `v26` 迁移 DDL：建立 `figure_numerals`、`feature_atoms`、`experiment_records`、`dag_runs` 与 `dag_nodes`
pub const MIGRATION_V26_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS figure_numerals (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    doc_id TEXT NOT NULL,
    figure_label TEXT DEFAULT '',
    numeral_code TEXT NOT NULL,
    part_name TEXT NOT NULL,
    embodiment_group TEXT NULL,
    anchor_tag TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_figure_numerals_doc_code
    ON figure_numerals(doc_id, numeral_code);

CREATE TABLE IF NOT EXISTS feature_atoms (
    atom_id TEXT PRIMARY KEY,
    doc_id TEXT NOT NULL,
    anchor_tag TEXT NOT NULL,
    tech_problem TEXT DEFAULT '',
    tech_means TEXT NOT NULL,
    tech_effect TEXT DEFAULT '',
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_feature_atoms_doc
    ON feature_atoms(doc_id);

CREATE TABLE IF NOT EXISTS experiment_records (
    exp_id TEXT PRIMARY KEY,
    run_id TEXT NULL,
    title TEXT NOT NULL,
    hypothesis TEXT NOT NULL,
    control_group_spec TEXT NOT NULL,
    test_group_spec TEXT NOT NULL,
    sandbox_output_json TEXT NULL,
    measured_data_json TEXT NULL,
    status TEXT NOT NULL DEFAULT 'designed'
);
CREATE INDEX IF NOT EXISTS idx_experiment_records_run
    ON experiment_records(run_id);

CREATE TABLE IF NOT EXISTS dag_runs (
    run_id TEXT PRIMARY KEY,
    card_kind TEXT NOT NULL,
    parent_run_id TEXT NULL,
    fork_from_step TEXT NULL,
    tier_mode TEXT NOT NULL DEFAULT 'standard',
    scheme_baseline_json TEXT NOT NULL DEFAULT '{}',
    mounted_own_doc_ids TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'chatting',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_dag_runs_parent
    ON dag_runs(parent_run_id);

CREATE TABLE IF NOT EXISTS dag_nodes (
    node_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    step_key TEXT NOT NULL,
    step_index INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending',
    input_json TEXT NOT NULL DEFAULT '{}',
    output_json TEXT NOT NULL DEFAULT '{}',
    checkpoint_payload_json TEXT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_dag_nodes_run_step
    ON dag_nodes(run_id, step_index);
"#;

/// 对任意独立 SQLite 连接开启 WAL + busy_timeout 并幂等补齐 v25/v26 金库表
pub fn ensure_vault_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA busy_timeout=5000; PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
    )?;
    conn.execute_batch(MIGRATION_V25_SQL)?;
    conn.execute_batch(MIGRATION_V26_SQL)?;
    Ok(())
}

/// 按需增量回填老库 `patents` 表记录至 `knowledge_docs`（单次上限 `batch_limit` 条，零启动阻塞）
pub fn backfill_legacy_patents_batch(conn: &Connection, batch_limit: usize) -> Result<usize> {
    let table_exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='patents'",
        [],
        |r| r.get(0),
    )?;
    if table_exists == 0 || batch_limit == 0 {
        return Ok(0);
    }

    let inserted = conn.execute(
        "INSERT OR IGNORE INTO knowledge_docs (
            doc_id, doc_kind, asset_role, external_id, title_zh, title_en,
            authors_or_assignees, pub_or_filing_date, venue_or_authority,
            filing_status, doi_verified, bibtex_raw, abstract_text,
            claims_json, fulltext_raw, fulltext_status, chunk_ver
        )
        SELECT
            p.id,
            'patent',
            'general_ref',
            COALESCE(NULLIF(p.patent_number, ''), p.id),
            COALESCE(p.title, ''),
            '',
            COALESCE(p.applicant, ''),
            COALESCE(NULLIF(p.publication_date, ''), COALESCE(p.filing_date, '')),
            COALESCE(p.country, ''),
            'public_disclosed',
            0,
            '',
            COALESCE(p.abstract_text, ''),
            COALESCE(p.claims, ''),
            COALESCE(p.description, ''),
            CASE WHEN LENGTH(COALESCE(p.description, '')) > 50 THEN 'fulltext_ready' ELSE 'summary_only' END,
            0
        FROM patents p
        WHERE NOT EXISTS (
            SELECT 1 FROM knowledge_docs kd WHERE kd.doc_id = p.id
        )
        LIMIT ?1",
        [batch_limit as i64],
    )?;

    Ok(inserted)
}
