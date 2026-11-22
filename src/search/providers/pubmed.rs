//! 模块职责：NCBI PubMed 生物医学与医工交叉学术检索源（v0.9.67，免费无 Key E-utilities）。
//! 上游调用者：`src/routes/search.rs::online_chain_providers` 与单源补搜 `/api/search/source`。
//! 模块专属禁区：严禁编造 PMID/DOI；必须显式设 `connect_timeout` + `timeout`，禁 `unwrap()`/`expect()`。

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

const ESEARCH_ENDPOINT: &str = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi";
const ESUMMARY_ENDPOINT: &str = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi";
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(15);
const SOURCE_LABEL: &str = "pubmed";
const MAX_AUTHORS: usize = 5;
pub const PER_PAGE: usize = 10;

#[derive(Debug, Clone)]
pub struct PubMedReply {
    pub status: u16,
    pub body: String,
}

#[derive(Debug, Clone)]
pub enum PubMedFault {
    Network(String),
}

pub trait PubMedTransport: Send + Sync {
    fn fetch(
        &self,
        url: String,
    ) -> Pin<Box<dyn Future<Output = Result<PubMedReply, PubMedFault>> + Send + '_>>;
}

pub struct ReqwestPubMedTransport {
    client: Result<reqwest::Client, String>,
}

impl ReqwestPubMedTransport {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(UPSTREAM_TIMEOUT)
            .user_agent("InnoForge/0.9.67 (Biomedical Academic Search)")
            .build()
            .map_err(|e| format!("PubMed HTTP 客户端初始化失败: {e}"));
        Self { client }
    }
}

impl Default for ReqwestPubMedTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl PubMedTransport for ReqwestPubMedTransport {
    fn fetch(
        &self,
        url: String,
    ) -> Pin<Box<dyn Future<Output = Result<PubMedReply, PubMedFault>> + Send + '_>> {
        Box::pin(async move {
            let client = self
                .client
                .as_ref()
                .map_err(|e| PubMedFault::Network(e.clone()))?;
            match client.get(&url).send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let body = resp.text().await.unwrap_or_default();
                    Ok(PubMedReply { status, body })
                }
                Err(e) => Err(PubMedFault::Network(e.to_string())),
            }
        })
    }
}

pub struct PubMedProvider {
    db: Arc<Database>,
    transport: Arc<dyn PubMedTransport>,
}

impl PubMedProvider {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            transport: Arc::new(ReqwestPubMedTransport::new()),
        }
    }

    pub fn with_transport(db: Arc<Database>, transport: Arc<dyn PubMedTransport>) -> Self {
        Self { db, transport }
    }

    pub fn esearch_url(query: &SearchQuery) -> String {
        let q = urlencoding::encode(query.keyword.trim());
        format!("{ESEARCH_ENDPOINT}?db=pubmed&term={q}&retmax={PER_PAGE}&retmode=json")
    }

    pub fn esummary_url(pmids: &[String]) -> String {
        let ids = pmids.join(",");
        format!("{ESUMMARY_ENDPOINT}?db=pubmed&id={ids}&retmode=json")
    }

    pub fn parse_esearch(body: &str) -> Result<(Vec<String>, Option<usize>), String> {
        let json: serde_json::Value =
            serde_json::from_str(body).map_err(|e| format!("PubMed esearch JSON 解析失败: {e}"))?;
        let res = json
            .get("esearchresult")
            .ok_or_else(|| "PubMed esearch 响应缺少 esearchresult".to_string())?;
        let count = res["count"].as_str().and_then(|s| s.parse::<usize>().ok());
        let ids = res["idlist"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        Ok((ids, count))
    }

    pub fn parse_esummary_rows(
        body: &str,
        pmids: &[String],
    ) -> Result<Vec<serde_json::Value>, String> {
        let json: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| format!("PubMed esummary JSON 解析失败: {e}"))?;
        let result = json
            .get("result")
            .ok_or_else(|| "PubMed esummary 响应缺少 result".to_string())?;
        let mut out = Vec::new();
        for pmid in pmids {
            if let Some(item) = result.get(pmid) {
                let mut obj = item.clone();
                if let Some(map) = obj.as_object_mut() {
                    map.insert("_pmid".to_string(), serde_json::Value::String(pmid.clone()));
                }
                out.push(obj);
            }
        }
        Ok(out)
    }

    pub fn summary_to_patent(item: &serde_json::Value) -> Patent {
        let pmid = item["_pmid"]
            .as_str()
            .or_else(|| item["uid"].as_str())
            .unwrap_or("")
            .trim();
        let title = item["title"]
            .as_str()
            .unwrap_or("")
            .trim()
            .trim_end_matches('.')
            .to_string();
        let journal = item["fulljournalname"]
            .as_str()
            .or_else(|| item["source"].as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let pubdate = item["pubdate"].as_str().unwrap_or("").trim().to_string();
        let year: String = pubdate.chars().take(4).collect();
        let doi = item["articleids"].as_array().and_then(|arr| {
            arr.iter().find_map(|id| {
                if id["idtype"].as_str() == Some("doi") {
                    id["value"]
                        .as_str()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                } else {
                    None
                }
            })
        });
        let identifier = doi.clone().unwrap_or_else(|| format!("PMID:{pmid}"));
        let authors: Vec<String> = item["authors"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a["name"].as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .take(MAX_AUTHORS)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let pubtypes: Vec<&str> = item["pubtype"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        let mut snippet_parts = vec![format!("PubMed PMID:{pmid}")];
        if let Some(d) = doi {
            snippet_parts.push(format!("DOI:{d}"));
        }
        if !journal.is_empty() {
            snippet_parts.push(format!("期刊: {journal}"));
        }
        if !pubtypes.is_empty() {
            snippet_parts.push(format!("类型: {}", pubtypes.join("/")));
        }
        if !pubdate.is_empty() {
            snippet_parts.push(format!("发表: {pubdate}"));
        }

        Patent {
            id: uuid::Uuid::new_v4().to_string(),
            patent_number: identifier,
            title,
            abstract_text: snippet_parts.join(" · "),
            description: String::new(),
            claims: String::new(),
            applicant: journal,
            inventor: authors.join(", "),
            filing_date: year,
            publication_date: pubdate,
            grant_date: None,
            ipc_codes: String::new(),
            cpc_codes: String::new(),
            priority_date: String::new(),
            country: String::new(),
            kind_code: "PUBMED".to_string(),
            family_id: None,
            legal_status: "Peer-Reviewed (PubMed)".to_string(),
            citations: "[]".into(),
            cited_by: "[]".into(),
            source: SOURCE_LABEL.into(),
            raw_json: item.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            images: "[]".into(),
            pdf_url: format!("https://pubmed.ncbi.nlm.nih.gov/{pmid}/"),
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
            Self::summary_to_patent,
        );
        if query.limit > 0 && patents.len() > query.limit {
            patents.truncate(query.limit);
        }
        patents
    }
}

impl SearchProvider for PubMedProvider {
    fn kind(&self) -> SourceKind {
        SourceKind::PubMed
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
                        source: SourceKind::PubMed,
                        status: AttemptStatus::Skipped,
                        latency_ms: 0,
                        hits: 0,
                        error: Some(format!(
                            "PubMed 学术源不支持翻页，第 {} 页请求已跳过。",
                            query.page
                        )),
                        hint: None,
                    }],
                    upstream_total: None,
                };
            }

            let search_url = Self::esearch_url(&query);
            println!(
                "[ONLINE] {SOURCE_LABEL} url={}",
                report_excerpt(&search_url)
            );

            let (pmids, upstream_total) = match self.transport.fetch(search_url).await {
                Ok(reply) if reply.status == 200 => match Self::parse_esearch(&reply.body) {
                    Ok(v) => v,
                    Err(e) => {
                        return SearchOutcome {
                            results: Vec::new(),
                            attempts: vec![AttemptReport {
                                source: SourceKind::PubMed,
                                status: AttemptStatus::Failed(FailKind::Parse),
                                latency_ms: started.elapsed().as_millis() as u64,
                                hits: 0,
                                error: Some(e),
                                hint: None,
                            }],
                            upstream_total: None,
                        };
                    }
                },
                Ok(reply) => {
                    return SearchOutcome {
                        results: Vec::new(),
                        attempts: vec![AttemptReport {
                            source: SourceKind::PubMed,
                            status: AttemptStatus::Failed(FailKind::for_http_status(reply.status)),
                            latency_ms: started.elapsed().as_millis() as u64,
                            hits: 0,
                            error: Some(format!("PubMed esearch HTTP {}", reply.status)),
                            hint: None,
                        }],
                        upstream_total: None,
                    };
                }
                Err(PubMedFault::Network(e)) => {
                    return SearchOutcome {
                        results: Vec::new(),
                        attempts: vec![AttemptReport {
                            source: SourceKind::PubMed,
                            status: AttemptStatus::Failed(FailKind::Network),
                            latency_ms: started.elapsed().as_millis() as u64,
                            hits: 0,
                            error: Some(format!("PubMed 网络失败: {}", report_excerpt(&e))),
                            hint: None,
                        }],
                        upstream_total: None,
                    };
                }
            };

            if pmids.is_empty() {
                return SearchOutcome {
                    results: Vec::new(),
                    attempts: vec![AttemptReport {
                        source: SourceKind::PubMed,
                        status: AttemptStatus::Success,
                        latency_ms: started.elapsed().as_millis() as u64,
                        hits: 0,
                        error: None,
                        hint: None,
                    }],
                    upstream_total,
                };
            }

            let summary_url = Self::esummary_url(&pmids);
            let (rows, status, error_note) = match self.transport.fetch(summary_url).await {
                Ok(reply) if reply.status == 200 => {
                    match Self::parse_esummary_rows(&reply.body, &pmids) {
                        Ok(rows) => (rows, AttemptStatus::Success, None),
                        Err(e) => (Vec::new(), AttemptStatus::Failed(FailKind::Parse), Some(e)),
                    }
                }
                Ok(reply) => (
                    Vec::new(),
                    AttemptStatus::Failed(FailKind::for_http_status(reply.status)),
                    Some(format!("PubMed esummary HTTP {}", reply.status)),
                ),
                Err(PubMedFault::Network(e)) => (
                    Vec::new(),
                    AttemptStatus::Failed(FailKind::Network),
                    Some(format!("PubMed esummary 网络失败: {}", report_excerpt(&e))),
                ),
            };

            let patents = if matches!(status, AttemptStatus::Success) {
                self.outcome_patents(&query, rows)
            } else {
                Vec::new()
            };
            let hits = patents.len();

            SearchOutcome {
                results: merged_from(SourceKind::PubMed, patents),
                attempts: vec![AttemptReport {
                    source: SourceKind::PubMed,
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

    struct DualStubTransport {
        esearch_body: String,
        esummary_body: String,
    }

    impl PubMedTransport for DualStubTransport {
        fn fetch(
            &self,
            url: String,
        ) -> Pin<Box<dyn Future<Output = Result<PubMedReply, PubMedFault>> + Send + '_>> {
            let body = if url.contains("esearch.fcgi") {
                self.esearch_body.clone()
            } else {
                self.esummary_body.clone()
            };
            Box::pin(async move { Ok(PubMedReply { status: 200, body }) })
        }
    }

    #[tokio::test]
    async fn pubmed_two_step_esearch_and_esummary_extracts_real_doi_and_journal() {
        let db = Arc::new(Database::init(":memory:").unwrap());
        let esearch = r#"{"esearchresult":{"count":"128","idlist":["39001122"]}}"#;
        let esummary = r#"{
            "result": {
                "uids": ["39001122"],
                "39001122": {
                    "uid": "39001122",
                    "title": "Thermal ablation catheter cooling optimization for cardiac arrhythmia.",
                    "fulljournalname": "Journal of Biomedical Engineering",
                    "pubdate": "2025 Mar",
                    "authors": [{"name": "Li W"}, {"name": "Wang Y"}],
                    "pubtype": ["Journal Article"],
                    "articleids": [
                        {"idtype": "pubmed", "value": "39001122"},
                        {"idtype": "doi", "value": "10.1016/j.jbe.2025.100099"}
                    ]
                }
            }
        }"#;
        let provider = PubMedProvider::with_transport(
            db,
            Arc::new(DualStubTransport {
                esearch_body: esearch.into(),
                esummary_body: esummary.into(),
            }),
        );
        let q = SearchQuery {
            keyword: "thermal ablation catheter cooling".into(),
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
        assert_eq!(out.upstream_total, Some(128));
        assert_eq!(out.results.len(), 1);
        let item = &out.results[0].summary;
        assert_eq!(item.patent_number, "10.1016/j.jbe.2025.100099");
        assert_eq!(item.applicant, "Journal of Biomedical Engineering");
        assert!(item.abstract_text.contains("PMID:39001122"));
    }
}
