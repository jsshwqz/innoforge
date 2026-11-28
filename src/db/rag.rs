use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatentChunk {
    pub id: String,
    pub patent_id: String,
    pub chunk_index: i32,
    pub source_type: String,
    /// 结构化分块的段落标签（Batch 1 任务 1.2）：`Claim:N` / `Desc:[0001]` /
    /// `Desc:具体实施方式` / `Desc:Window` / `Abstract`；存量数据为空串。
    /// 旧 JSON 反序列化靠 `#[serde(default)]` 兼容（AGENTS.md 2.2 数据不破）。
    #[serde(default)]
    pub section_tag: String,
    pub content: String,
    pub embedding: Vec<f32>,
}

/// 跨专利说明书实施例分块检索命中（用于专利法第26条第4款支撑核对）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbodimentChunkHit {
    pub chunk_id: String,
    pub patent_id: String,
    pub section_tag: String,
    pub content: String,
    pub score: f32,
}

use super::Database;

/// RRF（Reciprocal Rank Fusion）融合常数（Batch 1 任务 1.3 任务书口径 k=60）。
/// 与 `/api/search/vector`（routes/search.rs，专利表级 RRF）同一公式 `1/(k+rank)`、
/// 同一 rank 约定（1-based）；chunks 级融合以本模块 `rrf_fuse` 为单一出处。
pub const RRF_K: f32 = 60.0;

/// FTS5 查询 token 数上限（防御病态超长查询拖垮 MATCH）。
const FTS_MAX_TOKENS: usize = 64;

impl Database {
    /// Save all chunks for a patent.
    ///
    /// Batch 1 任务 1.3：同一事务内同步维护 `chunks_fts`（delete 旧 patent + 逐条
    /// insert bigram 索引正文），保证词汇通道与向量通道看到同一份数据。
    pub fn save_patent_chunks(
        &self,
        patent_id: &str,
        chunks: &[PatentChunk],
        model_name: &str,
    ) -> Result<usize, rusqlite::Error> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        let count = Self::write_chunks_in_tx(&tx, patent_id, chunks, model_name)?;
        tx.commit()?;
        Ok(count)
    }

    fn write_chunks_in_tx(
        tx: &rusqlite::Transaction<'_>,
        patent_id: &str,
        chunks: &[PatentChunk],
        model_name: &str,
    ) -> Result<usize, rusqlite::Error> {
        // Delete old chunks for this patent (both authoritative rows and FTS mirror)
        tx.execute(
            "DELETE FROM patent_chunks WHERE patent_id = ?1",
            rusqlite::params![patent_id],
        )?;
        tx.execute(
            "DELETE FROM chunks_fts WHERE patent_id = ?1",
            rusqlite::params![patent_id],
        )?;

        let mut count = 0;
        for chunk in chunks {
            let bytes: Vec<u8> = chunk
                .embedding
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect();
            tx.execute(
                "INSERT INTO patent_chunks (id, patent_id, chunk_index, source_type, section_tag, content, embedding, model_name) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    chunk.id, patent_id, chunk.chunk_index,
                    chunk.source_type, chunk.section_tag, chunk.content,
                    rusqlite::types::Value::Blob(bytes), model_name
                ],
            )?;
            tx.execute(
                "INSERT INTO chunks_fts(content, chunk_id, patent_id) VALUES (?1, ?2, ?3)",
                rusqlite::params![fts_bigram_text(&chunk.content), chunk.id, patent_id],
            )?;
            count += 1;
        }
        Ok(count)
    }

    /// 查询主数据库文件所在的磁盘路径（`:memory:` 内存库返回 `None`），无锁读取，供磁盘守卫与独立 WAL 写连接使用。
    pub fn main_db_file_path(&self) -> Option<std::path::PathBuf> {
        self.db_path.clone()
    }

    /// 为后台 GB 级批量导入打开独立的 SQLite WAL 写入连接。
    ///
    /// 在 SQLite WAL 模式下，独立写连接与主连接 `self.conn` 的前台读请求 100% 并发零互斥，
    /// 彻底杜绝后台批量导入长时间占用主连接 `Mutex<Connection>` 导致 Tokio 工作线程全员阻塞、Web 页面打不开。
    fn open_dedicated_wal_writer(
        db_path: &std::path::Path,
    ) -> Result<rusqlite::Connection, rusqlite::Error> {
        let conn = rusqlite::Connection::open(db_path)?;
        conn.execute_batch(
            "PRAGMA busy_timeout=30000; PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA wal_autocheckpoint=1000; PRAGMA journal_size_limit=33554432; PRAGMA temp_store=MEMORY; PRAGMA cache_size=-16000;",
        )?;
        Ok(conn)
    }

    /// 增量防覆盖铁律：若同规范公开号记录已存在于本机库（例如之前已 Enrich 过完整权利要求/说明书/附图），
    /// 批量导入轻量著录表时**绝不覆盖已有更丰富的全文/附图/中文标题/法律状态**，仅合并补齐缺失字段。
    pub fn batch_import_patents_with_chunks(
        &self,
        patents: &[crate::patent::Patent],
        model_name: &str,
    ) -> Result<(usize, usize), rusqlite::Error> {
        let (imported, chunks, _) =
            self.batch_import_patents_chunked(patents, false, model_name, None)?;
        Ok((imported, chunks))
    }

    /// GB 级批量分片入库引擎（独立 WAL 写连接零阻断主站 + 微事务分段提交 + 批间让出 + 实时取消信号）：
    /// - 文件库模式下自动打开独立 WAL 写连接，全程不占用 `self.conn` 互斥锁，前台页面/检索毫秒级响应；
    /// - `lightweight = true`：仅写 `patents` + `patents_fts`（跳过百万级 `patent_chunks` / `chunks_fts` / `patents_embedding`），
    ///   按 100 行微事务提交并在批间让出 30ms 给前台写入，防止大库跨盘写入触发 `SQLITE_BUSY` 或 WAL 暴涨；
    /// - `lightweight = false`：同步写入 `patent_chunks` + `chunks_fts` + `patents_embedding` (`char-tfidf-v2`)，按 50 行分段提交。
    ///
    /// 返回 `(imported, chunks_written, cancelled)`。
    pub fn batch_import_patents_chunked(
        &self,
        patents: &[crate::patent::Patent],
        lightweight: bool,
        model_name: &str,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<(usize, usize, bool), rusqlite::Error> {
        use std::sync::atomic::Ordering;

        let _bulk_guard = self.begin_bulk_write();
        // 轻量著录导入采用 500 行分段提交（减少 5 倍 WAL fsync 次数且在线搜索在 bulk_write 期间自动避让写锁，兼顾吞吐与响应）
        let chunk_size = if lightweight { 500 } else { 50 };
        let total_batches = patents.len().div_ceil(chunk_size);
        let mut dedicated_conn = self
            .db_path
            .as_deref()
            .and_then(|p| Self::open_dedicated_wal_writer(p).ok());
        let mut imported = 0usize;
        let mut chunks_written = 0usize;
        let mut cancelled = false;

        for (batch_idx, batch) in patents.chunks(chunk_size).enumerate() {
            if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                cancelled = true;
                break;
            }
            let (batch_imp, batch_chunks) = if let Some(ref mut writer) = dedicated_conn {
                Self::write_patent_batch_in_tx_with_retry(writer, batch, lightweight, model_name)?
            } else {
                let mut c = self.conn();
                Self::write_patent_batch_in_tx_with_retry(&mut c, batch, lightweight, model_name)?
            };
            imported += batch_imp;
            chunks_written += batch_chunks;

            // 批间短暂停顿让出 SQLite WAL 写锁，保证前台聊天/提交/设置写入立即获锁不报 SQLITE_BUSY
            if batch_idx + 1 < total_batches
                && !cancel.is_some_and(|flag| flag.load(Ordering::Relaxed))
            {
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
        }

        if !lightweight && chunks_written > 0 {
            if let Some(ref writer) = dedicated_conn {
                let _ = writer.execute("INSERT INTO chunks_fts(chunks_fts) VALUES('optimize')", []);
            } else {
                let c = self.conn();
                let _ = c.execute("INSERT INTO chunks_fts(chunks_fts) VALUES('optimize')", []);
            }
        }
        // 单文件批次完成后主动执行短超时 WAL TRUNCATE 检查点，防止连续导入上百个文件时 `-wal` 长期膨胀到数百 MB
        if imported > 0 {
            if let Some(ref writer) = dedicated_conn {
                let _ = writer.execute_batch(
                    "PRAGMA busy_timeout=3000; PRAGMA wal_checkpoint(TRUNCATE); PRAGMA busy_timeout=30000;",
                );
            }
        }
        Ok((imported, chunks_written, cancelled))
    }

    fn write_patent_batch_in_tx_with_retry(
        c: &mut rusqlite::Connection,
        batch: &[crate::patent::Patent],
        lightweight: bool,
        model_name: &str,
    ) -> Result<(usize, usize), rusqlite::Error> {
        for attempt in 1..=6u32 {
            match Self::write_patent_batch_in_tx(c, batch, lightweight, model_name) {
                Ok(res) => return Ok(res),
                Err(e) if format!("{e}").contains("database is locked") && attempt < 6 => {
                    std::thread::sleep(std::time::Duration::from_millis(250 * u64::from(attempt)));
                }
                Err(e) => return Err(e),
            }
        }
        Self::write_patent_batch_in_tx(c, batch, lightweight, model_name)
    }

    fn write_patent_batch_in_tx(
        c: &mut rusqlite::Connection,
        batch: &[crate::patent::Patent],
        lightweight: bool,
        model_name: &str,
    ) -> Result<(usize, usize), rusqlite::Error> {
        let mut imported = 0usize;
        let mut chunks_written = 0usize;
        let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

        for p in batch {
            let canonical = crate::patent::canonical_patent_key(&p.patent_number);
            let mut final_id = if p.id.trim().is_empty() {
                uuid::Uuid::new_v4().to_string()
            } else {
                p.id.clone()
            };
            let mut matched_existing = false;
            let mut final_patent_number = p.patent_number.clone();
            let mut final_title = p.title.clone();
            let mut final_abstract = p.abstract_text.clone();
            let mut final_description = p.description.clone();
            let mut final_claims = p.claims.clone();
            let mut final_applicant = p.applicant.clone();
            let mut final_inventor = p.inventor.clone();
            let mut final_filing_date = p.filing_date.clone();
            let mut final_pub_date = p.publication_date.clone();
            let mut final_grant_date = p.grant_date.clone();
            let mut final_ipc = p.ipc_codes.clone();
            let mut final_cpc = p.cpc_codes.clone();
            let mut final_priority_date = p.priority_date.clone();
            let mut final_country = p.country.clone();
            let mut final_kind_code = p.kind_code.clone();
            let mut final_family_id = p.family_id.clone();
            let mut final_legal_status = p.legal_status.clone();
            let mut final_citations = p.citations.clone();
            let mut final_cited_by = p.cited_by.clone();
            let mut final_source = p.source.clone();
            let mut final_raw_json = p.raw_json.clone();
            let mut final_images = p.images.clone();
            let mut final_pdf_url = p.pdf_url.clone();

            if !canonical.is_empty() {
                let mut stmt = tx.prepare_cached(
                        "SELECT id, patent_number, title, abstract_text, description, claims,
                                applicant, inventor, filing_date, publication_date, grant_date,
                                ipc_codes, cpc_codes, priority_date, country, kind_code,
                                family_id, legal_status, citations, cited_by, source, raw_json,
                                images, pdf_url
                         FROM patents
                         WHERE REPLACE(REPLACE(REPLACE(REPLACE(UPPER(patent_number), ' ', ''), '-', ''), '.', ''), '/', '') >= ?1
                           AND REPLACE(REPLACE(REPLACE(REPLACE(UPPER(patent_number), ' ', ''), '-', ''), '.', ''), '/', '') < ?2
                         ORDER BY LENGTH(COALESCE(claims, '')) + LENGTH(COALESCE(description, '')) + LENGTH(COALESCE(images, '')) DESC
                         LIMIT 20",
                    )?;
                let canonical_upper = format!("{canonical}\u{10FFFF}");
                let mut rows = stmt.query(rusqlite::params![canonical, canonical_upper])?;
                while let Some(row) = rows.next()? {
                    let existing_id: String = row.get(0)?;
                    let existing_number: String = row.get(1)?;
                    if crate::patent::canonical_patent_key(&existing_number) == canonical {
                        matched_existing = true;
                        final_id = existing_id;
                        if final_patent_number.trim().is_empty() {
                            final_patent_number = existing_number;
                        }
                        let existing_title: String = row.get(2).unwrap_or_default();
                        let existing_abstract: String = row.get(3).unwrap_or_default();
                        let existing_desc: String = row.get(4).unwrap_or_default();
                        let existing_claims: String = row.get(5).unwrap_or_default();
                        let existing_applicant: String = row.get(6).unwrap_or_default();
                        let existing_inventor: String = row.get(7).unwrap_or_default();
                        let existing_filing_date: String = row.get(8).unwrap_or_default();
                        let existing_pub_date: String = row.get(9).unwrap_or_default();
                        let existing_grant_date: Option<String> = row.get(10).ok().flatten();
                        let existing_ipc: String = row.get(11).unwrap_or_default();
                        let existing_cpc: String = row.get(12).unwrap_or_default();
                        let existing_priority_date: String = row.get(13).unwrap_or_default();
                        let existing_country: String = row.get(14).unwrap_or_default();
                        let existing_kind_code: String = row.get(15).unwrap_or_default();
                        let existing_family_id: Option<String> = row.get(16).ok().flatten();
                        let existing_legal_status: String = row.get(17).unwrap_or_default();
                        let existing_citations: String = row.get(18).unwrap_or_default();
                        let existing_cited_by: String = row.get(19).unwrap_or_default();
                        let existing_source: String = row.get(20).unwrap_or_default();
                        let existing_raw_json: String = row.get(21).unwrap_or_default();
                        let existing_images: String = row.get(22).unwrap_or_default();
                        let existing_pdf_url: String = row.get(23).unwrap_or_default();

                        let existing_has_cjk = existing_title
                            .chars()
                            .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch));
                        let new_has_cjk = final_title
                            .chars()
                            .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch));
                        if (existing_has_cjk && !new_has_cjk)
                            || (final_title.trim().is_empty() && !existing_title.trim().is_empty())
                        {
                            final_title = existing_title;
                        }
                        if existing_claims.len() > final_claims.len() {
                            final_claims = existing_claims;
                        }
                        if existing_desc.len() > final_description.len() {
                            final_description = existing_desc;
                        }
                        let existing_abs_has_cjk = existing_abstract
                            .chars()
                            .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch));
                        let new_abs_has_cjk = final_abstract
                            .chars()
                            .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch));
                        if (existing_abs_has_cjk
                            && !new_abs_has_cjk
                            && !existing_abstract.is_empty())
                            || (final_abstract.trim().is_empty()
                                && !existing_abstract.trim().is_empty())
                            || existing_abstract.len() > final_abstract.len()
                        {
                            final_abstract = existing_abstract;
                        }
                        final_applicant =
                            super::patent::pick_richer_str(&existing_applicant, &final_applicant);
                        final_inventor =
                            super::patent::pick_richer_str(&existing_inventor, &final_inventor);
                        final_filing_date = super::patent::pick_richer_str(
                            &existing_filing_date,
                            &final_filing_date,
                        );
                        final_pub_date =
                            super::patent::pick_richer_str(&existing_pub_date, &final_pub_date);
                        if final_grant_date
                            .as_deref()
                            .is_none_or(|s| s.trim().is_empty())
                        {
                            final_grant_date = existing_grant_date;
                        }
                        final_ipc =
                            super::patent::pick_richer_json_array(&existing_ipc, &final_ipc);
                        final_cpc =
                            super::patent::pick_richer_json_array(&existing_cpc, &final_cpc);
                        final_priority_date = super::patent::pick_richer_str(
                            &existing_priority_date,
                            &final_priority_date,
                        );
                        final_country =
                            super::patent::pick_richer_str(&existing_country, &final_country);
                        final_kind_code =
                            super::patent::pick_richer_str(&existing_kind_code, &final_kind_code);
                        if final_family_id.is_none() {
                            final_family_id = existing_family_id;
                        }
                        if final_legal_status.trim().is_empty() {
                            final_legal_status = existing_legal_status;
                        }
                        final_citations = super::patent::pick_richer_json_array(
                            &existing_citations,
                            &final_citations,
                        );
                        final_cited_by = super::patent::pick_richer_json_array(
                            &existing_cited_by,
                            &final_cited_by,
                        );
                        // 若库内已有非轻量富化来源，保留原来源，绝不被 lightweight 标签降级
                        if !existing_source.trim().is_empty()
                            && (!existing_source.ends_with("_lightweight")
                                || final_source.trim().is_empty())
                        {
                            final_source = existing_source;
                        }
                        final_raw_json =
                            super::patent::pick_richer_str(&existing_raw_json, &final_raw_json);
                        final_images =
                            super::patent::pick_richer_json_array(&existing_images, &final_images);
                        final_pdf_url =
                            super::patent::pick_richer_str(&existing_pdf_url, &final_pdf_url);
                        break;
                    }
                }
            }

            if matched_existing {
                let mut del_fts = tx.prepare_cached(
                        "DELETE FROM patents_fts WHERE rowid = (SELECT rowid FROM patents WHERE id = ?1)",
                    )?;
                del_fts.execute(rusqlite::params![final_id])?;
            }
            {
                let mut upsert_pat = tx.prepare_cached(
                        "INSERT OR REPLACE INTO patents (id,patent_number,title,abstract_text,description,claims,applicant,inventor,filing_date,publication_date,grant_date,ipc_codes,cpc_codes,priority_date,country,kind_code,family_id,legal_status,citations,cited_by,source,raw_json,images,pdf_url) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24)",
                    )?;
                upsert_pat.execute(rusqlite::params![
                    final_id,
                    final_patent_number,
                    final_title,
                    final_abstract,
                    final_description,
                    final_claims,
                    final_applicant,
                    final_inventor,
                    final_filing_date,
                    final_pub_date,
                    final_grant_date,
                    final_ipc,
                    final_cpc,
                    final_priority_date,
                    final_country,
                    final_kind_code,
                    final_family_id,
                    final_legal_status,
                    final_citations,
                    final_cited_by,
                    final_source,
                    final_raw_json,
                    final_images,
                    final_pdf_url
                ])?;
            }
            {
                let mut ins_fts = tx.prepare_cached(
                        "INSERT INTO patents_fts(rowid, patent_number, title, abstract_text, claims, applicant, inventor, ipc_codes) SELECT rowid, patent_number, title, abstract_text, claims, applicant, inventor, ipc_codes FROM patents WHERE id = ?1",
                    )?;
                ins_fts.execute(rusqlite::params![final_id])?;
            }

            if !lightweight {
                let chunks = crate::rag::build_chunks_from_patent(
                    &final_id,
                    &final_title,
                    &final_abstract,
                    &final_claims,
                    &final_description,
                );
                chunks_written += Self::write_chunks_in_tx(&tx, &final_id, &chunks, model_name)?;

                // 同步写入 v2 语义向量，确保非轻量批量导入后的专利立即可被「语义向量」模式检索
                let emb_text = format!("{final_title} {final_abstract} {final_claims}");
                if !emb_text.trim().is_empty() {
                    let emb = crate::db::vector::compute_tfidf_embedding(&emb_text);
                    let emb_bytes: Vec<u8> = emb.iter().flat_map(|f| f.to_le_bytes()).collect();
                    let emb_hash = emb
                        .iter()
                        .fold(0u64, |acc, f| {
                            acc.wrapping_add(f.to_bits() as u64) ^ 0x9E3779B9u64
                        })
                        .to_string();
                    let mut ins_emb = tx.prepare_cached(
                            "INSERT OR REPLACE INTO patents_embedding (patent_id, embedding, model_name, text_hash, updated_at) VALUES (?1, ?2, 'char-tfidf-v2', ?3, datetime('now'))",
                        )?;
                    ins_emb.execute(rusqlite::params![
                        final_id,
                        rusqlite::types::Value::Blob(emb_bytes),
                        emb_hash
                    ])?;
                }
            }
            imported += 1;
        }

        tx.commit()?;
        Ok((imported, chunks_written))
    }

    /// Search chunks for a patent by query text with cosine similarity.
    pub fn search_chunks(
        &self,
        patent_id: &str,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<(String, f32)>, rusqlite::Error> {
        let c = self.conn();
        let mut stmt =
            c.prepare("SELECT id, content, embedding FROM patent_chunks WHERE patent_id = ?1")?;
        let rows = stmt.query_map(rusqlite::params![patent_id], |row: &rusqlite::Row| {
            let id: String = row.get(0)?;
            let content: String = row.get(1)?;
            let blob_val: rusqlite::types::Value = row.get(2)?;
            match blob_val {
                rusqlite::types::Value::Blob(bytes) => {
                    let mut emb = Vec::with_capacity(bytes.len() / 4);
                    for i in (0..bytes.len()).step_by(4) {
                        if i + 3 < bytes.len() {
                            let b = [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]];
                            emb.push(f32::from_le_bytes(b));
                        }
                    }
                    Ok((id, content, emb))
                }
                _ => Ok((id, content, vec![])),
            }
        })?;

        let mut scores: Vec<(String, f32)> = Vec::new();
        for (id, _content, emb) in rows.flatten() {
            let sim = cosine_similarity(query, &emb);
            if sim > 0.05 {
                scores.push((id, sim));
            }
        }

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(limit);
        Ok(scores)
    }

    /// Get chunk by ID.
    pub fn get_chunk(&self, chunk_id: &str) -> Result<Option<PatentChunk>, rusqlite::Error> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT patent_id, chunk_index, source_type, section_tag, content, embedding FROM patent_chunks WHERE id = ?1"
        )?;
        let rows = stmt.query_map(rusqlite::params![chunk_id], |row: &rusqlite::Row| {
            let patent_id: String = row.get(0)?;
            let chunk_index: i32 = row.get(1)?;
            let source_type: String = row.get(2)?;
            let section_tag: String = row.get(3)?;
            let content: String = row.get(4)?;
            let blob_val: rusqlite::types::Value = row.get(5)?;
            let embedding = match blob_val {
                rusqlite::types::Value::Blob(bytes) => {
                    let mut emb = Vec::with_capacity(bytes.len() / 4);
                    for i in (0..bytes.len()).step_by(4) {
                        if i + 3 < bytes.len() {
                            let b = [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]];
                            emb.push(f32::from_le_bytes(b));
                        }
                    }
                    emb
                }
                _ => vec![],
            };
            Ok(PatentChunk {
                id: chunk_id.to_string(),
                patent_id,
                chunk_index,
                source_type,
                section_tag,
                content,
                embedding,
            })
        })?;

        if let Some(chunk) = rows.flatten().next() {
            return Ok(Some(chunk));
        }
        Ok(None)
    }

    /// Count chunks for a patent.
    pub fn count_chunks(&self, patent_id: &str) -> Result<i64, rusqlite::Error> {
        let c = self.conn();
        let count: i64 = c.query_row(
            "SELECT COUNT(*) FROM patent_chunks WHERE patent_id = ?1",
            rusqlite::params![patent_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// BM25 词汇通道检索（Batch 1 任务 1.3）。
    ///
    /// 走 `chunks_fts`（FTS5 unicode61 + CJK bigram 索引正文，见 [`fts_bigram_text`]），
    /// `bm25(chunks_fts)` 取负值越小越好，这里翻转为正分（越大越好）以与
    /// [`Database::search_chunks`] 的「score 越大越相关」同一语义。
    /// 查询表达式由 [`fts_query_expr`] 生成；空查询/零命中返回空列表（不报错，
    /// 上层 RRF 融合按「词汇通道无贡献」处理）。
    pub fn search_chunks_lexical(
        &self,
        patent_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(String, f32)>, rusqlite::Error> {
        let expr = fts_query_expr(query);
        if expr.is_empty() || limit == 0 {
            return Ok(vec![]);
        }
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT chunk_id, bm25(chunks_fts) AS bm25_score
             FROM chunks_fts
             WHERE chunks_fts MATCH ?1 AND patent_id = ?2
             ORDER BY bm25_score
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![expr, patent_id, limit as i64],
            |row: &rusqlite::Row| {
                let chunk_id: String = row.get(0)?;
                let score: f64 = row.get(1)?;
                Ok((chunk_id, -(score as f32)))
            },
        )?;
        Ok(rows.flatten().collect())
    }

    /// 跨专利说明书实施例分块 BM25 检索（走 `chunks_fts` + 回查 `patent_chunks.content`）。
    ///
    /// 用于权利要求树第 26 条第 4 款核对：从说明书及实施例分块中检索与权利要求参数/功能特征匹配的原文。
    pub fn search_embodiment_chunks_lexical(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<EmbodimentChunkHit>, rusqlite::Error> {
        let expr = fts_query_expr(query);
        if expr.is_empty() || limit == 0 {
            return Ok(vec![]);
        }
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT pc.id, pc.patent_id, COALESCE(pc.section_tag, ''), pc.content, bm25(chunks_fts) AS bm25_score
             FROM chunks_fts
             JOIN patent_chunks pc ON pc.id = chunks_fts.chunk_id
             WHERE chunks_fts MATCH ?1
             ORDER BY bm25_score
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![expr, limit as i64],
            |row: &rusqlite::Row| {
                let chunk_id: String = row.get(0)?;
                let patent_id: String = row.get(1)?;
                let section_tag: String = row.get(2)?;
                let content: String = row.get(3)?;
                let score: f64 = row.get(4)?;
                Ok(EmbodimentChunkHit {
                    chunk_id,
                    patent_id,
                    section_tag,
                    content,
                    score: -(score as f32),
                })
            },
        )?;
        Ok(rows.flatten().collect())
    }

    /// BM25 + 向量 RRF 融合检索（Batch 1 任务 1.3）。
    ///
    /// 两个通道各取 `limit * 2` 条候选，按 [`rrf_fuse`]（`1/(k+rank)`，k=[`RRF_K`]=60）
    /// 融合后取前 `limit` 条。词汇通道失败（FTS 缺行/查询异常）按空列表降级——
    /// 语义为「该通道本次无贡献」，向量通道仍独立可用；两通道都失败才返回空。
    pub fn search_chunks_hybrid_rrf(
        &self,
        patent_id: &str,
        query_text: &str,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(String, f32)>, rusqlite::Error> {
        if limit == 0 {
            return Ok(vec![]);
        }
        let deep = limit * 2;
        let vec_rank = self.search_chunks(patent_id, query_embedding, deep)?;
        let lex_rank = self
            .search_chunks_lexical(patent_id, query_text, deep)
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "lexical chunk channel failed, degrading to vector-only: {}",
                    e
                );
                vec![]
            });
        let vec_ids: Vec<String> = vec_rank.into_iter().map(|(id, _)| id).collect();
        let lex_ids: Vec<String> = lex_rank.into_iter().map(|(id, _)| id).collect();
        Ok(rrf_fuse(&vec_ids, &lex_ids, RRF_K, limit))
    }
}

/// Reciprocal Rank Fusion 纯函数（Batch 1 任务 1.3 的融合单一出处）。
///
/// 输入两个通道各自的 id 排名列表（越靠前越相关），输出融合分排序的前 `limit` 条：
/// `score(id) = Σ 1/(k + rank_i)`，rank 从 1 起（与 `/api/search/vector` 路由内联的
/// 专利表级融合同一公式与约定；chunks 级以本函数为出处，不复制第二份公式）。
/// 排序稳定：同分时保持向量通道优先（先到先得），保证测试确定性。
pub(crate) fn rrf_fuse(
    vec_rank: &[String],
    lex_rank: &[String],
    k: f32,
    limit: usize,
) -> Vec<(String, f32)> {
    use std::collections::HashMap;
    let mut order: Vec<String> = Vec::new();
    let mut scores: HashMap<String, f32> = HashMap::new();
    for (rank, id) in vec_rank.iter().enumerate() {
        if !scores.contains_key(id) {
            order.push(id.clone());
        }
        *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
    }
    for (rank, id) in lex_rank.iter().enumerate() {
        if !scores.contains_key(id) {
            order.push(id.clone());
        }
        *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
    }
    let mut fused: Vec<(String, f32)> = order
        .into_iter()
        .map(|id| {
            let s = scores.get(&id).copied().unwrap_or(0.0);
            (id, s)
        })
        .collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    fused.truncate(limit);
    fused
}

/// FTS5 索引侧正文变换：CJK 连续段做 bigram 空格切分（Batch 1 任务 1.3）。
///
/// unicode61 不切 CJK——「固态电池与充电方法」会整句成单 token，查询单字/bigram
/// 都匹配不上，BM25 通道对中文形同虚设。本变换把每个 CJK 连续段切成滑窗 bigram
/// （「固态电池」→「固态 态电 电池」；单字独存），拉丁词原样保留，标点归空格。
/// 仅用于 chunks_fts（派生索引）；权威正文 patent_chunks.content 不动
/// （AGENTS.md 2.5 数据完整性）。
pub(crate) fn fts_bigram_text(s: &str) -> String {
    let mut out = String::new();
    let mut latin = String::new();
    let mut cjk_run: Vec<char> = Vec::new();
    let mut have_token = false; // out 里是否已有 token（决定 token 之间是否补空格）
                                // 把累积的 Latin 词整词冲进 out。查询侧 [`fts_query_expr`] 同样把连续拉丁字母
                                // 累积成整词——两侧切分必须同源，逐字符拆会让英文被切成单字母 token（#78）。
    let flush_latin = |out: &mut String, latin: &mut String, have_token: &mut bool| {
        if latin.is_empty() {
            return;
        }
        if *have_token {
            out.push(' ');
        }
        out.push_str(latin);
        *have_token = true;
        latin.clear();
    };
    // 把累积的 CJK run 以滑窗 bigram 形式冲进 out（单字独存）。
    let flush_cjk_run = |out: &mut String, run: &mut Vec<char>, have_token: &mut bool| {
        if run.is_empty() {
            return;
        }
        if *have_token {
            out.push(' ');
        }
        if run.len() == 1 {
            out.push(run[0]);
        } else {
            for i in 0..run.len() - 1 {
                if i > 0 {
                    out.push(' '); // 首个 bigram 前的空格已在上方补过
                }
                out.push(run[i]);
                out.push(run[i + 1]);
            }
        }
        *have_token = true;
        run.clear();
    };
    for c in s.chars() {
        if is_cjk_char(c) {
            flush_latin(&mut out, &mut latin, &mut have_token);
            cjk_run.push(c);
        } else if c.is_alphanumeric() {
            flush_cjk_run(&mut out, &mut cjk_run, &mut have_token);
            latin.push(c);
        } else {
            // 标点/空白：切词边界，冲出累积但不产生 token
            flush_latin(&mut out, &mut latin, &mut have_token);
            flush_cjk_run(&mut out, &mut cjk_run, &mut have_token);
        }
    }
    flush_latin(&mut out, &mut latin, &mut have_token);
    flush_cjk_run(&mut out, &mut cjk_run, &mut have_token);
    out
}

/// FTS5 查询侧表达式生成：与 [`fts_bigram_text`] 同一切分规则（AGENTS.md 2.2）。
///
/// CJK 段 → 滑窗 bigram（单字段 → 单字），拉丁词 → 原词（小写化）；每 token 加引号
/// （防 FTS5 语法字符注入），`OR` 连接（召回优先，BM25 对多命中自然加权）。
/// 超过 [`FTS_MAX_TOKENS`] 截断；无 token 返回空串（调用方跳过词汇通道）。
pub(crate) fn fts_query_expr(q: &str) -> String {
    let mut tokens: Vec<String> = Vec::new();
    let mut latin: String = String::new();
    let mut cjk_run: Vec<char> = Vec::new();
    let flush_latin = |tokens: &mut Vec<String>, latin: &mut String| {
        if !latin.is_empty() {
            tokens.push(latin.to_lowercase());
            latin.clear();
        }
    };
    let flush_cjk = |tokens: &mut Vec<String>, run: &mut Vec<char>| {
        if run.is_empty() {
            return;
        }
        if run.len() == 1 {
            tokens.push(run.iter().collect::<String>());
        } else {
            for i in 0..run.len() - 1 {
                let bigram: String = run[i..i + 2].iter().collect();
                tokens.push(bigram);
            }
        }
        run.clear();
    };
    for c in q.chars() {
        if is_cjk_char(c) {
            flush_latin(&mut tokens, &mut latin);
            cjk_run.push(c);
        } else if c.is_ascii_alphanumeric() {
            flush_cjk(&mut tokens, &mut cjk_run);
            latin.push(c);
        } else {
            flush_latin(&mut tokens, &mut latin);
            flush_cjk(&mut tokens, &mut cjk_run);
        }
    }
    flush_latin(&mut tokens, &mut latin);
    flush_cjk(&mut tokens, &mut cjk_run);

    tokens.truncate(FTS_MAX_TOKENS);
    tokens
        .into_iter()
        .map(|t| format!("\"{}\"", t.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// 是否为 CJK 表意/假名/谚文字符（索引与查询两侧切分的字符域判据，单一出处）。
fn is_cjk_char(c: char) -> bool {
    matches!(c,
        '\u{3400}'..='\u{4DBF}'   // CJK 扩展 A
        | '\u{4E00}'..='\u{9FFF}' // CJK 基本区
        | '\u{F900}'..='\u{FAFF}' // CJK 兼容
        | '\u{3040}'..='\u{30FF}' // 平假名/片假名
        | '\u{AC00}'..='\u{D7AF}' // 谚文
    )
}

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }
    let dot: f32 = (0..len).map(|i| a[i] * b[i]).sum();
    let na: f32 = (0..len).map(|i| a[i] * a[i]).sum::<f32>().sqrt();
    let nb: f32 = (0..len).map(|i| b[i] * b[i]).sum::<f32>().sqrt();
    if na < 1e-8 || nb < 1e-8 {
        return 0.0;
    }
    (dot / (na * nb)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rag::chunker::compute_chunk_embedding;

    // ── rrf_fuse 纯函数 ─────────────────────────────────────────────────────────

    #[test]
    fn rrf_fuse_merges_and_ranks_both_channels() {
        let v = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let l = vec!["b".to_string(), "d".to_string()];
        let fused = rrf_fuse(&v, &l, 60.0, 10);
        let score_of = |id: &str| {
            fused
                .iter()
                .find(|(i, _)| i == id)
                .map(|(_, s)| *s)
                .expect("id must survive fusion")
        };
        // b 双通道命中 → 最高
        assert_eq!("b", fused[0].0, "双通道命中的应排第一: {fused:?}");
        assert!((score_of("b") - (1.0 / 62.0 + 1.0 / 61.0)).abs() < 1e-6);
        // a 只有向量 rank1
        assert!((score_of("a") - 1.0 / 61.0).abs() < 1e-6);
        // d 只有词汇 rank2
        assert!((score_of("d") - 1.0 / 62.0).abs() < 1e-6);
        // c 只有向量 rank3
        assert!((score_of("c") - 1.0 / 63.0).abs() < 1e-6);
    }

    #[test]
    fn rrf_fuse_respects_limit_and_stable_order() {
        let v: Vec<String> = (0..6).map(|i| format!("v{i}")).collect();
        let l: Vec<String> = (0..6).map(|i| format!("l{i}")).collect();
        let fused = rrf_fuse(&v, &l, 60.0, 4);
        assert_eq!(4, fused.len(), "limit 截断");
        assert_eq!("v0", fused[0].0, "同分时向量通道先到先得（确定性）");
        assert_eq!("l0", fused[1].0);
        // 分数单调不增
        for w in fused.windows(2) {
            assert!(w[0].1 >= w[1].1, "{fused:?}");
        }
    }

    // ── CJK bigram 切分（索引/查询两侧同一出处） ────────────────────────────────

    #[test]
    fn fts_bigram_text_splits_cjk_runs_and_keeps_latin() {
        assert_eq!("固态 态电 电池", fts_bigram_text("固态电池"));
        assert_eq!("电", fts_bigram_text("电"));
        assert_eq!("lithium battery", fts_bigram_text("lithium battery"));
        assert_eq!(
            "solid 固态 态电 电池 池技 技术",
            fts_bigram_text("solid固态电池技术")
        );
        assert_eq!("", fts_bigram_text("   "));
        // 标点归空格，不产生噪声 token
        assert_eq!("电池 充电", fts_bigram_text("电池、充电"));
    }

    #[test]
    fn fts_query_expr_quotes_tokens_and_uses_or() {
        let expr = fts_query_expr("固态电池");
        assert_eq!("\"固态\" OR \"态电\" OR \"电池\"", expr);
        // 空白/纯标点 → 空表达式（调用方跳过词汇通道）
        assert_eq!("", fts_query_expr("   "));
        assert_eq!("", fts_query_expr("。，！"));
        // 拉丁词小写化
        assert_eq!("\"battery\"", fts_query_expr("Battery"));
        // FTS 语法字符被引号包裹：token 内的引号被剥离，且每个 token 完整加引号
        let expr = fts_query_expr("a\"b");
        assert_eq!(
            "\"a\" OR \"b\"", expr,
            "token 内引号应被剥离，拆成 a/b 两个 token: {expr}"
        );
        assert!(!expr.contains("\"\""), "不得出现空 token 的双引号: {expr}");
    }

    // ── 端到端（内存库）：save → lexical → hybrid，section_tag 透传 ─────────────

    fn setup_db() -> (Database, String) {
        let db = Database::init(":memory:").expect("in-memory db");
        let patent = crate::types::patent::Patent {
            id: String::new(),
            patent_number: "CN000000099A".to_string(),
            title: "一种快离子导体材料".to_string(),
            abstract_text: "测试摘要".to_string(),
            description: String::new(),
            claims: String::new(),
            applicant: String::new(),
            inventor: String::new(),
            filing_date: String::new(),
            publication_date: String::new(),
            grant_date: None,
            ipc_codes: String::new(),
            cpc_codes: String::new(),
            priority_date: String::new(),
            country: "CN".to_string(),
            kind_code: "A".to_string(),
            family_id: None,
            legal_status: String::new(),
            citations: "[]".to_string(),
            cited_by: "[]".to_string(),
            source: String::new(),
            raw_json: String::new(),
            created_at: String::new(),
            images: "[]".to_string(),
            pdf_url: String::new(),
        };
        let patent_id = db.insert_patent(&patent).expect("insert patent");
        (db, patent_id)
    }

    fn make_chunk(patent_id: &str, idx: i32, section_tag: &str, content: &str) -> PatentChunk {
        PatentChunk {
            id: format!("{patent_id}-c{idx}"),
            patent_id: patent_id.to_string(),
            chunk_index: idx,
            source_type: "description".to_string(),
            section_tag: section_tag.to_string(),
            content: content.to_string(),
            embedding: compute_chunk_embedding(content),
        }
    }

    #[test]
    fn save_and_lexical_search_roundtrip_with_section_tag() {
        let (db, patent_id) = setup_db();
        let chunks = vec![
            make_chunk(
                &patent_id,
                0,
                "Desc:背景技术",
                "背景技术常规介绍，现有方案存在效率不足的问题。",
            ),
            make_chunk(
                &patent_id,
                1,
                "Claim:1",
                "一种固态电池，其特征在于包括钪碲化合物快离子导体层。",
            ),
        ];
        let n = db
            .save_patent_chunks(&patent_id, &chunks, "char-tfidf-v1")
            .expect("save");
        assert_eq!(2, n);

        // get_chunk 透传 section_tag
        let got = db
            .get_chunk(&format!("{patent_id}-c1"))
            .expect("get")
            .expect("exists");
        assert_eq!("Claim:1", got.section_tag);

        // 词汇通道：BM25 命中含「钪碲」的 chunk
        let lex = db
            .search_chunks_lexical(&patent_id, "钪碲化合物", 5)
            .expect("lexical");
        assert_eq!(1, lex.len(), "只有 c1 含查询 bigram: {lex:?}");
        assert_eq!(format!("{patent_id}-c1"), lex[0].0);
        assert!(lex[0].1 > 0.0, "翻转后的 BM25 分应为正: {:?}", lex[0].1);

        // 重复保存替换 FTS 行（delete-first），不残留旧索引。
        // 新内容刻意避开旧 bigram（原「不含钪碲」自身含该 bigram 会假红）。
        let updated = vec![make_chunk(
            &patent_id,
            0,
            "Desc:Window",
            "全新的内容描述，不含任何旧关键词。",
        )];
        db.save_patent_chunks(&patent_id, &updated, "char-tfidf-v1")
            .expect("resave");
        let lex2 = db
            .search_chunks_lexical(&patent_id, "钪碲", 5)
            .expect("lexical after resave");
        assert!(lex2.is_empty(), "旧索引必须随重存清空: {lex2:?}");
    }

    #[test]
    fn lexical_channel_degrades_on_unmatched_query() {
        let (db, patent_id) = setup_db();
        let chunks = vec![make_chunk(&patent_id, 0, "Abstract", "普通文本内容")];
        db.save_patent_chunks(&patent_id, &chunks, "char-tfidf-v1")
            .expect("save");
        let lex = db
            .search_chunks_lexical(&patent_id, "量子纠缠", 5)
            .expect("lexical");
        assert!(lex.is_empty(), "零命中返回空列表而非报错");
        let lex = db
            .search_chunks_lexical(&patent_id, "   ", 5)
            .expect("lexical");
        assert!(lex.is_empty(), "空查询返回空列表");
    }

    /// **RRF 召回率 >= 纯向量检索**（虚拟合成专利夹具，Batch 1 任务 1.3 验收锚点）。
    ///
    /// 夹具模拟向量通道出现高相似误报（如历史旧版 `char-tfidf-v1` 伪嵌入或近似向量撞车）：
    /// - `decoy`：文本与查询词元无关，但向量的余弦分布与查询高度撞车，纯向量 top-1；
    /// - `target`：含查询的稀有 bigram（钪碲化合物）但被大量重复填充词稀释，
    ///   纯向量排位靠后；
    /// - `f1`：另一块普通填充。
    ///
    /// 断言：① 纯向量 top-1 = decoy（向量通道抓不到目标）；② RRF 融合 top-1 = target
    /// （BM25 词汇通道把目标捞回）；③ RRF top-k ⊇ 纯向量 top-k（本夹具上召回率
    /// 严格优于纯向量）。
    #[test]
    fn hybrid_rrf_recall_beats_vector_only_on_synthetic_patent() {
        let (db, patent_id) = setup_db();
        let query_text = "钪碲化合物快离子导体材料制备方法";
        let query_emb = compute_chunk_embedding(query_text);

        let filler = "本实施例采用常规工艺进行制备，搅拌均匀后静置，随后进行干燥处理，\
                      重复上述步骤若干次，直至得到均匀稳定的最终产物，产物的形貌与预期一致。";
        let mut decoy = make_chunk(
            &patent_id,
            0,
            "Desc:背景技术",
            "锂枝晶抑制与界面阻抗调控研究进展综述。",
        );
        // 模拟向量通道撞车误报（与查询向量方向一致但文本无词元重叠）
        decoy.embedding = query_emb.clone();
        let chunks = vec![
            decoy,
            make_chunk(
                &patent_id,
                1,
                "Desc:[0042]",
                &format!("钪碲化合物作为快离子导体的主相。{filler}"),
            ),
            make_chunk(&patent_id, 2, "Desc:Window", filler),
        ];
        db.save_patent_chunks(&patent_id, &chunks, "char-tfidf-v1")
            .expect("save");

        // ① 纯向量 top-1 = decoy（c0）：向量撞车，而目标被稀释
        let vec_only = db
            .search_chunks(&patent_id, &query_emb, 3)
            .expect("vector search");
        assert!(!vec_only.is_empty(), "向量通道应有结果");
        let decoy_id = format!("{patent_id}-c0");
        assert_eq!(
            decoy_id, vec_only[0].0,
            "夹具前提：纯向量 top-1 应是权重分布撞车的 decoy: {vec_only:?}"
        );
        let vec_top: Vec<String> = vec_only.iter().map(|(id, _)| id.clone()).collect();

        // ② RRF 融合 top-1 = target（c1）
        let hybrid = db
            .search_chunks_hybrid_rrf(&patent_id, query_text, &query_emb, 3)
            .expect("hybrid search");
        let target_id = format!("{patent_id}-c1");
        assert_eq!(
            target_id, hybrid[0].0,
            "RRF 融合必须把词汇通道命中的目标捞回第一: {hybrid:?}"
        );

        // ③ RRF top-k ⊇ 纯向量 top-k（本夹具召回率 >= 纯向量）
        let hybrid_top: Vec<String> = hybrid.iter().map(|(id, _)| id.clone()).collect();
        for id in &vec_top {
            assert!(
                hybrid_top.contains(id),
                "纯向量命中的 {id} 被 RRF 融合挤掉了，召回率回退: vec={vec_top:?} hybrid={hybrid_top:?}"
            );
        }
    }

    #[test]
    fn batch_import_patents_with_chunks_is_idempotent_and_fts_searchable() {
        let db = Database::init(":memory:").expect("open in-memory db");
        let p1 = crate::patent::Patent {
            id: "".into(),
            patent_number: "CN119900001A".into(),
            title: "固态电解质陶瓷隔膜制备方法".into(),
            abstract_text: "本发明公开一种石榴石型固态电解质隔膜烧结工艺。".into(),
            claims: "1. 一种固态电解质隔膜，其特征在于采用等离子烧结。".into(),
            description: "[0001] 本发明属于固态电池技术领域。".into(),
            ..Default::default()
        };
        let p2 = crate::patent::Patent {
            id: "".into(),
            patent_number: "CN119900002A".into(),
            title: "微型隔离电磁阀".into(),
            abstract_text: "一种带有双层波纹管隔离的微型电磁阀。".into(),
            claims: "1. 一种电磁阀，包括阀芯与波纹管隔离件。".into(),
            description: "[0001] 防止介质污染线圈。".into(),
            ..Default::default()
        };

        let (imp1, chk1) = db
            .batch_import_patents_with_chunks(&[p1.clone(), p2.clone()], "tfidf-local")
            .expect("first batch import");
        assert_eq!(imp1, 2);
        assert!(chk1 >= 4);

        // 重复导入同一公开号必须幂等，不产生重复 chunk
        let (imp2, chk2) = db
            .batch_import_patents_with_chunks(&[p1, p2], "tfidf-local")
            .expect("second batch import");
        assert_eq!(imp2, 2);
        assert_eq!(chk1, chk2);

        let stored = db
            .get_patent("CN119900001A")
            .expect("get_patent")
            .expect("exists");
        assert_eq!(db.count_chunks(&stored.id).expect("count") as usize, 3);

        let hits = db
            .search_chunks_lexical(&stored.id, "等离子烧结", 5)
            .expect("lexical search");
        assert!(
            !hits.is_empty(),
            "批量导入后 chunks_fts 应立即支持 BM25 检索"
        );
    }

    /// #78 多术语 `CN990001001A` 合成专利召回对比基准：量化对比纯向量 vs RRF 混合召回前/后指标。
    #[test]
    fn hybrid_rrf_terminology_recall_benchmark_cn990001001a() {
        let db = Database::init(":memory:").expect("open in-memory db");
        let mut patent = crate::types::patent::Patent {
            id: String::new(),
            patent_number: "CN990001001A".to_string(),
            title: "复合固态电解质隔膜及电化学装置".to_string(),
            abstract_text: "涉及石榴石基陶瓷涂覆隔膜及其烧结与热稳定性控制。".to_string(),
            description: String::new(),
            claims: String::new(),
            applicant: String::new(),
            inventor: String::new(),
            filing_date: String::new(),
            publication_date: String::new(),
            grant_date: None,
            ipc_codes: String::new(),
            cpc_codes: String::new(),
            priority_date: String::new(),
            country: "CN".to_string(),
            kind_code: "A".to_string(),
            family_id: None,
            legal_status: String::new(),
            citations: "[]".to_string(),
            cited_by: "[]".to_string(),
            source: String::new(),
            raw_json: String::new(),
            created_at: String::new(),
            images: "[]".to_string(),
            pdf_url: String::new(),
        };
        let patent_id = db.insert_patent(&patent).expect("insert CN990001001A");
        patent.id = patent_id.clone();

        let filler = "本实施方式采用通用反应釜进行常规混合与干燥操作，按照预设时间间隔采样观察表面形貌与宏观均匀性，相关工艺步骤为本领域常规设置。";
        let chunks = vec![
            make_chunk(
                &patent_id,
                0,
                "Desc:[0001]",
                "电化学储能体系界面稳定性与隔膜机械强度综述研究。",
            ),
            make_chunk(
                &patent_id,
                1,
                "Claim:1",
                &format!("一种复合固态电解质隔膜，包含镧锆氧石榴石主相与柔性聚合物骨架。{filler}"),
            ),
            make_chunk(
                &patent_id,
                2,
                "Desc:[0018]",
                &format!("采用磁控溅射沉积非晶氧化铌阻挡层并配合超声雾化喷涂工艺。{filler}"),
            ),
            make_chunk(
                &patent_id,
                3,
                "Desc:[0024]",
                &format!("在氩气氛围下进行放电等离子烧结致密化成型，升温速率保持恒定。{filler}"),
            ),
            make_chunk(
                &patent_id,
                4,
                "Desc:[0042]",
                &format!("差示扫描量热测试表明材料具备高玻璃化转变阈值且抑制枝晶穿刺。{filler}"),
            ),
        ];
        db.save_patent_chunks(&patent_id, &chunks, "char-tfidf-v1")
            .expect("save benchmark chunks");

        let cases = [
            ("镧锆氧石榴石主相", format!("{patent_id}-c1")),
            ("超声雾化喷涂工艺", format!("{patent_id}-c2")),
            ("放电等离子烧结致密化", format!("{patent_id}-c3")),
        ];

        let mut vec_r1 = 0usize;
        let mut hyb_r1 = 0usize;
        let mut vec_r3 = 0usize;
        let mut hyb_r3 = 0usize;
        for (query, expected_id) in &cases {
            let emb = compute_chunk_embedding(query);
            let vec_hits = db.search_chunks(&patent_id, &emb, 3).expect("vec search");
            let hyb_hits = db
                .search_chunks_hybrid_rrf(&patent_id, query, &emb, 3)
                .expect("hybrid rrf search");
            if vec_hits.first().map(|x| &x.0) == Some(expected_id) {
                vec_r1 += 1;
            }
            if hyb_hits.first().map(|x| &x.0) == Some(expected_id) {
                hyb_r1 += 1;
            }
            if vec_hits.iter().any(|x| &x.0 == expected_id) {
                vec_r3 += 1;
            }
            if hyb_hits.iter().any(|x| &x.0 == expected_id) {
                hyb_r3 += 1;
            }
        }

        let n = cases.len();
        eprintln!(
            "CN990001001A recall@1: vector={vec_r1}/{n}, hybrid={hyb_r1}/{n}; recall@3: vector={vec_r3}/{n}, hybrid={hyb_r3}/{n}"
        );
        assert!(hyb_r1 >= vec_r1, "hybrid recall@1 must be >= vector-only");
        assert!(hyb_r3 >= vec_r3, "hybrid recall@3 must be >= vector-only");
        assert_eq!(
            hyb_r1, n,
            "hybrid RRF 应将 3 个专业术语查询全部稳定召回至 Rank-1"
        );
    }

    #[test]
    fn batch_import_patents_chunked_uses_dedicated_wal_writer_without_blocking_main_conn() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("wal_concurrent.db");
        let db = std::sync::Arc::new(
            Database::init(db_path.to_str().expect("utf8")).expect("init file db"),
        );

        let patents: Vec<crate::patent::Patent> = (0..150)
            .map(|i| crate::patent::Patent {
                id: format!("wal-bulk-{i}"),
                patent_number: format!("CN202699{:05}A", i),
                title: format!("并发 WAL 批量导入测试专利 {i}"),
                abstract_text: "测试后台独立 WAL 写连接不阻塞主连接前台查询".to_string(),
                description: String::new(),
                claims: String::new(),
                applicant: "测试机构".to_string(),
                inventor: "测试发明人".to_string(),
                filing_date: "2026-01-01".to_string(),
                publication_date: "2026-06-01".to_string(),
                grant_date: None,
                ipc_codes: r#"["F23D1/00"]"#.to_string(),
                cpc_codes: "[]".to_string(),
                priority_date: String::new(),
                country: "CN".to_string(),
                kind_code: "A".to_string(),
                family_id: None,
                legal_status: "公开".to_string(),
                citations: "[]".to_string(),
                cited_by: "[]".to_string(),
                source: "folder_sync_lightweight".to_string(),
                raw_json: String::new(),
                created_at: String::new(),
                images: "[]".to_string(),
                pdf_url: String::new(),
            })
            .collect();

        // 刻意持住主连接 Mutex 模拟前台查询；若批量导入仍争抢 self.conn 将直接死锁
        let main_guard = db.conn();
        let db_bg = db.clone();
        let handle = std::thread::spawn(move || {
            db_bg.batch_import_patents_chunked(&patents, true, "char-tfidf-v2", None)
        });
        let (imported, chunks, cancelled) = handle
            .join()
            .expect("worker thread join")
            .expect("dedicated wal bulk import succeeds while main conn is held");
        assert_eq!(imported, 150);
        assert_eq!(chunks, 0);
        assert!(!cancelled);

        // 主连接立即可见已提交的全部 150 条专利
        let count: i64 = main_guard
            .query_row("SELECT COUNT(*) FROM patents", [], |r| r.get(0))
            .expect("count patents");
        assert_eq!(count, 150);

        // 验证 journal_size_limit=32MB 生效且批次结束后 WAL 已被 TRUNCATE 截断回收
        let limit: i64 = main_guard
            .query_row("PRAGMA journal_size_limit", [], |r| r.get(0))
            .expect("journal_size_limit");
        assert_eq!(limit, 33_554_432);
        let wal_len = std::fs::metadata(format!("{}-wal", db_path.display()))
            .map(|m| m.len())
            .unwrap_or(0);
        assert!(
            wal_len <= 65_536,
            "bulk import 结束后 WAL 应被截断而非持续膨胀，实际 wal_len={wal_len}"
        );
    }
}
