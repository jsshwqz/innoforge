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
//! 4. ~~**未做**：跨源结果拼接（`MergedPatent.key` 的合并消费）属 MA4 剩余项~~
//!    **MA6c 已裁决：不做跨源拼接（2026-09-28）**。执行链按登记顺序择单一胜者、胜出源整份
//!    返回（`chain.rs::resolve_by_precedence`），架构上没有第二个源的输入可供拼接，
//!    该合并键因此始终只有测试在读、零生产消费点，`MergedPatent.key` 已随之删除；
//!    去重能力由 `merge::dedup_patent_summaries` 在真正需要它的本地 `/api/search` 路径提供。
//!    依据与销账记录见规格书 §6 与 `crate::search::model::MergedPatent` 文档。
//!    本源侧保留的真实锁是「著录公开号规范化后等于 `EP3445287`」（见 `tests` 内断言）。
//!    ~~熔断冷却（`FailKind::cools_down` 消费）属 MA5/MA6~~
//!    **MA6a 已落地**：本源 Quota 300s / Auth 900s 的源级冷却由
//!    [`crate::search::breaker`] 在 `routes/search.rs` 链前/链后接线消费；
//!    本源内的 `Retry-After` 单次退避保持不变（源内重试优先上游值，冷却是重试后的兜底窗口）。

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
                .connect_timeout(Duration::from_secs(3))
                .timeout(UPSTREAM_TIMEOUT)
                .build()
                .expect("上游 HTTP 客户端构建失败（TLS 后端初始化）——禁止静默退回无超时客户端，见 issue #55"),
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

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 标准化专利法律状态（Batch 11 / v0.9.54）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LegalStatus {
    Active,
    Pending,
    Expired,
    Rejected,
    #[default]
    Unknown,
}

impl LegalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Pending => "Pending",
            Self::Expired => "Expired",
            Self::Rejected => "Rejected",
            Self::Unknown => "Unknown",
        }
    }

    pub fn from_str_lossy(raw: &str) -> Self {
        let lower = raw.trim().to_ascii_lowercase();
        if lower.is_empty() || lower == "unknown" || lower.contains("未知") {
            return Self::Unknown;
        }
        if lower.contains("rejected")
            || lower.contains("withdrawn")
            || lower.contains("abandoned")
            || lower.contains("驳回")
            || lower.contains("撤回")
            || lower.contains("视撤")
            || lower.contains("无效")
            || lower.contains("放弃")
            || lower.contains("revoked")
            || lower.contains("invalid")
        {
            return Self::Rejected;
        }
        if lower.contains("expired")
            || lower.contains("expiration")
            || lower.contains("lapsed")
            || lower.contains("cease")
            || lower.contains("nonpayment")
            || lower.contains("non-payment")
            || lower.contains("termination")
            || lower.contains("失效")
            || lower.contains("终止")
            || lower.contains("期满")
            || lower.contains("过期")
        {
            return Self::Expired;
        }
        if lower.contains("active")
            || lower.contains("granted")
            || lower.contains("grant")
            || lower.contains("valid")
            || lower.contains("reinstated")
            || lower.contains("有效")
            || lower.contains("授权")
            || lower.contains("恢复")
        {
            return Self::Active;
        }
        if lower.contains("pending")
            || lower.contains("exam")
            || lower.contains("published")
            || lower.contains("publication")
            || lower.contains("审")
            || lower.contains("公开")
        {
            return Self::Pending;
        }
        Self::Unknown
    }

    pub fn badge_color(&self) -> &'static str {
        match self {
            Self::Active => "red",
            Self::Expired | Self::Rejected => "green",
            Self::Pending => "amber",
            Self::Unknown => "gray",
        }
    }
}

/// EPO OPS 法律状态与同族引文解析结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PatentLegalAndCitations {
    pub patent_number: String,
    pub legal_status: LegalStatus,
    pub legal_status_text: String,
    pub family_id: Option<String>,
    pub cited_patents: Vec<String>,
    pub citing_patents: Vec<String>,
    pub source: String,
}

fn extract_epo_doc_number(doc_id: &Value) -> Option<String> {
    let country = doc_id
        .get("country")
        .and_then(|v| v.get("$"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let num = doc_id
        .get("doc-number")
        .and_then(|v| v.get("$"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if num.is_empty() {
        return None;
    }
    let kind = doc_id
        .get("kind")
        .and_then(|v| v.get("$"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    Some(format!("{country}{num}{kind}"))
}

fn push_unique_citation(out: &mut Vec<String>, candidate: String) {
    let trimmed = candidate.trim().to_string();
    if !trimmed.is_empty() && !out.iter().any(|x| x.eq_ignore_ascii_case(&trimmed)) {
        out.push(trimmed);
    }
}

fn collect_citations_from_node(node: &Value, out: &mut Vec<String>) {
    match node {
        Value::Array(items) => {
            for item in items {
                collect_citations_from_node(item, out);
            }
        }
        Value::Object(map) => {
            if let Some(patcit) = map.get("patcit") {
                collect_citations_from_node(patcit, out);
            }
            if let Some(doc_id) = map.get("document-id") {
                for item in as_array(doc_id) {
                    if let Some(pn) = extract_epo_doc_number(item) {
                        push_unique_citation(out, pn);
                    }
                }
            }
            if let Some(pn_str) = map.get("patent_number").and_then(|v| v.as_str()) {
                push_unique_citation(out, pn_str.to_string());
            }
            if let Some(pn_str) = map.get("$").and_then(|v| v.as_str()) {
                push_unique_citation(out, pn_str.to_string());
            }
        }
        Value::String(s) => push_unique_citation(out, s.clone()),
        _ => {}
    }
}

fn classify_epo_legal_code(code: &str, desc: &str) -> LegalStatus {
    let combined = format!("{code} {desc}").to_ascii_uppercase();
    if combined.contains("LAPSE")
        || combined.contains("EXPIRED")
        || combined.contains("EXPIR")
        || combined.contains("CEASE")
        || combined.contains("TERM")
    {
        return LegalStatus::Expired;
    }
    if combined.contains("REVOK")
        || combined.contains("WITHD")
        || combined.contains("REFUS")
        || combined.contains("REJECT")
        || combined.contains("INVALID")
    {
        return LegalStatus::Rejected;
    }
    if combined.contains("GRANT")
        || combined.contains("PGFP")
        || combined.contains("ACTIVE")
        || code.trim().eq_ignore_ascii_case("L")
        || code.trim().eq_ignore_ascii_case("R")
    {
        return LegalStatus::Active;
    }
    if combined.contains("EXAM") || combined.contains("REQ") || combined.contains("PUB") {
        return LegalStatus::Pending;
    }
    LegalStatus::from_str_lossy(&format!("{code} {desc}"))
}

/// 从 EPO OPS biblio/legal JSON 提取标准化法律状态、同族 ID 与前后向引文。
pub fn parse_epo_ops_legal_and_citations(
    patent_number: &str,
    json: &Value,
) -> PatentLegalAndCitations {
    let mut out = PatentLegalAndCitations {
        patent_number: patent_number.trim().to_string(),
        source: "epo_ops".to_string(),
        ..Default::default()
    };

    let world = json.get("ops:world-patent-data").unwrap_or(json);

    // 1) 提取 family-id 与引文（references-cited / cited-by）
    if let Some(docs) = world
        .get("exchange-documents")
        .and_then(|v| v.get("exchange-document"))
    {
        for doc in as_array(docs) {
            if out.family_id.is_none() {
                out.family_id = doc
                    .get("@family-id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
            }
            if let Some(bib) = doc.get("bibliographic-data") {
                if let Some(refs) = bib.get("references-cited").and_then(|v| v.get("citation")) {
                    collect_citations_from_node(refs, &mut out.cited_patents);
                }
                if let Some(cited_by) = bib.get("cited-by").or_else(|| bib.get("citing-patents")) {
                    collect_citations_from_node(cited_by, &mut out.citing_patents);
                }
            }
        }
    }

    // 2) 提取 ops:legal 法律状态事件（取最后一条有效事件）
    if let Some(legal_arr) = world
        .get("ops:patent-family")
        .and_then(|v| v.get("ops:family-member"))
        .or_else(|| world.get("ops:legal"))
    {
        for member in as_array(legal_arr) {
            let events = member.get("ops:legal").unwrap_or(member);
            for ev in as_array(events) {
                let code = ev
                    .get("@code")
                    .and_then(|v| v.as_str())
                    .or_else(|| ev.get("code").and_then(|v| v.as_str()))
                    .unwrap_or("");
                let desc = ev
                    .get("ops:L500EP")
                    .and_then(|v| v.get("$"))
                    .and_then(|v| v.as_str())
                    .or_else(|| ev.get("desc").and_then(|v| v.as_str()))
                    .unwrap_or("");
                let parsed = classify_epo_legal_code(code, desc);
                if parsed != LegalStatus::Unknown {
                    out.legal_status = parsed;
                    out.legal_status_text = if desc.is_empty() {
                        parsed.as_str().to_string()
                    } else {
                        desc.trim().to_string()
                    };
                }
            }
        }
    }

    if out.legal_status_text.is_empty() {
        out.legal_status_text = out.legal_status.as_str().to_string();
    }
    out
}

/// 在线拉取 EPO OPS 法律状态与同族引文；无 EPO 凭据或网络失败时平稳回退为 `Unknown`。
pub async fn fetch_epo_legal_and_citations(
    patent_number: &str,
    consumer_key: &str,
    consumer_secret: &str,
) -> PatentLegalAndCitations {
    if consumer_key.trim().is_empty() || consumer_secret.trim().is_empty() {
        return PatentLegalAndCitations {
            patent_number: patent_number.trim().to_string(),
            legal_status: LegalStatus::Unknown,
            legal_status_text: "Unknown".to_string(),
            source: "none (missing_epo_credentials)".to_string(),
            ..Default::default()
        };
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return PatentLegalAndCitations {
                patent_number: patent_number.trim().to_string(),
                legal_status: LegalStatus::Unknown,
                legal_status_text: "Unknown".to_string(),
                source: "epo_ops_error".to_string(),
                ..Default::default()
            };
        }
    };

    use base64::Engine;
    let basic = base64::engine::general_purpose::STANDARD.encode(format!(
        "{}:{}",
        consumer_key.trim(),
        consumer_secret.trim()
    ));
    let token_resp = client
        .post("https://ops.epo.org/3.2/auth/accesstoken")
        .header("Authorization", format!("Basic {basic}"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body("grant_type=client_credentials")
        .send()
        .await;

    let access_token = match token_resp {
        Ok(r) if r.status().is_success() => r.json::<Value>().await.ok().and_then(|v| {
            v.get("access_token")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
        }),
        _ => None,
    };

    let Some(token) = access_token else {
        return PatentLegalAndCitations {
            patent_number: patent_number.trim().to_string(),
            legal_status: LegalStatus::Unknown,
            legal_status_text: "Unknown".to_string(),
            source: "epo_ops_auth_failed".to_string(),
            ..Default::default()
        };
    };

    let pn_clean: String = patent_number
        .trim()
        .to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let biblio_url = format!(
        "https://ops.epo.org/3.2/rest-services/published-data/publication/epodoc/{}/biblio",
        urlencoding::encode(&pn_clean)
    );
    let mut merged = PatentLegalAndCitations {
        patent_number: patent_number.trim().to_string(),
        legal_status: LegalStatus::Unknown,
        legal_status_text: "Unknown".to_string(),
        source: "epo_ops".to_string(),
        ..Default::default()
    };

    if let Ok(resp) = client
        .get(&biblio_url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/json")
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(json) = resp.json::<Value>().await {
                merged = parse_epo_ops_legal_and_citations(patent_number, &json);
            }
        }
    }

    let legal_url = format!(
        "https://ops.epo.org/3.2/rest-services/legal/publication/epodoc/{}",
        urlencoding::encode(&pn_clean)
    );
    if let Ok(resp) = client
        .get(&legal_url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/json")
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(json) = resp.json::<Value>().await {
                let legal_parsed = parse_epo_ops_legal_and_citations(patent_number, &json);
                if legal_parsed.legal_status != LegalStatus::Unknown {
                    merged.legal_status = legal_parsed.legal_status;
                    merged.legal_status_text = legal_parsed.legal_status_text;
                }
            }
        }
    }

    merged
}

#[cfg(test)]
mod legal_citation_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_epo_biblio_and_legal_status_correctly() {
        let mock = json!({
            "ops:world-patent-data": {
                "exchange-documents": {
                    "exchange-document": {
                        "@family-id": "44556677",
                        "bibliographic-data": {
                            "references-cited": {
                                "citation": [
                                    { "patcit": { "document-id": { "country": {"$":"US"}, "doc-number": {"$":"9000001"}, "kind": {"$":"B2"} } } },
                                    { "patcit": { "document-id": { "country": {"$":"EP"}, "doc-number": {"$":"1234567"}, "kind": {"$":"A1"} } } }
                                ]
                            },
                            "cited-by": ["CN110000001A"]
                        }
                    }
                },
                "ops:legal": [
                    { "@code": "PGFP", "ops:L500EP": { "$": "FEE PAYMENT YEAR 05" } },
                    { "@code": "LAPSE", "ops:L500EP": { "$": "LAPSE FOR NON-PAYMENT" } }
                ]
            }
        });
        let res = parse_epo_ops_legal_and_citations("EP3445287A1", &mock);
        assert_eq!(res.family_id.as_deref(), Some("44556677"));
        assert_eq!(res.legal_status, LegalStatus::Expired);
        assert_eq!(res.legal_status.badge_color(), "green");
        assert_eq!(res.cited_patents, vec!["US9000001B2", "EP1234567A1"]);
        assert_eq!(res.citing_patents, vec!["CN110000001A"]);
    }

    #[tokio::test]
    async fn fetch_epo_legal_gracefully_falls_back_without_credentials() {
        let res = fetch_epo_legal_and_citations("CN111111111A", "", "").await;
        assert_eq!(res.legal_status, LegalStatus::Unknown);
        assert_eq!(res.legal_status.as_str(), "Unknown");
        assert!(res.source.contains("missing_epo_credentials"));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 测试（全部离线：假传输 + 假时钟，零网络。`pub(crate)` 是为了让 `chain.rs` 的
// 降级链用例复用同一套假件 —— 与 `providers/google_patents_xhr.rs::tests` 同构。）
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "epo_ops_tests.rs"]
pub(crate) mod tests;
