//! Firecrawl 网页搜索回退源（冻结期后队列 #1，2026-11-08 用户拍板提前启动）。
//!
//! ## 定位：**链后条件回退，不是链上第四源**
//!
//! 在线专利链（SerpAPI → Google Patents XHR → EPO OPS）0 命中时，由
//! `routes/search.rs` 条件调用本源。刻意**不**注册进 [`crate::search::chain::SourceChain`]：
//! 链是「并行发起 + 优先级择胜」，第四源意味着每次搜索都烧一发付费配额；
//! 而本源只在「专利 0 命中」这个分支才该参与。
//!
//! ## 结果的诚实表示（不伪装成专利著录）
//!
//! 网页命中的是网页（title/snippet/url），不是专利记录——包装成
//! [`PatentSummary`](crate::types::search::PatentSummary) 再渲染成专利卡片
//! 是撒谎（申请人/申请日全是空位、点进去 404）。因此本源返回
//! [`FirecrawlWebHit`]，由路由层以**纯新增顶层键 `web_results`** 交付，
//! `patents` 恒为空数组；前端渲染独立「网页搜索结果」区块，外链新窗口打开。
//!
//! ## Prompt 注入边界（AGENTS.md 2.6）
//!
//! 网页内容是外部不可信输入。本源只做「取回 + 透传」；当前回退路径不把
//! 网页正文喂给任何 AI prompt（仅在搜索结果页展示摘要）。未来若要把网页
//! 内容并入 AI 上下文，必须先经 `<user_input>` 式边界隔离，禁止原文拼接。

use crate::search::model::{report_excerpt, FailKind, Lang, SearchQuery};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

/// Firecrawl v1 search 端点（官方 API；key 经请求头 Bearer 传递，不进 URL——
/// 与 SerpAPI 的 query-string 传 key 不同，避免 key 落进任何日志里的 URL）。
pub const SEARCH_ENDPOINT: &str = "https://api.firecrawl.dev/v1/search";

/// 单次回退搜索的条数上限。回退场景是「专利源 0 命中」，网页结果只是线索，
/// 8 条足够指路；配额按 credit 计费，宁可少取。
pub const WEB_RESULT_LIMIT: usize = 8;

/// 上游超时。回退发生在链跑完之后（预算 60s 已被消耗一部分），15s 与
/// 在线源同级，留给本地兜底的时间仍然充足。
pub(crate) const WEB_SEARCH_TIMEOUT_SECS: u64 = 15;

pub struct FirecrawlReply {
    pub status: u16,
    pub body: String,
}

/// 传输层故障（没能拿到响应时与 [`FirecrawlReply`] 区分，供 `FailKind::Network` 归因）。
#[derive(Debug, Clone)]
pub enum FirecrawlFault {
    Network(String),
}

/// HTTP 传输抽象——离线测试的注入点（与 [`crate::search::providers::google_patents_xhr::XhrTransport`]
/// 同一手法）。
pub trait FirecrawlTransport: Send + Sync {
    fn post(
        &self,
        url: String,
        json_body: String,
        api_key: String,
    ) -> Pin<Box<dyn Future<Output = Result<FirecrawlReply, FirecrawlFault>> + Send + '_>>;
}

/// 网页命中。字段刻意保持最小：title / snippet / url，其余（作者、日期）网页
/// 侧没有可信来源，宁缺毋滥。
#[derive(Debug, Clone, PartialEq)]
pub struct FirecrawlWebHit {
    pub title: String,
    pub snippet: String,
    pub url: String,
}

/// 生产传输：reqwest，单请求超时 [`WEB_SEARCH_TIMEOUT_SECS`]。
#[derive(Clone)]
struct ReqwestFirecrawlTransport {
    client: reqwest::Client,
}

impl ReqwestFirecrawlTransport {
    fn new() -> Self {
        // 连接超时 500ms（errors.md #74 同款纪律：探活/外联类请求不允许连接悬挂），
        // 总超时 15s 兜底。
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(500))
            .timeout(Duration::from_secs(WEB_SEARCH_TIMEOUT_SECS))
            .build()
            .expect("上游 HTTP 客户端构建失败（TLS 后端初始化）——禁止静默退回无超时客户端，见 issue #55");
        Self { client }
    }
}

impl FirecrawlTransport for ReqwestFirecrawlTransport {
    fn post(
        &self,
        url: String,
        json_body: String,
        api_key: String,
    ) -> Pin<Box<dyn Future<Output = Result<FirecrawlReply, FirecrawlFault>> + Send + '_>> {
        let client = self.client.clone();
        Box::pin(async move {
            let resp = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .body(json_body)
                .send()
                .await
                .map_err(|e| FirecrawlFault::Network(report_excerpt(&e.to_string())))?;
            let status = resp.status().as_u16();
            let body = resp
                .text()
                .await
                .map_err(|e| FirecrawlFault::Network(report_excerpt(&e.to_string())))?;
            Ok(FirecrawlReply { status, body })
        })
    }
}

/// 供网页搜索使用的查询词：中文语境追加「专利」、英文语境追加 "patent"，
/// 把「专利 0 命中 → 网页搜索」的意图显式带进网页域。
/// 纯函数（含空查询钳制），由 `web_query_for_matches_language` 锁死。
pub fn web_query_for(query: &SearchQuery) -> String {
    let keyword = query.keyword.trim();
    if keyword.is_empty() {
        return String::new();
    }
    match query.language {
        Some(Lang::Chinese) => format!("{} 专利", keyword),
        _ => format!("{} patent", keyword),
    }
}

/// 请求体：`{"query": ..., "limit": 8}`（Firecrawl v1 search 最小参数集）。
/// 键序锁死便于 diff 审查；body 由 [`FirecrawlSearchProvider::request_body`] 生成，
/// 测试断言逐字形状。
pub(crate) fn request_body(web_query: &str, limit: usize) -> String {
    format!(
        "{{\"query\":\"{}\",\"limit\":{}}}",
        json_escape(web_query),
        limit
    )
}

/// 最小 JSON 字符串转义（引号 / 反斜杠 / 控制字符）。手写而非引 serde_json：
/// 转义规则单一、可逐字测试，且避免为一条请求体引入 Value 包装层。
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// 解析 Firecrawl v1 search 响应体 → 网页命中列表。
///
/// 已知形状（官方 API）：`{"success": true, "data": [{"title": .., "description": .., "url": ..}, ..]}`。
/// 解析纪律：缺字段的条目跳过（title/url 全空视为不可用行，不造占位）；解析失败返回
/// `Err`（路由层记 `FailKind::Parse`——上游改版必须暴露，不许静默吞掉）。
pub fn parse_search_response(body: &str) -> Result<Vec<FirecrawlWebHit>, String> {
    let json: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        format!(
            "Firecrawl 响应不是合法 JSON：{}",
            report_excerpt(&e.to_string())
        )
    })?;
    if let Some(err) = json.get("error").and_then(|v| v.as_str()) {
        return Err(format!("Firecrawl 上游报错：{}", report_excerpt(err)));
    }
    let data = json.get("data").and_then(|v| v.as_array()).ok_or_else(|| {
        format!(
            "Firecrawl 响应缺 data 数组（上游可能改版）：{}",
            report_excerpt(body)
        )
    })?;
    let mut hits = Vec::new();
    for item in data {
        let title = item
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let url = item
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if url.is_empty() {
            continue;
        }
        let snippet = item
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        hits.push(FirecrawlWebHit {
            title,
            snippet,
            url,
        });
    }
    Ok(hits)
}

/// Firecrawl 网页搜索回退客户端。
pub struct FirecrawlSearchProvider {
    api_key: String,
    transport: Arc<dyn FirecrawlTransport>,
}

impl FirecrawlSearchProvider {
    pub fn new(api_key: String) -> Self {
        Self::with_transport(api_key, Arc::new(ReqwestFirecrawlTransport::new()))
    }

    pub fn with_transport(api_key: String, transport: Arc<dyn FirecrawlTransport>) -> Self {
        Self { api_key, transport }
    }

    /// 单请求体形状（测试锁用）。
    pub(crate) fn request_body(&self, query: &SearchQuery) -> String {
        request_body(&web_query_for(query), WEB_RESULT_LIMIT)
    }

    /// 执行网页搜索。成功返回命中列表；失败返回 `(FailKind, 技术性原因)`——
    /// 429 → Quota、401/403 → Auth、其余 HTTP → 按状态码归类、传输层故障 → Network、
    /// 响应结构问题 → Parse（上游改版必须暴露）。
    pub async fn web_search(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<FirecrawlWebHit>, (FailKind, String)> {
        let body = self.request_body(query);
        let reply = self
            .transport
            .post(SEARCH_ENDPOINT.to_string(), body, self.api_key.clone())
            .await
            .map_err(|fault| match fault {
                FirecrawlFault::Network(msg) => (
                    FailKind::Network,
                    format!("Firecrawl 传输层故障（Network）：{}", msg),
                ),
            })?;
        if reply.status == 429 {
            return Err((
                FailKind::Quota,
                format!(
                    "Firecrawl 上游返回 HTTP 429（限流/额度受限）：{}",
                    report_excerpt(&reply.body)
                ),
            ));
        }
        if reply.status == 401 || reply.status == 403 {
            return Err((
                FailKind::Auth,
                format!(
                    "Firecrawl 上游返回 HTTP {}（凭证无效）：{}",
                    reply.status,
                    report_excerpt(&reply.body)
                ),
            ));
        }
        if !(200..300).contains(&reply.status) {
            return Err((
                FailKind::Quota,
                format!(
                    "Firecrawl 上游返回 HTTP {}：{}",
                    reply.status,
                    report_excerpt(&reply.body)
                ),
            ));
        }
        parse_search_response(&reply.body).map_err(|msg| (FailKind::Parse, msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 固定回放传输：按序回放预设响应/故障，并记录收到的请求体供断言。
    struct ScriptedTransport {
        script: Mutex<VecDeque<Result<FirecrawlReply, FirecrawlFault>>>,
        seen_bodies: Arc<Mutex<Vec<String>>>,
        seen_keys: Arc<Mutex<Vec<String>>>,
    }

    use std::collections::VecDeque;

    impl ScriptedTransport {
        fn new(script: Vec<Result<FirecrawlReply, FirecrawlFault>>) -> Self {
            Self {
                script: Mutex::new(VecDeque::from(script)),
                seen_bodies: Arc::new(Mutex::new(Vec::new())),
                seen_keys: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl FirecrawlTransport for ScriptedTransport {
        fn post(
            &self,
            _url: String,
            json_body: String,
            api_key: String,
        ) -> Pin<Box<dyn Future<Output = Result<FirecrawlReply, FirecrawlFault>> + Send + '_>>
        {
            let body_rec = Arc::clone(&self.seen_bodies);
            let key_rec = Arc::clone(&self.seen_keys);
            let next = self
                .script
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .pop_front()
                .unwrap_or_else(|| {
                    Err(FirecrawlFault::Network(
                        "脚本耗尽（测试不应走到这里）".to_string(),
                    ))
                });
            Box::pin(async move {
                body_rec
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(json_body);
                key_rec
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(api_key);
                next
            })
        }
    }

    fn query_cn(kw: &str) -> SearchQuery {
        SearchQuery {
            keyword: kw.to_string(),
            ..Default::default()
        }
    }

    fn query_en(kw: &str) -> SearchQuery {
        let mut q = query_cn(kw);
        q.language = Some(Lang::English);
        q
    }

    fn ok_reply(body: &str) -> Result<FirecrawlReply, FirecrawlFault> {
        Ok(FirecrawlReply {
            status: 200,
            body: body.to_string(),
        })
    }

    const REAL_SHAPE_REPLY: &str = r#"{"success":true,"data":[
        {"title":"一种数字式流量智能调节系统 - Google Patents","url":"https://patents.google.com/patent/CN114962757A/zh","description":"本发明涉及流量调节领域。"},
        {"title":"数字式流量调节研讨（论坛帖）","url":"https://example.com/thread/1","description":"论坛讨论。"},
        {"title":"","url":"","description":"缺 URL 的行应被跳过"}
    ]}"#;

    #[tokio::test]
    async fn web_search_parses_official_shape_and_skips_urlless_rows() {
        let transport = Arc::new(ScriptedTransport::new(vec![ok_reply(REAL_SHAPE_REPLY)]));
        let provider = FirecrawlSearchProvider::with_transport("FC-TEST".into(), transport.clone());
        let hits = provider
            .web_search(&query_cn("流量调节"))
            .await
            .expect("应成功");
        assert_eq!(2, hits.len(), "缺 URL 的行必须跳过，不造占位");
        assert_eq!("一种数字式流量智能调节系统 - Google Patents", hits[0].title);
        assert_eq!(
            "https://patents.google.com/patent/CN114962757A/zh",
            hits[0].url
        );
        assert_eq!("本发明涉及流量调节领域。", hits[0].snippet);
        // 请求体形状：中文查询追加「专利」，limit=8，key 走 Bearer 头
        let bodies = transport.seen_bodies.lock().unwrap();
        assert_eq!(
            r#"{"query":"流量调节 专利","limit":8}"#, bodies[0],
            "请求体键序与追加词锁死：{:?}",
            bodies[0]
        );
        let keys = transport.seen_keys.lock().unwrap();
        assert_eq!(vec!["FC-TEST".to_string()], *keys, "key 经头传递");
    }

    #[tokio::test]
    async fn web_query_for_appends_language_keyword_and_clamps_empty() {
        assert_eq!("流量调节 专利", web_query_for(&query_cn("流量调节")));
        assert_eq!("battery patent", web_query_for(&query_en("battery")));
        let mut empty = query_cn("   ");
        empty.keyword = "   ".to_string();
        assert_eq!("", web_query_for(&empty), "空查询钳制为空串（不发上游）");
    }

    #[tokio::test]
    async fn web_search_maps_429_to_quota_and_401_to_auth() {
        let transport = Arc::new(ScriptedTransport::new(vec![
            Ok(FirecrawlReply {
                status: 429,
                body: "rate limited".into(),
            }),
            Ok(FirecrawlReply {
                status: 401,
                body: "unauthorized".into(),
            }),
            Ok(FirecrawlReply {
                status: 500,
                body: "boom".into(),
            }),
        ]));
        let provider = FirecrawlSearchProvider::with_transport("FC-TEST".into(), transport);
        let (kind, msg) = provider.web_search(&query_cn("x")).await.unwrap_err();
        assert_eq!(FailKind::Quota, kind);
        assert!(msg.contains("429"), "{msg}");
        let (kind, msg) = provider.web_search(&query_cn("x")).await.unwrap_err();
        assert_eq!(FailKind::Auth, kind);
        assert!(msg.contains("401"), "{msg}");
        let (kind, _) = provider.web_search(&query_cn("x")).await.unwrap_err();
        assert_eq!(
            FailKind::Quota,
            kind,
            "其余 HTTP 错误归 Quota（与 xhr 同判据）"
        );
    }

    #[tokio::test]
    async fn web_search_transport_fault_maps_to_network_and_parse_to_parse() {
        let transport = Arc::new(ScriptedTransport::new(vec![
            Err(FirecrawlFault::Network("connect timeout".into())),
            ok_reply("不是 JSON"),
            ok_reply(r#"{"success":true}"#),
        ]));
        let provider = FirecrawlSearchProvider::with_transport("FC-TEST".into(), transport);
        let (kind, msg) = provider.web_search(&query_cn("x")).await.unwrap_err();
        assert_eq!(FailKind::Network, kind);
        assert!(msg.contains("connect timeout"), "{msg}");
        let (kind, _) = provider.web_search(&query_cn("x")).await.unwrap_err();
        assert_eq!(FailKind::Parse, kind, "非 JSON 响应 = 上游改版，必须暴露");
        let (kind, msg) = provider.web_search(&query_cn("x")).await.unwrap_err();
        assert_eq!(FailKind::Parse, kind);
        assert!(msg.contains("data 数组"), "{msg}");
    }

    #[test]
    fn request_body_escapes_quotes_and_control_chars() {
        assert_eq!(
            r#"{"query":"说\"引号\"与\\反斜杠","limit":8}"#,
            request_body("说\"引号\"与\\反斜杠", 8)
        );
        assert_eq!(r#"{"query":"a\nb","limit":8}"#, request_body("a\nb", 8));
    }

    #[test]
    fn parse_search_response_error_field_short_circuits() {
        let msg = parse_search_response(r#"{"error":"credits exhausted"}"#).unwrap_err();
        assert!(msg.contains("credits exhausted"), "{msg}");
    }
}
