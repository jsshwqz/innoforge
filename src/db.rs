use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::Mutex;
use crate::patent::{Patent, PatentSummary, SearchType};

pub struct Database { conn: Mutex<Connection> }

impl Database {
    pub fn init(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS patents (
                id TEXT PRIMARY KEY, patent_number TEXT NOT NULL, title TEXT NOT NULL,
                abstract_text TEXT, description TEXT, claims TEXT, applicant TEXT,
                inventor TEXT, filing_date TEXT, publication_date TEXT, grant_date TEXT,
                ipc_codes TEXT, cpc_codes TEXT, priority_date TEXT, country TEXT,
                kind_code TEXT, family_id TEXT, legal_status TEXT, citations TEXT,
                cited_by TEXT, source TEXT, raw_json TEXT,
                created_at TEXT DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_pn ON patents(patent_number);
            CREATE INDEX IF NOT EXISTS idx_applicant ON patents(applicant);
            CREATE INDEX IF NOT EXISTS idx_inventor ON patents(inventor);
            CREATE VIRTUAL TABLE IF NOT EXISTS patents_fts USING fts5(
                patent_number, title, abstract_text, claims, applicant, inventor, ipc_codes,
                content='patents', content_rowid='rowid'
            );
        ")?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn insert_patent(&self, p: &Patent) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute("INSERT OR REPLACE INTO patents (id,patent_number,title,abstract_text,description,claims,applicant,inventor,filing_date,publication_date,grant_date,ipc_codes,cpc_codes,priority_date,country,kind_code,family_id,legal_status,citations,cited_by,source,raw_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)",
            params![p.id,p.patent_number,p.title,p.abstract_text,p.description,p.claims,p.applicant,p.inventor,p.filing_date,p.publication_date,p.grant_date,p.ipc_codes,p.cpc_codes,p.priority_date,p.country,p.kind_code,p.family_id,p.legal_status,p.citations,p.cited_by,p.source,p.raw_json])?;
        let _ = c.execute("INSERT INTO patents_fts(patents_fts) VALUES('rebuild')", []);
        Ok(())
    }

    pub fn get_patent(&self, id: &str) -> Result<Option<Patent>> {
        let c = self.conn.lock().unwrap();
        let mut stmt = c.prepare("SELECT id,patent_number,title,abstract_text,description,claims,applicant,inventor,filing_date,publication_date,grant_date,ipc_codes,cpc_codes,priority_date,country,kind_code,family_id,legal_status,citations,cited_by,source,raw_json,created_at FROM patents WHERE id=?1 OR patent_number=?1")?;
        let result = stmt.query_row(params![id], |r| Ok(Self::row_to_patent(r))).optional()?;
        Ok(result)
    }

    /// 检测搜索类型
    pub fn detect_search_type(&self, query: &str) -> SearchType {
        let q = query.trim();
        
        // 专利号格式检测 (如 CN1234567A, US10000000B2)
        if q.len() >= 6 && q.len() <= 20 {
            let upper = q.to_uppercase();
            let country_codes = ["CN", "US", "EP", "JP", "KR", "TW", "HK", "WO", "PCT"];
            for code in country_codes {
                if upper.starts_with(code) {
                    return SearchType::PatentNumber;
                }
            }
            // 纯数字也可能是专利号
            if q.chars().all(|c| c.is_ascii_digit()) && q.len() >= 7 {
                return SearchType::PatentNumber;
            }
        }
        
        // 检测是否是人名（中文 2-4 字，或英文 2-3 个单词）
        if !q.is_empty() {
            // 中文人名：2-4 个汉字，不包含空格
            if q.chars().all(|c| c.is_ascii_punctuation() || c.is_ascii_whitespace() || c <= '\u{7F}') == false {
                let chinese_chars: Vec<char> = q.chars().filter(|c| *c > '\u{7F}').collect();
                if chinese_chars.len() >= 2 && chinese_chars.len() <= 5 && !q.contains(' ') {
                    return SearchType::Inventor;
                }
            }
            // 英文人名：2-3 个单词，每个单词首字母大写
            let words: Vec<&str> = q.split_whitespace().collect();
            if words.len() >= 2 && words.len() <= 4 {
                let all_caps = words.iter().all(|w| w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false));
                if all_caps {
                    return SearchType::Inventor;
                }
            }
        }
        
        // 检测是否包含公司后缀
        let company_keywords = ["公司", "集团", "股份", "有限", "责任", "corporation", "corp", "inc", "ltd", "gmbh", "co.", "company"];
        let q_lower = q.to_lowercase();
        if company_keywords.iter().any(|k| q_lower.contains(k)) {
            return SearchType::Applicant;
        }
        
        // 默认使用混合搜索
        SearchType::Mixed
    }

    /// 智能搜索 - 根据搜索类型自动选择最佳搜索策略
    pub fn search_smart(&self, query: &str, search_type: Option<&SearchType>, country: Option<&str>, page: usize, page_size: usize) -> Result<(Vec<PatentSummary>, usize, SearchType)> {
        let detected_type = if let Some(st) = search_type {
            st.clone()
        } else {
            self.detect_search_type(query)
        };
        
        match detected_type {
            SearchType::PatentNumber => {
                // 专利号精确搜索
                self.search_by_patent_number(query, page, page_size)
                    .map(|(p, t)| (p, t, SearchType::PatentNumber))
            }
            SearchType::Applicant => {
                // 申请人搜索
                self.search_by_applicant(query, country, page, page_size)
                    .map(|(p, t)| (p, t, SearchType::Applicant))
            }
            SearchType::Inventor => {
                // 发明人搜索
                self.search_by_inventor(query, country, page, page_size)
                    .map(|(p, t)| (p, t, SearchType::Inventor))
            }
            SearchType::Keyword | SearchType::Mixed => {
                // 关键词/混合搜索
                self.search_like(query, country, page, page_size)
                    .map(|(p, t)| (p, t, detected_type.clone()))
            }
        }
    }

    /// 按专利号搜索
    fn search_by_patent_number(&self, query: &str, page: usize, page_size: usize) -> Result<(Vec<PatentSummary>, usize)> {
        let c = self.conn.lock().unwrap();
        let offset = page.saturating_sub(1) * page_size;
        let q = format!("%{}%", query.replace(" ", ""));
        
        let total: usize = c.prepare("SELECT COUNT(*) FROM patents WHERE REPLACE(patent_number, ' ', '') LIKE ?1")?
            .query_row(params![q], |r| r.get(0))?;
        
        let mut stmt = c.prepare("SELECT id,patent_number,title,abstract_text,applicant,inventor,filing_date,country FROM patents WHERE REPLACE(patent_number, ' ', '') LIKE ?1 ORDER BY filing_date DESC LIMIT ?2 OFFSET ?3")?;
        let rows = stmt.query_map(params![q, page_size as i64, offset as i64], |r| {
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
            })
        })?.filter_map(|r| r.ok()).collect();
        
        Ok((rows, total))
    }

    /// 按申请人搜索（支持模糊匹配）
    fn search_by_applicant(&self, query: &str, country: Option<&str>, page: usize, page_size: usize) -> Result<(Vec<PatentSummary>, usize)> {
        let c = self.conn.lock().unwrap();
        let offset = page.saturating_sub(1) * page_size;
        let q = format!("%{}%", query);
        
        // 计算总数
        let total: usize = if let Some(country_filter) = country {
            c.prepare("SELECT COUNT(*) FROM patents WHERE applicant LIKE ?1 AND country = ?2")?
                .query_row(params![q, country_filter], |r| r.get(0))?
        } else {
            c.prepare("SELECT COUNT(*) FROM patents WHERE applicant LIKE ?1")?
                .query_row(params![q], |r| r.get(0))?
        };
        
        // 获取结果并计算相关性分数 - 使用辅助函数避免生命周期问题
        let rows = if let Some(country_filter) = country {
            self.search_applicant_with_country(&c, &q, country_filter, page_size, offset, query)?
        } else {
            self.search_applicant_without_country(&c, &q, page_size, offset, query)?
        };
        
        Ok((rows, total))
    }

    fn search_applicant_with_country(&self, c: &Connection, q: &str, country: &str, page_size: usize, offset: usize, query: &str) -> Result<Vec<PatentSummary>> {
        let mut stmt = c.prepare("SELECT id,patent_number,title,abstract_text,applicant,inventor,filing_date,country FROM patents WHERE applicant LIKE ?1 AND country = ?2 ORDER BY filing_date DESC LIMIT ?3 OFFSET ?4")?;
        let rows: Vec<PatentSummary> = stmt.query_map(params![q, country, page_size as i64, offset as i64], |row| {
            let applicant = row.get::<_, String>(4).unwrap_or_default();
            let score = calculate_applicant_relevance(query, &applicant);
            Ok(PatentSummary {
                id: row.get(0)?,
                patent_number: row.get(1)?,
                title: row.get(2)?,
                abstract_text: row.get::<_, String>(3).unwrap_or_default(),
                applicant,
                inventor: row.get::<_, String>(5).unwrap_or_default(),
                filing_date: row.get::<_, String>(6).unwrap_or_default(),
                country: row.get::<_, String>(7).unwrap_or_default(),
                relevance_score: Some(score),
            })
        })?.filter_map(|r| r.ok()).collect();
        Ok(rows)
    }

    fn search_applicant_without_country(&self, c: &Connection, q: &str, page_size: usize, offset: usize, query: &str) -> Result<Vec<PatentSummary>> {
        let mut stmt = c.prepare("SELECT id,patent_number,title,abstract_text,applicant,inventor,filing_date,country FROM patents WHERE applicant LIKE ?1 ORDER BY filing_date DESC LIMIT ?2 OFFSET ?3")?;
        let rows: Vec<PatentSummary> = stmt.query_map(params![q, page_size as i64, offset as i64], |row| {
            let applicant = row.get::<_, String>(4).unwrap_or_default();
            let score = calculate_applicant_relevance(query, &applicant);
            Ok(PatentSummary {
                id: row.get(0)?,
                patent_number: row.get(1)?,
                title: row.get(2)?,
                abstract_text: row.get::<_, String>(3).unwrap_or_default(),
                applicant,
                inventor: row.get::<_, String>(5).unwrap_or_default(),
                filing_date: row.get::<_, String>(6).unwrap_or_default(),
                country: row.get::<_, String>(7).unwrap_or_default(),
                relevance_score: Some(score),
            })
        })?.filter_map(|r| r.ok()).collect();
        Ok(rows)
    }

    /// 按发明人搜索（支持模糊匹配）
    fn search_by_inventor(&self, query: &str, country: Option<&str>, page: usize, page_size: usize) -> Result<(Vec<PatentSummary>, usize)> {
        let c = self.conn.lock().unwrap();
        let offset = page.saturating_sub(1) * page_size;
        let q = format!("%{}%", query);
        
        let total: usize = if let Some(country_filter) = country {
            c.prepare("SELECT COUNT(*) FROM patents WHERE inventor LIKE ?1 AND country = ?2")?
                .query_row(params![q, country_filter], |r| r.get(0))?
        } else {
            c.prepare("SELECT COUNT(*) FROM patents WHERE inventor LIKE ?1")?
                .query_row(params![q], |r| r.get(0))?
        };
        
        // 获取结果并计算相关性分数 - 使用辅助函数避免生命周期问题
        let rows = if let Some(country_filter) = country {
            self.search_inventor_with_country(&c, &q, country_filter, page_size, offset, query)?
        } else {
            self.search_inventor_without_country(&c, &q, page_size, offset, query)?
        };
        
        Ok((rows, total))
    }

    fn search_inventor_with_country(&self, c: &Connection, q: &str, country: &str, page_size: usize, offset: usize, query: &str) -> Result<Vec<PatentSummary>> {
        let mut stmt = c.prepare("SELECT id,patent_number,title,abstract_text,applicant,inventor,filing_date,country FROM patents WHERE inventor LIKE ?1 AND country = ?2 ORDER BY filing_date DESC LIMIT ?3 OFFSET ?4")?;
        let rows: Vec<PatentSummary> = stmt.query_map(params![q, country, page_size as i64, offset as i64], |row| {
            let inventor = row.get::<_, String>(5).unwrap_or_default();
            let score = calculate_inventor_relevance(query, &inventor);
            Ok(PatentSummary {
                id: row.get(0)?,
                patent_number: row.get(1)?,
                title: row.get(2)?,
                abstract_text: row.get::<_, String>(3).unwrap_or_default(),
                applicant: row.get::<_, String>(4).unwrap_or_default(),
                inventor,
                filing_date: row.get::<_, String>(6).unwrap_or_default(),
                country: row.get::<_, String>(7).unwrap_or_default(),
                relevance_score: Some(score),
            })
        })?.filter_map(|r| r.ok()).collect();
        Ok(rows)
    }

    fn search_inventor_without_country(&self, c: &Connection, q: &str, page_size: usize, offset: usize, query: &str) -> Result<Vec<PatentSummary>> {
        let mut stmt = c.prepare("SELECT id,patent_number,title,abstract_text,applicant,inventor,filing_date,country FROM patents WHERE inventor LIKE ?1 ORDER BY filing_date DESC LIMIT ?2 OFFSET ?3")?;
        let rows: Vec<PatentSummary> = stmt.query_map(params![q, page_size as i64, offset as i64], |row| {
            let inventor = row.get::<_, String>(5).unwrap_or_default();
            let score = calculate_inventor_relevance(query, &inventor);
            Ok(PatentSummary {
                id: row.get(0)?,
                patent_number: row.get(1)?,
                title: row.get(2)?,
                abstract_text: row.get::<_, String>(3).unwrap_or_default(),
                applicant: row.get::<_, String>(4).unwrap_or_default(),
                inventor,
                filing_date: row.get::<_, String>(6).unwrap_or_default(),
                country: row.get::<_, String>(7).unwrap_or_default(),
                relevance_score: Some(score),
            })
        })?.filter_map(|r| r.ok()).collect();
        Ok(rows)
    }

    pub fn search_fts(&self, query: &str, page: usize, page_size: usize) -> Result<(Vec<PatentSummary>, usize)> {
        let c = self.conn.lock().unwrap();
        let offset = page.saturating_sub(1) * page_size;
        let total: usize = c.prepare("SELECT COUNT(*) FROM patents_fts WHERE patents_fts MATCH ?1")?
            .query_row(params![query], |r| r.get(0)).unwrap_or(0);
        let mut stmt = c.prepare("SELECT p.id,p.patent_number,p.title,p.abstract_text,p.applicant,p.filing_date,p.country FROM patents p INNER JOIN patents_fts f ON p.rowid=f.rowid WHERE patents_fts MATCH ?1 ORDER BY rank LIMIT ?2 OFFSET ?3")?;
        let rows = stmt.query_map(params![query, page_size as i64, offset as i64], |r| {
            Ok(PatentSummary {
                id: r.get(0)?,
                patent_number: r.get(1)?,
                title: r.get(2)?,
                abstract_text: r.get::<_, String>(3).unwrap_or_default(),
                applicant: r.get::<_, String>(4).unwrap_or_default(),
                inventor: String::new(),
                filing_date: r.get::<_, String>(5).unwrap_or_default(),
                country: r.get::<_, String>(6).unwrap_or_default(),
                relevance_score: None,
            })
        })?.filter_map(|r| r.ok()).collect();
        Ok((rows, total))
    }

    pub fn search_like(&self, query: &str, country: Option<&str>, page: usize, page_size: usize) -> Result<(Vec<PatentSummary>, usize)> {
        let c = self.conn.lock().unwrap();
        let offset = page.saturating_sub(1) * page_size;
        let q = format!("%{}%", query);
        let has_country = country.is_some();
        
        // 扩展搜索范围，包含 inventor 字段
        let where_clause = if has_country {
            "WHERE (title LIKE ?1 OR abstract_text LIKE ?1 OR applicant LIKE ?1 OR inventor LIKE ?1 OR patent_number LIKE ?1) AND country=?2"
        } else {
            "WHERE title LIKE ?1 OR abstract_text LIKE ?1 OR applicant LIKE ?1 OR inventor LIKE ?1 OR patent_number LIKE ?1"
        };
        
        let total: usize = if has_country {
            c.prepare(&format!("SELECT COUNT(*) FROM patents {where_clause}"))?.query_row(params![q, country.unwrap()], |r| r.get(0))?
        } else {
            c.prepare(&format!("SELECT COUNT(*) FROM patents {where_clause}"))?.query_row(params![q], |r| r.get(0))?
        };
        
        let rows: Vec<PatentSummary> = if has_country {
            let sql = format!("SELECT id,patent_number,title,abstract_text,applicant,inventor,filing_date,country FROM patents {where_clause} ORDER BY filing_date DESC LIMIT ?3 OFFSET ?4");
            let mut stmt = c.prepare(&sql)?;
            let r = stmt.query_map(params![q, country.unwrap(), page_size as i64, offset as i64], |row| {
                let applicant = row.get::<_, String>(4).unwrap_or_default();
                let inventor = row.get::<_, String>(5).unwrap_or_default();
                let score = calculate_mixed_relevance(query, &applicant, &inventor, &row.get::<_, String>(2).unwrap_or_default());
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
                })
            })?.filter_map(|r| r.ok()).collect();
            r
        } else {
            let sql = format!("SELECT id,patent_number,title,abstract_text,applicant,inventor,filing_date,country FROM patents {where_clause} ORDER BY filing_date DESC LIMIT ?2 OFFSET ?3");
            let mut stmt = c.prepare(&sql)?;
            let r = stmt.query_map(params![q, page_size as i64, offset as i64], |row| {
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
                })
            })?.filter_map(|r| r.ok()).collect();
            r
        };
        Ok((rows, total))
    }

    fn row_to_summary(r: &rusqlite::Row) -> rusqlite::Result<PatentSummary> {
        Ok(PatentSummary {
            id: r.get(0)?,
            patent_number: r.get(1)?,
            title: r.get(2)?,
            abstract_text: r.get::<_, String>(3).unwrap_or_default(),
            applicant: r.get::<_, String>(4).unwrap_or_default(),
            inventor: r.get::<_, String>(5).unwrap_or_default(),
            filing_date: r.get::<_, String>(6).unwrap_or_default(),
            country: r.get::<_, String>(7).unwrap_or_default(),
            relevance_score: None,
        })
    }

    fn row_to_patent(r: &rusqlite::Row) -> Patent {
        Patent {
            id: r.get(0).unwrap_or_default(), patent_number: r.get(1).unwrap_or_default(),
            title: r.get(2).unwrap_or_default(), abstract_text: r.get(3).unwrap_or_default(),
            description: r.get(4).unwrap_or_default(), claims: r.get(5).unwrap_or_default(),
            applicant: r.get(6).unwrap_or_default(), inventor: r.get(7).unwrap_or_default(),
            filing_date: r.get(8).unwrap_or_default(), publication_date: r.get(9).unwrap_or_default(),
            grant_date: r.get(10).ok(), ipc_codes: r.get(11).unwrap_or_default(),
            cpc_codes: r.get(12).unwrap_or_default(), priority_date: r.get(13).unwrap_or_default(),
            country: r.get(14).unwrap_or_default(), kind_code: r.get(15).unwrap_or_default(),
            family_id: r.get(16).ok(), legal_status: r.get(17).unwrap_or_default(),
            citations: r.get(18).unwrap_or_default(), cited_by: r.get(19).unwrap_or_default(),
            source: r.get(20).unwrap_or_default(), raw_json: r.get(21).unwrap_or_default(),
            created_at: r.get(22).unwrap_or_default(),
        }
    }
}

/// 计算申请人相关性分数
/// 精确匹配：100 分
/// 包含查询词：80-99 分（根据位置）
/// 部分匹配：50-79 分
fn calculate_applicant_relevance(query: &str, applicant: &str) -> f64 {
    let q = query.trim().to_lowercase();
    let a = applicant.trim().to_lowercase();
    
    // 精确匹配（包括去除空格后）
    if q == a || q.replace(" ", "") == a.replace(" ", "") {
        return 100.0;
    }
    
    // 完全包含（查询词是申请人的子串）
    if a.starts_with(&q) {
        return 95.0;
    }
    if a.contains(&q) {
        return 90.0;
    }
    
    // 分词匹配（按空格或标点分割）
    let q_words: Vec<&str> = q.split(|c: char| c.is_whitespace() || c == ',' || c == '.').filter(|s| !s.is_empty()).collect();
    let a_words: Vec<&str> = a.split(|c: char| c.is_whitespace() || c == ',' || c == '.').filter(|s| !s.is_empty()).collect();
    
    let mut matched_words = 0;
    for qw in &q_words {
        for aw in &a_words {
            if aw.contains(qw) || qw.contains(aw) {
                matched_words += 1;
                break;
            }
        }
    }
    
    if !q_words.is_empty() {
        let match_ratio = matched_words as f64 / q_words.len() as f64;
        if match_ratio > 0.0 {
            return 50.0 + (match_ratio * 40.0);
        }
    }
    
    // 模糊匹配
    30.0
}

/// 计算发明人相关性分数
fn calculate_inventor_relevance(query: &str, inventor: &str) -> f64 {
    let q = query.trim().to_lowercase();
    let i = inventor.trim().to_lowercase();
    
    // 精确匹配
    if q == i || q.replace(" ", "") == i.replace(" ", "") {
        return 100.0;
    }
    
    // 包含匹配
    if i.contains(&q) {
        return 90.0;
    }
    
    // 中文人名特殊处理：姓 + 名
    let q_chars: Vec<char> = q.chars().filter(|c| *c > '\u{7F}').collect();
    let i_chars: Vec<char> = i.chars().filter(|c| *c > '\u{7F}').collect();
    
    if !q_chars.is_empty() && !i_chars.is_empty() {
        // 姓氏匹配
        if q_chars.first() == i_chars.first() {
            if q_chars.len() <= 2 || i_chars.len() <= 2 {
                return 85.0; // 姓氏相同，可能是简称
            }
        }
        
        // 检查是否包含所有查询字符
        let all_contained = q_chars.iter().all(|qc| i_chars.contains(qc));
        if all_contained {
            return 80.0;
        }
    }
    
    // 分词匹配
    let q_words: Vec<&str> = q.split_whitespace().collect();
    let i_words: Vec<&str> = i.split_whitespace().collect();
    
    for qw in &q_words {
        for iw in &i_words {
            if iw.contains(qw) {
                return 75.0;
            }
        }
    }
    
    30.0
}

/// 计算混合搜索的相关性分数
fn calculate_mixed_relevance(query: &str, applicant: &str, inventor: &str, title: &str) -> f64 {
    let q = query.trim().to_lowercase();
    
    // 申请人匹配权重最高
    let applicant_score = calculate_applicant_relevance(query, applicant);
    if applicant_score >= 90.0 {
        return applicant_score;
    }
    
    // 发明人匹配
    let inventor_score = calculate_inventor_relevance(query, inventor);
    if inventor_score >= 90.0 {
        return inventor_score;
    }
    
    // 标题匹配
    let t = title.trim().to_lowercase();
    if t == q {
        return 95.0;
    }
    if t.starts_with(&q) {
        return 85.0;
    }
    if t.contains(&q) {
        return 75.0;
    }
    
    // 返回最高分
    applicant_score.max(inventor_score).max(40.0)
}
