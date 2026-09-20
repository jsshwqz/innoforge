//! SerpAPI（google_patents）检索源 —— [`SearchProvider`] 的首个实现
//!
//! 本文件是 `src/routes/search.rs` 现役 SerpAPI 调用链的**逐字抽出**（spec §3 改造点 1）：
//! URL 参数集合与拼装顺序、`page` 钳制、中文 `hl/gl/lr` 约束、结果映射
//! （`serp_to_patent` → 入库缓存 → hybrid(pos*0.4+content*0.6) 打分 → 相关性放行 →
//! 中文二次过滤 → 去重排序）、以及所有用户可见的失败文案，全部保持迁移前语义。
//!
//! 「改造前后一致」不是口头承诺，由本模块的测试锁死：
//! - `search_url_matches_legacy_builder`：与旧 `format!` 的**参照实现**逐字符比对（上游请求参数）；
//! - `outcome_patents_match_legacy_mapping`：与旧映射的**参照实现**逐字段比对（结果映射）；
//! - 上述两条覆盖同一批 fixture，等价于对 `/api/search/online` 的形状回归。
//!
//! FailKind 的归类（spec §1）是 MA1 新增的**记账**，只写入 `AttemptReport`，
//! 不参与 MA1 的任何控制流（单源无需切源），因此不影响响应内容。

use crate::db::Database;
use crate::patent::Patent;
use crate::search::merge::merged_from;
use crate::search::model::{
    report_excerpt, AttemptReport, AttemptStatus, FailKind, Lang, SearchOutcome, SearchQuery,
    SourceKind,
};
use crate::search::provider::SearchProvider;
use crate::search::query::render_q;
use crate::search::relevance::rank_and_gate_hits;
use crate::types::search::PatentSummary;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 上游单次请求超时。**保持旧值 30s**（`ONLINE_UPSTREAM_TIMEOUT_SECS`）：
/// spec §1 建议收紧到 15s，但那属于 MA2 执行链的行为变更，MA1 只做「行为保持」的抽出。
const UPSTREAM_TIMEOUT_SECS: u64 = 30;

/// SerpAPI `google_patents` 引擎封装（付费主源，spec §3）。
pub struct SerpApiProvider {
    /// 本次请求选用的 Key —— 由调用方从 `AppConfig::next_serpapi_key()` 轮转取得后注入，
    /// provider 自身不做 Key 轮转（那是配置的既有职责，MA1 不上提）。
    api_key: String,
    /// 命中结果需要缓存入库（旧行为：`insert_patent` 后用真实 stored id 回填 `summary.id`）。
    db: Arc<Database>,
}

/// 该查询是否按「中文语境」请求上游（等价于旧 `is_cn_query`）。
fn is_cn_query(query: &SearchQuery) -> bool {
    matches!(query.language, Some(Lang::Chinese))
}

/// 旧链路的分页钳制：`page < 1 → 1`。
fn page_param(query: &SearchQuery) -> usize {
    if query.page < 1 {
        1
    } else {
        query.page
    }
}

/// `&country=XX`：旧代码对空串刻意不下发该参数，且**未** 做 URL 编码 —— 原样保留。
fn country_param(query: &SearchQuery) -> String {
    match query.country.as_deref() {
        Some(c) if !c.is_empty() => format!("&country={}", c),
        _ => String::new(),
    }
}

/// `&sort=new|old`：只认这两个值，其余（含 "relevance"）不下发。
fn sort_param(query: &SearchQuery) -> &'static str {
    match query.sort_by.as_deref() {
        Some("new") => "&sort=new",
        Some("old") => "&sort=old",
        _ => "",
    }
}

/// 中文查询时的地理 + 语种约束（「要中文就给中文」在上游参数上的落点）。
fn lang_param(query: &SearchQuery) -> &'static str {
    if is_cn_query(query) {
        "&hl=zh-cn&gl=cn&lr=lang_zh-CN"
    } else {
        ""
    }
}

impl SerpApiProvider {
    pub fn new(api_key: String, db: Arc<Database>) -> Self {
        SerpApiProvider { api_key, db }
    }

    /// 与旧代码一致的客户端构造：超时 30s，builder 失败时退回默认客户端。
    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(UPSTREAM_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    }

    /// 关键词检索的上游请求 URL。
    ///
    /// 参数顺序 `engine → q → page → country → sort → lang → api_key` 与旧 `format!` 逐字相同：
    /// SerpAPI 对参数顺序不敏感，但**锁死顺序**才能让「改造前后请求一致」可被 diff 审查。
    pub(crate) fn search_url(query: &SearchQuery, api_key: &str) -> String {
        format!(
            "https://serpapi.com/search.json?engine=google_patents&q={}&page={}{}{}{}&api_key={}",
            urlencoding::encode(&render_q(query)),
            page_param(query),
            country_param(query),
            sort_param(query),
            lang_param(query),
            api_key
        )
    }

    /// 详情直查（`google_patents_details`）的请求 URL。
    fn details_url(patent_id: &str, api_key: &str) -> String {
        format!(
            "https://serpapi.com/search.json?engine=google_patents_details&patent_id={}&api_key={}",
            urlencoding::encode(patent_id),
            api_key
        )
    }

    /// 用关键词搜出 `patent_id`（CN 申请号 → 公开号的发现步骤，旧 `find_publication_patent_id`）。
    async fn find_publication_patent_id(
        client: &reqwest::Client,
        api_key: &str,
        candidates: &[String],
    ) -> Option<String> {
        for candidate in candidates {
            let c = candidate.trim();
            if c.is_empty() {
                continue;
            }
            let search_url = format!(
                "https://serpapi.com/search.json?engine=google_patents&q={}&page=1&api_key={}",
                urlencoding::encode(c),
                api_key
            );
            let resp = client.get(&search_url).send().await.ok()?;
            let body = resp.text().await.ok()?;
            let json: serde_json::Value = serde_json::from_str(&body).ok()?;
            if let Some(id) = json["organic_results"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|r| r["patent_id"].as_str())
                .map(|s| s.to_string())
            {
                return Some(id);
            }
        }
        None
    }

    /// `organic_results[] → PatentSummary[]`，随后套用中文二次过滤与去重。
    ///
    /// 这条链路的**实现自 MA2a 起住在 [`rank_and_gate_hits`]**（与 Google Patents XHR 源共用，
    /// AGENTS.md 2.2 禁止同一打分链路存在第二份实现）。本方法只剩「注入本源的字段映射器
    /// [`serp_to_patent`]」这一层职责，行为与 MA1 逐字一致，
    /// 等价性继续由 `outcome_patents_match_legacy_mapping` 对**迁移前**参照实现逐字段比对锁死。
    fn outcome_patents(
        &self,
        keyword: &str,
        cn_query: bool,
        results: &[serde_json::Value],
    ) -> Vec<PatentSummary> {
        rank_and_gate_hits(
            self.db.as_ref(),
            SourceKind::SerpApi.as_str(),
            keyword,
            cn_query,
            results,
            serp_to_patent,
        )
    }
}

impl SearchProvider for SerpApiProvider {
    fn kind(&self) -> SourceKind {
        SourceKind::SerpApi
    }

    fn search<'a>(
        &'a self,
        query: SearchQuery,
    ) -> Pin<Box<dyn Future<Output = SearchOutcome> + Send + 'a>> {
        Box::pin(async move {
            let started = Instant::now();
            // 旧 `upstream_hint`：所有分支的赋值都保持「先判 is_none() 再写」的首个非空语义。
            let mut hint: Option<String> = None;
            let mut status = AttemptStatus::Success;
            let mut upstream_total: Option<usize> = None;
            let mut patents: Vec<PatentSummary> = Vec::new();

            let client = Self::client();
            let search_query = render_q(&query);
            // 技术性失败原因（spec §1：`error` 带上下文，禁止裸 `err.to_string()`）。
            // 与用户可见的 `hint` 分开记账：hint 必须逐字保持旧文案，error 是 MA5 面板的新数据源。
            let mut error_note: Option<String> = None;
            println!(
                "[ONLINE] SerpAPI query='{}' page={} country_param='{}'",
                search_query,
                page_param(&query),
                country_param(&query)
            );
            let url = Self::search_url(&query, &self.api_key);
            match client.get(&url).send().await {
                Ok(resp) => {
                    let resp_status = resp.status();
                    println!("[ONLINE] SerpAPI status: {}", resp_status);
                    match resp.text().await {
                        Ok(body) => {
                            println!("[ONLINE] SerpAPI body len={}", body.len());
                            if !resp_status.is_success() {
                                if resp_status.as_u16() == 429 {
                                    hint = Some(
                                        "SerpAPI 触发限流/额度限制（429），已自动尝试下游回退。"
                                            .to_string(),
                                    );
                                    error_note = Some(format!(
                                        "SerpAPI 上游返回 HTTP 429（限流/额度受限）：{}",
                                        report_excerpt(&body)
                                    ));
                                    status = AttemptStatus::Failed(FailKind::Quota);
                                } else {
                                    hint = Some(format!(
                                        "SerpAPI 请求失败（HTTP {}），已自动尝试下游回退。",
                                        resp_status
                                    ));
                                    error_note = Some(format!(
                                        "SerpAPI 上游返回 HTTP {}：{}",
                                        resp_status,
                                        report_excerpt(&body)
                                    ));
                                    status =
                                        AttemptStatus::Failed(fail_kind_for_status(resp_status));
                                }
                            } else if let Ok(json) =
                                serde_json::from_str::<serde_json::Value>(&body)
                            {
                                if let Some(err) = json.get("error").and_then(|v| v.as_str()) {
                                    println!("[ONLINE] SerpAPI error: {}", err);
                                    let err_l = err.to_lowercase();
                                    error_note = Some(format!(
                                        "SerpAPI 上游报错（engine=google_patents）：{}",
                                        report_excerpt(err)
                                    ));
                                    if err_l.contains("too many requests")
                                        || err_l.contains("rate limit")
                                        || err_l.contains("quota")
                                    {
                                        hint = Some(
                                            "SerpAPI 配额或频率受限，已自动尝试下游回退。"
                                                .to_string(),
                                        );
                                        status = AttemptStatus::Failed(FailKind::Quota);
                                    } else {
                                        let short = err.chars().take(120).collect::<String>();
                                        hint = Some(format!(
                                            "SerpAPI 返回错误：{}，已自动尝试下游回退。",
                                            short
                                        ));
                                        status = AttemptStatus::Failed(fail_kind_for_error(&err_l));
                                    }
                                } else {
                                    let total = json["search_information"]["total_results"]
                                        .as_u64()
                                        .unwrap_or(0)
                                        as usize;
                                    upstream_total = Some(total);
                                    if let Some(results) = json["organic_results"].as_array() {
                                        println!(
                                            "[ONLINE] SerpAPI results: {}, total: {}",
                                            results.len(),
                                            total
                                        );
                                        patents = self.outcome_patents(
                                            &query.keyword,
                                            is_cn_query(&query),
                                            results,
                                        );
                                    }
                                    // 旧代码只在这条「解析成功但零结果」的路径上补该文案；
                                    // 网络/读体失败等路径不得顺手覆盖，否则用户可见提示会变。
                                    if patents.is_empty() {
                                        println!(
                                            "[ONLINE] SerpAPI returned empty; fallback to local DB"
                                        );
                                        if hint.is_none() {
                                            hint = Some(
                                                "SerpAPI 无结果，已自动尝试下游回退。".to_string(),
                                            );
                                        }
                                        error_note = Some(
                                            "SerpAPI 返回 0 条可用结果（上游 organic_results 为空或全被过滤)"
                                                .to_string(),
                                        );
                                    }
                                }
                            } else if hint.is_none() {
                                hint = Some(
                                    "SerpAPI 返回内容无法解析，已自动尝试下游回退。".to_string(),
                                );
                                error_note = Some(format!(
                                    "SerpAPI 响应不是合法 JSON（上游结构可能已变化），片段：{}",
                                    report_excerpt(&body)
                                ));
                                status = AttemptStatus::Failed(FailKind::Parse);
                            }
                        }
                        Err(e) => {
                            // 旧代码此处 `if let Ok(body)` 失败即静默继续，不产生文案。
                            println!("[ONLINE] SerpAPI body read error: {}", e);
                            error_note = Some(format!("读取 SerpAPI 响应体失败：{}", e));
                            status = AttemptStatus::Failed(FailKind::Network);
                        }
                    }
                }
                Err(e) => {
                    println!("[ONLINE] SerpAPI request error: {}", e);
                    hint = Some(format!("SerpAPI 请求异常：{}，已自动尝试下游回退。", e));
                    error_note = Some(format!("请求 SerpAPI 上游失败（网络/超时）：{}", e));
                    status = AttemptStatus::Failed(FailKind::Network);
                }
            }

            let hits = patents.len();
            SearchOutcome {
                results: merged_from(SourceKind::SerpApi, patents),
                attempts: vec![AttemptReport {
                    source: SourceKind::SerpApi,
                    status,
                    latency_ms: started.elapsed().as_millis() as u64,
                    hits,
                    error: error_note,
                    hint,
                }],
                upstream_total,
            }
        })
    }

    /// 精确专利号直查（旧 `try_exact_patent_lookup`）。
    ///
    /// 返回 `Option<PatentSummary>`：旧代码返回整段 JSON，但 JSON 的形状属于**端点**职责
    /// 而非数据源职责，故交由 `routes/search.rs` 用 [`exact_lookup_response`] 还原，
    /// 对外形状与旧值逐字一致。
    fn lookup_exact<'a>(
        &'a self,
        query: String,
    ) -> Pin<Box<dyn Future<Output = Option<PatentSummary>> + Send + 'a>> {
        Box::pin(async move {
            let q = query.trim().to_string();
            let digits: String = q.chars().filter(|c| c.is_ascii_digit()).collect();
            let api_key = self.api_key.as_str();
            let client = match reqwest::Client::builder()
                .timeout(Duration::from_secs(UPSTREAM_TIMEOUT_SECS))
                .build()
            {
                Ok(c) => c,
                Err(_) => return None,
            };

            // Step 1: Determine patent_id to look up
            let patent_id: String;

            let is_bare_cn_app = digits.len() >= 10
                && digits.len() <= 15
                && q.chars().all(|c| c.is_ascii_digit() || c == '.');

            let is_cn_app_with_prefix =
                q.starts_with("CN") && q.contains('.') && digits.len() >= 10 && digits.len() <= 15;

            if ["CN", "US", "EP", "WO", "JP", "KR"]
                .iter()
                .any(|prefix| q.starts_with(prefix))
            {
                if is_cn_app_with_prefix {
                    // CN APPLICATION number with CN prefix (e.g., CN202420009882.7)
                    // Google Patents indexes by PUBLICATION number, not application number.
                    // We must first search to discover the publication number.
                    let mut candidates = vec![q.clone(), q.replace('.', "")];
                    let dot_pos = q.find('.').unwrap_or(q.len());
                    let pre_dot_digits: String = q[..dot_pos]
                        .chars()
                        .filter(|c| c.is_ascii_digit())
                        .collect();
                    if !pre_dot_digits.is_empty() {
                        candidates.push(pre_dot_digits.clone());
                        if pre_dot_digits.len() > 1 {
                            candidates.push(format!("CN{}", pre_dot_digits));
                        }
                    }

                    println!(
                        "[EXACT] CN app number with prefix detected, trying candidates: {:?}",
                        candidates
                    );
                    match Self::find_publication_patent_id(&client, api_key, &candidates).await {
                        Some(id) => {
                            let id = if id.contains("/CN") {
                                id.replace("/en", "/zh")
                            } else {
                                id
                            };
                            println!("[EXACT] Found publication via keyword search: {}", id);
                            patent_id = id;
                        }
                        None => {
                            println!(
                                "[EXACT] Keyword search returned no results for {:?}",
                                candidates
                            );
                            return None;
                        }
                    }
                } else {
                    // Already has country prefix — publication number, try directly
                    let no_dot = q.replace('.', "");
                    let lang = if q.starts_with("CN") { "zh" } else { "en" };
                    patent_id = format!("patent/{}/{}", no_dot, lang);
                }
            } else if is_bare_cn_app {
                // Bare Chinese APPLICATION number (e.g. 202210835143.9)
                // Google Patents indexes by PUBLICATION number, not application number.
                // We must first search to discover the publication number.
                let mut candidates = Vec::new();
                candidates.push(q.clone());
                candidates.push(digits.clone());
                if digits.len() >= 13 {
                    candidates.push(digits[..digits.len() - 1].to_string());
                    candidates.push(format!("CN{}", &digits[..digits.len() - 1]));
                }
                if digits.len() >= 12 {
                    candidates.push(format!("CN{}", &digits[..12]));
                }
                println!(
                    "[EXACT] Bare CN app number detected, trying candidates: {:?}",
                    candidates
                );

                match Self::find_publication_patent_id(&client, api_key, &candidates).await {
                    Some(id) => {
                        // For CN patents, use /zh to get Chinese results
                        let id = if id.contains("/CN") {
                            id.replace("/en", "/zh")
                        } else {
                            id
                        };
                        println!("[EXACT] Found publication via keyword search: {}", id);
                        patent_id = id;
                    }
                    None => {
                        println!(
                            "[EXACT] Keyword search returned no results for {:?}",
                            candidates
                        );
                        return None;
                    }
                }
            } else {
                // Default: use /en for non-CN patents
                patent_id = format!("patent/{}/en", q);
            }

            // Step 2: Fetch full details via google_patents_details
            let url = Self::details_url(&patent_id, api_key);
            println!("[EXACT] Fetching details for: {}", patent_id);

            let resp = client.get(&url).send().await.ok()?;
            let body = resp.text().await.ok()?;
            let json: serde_json::Value = serde_json::from_str(&body).ok()?;

            if json.get("error").is_some() {
                println!("[EXACT] Details API error: {}", json["error"]);
                return None;
            }

            let title = json["title"].as_str().unwrap_or("");
            if title.is_empty() {
                println!("[EXACT] Details returned empty title");
                return None;
            }

            // Extract inventors/assignees from arrays
            let inventor = json["inventors"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v["name"].as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_default();
            let assignee = json["assignees"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_default();

            let pub_number = json["publication_number"]
                .as_str()
                .unwrap_or(&q)
                .to_string();
            let country = pub_number.chars().take(2).collect::<String>();
            let patent = Patent {
                id: uuid::Uuid::new_v4().to_string(),
                patent_number: pub_number.clone(),
                title: title.to_string(),
                abstract_text: json["abstract"].as_str().unwrap_or("").to_string(),
                description: json["description"].as_str().unwrap_or("").to_string(),
                claims: json["claims"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join("\n\n")
                    })
                    .unwrap_or_default(),
                applicant: assignee.clone(),
                inventor: inventor.clone(),
                filing_date: json["filing_date"].as_str().unwrap_or("").to_string(),
                publication_date: json["publication_date"].as_str().unwrap_or("").to_string(),
                grant_date: json["grant_date"].as_str().map(|s| s.to_string()),
                ipc_codes: String::new(),
                cpc_codes: String::new(),
                priority_date: json["priority_date"].as_str().unwrap_or("").to_string(),
                country: country.clone(),
                kind_code: String::new(),
                family_id: None,
                legal_status: String::new(),
                citations: "[]".into(),
                cited_by: "[]".into(),
                source: "serpapi_exact".into(),
                raw_json: body,
                created_at: chrono::Utc::now().to_rfc3339(),
                images: "[]".into(),
                pdf_url: json["pdf"].as_str().unwrap_or("").to_string(),
            };

            // Cache to local DB, use actual stored id
            let saved_id = self
                .db
                .insert_patent(&patent)
                .unwrap_or_else(|_| patent.id.clone());

            let summary = PatentSummary {
                id: saved_id,
                patent_number: patent.patent_number.clone(),
                title: patent.title.clone(),
                abstract_text: patent.abstract_text.clone(),
                applicant: patent.applicant.clone(),
                inventor: patent.inventor.clone(),
                filing_date: patent.filing_date.clone(),
                country,
                relevance_score: Some(100.0),
                score_source: Some("exact_lookup".to_string()),
            };

            println!(
                "[EXACT] Found patent: {} — {}",
                summary.patent_number, summary.title
            );
            Some(summary)
        })
    }
}

/// HTTP 状态码 → 失败分类（spec §1）。仅用于 `AttemptReport` 记账。
///
/// MA2a 起判定规则本身已上提到 [`FailKind::for_http_status`]（两个在线源共用一份出处，
/// AGENTS.md 2.2），此处只保留 `reqwest::StatusCode` 的解包薄壳，
/// 让既有单测 `fail_kind_classification_does_not_affect_hints` 继续锁死旧行为。
fn fail_kind_for_status(status: reqwest::StatusCode) -> FailKind {
    FailKind::for_http_status(status.as_u16())
}

/// 上游 error 文案 → 失败分类（spec §1）。仅用于 `AttemptReport` 记账。
fn fail_kind_for_error(err_lower: &str) -> FailKind {
    if err_lower.contains("invalid api key")
        || err_lower.contains("api_key")
        || err_lower.contains("unauthorized")
    {
        FailKind::Auth
    } else {
        FailKind::Parse
    }
}

/// 诊断字段用的安全截断已上提到 `model::report_excerpt`（MA2a 起两个在线源共用一份取值规则）。

/// SerpAPI `organic_results[]` 单条 → 域内 `Patent`（旧 `serp_to_patent`，字段映射未改）。
fn serp_to_patent(r: &serde_json::Value) -> Patent {
    let pub_num = r["publication_number"].as_str().unwrap_or("").to_string();
    let country = pub_num.chars().take(2).collect::<String>();
    Patent {
        id: uuid::Uuid::new_v4().to_string(),
        patent_number: pub_num,
        title: r["title"].as_str().unwrap_or("").to_string(),
        abstract_text: r["snippet"].as_str().unwrap_or("").to_string(),
        description: String::new(),
        claims: String::new(),
        applicant: r["assignee"].as_str().unwrap_or("").to_string(),
        inventor: r["inventor"].as_str().unwrap_or("").to_string(),
        filing_date: r["filing_date"].as_str().unwrap_or("").to_string(),
        publication_date: r["publication_date"].as_str().unwrap_or("").to_string(),
        grant_date: r["grant_date"].as_str().map(|s| s.to_string()),
        ipc_codes: String::new(),
        cpc_codes: String::new(),
        priority_date: r["priority_date"].as_str().unwrap_or("").to_string(),
        country,
        kind_code: String::new(),
        family_id: None,
        legal_status: String::new(),
        citations: "[]".into(),
        cited_by: "[]".into(),
        source: "serpapi".into(),
        raw_json: r.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        images: "[]".into(),
        pdf_url: String::new(),
    }
}

/// 精确直查命中后 `/api/search/online` 的响应形状。
///
/// 字段集合与旧 `try_exact_patent_lookup` 尾部构造的 JSON 完全一致
/// （`patents/total/page/page_size/source`，`source` 恒为 `"serpapi_exact"`）。
pub(crate) fn exact_lookup_response(summary: PatentSummary) -> serde_json::Value {
    serde_json::json!({
        "patents": [summary],
        "total": 1,
        "page": 1,
        "page_size": 10,
        "source": "serpapi_exact"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    // MA2a 起这些判定只在 `cfg(test)` 下被本模块用到（生产侧改走 `rank_and_gate_hits`）。
    // 留在父模块的 `use` 会让非 test 构建报 unused_imports，故在此就地导入。
    use crate::search::merge::dedup_patent_summaries;
    use crate::search::model::Lang;
    use crate::search::relevance::{
        calculate_online_relevance, contains_cjk, is_online_result_relevant,
    };
    use serde_json::json;

    fn db() -> Arc<Database> {
        Arc::new(Database::init(":memory:").expect("in-memory db"))
    }

    fn q(keyword: &str) -> SearchQuery {
        SearchQuery {
            keyword: keyword.to_string(),
            country: None,
            language: Some(Lang::English),
            assignee: None,
            exact_assignee: false,
            date_from: None,
            date_to: None,
            limit: 0,
            page: 1,
            sort_by: None,
            search_type: None,
        }
    }

    /// **改造前**的 URL 拼装（从 `git show b08f1c5:src/routes/search.rs` 的 `format!` 逐字复制），
    /// 只作为测试参照实现存在；生产代码走 [`SerpApiProvider::search_url`]。
    ///
    /// 入参取「已渲染好的 q 串 + 4 个散装参数」，与旧代码里 q 串由
    /// `build_online_query` 先算出、再拼进 URL 的分层一致；
    /// q 串本身的等价性由 `query::render_q_matches_pre_migration_implementation` 单独锁死。
    fn legacy_url_from_q(
        search_query: &str,
        page: usize,
        country: Option<&str>,
        sort_by: Option<&str>,
        is_cn_query: bool,
        api_key: &str,
    ) -> String {
        let serp_page = if page < 1 { 1 } else { page };
        let country_param = match country {
            Some(c) if !c.is_empty() => format!("&country={}", c),
            _ => String::new(),
        };
        let sort_param = match sort_by {
            Some("new") => "&sort=new",
            Some("old") => "&sort=old",
            _ => "",
        };
        let lang_param = if is_cn_query {
            "&hl=zh-cn&gl=cn&lr=lang_zh-CN"
        } else {
            ""
        };
        format!(
            "https://serpapi.com/search.json?engine=google_patents&q={}&page={}{}{}{}&api_key={}",
            urlencoding::encode(search_query),
            serp_page,
            country_param,
            sort_param,
            lang_param,
            api_key
        )
    }

    /// 同一 query 改造前后上游请求参数**逐字符一致**（DoD「行为保持」的核心证据）。
    #[test]
    fn search_url_matches_legacy_builder() {
        use crate::types::search::SearchType;
        let cases: Vec<SearchQuery> = vec![
            q("foldable hinge"),
            SearchQuery {
                page: 0, // 钳制到 1
                ..q("foldable hinge")
            },
            SearchQuery {
                page: 3,
                ..q("foldable hinge")
            },
            SearchQuery {
                country: Some("CN".to_string()),
                ..q("固态电池")
            },
            SearchQuery {
                country: Some(String::new()), // 空串不下发 country 参数
                ..q("固态电池")
            },
            SearchQuery {
                language: Some(Lang::Chinese),
                country: Some("CN".to_string()),
                ..q("固态电池")
            },
            SearchQuery {
                sort_by: Some("new".to_string()),
                ..q("hinge")
            },
            SearchQuery {
                sort_by: Some("relevance".to_string()), // 不识别的值不下发
                ..q("hinge")
            },
            SearchQuery {
                search_type: Some(SearchType::Applicant),
                date_from: Some("20200101".to_string()),
                date_to: Some("20241231".to_string()),
                language: Some(Lang::Chinese),
                page: 7,
                sort_by: Some("old".to_string()),
                country: Some("US".to_string()),
                ..q("张三")
            },
            SearchQuery {
                search_type: Some(SearchType::PatentNumber),
                ..q("CN202420009882.7")
            },
        ];
        for query in &cases {
            // q 串走**迁移前**的渲染实现；`is_cn_query` 只是把入参里已有的 language 字段
            // 还原成旧的那个 bool（纯投影，不涉及判定规则——规则等价性由
            // `query::resolve_lang_matches_pre_migration_region_flags` 单独证明）。
            let legacy_q = crate::search::query::test_support::legacy_build_online_query(
                &query.keyword,
                query.search_type.as_ref(),
                query.date_from.as_deref(),
                query.date_to.as_deref(),
            );
            let legacy = legacy_url_from_q(
                &legacy_q,
                query.page,
                query.country.as_deref(),
                query.sort_by.as_deref(),
                is_cn_query(query),
                "KEY123",
            );
            assert_eq!(
                legacy,
                SerpApiProvider::search_url(query, "KEY123"),
                "keyword={}",
                query.keyword
            );
        }
    }

    /// 具体 golden：肉眼可读地钉住一条真实形态的 URL。
    #[test]
    fn search_url_golden_cn_query() {
        let query = SearchQuery {
            language: Some(Lang::Chinese),
            country: Some("CN".to_string()),
            sort_by: Some("new".to_string()),
            page: 2,
            ..q("固态电池")
        };
        assert_eq!(
            "https://serpapi.com/search.json?engine=google_patents&q=%E5%9B%BA%E6%80%81%E7%94%B5%E6%B1%A0&page=2&country=CN&sort=new&hl=zh-cn&gl=cn&lr=lang_zh-CN&api_key=K",
            SerpApiProvider::search_url(&query, "K")
        );
    }

    /// details 端点 URL 亦与旧实现一致。
    #[test]
    fn details_url_matches_legacy() {
        let patent_id = "patent/CN109876543A/zh";
        assert_eq!(
            format!(
                "https://serpapi.com/search.json?engine=google_patents_details&patent_id={}&api_key=K",
                urlencoding::encode(patent_id)
            ),
            SerpApiProvider::details_url(patent_id, "K")
        );
    }

    fn fixture_results() -> Vec<serde_json::Value> {
        vec![
            json!({
                "publication_number": "CN1123456A",
                "title": "固态电池",
                "snippet": "一种全固态电池结构。",
                "assignee": "某研究院",
                "inventor": "张三",
                "filing_date": "2024-01-01",
                "publication_date": "2024-06-01",
                "country_status": "active"
            }),
            json!({
                "publication_number": "CN1123457B",
                "title": "固态电池制造方法",
                "snippet": "涉及电池制造。",
                "assignee": "某大学",
                "inventor": "李四",
                "filing_date": "2024-02-02",
                "publication_date": "2024-07-02"
            }),
            json!({
                "publication_number": "US9000000B2",
                "title": "Unrelated dust collector",
                "snippet": "industrial dust collector device",
                "assignee": "Acme",
                "inventor": "Nobody"
            }),
            json!({
                "publication_number": "US1000000B1",
                "title": "",
                "snippet": "空标题应被丢弃（旧行为）"
            }),
        ]
    }

    /// **改造前**的结果映射参照实现（旧 `api_search_online` 内联逻辑的等价转写，
    /// 打分/过滤/去重顺序与文案均未动），用于逐字段比对。
    fn legacy_map(keyword: &str, cn: bool, results: &[serde_json::Value]) -> Vec<PatentSummary> {
        let mut patents = Vec::new();
        for (idx, r) in results.iter().enumerate() {
            let p = serp_to_patent(r);
            if !p.title.is_empty() {
                let saved_id = p.id.clone();
                let position_score = (98.0 - idx as f64 * 3.0).max(30.0);
                let content_score = calculate_online_relevance(
                    keyword,
                    &p.title,
                    &p.abstract_text,
                    &p.applicant,
                    &p.inventor,
                );
                if !is_online_result_relevant(
                    keyword,
                    &p.title,
                    &p.abstract_text,
                    content_score,
                    cn,
                    &p.inventor,
                ) {
                    continue;
                }
                let score = (position_score * 0.4 + content_score * 0.6).min(100.0);
                let source = format!(
                    "hybrid(pos:{:.0}+content:{:.0})",
                    position_score, content_score
                );
                patents.push(PatentSummary {
                    id: saved_id,
                    patent_number: p.patent_number.clone(),
                    title: p.title.clone(),
                    abstract_text: p.abstract_text.clone(),
                    applicant: p.applicant.clone(),
                    inventor: p.inventor.clone(),
                    filing_date: p.filing_date.clone(),
                    country: p.country.clone(),
                    relevance_score: Some(score),
                    score_source: Some(source),
                });
            }
        }
        if cn {
            let zh: Vec<PatentSummary> = patents
                .iter()
                .filter(|p| contains_cjk(&p.title) || contains_cjk(&p.abstract_text))
                .cloned()
                .collect();
            if !zh.is_empty() {
                patents = zh;
            }
        }
        dedup_patent_summaries(patents)
    }

    /// 结果映射等价：与旧映射参照实现逐字段比对（只比 `id` 以外的字段——新实现回填真实
    /// stored id，参照实现同样走 `insert_patent` 但拿不到同一 UUID，故显式排除）。
    /// 比对前按 `patent_number` 排序：旧代码的去重走 `HashMap`，同分项的先后本就不确定，
    /// 用排序后比较避免把这个既有不确定性当成迁移差异。
    #[test]
    fn outcome_patents_match_legacy_mapping() {
        fn comparable(mut items: Vec<PatentSummary>) -> Vec<PatentSummary> {
            for it in items.iter_mut() {
                it.id = String::new();
            }
            items.sort_by(|a, b| a.patent_number.cmp(&b.patent_number));
            items
        }
        for cn in [false, true] {
            let provider = SerpApiProvider::new("K".to_string(), db());
            let results = fixture_results();
            let new = comparable(provider.outcome_patents("固态电池", cn, &results));
            let old = comparable(legacy_map("固态电池", cn, &results));
            assert_eq!(old.len(), new.len(), "cn={}", cn);
            assert_eq!(old, new, "cn={}", cn);
        }
    }

    /// 中文语境下英文噪声被过滤掉（PRD N2 的最小回归锚点），且命中的两条按分数降序。
    #[test]
    fn cn_query_keeps_only_chinese_hits() {
        let provider = SerpApiProvider::new("K".to_string(), db());
        let out = provider.outcome_patents("固态电池", true, &fixture_results());
        assert_eq!(2, out.len());
        assert!(out.iter().all(|p| p.country.starts_with("CN")));
        assert_eq!("固态电池", out[0].title);
        let scores: Vec<f64> = out.iter().filter_map(|p| p.relevance_score).collect();
        assert!(
            scores.windows(2).all(|w| w[0] >= w[1]),
            "必须按分数降序: {:?}",
            scores
        );
        // 混合分公式与 score_source 文案逐字保持：pos = (98 - idx*3).max(30)，
        // score = pos*0.4 + content*0.6（封顶 100）。此处首条命中 content = 95
        // （标题全等 +50、摘要包含 +15、基础分 30）。
        assert_eq!(
            Some((98.0f64 * 0.4 + 95.0f64 * 0.6).min(100.0)),
            out[0].relevance_score
        );
        assert_eq!(
            Some("hybrid(pos:98+content:95)".to_string()),
            out[0].score_source
        );
    }

    /// 发明人姓名精确命中时直接放行（PRD N2 的姓名检索锚点），
    /// 但中文语境仍会剔除标题/摘要无中文的英文噪声——两道判定的先后次序即旧行为。
    #[test]
    fn inventor_hit_passes_gate_yet_cn_filter_still_applies() {
        let results = vec![
            json!({
                "publication_number": "US9000001B2",
                "title": "Display assembly",
                "snippet": "A display assembly for an electronic device.",
                "assignee": "Acme",
                "inventor": "张三"
            }),
            json!({
                "publication_number": "CN1123457A",
                "title": "一种显示模组",
                "snippet": "涉及一种显示模组。",
                "assignee": "某公司",
                "inventor": "张三"
            }),
        ];
        let provider = SerpApiProvider::new("K".to_string(), db());
        let titles =
            |v: &Vec<PatentSummary>| v.iter().map(|p| p.title.clone()).collect::<Vec<String>>();

        let cn = provider.outcome_patents("张三", true, &results);
        assert_eq!(vec!["一种显示模组".to_string()], titles(&cn));

        // 非中文语境不启用语言过滤，两条都保留
        let intl = provider.outcome_patents("张三", false, &results);
        assert_eq!(2, intl.len());
        assert!(intl.iter().any(|p| p.country == "US"));
        assert!(intl.iter().any(|p| p.country == "CN"));
    }

    /// 命中结果确实写进了本地库（旧行为：在线结果缓存入库，断网可复检的前置）。
    #[test]
    fn online_hits_are_cached_to_db() {
        let db = db();
        let provider = SerpApiProvider::new("K".to_string(), db.clone());
        let out = provider.outcome_patents("固态电池", true, &fixture_results());
        assert!(!out.is_empty());
        for s in &out {
            assert!(!s.id.is_empty());
            let stored = db
                .get_patents_by_ids(std::slice::from_ref(&s.id))
                .unwrap_or_default();
            assert_eq!(1, stored.len(), "结果应已入库: {}", s.patent_number);
        }
    }

    /// 精确直查的响应形状 = 旧 `try_exact_patent_lookup` 尾部 JSON。
    #[test]
    fn exact_lookup_response_shape_is_locked() {
        let summary = PatentSummary {
            id: "id-1".to_string(),
            patent_number: "CN1123456A".to_string(),
            title: "固态电池".to_string(),
            abstract_text: String::new(),
            applicant: String::new(),
            inventor: "张三".to_string(),
            filing_date: String::new(),
            country: "CN".to_string(),
            relevance_score: Some(100.0),
            score_source: Some("exact_lookup".to_string()),
        };
        let value = exact_lookup_response(summary.clone());
        assert_eq!(
            json!({
                "patents": [summary],
                "total": 1,
                "page": 1,
                "page_size": 10,
                "source": "serpapi_exact"
            }),
            value
        );
    }

    /// FailKind 归类只做记账，且严格不触碰用户可见文案（MA1 行为保持的护栏）。
    #[test]
    fn fail_kind_classification_does_not_affect_hints() {
        let code = |n: u16| {
            reqwest::StatusCode::from_u16(n).expect("fixture status code must be a valid http code")
        };
        assert_eq!(FailKind::Quota, fail_kind_for_status(code(429)));
        assert_eq!(FailKind::Auth, fail_kind_for_status(code(403)));
        assert_eq!(FailKind::Network, fail_kind_for_status(code(503)));
        assert_eq!(FailKind::Auth, fail_kind_for_error("invalid api key"));
        assert_eq!(FailKind::Parse, fail_kind_for_error("something odd"));
    }
}
