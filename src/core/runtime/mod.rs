//! 运行时调度、9 服务商容灾路由与一源四出编译内核（`src/core/runtime/` · T-205）
//!
//! - [`llm_router`]：9 服务商自动容灾、看图模型路由、60s 硬超时、日预算与 `<user_input>` 隔离
//! - [`dag_runner`]：三档运行深度、任意步骤分叉 Fork、人工卡点暂停恢复与统一 SSE 事件流
//! - [`forge_export`]：六大体裁 × 五种格式（`.docx` 含修订留痕 / `.tex`+`.bib` / `.xlsx` / `.md` / `.pdf`）统一编译器

pub mod dag_runner;
pub mod forge_export;
pub mod llm_router;

pub use dag_runner::{DagRunner, TierDepthMode};
pub use forge_export::{CompileDeliverableRequest, CompiledDeliverable, ForgeExporter};
pub use llm_router::{LlmProviderSpec, LlmRouter};

/// 对上层 7 张卡片暴露的统一运行时与成果锻造门面（§3.2.5 `RuntimeForgeService`）
pub struct RuntimeForgeService;
