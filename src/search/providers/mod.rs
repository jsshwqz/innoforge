//! 内置检索源实现 / Built-in search providers
//!
//! - [`serpapi`]：付费主源（spec §3，MA1 自 `src/routes/search.rs` 抽出）。
//! - [`google_patents_xhr`]：Google Patents 隐藏 XHR 直抓源（spec §2，免费无 Key，MA2a）。
//! - [`epo_ops`]：EPO OPS 源（spec §4，免费注册、英文/EU 域，MA2b）。
//!
//! 三源由执行链 [`crate::search::chain::SourceChain`] 装进
//! `Vec<Arc<dyn SearchProvider>>`（trait 的 object-safety 由
//! `crate::search::provider` 的 `trait_is_dyn_compatible_and_registry_builds` 锁死）。
//! MA4 的本地 FTS 兜底源（spec §7）在此追加。

pub mod epo_ops;
pub mod google_patents_xhr;
pub mod serpapi;
