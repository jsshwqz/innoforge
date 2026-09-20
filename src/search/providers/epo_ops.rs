//! EPO OPS 源 / European Patent Office — Open Patent Services（spec §4，MA2b）
//!
//! MA2 执行链里的**第三个**在线源：SerpAPI 与 Google Patents XHR 都没有可用结果时，
//! 由本源顶上（`routes/search.rs` 的登记顺序 `[SerpAPI(有 Key) → GooglePatentsXhr → EpoOps(有 Key)]`）。
//! 定位与 spec §4 一致：**英文 / EU 域补充源，对中文检索贡献有限**（依据见下方 CQL 索引目录一节）。
//!
//! ## 端点与鉴权（官方 Reference Guide v1.3.20 §Authentication + §CQL search，逐条实证）
//!
//! 1. **取 token**：`POST https://ops.epo.org/3.2/auth/accesstoken`
//!    - `Authorization: Basic base64(消费者 Key:消费者 Secret)`
//!    - `Content-Type: application/x-www-form-urlencoded`，正文 `grant_type=client_credentials`
//!    - 响应（官方样例）：`{"expires_in":"1199","token_type":"Bearer","access_token":"4AWo...","status":"approved", …}`
//!      —— 注意 **`expires_in` 是字符串**，取值时必须同时容忍数字形态。
//!    - 官方原文：*The access token produced is valid for approximately 20mins.
//!      A New access token should be requested as soon as an invalid access token message is received from OPS.*
//!      → 落地为本模块的三件事：**进程内缓存 token** + **到期前 [`TOKEN_REFRESH_MARGIN`] 提前刷新**
//!      + **数据请求撞 401/403 时作废缓存、重取一次 token 并重试**（见 [`EpoOpsProvider::fetch_rows`]）。
//! 2. **检索**：`GET /3.2/rest-services/published-data/search/biblio?q=<CQL>&Range=<b>-<e>`
//!    带 `Authorization: Bearer <token>`、`Accept: application/json`。
//!
//! ⚠️ **与 spec §4 的一处出入（按官方文档更正）**：spec 写的是 `/published-data/search`，
//! 且给出的 CQL 索引里有 `cn=CN`。两处都与官方 Reference Guide 冲突，实测取官方口径：
//! - `/published-data/search`（无 constituent）的响应**只含公开号引用**
//!   （`ops:publication-reference` → country / doc-number / kind，无标题无摘要），
//!   拿不到 [`crate::types::search::PatentSummary`] 必需的 title ——
//!   `relevance::rank_and_gate_hits` 的第 2 步就是「空标题丢弃」（旧行为），
//!   用它做检索源会**恒定返回 0 条**。故本源用同一服务的
//!   `/published-data/search/{constituent}`，constituent 取 `biblio`
//!   （取值域 `biblio | full-cycle | abstract`，见仓外证据 openapi 定义；官方响应示例见下节）。
//! - CQL 索引目录里 **`cn` = "IPC8 core additional class"（分类号）**，不是国家；
//!   官方示例的国家限定是**裸国家码**（`pa all "…" and JP`）或 `pn=`/`pr=` 前缀。
//!   故 [`render_cql`] 用 `and <国家码>`，绝不生成 `cn=CN`。
//! - 官方目录里亦无 `ps=`（spec §4 列出的第四个别名）；`ti/ab/ta/pa/in/pn/pd` 均在目录内，本源只用这些。
//!
//! ## CQL 索引（官方 Appendix 4.2「CQL index catalogue」逐条照抄）
//!
//! | 索引 | 含义 | 官方限定 |
//! |------|------|----------|
//! | `ti` | title | **in English** |
//! | `ab` | abstract | **in English** |
//! | `ta` | title or abstract | in English |
//! | `pa` / `in` / `ia` | applicant / inventor / 二者 | 姓名 |
//! | `pn` / `ap` / `pr` / `num` | 公开号 / 申请号 / 优先权号 / 三者 | 号 |
//! | `pd` | 公开日 | `pd="20051212 20051214"`（引号内两值 = 区间） |
//!
//! `ti`/`ab` 只覆盖**英文著录数据**，这就是「EPO 排第三、对中文贡献有限」的文档级依据。
//! 中文关键词打到 `ti=` 上大概率零命中，这是本源的真实能力边界，**不在代码里伪装**：
//! 零命中即如实以 `Success + hits=0` 记账，让链路继续按优先级规则走（见 `chain.rs`）。
//!
//! ## 响应结构与建模（⚠️ 实证材料汇总）
//!
//! 官方 Reference Guide v1.3.20 §CQL search「Response example (with biblio)」（XML）：
//!
//! ```xml
//! <ops:world-patent-data>
//!  <ops:biblio-search total-result-count="10000">
//!   <ops:query syntax="CQL">applicant=IBM</ops:query>
//!   <ops:range begin="1" end="25"/>
//!   <ops:search-result>
//!    <exchange-documents>
//!     <exchange-document system="ops.epo.org" family-id="37717388"
//!                        country="KR" doc-number="20100130646" kind="A">
//!      <bibliographic-data>
//!       <publication-reference><document-id document-id-type="docdb">
//!         <country>KR</country><doc-number>20100130646</doc-number>
//!         <kind>A</kind><date>20101213</date></document-id> …</publication-reference>
//!       <application-reference …><document-id …><date>20060720</date>…</document-id></application-reference>
//!       <parties><applicants><applicant sequence="1" data-format="epodoc">
//!         <applicant-name><name>IBM [US]</name></applicant-name></applicant>…</applicants></parties>
//!       <invention-title lang="en">INJECTION MOLDED MICROLENSES …</invention-title>
//!      </bibliographic-data>
//!     </exchange-document>
//!    </exchange-documents>
//!   </ops:search-result>
//!  </ops:biblio-search>
//! </ops:world-patent-data>
//! ```
//!
//! 请求 `Accept: application/json` 时 OPS 的 JSON 渲染规则（**由真实抓取件钉死**，
//! 出处见本模块 `tests` 文件头的 EVIDENCE 注释块）：属性 → `"@名称"`、元素文本 → `{"$": "文本"}`、
//! 命名空间前缀保留（`ops:biblio-search`），且**单值/数组多态**（同一键可能是对象也可能是数组，
//! 故 [`as_array`] 必须两种都吃）。官方样例里 `<total-result-count>` 上限 10000、
//! `Range` 跨度上限 100、可翻检总量上限 2000（见 [`MAX_RETRIEVABLE`]）。
//!
//! ## 已知坑与处置
//! 1. **无 Key 即不登记**：`routes/search.rs` 只在配齐 `EPO_KEY`+`EPO_SECRET` 时把本源装进链路；
//!    若被登记但凭证为空（例如未来别的调用方直接构造），本源**不发请求**，
//!    以 [`AttemptStatus::Skipped`] 记账（与 SerpAPI 缺 Key 的现役语义一致，见 `chain.rs::skipped_primary_does_not_block_fallback`）。
//! 2. **限流**：OPS 用 `X-Throttling-Control` 头与 403/429 表达限流，`Retry-After` 存在则尊重（spec §1）。
//! 3. **匿名请求**（无 Authorization 头）实测返回 **403** 而非 401（见 EVIDENCE_LIVE 第 2 条），
//!    所以「token 过期」与「压根没带 token」都可能落到 403：两者的正确处置都是
//!    「重取 token 再试一发」，故 auth 重试判定为 **401 或 403**（口径见 [`FailKind::for_http_status`]）。
//! 4. **未做**：跨源结果拼接（`MergedPatent.key` 的合并消费）属 MA4；熔断冷却
//!    （`FailKind::cools_down` 消费）属 MA5/MA6。

use crate::db::Database;
use crate::patent::Patent;
use crate::search::merge::merged_from;
use crate::search::model::{
    report_excerpt, AttemptReport, AttemptStatus, FailKind, Lang, SearchOutcome, SearchQuery,
    SourceKind,
};
use crate::search::provider::SearchProvider;
use crate::search::query::normalize_date;
use crate::search::relevance::rank_and_gate_hits;
use crate::types::search::{PatentSummary, SearchType};
use base64::Engine;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// OAuth2 取 token 端点（spec §4）。
const AUTH_ENDPOINT: &str = "https://ops.epo.org/3.2/auth/accesstoken";

/// 检索端点：`published-data/search/biblio`（含著录数据，理由见文件头「与 spec §4 的一处出入」）。
const SEARCH_ENDPOINT: &str = "https://ops.epo.org/3.2/rest-services/published-data/search/biblio";

/// 单次上游请求超时（spec §1 的 15s；与 MA2a 的新源口径一致）。
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(15);

/// 本源的固定标识，进 `AttemptReport`、日志与 `Patent.source`。
const SOURCE_LABEL: &str = "epo_ops";

/// token 提前刷新余量：剩余有效期不足 60s 即重取（官方 `expires_in` ≈1199s）。
pub const TOKEN_REFRESH_MARGIN: Duration = Duration::from_secs(60);

/// `expires_in` 缺失/非法时采用的兜底 TTL（官方样例值 1199s，保守取 15 分钟）。
pub const DEFAULT_TOKEN_TTL: Duration = Duration::from_secs(900);

/// 单发 `Range` 跨度上限（spec §4 的 `Range=1-25`；官方文档另限「跨度 ≤100」）。
pub const DEFAULT_RANGE_SPAN: usize = 25;

/// 官方文档：结果总数 >2000 时无法继续翻检（`total-result-count` 亦封顶 10000）。
pub const MAX_RETRIEVABLE: usize = 2000;

/// 上游总尝试次数（spec §1「独立超时 + 单次重试」→ 首发 + 1 次重试 = 2）。
pub const MAX_ATTEMPTS: usize = 2;

/// 被限流/网络故障后的退避基数（与 MA2a 的 XHR 同源取值，便于面板统一解释）。
pub const BASE_BACKOFF: Duration = Duration::from_secs(2);

// ─────────────────────────────────────────────────────────────────────────────
// 传输层（**降级链与鉴权用例的注入点**，照抄 providers/google_patents_xhr.rs 的模式）
// ─────────────────────────────────────────────────────────────────────────────

/// 一次 HTTP 往返的方法与头部。刻意把 `authorization` 作为**显式字段**带出，
/// 这样测试才能断言「第二次请求用的是新 token」「token 命中缓存时压根没有 POST」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpoMethod {
    Post,
    Get,
}

/// 一次待发出的请求。
#[derive(Debug, Clone)]
pub struct EpoRequest {
    pub method: EpoMethod,
    pub url: String,
    /// 完整的 `Authorization` 头值（`Basic …` / `Bearer …`）。
    pub authorization: String,
    /// form 正文（仅取 token 时用：`grant_type=client_credentials`）。
    pub body: Option<String>,
}

/// 一次 HTTP 往返的结果（`Retry-After` 单独带出，spec §1 要求 429 尊重它）。
#[derive(Debug, Clone)]
pub struct EpoReply {
    pub status: u16,
    pub body: String,
    pub retry_after: Option<Duration>,
}

/// 传输层故障（只有「没能拿到响应」才需要与 [`EpoReply`] 区分，供 `FailKind::Network` 归因）。
#[derive(Debug, Clone)]
pub enum EpoFault {
    /// 超时 / 连接失败 / DNS
    Network(String),
}

/// HTTP 传输抽象 —— 离线注入点。
pub trait EpoTransport: Send + Sync {
    fn send(
        &self,
        request: EpoRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EpoReply, EpoFault>> + Send + '_>>;
}

/// 时间抽象 —— token 有效期与退避的**假时钟注入点**，让「提前刷新」可在毫秒级断言。
pub trait EpoClock: Send + Sync {
    /// 单调毫秒（只用于算有效期与耗时，不做绝对时间）。
    fn now_ms(&self) -> u64;
    fn sleep(&self, dur: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// 生产传输：reqwest + 15s 超时。
pub struct ReqwestEpoTransport {
    client: reqwest::Client,
}

impl ReqwestEpoTransport {
    pub fn new() -> Self {
        ReqwestEpoTransport {
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

impl Default for ReqwestEpoTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl EpoTransport for ReqwestEpoTransport {
    fn send(
        &self,
        request: EpoRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EpoReply, EpoFault>> + Send + '_>> {
        Box::pin(async move {
            let EpoRequest {
                method,
                url,
                authorization,
                body,
            } = request;
            let builder = match method {
                EpoMethod::Post => {
                    let mut req = self
                        .client
                        .post(&url)
                        .header("Content-Type", "application/x-www-form-urlencoded");
                    if let Some(b) = body {
                        req = req.body(b);
                    }
                    req
                }
                // 检索请求显式要 JSON（真实抓取件证明 JSON 渲染规则，见文件头）
                EpoMethod::Get => self.client.get(&url).header("Accept", "application/json"),
            }
            .header("Authorization", authorization);

            match builder.send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let retry_after = Self::retry_after(resp.headers());
                    match resp.text().await {
                        Ok(body) => Ok(EpoReply {
                            status,
                            body,
                            retry_after,
                        }),
                        Err(e) => Err(EpoFault::Network(format!("读取响应体失败: {e}"))),
                    }
                }
                Err(e) => Err(EpoFault::Network(format!("请求未发出/未完成: {e}"))),
            }
        })
    }
}

/// 生产时钟：`Instant` 单调毫秒 + `tokio::time::sleep`。
pub struct SystemEpoClock;

static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

impl EpoClock for SystemEpoClock {
    fn now_ms(&self) -> u64 {
        // 以进程启动点为零点的单调毫秒；只用于差值，故基准无关紧要。
        START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_millis() as u64
    }

    fn sleep(&self, dur: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move { tokio::time::sleep(dur).await })
    }
}

/// 缓存的 access token 及其到期时刻（单调毫秒）。
#[derive(Debug, Clone)]
struct CachedToken {
    token: String,
    expires_at_ms: u64,
}

/// EPO OPS 源。
pub struct EpoOpsProvider {
    consumer_key: String,
    consumer_secret: String,
    db: Arc<Database>,
    transport: Arc<dyn EpoTransport>,
    clock: Arc<dyn EpoClock>,
    /// token 缓存。**不跨 await 持有锁**（见 [`Self::cached_token`] / [`Self::store_token`]），
    /// 因此用 `std::sync::Mutex` 即可，无需 async 锁。
    token: Mutex<Option<CachedToken>>,
}

impl EpoOpsProvider {
    /// 生产构造：真实 reqwest 传输 + 系统时钟。
    pub fn new(consumer_key: String, consumer_secret: String, db: Arc<Database>) -> Self {
        EpoOpsProvider {
            consumer_key,
            consumer_secret,
            db,
            transport: Arc::new(ReqwestEpoTransport::new()),
            clock: Arc::new(SystemEpoClock),
            token: Mutex::new(None),
        }
    }

    /// 测试构造：注入假传输 / 假时钟（离线，绝不出网）。
    pub fn with_transport(
        consumer_key: String,
        consumer_secret: String,
        db: Arc<Database>,
        transport: Arc<dyn EpoTransport>,
        clock: Arc<dyn EpoClock>,
    ) -> Self {
        EpoOpsProvider {
            consumer_key,
            consumer_secret,
            db,
            transport,
            clock,
            token: Mutex::new(None),
        }
    }

    /// 凭证是否齐备 —— `routes/search.rs` 据此决定**要不要把本源登记进链路**。
    pub fn has_credentials(&self) -> bool {
        !self.consumer_key.trim().is_empty() && !self.consumer_secret.trim().is_empty()
    }

    /// `Authorization: Basic base64(key:secret)`（spec §4）。
    fn basic_authorization(&self) -> String {
        let raw = format!(
            "{}:{}",
            self.consumer_key.trim(),
            self.consumer_secret.trim()
        );
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(raw.as_bytes())
        )
    }

    /// 命中且未到提前刷新点的 token。
    fn cached_token(&self) -> Option<String> {
        let now = self.clock.now_ms();
        let margin_ms = TOKEN_REFRESH_MARGIN.as_millis() as u64;
        let guard = self.token.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .as_ref()
            .filter(|c| c.expires_at_ms > now.saturating_add(margin_ms))
            .map(|c| c.token.clone())
    }

    fn store_token(&self, token: &str, ttl: Duration) {
        let ttl_ms = ttl.as_millis() as u64;
        let expires_at_ms = self.clock.now_ms().saturating_add(ttl_ms);
        let mut guard = self.token.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(CachedToken {
            token: token.to_string(),
            expires_at_ms,
        });
    }

    /// 作废缓存（撞 401/403 时用）。
    fn invalidate_token(&self) {
        let mut guard = self.token.lock().unwrap_or_else(|e| e.into_inner());
        *guard = None;
    }

    /// 取一个可用 token：先查缓存（**命中即不发请求**，spec §4「token 需缓存」），
    /// 未命中或已接近到期才去 POST（spec §4「提前刷新」）。
    async fn access_token(&self) -> Result<String, (FailKind, String)> {
        if let Some(token) = self.cached_token() {
            return Ok(token);
        }
        let request = EpoRequest {
            method: EpoMethod::Post,
            url: AUTH_ENDPOINT.to_string(),
            authorization: self.basic_authorization(),
            body: Some("grant_type=client_credentials".to_string()),
        };
        let reply = self
            .transport
            .send(request)
            .await
            .map_err(|EpoFault::Network(msg)| {
                (
                    FailKind::Network,
                    format!("{SOURCE_LABEL} 取 token 网络故障：{msg}"),
                )
            })?;

        if reply.status >= 400 {
            let kind = FailKind::for_http_status(reply.status);
            return Err((
                kind,
                format!(
                    "{SOURCE_LABEL} 取 token 被拒 HTTP {}（{}）：{}",
                    reply.status,
                    kind_label(kind),
                    report_excerpt(&reply.body)
                ),
            ));
        }
        let json: serde_json::Value = serde_json::from_str(&reply.body).map_err(|e| {
            (
                FailKind::Parse,
                format!(
                    "{SOURCE_LABEL} token 响应不是合法 JSON（上游结构可能已变化），\
                     原因：{e}，片段：{}",
                    report_excerpt(&reply.body)
                ),
            )
        })?;
        let token = json["access_token"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                (
                    FailKind::Parse,
                    format!(
                        "{SOURCE_LABEL} token 响应缺少 access_token 字段，片段：{}",
                        report_excerpt(&reply.body)
                    ),
                )
            })?;
        // 官方样例里 "expires_in":"1199" 是字符串；数字形态一并容忍。
        let ttl = json["expires_in"]
            .as_str()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .or_else(|| json["expires_in"].as_u64())
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_TOKEN_TTL);
        // 提前刷新只在**读取处**（[`Self::cached_token`]）判一次余量，避免「存的时候减一次、
        // 读的时候再减一次」的双重保守把 1199s 的 token 砍成 1079s；0 值/畸形值兜底成 1s。
        let ttl = ttl.max(Duration::from_secs(1));
        self.store_token(token, ttl);
        Ok(token.to_string())
    }

    /// 一发检索请求 + 失败分类（与 XHR 源的 `attempt_fetch` 同构）。
    async fn attempt_fetch(
        &self,
        url: &str,
        token: &str,
    ) -> Result<(Vec<serde_json::Value>, Option<usize>), (FailKind, String, Option<Duration>)> {
        let request = EpoRequest {
            method: EpoMethod::Get,
            url: url.to_string(),
            authorization: format!("Bearer {token}"),
            body: None,
        };
        match self.transport.send(request).await {
            Ok(reply) => {
                if reply.status >= 400 {
                    let kind = FailKind::for_http_status(reply.status);
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
                match serde_json::from_str::<serde_json::Value>(&reply.body) {
                    Ok(json) => Ok((search_rows(&json), total_result_count(&json))),
                    Err(e) => Err((
                        FailKind::Parse,
                        format!(
                            "{SOURCE_LABEL} 响应不是合法 JSON（上游结构可能已变化），\
                             原因：{e}，片段：{}",
                            report_excerpt(&reply.body)
                        ),
                        None,
                    )),
                }
            }
            Err(EpoFault::Network(msg)) => Err((
                FailKind::Network,
                format!("{SOURCE_LABEL} 网络故障：{msg}"),
                None,
            )),
        }
    }

    /// 带 token 刷新与退避的取结果循环。
    ///
    /// 三条规则分得很清（各自对应 spec §1/§4 的一句话）：
    /// 1. **Auth（401/403）且本次调用尚未重取过 token** → 作废缓存 → 重新取 token → 再发一发；
    ///    这条**不占用**退避重试预算（它修的是凭证，不是抖动）。
    /// 2. **Parse** → 不重试（spec §1：结构变化要暴露成 bug，重试只会掩盖）。
    /// 3. 其余（Network/Quota）→ 尊重 `Retry-After`，否则指数退避，重试 [`MAX_ATTEMPTS`] 次封顶。
    async fn fetch_rows(
        &self,
        url: &str,
    ) -> Result<(Vec<serde_json::Value>, Option<usize>), (FailKind, String)> {
        let mut transport_attempts = 0usize;
        let mut auth_retried = false;
        loop {
            let token = self.access_token().await?;
            match self.attempt_fetch(url, &token).await {
                Ok(rows) => return Ok(rows),
                Err((kind, msg, retry_after)) => {
                    if kind == FailKind::Auth && !auth_retried {
                        println!(
                            "[ONLINE] {SOURCE_LABEL} token 被判无效（HTTP 401/403）\
                             → 作废缓存并重取一次后重试"
                        );
                        auth_retried = true;
                        self.invalidate_token();
                        continue;
                    }
                    transport_attempts += 1;
                    if kind == FailKind::Auth || !kind.switches_source() {
                        // 第二次 Auth 说明**凭证本身**有问题（token 已经是刚取的新 token），
                        // 再退避重发只会白耗一次配额；Parse 同理（spec §1：不重试）。
                        return Err((kind, msg));
                    }
                    if transport_attempts >= MAX_ATTEMPTS {
                        return Err((kind, msg));
                    }
                    let backoff = retry_after.unwrap_or_else(|| backoff_for(transport_attempts));
                    println!(
                        "[ONLINE] {SOURCE_LABEL} {kind:?} → 退避 {backoff:?} 后重试\
                         （第 {transport_attempts} 次失败）"
                    );
                    self.clock.sleep(backoff).await;
                }
            }
        }
    }

    /// 命中行 → `PatentSummary[]`（打分/放行/中文过滤/去重与两兄弟源共用
    /// [`rank_and_gate_hits`]，避免第二份实现）。
    fn outcome_patents(
        &self,
        query: &SearchQuery,
        rows: Vec<serde_json::Value>,
    ) -> Vec<PatentSummary> {
        // 内容打分的关键词用**用户原词**：既不能塞 CQL 串（`ti=`/`ab=` 是索引名），
        // 也不能塞 Google 语法的 render_q 产物（`assignee:"…"` 同理）——
        // 三源的 q 语法各不相同，只有用户原词是同一把尺子，混进语法串只会污染相关性分。
        let keyword = query.keyword.trim();
        let mut patents = rank_and_gate_hits(
            self.db.as_ref(),
            SOURCE_LABEL,
            keyword,
            epo_wants_chinese_gate(query),
            &rows,
            epo_to_patent,
        );
        if query.limit > 0 && patents.len() > query.limit {
            patents.truncate(query.limit);
        }
        patents
    }
}

/// 中文语境是否按「要中文给中文」闸门过滤（与另两源同语义）。
///
/// 单列成函数而不是内联 `matches!`，是为了让**这条口径可被单测锁死**：
/// EPO 的 `ti/ab` 只有英文著录数据，若中文语境下仍坚持 CJK 闸门，
/// 本源在 CN 查询上必然零命中 —— 这是预期行为（spec §4 的定位），
/// 但必须显式记录，不能被读者误读成 bug。
fn epo_wants_chinese_gate(query: &SearchQuery) -> bool {
    matches!(query.language, Some(Lang::Chinese))
}

/// [`FailKind`] 的中文短标签，只进 `AttemptReport::error` 文案，不进任何对外字段。
/// （与 XHR 源的同名私有函数各自保留：两源的文案口径允许独立演化，且不进对外字段。）
fn kind_label(kind: FailKind) -> &'static str {
    match kind {
        FailKind::Network => "网络故障",
        FailKind::Quota => "限流/配额",
        FailKind::Auth => "鉴权",
        FailKind::Parse => "响应结构变化",
    }
}

/// 第 `attempt` 次失败后的退避时长（指数）。
fn backoff_for(attempt: usize) -> Duration {
    let factor = 1u32 << (attempt.saturating_sub(1).min(4)) as u32;
    BASE_BACKOFF.saturating_mul(factor)
}

// ─────────────────────────────────────────────────────────────────────────────
// CQL 渲染（spec §4 语法 + 官方索引目录）
// ─────────────────────────────────────────────────────────────────────────────

/// CQL 里会改变查询语义的字符：引号、括号、等号、布尔算子依赖的分隔符。
/// 用户输入进 `ti="…"` 前必须清掉，否则一次输入就能改写查询结构
/// （AGENTS.md 2.6 的边界隔离精神同样适用于上游查询语言）。
fn sanitize_cql_term(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .filter(|c| !matches!(c, '"' | '\'' | '(' | ')' | '=' | '/' | ':' | ';'))
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 由 [`SearchQuery`] 渲染 OPS 的 CQL `q` 串。
///
/// 参数映射（官方索引目录见文件头）：
/// - `SearchType::Applicant` → `pa="<姓名>"`；`Inventor` → `in="<姓名>"`
/// - `PatentNumber` → `pn=<号>`（**不加引号**，官方示例 `pn=EP1000000` 形态；
///   且此时不再叠加国家/日期 —— 号码本身已唯一）
/// - 其他（关键词/混合）→ `(ti="<词>" or ab="<词>")`（引号 = 官方「exact words/phrase」语义）
/// - `country` → `and <CC>`（裸国家码，官方示例 `pa all "…" and JP`；**不是** `cn=`）
/// - `date_from`/`date_to` → `pd="YYYYMMDD"` / `pd="YYYYMMDD YYYYMMDD"`（官方区间写法）
///   —— 这一点与 XHR 源相反：OPS 的服务端日期过滤是官方文档明列能力，故下发；
///   日期串归一失败（[`normalize_date`] 返回 `None`）时**整条日期条件省略**，
///   即「过滤失效绝不变成结果消失」。
///
/// `lang` 参数刻意不存在：官方 `ti/ab` 只有英文索引，OPS 的 CQL 目录里没有
/// 「按界面语种检索」的索引位，编造一个 `lang=` 只会换来一个未验证的上游行为。
pub(crate) fn render_cql(query: &SearchQuery) -> String {
    let term = sanitize_cql_term(query.keyword.trim());
    if term.is_empty() {
        return String::new();
    }
    let is_number_lookup = matches!(query.search_type, Some(SearchType::PatentNumber));

    let base = if is_number_lookup {
        format!("pn={}", term.replace(' ', ""))
    } else {
        match query.search_type {
            Some(SearchType::Applicant) => format!("pa=\"{term}\""),
            Some(SearchType::Inventor) => format!("in=\"{term}\""),
            _ => format!("(ti=\"{term}\" or ab=\"{term}\")"),
        }
    };
    if is_number_lookup {
        return base;
    }

    let mut clauses = vec![base];
    if let Some(c) = query.country.as_deref() {
        let cc = c.trim().to_ascii_uppercase();
        if cc.len() == 2 && cc.chars().all(|ch| ch.is_ascii_alphabetic()) {
            clauses.push(cc);
        }
    }
    let from = query.date_from.as_deref().and_then(normalize_date);
    let to = query.date_to.as_deref().and_then(normalize_date);
    let pd = match (from, to) {
        (Some(f), Some(t)) => Some(format!("pd=\"{f} {t}\"")),
        (Some(f), None) => Some(format!("pd=\"{f}\"")),
        (None, Some(t)) => Some(format!("pd=\"{t}\"")),
        (None, None) => None,
    };
    if let Some(pd) = pd {
        clauses.push(pd);
    }
    clauses.join(" and ")
}

/// 本页的 `Range` 值（`begin-end`，1 起始闭区间）。
///
/// `limit` 决定跨度（封顶 [`DEFAULT_RANGE_SPAN`]，因为 spec §4 就是 `Range=1-25`，
/// 而官方另限「跨度 ≤100」）；`page` 决定偏移，begin 封顶 [`MAX_RETRIEVABLE`]。
pub(crate) fn range_param(query: &SearchQuery) -> String {
    let span = if query.limit == 0 {
        DEFAULT_RANGE_SPAN
    } else {
        // clamp 只在 max<min 时 panic，而这里是 `1..=DEFAULT_RANGE_SPAN(25)` 的常量区间，
        // 结构上不可能反序；limit 为 0 的分支在上面已单独处理。
        query.limit.clamp(1, DEFAULT_RANGE_SPAN)
    };
    let page = query.page.max(1);
    let begin = ((page - 1) * span + 1).min(MAX_RETRIEVABLE.max(1));
    let end = (begin + span - 1).min(MAX_RETRIEVABLE);
    let end = if end < begin { begin } else { end };
    format!("{begin}-{end}")
}

/// 完整检索 URL：`?q=<urlencoded CQL>&Range=<b>-<e>`。
pub(crate) fn request_url(query: &SearchQuery) -> String {
    let cql = render_cql(query);
    format!(
        "{SEARCH_ENDPOINT}?q={}&Range={}",
        urlencoding::encode(&cql),
        range_param(query)
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// OPS JSON → 域内结构
// ─────────────────────────────────────────────────────────────────────────────

/// 同一键在 OPS JSON 里可能是对象、也可能是数组（XML 多态），统一成数组再看。
fn as_array(v: &serde_json::Value) -> Vec<&serde_json::Value> {
    match v {
        serde_json::Value::Array(items) => items.iter().collect(),
        serde_json::Value::Null => Vec::new(),
        other => vec![other],
    }
}

/// 元素文本：`{"$": "文本"}` 或直接字符串；都不是则空串（**绝不 panic**）。
fn json_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.trim().to_string(),
        serde_json::Value::Object(obj) => obj
            .get("$")
            .and_then(|x| x.as_str())
            .map(|s| s.trim().to_string())
            .or_else(|| {
                // 兜底：真实件里有 `{"it":"文件名"}` 这种「单个非属性键即文本」的形态
                // （见 `tests` 的 L5 抓取件）。
                // ⚠️ 必须**排除 `@属性` 键**：`{"@lang":"en"}` 是「只有属性的空文本节点」，
                // 若把属性值当文本，就会凭空造出一个标题（`titleless_rows_...` 与
                // `malformed_documents_...` 两条用例一起锁死这一点）。
                let mut only_text: Option<String> = None;
                for (key, value) in obj {
                    if key.starts_with('@') {
                        continue;
                    }
                    match value.as_str() {
                        Some(s) if only_text.is_none() => only_text = Some(s.trim().to_string()),
                        // 第二个文本键，或值不是字符串 → 语义不唯一，宁可不猜
                        _ => return None,
                    }
                }
                only_text
            })
            .unwrap_or_default(),
        serde_json::Value::Array(items) => items
            .iter()
            .map(json_text)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// 属性值：`"@名称"`。官方样例里属性全是字符串，数字形态一并转成字符串。
fn json_attr(v: &serde_json::Value, name: &str) -> String {
    let key = format!("@{name}");
    match &v[key.as_str()] {
        serde_json::Value::String(s) => s.trim().to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        other => json_text(other),
    }
}

/// 键名匹配：OPS 的 JSON 渲染**保留 XML 命名空间前缀**（`ops:biblio-search`、
/// `ipcx:classification`、`dwk:abstract`…），故查 `biblio-search` 也要能吃下
/// `ops:biblio-search`。注意 `@属性` 永远不匹配同名元素键。
fn key_is(k: &str, name: &str) -> bool {
    k == name || k.ends_with(&format!(":{name}"))
}

/// 深度收集某个键的所有取值（**同层按键名排序遍历**，见 [`find_key`] 的注释）。
///
/// 为什么要深度找而不是照抄固定路径：OPS 的 XML→JSON 渲染会在
/// 「单值 / 多值」之间切换对象与数组形态，个别 constituent 还会多包一层容器
/// （`exchange-documents`、`ops:search-result`）。深度找键对这三类差异都免疫。
///
/// ⚠️ **数组序可靠、对象键序不可靠**：本项目未启用 serde_json 的 `preserve_order`，
/// `Map` 即 `BTreeMap`，同一对象内的键按**字典序**产出。所以深度找到的多条同键结果
/// 不代表 XML 文档序（`application-reference` 会排在 `publication-reference` 之前）。
/// 因此凡「同层多个容器都含同一子键」的取值，调用方必须先用 [`find_first`] 把
/// 范围收窄到正确的容器（见 [`epo_to_patent`] 里的 publication/application 分段），
/// 绝不能直接吃跨容器的第一条。
fn find_key<'a>(v: &'a serde_json::Value, key: &str, out: &mut Vec<&'a serde_json::Value>) {
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map {
                if key_is(k, key) {
                    out.extend(as_array(val));
                }
                find_key(val, key, out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                find_key(item, key, out);
            }
        }
        _ => {}
    }
}

fn find_first<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    let mut hits = Vec::new();
    find_key(v, key, &mut hits);
    hits.into_iter().next()
}

fn find_all<'a>(v: &'a serde_json::Value, key: &str) -> Vec<&'a serde_json::Value> {
    let mut hits = Vec::new();
    find_key(v, key, &mut hits);
    hits
}

/// 同一语义在 OPS 里有多种元素名时（经典 schema vs DOCX），按候选顺序依次深找。
/// 返回**第一个有命中的候选**的全部结果，不把不同候选的结果混在一起。
fn find_any<'a>(v: &'a serde_json::Value, names: &[&str]) -> Vec<&'a serde_json::Value> {
    names
        .iter()
        .map(|name| find_all(v, name))
        .find(|hits| !hits.is_empty())
        .unwrap_or_default()
}

/// `total-result-count`（官方封顶 10000）→ 对外 `total`。
fn total_result_count(json: &serde_json::Value) -> Option<usize> {
    find_all(json, "biblio-search")
        .into_iter()
        .find_map(|node| {
            let raw = json_attr(node, "total-result-count");
            raw.parse::<usize>().ok()
        })
}

/// 摊平出 `exchange-document` 列表（`search/biblio` 的每条命中就是一个 exchange-document）。
///
/// 零命中时官方响应里没有 `exchange-document` 键（`ops:search-result` 为空对象或缺席），
/// 这里返回空数组而不是报错 —— 「没有结果」与「结构变了」是两回事（spec §1 的 Parse 判定）。
fn search_rows(json: &serde_json::Value) -> Vec<serde_json::Value> {
    find_all(json, "exchange-document")
        .into_iter()
        .filter(|doc| doc.is_object())
        .cloned()
        .collect()
}

/// `20101213` → `2010-12-13`；不是 8 位数字则原样返回（展示友好，不猜语义）。
fn format_ops_date(raw: &str) -> String {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 8 {
        format!("{}-{}-{}", &digits[0..4], &digits[4..6], &digits[6..8])
    } else {
        raw.trim().to_string()
    }
}

/// `document-id` 里取指定 `document-id-type` 的子字段文本。
fn document_id_field(scope: &serde_json::Value, id_type: &str, field: &str) -> Option<String> {
    find_all(scope, "document-id").into_iter().find_map(|doc| {
        let got = json_attr(doc, "document-id-type");
        if !got.is_empty() && got != id_type {
            return None;
        }
        let value = find_first(doc, field).map(json_text);
        value.filter(|s| !s.is_empty())
    })
}

/// 把 OPS 的一条 `exchange-document` 映射为域内 [`Patent`]。
///
/// 取值顺序刻意「先属性、后嵌套」：`exchange-document` 自己带
/// `country/doc-number/kind` 属性（官方样例），`bibliographic-data` 里是同一份的展开形态，
/// 任缺其一都能补齐。**所有取值都用 `as_str`/`json_text` 系列容错，缺字段只降级为空串。**
///
/// ⚠️ **必须先收窄容器再取子字段**：`publication-reference`、`application-reference`、
/// `priority-claim` 里装的是**同名的 `document-id`（都带 `date`/`doc-number`/`kind`）**，
/// 而本项目里 JSON 对象的键是**字典序**遍历（见 [`find_key`] 注释）——
/// 直接对整条文档深找 `document-id` 会把「申请号」当公开号、「申请日」当公开日
/// （`application-reference` < `publication-reference`）。故下方一律
/// 先定位 `publication-reference` / `application-reference` / `priority-claims` 再取字段。
///
/// 注意 `raw_json` 存的是**该条命中的完整原始 JSON**，不截断（AGENTS.md 2.5）。
fn epo_to_patent(doc: &serde_json::Value) -> Patent {
    let country_attr = json_attr(doc, "country");
    let number_attr = json_attr(doc, "doc-number");
    let kind_attr = json_attr(doc, "kind");

    // 优先在 bibliographic-data 内取公开著录项（缺这层包装时退回整条文档，容错而非报错）。
    let scope = find_first(doc, "bibliographic-data").unwrap_or(doc);
    let publication_refs = find_all(scope, "publication-reference");
    let pub_field = |id_type: &str, field: &str| {
        publication_refs
            .iter()
            .find_map(|pr| document_id_field(pr, id_type, field))
    };

    let country = first_non_empty([
        &country_attr,
        &pub_field("docdb", "country").unwrap_or_default(),
    ]);
    let number = first_non_empty([
        &number_attr,
        &pub_field("docdb", "doc-number").unwrap_or_default(),
    ]);
    let kind = first_non_empty([&kind_attr, &pub_field("docdb", "kind").unwrap_or_default()]);

    // epodoc 形态本身就是「国家+号码」的规范串（官方样例：KR20100130646），优先用它。
    let epodoc = pub_field("epodoc", "doc-number");
    let mut patent_number = match epodoc.as_deref() {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => format!("{country}{number}"),
    };
    patent_number = patent_number.to_uppercase().replace([' ', '-', '.'], "");
    if !kind.is_empty() && !patent_number.ends_with(&kind.to_uppercase()) {
        patent_number.push_str(&kind.to_uppercase());
    }
    let country_code = if country.is_empty() {
        patent_number
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect::<String>()
    } else {
        country.to_ascii_uppercase()
    };

    // 标题：官方样例是 `<invention-title lang="en">`，多语种时按 en 优先、否则取首个。
    let titles = find_all(scope, "invention-title");
    let title = titles
        .iter()
        .copied()
        .find(|t| json_attr(t, "lang").eq_ignore_ascii_case("en"))
        .or_else(|| titles.first().copied())
        .map(json_text)
        .unwrap_or_default();

    // 摘要：`<abstract lang="en"><p num="0">…</p></abstract>`，段落按顺序拼接。
    // 经典 schema 用 `abstract`，DOCX 用 `invention-abstract`（`dwk:` 前缀由 [`key_is`] 吃掉），
    // 两种都试，取第一个非空。
    let abstract_text = find_any(scope, &["abstract", "invention-abstract"])
        .into_iter()
        .map(|ab| {
            find_all(ab, "p")
                .into_iter()
                .map(json_text)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .find(|s| !s.is_empty())
        .unwrap_or_default();

    // 申请人：同一人常有 epodoc / original 两份写法（官方样例即如此），取第一份非空名字。
    let applicant = find_all(scope, "applicant")
        .into_iter()
        .find_map(|a| {
            find_first(a, "name")
                .map(json_text)
                .or_else(|| find_first(a, "applicant-name").map(json_text))
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_default();

    // 发明人：多个人是真数据，逐个取名字后去重拼接。
    let mut inventor_names: Vec<String> = Vec::new();
    for inv in find_all(scope, "inventor") {
        let name = find_first(inv, "name")
            .map(json_text)
            .or_else(|| find_first(inv, "inventor-name").map(json_text))
            .unwrap_or_default();
        if !name.is_empty() && !inventor_names.contains(&name) {
            inventor_names.push(name);
        }
    }
    let inventor = inventor_names.join(", ");

    let publication_date = pub_field("docdb", "date")
        .or_else(|| pub_field("epodoc", "date"))
        .map(|d| format_ops_date(&d))
        .unwrap_or_default();
    let filing_date = find_all(scope, "application-reference")
        .into_iter()
        .find_map(|aref| {
            document_id_field(aref, "docdb", "date")
                .or_else(|| document_id_field(aref, "epodoc", "date"))
                .or_else(|| find_first(aref, "date").map(json_text))
                .filter(|s| !s.is_empty())
                .map(|d| format_ops_date(&d))
        })
        .unwrap_or_default();
    let priority_date = find_all(scope, "priority-claims")
        .into_iter()
        .chain(find_all(scope, "priority-claim"))
        .find_map(|pc| {
            find_all(pc, "date")
                .into_iter()
                .map(json_text)
                .find(|s| !s.is_empty())
                .map(|d| format_ops_date(d.as_str()))
        })
        .unwrap_or_default();

    Patent {
        id: uuid::Uuid::new_v4().to_string(),
        patent_number,
        title: collapse_spaces(&title),
        abstract_text: collapse_spaces(&abstract_text),
        description: String::new(),
        claims: String::new(),
        applicant: collapse_spaces(&applicant),
        inventor: collapse_spaces(&inventor),
        filing_date,
        publication_date,
        grant_date: None,
        ipc_codes: String::new(),
        cpc_codes: String::new(),
        priority_date,
        country: country_code,
        kind_code: kind.to_uppercase(),
        family_id: find_first(doc, "family-id")
            .map(json_text)
            .or_else(|| {
                let attr = json_attr(doc, "family-id");
                (!attr.is_empty()).then_some(attr)
            })
            .filter(|s| !s.is_empty()),
        legal_status: String::new(),
        citations: "[]".into(),
        cited_by: "[]".into(),
        source: SOURCE_LABEL.into(),
        raw_json: doc.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        images: "[]".into(),
        pdf_url: String::new(),
    }
}

/// 首个非空串。
fn first_non_empty<'a>(items: impl IntoIterator<Item = &'a String>) -> String {
    items
        .into_iter()
        .map(|s| s.trim().to_string())
        .find(|s| !s.is_empty())
        .unwrap_or_default()
}

/// 折叠连续空白（OPS 的 `<p>` 段落里常见换行缩进带入的多空格）。
fn collapse_spaces(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl SearchProvider for EpoOpsProvider {
    fn kind(&self) -> SourceKind {
        SourceKind::EpoOps
    }

    fn search<'a>(
        &'a self,
        query: SearchQuery,
    ) -> Pin<Box<dyn Future<Output = SearchOutcome> + Send + 'a>> {
        Box::pin(async move {
            let started = self.clock.now_ms();

            // 凭证缺失 / 关键词渲染为空：不发请求，Skipped（spec §4 + chain 的 Skip 语义）
            if !self.has_credentials() {
                return skipped_outcome("未配置 EPO_KEY/EPO_SECRET，本轮未发起 EPO OPS 请求");
            }
            let cql = render_cql(&query);
            if cql.is_empty() {
                return skipped_outcome("CQL 关键词为空，本轮未发起 EPO OPS 请求");
            }

            let url = request_url(&query);
            println!(
                "[ONLINE] {SOURCE_LABEL} q={} range={} page={}",
                report_excerpt(&cql),
                range_param(&query),
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
                results: merged_from(SourceKind::EpoOps, patents),
                attempts: vec![AttemptReport {
                    source: SourceKind::EpoOps,
                    status,
                    latency_ms: self.clock.now_ms().saturating_sub(started),
                    hits,
                    error: error_note,
                    hint: None,
                }],
                upstream_total,
            }
        })
    }
}

/// 「本源未参与」的结果（不发网络请求），语义与 `routes/search.rs::serpapi_skipped_outcome` 一致：
/// `Skipped` + `latency_ms = 0`（压根没发出请求，耗时不该凭空记账）。
fn skipped_outcome(reason: &str) -> SearchOutcome {
    SearchOutcome {
        results: vec![],
        attempts: vec![AttemptReport {
            source: SourceKind::EpoOps,
            status: AttemptStatus::Skipped,
            latency_ms: 0,
            hits: 0,
            error: Some(reason.to_string()),
            hint: None,
        }],
        upstream_total: None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 测试（全部离线：假传输 + 假时钟，零网络。`pub(crate)` 是为了让 `chain.rs` 的
// 降级链用例复用同一套假件 —— 与 `providers/google_patents_xhr.rs::tests` 同构。）
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicU64, Ordering};

    // ─────────────────────────────────────────────────────────────────────────
    // ⚠️ EVIDENCE：本模块所有断言的实证材料（MA2b，2026-09-21 本机采集）
    //
    // L1 无效凭证取 token（**本轮实测**，只读、无副作用）
    //     curl -s -X POST https://ops.epo.org/3.2/auth/accesstoken \
    //          -u invalid-key:invalid-secret -d grant_type=client_credentials
    //   → HTTP 401，正文见 [`REAL_401_TOKEN_BODY`]
    //   → 依据：401 归 `FailKind::Auth`（`model.rs::for_http_status`），
    //     且「凭证不对」必须在**发数据请求之前**就暴露（省一次配额）。
    //
    // L2 匿名（不带 Authorization）打 search/biblio（**本轮实测**）
    //     curl -s -D - "https://ops.epo.org/3.2/rest-services/published-data/search/biblio?q=ti%3Dbattery&Range=1-25"
    //   → HTTP **403** + 响应头 `x-rejection-reason: AnonymousQuotaPerDay`，
    //     正文见 [`REAL_403_ANONYMOUS_BODY`]
    //   → 依据：**过期 token 与压根没带 token 都表现为 403**，故 auth 重试判定取
    //     「401 或 403」（两者正确处置相同：重取 token 再试一发）。
    //
    // L3 官方 Reference Guide for OPS v1.3.20（EPO 出版件）§Authentication：
    //     响应样例 `{"expires_in":"1199","token_type":"Bearer","access_token":"4AWo…","status":"approved"}`
    //     原文 "The access token produced is valid for approximately 20mins.
    //           A New access token should be requested as soon as an invalid access
    //           token message is received from OPS."
    //   → 依据：**token 缓存 + 到期前 60s 提前刷新 + 撞 401/403 重取一次**（L1/L2 的现场语义）。
    //   ⚠️ 注意 `expires_in` 在官方样例里是**字符串**，[`DOC_TOKEN_REPLY`] 保留该形态，
    //     数字形态由 `token_ttl_parses_string_number_and_garbage` 另行锁死。
    //
    // L4 官方 Reference Guide v1.3.20 §CQL search「Response example (with biblio)」（XML）
    //     —— KR / 20100130646 / A、total-result-count="10000"、invention-title lang="en"。
    //   → 本文件 [`SEARCH_BIBLIO_REPLY`] 的**第 1 条**就是它按 JSON 规则（见 L5）的逐字段转写。
    //
    // L5 真实 OPS **JSON** 抓取件（匿名可得，无凭证）：
    //     GET /3.2/rest-services/published-data/docdb/EP3445287/biblio (Accept: application/json)
    //     → 403 的 JSON 错误体本身即 OPS JSON 渲染规则的实物：
    //       {"ops:world-patent-data":{"@xmlns":{…},"ops:document-inquiry":
    //         {"ops:publication-reference":{"document-id":
    //           {"@document-id-type":"epodoc","doc-number":{"$":"EP3445287"}}}, …
    //   → 钉死四件事：属性→`"@名"`；元素文本→`{"$":"文本"}`；重复元素→数组（单元素→对象，
    //     **多态**）；命名空间前缀（`ops:`）**保留**。
    //
    // L6 EPO 官方测试件 `search/response.xml`（无 constituent 的 CQL 检索响应，XML）：
    //     <ops:biblio-search total-result-count="305797" publications-count="3">
    //       <ops:query syntax="CQL">pa = Siemens</ops:query><ops:range begin="1" end="3"/>
    //       <ops:search-result><ops:publication-reference …><document-id …>…
    //   → 钉死 spec §4 的那个坑：**不带 constituent 的响应只有公开号引用，没有任何标题**，
    //     用作检索源会被「空标题丢弃」恒定清零 → 本源改用 `/search/biblio`。
    //
    // L7 官方 CQL 索引目录（Appendix）：`ti`/`ab`/`ta` 标注 "in English"；
    //     `pd="20051212 20051214"` 为区间写法；**`cn` = IPC8 core additional class（不是国家）**；
    //     国家限定用裸码（`pa all "…" and JP`）→ [`render_cql`] 的三条硬断言。
    //
    // ⚠️ 诚实声明：本包**没有**已配置凭证下的 `search/biblio` 真机 JSON 响应件
    //   （L5 是匿名 403 体，L4/L6 是官方文档件）。因此
    //   ① [`SEARCH_BIBLIO_REPLY`] 的第 2、3 条为**合成件**（号码 EP3445287 真实存在，
    //      但标题/摘要/发明人为构造文本，只用于覆盖映射分支，不作为该公开号的著录事实）；
    //   ② 「真机 smoke（拿 Key 打一發，核对 total-result-count 与首条标题）」列为**遗留项**。
    // ─────────────────────────────────────────────────────────────────────────

    /// L1 现场件：无效凭证的 401 响应体（逐字节照抄，含原文的制表符与换行）。
    pub(crate) const REAL_401_TOKEN_BODY: &str =
        "<error><code>401</code><message>ClientId is Invalid</message>\n\t\t\t</error>\n\t\t";

    /// L2 现场件：匿名检索的 403 响应体（`x-rejection-reason: AnonymousQuotaPerDay`）。
    pub(crate) const REAL_403_ANONYMOUS_BODY: &str = "<error><code>403</code><message>This request has been rejected due to the violation of Fair Use policy</message><moreInfo>https://www.epo.org/service-support/ordering/fair-use.html</moreInfo>\n\t\t\t\t</error>\n\t\t\t";

    /// L3 官方 token 响应样例（`expires_in` 为字符串 1199s ≈ 20 分钟）。
    const DOC_TOKEN_REPLY: &str = r#"{
  "access_token": "4AWoVXmKq0gG7hQz1fZ3exampletoken",
  "expires_in": "1199",
  "token_type": "Bearer",
  "status": "approved"
}"#;

    /// L4 + L5：官方「with biblio」样例的 JSON 转写，共 3 条命中。
    ///
    /// - 第 1 条 = **官方文档样例本体**（KR20100130646A；官方样例里没有摘要，这里也不给，
    ///   用它锁死「无摘要仍可成条」与 `epodoc` 号已含 kind 的分支）。
    /// - 第 2 条 = 合成（覆盖 `@lang="en"` 之外的标题在前、多段 `p` 摘要、多发明人去重、
    ///   `priority-claims`、`Range` 之外的一切字段）。
    /// - 第 3 条 = 合成（覆盖「只有属性 + 单对象形态」的最小形态：无 application-reference、
    ///   无 priority、`document-id` 与 `p` 都是对象而不是数组）。
    ///
    /// ⚠️ 第 3 条**刻意带一个发明人**：共用的 [`crate::search::relevance::calculate_online_relevance`]
    /// 对**空 inventor** 会走 `query.contains("")` 恒真而 +15（MA1 起三源共用的既有行为，本包不动它），
    /// 缺了发明人两条命中的先后就由这个隐藏加分决定，位置分断言失去意义。
    pub(crate) const SEARCH_BIBLIO_REPLY: &str = r#"{
  "ops:world-patent-data": {
    "@xmlns": { "ops": "http://ops.epo.org", "$": "http://www.epo.org/exchange" },
    "ops:biblio-search": {
      "@total-result-count": "10000",
      "@publications-count": "3",
      "ops:query": { "@syntax": "CQL", "$": "(ti=\"battery\" or ab=\"battery\")" },
      "ops:range": { "@begin": "1", "@end": "25" },
      "ops:search-result": {
        "exchange-documents": {
          "exchange-document": [
            {
              "@system": "ops.epo.org",
              "@family-id": "37717388",
              "@country": "KR",
              "@doc-number": "20100130646",
              "@kind": "A",
              "bibliographic-data": {
                "publication-reference": {
                  "document-id": [
                    { "@document-id-type": "docdb", "country": { "$": "KR" },
                      "doc-number": { "$": "20100130646" }, "kind": { "$": "A" },
                      "date": { "$": "20101213" } },
                    { "@document-id-type": "epodoc", "country": { "$": "KR" },
                      "doc-number": { "$": "KR20100130646A" }, "date": { "$": "20101213" } }
                  ]
                },
                "application-reference": {
                  "document-id": [
                    { "@document-id-type": "docdb", "country": { "$": "KR" },
                      "doc-number": { "$": "1020060006646" }, "date": { "$": "20060720" } }
                  ]
                },
                "invention-title": {
                  "@lang": "en",
                  "$": "INJECTION MOLDED MICROLENSES FOR LED LIGHT EXTRACTION AND CONTROL AND METHOD FOR MANUFACTURING THE SAME"
                },
                "parties": {
                  "applicants": {
                    "applicant": [
                      { "@data-format": "epodoc",
                        "applicant-name": { "name": { "$": "LG PHILIPS LIGHTING CO., LTD. [KR]" } } },
                      { "@data-format": "original",
                        "applicant-name": { "name": { "$": "LG PHILIPS LIGHTING CO., LTD." } } }
                    ]
                  },
                  "inventors": {
                    "inventor": { "inventor-name": { "name": { "$": "KOO, JAE-HYUN" } } }
                  }
                }
              }
            },
            {
              "@system": "ops.epo.org",
              "@family-id": "63039804",
              "@country": "EP",
              "@doc-number": "3445287",
              "@kind": "B1",
              "bibliographic-data": {
                "publication-reference": {
                  "document-id": [
                    { "@document-id-type": "docdb", "country": { "$": "EP" },
                      "doc-number": { "$": "3445287" }, "kind": { "$": "B1" },
                      "date": { "$": "20181226" } },
                    { "@document-id-type": "epodoc", "doc-number": { "$": "EP3445287" } }
                  ]
                },
                "application-reference": {
                  "document-id": { "@document-id-type": "docdb", "country": { "$": "EP" },
                    "doc-number": { "$": "16199729" }, "date": { "$": "20150707" } }
                },
                "invention-title": [
                  { "@lang": "de", "$": "FESTKÖRPERELEKTROLYT UND ALLESFESTKÖRPERBATTERIE" },
                  { "@lang": "en", "$": "SOLID ELECTROLYTE COMPOSITION AND ALL-SOLID-STATE BATTERY USING SAME" }
                ],
                "abstract": {
                  "@lang": "en",
                  "p": [
                    { "@num": "0", "$": "The present invention provides a solid electrolyte composition which is suitable for use in an all-solid-state battery." },
                    { "@num": "1", "$": "The all-solid-state battery exhibits improved cycle life and high-rate discharge characteristics." }
                  ]
                },
                "parties": {
                  "applicants": {
                    "applicant": { "@data-format": "epodoc",
                      "applicant-name": { "name": { "$": "TOYOTA JIDOSHA KABUSHIKI KAISHA [JP]" } } }
                  },
                  "inventors": {
                    "inventor": [
                      { "inventor-name": { "name": { "$": "UMEDA, YOSUKE" } } },
                      { "inventor-name": { "name": { "$": "SAITO, TAKESHI" } } },
                      { "@data-format": "original", "inventor-name": { "name": { "$": "UMEDA, YOSUKE" } } }
                    ]
                  }
                },
                "priority-claims": {
                  "priority-claim": {
                    "@sequence": "1",
                    "document-id": { "@document-id-type": "docdb", "country": { "$": "JP" },
                      "doc-number": { "$": "2015143208" }, "date": { "$": "20150714" } }
                  }
                }
              }
            },
            {
              "@system": "ops.epo.org",
              "@family-id": "63039805",
              "@country": "JP",
              "@doc-number": "2018123456",
              "@kind": "A",
              "bibliographic-data": {
                "publication-reference": {
                  "document-id": { "@document-id-type": "docdb", "country": { "$": "JP" },
                    "doc-number": { "$": "2018123456" }, "kind": { "$": "A" },
                    "date": { "$": "20180907" } }
                },
                "invention-title": { "@lang": "en", "$": "BATTERY CONTROL DEVICE AND BATTERY SYSTEM" },
                "abstract": {
                  "@lang": "en",
                  "p": { "@num": "0", "$": "A battery control device for a secondary battery pack." }
                },
                "parties": {
                  "applicants": {
                    "applicant": { "applicant-name": { "name": { "$": "HITACHI ASTEMO, LTD. [JP]" } } }
                  },
                  "inventors": {
                    "inventor": { "inventor-name": { "name": { "$": "TANAKA, KENJI" } } }
                  }
                }
              }
            }
          ]
        }
      }
    }
  }
}"#;

    /// 零命中形态：官方响应里 `ops:search-result` 直接空掉（L4/L6 的同一形状）。
    pub(crate) const EMPTY_SEARCH_REPLY: &str = r#"{
  "ops:world-patent-data": {
    "ops:biblio-search": {
      "@total-result-count": "0",
      "ops:query": { "@syntax": "CQL", "$": "(ti=\"nothingmatching\" or ab=\"nothingmatching\")" },
      "ops:range": { "@begin": "1", "@end": "25" },
      "ops:search-result": {}
    }
  }
}"#;

    // ── 假时钟 / 假传输（离线注入点） ─────────────────────────────────────────

    /// 一发请求的预定结果：`Ok`=上游回了响应（含 4xx/5xx），`Err`=压根没收到响应。
    pub(crate) type Scripted = Result<EpoReply, EpoFault>;

    /// 上游回了这么一个响应（命名与 `google_patents_xhr::tests::reply` 对齐，便于链用例复用）。
    pub(crate) fn reply(status: u16, body: &str) -> Scripted {
        Ok(EpoReply {
            status,
            body: body.to_string(),
            retry_after: None,
        })
    }

    /// 同上，但带 `Retry-After`（spec §1 要求 429 尊重它）。
    pub(crate) fn reply_retry(status: u16, body: &str, secs: u64) -> Scripted {
        Ok(EpoReply {
            status,
            body: body.to_string(),
            retry_after: Some(Duration::from_secs(secs)),
        })
    }

    /// 网络故障：请求发不出去 / 没等到响应。
    pub(crate) fn fault(msg: &str) -> Scripted {
        Err(EpoFault::Network(msg.to_string()))
    }

    /// 一份 token 响应（`expires_in` 走官方样例的**字符串**形态，见 L3）。
    pub(crate) fn token_reply(token: &str, expires_in: &str) -> Scripted {
        token_body(&format!(
            "{{\"access_token\":\"{token}\",\"expires_in\":\"{expires_in}\",\"token_type\":\"Bearer\",\"status\":\"approved\"}}"
        ))
    }

    /// 自定义 token 响应正文（用于测数字型 `expires_in`、缺字段、非 JSON 等形态）。
    pub(crate) fn token_body(body: &str) -> Scripted {
        reply(200, body)
    }

    /// 假时钟：`sleep` 立即完成但**推进虚拟时间**并留痕，
    /// 于是「20 分钟后提前刷新」这类断言可以在毫秒级跑完，退避也不会真的等。
    pub(crate) struct FakeEpoClock {
        now: AtomicU64,
        sleeps: Mutex<Vec<Duration>>,
    }

    impl FakeEpoClock {
        pub(crate) fn new() -> Arc<Self> {
            Arc::new(FakeEpoClock {
                now: AtomicU64::new(0),
                sleeps: Mutex::new(Vec::new()),
            })
        }
        pub(crate) fn recorded(&self) -> Vec<Duration> {
            self.sleeps.lock().expect("lock").clone()
        }
        /// 直接把时钟往前拨（模拟真实流逝，不发请求）。
        pub(crate) fn advance_secs(&self, secs: u64) {
            self.now.fetch_add(secs * 1000, Ordering::SeqCst);
        }
    }

    impl EpoClock for FakeEpoClock {
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

    /// 假传输：**token（POST）与检索（GET）各一条脚本队列**，并把每一发的
    /// `method/url/authorization/body` 全量留痕 —— token 缓存、401 重取、
    /// 「未配置 Key 就一发都不发」这些断言全靠它。
    pub(crate) struct FakeEpoTransport {
        tokens: Mutex<VecDeque<Scripted>>,
        searches: Mutex<VecDeque<Scripted>>,
        token_fallback: Scripted,
        search_fallback: Scripted,
        requests: Mutex<Vec<EpoRequest>>,
    }

    impl FakeEpoTransport {
        pub(crate) fn new(token_script: Vec<Scripted>, search_script: Vec<Scripted>) -> Arc<Self> {
            let mut tokens = VecDeque::from(token_script);
            let mut searches = VecDeque::from(search_script);
            // 与 XHR 假件同一手法：**末发兼作脚本耗尽后的兜底**，
            // 这样「测试里没排到的那一发」不会 panic（是否多发由 *_calls 断言把关）。
            let token_fallback = tokens
                .pop_back()
                .unwrap_or_else(|| token_reply("TOK-DEFAULT", "1199"));
            let search_fallback = searches
                .pop_back()
                .unwrap_or_else(|| reply(200, EMPTY_SEARCH_REPLY));
            Arc::new(FakeEpoTransport {
                tokens: Mutex::new(tokens),
                searches: Mutex::new(searches),
                token_fallback,
                search_fallback,
                requests: Mutex::new(Vec::new()),
            })
        }

        pub(crate) fn requests(&self) -> Vec<EpoRequest> {
            self.requests.lock().expect("lock").clone()
        }
        fn filter<F: Fn(&EpoRequest) -> bool>(&self, f: F) -> Vec<EpoRequest> {
            self.requests
                .lock()
                .expect("lock")
                .iter()
                // `iter()` 给的是 `&&EpoRequest`，必须显式解一层引用喂给 `f`
                .filter(|r| f(r))
                .cloned()
                .collect()
        }
        pub(crate) fn token_calls(&self) -> usize {
            self.filter(|r| r.method == EpoMethod::Post).len()
        }
        pub(crate) fn search_calls(&self) -> usize {
            self.filter(|r| r.method == EpoMethod::Get).len()
        }
        pub(crate) fn total_calls(&self) -> usize {
            self.requests.lock().expect("lock").len()
        }
        pub(crate) fn authorizations(&self, method: EpoMethod) -> Vec<String> {
            self.filter(|r| r.method == method)
                .into_iter()
                .map(|r| r.authorization)
                .collect()
        }
        pub(crate) fn search_urls(&self) -> Vec<String> {
            self.filter(|r| r.method == EpoMethod::Get)
                .into_iter()
                .map(|r| r.url)
                .collect()
        }
    }

    impl EpoTransport for FakeEpoTransport {
        fn send(
            &self,
            request: EpoRequest,
        ) -> Pin<Box<dyn Future<Output = Result<EpoReply, EpoFault>> + Send + '_>> {
            Box::pin(async move {
                self.requests.lock().expect("push").push(request.clone());
                let scripted = match request.method {
                    EpoMethod::Post => self.tokens.lock().expect("pop").pop_front(),
                    EpoMethod::Get => self.searches.lock().expect("pop").pop_front(),
                };
                match scripted {
                    Some(s) => s,
                    None => match request.method {
                        EpoMethod::Post => self.token_fallback.clone(),
                        EpoMethod::Get => self.search_fallback.clone(),
                    },
                }
            })
        }
    }

    /// 组一个完全离线的源实例：假传输 + 假时钟，绝不出网。
    pub(crate) fn provider(
        token_script: Vec<Scripted>,
        search_script: Vec<Scripted>,
    ) -> (EpoOpsProvider, Arc<FakeEpoTransport>, Arc<FakeEpoClock>) {
        provider_with("EPO_KEY", "EPO_SECRET", token_script, search_script)
    }

    /// 同 [`provider`]，但自定义凭证（用于「未配置 Key」的用例）。
    pub(crate) fn provider_with(
        key: &str,
        secret: &str,
        token_script: Vec<Scripted>,
        search_script: Vec<Scripted>,
    ) -> (EpoOpsProvider, Arc<FakeEpoTransport>, Arc<FakeEpoClock>) {
        let clock = FakeEpoClock::new();
        let transport = FakeEpoTransport::new(token_script, search_script);
        let p = EpoOpsProvider::with_transport(
            key.to_string(),
            secret.to_string(),
            db(),
            transport.clone(),
            clock.clone(),
        );
        (p, transport, clock)
    }

    pub(crate) fn db() -> Arc<Database> {
        Arc::new(Database::init(":memory:").expect("in-memory db"))
    }

    /// spec §1 默认语境（中国 + 中文）：与 `google_patents_xhr::tests::query` 同名同形，
    /// 便于 `chain.rs` 用同一把尺子喂三源。
    pub(crate) fn query(keyword: &str) -> SearchQuery {
        SearchQuery {
            keyword: keyword.to_string(),
            ..Default::default()
        }
    }

    /// 英文语境 + 不限国家：OPS 的 `ti/ab` 只有英文索引（L7），
    /// 命中类用例走这一形，避免把「语种能力边界」和「映射对不对」混在一个断言里。
    pub(crate) fn en_query(keyword: &str) -> SearchQuery {
        SearchQuery {
            keyword: keyword.to_string(),
            country: None,
            language: Some(Lang::English),
            ..Default::default()
        }
    }

    // ── ① CQL / Range / URL（spec §4 + L7） ───────────────────────────────────

    #[test]
    fn keyword_query_renders_title_or_abstract_pair() {
        assert_eq!(
            "(ti=\"battery\" or ab=\"battery\")",
            render_cql(&en_query("battery"))
        );
        // 国家条件按官方形态**追加裸码**，其余不变
        assert_eq!(
            "(ti=\"battery\" or ab=\"battery\") and CN",
            render_cql(&query("battery"))
        );
        // Keyword / Mixed 与「未指定」同形：OPS 没有「按界面语种检索」的索引位，
        // 编造一个 `lang=` 只会换来一个未验证的上游行为。
        for t in [None, Some(SearchType::Keyword), Some(SearchType::Mixed)] {
            let mut q = en_query("battery");
            q.search_type = t;
            assert_eq!("(ti=\"battery\" or ab=\"battery\")", render_cql(&q));
        }
    }

    #[test]
    fn search_type_maps_to_the_documented_indexes() {
        let mut q = en_query("SIEMENS");
        q.search_type = Some(SearchType::Applicant);
        assert_eq!("pa=\"SIEMENS\"", render_cql(&q));
        q.search_type = Some(SearchType::Inventor);
        assert_eq!("in=\"SIEMENS\"", render_cql(&q));

        // 号检索走的是**另一条形**：官方样例是 `pn=EP1000000`（不加引号）
        let mut nq = en_query("EP3445287 A1");
        nq.search_type = Some(SearchType::PatentNumber);
        assert_eq!("pn=EP3445287A1", render_cql(&nq), "空格被去掉，引号不加");
        // 号码本身已唯一 → **不再叠加**国家/日期（叠了只会把精确查变成零命中）
        nq.country = Some("CN".to_string());
        nq.date_from = Some("2020-01-05".to_string());
        assert_eq!("pn=EP3445287A1", render_cql(&nq));
    }

    #[test]
    fn country_is_a_bare_code_and_never_the_cn_index() {
        // L7：CQL 目录里的 `cn` 是「IPC8 core additional class」，**不是国家**。
        // spec §4 写的 `cn=CN` 会把分类号当国家用 → 这里锁死绝不生成 `cn=`。
        for cc in ["jp", "US", " KR "] {
            let mut q = en_query("battery");
            q.country = Some(cc.to_string());
            let cql = render_cql(&q);
            assert!(
                cql.ends_with(&format!(" and {}", cc.trim().to_uppercase())),
                "{cql}"
            );
            assert!(!cql.contains("cn="), "不得把国家塞进分类号索引: {cql}");
        }
        // 认不出的输入**省略**该条件，而不是发一条语法错的查询去撞上游 400
        let mut q = en_query("battery");
        q.country = Some("China".to_string());
        assert_eq!("(ti=\"battery\" or ab=\"battery\")", render_cql(&q));
    }

    #[test]
    fn dates_render_as_ops_window_and_bad_dates_are_dropped() {
        let mut q = en_query("battery");
        q.date_from = Some("2020-01-05".to_string());
        q.date_to = Some("2020/3/9".to_string());
        assert_eq!(
            "(ti=\"battery\" or ab=\"battery\") and pd=\"20200105 20200309\"",
            render_cql(&q),
            "官方区间写法：引号内两个 YYYYMMDD"
        );
        q.date_to = None;
        assert!(
            render_cql(&q).ends_with("and pd=\"20200105\""),
            "{}",
            render_cql(&q)
        );
        q.date_from = Some("last tuesday".to_string());
        assert_eq!(
            "(ti=\"battery\" or ab=\"battery\")",
            render_cql(&q),
            "归一失败 → 条件省略：过滤失效绝不能变成结果消失"
        );
    }

    #[test]
    fn cql_metacharacters_cannot_rewrite_the_query_shape() {
        // AGENTS.md 2.6 的边界隔离精神同样适用于上游查询语言：
        // 一次用户输入不得把「一条关键词条件」改写成「带申请人布尔条件的查询」。
        let cql = render_cql(&en_query("battery\" or pa=\"_evil (x) = y; z:w/v'"));
        assert_eq!(1, cql.matches("ti=").count(), "{cql}");
        assert_eq!(
            4,
            cql.matches('"').count(),
            "引号只能在模板里，不能来自输入: {cql}"
        );
        assert!(!cql.contains("pa="), "注入的申请人条件不得成立: {cql}");
        assert!(!cql.contains(" and "), "注入的布尔算子不得成立: {cql}");
        assert_eq!(
            "(ti=\"battery or pa_evil x y zwv\" or ab=\"battery or pa_evil x y zwv\")",
            cql
        );
    }

    #[test]
    fn blank_keyword_renders_no_cql() {
        assert!(render_cql(&en_query("   ")).is_empty());
        assert!(
            render_cql(&en_query("\"()=\"")).is_empty(),
            "全是元字符 → 清洗后为空"
        );
    }

    #[test]
    fn range_and_url_follow_the_documented_window_rules() {
        let mut q = en_query("battery");
        q.limit = 25;
        assert_eq!("1-25", range_param(&q), "spec §4 的 Range=1-25");
        q.limit = 500;
        assert_eq!(
            "1-25",
            range_param(&q),
            "跨度封顶（官方另限 ≤100，本源取 spec 的 25）"
        );
        q.limit = 0;
        assert_eq!(
            "1-25",
            range_param(&q),
            "limit=0 → 用默认跨度，绝不发空 Range"
        );
        q.limit = 25;
        assert_eq!("1-25", range_param(&q));
        q.page = 2;
        assert_eq!(
            "26-50",
            range_param(&q),
            "页码换成 Range 偏移，不臆造 page 参数"
        );
        q.page = 0;
        assert_eq!("1-25", range_param(&q), "非法页码钳到 1");
        q.page = 81;
        assert_eq!(
            "2000-2000",
            range_param(&q),
            "官方：>2000 不可翻检 → begin 封顶，绝不发越界窗口"
        );

        q.limit = 20;
        q.page = 1;
        let url = request_url(&q);
        assert!(url.starts_with(SEARCH_ENDPOINT), "{url}");
        assert!(url.contains("&Range=1-20"), "{url}");
        assert!(
            !url.contains('"') && !url.contains('('),
            "q 必须 URL 编码: {url}"
        );
        assert!(url.contains("%22battery%22"), "{url}");
    }

    // ── ② 凭证缺失 / 空词：Skipped 且一发都不发（chain 的 Skip 语义） ──────────

    #[tokio::test]
    async fn missing_credentials_are_skipped_without_any_request() {
        for (key, secret) in [("", "s"), ("k", ""), ("   ", "  "), ("", "")] {
            let (p, transport, clock) = provider_with(key, secret, vec![], vec![]);
            assert!(!p.has_credentials(), "{key:?}/{secret:?} 不算配置好");
            let outcome = p.search(en_query("battery")).await;
            assert_eq!(AttemptStatus::Skipped, outcome.attempts[0].status);
            assert_eq!(0, transport.total_calls(), "没凭证就不能发出任何请求");
            assert_eq!(0, outcome.attempts[0].latency_ms, "未发请求不该记耗时");
            assert!(clock.recorded().is_empty());
            let err = outcome.attempts[0].error.clone().expect("跳过原因");
            assert!(
                err.contains("EPO_KEY") && err.contains("EPO_SECRET"),
                "{err}"
            );
            assert!(outcome.results.is_empty());
        }
    }

    #[tokio::test]
    async fn blank_keyword_is_skipped_without_any_request() {
        let (p, transport, clock) = provider(vec![], vec![]);
        let outcome = p.search(en_query("   ")).await;
        assert_eq!(AttemptStatus::Skipped, outcome.attempts[0].status);
        assert_eq!(0, transport.total_calls(), "空 CQL 打上游只会拿到 400");
        assert_eq!(0, outcome.attempts[0].latency_ms);
        assert!(clock.recorded().is_empty());
        assert!(outcome.attempts[0]
            .error
            .clone()
            .expect("reason")
            .contains("CQL"));
    }

    // ── ③ token 缓存与提前刷新（验收用例 ①） ──────────────────────────────────

    #[tokio::test]
    async fn access_token_is_cached_across_searches() {
        // L3：token ≈20 分钟有效 → 两次检索只应发**一次** POST
        let (p, transport, _clock) = provider(
            vec![token_reply("TOK-A", "1199")],
            vec![reply(200, SEARCH_BIBLIO_REPLY)],
        );
        let first = p.search(en_query("battery")).await;
        let second = p.search(en_query("electrolyte")).await;

        assert_eq!(
            1,
            transport.token_calls(),
            "token 必须复用，第二次检索不得再取"
        );
        assert_eq!(2, transport.search_calls());
        assert_eq!(
            vec!["Bearer TOK-A".to_string(), "Bearer TOK-A".to_string()],
            transport.authorizations(EpoMethod::Get)
        );
        assert!(first.attempts[0].produced_hits());
        assert!(second.attempts[0].produced_hits());
    }

    #[tokio::test]
    async fn token_refreshes_before_expiry_not_after() {
        let (p, transport, clock) = provider(
            vec![token_reply("TOK-A", "1199"), token_reply("TOK-B", "1199")],
            vec![reply(200, EMPTY_SEARCH_REPLY)],
        );
        p.search(en_query("battery")).await;
        assert_eq!(1, transport.token_calls());

        // 剩余 99s（> 60s 余量）→ 仍用缓存
        clock.advance_secs(1100);
        p.search(en_query("battery")).await;
        assert_eq!(1, transport.token_calls(), "尚在余量之外，不该重取");

        // 剩余 49s（< TOKEN_REFRESH_MARGIN）→ **提前**刷新（绝不卡到 0 点撞 401）
        clock.advance_secs(50);
        p.search(en_query("battery")).await;
        assert_eq!(2, transport.token_calls(), "临近到期必须提前重取");
        let gets = transport.authorizations(EpoMethod::Get);
        assert_eq!(3, gets.len());
        assert_eq!("Bearer TOK-B", gets[2].as_str(), "{gets:?}");
    }

    #[tokio::test]
    async fn token_request_is_basic_auth_form_post() {
        let (p, transport, _clock) = provider(
            vec![token_reply("TOK-A", "1199")],
            vec![reply(200, EMPTY_SEARCH_REPLY)],
        );
        p.search(en_query("battery")).await;
        let posts = transport.requests().into_iter().collect::<Vec<_>>();
        let EpoRequest {
            method,
            url,
            authorization,
            body,
        } = &posts[0];
        assert_eq!(&EpoMethod::Post, method);
        assert_eq!(AUTH_ENDPOINT, url);
        // 期望值是手算的 base64("EPO_KEY:EPO_SECRET")——**不调用生产函数生成期望值**，
        // 否则实现里漏掉冒号这类错就测不出来了。
        assert_eq!("Basic RVBPX0tFWTpFUE9fU0VDUkVU", authorization.as_str());
        assert_eq!(&Some("grant_type=client_credentials".to_string()), body);
    }

    #[tokio::test]
    async fn credential_whitespace_does_not_change_the_basic_header() {
        let (p, transport, _clock) = provider_with(
            "  EPO_KEY ",
            "\tEPO_SECRET\n",
            vec![token_reply("TOK-A", "1199")],
            vec![reply(200, EMPTY_SEARCH_REPLY)],
        );
        assert!(p.has_credentials(), "前后空白不算没配");
        p.search(en_query("battery")).await;
        assert_eq!(
            Some("Basic RVBPX0tFWTpFUE9fU0VDUkVU"),
            transport
                .authorizations(EpoMethod::Post)
                .first()
                .map(|s| s.as_str())
        );
    }

    /// 只靠**外部可见行为**（下一次检索有没有再发 POST）判 TTL 取值，不偷看内部字段。
    async fn token_calls_after(body: Scripted, elapsed_secs: u64) -> usize {
        let (p, transport, clock) = provider(
            vec![body, token_reply("TOK-2", "1199")],
            vec![reply(200, EMPTY_SEARCH_REPLY)],
        );
        p.search(en_query("battery")).await;
        clock.advance_secs(elapsed_secs);
        p.search(en_query("battery")).await;
        transport.token_calls()
    }

    #[tokio::test]
    async fn token_ttl_accepts_string_number_and_garbage() {
        // L3 官方样例：字符串 "1199"
        assert_eq!(
            1,
            token_calls_after(reply(200, DOC_TOKEN_REPLY), 100).await,
            "1199s 的 token 在 100s 后仍可复用"
        );
        assert_eq!(
            2,
            token_calls_after(reply(200, DOC_TOKEN_REPLY), 1150).await,
            "剩余 49s 时必须提前刷新"
        );
        // 数字形态（上游改类型的兜底）
        assert_eq!(
            2,
            token_calls_after(
                token_body(r#"{"access_token":"T","expires_in":1199}"#),
                1150
            )
            .await
        );
        // 畸形/缺失 expires_in → DEFAULT_TOKEN_TTL(900s) 兜底，而不是「永不过期」
        assert_eq!(
            1,
            token_calls_after(
                token_body(r#"{"access_token":"T","expires_in":"soon"}"#),
                800
            )
            .await
        );
        assert_eq!(
            2,
            token_calls_after(token_body(r#"{"access_token":"T"}"#), 850).await,
            "缺 expires_in 也要按兜底 TTL 刷新，绝不能缓存成永久"
        );
    }

    // ── ④ 401/403 → 重取 token → 重试（验收用例 ②） ───────────────────────────

    #[tokio::test]
    async fn http_401_refetches_token_then_succeeds() {
        let (p, transport, clock) = provider(
            vec![
                token_reply("TOK-OLD", "1199"),
                token_reply("TOK-NEW", "1199"),
            ],
            vec![
                reply(401, REAL_401_TOKEN_BODY),
                reply(200, SEARCH_BIBLIO_REPLY),
            ],
        );
        let outcome = p.search(en_query("battery")).await;

        assert_eq!(2, transport.token_calls(), "撞 401 必须重取一次 token");
        assert_eq!(2, transport.search_calls(), "重取后要重试那一发检索");
        let gets = transport.authorizations(EpoMethod::Get);
        assert_eq!("Bearer TOK-OLD", gets[0]);
        assert_eq!("Bearer TOK-NEW", gets[1], "重试必须带**新** token");
        assert_eq!(
            AttemptStatus::Success,
            outcome.attempts[0].status,
            "重试成功后不得留下失败态"
        );
        assert!(outcome.attempts[0].produced_hits());
        assert!(
            clock.recorded().is_empty(),
            "auth 重试不占用退避预算：{:?}",
            clock.recorded()
        );
    }

    #[tokio::test]
    async fn http_403_anonymous_rejection_is_treated_as_stale_token() {
        // L2 实证：过期 token 与压根没带 token 都表现为 403 → 处置一致（重取再试一发）
        let (p, transport, _clock) = provider(
            vec![
                token_reply("TOK-OLD", "1199"),
                token_reply("TOK-NEW", "1199"),
            ],
            vec![
                reply(403, REAL_403_ANONYMOUS_BODY),
                reply(200, EMPTY_SEARCH_REPLY),
            ],
        );
        let outcome = p.search(en_query("battery")).await;
        assert_eq!(2, transport.token_calls());
        assert_eq!(2, transport.search_calls());
        assert_eq!(AttemptStatus::Success, outcome.attempts[0].status);
        assert_eq!(0, outcome.attempts[0].hits);
    }

    #[tokio::test]
    async fn second_auth_failure_reports_auth_without_endless_retry() {
        let (p, transport, clock) = provider(
            vec![token_reply("TOK-1", "1199"), token_reply("TOK-2", "1199")],
            vec![
                reply(401, REAL_401_TOKEN_BODY),
                reply(403, REAL_403_ANONYMOUS_BODY),
            ],
        );
        let outcome = p.search(en_query("battery")).await;
        assert_eq!(2, transport.token_calls());
        assert_eq!(
            2,
            transport.search_calls(),
            "第二次 Auth 立即收手：新 token 仍被拒 = 凭证本身有问题"
        );
        assert!(
            clock.recorded().is_empty(),
            "Auth 不退避，再等也只是白耗配额: {:?}",
            clock.recorded()
        );
        assert_eq!(
            AttemptStatus::Failed(FailKind::Auth),
            outcome.attempts[0].status
        );
        assert!(outcome.results.is_empty());
        let err = outcome.attempts[0].error.clone().expect("auth note");
        assert!(err.contains("鉴权"), "{err}");
        assert!(err.contains(SOURCE_LABEL), "错误文案要带源名: {err}");
    }

    #[tokio::test]
    async fn invalid_credentials_fail_before_any_search_request() {
        let (p, transport, _clock) = provider(
            vec![reply(401, REAL_401_TOKEN_BODY)],
            vec![reply(200, SEARCH_BIBLIO_REPLY)],
        );
        let outcome = p.search(en_query("battery")).await;
        assert_eq!(1, transport.token_calls());
        assert_eq!(
            0,
            transport.search_calls(),
            "凭证无效时一发数据请求都不该发（省配额，也是 L1 的直接结论）"
        );
        assert_eq!(
            AttemptStatus::Failed(FailKind::Auth),
            outcome.attempts[0].status
        );
        let err = outcome.attempts[0].error.clone().expect("note");
        assert!(
            err.contains("ClientId is Invalid"),
            "上游原文要进 error，供 MA5 面板定位: {err}"
        );
    }

    #[tokio::test]
    async fn token_network_fault_is_network_and_sends_no_search() {
        let (p, transport, _clock) = provider(
            vec![fault("connection refused")],
            vec![reply(200, SEARCH_BIBLIO_REPLY)],
        );
        let outcome = p.search(en_query("battery")).await;
        assert_eq!(
            AttemptStatus::Failed(FailKind::Network),
            outcome.attempts[0].status
        );
        assert_eq!(0, transport.search_calls());
        let err = outcome.attempts[0].error.clone().expect("note");
        assert!(err.contains("取 token"), "{err}");
        assert!(err.contains("connection refused"), "{err}");
    }

    #[tokio::test]
    async fn token_response_missing_access_token_is_parse_failure() {
        let (p, transport, _clock) = provider(
            vec![token_body(r#"{"expires_in":"1199","status":"approved"}"#)],
            vec![reply(200, SEARCH_BIBLIO_REPLY)],
        );
        let outcome = p.search(en_query("battery")).await;
        assert_eq!(
            AttemptStatus::Failed(FailKind::Parse),
            outcome.attempts[0].status,
            "缺字段要暴露成 Parse，绝不能猜一个 token 出去"
        );
        assert_eq!(0, transport.search_calls());
    }

    #[tokio::test]
    async fn non_json_token_response_is_parse_failure() {
        let (p, _transport, _clock) = provider(
            vec![reply(200, "<html>not json</html>")],
            vec![reply(200, SEARCH_BIBLIO_REPLY)],
        );
        let outcome = p.search(en_query("battery")).await;
        assert_eq!(
            AttemptStatus::Failed(FailKind::Parse),
            outcome.attempts[0].status
        );
    }

    // ── ⑤ 其余失败分类与退避（spec §1） ───────────────────────────────────────

    #[tokio::test]
    async fn network_fault_retries_once_then_reports_network() {
        let (p, transport, clock) = provider(
            vec![token_reply("TOK", "1199")],
            vec![fault("timeout"), fault("timeout")],
        );
        let outcome = p.search(en_query("battery")).await;
        assert_eq!(1, transport.token_calls(), "重试复用 token");
        assert_eq!(
            MAX_ATTEMPTS,
            transport.search_calls(),
            "spec §1：单次重试 → 共 2 发"
        );
        assert_eq!(vec![BASE_BACKOFF], clock.recorded(), "只在两发之间退避一次");
        assert_eq!(
            AttemptStatus::Failed(FailKind::Network),
            outcome.attempts[0].status
        );
        assert!(outcome.results.is_empty());
    }

    #[tokio::test]
    async fn quota_honours_retry_after_and_recovers() {
        let (p, transport, clock) = provider(
            vec![token_reply("TOK", "1199")],
            vec![
                reply_retry(429, "Too Many Requests", 7),
                reply(200, EMPTY_SEARCH_REPLY),
            ],
        );
        let outcome = p.search(en_query("battery")).await;
        assert_eq!(2, transport.search_calls());
        assert_eq!(
            vec![Duration::from_secs(7)],
            clock.recorded(),
            "上游给了 Retry-After 就用它，不用本地指数值"
        );
        assert_eq!(AttemptStatus::Success, outcome.attempts[0].status);
        assert_eq!(
            1,
            transport.token_calls(),
            "429 不是鉴权问题，不该重取 token"
        );
    }

    #[tokio::test]
    async fn server_error_is_network_and_recovers_after_one_retry() {
        let (p, transport, _clock) = provider(
            vec![token_reply("TOK", "1199")],
            vec![
                reply(503, "Service Unavailable"),
                reply(200, SEARCH_BIBLIO_REPLY),
            ],
        );
        let outcome = p.search(en_query("battery")).await;
        assert_eq!(2, transport.search_calls());
        assert_eq!(AttemptStatus::Success, outcome.attempts[0].status);
        assert!(outcome.attempts[0].produced_hits());
    }

    #[tokio::test]
    async fn parse_failure_never_retries() {
        // spec §1：Parse 不降级、也不重试（上游结构变了要暴露成 bug，重试只会掩盖）
        let (p, transport, clock) = provider(
            vec![token_reply("TOK", "1199")],
            vec![reply(200, "<html>OPS is not JSON today</html>")],
        );
        let outcome = p.search(en_query("battery")).await;
        assert_eq!(1, transport.search_calls(), "Parse 不该触发重试");
        assert!(clock.recorded().is_empty(), "Parse 也不退避");
        assert_eq!(
            AttemptStatus::Failed(FailKind::Parse),
            outcome.attempts[0].status
        );
        assert!(outcome.results.is_empty());
    }

    // ── ⑥ 响应 → 域内结构（验收用例 ③） ───────────────────────────────────────

    fn fixture_json() -> serde_json::Value {
        serde_json::from_str(SEARCH_BIBLIO_REPLY).expect("L4/L5 样例必须是合法 JSON")
    }

    fn fixture_rows() -> Vec<serde_json::Value> {
        search_rows(&fixture_json())
    }

    #[test]
    fn fixture_shape_is_what_the_parser_expects() {
        assert_eq!(3, fixture_rows().len(), "官方样例 1 条 + 合成 2 条");
        assert_eq!(
            Some(10000),
            total_result_count(&fixture_json()),
            "total-result-count 是**字符串属性**，必须解析成数"
        );
        let empty: serde_json::Value = serde_json::from_str(EMPTY_SEARCH_REPLY).expect("零命中件");
        assert!(
            search_rows(&empty).is_empty(),
            "零命中 = 空列表，绝不能报成 Parse（spec §1 的两回事）"
        );
        assert_eq!(Some(0), total_result_count(&empty));
        // 完全不是检索响应的 JSON：空列表 + 无 total（不报错）
        let junk: serde_json::Value =
            serde_json::from_str("{\"error\":{\"code\":\"500\"}}").expect("json");
        assert!(search_rows(&junk).is_empty());
        assert_eq!(None, total_result_count(&junk));
    }

    /// 验收用例 ③（golden）：**官方文档样例**那条命中逐字段核对。
    #[test]
    fn golden_documentation_sample_maps_field_by_field() {
        let doc = fixture_rows().remove(0);
        let p = epo_to_patent(&doc);
        assert_eq!(
            "KR20100130646A", p.patent_number,
            "epodoc 号已含 kind → 绝不重复追加"
        );
        assert_eq!(
            "INJECTION MOLDED MICROLENSES FOR LED LIGHT EXTRACTION AND CONTROL AND METHOD FOR MANUFACTURING THE SAME",
            p.title
        );
        assert!(
            p.abstract_text.is_empty(),
            "官方样例本就没有摘要 → 空摘要不是丢条理由"
        );
        assert_eq!("LG PHILIPS LIGHTING CO., LTD. [KR]", p.applicant);
        assert_eq!("KOO, JAE-HYUN", p.inventor);
        assert_eq!(
            "2006-07-20", p.filing_date,
            "application-reference 的申请日"
        );
        assert_eq!(
            "2010-12-13", p.publication_date,
            "publication-reference 的公开日 —— 与申请日**不得串台**（对象键是字典序，见 find_key 注释）"
        );
        assert_eq!("", p.priority_date, "官方样例无优先权 → 空串，不猜");
        assert_eq!("KR", p.country);
        assert_eq!("A", p.kind_code);
        assert_eq!(
            Some("37717388".to_string()),
            p.family_id,
            "family-id 在样例里是**属性**"
        );
        assert_eq!(SOURCE_LABEL, p.source);
        assert!(!p.id.is_empty());
        // AGENTS.md 2.5：入库原文必须完整，不能被展示用的截断串污染
        let round_trip: serde_json::Value =
            serde_json::from_str(&p.raw_json).expect("raw_json 完整可解析");
        assert_eq!(doc, round_trip, "raw_json 要等于**整条** exchange-document");
    }

    #[test]
    fn constructed_document_covers_multi_value_branches() {
        let doc = fixture_rows().remove(1);
        let p = epo_to_patent(&doc);
        assert_eq!(
            "EP3445287B1", p.patent_number,
            "epodoc 号不含 kind → 由 @kind 补齐"
        );
        assert_eq!(
            "SOLID ELECTROLYTE COMPOSITION AND ALL-SOLID-STATE BATTERY USING SAME", p.title,
            "德语标题排在数组**前面**，仍必须选 @lang=en（多语种只有一份时取首个）"
        );
        assert_eq!(
            "The present invention provides a solid electrolyte composition which is suitable for use in an all-solid-state battery. \
             The all-solid-state battery exhibits improved cycle life and high-rate discharge characteristics.",
            p.abstract_text,
            "多段 <p> 按顺序拼成一句"
        );
        assert_eq!(
            "UMEDA, YOSUKE, SAITO, TAKESHI", p.inventor,
            "同一人的 epodoc/original 两份写法只留一次"
        );
        assert_eq!("TOYOTA JIDOSHA KABUSHIKI KAISHA [JP]", p.applicant);
        assert_eq!("2015-07-07", p.filing_date);
        assert_eq!("2018-12-26", p.publication_date);
        assert_eq!("2015-07-14", p.priority_date, "priority-claims 里的日期");
        assert_eq!("EP", p.country);
        assert_eq!(Some("63039804".to_string()), p.family_id);
    }

    #[test]
    fn minimal_document_falls_back_to_attributes_and_single_objects() {
        let doc = fixture_rows().remove(2);
        let p = epo_to_patent(&doc);
        assert_eq!(
            "JP2018123456A", p.patent_number,
            "无 epodoc 形态时 = 国家 + 号码 + kind"
        );
        assert_eq!("BATTERY CONTROL DEVICE AND BATTERY SYSTEM", p.title);
        assert_eq!(
            "A battery control device for a secondary battery pack.", p.abstract_text,
            "`p` 是对象而非数组也要能吃下（单/多态）"
        );
        assert_eq!("HITACHI ASTEMO, LTD. [JP]", p.applicant);
        assert_eq!(
            "TANAKA, KENJI", p.inventor,
            "`inventor` 是对象而不是数组也要能吃下"
        );
        assert_eq!(
            "", p.filing_date,
            "缺 application-reference → 空串，绝不拿公开日冒充"
        );
        assert_eq!("2018-09-07", p.publication_date);
        assert_eq!("", p.priority_date);
        assert_eq!("JP", p.country);
    }

    #[test]
    fn malformed_documents_degrade_to_empty_fields_and_never_panic() {
        for raw in [
            "{}",
            "{\"bibliographic-data\":null}",
            "\"not-an-object\"",
            "[1,2,3]",
            "5",
            "null",
            "{\"invention-title\":{\"@lang\":\"en\"}}",
            "{\"abstract\":{\"p\":[{\"$\":\"\"},{\"x\":1}]}}",
        ] {
            let v: serde_json::Value = serde_json::from_str(raw).expect("json literal");
            let p = epo_to_patent(&v); // **绝不 panic**（AGENTS.md 2.1：上游畸形值只降级）
            assert!(p.title.is_empty(), "{raw} → {:?}", p.title);
            assert_eq!(SOURCE_LABEL, p.source);
        }
    }

    #[test]
    fn titleless_rows_are_dropped_by_the_shared_pipeline() {
        // 「空标题丢弃」是 [`rank_and_gate_hits`] 的第 2 步旧行为，三源共用；
        // 这里锁死 EPO 也走同一条，而不是各源自己再写一份。
        let rows = vec![
            serde_json::json!({"@country":"EP","@doc-number":"3445287","@kind":"B1",
                               "bibliographic-data":{"invention-title":[{"@lang":"en","$":"battery"}]}}),
            serde_json::json!({"@country":"EP","@doc-number":"9999999","@kind":"A",
                               "bibliographic-data":{}}),
        ];
        let hits = rank_and_gate_hits(&db(), SOURCE_LABEL, "battery", false, &rows, epo_to_patent);
        assert_eq!(1, hits.len(), "无标题那条被丢掉");
        assert_eq!("battery", hits[0].title);
        assert_eq!("EP3445287B1", hits[0].patent_number);
    }

    #[test]
    fn json_text_and_ops_date_rules() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"dollar":{"$":"  x  "},"arr":["p","q"],"it":{"it":"20181226_nbt.txt"},
                "multi":{"a":1,"b":2},"num":42,"nul":null}"#,
        )
        .expect("json");
        assert_eq!(
            "x",
            json_text(&v["dollar"]),
            "「$」形态是文本，两侧空白去掉"
        );
        assert_eq!("p q", json_text(&v["arr"]), "数组按序拼");
        assert_eq!(
            "20181226_nbt.txt",
            json_text(&v["it"]),
            "真实件里的「it」形态（L5）：单键对象兜底"
        );
        assert_eq!("", json_text(&v["multi"]), "多键对象不猜哪个是文本");
        assert_eq!("42", json_text(&v["num"]));
        assert_eq!("", json_text(&v["nul"]));
        assert_eq!("", json_text(&v["missing"]));

        assert_eq!("2010-12-13", format_ops_date("20101213"));
        assert_eq!("2010-12-13", format_ops_date(" 20101213 "));
        assert_eq!(
            "2018-12-26",
            format_ops_date("2018/12/26"),
            "已是 ISO 也一并规整"
        );
        assert_eq!(
            "EP-1",
            format_ops_date("EP-1"),
            "不是 8 位数字就不动它（不猜语义）"
        );
        assert_eq!("", format_ops_date(""));
    }

    // ── ⑦ provider 级结果：命中 / 零命中 / limit / 中文语境 ────────────────────

    #[tokio::test]
    async fn fixture_rows_become_summaries_through_the_shared_ranking_pipeline() {
        let (p, transport, _clock) = provider(
            vec![token_reply("TOK", "1199")],
            vec![reply(200, SEARCH_BIBLIO_REPLY)],
        );
        let outcome = p.search(en_query("battery")).await;
        assert_eq!(1, transport.search_calls());

        let report = &outcome.attempts[0];
        assert_eq!(SourceKind::EpoOps, report.source);
        assert_eq!(AttemptStatus::Success, report.status);
        assert_eq!(
            Some(10000),
            outcome.upstream_total,
            "上游 total 原样透传，绝不改成结果条数"
        );

        let hits = outcome.summaries();
        assert_eq!(
            2,
            hits.len(),
            "3 条原始命中里，KR 微透镜件与 battery 无关 → 被共用的相关性闸门挡掉"
        );
        assert_eq!("EP3445287B1", hits[0].patent_number);
        assert_eq!(
            "SOLID ELECTROLYTE COMPOSITION AND ALL-SOLID-STATE BATTERY USING SAME",
            hits[0].title
        );
        assert_eq!("EP", hits[0].country);
        assert_eq!("2015-07-07", hits[0].filing_date);
        assert_eq!("JP2018123456A", hits[1].patent_number);
        assert!(
            hits[0].relevance_score.unwrap_or_default()
                >= hits[1].relevance_score.unwrap_or_default(),
            "按分数降序"
        );
        assert!(hits[0]
            .score_source
            .clone()
            .expect("score_source")
            .starts_with("hybrid(pos:"));
        assert!(
            outcome
                .results
                .iter()
                .all(|m| m.sources == vec![SourceKind::EpoOps]),
            "每条都标了本源，为 MA4 的跨源合并留好位"
        );
        assert_eq!("EP3445287", outcome.results[0].key, "去重键 = 规范化公开号");
    }

    #[tokio::test]
    async fn limit_truncates_client_side_without_inventing_upstream_params() {
        let mut q = en_query("battery");
        q.limit = 1;
        let (p, transport, _clock) = provider(
            vec![token_reply("TOK", "1199")],
            vec![reply(200, SEARCH_BIBLIO_REPLY)],
        );
        let outcome = p.search(q).await;
        assert_eq!(1, outcome.results.len(), "limit=1 → 只留 1 条");
        assert_eq!(
            Some(10000),
            outcome.upstream_total,
            "上游 total 不因客户端截断而变"
        );
        let url = transport
            .search_urls()
            .into_iter()
            .next()
            .expect("一发检索");
        assert!(url.contains("Range=1-1"), "跨度按 limit 收窄: {url}");
        assert!(
            !url.to_lowercase().contains("num=") && !url.to_lowercase().contains("count="),
            "不得臆造条数参数: {url}"
        );
    }

    #[tokio::test]
    async fn zero_hits_are_reported_honestly_as_success() {
        let (p, transport, _clock) = provider(
            vec![token_reply("TOK", "1199")],
            vec![reply(200, EMPTY_SEARCH_REPLY)],
        );
        let outcome = p.search(en_query("nothingmatching")).await;
        assert_eq!(1, transport.search_calls());
        assert_eq!(
            AttemptStatus::Success,
            outcome.attempts[0].status,
            "查无此结果是**成功**，不是失败（否则链路会误判成源故障）"
        );
        assert_eq!(0, report_hits(&outcome));
        assert_eq!(Some(0), outcome.upstream_total);
        assert!(outcome.results.is_empty());
    }

    /// spec §4 的定位：OPS 的 `ti/ab` 只有英文著录数据（L7）。中文语境打英文库零命中是
    /// **本源的真实能力边界**，不在代码里伪装成成功，也不伪造结果：如实 Success + hits=0，
    /// 由 `chain.rs` 按优先级规则继续走（MA4 的本地兜底负责中文）。
    #[tokio::test]
    async fn chinese_query_on_english_fixture_is_success_with_zero_hits() {
        let (p, transport, _clock) = provider(
            vec![token_reply("TOK", "1199")],
            vec![reply(200, SEARCH_BIBLIO_REPLY)],
        );
        let outcome = p.search(query("固态电池")).await;
        assert_eq!(
            1,
            transport.search_calls(),
            "照样发请求：零命中由上游回答，不靠本地臆断"
        );
        assert_eq!(AttemptStatus::Success, outcome.attempts[0].status);
        assert_eq!(0, report_hits(&outcome), "CN 闸门（62 分）挡掉纯英文著录");
        assert!(outcome.results.is_empty());
        assert_eq!(
            Some(10000),
            outcome.upstream_total,
            "上游说有多少条就报多少，不因本地过滤而改数"
        );
    }

    #[test]
    fn chinese_gate_flag_follows_language_only() {
        // 单列一测锁死这条口径：只有**显式** Lang::Chinese 才开中文闸门。
        assert!(epo_wants_chinese_gate(&query("固态电池")));
        for lang in [Some(Lang::English), Some(Lang::All), None] {
            let mut q = query("固态电池");
            q.language = lang;
            assert!(!epo_wants_chinese_gate(&q), "{lang:?} 不该开中文闸门");
        }
    }

    #[tokio::test]
    async fn kind_and_lookup_exact_defaults() {
        let (p, _transport, _clock) = provider(vec![], vec![]);
        assert_eq!(SourceKind::EpoOps, p.kind());
        assert_eq!(
            "epo_ops",
            SourceKind::EpoOps.as_str(),
            "对外 source 字段的稳定串"
        );
        assert_eq!(SOURCE_LABEL, SourceKind::EpoOps.as_str());
        assert!(
            p.lookup_exact("EP3445287".to_string()).await.is_none(),
            "search/biblio 没有 details 形态 → 用 trait 默认实现"
        );
    }

    #[test]
    fn backoff_and_kind_labels_stay_aligned_with_the_contract() {
        assert_eq!(BASE_BACKOFF, backoff_for(1));
        assert_eq!(BASE_BACKOFF * 2, backoff_for(2));
        assert_eq!(BASE_BACKOFF * 4, backoff_for(3));
        let labels: Vec<&str> = [
            FailKind::Network,
            FailKind::Quota,
            FailKind::Auth,
            FailKind::Parse,
        ]
        .iter()
        .copied()
        .map(kind_label)
        .collect();
        assert_eq!(
            4,
            labels
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            "{labels:?}"
        );
        for l in &labels {
            assert!(!l.is_empty());
        }
    }

    fn report_hits(outcome: &SearchOutcome) -> usize {
        outcome.attempts[0].hits
    }
}
