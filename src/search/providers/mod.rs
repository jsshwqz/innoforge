//! 内置检索源实现 / Built-in search providers
//!
//! MA1 只有 [`serpapi`]（现役唯一在线源，自 `src/routes/search.rs` 抽出）。
//! MA2 在此追加 `google_patents_xhr` / `epo_ops`，并由执行链装进
//! `Vec<Box<dyn SearchProvider>>`（trait 的 object-safety 由
//! `crate::search::provider` 的 `trait_is_dyn_compatible_and_registry_builds` 锁死）。

pub mod serpapi;
