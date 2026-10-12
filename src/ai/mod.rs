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
//! - whitespace_search: 去空格文本搜索（C1/C2/C9/C11/C12 纯函数基础设施）
//! - paraphrase_check: C1 转述失真检测（纯函数层）
//! - zero_evidence: C2 零举证检测
//! - tech_problem_check: C5 区别特征提取 + C11 技术问题失真检测
//! - search_report: C3 检索报告解析 + C7 从属权项评述 + C8 答复期限
//! - claim_coverage: C9 区别特征覆盖检测
//! - citation_check: C4 对比文件引文逐字核对
//! - targeted_claims: C6 针对权项 + 末段原文提取
//! - feature_attribution: C10 区别特征双向归属检测
//! - prohibition_check: C12 答复禁令自查
//! - golden_regression: C13 黄金回归测试

mod adversarial;
pub mod case_input;
mod chat;
#[allow(dead_code)] // 见文件头说明：lib/bin 双份编译，bin 侧尚未触达
pub mod citation_check;
#[allow(dead_code)] // 见文件头说明：lib/bin 双份编译，bin 侧尚未触达
pub mod claim_coverage;
pub mod client;
mod evidence;
#[allow(dead_code)] // 见文件头说明：lib/bin 双份编译，bin 侧尚未触达
pub mod extraction;
#[allow(dead_code)] // 见文件头说明：lib/bin 双份编译，bin 侧尚未触达
pub mod extraction_quality;
mod fact_check;
#[allow(dead_code)] // 见文件头说明：lib/bin 双份编译，bin 侧尚未触达
pub mod feature_attribution;
#[allow(dead_code)] // 见文件头说明：lib/bin 双份编译，bin 侧尚未触达
pub mod golden_regression;
mod idea;
pub mod output_guard;
#[allow(dead_code)] // 见文件头说明：lib/bin 双份编译，bin 侧尚未触达
pub mod paraphrase_check;
pub mod patent;
#[allow(dead_code)] // 见文件头说明：lib/bin 双份编译，bin 侧尚未触达
pub mod pre_submission_check;
pub mod prohibition_check;
mod providers;
pub mod response_pipeline;
#[allow(dead_code)] // 见文件头说明：lib/bin 双份编译，bin 侧尚未触达
pub mod search_report;
#[allow(dead_code)] // 见文件头说明：lib/bin 双份编译，bin 侧尚未触达
pub mod strategy_arbitration;
#[allow(dead_code)] // 见文件头说明：lib/bin 双份编译，bin 侧尚未触达
pub mod targeted_claims;
#[allow(dead_code)] // 见文件头说明：lib/bin 双份编译，bin 侧尚未触达
pub mod tech_problem_check;
mod tests;
#[allow(dead_code)] // 见文件头说明：lib/bin 双份编译，bin 侧尚未触达
pub mod whitespace_search;
pub mod zero_evidence;

#[allow(unused_imports)]
pub use adversarial::{
    run_adversarial_deliberation, AdversarialParams, AdversarialResult, AgentTurn, ExaminerTurn,
    RoundRecord,
};
pub(crate) use client::{
    oa_capacity_error, OA_DISCUSSION_ANALYSIS_MAX_CHARS, OA_DISCUSSION_HISTORY_MAX_CHARS,
    OA_DISCUSSION_OA_MAX_CHARS, OA_RESPONSE_ANALYSIS_MAX_CHARS, OA_RESPONSE_DISCUSSION_MAX_CHARS,
    OA_RESPONSE_OA_MAX_CHARS,
};
#[allow(unused_imports)]
pub use client::{
    safe_truncate_chars, truncate_for_ai, truncate_for_ai_internal, AiClient, AiUsageInfo, Message,
};
#[allow(unused_imports)]
pub use evidence::{collect_evidence, Evidence, EvidenceType};
#[allow(unused_imports)]
pub use fact_check::{check_oa_analysis, format_report, FactCheckReport, FactWarning};
#[allow(unused_imports)]
pub use providers::{find_preset, preset_ids, ProviderPreset, TOP3_PRESETS};
