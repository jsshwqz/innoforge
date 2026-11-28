//! 模块职责：专利主表（`patents`）增删改查、规范专利号入库去重合并、全文检索（`patents_fts`）与统计分析。
//! 上游调用者：`src/routes/patent.rs`、`src/routes/search.rs`、`src/search/enrichment.rs`。
//! 模块专属禁区：严禁在 `insert_patent` 中使用带 `LIMIT 200` 的前 2 字符宽泛前缀匹配导致大库漏去重；严禁让浅层搜索结果覆盖已有全文/附图/IPC/法律状态字段。

use super::relevance::{calculate_field_relevance, calculate_mixed_relevance};
use crate::patent::{Patent, PatentSummary, SearchType};
use anyhow::Result;
use rusqlite::{params, OptionalExtension};

pub(super) fn pick_richer_str(existing: &str, incoming: &str) -> String {
    let e = existing.trim();
    let n = incoming.trim();
    if n.is_empty() {
        return existing.to_string();
    }
    if e.is_empty() {
        return incoming.to_string();
    }
    if e.len() > n.len() {
        existing.to_string()
    } else {
        incoming.to_string()
    }
}

pub(super) fn pick_richer_json_array(existing: &str, incoming: &str) -> String {
    let e_empty = existing.trim().is_empty() || existing.trim() == "[]";
    let n_empty = incoming.trim().is_empty() || incoming.trim() == "[]";
    if n_empty && !e_empty {
        return existing.to_string();
    }
    if e_empty {
        return incoming.to_string();
    }
    if existing.len() > incoming.len() {
        existing.to_string()
    } else {
        incoming.to_string()
    }
}

impl super::Database {
    pub fn insert_patent(&self, p: &Patent) -> Result<String> {
        let c = self.conn();
        let canonical = crate::patent::canonical_patent_key(&p.patent_number);
        let mut final_id = p.id.clone();
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
        let mut final_family_id = p.family_id.clone();
        let mut final_legal_status = p.legal_status.clone();
        let mut final_citations = p.citations.clone();
        let mut final_cited_by = p.cited_by.clone();
        let mut final_images = p.images.clone();
        let mut final_pdf_url = p.pdf_url.clone();

        // 按规范公开号范围查表达式索引 idx_patents_norm_pn（消除 42MB 全表 LIKE 扫描），
        // 命中后合并保留所有已富化字段（权利要求/说明书/附图/PDF/IPC/法律状态等）。
        if !canonical.is_empty() {
            let mut stmt = c.prepare(
                "SELECT id, patent_number, title, abstract_text, description, claims,
                        applicant, inventor, filing_date, publication_date, grant_date,
                        ipc_codes, cpc_codes, priority_date, family_id, legal_status,
                        citations, cited_by, images, pdf_url
                 FROM patents
                 WHERE REPLACE(REPLACE(REPLACE(REPLACE(UPPER(patent_number), ' ', ''), '-', ''), '.', ''), '/', '') >= ?1
                   AND REPLACE(REPLACE(REPLACE(REPLACE(UPPER(patent_number), ' ', ''), '-', ''), '.', ''), '/', '') < ?2
                 ORDER BY LENGTH(COALESCE(claims, '')) + LENGTH(COALESCE(images, '')) DESC
                 LIMIT 20",
            )?;
            let canonical_upper = format!("{canonical}\u{10FFFF}");
            let mut rows = stmt.query(params![canonical, canonical_upper])?;
            while let Some(row) = rows.next()? {
                let existing_id: String = row.get(0)?;
                let existing_number: String = row.get(1)?;
                if crate::patent::canonical_patent_key(&existing_number) == canonical {
                    final_id = existing_id;
                    let existing_title: String = row.get(2).unwrap_or_default();
                    let existing_abstract: String = row.get(3).unwrap_or_default();
                    let existing_desc: String = row.get(4).unwrap_or_default();
                    let existing_claims_text: String = row.get(5).unwrap_or_default();
                    let existing_applicant: String = row.get(6).unwrap_or_default();
                    let existing_inventor: String = row.get(7).unwrap_or_default();
                    let existing_filing_date: String = row.get(8).unwrap_or_default();
                    let existing_pub_date: String = row.get(9).unwrap_or_default();
                    let existing_grant_date: Option<String> = row.get(10).ok().flatten();
                    let existing_ipc: String = row.get(11).unwrap_or_default();
                    let existing_cpc: String = row.get(12).unwrap_or_default();
                    let existing_priority_date: String = row.get(13).unwrap_or_default();
                    let existing_family_id: Option<String> = row.get(14).ok().flatten();
                    let existing_legal_status: String = row.get(15).unwrap_or_default();
                    let existing_citations: String = row.get(16).unwrap_or_default();
                    let existing_cited_by: String = row.get(17).unwrap_or_default();
                    let existing_images: String = row.get(18).unwrap_or_default();
                    let existing_pdf_url: String = row.get(19).unwrap_or_default();

                    // 若已有中文标题，严禁被新抓到的纯英文标题冲刷覆盖（保持中文原文）
                    let existing_has_cjk = existing_title
                        .chars()
                        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
                    let new_has_cjk = p
                        .title
                        .chars()
                        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
                    if (existing_has_cjk && !new_has_cjk)
                        || (final_title.trim().is_empty() && !existing_title.trim().is_empty())
                    {
                        final_title = existing_title;
                    }
                    if existing_claims_text.len() > p.claims.len() {
                        final_claims = existing_claims_text;
                    }
                    if existing_desc.len() > p.description.len() {
                        final_description = existing_desc;
                    }
                    let existing_abs_has_cjk = existing_abstract
                        .chars()
                        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
                    let new_abs_has_cjk = p
                        .abstract_text
                        .chars()
                        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
                    if (existing_abs_has_cjk && !new_abs_has_cjk && !existing_abstract.is_empty())
                        || (final_abstract.trim().is_empty()
                            && !existing_abstract.trim().is_empty())
                    {
                        final_abstract = existing_abstract;
                    }
                    final_applicant = pick_richer_str(&existing_applicant, &final_applicant);
                    final_inventor = pick_richer_str(&existing_inventor, &final_inventor);
                    final_filing_date = pick_richer_str(&existing_filing_date, &final_filing_date);
                    final_pub_date = pick_richer_str(&existing_pub_date, &final_pub_date);
                    if final_grant_date
                        .as_deref()
                        .is_none_or(|s| s.trim().is_empty())
                    {
                        final_grant_date = existing_grant_date;
                    }
                    final_ipc = pick_richer_json_array(&existing_ipc, &final_ipc);
                    final_cpc = pick_richer_json_array(&existing_cpc, &final_cpc);
                    final_priority_date =
                        pick_richer_str(&existing_priority_date, &final_priority_date);
                    if final_family_id.is_none() {
                        final_family_id = existing_family_id;
                    }
                    // 仅当新法律状态为空时沿用旧值（保留用户/外部最新合法状态更新能力）
                    if final_legal_status.trim().is_empty() {
                        final_legal_status = existing_legal_status;
                    }
                    final_citations = pick_richer_json_array(&existing_citations, &final_citations);
                    final_cited_by = pick_richer_json_array(&existing_cited_by, &final_cited_by);
                    final_images = pick_richer_json_array(&existing_images, &final_images);
                    final_pdf_url = pick_richer_str(&existing_pdf_url, &final_pdf_url);
                    break;
                }
            }
        }

        // Delete old FTS entry if replacing
        if let Err(e) = c.execute(
            "DELETE FROM patents_fts WHERE rowid = (SELECT rowid FROM patents WHERE id = ?1)",
            params![final_id],
        ) {
            tracing::warn!("FTS delete for patent {} failed: {}", final_id, e);
        }
        c.execute("INSERT OR REPLACE INTO patents (id,patent_number,title,abstract_text,description,claims,applicant,inventor,filing_date,publication_date,grant_date,ipc_codes,cpc_codes,priority_date,country,kind_code,family_id,legal_status,citations,cited_by,source,raw_json,images,pdf_url) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24)",
            params![final_id,p.patent_number,final_title,final_abstract,final_description,final_claims,final_applicant,final_inventor,final_filing_date,final_pub_date,final_grant_date,final_ipc,final_cpc,final_priority_date,p.country,p.kind_code,final_family_id,final_legal_status,final_citations,final_cited_by,p.source,p.raw_json,final_images,final_pdf_url])?;
        // Insert single row into FTS index (incremental, not full rebuild)
        if let Err(e) = c.execute(
            "INSERT INTO patents_fts(rowid, patent_number, title, abstract_text, claims, applicant, inventor, ipc_codes) SELECT rowid, patent_number, title, abstract_text, claims, applicant, inventor, ipc_codes FROM patents WHERE id = ?1",
            params![final_id],
        ) {
            tracing::warn!("FTS insert for patent {} failed: {}", final_id, e);
        }
        Ok(final_id)
    }

    /// 局部更新单篇专利标题并同步更新 FTS 索引
    pub fn update_patent_title(&self, id: &str, title: &str) -> Result<bool> {
        let c = self.conn();
        let rows_affected = c.execute(
            "UPDATE patents SET title = ?1 WHERE id = ?2",
            params![title, id],
        )?;
        if rows_affected > 0 {
            let _ = c.execute(
                "UPDATE patents_fts SET title = ?1 WHERE rowid = (SELECT rowid FROM patents WHERE id = ?2)",
                params![title, id],
            );
        }
        Ok(rows_affected > 0)
    }

    pub fn get_patent(&self, id: &str) -> Result<Option<Patent>> {
        self.with_read_conn(|c| {
            let mut stmt = c.prepare("SELECT id,patent_number,title,abstract_text,description,claims,applicant,inventor,filing_date,publication_date,grant_date,ipc_codes,cpc_codes,priority_date,country,kind_code,family_id,legal_status,citations,cited_by,source,raw_json,created_at,images,pdf_url FROM patents WHERE id=?1 OR patent_number=?1 ORDER BY (CASE WHEN id=?1 THEN 1 ELSE 0 END) DESC, (LENGTH(claims) + LENGTH(description) + LENGTH(images)) DESC LIMIT 1")?;
            let result = stmt
                .query_row(params![id], |r| Ok(Self::row_to_patent(r)))
                .optional()?;
            Ok(result)
        })
    }
    /// 批量查询专利完整信息（WHERE id IN (...)），避免 N+1 查询。
    /// Batch query patents by IDs using WHERE id IN (...) to avoid N+1 queries.
    /// Chunks to 500 per batch due to SQLite parameter limit.
    pub fn get_patents_by_ids(&self, ids: &[String]) -> Result<Vec<Patent>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        self.with_read_conn(|c| {
            let mut result = Vec::new();
            const CHUNK_SIZE: usize = 500;

            for chunk in ids.chunks(CHUNK_SIZE) {
                let placeholders = vec!["?"; chunk.len()].join(",");
                let sql = format!(
                    "SELECT id,patent_number,title,abstract_text,description,claims,applicant,inventor,                  filing_date,publication_date,grant_date,ipc_codes,cpc_codes,priority_date,                  country,kind_code,family_id,legal_status,citations,cited_by,source,raw_json,                  created_at,images,pdf_url FROM patents WHERE id IN ({}) LIMIT {}",
                    placeholders,
                    chunk.len()
                );

                let mut stmt = c.prepare(&sql)?;
                let params: Vec<&str> = chunk.iter().map(|id| id.as_str()).collect();
                let rows =
                    stmt.query_map(rusqlite::params_from_iter(params.iter().copied()), |r| {
                        Ok(Self::row_to_patent(r))
                    })?;

                for row in rows.flatten() {
                    result.push(row);
                }
            }

            Ok(result)
        })
    }

    /// 批量按专利号/DOI 查询记录（避免 N+1 逐条查库，直接命中 `idx_patents_norm_pn` 表达式索引，杜绝 `LOWER(patent_number)` 全表扫描）
    pub fn get_patents_by_numbers(&self, numbers: &[String]) -> Result<Vec<Patent>> {
        let normalized: Vec<String> = numbers
            .iter()
            .map(|n| n.trim().to_uppercase().replace([' ', '-', '.', '/'], ""))
            .filter(|n| !n.is_empty())
            .collect();
        if normalized.is_empty() {
            return Ok(Vec::new());
        }

        self.with_read_conn(|c| {
            let mut result = Vec::new();
            const CHUNK_SIZE: usize = 500;

            for chunk in normalized.chunks(CHUNK_SIZE) {
                let placeholders = vec!["?"; chunk.len()].join(",");
                let sql = format!(
                    "SELECT id,patent_number,title,abstract_text,description,claims,applicant,inventor, \
                     filing_date,publication_date,grant_date,ipc_codes,cpc_codes,priority_date, \
                     country,kind_code,family_id,legal_status,citations,cited_by,source,raw_json, \
                     created_at,images,pdf_url FROM patents \
                     WHERE REPLACE(REPLACE(REPLACE(REPLACE(UPPER(patent_number), ' ', ''), '-', ''), '.', ''), '/', '') IN ({}) LIMIT {}",
                    placeholders,
                    chunk.len().saturating_mul(2)
                );

                let mut stmt = c.prepare(&sql)?;
                let params: Vec<&str> = chunk.iter().map(|n| n.as_str()).collect();
                let rows =
                    stmt.query_map(rusqlite::params_from_iter(params.iter().copied()), |r| {
                        Ok(Self::row_to_patent(r))
                    })?;

                for row in rows.flatten() {
                    result.push(row);
                }
            }

            Ok(result)
        })
    }

    /// 按专利号模糊查找（归一化后匹配），支持带空格/点的输入
    /// Flexible patent number lookup with normalization (handles spaces, dots, kind codes)
    pub fn find_patent_by_number(&self, number: &str) -> Result<Option<Patent>> {
        let canonical = crate::patent::canonical_patent_key(number);
        if canonical.is_empty() {
            return self.get_patent(number);
        }
        self.with_read_conn(|c| {
            let canonical_upper = format!("{canonical}\u{10FFFF}");
            let mut stmt = c.prepare(
                "SELECT id,patent_number,title,abstract_text,description,claims,applicant,inventor,filing_date,publication_date,grant_date,ipc_codes,cpc_codes,priority_date,country,kind_code,family_id,legal_status,citations,cited_by,source,raw_json,created_at,images,pdf_url FROM patents WHERE REPLACE(REPLACE(REPLACE(REPLACE(UPPER(patent_number), ' ', ''), '-', ''), '.', ''), '/', '') >= ?1 AND REPLACE(REPLACE(REPLACE(REPLACE(UPPER(patent_number), ' ', ''), '-', ''), '.', ''), '/', '') < ?2 ORDER BY (LENGTH(claims) + LENGTH(description) + LENGTH(images)) DESC LIMIT 20",
            )?;
            let rows = stmt.query_map(params![canonical, canonical_upper], |r| {
                Ok(Self::row_to_patent(r))
            })?;
            for p in rows.flatten() {
                if crate::patent::canonical_patent_key(&p.patent_number) == canonical {
                    return Ok(Some(p));
                }
            }
            Ok(None)
        })
    }

    /// Detect search type from query string.
    pub fn detect_search_type(&self, query: &str) -> SearchType {
        let q = query.trim();

        // Patent number format (e.g., CN1234567A, US10000000B2, ZL9999.1)
        // Use char count (not byte len) to handle multibyte chars correctly
        let char_count = q.chars().count();
        if (6..=30).contains(&char_count) {
            let upper = q.to_uppercase();
            let country_codes = ["CN", "US", "EP", "JP", "KR", "TW", "HK", "WO", "PCT", "ZL"];
            for code in country_codes {
                if let Some(rest) = upper.strip_prefix(code) {
                    // Verify it actually has digits after the prefix (not just "CN公司")
                    if rest.chars().any(|c| c.is_ascii_digit()) {
                        return SearchType::PatentNumber;
                    }
                }
            }
            // Pure digits (7+) or digit.digit application number format (e.g. 9999.1)
            let digits_only: String = q.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits_only.len() >= 7 && q.chars().all(|c| c.is_ascii_digit() || c == '.') {
                return SearchType::PatentNumber;
            }
        }
        // Short patent-like queries with country prefix (e.g. "CN123")
        if (4..6).contains(&char_count) {
            let upper = q.to_uppercase();
            let prefixes = ["CN", "US", "EP", "JP", "WO", "ZL"];
            for code in prefixes {
                if let Some(rest) = upper.strip_prefix(code) {
                    if rest.chars().all(|c| c.is_ascii_digit()) {
                        return SearchType::PatentNumber;
                    }
                }
            }
        }

        // Company keywords (check BEFORE name detection to avoid misclassifying company names)
        let company_keywords = [
            "公司",
            "集团",
            "股份",
            "有限",
            "责任",
            "corporation",
            "corp",
            "inc",
            "ltd",
            "gmbh",
            "co.",
            "co,",
            "company",
            "tech",
            "technologies",
            "systems",
            "global",
            "group",
            "energy",
            "electronics",
            "motors",
            "pharma",
            "lab",
            "labs",
        ];
        let q_lower = q.to_lowercase();
        if company_keywords.iter().any(|k| q_lower.contains(k)) {
            return SearchType::Applicant;
        }

        // # issue #72 v2 — 不再用 is_likely_name 作路由门。
        // 技术词首字与常见姓重合（高温/金属/白色/向心），姓氏门是按测试词拟合，
        // 违反通用性。姓名召回改由 Mixed 内含的 inventor 字段搜索保证；
        // 仅显式 search_type=inventor 才走 inventor-only（见 search_smart_exact）。
        SearchType::Mixed
    }

    /// Smart search: choose strategy by detected or requested search type.
    ///
    /// 兼容入口：申请人一律走既有 `LIKE %词%` 模糊匹配（与 MA4b 之前逐字一致）。
    /// 需要精确等值通道（规格书 §7「本人姓名精确搜索」）的调用方使用
    /// [`Self::search_smart_exact`]，本函数是其 `exact_assignee = false` 的特化。
    #[allow(clippy::too_many_arguments)]
    pub fn search_smart(
        &self,
        query: &str,
        search_type: Option<&SearchType>,
        country: Option<&str>,
        date_from: Option<&str>,
        date_to: Option<&str>,
        page: usize,
        page_size: usize,
    ) -> Result<(Vec<PatentSummary>, usize, SearchType)> {
        self.search_smart_exact(
            query,
            search_type,
            country,
            date_from,
            date_to,
            page,
            page_size,
            false,
        )
    }

    /// MA4b：带**申请人精确匹配开关**的智能检索。
    ///
    /// `exact_assignee = true` 且实际路由到 [`SearchType::Applicant`] 时，本地 SQL 从
    /// `applicant LIKE %词%` 切换为 `applicant = ?`（等值，参数化，白名单不变）；
    /// 其余类型（发明人/专利号/关键词/混合）**不受本开关影响**，与
    /// [`Self::search_smart`] 逐字同路径——默认值即旧行为。
    #[allow(clippy::too_many_arguments)]
    pub fn search_smart_exact(
        &self,
        query: &str,
        search_type: Option<&SearchType>,
        country: Option<&str>,
        date_from: Option<&str>,
        date_to: Option<&str>,
        page: usize,
        page_size: usize,
        exact_assignee: bool,
    ) -> Result<(Vec<PatentSummary>, usize, SearchType)> {
        let detected_type = if let Some(st) = search_type {
            let auto_detected = self.detect_search_type(query);
            if matches!(st, SearchType::Mixed) && matches!(auto_detected, SearchType::PatentNumber)
            {
                auto_detected
            } else {
                st.clone()
            }
        } else {
            self.detect_search_type(query)
        };

        match detected_type {
            SearchType::PatentNumber => self
                .search_by_patent_number(query, date_from, date_to, page, page_size)
                .map(|(p, t)| (p, t, SearchType::PatentNumber)),
            SearchType::Applicant => self
                .search_by_field(
                    query,
                    "applicant",
                    country,
                    date_from,
                    date_to,
                    page,
                    page_size,
                    exact_assignee,
                )
                .map(|(p, t)| (p, t, SearchType::Applicant)),
            SearchType::Inventor => self
                .search_by_field(
                    query, "inventor", country, date_from, date_to, page, page_size, false,
                )
                .map(|(p, t)| (p, t, SearchType::Inventor)),
            SearchType::Keyword => {
                let has_filters = country.filter(|s| !s.is_empty()).is_some()
                    || date_from.filter(|s| !s.is_empty()).is_some()
                    || date_to.filter(|s| !s.is_empty()).is_some();

                if has_filters {
                    self.search_like(query, country, date_from, date_to, page, page_size)
                        .map(|(p, t)| (p, t, SearchType::Keyword))
                } else {
                    self.search_fts(query, page, page_size)
                        .map(|(p, t)| (p, t, SearchType::Keyword))
                }
            }
            SearchType::Mixed => self
                .search_like(query, country, date_from, date_to, page, page_size)
                .map(|(p, t)| (p, t, SearchType::Mixed)),
        }
    }

    /// 大库全表扫描保护阈值：超过 20,000 件专利时，禁用 `LIKE '%...%'` 全表扫描，
    /// 改走 `patents_fts` 倒排索引 + 前缀/规范号覆盖索引（从数分钟扫盘压至 <20ms）。
    const LARGE_DB_FULLSCAN_GUARD_ROWS: usize = 20_000;

    fn patent_row_count_fast(c: &rusqlite::Connection) -> usize {
        // O(1) B-tree 右叶查找（<0.1ms），避免大库上每次检索重复扫全索引计数
        c.query_row("SELECT COALESCE(MAX(rowid), 0) FROM patents", [], |r| {
            r.get::<_, i64>(0)
        })
        .map(|n| n.max(0) as usize)
        .unwrap_or(0)
    }

    /// 将用户输入切分为安全的 FTS5 前缀词查询（剔除中文括号/标点与 FTS5 保留符，防语法错并支持公司名/长词高速命中）。
    fn build_fts_token_query(query: &str, field_scope: Option<&str>, or_mode: bool) -> String {
        let reserved = ["AND", "OR", "NOT", "NEAR"];
        let mut raw_tokens: Vec<String> = Vec::new();
        for seg in query.split(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    '（' | '）'
                        | '('
                        | ')'
                        | '，'
                        | ','
                        | '、'
                        | '；'
                        | ';'
                        | '：'
                        | ':'
                        | '/'
                        | '\\'
                        | '-'
                        | '"'
                        | '\''
                        | '*'
                        | '^'
                        | '{'
                        | '}'
                        | '['
                        | ']'
                        | '《'
                        | '》'
                        | '。'
                        | '！'
                        | '？'
                        | '!'
                        | '?'
                )
        }) {
            let trimmed = seg.trim();
            if trimmed.is_empty() {
                continue;
            }
            if reserved.contains(&trimmed.to_ascii_uppercase().as_str()) && trimmed.len() <= 4 {
                continue;
            }
            if !raw_tokens.iter().any(|t| t == trimmed) {
                raw_tokens.push(trimmed.to_string());
            }
        }
        // 对于单段较长无标点中文查询，再补充提取核心词以提升 FTS 召回
        if or_mode && raw_tokens.len() <= 1 {
            for kw in crate::search::relevance::extract_query_words(query) {
                let clean: String = kw
                    .chars()
                    .filter(|c| !matches!(*c, '"' | '*' | '^' | '(' | ')' | '{' | '}' | ':'))
                    .collect();
                let trimmed = clean.trim();
                if !trimmed.is_empty()
                    && !reserved.contains(&trimmed.to_ascii_uppercase().as_str())
                    && !raw_tokens.iter().any(|t| t == trimmed)
                {
                    raw_tokens.push(trimmed.to_string());
                }
            }
        }
        if raw_tokens.is_empty() {
            return String::new();
        }
        let quoted: Vec<String> = raw_tokens
            .into_iter()
            .take(8)
            .map(|t| format!("\"{t}\"*"))
            .collect();
        let expr = if or_mode {
            quoted.join(" OR ")
        } else {
            quoted.join(" ")
        };
        match field_scope {
            Some("applicant") => format!("applicant : ({expr})"),
            Some("inventor") => format!("inventor : ({expr})"),
            _ => expr,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn search_fts_with_filters_on_conn(
        c: &rusqlite::Connection,
        query: &str,
        field_scope: Option<&str>,
        country: Option<&str>,
        date_from: &str,
        date_to: &str,
        page: usize,
        page_size: usize,
        or_mode: bool,
    ) -> Result<(Vec<PatentSummary>, usize)> {
        let fts_q = Self::build_fts_token_query(query, field_scope, or_mode);
        if fts_q.is_empty() {
            return Ok((vec![], 0));
        }
        let offset = page.saturating_sub(1) * page_size;
        let country_val = country.unwrap_or("");
        let has_filters = !country_val.is_empty() || !date_from.is_empty() || !date_to.is_empty();
        let total: usize = if !has_filters {
            c.prepare("SELECT COUNT(*) FROM patents_fts WHERE patents_fts MATCH ?1")?
                .query_row(params![fts_q], |r| r.get(0))
                .unwrap_or(0)
        } else {
            c.prepare(
                "SELECT COUNT(*) FROM patents p INNER JOIN patents_fts f ON p.rowid = f.rowid
                 WHERE patents_fts MATCH ?1
                   AND (?2 = '' OR p.country = ?2)
                   AND (?3 = '' OR p.filing_date >= ?3)
                   AND (?4 = '' OR p.filing_date <= ?4)",
            )?
            .query_row(params![fts_q, country_val, date_from, date_to], |r| {
                r.get(0)
            })
            .unwrap_or(0)
        };
        if total == 0 {
            return Ok((vec![], 0));
        }
        let mut stmt = if !has_filters {
            // 无额外过滤时先在 FTS5 内 LIMIT/OFFSET 再回表取至多 page_size 行，避免数万命中全回表扫盘
            c.prepare(
                "SELECT p.id,p.patent_number,p.title,p.abstract_text,p.applicant,p.inventor,p.filing_date,p.country,f.rank
                 FROM (SELECT rowid, rank FROM patents_fts WHERE patents_fts MATCH ?1 ORDER BY rank LIMIT ?2 OFFSET ?3) f
                 INNER JOIN patents p ON p.rowid = f.rowid
                 ORDER BY f.rank",
            )?
        } else {
            c.prepare(
                "SELECT p.id,p.patent_number,p.title,p.abstract_text,p.applicant,p.inventor,p.filing_date,p.country,f.rank
                 FROM patents p INNER JOIN patents_fts f ON p.rowid = f.rowid
                 WHERE patents_fts MATCH ?1
                   AND (?2 = '' OR p.country = ?2)
                   AND (?3 = '' OR p.filing_date >= ?3)
                   AND (?4 = '' OR p.filing_date <= ?4)
                 ORDER BY f.rank LIMIT ?5 OFFSET ?6",
            )?
        };
        let rows = if !has_filters {
            stmt.query_map(params![fts_q, page_size as i64, offset as i64], |r| {
                let applicant = r.get::<_, String>(4).unwrap_or_default();
                let inventor = r.get::<_, String>(5).unwrap_or_default();
                let title = r.get::<_, String>(2).unwrap_or_default();
                let rank: f64 = r.get::<_, f64>(8).unwrap_or(0.0);
                let (score, src) = match field_scope {
                    Some("applicant") => calculate_field_relevance(query, &applicant, "applicant"),
                    Some("inventor") => calculate_field_relevance(query, &inventor, "inventor"),
                    _ => {
                        let s = ((-rank).min(20.0) / 20.0 * 70.0 + 30.0).min(100.0);
                        (s, "FTS5-BM25".to_string())
                    }
                };
                Ok(PatentSummary {
                    id: r.get(0)?,
                    patent_number: r.get(1)?,
                    title,
                    abstract_text: r.get::<_, String>(3).unwrap_or_default(),
                    applicant,
                    inventor,
                    filing_date: r.get::<_, String>(6).unwrap_or_default(),
                    country: r.get::<_, String>(7).unwrap_or_default(),
                    relevance_score: Some(score),
                    score_source: Some(src),
                    matched_fields: None,
                })
            })?
            .filter_map(|r| r.ok())
            .collect()
        } else {
            stmt.query_map(
                params![
                    fts_q,
                    country_val,
                    date_from,
                    date_to,
                    page_size as i64,
                    offset as i64
                ],
                |r| {
                    let applicant = r.get::<_, String>(4).unwrap_or_default();
                    let inventor = r.get::<_, String>(5).unwrap_or_default();
                    let title = r.get::<_, String>(2).unwrap_or_default();
                    let rank: f64 = r.get::<_, f64>(8).unwrap_or(0.0);
                    let (score, src) = match field_scope {
                        Some("applicant") => {
                            calculate_field_relevance(query, &applicant, "applicant")
                        }
                        Some("inventor") => calculate_field_relevance(query, &inventor, "inventor"),
                        _ => {
                            let s = ((-rank).min(20.0) / 20.0 * 70.0 + 30.0).min(100.0);
                            (s, "FTS5-BM25".to_string())
                        }
                    };
                    Ok(PatentSummary {
                        id: r.get(0)?,
                        patent_number: r.get(1)?,
                        title,
                        abstract_text: r.get::<_, String>(3).unwrap_or_default(),
                        applicant,
                        inventor,
                        filing_date: r.get::<_, String>(6).unwrap_or_default(),
                        country: r.get::<_, String>(7).unwrap_or_default(),
                        relevance_score: Some(score),
                        score_source: Some(src),
                        matched_fields: None,
                    })
                },
            )?
            .filter_map(|r| r.ok())
            .collect()
        };
        Ok((rows, total))
    }

    /// Search by patent number.
    fn search_by_patent_number(
        &self,
        query: &str,
        date_from: Option<&str>,
        date_to: Option<&str>,
        page: usize,
        page_size: usize,
    ) -> Result<(Vec<PatentSummary>, usize)> {
        self.with_read_conn(|c| {
            let offset = page.saturating_sub(1) * page_size;
            let date_from = date_from.unwrap_or("");
            let date_to = date_to.unwrap_or("");

            // 优先走归一化专利号表达式索引 `idx_patents_canonical_number`（毫秒级），避免 10 万+ 大库全表 REPLACE LIKE
            let canonical = crate::patent::canonical_patent_key(query);
            if !canonical.is_empty() {
                let canonical_upper = format!("{canonical}\u{10FFFF}");
                let idx_total: usize = c
                    .prepare(
                        "SELECT COUNT(*) FROM patents
                         WHERE REPLACE(REPLACE(REPLACE(REPLACE(UPPER(patent_number), ' ', ''), '-', ''), '.', ''), '/', '') >= ?1
                           AND REPLACE(REPLACE(REPLACE(REPLACE(UPPER(patent_number), ' ', ''), '-', ''), '.', ''), '/', '') < ?2
                           AND (?3 = '' OR filing_date >= ?3)
                           AND (?4 = '' OR filing_date <= ?4)",
                    )?
                    .query_row(params![canonical, canonical_upper, date_from, date_to], |r| r.get(0))
                    .unwrap_or(0);
                if idx_total > 0 || Self::patent_row_count_fast(c) >= Self::LARGE_DB_FULLSCAN_GUARD_ROWS {
                    let mut stmt = c.prepare(
                        "SELECT id,patent_number,title,abstract_text,applicant,inventor,filing_date,country
                         FROM patents
                         WHERE REPLACE(REPLACE(REPLACE(REPLACE(UPPER(patent_number), ' ', ''), '-', ''), '.', ''), '/', '') >= ?1
                           AND REPLACE(REPLACE(REPLACE(REPLACE(UPPER(patent_number), ' ', ''), '-', ''), '.', ''), '/', '') < ?2
                           AND (?3 = '' OR filing_date >= ?3)
                           AND (?4 = '' OR filing_date <= ?4)
                         ORDER BY filing_date DESC LIMIT ?5 OFFSET ?6",
                    )?;
                    let rows = stmt
                        .query_map(
                            params![
                                canonical,
                                canonical_upper,
                                date_from,
                                date_to,
                                page_size as i64,
                                offset as i64
                            ],
                            |r| {
                                Ok(PatentSummary {
                                    id: r.get(0)?,
                                    patent_number: r.get(1)?,
                                    title: r.get(2)?,
                                    abstract_text: r.get::<_, String>(3).unwrap_or_default(),
                                    applicant: r.get::<_, String>(4).unwrap_or_default(),
                                    inventor: r.get::<_, String>(5).unwrap_or_default(),
                                    filing_date: r.get::<_, String>(6).unwrap_or_default(),
                                    country: r.get::<_, String>(7).unwrap_or_default(),
                                    relevance_score: Some(100.0),
                                    score_source: Some("patent number exact match".to_string()),
                                    matched_fields: None,
                                })
                            },
                        )?
                        .filter_map(|r| r.ok())
                        .collect();
                    return Ok((rows, idx_total));
                }
            }

            // Strip spaces and dots for flexible matching (e.g. 9999.1 → 99991)
            let q_clean = query.replace([' ', '.'], "");
            // Also strip CN/ZL prefix to match against bare numbers in patent_number field
            let q_digits: String = q_clean.chars().filter(|ch| ch.is_ascii_digit()).collect();
            let q_with_prefix = format!("%{}%", query.replace(' ', ""));
            let q_digits_like = format!("%{}%", q_digits);

            // Match either the original query or just the digit portion
            let total: usize = c
                .prepare(
                    "SELECT COUNT(*) FROM patents
                 WHERE (REPLACE(REPLACE(patent_number, ' ', ''), '.', '') LIKE ?1
                        OR REPLACE(REPLACE(patent_number, ' ', ''), '.', '') LIKE ?4)
                 AND (?2 = '' OR filing_date >= ?2)
                 AND (?3 = '' OR filing_date <= ?3)",
                )?
                .query_row(
                    params![q_with_prefix, date_from, date_to, q_digits_like],
                    |r| r.get(0),
                )?;

            let mut stmt = c.prepare(
                "SELECT id,patent_number,title,abstract_text,applicant,inventor,filing_date,country
                 FROM patents
                 WHERE (REPLACE(REPLACE(patent_number, ' ', ''), '.', '') LIKE ?1
                        OR REPLACE(REPLACE(patent_number, ' ', ''), '.', '') LIKE ?4)
                 AND (?2 = '' OR filing_date >= ?2)
                 AND (?3 = '' OR filing_date <= ?3)
                 ORDER BY filing_date DESC LIMIT ?5 OFFSET ?6",
            )?;
            let rows = stmt
                .query_map(
                    params![
                        q_with_prefix,
                        date_from,
                        date_to,
                        q_digits_like,
                        page_size as i64,
                        offset as i64
                    ],
                    |r| {
                        Ok(PatentSummary {
                            id: r.get(0)?,
                            patent_number: r.get(1)?,
                            title: r.get(2)?,
                            abstract_text: r.get::<_, String>(3).unwrap_or_default(),
                            applicant: r.get::<_, String>(4).unwrap_or_default(),
                            inventor: r.get::<_, String>(5).unwrap_or_default(),
                            filing_date: r.get::<_, String>(6).unwrap_or_default(),
                            country: r.get::<_, String>(7).unwrap_or_default(),
                            relevance_score: Some(100.0),
                            score_source: Some("patent number exact match".to_string()),
                            matched_fields: None,
                        })
                    },
                )?
                .filter_map(|r| r.ok())
                .collect();

            Ok((rows, total))
        })
    }

    /// Generic search by a single field (applicant or inventor) with optional country filter.
    ///
    /// `exact = true`（仅申请人通道由 [`Self::search_smart_exact`] 置真）时谓词从
    /// `LIKE %词%` 切换为 `= 词` 等值匹配——两者都继续用 `?1` 参数绑定，字段名仍走
    /// 白名单常量，SQL 注入面不增（AGENTS.md 2.7 / 既有白名单不削弱）。
    #[allow(clippy::too_many_arguments)]
    fn search_by_field(
        &self,
        query: &str,
        field: &str, // "applicant" or "inventor"
        country: Option<&str>,
        date_from: Option<&str>,
        date_to: Option<&str>,
        page: usize,
        page_size: usize,
        exact: bool,
    ) -> Result<(Vec<PatentSummary>, usize)> {
        // Whitelist field names to prevent SQL injection
        let field = match field {
            "applicant" => "applicant",
            "inventor" => "inventor",
            _ => return Err(anyhow::anyhow!("invalid search field: {}", field)),
        };
        self.with_read_conn(|c| {
            let offset = page.saturating_sub(1) * page_size;
            let date_from = date_from.unwrap_or("");
            let date_to = date_to.unwrap_or("");

            // 大库保护：模糊申请人/发明人检索在 >= 20,000 行大库下改走 FTS5 倒排索引 + 前缀 B-tree 索引，
            // 彻底根除 `applicant LIKE '%...%'` 双趟 1.1 GB 全表扫盘卡死全站。
            if !exact && Self::patent_row_count_fast(c) >= Self::LARGE_DB_FULLSCAN_GUARD_ROWS {
                let (rows, total) = Self::search_fts_with_filters_on_conn(
                    c,
                    query,
                    Some(field),
                    country,
                    date_from,
                    date_to,
                    page,
                    page_size,
                    false,
                )?;
                if total > 0 {
                    return Ok((rows, total));
                }
                // 再试索引前缀范围（idx_applicant / idx_inventor B-tree，毫秒级）
                let prefix = query.trim();
                if !prefix.is_empty() {
                    let upper = format!("{prefix}\u{10FFFF}");
                    let country_val = country.unwrap_or("");
                    let count_sql = format!(
                        "SELECT COUNT(*) FROM patents WHERE {field} >= ?1 AND {field} < ?2
                         AND (?3 = '' OR country = ?3)
                         AND (?4 = '' OR filing_date >= ?4)
                         AND (?5 = '' OR filing_date <= ?5)"
                    );
                    let total_pref: usize = c
                        .prepare(&count_sql)?
                        .query_row(params![prefix, upper, country_val, date_from, date_to], |r| {
                            r.get(0)
                        })
                        .unwrap_or(0);
                    if total_pref > 0 {
                        let data_sql = format!(
                            "SELECT id,patent_number,title,abstract_text,applicant,inventor,filing_date,country
                             FROM patents WHERE {field} >= ?1 AND {field} < ?2
                             AND (?3 = '' OR country = ?3)
                             AND (?4 = '' OR filing_date >= ?4)
                             AND (?5 = '' OR filing_date <= ?5)
                             ORDER BY filing_date DESC LIMIT ?6 OFFSET ?7"
                        );
                        let mut stmt = c.prepare(&data_sql)?;
                        let pref_rows: Vec<PatentSummary> = stmt
                            .query_map(
                                params![
                                    prefix,
                                    upper,
                                    country_val,
                                    date_from,
                                    date_to,
                                    page_size as i64,
                                    offset as i64
                                ],
                                |row| self.row_to_summary_with_relevance(row, query, field),
                            )?
                            .filter_map(|r| r.ok())
                            .collect();
                        return Ok((pref_rows, total_pref));
                    }
                }
                return Ok((vec![], 0));
            }

            // 精确通道绑定原词（trim 后等值，命中 idx_applicant / idx_inventor）；小库模糊通道保持旧 `%词%` 形态逐字不变。
            let q = if exact {
                query.trim().to_string()
            } else {
                format!("%{}%", query)
            };
            let pred = if exact {
                format!("{} = ?1", field)
            } else {
                format!("{} LIKE ?1", field)
            };

            // Build WHERE clause dynamically based on whether country filter is present
            let (count_sql, data_sql) = if let Some(country_val) = country.filter(|s| !s.is_empty()) {
                let count = format!(
                    "SELECT COUNT(*) FROM patents WHERE {} AND country = ?2
                     AND (?3 = '' OR filing_date >= ?3)
                     AND (?4 = '' OR filing_date <= ?4)",
                    pred
                );
                let data = format!(
                    "SELECT id,patent_number,title,abstract_text,applicant,inventor,filing_date,country
                     FROM patents WHERE {} AND country = ?2
                     AND (?3 = '' OR filing_date >= ?3)
                     AND (?4 = '' OR filing_date <= ?4)
                     ORDER BY filing_date DESC LIMIT ?5 OFFSET ?6",
                    pred
                );

                let total: usize = c
                    .prepare(&count)?
                    .query_row(params![q, country_val, date_from, date_to], |r| r.get(0))?;

                let mut stmt = c.prepare(&data)?;
                let rows: Vec<PatentSummary> = stmt
                    .query_map(
                        params![
                            q,
                            country_val,
                            date_from,
                            date_to,
                            page_size as i64,
                            offset as i64
                        ],
                        |row| self.row_to_summary_with_relevance(row, query, field),
                    )?
                    .filter_map(|r| r.ok())
                    .collect();

                return Ok((rows, total));
            } else {
                let count = format!(
                    "SELECT COUNT(*) FROM patents WHERE {}
                     AND (?2 = '' OR filing_date >= ?2)
                     AND (?3 = '' OR filing_date <= ?3)",
                    pred
                );
                let data = format!(
                    "SELECT id,patent_number,title,abstract_text,applicant,inventor,filing_date,country
                     FROM patents WHERE {}
                     AND (?2 = '' OR filing_date >= ?2)
                     AND (?3 = '' OR filing_date <= ?3)
                     ORDER BY filing_date DESC LIMIT ?4 OFFSET ?5",
                    pred
                );
                (count, data)
            };

            let total: usize = c
                .prepare(&count_sql)?
                .query_row(params![q, date_from, date_to], |r| r.get(0))?;

            let mut stmt = c.prepare(&data_sql)?;
            let rows: Vec<PatentSummary> = stmt
                .query_map(
                    params![q, date_from, date_to, page_size as i64, offset as i64],
                    |row| self.row_to_summary_with_relevance(row, query, field),
                )?
                .filter_map(|r| r.ok())
                .collect();

            Ok((rows, total))
        })
    }

    /// Map a row to PatentSummary with relevance scoring based on field type.
    fn row_to_summary_with_relevance(
        &self,
        row: &rusqlite::Row,
        query: &str,
        field: &str,
    ) -> rusqlite::Result<PatentSummary> {
        let applicant = row.get::<_, String>(4).unwrap_or_default();
        let inventor = row.get::<_, String>(5).unwrap_or_default();

        let (score, source) = match field {
            "applicant" => calculate_field_relevance(query, &applicant, "applicant"),
            "inventor" => calculate_field_relevance(query, &inventor, "inventor"),
            _ => (50.0, "unknown field".to_string()),
        };

        Ok(PatentSummary {
            id: row.get(0)?,
            patent_number: row.get(1)?,
            title: row.get(2)?,
            abstract_text: row.get::<_, String>(3).unwrap_or_default(),
            applicant,
            inventor,
            filing_date: row.get::<_, String>(6).unwrap_or_default(),
            country: row.get::<_, String>(7).unwrap_or_default(),
            relevance_score: Some(score),
            score_source: Some(source),
            matched_fields: None,
        })
    }

    /// Sanitize user input for FTS5 MATCH queries.
    /// Wraps each token in double quotes to prevent FTS5 syntax injection.
    fn sanitize_fts_query(query: &str) -> String {
        // FTS5 reserved operators that must not appear as bare tokens
        let reserved = ["AND", "OR", "NOT", "NEAR"];
        query
            .split_whitespace()
            .map(|word| {
                // Strip all FTS5 special characters: " * ^ ( ) { } :
                let clean: String = word
                    .chars()
                    .filter(|c| !matches!(*c, '"' | '*' | '^' | '(' | ')' | '{' | '}' | ':'))
                    .collect();
                if clean.is_empty() {
                    return String::new();
                }
                // Skip FTS5 reserved keywords (they cause syntax errors when quoted alone)
                if reserved.contains(&clean.to_uppercase().as_str()) && clean.len() <= 4 {
                    return String::new();
                }
                format!("\"{}\"", clean)
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn search_fts(
        &self,
        query: &str,
        page: usize,
        page_size: usize,
    ) -> Result<(Vec<PatentSummary>, usize)> {
        self.with_read_conn(|c| {
            let offset = page.saturating_sub(1) * page_size;
            let safe_query = Self::sanitize_fts_query(query);
            if safe_query.is_empty() {
                return Ok((vec![], 0));
            }
            let total: usize = c
                .prepare("SELECT COUNT(*) FROM patents_fts WHERE patents_fts MATCH ?1")?
                .query_row(params![safe_query], |r| r.get(0))
                .unwrap_or(0);
            // BM25 权重：title(1) 最高, abstract(2) 次之, claims(3) 中等, applicant/inventor(4-5) 较低
            // 列顺序: patent_number(0), title(1), abstract_text(2), claims(3), applicant(4), inventor(5), ipc_codes(6)
            let mut stmt = c.prepare("SELECT p.id,p.patent_number,p.title,p.abstract_text,p.applicant,p.inventor,p.filing_date,p.country,f.rank FROM patents p INNER JOIN patents_fts f ON p.rowid=f.rowid WHERE patents_fts MATCH ?1 ORDER BY bm25(patents_fts, 0.0, 5.0, 10.0, 5.0, 3.0, 2.0, 2.0, 1.0) LIMIT ?2 OFFSET ?3")?;
            let rows = stmt
                .query_map(params![safe_query, page_size as i64, offset as i64], |r| {
                    let rank: f64 = r.get::<_, f64>(8).unwrap_or(0.0);
                    let score = ((-rank).min(20.0) / 20.0 * 70.0 + 30.0).min(100.0);
                    Ok(PatentSummary {
                        id: r.get(0)?,
                        patent_number: r.get(1)?,
                        title: r.get(2)?,
                        abstract_text: r.get::<_, String>(3).unwrap_or_default(),
                        applicant: r.get::<_, String>(4).unwrap_or_default(),
                        inventor: r.get::<_, String>(5).unwrap_or_default(),
                        filing_date: r.get::<_, String>(6).unwrap_or_default(),
                        country: r.get::<_, String>(7).unwrap_or_default(),
                        relevance_score: Some(score),
                        score_source: Some("FTS5-BM25".to_string()),
                        matched_fields: None,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
            Ok((rows, total))
        })
    }

    /// Multi-token LIKE search across title/abstract/applicant/inventor/patent_number.
    ///
    /// # issue #72 — 多词语义显式化
    /// 旧实现把整串 `query` 塞进一个 `%…%` LIKE（含空格），导致「散热底座 均热板」
    /// 这类两词查询——两个词单独都命中——合起来 0 命中。现按空白切 token，走 **OR 语义**：
    /// 每个 token 命中任一字段即返回该行，按命中数（`relevance_score`）排序。
    /// 单 token 时行为与旧 `%query%` 整串 LIKE 逐字一致。
    pub fn search_like(
        &self,
        query: &str,
        country: Option<&str>,
        date_from: Option<&str>,
        date_to: Option<&str>,
        page: usize,
        page_size: usize,
    ) -> Result<(Vec<PatentSummary>, usize)> {
        self.with_read_conn(|c| {
            let date_from_s = date_from.unwrap_or("");
            let date_to_s = date_to.unwrap_or("");
            // 大库全表扫盘保护：>= 20,000 行时，严禁触发多列 35 路 OR LIKE 全盘扫表（会导致全站卡死），
            // 直接走 FTS5 前缀/词组 OR 倒排索引 + SQL 国家/日期条件（< 20ms）。
            if Self::patent_row_count_fast(c) >= Self::LARGE_DB_FULLSCAN_GUARD_ROWS {
                return Self::search_fts_with_filters_on_conn(
                    c,
                    query,
                    None,
                    country.filter(|s| !s.is_empty()),
                    date_from_s,
                    date_to_s,
                    page,
                    page_size,
                    true,
                );
            }
            let offset = page.saturating_sub(1) * page_size;

        // 多词查询切 token：优先尊重用户空格，无空格中文长句自动提取核心技术词，均走 OR 语义。
        let extracted_words = crate::search::relevance::extract_query_words(query);
        let tokens: Vec<&str> = if !extracted_words.is_empty() {
            extracted_words.iter().map(|s| s.as_str()).collect()
        } else {
            query.split_whitespace().filter(|s| !s.is_empty()).collect()
        };
        if tokens.is_empty() {
            return Ok((vec![], 0));
        }
        let like_patterns: Vec<String> = tokens.iter().map(|t| format!("%{}%", t)).collect();

        // 构建 OR 子句：(title LIKE ?1 OR abstract_text LIKE ?1 OR … OR title LIKE ?2 OR …)
        // 同一 token 的所有字段共享一个参数 ?N，减少绑定数。
        const FIELDS: &[&str] = &[
            "title",
            "abstract_text",
            "applicant",
            "inventor",
            "patent_number",
        ];
        let mut or_parts: Vec<String> = Vec::new();
        for (i, _token) in tokens.iter().enumerate() {
            let param = i + 1;
            for field in FIELDS {
                or_parts.push(format!("{field} LIKE ?{param}"));
            }
        }
        let match_clause = or_parts.join(" OR ");

        // ── issue #85: ORDER BY 命中 token 数 DESC, filing_date DESC ──
        // 核心不变量：ORDER BY 里数命中用的列集合必须与 WHERE 里那串 OR LIKE
        // 用的列集合逐字相同。两处共用上面的 FIELDS 常量，杜绝「WHERE 查 5 列、
        // ORDER BY 只数 4 列」的口径分裂——否则会出现「结果集里有它、排序数不到
        // 它」的新缺陷。
        let mut hit_terms: Vec<String> = Vec::with_capacity(tokens.len());
        for (i, _token) in tokens.iter().enumerate() {
            let param = i + 1;
            let field_checks: Vec<String> = FIELDS
                .iter()
                .map(|f| format!("{f} LIKE ?{param}"))
                .collect();
            hit_terms.push(format!(
                "CASE WHEN ({}) THEN 1 ELSE 0 END",
                field_checks.join(" OR ")
            ));
        }
        let hit_count_expr = hit_terms.join(" + ");
        // filing_date DESC 作为同分兜底，保证确定性
        let order_clause = format!("ORDER BY ({hit_count_expr}) DESC, filing_date DESC");

        let date_from = date_from.unwrap_or("");
        let date_to = date_to.unwrap_or("");
        let country = country.filter(|s| !s.is_empty());
        let n_like = like_patterns.len();

        // 收集参数值（LIKE patterns 在前，后续 country/date/limit/offset）
        let param_values: Vec<String> = like_patterns.clone();

        let (where_clause, total, rows) = if let Some(country_val) = country {
            let country_idx = n_like + 1;
            let df_idx = n_like + 2;
            let dt_idx = n_like + 3;
            let where_clause = format!(
                "WHERE ({match_clause}) AND country=?{country_idx}
                 AND (?{df_idx} = '' OR filing_date >= ?{df_idx})
                 AND (?{dt_idx} = '' OR filing_date <= ?{dt_idx})"
            );

            // 单趟覆盖索引查询：用窗口函数 COUNT(*) OVER() 复用同一次扫描，避免双重全表扫描
            let data_sql = format!(
                "SELECT id,patent_number,title,abstract_text,applicant,inventor,filing_date,country,COUNT(*) OVER()
                 FROM patents {where_clause} {order_clause} LIMIT ?{} OFFSET ?{}",
                n_like + 4,
                n_like + 5
            );
            let mut data_params = param_values.clone();
            data_params.push(country_val.to_string());
            data_params.push(date_from.to_string());
            data_params.push(date_to.to_string());
            data_params.push(page_size.to_string());
            data_params.push(offset.to_string());

            let mut stmt = c.prepare(&data_sql)?;
            let mut window_total: Option<usize> = None;
            let rows: Vec<PatentSummary> = stmt
                .query_map(rusqlite::params_from_iter(data_params.iter()), |row| {
                    if window_total.is_none() {
                        window_total = row.get::<_, usize>(8).ok();
                    }
                    let applicant = row.get::<_, String>(4).unwrap_or_default();
                    let inventor = row.get::<_, String>(5).unwrap_or_default();
                    let title = row.get::<_, String>(2).unwrap_or_default();
                    let score = calculate_mixed_relevance(query, &applicant, &inventor, &title);
                    Ok(PatentSummary {
                        id: row.get(0)?,
                        patent_number: row.get(1)?,
                        title,
                        abstract_text: row.get::<_, String>(3).unwrap_or_default(),
                        applicant,
                        inventor,
                        filing_date: row.get::<_, String>(6).unwrap_or_default(),
                        country: row.get::<_, String>(7).unwrap_or_default(),
                        relevance_score: Some(score),
                        score_source: Some("混合检索命中".to_string()),
                        matched_fields: None,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
            let total = if let Some(t) = window_total {
                t
            } else if offset == 0 {
                0
            } else {
                let count_sql = format!("SELECT COUNT(*) FROM patents {where_clause}");
                c.prepare(&count_sql)?.query_row(
                    rusqlite::params_from_iter(
                        param_values.iter().chain(
                            [
                                country_val.to_string(),
                                date_from.to_string(),
                                date_to.to_string(),
                            ]
                            .iter(),
                        ),
                    ),
                    |r| r.get(0),
                )?
            };
            (where_clause, total, rows)
        } else {
            let df_idx = n_like + 1;
            let dt_idx = n_like + 2;
            let where_clause = format!(
                "WHERE ({match_clause})
                 AND (?{df_idx} = '' OR filing_date >= ?{df_idx})
                 AND (?{dt_idx} = '' OR filing_date <= ?{dt_idx})"
            );

            // 单趟覆盖索引查询：用窗口函数 COUNT(*) OVER() 复用同一次扫描，避免双重全表扫描
            let data_sql = format!(
                "SELECT id,patent_number,title,abstract_text,applicant,inventor,filing_date,country,COUNT(*) OVER()
                 FROM patents {where_clause} {order_clause} LIMIT ?{} OFFSET ?{}",
                n_like + 3,
                n_like + 4
            );
            let mut data_params = param_values.clone();
            data_params.push(date_from.to_string());
            data_params.push(date_to.to_string());
            data_params.push(page_size.to_string());
            data_params.push(offset.to_string());

            let mut stmt = c.prepare(&data_sql)?;
            let mut window_total: Option<usize> = None;
            let rows: Vec<PatentSummary> = stmt
                .query_map(rusqlite::params_from_iter(data_params.iter()), |row| {
                    if window_total.is_none() {
                        window_total = row.get::<_, usize>(8).ok();
                    }
                    let applicant = row.get::<_, String>(4).unwrap_or_default();
                    let inventor = row.get::<_, String>(5).unwrap_or_default();
                    let title = row.get::<_, String>(2).unwrap_or_default();
                    let score = calculate_mixed_relevance(query, &applicant, &inventor, &title);
                    Ok(PatentSummary {
                        id: row.get(0)?,
                        patent_number: row.get(1)?,
                        title,
                        abstract_text: row.get::<_, String>(3).unwrap_or_default(),
                        applicant,
                        inventor,
                        filing_date: row.get::<_, String>(6).unwrap_or_default(),
                        country: row.get::<_, String>(7).unwrap_or_default(),
                        relevance_score: Some(score),
                        score_source: Some("混合检索命中".to_string()),
                        matched_fields: None,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
            let total = if let Some(t) = window_total {
                t
            } else if offset == 0 {
                0
            } else {
                let count_sql = format!("SELECT COUNT(*) FROM patents {where_clause}");
                c.prepare(&count_sql)?.query_row(
                    rusqlite::params_from_iter(
                        param_values
                            .iter()
                            .chain([date_from.to_string(), date_to.to_string()].iter()),
                    ),
                    |r| r.get(0),
                )?
            };
            (where_clause, total, rows)
        };
        let _ = where_clause;
        Ok((rows, total))
        })
    }

    // ── IPC Classification ────────────────────────────────────────────────────

    pub fn get_all_ipc_codes(&self) -> Result<Vec<String>> {
        self.with_read_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT ipc_codes FROM patents WHERE ipc_codes != '' AND ipc_codes IS NOT NULL",
            )?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .filter_map(|r| r.ok())
                .collect();
            Ok(rows)
        })
    }

    /// 快速获取 `patents` 表行数与最大 `rowid` 水位（毫秒级，用于 IPC 分类树缓存失效校验）。
    pub fn get_patent_table_watermark(&self) -> Result<(usize, i64)> {
        self.with_read_conn(|c| {
            let (cnt, max_rowid): (i64, i64) = c.query_row(
                "SELECT COUNT(*), COALESCE(MAX(rowid), 0) FROM patents",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            Ok((cnt.max(0) as usize, max_rowid))
        })
    }

    /// 按 `ipc_codes` 原始字符串分组统计数量，大幅减少大库向 Rust 侧搬运重复字符串的内存开销。
    pub fn get_ipc_code_counts(&self) -> Result<Vec<(String, usize)>> {
        self.with_read_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT ipc_codes, COUNT(*) FROM patents \
                 WHERE ipc_codes IS NOT NULL AND ipc_codes != '' AND ipc_codes != '[]' \
                 GROUP BY ipc_codes",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    let raw: String = row.get(0)?;
                    let cnt: i64 = row.get(1)?;
                    Ok((raw, cnt.max(0) as usize))
                })?
                .filter_map(|r| r.ok())
                .collect();
            Ok(rows)
        })
    }

    /// 按 rowid 倒序快速取出最近入库的专利 ID（毫秒级，避免 FTO 兜底调用空串 `search_like("")` 恒返回空）。
    pub fn list_recent_patent_ids(&self, limit: usize) -> Result<Vec<String>> {
        self.with_read_conn(|c| {
            let mut stmt = c.prepare("SELECT id FROM patents ORDER BY rowid DESC LIMIT ?1")?;
            let rows = stmt
                .query_map(params![limit.clamp(1, 200) as i64], |row| {
                    row.get::<_, String>(0)
                })?
                .filter_map(|r| r.ok())
                .collect();
            Ok(rows)
        })
    }

    pub fn search_by_ipc(&self, code: &str) -> Result<Vec<serde_json::Value>> {
        let target = code.trim().to_uppercase();
        if target.is_empty() {
            return Ok(Vec::new());
        }
        self.with_read_conn(|c| {
            // 大库走 FTS5 `ipc_codes` 倒排前缀索引（<10ms），彻底避免点击冷门部类时 1.1 GB 全表 LIKE 扫盘
            let use_fts_fast = Self::patent_row_count_fast(c) >= Self::LARGE_DB_FULLSCAN_GUARD_ROWS;
            let raw_rows: Vec<(String, serde_json::Value)> = if use_fts_fast {
                let clean: String = target
                    .chars()
                    .filter(|ch| ch.is_ascii_alphanumeric())
                    .collect();
                if clean.is_empty() {
                    return Ok(Vec::new());
                }
                let fts_q = format!("ipc_codes : (\"{clean}\"*)");
                let mut stmt = c.prepare(
                    "SELECT p.id, p.patent_number, p.title, p.abstract_text, p.applicant, p.filing_date, p.country, p.ipc_codes
                     FROM (SELECT rowid FROM patents_fts WHERE patents_fts MATCH ?1 LIMIT 300) f
                     INNER JOIN patents p ON p.rowid = f.rowid
                     ORDER BY p.filing_date DESC",
                )?;
                let items: Vec<(String, serde_json::Value)> = stmt
                    .query_map(params![fts_q], |row| {
                        let ipc_raw: String = row.get::<_, String>(7).unwrap_or_default();
                        Ok((
                            ipc_raw.clone(),
                            serde_json::json!({
                                "id": row.get::<_, String>(0)?,
                                "patent_number": row.get::<_, String>(1)?,
                                "title": row.get::<_, String>(2)?,
                                "abstract_text": row.get::<_, String>(3).unwrap_or_default(),
                                "applicant": row.get::<_, String>(4).unwrap_or_default(),
                                "filing_date": row.get::<_, String>(5).unwrap_or_default(),
                                "country": row.get::<_, String>(6).unwrap_or_default(),
                                "ipc_codes": ipc_raw,
                            }),
                        ))
                    })?
                    .filter_map(|r| r.ok())
                    .collect();
                items
            } else {
                let pattern = format!("%{}%", target);
                // 先放大候选取 300 条，再对解析出的真实 IPC 列表执行严格前缀匹配（防止查部类 "F" 误命中 "G06F"）
                let mut stmt = c.prepare(
                    "SELECT id, patent_number, title, abstract_text, applicant, filing_date, country, ipc_codes
                     FROM patents WHERE ipc_codes LIKE ?1 ORDER BY filing_date DESC LIMIT 300",
                )?;
                let items: Vec<(String, serde_json::Value)> = stmt
                    .query_map(params![pattern], |row| {
                        let ipc_raw: String = row.get::<_, String>(7).unwrap_or_default();
                        Ok((
                            ipc_raw.clone(),
                            serde_json::json!({
                                "id": row.get::<_, String>(0)?,
                                "patent_number": row.get::<_, String>(1)?,
                                "title": row.get::<_, String>(2)?,
                                "abstract_text": row.get::<_, String>(3).unwrap_or_default(),
                                "applicant": row.get::<_, String>(4).unwrap_or_default(),
                                "filing_date": row.get::<_, String>(5).unwrap_or_default(),
                                "country": row.get::<_, String>(6).unwrap_or_default(),
                                "ipc_codes": ipc_raw,
                            }),
                        ))
                    })?
                    .filter_map(|r| r.ok())
                    .collect();
                items
            };
            let rows = raw_rows
                .into_iter()
                .filter(|(ipc_raw, _)| {
                    crate::routes::parse_ipc_code_list(ipc_raw)
                        .iter()
                        .any(|c| c.starts_with(&target))
                })
                .take(100)
                .map(|(_, val)| val)
                .collect();
            Ok(rows)
        })
    }

    fn row_to_patent(r: &rusqlite::Row) -> Patent {
        Patent {
            id: r.get(0).unwrap_or_default(),
            patent_number: r.get(1).unwrap_or_default(),
            title: r.get(2).unwrap_or_default(),
            abstract_text: r.get(3).unwrap_or_default(),
            description: r.get(4).unwrap_or_default(),
            claims: r.get(5).unwrap_or_default(),
            applicant: r.get(6).unwrap_or_default(),
            inventor: r.get(7).unwrap_or_default(),
            filing_date: r.get(8).unwrap_or_default(),
            publication_date: r.get(9).unwrap_or_default(),
            grant_date: r.get(10).ok(),
            ipc_codes: r.get(11).unwrap_or_default(),
            cpc_codes: r.get(12).unwrap_or_default(),
            priority_date: r.get(13).unwrap_or_default(),
            country: r.get(14).unwrap_or_default(),
            kind_code: r.get(15).unwrap_or_default(),
            family_id: r.get(16).ok(),
            legal_status: r.get(17).unwrap_or_default(),
            citations: r.get(18).unwrap_or_default(),
            cited_by: r.get(19).unwrap_or_default(),
            source: r.get(20).unwrap_or_default(),
            raw_json: r.get(21).unwrap_or_default(),
            created_at: r.get(22).unwrap_or_default(),
            images: r.get(23).unwrap_or_default(),
            pdf_url: r.get(24).unwrap_or_default(),
        }
    }

    /// 更新专利的法律状态 / Update patent legal status
    pub fn update_patent_legal_status(
        &self,
        patent_number: &str,
        legal_status: &str,
    ) -> Result<()> {
        let c = self.conn();
        c.execute(
            "UPDATE patents SET legal_status = ?1 WHERE patent_number = ?2 OR id = ?2",
            params![legal_status, patent_number],
        )?;
        Ok(())
    }

    /// 同步更新专利法律状态、同族 ID 与前后向引文 JSON（Batch 11 / v0.9.54）。
    pub fn update_patent_legal_and_citations(
        &self,
        patent_number: &str,
        legal_status: &str,
        family_id: Option<&str>,
        citations_json: Option<&str>,
        cited_by_json: Option<&str>,
    ) -> Result<()> {
        let c = self.conn();
        c.execute(
            "UPDATE patents
             SET legal_status = CASE WHEN ?1 <> '' THEN ?1 ELSE legal_status END,
                 family_id = COALESCE(?2, family_id),
                 citations = CASE WHEN ?3 IS NOT NULL AND ?3 <> '' THEN ?3 ELSE citations END,
                 cited_by = CASE WHEN ?4 IS NOT NULL AND ?4 <> '' THEN ?4 ELSE cited_by END
             WHERE patent_number = ?5 OR id = ?5",
            params![
                legal_status,
                family_id,
                citations_json,
                cited_by_json,
                patent_number
            ],
        )?;
        Ok(())
    }

    /// 查询同族专利列表（按 family_id 匹配并排除自身）。
    pub fn list_family_patents(
        &self,
        family_id: &str,
        exclude_patent_number: &str,
    ) -> Result<Vec<Patent>> {
        if family_id.trim().is_empty() {
            return Ok(Vec::new());
        }
        self.with_read_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT id,patent_number,title,abstract_text,description,claims,applicant,inventor,
                        filing_date,publication_date,grant_date,ipc_codes,cpc_codes,priority_date,
                        country,kind_code,family_id,legal_status,citations,cited_by,source,raw_json,
                        created_at,images,pdf_url
                 FROM patents
                 WHERE family_id = ?1 AND patent_number <> ?2
                 LIMIT 30",
            )?;
            let rows = stmt
                .query_map(params![family_id.trim(), exclude_patent_number], |r| {
                    Ok(Self::row_to_patent(r))
                })?
                .filter_map(|r| r.ok())
                .collect();
            Ok(rows)
        })
    }
}

#[cfg(test)]
mod insert_dedup_tests {
    use crate::db::Database;
    use crate::patent::Patent;

    #[test]
    fn insert_patent_dedups_beyond_200_rows_and_preserves_rich_fields() {
        let db = Database::init(":memory:").expect("init db");
        // 先插入 220 条 CN 专利，突破旧版 `%CN% + LIMIT 200` 盲区
        for i in 0..220 {
            let p = Patent {
                id: format!("seed-{i}"),
                patent_number: format!("CN99100{i:04}A"),
                title: format!("测试专利 {i}"),
                ..Default::default()
            };
            db.insert_patent(&p).expect("seed");
        }

        // 第 221 条：带完整权利要求、图纸、PDF 和 IPC
        let rich = Patent {
            id: "rich-target-id".to_string(),
            patent_number: "CN999888777B".to_string(),
            title: "高效伺服冷却系统".to_string(),
            claims: "1. 一种高效伺服冷却系统，包括散热器与油泵。".to_string(),
            images: r#"["data:image/jpeg;base64,AAA"]"#.to_string(),
            pdf_url: "https://patentimages.storage.googleapis.com/demo.pdf".to_string(),
            ipc_codes: r#"["H02K9/19"]"#.to_string(),
            grant_date: Some("2024-06-14".to_string()),
            ..Default::default()
        };
        let id1 = db.insert_patent(&rich).expect("insert rich");
        assert_eq!(id1, "rich-target-id");

        // 再次插入同号但浅层字段（无图无权要无 IPC），必须复用原 ID 且绝不冲掉附图与 IPC
        let shallow = Patent {
            id: "dup-new-uuid".to_string(),
            patent_number: "CN 999888777 B".to_string(),
            title: "High efficiency servo cooling".to_string(),
            images: "[]".to_string(),
            ipc_codes: "[]".to_string(),
            ..Default::default()
        };
        let id2 = db.insert_patent(&shallow).expect("insert shallow dup");
        assert_eq!(id2, id1, "超过 200 条后同号仍必须复用到同一主行");

        let count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM patents WHERE patent_number LIKE '%999888777%'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(count, 1, "同公开号不得产生重复行");

        let loaded = db.get_patent(&id1).expect("get").expect("exists");
        assert_eq!(loaded.title, "高效伺服冷却系统");
        assert!(loaded.images.contains("data:image/jpeg;base64,AAA"));
        assert!(loaded.ipc_codes.contains("H02K9/19"));
        assert_eq!(loaded.grant_date.as_deref(), Some("2024-06-14"));
    }

    #[test]
    fn get_patent_by_number_prefers_richest_fulltext_row() {
        let db = Database::init(":memory:").expect("mem db");
        // 模拟历史库中残留的“旧浅层行 + 新全文行”同专利号双行
        {
            let c = db.conn();
            c.execute(
                "INSERT INTO patents (id, patent_number, title, abstract_text, description, claims, images) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    "legacy-shallow-id",
                    "CN99990001U",
                    "旧摘要行",
                    "仅有摘要",
                    "",
                    "",
                    "[]"
                ],
            )
            .expect("insert legacy shallow");
            c.execute(
                "INSERT INTO patents (id, patent_number, title, abstract_text, description, claims, images) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    "newer-fulltext-id",
                    "CN99990001U",
                    "完整全文行",
                    "完整摘要",
                    "具体实施方式：定子铁芯内设有螺旋油冷流道，油泵压力0.4MPa。",
                    "1. 一种油冷定子铁芯结构，其特征在于设有螺旋流道。",
                    r#"["data:image/png;base64,BBB"]"#
                ],
            )
            .expect("insert newer fulltext");
        }

        // 按专利号查询必须命中内容最完整的全文行，而不是较早插入的空摘要行
        let by_num = db
            .get_patent("CN99990001U")
            .expect("get by num")
            .expect("exists");
        assert_eq!(by_num.id, "newer-fulltext-id");
        assert!(by_num.claims.contains("螺旋流道"));
        assert!(by_num.description.contains("0.4MPa"));

        // 按精确 ID 查询仍应返回精确匹配行
        let by_id = db
            .get_patent("legacy-shallow-id")
            .expect("get by id")
            .expect("exists");
        assert_eq!(by_id.id, "legacy-shallow-id");

        // 归一化公开号查找 find_patent_by_number 也必须优先命中全文行
        let by_norm = db
            .find_patent_by_number("CN 99990001 U")
            .expect("find by norm")
            .expect("exists");
        assert_eq!(by_norm.id, "newer-fulltext-id");
        assert!(by_norm.claims.contains("螺旋流道"));
    }

    #[test]
    fn fts_fast_path_handles_chinese_parentheses_and_concurrent_wal_reader_never_blocks() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_file = tmp.path().join("concurrent_read.db");
        let db = std::sync::Arc::new(
            Database::init(db_file.to_str().expect("path")).expect("init file db"),
        );
        db.insert_patent(&Patent {
            id: "wal-read-1".into(),
            patent_number: "CN990951A".into(),
            title: "绿色建材 低碳胶凝材料".into(),
            abstract_text: "建筑环保材料制备工艺".into(),
            applicant: "星辰（示例）绿色建材有限公司".into(),
            ipc_codes: "[\"C04B 28/00\"]".into(),
            filing_date: "2026-05-01".into(),
            country: "CN".into(),
            ..Default::default()
        })
        .expect("insert");

        // 验证带中文括号的公司名在 FTS5 通道下无语法错误且精准命中（含无过滤子查询路径与过滤路径）
        let (rows, total) = db
            .with_read_conn(|c| {
                Database::search_fts_with_filters_on_conn(
                    c,
                    "星辰（示例）绿色建材有限公司",
                    Some("applicant"),
                    Some("CN"),
                    "",
                    "",
                    1,
                    10,
                    false,
                )
            })
            .expect("fts applicant query");
        assert_eq!(total, 1);
        assert_eq!(rows[0].patent_number, "CN990951A");

        let (unfiltered_rows, unfiltered_total) = db
            .with_read_conn(|c| {
                Database::search_fts_with_filters_on_conn(
                    c,
                    "星辰（示例）绿色建材有限公司",
                    Some("applicant"),
                    None,
                    "",
                    "",
                    1,
                    10,
                    false,
                )
            })
            .expect("fts applicant unfiltered query");
        assert_eq!(unfiltered_total, 1);
        assert_eq!(unfiltered_rows[0].patent_number, "CN990951A");

        // 反卡死核心实测：主连接互斥锁 `db.conn()` 被故意持有时，独立 WAL 只读连接必须毫秒级完成查询不阻塞！
        let _hold_main_mutex = db.conn();
        let db_reader = db.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let fts_res = db_reader.search_fts("绿色建材", 1, 10).map(|(_, t)| t);
            let batch_res = db_reader
                .get_patents_by_numbers(&["cn 990951 a".to_string()])
                .map(|v| v.len());
            let _ = tx.send((fts_res, batch_res));
        });
        let (fts_res, batch_res) = rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .expect("WAL concurrent reader must not block while main conn mutex is held");
        assert_eq!(fts_res.expect("search_fts ok"), 1);
        assert_eq!(batch_res.expect("get_patents_by_numbers ok"), 1);
    }
}
