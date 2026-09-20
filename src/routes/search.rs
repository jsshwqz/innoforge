use super::{escape_csv, parse_search_type, AppState};
use crate::patent::*;
// MA1：q 串渲染 / 相关性判定 / 排序去重 / 上游调用链均已抽到 `crate::search`，
// 本文件只保留「端点」职责：请求解析、响应形状、超时预算与本地兜底。
use crate::search::merge::{dedup_patent_summaries, sort_by_relevance};
use crate::search::model::{
    AttemptReport, AttemptStatus, Lang, SearchOutcome, SearchQuery, SourceKind,
};
use crate::search::provider::SearchProvider;
use crate::search::providers::serpapi::{exact_lookup_response, SerpApiProvider};
use crate::search::query::resolve_lang;
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::time::Instant;

const ONLINE_TOTAL_BUDGET_SECS: u64 = 60;

/// 没有可用 Key 时的「本源未参与」结果。
///
/// 旧代码在 `api_key_opt` 为 `None` 时只设一句 `upstream_hint` 然后落到本地兜底；
/// 这里用同一份文案构造等价的 [`SearchOutcome`]，使后续控制流（hint / 预算 / 兜底）
/// 与在线源真正跑过之后完全走同一条路径，避免两份分支各自演化。
fn serpapi_skipped_outcome(hint: &str) -> SearchOutcome {
    SearchOutcome {
        results: vec![],
        attempts: vec![AttemptReport {
            source: SourceKind::SerpApi,
            status: AttemptStatus::Skipped,
            latency_ms: 0,
            hits: 0,
            error: Some("未配置 SERPAPI_KEY，本轮未发起在线请求".to_string()),
            hint: Some(hint.to_string()),
        }],
        upstream_total: None,
    }
}

pub async fn api_search(
    State(s): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Json<SearchResult> {
    // 空 query 校验
    if req.query.trim().is_empty() {
        return Json(SearchResult {
            patents: vec![],
            total: 0,
            page: req.page,
            page_size: req.page_size,
            search_type: Some("mixed".into()),
            dedup_removed: 0,
            categories: None,
        });
    }

    let search_type = parse_search_type(req.search_type.as_deref());
    let (mut patents, total, detected_type) = match s.db.search_smart(
        &req.query,
        search_type.as_ref(),
        req.country.as_deref(),
        req.date_from.as_deref(),
        req.date_to.as_deref(),
        req.page,
        req.page_size,
    ) {
        Ok((patents, total, search_type)) => (patents, total, search_type),
        Err(e) => {
            tracing::error!("search_smart failed: {}", e);
            (vec![], 0, SearchType::Mixed)
        }
    };

    // IPC/CPC post-filtering: batch-fetch patents to avoid N+1 queries
    let ipc_filter = req.ipc.as_deref().unwrap_or("").trim().to_lowercase();
    let cpc_filter = req.cpc.as_deref().unwrap_or("").trim().to_lowercase();
    if !ipc_filter.is_empty() || !cpc_filter.is_empty() {
        // Batch query to avoid N+1 (AGENTS.md 2.7)
        let ids: Vec<String> = patents.iter().map(|p| p.id.clone()).collect();
        let full_patents = s.db.get_patents_by_ids(&ids).unwrap_or_default();
        let cache: std::collections::HashMap<String, crate::patent::Patent> = full_patents
            .into_iter()
            .map(|p| (p.id.clone(), p))
            .collect();
        patents.retain(|p| {
            let matches_ipc = if ipc_filter.is_empty() {
                true
            } else if let Some(full) = cache.get(&p.id) {
                full.ipc_codes.to_lowercase().contains(&ipc_filter)
            } else {
                false
            };
            let matches_cpc = if cpc_filter.is_empty() {
                true
            } else if let Some(full) = cache.get(&p.id) {
                full.cpc_codes.to_lowercase().contains(&cpc_filter)
            } else {
                false
            };
            matches_ipc && matches_cpc
        });
    }

    // Deduplication: remove patents with same base number (e.g. CN123456A vs CN123456B)
    let pre_dedup_count = patents.len();
    let mut seen_base_numbers = std::collections::HashSet::new();
    patents.retain(|p| {
        let base = crate::patent::canonical_patent_key(&p.patent_number);
        seen_base_numbers.insert(base)
    });
    let dedup_removed = pre_dedup_count - patents.len();

    if let Some(sort_by) = req.sort_by.as_deref() {
        match sort_by {
            "new" => patents.sort_by(|a, b| b.filing_date.cmp(&a.filing_date)),
            "old" => patents.sort_by(|a, b| a.filing_date.cmp(&b.filing_date)),
            _ => sort_by_relevance(&mut patents),
        }
    } else {
        match detected_type {
            SearchType::Inventor | SearchType::Applicant => {
                sort_by_relevance(&mut patents);
                patents.retain(|p| p.relevance_score.unwrap_or(0.0) >= 50.0);
            }
            _ => sort_by_relevance(&mut patents),
        }
    }

    // Build category statistics for large result sets
    let categories = if patents.len() >= 10 {
        let mut by_applicant: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut by_country: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for p in &patents {
            let app = if p.applicant.is_empty() {
                "未知".to_string()
            } else {
                // Normalize applicant name (take first 20 chars to group variants)
                p.applicant.chars().take(20).collect()
            };
            *by_applicant.entry(app).or_insert(0) += 1;
            let country = if p.country.is_empty() {
                "未知".to_string()
            } else {
                p.country.clone()
            };
            *by_country.entry(country).or_insert(0) += 1;
        }
        let mut groups: Vec<CategoryGroup> = Vec::new();
        // Top applicants
        let mut app_list: Vec<_> = by_applicant.into_iter().collect();
        app_list.sort_by_key(|item| std::cmp::Reverse(item.1));
        for (name, count) in app_list.iter().take(5) {
            if *count >= 2 {
                groups.push(CategoryGroup {
                    label: format!("申请人: {}", name),
                    count: *count,
                });
            }
        }
        // Countries
        let mut country_list: Vec<_> = by_country.into_iter().collect();
        country_list.sort_by_key(|item| std::cmp::Reverse(item.1));
        for (name, count) in country_list.iter().take(5) {
            groups.push(CategoryGroup {
                label: format!("国家: {}", name),
                count: *count,
            });
        }
        if groups.is_empty() {
            None
        } else {
            Some(groups)
        }
    } else {
        None
    };

    let search_type_str = match detected_type {
        SearchType::Applicant => "applicant",
        SearchType::Inventor => "inventor",
        SearchType::PatentNumber => "patent_number",
        SearchType::Keyword => "keyword",
        SearchType::Mixed => "mixed",
    };

    let final_total = if !ipc_filter.is_empty() || !cpc_filter.is_empty() {
        patents.len()
    } else if dedup_removed > 0 {
        total.saturating_sub(dedup_removed)
    } else {
        total
    };

    Json(SearchResult {
        patents,
        total: final_total,
        page: req.page,
        page_size: req.page_size,
        search_type: Some(search_type_str.to_string()),
        categories,
        dedup_removed,
    })
}

/// `POST /api/search/online` — 在线检索端点。
///
/// 端点职责：请求解析 → 区域判定 → 取 Key → 交给 [`SerpApiProvider`] → 按旧形状出参 →
/// 超时预算 → 本地库兜底。上游调用与结果映射见 `crate::search::providers::serpapi`。
pub async fn api_search_online(
    State(s): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Json<serde_json::Value> {
    println!(
        "[ONLINE] query='{}' page={} country={:?} region={:?}",
        req.query, req.page, req.country, req.region
    );
    let online_search_type = parse_search_type(req.search_type.as_deref())
        .or_else(|| Some(s.db.detect_search_type(&req.query)));

    // 空查询：不发上游请求，直接返回（形状与旧代码逐字一致）
    let query_trimmed = req.query.trim();
    if query_trimmed.is_empty() {
        return Json(json!({
            "patents": [],
            "total": 0,
            "page": req.page,
            "page_size": req.page_size,
            "message": "查询词为空，已跳过在线检索"
        }));
    }
    // 搜索区域判定：用户明确选择 > 自动检测。
    // 判定规则已迁至 `search::query::resolve_lang`（`Lang::Chinese` ⇔ 旧 `is_cn_query`，
    // `Lang::English` ⇔ 旧 `is_intl_query`），与旧实现逐用例等价，
    // 由 `resolve_lang_matches_pre_migration_region_flags` 锁死。
    let lang = resolve_lang(req.region.as_deref(), req.country.as_deref(), query_trimmed);
    let is_cn_query = matches!(lang, Lang::Chinese);
    println!(
        "[ONLINE] region resolve: is_cn={} is_intl={}",
        is_cn_query, !is_cn_query
    );
    let online_start = Instant::now();

    // 当前策略：在线检索仅走国外数据源链路（SerpAPI / Google Patents）
    // 不再进入 CNIPR / 百度 / 搜狗 分支。
    let search_query = SearchQuery {
        keyword: req.query.clone(),
        country: req.country.clone(),
        language: Some(lang),
        assignee: None,
        exact_assignee: false,
        date_from: req.date_from.clone(),
        date_to: req.date_to.clone(),
        limit: req.page_size,
        page: req.page,
        sort_by: req.sort_by.clone(),
        search_type: online_search_type.clone(),
    };

    // Round-robin 选取一个可用 Key —— 属配置层既有职责，MA1 不上提到 provider。
    let api_key_opt = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .next_serpapi_key();

    let outcome = match api_key_opt {
        Some(api_key) => {
            let provider = SerpApiProvider::new(api_key, s.db.clone());
            // ── 精确专利号查询：当检测为 PatentNumber 时，先尝试 SerpAPI Details 精确抓取 ──
            if matches!(online_search_type.as_ref(), Some(SearchType::PatentNumber)) {
                if let Some(summary) = provider.lookup_exact(req.query.clone()).await {
                    return Json(exact_lookup_response(summary));
                }
            }
            provider.search(search_query).await
        }
        None => {
            println!("[ONLINE] No SERPAPI_KEY configured");
            serpapi_skipped_outcome("未配置有效 SerpAPI Key，已自动尝试下游回退。")
        }
    };

    let mut upstream_hint = outcome.hint();

    // 上游成功且有命中 → 按旧形状直接返回（旧代码是在 SerpAPI 分支内提前 return）。
    if outcome.succeeded_from(SourceKind::SerpApi) {
        let mut out = json!({
            "patents": outcome.summaries(),
            "total": outcome.upstream_total.unwrap_or(0),
            "page": req.page,
            "page_size": 10,
            "source": SourceKind::SerpApi.as_str()
        });
        if let Some(h) = upstream_hint.take() {
            out["hint"] = json!(h);
        }
        return Json(out);
    }

    if online_start.elapsed().as_secs() >= ONLINE_TOTAL_BUDGET_SECS {
        let msg = format!(
            "在线检索超时预算已用尽（{}s），已跳过后续远端回退并改走本地兜底。",
            ONLINE_TOTAL_BUDGET_SECS
        );
        if upstream_hint.is_none() {
            upstream_hint = Some(msg);
        }
    }

    // 仅使用 SerpAPI 搜索，所有其他搜索源（Firecrawl/Google Patents直连/Bing/CNIPR/搜狗）已屏蔽
    // 在线搜索无结果时直接回退本地数据库

    // Fallback 2: local DB search
    println!("[ONLINE] Falling back to local DB");
    let local =
        s.db.search_smart(
            &req.query,
            online_search_type.as_ref(),
            req.country.as_deref(),
            req.date_from.as_deref(),
            req.date_to.as_deref(),
            req.page,
            req.page_size,
        )
        .ok()
        .map(|(p, t, _)| (p, t));
    if let Some((patents, total)) = local {
        let patents = dedup_patent_summaries(patents);
        if total > 0 {
            let hint_text = upstream_hint.unwrap_or_else(|| {
                "国外在线源暂时未返回结果，已回退本地缓存。建议配置 SerpAPI 提升命中率。"
                    .to_string()
            });
            let dedup_total = total.max(patents.len());
            return Json(json!({
                "patents": patents,
                "total": dedup_total,
                "page": req.page,
                "page_size": req.page_size,
                "source": "local",
                "hint": hint_text
            }));
        }
    }
    let enc = urlencoding::encode(&req.query);
    let mut out = json!({
        "patents": [], "total": 0, "page": 1, "page_size": 20,
        "google_url": format!("https://patents.google.com/?q={enc}&oq={enc}"),
        "message": "未找到结果，可尝试在 Google Patents 上搜索"
    });
    if let Some(h) = upstream_hint {
        out["hint"] = json!(h);
    }
    Json(out)
}

/// Vector hybrid search endpoint — RRF fuse BM25 + vector similarity.
pub async fn api_search_vector(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    use crate::patent::SearchType;

    let query = req["query"].as_str().unwrap_or("");
    let limit = req["limit"].as_u64().unwrap_or(20) as usize;
    let k = req["rrf_k"].as_u64().unwrap_or(60) as usize;

    if query.is_empty() {
        return Json(json!({"error": "查询不能为空"}));
    }

    // BM25 layer
    let bm25_result: Vec<(String, f64)> =
        match s
            .db
            .search_smart(query, Some(&SearchType::Mixed), None, None, None, 1, limit)
        {
            Ok((patents, _total, _detected)) => patents
                .into_iter()
                .enumerate()
                .map(|(i, p)| (p.id, (limit as f64 - i as f64) / limit as f64))
                .collect(),
            Err(e) => {
                tracing::warn!("BM25 search failed: {}", e);
                vec![]
            }
        };

    // Vector layer: compute query embedding and search cached embeddings
    let vector_results: Vec<(String, f32)> = {
        let query_embedding = compute_char_tfidf_embedding(query);
        let count = s.db.count_embeddings().unwrap_or_default();
        let mut results = Vec::new();

        if count > 0 {
            if let Ok(mut stmt) =
                s.db.conn()
                    .prepare("SELECT patent_id, embedding FROM patents_embedding")
            {
                if let Ok(rows) = stmt.query_map(rusqlite::params![], |row: &rusqlite::Row| {
                    let pid: String = row.get(0)?;
                    let blob_val: rusqlite::types::Value = row.get(1)?;
                    match blob_val {
                        rusqlite::types::Value::Blob(bytes) => {
                            let mut emb = Vec::with_capacity(bytes.len() / 4);
                            for i in (0..bytes.len()).step_by(4) {
                                if i + 3 < bytes.len() {
                                    let b = [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]];
                                    emb.push(f32::from_le_bytes(b));
                                }
                            }
                            Ok((pid, emb))
                        }
                        _ => Ok((pid, vec![])),
                    }
                }) {
                    for (pid, emb) in rows.flatten() {
                        let sim = cosine_similarity(&query_embedding, &emb);
                        if sim > 0.1 {
                            results.push((pid, sim));
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    };

    // RRF fuse
    let fused: Vec<(String, f64)> = {
        use std::collections::HashMap;
        let mut scores: HashMap<String, f64> = HashMap::new();
        for (i, (id, _)) in bm25_result.iter().enumerate() {
            let rank = i + 1;
            *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (k as f64 + rank as f64);
        }
        for (i, (id, _)) in vector_results.iter().enumerate() {
            let rank = i + 1;
            *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (k as f64 + rank as f64);
        }
        let mut sorted: Vec<(String, f64)> = scores.into_iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted
    };

    let mut top_patents: Vec<serde_json::Value> = Vec::new();
    for (id, fused_score) in fused.into_iter().take(limit) {
        if let Ok(Some(patent)) = s.db.get_patent(&id) {
            let score_percent = (fused_score * 100.0).round() * 100.0 / 100.0;
            top_patents.push(json!({
                "id": patent.id,
                "patent_number": patent.patent_number,
                "title": patent.title,
                "applicant": patent.applicant,
                "inventor": patent.inventor,
                "relevance_score": score_percent,
                "fused_score": fused_score,
            }));
        }
    }

    Json(json!({
        "patents": top_patents,
        "total": top_patents.len(),
        "source": "hybrid",
        "method": "rrf",
        "rrf_k": k,
        "bm25_count": bm25_result.len(),
        "vector_count": vector_results.len(),
    }))
}

/// Character n-gram TF-IDF embedding computation.
fn compute_char_tfidf_embedding(text: &str) -> Vec<f32> {
    use std::collections::HashMap;

    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let mut tf: HashMap<String, f32> = HashMap::new();
    for n in 2..=4 {
        if cleaned.len() >= n {
            for i in 0..=cleaned.len() - n {
                let gram: String = cleaned[i..i + n].chars().collect();
                *tf.entry(gram).or_insert(0.0) += 1.0;
            }
        }
    }
    if tf.is_empty() {
        return vec![0.0f32];
    }
    let doc_len = tf.values().sum::<f32>();
    for count in tf.values_mut() {
        *count = 1.0 + (*count / doc_len).log2();
    }
    let norm_sq: f32 = tf.values().map(|v| v * v).sum();
    let norm = if norm_sq > 0.0 { norm_sq.sqrt() } else { 1.0 };
    let mut emb: Vec<f32> = tf.values().map(|v| v / norm).collect();
    emb.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    const FIXED: usize = 512;
    if emb.len() < FIXED {
        emb.resize(FIXED, 0.0);
    } else {
        emb.truncate(FIXED);
    }
    emb
}

/// Cosine similarity between two vectors.
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
pub async fn api_search_stats(
    State(s): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Json<serde_json::Value> {
    let search_type = parse_search_type(req.search_type.as_deref());
    let all_results = match s.db.search_smart(
        &req.query,
        search_type.as_ref(),
        req.country.as_deref(),
        req.date_from.as_deref(),
        req.date_to.as_deref(),
        1,
        10000,
    ) {
        Ok((p, _, _)) => p,
        Err(_) => vec![],
    };

    let mut applicant_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut country_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut year_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for p in &all_results {
        let applicant = if p.applicant.is_empty() {
            "未知".to_string()
        } else {
            p.applicant.clone()
        };
        *applicant_counts.entry(applicant).or_insert(0) += 1;

        let country = if p.country.is_empty() {
            "未知".to_string()
        } else {
            p.country.clone()
        };
        *country_counts.entry(country).or_insert(0) += 1;

        let year = p.filing_date.chars().take(4).collect::<String>();
        if year.len() == 4 {
            *year_counts.entry(year).or_insert(0) += 1;
        }
    }

    let mut applicants: Vec<_> = applicant_counts.into_iter().collect();
    applicants.sort_by_key(|item| std::cmp::Reverse(item.1));
    let top_applicants: Vec<_> = applicants.into_iter().take(10).collect();

    let mut countries: Vec<_> = country_counts.into_iter().collect();
    countries.sort_by_key(|item| std::cmp::Reverse(item.1));

    let mut years: Vec<_> = year_counts.into_iter().collect();
    years.sort_by(|a, b| a.0.cmp(&b.0));

    Json(json!({
        "total": all_results.len(),
        "applicants": top_applicants,
        "countries": countries,
        "years": years,
    }))
}

pub async fn api_export_csv(
    State(s): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> axum::response::Response {
    let search_type = parse_search_type(req.search_type.as_deref());
    let all_results = match s.db.search_smart(
        &req.query,
        search_type.as_ref(),
        req.country.as_deref(),
        req.date_from.as_deref(),
        req.date_to.as_deref(),
        1,
        10000,
    ) {
        Ok((p, _, _)) => p,
        Err(_) => vec![],
    };

    let mut csv_data = String::from("专利号,标题,申请人,发明人,申请日,公开日,国家/地区,摘要\n");
    for p in all_results {
        let abstract_preview: String = p.abstract_text.chars().take(150).collect();
        let row = format!(
            "{},{},{},{},{},{},{},{}\n",
            escape_csv(&p.patent_number),
            escape_csv(&p.title),
            escape_csv(&p.applicant),
            escape_csv(&p.inventor),
            escape_csv(&p.filing_date),
            escape_csv(&p.filing_date),
            escape_csv(&p.country),
            escape_csv(&abstract_preview)
        );
        csv_data.push_str(&row);
    }

    let filename = format!("patents_{}.csv", chrono::Utc::now().format("%Y%m%d_%H%M%S"));

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                &format!("attachment; filename=\"{}\"", filename),
            ),
        ],
        format!("\u{FEFF}{}", csv_data),
    )
        .into_response()
}

pub async fn api_export_xlsx(
    State(s): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> impl IntoResponse {
    let search_type = parse_search_type(req.search_type.as_deref());
    let all_results = match s.db.search_smart(
        &req.query,
        search_type.as_ref(),
        req.country.as_deref(),
        req.date_from.as_deref(),
        req.date_to.as_deref(),
        1,
        10000,
    ) {
        Ok((p, _, _)) => p,
        Err(_) => vec![],
    };

    let mut workbook = rust_xlsxwriter::Workbook::new();
    let sheet = workbook.add_worksheet();

    // Header style
    let header_format = rust_xlsxwriter::Format::new().set_bold();

    let headers = [
        "Patent No.",
        "Title",
        "Applicant",
        "Inventor",
        "Filing Date",
        "Country",
        "Abstract",
    ];
    for (col, h) in headers.iter().enumerate() {
        let _ = sheet.write_string_with_format(0, col as u16, *h, &header_format);
    }

    for (row, p) in all_results.iter().enumerate() {
        let r = (row + 1) as u32;
        let _ = sheet.write_string(r, 0, &p.patent_number);
        let _ = sheet.write_string(r, 1, &p.title);
        let _ = sheet.write_string(r, 2, &p.applicant);
        let _ = sheet.write_string(r, 3, &p.inventor);
        let _ = sheet.write_string(r, 4, &p.filing_date);
        let _ = sheet.write_string(r, 5, &p.country);
        let abstract_preview: String = p.abstract_text.chars().take(200).collect();
        let _ = sheet.write_string(r, 6, &abstract_preview);
    }

    // Set column widths
    let _ = sheet.set_column_width(0, 18);
    let _ = sheet.set_column_width(1, 40);
    let _ = sheet.set_column_width(2, 25);
    let _ = sheet.set_column_width(3, 20);
    let _ = sheet.set_column_width(4, 12);
    let _ = sheet.set_column_width(5, 8);
    let _ = sheet.set_column_width(6, 50);

    match workbook.save_to_buffer() {
        Ok(buffer) => {
            let filename = format!(
                "patents_{}.xlsx",
                chrono::Utc::now().format("%Y%m%d_%H%M%S")
            );
            (
                StatusCode::OK,
                [
                    (
                        header::CONTENT_TYPE,
                        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                            .to_string(),
                    ),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{}\"", filename),
                    ),
                ],
                buffer,
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to generate Excel: {}", e),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── `/api/search/online` 的四条终态响应形状（MA1 抽出前后对照，逐字来自
    // `git show b08f1c5:src/routes/search.rs` 的 `api_search_online`）──
    //
    // | 路径 | 字段 | 锁定该形状的测试 |
    // |---|---|---|
    // | 空查询 | `patents/total/page/page_size/message` | `api_search_online` 的 early-return（未改） |
    // | 精确直查命中 | `patents/total=1/page=1/page_size=10/source=serpapi_exact` | `providers::serpapi::exact_lookup_response_shape_is_locked` |
    // | 在线命中 | `patents/total/page/page_size=10/source=serpapi(+hint)` | `online_hit_response_keeps_legacy_shape` |
    // | 本地兜底 | `patents/total/page/page_size/source=local/hint` | 兜底段（未改） |
    //
    // 上游请求参数与结果映射的一致性证据在 `src/search/` 侧：
    // `query::render_q_matches_pre_migration_implementation`、
    // `query::resolve_lang_matches_pre_migration_region_flags`、
    // `providers::serpapi::search_url_matches_legacy_builder`、
    // `providers::serpapi::outcome_patents_match_legacy_mapping`。

    /// 未配置 Key 的路径必须与「源跑过但零命中」走同一条后续控制流：
    /// hint 文案逐字保持旧值，且不得被误判为在线命中。
    #[test]
    fn no_key_outcome_keeps_legacy_hint_and_falls_back() {
        let outcome = serpapi_skipped_outcome("未配置有效 SerpAPI Key，已自动尝试下游回退。");
        assert_eq!(
            Some("未配置有效 SerpAPI Key，已自动尝试下游回退。".to_string()),
            outcome.hint()
        );
        assert!(!outcome.succeeded_from(SourceKind::SerpApi));
        assert!(outcome.summaries().is_empty());
        assert_eq!(None, outcome.upstream_total);
    }

    /// 在线命中时 `SearchOutcome` 能还原出与旧代码同形的 JSON 片段
    /// （`total` 取上游 `search_information.total_results`，`page_size` 恒为 10）。
    #[test]
    fn online_hit_response_keeps_legacy_shape() {
        let summary = PatentSummary {
            id: "saved-1".to_string(),
            patent_number: "CN1123456A".to_string(),
            title: "固态电池".to_string(),
            abstract_text: "一种全固态电池结构。".to_string(),
            applicant: "某研究院".to_string(),
            inventor: "张三".to_string(),
            filing_date: "2024-01-01".to_string(),
            country: "CN".to_string(),
            relevance_score: Some(87.2),
            score_source: Some("hybrid(pos:98+content:80)".to_string()),
        };
        let outcome = SearchOutcome {
            results: crate::search::merge::merged_from(SourceKind::SerpApi, vec![summary.clone()]),
            attempts: vec![AttemptReport {
                source: SourceKind::SerpApi,
                status: AttemptStatus::Success,
                latency_ms: 120,
                hits: 1,
                error: None,
                hint: None,
            }],
            upstream_total: Some(1427),
        };

        assert!(outcome.succeeded_from(SourceKind::SerpApi));
        let mut out = json!({
            "patents": outcome.summaries(),
            "total": outcome.upstream_total.unwrap_or(0),
            "page": 2usize,
            "page_size": 10,
            "source": SourceKind::SerpApi.as_str()
        });
        if let Some(h) = outcome.hint() {
            out["hint"] = json!(h);
        }
        assert_eq!(
            json!({
                "patents": [summary],
                "total": 1427,
                "page": 2,
                "page_size": 10,
                "source": "serpapi"
            }),
            out
        );
        // hint 缺失时不得凭空多出该键（旧代码只在 Some 时插入）
        assert!(out.get("hint").is_none());
    }
}
