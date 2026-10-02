//! 公共类型门面 / Public Type Facade
//!
//! 本文件曾是「无域边界的杂物桶」（types-migration-map.md §1）。T1.1 缩水版已把
//! **search / patent / chat / idea** 四域的公共类型归位到 [`crate::types`]，
//! 此处保留两件事：
//!
//! 1. **CAD 族类型**——尚未纳入缩水版范围（types-migration-map.md 的 `types/cad.rs`
//!    属后续任务），仍在本文件内定义；
//! 2. **公共门面 re-export**——AGENTS.md §2.2 要求公共类型统一从 `src/patent.rs` 可见，
//!    故本文件 `pub use` 四个域模块，使既有调用点（`crate::patent::*`、
//!    `innoforge::patent::*`）的路径与行为完全不变。新增代码请直接引用 `crate::types::*`。
//!
//! 迁移的零行为变化由 [`crate::types::serde_snapshot`] 的逐字段 JSON 快照钉住。
//! Core data structures: re-exports of domain types + the CAD family.

use serde::{Deserialize, Serialize};

// ── 公共类型门面 / Domain type facade（定义已归位 src/types/）────────────────
pub use crate::types::chat::{AiChatRequest, AiResponse, ChatMessage, SYSTEM_PROMPT_PRESETS};
pub use crate::types::idea::{
    ClaimNode, ClaimType, CreateFeatureCardRequest, Evidence, FeatureCard, Idea, IdeaChatRequest,
    IdeaSubmitRequest, IdeaSummary, ResearchState, TechnicalFeature, TextAttachment,
};
pub use crate::types::patent::{
    canonical_patent_key, FetchPatentRequest, ImportRequest, LegalEvent, LegalStatusResult, Patent,
};
pub use crate::types::search::{
    CategoryGroup, PatentSummary, PipelinePatentHit, ReferenceChunk, SearchRequest, SearchResult,
    SearchType,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CadContextKind {
    Idea,
    Patent,
    Oa,
}

impl CadContextKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idea => "idea",
            Self::Patent => "patent",
            Self::Oa => "oa",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CadValidation {
    pub valid: bool,
    #[serde(default)]
    pub fixed: bool,
    #[serde(default)]
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CadArtifact {
    pub id: String,
    pub context_kind: CadContextKind,
    pub context_id: String,
    pub parent_artifact_id: Option<String>,
    pub revision: i64,
    pub prompt: String,
    pub assumptions: Vec<String>,
    pub preview_rel_path: String,
    pub fcstd_rel_path: String,
    pub step_rel_path: Option<String>,
    pub validation: CadValidation,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CadDrawRequest {
    pub context_kind: CadContextKind,
    pub context_id: String,
    pub prompt: String,
    #[serde(default)]
    pub conversation_context: String,
    pub parent_artifact_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CadAvailability {
    Ready,
    Starting,
    Unavailable,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CadStatus {
    pub availability: CadAvailability,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CadDrawResponse {
    pub artifact: CadArtifact,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{canonical_patent_key, Patent, SearchType};
    use serde_json::json;

    /// 门面必须仍然把域类型暴露在同一路径下（既有调用点零改动的前提）。
    #[test]
    fn facade_still_exposes_domain_types() {
        let parsed: SearchType =
            serde_json::from_value(json!("Applicant")).expect("legacy search type via facade");
        assert_eq!(SearchType::Applicant, parsed);
        let minimal: Patent =
            serde_json::from_value(json!({"patent_number": "CN1A", "title": "T"}))
                .expect("legacy patent via facade");
        assert_eq!("CN1A", minimal.patent_number);
        assert!(minimal.grant_date.is_none());
        // canonical_patent_key 随专利核心族归位，仍从本模块可达
        assert_eq!("CN116401354", canonical_patent_key("CN 116401354 A"));
    }
}
