use super::{build_online_query, escape_csv, parse_search_type, AppState};
use crate::patent::*;
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::time::Instant;

const ONLINE_UPSTREAM_TIMEOUT_SECS: u64 = 30;
const ONLINE_TOTAL_BUDGET_SECS: u64 = 60;

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
        let mut cache: std::collections::HashMap<String, crate::patent::Patent> =
            std::collections::HashMap::new();
        for p in &patents {
            if let Ok(Some(full)) = s.db.get_patent(&p.id) {
                cache.insert(p.id.clone(), full);
            }
        }
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

fn sort_by_relevance(patents: &mut [PatentSummary]) {
    patents.sort_by(|a, b| {
        let sa = a.relevance_score.unwrap_or(0.0);
        let sb = b.relevance_score.unwrap_or(0.0);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
}

fn dedup_patent_summaries(items: Vec<PatentSummary>) -> Vec<PatentSummary> {
    let mut best_by_key: std::collections::HashMap<String, PatentSummary> =
        std::collections::HashMap::new();
    for item in items {
        let key = crate::patent::canonical_patent_key(&item.patent_number);
        let dedup_key = if key.is_empty() {
            format!("TITLE::{}", item.title.trim().to_uppercase())
        } else {
            key
        };
        match best_by_key.get_mut(&dedup_key) {
            None => {
                best_by_key.insert(dedup_key, item);
            }
            Some(existing) => {
                let old_score = existing.relevance_score.unwrap_or(0.0);
                let new_score = item.relevance_score.unwrap_or(0.0);
                let old_info = existing.title.len()
                    + existing.abstract_text.len()
                    + existing.applicant.len()
                    + existing.inventor.len();
                let new_info = item.title.len()
                    + item.abstract_text.len()
                    + item.applicant.len()
                    + item.inventor.len();
                if new_score > old_score || (new_score == old_score && new_info > old_info) {
                    *existing = item;
                }
            }
        }
    }
    let mut out: Vec<PatentSummary> = best_by_key.into_values().collect();
    sort_by_relevance(&mut out);
    out
}

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

    // 搜索区域判定：用户明确选择 > 自动检测
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
    let looks_like_cn_patent_number = {
        let digits_only: String = query_trimmed
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect();
        digits_only.len() >= 10
            && digits_only.len() <= 15
            && query_trimmed
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.')
    };
    let auto_cn = matches!(req.country.as_deref(), Some("CN"))
        || query_trimmed.starts_with("CN")
        || query_trimmed.starts_with("ZL")
        || looks_like_cn_patent_number
        || query_trimmed
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));

    let is_cn_query = match req.region.as_deref() {
        Some("cn") => true,    // 用户明确选国内
        Some("intl") => false, // 用户明确选国外
        _ => auto_cn,          // 自动检测
    };
    let is_intl_query = match req.region.as_deref() {
        Some("intl") => true,
        Some("cn") => false,
        _ => !auto_cn,
    };
    println!(
        "[ONLINE] region resolve: is_cn={} is_intl={}",
        is_cn_query, is_intl_query
    );
    let mut upstream_hint: Option<String> = None;
    let online_start = Instant::now();
    let mut _remote_budget_exhausted = false;

    // 当前策略：在线检索仅走国外数据源链路（SerpAPI / Google Patents）
    // 不再进入 CNIPR / 百度 / 搜狗 分支。

    // ── 精确专利号查询：当检测为 PatentNumber 时，先尝试 SerpAPI Details 精确抓取 ──
    // Round-robin 选取一个可用 Key
    let api_key_opt = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .next_serpapi_key();
    if matches!(online_search_type.as_ref(), Some(SearchType::PatentNumber)) {
        if let Some(ref api_key) = api_key_opt {
            if let Some(result) = try_exact_patent_lookup(&req.query, api_key, &s).await {
                return Json(result);
            }
        }
    }

    if let Some(ref api_key) = api_key_opt {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(ONLINE_UPSTREAM_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let serp_page = if req.page < 1 { 1 } else { req.page };
        let search_query = build_online_query(
            &req.query,
            online_search_type.as_ref(),
            req.date_from.as_deref(),
            req.date_to.as_deref(),
        );
        let country_param = match req.country.as_deref() {
            Some(c) if !c.is_empty() => format!("&country={}", c),
            _ => String::new(),
        };
        let sort_param = match req.sort_by.as_deref() {
            Some("new") => "&sort=new",
            Some("old") => "&sort=old",
            _ => "",
        };
        // 中文查询时请求中文结果（并尽量约束语种）
        let lang_param = if is_cn_query {
            "&hl=zh-cn&lr=lang_zh-CN"
        } else {
            ""
        };
        let url = format!(
            "https://serpapi.com/search.json?engine=google_patents&q={}&page={}{}{}{}&api_key={}",
            urlencoding::encode(&search_query),
            serp_page,
            country_param,
            sort_param,
            lang_param,
            api_key
        );
        println!(
            "[ONLINE] SerpAPI query='{}' page={} country_param='{}'",
            search_query, serp_page, country_param
        );
        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status();
                println!("[ONLINE] SerpAPI status: {}", status);
                if let Ok(body) = resp.text().await {
                    println!("[ONLINE] SerpAPI body len={}", body.len());
                    if !status.is_success() {
                        if status.as_u16() == 429 {
                            upstream_hint = Some(
                                "SerpAPI 触发限流/额度限制（429），已自动尝试下游回退。"
                                    .to_string(),
                            );
                        } else {
                            upstream_hint = Some(format!(
                                "SerpAPI 请求失败（HTTP {}），已自动尝试下游回退。",
                                status
                            ));
                        }
                    } else if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                        if let Some(err) = json.get("error").and_then(|v| v.as_str()) {
                            println!("[ONLINE] SerpAPI error: {}", err);
                            let err_l = err.to_lowercase();
                            if err_l.contains("too many requests")
                                || err_l.contains("rate limit")
                                || err_l.contains("quota")
                            {
                                upstream_hint = Some(
                                    "SerpAPI 配额或频率受限，已自动尝试下游回退。".to_string(),
                                );
                            } else {
                                let short = err.chars().take(120).collect::<String>();
                                upstream_hint = Some(format!(
                                    "SerpAPI 返回错误：{}，已自动尝试下游回退。",
                                    short
                                ));
                            }
                        } else {
                            let total = json["search_information"]["total_results"]
                                .as_u64()
                                .unwrap_or(0) as usize;
                            let mut patents = Vec::new();
                            if let Some(results) = json["organic_results"].as_array() {
                                println!(
                                    "[ONLINE] SerpAPI results: {}, total: {}",
                                    results.len(),
                                    total
                                );
                                for (idx, r) in results.iter().enumerate() {
                                    let p = serp_to_patent(r);
                                    if !p.title.is_empty() {
                                        let saved_id = s.db.insert_patent(&p).unwrap_or_else(|e| {
                                            tracing::warn!(
                                                "Failed to cache online patent {}: {}",
                                                p.patent_number,
                                                e
                                            );
                                            p.id.clone()
                                        });
                                        // Hybrid relevance: position + content matching
                                        let position_score = (98.0 - idx as f64 * 3.0).max(30.0);
                                        let content_score = calculate_online_relevance(
                                            &req.query,
                                            &p.title,
                                            &p.abstract_text,
                                            &p.applicant,
                                            &p.inventor,
                                        );
                                        tracing::debug!(
                                            "SerpAPI filter: query={}, title={}, applicant={}, inventor={}, content_score={:.1}",
                                            &req.query, &p.title, &p.applicant, &p.inventor, content_score
                                        );
                                        if !is_online_result_relevant(
                                            &req.query,
                                            &p.title,
                                            &p.abstract_text,
                                            content_score,
                                            is_cn_query,
                                            &p.inventor,
                                        ) {
                                            continue;
                                        }
                                        let score =
                                            (position_score * 0.4 + content_score * 0.6).min(100.0);
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
                            }
                            if is_cn_query {
                                let zh_patents: Vec<PatentSummary> = patents
                                    .iter()
                                    .filter(|p| {
                                        contains_cjk(&p.title) || contains_cjk(&p.abstract_text)
                                    })
                                    .cloned()
                                    .collect();
                                if !zh_patents.is_empty() {
                                    patents = zh_patents;
                                }
                            }
                            patents = dedup_patent_summaries(patents);
                            if !patents.is_empty() {
                                let mut out = json!({
                                    "patents": patents,
                                    "total": total,
                                    "page": req.page,
                                    "page_size": 10,
                                    "source": "serpapi"
                                });
                                if let Some(h) = upstream_hint.take() {
                                    out["hint"] = json!(h);
                                }
                                return Json(out);
                            }
                            println!("[ONLINE] SerpAPI returned empty; fallback to local DB");
                            if upstream_hint.is_none() {
                                upstream_hint =
                                    Some("SerpAPI 无结果，已自动尝试下游回退。".to_string());
                            }
                        }
                    } else if upstream_hint.is_none() {
                        upstream_hint =
                            Some("SerpAPI 返回内容无法解析，已自动尝试下游回退。".to_string());
                    }
                }
            }
            Err(e) => {
                println!("[ONLINE] SerpAPI request error: {}", e);
                upstream_hint = Some(format!("SerpAPI 请求异常：{}，已自动尝试下游回退。", e));
            }
        }
    } else {
        println!("[ONLINE] No SERPAPI_KEY configured");
        upstream_hint = Some("未配置有效 SerpAPI Key，已自动尝试下游回退。".to_string());
    }

    if online_start.elapsed().as_secs() >= ONLINE_TOTAL_BUDGET_SECS {
        _remote_budget_exhausted = true;
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

/// 计算文本相似度分数（用于在线搜索排序和过滤）
fn calculate_online_relevance(
    query: &str,
    title: &str,
    abstract_text: &str,
    applicant: &str,
    inventor: &str,
) -> f64 {
    let q = query.trim().to_lowercase();
    let t = title.trim().to_lowercase();
    let a = abstract_text.trim().to_lowercase();
    let app = applicant.trim().to_lowercase();
    let inv = inventor.trim().to_lowercase();

    let mut score = 30.0;

    // Title matching (most important, max +50)
    if t == q {
        score += 50.0;
    } else if t.contains(&q) {
        score += 35.0;
    } else {
        // Word-level matching in title
        let q_words: Vec<&str> = q.split_whitespace().filter(|w| w.len() > 1).collect();
        if !q_words.is_empty() {
            let matches = q_words.iter().filter(|w| t.contains(*w)).count();
            score += (matches as f64 / q_words.len() as f64) * 30.0;
        }
        // Chinese bigram matching
        let q_chars: Vec<char> = q
            .chars()
            .filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c))
            .collect();
        if q_chars.len() >= 2 {
            let q_bigrams: Vec<String> = q_chars.windows(2).map(|w| w.iter().collect()).collect();
            let t_chars: Vec<char> = t
                .chars()
                .filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c))
                .collect();
            let t_bigrams: Vec<String> = if t_chars.len() >= 2 {
                t_chars.windows(2).map(|w| w.iter().collect()).collect()
            } else {
                vec![]
            };
            if !q_bigrams.is_empty() && !t_bigrams.is_empty() {
                let matches = q_bigrams.iter().filter(|bg| t_bigrams.contains(bg)).count();
                score += (matches as f64 / q_bigrams.len() as f64) * 25.0;
            }
        } else if !q_chars.is_empty() {
            let matches = q_chars.iter().filter(|c| t.contains(**c)).count();
            score += (matches as f64 / q_chars.len() as f64) * 20.0;
        }
    }

    // Abstract matching (secondary, max +15)
    if a.contains(&q) {
        score += 15.0;
    } else {
        let q_words: Vec<&str> = q.split_whitespace().filter(|w| w.len() > 1).collect();
        if !q_words.is_empty() {
            let matches = q_words.iter().filter(|w| a.contains(*w)).count();
            score += (matches as f64 / q_words.len() as f64) * 10.0;
        }
        let q_chars: Vec<char> = q
            .chars()
            .filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c))
            .collect();
        if q_chars.len() >= 2 {
            let q_bigrams: Vec<String> = q_chars.windows(2).map(|w| w.iter().collect()).collect();
            let a_chars: Vec<char> = a
                .chars()
                .filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c))
                .collect();
            let a_bigrams: Vec<String> = if a_chars.len() >= 2 {
                a_chars.windows(2).map(|w| w.iter().collect()).collect()
            } else {
                vec![]
            };
            if !q_bigrams.is_empty() && !a_bigrams.is_empty() {
                let matches = q_bigrams.iter().filter(|bg| a_bigrams.contains(bg)).count();
                score += (matches as f64 / q_bigrams.len() as f64) * 8.0;
            }
        }
    }

    // Applicant matching (bonus, max +5)
    if app.contains(&q) {
        score += 5.0;
    }

    // Inventor matching (bonus, max +15) — 用于中文发明人姓名搜索
    if inv.contains(&q) || q.contains(&inv) {
        score += 15.0;
    }

    score.min(100.0)
}

/// 检查字符串是否包含中文字符
fn contains_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))
}

/// 判断在线搜索结果的关联性，发明人姓名匹配时直接放行
fn is_online_result_relevant(
    query: &str,
    title: &str,
    abstract_text: &str,
    content_score: f64,
    is_cn_query: bool,
    inventor: &str,
) -> bool {
    let q = query.trim();
    if q.is_empty() {
        return false;
    }
    let t = title.to_lowercase();
    let a = abstract_text.to_lowercase();
    let ql = q.to_lowercase();

    // 发明人姓名直接匹配：查询词命中的发明人姓名，直接放行
    let inv_lower = inventor.to_lowercase();
    if !inv_lower.is_empty()
        && q.chars()
            .all(|c| c.is_ascii_alphabetic() || c.is_whitespace())
    {
        // 英文姓名：双向包含检查（查询包含发明人，或发明人包含查询）
        if inv_lower.contains(&ql) || ql.contains(&inv_lower) {
            return true;
        }
    }
    // 中文姓名/拼音：查询词中的每个字都出现在发明人字段中
    if !inv_lower.is_empty() {
        let q_clean: String = q.chars().filter(|c| !c.is_ascii_punctuation()).collect();
        if q_clean.chars().all(|c| {
            c.is_ascii_alphabetic() || c.is_ascii_digit() || ('\u{4E00}'..='\u{9FFF}').contains(&c)
        }) && q_clean.len() >= 2
        {
            let all_in_inventor = q_clean.chars().all(|c| inv_lower.contains(c));
            if all_in_inventor {
                return true;
            }
        }
    }

    // 直接匹配优先保留
    if t.contains(&ql) || a.contains(&ql) {
        return true;
    }

    // 中文查询：门槛更高，避免无关英文噪声
    if is_cn_query {
        if contains_cjk(title) || contains_cjk(abstract_text) {
            return content_score >= 45.0;
        }
        return content_score >= 62.0;
    }

    // 英文/国际查询：多词技术查询至少命中两个查询词
    let query_terms: Vec<&str> = ql
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() >= 3)
        .filter(|w| {
            !matches!(
                *w,
                "patent"
                    | "device"
                    | "method"
                    | "system"
                    | "apparatus"
                    | "mobile"
                    | "phone"
                    | "electronic"
            )
        })
        .collect();
    if query_terms.len() >= 2 {
        let haystack = format!("{t} {a}");
        let matched = query_terms
            .iter()
            .filter(|term| haystack.contains(**term))
            .count();
        let required_matches = if query_terms.len() <= 3 {
            query_terms.len()
        } else {
            (query_terms.len() * 2).div_ceil(3)
        };
        if matched < required_matches {
            return false;
        }
    }

    content_score >= 40.0
}

pub(crate) fn serp_to_patent(r: &serde_json::Value) -> Patent {
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

// 以下搜索源（Firecrawl/Bing/Google Patents直连/搜狗/CNIPR）已全部屏蔽，仅保留 SerpAPI

async fn try_exact_patent_lookup(
    query: &str,
    api_key: &str,
    state: &super::AppState,
) -> Option<serde_json::Value> {
    let q = query.trim();
    let digits: String = q.chars().filter(|c| c.is_ascii_digit()).collect();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(ONLINE_UPSTREAM_TIMEOUT_SECS))
        .build()
        .ok()?;

    // Step 1: Determine patent_id to look up
    let patent_id: String;

    let is_bare_cn_app = digits.len() >= 10
        && digits.len() <= 15
        && q.chars().all(|c| c.is_ascii_digit() || c == '.');

    let is_cn_app_with_prefix =
        q.starts_with("CN") && q.contains('.') && digits.len() >= 10 && digits.len() <= 15;

    if q.starts_with("CN")
        || q.starts_with("US")
        || q.starts_with("EP")
        || q.starts_with("WO")
        || q.starts_with("JP")
        || q.starts_with("KR")
    {
        if is_cn_app_with_prefix {
            // CN APPLICATION number with CN prefix (e.g., CN202420009882.7)
            // Google Patents indexes by PUBLICATION number, not application number.
            // We must first search to discover the publication number.
            let mut candidates = vec![q.to_string(), q.replace('.', "")];
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
            let found_id = find_publication_patent_id(&client, api_key, &candidates).await;
            if let Some(id) = found_id {
                let id = if id.contains("/CN") {
                    id.replace("/en", "/zh")
                } else {
                    id
                };
                println!("[EXACT] Found publication via keyword search: {}", id);
                patent_id = id;
            } else {
                println!(
                    "[EXACT] Keyword search returned no results for {:?}",
                    candidates
                );
                return None;
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
        candidates.push(q.to_string());
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

        match find_publication_patent_id(&client, api_key, &candidates).await {
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
    let url = format!(
        "https://serpapi.com/search.json?engine=google_patents_details&patent_id={}&api_key={}",
        urlencoding::encode(&patent_id),
        api_key
    );
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

    let pub_number = json["publication_number"].as_str().unwrap_or(q).to_string();
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
    let saved_id = state.db.insert_patent(&patent).unwrap_or(patent.id.clone());

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
    Some(serde_json::json!({
        "patents": [summary],
        "total": 1,
        "page": 1,
        "page_size": 10,
        "source": "serpapi_exact"
    }))
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intl_relevance_rejects_generic_phone_results_for_specific_hinge_query() {
        let query = "foldable phone hinge dustproof patent";
        assert!(!is_online_result_relevant(
            query,
            "mobile phone",
            "A mobile phone includes a touch screen and a camera.",
            42.0,
            false,
            "",
        ));
    }

    #[test]
    fn intl_relevance_rejects_results_missing_specific_constraint_terms() {
        let query = "foldable phone hinge dustproof patent";
        assert!(!is_online_result_relevant(
            query,
            "mobile phone",
            "A foldable mobile communication terminal has a biaxial hinge device.",
            60.0,
            false,
            "",
        ));
    }

    #[test]
    fn intl_relevance_keeps_specific_hinge_results() {
        let query = "foldable phone hinge dustproof patent";
        assert!(is_online_result_relevant(
            query,
            "Dustproof hinge for a foldable electronic device",
            "The hinge blocks dust ingress while the foldable phone opens and closes.",
            52.0,
            false,
            "",
        ));
    }
}
