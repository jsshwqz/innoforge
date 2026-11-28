//! OpenAlex 学术文献源 / academic works search（免费无 Key，Batch 1 任务 1.5）
//!
//! ## 定位
//!
//! 在线检索链的**学术期刊补充源**：专利三源（SerpAPI / Google Patents XHR / EPO OPS）
//! 都拿不到内容时，OpenAlex 提供 2.5 亿+ 篇学术文献（期刊论文、会议论文、预印本）
//! 的著录与摘要检索——对「技术调研 / 现有技术线索」场景是专利库之外的重要证据面。
//! 完全免费、无需 Key（OpenAlex 官方政策：匿名配额 100k 次/天、10 次/秒，远超本工具
//! 用量），故无需 Google Patents XHR 那套全局最小间隔 + 抖动限速；保留「独立超时 +
//! 单次重试」（spec §1）与 `FailKind` 记账。
//!
//! ## 端点（2026-11-16 实测，样本存仓库外 `D:\Temp\openalex-sample.json`）
//!
//! `GET https://api.openalex.org/works?search={关键词}&per-page=10`
//!
//! 响应：`{ "meta": { "count": 444397, ... }, "results": [ { "id", "display_name",
//! "doi": "https://doi.org/10.1002/aenm.202301886", "publication_year": 2023,
//! "publication_date": "2023-10-09", "type": "article", "language": "en",
//! "primary_location": { "source": { "display_name": "Advanced Energy Materials" } },
//! "authorships": [ { "author": { "display_name": ... } } ],
//! "abstract_inverted_index": { "Abstract": [0], "Solid‐state": [1], ... } } ] }`
//!
//! ## 字段映射（任务书：提取标题、期刊名、年份与真实 doi）
//!
//! - `display_name` → `Patent.title`（缺则回退 `title` 键）
//! - `primary_location.source.display_name` → `Patent.applicant`（期刊名；预印本等
//!   无期刊记录为空串）
//! - `publication_year` → `Patent.filing_date`（年份字符串；缺则回退
//!   `publication_date` 前 4 位）
//! - `doi` → 去 `https://doi.org/` 前缀后的裸 DOI（"10.xxxx/yyy"）→
//!   `Patent.patent_number`。⚠️ 如实标注：这不是专利号，结果卡把它当编号展示——
//!   该字段在 openalex 结果里承担「唯一标识」语义，source 标记 `openalex`，
//!   用户可辨别这是文献而非专利著录。
//! - `abstract_inverted_index` → 倒排还原为摘要正文（见
//!   [`abstract_from_inverted_index`]）
//! - `authorships[].author.display_name` → `Patent.inventor`（前 5 位作者逗号连接）
//! - `country` 置空（文献无专利国家码语义）
//!
//! ## 已知边界（如实标注，不过度承诺）
//!
//! 1. **中文查询命中弱**：OpenAlex 索引以英文著录为主，中文关键词与英文标题/摘要
//!    无字面重叠时，会被共用的 [`rank_and_gate_hits`] 相关性门控如实过滤——这是数据
//!    现实（与 EPO OPS 同类边界），不是本源 bug；英文/含英文术语的查询命中良好。
//! 2. **无全文**：OpenAlex 只返回著录 + 摘要（`has_fulltext` 仅表示该库有全文索引，
//!    不提供正文下载）；下游 RAG 切片只吃摘要。
//! 3. **无翻页**：与 Google Patents XHR 同口径，`page > 1` 直接 Skipped（零出网、
//!    不伪造分页；跳过原因进 `error` 不进 `hint`，防泄漏成顶层横幅）。
//! 4. **关键词直传**：OpenAlex 用自然语言全文检索，`render_q` 的专利引擎语法
//!    （`assignee:"…"` / `after:` 等）不适用，禁止串语法——直接下发
//!    `query.keyword`（trim 后一次百分号编码）。

use crate::db::Database;
use crate::patent::Patent;
use crate::search::merge::merged_from;
use crate::search::model::{
    report_excerpt, AttemptReport, AttemptStatus, FailKind, Lang, SearchOutcome, SearchQuery,
    SourceKind,
};
use crate::search::provider::SearchProvider;
use crate::search::relevance::rank_and_gate_hits;
use crate::types::search::PatentSummary;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

/// 端点（官方文档；2026-11-16 实测 HTTP 200）。
const API_ENDPOINT: &str = "https://api.openalex.org/works";

/// 单次上游请求超时（spec §1 的 15s 口径，与其他免费源一致）。
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(15);

/// 每页条数（任务书固定 10）。
pub const PER_PAGE: usize = 10;

/// 上游总尝试次数（spec §1「独立超时 + 单次重试」→ 首发 + 1 次重试 = 2）。
pub const MAX_ATTEMPTS: usize = 2;

/// 首次失败后的退避基数（指数：base, base*2 …）。
const BASE_BACKOFF: Duration = Duration::from_secs(2);

/// 本源的固定标识，进 `AttemptReport`、日志与 `Patent.source`。
const SOURCE_LABEL: &str = "openalex";

/// 作者列表最多保留人数（避免超长作者串挤占结果卡）。
const MAX_AUTHORS: usize = 5;

/// 一次 HTTP 往返的结果。
#[derive(Debug, Clone)]
pub struct OpenAlexReply {
    pub status: u16,
    pub body: String,
}

/// 传输层故障（只有「没能拿到响应」才需要与 `OpenAlexReply` 区分）。
#[derive(Debug, Clone)]
pub enum OpenAlexFault {
    /// 超时 / 连接失败 / DNS
    Network(String),
}

/// HTTP 传输抽象 —— 测试的注入点（离线，不出网）。
pub trait OpenAlexTransport: Send + Sync {
    fn fetch(
        &self,
        url: String,
    ) -> Pin<Box<dyn Future<Output = Result<OpenAlexReply, OpenAlexFault>> + Send + '_>>;
}

/// 生产传输：reqwest + 15s 超时 + 3s 连接超时（防跨境/代理 TLS 握手 >500ms 误杀）。
pub struct ReqwestOpenAlexTransport {
    client: reqwest::Client,
}

impl ReqwestOpenAlexTransport {
    pub fn new() -> Self {
        ReqwestOpenAlexTransport {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(3))
                .timeout(UPSTREAM_TIMEOUT)
                .build()
                .expect("上游 HTTP 客户端构建失败（TLS 后端初始化）——禁止静默退回无超时客户端，见 issue #55"),
        }
    }
}

impl Default for ReqwestOpenAlexTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenAlexTransport for ReqwestOpenAlexTransport {
    fn fetch(
        &self,
        url: String,
    ) -> Pin<Box<dyn Future<Output = Result<OpenAlexReply, OpenAlexFault>> + Send + '_>> {
        Box::pin(async move {
            match self.client.get(&url).send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    match resp.text().await {
                        Ok(body) => Ok(OpenAlexReply { status, body }),
                        Err(e) => Err(OpenAlexFault::Network(format!("读取响应体失败: {e}"))),
                    }
                }
                Err(e) => Err(OpenAlexFault::Network(format!("请求未发出/未完成: {e}"))),
            }
        })
    }
}

/// OpenAlex 学术文献源。
pub struct OpenAlexProvider {
    db: Arc<Database>,
    transport: Arc<dyn OpenAlexTransport>,
}

impl OpenAlexProvider {
    /// 生产构造：真实 reqwest 传输。
    pub fn new(db: Arc<Database>) -> Self {
        OpenAlexProvider {
            db,
            transport: Arc::new(ReqwestOpenAlexTransport::new()),
        }
    }

    /// 测试构造：注入假传输（离线，不出网）。
    pub fn with_transport(db: Arc<Database>, transport: Arc<dyn OpenAlexTransport>) -> Self {
        OpenAlexProvider { db, transport }
    }

    /// 完整请求 URL：`?search={关键词}&per-page=10`。
    ///
    /// 关键词**直传**（见模块头「已知边界 4」）；日期/国家/语言等专利特有过滤器
    /// 不下发（OpenAlex 的过滤语法不同——`from_publication_date` 等——语义也对不上
    /// 专利申请日）；`sort_by`/`page` 同理不下发（本源不支持翻页）。
    pub(crate) fn request_url(query: &SearchQuery) -> String {
        let keyword = query.keyword.trim();
        if keyword.is_empty() {
            return format!("{}?per-page={}", API_ENDPOINT, PER_PAGE);
        }
        format!(
            "{}?search={}&per-page={}",
            API_ENDPOINT,
            urlencoding::encode(keyword),
            PER_PAGE
        )
    }

    /// 一发请求 + 失败分类。
    async fn attempt_fetch(&self, url: &str) -> Result<OpenAlexReply, (FailKind, String)> {
        match self.transport.fetch(url.to_string()).await {
            Ok(reply) => {
                if reply.status >= 400 {
                    let kind = FailKind::for_http_status(reply.status);
                    return Err((
                        kind,
                        format!(
                            "{SOURCE_LABEL} 上游返回 HTTP {}（{}）：{}",
                            reply.status,
                            kind_label(kind),
                            report_excerpt(&reply.body)
                        ),
                    ));
                }
                Ok(reply)
            }
            Err(OpenAlexFault::Network(msg)) => {
                Err((FailKind::Network, format!("{SOURCE_LABEL} 网络故障：{msg}")))
            }
        }
    }

    /// 带单次重试的取页循环（spec §1「独立超时 + 单次重试」）。
    /// 返回 `(结果行数组, 上游总数 meta.count)`。
    async fn fetch_rows(
        &self,
        url: &str,
    ) -> Result<(Vec<serde_json::Value>, Option<usize>), (FailKind, String)> {
        let mut attempt = 0usize;
        loop {
            attempt += 1;
            match self.attempt_fetch(url).await {
                Ok(reply) => match serde_json::from_str::<serde_json::Value>(&reply.body) {
                    Ok(json) => {
                        let rows = json["results"].as_array().cloned().unwrap_or_default();
                        let total = json["meta"]["count"].as_u64().map(|v| v as usize);
                        return Ok((rows, total));
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
                },
                Err((kind, msg)) => {
                    // spec §1：Parse 不重试；其余（Network/Quota）退避后重试一次。
                    if !kind.switches_source() || attempt >= MAX_ATTEMPTS {
                        return Err((kind, msg));
                    }
                    let backoff = backoff_for(attempt);
                    println!("[ONLINE] {SOURCE_LABEL} {kind:?} → 退避 {backoff:?} 后重试（第 {attempt} 次失败）");
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    }

    /// 命中行 → `PatentSummary[]`（打分/放行/中文过滤/去重走共用的
    /// [`rank_and_gate_hits`]，避免第二份实现）。
    fn outcome_patents_with_db(
        db: &Database,
        query: &SearchQuery,
        rows: Vec<serde_json::Value>,
    ) -> Vec<PatentSummary> {
        let cn_query = matches!(query.language, Some(Lang::Chinese));
        let keyword = query.keyword.trim();
        let mut patents =
            rank_and_gate_hits(db, SOURCE_LABEL, keyword, cn_query, &rows, work_to_patent);
        if query.limit > 0 && patents.len() > query.limit {
            patents.truncate(query.limit);
        }
        patents
    }
}

/// [`FailKind`] 的中文短标签，只进 `AttemptReport::error` 文案。
fn kind_label(kind: FailKind) -> &'static str {
    match kind {
        FailKind::Network => "网络故障",
        FailKind::Quota => "限流/配额",
        FailKind::Auth => "鉴权",
        FailKind::Parse => "响应结构变化",
    }
}

/// 第 `attempt` 次失败后的退避时长（指数，与 google_patents_xhr 同公式）。
fn backoff_for(attempt: usize) -> Duration {
    let factor = 1u32 << (attempt.saturating_sub(1).min(4)) as u32;
    BASE_BACKOFF.saturating_mul(factor)
}

/// 还原 `abstract_inverted_index` 为摘要正文。
///
/// OpenAlex 把摘要存成 `{词: [出现位置...]}` 的倒排索引——按声明位置放回、空格连接。
/// 位置可能缺位/越界：缺位跳过（如实缺词）、越界丢弃（宁缺不乱序）；
/// 缺键/非对象/空索引返回空串，绝不 panic。
pub fn abstract_from_inverted_index(inv: &serde_json::Value) -> String {
    let Some(map) = inv.as_object() else {
        return String::new();
    };
    let mut total_occ: usize = 0;
    for positions in map.values() {
        if let Some(list) = positions.as_array() {
            for p in list {
                if let Some(idx) = p.as_i64() {
                    if idx >= 0 {
                        total_occ += 1;
                    }
                }
            }
        }
    }
    if total_occ == 0 {
        return String::new();
    }
    let pos_cap = total_occ.saturating_mul(4).clamp(16, 4096);
    let mut max_pos: Option<usize> = None;
    for positions in map.values() {
        if let Some(list) = positions.as_array() {
            for p in list {
                if let Some(idx) = p.as_i64() {
                    if idx >= 0 && (idx as usize) < pos_cap {
                        let u = idx as usize;
                        max_pos = Some(max_pos.map_or(u, |m| m.max(u)));
                    }
                }
            }
        }
    }
    let Some(max_pos) = max_pos else {
        return String::new();
    };
    let mut slots: Vec<&str> = vec![""; max_pos + 1];
    for (word, positions) in map {
        let Some(list) = positions.as_array() else {
            continue;
        };
        for p in list {
            if let Some(idx) = p.as_i64() {
                if idx >= 0 && (idx as usize) < pos_cap {
                    let u = idx as usize;
                    if u < slots.len() && slots[u].is_empty() {
                        slots[u] = word.as_str();
                    }
                }
            }
        }
    }
    let mut out = String::new();
    for word in &slots {
        if word.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// 剥离 DOI 的 `https://doi.org/` 前缀取裸 DOI（"10.xxxx/yyy"）。
/// 已是裸 DOI / 空串原样返回（前缀匹配含 http/doi.org/DOI: 变体）。
pub fn strip_doi_prefix(doi: &str) -> String {
    let doi = doi.trim();
    if let Some(rest) = doi
        .strip_prefix("https://doi.org/")
        .or_else(|| doi.strip_prefix("http://doi.org/"))
        .or_else(|| doi.strip_prefix("doi.org/"))
        .or_else(|| doi.strip_prefix("DOI:"))
    {
        rest.trim().to_string()
    } else {
        doi.to_string()
    }
}

/// 一条 OpenAlex work 记录 → 域内 [`Patent`]。
///
/// 全部容错取值，缺字段只降级为空串，绝不 panic（映射健壮性底线，
/// 与 google_patents_xhr::xhr_to_patent 同纪律）。
fn work_to_patent(r: &serde_json::Value) -> Patent {
    // display_name 是规范标题；title 是后备（多数记录两者同值）。
    let title = r["display_name"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| r["title"].as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let doi = strip_doi_prefix(r["doi"].as_str().unwrap_or(""));

    // 期刊名：primary_location.source.display_name（预印本/书籍章节可能无 → 空串）。
    let journal = r["primary_location"]["source"]["display_name"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();

    // 年份：publication_year 优先（任务书口径），缺则回退 publication_date 前 4 位。
    let year = r["publication_year"]
        .as_i64()
        .map(|y| y.to_string())
        .or_else(|| {
            r["publication_date"].as_str().and_then(|d| {
                let head: Vec<char> = d.chars().take(4).collect();
                if head.len() == 4 && head.iter().all(|c| c.is_ascii_digit()) {
                    Some(head.into_iter().collect())
                } else {
                    None
                }
            })
        })
        .unwrap_or_default();

    // 作者：前 MAX_AUTHORS 位 display_name，逗号连接。
    let mut authors: Vec<String> = Vec::new();
    if let Some(list) = r["authorships"].as_array() {
        for a in list {
            if authors.len() >= MAX_AUTHORS {
                break;
            }
            let name = a["author"]["display_name"].as_str().unwrap_or("").trim();
            if !name.is_empty() {
                authors.push(name.to_string());
            }
        }
    }

    Patent {
        id: uuid::Uuid::new_v4().to_string(),
        patent_number: doi,
        title,
        abstract_text: abstract_from_inverted_index(&r["abstract_inverted_index"]),
        description: String::new(),
        claims: String::new(),
        applicant: journal,
        inventor: authors.join(", "),
        filing_date: year,
        publication_date: r["publication_date"].as_str().unwrap_or("").to_string(),
        grant_date: None,
        ipc_codes: String::new(),
        cpc_codes: String::new(),
        priority_date: String::new(),
        country: String::new(),
        kind_code: String::new(),
        family_id: None,
        legal_status: String::new(),
        citations: "[]".into(),
        cited_by: "[]".into(),
        source: SOURCE_LABEL.into(),
        raw_json: r.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        images: "[]".into(),
        pdf_url: String::new(),
    }
}

impl SearchProvider for OpenAlexProvider {
    fn kind(&self) -> SourceKind {
        SourceKind::OpenAlex
    }

    fn search<'a>(
        &'a self,
        query: SearchQuery,
    ) -> Pin<Box<dyn Future<Output = SearchOutcome> + Send + 'a>> {
        Box::pin(async move {
            let started = std::time::Instant::now();

            // 免费文献源不支持翻页：page > 1 直接 Skipped（零出网），
            // 绝不把第 1 页内容伪装成第 N 页返回（与 Google Patents XHR 同一口径；
            // 跳过原因进 `error` 不进 `hint`，防泄漏成顶层横幅）。
            if query.page > 1 {
                println!(
                    "[ONLINE] {SOURCE_LABEL} page={} skipped (pagination not supported)",
                    query.page
                );
                return SearchOutcome {
                    results: Vec::new(),
                    attempts: vec![AttemptReport {
                        source: SourceKind::OpenAlex,
                        status: AttemptStatus::Skipped,
                        latency_ms: 0,
                        hits: 0,
                        error: Some(format!(
                            "OpenAlex 学术文献源不支持翻页，第 {page} 页请求已跳过，避免返回与第 1 页重复的内容。",
                            page = query.page
                        )),
                        hint: None,
                    }],
                    upstream_total: None,
                };
            }

            let url = Self::request_url(&query);
            println!("[ONLINE] {SOURCE_LABEL} url={}", report_excerpt(&url));

            let (rows, upstream_total, status, error_note) = match self.fetch_rows(&url).await {
                Ok((rows, total)) => (rows, total, AttemptStatus::Success, None),
                Err((kind, msg)) => (Vec::new(), None, AttemptStatus::Failed(kind), Some(msg)),
            };

            let patents = if matches!(status, AttemptStatus::Success) {
                let db = self.db.clone();
                let q = query.clone();
                tokio::task::spawn_blocking(move || Self::outcome_patents_with_db(&db, &q, rows))
                    .await
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let hits = patents.len();

            SearchOutcome {
                results: merged_from(SourceKind::OpenAlex, patents),
                attempts: vec![AttemptReport {
                    source: SourceKind::OpenAlex,
                    status,
                    latency_ms: started.elapsed().as_millis() as u64,
                    hits,
                    error: error_note,
                    hint: None,
                }],
                upstream_total,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-11-16 实测响应第 0 条（著录字段原样；倒排摘要截取前几个词）。
    /// 只用于喂解析器与断言映射，不参与数据完整性声明。
    const REAL_WORK: &str = r#"{
      "id": "https://openalex.org/W4387520111",
      "display_name": "A Roadmap for Solid‐State Batteries",
      "title": "A Roadmap for Solid‐State Batteries",
      "doi": "https://doi.org/10.1002/aenm.202301886",
      "publication_year": 2023,
      "publication_date": "2023-10-09",
      "type": "article",
      "language": "en",
      "primary_location": {
        "source": { "display_name": "Advanced Energy Materials" }
      },
      "authorships": [
        { "author": { "display_name": "Thomas Schmaltz" } },
        { "author": { "display_name": "Maximilian Fichtner" } }
      ],
      "abstract_inverted_index": {
        "Abstract": [0],
        "Solid‐state": [1],
        "batteries": [2, 12],
        "are": [3],
        "considered": [4],
        "key": [5]
      }
    }"#;

    // ── 倒排摘要还原 ─────────────────────────────────────────────────────────────

    #[test]
    fn abstract_inverted_index_restores_word_order() {
        let inv = serde_json::json!({
            "considered": [4],
            "Abstract": [0],
            "Solid‐state": [1],
            "batteries": [2, 12],
            "are": [3]
        });
        let text = abstract_from_inverted_index(&inv);
        assert_eq!(
            "Abstract Solid‐state batteries are considered batteries",
            text
        );
    }

    /// 缺键 / 非对象 / 空索引 / 位置越界一律安全降级，绝不 panic。
    #[test]
    fn abstract_inverted_index_degrades_safely() {
        assert_eq!("", abstract_from_inverted_index(&serde_json::json!(null)));
        assert_eq!("", abstract_from_inverted_index(&serde_json::json!({})));
        assert_eq!(
            "",
            abstract_from_inverted_index(&serde_json::json!("not-a-map"))
        );
        // 越界位置丢弃，不 panic 也不乱序（实索引可能比声明的 max 还大 → 丢弃）
        let inv = serde_json::json!({"word": [0, 99], "two": [1]});
        assert_eq!("word two", abstract_from_inverted_index(&inv));
        // 重复位置：首个先占，不重复插入
        let inv = serde_json::json!({"a": [0], "b": [0, 1]});
        assert_eq!("a b", abstract_from_inverted_index(&inv));
    }

    // ── DOI 前缀剥离 ─────────────────────────────────────────────────────────────

    #[test]
    fn strips_doi_prefix_variants() {
        assert_eq!(
            "10.1002/aenm.202301886",
            strip_doi_prefix("https://doi.org/10.1002/aenm.202301886")
        );
        assert_eq!("10.1/abc", strip_doi_prefix("http://doi.org/10.1/abc"));
        assert_eq!("10.1/abc", strip_doi_prefix("doi.org/10.1/abc"));
        assert_eq!("10.1/abc", strip_doi_prefix("DOI:10.1/abc"));
        // 已是裸 DOI：原样保留
        assert_eq!("10.1/abc", strip_doi_prefix("10.1/abc"));
        assert_eq!("", strip_doi_prefix("   "));
    }

    // ── work → Patent 映射 ───────────────────────────────────────────────────────

    #[test]
    fn maps_real_work_to_patent_fields() {
        let row: serde_json::Value = serde_json::from_str(REAL_WORK).expect("real work");
        let p = work_to_patent(&row);
        assert_eq!("10.1002/aenm.202301886", p.patent_number, "裸 DOI");
        assert_eq!("A Roadmap for Solid‐State Batteries", p.title);
        assert_eq!(
            "Advanced Energy Materials", p.applicant,
            "期刊名进 applicant"
        );
        assert_eq!("2023", p.filing_date, "年份");
        assert_eq!("2023-10-09", p.publication_date);
        assert_eq!("Thomas Schmaltz, Maximilian Fichtner", p.inventor);
        assert!(
            p.abstract_text
                .starts_with("Abstract Solid‐state batteries are considered key"),
            "倒排摘要按位还原: {}",
            p.abstract_text
        );
        assert_eq!("openalex", p.source);
        assert!(p.raw_json.contains("W4387520111"), "原文完整入库");
    }

    /// 缺字段/畸形结构一律降级为空串，不 panic。
    #[test]
    fn missing_fields_degrade_to_empty_strings() {
        let p = work_to_patent(&serde_json::json!({"display_name": "只有标题"}));
        assert_eq!("只有标题", p.title);
        assert_eq!("", p.patent_number);
        assert_eq!("", p.applicant);
        assert_eq!("", p.filing_date);
        // 连 display_name 都没有也不能炸
        assert_eq!("", work_to_patent(&serde_json::json!({})).title);
    }

    /// 无期刊 + 缺 display_name 的预印本形态：title 回退 title 键、年份回退日期前四位。
    #[test]
    fn preprint_fallbacks() {
        let p = work_to_patent(&serde_json::json!({
            "title": "A preprint",
            "publication_date": "2024-03-05"
        }));
        assert_eq!("A preprint", p.title, "display_name 缺失回退 title 键");
        assert_eq!("", p.applicant);
        assert_eq!("2024", p.filing_date, "缺 publication_year 回退日期前四位");
    }

    /// 作者超过 MAX_AUTHORS 截断（防超长作者串）。
    #[test]
    fn authors_truncated_to_max() {
        let many: Vec<serde_json::Value> = (0..10)
            .map(|i| serde_json::json!({"author": {"display_name": format!("Author{i}")}}))
            .collect();
        let p = work_to_patent(&serde_json::json!({
            "display_name": "t",
            "authorships": many
        }));
        assert_eq!(MAX_AUTHORS, p.inventor.split(", ").count());
    }

    // ── 请求 URL 形状 ─────────────────────────────────────────────────────────────

    fn query(keyword: &str) -> SearchQuery {
        SearchQuery {
            keyword: keyword.to_string(),
            country: None,
            language: None,
            assignee: None,
            exact_assignee: false,
            date_from: None,
            date_to: None,
            limit: 10,
            page: 1,
            sort_by: None,
            search_type: None,
        }
    }

    #[test]
    fn request_url_encodes_keyword_once_and_pins_per_page() {
        let url = OpenAlexProvider::request_url(&query("solid state battery"));
        assert!(
            url.starts_with(
                "https://api.openalex.org/works?search=solid%20state%20battery&per-page=10"
            ),
            "{url}"
        );
        assert!(!url.contains("%25"), "出现双重编码: {url}");
    }

    #[test]
    fn request_url_empty_keyword_omits_search_param() {
        let url = OpenAlexProvider::request_url(&query("   "));
        assert_eq!("https://api.openalex.org/works?per-page=10", url);
    }

    // ── 离线搜索链路（假传输） ────────────────────────────────────────────────────

    struct FakeTransport {
        body: String,
        status: u16,
    }
    impl OpenAlexTransport for FakeTransport {
        fn fetch(
            &self,
            _url: String,
        ) -> Pin<Box<dyn Future<Output = Result<OpenAlexReply, OpenAlexFault>> + Send + '_>>
        {
            Box::pin(async move {
                Ok(OpenAlexReply {
                    status: self.status,
                    body: self.body.clone(),
                })
            })
        }
    }

    fn provider(body: &str, status: u16) -> OpenAlexProvider {
        OpenAlexProvider::with_transport(
            Arc::new(Database::init(":memory:").expect("in-memory db")),
            Arc::new(FakeTransport {
                body: body.to_string(),
                status,
            }),
        )
    }

    /// 端到端（离线）：搜索 → 映射 → 打分门控 → 记账，全链可跑且 hits 如实。
    #[tokio::test]
    async fn search_maps_real_reply_end_to_end() {
        let body = format!(r#"{{"meta":{{"count":444397}},"results":[{REAL_WORK}]}}"#);
        let p = provider(&body, 200);
        let outcome = p.search(query("solid state battery roadmap")).await;
        assert!(matches!(outcome.attempts[0].status, AttemptStatus::Success));
        assert_eq!(1, outcome.attempts[0].hits);
        assert_eq!(
            Some(SourceKind::OpenAlex),
            outcome.winning_source(),
            "胜出源必须如实标注"
        );
        let summary = &outcome.results[0].summary;
        assert_eq!("10.1002/aenm.202301886", summary.patent_number);
        assert!(summary.title.contains("Roadmap"));
        assert_eq!("openalex", outcome.results[0].sources[0].as_str());
        assert_eq!(Some(444397), outcome.upstream_total, "meta.count 透传");
    }

    /// HTTP 失败分类（404 → Parse 语义外的公共表：500..=599 才是 Network，
    /// 404 走默认 Parse；401 → Auth）且错误文案带源名。
    #[tokio::test]
    async fn http_failures_classified_and_labeled() {
        let p = provider("not found", 404);
        let outcome = p.search(query("x")).await;
        assert!(matches!(
            outcome.attempts[0].status,
            AttemptStatus::Failed(FailKind::Parse)
        ));
        let err = outcome.attempts[0].error.clone().expect("error note");
        assert!(err.contains("openalex"), "{err}");

        let p = provider("denied", 401);
        let outcome = p.search(query("x")).await;
        assert!(matches!(
            outcome.attempts[0].status,
            AttemptStatus::Failed(FailKind::Auth)
        ));
    }

    /// 2xx 但非 JSON → Parse（不重试不降级）。
    #[tokio::test]
    async fn parse_failure_reports_parse() {
        let p = provider("<html>boom</html>", 200);
        let outcome = p.search(query("x")).await;
        assert!(matches!(
            outcome.attempts[0].status,
            AttemptStatus::Failed(FailKind::Parse)
        ));
        assert!(outcome.results.is_empty());
    }

    /// page > 1 零出网 Skipped，不伪造分页，且不得泄漏成顶层 hint。
    #[tokio::test]
    async fn page_beyond_first_is_skipped() {
        let p = provider(r#"{"meta":{"count":0},"results":[]}"#, 200);
        let mut q = query("x");
        q.page = 2;
        let outcome = p.search(q).await;
        assert_eq!(0, outcome.results.len());
        assert!(matches!(outcome.attempts[0].status, AttemptStatus::Skipped));
        assert_eq!(None, outcome.hint(), "Skipped 不得提升为顶层 hint");
        let reason = outcome.attempts[0].error.as_deref().unwrap_or_default();
        assert!(reason.contains("不支持翻页"), "{reason}");
    }

    /// kind 与 trait 默认 lookup_exact（本源无 details 端点）。
    #[tokio::test]
    async fn kind_and_lookup_exact_defaults() {
        let p = provider(r#"{"results":[]}"#, 200);
        assert_eq!(SourceKind::OpenAlex, p.kind());
        assert_eq!("openalex", SourceKind::OpenAlex.as_str());
        assert!(p.lookup_exact("10.1/x".to_string()).await.is_none());
    }
}
