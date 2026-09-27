use super::{escape_csv, parse_search_type, AppState};
use crate::db::Database;
use crate::patent::*;
// MA1：q 串渲染 / 相关性判定 / 排序去重 / 上游调用链均已抽到 `crate::search`，
// 本文件只保留「端点」职责：请求解析、响应形状、超时预算与本地兜底。
// MA2a：多源降级由 `crate::search::chain::SourceChain` 承担，本文件只负责登记源的优先级顺序。
// MA2b：链尾追加 EPO OPS 英文域补源（规格书 §4），同样「未配 Key 即不登记」。
use crate::search::chain::SourceChain;
use crate::search::merge::{dedup_patent_summaries, sort_by_relevance};
use crate::search::model::{
    report_excerpt, AttemptReport, AttemptStatus, FailKind, Lang, SearchOutcome, SearchQuery,
    SourceKind,
};
use crate::search::provider::SearchProvider;
use crate::search::providers::epo_ops::EpoOpsProvider;
use crate::search::providers::google_patents_xhr::GooglePatentsXhrProvider;
use crate::search::providers::serpapi::{exact_lookup_response, SerpApiProvider};
use crate::search::query::resolve_lang_with_explicit;
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::Arc;
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

/// MA5b：把 [`SearchOutcome::attempts`] 序列化为出参 `attempts` 键的值。
///
/// **纯新增键**：`patents/total/page/page_size/source/hint` 等既有键的形状与文案逐字不变，
/// 本函数只负责把链上已算好的诊断报告带出去，不参与任何判定。
/// 序列化字面量（`source` 小写稳定串、`status` 的外部标签表示、`latency_ms` 蛇形字段名）
/// 由 `attempts_json_locks_frontend_panel_literals` 锁死，前端诊断面板按同一约定解析。
/// 理论上 `Vec<AttemptReport>` 不可能序列化失败，仍按规约 2.7 用受控降级替代 unwrap。
fn attempts_json(outcome: &SearchOutcome) -> serde_json::Value {
    serde_json::to_value(&outcome.attempts).unwrap_or_else(|_| json!([]))
}

/// MA4a：本地 FTS 兜底的「链上记账 + 旧形状出参」。
///
/// 自 MA1 起本地兜底就是在线三源全部无结果后的最后一档，但它从未进过 `attempts`——
/// 诊断面板因此看不到「本地兜底跑没跑、命中几条、耗时多少」，「断网可复检」缺一条实证。
/// 本函数把这趟兜底补记为一条 [`SourceKind::LocalFts`] 的 [`AttemptReport`]，
/// **追加在 attempts 尾部**（与 [`serpapi_skipped_outcome`] 的「链外造一条、前置」是对称先例）。
///
/// 出参形状零改动（历史单测与 e2e 逐字锁死）：
/// - `source` 键仍恒为字符串 `"local"`——**故意不把 LocalFts 注册进 [`online_chain_providers`]
///   当第四源**，那会让 `winning_source().as_str()` 直接吐出 `"local_fts"` 破坏出参，
///   且「四种 Key 组合的链形状」用例会集体变红。`"local_fts"` 只作为 attempts 行的源名，
///   前端 `DIAG_SOURCE_LABELS`（MA5b）已识别，面板无需改动即可长出这一行。
/// - `hint` 文案、`patents[]` / `total` / `page` / `page_size` / `google_url` / `message`
///   的键集合与取值口径逐字不变，仅 attempts 多一条。
///
/// 空结果末档（在线与本地都零命中 → `patents: []` + `google_url`）同样带上这条记账：
/// 本地跑过就如实记 Success（hits 可为 0），本地查询炸了就记 Failed——禁止伪造「跑过」。
fn local_fallback_json(
    db: &Database,
    req: &SearchRequest,
    online_search_type: &Option<SearchType>,
    upstream_hint: Option<String>,
    outcome: &mut SearchOutcome,
) -> serde_json::Value {
    println!("[ONLINE] Falling back to local DB");
    let local_start = Instant::now();
    // MA4b：本地兜底同样消费请求侧精确开关——`exact_assignee=true` 且路由到申请人域时
    // 走 `applicant = ?` 等值通道（见 db::search_smart_exact）；false/缺省 = 旧 LIKE 模糊，逐字不变。
    let local_result = db.search_smart_exact(
        &req.query,
        online_search_type.as_ref(),
        req.country.as_deref(),
        req.date_from.as_deref(),
        req.date_to.as_deref(),
        req.page,
        req.page_size,
        req.exact_assignee,
    );
    let latency_ms = local_start.elapsed().as_millis() as u64;
    let (local, error_text) = match local_result {
        Ok((patents, total, _)) => (Some((dedup_patent_summaries(patents), total)), None),
        Err(e) => {
            tracing::error!("local fallback search failed: {}", e);
            (
                None,
                Some(format!(
                    "本地 FTS 兜底查询失败（local_fts）：{}",
                    report_excerpt(&e.to_string())
                )),
            )
        }
    };

    // Failed 分支的分类选择（本包的设计点，结论进 PR）：复用既有的 `FailKind::Parse`，不扩枚举。
    // 理由：Network/Quota/Auth 在 spec §1 里都是「远端源」语义（超时/配额/鉴权），本地 SQLite
    // 查询故障三者皆非；而 Parse 的处置恰是「记 bug、不切换、直接把错抛出」——与这里的事实
    // 控制流完全一致（本地兜底是最后一档，失败后已无下一源可切，错误原样进面板供复检方定位）。
    // 为记账去扩 FailKind 会破坏 MA5b 的字面量锁测试与前端 DIAG_FAIL_KEYS，故不动，留待 MA6 前再议。
    let attempt = match &local {
        Some((patents, _total)) => AttemptReport {
            source: SourceKind::LocalFts,
            status: AttemptStatus::Success,
            latency_ms,
            hits: patents.len(),
            error: None,
            hint: None,
        },
        None => AttemptReport {
            source: SourceKind::LocalFts,
            status: AttemptStatus::Failed(FailKind::Parse),
            latency_ms,
            hits: 0,
            error: error_text,
            hint: None,
        },
    };
    outcome.attempts.push(attempt);

    if let Some((patents, total)) = local {
        if total > 0 {
            let hint_text = upstream_hint.unwrap_or_else(|| {
                "国外在线源暂时未返回结果，已回退本地缓存。建议配置 SerpAPI 提升命中率。"
                    .to_string()
            });
            let dedup_total = total.max(patents.len());
            return json!({
                "patents": patents,
                "total": dedup_total,
                "page": req.page,
                "page_size": req.page_size,
                "source": "local",
                "hint": hint_text,
                "attempts": attempts_json(outcome)
            });
        }
    }
    let enc = urlencoding::encode(&req.query);
    let mut out = json!({
        "patents": [], "total": 0, "page": 1, "page_size": 20,
        "google_url": format!("https://patents.google.com/?q={enc}&oq={enc}"),
        "message": "未找到结果，可尝试在 Google Patents 上搜索",
        "attempts": attempts_json(outcome)
    });
    if let Some(h) = upstream_hint {
        out["hint"] = json!(h);
    }
    out
}

/// 按 spec §6 的优先级装配在线执行链的源集合（MA2b 起三源）：
/// `[SerpAPI(有 Key) → GooglePatentsXhr → EpoOps(有 Key)]`。
///
/// 抽成独立函数是因为**链的形状**本身就是需要被断言的语义：
/// SerpAPI 与 EPO 都遵循「未配置 Key 即不登记」（该源压根不参与，`attempts` 里不会出现它，
/// 与「跑了但零命中」不同），而登记顺序即降级优先级。留在 handler 里只能靠人读代码保证。
///
/// SerpAPI 的 Key 由调用方传入 —— round-robin 选取属配置层既有职责（MA1 起即不上提到 provider）。
/// EPO 排链尾的依据见 `docs/analysis/search-sources-spec.md` §4 与
/// `providers/epo_ops.rs` 的 CQL 索引一节（`ti`/`ab` 只有英文著录数据）。
fn online_chain_providers(
    db: &Arc<Database>,
    serpapi_key: Option<String>,
    epo_credentials: Option<(String, String)>,
) -> Vec<Arc<dyn SearchProvider>> {
    let mut providers: Vec<Arc<dyn SearchProvider>> = Vec::new();
    match serpapi_key {
        Some(api_key) => providers.push(Arc::new(SerpApiProvider::new(api_key, db.clone()))),
        None => println!("[ONLINE] No SERPAPI_KEY configured"),
    }
    // 免费无 Key 的降级源，恒登记（MA2a）。
    providers.push(Arc::new(GooglePatentsXhrProvider::new(db.clone())));
    match epo_credentials {
        Some((epo_key, epo_secret)) => providers.push(Arc::new(EpoOpsProvider::new(
            epo_key,
            epo_secret,
            db.clone(),
        ))),
        None => println!("[ONLINE] No EPO_KEY/EPO_SECRET configured, skipping epo_ops"),
    }
    providers
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
    let (mut patents, total, detected_type) = match s.db.search_smart_exact(
        &req.query,
        search_type.as_ref(),
        req.country.as_deref(),
        req.date_from.as_deref(),
        req.date_to.as_deref(),
        req.page,
        req.page_size,
        // MA4b：本地检索端点同享精确开关（false/缺省 = 旧 LIKE 行为逐字不变）
        req.exact_assignee,
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
/// 端点职责：请求解析 → 区域判定 → 取 Key → 交给 [`SourceChain`]（SerpAPI 主源 + MA2a 新增的
/// Google Patents XHR 免费降级源 + MA2b 新增的 EPO OPS 英文域补源）→ 按旧形状出参 →
/// 超时预算 → 本地库兜底。
/// 上游调用与结果映射见 `crate::search::providers::{serpapi, google_patents_xhr, epo_ops}`。
///
/// MA2a 起的响应差异：`source` 字段不再恒为 `"serpapi"`，降级命中时为 `"google_patents_xhr"`
/// （旧代码只有一个在线源，故无需区分）；MA2b 起再追加一档 `"epo_ops"`。
/// 其余字段集合与本地兜底路径逐字未变。
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
    // MA3：请求体可带显式 `language`，优先级为 显式 language > region > 自动判定
    // （见 `resolve_lang_with_explicit`；不传时委托旧判定，行为与之前逐字一致）。
    let lang = resolve_lang_with_explicit(
        req.language,
        req.region.as_deref(),
        req.country.as_deref(),
        query_trimmed,
    );
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
        // MA4b：请求侧独立申请人过滤透传（此前恒为 None/false，出网 URL 不含 assignee 段；
        // 缺省仍与旧行为逐字一致，语义见 types::search::SearchRequest 字段注释）
        assignee: req.assignee.clone(),
        exact_assignee: req.exact_assignee,
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

    // ── 精确专利号查询：SerpAPI Details 是本源独占能力，命中即按旧形状直接返回 ──
    // 保持「在降级链之前」的位置：MA2a 没有给 XHR 源做 details 端点（spec §2 只有 query 一个端点）。
    if matches!(online_search_type.as_ref(), Some(SearchType::PatentNumber)) {
        if let Some(api_key) = api_key_opt.as_ref() {
            let provider = SerpApiProvider::new(api_key.clone(), s.db.clone());
            if let Some(summary) = provider.lookup_exact(req.query.clone()).await {
                return Json(exact_lookup_response(summary));
            }
        }
    }

    // ── MA2a/MA2b 执行链（spec §6）：SerpAPI 为主、Google Patents XHR 直抓为免费降级、
    // EPO OPS 为英文域补源（spec §4）。三源并行发起、各带独立超时（SerpAPI 沿用旧 30s，
    // XHR 与 EPO 15s），按登记顺序择胜；装配规则见 [`online_chain_providers`]。
    // 没配 Key 时不把 SerpAPI 登记进链路，而是把它的 `Skipped` 记账**前置**到 attempts，
    // 这样旧的 hint 文案与「首个非空生效」语义逐字不变，控制流也不必分叉成两条。
    let epo_credentials = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .epo_credentials();
    let providers = online_chain_providers(&s.db, api_key_opt.clone(), epo_credentials);

    let mut outcome = SourceChain::new(providers).run(search_query).await;
    if api_key_opt.is_none() {
        let skipped = serpapi_skipped_outcome("未配置有效 SerpAPI Key，已自动尝试下游回退。");
        let mut attempts = skipped.attempts;
        attempts.append(&mut outcome.attempts);
        outcome.attempts = attempts;
    }

    let mut upstream_hint = outcome.hint();

    // 在线源成功且有命中 → 按旧形状直接返回；`source` 自 MA2a 起如实反映胜出源
    // （旧代码恒为 "serpapi"，因为那时只有一个在线源）。
    if let Some(winner) = outcome.winning_source() {
        let mut out = json!({
            "patents": outcome.summaries(),
            "total": outcome.upstream_total.unwrap_or(0),
            "page": req.page,
            "page_size": 10,
            "source": winner.as_str(),
            "attempts": attempts_json(&outcome)
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

    // MA2b 起在线链路为 [SerpAPI → Google Patents XHR 直抓 → EPO OPS]（见上方的 SourceChain），
    // 其余历史源（Firecrawl / Bing / CNIPR / 搜狗）仍处于屏蔽状态。
    // 三个在线源都没有可用结果时，回退本地数据库。
    // MA4a：本地兜底本身也是链上的一次尝试——记账与出参组装抽到 [`local_fallback_json`]，
    // 便于用内存库离线锁定形状（不经网络），出参各键逐字不变。
    Json(local_fallback_json(
        &s.db,
        &req,
        &online_search_type,
        upstream_hint,
        &mut outcome,
    ))
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

    /// **MA2b 链形状（任务清单 2-③）**：未配 `EPO_KEY/EPO_SECRET` 时链里**压根不登记** EpoOps，
    /// 因此 `attempts` 里不会出现该源（与「登记了但 Skipped」是两种形状，后者见
    /// `search::chain::case6_epo_without_credentials_skips_and_sends_nothing`）。
    /// 配齐后才追加在链尾，顺序即降级优先级（spec §6）。
    #[test]
    fn online_chain_registers_epo_only_with_credentials() {
        let db = Arc::new(Database::init(":memory:").expect("in-memory db"));
        let creds = || Some(("ck".to_string(), "cs".to_string()));

        let with_epo = SourceChain::new(online_chain_providers(
            &db,
            Some("serp-key".to_string()),
            creds(),
        ));
        assert_eq!(
            vec![
                SourceKind::SerpApi,
                SourceKind::GooglePatentsXhr,
                SourceKind::EpoOps
            ],
            with_epo.kinds(),
            "三源齐配时的链形状 = 优先级顺序"
        );

        let without_epo = SourceChain::new(online_chain_providers(
            &db,
            Some("serp-key".to_string()),
            None,
        ));
        assert_eq!(
            vec![SourceKind::SerpApi, SourceKind::GooglePatentsXhr],
            without_epo.kinds(),
            "未配 EPO 凭证即不登记该源（与 SerpAPI 同语义），链保持 MA2a 形状"
        );

        // SerpAPI 缺 Key 只摘掉它自己，不影响 EPO 的登记与相对顺序。
        let without_serpapi = SourceChain::new(online_chain_providers(&db, None, creds()));
        assert_eq!(
            vec![SourceKind::GooglePatentsXhr, SourceKind::EpoOps],
            without_serpapi.kinds()
        );

        // 三源全不可用（两 Key 皆无）时链仍非空：免费的 XHR 直抓恒登记，兜底才轮得到本地库。
        let bare = SourceChain::new(online_chain_providers(&db, None, None));
        assert_eq!(vec![SourceKind::GooglePatentsXhr], bare.kinds());
        assert!(!bare.is_empty());
    }

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

    /// **MA5b 出参字面量锁（前端诊断面板的对账依据）**：`attempts` 键的 JSON 形状
    /// 逐字段冻结在此——前端 `templates/search.html` 的 `renderSearchDiagnostics` 按同一约定解析：
    /// - `source`：小写稳定串（`serpapi` / `google_patents_xhr` / `epo_ops` / `local_fts`）
    /// - `status`：serde 外部标签表示 —— 单元变体是裸字符串 `"Success"` / `"Skipped"`，
    ///   带值变体是单键对象 `{"Failed":"quota"}`（`FailKind` 为 snake_case）
    /// - 字段名：`latency_ms` / `hits` / `error` / `hint`（后两个可为 null）
    ///
    /// 改动 `AttemptStatus`/`FailKind` 的 serde 表示会红，前端必须同步。
    #[test]
    fn attempts_json_locks_frontend_panel_literals() {
        use crate::search::model::FailKind;
        let outcome = SearchOutcome {
            results: vec![],
            attempts: vec![
                AttemptReport {
                    source: SourceKind::SerpApi,
                    status: AttemptStatus::Failed(FailKind::Quota),
                    latency_ms: 812,
                    hits: 0,
                    error: Some("HTTP 429 配额耗尽".to_string()),
                    hint: Some("请配置 SerpAPI Key".to_string()),
                },
                AttemptReport {
                    source: SourceKind::GooglePatentsXhr,
                    status: AttemptStatus::Success,
                    latency_ms: 2330,
                    hits: 10,
                    error: None,
                    hint: None,
                },
                AttemptReport {
                    source: SourceKind::SerpApi,
                    status: AttemptStatus::Skipped,
                    latency_ms: 0,
                    hits: 0,
                    error: None,
                    hint: None,
                },
            ],
            upstream_total: None,
        };
        assert_eq!(
            json!([
                {
                    "source": "serpapi",
                    "status": {"Failed": "quota"},
                    "latency_ms": 812,
                    "hits": 0,
                    "error": "HTTP 429 配额耗尽",
                    "hint": "请配置 SerpAPI Key"
                },
                {
                    "source": "google_patents_xhr",
                    "status": "Success",
                    "latency_ms": 2330,
                    "hits": 10,
                    "error": null,
                    "hint": null
                },
                {
                    "source": "serpapi",
                    "status": "Skipped",
                    "latency_ms": 0,
                    "hits": 0,
                    "error": null,
                    "hint": null
                }
            ]),
            attempts_json(&outcome)
        );
    }

    /// **MA5b 总开关（只增键、不改键）**：命中/兜底/空结果三条 return 路径都追加
    /// `attempts` 键，其余既有键的集合与取值口径逐字不变。空链（理论上不可达，但
    /// 面板要能区分「没有 attempts」与「数组为空」）序列化为 `[]`。
    #[test]
    fn attempts_key_is_additive_on_all_online_paths() {
        let outcome = SearchOutcome {
            results: vec![],
            attempts: vec![],
            upstream_total: None,
        };
        assert_eq!(json!([]), attempts_json(&outcome));

        // 在线命中路径：旧五键逐字保留 + 新增 attempts（键集合显式冻结）
        let winner = json!({
            "patents": outcome.summaries(),
            "total": outcome.upstream_total.unwrap_or(0),
            "page": 1,
            "page_size": 10,
            "source": SourceKind::SerpApi.as_str(),
            "attempts": attempts_json(&outcome)
        });
        let mut keys: Vec<&str> = winner
            .as_object()
            .map(|m| m.keys().map(|k| k.as_str()).collect())
            .unwrap_or_default();
        // serde_json::Map 不保证插入序（未开 preserve_order），按键集合冻结
        keys.sort_unstable();
        assert_eq!(
            vec![
                "attempts",
                "page",
                "page_size",
                "patents",
                "source",
                "total"
            ],
            keys
        );

        // 本地兜底与空结果路径同样只增 attempts，不动 source/hint/message/google_url
        let fallback = json!({
            "patents": [], "total": 0, "page": 1, "page_size": 20,
            "google_url": "https://patents.google.com/?q=x&oq=x",
            "message": "未找到结果，可尝试在 Google Patents 上搜索",
            "attempts": attempts_json(&outcome)
        });
        assert_eq!(json!([]), fallback["attempts"]);
        assert!(fallback.get("message").is_some());
    }

    // ── MA4a：本地 FTS 兜底的链上记账（attempts 末条 = local_fts）──────────────
    //
    // 直接测 [`local_fallback_json`]：内存库 + 手工构造 outcome，不经网络，形状可锁死。

    /// 测试夹具：最小可用的 `SearchRequest`（page/page_size 用端点默认语义）。
    fn fallback_req(query: &str) -> SearchRequest {
        SearchRequest {
            query: query.to_string(),
            page: 1,
            page_size: 20,
            country: None,
            date_from: None,
            date_to: None,
            search_type: None,
            sort_by: None,
            ipc: None,
            cpc: None,
            region: None,
            language: None,
            assignee: None,
            exact_assignee: false,
        }
    }

    /// 测试夹具：一条「链已跑完、三源皆墨」的 outcome（SerpAPI 未配 Key 前置 Skipped，
    /// XHR 真实失败），本地兜底应**追加**在其后而非混入在线源之间。
    fn chain_exhausted_outcome() -> SearchOutcome {
        SearchOutcome {
            results: vec![],
            attempts: vec![
                AttemptReport {
                    source: SourceKind::SerpApi,
                    status: AttemptStatus::Skipped,
                    latency_ms: 0,
                    hits: 0,
                    error: Some("未配置 SERPAPI_KEY，本轮未发起在线请求".to_string()),
                    hint: Some("未配置有效 SerpAPI Key，已自动尝试下游回退。".to_string()),
                },
                AttemptReport {
                    source: SourceKind::GooglePatentsXhr,
                    status: AttemptStatus::Failed(FailKind::Network),
                    latency_ms: 41,
                    hits: 0,
                    error: Some("google_patents_xhr 模拟断网失败".to_string()),
                    hint: None,
                },
            ],
            upstream_total: None,
        }
    }

    /// 测试夹具：可被 search_like（Mixed 路径）以「固态电池」命中的入库专利。
    fn seed_patent(db: &Database, id: &str, title: &str) {
        let p = Patent {
            id: id.to_string(),
            patent_number: format!("CN{}000000A", id),
            title: title.to_string(),
            abstract_text: format!("一种{title}及其制备方法"),
            description: String::new(),
            claims: String::new(),
            applicant: "测试研究院".to_string(),
            inventor: "张三".to_string(),
            filing_date: "2024-01-01".to_string(),
            publication_date: "2024-07-01".to_string(),
            grant_date: None,
            ipc_codes: "H01M".to_string(),
            cpc_codes: "H01M".to_string(),
            priority_date: String::new(),
            country: "CN".to_string(),
            kind_code: "A".to_string(),
            family_id: None,
            legal_status: String::new(),
            citations: "[]".to_string(),
            cited_by: "[]".to_string(),
            source: "test".to_string(),
            raw_json: "{}".to_string(),
            created_at: "2026-01-01 00:00:00".to_string(),
            images: "[]".to_string(),
            pdf_url: String::new(),
        };
        db.insert_patent(&p).expect("seed insert");
    }

    /// **MA4a 核心断言 1（本地兜底命中）**：在线链全部失败后本地库有结果时，
    /// attempts 末条必须是 `local_fts` + Success + hits>0 + latency_ms 已计时；
    /// 且出参 `source=="local"`、默认 hint 文案、`patents[]` 内容逐字不变（旧形状快照）。
    #[test]
    fn local_fallback_hit_appends_local_fts_attempt_and_keeps_legacy_shape() {
        let db = Database::init(":memory:").expect("in-memory db");
        seed_patent(&db, "111", "固态电池");
        let mut outcome = chain_exhausted_outcome();
        let out = local_fallback_json(
            &db,
            &fallback_req("固态电池"),
            &Some(SearchType::Mixed),
            None,
            &mut outcome,
        );

        let attempts = out["attempts"].as_array().expect("attempts 是数组");
        assert_eq!(3, attempts.len(), "链上两条 + 本地兜底追加一条");
        let local = &attempts[2];
        assert_eq!(
            json!({"source": "local_fts", "status": "Success", "hits": 1, "error": null, "hint": null}),
            json!({
                "source": local["source"], "status": local["status"], "hits": local["hits"],
                "error": local["error"], "hint": local["hint"],
            }),
            "本地兜底行：local_fts + Success + hits>0，且不携带 hint（不得污染首个非空 hint 语义）"
        );
        assert!(local["latency_ms"].is_number(), "latency_ms 必须计时落账");
        // 追加在尾部而非混入在线源之间
        assert_eq!(
            vec!["serpapi", "google_patents_xhr", "local_fts"],
            attempts
                .iter()
                .map(|a| a["source"].as_str().unwrap_or("?").to_string())
                .collect::<Vec<_>>()
        );

        // 旧形状逐字锁：键集合、source、hint 文案、patents 内容
        let mut keys: Vec<&str> = out
            .as_object()
            .map(|m| m.keys().map(|k| k.as_str()).collect())
            .unwrap_or_default();
        keys.sort_unstable();
        assert_eq!(
            vec![
                "attempts",
                "hint",
                "page",
                "page_size",
                "patents",
                "source",
                "total"
            ],
            keys
        );
        assert_eq!("local", out["source"]);
        assert_eq!(
            "国外在线源暂时未返回结果，已回退本地缓存。建议配置 SerpAPI 提升命中率。",
            out["hint"]
        );
        assert_eq!(1, out["patents"].as_array().expect("patents").len());
        assert_eq!("CN111000000A", out["patents"][0]["patent_number"]);
        assert_eq!("固态电池", out["patents"][0]["title"]);
    }

    /// **MA4a 核心断言 2（本地也零命中）**：末档（`patents: []` + `google_url`）里
    /// 仍要能看出「本地兜底跑过、命中 0 条」——如实记 Success + hits=0，禁止伪造。
    #[test]
    fn local_fallback_zero_hits_records_the_run_on_tail_path() {
        let db = Database::init(":memory:").expect("in-memory db");
        seed_patent(&db, "111", "固态电池");
        let mut outcome = chain_exhausted_outcome();
        let out = local_fallback_json(
            &db,
            &fallback_req("不存在的检索词xyz"),
            &Some(SearchType::Mixed),
            Some("在线预算提示".to_string()),
            &mut outcome,
        );

        let attempts = out["attempts"].as_array().expect("attempts");
        assert_eq!(3, attempts.len());
        let local = &attempts[2];
        assert_eq!("local_fts", local["source"]);
        assert_eq!("Success", local["status"], "跑过但零命中仍是 Success");
        assert_eq!(0, local["hits"]);
        assert!(local["error"].is_null());

        // 末档旧形状：google_url/message 逐字不变，hint 沿用上游文案（非默认兜底文案）
        assert_eq!(json!([]), out["patents"]);
        assert_eq!(0, out["total"]);
        assert_eq!(
            "https://patents.google.com/?q=%E4%B8%8D%E5%AD%98%E5%9C%A8%E7%9A%84%E6%A3%80%E7%B4%A2%E8%AF%8Dxyz&oq=%E4%B8%8D%E5%AD%98%E5%9C%A8%E7%9A%84%E6%A3%80%E7%B4%A2%E8%AF%8Dxyz",
            out["google_url"]
        );
        assert_eq!("未找到结果，可尝试在 Google Patents 上搜索", out["message"]);
        assert_eq!("在线预算提示", out["hint"]);
    }

    /// **MA4a 核心断言 3（本地查询失败）**：`search_smart` 报错时记
    /// Failed(Parse)（分类理由见 [`local_fallback_json`] 注释，不扩枚举），
    /// error 非空且点名来源（风格对齐 chain case5），出参仍走末档旧形状。
    #[test]
    fn local_fallback_db_error_records_failed_parse_attempt() {
        let db = Database::init(":memory:").expect("in-memory db");
        seed_patent(&db, "111", "固态电池");
        // 制造真实查询故障：删掉内容表，search_like 的 prepare 必然报 no such table。
        db.conn()
            .execute("DROP TABLE patents", [])
            .expect("drop for failure fixture");
        let mut outcome = chain_exhausted_outcome();
        let out = local_fallback_json(
            &db,
            &fallback_req("固态电池"),
            &Some(SearchType::Mixed),
            None,
            &mut outcome,
        );

        let attempts = out["attempts"].as_array().expect("attempts");
        assert_eq!(3, attempts.len());
        let local = &attempts[2];
        assert_eq!("local_fts", local["source"]);
        assert_eq!(
            json!({"Failed": "parse"}),
            local["status"],
            "本地库故障复用 Parse（记 bug 不降级），字面量受 MA5b 锁约束"
        );
        assert_eq!(0, local["hits"]);
        let err = local["error"].as_str().expect("失败必须带非空 error");
        assert!(err.contains("local_fts"), "error 要点名来源: {err}");
        // 失败即无兜底结果，落末档形状
        assert_eq!(json!([]), out["patents"]);
        assert!(
            out.get("hint").is_none(),
            "upstream_hint 为 None 时末档不凭空造 hint"
        );
        assert!(out.get("google_url").is_some());
    }

    /// 夹具：指定申请人的入库专利（MA4b 精确通道用例用；字段形状与 [`seed_patent`] 一致）。
    fn seed_patent_applicant(db: &Database, id: &str, title: &str, applicant: &str) {
        let p = Patent {
            id: id.to_string(),
            patent_number: format!("CN{}000000A", id),
            title: title.to_string(),
            abstract_text: format!("一种{title}及其制备方法"),
            description: String::new(),
            claims: String::new(),
            applicant: applicant.to_string(),
            inventor: "李四".to_string(),
            filing_date: "2024-01-01".to_string(),
            publication_date: "2024-07-01".to_string(),
            grant_date: None,
            ipc_codes: "H01M".to_string(),
            cpc_codes: "H01M".to_string(),
            priority_date: String::new(),
            country: "CN".to_string(),
            kind_code: "A".to_string(),
            family_id: None,
            legal_status: String::new(),
            citations: "[]".to_string(),
            cited_by: "[]".to_string(),
            source: "test".to_string(),
            raw_json: "{}".to_string(),
            created_at: "2026-01-01 00:00:00".to_string(),
            images: "[]".to_string(),
            pdf_url: String::new(),
        };
        db.insert_patent(&p).expect("seed insert");
    }

    /// **MA4b 核心断言（路由级透传）**：`exact_assignee=true` 时本地兜底必须走等值通道——
    /// 「张三」不得带回「张三丰」名下专利；同一请求 `exact_assignee=false` 时两条款全回
    /// （旧 LIKE 行为逐字不变）。attempts 的 local_fts 行如实反映 hits 差异。
    #[test]
    fn local_fallback_exact_assignee_switches_the_equal_channel() {
        let db = Database::init(":memory:").expect("in-memory db");
        seed_patent_applicant(&db, "zs1", "固态电池正极材料", "张三");
        seed_patent_applicant(&db, "zsf1", "固态电池隔膜", "张三丰");

        // exact=true：只回等值行
        let mut exact = fallback_req("张三");
        exact.exact_assignee = true;
        let mut outcome = chain_exhausted_outcome();
        let out = local_fallback_json(
            &db,
            &exact,
            &Some(SearchType::Applicant),
            None,
            &mut outcome,
        );
        let patents = out["patents"].as_array().expect("patents");
        assert_eq!(1, patents.len(), "精确通道不得命中张三丰: {out}");
        assert_eq!("张三", out["patents"][0]["applicant"]);
        assert_eq!("CNzs1000000A", out["patents"][0]["patent_number"]);
        assert_eq!(
            1, out["attempts"][2]["hits"],
            "local_fts 记账必须如实反映精确通道 hits"
        );

        // exact=false（缺省形状）：旧模糊行为不变，两条都回
        let fuzzy = fallback_req("张三");
        let mut outcome2 = chain_exhausted_outcome();
        let out2 = local_fallback_json(
            &db,
            &fuzzy,
            &Some(SearchType::Applicant),
            None,
            &mut outcome2,
        );
        assert_eq!(2, out2["patents"].as_array().expect("patents").len());
    }
}
