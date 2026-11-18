//! 卡片 ⑥：全谱系成果锻造工坊（`src/cards/deliverable_forge/`）
//!
//! 严格遵循 `AGENTS.md` 公理 1（独立自治）与公理 6（一源四出 + 先锁专利申请日后公开时序守卫）。
//! 仅允许调用 `crate::core::*` 与 `crate::routes::AppState`，严禁引用 `crate::cards::*` 下其他卡片模块。

pub mod prompts;
pub mod routes;
pub mod service;

pub use routes::*;
