//! 创研台 InnoForge v1.0 上层七大零耦合独立功能卡片分片（`src/cards/`）
//!
//! **强制红线（`AGENTS.md` §〇 公理 1 & §二.2）**：
//! - 首页 7 张功能卡片 100% 独立自治、即点即用、互不捆绑前置工程；
//! - `src/cards/*` 各子模块之间**严禁直接相互引用**（由静态架构守卫强制拦截），仅允许调用 `crate::core::*` 并经本机 SQLite 金库互通资产。

pub mod deep_dissect;
pub mod deliverable_forge;
pub mod idea_studio;
pub mod multimodal_api;
pub mod omni_search;
pub mod rebuttal_hub;
pub mod scheme_matrix;
pub mod settings_vault;
