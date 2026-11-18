//! 卡片 ④ 多方案对决与矩阵卡（`src/cards/scheme_matrix/` · T-305）
//!
//! 严格遵守 `AGENTS.md` §二.2 架构边界：
//! - 仅可调用 `crate::core::*` 与 `crate::routes::AppState`；
//! - 严禁引用 `crate::cards::` 下任何其他卡片模块。

pub mod prompts;
pub mod routes;
pub mod service;

pub use routes::*;
