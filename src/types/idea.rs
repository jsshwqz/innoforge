//! 创意域类型 / Idea domain types
//!
//! 自 `src/patent.rs` 迁出（T1.1 缩水版，types-migration-map.md §1「创意域」）。
//! 字段与 serde 属性逐字保持，仅更换定义位置。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Idea {
    pub id: String,
    pub title: String,
    pub description: String,
    pub input_type: String,
    pub status: String,
    pub analysis: String,
    pub web_results: String,
    pub patent_results: String,
    pub novelty_score: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub discussion_summary: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdeaSubmitRequest {
    pub title: String,
    pub description: String,
    #[serde(default = "default_text")]
    pub input_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextAttachment {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeaChatRequest {
    pub message: String,
    #[serde(default)]
    pub depth: String,
    #[serde(default)]
    pub images: Vec<String>,
    #[serde(default)]
    pub attachments: Vec<TextAttachment>,
}

fn default_text() -> String {
    "text".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdeaSummary {
    pub id: String,
    pub title: String,
    pub status: String,
    pub novelty_score: Option<f64>,
    pub created_at: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub message_count: i32,
}

/// 证据条目 — 从结论到原始来源的可追溯链 / Evidence entry — traceable link from conclusion to source
///
/// 自 `src/pipeline/context.rs` 迁出（types-migration-map.md §2：被 `db/evidence.rs` 反向引用，
/// 故归创意域）。字段与 serde 属性逐字保持。
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
///
/// 自 `src/pipeline/context.rs` 迁出（types-migration-map.md §2：被 `db/research_state.rs`
/// 反向引用）。字段与 serde 属性逐字保持。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResearchState {
    pub current_hypothesis: String,
    pub excluded_paths: Vec<String>,
    pub open_questions: Vec<String>,
    pub verified_claims: Vec<String>,
}

// ── Feature Cards ────────────────────────────────────────────────────────────

/// A feature card linked to an idea, capturing a specific inventive feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureCard {
    pub id: String,
    pub idea_id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub novelty_score: Option<f64>,
    pub created_at: String,
    // 5 维结构化字段
    #[serde(default)]
    pub technical_problem: String,
    #[serde(default)]
    pub core_structure: String,
    #[serde(default)]
    pub key_relations: String,
    #[serde(default)]
    pub process_steps: String,
    #[serde(default)]
    pub application_scenarios: String,
}

/// Request body for creating a new feature card.
#[derive(Debug, Deserialize)]
pub struct CreateFeatureCardRequest {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub novelty_score: Option<f64>,
    #[serde(default)]
    pub technical_problem: String,
    #[serde(default)]
    pub core_structure: String,
    #[serde(default)]
    pub key_relations: String,
    #[serde(default)]
    pub process_steps: String,
    #[serde(default)]
    pub application_scenarios: String,
}

// ── 权利要求树 / Claim Tree ──────────────────────────────────────

/// 权利要求节点
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ClaimNode {
    pub id: String,
    pub idea_id: String,
    pub claim_number: u32,
    pub claim_type: ClaimType,
    pub parent_claim_id: Option<String>,
    pub content: String,
    pub features: Vec<TechnicalFeature>,
    pub created_at: String,
}

/// 权利要求类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum ClaimType {
    Independent,
    Dependent,
}

/// 必要技术特征
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct TechnicalFeature {
    pub id: String,
    pub claim_id: String,
    pub description: String,
    pub novelty_flag: bool,
    pub evidence_ids: Vec<String>,
}
