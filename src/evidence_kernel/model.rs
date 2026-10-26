//! 证据内核数据契约 / Evidence kernel data contracts.
//!
//! 设计文档：`docs/design/evidence-kernel-v2.md`（含 ADR 与不变量 I1–I5）。
//! 所有类型均可 serde 序列化：内核报告以 JSON 进 pipeline 快照（ADR-8，不加表）。

use serde::{Deserialize, Serialize};

/// 锚点的定位种类判别（编号单独存 u32，判等用整数比较——零填充即自然相等）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LocatorKind {
    /// 权利要求 N（`[D1:权利要求2]`）
    Claim,
    /// 说明书第 N 段（`[D1:第0035段]`，35 与 0035 视为同一编号）
    Paragraph,
    /// 检索全文切片 N（`[引用3]` 这类 chunk 容器）
    Chunk,
}

/// 证据宇宙中一个可寻址的文本段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceSection {
    /// 规范段 id，如 `D1:para:35`、`D1:claim:2`、`引用3:chunk:0`
    pub section_id: String,
    pub kind: LocatorKind,
    pub number: u32,
    /// 手头真实持有的文本（切片/原文段/摘要）。绝不虚构。
    pub text: String,
}

/// 一份来源文档（一篇对比专利 / OA 文本 / 本申请文本 / 引用块）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceDoc {
    pub doc_id: String,
    pub kind: String,
    pub title: String,
    pub sections: Vec<SourceSection>,
}

/// 一次分析实际持有的全部可寻址文本。只由代码从 ctx 构造，编号可信（解 EK-P7）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceCorpus {
    pub docs: Vec<SourceDoc>,
}

impl SourceCorpus {
    pub fn section_count(&self) -> usize {
        self.docs.iter().map(|d| d.sections.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.section_count() == 0
    }
}

/// 解析后的锚点三元组（issue #79：doc_id × kind × number 精确匹配）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Anchor {
    pub doc_id: String,
    pub kind: LocatorKind,
    pub number: u32,
}

/// AI 文本中的一处证据引用（原始串 + 解析结果 + 可选引文）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceRef {
    /// 原始锚点串（如 `[D1:第0035段]`、`【依据：对比文件D1第35段】`），unparsed 时也保留
    pub raw: String,
    /// 解析失败为 None（Unparsed 与 Unverified 分开统计，#79）
    pub anchor: Option<Anchor>,
    /// 锚点所在句内的引文片段（可为空；为空时核验降级为仅锚点）
    pub quote: String,
}

/// 结论句种类。分类器（确定性启发式，ADR-3）产出。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    /// 断言来源文档内容/区别的事实句 —— 必须绑定已核验证据（不变量 I4）
    FactFromSource,
    /// 推论句（可无引用，但计入 coverage 视角，不进 grounding 分母的强约束）
    AnalysisInference,
    /// 法律论证句（应当/不具备/创造性……）
    LegalArgument,
    /// 豁免类：标题、建议、元信息、降级标注句
    Meta,
}

/// 单条结论句及其引用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Claim {
    pub id: String,
    pub sentence_index: usize,
    pub text: String,
    pub kind: ClaimKind,
    pub evidence: Vec<EvidenceRef>,
}

/// 单条证据引用的核验结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum VerifyStatus {
    /// 锚点精确命中 + 引文归一化后逐字存在于该段
    VerifiedExact { section_id: String },
    /// 锚点精确命中 + 引文近似匹配（3-gram Jaccard ≥ 0.85，分数明示）
    VerifiedApprox { section_id: String, score: f64 },
    /// 锚点精确命中但无引文（如 `[引用3]` 风格）——单独计量，不冒充逐字核验
    AnchorOnly { section_id: String },
    /// 三元组在 corpus 中无精确命中（含伪造段落号，负控 NC1）
    AnchorNotFound,
    /// 锚点命中但引文与该段对不上（best_score 明示近似程度）
    QuoteMismatch { best_score: f64 },
    /// 原始串无法解析为三元组
    UnparsedAnchor,
}

impl VerifyStatus {
    /// 是否算「逐字核验」及其 grounding 权重（ADR 修订，PR #125 复核 Blocking / 负控 NC4）：
    /// Exact/Approx=1.0；AnchorOnly（仅锚点、无引文）⇒ `None`——锚点存在（existence）
    /// 不等于有支撑（support），它单独走 `anchor_only` 计量与 critic 清单，
    /// 不得冒充核验抬分或越过决策门。
    pub fn verified_weight(&self) -> Option<f64> {
        match self {
            VerifyStatus::VerifiedExact { .. } | VerifyStatus::VerifiedApprox { .. } => Some(1.0),
            _ => None,
        }
    }
}

/// 断言与证据的最终关系。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Support,
    Contradict,
    Insufficient,
}

/// 一条 claim 的核验明细。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaimRecord {
    pub claim_id: String,
    pub sentence_index: usize,
    pub text: String,
    pub kind: ClaimKind,
    pub verifications: Vec<(EvidenceRef, VerifyStatus)>,
    /// 被降级标注（修复轮耗尽仍有未支撑事实断言，ADR-6）
    pub downgraded: bool,
}

impl ClaimRecord {
    /// Support 要求至少一条**逐字核验**（Exact/Approx）；仅锚点命中（AnchorOnly）
    /// 不算支撑（existence ≠ support，NC4），降级句恒 Insufficient。
    pub fn relation(&self) -> Relation {
        let has_verified = self
            .verifications
            .iter()
            .any(|(_, s)| s.verified_weight().is_some());
        if self.kind == ClaimKind::Meta || self.kind == ClaimKind::AnalysisInference {
            return Relation::Insufficient;
        }
        if has_verified && !self.downgraded {
            Relation::Support
        } else {
            Relation::Insufficient
        }
    }
}

/// 核验计量（#79：hallucination_rate 分母 = 可解析锚点数）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GroundingMetrics {
    pub fact_claims: usize,
    pub verified_exact: usize,
    pub verified_approx: usize,
    pub anchor_only: usize,
    pub anchor_not_found: usize,
    pub quote_mismatch: usize,
    pub unparsed_anchor: usize,
    /// 已核验事实句 / 事实句总数（无事实句时为 None）
    pub grounding: Option<f64>,
    /// (AnchorNotFound + QuoteMismatch) / 可解析锚点数；分母 0 时为 None（不许默认 0 分绿）
    pub hallucination_rate: Option<f64>,
}

/// 五维评分（grounding 为乘法门，见 rubric.rs）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RubricScore {
    pub grounding: f64,
    pub coverage: f64,
    pub legal_reasoning: f64,
    pub clarity: f64,
    pub consistency: f64,
    /// grounding × 其余四维加权；InsufficientEvidence 恒 0（不变量 I3）
    pub overall: f64,
}

/// 自我批判条目（未支撑句清单，修复循环的输入与产物）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub claim_id: String,
    pub sentence_index: usize,
    pub text: String,
    pub reason: String,
}

/// 内核整体状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelStatus {
    /// 事实句全部有证据支撑
    Ok,
    /// 存在降级标注（未支撑句被标【未证实】保留）
    Degraded,
    /// 空证据硬门：不产出事实断言（不变量 I3）
    InsufficientEvidence,
}

/// 内核完整输出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KernelReport {
    pub status: KernelStatus,
    pub claims: Vec<ClaimRecord>,
    pub metrics: GroundingMetrics,
    pub rubric: RubricScore,
    pub critique: Vec<Finding>,
    pub repair_rounds: u32,
    /// InsufficientEvidence 的原因说明（其余状态为空）
    pub insufficient_reason: String,
}
