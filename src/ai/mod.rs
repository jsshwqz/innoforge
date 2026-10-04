//! AI 多模型容灾客户端 / Multi-Provider AI Client with Failover
//!
//! 支持 6 种 AI 服务商自动切换：智谱 GLM、OpenRouter、Gemini、OpenAI、NVIDIA、DeepSeek。
//!
//! 模块结构：
//! - client: 核心 HTTP 客户端与多 provider failover
//! - chat: 聊天接口（单轮/多轮/流式）
//! - evidence: 证据自动收集与整理
//! - patent: 专利分析（摘要/权利要求/侵权/对比/批量）
//! - fact_check: OA 分析事实校验层（防幻觉/A33合规/数据来源）
//! - idea: 创意分析与图片描述
//! - providers: 御三家预设配置（OpenAI + Claude + Gemini）

mod chat;
pub mod client;
mod evidence;
mod fact_check;
mod idea;
pub mod patent;
mod providers;
mod tests;

pub(crate) use client::{
    oa_capacity_error, OA_DISCUSSION_ANALYSIS_MAX_CHARS, OA_DISCUSSION_HISTORY_MAX_CHARS,
    OA_DISCUSSION_OA_MAX_CHARS, OA_RESPONSE_ANALYSIS_MAX_CHARS, OA_RESPONSE_DISCUSSION_MAX_CHARS,
    OA_RESPONSE_OA_MAX_CHARS,
};
#[allow(unused_imports)]
pub use client::{safe_truncate_chars, truncate_for_ai, AiClient, AiUsageInfo, Message};
#[allow(unused_imports)]
pub use evidence::{collect_evidence, Evidence, EvidenceType};
#[allow(unused_imports)]
pub use fact_check::{check_oa_analysis, format_report, FactCheckReport, FactWarning};
#[allow(unused_imports)]
pub use providers::{find_preset, preset_ids, ProviderPreset, TOP3_PRESETS};
