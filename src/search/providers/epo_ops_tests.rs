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
        hits[0].relevance_score.unwrap_or_default() >= hits[1].relevance_score.unwrap_or_default(),
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
        "每条都标了本源（出参 source 字段与诊断面板读的就是它）"
    );
    assert_eq!(
        "EP3445287",
        crate::patent::canonical_patent_key(&outcome.results[0].summary.patent_number),
        "EPO 著录的公开号规范化后仍是 EP3445287（去空格/去 kind code）——\
         MA6c 前这条断言读的是 MergedPatent.key，字段按裁决删除后改为就地求值，\
         锁的语义（本源输出的规范化公开号形状）一字未减"
    );
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
