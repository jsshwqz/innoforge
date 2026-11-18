//! 卡片 ①：全域情报检索卡（`src/cards/omni_search/`）
//!
//! 严格对应 `AGENTS.md` 公理 1（独立自治）与公理 3（四维全域情报检索：
//! 全球专利 + 学术期刊/会议/预印本 + 开源代码工程 + 行业标准）。
//! 严禁直接引用 `crate::cards::` 下任何其他卡片模块，仅调用 `crate::core::*` 与 `crate::routes::AppState`。

pub mod prompts;
pub mod routes;
pub mod service;

pub use routes::*;
