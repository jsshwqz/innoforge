//! 多源执行链 / Source chain（spec §6「MA2 核心」）
//!
//! ## 为什么是「并行发起 + 优先级择胜」
//!
//! spec §6 明确否掉了严格串行瀑布（太慢），要求 `[SerpApi, GooglePatentsXhr]` 并行发起、
//! 任一成功即开始返回。本模块照此实现：所有源**同时**发出，各自带独立超时，
//! 回来后按 `providers` 的**登记顺序**决定谁的内容真正返回。
//!
//! ## `FailKind::Parse` 在并行模型下的语义（spec §1 的落点）
//!
//! §1 说 Parse「记 bug 不切换直接报错」。串行下这等价于「不发下一个请求」；
//! 并行下下一个请求已经发出去了，收不回来。因此这里的忠实实现是：
//! **择胜扫描在 Parse 失败处截断** —— 更高优先级的源结构变了，就不再拿更低优先级的
//! 内容顶上（那会把「上游改版」这个必须暴露的 bug 洗成一次看似正常的搜索），
//! 但所有已发出的尝试仍完整记进 `attempts` 供 MA5 面板呈现。
//! 该规则由 `parse_failure_stops_degradation_but_keeps_attempts` 锁死。
//!
//! ## 反爬风险的权衡（实证）
//!
//! 每次搜索都并发打一发 XHR 请求，会让 Google 侧的限流更容易命中
//! （实测同 IP 3 发 / 15s 内即 503，见 `providers::google_patents_xhr` 的 EVIDENCE）。
//! 代价与收益的取舍放在这里说明白：本源的进程内最小间隔 + 抖动限速
//! （[`crate::search::providers::google_patents_xhr::MIN_REQUEST_INTERVAL`]）
//! 是把它压在「个人本地工具自用频率」这条合规边界内的手段；
//! 若后续观测到常态性 503，应在 MA5 的面板上暴露冷却状态（`FailKind::cools_down` 的消费点），
//! 而不是回到串行瀑布。

use crate::search::model::{AttemptReport, SearchOutcome};
use crate::search::model::{AttemptStatus, SourceKind};
use crate::search::provider::SearchProvider;
use std::sync::Arc;

/// 一个按优先级排列的在线源集合。
#[derive(Clone)]
pub struct SourceChain {
    providers: Vec<Arc<dyn SearchProvider>>,
}

impl SourceChain {
    /// `providers` 的顺序即胜出优先级（主源在前）。空集合合法，`run` 返回空结果。
    pub fn new(providers: Vec<Arc<dyn SearchProvider>>) -> Self {
        SourceChain { providers }
    }

    /// 已登记的源（按优先级），供日志与 MA5 面板显示链路形状。
    pub fn kinds(&self) -> Vec<SourceKind> {
        self.providers.iter().map(|p| p.kind()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    /// 并行发起全部源，按登记顺序择胜。永不返回 `Err`（失败以 `attempts` 记账）。
    pub async fn run(&self, query: crate::search::model::SearchQuery) -> SearchOutcome {
        let outcomes = futures::future::join_all(self.providers.iter().map(|provider| {
            let provider = provider.clone();
            let attempt_query = query.clone();
            async move { provider.search(attempt_query).await }
        }))
        .await;
        resolve_by_precedence(outcomes)
    }
}

/// 纯函数形态的择胜规则 —— 与 `run` 拆开，好让降级链用例不必真起并发即可断言。
///
/// 两条独立的账：
/// - `attempts`：**全量**收集所有已发出尝试的报告（spec §6「AttemptReport 全量记录进诊断面板」）。
///   并行模型下即使某源的结果没被采用，它确实跑了，面板就该看见。
/// - `winner`：按优先级扫描，首个「带回可用内容」的胜出；遇到 Parse 失败即截断，不再下探。
pub(crate) fn resolve_by_precedence(outcomes: Vec<SearchOutcome>) -> SearchOutcome {
    // 「这一路真的带回了可用内容」= 有成功且有命中的尝试，**且**结果数组非空。
    // 两个条件都要：只看 hits 会漏过「报告说命中但结果为空」的实现自相矛盾；
    // 只看 results 会把 Failed 状态下残留的结果数组误当成功。
    fn produced(o: &SearchOutcome) -> bool {
        !o.results.is_empty() && o.attempts.iter().any(|a| a.produced_hits())
    }

    fn blocks_degradation(o: &SearchOutcome) -> bool {
        // spec §1 的处置表在这里只有一条生效分支：Parse → 不切换。
        // 直接复用 FailKind::switches_source()，避免「哪些失败该降级」出现第二份判定。
        o.attempts
            .iter()
            .any(|a| matches!(a.status, AttemptStatus::Failed(kind) if !kind.switches_source()))
    }

    let attempts: Vec<AttemptReport> = outcomes
        .iter()
        .flat_map(|o| o.attempts.iter().cloned())
        .collect();

    let mut winner: Option<SearchOutcome> = None;
    for outcome in &outcomes {
        if produced(outcome) {
            winner = Some(outcome.clone());
            break;
        }
        // spec §1：Parse 不再往下降级（详见模块头的并行语义说明）。
        if blocks_degradation(outcome) {
            break;
        }
    }

    SearchOutcome {
        results: winner
            .as_ref()
            .map(|w| w.results.clone())
            .unwrap_or_default(),
        attempts,
        upstream_total: winner.and_then(|w| w.upstream_total),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patent::canonical_patent_key;
    use crate::search::model::{FailKind, MergedPatent, SearchQuery};
    use crate::search::providers::epo_ops as epo;
    use crate::search::providers::google_patents_xhr as xhr;
    use crate::search::providers::google_patents_xhr::GooglePatentsXhrProvider;
    use crate::types::search::PatentSummary;
    use std::future::Future;
    use std::pin::Pin;

    /// 只回放一份预设 `SearchOutcome` 的桩源 —— 扮演「SerpAPI 那一头」，
    /// 让降级链用例不必真起 SerpAPI 调用（也不需要它的 Key）。
    struct StubProvider {
        kind: SourceKind,
        outcome: SearchOutcome,
    }

    impl SearchProvider for StubProvider {
        fn kind(&self) -> SourceKind {
            self.kind
        }
        fn search<'a>(
            &'a self,
            _query: SearchQuery,
        ) -> Pin<Box<dyn Future<Output = SearchOutcome> + Send + 'a>> {
            Box::pin(async move { self.outcome.clone() })
        }
    }

    fn summary(patent_number: &str, title: &str) -> PatentSummary {
        PatentSummary {
            id: format!("stub-{patent_number}"),
            patent_number: patent_number.to_string(),
            title: title.to_string(),
            abstract_text: String::new(),
            applicant: String::new(),
            inventor: String::new(),
            filing_date: String::new(),
            country: "CN".to_string(),
            relevance_score: Some(90.0),
            score_source: None,
        }
    }

    /// 主源桩：可控制「状态 + 是否带回结果」，两者刻意分开，
    /// 因为「Success 但零命中」和「Failed」在链路上的走向完全不同（见下面两条用例）。
    fn stub(
        source: SourceKind,
        status: AttemptStatus,
        with_results: bool,
    ) -> Arc<dyn SearchProvider> {
        let hits = if with_results { 1 } else { 0 };
        Arc::new(StubProvider {
            kind: source,
            outcome: SearchOutcome {
                results: if with_results {
                    vec![MergedPatent {
                        key: canonical_patent_key("CN900000000A"),
                        sources: vec![source],
                        summary: summary("CN900000000A", "主源桩结果"),
                    }]
                } else {
                    Vec::new()
                },
                attempts: vec![AttemptReport {
                    source,
                    status,
                    latency_ms: 5,
                    hits,
                    error: match status {
                        AttemptStatus::Failed(kind) => {
                            Some(format!("{} 模拟失败（{kind:?}）", source.as_str()))
                        }
                        AttemptStatus::Skipped => Some("未配置 SERPAPI_KEY".to_string()),
                        AttemptStatus::Success => None,
                    },
                    hint: None,
                }],
                upstream_total: if with_results { Some(1) } else { None },
            },
        })
    }

    /// 真 XHR provider + 真实冒烟 fixture（离线的假传输），
    /// 用于证明降级出来的内容确实走完了「映射 → 打分 → 放行 → 中文过滤 → 去重」全链。
    fn xhr_ok() -> Arc<dyn SearchProvider> {
        let (p, _transport, _clock) =
            xhr::tests::provider(vec![xhr::tests::reply(200, xhr::tests::REAL_CN_REPLY)]);
        Arc::new(p)
    }

    /// 真 XHR provider，但上游恒返 503 反爬页（假传输，不出网）。
    fn xhr_permanent_503() -> Arc<dyn SearchProvider> {
        let (p, _transport, _clock) = xhr::tests::provider(vec![XhrReplyOf::bot_blocked()]);
        Arc::new(p)
    }

    /// 真 EPO OPS provider + 官方样例转写件（假 token 传输 / 假检索传输 / 假时钟，全程不出网）。
    /// 与 [`xhr_ok`] 同一手法：链用例吃真 provider，才能证明降级内容确实走完了
    /// 「取 token → 映射 → 打分 → 放行 → 去重」全链，而不是桩函数自证。
    fn epo_ok() -> Arc<dyn SearchProvider> {
        let (p, _transport, _clock) = epo::tests::provider(
            vec![epo::tests::token_reply("TOK", "1199")],
            vec![epo::tests::reply(200, epo::tests::SEARCH_BIBLIO_REPLY)],
        );
        Arc::new(p)
    }

    /// 真 EPO OPS provider，但取 token 就撞 401（凭证无效，L1 现场件）。
    /// 该形态下本源**一发数据请求都不发**（省配额），故失败原因必定是 `Auth`。
    fn epo_auth_failed() -> Arc<dyn SearchProvider> {
        let (p, _transport, _clock) = epo::tests::provider(
            vec![epo::tests::reply(401, epo::tests::REAL_401_TOKEN_BODY)],
            vec![epo::tests::reply(200, epo::tests::SEARCH_BIBLIO_REPLY)],
        );
        Arc::new(p)
    }

    /// 名字里的 `XhrReplyOf` 只是「构造一个 XhrReply」的短别名，避免测试里重复写状态码。
    struct XhrReplyOf;

    impl XhrReplyOf {
        fn bot_blocked() -> xhr::XhrReply {
            xhr::XhrReply {
                status: 503,
                body: xhr::tests::REAL_503_BODY.to_string(),
                retry_after: None,
            }
        }
    }

    fn cn_query() -> SearchQuery {
        xhr::tests::query("动车组电池健康状态")
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // spec §6 降级链三用例（注入式假 transport / 假时钟，全程离线）
    // ─────────────────────────────────────────────────────────────────────────────

    /// **用例 ①**：主源 SerpAPI 配额挂 → 免费的 Google Patents XHR 直抓源出结果。
    #[tokio::test]
    async fn case1_serpapi_down_degrades_to_xhr_results() {
        let chain = SourceChain::new(vec![
            stub(
                SourceKind::SerpApi,
                AttemptStatus::Failed(FailKind::Quota),
                false,
            ),
            xhr_ok(),
        ]);
        assert_eq!(
            vec![SourceKind::SerpApi, SourceKind::GooglePatentsXhr],
            chain.kinds(),
            "登记顺序即优先级，routes 层依赖它决定 source 字段"
        );

        let outcome = chain.run(cn_query()).await;

        assert!(
            !outcome.results.is_empty(),
            "SerpAPI 挂掉后必须仍能给用户返回结果（MA2a 验收锚点）"
        );
        assert_eq!(
            Some(SourceKind::GooglePatentsXhr),
            outcome.winning_source(),
            "胜出源必须如实标注，否则 /api/search/online 会错报 source"
        );
        assert!(outcome.succeeded_from(SourceKind::GooglePatentsXhr));
        assert!(!outcome.succeeded_from(SourceKind::SerpApi));
        let statuses: Vec<(SourceKind, AttemptStatus)> = outcome
            .attempts
            .iter()
            .map(|a| (a.source, a.status))
            .collect();
        assert_eq!(
            vec![
                (SourceKind::SerpApi, AttemptStatus::Failed(FailKind::Quota)),
                (SourceKind::GooglePatentsXhr, AttemptStatus::Success),
            ],
            statuses
        );
        assert_eq!(Some(14627), outcome.upstream_total, "total 取胜出源的");
        assert!(
            outcome.results[0]
                .summary
                .title
                .contains("动车组电池健康状态"),
            "{:?}",
            outcome.results[0].summary.title
        );
        assert_eq!(
            vec![SourceKind::GooglePatentsXhr],
            outcome.results[0].sources
        );
    }

    /// **用例 ②**：XHR 撞上实测的 503 反爬 → 退避 → 重试成功，链路仍然按时交出结果。
    ///
    /// 与 provider 侧 `rate_limited_503_backs_off_then_succeeds` 的分工：那条看单源内部
    /// 有没有真的退避；这条看**退避期间链路没有把这一路判死**，恢复后仍能顶上来。
    #[tokio::test]
    async fn case2_xhr_rate_limited_backs_off_then_chain_succeeds() {
        let clock = xhr::tests::FakeClock::new();
        let transport = xhr::tests::FakeTransport::new(vec![
            XhrReplyOf::bot_blocked(),
            xhr::tests::reply(200, xhr::tests::REAL_CN_REPLY),
        ]);
        let provider = GooglePatentsXhrProvider::with_transport(
            xhr::tests::db(),
            transport.clone(),
            clock.clone(),
        );
        let chain = SourceChain::new(vec![
            stub(
                SourceKind::SerpApi,
                AttemptStatus::Failed(FailKind::Network),
                false,
            ),
            Arc::new(provider),
        ]);

        let outcome = chain.run(cn_query()).await;

        assert_eq!(
            2,
            transport.fetch_count(),
            "首发 503 + 退避后重试 1 发，共 2 发"
        );
        assert!(
            clock.recorded().contains(&xhr::BASE_BACKOFF),
            "退避未发生: {:?}",
            clock.recorded()
        );
        assert_eq!(
            Some(SourceKind::GooglePatentsXhr),
            outcome.winning_source(),
            "退避成功后这一路必须仍然可用，不能因为一次 503 就被链路判死"
        );
        assert!(!outcome.results.is_empty());
        assert_eq!(
            AttemptStatus::Success,
            outcome.attempts[1].status,
            "重试成功后不得把中间态 503 留在最终报告里"
        );
    }

    /// **用例 ③**：两个在线源都挂 → 空结果，但 `attempts` 各带自己的 `FailKind`，
    /// 供上层区分「该继续本地兜底」还是「上游改版要报 bug」。
    #[tokio::test]
    async fn case3_both_sources_down_reports_each_fail_kind() {
        let chain = SourceChain::new(vec![
            stub(
                SourceKind::SerpApi,
                AttemptStatus::Failed(FailKind::Auth),
                false,
            ),
            xhr_permanent_503(),
        ]);

        let outcome = chain.run(cn_query()).await;

        assert!(outcome.results.is_empty(), "双挂不得凭空造结果");
        assert!(outcome.upstream_total.is_none());
        assert_eq!(2, outcome.attempts.len(), "{:?}", outcome.attempts);

        let (serp, xhr_report) = (&outcome.attempts[0], &outcome.attempts[1]);
        assert_eq!(
            (SourceKind::SerpApi, FailKind::Auth),
            (serp.source, fail_of(serp))
        );
        assert_eq!(SourceKind::GooglePatentsXhr, xhr_report.source);
        assert_eq!(
            FailKind::Quota,
            fail_of(xhr_report),
            "实测 503 反爬页要判 Quota（见 FailKind::for_xhr_reply），\
             误判成 Parse 会直接掐断降级，误判成 Network 会让面板把限流显示成断网"
        );
        assert_eq!(0, xhr_report.hits);
        for a in &outcome.attempts {
            let err = a.error.clone().expect("失败必须带可读原因，禁止 None");
            assert!(!err.trim().is_empty(), "禁止空 error");
            assert!(
                err.contains(a.source.as_str()),
                "error 要点明是哪个源: {err}"
            );
        }
    }

    fn fail_of(report: &AttemptReport) -> FailKind {
        match report.status {
            AttemptStatus::Failed(kind) => kind,
            other => panic!("期望失败状态，实得 {other:?}"),
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // MA2b：第三源 EPO OPS 进链（spec §4 + §6）
    // ─────────────────────────────────────────────────────────────────────────────

    /// **用例 ④（MA2b 验收锚点 / 任务清单 2-①）**：前两源都拿不到可用结果时，
    /// 链尾的 EPO OPS 顶上，且 `winning_source` 如实标成 `epo_ops`。
    ///
    /// 为什么 EPO 用例吃英文查询：OPS 的 `ti`/`ab` 只有英文著录索引（spec §4 +
    /// `providers/epo_ops.rs` 的 L7），中文查询下它本就零命中 —— 那是能力边界，
    /// 由 provider 侧的 `chinese_query_on_english_fixture_is_success_with_zero_hits` 锁死，
    /// 不能拿它来证明降级链路通不通。
    #[tokio::test]
    async fn case4_first_two_sources_down_degrades_to_epo() {
        let chain = SourceChain::new(vec![
            stub(
                SourceKind::SerpApi,
                AttemptStatus::Failed(FailKind::Network),
                false,
            ),
            xhr_permanent_503(),
            epo_ok(),
        ]);
        assert_eq!(
            vec![
                SourceKind::SerpApi,
                SourceKind::GooglePatentsXhr,
                SourceKind::EpoOps
            ],
            chain.kinds(),
            "登记顺序即优先级：EPO 排链尾（spec §4「对中文检索贡献有限」）"
        );

        let outcome = chain.run(epo::tests::en_query("battery")).await;

        assert!(
            !outcome.results.is_empty(),
            "前两源全挂时必须由第三源顶上（MA2b 验收锚点）"
        );
        assert_eq!(
            Some(SourceKind::EpoOps),
            outcome.winning_source(),
            "胜出源必须如实标注，否则 /api/search/online 的 source 字段会错报"
        );
        assert!(outcome.succeeded_from(SourceKind::EpoOps));
        assert!(!outcome.succeeded_from(SourceKind::SerpApi));
        assert!(!outcome.succeeded_from(SourceKind::GooglePatentsXhr));
        let statuses: Vec<(SourceKind, AttemptStatus)> = outcome
            .attempts
            .iter()
            .map(|a| (a.source, a.status))
            .collect();
        assert_eq!(
            vec![
                (
                    SourceKind::SerpApi,
                    AttemptStatus::Failed(FailKind::Network)
                ),
                (
                    SourceKind::GooglePatentsXhr,
                    AttemptStatus::Failed(FailKind::Quota)
                ),
                (SourceKind::EpoOps, AttemptStatus::Success),
            ],
            statuses,
            "三源全量记账，没被采用的那两路也不能从面板上消失"
        );
        assert_eq!(
            Some(10000),
            outcome.upstream_total,
            "total 取胜出源（EPO 上游的 total-result-count）"
        );
        assert_eq!(
            vec![SourceKind::EpoOps],
            outcome.results[0].sources,
            "结果条目要挂得上真正的命中源"
        );
        assert_eq!("EP3445287B1", outcome.summaries()[0].patent_number);
    }

    /// **用例 ⑤（任务清单 2-②）**：三源全挂 → 空结果，但 `attempts` 三源各带自己的
    /// `FailKind`，且 error 点名来源（上层据此区分「该本地兜底」与「上游改版要报 bug」）。
    #[tokio::test]
    async fn case5_all_three_sources_down_reports_each_fail_kind() {
        let chain = SourceChain::new(vec![
            stub(
                SourceKind::SerpApi,
                AttemptStatus::Failed(FailKind::Auth),
                false,
            ),
            xhr_permanent_503(),
            epo_auth_failed(),
        ]);

        let outcome = chain.run(epo::tests::en_query("battery")).await;

        assert!(outcome.results.is_empty(), "三挂不得凭空造结果");
        assert_eq!(None, outcome.winning_source());
        assert!(outcome.upstream_total.is_none());
        assert_eq!(3, outcome.attempts.len(), "{:?}", outcome.attempts);

        let kinds: Vec<(SourceKind, FailKind)> = outcome
            .attempts
            .iter()
            .map(|a| (a.source, fail_of(a)))
            .collect();
        assert_eq!(
            vec![
                (SourceKind::SerpApi, FailKind::Auth),
                (SourceKind::GooglePatentsXhr, FailKind::Quota),
                (SourceKind::EpoOps, FailKind::Auth),
            ],
            kinds,
            "三源的失败原因各归各的：误判成 Parse 会掐断降级，误判成 Network 会把限流显示成断网"
        );
        for a in &outcome.attempts {
            let err = a.error.clone().expect("失败必须带可读原因，禁止 None");
            assert!(!err.trim().is_empty(), "禁止空 error");
            assert!(
                err.contains(a.source.as_str()),
                "error 要点明是哪个源: {err}"
            );
            assert_eq!(0, a.hits, "失败的那一路不得带命中数");
        }
        let epo_report = &outcome.attempts[2];
        assert!(
            epo_report.error.clone().expect("note").contains("鉴权"),
            "EPO 的 401 要说清是凭证问题：{:?}",
            epo_report.error
        );
    }

    /// **用例 ⑥（任务清单 2-③ 的另一半）**：链上「未配 EPO Key」的正常处置是**压根不登记**
    /// （见 `routes/search.rs::online_chain_registers_epo_only_with_credentials`）。
    /// 这里锁死万一被登记进来时的形状：不发任何请求、记 `Skipped`、不伪造结果。
    #[tokio::test]
    async fn case6_epo_without_credentials_skips_and_sends_nothing() {
        let (provider, transport, _clock) = epo::tests::provider_with(
            "",
            "",
            vec![epo::tests::token_reply("TOK", "1199")],
            vec![epo::tests::reply(200, epo::tests::SEARCH_BIBLIO_REPLY)],
        );
        let chain = SourceChain::new(vec![
            stub(
                SourceKind::SerpApi,
                AttemptStatus::Failed(FailKind::Network),
                false,
            ),
            xhr_permanent_503(),
            Arc::new(provider),
        ]);

        let outcome = chain.run(epo::tests::en_query("battery")).await;

        assert!(outcome.results.is_empty(), "没凭证的源不得凭空顶结果");
        assert_eq!(None, outcome.winning_source());
        let epo_report = outcome
            .attempts
            .iter()
            .find(|a| a.source == SourceKind::EpoOps)
            .expect("这一条用例是「已登记但没凭证」，故必须留痕");
        assert_eq!(AttemptStatus::Skipped, epo_report.status);
        assert_eq!(0, epo_report.latency_ms, "压根没发出请求，耗时不能凭空记账");
        let err = epo_report
            .error
            .clone()
            .expect("Skipped 也要说明为什么没跑");
        assert!(
            err.contains("EPO_KEY") && err.contains("EPO_SECRET"),
            "{err}"
        );
        assert_eq!(
            0,
            transport.total_calls(),
            "未配凭证时 token 与检索一发都不能发（token 也算配额）"
        );
    }

    /// spec §1：主源 Parse（上游改版）时**不降级**，但并行已发出的尝试仍全量记账。
    #[tokio::test]
    async fn parse_failure_stops_degradation_but_keeps_attempts() {
        let chain = SourceChain::new(vec![
            stub(
                SourceKind::SerpApi,
                AttemptStatus::Failed(FailKind::Parse),
                false,
            ),
            xhr_ok(),
        ]);
        let outcome = chain.run(cn_query()).await;
        assert!(
            outcome.results.is_empty(),
            "主源结构变化时不能拿降级源的内容把改版洗成一次正常搜索"
        );
        assert_eq!(
            2,
            outcome.attempts.len(),
            "并行已经发出去的请求不能从诊断面板上消失"
        );
        assert_eq!(
            SourceKind::GooglePatentsXhr,
            outcome.attempts[1].source,
            "attempts 顺序 = 优先级顺序"
        );
    }

    /// 主源正常时，降级源的结果不得顶掉它（并行发起 + 优先级择胜）。
    #[tokio::test]
    async fn primary_wins_when_both_produce_hits() {
        let chain = SourceChain::new(vec![
            stub(SourceKind::SerpApi, AttemptStatus::Success, true),
            xhr_ok(),
        ]);
        let outcome = chain.run(cn_query()).await;
        assert_eq!(Some(SourceKind::SerpApi), outcome.winning_source());
        assert_eq!(
            1,
            outcome.results.len(),
            "只取胜出源，不做跨源拼接（那是 MA2b）"
        );
        assert_eq!(2, outcome.attempts.len());
    }

    /// 「成功但零命中」既不是失败也不算胜出，要继续往下降。
    #[tokio::test]
    async fn empty_success_still_degrades() {
        let chain = SourceChain::new(vec![
            stub(SourceKind::SerpApi, AttemptStatus::Success, false),
            xhr_ok(),
        ]);
        let outcome = chain.run(cn_query()).await;
        assert_eq!(Some(SourceKind::GooglePatentsXhr), outcome.winning_source());
    }

    /// Skipped（没配 Key）不阻断降级，也不能被当成「有结果」。
    #[tokio::test]
    async fn skipped_primary_does_not_block_fallback() {
        let chain = SourceChain::new(vec![
            stub(SourceKind::SerpApi, AttemptStatus::Skipped, false),
            xhr_ok(),
        ]);
        let outcome = chain.run(cn_query()).await;
        assert_eq!(Some(SourceKind::GooglePatentsXhr), outcome.winning_source());
        assert_eq!(AttemptStatus::Skipped, outcome.attempts[0].status);
    }

    #[tokio::test]
    async fn empty_chain_returns_empty_outcome() {
        let chain = SourceChain::new(Vec::new());
        assert!(chain.is_empty());
        let outcome = chain.run(cn_query()).await;
        assert!(outcome.results.is_empty() && outcome.attempts.is_empty());
        assert_eq!(None, outcome.winning_source());
    }
}
