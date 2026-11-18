//! 卡片 ⑦：系统配置与知识金库卡（`src/cards/settings_vault/` · T-403）
//!
//! 严格遵守 `AGENTS.md` §二.2 卡片垂直隔离红线：
//! 仅依赖 `crate::core::*`、`crate::app::*` 与 `crate::routes::AppState`，严禁引用 `crate::cards::*` 下其他卡片模块。

pub mod prompts;
pub mod routes;
pub mod service;

pub use routes::*;
