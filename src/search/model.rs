//! 统一检索契约 / Search contract（MA1）
//!
//! 逐字段对齐 `docs/analysis/search-sources-spec.md` §1 的契约片段（`SearchQuery` / `Lang` /
//! `SourceKind` / `AttemptStatus` / `FailKind` / `AttemptReport` / `SearchOutcome` / `MergedPatent`）。
//! spec 片段之外新增的字段一律带 `/// MA1 追加` 或 `/// MA2/MA4 预留` 注明，避免「契约漂移无据可查」。
//!
//! 落位说明：spec 写的是 `crates/search/src/model.rs`，但 T0.x 仓库仍是单 crate，
//! 故按 types-migration-map.md §6 裁决 1 的等价原则落 `src/search/model.rs`。

use crate::types::search::{PatentSummary, SearchType};
use serde::{Deserialize, Serialize};

/// 检索语言意图（spec §1）。默认 Chinese —— 这是 PRD N2「要中文就给中文」的契约位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lang {
    Chinese,
    English,
    All,
}

/// 数据源标识（spec §1）。序列化用小写稳定串，供 MA5 诊断面板与日志复用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceKind {
    #[serde(rename = "serpapi")]
    SerpApi,
    #[serde(rename = "google_patents_xhr")]
    GooglePatentsXhr,
    #[serde(rename = "epo_ops")]
    EpoOps,
    #[serde(rename = "local_fts")]
    LocalFts,
}

impl SourceKind {
    /// 稳定字符串形式。**现役 `/api/search/online` 的 `source` 字段直接取此值**，
    /// 故其返回值由 `serpapi_provider_keeps_legacy_source_label` 锁死，改动即破坏前端。
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::SerpApi => "serpapi",
            SourceKind::GooglePatentsXhr => "google_patents_xhr",
            SourceKind::EpoOps => "epo_ops",
            SourceKind::LocalFts => "local_fts",
        }
    }
}

/// 失败分类（spec §1：决定是否切下一源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailKind {
    /// 超时 / DNS / 连接失败
    Network,
    /// 402 / 429 / 配额文案
    Quota,
    /// Key 无效
    Auth,
    /// 响应结构变化
    Parse,
}

impl FailKind {
    /// 是否应切换下一源（spec §1：Parse 记 bug 不切换，直接报错）。
    /// **MA2a 起由执行链 [`crate::search::chain::SourceChain`] 消费**：主源以 Parse 失败时不再降级。
    pub fn switches_source(self) -> bool {
        !matches!(self, FailKind::Parse)
    }

    /// 是否进入源级冷却（spec §1：Quota/Auth → 冷却；spec §6：连续 3 次后冷却 10 分钟）。
    #[allow(dead_code)] // MA2 熔断器消费
    pub fn cools_down(self) -> bool {
        matches!(self, FailKind::Quota | FailKind::Auth)
    }
}

/// 单次尝试状态（spec §1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttemptStatus {
    Success,
    Failed(FailKind),
    Skipped,
}

/// 单次源尝试的诊断报告（spec §1，MA5 诊断面板数据源）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttemptReport {
    pub source: SourceKind,
    pub status: AttemptStatus,
    /// 本次尝试耗时（毫秒）。旧链路不测耗时，MA1 起为面板补上，不参与任何判定。
    pub latency_ms: u64,
    pub hits: usize,
    /// 用户可读的原因描述（spec §1：禁止裸 `err.to_string()`，须带上下文）。
    pub error: Option<String>,
    /// **MA1 追加**：旧 `api_search_online` 的 `upstream_hint` 文案。
    /// 旧代码用「先 plain 后 `is_none()` 守卫」的写法，实际可达路径上等价于「首个非空生效」，
    /// 该语义由 [`SearchOutcome::hint`] 与 `first_non_empty_hint_wins` 单测固定。
    pub hint: Option<String>,
}

impl AttemptReport {
    /// 该次尝试是否算「拿到了可用结果」（成功且有命中）。
    pub fn produced_hits(&self) -> bool {
        matches!(self.status, AttemptStatus::Success) && self.hits > 0
    }
}

/// 合并后的单条专利（spec §1；去重键见 spec §6 = 规范化 publication_number）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergedPatent {
    /// 规范化 publication_number（`crate::patent::canonical_patent_key`），空号退化为标题键。
    #[allow(dead_code)] // MA2a 仍只有单源胜出（无跨源合并），MA2b 多源合并时消费
    pub key: String,
    /// 复用既有对外结构，保证 `/api/search/online` 的 patents[] 字段一字不变。
    pub summary: PatentSummary,
    /// 命中该条的源集合（MA1 恒为单元素，由 `merge::merged_from` 写入）。
    /// **MA2a 起被读取**：[`SearchOutcome::winning_source`] 用它决定 `/api/search/online`
    /// 的 `source` 字段值；MA2b 多源合并后同一条目可能挂多个源。
    pub sources: Vec<SourceKind>,
}

/// 一次检索的结果 + 全量尝试报告（spec §1）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchOutcome {
    /// 多源合并去重后的结果。MA1 只有 SerpAPI 一个在线源，等价于该源的映射结果。
    pub results: Vec<MergedPatent>,
    pub attempts: Vec<AttemptReport>,
    /// **MA1 追加**：旧链路 `search_information.total_results` 透传值。
    /// spec §1 片段没有该字段，但 `/api/search/online` 的 `total` 依赖它，缺失即改变 API 形状。
    pub upstream_total: Option<usize>,
}

impl SearchOutcome {
    /// 首个非空 hint（等价于旧 `upstream_hint` 的实际取值规则）。
    pub fn hint(&self) -> Option<String> {
        self.attempts.iter().find_map(|a| a.hint.clone())
    }

    /// 还原对外 `patents` 数组（只读视图，形状与旧代码逐字一致）。
    pub fn summaries(&self) -> Vec<PatentSummary> {
        self.results.iter().map(|m| m.summary.clone()).collect()
    }

    /// 该次结果是否来自某个成功且有命中的源（旧链路 `source` 字段的判定条件）。
    pub fn succeeded_from(&self, source: SourceKind) -> bool {
        self.attempts
            .iter()
            .any(|a| a.source == source && a.produced_hits())
    }

    /// **MA2a 追加**：结果实际出自哪个源（spec §6「先到先得」后需要对外如实标源）。
    ///
    /// 取首个带源的合并条目的第一个源。执行链只把「胜出源」的结果装进 `results`，
    /// 因此本函数等价于「本次返回内容出自哪一路」；`results` 为空时返回 `None`
    /// （此时 `routes/search.rs` 继续走本地兜底，与 MA1 行为一致）。
    pub fn winning_source(&self) -> Option<SourceKind> {
        self.results.iter().find_map(|m| m.sources.first().copied())
    }
}

/// [`AttemptReport::error`] / 日志片段的安全截断：**仅用于展示**，
/// 绝不用于任何送往上游、入库或传给 AI 的数据（AGENTS.md 2.5 截断纪律）。
///
/// MA1 时它是 `providers/serpapi.rs` 的私有函数；MA2a 接入第二个在线源时上提到契约模块，
/// 避免同一取值规则出现第二份实现（AGENTS.md 2.2）。
pub fn report_excerpt(s: &str) -> String {
    const MAX: usize = 200;
    if s.chars().count() <= MAX {
        return s.to_string();
    }
    let mut out: String = s.chars().take(MAX).collect();
    out.push('…');
    out
}

/// 统一检索入参（spec §1）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchQuery {
    /// 用户输入，原样保留（spec §1）。
    pub keyword: String,
    pub country: Option<String>,
    pub language: Option<Lang>,
    /// MA4 预留：申请人过滤。
    #[allow(dead_code)] // MA4 精确匹配消费
    pub assignee: Option<String>,
    /// MA4 预留：精确匹配开关。
    #[allow(dead_code)] // MA4 精确匹配消费
    pub exact_assignee: bool,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    /// 返回条数上限。MA1 不消费（SerpAPI 固定 10 条/页，与旧链路一致）。
    /// **MA2a 起由 Google Patents XHR 源消费**（`limit > 0` 时截断该源返回条数）。
    /// 注：spec §2 列出的 XHR 参数集中没有条数参数（存档证据里亦无），故在上游已分页的
    /// 结果内做**客户端截断**，不伪造 `num=` 参数。
    pub limit: usize,
    /// **MA1 追加**：旧链路的分页语义（`page` 直接进 URL，且带 `<1 → 1` 钳制）。
    pub page: usize,
    /// **MA1 追加**：旧链路的 `sort_by`（仅 "new"/"old" 生效，与旧代码一致）。
    pub sort_by: Option<String>,
    /// **MA1 追加**：旧链路按 `SearchType` 渲染 q 串（applicant/inventor/patent_number 语法不同）。
    pub search_type: Option<SearchType>,
}

impl Default for SearchQuery {
    /// spec §1 的默认值：中国 + 中文 + 20 条 + 第 1 页。
    /// 注意：现役 `/api/search/online` 不用默认值，而是逐字段从 `SearchRequest` 映射，
    /// 以免「默认值顺手改了线上行为」。
    fn default() -> Self {
        SearchQuery {
            keyword: String::new(),
            country: Some("CN".to_string()),
            language: Some(Lang::Chinese),
            assignee: None,
            exact_assignee: false,
            date_from: None,
            date_to: None,
            limit: 20,
            page: 1,
            sort_by: None,
            search_type: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 对外 JSON 的 `source` 字段取自这里，锁死为旧串。
    #[test]
    fn serpapi_provider_keeps_legacy_source_label() {
        assert_eq!("serpapi", SourceKind::SerpApi.as_str());
        assert_eq!(
            json!("serpapi"),
            serde_json::to_value(SourceKind::SerpApi.as_str()).expect("serialize str")
        );
    }

    /// spec §1 的 FailKind → 处置决策，必须逐条对齐（MA2 的控制流依据）。
    #[test]
    fn fail_kind_dispositions_match_spec() {
        assert!(FailKind::Network.switches_source());
        assert!(FailKind::Quota.switches_source());
        assert!(FailKind::Auth.switches_source());
        assert!(!FailKind::Parse.switches_source());
        assert!(!FailKind::Network.cools_down());
        assert!(FailKind::Quota.cools_down());
        assert!(FailKind::Auth.cools_down());
        assert!(!FailKind::Parse.cools_down());
    }

    /// 旧 `upstream_hint` 是「首个非空生效」，不是「最后一个覆盖」。
    #[test]
    fn first_non_empty_hint_wins() {
        let outcome = SearchOutcome {
            results: Vec::new(),
            attempts: vec![
                AttemptReport {
                    source: SourceKind::SerpApi,
                    status: AttemptStatus::Skipped,
                    latency_ms: 0,
                    hits: 0,
                    error: None,
                    hint: None,
                },
                AttemptReport {
                    source: SourceKind::SerpApi,
                    status: AttemptStatus::Failed(FailKind::Quota),
                    latency_ms: 12,
                    hits: 0,
                    error: Some("HTTP 429".to_string()),
                    hint: Some("第一条提示".to_string()),
                },
                AttemptReport {
                    source: SourceKind::LocalFts,
                    status: AttemptStatus::Success,
                    latency_ms: 3,
                    hits: 1,
                    error: None,
                    hint: Some("第二条提示（应被忽略）".to_string()),
                },
            ],
            upstream_total: None,
        };
        assert_eq!(Some("第一条提示".to_string()), outcome.hint());
        assert_eq!(3, outcome.attempts.len());
        assert!(!outcome.succeeded_from(SourceKind::SerpApi)); // 成功的那条 hits=0 不算数
    }

    #[test]
    fn outcome_reads_back_summaries() {
        let summary = PatentSummary {
            id: "p1".to_string(),
            patent_number: "CN123456A".to_string(),
            title: "标题".to_string(),
            abstract_text: "摘要".to_string(),
            applicant: "申请人".to_string(),
            inventor: "发明人".to_string(),
            filing_date: "2024-01-01".to_string(),
            country: "CN".to_string(),
            relevance_score: Some(88.0),
            score_source: Some("hybrid(pos:98+content:88)".to_string()),
        };
        let outcome = SearchOutcome {
            results: vec![MergedPatent {
                key: "CN123456A".to_string(),
                sources: vec![SourceKind::SerpApi],
                summary: summary.clone(),
            }],
            attempts: vec![AttemptReport {
                source: SourceKind::SerpApi,
                status: AttemptStatus::Success,
                latency_ms: 7,
                hits: 1,
                error: None,
                hint: None,
            }],
            upstream_total: Some(1),
        };
        assert_eq!(vec![summary], outcome.summaries());
        assert!(outcome.succeeded_from(SourceKind::SerpApi));
        assert!(!outcome.succeeded_from(SourceKind::EpoOps));
    }

    /// MA2a：`/api/search/online` 的 `source` 字段改由合并条目上挂的源决定，
    /// 因此「哪个源胜出」必须如实可读，且空结果时不得凭空标源（否则会跳过本地兜底）。
    #[test]
    fn winning_source_reads_merged_sources() {
        let outcome = SearchOutcome {
            results: vec![MergedPatent {
                key: "CN123456A".to_string(),
                summary: PatentSummary {
                    id: "p1".to_string(),
                    patent_number: "CN123456A".to_string(),
                    title: "固态电池".to_string(),
                    abstract_text: String::new(),
                    applicant: String::new(),
                    inventor: String::new(),
                    filing_date: String::new(),
                    country: "CN".to_string(),
                    relevance_score: None,
                    score_source: None,
                },
                sources: vec![SourceKind::GooglePatentsXhr],
            }],
            attempts: vec![],
            upstream_total: None,
        };
        assert_eq!(
            Some(SourceKind::GooglePatentsXhr),
            outcome.winning_source()
        );
        assert!(SearchOutcome {
            results: vec![],
            attempts: vec![],
            upstream_total: None,
        }
        .winning_source()
        .is_none());
    }

    /// 诊断截断只用于展示：200 字以内原样、超出加省略号，且**按字符**截断
    /// （按字节截会在中文中间炸出非法 UTF-8 边界）。
    #[test]
    fn report_excerpt_is_display_only_and_char_safe() {
        assert_eq!("短文本", report_excerpt("短文本"));
        let exactly_max = "汉".repeat(200);
        assert_eq!(exactly_max, report_excerpt(&exactly_max));
        let long = "汉".repeat(250);
        let out = report_excerpt(&long);
        assert_eq!(201, out.chars().count());
        assert!(out.ends_with('…'));
        assert!(out.starts_with("汉汉汉"));
    }
}
