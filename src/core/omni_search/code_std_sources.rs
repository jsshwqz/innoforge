//! 开源代码工程与行业标准检索适配器（`code_std_sources.rs`）
//!
//! 实现：
//! 1. [`OpenSourceCodeAdapter`]：对接 GitHub Repositories Search API 兼容 JSON
//!    （`/search/repositories?q=...`），提取仓库信息与开源协议并内置 `GPL/AGPL` 传染性风险提示；
//! 2. [`TechStandardAdapter`]：检索行业标准与白皮书（ISO / IEEE / IEC / GB/T 元数据端点）。

use serde_json::Value;

use crate::core::vault::models::{
    AssetRole, DocKind, FilingStatus, KnowledgeDoc, OmniSearchHit, SearchRealm,
};

use super::query_planner::QueryPlan;
use super::{build_http_client, BoxFuture, OmniSourceAdapter};

const DEFAULT_GITHUB_BASE: &str = "https://api.github.com";
const DEFAULT_STANDARD_BASE: &str = "https://api.crossref.org";

/// 开源代码工程检索适配器（GitHub Search API 兼容）
pub struct OpenSourceCodeAdapter {
    base_url: String,
    token: Option<String>,
    client: reqwest::Client,
}

impl OpenSourceCodeAdapter {
    pub fn new() -> anyhow::Result<Self> {
        Self::with_base_url(DEFAULT_GITHUB_BASE)
    }

    pub fn with_base_url(base_url: impl Into<String>) -> anyhow::Result<Self> {
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token: None,
            client: build_http_client()?,
        })
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        let t = token.into().trim().to_string();
        if !t.is_empty() {
            self.token = Some(t);
        }
        self
    }
}

impl OmniSourceAdapter for OpenSourceCodeAdapter {
    fn name(&self) -> &'static str {
        "github_code"
    }

    fn realm(&self) -> SearchRealm {
        SearchRealm::CodeRepo
    }

    fn search<'a>(
        &'a self,
        plan: &'a QueryPlan,
        max_results: usize,
    ) -> BoxFuture<'a, anyhow::Result<Vec<OmniSearchHit>>> {
        Box::pin(async move {
            let q = if plan.code_query.trim().is_empty() {
                plan.raw_query.trim()
            } else {
                plan.code_query.trim()
            };
            if q.is_empty() || max_results == 0 {
                return Ok(Vec::new());
            }

            let per_page = max_results.min(30);
            let url = format!(
                "{}/search/repositories?q={}&per_page={}",
                self.base_url,
                urlencoding::encode(q),
                per_page
            );

            let mut req = self
                .client
                .get(&url)
                .header("Accept", "application/vnd.github+json");
            if let Some(ref token) = self.token {
                req = req.bearer_auth(token);
            }

            let resp = req.send().await?.error_for_status()?;
            let body: Value = resp.json().await?;
            let items = body
                .get("items")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            let mut hits = Vec::with_capacity(items.len());
            for (idx, item) in items.into_iter().take(max_results).enumerate() {
                let full_name = item
                    .get("full_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if full_name.is_empty() {
                    continue;
                }

                let description = item
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let html_url = item
                    .get("html_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let stars = item
                    .get("stargazers_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let updated_at = item
                    .get("updated_at")
                    .and_then(|v| v.as_str())
                    .map(|s| s.chars().take(10).collect())
                    .unwrap_or_default();
                let language = item
                    .get("language")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();

                let spdx_id = item
                    .pointer("/license/spdx_id")
                    .or_else(|| item.pointer("/license/name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .trim()
                    .to_string();

                let license_risk = classify_license_risk(&spdx_id);
                let owner = full_name.split('/').next().unwrap_or("").to_string();

                let abstract_text = if description.is_empty() {
                    format!("Stars: {stars} | Language: {language} | License: {spdx_id}")
                } else {
                    format!(
                        "{description} (Stars: {stars}, Language: {language}, License: {spdx_id})"
                    )
                };

                let match_reason = match license_risk {
                    Some(risk) => format!("开源代码命中 ({spdx_id}, Stars: {stars}) · {risk}"),
                    None => format!("开源代码命中 ({spdx_id}, Stars: {stars})"),
                };

                let venue = match license_risk {
                    Some(risk) => format!("GitHub ({spdx_id} · {risk})"),
                    None => format!("GitHub ({spdx_id})"),
                };

                hits.push(OmniSearchHit {
                    doc: KnowledgeDoc {
                        doc_id: format!("code:{full_name}"),
                        doc_kind: DocKind::CodeRepo,
                        asset_role: AssetRole::GeneralRef,
                        external_id: full_name.clone(),
                        title_zh: String::new(),
                        title_en: full_name,
                        authors_or_assignees: owner,
                        pub_or_filing_date: updated_at,
                        venue_or_authority: venue,
                        filing_status: FilingStatus::PublicDisclosed,
                        doi_verified: false,
                        bibtex_raw: String::new(),
                        abstract_text,
                        claims_json: String::new(),
                        fulltext_raw: String::new(),
                        fulltext_status: "metadata_only".to_string(),
                        chunk_ver: 0,
                    },
                    realm: SearchRealm::CodeRepo,
                    source_provider: "github_code".to_string(),
                    relevance_score: (0.78 + ((stars as f64).log10().max(0.0) * 0.02)
                        - (idx as f64) * 0.02)
                        .clamp(0.1, 0.95),
                    match_reason,
                    source_url: html_url,
                });
            }

            Ok(hits)
        })
    }
}

fn classify_license_risk(spdx_id: &str) -> Option<&'static str> {
    let upper = spdx_id.to_uppercase();
    if upper.contains("AGPL") {
        Some("AGPL 强传染性许可证风险提示：网络服务分发亦需开源")
    } else if upper.contains("GPL") && !upper.contains("LGPL") {
        Some("GPL 传染性许可证风险提示：闭源商业集成需隔离或更换")
    } else if upper.contains("LGPL") {
        Some("LGPL 弱传染性提示：静态链接需注意开源合规")
    } else {
        None
    }
}

/// 行业标准与技术白皮书检索适配器（ISO / IEEE / IEC / GB/T 元数据端点）
pub struct TechStandardAdapter {
    base_url: String,
    client: reqwest::Client,
}

impl TechStandardAdapter {
    pub fn new() -> anyhow::Result<Self> {
        Self::with_base_url(DEFAULT_STANDARD_BASE)
    }

    pub fn with_base_url(base_url: impl Into<String>) -> anyhow::Result<Self> {
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: build_http_client()?,
        })
    }
}

impl OmniSourceAdapter for TechStandardAdapter {
    fn name(&self) -> &'static str {
        "tech_standard"
    }

    fn realm(&self) -> SearchRealm {
        SearchRealm::Standard
    }

    fn search<'a>(
        &'a self,
        plan: &'a QueryPlan,
        max_results: usize,
    ) -> BoxFuture<'a, anyhow::Result<Vec<OmniSearchHit>>> {
        Box::pin(async move {
            let q = if plan.standard_query.trim().is_empty() {
                plan.raw_query.trim()
            } else {
                plan.standard_query.trim()
            };
            if q.is_empty() || max_results == 0 {
                return Ok(Vec::new());
            }

            let limit = max_results.min(30);
            let url = format!(
                "{}/standards/search?q={}&limit={}",
                self.base_url,
                urlencoding::encode(q),
                limit
            );

            let resp = self.client.get(&url).send().await?.error_for_status()?;
            let body: Value = resp.json().await?;
            let items = body
                .get("items")
                .or_else(|| body.get("results"))
                .or_else(|| body.get("data"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            let mut hits = Vec::with_capacity(items.len());
            for (idx, item) in items.into_iter().take(max_results).enumerate() {
                let code = item
                    .get("standard_id")
                    .or_else(|| item.get("code"))
                    .or_else(|| item.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let title = item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if title.is_empty() && code.is_empty() {
                    continue;
                }

                let organization = item
                    .get("organization")
                    .or_else(|| item.get("issuer"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Standard Body")
                    .trim()
                    .to_string();
                let summary = item
                    .get("abstract")
                    .or_else(|| item.get("summary"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let pub_date = item
                    .get("published_date")
                    .or_else(|| item.get("year"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let source_url = item
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();

                let external_id = if code.is_empty() {
                    format!("STD-{idx}")
                } else {
                    code
                };
                let doc_id = format!("standard:{external_id}");
                let (title_zh, title_en) = if title
                    .chars()
                    .any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))
                {
                    (title, String::new())
                } else {
                    (String::new(), title)
                };

                hits.push(OmniSearchHit {
                    doc: KnowledgeDoc {
                        doc_id,
                        doc_kind: DocKind::Standard,
                        asset_role: AssetRole::GeneralRef,
                        external_id: external_id.clone(),
                        title_zh,
                        title_en,
                        authors_or_assignees: organization.clone(),
                        pub_or_filing_date: pub_date,
                        venue_or_authority: organization.clone(),
                        filing_status: FilingStatus::PublicDisclosed,
                        doi_verified: false,
                        bibtex_raw: String::new(),
                        abstract_text: summary,
                        claims_json: String::new(),
                        fulltext_raw: String::new(),
                        fulltext_status: "metadata_only".to_string(),
                        chunk_ver: 0,
                    },
                    realm: SearchRealm::Standard,
                    source_provider: "tech_standard".to_string(),
                    relevance_score: (0.76 - (idx as f64) * 0.02).max(0.1),
                    match_reason: format!("行业标准/白皮书命中 ({organization}: {external_id})"),
                    source_url,
                });
            }

            Ok(hits)
        })
    }
}
