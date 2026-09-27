//! # 统一检索层 / Search layer（MA1）
//!
//! 依据 `docs/analysis/search-sources-spec.md` §1「统一契约」建立的检索抽象层：
//! - [`model`]：契约数据结构（`SearchQuery` / `SearchOutcome` / `AttemptReport` / `FailKind` …）
//! - [`query`]：q 串渲染与国内/国外判定（原 `routes/mod.rs::build_online_query`）
//! - [`relevance`]：在线结果相关性打分与放行判定（原 `routes/search.rs` 私有函数）
//! - [`merge`]：排序与去重（原 `routes/search.rs` 私有函数）
//! - [`provider`]：[`SearchProvider`](provider::SearchProvider) trait 本体
//! - [`providers`]：具体源的实现（MA2a 起 `serpapi` + `google_patents_xhr`，
//!   MA2b 起追加 `epo_ops`；MA4 的本地 FTS 兜底源在此追加）
//! - [`chain`]：[`SourceChain`](chain::SourceChain) 多源并行执行链与降级择胜规则（MA2a）
//! - [`breaker`]：源级熔断冷却表 —— `FailKind::cools_down()` 的生产消费点（MA6a）
//!
//! ## MA1 边界（有意不做的事）
//! - ~~不做新源接入（Google Patents XHR / EPO OPS 属 MA2）~~
//!   **MA2a 已接入 Google Patents XHR、MA2b 已接入 EPO OPS（执行链第三源）**；
//! - ~~不做并行执行链与源级熔断（spec §6 属 MA2），故本层部分判定方法（`FailKind::switches_source`
//!   等）尚无消费方，已就地标注 `#[allow(dead_code)]` 并说明归属里程碑~~
//!   **MA2a 起 `switches_source` 已被消费**（`providers::google_patents_xhr` 的重试判定 +
//!   `chain` 的 Parse 截断）；源级熔断 ~~（连续 3 次 Quota/Auth 冷却 10 分钟）
//!   仍属 MA5/MA6~~ **MA6a 已落地**（[`breaker`]：单次 Quota/Auth 即按
//!   `SourceKind × FailKind` 常量表冷却，进程内共享，接入点在
//!   `routes/search.rs::api_search_online`；「连续 3 次」阈值被有意放弃，理由见
//!   `breaker.rs` 模块头与规格书 §6）；
//! - ~~不做本地 FTS provider（本地兜底链路仍留在 `routes/search.rs`，单一写入口改造属 MA4）~~
//!   **MA4a 已交付本地兜底的链上记账**（`routes/search.rs::local_fallback_json` 把每次
//!   兜底补记为 attempts 末条 `local_fts`，`SourceKind::LocalFts` 自此有真实生产消费点）；
//!   「兜底改造为 provider 进链」仍不做——那会让出参 `source` 变 `"local_fts"` 破坏旧形状，
//!   ~~跨源合并去重（`MergedPatent.key`）仍属 MA4 剩余项~~ **MA6c 已裁决不做**
//!   （2026-09-28：执行链按登记顺序择单一胜者、胜出源整份返回，结构上无第二个源的输入
//!   可拼接，该键从未有生产消费点，字段已删除；去重由 [`merge::dedup_patent_summaries`]
//!   服务本地 `/api/search` 路径，依据见规格书 §6）；
//!   assignee 精确匹配 ~~仍属 MA4 剩余项~~ **MA4b 已交付**；
//! - 不改任何对外 API 形状：`/api/search/online` 的响应字段、上游请求参数、结果映射
//!   与迁移前逐字一致，由 `providers::serpapi` 与 `query` 的参照实现单测锁死。

pub mod breaker;
pub mod chain;
pub mod merge;
pub mod model;
pub mod provider;
pub mod providers;
pub mod query;
pub mod relevance;
