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
