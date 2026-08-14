use super::state::PipelineStep;
use serde::{Deserialize, Serialize};

fn default_branch() -> String {
    "main".to_string()
}

/// AI 调用成本记录 — 追踪每次 AI 调用的 token 消耗和估算成本
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCostRecord {
    pub id: String,
    pub pipeline_run_id: String,
    pub step: String,
    pub model: String,
    pub provider: String,
    pub timestamp: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub estimated_cost_cents: f64,
    pub duration_ms: i64,
}

/// RAG 参考切片 — 检索到的专利文档片段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceChunk {
    pub id: String,
    pub patent_id: String,
    pub chunk_index: i32,
    pub source_type: String,
    pub content: String,
    pub relevance_score: f32,
}

/// 实验执行结果
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExperimentResult {
    pub script_path: String,
    pub language: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub metrics: serde_json::Value,
    pub duration_ms: u64,
    pub success: bool,
}

/// 相似度条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarityEntry {
    pub source_id: String,
    pub source_title: String,
    pub source_type: String, // "web" or "patent"
    pub tfidf_score: f64,
    pub jaccard_score: f64,
    pub combined_score: f64,
}

/// 排序后的匹配结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedMatch {
    pub rank: usize,
    pub source_id: String,
    pub source_title: String,
    pub source_type: String,
    pub source_url: String,
    pub snippet: String,
    pub combined_score: f64,
    pub tokens: Vec<String>,
}

/// 现有技术聚类
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorArtCluster {
    pub cluster_id: usize,
    pub topic: String,
    pub patent_indices: Vec<usize>,
    pub representative_title: String,
    pub avg_similarity: f64,
}

/// 矛盾信号
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    pub source_a: String,
    pub source_b: String,
    pub dimension: String,
    pub signal_strength: f64,
    pub opportunity: String,
}

/// 新颖性评分细项
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    pub max_similarity: f64,
    pub avg_top5_similarity: f64,
    pub contradiction_bonus: f64,
    pub coverage_gap_bonus: f64,
    pub final_score: f64,
}

/// 步骤执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step: PipelineStep,
    pub duration_ms: u64,
    pub status: String,
    pub error: Option<String>,
}

/// 搜索结果（通用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub link: String,
    pub source: String,
}

/// 单个维度的推演结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionInsight {
    pub dimension: String,
    pub label: String,
    pub reasoning: String,
    pub key_insight: String,
}

/// 多维深度推演结果
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeepReasoningResult {
    pub dimensions: Vec<DimensionInsight>,
    pub synthesis: String,
    pub novel_directions: Vec<String>,
    pub blind_spots: Vec<String>,
}

/// 流水线进度消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineProgress {
    pub step: PipelineStep,
    pub step_index: usize,
    pub total_steps: usize,
    pub status: StepStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_step: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_progress: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Running,
    Done,
    Skipped,
    Error,
}

/// 智能体输出 — 带置信度和证据引用的 Agent 结果 / Agent output with confidence and evidence refs
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentOutput {
    /// 输出内容（字符串化存储，JSON 序列化）
    pub content: String,
    /// 置信度 0.0-1.0 / Confidence score
    pub confidence: f64,
    /// 证据引用 ID 列表 / Evidence reference IDs
    pub evidence_refs: Vec<String>,
    /// 自述不确定点 / Self-reported uncertainties
    pub uncertainty: Vec<String>,
    /// 自检反思结果 / Self-reflection result
    pub self_reflection: Option<String>,
    /// 执行耗时 / Duration in ms
    pub duration_ms: u64,
    /// Token 用量 / Token count used
    pub tokens_used: u64,
    /// 来源步骤 / Source pipeline step
    pub source_step: String,
}

/// 反思评估结果 — 多智能体 Pipeline 的质量评分 / Reflection result - quality scoring
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReflectionResult {
    /// 整体质量分 0.0-1.0 / Overall quality score
    pub quality_score: f64,
    /// 完整性得分 / Completeness score
    pub completeness_score: f64,
    /// 一致性得分 / Consistency score
    pub consistency_score: f64,
    /// 证据充分性得分 / Evidence score
    pub evidence_score: f64,
    /// 是否需要重试 / Whether retry is needed
    pub needs_retry: bool,
    /// 重试原因 / Retry reason
    pub retry_reason: Option<String>,
    /// 改进建议 / Improvement suggestions
    pub improvement_suggestions: Vec<String>,
    /// 评估来源步骤 / Source step that was evaluated
    pub evaluated_step: String,
}

/// 辩论合成分结果 — 多智能体分歧的合并结论 / Debate synthesis result
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DebateResult {
    /// 合成分结论 / Synthesized conclusion
    pub conclusion: String,
    /// 各方观点摘要 / Summary of each side
    pub perspectives: Vec<String>,
    /// 共识点 / Consensus points
    pub consensus: Vec<String>,
    /// 分歧点 / Divergence points
    pub divergences: Vec<String>,
    /// 最终推荐的决策 / Recommended decision
    pub recommendation: String,
}

/// 证据条目 — 从结论到原始来源的可追溯链 / Evidence entry — traceable link from conclusion to source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub id: String,
    pub idea_id: String,
    /// 结论描述 / Claim description
    pub claim: String,
    /// "patent" | "web" | "contradiction" | "scoring"
    pub source_type: String,
    pub source_id: String,
    pub source_title: String,
    pub source_url: String,
    /// 权利要求编号（预留）/ Patent claim number (reserved for future)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_number: Option<String>,
    /// 原文摘录 / Original text excerpt
    pub excerpt: String,
    /// "supports" | "contradicts" | "partial"
    pub relation: String,
    /// 0.0-1.0，算法计算非 AI 生成 / Algorithmic confidence, not AI-generated
    pub confidence: f64,
    /// 生成该证据的 pipeline 步骤 / Pipeline step that produced this evidence
    pub produced_by: String,
    pub created_at: String,
}

/// 研发状态机 — 跟踪研究过程中的假设、排除路径和开放问题
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResearchState {
    pub current_hypothesis: String,
    pub excluded_paths: Vec<String>,
    pub open_questions: Vec<String>,
    pub verified_claims: Vec<String>,
}

/// 流水线上下文 — 在步骤间传递的数据载体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineContext {
    pub idea_id: String,
    pub title: String,
    pub description: String,

    // Step 1: ParseInput
    pub keywords: Vec<String>,
    pub technical_domain: String,

    // Step 2: ExpandQuery
    pub expanded_queries: Vec<String>,

    // Step 3-4: Search
    pub web_results: Vec<SearchResult>,
    pub patent_results: Vec<SearchResult>,

    // Step 5: DiversityGate
    pub diversity_score: f64,
    pub coverage_dimensions: Vec<String>,
    pub retry_count: u32,

    // Step 6: ComputeSimilarity
    pub similarity_scores: Vec<SimilarityEntry>,

    // Step 7: RankAndFilter
    pub top_matches: Vec<RankedMatch>,

    // Step 8: PriorArtCluster
    pub prior_art_clusters: Vec<PriorArtCluster>,

    // Step 9: DetectContradictions
    pub contradictions: Vec<Contradiction>,

    // Step 10: ScoreNovelty
    pub novelty_score: f64,
    pub score_breakdown: ScoreBreakdown,

    // Step 11-12: AI Analysis
    pub ai_analysis: String,
    pub action_plan: String,

    // Step 16: GenerateOaResponse
    #[serde(default)]
    pub oa_response: String,

    // 多维深度推演结果
    #[serde(default)]
    pub deep_reasoning: DeepReasoningResult,

    // 证据链 / Evidence chain
    #[serde(default)]
    pub evidence_chain: Vec<Evidence>,

    // 持久化记忆条目 / Persistent memory entries extracted from this pipeline run
    #[serde(default)]
    pub memory_entries: Vec<crate::db::memory::IdeaMemory>,

    // 多智能体反思历史 / Multi-agent reflection history
    #[serde(default)]
    pub reflection_history: Vec<ReflectionResult>,

    // 辩论合成分结果 / Debate synthesis results
    #[serde(default)]
    pub debate_results: Vec<DebateResult>,

    // 智能体输出记录 / Agent output records
    #[serde(default)]
    pub agent_outputs: Vec<AgentOutput>,

    // 研发状态机 / Research state machine
    #[serde(default)]
    pub research_state: ResearchState,

    // 版本管理 / Version control
    #[serde(default = "default_branch")]
    pub branch_id: String,
    #[serde(default)]
    pub iteration_count: u32,
    #[serde(default)]
    pub parent_version_id: String,

    // 实验结果 / Experiment results
    #[serde(default)]
    pub experiment_results: Vec<ExperimentResult>,

    // 元数据
    pub current_step: PipelineStep,
    pub step_results: Vec<StepResult>,
}

impl PipelineContext {
    pub fn new(idea_id: &str, title: &str, description: &str) -> Self {
        Self {
            idea_id: idea_id.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            keywords: Vec::new(),
            technical_domain: String::new(),
            expanded_queries: Vec::new(),
            web_results: Vec::new(),
            patent_results: Vec::new(),
            diversity_score: 0.0,
            coverage_dimensions: Vec::new(),
            retry_count: 0,
            similarity_scores: Vec::new(),
            top_matches: Vec::new(),
            prior_art_clusters: Vec::new(),
            contradictions: Vec::new(),
            novelty_score: 0.0,
            score_breakdown: ScoreBreakdown::default(),
            ai_analysis: String::new(),
            action_plan: String::new(),
            oa_response: String::new(),
            deep_reasoning: DeepReasoningResult::default(),
            evidence_chain: Vec::new(),
            memory_entries: Vec::new(),
            reflection_history: Vec::new(),
            debate_results: Vec::new(),
            agent_outputs: Vec::new(),
            research_state: ResearchState::default(),
            branch_id: "main".to_string(),
            iteration_count: 0,
            parent_version_id: String::new(),
            experiment_results: Vec::new(),
            current_step: PipelineStep::ParseInput,
            step_results: Vec::new(),
        }
    }
}
