//! 四大免 Key 开放学术论文检索源与真实 DOI/BibTeX 铁证固化（`scholar_sources.rs`）
//!
//! 实现 OpenAlex、Crossref、arXiv 与 Semantic Scholar 四个独立异步适配器，
//! 并提供确定性 [`normalize_and_verify_doi`] 与 [`build_bibtex_entry`]：
//! **仅当上游权威元数据源返回合法 DOI 时才标记 `doi_verified = true` 并生成 BibTeX**，
//! 绝不编造假 DOI。

use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

use crate::core::vault::models::{
    AssetRole, DocKind, FilingStatus, KnowledgeDoc, OmniSearchHit, SearchRealm,
};

use super::query_planner::QueryPlan;
use super::{build_http_client, safe_truncate_chars, BoxFuture, OmniSourceAdapter};

const DEFAULT_OPENALEX_BASE: &str = "https://api.openalex.org";
const DEFAULT_CROSSREF_BASE: &str = "https://api.crossref.org";
const DEFAULT_ARXIV_BASE: &str = "https://export.arxiv.org";
const DEFAULT_S2_BASE: &str = "https://api.semanticscholar.org";

/// OpenAlex 倒排索引摘要还原最大词数上限（防畸形载荷导致 OOM）
const MAX_INVERTED_ABSTRACT_WORDS: usize = 5000;

static DOI_REGEX: OnceLock<Option<Regex>> = OnceLock::new();

fn doi_regex() -> Option<&'static Regex> {
    DOI_REGEX
        .get_or_init(|| Regex::new(r"^10\.\d{4,9}/\S+$").ok())
        .as_ref()
}

/// 真实 DOI 归一化与格式校验：
/// 1. 剥离 `https://doi.org/` / `http://doi.org/` / `https://dx.doi.org/` / `doi:` 前缀并转小写；
/// 2. 拒绝任何空白或控制字符；
/// 3. 严格校验符合 `^10\.\d{4,9}/\S+$`。
pub fn normalize_and_verify_doi(raw: &str) -> Option<String> {
    let mut s = raw.trim().to_string();
    if s.is_empty() {
        return None;
    }

    // 循环剥离常见前缀（大小写不敏感）
    loop {
        let lower = s.to_lowercase();
        let stripped = if lower.starts_with("https://doi.org/") {
            s.chars().skip("https://doi.org/".chars().count()).collect()
        } else if lower.starts_with("http://doi.org/") {
            s.chars().skip("http://doi.org/".chars().count()).collect()
        } else if lower.starts_with("https://dx.doi.org/") {
            s.chars()
                .skip("https://dx.doi.org/".chars().count())
                .collect()
        } else if lower.starts_with("http://dx.doi.org/") {
            s.chars()
                .skip("http://dx.doi.org/".chars().count())
                .collect()
        } else if lower.starts_with("doi:") {
            s.chars().skip("doi:".chars().count()).collect()
        } else {
            break;
        };
        s = stripped;
        s = s.trim().to_string();
    }

    let normalized = s.to_lowercase();
    if normalized.is_empty()
        || normalized
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return None;
    }

    let re = doi_regex()?;
    if re.is_match(&normalized) {
        Some(normalized)
    } else {
        None
    }
}

/// 根据已核验元数据生成标准 `@article{...}` 或 `@inproceedings{...}` BibTeX 条目
#[allow(clippy::too_many_arguments)]
pub fn build_bibtex_entry(
    doc_kind: DocKind,
    cite_key: &str,
    title: &str,
    authors: &str,
    year: &str,
    venue: &str,
    doi: Option<&str>,
    url: Option<&str>,
) -> String {
    let entry_type = match doc_kind {
        DocKind::ConfPaper => "inproceedings",
        _ => "article",
    };
    let clean_key: String = cite_key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let final_key = if clean_key.is_empty() {
        "ref_item".to_string()
    } else {
        clean_key
    };

    let clean_title = sanitize_bib_field(title);
    let clean_authors = sanitize_bib_field(if authors.trim().is_empty() {
        "Unknown"
    } else {
        authors
    });
    let clean_year = sanitize_bib_field(if year.trim().is_empty() { "n.d." } else { year });
    let clean_venue = sanitize_bib_field(venue);

    let mut lines = vec![
        format!("@{entry_type}{{{final_key},"),
        format!("  title = {{{clean_title}}},"),
        format!("  author = {{{clean_authors}}},"),
        format!("  year = {{{clean_year}}},"),
    ];

    if !clean_venue.is_empty() {
        let venue_field = if doc_kind == DocKind::ConfPaper {
            "booktitle"
        } else {
            "journal"
        };
        lines.push(format!("  {venue_field} = {{{clean_venue}}},"));
    }

    if let Some(d) = doi.filter(|s| !s.trim().is_empty()) {
        lines.push(format!("  doi = {{{}}},", sanitize_bib_field(d)));
    }
    if let Some(u) = url.filter(|s| !s.trim().is_empty()) {
        lines.push(format!("  url = {{{}}},", sanitize_bib_field(u)));
    }
    lines.push("}".to_string());
    lines.join("\n")
}

fn sanitize_bib_field(s: &str) -> String {
    s.replace(['{', '}'], "")
        .replace(['\r', '\n'], " ")
        .trim()
        .to_string()
}

fn make_cite_key(provider: &str, doi_or_id: &str, year: &str) -> String {
    let suffix: String = doi_or_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(16)
        .collect();
    if year.is_empty() {
        format!("{provider}_{suffix}")
    } else {
        format!("{provider}_{year}_{suffix}")
    }
}

/// OpenAlex 学术检索适配器（`/works?search=...&per-page=...`）
pub struct OpenAlexAdapter {
    base_url: String,
    client: reqwest::Client,
}

impl OpenAlexAdapter {
    pub fn new() -> anyhow::Result<Self> {
        Self::with_base_url(DEFAULT_OPENALEX_BASE)
    }

    pub fn with_base_url(base_url: impl Into<String>) -> anyhow::Result<Self> {
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: build_http_client()?,
        })
    }
}

impl OmniSourceAdapter for OpenAlexAdapter {
    fn name(&self) -> &'static str {
        "openalex"
    }

    fn realm(&self) -> SearchRealm {
        SearchRealm::Scholar
    }

    fn search<'a>(
        &'a self,
        plan: &'a QueryPlan,
        max_results: usize,
    ) -> BoxFuture<'a, anyhow::Result<Vec<OmniSearchHit>>> {
        Box::pin(async move {
            let q = if plan.scholar_query.trim().is_empty() {
                plan.raw_query.trim()
            } else {
                plan.scholar_query.trim()
            };
            if q.is_empty() || max_results == 0 {
                return Ok(Vec::new());
            }

            let per_page = max_results.min(50);
            let url = format!(
                "{}/works?search={}&per-page={}",
                self.base_url,
                urlencoding::encode(q),
                per_page
            );
            let resp = self.client.get(&url).send().await?.error_for_status()?;
            let body: Value = resp.json().await?;
            let items = body
                .get("results")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            let mut hits = Vec::with_capacity(items.len());
            for (idx, item) in items.into_iter().take(max_results).enumerate() {
                let title = item
                    .get("title")
                    .or_else(|| item.get("display_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if title.is_empty() {
                    continue;
                }

                let raw_doi = item.get("doi").and_then(|v| v.as_str()).unwrap_or("");
                let verified_doi = normalize_and_verify_doi(raw_doi);

                let oa_id = item
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let year = item
                    .get("publication_year")
                    .and_then(|v| v.as_i64())
                    .map(|y| y.to_string())
                    .unwrap_or_default();
                let pub_date = item
                    .get("publication_date")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&year)
                    .to_string();

                let venue = item
                    .pointer("/primary_location/source/display_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let authors = item
                    .get("authorships")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|a| {
                                a.pointer("/author/display_name").and_then(|n| n.as_str())
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();

                let abstract_text = item
                    .get("abstract_inverted_index")
                    .map(reconstruct_openalex_abstract)
                    .unwrap_or_default();

                let work_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let doc_kind = if work_type.contains("proceedings") {
                    DocKind::ConfPaper
                } else {
                    DocKind::JournalPaper
                };

                let source_url = if let Some(ref d) = verified_doi {
                    format!("https://doi.org/{d}")
                } else {
                    oa_id.clone()
                };

                let (external_id, doi_verified, bibtex_raw) = match verified_doi {
                    Some(ref d) => {
                        let key = make_cite_key("openalex", d, &year);
                        let bib = build_bibtex_entry(
                            doc_kind,
                            &key,
                            &title,
                            &authors,
                            &year,
                            &venue,
                            Some(d),
                            Some(&source_url),
                        );
                        (d.clone(), true, bib)
                    }
                    None => (oa_id.clone(), false, String::new()),
                };

                let doc_id = if external_id.is_empty() {
                    format!("scholar:openalex:{idx}")
                } else {
                    format!("scholar:{external_id}")
                };

                hits.push(OmniSearchHit {
                    doc: KnowledgeDoc {
                        doc_id,
                        doc_kind,
                        asset_role: AssetRole::GeneralRef,
                        external_id,
                        title_zh: String::new(),
                        title_en: title,
                        authors_or_assignees: authors,
                        pub_or_filing_date: pub_date,
                        venue_or_authority: venue,
                        filing_status: FilingStatus::PublicDisclosed,
                        doi_verified,
                        bibtex_raw,
                        abstract_text,
                        claims_json: String::new(),
                        fulltext_raw: String::new(),
                        fulltext_status: "metadata_only".to_string(),
                        chunk_ver: 0,
                    },
                    realm: SearchRealm::Scholar,
                    source_provider: "openalex".to_string(),
                    relevance_score: (0.85 - (idx as f64) * 0.02).max(0.1),
                    match_reason: format!(
                        "OpenAlex 学术命中 (DOI 核验: {})",
                        if doi_verified {
                            "已通过"
                        } else {
                            "未提供"
                        }
                    ),
                    source_url,
                });
            }

            Ok(hits)
        })
    }
}

/// 从 OpenAlex `abstract_inverted_index` 还原摘要文本，硬上限 5000 词防 OOM
fn reconstruct_openalex_abstract(inverted: &Value) -> String {
    let Some(obj) = inverted.as_object() else {
        return String::new();
    };

    let mut pairs: Vec<(usize, &str)> = Vec::new();
    for (word, positions_val) in obj {
        let Some(positions) = positions_val.as_array() else {
            continue;
        };
        for pos_val in positions {
            if let Some(pos) = pos_val.as_u64() {
                if pos <= (MAX_INVERTED_ABSTRACT_WORDS as u64) * 4 {
                    pairs.push((pos as usize, word.as_str()));
                    if pairs.len() >= MAX_INVERTED_ABSTRACT_WORDS {
                        break;
                    }
                }
            }
        }
        if pairs.len() >= MAX_INVERTED_ABSTRACT_WORDS {
            break;
        }
    }

    pairs.sort_by_key(|(pos, _)| *pos);
    let words: Vec<&str> = pairs
        .into_iter()
        .take(MAX_INVERTED_ABSTRACT_WORDS)
        .map(|(_, w)| w)
        .collect();
    words.join(" ")
}

/// Crossref 学术检索适配器（`/works?query=...&rows=...`）
pub struct CrossrefAdapter {
    base_url: String,
    client: reqwest::Client,
}

impl CrossrefAdapter {
    pub fn new() -> anyhow::Result<Self> {
        Self::with_base_url(DEFAULT_CROSSREF_BASE)
    }

    pub fn with_base_url(base_url: impl Into<String>) -> anyhow::Result<Self> {
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: build_http_client()?,
        })
    }
}

impl OmniSourceAdapter for CrossrefAdapter {
    fn name(&self) -> &'static str {
        "crossref"
    }

    fn realm(&self) -> SearchRealm {
        SearchRealm::Scholar
    }

    fn search<'a>(
        &'a self,
        plan: &'a QueryPlan,
        max_results: usize,
    ) -> BoxFuture<'a, anyhow::Result<Vec<OmniSearchHit>>> {
        Box::pin(async move {
            let q = if plan.scholar_query.trim().is_empty() {
                plan.raw_query.trim()
            } else {
                plan.scholar_query.trim()
            };
            if q.is_empty() || max_results == 0 {
                return Ok(Vec::new());
            }

            let rows = max_results.min(50);
            let url = format!(
                "{}/works?query={}&rows={}",
                self.base_url,
                urlencoding::encode(q),
                rows
            );
            let resp = self.client.get(&url).send().await?.error_for_status()?;
            let body: Value = resp.json().await?;
            let items = body
                .pointer("/message/items")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            let mut hits = Vec::with_capacity(items.len());
            for (idx, item) in items.into_iter().take(max_results).enumerate() {
                let title = item
                    .get("title")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if title.is_empty() {
                    continue;
                }

                let raw_doi = item.get("DOI").and_then(|v| v.as_str()).unwrap_or("");
                let verified_doi = normalize_and_verify_doi(raw_doi);

                let authors = item
                    .get("author")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|a| {
                                let given = a.get("given").and_then(|v| v.as_str()).unwrap_or("");
                                let family = a.get("family").and_then(|v| v.as_str()).unwrap_or("");
                                let full = format!("{given} {family}").trim().to_string();
                                if full.is_empty() {
                                    None
                                } else {
                                    Some(full)
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();

                let venue = item
                    .get("container-title")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();

                let (year, pub_date) = extract_crossref_date(&item);

                let abstract_raw = item.get("abstract").and_then(|v| v.as_str()).unwrap_or("");
                let abstract_text = strip_simple_xml_tags(abstract_raw);

                let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let doc_kind = if item_type.contains("proceedings") {
                    DocKind::ConfPaper
                } else {
                    DocKind::JournalPaper
                };

                let item_url = item
                    .get("URL")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let source_url = if let Some(ref d) = verified_doi {
                    format!("https://doi.org/{d}")
                } else {
                    item_url
                };

                let (external_id, doi_verified, bibtex_raw) = match verified_doi {
                    Some(ref d) => {
                        let key = make_cite_key("crossref", d, &year);
                        let bib = build_bibtex_entry(
                            doc_kind,
                            &key,
                            &title,
                            &authors,
                            &year,
                            &venue,
                            Some(d),
                            Some(&source_url),
                        );
                        (d.clone(), true, bib)
                    }
                    None => (String::new(), false, String::new()),
                };

                let doc_id = if external_id.is_empty() {
                    format!("scholar:crossref:{idx}")
                } else {
                    format!("scholar:{external_id}")
                };

                hits.push(OmniSearchHit {
                    doc: KnowledgeDoc {
                        doc_id,
                        doc_kind,
                        asset_role: AssetRole::GeneralRef,
                        external_id,
                        title_zh: String::new(),
                        title_en: title,
                        authors_or_assignees: authors,
                        pub_or_filing_date: pub_date,
                        venue_or_authority: venue,
                        filing_status: FilingStatus::PublicDisclosed,
                        doi_verified,
                        bibtex_raw,
                        abstract_text,
                        claims_json: String::new(),
                        fulltext_raw: String::new(),
                        fulltext_status: "metadata_only".to_string(),
                        chunk_ver: 0,
                    },
                    realm: SearchRealm::Scholar,
                    source_provider: "crossref".to_string(),
                    relevance_score: (0.84 - (idx as f64) * 0.02).max(0.1),
                    match_reason: format!(
                        "Crossref 权威题录命中 (DOI 核验: {})",
                        if doi_verified {
                            "已通过"
                        } else {
                            "未通过/缺失"
                        }
                    ),
                    source_url,
                });
            }

            Ok(hits)
        })
    }
}

fn extract_crossref_date(item: &Value) -> (String, String) {
    let date_obj = item
        .get("published")
        .or_else(|| item.get("published-print"))
        .or_else(|| item.get("published-online"))
        .or_else(|| item.get("created"));

    let Some(parts) = date_obj
        .and_then(|v| v.get("date-parts"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_array())
    else {
        return (String::new(), String::new());
    };

    let nums: Vec<String> = parts
        .iter()
        .filter_map(|n| n.as_i64().map(|v| format!("{v:02}")))
        .collect();
    if nums.is_empty() {
        return (String::new(), String::new());
    }
    let year = parts
        .first()
        .and_then(|n| n.as_i64())
        .map(|y| y.to_string())
        .unwrap_or_default();
    let full = if let Some(first) = parts.first().and_then(|n| n.as_i64()) {
        let mut segs = vec![first.to_string()];
        for p in parts.iter().skip(1) {
            if let Some(v) = p.as_i64() {
                segs.push(format!("{v:02}"));
            }
        }
        segs.join("-")
    } else {
        nums.join("-")
    };
    (year, full)
}

/// arXiv 预印本 Atom API 适配器（`/api/query?search_query=all:...&max_results=...`）
pub struct ArxivAdapter {
    base_url: String,
    client: reqwest::Client,
}

impl ArxivAdapter {
    pub fn new() -> anyhow::Result<Self> {
        Self::with_base_url(DEFAULT_ARXIV_BASE)
    }

    pub fn with_base_url(base_url: impl Into<String>) -> anyhow::Result<Self> {
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: build_http_client()?,
        })
    }
}

impl OmniSourceAdapter for ArxivAdapter {
    fn name(&self) -> &'static str {
        "arxiv"
    }

    fn realm(&self) -> SearchRealm {
        SearchRealm::Scholar
    }

    fn search<'a>(
        &'a self,
        plan: &'a QueryPlan,
        max_results: usize,
    ) -> BoxFuture<'a, anyhow::Result<Vec<OmniSearchHit>>> {
        Box::pin(async move {
            let q = if plan.scholar_query.trim().is_empty() {
                plan.raw_query.trim()
            } else {
                plan.scholar_query.trim()
            };
            if q.is_empty() || max_results == 0 {
                return Ok(Vec::new());
            }

            let limit = max_results.min(50);
            let url = format!(
                "{}/api/query?search_query=all:{}&max_results={}",
                self.base_url,
                urlencoding::encode(q),
                limit
            );
            let resp = self.client.get(&url).send().await?.error_for_status()?;
            let xml = resp.text().await?;

            let entries = parse_arxiv_atom_entries(&xml);
            let mut hits = Vec::with_capacity(entries.len());

            for (idx, entry) in entries.into_iter().take(max_results).enumerate() {
                if entry.title.is_empty() {
                    continue;
                }
                let verified_doi = entry.doi.as_deref().and_then(normalize_and_verify_doi);

                let year: String = entry.published.chars().take(4).collect();
                let arxiv_id = extract_arxiv_id(&entry.id);
                let source_url = if entry.id.trim().is_empty() {
                    String::new()
                } else {
                    entry.id.clone()
                };

                let (external_id, doi_verified, bibtex_raw) = match verified_doi {
                    Some(ref d) => {
                        let key = make_cite_key("arxiv", d, &year);
                        let bib = build_bibtex_entry(
                            DocKind::Preprint,
                            &key,
                            &entry.title,
                            &entry.authors,
                            &year,
                            "arXiv",
                            Some(d),
                            Some(&source_url),
                        );
                        (d.clone(), true, bib)
                    }
                    None => (arxiv_id.clone(), false, String::new()),
                };

                let doc_id = if external_id.is_empty() {
                    format!("scholar:arxiv:{idx}")
                } else {
                    format!("scholar:{external_id}")
                };

                hits.push(OmniSearchHit {
                    doc: KnowledgeDoc {
                        doc_id,
                        doc_kind: DocKind::Preprint,
                        asset_role: AssetRole::GeneralRef,
                        external_id,
                        title_zh: String::new(),
                        title_en: entry.title,
                        authors_or_assignees: entry.authors,
                        pub_or_filing_date: entry.published,
                        venue_or_authority: "arXiv".to_string(),
                        filing_status: FilingStatus::PublicDisclosed,
                        doi_verified,
                        bibtex_raw,
                        abstract_text: entry.summary,
                        claims_json: String::new(),
                        fulltext_raw: String::new(),
                        fulltext_status: "metadata_only".to_string(),
                        chunk_ver: 0,
                    },
                    realm: SearchRealm::Scholar,
                    source_provider: "arxiv".to_string(),
                    relevance_score: (0.80 - (idx as f64) * 0.02).max(0.1),
                    match_reason: format!(
                        "arXiv 预印本命中 (DOI 核验: {})",
                        if doi_verified {
                            "已通过"
                        } else {
                            "仅 arXiv ID"
                        }
                    ),
                    source_url,
                });
            }

            Ok(hits)
        })
    }
}

struct ArxivEntry {
    id: String,
    title: String,
    summary: String,
    published: String,
    authors: String,
    doi: Option<String>,
}

fn parse_arxiv_atom_entries(xml: &str) -> Vec<ArxivEntry> {
    let mut entries = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<entry>") {
        let after_start = rest.split_at(start + "<entry>".len()).1;
        let Some(end) = after_start.find("</entry>") else {
            break;
        };
        let block = after_start.split_at(end).0;
        rest = after_start.split_at(end + "</entry>".len()).1;

        let id = extract_xml_tag_text(block, "id").unwrap_or_default();
        let title = normalize_whitespace(&unescape_xml(
            &extract_xml_tag_text(block, "title").unwrap_or_default(),
        ));
        let summary = normalize_whitespace(&unescape_xml(
            &extract_xml_tag_text(block, "summary").unwrap_or_default(),
        ));
        let published = extract_xml_tag_text(block, "published")
            .map(|s| s.chars().take(10).collect())
            .unwrap_or_default();
        let doi =
            extract_xml_tag_text(block, "arxiv:doi").or_else(|| extract_xml_tag_text(block, "doi"));

        let authors = extract_all_xml_tag_texts(block, "name")
            .into_iter()
            .map(|a| normalize_whitespace(&unescape_xml(&a)))
            .filter(|a| !a.is_empty())
            .collect::<Vec<_>>()
            .join(", ");

        entries.push(ArxivEntry {
            id,
            title,
            summary,
            published,
            authors,
            doi,
        });
    }
    entries
}

fn extract_xml_tag_text(block: &str, tag: &str) -> Option<String> {
    let open_exact = format!("<{tag}>");
    let open_prefix = format!("<{tag} ");
    let close_tag = format!("</{tag}>");

    let start_pos = block
        .find(&open_exact)
        .map(|i| i + open_exact.len())
        .or_else(|| {
            let i = block.find(&open_prefix)?;
            let tail = block.split_at(i).1;
            let gt = tail.find('>')?;
            Some(i + gt + 1)
        })?;

    let tail = block.split_at(start_pos).1;
    let end_pos = tail.find(&close_tag)?;
    Some(tail.split_at(end_pos).0.trim().to_string())
}

fn extract_all_xml_tag_texts(block: &str, tag: &str) -> Vec<String> {
    let mut out = Vec::new();
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut rest = block;
    while let Some(i) = rest.find(&open) {
        let after = rest.split_at(i + open.len()).1;
        let Some(j) = after.find(&close) else {
            break;
        };
        out.push(after.split_at(j).0.trim().to_string());
        rest = after.split_at(j + close.len()).1;
    }
    out
}

fn extract_arxiv_id(id_url: &str) -> String {
    let trimmed = id_url.trim().trim_end_matches('/');
    if let Some(last) = trimmed.rsplit('/').next() {
        if !last.is_empty() {
            return format!("arXiv:{last}");
        }
    }
    trimmed.to_string()
}

fn unescape_xml(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_simple_xml_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    normalize_whitespace(&unescape_xml(&out))
}

/// Semantic Scholar 学术检索适配器
/// (`/graph/v1/paper/search?query=...&limit=...&fields=title,abstract,authors,year,venue,externalIds,citationStyles`)
pub struct SemanticScholarAdapter {
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl SemanticScholarAdapter {
    pub fn new() -> anyhow::Result<Self> {
        Self::with_base_url(DEFAULT_S2_BASE)
    }

    pub fn with_base_url(base_url: impl Into<String>) -> anyhow::Result<Self> {
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: None,
            client: build_http_client()?,
        })
    }

    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        let k = key.into().trim().to_string();
        if !k.is_empty() {
            self.api_key = Some(k);
        }
        self
    }
}

impl OmniSourceAdapter for SemanticScholarAdapter {
    fn name(&self) -> &'static str {
        "semantic_scholar"
    }

    fn realm(&self) -> SearchRealm {
        SearchRealm::Scholar
    }

    fn search<'a>(
        &'a self,
        plan: &'a QueryPlan,
        max_results: usize,
    ) -> BoxFuture<'a, anyhow::Result<Vec<OmniSearchHit>>> {
        Box::pin(async move {
            let q = if plan.scholar_query.trim().is_empty() {
                plan.raw_query.trim()
            } else {
                plan.scholar_query.trim()
            };
            if q.is_empty() || max_results == 0 {
                return Ok(Vec::new());
            }

            let limit = max_results.min(50);
            let url = format!(
                "{}/graph/v1/paper/search?query={}&limit={}&fields=title,abstract,authors,year,venue,externalIds,citationStyles,url",
                self.base_url,
                urlencoding::encode(q),
                limit
            );

            let mut req = self.client.get(&url);
            if let Some(ref key) = self.api_key {
                req = req.header("x-api-key", key);
            }
            let resp = req.send().await?.error_for_status()?;
            let body: Value = resp.json().await?;
            let items = body
                .get("data")
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

                let raw_doi = item
                    .pointer("/externalIds/DOI")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let verified_doi = normalize_and_verify_doi(raw_doi);

                let paper_id = item
                    .get("paperId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let year = item
                    .get("year")
                    .and_then(|v| v.as_i64())
                    .map(|y| y.to_string())
                    .unwrap_or_default();
                let venue = item
                    .get("venue")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let abstract_text = item
                    .get("abstract")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let authors = item
                    .get("authors")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();

                let item_url = item
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let source_url = if let Some(ref d) = verified_doi {
                    format!("https://doi.org/{d}")
                } else {
                    item_url
                };

                let upstream_bib = item
                    .pointer("/citationStyles/bibtex")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();

                let (external_id, doi_verified, bibtex_raw) = match verified_doi {
                    Some(ref d) => {
                        let bib = if !upstream_bib.is_empty() {
                            upstream_bib
                        } else {
                            let key = make_cite_key("s2", d, &year);
                            build_bibtex_entry(
                                DocKind::JournalPaper,
                                &key,
                                &title,
                                &authors,
                                &year,
                                &venue,
                                Some(d),
                                Some(&source_url),
                            )
                        };
                        (d.clone(), true, bib)
                    }
                    None => (safe_truncate_chars(&paper_id, 64), false, String::new()),
                };

                let doc_id = if external_id.is_empty() {
                    format!("scholar:s2:{idx}")
                } else {
                    format!("scholar:{external_id}")
                };

                hits.push(OmniSearchHit {
                    doc: KnowledgeDoc {
                        doc_id,
                        doc_kind: DocKind::JournalPaper,
                        asset_role: AssetRole::GeneralRef,
                        external_id,
                        title_zh: String::new(),
                        title_en: title,
                        authors_or_assignees: authors,
                        pub_or_filing_date: year,
                        venue_or_authority: venue,
                        filing_status: FilingStatus::PublicDisclosed,
                        doi_verified,
                        bibtex_raw,
                        abstract_text,
                        claims_json: String::new(),
                        fulltext_raw: String::new(),
                        fulltext_status: "metadata_only".to_string(),
                        chunk_ver: 0,
                    },
                    realm: SearchRealm::Scholar,
                    source_provider: "semantic_scholar".to_string(),
                    relevance_score: (0.82 - (idx as f64) * 0.02).max(0.1),
                    match_reason: format!(
                        "Semantic Scholar 命中 (DOI 核验: {})",
                        if doi_verified {
                            "已通过"
                        } else {
                            "未提供"
                        }
                    ),
                    source_url,
                });
            }

            Ok(hits)
        })
    }
}
