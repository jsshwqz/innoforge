//! 模块职责：Semantic Scholar 学术文献检索源（v0.9.67，含引用数与高影响力引用数）。
//! 上游调用者：`src/routes/search.rs::online_chain_providers` 与单源补搜 `/api/search/source`。
//! 模块专属禁区：严禁编造 DOI 或引用数；必须显式设 `connect_timeout` + `timeout`，禁 `unwrap()`/`expect()`。

use crate::db::Database;
use crate::patent::Patent;
use crate::search::merge::merged_from;
use crate::search::model::{
    report_excerpt, AttemptReport, AttemptStatus, FailKind, Lang, SearchOutcome, SearchQuery,
    SourceKind,
};
use crate::search::provider::SearchProvider;
use crate::search::relevance::rank_and_gate_hits;
use crate::types::search::PatentSummary;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

const API_ENDPOINT: &str = "https://api.semanticscholar.org/graph/v1/paper/search";
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(15);
const SOURCE_LABEL: &str = "semantic_scholar";
const MAX_AUTHORS: usize = 5;
pub const PER_PAGE: usize = 10;

#[derive(Debug, Clone)]
pub struct SemanticScholarReply {
    pub status: u16,
    pub body: String,
}

#[derive(Debug, Clone)]
pub enum SemanticScholarFault {
    Network(String),
}

pub trait SemanticScholarTransport: Send + Sync {
    fn fetch(
        &self,
        url: String,
        api_key: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<SemanticScholarReply, SemanticScholarFault>> + Send + '_>>;
}

pub struct ReqwestSemanticScholarTransport {
    client: Result<reqwest::Client, String>,
}

impl ReqwestSemanticScholarTransport {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(UPSTREAM_TIMEOUT)
            .user_agent("InnoForge/0.9.67 (Academic Search)")
            .build()
            .map_err(|e| format!("Semantic Scholar HTTP 客户端初始化失败: {e}"));
        Self { client }
    }
}

impl Default for ReqwestSemanticScholarTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticScholarTransport for ReqwestSemanticScholarTransport {
    fn fetch(
        &self,
        url: String,
        api_key: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<SemanticScholarReply, SemanticScholarFault>> + Send + '_>>
    {
        Box::pin(async move {
            let client = self
                .client
                .as_ref()
                .map_err(|e| SemanticScholarFault::Network(e.clone()))?;
            let mut req = client.get(&url);
            if let Some(key) = api_key.filter(|k| !k.trim().is_empty()) {
                req = req.header("x-api-key", key);
            }
            match req.send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let body = resp.text().await.unwrap_or_default();
                    Ok(SemanticScholarReply { status, body })
                }
                Err(e) => Err(SemanticScholarFault::Network(e.to_string())),
            }
        })
    }
}

pub struct SemanticScholarProvider {
    db: Arc<Database>,
    transport: Arc<dyn SemanticScholarTransport>,
    api_key: Option<String>,
}

impl SemanticScholarProvider {
    pub fn new(db: Arc<Database>) -> Self {
        let api_key = std::env::var("S2_API_KEY")
            .ok()
            .filter(|v| !v.trim().is_empty());
        Self {
            db,
            transport: Arc::new(ReqwestSemanticScholarTransport::new()),
            api_key,
        }
    }

    pub fn with_transport(
        db: Arc<Database>,
        transport: Arc<dyn SemanticScholarTransport>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            db,
            transport,
            api_key,
        }
    }

    pub fn request_url(query: &SearchQuery) -> String {
        let q = urlencoding::encode(query.keyword.trim());
        let fields = "paperId,title,abstract,year,authors,venue,citationCount,influentialCitationCount,externalIds,url";
        format!("{API_ENDPOINT}?query={q}&limit={PER_PAGE}&fields={fields}")
    }

    pub fn parse_rows(body: &str) -> Result<(Vec<serde_json::Value>, Option<usize>), String> {
        let json: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| format!("Semantic Scholar JSON 解析失败: {e}"))?;
        let total = json
            .get("total")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
        let arr = json
            .get("data")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "Semantic Scholar 响应缺少 data 数组".to_string())?;
        Ok((arr.clone(), total))
    }

    pub fn paper_to_patent(item: &serde_json::Value) -> Patent {
        let title = item["title"].as_str().unwrap_or("").trim().to_string();
        let paper_id = item["paperId"].as_str().unwrap_or("").trim();
        let doi = item["externalIds"]["DOI"]
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let identifier = doi.unwrap_or_else(|| {
            if !paper_id.is_empty() {
                format!("S2:{paper_id}")
            } else {
                String::new()
            }
        });
        let venue = item["venue"].as_str().unwrap_or("").trim().to_string();
        let year = item["year"]
            .as_i64()
            .map(|y| y.to_string())
            .unwrap_or_default();
        let cites = item["citationCount"].as_u64().unwrap_or(0);
        let influential = item["influentialCitationCount"].as_u64().unwrap_or(0);
        let raw_abs = item["abstract"].as_str().unwrap_or("").trim();

        let impact_prefix = if cites > 0 || influential > 0 {
            format!("[引用 {} / 高影响力引用 {}] ", cites, influential)
        } else {
            String::new()
        };
        let abstract_text = if raw_abs.is_empty() {
            impact_prefix.trim().to_string()
        } else {
            format!("{impact_prefix}{raw_abs}")
        };

        let authors: Vec<String> = item["authors"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|a| a["name"].as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .take(MAX_AUTHORS)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let url = item["url"].as_str().unwrap_or("").trim().to_string();
        Patent {
            id: uuid::Uuid::new_v4().to_string(),
            patent_number: identifier,
            title,
            abstract_text,
            description: String::new(),
            claims: String::new(),
            applicant: venue,
            inventor: authors.join(", "),
            filing_date: year.clone(),
            publication_date: year,
            grant_date: None,
            ipc_codes: String::new(),
            cpc_codes: String::new(),
            priority_date: String::new(),
            country: String::new(),
            kind_code: "ARTICLE".to_string(),
            family_id: None,
            legal_status: format!("Cited:{cites}|Influential:{influential}"),
            citations: "[]".into(),
            cited_by: "[]".into(),
            source: SOURCE_LABEL.into(),
            raw_json: item.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            images: "[]".into(),
            pdf_url: url,
        }
    }

    fn outcome_patents(
        &self,
        query: &SearchQuery,
        rows: Vec<serde_json::Value>,
    ) -> Vec<PatentSummary> {
        let cn_query = matches!(query.language, Some(Lang::Chinese));
        let mut patents = rank_and_gate_hits(
            self.db.as_ref(),
            SOURCE_LABEL,
            query.keyword.trim(),
            cn_query,
            &rows,
            Self::paper_to_patent,
        );
        if query.limit > 0 && patents.len() > query.limit {
            patents.truncate(query.limit);
        }
        patents
    }
}

impl SearchProvider for SemanticScholarProvider {
    fn kind(&self) -> SourceKind {
        SourceKind::SemanticScholar
    }

    fn search<'a>(
        &'a self,
        query: SearchQuery,
    ) -> Pin<Box<dyn Future<Output = SearchOutcome> + Send + 'a>> {
        Box::pin(async move {
            let started = std::time::Instant::now();
            if query.page > 1 {
                return SearchOutcome {
                    results: Vec::new(),
                    attempts: vec![AttemptReport {
                        source: SourceKind::SemanticScholar,
                        status: AttemptStatus::Skipped,
                        latency_ms: 0,
                        hits: 0,
                        error: Some(format!(
                            "Semantic Scholar 学术源不支持翻页，第 {} 页请求已跳过。",
                            query.page
                        )),
                        hint: None,
                    }],
                    upstream_total: None,
                };
            }

            let url = Self::request_url(&query);
            println!("[ONLINE] {SOURCE_LABEL} url={}", report_excerpt(&url));

            let (rows, upstream_total, status, error_note) =
                match self.transport.fetch(url, self.api_key.clone()).await {
                    Ok(reply) if reply.status == 200 => match Self::parse_rows(&reply.body) {
                        Ok((rows, total)) => (rows, total, AttemptStatus::Success, None),
                        Err(e) => (
                            Vec::new(),
                            None,
                            AttemptStatus::Failed(FailKind::Parse),
                            Some(e),
                        ),
                    },
                    Ok(reply) => {
                        let fk = FailKind::for_http_status(reply.status);
                        let msg = format!(
                            "Semantic Scholar HTTP {} ({})",
                            reply.status,
                            report_excerpt(&reply.body)
                        );
                        (Vec::new(), None, AttemptStatus::Failed(fk), Some(msg))
                    }
                    Err(SemanticScholarFault::Network(e)) => (
                        Vec::new(),
                        None,
                        AttemptStatus::Failed(FailKind::Network),
                        Some(format!("Semantic Scholar 网络失败: {}", report_excerpt(&e))),
                    ),
                };

            let patents = if matches!(status, AttemptStatus::Success) {
                self.outcome_patents(&query, rows)
            } else {
                Vec::new()
            };
            let hits = patents.len();

            SearchOutcome {
                results: merged_from(SourceKind::SemanticScholar, patents),
                attempts: vec![AttemptReport {
                    source: SourceKind::SemanticScholar,
                    status,
                    latency_ms: started.elapsed().as_millis() as u64,
                    hits,
                    error: error_note,
                    hint: None,
                }],
                upstream_total,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::model::Lang;
    use crate::types::search::SearchType;

    struct StubTransport {
        reply: Result<SemanticScholarReply, SemanticScholarFault>,
    }

    impl SemanticScholarTransport for StubTransport {
        fn fetch(
            &self,
            _url: String,
            _api_key: Option<String>,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<SemanticScholarReply, SemanticScholarFault>> + Send + '_,
            >,
        > {
            let r = self.reply.clone();
            Box::pin(async move { r })
        }
    }

    #[tokio::test]
    async fn parses_semantic_scholar_with_doi_and_influential_citations() {
        let db = Arc::new(Database::init(":memory:").unwrap());
        let body = r#"{
            "total": 42,
            "data": [{
                "paperId": "abc123def",
                "title": "Direct Oil Immersion Cooling for High Power Density Electric Motors",
                "abstract": "Experimental thermal validation of direct dielectric oil immersion cooling in traction motor stator windings.",
                "year": 2025,
                "venue": "IEEE Transactions on Industrial Electronics",
                "citationCount": 68,
                "influentialCitationCount": 14,
                "externalIds": { "DOI": "10.1109/TIE.2025.0001234" },
                "authors": [{"name": "Alice Chen"}, {"name": "Bob Zhang"}],
                "url": "https://www.semanticscholar.org/paper/abc123def"
            }]
        }"#;
        let provider = SemanticScholarProvider::with_transport(
            db,
            Arc::new(StubTransport {
                reply: Ok(SemanticScholarReply {
                    status: 200,
                    body: body.into(),
                }),
            }),
            None,
        );
        let q = SearchQuery {
            keyword: "immersion cooling electric motor".into(),
            country: None,
            language: Some(Lang::English),
            assignee: None,
            exact_assignee: false,
            date_from: None,
            date_to: None,
            limit: 10,
            page: 1,
            sort_by: None,
            search_type: Some(SearchType::Keyword),
        };
        let out = provider.search(q).await;
        assert_eq!(out.attempts[0].status, AttemptStatus::Success);
        assert_eq!(out.upstream_total, Some(42));
        assert_eq!(out.results.len(), 1);
        let item = &out.results[0].summary;
        assert_eq!(item.patent_number, "10.1109/TIE.2025.0001234");
        assert!(item.abstract_text.contains("引用 68 / 高影响力引用 14"));
        assert_eq!(
            item.applicant,
            "IEEE Transactions on Industrial Electronics"
        );
    }

    #[tokio::test]
    async fn semantic_scholar_rate_limit_429_reports_quota_failure() {
        let db = Arc::new(Database::init(":memory:").unwrap());
        let provider = SemanticScholarProvider::with_transport(
            db,
            Arc::new(StubTransport {
                reply: Ok(SemanticScholarReply {
                    status: 429,
                    body: "Too Many Requests".into(),
                }),
            }),
            None,
        );
        let q = SearchQuery {
            keyword: "solid state battery".into(),
            country: None,
            language: Some(Lang::English),
            assignee: None,
            exact_assignee: false,
            date_from: None,
            date_to: None,
            limit: 10,
            page: 1,
            sort_by: None,
            search_type: Some(SearchType::Keyword),
        };
        let out = provider.search(q).await;
        assert_eq!(
            out.attempts[0].status,
            AttemptStatus::Failed(FailKind::Quota)
        );
        assert!(out.results.is_empty());
    }
}
