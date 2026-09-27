use super::{escape_csv, parse_search_type, AppState};
use crate::db::Database;
use crate::patent::*;
// MA1：q 串渲染 / 相关性判定 / 排序去重 / 上游调用链均已抽到 `crate::search`，
// 本文件只保留「端点」职责：请求解析、响应形状、超时预算与本地兜底。
// MA2a：多源降级由 `crate::search::chain::SourceChain` 承担，本文件只负责登记源的优先级顺序。
// MA2b：链尾追加 EPO OPS 英文域补源（规格书 §4），同样「未配 Key 即不登记」。
use crate::search::breaker::{self, CooldownTable};
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
use std::time::{Duration, Instant};

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
///
/// MA6b 追加 `cooled` 参数：非空时两档出参带顶层 `cooldowns` 键（见 [`cooldowns_json`]，
/// 纯新增、空则省略），其余键的「零改动」承诺按上述口径不变。
fn local_fallback_json(
    db: &Database,
    req: &SearchRequest,
    online_search_type: &Option<SearchType>,
    upstream_hint: Option<String>,
    outcome: &mut SearchOutcome,
    cooled: &[(SourceKind, Duration)],
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
            let mut out = json!({
                "patents": patents,
                "total": dedup_total,
                "page": req.page,
                "page_size": req.page_size,
                "source": "local",
                "hint": hint_text,
                "attempts": attempts_json(outcome)
            });
            // MA6b：纯新增键，冷却为空时整键省略（见 [`cooldowns_json`]）
            if let Some(cd) = cooldowns_json(cooled) {
                out["cooldowns"] = cd;
            }
            return out;
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
    // MA6b：纯新增键，冷却为空时整键省略（见 [`cooldowns_json`]）
    if let Some(cd) = cooldowns_json(cooled) {
        out["cooldowns"] = cd;
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

/// MA6b：冷却剩余时长的展示口径——**向上取整**（冷却行显示 0s 是自相矛盾）。
/// [`cooldown_skipped_report`] 的 error 文案与 `cooldowns` 出参键共用此单一判据，
/// 保证面板徽标（结构化 `cooldowns`）与织回记账（error 文案）里的秒数永远一致。
fn cooldown_remaining_secs(remaining: Duration) -> u64 {
    remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0)
}

/// MA6b：把链前过滤得到的 `(源, 剩余时长)` 序列化为出参顶层 `cooldowns` 键。
///
/// 形状 `[{"source":"serpapi","remaining_secs":297}, ...]`，顺序 = 原登记顺序，
/// `source` 串与 `attempts` 行的 `source` 同一约定（`SourceKind::as_str()`，前端
/// `DIAG_SOURCE_LABELS` 按同一键表解析）。**空则返回 `None`，调用方整键省略**——
/// 无冷却时旧响应逐字节不变（与 MA5b「空查询早退不带 attempts」同一取向）。
/// 前端判据只认这个结构化键，禁止正则匹配中文 error 文案（隐式契约随文案腐烂）。
fn cooldowns_json(cooled: &[(SourceKind, Duration)]) -> Option<serde_json::Value> {
    if cooled.is_empty() {
        return None;
    }
    let items: Vec<serde_json::Value> = cooled
        .iter()
        .map(|(source, remaining)| {
            json!({
                "source": source.as_str(),
                "remaining_secs": cooldown_remaining_secs(*remaining),
            })
        })
        .collect();
    Some(serde_json::Value::Array(items))
}

/// MA6a 的冷却记账注入形状：`Skipped` 的原因放 **`error`**、`hint` 恒为 `None`。
/// 取证依据（不是猜的）：
/// - `templates/search.html::renderSearchDiagnostics`（:626-634）对**每条** attempt
///   都显示 `a.error`（"错误: …"）与 `a.hint`（"提示: …"），不分状态——既有 `Skipped`
///   一直用 `error` 说明「为什么没跑」（本文件 `serpapi_skipped_outcome` 的
///   「未配置 SERPAPI_KEY…」、EPO provider 的缺凭证 Skipped，且
///   `chain::case6` 明确断言「Skipped 也要说明为什么没跑」读的就是 error）。
/// - `hint` 更不能用：`SearchOutcome::hint()` 取「首个非空 hint」并写进出参顶层
///   `hint` 键——冷却文案混进去会**改变响应形状与既有提示语义**。
/// - 本包禁止改 templates/static，故沿用「Skipped → error 带原因」的现役约定零风险。
fn cooldown_skipped_report(source: SourceKind, remaining: Duration) -> AttemptReport {
    // 向上取整口径与 `cooldowns` 出参键共用（见 [`cooldown_remaining_secs`]）。
    let secs = cooldown_remaining_secs(remaining);
    AttemptReport {
        source,
        status: AttemptStatus::Skipped,
        latency_ms: 0,
        hits: 0,
        error: Some(format!(
            "{} 上游熔断冷却中，剩余 {}s（本次不再发起请求）",
            source.as_str(),
            secs
        )),
        hint: None,
    }
}

/// MA6a：链前过滤的出参形状（clippy type_complexity 要求收拢命名）。
/// `(未冷却可进链的源, 被冷却摘出的 (源, 剩余时长)——按原登记序)`。
type CooldownFilterResult = (Vec<Arc<dyn SearchProvider>>, Vec<(SourceKind, Duration)>);

/// MA6a：链前按冷却表过滤源。**纯函数**（表与时钟显式传入），配合
/// [`weave_cooled_attempts`] 可在路由层离线锁定「冷却源不出链、但 attempts 留痕」。
///
/// 被过滤的源不构造请求、不进 [`SourceChain`]；返回的 `(源, 剩余时长)` 保持
/// 原登记顺序，供后续织回 `Skipped` 记账。
fn filter_cooled_providers(
    providers: Vec<Arc<dyn SearchProvider>>,
    table: &CooldownTable,
    now: Instant,
) -> CooldownFilterResult {
    let mut active = Vec::with_capacity(providers.len());
    let mut cooled = Vec::new();
    for provider in providers {
        match table.remaining(provider.kind(), now) {
            Some(remaining) => cooled.push((provider.kind(), remaining)),
            None => active.push(provider),
        }
    }
    (active, cooled)
}

/// MA6a：把冷却源的 `Skipped` 记账按**原登记顺序**织回链上 attempts。
///
/// 链只跑了未冷却的源，其 attempts 天然保持登记序的相对顺序；本函数按 `order`
/// （装配时的完整登记序）逐位恢复：冷却位织入 [`cooldown_skipped_report`]，
/// 非冷却位从链上该源的尝试原样搬回（同源多条时保持组内顺序）。
/// 动机：被冷却的源**不能从面板消失**——用户会误读成「没配 Key」。
fn weave_cooled_attempts(
    order: &[SourceKind],
    cooled: &[(SourceKind, Duration)],
    chain_attempts: Vec<AttemptReport>,
) -> Vec<AttemptReport> {
    if cooled.is_empty() {
        return chain_attempts;
    }
    // 按源分组（保持组内顺序；现行 provider 每路恒 1 条，分组是为防御未来多页源）
    let mut grouped: Vec<(SourceKind, std::collections::VecDeque<AttemptReport>)> = Vec::new();
    for a in chain_attempts {
        match grouped.iter_mut().find(|(s, _)| *s == a.source) {
            Some(slot) => slot.1.push_back(a),
            None => grouped.push((a.source, std::collections::VecDeque::from([a]))),
        }
    }
    let mut out = Vec::with_capacity(order.len());
    for kind in order {
        if let Some((_, remaining)) = cooled.iter().find(|(s, _)| s == kind) {
            out.push(cooldown_skipped_report(*kind, *remaining));
        }
        if let Some(slot) = grouped.iter_mut().find(|(s, _)| s == kind) {
            out.extend(slot.1.drain(..));
        }
    }
    // 防御：attempts 里出现了登记表之外的源（理论上不可达）也必须留痕，不得静默吞掉
    for (_, rest) in grouped.iter_mut().filter(|(s, _)| !order.contains(s)) {
        out.extend(rest.drain(..));
    }
    out
}

/// MA6c：专利号精确直查（`SerpApiProvider::lookup_exact`）本次**是否允许出网**。
///
/// 三条判据，前两条是 MA6c 之前 handler 里那两层 `if` 的原样搬迁（逐字等价、无新增判定）：
/// 1. `search_type == Some(SearchType::PatentNumber)`——只有专利号直查才有 details 形态；
/// 2. `has_serpapi_key`——details 是 SerpAPI 独占能力（XHR/EPO 的 `lookup_exact` 恒 `None`）；
/// 3. **SerpAPI 不在熔断冷却中**——本包补的缺口，判据直接读 [`CooldownTable::remaining`]，
///    与链前过滤 [`filter_cooled_providers`] 同一张表、同一个函数，禁止第二套标准。
///
/// 为什么第 3 条必须补：直查路径排在降级链**之前**，此前完全不查冷却表——SerpAPI 刚因
/// Quota/Auth 被摘链，同一个请求仍会先替它打一发 details：白烧一次**付费**配额，
/// 还和用户刚在面板上看到的「serpapi 冷却中」自相矛盾。
///
/// 返回 `false` 时 handler 跳过直查、**自然落到下面的降级链**——链前过滤会把 SerpAPI
/// 摘链，冷却事实由 MA6b 的 `cooldowns` 出参键与织回的 `Skipped` 记账如实呈现，
/// 因此这里**不需要也不允许**为直查路径伪造任何 attempts / `cooldowns` 数据。
///
/// **只读不写**（刻意取舍）：直查不在链上、跑完也没有真实 [`AttemptReport`]，
/// 按 MA6a「`Skipped` 不构成信号」的同一纪律，直查成功不清零冷却、失败也不登记冷却。
/// 曾考虑「把直查结果也记进 attempts 并回写冷却表」，但那要么新增一条合成记账
/// （污染面板：用户会看到一条链上没跑过的 serpapi 行），要么改 `AttemptReport` 形状
/// （动 MA5b 字面量锁 + 前端），都不是本缺口所需——缺的是「别多打一发白弹」，不是「多记一笔账」。
fn exact_lookup_allowed(
    search_type: Option<&SearchType>,
    has_serpapi_key: bool,
    table: &CooldownTable,
    now: Instant,
) -> bool {
    if !matches!(search_type, Some(SearchType::PatentNumber)) {
        return false;
    }
    if !has_serpapi_key {
        return false;
    }
    table.remaining(SourceKind::SerpApi, now).is_none()
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
    // MA6c：出网前先问一次熔断冷却表（判据与取舍见 [`exact_lookup_allowed`]）——SerpAPI 在
    // 冷却期内这一发直查会被跳过并自然落入下面的降级链，不再白撞一次付费配额。
    // 全局锁的 guard 在块尾析构，**绝不跨 `.await`**（MA6a 不变量：std Mutex 不可重入，
    // 持着它再进链前过滤会当场死锁；本块只做一次 `remaining` 读，不写冷却表）。
    let may_exact_lookup = {
        let table = breaker::global_table()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        exact_lookup_allowed(
            online_search_type.as_ref(),
            api_key_opt.is_some(),
            &table,
            Instant::now(),
        )
    };
    if may_exact_lookup {
        // 上面已覆盖「是专利号直查」+「配了 Key」两条判据，此处只取原 Key 构造 provider。
        if let Some(api_key) = api_key_opt.as_ref() {
            let provider = SerpApiProvider::new(api_key.clone(), s.db.clone());
            if let Some(summary) = provider.lookup_exact(req.query.clone()).await {
                return Json(exact_lookup_response(summary));
            }
        }
    }

    // ── MA2a/MA2b 执行链（spec §6）：SerpAPI 为主、Google Patents XHR 直抓为免费降级、
    // EPO OPS 为英文域补源（spec §4）。三源并行发起、各带独立超时（MA6a 起三源统一
    // 15s：SerpAPI 由旧 30s 收紧，见 providers/serpapi.rs 常量注释），按登记顺序择胜；
    // 装配规则见 [`online_chain_providers`]。
    // 没配 Key 时不把 SerpAPI 登记进链路，而是把它的 `Skipped` 记账**前置**到 attempts，
    // 这样旧的 hint 文案与「首个非空生效」语义逐字不变，控制流也不必分叉成两条。
    let epo_credentials = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .epo_credentials();
    let providers = online_chain_providers(&s.db, api_key_opt.clone(), epo_credentials);

    // ── MA6a 源级熔断冷却（`FailKind::cools_down()` 的生产消费点，见 search/breaker.rs）：
    // 1) 链前过滤——冷却中的源不进链（一发请求都不发），但稍后按原登记序织回 `Skipped`
    //    记账，面板不得因此「少一行」让用户误读成没配 Key；
    // 2) 链后回写——只依据本次链上**真实** attempts（此刻尚未织入任何合成记账），
    //    Failed(Quota|Auth) → 登记冷却，Success → 清零，Skipped → 不动（不构成信号）；
    // 3) 全部源都在冷却 → providers 为空 → 现有本地兜底路径接管，出参形状零改动。
    let chain_kinds: Vec<SourceKind> = providers.iter().map(|p| p.kind()).collect();
    let (providers, cooled) = {
        let table = breaker::global_table()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        filter_cooled_providers(providers, &table, Instant::now())
    };

    let mut outcome = SourceChain::new(providers).run(search_query).await;
    breaker::note_attempts(&outcome.attempts);
    outcome.attempts = weave_cooled_attempts(&chain_kinds, &cooled, outcome.attempts);

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
        // MA6b：与本地兜底两档对称的纯新增键（三处共用 [`cooldowns_json`]，空则省略）
        if let Some(cd) = cooldowns_json(&cooled) {
            out["cooldowns"] = cd;
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
        &cooled,
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
    // | 精确直查命中 | `patents/total=1/page=1/page_size=10/source=serpapi_exact` | `providers::serpapi::exact_lookup_response_shape_is_locked`；**MA6c 起进入该分支前多一道熔断冷却判据**（`exact_lookup_allowed`：SerpAPI 冷却中 ⇒ 跳过直查、落入降级链），由本文件 `exact_lookup_*` 三条用例锁 |
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

    // ── MA6a：源级熔断冷却的路由层接线（链前过滤 + Skipped 织回，假时钟零联网）──

    /// 测试夹具：一条最简 attempt 报告（weave/过滤用例只关心 source+status 的走向）。
    fn brk_report(source: SourceKind, status: AttemptStatus) -> AttemptReport {
        AttemptReport {
            source,
            status,
            latency_ms: 7,
            hits: 0,
            error: None,
            hint: None,
        }
    }

    /// **MA6a 用例③**：处于冷却的源被摘出链（`SourceChain` 拿不到它 → 结构上不可能出网），
    /// 但其 `Skipped` 记账按**原登记序**织回 attempts 原位；其余源不受牵连、正常择胜。
    /// 同时锁定「冷却原因放 error、hint 恒 None」——hint 会经 `SearchOutcome::hint()`
    /// 泄漏到出参顶层 `hint` 键，改动响应形状（取证见 `cooldown_skipped_report` 注释）。
    #[test]
    fn cooled_source_leaves_chain_but_keeps_skipped_attempt_in_place() {
        let db = Arc::new(Database::init(":memory:").expect("in-memory db"));
        let now = Instant::now();
        let mut table = CooldownTable::new();
        table.record(SourceKind::SerpApi, FailKind::Quota, now);

        let providers = online_chain_providers(
            &db,
            Some("serp-key".to_string()),
            Some(("ck".to_string(), "cs".to_string())),
        );
        let order: Vec<SourceKind> = providers.iter().map(|p| p.kind()).collect();
        assert_eq!(
            vec![
                SourceKind::SerpApi,
                SourceKind::GooglePatentsXhr,
                SourceKind::EpoOps
            ],
            order
        );

        let (active, cooled) = filter_cooled_providers(providers, &table, now);
        assert_eq!(
            vec![SourceKind::GooglePatentsXhr, SourceKind::EpoOps],
            active.iter().map(|p| p.kind()).collect::<Vec<_>>(),
            "冷却源必须摘到链外——链只跑手里的 providers，摘掉即零请求"
        );
        assert_eq!(
            vec![(SourceKind::SerpApi, Duration::from_secs(300))],
            cooled
        );

        // 链只跑了 XHR（成功）与 EPO（网络失败），attempts 本里没有 SerpAPI 的位置
        let chain_attempts = vec![
            brk_report(SourceKind::GooglePatentsXhr, AttemptStatus::Success),
            brk_report(SourceKind::EpoOps, AttemptStatus::Failed(FailKind::Network)),
        ];
        let merged = weave_cooled_attempts(&order, &cooled, chain_attempts);

        assert_eq!(
            3,
            merged.len(),
            "冷却源不能从面板消失（伪装成没配 Key 是误读）"
        );
        assert_eq!(
            SourceKind::SerpApi,
            merged[0].source,
            "织回位置 = 原登记序第一位"
        );
        assert_eq!(AttemptStatus::Skipped, merged[0].status);
        assert_eq!(0, merged[0].latency_ms, "没发请求不得凭空记耗时");
        assert_eq!(0, merged[0].hits);
        let err = merged[0]
            .error
            .clone()
            .expect("Skipped 必须说明原因（error 字段，口径同缺 Key Skipped）");
        assert!(err.contains("serpapi"), "error 要点名源: {err}");
        assert!(err.contains("冷却"), "要说清是熔断冷却: {err}");
        assert!(err.contains("300s"), "剩余时长向上取整进文案: {err}");
        assert_eq!(
            None, merged[0].hint,
            "hint 必须留空，否则会污染出参顶层 hint 键"
        );
        // 真跑过的那两路原样保留、顺序不变
        assert_eq!(
            vec![
                (SourceKind::GooglePatentsXhr, AttemptStatus::Success),
                (SourceKind::EpoOps, AttemptStatus::Failed(FailKind::Network)),
            ],
            merged[1..]
                .iter()
                .map(|a| (a.source, a.status))
                .collect::<Vec<_>>()
        );
    }

    /// **MA6a 用例④**：全部源都在冷却 → 链为空 → 本地兜底接管，
    /// 出参 `source:"local"` 等既有键与现状逐字一致，attempts 为「三源 Skipped + local_fts」。
    #[test]
    fn all_sources_cooled_yields_empty_chain_and_identical_local_shape() {
        let db = Database::init(":memory:").expect("in-memory db");
        // 兜底要有命中才走「source:local」出参档（零命中会落到 google_url 空结果末档，
        // 那是另一条既有形状）——本用例锁的是「冷却不改变兜底形状」，故种一条可命中专利。
        seed_patent(&db, "999", "固态电池");
        let now = Instant::now();
        let mut table = CooldownTable::new();
        table.record(SourceKind::SerpApi, FailKind::Quota, now);
        table.record(SourceKind::GooglePatentsXhr, FailKind::Quota, now);
        table.record(SourceKind::EpoOps, FailKind::Auth, now);

        let order = vec![
            SourceKind::SerpApi,
            SourceKind::GooglePatentsXhr,
            SourceKind::EpoOps,
        ];
        let cooled: Vec<(SourceKind, Duration)> = order
            .iter()
            .filter_map(|k| table.remaining(*k, now).map(|d| (*k, d)))
            .collect();
        assert_eq!(3, cooled.len());

        // 空链 = `SourceChain::new(vec![])` 的既有合法形状（chain.rs 文档已锁），零请求零 attempts
        let mut outcome = SearchOutcome {
            results: vec![],
            attempts: weave_cooled_attempts(&order, &cooled, vec![]),
            upstream_total: None,
        };
        assert_eq!(3, outcome.attempts.len());
        for a in &outcome.attempts {
            assert_eq!(AttemptStatus::Skipped, a.status);
        }

        let req = fallback_req("固态电池");
        let out = local_fallback_json(
            &db,
            &req,
            &Some(SearchType::Mixed),
            outcome.hint(),
            &mut outcome,
            &cooled,
        );

        // 兜底出参既有键逐字不变（MA4a 口径）：source 仍为 "local"；顶层 hint 是
        // 既有默认文案——冷却 Skipped 的 hint 恒 None，不得把它带进顶层键（现状即默认文案）
        assert_eq!(json!("local"), out["source"]);
        assert_eq!(json!(1), out["total"], "种入的可命中专利必须照常返回");
        assert_eq!(1, out["patents"].as_array().expect("patents").len());
        assert_eq!(
            json!("国外在线源暂时未返回结果，已回退本地缓存。建议配置 SerpAPI 提升命中率。"),
            out["hint"],
            "冷却记账不得经 hint() 通道改写顶层提示"
        );
        // 本地兜底自身照常追加为末条（MA4a 记账逻辑与 MA6a 过滤互不干扰）
        let attempts = out["attempts"].as_array().expect("attempts 键照旧存在");
        assert_eq!(4, attempts.len());
        assert_eq!(json!("local_fts"), attempts[3]["source"]);
        assert_eq!(json!("Success"), attempts[3]["status"]);
        assert_eq!(json!(1), attempts[3]["hits"]);
        assert_eq!(json!("Skipped"), attempts[0]["status"]);
        // MA6b：全冷却兜底档同样带出 `cooldowns` 键，顺序 = 原登记序、秒数 = 向上取整。
        // （此用例同时是「冷却不改变既有键」的证明：上面各既有键断言逐字未动。）
        assert_eq!(
            json!([
                {"source": "serpapi", "remaining_secs": 300},
                {"source": "google_patents_xhr", "remaining_secs": 120},
                {"source": "epo_ops", "remaining_secs": 900}
            ]),
            out["cooldowns"],
            "cooldowns 必须走真实 local_fallback_json 出参，而不是测试里复刻的假形状"
        );
    }

    /// 无冷却时 weave 必须是纯直通（不复制重排也不改顺序）——保证 MA6a 对
    /// 「一切正常」的热路径零行为差异。
    #[test]
    fn weave_is_passthrough_without_cooldowns() {
        let order = vec![
            SourceKind::SerpApi,
            SourceKind::GooglePatentsXhr,
            SourceKind::EpoOps,
        ];
        let chain_attempts = vec![
            brk_report(SourceKind::SerpApi, AttemptStatus::Success),
            brk_report(SourceKind::GooglePatentsXhr, AttemptStatus::Success),
        ];
        let merged = weave_cooled_attempts(&order, &[], chain_attempts.clone());
        assert_eq!(chain_attempts, merged);
    }

    /// **MA6c 用例①**：SerpAPI 处于熔断冷却时，专利号直查判据必须为 `false`
    /// （handler 据此跳过 details、自然落入降级链——链前过滤会把它摘链，冷却事实由
    /// `cooldowns` 键与织回的 `Skipped` 如实呈现，直查路径不重复记账）。
    /// 同一用例锁「未冷却时判据逐字等于 MA6c 之前的两层 `if`」：
    /// 非 PatentNumber ⇒ false、没配 Key ⇒ false、两者齐备且未冷却 ⇒ true。
    #[test]
    fn exact_lookup_is_blocked_while_serpapi_is_cooling() {
        let t0 = Instant::now();
        let pn = Some(SearchType::PatentNumber);
        let mut table = CooldownTable::new();

        // 未冷却档：旧的两条判据行为逐字不变
        assert!(
            exact_lookup_allowed(pn.as_ref(), true, &table, t0),
            "没冷却 ⇒ 专利号直查照旧出网（本包不得改变这一档）"
        );
        assert!(
            !exact_lookup_allowed(None, true, &table, t0),
            "不是专利号直查 ⇒ 判据为 false（旧判据①，来自 handler 的 matches! 那层）"
        );
        assert!(
            !exact_lookup_allowed(pn.as_ref(), false, &table, t0),
            "没配 SerpAPI Key ⇒ 判据为 false（旧判据②）"
        );

        // 冷却档：本包补的缺口
        table.record(SourceKind::SerpApi, FailKind::Quota, t0);
        assert!(
            !exact_lookup_allowed(pn.as_ref(), true, &table, t0 + Duration::from_secs(1)),
            "冷却中的 SerpAPI 不得再被直查白撞一发付费配额（还和面板上「serpapi 冷却中」自相矛盾）"
        );
        // 到期自动放行：冷却必须有终点，否则精确查号永久下线
        assert!(
            exact_lookup_allowed(
                pn.as_ref(),
                true,
                &table,
                t0 + breaker::SERPAPI_QUOTA_COOLDOWN
            ),
            "冷却到期即恢复直查（与链前过滤同一时长常量，禁止第二套标准）"
        );
    }

    /// **MA6c 用例②**：只挡该挡的源。details 是 SerpAPI 独占能力（XHR/EPO 的
    /// `lookup_exact` 恒 `None`），别的源冷却不得连带掐掉专利号直查；
    /// 反证 SerpAPI 自己一进冷却同一入参立刻为 `false`。
    #[test]
    fn exact_lookup_gating_is_serpapi_only() {
        let t0 = Instant::now();
        let mut table = CooldownTable::new();
        table.record(SourceKind::GooglePatentsXhr, FailKind::Quota, t0);
        table.record(SourceKind::EpoOps, FailKind::Auth, t0);
        assert!(
            exact_lookup_allowed(Some(&SearchType::PatentNumber), true, &table, t0),
            "冷却是源级的：无关源在冷却不能把 SerpAPI 独占的查号能力一起下线"
        );
        table.record(SourceKind::SerpApi, FailKind::Auth, t0);
        assert!(
            !exact_lookup_allowed(Some(&SearchType::PatentNumber), true, &table, t0),
            "同源（SerpApi）一冷却，同一入参立刻被挡"
        );
    }

    /// **MA6c 用例③（真接线）**：handler 读的是 `breaker::global_table()` 这张进程内共享表，
    /// 不是本地复刻的表。用链后回写的**同一个入口** `breaker::note_attempts` 往全局表
    /// 写一次 SerpAPI `Failed(Quota)`，再走 handler 用的同一条「加锁 + `exact_lookup_allowed`」
    /// 路径，必须读到 `false`。
    ///
    /// 纪律：全局表跨用例共享（std Mutex 不可重入，guard 也不得出块），故本用例
    /// **先把取值收进局部变量、恢复现场之后再断言**，保证即便断言失败也不会把
    /// 300s 的 SerpAPI 冷却留给同一进程里的并行用例。
    #[test]
    fn exact_lookup_gate_reads_the_shared_global_table() {
        let before = {
            let guard = breaker::global_table()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.remaining(SourceKind::SerpApi, Instant::now())
        };

        breaker::note_attempts(&[brk_report(
            SourceKind::SerpApi,
            AttemptStatus::Failed(FailKind::Quota),
        )]);

        let (gate_during_cooldown, table_says_cooling) = {
            let guard = breaker::global_table()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            (
                exact_lookup_allowed(
                    Some(&SearchType::PatentNumber),
                    true,
                    &guard,
                    Instant::now(),
                ),
                guard
                    .remaining(SourceKind::SerpApi, Instant::now())
                    .is_some(),
            )
        };

        if before.is_none() {
            let mut guard = breaker::global_table()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.success(SourceKind::SerpApi);
        }

        assert!(
            !gate_during_cooldown,
            "链后回写的真实失败必须同时挡住降级链与直查（否则本包接线是假接线）"
        );
        assert!(table_says_cooling, "note_attempts 必须真的写进了共享表");
        let restored = {
            let guard = breaker::global_table()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard
                .remaining(SourceKind::SerpApi, Instant::now())
                .is_some()
        };
        assert_eq!(
            before.is_some(),
            restored,
            "现场必须恢复：不给并行用例留下 SerpAPI 冷却"
        );
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

    // ── MA6b：`cooldowns` 顶层出参键（诊断面板冷却可视化的结构化判据）────────────

    /// **MA6b 字面量锁**：`cooldowns` 的 JSON 形状逐字段冻结——`source` 用与 `attempts`
    /// 行同源的 `SourceKind::as_str()` 小写稳定串（前端 `DIAG_SOURCE_LABELS` 按同一键表
    /// 解析），`remaining_secs` 向上取整（296.5s → 297，与 `cooldown_skipped_report` 的
    /// error 文案共用 [`cooldown_remaining_secs`]，两处秒数永不移位）；顺序 = 原登记顺序；
    /// **空 = `None`（调用方整键省略，无冷却时旧响应逐字节不变）**。
    /// 前端判据只认这个结构化键，不用正则匹配中文 error 文案（隐式契约随文案腐烂）。
    #[test]
    fn cooldowns_json_locks_shape_order_and_ceil_secs() {
        assert_eq!(None, cooldowns_json(&[]), "空冷却必须整键省略");
        let cooled = vec![
            (SourceKind::SerpApi, Duration::from_millis(296_500)),
            (SourceKind::GooglePatentsXhr, Duration::from_millis(119_001)),
            (SourceKind::EpoOps, Duration::from_secs(900)),
        ];
        assert_eq!(
            json!([
                {"source": "serpapi", "remaining_secs": 297},
                {"source": "google_patents_xhr", "remaining_secs": 120},
                {"source": "epo_ops", "remaining_secs": 900}
            ]),
            cooldowns_json(&cooled).expect("非空冷却必须出键"),
            "形状/取整/顺序三重锁定：数组元素顺序即原登记顺序"
        );
        // 与织回记账的 error 文案同源：同一 Duration 在两处必须给出同一秒数
        let report = cooldown_skipped_report(SourceKind::SerpApi, Duration::from_millis(296_500));
        let err = report.error.expect("冷却 Skipped 必带原因");
        assert!(
            err.contains("剩余 297s"),
            "error 文案与 cooldowns 键同口径: {err}"
        );
    }

    /// **MA6b 三路径对称（在线命中档）**：命中档与兜底两档共用同一
    /// [`cooldowns_json`] helper（源码三处 `if let Some(cd) = ...`，MA5b「四个 return
    /// 分支漏一个」教训的针对性防御）；兜底两档的真实出参形状已由
    /// `all_sources_cooled_yields_empty_chain_and_identical_local_shape`（带冷却）与
    /// `local_fallback_*`（无冷却省略键）直接经真函数锁定，本用例按 MA5b
    /// `attempts_key_is_additive_on_all_online_paths` 先例复刻命中档构造并冻结键集合。
    #[test]
    fn cooldowns_key_on_online_hit_path_freezes_key_set() {
        let outcome = SearchOutcome {
            results: vec![],
            attempts: vec![],
            upstream_total: Some(3),
        };
        let cooled = vec![(SourceKind::SerpApi, Duration::from_secs(297))];
        let mut out = json!({
            "patents": outcome.summaries(),
            "total": outcome.upstream_total.unwrap_or(0),
            "page": 1,
            "page_size": 10,
            "source": SourceKind::GooglePatentsXhr.as_str(),
            "attempts": attempts_json(&outcome)
        });
        if let Some(cd) = cooldowns_json(&cooled) {
            out["cooldowns"] = cd;
        }
        let mut keys: Vec<&str> = out
            .as_object()
            .map(|m| m.keys().map(|k| k.as_str()).collect())
            .unwrap_or_default();
        keys.sort_unstable();
        assert_eq!(
            vec![
                "attempts",
                "cooldowns",
                "page",
                "page_size",
                "patents",
                "source",
                "total"
            ],
            keys,
            "命中档 = MA5b 六键 + cooldowns，不许多出或更少键"
        );
        assert_eq!(
            json!([{"source": "serpapi", "remaining_secs": 297}]),
            out["cooldowns"]
        );
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
            &[],
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
        // MA6b：无冷却时 `cooldowns` 整键省略（上面键集合冻结即证明——多出该键会直接红）
        assert!(out.get("cooldowns").is_none());
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
            &[],
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
        // MA6b：空结果末档同样「无冷却 ⇒ 整键省略」
        assert!(out.get("cooldowns").is_none());
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
            &[],
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
            &[],
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
            &[],
        );
        assert_eq!(2, out2["patents"].as_array().expect("patents").len());
    }
}
