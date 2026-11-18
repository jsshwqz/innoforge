//! 专利检索源适配器（`patent_sources.rs`）
//!
//! 支持：
//! 1. [`SerpApiPatentAdapter`]：对接可配置专利 HTTP JSON 端点（默认 SerpAPI Google Patents 格式），
//!    支持自定义 `base_url` 以便离线单测打本地 mock server；
//! 2. [`LocalSqlitePatentAdapter`]：基于 `tokio::task::spawn_blocking` 只读检索本机 SQLite
//!    `knowledge_docs` / `patents` 表（不阻塞异步运行时）。

use std::path::PathBuf;

use rusqlite::OpenFlags;
use serde_json::Value;

use crate::core::vault::models::{
    canonical_patent_key, AssetRole, DocKind, FilingStatus, KnowledgeDoc, OmniSearchHit,
    SearchRealm,
};

use super::query_planner::QueryPlan;
use super::{build_http_client, BoxFuture, OmniSourceAdapter};

const DEFAULT_SERPAPI_BASE: &str = "https://serpapi.com";

/// 可配置专利 HTTP JSON 检索适配器（兼容 SerpAPI Google Patents 格式）
pub struct SerpApiPatentAdapter {
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl SerpApiPatentAdapter {
    pub fn new(api_key: Option<String>) -> anyhow::Result<Self> {
        Self::with_base_url(DEFAULT_SERPAPI_BASE, api_key)
    }

    pub fn with_base_url(
        base_url: impl Into<String>,
        api_key: Option<String>,
    ) -> anyhow::Result<Self> {
        let cleaned_key = api_key.and_then(|k| {
            let t = k.trim().to_string();
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        });
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: cleaned_key,
            client: build_http_client()?,
        })
    }
}

impl OmniSourceAdapter for SerpApiPatentAdapter {
    fn name(&self) -> &'static str {
        "serpapi_patent"
    }

    fn realm(&self) -> SearchRealm {
        SearchRealm::Patent
    }

    fn search<'a>(
        &'a self,
        plan: &'a QueryPlan,
        max_results: usize,
    ) -> BoxFuture<'a, anyhow::Result<Vec<OmniSearchHit>>> {
        Box::pin(async move {
            let q = if plan.patent_boolean_query.trim().is_empty() {
                plan.raw_query.trim()
            } else {
                plan.patent_boolean_query.trim()
            };
            if q.is_empty() || max_results == 0 {
                return Ok(Vec::new());
            }

            let num = max_results.min(50);
            let mut url = format!(
                "{}/search.json?engine=google_patents&q={}&num={}",
                self.base_url,
                urlencoding::encode(q),
                num
            );
            if let Some(ref key) = self.api_key {
                url.push_str("&api_key=");
                url.push_str(&urlencoding::encode(key));
            }

            let resp = self.client.get(&url).send().await?.error_for_status()?;
            let body: Value = resp.json().await?;
            let items = body
                .get("organic_results")
                .or_else(|| body.get("results"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            let mut hits = Vec::with_capacity(items.len());
            for (idx, item) in items.into_iter().take(max_results).enumerate() {
                let title = item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if title.is_empty() {
                    continue;
                }

                let raw_pub_no = item
                    .get("publication_number")
                    .or_else(|| item.get("patent_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let cleaned_pub_no = raw_pub_no
                    .strip_prefix("patent/")
                    .unwrap_or(raw_pub_no)
                    .split('/')
                    .next()
                    .unwrap_or(raw_pub_no)
                    .to_string();

                let canonical = canonical_patent_key(&cleaned_pub_no);
                let external_id = if !cleaned_pub_no.is_empty() {
                    cleaned_pub_no.clone()
                } else {
                    canonical.clone()
                };

                let doc_id = if !canonical.is_empty() {
                    format!("patent:{canonical}")
                } else if !external_id.is_empty() {
                    format!("patent:{external_id}")
                } else {
                    format!("patent:serpapi:{idx}")
                };

                let snippet = item
                    .get("snippet")
                    .or_else(|| item.get("abstract"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();

                let assignee = item
                    .get("assignee")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let inventor = item
                    .get("inventor")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let authors_or_assignees = match (assignee.is_empty(), inventor.is_empty()) {
                    (false, false) => format!("{assignee} / {inventor}"),
                    (false, true) => assignee.to_string(),
                    (true, false) => inventor.to_string(),
                    (true, true) => String::new(),
                };

                let pub_date = item
                    .get("publication_date")
                    .or_else(|| item.get("filing_date"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();

                let source_url = item
                    .get("patent_link")
                    .or_else(|| item.get("link"))
                    .or_else(|| item.get("serpapi_link"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();

                let (title_zh, title_en) = if contains_cjk(&title) {
                    (title, String::new())
                } else {
                    (String::new(), title)
                };

                hits.push(OmniSearchHit {
                    doc: KnowledgeDoc {
                        doc_id,
                        doc_kind: DocKind::Patent,
                        asset_role: AssetRole::GeneralRef,
                        external_id,
                        title_zh,
                        title_en,
                        authors_or_assignees,
                        pub_or_filing_date: pub_date,
                        venue_or_authority: "Google Patents".to_string(),
                        filing_status: FilingStatus::PublicDisclosed,
                        doi_verified: false,
                        bibtex_raw: String::new(),
                        abstract_text: snippet,
                        claims_json: String::new(),
                        fulltext_raw: String::new(),
                        fulltext_status: "metadata_only".to_string(),
                        chunk_ver: 0,
                    },
                    realm: SearchRealm::Patent,
                    source_provider: "serpapi_patent".to_string(),
                    relevance_score: (0.86 - (idx as f64) * 0.02).max(0.1),
                    match_reason: "全球专利库检索命中".to_string(),
                    source_url,
                });
            }

            Ok(hits)
        })
    }
}

/// 本机 SQLite 专利库只读检索适配器（支持 `knowledge_docs` 与兼容存量 `patents` 表）
pub struct LocalSqlitePatentAdapter {
    db_path: PathBuf,
}

impl LocalSqlitePatentAdapter {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
        }
    }
}

impl OmniSourceAdapter for LocalSqlitePatentAdapter {
    fn name(&self) -> &'static str {
        "local_sqlite_patent"
    }

    fn realm(&self) -> SearchRealm {
        SearchRealm::Patent
    }

    fn search<'a>(
        &'a self,
        plan: &'a QueryPlan,
        max_results: usize,
    ) -> BoxFuture<'a, anyhow::Result<Vec<OmniSearchHit>>> {
        Box::pin(async move {
            if max_results == 0 || !self.db_path.exists() {
                return Ok(Vec::new());
            }
            let db_path = self.db_path.clone();
            let keywords = if plan.keywords.is_empty() {
                vec![plan.raw_query.trim().to_string()]
            } else {
                plan.keywords.clone()
            };

            let hits = tokio::task::spawn_blocking(move || {
                query_local_sqlite_patents(&db_path, &keywords, max_results)
            })
            .await
            .map_err(|err| anyhow::anyhow!("local sqlite patent task join failed: {err}"))??;

            Ok(hits)
        })
    }
}

fn query_local_sqlite_patents(
    db_path: &PathBuf,
    keywords: &[String],
    max_results: usize,
) -> anyhow::Result<Vec<OmniSearchHit>> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let first_kw = keywords
        .first()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("");
    if first_kw.is_empty() {
        return Ok(Vec::new());
    }
    let pattern = format!("%{first_kw}%");
    let limit = max_results as i64;

    // 优先查询 v1.0 统一 knowledge_docs 表
    let has_knowledge_docs: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='knowledge_docs')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if has_knowledge_docs {
        let mut stmt = conn.prepare(
            "SELECT doc_id, asset_role, external_id, title_zh, title_en, \
             authors_or_assignees, pub_or_filing_date, venue_or_authority, \
             filing_status, abstract_text \
             FROM knowledge_docs \
             WHERE doc_kind = 'patent' \
               AND (title_zh LIKE ?1 OR title_en LIKE ?1 OR abstract_text LIKE ?1 OR external_id LIKE ?1) \
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![pattern, limit], |row| {
            let doc_id: String = row.get(0)?;
            let asset_role_str: String = row.get(1)?;
            let external_id: String = row.get(2)?;
            let title_zh: String = row.get(3)?;
            let title_en: String = row.get(4)?;
            let authors_or_assignees: String = row.get(5)?;
            let pub_or_filing_date: String = row.get(6)?;
            let venue_or_authority: String = row.get(7)?;
            let filing_status_str: String = row.get(8)?;
            let abstract_text: String = row.get(9)?;
            Ok(KnowledgeDoc {
                doc_id,
                doc_kind: DocKind::Patent,
                asset_role: AssetRole::parse(&asset_role_str),
                external_id,
                title_zh,
                title_en,
                authors_or_assignees,
                pub_or_filing_date,
                venue_or_authority,
                filing_status: FilingStatus::parse(&filing_status_str),
                doi_verified: false,
                bibtex_raw: String::new(),
                abstract_text,
                claims_json: String::new(),
                fulltext_raw: String::new(),
                fulltext_status: "local_vault".to_string(),
                chunk_ver: 0,
            })
        })?;

        let mut hits = Vec::new();
        for (idx, doc_res) in rows.enumerate() {
            if let Ok(doc) = doc_res {
                hits.push(OmniSearchHit {
                    doc,
                    realm: SearchRealm::Patent,
                    source_provider: "local_sqlite_patent".to_string(),
                    relevance_score: (0.90 - (idx as f64) * 0.02).max(0.1),
                    match_reason: "本机知识金库专利命中".to_string(),
                    source_url: String::new(),
                });
            }
        }
        return Ok(hits);
    }

    // 兼容存量 patents 表
    let has_patents: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='patents')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if !has_patents {
        return Ok(Vec::new());
    }

    let mut stmt = conn.prepare(
        "SELECT patent_number, title, abstract_text, applicant, publication_date, country \
         FROM patents \
         WHERE title LIKE ?1 OR abstract_text LIKE ?1 OR patent_number LIKE ?1 \
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![pattern, limit], |row| {
        let patent_number: String = row.get(0)?;
        let title: String = row.get(1)?;
        let abstract_text: String = row.get(2)?;
        let applicant: String = row.get(3)?;
        let publication_date: String = row.get(4)?;
        let country: String = row.get(5)?;
        let canonical = canonical_patent_key(&patent_number);
        let doc_id = if canonical.is_empty() {
            format!("patent:{patent_number}")
        } else {
            format!("patent:{canonical}")
        };
        let (title_zh, title_en) = if contains_cjk(&title) {
            (title, String::new())
        } else {
            (String::new(), title)
        };
        Ok(KnowledgeDoc {
            doc_id,
            doc_kind: DocKind::Patent,
            asset_role: AssetRole::GeneralRef,
            external_id: patent_number,
            title_zh,
            title_en,
            authors_or_assignees: applicant,
            pub_or_filing_date: publication_date,
            venue_or_authority: country,
            filing_status: FilingStatus::PublicDisclosed,
            doi_verified: false,
            bibtex_raw: String::new(),
            abstract_text,
            claims_json: String::new(),
            fulltext_raw: String::new(),
            fulltext_status: "local_legacy".to_string(),
            chunk_ver: 0,
        })
    })?;

    let mut hits = Vec::new();
    for (idx, doc_res) in rows.enumerate() {
        if let Ok(doc) = doc_res {
            hits.push(OmniSearchHit {
                doc,
                realm: SearchRealm::Patent,
                source_provider: "local_sqlite_patent".to_string(),
                relevance_score: (0.88 - (idx as f64) * 0.02).max(0.1),
                match_reason: "本机存量专利库命中".to_string(),
                source_url: String::new(),
            });
        }
    }
    Ok(hits)
}

fn contains_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))
}
