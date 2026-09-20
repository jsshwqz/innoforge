//! Google Patents 直抓源 / hidden XHR endpoint（spec §2，免费无 Key）
//!
//! MA2 执行链里的第二个在线源：SerpAPI 无 Key / 配额耗尽 / 网络故障时的免费降级路。
//!
//! ## 端点与参数（spec §2 逐条实证，证据汇总见本模块末尾的 `EVIDENCE` 常量注释）
//!
//! `GET https://patents.google.com/xhr/query?url={urlencode(内层查询串)}`
//! 内层串就是搜索框语法：`q=…&country=CN&language=CHINESE&sort=new&assignee=…`
//!
//! ## 响应结构（实证，与网页 DOM 不同 —— spec §2 已知坑 3）
//!
//! ```text
//! { "results": {
//!     "total_num_results": 14627, "total_num_pages": 77, "num_page": 0,
//!     "cluster": [ { "result": [ { "id": "patent/CN120275837B/zh", "rank": 0,
//!                                  "patent": { title, snippet, publication_number,
//!                                              assignee, inventor, priority_date,
//!                                              filing_date, publication_date, grant_date,
//!                                              language, thumbnail, pdf, family_metadata } } ] } ] } }
//! ```
//! 零命中时 `cluster` 是 `[{}]` —— **没有 `result` 键**，映射必须容错，
//! 不能用 `expect`/索引直取（本仓禁 unwrap，见 AGENTS.md 2.7）。
//!
//! ## 已知坑的处置
//! 1. **反爬 503（实证）**：本机连打 3 发（间隔 <8s）即返回 503 + HTML「Sorry…」页。
//!    → 全局最小间隔 [`MIN_REQUEST_INTERVAL`]（2s）+ 抖动，命中后按 [`FailKind::for_xhr_reply`]
//!    判为 `Quota` 并指数退避重试一次（spec §1：429 尊重 `Retry-After`）。
//! 2. **服务端日期过滤静默返回空**（spec §2，存档原文已确认）
//!    → 内层串**不下发** `after=`/`before=`，改由 [`filter_by_publication_window`] 客户端二次过滤。
//! 3. **结构不同于 DOM** → 按上面的实测结构建模。
//! 4. `<b>` 高亮标签（存档原文称字段会带）→ [`strip_highlight_tags`] 防御性剥离；
//!    本次 CN 冒烟响应中实测 0 处，故剥离只做兼容不做依据。

use crate::db::Database;
use crate::patent::Patent;
use crate::search::merge::merged_from;
use crate::search::model::{
    report_excerpt, AttemptReport, AttemptStatus, FailKind, Lang, SearchOutcome, SearchQuery,
    SourceKind,
};
use crate::search::provider::SearchProvider;
use crate::search::query::{normalize_date, render_q};
use crate::search::relevance::rank_and_gate_hits;
use crate::types::search::PatentSummary;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

/// 端点（spec §2）。
const XHR_ENDPOINT: &str = "https://patents.google.com/xhr/query";

/// 单次上游请求超时。spec §1 要求 15s —— MA1 刻意保留旧 30s 未动，**本项即该遗留的落点**；
/// SerpAPI 侧的 30s 本次不动（改了会改变现役端点的超时预算分配）。
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(15);

/// 全局最小请求间隔（spec §2 已知坑 1「建议 ≥2s 间隔」的下限）。
pub const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(2);

/// 间隔抖动上限：避免固定节奏被识别为机器行为。
pub const MAX_JITTER: Duration = Duration::from_millis(700);

/// 首次被限流后的退避基数（指数：base, base*2 …）。
pub const BASE_BACKOFF: Duration = Duration::from_secs(2);

/// 上游总尝试次数（spec §1：「带独立超时 + 单次重试」→ 首发 + 1 次重试 = 2）。
pub const MAX_ATTEMPTS: usize = 2;

/// 本源的固定标识，进 `AttemptReport`、日志与 `Patent.source`。
const SOURCE_LABEL: &str = "google_patents_xhr";

/// 一次 HTTP 往返的结果（`Retry-After` 单独带出，spec §1 要求 429 尊重它）。
#[derive(Debug, Clone)]
pub struct XhrReply {
    pub status: u16,
    pub body: String,
    pub retry_after: Option<Duration>,
}

/// 传输层故障（只有「没能拿到响应」才需要与 `XhrReply` 区分，供 `FailKind::Network` 归因）。
#[derive(Debug, Clone)]
pub enum XhrFault {
    /// 超时 / 连接失败 / DNS
    Network(String),
}

/// HTTP 传输抽象 —— **降级链测试的注入点**（离线，不出网）。
pub trait XhrTransport: Send + Sync {
    fn fetch(
        &self,
        url: String,
    ) -> Pin<Box<dyn Future<Output = Result<XhrReply, XhrFault>> + Send + '_>>;
}

/// 时间抽象 —— 限速/退避的**假时钟注入点**，让退避断言可在毫秒级完成。
pub trait XhrClock: Send + Sync {
    /// 单调毫秒（只用于测量间隔与耗时，不做绝对时间）。
    fn now_ms(&self) -> u64;
    fn sleep(&self, dur: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// 生产传输：reqwest + 15s 超时。
pub struct ReqwestXhrTransport {
    client: reqwest::Client,
}

impl ReqwestXhrTransport {
    pub fn new() -> Self {
        ReqwestXhrTransport {
            client: reqwest::Client::builder()
                .timeout(UPSTREAM_TIMEOUT)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// 解析 `Retry-After`（秒数形式）。非法/缺失一律 `None`，绝不 panic。
    fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
        headers
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(Duration::from_secs)
    }
}

impl Default for ReqwestXhrTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl XhrTransport for ReqwestXhrTransport {
    fn fetch(
        &self,
        url: String,
    ) -> Pin<Box<dyn Future<Output = Result<XhrReply, XhrFault>> + Send + '_>> {
        Box::pin(async move {
            match self.client.get(&url).send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let retry_after = Self::retry_after(resp.headers());
                    match resp.text().await {
                        Ok(body) => Ok(XhrReply {
                            status,
                            body,
                            retry_after,
                        }),
                        Err(e) => Err(XhrFault::Network(format!("读取响应体失败: {e}"))),
                    }
                }
                Err(e) => Err(XhrFault::Network(format!("请求未发出/未完成: {e}"))),
            }
        })
    }
}

/// 生产时钟：`Instant` 单调毫秒 + `tokio::time::sleep`。
pub struct SystemXhrClock;

impl XhrClock for SystemXhrClock {
    fn now_ms(&self) -> u64 {
        // 以进程启动点为零点的单调毫秒；只用于差值，故基准无关紧要。
        START.get_or_init(Instant::now).elapsed().as_millis() as u64
    }

    fn sleep(&self, dur: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move { tokio::time::sleep(dur).await })
    }
}

static START: OnceLock<Instant> = OnceLock::new();

/// Google Patents 直抓源。
pub struct GooglePatentsXhrProvider {
    db: Arc<Database>,
    transport: Arc<dyn XhrTransport>,
    clock: Arc<dyn XhrClock>,
    /// 全局（进程内跨实例）上次请求时刻，配合 `Mutex` 实现最小间隔限速。
    last_request_ms: Arc<AtomicU64>,
}

impl GooglePatentsXhrProvider {
    /// 生产构造：真实 reqwest 传输 + 系统时钟。
    pub fn new(db: Arc<Database>) -> Self {
        GooglePatentsXhrProvider {
            db,
            transport: Arc::new(ReqwestXhrTransport::new()),
            clock: Arc::new(SystemXhrClock),
            last_request_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 测试构造：注入假传输 / 假时钟（降级链三用例用，全程离线）。
    pub fn with_transport(
        db: Arc<Database>,
        transport: Arc<dyn XhrTransport>,
        clock: Arc<dyn XhrClock>,
    ) -> Self {
        GooglePatentsXhrProvider {
            db,
            transport,
            clock,
            last_request_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 该查询是否按中文语境请求（与 SerpAPI 侧同名判定同语义；此处独立成文是因为
    /// 两源的 `language` 参数取值域不同，判定本身仍是一行的 trivial 表达式）。
    fn wants_chinese(query: &SearchQuery) -> bool {
        matches!(query.language, Some(Lang::Chinese))
    }

    /// 内层 `url=` 串（spec §2 的搜索框语法）。
    ///
    /// 参数顺序固定 `q → country → language → assignee → inventor → sort`，
    /// 便于逐字符断言。**日期参数刻意缺席**（已知坑 2），`page`/`num` 亦不下发
    /// （见 [`Self::pagination_note`]）。
    pub(crate) fn inner_url(query: &SearchQuery) -> String {
        // 摘掉日期后交给共用的 render_q，保证 q 串渲染全仓只有一份出处（AGENTS.md 2.2）；
        // 日期改由客户端过滤，见 filter_by_publication_window。
        let mut q_source = query.clone();
        q_source.date_from = None;
        q_source.date_to = None;

        let mut parts = vec![format!("q={}", render_q(&q_source))];
        if let Some(c) = query.country.as_deref() {
            if !c.is_empty() {
                parts.push(format!("country={}", c.trim().to_ascii_uppercase()));
            }
        }
        match query.language {
            Some(Lang::Chinese) => parts.push("language=CHINESE".to_string()),
            Some(Lang::English) => parts.push("language=ENGLISH".to_string()),
            // All / 未指定：不下发，让上游自己跨语种
            _ => {}
        }
        if let Some(a) = query.assignee.as_deref() {
            if !a.trim().is_empty() {
                parts.push(format!("assignee={}", a.trim()));
            }
        }
        match query.sort_by.as_deref() {
            Some("new") => parts.push("sort=new".to_string()),
            Some("old") => parts.push("sort=old".to_string()),
            _ => {}
        }
        parts.join("&")
    }

    /// 完整请求 URL：`?url=` + 对内层串的**一次**百分号编码（实证：此形态返回 200 JSON）。
    pub(crate) fn request_url(query: &SearchQuery) -> String {
        format!(
            "{}?url={}",
            XHR_ENDPOINT,
            urlencoding::encode(&Self::inner_url(query))
        )
    }

    /// 翻页能力说明（MA2a 的如实边界，不是 TODO 暗坑）。
    ///
    /// 响应里的 `num_page` / `total_num_pages` 表明上游按页返回，但**页参数名未能在本次
    /// 实证中确认**（探测期间本机 IP 被 503 挡住，见 EVIDENCE 第 5 条），spec §2 的参数清单
    /// 里也没有它。宁可不翻页，也不伪造一个未验证的参数：`page > 1` 时本源返回第 1 页内容
    /// 并在 `AttemptReport::hint` 明示，交由 MA2b/MA3 补齐。
    fn pagination_note(query: &SearchQuery) -> Option<String> {
        if query.page > 1 {
            Some(
                "Google Patents 直抓源暂不支持翻页（页参数未实证），本次返回第 1 页内容。"
                    .to_string(),
            )
        } else {
            None
        }
    }

    /// 等待到允许发起下一次请求（spec §2 已知坑 1 的限速落点）。
    ///
    /// 用 `compare_exchange` 抢占时间槽：并发检索时后到者看到的是**已预约**的槽位，
    /// 因此不会有两个请求挤在同一瞬间打出去。
    async fn wait_for_slot(&self) {
        let min_gap = MIN_REQUEST_INTERVAL.as_millis() as u64;
        loop {
            let now = self.clock.now_ms();
            let last = self.last_request_ms.load(Ordering::SeqCst);
            if last != 0 && now.saturating_sub(last) < min_gap {
                let wait = min_gap - (now.saturating_sub(last));
                self.clock.sleep(Duration::from_millis(wait)).await;
                continue;
            }
            // 抖动：把 0..=MAX_JITTER 的随机毫秒加进「下次可发时刻」上，天然顺延自己的槽位。
            let jitter_ms = jitter_millis();
            if self
                .last_request_ms
                .compare_exchange(last, now + jitter_ms, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                if jitter_ms > 0 {
                    self.clock.sleep(Duration::from_millis(jitter_ms)).await;
                }
                return;
            }
        }
    }

    /// 一发请求 + 失败分类，返回「已归类好的结果或失败」。
    /// 与 `search()` 的分工：本函数只管一次往返及其记账素材，重试节奏在调用侧。
    ///
    /// 失败三元组的最后一项是上游给的 `Retry-After`（spec §1：429 必须尊重它）。
    async fn attempt_fetch(
        &self,
        url: &str,
    ) -> Result<XhrReply, (FailKind, String, Option<Duration>)> {
        match self.transport.fetch(url.to_string()).await {
            Ok(reply) => {
                if reply.status >= 400 {
                    // 判定顺序：先看状态码 + 反爬文案，再看是否 Parse（见 for_xhr_reply 注释）。
                    let kind = FailKind::for_xhr_reply(reply.status, &reply.body);
                    let retry_after = reply.retry_after;
                    return Err((
                        kind,
                        format!(
                            "{SOURCE_LABEL} 上游返回 HTTP {}（{}）：{}",
                            reply.status,
                            kind_label(kind),
                            report_excerpt(&reply.body)
                        ),
                        retry_after,
                    ));
                }
                Ok(reply)
            }
            Err(XhrFault::Network(msg)) => Err((
                FailKind::Network,
                format!("{SOURCE_LABEL} 网络故障：{msg}"),
                None,
            )),
        }
    }

    /// 命中行 → `PatentSummary[]`（打分/放行/中文过滤/去重走与 SerpAPI 共用的
    /// [`rank_and_gate_hits`]，避免第二份实现）。
    fn outcome_patents(
        &self,
        query: &SearchQuery,
        rows: Vec<serde_json::Value>,
    ) -> Vec<PatentSummary> {
        let cn_query = Self::wants_chinese(query);
        // 已知坑 2：服务端日期过滤在 XHR 路径上静默返回空 → 客户端二次过滤兜底。
        let rows = filter_by_publication_window(
            rows,
            query.date_from.as_deref(),
            query.date_to.as_deref(),
        );
        let keyword = render_q(query);
        let mut patents = rank_and_gate_hits(
            self.db.as_ref(),
            SOURCE_LABEL,
            &keyword,
            cn_query,
            &rows,
            xhr_to_patent,
        );
        // spec §2 未提供条数参数（存档证据亦无），故按 limit 做客户端截断（见 model.rs::limit 注释）。
        if query.limit > 0 && patents.len() > query.limit {
            patents.truncate(query.limit);
        }
        patents
    }

    /// 带限速与退避的取页循环（spec §1「独立超时 + 单次重试」+ spec §2「限速 + 退避」的落点）。
    ///
    /// `Ok(行数组, total_num_results)` = 拿到并解析成功；
    /// `Err(FailKind, 用户可读原因)` = 终态失败，分类已由 spec §1 的处置表定好。
    /// 拆成独立方法而非内联在 `search()` 里，是为了让「重试节奏」这件事只有一处、可直接读。
    async fn fetch_rows(
        &self,
        url: &str,
    ) -> Result<(Vec<serde_json::Value>, Option<usize>), (FailKind, String)> {
        let mut attempt = 0usize;
        loop {
            attempt += 1;
            self.wait_for_slot().await;
            match self.attempt_fetch(url).await {
                Ok(reply) => {
                    match serde_json::from_str::<serde_json::Value>(&reply.body) {
                        Ok(json) => {
                            let total = json["results"]["total_num_results"]
                                .as_u64()
                                .map(|v| v as usize);
                            return Ok((flatten_rows(&json), total));
                        }
                        Err(e) => {
                            // 2xx 但正文不是 JSON：结构变了 → Parse（spec §1：不重试、不降级）。
                            return Err((
                                FailKind::Parse,
                                format!(
                                    "{SOURCE_LABEL} 响应不是合法 JSON（上游结构可能已变化），\
                                     原因：{e}，片段：{}",
                                    report_excerpt(&reply.body)
                                ),
                            ));
                        }
                    }
                }
                Err((kind, msg, retry_after)) => {
                    // spec §1：Parse 不重试；其余（Network/Quota）退避后重试一次。
                    if !kind.switches_source() || attempt >= MAX_ATTEMPTS {
                        return Err((kind, msg));
                    }
                    // 429/503 带 Retry-After 时以上游为准（spec §1），否则按指数退避。
                    let backoff = retry_after.unwrap_or_else(|| backoff_for(attempt));
                    println!(
                        "[ONLINE] {SOURCE_LABEL} {kind:?} → 退避 {backoff:?} 后重试\
                         （第 {attempt} 次失败）"
                    );
                    self.clock.sleep(backoff).await;
                }
            }
        }
    }
}

/// [`FailKind`] 的中文短标签，只进 `AttemptReport::error` 文案，不进任何对外字段。
fn kind_label(kind: FailKind) -> &'static str {
    match kind {
        FailKind::Network => "网络故障",
        FailKind::Quota => "限流/配额",
        FailKind::Auth => "鉴权",
        FailKind::Parse => "响应结构变化",
    }
}

/// 0..=MAX_JITTER 的伪随机抖动毫秒。
///
/// 不引入 rand crate（AGENTS.md 禁新增依赖）：用纳秒时间戳取模，抖动只需破坏固定节奏，
/// 不需要密码学质量。
fn jitter_millis() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    nanos % (MAX_JITTER.as_millis() as u64 + 1)
}

/// 把 XHR 一条 `result` 记录映射为域内 [`Patent`]。
///
/// 字段名以实测响应为准（见文件头结构图）：`patent.publication_number` / `title` /
/// `snippet` / `assignee` / `inventor` / `filing_date` / `priority_date` /
/// `publication_date` / `grant_date`。**全部用 `as_str().unwrap_or("")` 取值**，
/// 缺字段只降级为空串，绝不 panic。
fn xhr_to_patent(r: &serde_json::Value) -> Patent {
    let p = &r["patent"];
    let clean = |key: &str| strip_highlight_tags(p[key].as_str().unwrap_or(""));
    let pub_num = clean("publication_number");
    let country = pub_num
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_uppercase();
    Patent {
        id: uuid::Uuid::new_v4().to_string(),
        patent_number: pub_num,
        title: clean("title"),
        abstract_text: clean("snippet"),
        description: String::new(),
        claims: String::new(),
        applicant: clean("assignee"),
        inventor: clean("inventor"),
        filing_date: clean("filing_date"),
        publication_date: clean("publication_date"),
        grant_date: if p["grant_date"].is_string() {
            Some(clean("grant_date"))
        } else {
            None
        },
        ipc_codes: String::new(),
        cpc_codes: String::new(),
        priority_date: clean("priority_date"),
        country,
        kind_code: String::new(),
        family_id: None,
        legal_status: String::new(),
        citations: "[]".into(),
        cited_by: "[]".into(),
        source: SOURCE_LABEL.into(),
        raw_json: r.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        images: "[]".into(),
        pdf_url: clean("pdf"),
    }
}

/// 剥离 `<b>` / `</b>` 高亮标记并压缩多余空白（spec §2 已知坑 4 / 存档原文）。
///
/// 注意：这是**展示字段清洗**，不影响入库的 `raw_json`（原文完整保留，符合 AGENTS.md 2.5
/// 「截断不得破坏数据完整性」）。
pub fn strip_highlight_tags(s: &str) -> String {
    if !s.contains('<') {
        return s.trim().to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut skip = false;
    for ch in s.chars() {
        match ch {
            '<' => skip = true,
            '>' => skip = false,
            c if !skip => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 客户端日期过滤（spec §2 已知坑 2 的修法：上游 `after=` 在 XHR 路径上静默返回空）。
///
/// 以 `publication_date` 为比较基准（缺则回退 `filing_date`），两边都归一为 8 位数字
/// `YYYYMMDD` 后做闭区间比较；日期串无法归一时**保留该条**（宁可多给不误删，
/// 过滤失效不能变成结果消失）。
pub fn filter_by_publication_window(
    rows: Vec<serde_json::Value>,
    date_from: Option<&str>,
    date_to: Option<&str>,
) -> Vec<serde_json::Value> {
    let from = date_from.and_then(normalize_date);
    let to = date_to.and_then(normalize_date);
    if from.is_none() && to.is_none() {
        return rows;
    }
    rows.into_iter()
        .filter(|r| {
            let p = &r["patent"];
            let day = p["publication_date"]
                .as_str()
                .and_then(normalize_date)
                .or_else(|| p["filing_date"].as_str().and_then(normalize_date));
            match day {
                // 该条没有任何可用日期：不做判断，保留
                None => true,
                Some(day) => {
                    from.as_ref().is_none_or(|f| day >= *f) && to.as_ref().is_none_or(|t| day <= *t)
                }
            }
        })
        .collect()
}

/// 从 `results.cluster[*].result[*]` 摊平出命中行。
///
/// `cluster` 是**数组**（同一次响应可能带多个聚类，实证见零命中时的 `[{}]` 形态），
/// 这里把所有 cluster 的 result 顺序拼接，保持上游 rank 的相对次序。
fn flatten_rows(json: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut rows = Vec::new();
    if let Some(clusters) = json["results"]["cluster"].as_array() {
        for cluster in clusters {
            if let Some(results) = cluster["result"].as_array() {
                rows.extend(results.iter().cloned());
            }
        }
    }
    rows
}

impl SearchProvider for GooglePatentsXhrProvider {
    fn kind(&self) -> SourceKind {
        SourceKind::GooglePatentsXhr
    }

    fn search<'a>(
        &'a self,
        query: SearchQuery,
    ) -> Pin<Box<dyn Future<Output = SearchOutcome> + Send + 'a>> {
        Box::pin(async move {
            let started = self.clock.now_ms();
            let url = Self::request_url(&query);
            let hint = Self::pagination_note(&query);
            println!(
                "[ONLINE] {SOURCE_LABEL} url={} page={}",
                report_excerpt(&url),
                query.page
            );

            let (rows, upstream_total, status, error_note) = match self.fetch_rows(&url).await {
                Ok((rows, total)) => (rows, total, AttemptStatus::Success, None),
                Err((kind, msg)) => (Vec::new(), None, AttemptStatus::Failed(kind), Some(msg)),
            };

            let patents = if matches!(status, AttemptStatus::Success) {
                self.outcome_patents(&query, rows)
            } else {
                Vec::new()
            };
            let hits = patents.len();

            SearchOutcome {
                results: merged_from(SourceKind::GooglePatentsXhr, patents),
                attempts: vec![AttemptReport {
                    source: SourceKind::GooglePatentsXhr,
                    status,
                    latency_ms: self.clock.now_ms().saturating_sub(started),
                    hits,
                    error: error_note,
                    hint,
                }],
                upstream_total,
            }
        })
    }
}

/// 第 `attempt` 次失败后的退避时长（指数）。
fn backoff_for(attempt: usize) -> Duration {
    let factor = 1u32 << (attempt.saturating_sub(1).min(4)) as u32;
    BASE_BACKOFF.saturating_mul(factor)
}

#[cfg(test)]
//
// `pub(crate)` 是刻意的：`search::chain` 的降级链三用例复用本模块的假传输/假时钟与
// 真实冒烟 fixture（见 chain::tests），避免在两个模块里各写一份同源测试脚手架。
// 除测试外无人可见（整块受 #[cfg(test)] 保护），不会泄漏进生产 API。
pub(crate) mod tests {
    use super::*;
    use crate::types::search::SearchType;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    // ─────────────────────────────────────────────────────────────────────────────
    // ⚠️ 实证材料（spec §2 / §8）
    //
    // 冒烟命令（只读 GET，无副作用）：
    //   curl -s -o xhr_cn.json -w "HTTP %{http_code} size=%{size_download} time=%{time_total}\n" \
    //     "https://patents.google.com/xhr/query?url=q%3D%28%E5%9B%BA%E6%80%81%E7%94%B5%E6%B1%A0%29\
    // %26country%3DCN%26language%3DCHINESE%26sort%3Dnew"
    //   → HTTP 200  size=22006  time=6.43s   （2026-09-20，本机住宅/办公出口 IP）
    //   首条 title = 「 一种动车组电池健康状态评估与寿命预测方法及系统」
    //   total_num_results = 14627, total_num_pages = 77, num_page = 0
    //   patent 键集 = title,snippet,priority_date,filing_date,grant_date,publication_date,
    //                  inventor,assignee,publication_number,language,thumbnail,pdf,family_metadata
    //
    //   反爬现场：紧随其后连打 3 发（间隔 <8s）
    //     → HTTP 503  size=1103  body = `<html>…<title>Sorry...</title>…`（**不是 JSON**）
    //     随后 20s / 8s 间隔的探测仍全部 503（封禁持续数分钟以上）
    //   → 这条现场就是「限速 + 退避 + 503 判 Quota 而非 Parse/Network」三件事的直接依据。
    //
    // 下面两个常量是那次 200 响应的**逐字段摘录**（snippet 为控制篇幅做了截断，
    // 只用于喂解析器，不参与任何被断言的数据完整性；其余字段值原样保留）。
    // ─────────────────────────────────────────────────────────────────────────────

    pub(crate) const REAL_CN_REPLY: &str = r#"{
 "results": {
  "total_num_results": 14627,
  "total_num_pages": 77,
  "many_results": false,
  "num_page": 0,
  "cluster": [
   {
    "result": [
     {
      "id": "patent/CN120275837B/zh",
      "rank": 0,
      "patent": {
       "title": " 一种动车组电池健康状态评估与寿命预测方法及系统",
       "snippet": " 本申请涉及动车电池组评估技术领域，提供了一种动车组电池健康状态评估与寿命预测方法及系统，包括通过浮充状态下的电压波动和…[截断，仅测试用]",
       "priority_date": "2025-06-11",
       "filing_date": "2025-06-11",
       "grant_date": "2025-09-05",
       "publication_date": "2025-09-05",
       "inventor": "陈奎",
       "assignee": "西南交通大学",
       "publication_number": "CN120275837B",
       "language": "zh",
       "thumbnail": "",
       "pdf": "",
       "family_metadata": { "aggregated": { "country_status": [ { "country_code": "CN" } ] } }
      }
     },
     {
      "id": "patent/CN120598460A/zh",
      "rank": 1,
      "patent": {
       "title": " 一种多模态融合的agv动态路径规划与集群调度系统",
       "snippet": " 本发明公开了一种多模态融合的AGV动态路径规划与集群调度系统，涉及多模态感知与数据融合技术领域，包括多模态感知模块：通…[截断，仅测试用]",
       "priority_date": "2025-05-23",
       "filing_date": "2025-05-23",
       "publication_date": "2025-09-05",
       "inventor": "刘徐丞",
       "assignee": "华东交通大学",
       "publication_number": "CN120598460A",
       "language": "zh",
       "thumbnail": "",
       "pdf": "",
       "family_metadata": { "aggregated": { "country_status": [ { "country_code": "CN" } ] } }
      }
     }
    ]
   }
  ],
  "chem_exhausted": false,
  "summary": {}
 }
}"#;

    /// ⚠️ 零命中形态：实测 `{"results":{"total_num_results":0,…,"cluster":[{}],…}}`
    /// —— `cluster` 里那个对象**没有 `result` 键**。映射若直取即 panic。
    pub(crate) const REAL_EMPTY_REPLY: &str = r#"{"results":{"total_num_results":0,"total_num_pages":0,"many_results":false,"num_page":0,"cluster":[{}],"chem_exhausted":false,"summary":{}}}"#;

    /// ⚠️ 反爬页形态（实测 503 响应体的开头，HTML 而非 JSON）。
    pub(crate) const REAL_503_BODY: &str = "<html><head><meta http-equiv=\"content-type\" content=\"text/html; charset=utf-8\"/><title>Sorry...</title><style> body { font-family: verdana, arial, sans-serif; }</style></head><body><div><table><tr><td><b><font face=sans-serif size=10><font color=#4285f4>G</font>";

    pub(crate) fn query(keyword: &str) -> SearchQuery {
        SearchQuery {
            keyword: keyword.to_string(),
            country: Some("CN".to_string()),
            language: Some(Lang::Chinese),
            assignee: None,
            exact_assignee: false,
            date_from: None,
            date_to: None,
            limit: 20,
            page: 1,
            sort_by: Some("new".to_string()),
            search_type: None,
        }
    }

    pub(crate) fn db() -> Arc<Database> {
        Arc::new(Database::init(":memory:").expect("in-memory db"))
    }

    // ── 假时钟 / 假传输（离线注入点） ────────────────────────────────────────────

    /// 假时钟：`sleep` 直接推进虚拟时间并留痕，使限速/退避断言在毫秒级完成。
    pub(crate) struct FakeClock {
        now: AtomicU64,
        sleeps: Mutex<Vec<Duration>>,
    }

    impl FakeClock {
        pub(crate) fn new() -> Arc<Self> {
            Arc::new(FakeClock {
                now: AtomicU64::new(0),
                sleeps: Mutex::new(Vec::new()),
            })
        }
        pub(crate) fn recorded(&self) -> Vec<Duration> {
            self.sleeps.lock().expect("lock").clone()
        }
    }

    impl XhrClock for FakeClock {
        fn now_ms(&self) -> u64 {
            self.now.load(Ordering::SeqCst)
        }
        fn sleep(&self, dur: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
            Box::pin(async move {
                self.now.fetch_add(dur.as_millis() as u64, Ordering::SeqCst);
                self.sleeps.lock().expect("push").push(dur);
            })
        }
    }

    /// 假传输：按脚本逐发返回，并记录每次请求的 URL（用于断言发出次数与 URL 形状）。
    pub(crate) struct FakeTransport {
        script: Mutex<VecDeque<XhrReply>>,
        /// 兜底：脚本耗尽后的固定回复（避免测试里出现「无脚本可用」的 panic）。
        fallback: XhrReply,
        urls: Mutex<Vec<String>>,
    }

    impl FakeTransport {
        pub(crate) fn new(script: Vec<XhrReply>) -> Arc<Self> {
            let mut queue = VecDeque::from(script);
            let fallback = queue
                .pop_back()
                .unwrap_or_else(|| reply(200, REAL_EMPTY_REPLY));
            Arc::new(FakeTransport {
                script: Mutex::new(queue),
                fallback,
                urls: Mutex::new(Vec::new()),
            })
        }
        pub(crate) fn fetch_count(&self) -> usize {
            self.urls.lock().expect("lock").len()
        }
        pub(crate) fn urls(&self) -> Vec<String> {
            self.urls.lock().expect("lock").clone()
        }
    }

    pub(crate) fn reply(status: u16, body: &str) -> XhrReply {
        XhrReply {
            status,
            body: body.to_string(),
            retry_after: None,
        }
    }

    impl XhrTransport for FakeTransport {
        fn fetch(
            &self,
            url: String,
        ) -> Pin<Box<dyn Future<Output = Result<XhrReply, XhrFault>> + Send + '_>> {
            Box::pin(async move {
                self.urls.lock().expect("push").push(url);
                let next = self.script.lock().expect("pop").pop_front();
                Ok(next.unwrap_or_else(|| self.fallback.clone()))
            })
        }
    }

    /// 组一个完全离线的源实例：假传输 + 假时钟，绝不出网。
    pub(crate) fn provider(
        script: Vec<XhrReply>,
    ) -> (GooglePatentsXhrProvider, Arc<FakeTransport>, Arc<FakeClock>) {
        let clock = FakeClock::new();
        let transport = FakeTransport::new(script);
        let p = GooglePatentsXhrProvider::with_transport(db(), transport.clone(), clock.clone());
        (p, transport, clock)
    }

    // ── 内层串 / 完整 URL（spec §2 参数集） ──────────────────────────────────────

    #[test]
    fn inner_url_emits_spec_2_param_set() {
        let mut q = query("固态电池");
        q.assignee = Some(" 张三 ".to_string());
        let inner = GooglePatentsXhrProvider::inner_url(&q);
        assert_eq!(
            "q=固态电池&country=CN&language=CHINESE&assignee=张三&sort=new",
            inner
        );
    }

    #[test]
    fn language_maps_to_upper_token_and_all_omits_param() {
        let mut en = query("battery");
        en.language = Some(Lang::English);
        assert!(GooglePatentsXhrProvider::inner_url(&en).contains("language=ENGLISH"));

        let mut all = query("battery");
        all.language = Some(Lang::All);
        assert!(
            !GooglePatentsXhrProvider::inner_url(&all).contains("language="),
            "All = 不限语种，不该下发 language 参数"
        );

        let none = query("battery");
        assert!(GooglePatentsXhrProvider::inner_url(&none).contains("language=CHINESE"));
    }

    #[test]
    fn country_is_uppercased_and_blank_dropped() {
        let mut q = query("x");
        q.country = Some(" us ".to_string());
        assert!(GooglePatentsXhrProvider::inner_url(&q).contains("country=US"));
        q.country = Some("".to_string());
        assert!(!GooglePatentsXhrProvider::inner_url(&q).contains("country="));
    }

    #[test]
    fn sort_only_accepts_new_and_old() {
        let mut q = query("x");
        for (value, expect) in [("new", true), ("old", true), ("relevance", false)] {
            q.sort_by = Some(value.to_string());
            assert_eq!(
                expect,
                GooglePatentsXhrProvider::inner_url(&q).contains("sort="),
                "sort={value}"
            );
        }
        q.sort_by = None;
        assert!(!GooglePatentsXhrProvider::inner_url(&q).contains("sort="));
    }

    /// ⚠️ 已知坑 2 的落点：日期**绝不进**内层串（服务端过滤会静默返回空集），
    /// 但 q 串里也不能出现 `after:`/`before:`（那是同一类服务端过滤）。
    #[test]
    fn date_filters_are_never_sent_upstream() {
        let mut q = query("固态电池");
        q.date_from = Some("2024-01-01".to_string());
        q.date_to = Some("2024-12-31".to_string());
        let inner = GooglePatentsXhrProvider::inner_url(&q);
        assert!(!inner.contains("after"), "{inner}");
        assert!(!inner.contains("before"), "{inner}");
        assert!(!inner.contains("2024"), "{inner}");
        // q 本身仍应保留关键词（摘日期不摘词）
        assert!(inner.starts_with("q=固态电池"), "{inner}");
    }

    #[test]
    fn request_url_single_encodes_inner_string() {
        let url = GooglePatentsXhrProvider::request_url(&query("固态电池"));
        assert!(
            url.starts_with("https://patents.google.com/xhr/query?url="),
            "{url}"
        );
        let inner = url
            .strip_prefix("https://patents.google.com/xhr/query?url=")
            .expect("prefix");
        // 内层串被**一次**编码：`=` → %3D、`&` → %26，且没有 %2525 这类双重编码痕迹
        assert!(inner.contains("%3D"), "{inner}");
        assert!(inner.contains("%26country%3DCN"), "{inner}");
        assert!(!inner.contains("%2525"), "出现双重编码: {inner}");
    }

    #[test]
    fn patent_number_search_type_still_flows_through_render_q() {
        let mut q = query("CN202420009882.7");
        q.search_type = Some(SearchType::PatentNumber);
        let inner = GooglePatentsXhrProvider::inner_url(&q);
        assert!(
            inner.starts_with("q=\"CN202420009882.7\" OR \"202420009882\""),
            "{inner}"
        );
    }

    // ── 响应建模（⚠️ 真实结构） ──────────────────────────────────────────────────

    #[test]
    fn flatten_rows_reads_real_cluster_shape() {
        let json: serde_json::Value = serde_json::from_str(REAL_CN_REPLY).expect("real reply");
        let rows = flatten_rows(&json);
        assert_eq!(2, rows.len());
        assert_eq!(
            "CN120275837B",
            rows[0]["patent"]["publication_number"]
                .as_str()
                .expect("pub")
        );
    }

    /// ⚠️ 真实反例：零命中时 `cluster: [{}]`（没有 result 键）。必须摊成空而非 panic。
    #[test]
    fn empty_cluster_object_is_tolerated() {
        let json: serde_json::Value = serde_json::from_str(REAL_EMPTY_REPLY).expect("empty reply");
        assert!(flatten_rows(&json).is_empty());
    }

    #[test]
    fn maps_real_row_to_patent_fields() {
        let json: serde_json::Value = serde_json::from_str(REAL_CN_REPLY).expect("real reply");
        let row = json["results"]["cluster"][0]["result"][0].clone();
        let p = xhr_to_patent(&row);
        assert_eq!("CN120275837B", p.patent_number);
        assert_eq!("CN", p.country, "国家码取公开号前缀字母");
        assert_eq!(
            "一种动车组电池健康状态评估与寿命预测方法及系统", p.title,
            "首尾空格应清掉"
        );
        assert_eq!("西南交通大学", p.applicant);
        assert_eq!("陈奎", p.inventor);
        assert_eq!("2025-06-11", p.filing_date);
        assert_eq!("2025-09-05", p.publication_date);
        assert_eq!(Some("2025-09-05".to_string()), p.grant_date);
        assert_eq!(SOURCE_LABEL, p.source);
        assert!(p.raw_json.contains("CN120275837B"), "原文完整入库");

        // 第 2 条上游就没有 grant_date → 必须是 None，不能是 Some("")
        let row2 = json["results"]["cluster"][0]["result"][1].clone();
        assert_eq!(None, xhr_to_patent(&row2).grant_date);
    }

    /// 缺字段/畸形结构一律降级为空串，不 panic（spec §2 已知坑 3 的健壮性底线）。
    #[test]
    fn missing_fields_degrade_to_empty_strings() {
        let p = xhr_to_patent(&serde_json::json!({"patent": {"title": "只有标题"}}));
        assert_eq!("只有标题", p.title);
        assert_eq!("", p.patent_number);
        assert_eq!("", p.country);
        assert_eq!(None, p.grant_date);
        // 连 patent 键都没有也不能炸
        assert_eq!("", xhr_to_patent(&serde_json::json!({})).title);
    }

    #[test]
    fn strip_highlight_tags_removes_b_and_collapses_spaces() {
        // 存档证据称命中词会被 <b> 包住；实测这批响应里没有，故只做兼容。
        assert_eq!(
            "固态 电池 装置",
            strip_highlight_tags("  <b>固态</b> <b>电池</b> 装置 ")
        );
        assert_eq!("无标签", strip_highlight_tags(" 无标签 "));
        // 小于号出现在非标签场合也不吞字（只做标签剔除，不做 HTML 解析）
        assert_eq!("a & b", strip_highlight_tags("a & b"));
    }

    // ── 客户端日期过滤（已知坑 2 的修法） ────────────────────────────────────────

    fn row_with(pubdate: &str) -> serde_json::Value {
        serde_json::json!({ "patent": { "publication_number": "CN1", "title": "t", "publication_date": pubdate } })
    }

    #[test]
    fn client_side_date_window_filters_by_publication_date() {
        let rows = vec![
            row_with("2023-12-31"),
            row_with("2024-06-15"),
            row_with("2025-01-01"),
        ];
        let kept = filter_by_publication_window(rows, Some("2024-01-01"), Some("2024-12-31"));
        assert_eq!(1, kept.len());
        assert_eq!(
            "2024-06-15",
            kept[0]["patent"]["publication_date"].as_str().expect("d")
        );
    }

    /// 日期写法不统一（`2024/1/5`、`20240105`）也要能比，否则过滤形同失效。
    #[test]
    fn date_window_normalizes_loose_formats() {
        let rows = vec![row_with("2024-01-05")];
        assert_eq!(
            1,
            filter_by_publication_window(rows.clone(), Some("2024/1/5"), Some("20240105")).len()
        );
        assert_eq!(
            0,
            filter_by_publication_window(rows.clone(), Some("2024-01-06"), None).len()
        );
        // 无法归一的边界（垃圾串）→ 不过滤，保留全部：过滤失效不能变成结果消失
        assert_eq!(
            1,
            filter_by_publication_window(rows, Some("not-a-date"), None).len()
        );
    }

    #[test]
    fn date_window_keeps_rows_without_any_date() {
        let rows = vec![
            serde_json::json!({ "patent": { "title": "无日期", "publication_number": "CN1" } }),
        ];
        assert_eq!(
            1,
            filter_by_publication_window(rows, Some("1999-01-01"), None).len()
        );
    }

    // ── 限速 / 退避 / 失败分类 ───────────────────────────────────────────────────

    /// ⚠️ 已知坑 1（实测）：503 + HTML「Sorry…」页必须判 **Quota**，
    /// 既不是 Network（只看状态码的结果）也不是 Parse（只试 JSON 解析的结果，而 Parse 不降级）。
    #[test]
    fn real_503_bot_page_classifies_as_quota() {
        assert_eq!(FailKind::Quota, FailKind::for_xhr_reply(503, REAL_503_BODY));
        assert_eq!(
            FailKind::Quota,
            FailKind::for_xhr_reply(429, "Too Many Requests")
        );
        assert_eq!(
            FailKind::Quota,
            FailKind::for_xhr_reply(503, "we detected unusual traffic from your network")
        );
        // 改判只作用于 429/503：500 仍走公共表 → Network；404 仍 → Parse
        assert_eq!(
            FailKind::Network,
            FailKind::for_xhr_reply(500, REAL_503_BODY)
        );
        assert_eq!(FailKind::Parse, FailKind::for_xhr_reply(404, "sorry"));
        // 5xx 里没有反爬文案时不得被误判成 Quota（否则真故障会被当成限流去退避）
        assert_eq!(FailKind::Network, FailKind::for_xhr_reply(503, ""));
    }

    /// spec §1 + §2：命中限流 → 退避 → 重试成功（**降级链用例 ②** 的单源视角）。
    #[tokio::test]
    async fn rate_limited_503_backs_off_then_succeeds() {
        let (p, transport, clock) = provider(vec![
            XhrReply {
                status: 503,
                body: REAL_503_BODY.to_string(),
                retry_after: None,
            },
            reply(200, REAL_CN_REPLY),
        ]);

        let outcome = p.search(query("动车组电池健康状态")).await;

        assert_eq!(
            2,
            transport.fetch_count(),
            "应发出 2 发：首发 503 + 退避后重试 1 发"
        );
        let report = &outcome.attempts[0];
        assert_eq!(
            AttemptStatus::Success,
            report.status,
            "重试成功后不得留下失败态"
        );
        assert!(report.produced_hits(), "hits={}", report.hits);
        assert_eq!(
            Some(14627),
            outcome.upstream_total,
            "total_num_results 应透传成 API 的 total"
        );
        // 退避确实发生，且时长恰是 BASE_BACKOFF（抖动 ≤700ms，不可能贡献一个整 2s 的 sleep）
        assert!(
            clock.recorded().contains(&BASE_BACKOFF),
            "未见退避: {:?}",
            clock.recorded()
        );
        assert_eq!(2, transport.urls().len(), "两发打的是同一个 URL");
    }

    /// spec §1：429 带 `Retry-After` 时以上游为准，不用本地指数值。
    #[tokio::test]
    async fn retry_after_header_overrides_local_backoff() {
        let (p, _transport, clock) = provider(vec![
            XhrReply {
                status: 429,
                body: "Too Many Requests".to_string(),
                retry_after: Some(Duration::from_secs(7)),
            },
            reply(200, REAL_EMPTY_REPLY),
        ]);
        p.search(query("固态电池")).await;
        assert!(
            clock
                .recorded()
                .iter()
                .any(|d| *d == Duration::from_secs(7)),
            "未尊重 Retry-After: {:?}",
            clock.recorded()
        );
    }

    /// 连续两发之间必须拉开最小间隔（防「连打 3 发即 503」的实测形态在本机重演）。
    #[tokio::test]
    async fn consecutive_searches_respect_min_interval() {
        let (p, transport, clock) = provider(vec![reply(200, REAL_EMPTY_REPLY)]);
        p.search(query("a")).await;
        p.search(query("b")).await;
        assert_eq!(2, transport.fetch_count());
        // 第二发必须等满 min gap：记到的 sleep 里必然有一个 ≥ 2s（其余只会是 ≤700ms 的抖动）
        assert!(
            clock.recorded().iter().any(|d| *d >= MIN_REQUEST_INTERVAL),
            "两次请求之间没有触发最小间隔等待: {:?}",
            clock.recorded()
        );
    }

    /// Parse（2xx 但正文非 JSON）不重试、不降级：只发一发。
    #[tokio::test]
    async fn parse_failure_stops_retrying() {
        let (p, transport, clock) = provider(vec![reply(200, "<html>not json at all")]);
        let outcome = p.search(query("固态电池")).await;
        assert_eq!(1, transport.fetch_count(), "Parse 不该触发重试");
        assert_eq!(
            AttemptStatus::Failed(FailKind::Parse),
            outcome.attempts[0].status
        );
        assert!(outcome.results.is_empty());
        // 除抖动外没有退避 sleep
        assert!(
            clock.recorded().iter().all(|d| *d < MIN_REQUEST_INTERVAL),
            "{:?}",
            clock.recorded()
        );
    }

    /// 网络故障重试一次后如实报告 Network，且仍只发两发（spec §1「单次重试」）。
    #[tokio::test]
    async fn network_fault_retries_once_then_reports_network() {
        #[derive(Default)]
        struct AlwaysDown;
        impl XhrTransport for AlwaysDown {
            fn fetch(
                &self,
                _url: String,
            ) -> Pin<Box<dyn Future<Output = Result<XhrReply, XhrFault>> + Send + '_>> {
                Box::pin(async { Err(XhrFault::Network("connection timed out".to_string())) })
            }
        }
        let clock = FakeClock::new();
        let p = GooglePatentsXhrProvider::with_transport(db(), Arc::new(AlwaysDown), clock.clone());
        let outcome = p.search(query("固态电池")).await;
        assert_eq!(
            AttemptStatus::Failed(FailKind::Network),
            outcome.attempts[0].status
        );
        let err = outcome.attempts[0].error.clone().expect("error note");
        assert!(err.contains(SOURCE_LABEL), "错误文案要带源名: {err}");
        assert!(err.contains("网络故障"), "错误文案要带分类: {err}");
        assert!(outcome.results.is_empty());
        assert!(
            clock.recorded().contains(&BASE_BACKOFF),
            "网络故障同样要退避: {:?}",
            clock.recorded()
        );
    }

    #[tokio::test]
    async fn limit_truncates_without_inventing_upstream_param() {
        let mut q = query("动车组电池健康状态");
        q.limit = 1;
        let (p, transport, _clock) = provider(vec![reply(200, REAL_CN_REPLY)]);
        let outcome = p.search(q).await;
        assert_eq!(
            1,
            outcome.results.len(),
            "limit=1 应截断到 1 条（上游 2 条）"
        );
        // 截断只发生在客户端：URL 里不得出现伪造的条数参数
        let url = transport
            .urls()
            .into_iter()
            .next()
            .expect("one request issued");
        let decoded = urlencoding::decode(&url)
            .ok()
            .map(|s| s.to_string())
            .unwrap_or(url);
        assert!(!decoded.contains("num="), "不得凭空发明条数参数: {decoded}");
    }

    #[tokio::test]
    async fn page_gt_one_returns_first_page_with_honest_hint() {
        let mut q = query("动车组电池健康状态");
        q.page = 3;
        let (p, _transport, _clock) = provider(vec![reply(200, REAL_CN_REPLY)]);
        let outcome = p.search(q).await;
        let hint = outcome.attempts[0].hint.clone().expect("pagination hint");
        assert!(hint.contains("翻页"), "{hint}");
        assert!(
            !outcome.results.is_empty(),
            "提示不等于把结果清空——数据完整性优先（AGENTS.md 2.5）"
        );
    }

    #[tokio::test]
    async fn kind_and_lookup_exact_defaults() {
        let (p, _transport, _clock) = provider(vec![]);
        assert_eq!(SourceKind::GooglePatentsXhr, p.kind());
        assert_eq!("google_patents_xhr", SourceKind::GooglePatentsXhr.as_str());
        // 本源没有 details 端点 → 用 trait 默认实现，返回 None（见 provider.rs 注释）
        assert!(p.lookup_exact("CN1".to_string()).await.is_none());
    }
}
