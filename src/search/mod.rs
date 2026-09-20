//! # 统一检索层 / Search layer（MA1）
//!
//! 依据 `docs/analysis/search-sources-spec.md` §1「统一契约」建立的检索抽象层：
//! - [`model`]：契约数据结构（`SearchQuery` / `SearchOutcome` / `AttemptReport` / `FailKind` …）
//! - [`query`]：q 串渲染与国内/国外判定（原 `routes/mod.rs::build_online_query`）
//! - [`relevance`]：在线结果相关性打分与放行判定（原 `routes/search.rs` 私有函数）
//! - [`merge`]：排序与去重（原 `routes/search.rs` 私有函数）
//! - [`provider`]：[`SearchProvider`](provider::SearchProvider) trait 本体
//! - [`providers`]：具体源的实现（MA2a 起为 `serpapi` + `google_patents_xhr`）
//! - [`chain`]：[`SourceChain`](chain::SourceChain) 多源并行执行链与降级择胜规则（MA2a）
//!
//! ## MA1 边界（有意不做的事）
//! - ~~不做新源接入（Google Patents XHR / EPO OPS 属 MA2）~~ **MA2a 已接入 Google Patents XHR**；
//! - ~~不做并行执行链与源级熔断（spec §6 属 MA2），故本层部分判定方法（`FailKind::switches_source`
//!   等）尚无消费方，已就地标注 `#[allow(dead_code)]` 并说明归属里程碑~~
//!   **MA2a 起 `switches_source` 已被消费**（`providers::google_patents_xhr` 的重试判定 +
//!   `chain` 的 Parse 截断）；源级熔断（连续 3 次 Quota/Auth 冷却 10 分钟）
//!   仍属 MA5/MA6；
//! - 不做本地 FTS provider（本地兜底链路仍留在 `routes/search.rs`，单一写入口改造属 MA4）；
//! - 不改任何对外 API 形状：`/api/search/online` 的响应字段、上游请求参数、结果映射
//!   与迁移前逐字一致，由 `providers::serpapi` 与 `query` 的参照实现单测锁死。

pub mod chain;
pub mod merge;
pub mod model;
pub mod provider;
pub mod providers;
pub mod query;
pub mod relevance;
