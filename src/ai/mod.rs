//! AI 多模型容灾客户端 / Multi-Provider AI Client with Failover
//!
//! 支持 6 种 AI 服务商自动切换：智谱 GLM、OpenRouter、Gemini、OpenAI、NVIDIA、DeepSeek。
//!
//! 模块结构：
//! - client: 核心 HTTP 客户端与多 provider failover
//! - chat: 聊天接口（单轮/多轮/流式）
//! - patent: 专利分析（摘要/权利要求/侵权/对比/批量）
//! - fact_check: OA 分析事实校验层（防幻觉/A33合规/数据来源，预留）
//! - idea: 创意分析与图片描述

mod chat;
mod client;
mod fact_check;
mod idea;
mod patent;
mod tests;

pub(crate) use client::{
    oa_capacity_error, OA_DISCUSSION_ANALYSIS_MAX_CHARS, OA_DISCUSSION_HISTORY_MAX_CHARS,
    OA_DISCUSSION_OA_MAX_CHARS,
};
#[allow(unused_imports)]
pub use client::{safe_truncate_chars, AiClient, Message};
// Note: fact_check is a预留 layer — currently not wired into the main OA flow.
// The functions are still public; downstream code can use them when ready.
#[allow(unused_imports)]
pub use fact_check::{check_oa_analysis, format_report, FactCheckReport, FactWarning};
