//! 卡片 ③：单篇文献扒皮与深析卡（`src/cards/deep_dissect/`）
//!
//! 严格遵守 `AGENTS.md` §〇 公理 1（独立自治、互不捆绑前置工程）与公理 4（多模态复合输入与多轮共创）：
//! - 仅调用 `crate::core::*` 与 `crate::routes::AppState`，严禁引用 `crate::cards::*` 下其他卡片模块；
//! - 长专利与长论文无截断入库切分（`VaultService::upsert_document_with_chunks`）；
//! - 专利/论文双模式结构化深析 + O(1) 锚点核验 + `feature_atoms` 自动沉淀；
//! - 基于 `VaultService::hybrid_search_chunks` + 附图词典 `figure_numerals` 的多轮带图局部追问。

pub mod prompts;
pub mod routes;
pub mod service;

pub use routes::*;
