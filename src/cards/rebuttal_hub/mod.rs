//! 卡片 ⑤：审查与审稿三栖抗辩中心（`src/cards/rebuttal_hub/` · T-401）
//!
//! 严格遵守 `AGENTS.md` §〇 公理 1 & 公理 5：
//! - 卡片完全独立自治，严禁引用 `crate::cards::` 下任何其他卡片模块；
//! - 仅调用 `crate::core::evidence_rebuttal`、`crate::core::vault`、`crate::core::runtime` 与 `crate::routes::AppState`；
//! - 覆盖三大独立子模式：`patent_oa`（已有专利官方 OA/复审）、`patent_pre_mock`（新专利提交前魔鬼预审）、
//!   `paper_rebuttal`（期刊论文模拟同行评审与返修逐条回复 Response to Reviewers）。

pub mod prompts;
pub mod routes;
pub mod service;

pub use routes::*;
