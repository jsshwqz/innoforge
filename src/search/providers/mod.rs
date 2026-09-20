//! 内置检索源实现 / Built-in search providers
//!
//! - [`serpapi`]：付费主源（spec §3，MA1 自 `src/routes/search.rs` 抽出）。
//! - [`google_patents_xhr`]：Google Patents 隐藏 XHR 直抓源（spec §2，免费无 Key，MA2a）。
//!
//! MA2b 在此追加 `epo_ops`，并由执行链 [`crate::search::chain::SourceChain`] 装进
//! `Vec<Arc<dyn SearchProvider>>`（trait 的 object-safety 由
//! `crate::search::provider` 的 `trait_is_dyn_compatible_and_registry_builds` 锁死）。

pub mod google_patents_xhr;
pub mod serpapi;
