//! 知识金库与双栖混合检索内核（`src/core/vault/mod.rs`）
//!
//! 对外统一导出子模块与高层门面 [`VaultService`]，支撑卡片 ①～⑦ 独立通过 SQLite 金库互通。

pub mod chunker;
pub mod local_rag;
pub mod models;
pub mod repo;
pub mod schema_mig;

use anyhow::Result;
use rusqlite::Connection;

use models::{
    AssetRole, DagNode, DagRun, DocChunk, DocKind, ExperimentRecord, FeatureAtom, FigureNumeral,
    FilingStatus, HybridEvidenceBundle, KnowledgeDoc, SearchSceneMode,
};

/// 知识金库高层门面服务（无状态，直接复用传入的 WAL SQLite 连接）
#[derive(Debug, Default, Clone, Copy)]
pub struct VaultService;

impl VaultService {
    pub fn ensure_tables(conn: &Connection) -> Result<()> {
        schema_mig::ensure_vault_tables(conn)
    }

    pub fn backfill_legacy_patents_batch(conn: &Connection, batch_limit: usize) -> Result<usize> {
        schema_mig::backfill_legacy_patents_batch(conn, batch_limit)
    }

    pub fn build_fts_tokens(text: &str) -> String {
        chunker::build_fts_tokens(text)
    }

    pub fn upsert_document_with_chunks(conn: &Connection, doc: &KnowledgeDoc) -> Result<()> {
        repo::upsert_document_with_chunks(conn, doc)
    }

    pub fn get_document(conn: &Connection, doc_id: &str) -> Result<Option<KnowledgeDoc>> {
        repo::get_document(conn, doc_id)
    }

    pub fn list_documents(
        conn: &Connection,
        role_filter: Option<AssetRole>,
        kind_filter: Option<DocKind>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<KnowledgeDoc>> {
        repo::list_documents(conn, role_filter, kind_filter, limit, offset)
    }

    pub fn update_doc_role_and_filing(
        conn: &Connection,
        doc_id: &str,
        asset_role: AssetRole,
        filing_status: FilingStatus,
    ) -> Result<bool> {
        repo::update_doc_role_and_filing(conn, doc_id, asset_role, filing_status)
    }

    pub fn delete_document(conn: &Connection, doc_id: &str) -> Result<bool> {
        repo::delete_document(conn, doc_id)
    }

    pub fn get_chunk_by_anchor(
        conn: &Connection,
        doc_id: &str,
        section_tag: &str,
    ) -> Result<Option<DocChunk>> {
        repo::get_chunk_by_anchor(conn, doc_id, section_tag)
    }

    pub fn get_figure_numerals(conn: &Connection, doc_id: &str) -> Result<Vec<FigureNumeral>> {
        repo::get_figure_numerals(conn, doc_id)
    }

    pub fn save_feature_atoms(conn: &Connection, atoms: &[FeatureAtom]) -> Result<()> {
        repo::save_feature_atoms(conn, atoms)
    }

    pub fn list_feature_atoms(conn: &Connection, doc_id: &str) -> Result<Vec<FeatureAtom>> {
        repo::list_feature_atoms(conn, doc_id)
    }

    pub fn save_and_verify_experiment(
        conn: &Connection,
        exp: &ExperimentRecord,
    ) -> Result<Option<String>> {
        repo::save_and_verify_experiment(conn, exp)
    }

    pub fn get_experiment(conn: &Connection, exp_id: &str) -> Result<Option<ExperimentRecord>> {
        repo::get_experiment(conn, exp_id)
    }

    pub fn upsert_dag_run(conn: &Connection, run: &DagRun) -> Result<()> {
        repo::upsert_dag_run(conn, run)
    }

    pub fn get_dag_run(conn: &Connection, run_id: &str) -> Result<Option<DagRun>> {
        repo::get_dag_run(conn, run_id)
    }

    pub fn list_child_forks(conn: &Connection, parent_run_id: &str) -> Result<Vec<DagRun>> {
        repo::list_child_forks(conn, parent_run_id)
    }

    pub fn upsert_dag_node(conn: &Connection, node: &DagNode) -> Result<()> {
        repo::upsert_dag_node(conn, node)
    }

    pub fn list_dag_nodes(conn: &Connection, run_id: &str) -> Result<Vec<DagNode>> {
        repo::list_dag_nodes(conn, run_id)
    }

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
        local_rag::hybrid_search_chunks(
            conn,
            query,
            mode,
            doc_ids,
            role_filter,
            query_embedding,
            top_k,
            char_budget,
        )
    }
}
