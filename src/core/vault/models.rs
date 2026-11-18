//! 全域统一数据模型与五大共享内核公共契约结构体（`src/core/vault/models.rs`）
//!
//! 合并承接原 `src/patent.rs` 与 `src/types/*`，所有公共结构体与枚举统一派生
//! `#[derive(Debug, Clone, Serialize, Deserialize)]`（遵守 `AGENTS.md` §三.3）。

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

// 兼容重导出存量模型，确保过渡期既有代码零破坏
pub use crate::patent::*;
pub use crate::types::*;

/// 统一文献资产类型（`knowledge_docs.doc_kind`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    Patent,
    JournalPaper,
    ConfPaper,
    Preprint,
    CodeRepo,
    Standard,
    InternalScheme,
}

impl DocKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Patent => "patent",
            Self::JournalPaper => "journal_paper",
            Self::ConfPaper => "conf_paper",
            Self::Preprint => "preprint",
            Self::CodeRepo => "code_repo",
            Self::Standard => "standard",
            Self::InternalScheme => "internal_scheme",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "patent" => Self::Patent,
            "journal_paper" => Self::JournalPaper,
            "conf_paper" => Self::ConfPaper,
            "preprint" => Self::Preprint,
            "code_repo" => Self::CodeRepo,
            "standard" => Self::Standard,
            "internal_scheme" => Self::InternalScheme,
            _ => Self::Patent,
        }
    }
}

/// 文献资产角色（`knowledge_docs.asset_role` · 支撑公理 2 存量与增量并行）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetRole {
    OwnAsset,
    CompetitorWall,
    ReviewCited,
    #[default]
    GeneralRef,
}

impl AssetRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OwnAsset => "own_asset",
            Self::CompetitorWall => "competitor_wall",
            Self::ReviewCited => "review_cited",
            Self::GeneralRef => "general_ref",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "own_asset" => Self::OwnAsset,
            "competitor_wall" => Self::CompetitorWall,
            "review_cited" => Self::ReviewCited,
            _ => Self::GeneralRef,
        }
    }
}

/// 专利递交与公开状态（`knowledge_docs.filing_status` · 支撑公理 6 公开时序守卫）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilingStatus {
    UnfiledSecret,
    PatentFiled,
    #[default]
    PublicDisclosed,
}

impl FilingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UnfiledSecret => "unfiled_secret",
            Self::PatentFiled => "patent_filed",
            Self::PublicDisclosed => "public_disclosed",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "unfiled_secret" => Self::UnfiledSecret,
            "patent_filed" => Self::PatentFiled,
            _ => Self::PublicDisclosed,
        }
    }
}

/// 统一四维文献资产实体（对应 `knowledge_docs` 表）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDoc {
    pub doc_id: String,
    pub doc_kind: DocKind,
    #[serde(default)]
    pub asset_role: AssetRole,
    #[serde(default)]
    pub external_id: String,
    #[serde(default)]
    pub title_zh: String,
    #[serde(default)]
    pub title_en: String,
    #[serde(default)]
    pub authors_or_assignees: String,
    #[serde(default)]
    pub pub_or_filing_date: String,
    #[serde(default)]
    pub venue_or_authority: String,
    #[serde(default)]
    pub filing_status: FilingStatus,
    #[serde(default)]
    pub doi_verified: bool,
    #[serde(default)]
    pub bibtex_raw: String,
    #[serde(default)]
    pub abstract_text: String,
    #[serde(default)]
    pub claims_json: String,
    #[serde(default)]
    pub fulltext_raw: String,
    #[serde(default)]
    pub fulltext_status: String,
    #[serde(default)]
    pub chunk_ver: i32,
}

/// 双栖结构化切片章节语义类别（`doc_chunks.section_kind`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    ClaimIndep,
    ClaimDep,
    Abstract,
    TechField,
    Background,
    Summary,
    Drawings,
    Embodiment,
    PaperIntro,
    PaperMethod,
    PaperExperiment,
    PaperTableFig,
    PaperLimitation,
    Fallback,
}

impl SectionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClaimIndep => "claim_indep",
            Self::ClaimDep => "claim_dep",
            Self::Abstract => "abstract",
            Self::TechField => "tech_field",
            Self::Background => "background",
            Self::Summary => "summary",
            Self::Drawings => "drawings",
            Self::Embodiment => "embodiment",
            Self::PaperIntro => "paper_intro",
            Self::PaperMethod => "paper_method",
            Self::PaperExperiment => "paper_experiment",
            Self::PaperTableFig => "paper_table_fig",
            Self::PaperLimitation => "paper_limitation",
            Self::Fallback => "fallback",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "claim_indep" => Self::ClaimIndep,
            "claim_dep" => Self::ClaimDep,
            "abstract" => Self::Abstract,
            "tech_field" => Self::TechField,
            "background" => Self::Background,
            "summary" => Self::Summary,
            "drawings" => Self::Drawings,
            "embodiment" => Self::Embodiment,
            "paper_intro" => Self::PaperIntro,
            "paper_method" => Self::PaperMethod,
            "paper_experiment" => Self::PaperExperiment,
            "paper_table_fig" => Self::PaperTableFig,
            "paper_limitation" => Self::PaperLimitation,
            _ => Self::Fallback,
        }
    }

    /// 章节语义先验权重（背景技术降权 0.75，独立权项与核心实验提权 1.15～1.20）
    pub fn rrf_weight(&self) -> f64 {
        match self {
            Self::Background => 0.75,
            Self::ClaimIndep => 1.20,
            Self::ClaimDep => 1.15,
            Self::PaperExperiment | Self::PaperMethod => 1.18,
            Self::Embodiment | Self::PaperTableFig => 1.12,
            Self::PaperLimitation => 1.10,
            _ => 1.0,
        }
    }
}

/// 专利与论文双栖结构化切片实体（对应 `doc_chunks` 表）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocChunk {
    #[serde(default)]
    pub chunk_id: i64,
    pub doc_id: String,
    pub section_kind: SectionKind,
    /// 标准锚点示例：`Claim:1`、`Desc:[0024]`、`Desc:[0023-0024]`、`Paper:Sec3.1`、`Paper:Table2`
    pub section_tag: String,
    pub claim_no: Option<i32>,
    pub parent_claim_no: Option<i32>,
    pub para_start: Option<i32>,
    pub para_end: Option<i32>,
    pub embodiment_group: Option<String>,
    pub raw_content: String,
    pub fts_tokens: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_blob: Option<Vec<u8>>,
    pub char_count: usize,
}

/// 专利附图部件标号词典项（对应 `figure_numerals` 表）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FigureNumeral {
    #[serde(default)]
    pub id: i64,
    pub doc_id: String,
    #[serde(default)]
    pub figure_label: String,
    pub numeral_code: String,
    pub part_name: String,
    pub embodiment_group: Option<String>,
    pub anchor_tag: String,
}

/// 经核实技术特征原子（对应 `feature_atoms` 表）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureAtom {
    pub atom_id: String,
    pub doc_id: String,
    pub anchor_tag: String,
    #[serde(default)]
    pub tech_problem: String,
    pub tech_means: String,
    #[serde(default)]
    pub tech_effect: String,
    pub created_at: String,
}

/// 对照实验记录状态（`experiment_records.status`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    Designed,
    SandboxRan,
    RealDataFilled,
    Verified,
}

impl ExperimentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Designed => "designed",
            Self::SandboxRan => "sandbox_ran",
            Self::RealDataFilled => "real_data_filled",
            Self::Verified => "verified",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "sandbox_ran" => Self::SandboxRan,
            "real_data_filled" => Self::RealDataFilled,
            "verified" => Self::Verified,
            _ => Self::Designed,
        }
    }
}

/// 对照实验与实测账本记录（对应 `experiment_records` 表）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentRecord {
    pub exp_id: String,
    pub run_id: Option<String>,
    pub title: String,
    pub hypothesis: String,
    pub control_group_spec: String,
    pub test_group_spec: String,
    pub sandbox_output_json: Option<String>,
    pub measured_data_json: Option<String>,
    pub status: ExperimentStatus,
}

impl ExperimentRecord {
    /// 当真实测量数据回填或核验通过后，生成标准铁证锚点 `Exp:{exp_id}`
    pub fn evidence_anchor(&self) -> Option<String> {
        if matches!(
            self.status,
            ExperimentStatus::RealDataFilled | ExperimentStatus::Verified
        ) {
            Some(format!("Exp:{}", self.exp_id))
        } else {
            None
        }
    }
}

/// 可分叉任务运行状态（`dag_runs.status`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DagRunStatus {
    Chatting,
    Running,
    PausedCheckpoint,
    Completed,
    Failed,
}

impl DagRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Chatting => "chatting",
            Self::Running => "running",
            Self::PausedCheckpoint => "paused_checkpoint",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "running" => Self::Running,
            "paused_checkpoint" => Self::PausedCheckpoint,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            _ => Self::Chatting,
        }
    }
}

/// 可分叉任务主记录（对应 `dag_runs` 表）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagRun {
    pub run_id: String,
    pub card_kind: String,
    pub parent_run_id: Option<String>,
    pub fork_from_step: Option<String>,
    pub tier_mode: String,
    pub scheme_baseline_json: String,
    pub mounted_own_doc_ids: Vec<String>,
    pub status: DagRunStatus,
    pub created_at: String,
    pub updated_at: String,
}

/// 可分叉任务步骤节点快照（对应 `dag_nodes` 表）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    pub node_id: String,
    pub run_id: String,
    pub step_key: String,
    pub step_index: i32,
    pub status: String,
    pub input_json: String,
    pub output_json: String,
    pub checkpoint_payload_json: Option<String>,
    pub updated_at: String,
}

/// 混合检索场景模式（`VaultService::hybrid_search_chunks`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSceneMode {
    PatentCompare,
    PaperSurvey,
    AnchorExact,
}

/// 混合检索命中切片
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridChunkHit {
    pub chunk: DocChunk,
    pub rrf_score: f64,
    /// 自动回溯拼接后的完整上下文（如从属权项自动前置父权项链 + 邻接段）
    pub assembled_context: String,
}

/// 混合检索证据包
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridEvidenceBundle {
    pub query: String,
    pub mode: SearchSceneMode,
    pub hits: Vec<HybridChunkHit>,
    pub total_chars: usize,
    pub figure_glossary: Vec<FigureNumeral>,
}

/// 四维检索圈层枚举（`OmniSearchService`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchRealm {
    Patent,
    Scholar,
    CodeRepo,
    Standard,
}

/// 四维归一化检索命中项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmniSearchHit {
    pub doc: KnowledgeDoc,
    pub realm: SearchRealm,
    pub source_provider: String,
    pub relevance_score: f64,
    pub match_reason: String,
    pub source_url: String,
}

/// 引证锚点核验绿/黄/红三态（对应 `<if-citation-chip>` `verdict`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationVerdict {
    Verified,
    Summary,
    Unverified,
}

/// 单条引证 O(1) 回查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedCitation {
    pub raw_mention: String,
    pub doc_id: String,
    pub anchor_tag: String,
    pub verdict: CitationVerdict,
    pub matched_excerpt: String,
    pub reason: String,
}

/// 跨实施例强行拼凑特征预警
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StitchingAlert {
    pub doc_id: String,
    pub cited_features: Vec<String>,
    pub conflicting_embodiment_groups: Vec<String>,
    pub rebuttal_rationale: String,
}

/// 先专利申请日后论文/开源公开时序守卫结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityCheckResult {
    /// `green` / `yellow` / `red`
    pub lamp: String,
    pub has_conflict: bool,
    pub conflicting_doc_ids: Vec<String>,
    pub advisory_zh: String,
    pub advisory_en: String,
    pub stripped_knowhow_fields: Vec<String>,
}

/// 五维总闸门最终裁决（根治 `ERR-02`：核验未过严禁签发 `PassSubmittable`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateDecision {
    PassSubmittable,
    DowngradeNeedsEvidence,
    HardBlocked,
}

/// 五维质量与合规门禁体检单
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiveDimGateReport {
    pub decision: GateDecision,
    pub anchor_accuracy_score: f64,
    pub scope_compliance_score: f64,
    pub logic_completeness_score: f64,
    pub embodiment_consistency_score: f64,
    pub priority_privacy_score: f64,
    pub overall_score: f64,
    pub blocking_reasons: Vec<String>,
    pub downgrade_warnings: Vec<String>,
    pub verified_citations: Vec<VerifiedCitation>,
}

/// 一源四出（含三栖抗辩）六大导出体裁
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliverableGenre {
    ProductPrdTrd,
    SurveyFtoReport,
    AcademicPaper,
    PatentDisclosure,
    OaPackage,
    PaperRebuttalPackage,
}

/// 导出目标文件格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliverableFormat {
    Docx,
    Md,
    Tex,
    Xlsx,
    Pdf,
}

/// 全仓统一 SSE 流式事件信封（第五章契约）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum SseEnvelopeEvent {
    StageUpdate {
        stage: String,
        percent: u8,
        elapsed_ms: u64,
        message: String,
    },
    TokenDelta {
        delta: String,
    },
    CheckpointRequired {
        checkpoint_kind: String,
        candidates: serde_json::Value,
    },
    CitationAudit {
        citations: Vec<VerifiedCitation>,
    },
    GateResult {
        report: FiveDimGateReport,
    },
    Completed {
        artifact_id: String,
        summary: serde_json::Value,
    },
    Error {
        code: String,
        message_zh: String,
        message_en: String,
        recoverable_hint: String,
    },
}
