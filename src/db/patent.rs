use super::relevance::{calculate_field_relevance, calculate_mixed_relevance};
use crate::patent::{Patent, PatentSummary, SearchType};
use anyhow::Result;
use rusqlite::{params, OptionalExtension};

impl super::Database {
    pub fn insert_patent(&self, p: &Patent) -> Result<String> {
        let c = self.conn();
        let canonical = crate::patent::canonical_patent_key(&p.patent_number);
        let mut final_id = p.id.clone();

        // Reuse existing row id for same canonical publication key to avoid duplicate rows.
        if !canonical.is_empty() {
            let mut stmt = c.prepare(
                "SELECT id, patent_number FROM patents
                 WHERE patent_number LIKE ?1
                    OR REPLACE(REPLACE(UPPER(patent_number), ' ', ''), '.', '') LIKE ?2
                 LIMIT 200",
            )?;
            let fuzzy_like = format!("%{}%", p.patent_number.chars().take(2).collect::<String>());
            let canonical_like = format!("%{}%", canonical);
            let mut rows = stmt.query(params![fuzzy_like, canonical_like])?;
            while let Some(row) = rows.next()? {
                let existing_id: String = row.get(0)?;
                let existing_number: String = row.get(1)?;
                if crate::patent::canonical_patent_key(&existing_number) == canonical {
                    final_id = existing_id;
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
            params![final_id,p.patent_number,p.title,p.abstract_text,p.description,p.claims,p.applicant,p.inventor,p.filing_date,p.publication_date,p.grant_date,p.ipc_codes,p.cpc_codes,p.priority_date,p.country,p.kind_code,p.family_id,p.legal_status,p.citations,p.cited_by,p.source,p.raw_json,p.images,p.pdf_url])?;
        // Insert single row into FTS index (incremental, not full rebuild)
        if let Err(e) = c.execute(
            "INSERT INTO patents_fts(rowid, patent_number, title, abstract_text, claims, applicant, inventor, ipc_codes) SELECT rowid, patent_number, title, abstract_text, claims, applicant, inventor, ipc_codes FROM patents WHERE id = ?1",
            params![final_id],
        ) {
            tracing::warn!("FTS insert for patent {} failed: {}", final_id, e);
        }
        Ok(final_id)
    }

    pub fn get_patent(&self, id: &str) -> Result<Option<Patent>> {
        let c = self.conn();
        let mut stmt = c.prepare("SELECT id,patent_number,title,abstract_text,description,claims,applicant,inventor,filing_date,publication_date,grant_date,ipc_codes,cpc_codes,priority_date,country,kind_code,family_id,legal_status,citations,cited_by,source,raw_json,created_at,images,pdf_url FROM patents WHERE id=?1 OR patent_number=?1")?;
        let result = stmt
            .query_row(params![id], |r| Ok(Self::row_to_patent(r)))
            .optional()?;
        Ok(result)
    }
    /// 批量查询专利完整信息（WHERE id IN (...)），避免 N+1 查询。
    /// Batch query patents by IDs using WHERE id IN (...) to avoid N+1 queries.
    /// Chunks to 500 per batch due to SQLite parameter limit.
    pub fn get_patents_by_ids(&self, ids: &[String]) -> Result<Vec<Patent>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let c = self.conn();
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
            let rows = stmt.query_map(rusqlite::params_from_iter(params.iter().copied()), |r| {
                Ok(Self::row_to_patent(r))
            })?;

            for row in rows.flatten() {
                result.push(row);
            }
        }

        Ok(result)
    }

    /// 按专利号模糊查找（归一化后匹配），支持带空格/点的输入
    /// Flexible patent number lookup with normalization (handles spaces, dots, kind codes)
    pub fn find_patent_by_number(&self, number: &str) -> Result<Option<Patent>> {
        let canonical = crate::patent::canonical_patent_key(number);
        if canonical.is_empty() {
            return self.get_patent(number);
        }
        let c = self.conn();
        let like = format!("%{}%", canonical);
        let mut stmt = c.prepare(
            "SELECT id,patent_number,title,abstract_text,description,claims,applicant,inventor,filing_date,publication_date,grant_date,ipc_codes,cpc_codes,priority_date,country,kind_code,family_id,legal_status,citations,cited_by,source,raw_json,created_at,images,pdf_url FROM patents WHERE REPLACE(REPLACE(UPPER(patent_number), ' ', ''), '.', '') LIKE ?1 LIMIT 5",
        )?;
        let rows = stmt.query_map(params![like], |r| Ok(Self::row_to_patent(r)))?;
        for p in rows.flatten() {
            if crate::patent::canonical_patent_key(&p.patent_number) == canonical {
                return Ok(Some(p));
            }
        }
        Ok(None)
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

    /// Search by patent number.
    fn search_by_patent_number(
        &self,
        query: &str,
        date_from: Option<&str>,
        date_to: Option<&str>,
        page: usize,
        page_size: usize,
    ) -> Result<(Vec<PatentSummary>, usize)> {
        let c = self.conn();
        let offset = page.saturating_sub(1) * page_size;
        // Strip spaces and dots for flexible matching (e.g. 9999.1 → 99991)
        let q_clean = query.replace([' ', '.'], "");
        // Also strip CN/ZL prefix to match against bare numbers in patent_number field
        let q_digits: String = q_clean.chars().filter(|c| c.is_ascii_digit()).collect();
        let q_with_prefix = format!("%{}%", query.replace(' ', ""));
        let q_digits_like = format!("%{}%", q_digits);
        let date_from = date_from.unwrap_or("");
        let date_to = date_to.unwrap_or("");

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
        let c = self.conn();
        let offset = page.saturating_sub(1) * page_size;
        // 精确通道绑定原词（trim 后等值）；模糊通道保持旧 `%词%` 形态逐字不变。
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
        let date_from = date_from.unwrap_or("");
        let date_to = date_to.unwrap_or("");

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
        let c = self.conn();
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
                // FTS5 BM25 rank is negative (closer to 0 = better). Normalize to 0-100.
                // Typical range: -20 (very relevant) to 0 (less relevant)
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
        let c = self.conn();
        let offset = page.saturating_sub(1) * page_size;

        // 按空白切 token；单 token 时退化为旧整串 LIKE（issue #72）。
        let tokens: Vec<&str> = query.split_whitespace().filter(|s| !s.is_empty()).collect();
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
        let mut param_values: Vec<String> = like_patterns.clone();

        let (where_clause, total, rows) = if let Some(country_val) = country {
            let country_idx = n_like + 1;
            let df_idx = n_like + 2;
            let dt_idx = n_like + 3;
            let where_clause = format!(
                "WHERE ({match_clause}) AND country=?{country_idx}
                 AND (?{df_idx} = '' OR filing_date >= ?{df_idx})
                 AND (?{dt_idx} = '' OR filing_date <= ?{dt_idx})"
            );
            let count_sql = format!("SELECT COUNT(*) FROM patents {where_clause}");
            let total: usize = c.prepare(&count_sql)?.query_row(
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
            )?;

            let data_sql = format!(
                "SELECT id,patent_number,title,abstract_text,applicant,inventor,filing_date,country
                 FROM patents {where_clause} {order_clause} LIMIT ?{} OFFSET ?{}",
                n_like + 4,
                n_like + 5
            );
            param_values.push(country_val.to_string());
            param_values.push(date_from.to_string());
            param_values.push(date_to.to_string());
            param_values.push(page_size.to_string());
            param_values.push(offset.to_string());

            let mut stmt = c.prepare(&data_sql)?;
            let rows: Vec<PatentSummary> = stmt
                .query_map(rusqlite::params_from_iter(param_values.iter()), |row| {
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
                        score_source: Some("mixed search match".to_string()),
                        matched_fields: None,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
            (where_clause, total, rows)
        } else {
            let df_idx = n_like + 1;
            let dt_idx = n_like + 2;
            let where_clause = format!(
                "WHERE ({match_clause})
                 AND (?{df_idx} = '' OR filing_date >= ?{df_idx})
                 AND (?{dt_idx} = '' OR filing_date <= ?{dt_idx})"
            );
            let count_sql = format!("SELECT COUNT(*) FROM patents {where_clause}");
            let total: usize = c.prepare(&count_sql)?.query_row(
                rusqlite::params_from_iter(
                    param_values
                        .iter()
                        .chain([date_from.to_string(), date_to.to_string()].iter()),
                ),
                |r| r.get(0),
            )?;

            let data_sql = format!(
                "SELECT id,patent_number,title,abstract_text,applicant,inventor,filing_date,country
                 FROM patents {where_clause} {order_clause} LIMIT ?{} OFFSET ?{}",
                n_like + 3,
                n_like + 4
            );
            param_values.push(date_from.to_string());
            param_values.push(date_to.to_string());
            param_values.push(page_size.to_string());
            param_values.push(offset.to_string());

            let mut stmt = c.prepare(&data_sql)?;
            let rows: Vec<PatentSummary> = stmt
                .query_map(rusqlite::params_from_iter(param_values.iter()), |row| {
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
                        score_source: Some("mixed search match".to_string()),
                        matched_fields: None,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
            (where_clause, total, rows)
        };
        let _ = where_clause;
        Ok((rows, total))
    }
    // ── IPC Classification ────────────────────────────────────────────────────

    pub fn get_all_ipc_codes(&self) -> Result<Vec<String>> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT ipc_codes FROM patents WHERE ipc_codes != '' AND ipc_codes IS NOT NULL",
        )?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    pub fn search_by_ipc(&self, code: &str) -> Result<Vec<serde_json::Value>> {
        let c = self.conn();
        let pattern = format!("%{}%", code);
        let mut stmt = c.prepare(
            "SELECT id, patent_number, title, abstract_text, applicant, filing_date, country, ipc_codes
             FROM patents WHERE ipc_codes LIKE ?1 ORDER BY filing_date DESC LIMIT 100",
        )?;
        let rows = stmt
            .query_map(params![pattern], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "patent_number": row.get::<_, String>(1)?,
                    "title": row.get::<_, String>(2)?,
                    "abstract_text": row.get::<_, String>(3).unwrap_or_default(),
                    "applicant": row.get::<_, String>(4).unwrap_or_default(),
                    "filing_date": row.get::<_, String>(5).unwrap_or_default(),
                    "country": row.get::<_, String>(6).unwrap_or_default(),
                    "ipc_codes": row.get::<_, String>(7).unwrap_or_default(),
                }))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
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
            "UPDATE patents SET legal_status = ?1 WHERE patent_number = ?2",
            params![legal_status, patent_number],
        )?;
        Ok(())
    }
}
