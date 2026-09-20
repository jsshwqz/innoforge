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
use crate::search::query::render_q;
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
                    from.as_ref().map_or(true, |f| day >= *f)
                        && to.as_ref().map_or(true, |t| day <= *t)
                }
            }
        })
        .collect()
}

/// `2024-01-05` / `2024/1/5` / `20240105` → `"20240105"`；不足 8 位数字则 `None`。
fn normalize_date(s: &str) -> Option<String> {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 8 {
        Some(digits[..8].to_string())
    } else {
        None
    }
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
