//! 领域类型地基 / Domain Type Foundation（T1.1 缩水版）
//!
//! 按 `docs/analysis/types-migration-map.md` 的归属规则，把**领域公共类型**从
//! route / module 文件中抽出归位到本目录，为后续 workspace 化（`crates/types`）预留落位。
//!
//! 本缩水版只拆 **search / patent / chat / idea 四域**（M-A/M-B 施工要碰的域）：
//!
//! - [`patent`] — 专利核心与法律状态（`Patent`、`canonical_patent_key` 等）
//! - [`search`] — 检索请求/响应与命中（`SearchRequest`、`SearchResult`、`PatentSummary` 等）
//! - [`chat`] — AI 对话请求/响应与聊天记录（`AiChatRequest`、`ChatMessage` 等）
//! - [`idea`] — 创意与权利要求（`Idea`、`FeatureCard`、`ClaimNode` 等）
//!
//! 未纳入本次拆分的族（CAD 族、pipeline 内部类型、`AppConfig`/`AppState`）留在原处，
//! 由 types-migration-map 的后续任务（T1.2/T3.0）处理。
//!
//! ## 兼容门面 / Compatibility facade
//!
//! [`crate::patent`] 保留为**公共类型门面**（AGENTS.md §2.2），内部 `pub use` 本目录，
//! 因此既有调用点（`crate::patent::*`、`innoforge::patent::*`）路径不变。
//! 新代码请直接引用本目录的域模块。
//!
//! ## 不变式
//!
//! 迁移**只搬定义、不改字段与 serde 属性**。[`serde_snapshot`] 用逐字段 JSON 快照钉住
//! 序列化形状，任何字段增删/改名/`skip_serializing_if` 变化都会被测试拦下。

pub mod chat;
pub mod idea;
pub mod patent;
pub mod search;

#[cfg(test)]
mod serde_snapshot;
