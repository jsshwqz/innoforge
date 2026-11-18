//! 卡片 ④ 多方案对决与矩阵卡核心业务服务（`src/cards/scheme_matrix/service.rs`）
//!
//! 职责：
//! 1. 解析 2～6 个对比对象（本机 `doc_id`、`run_id` 或内联方案文本/参数）；
//! 2. 生成三视角结构化行列矩阵（`engineering_spec` / `paper_baseline` / `claim_chart`）；
//! 3. 执行纯字符数组（`Vec<char>`）线性空间 LCS / Hirschberg 语义差异比对（零 `[..` 切片语法）；
//! 4. 调用 `EvidenceRebuttalService::detect_cross_embodiment_stitching` 输出跨实施例强行拼凑预警；
//! 5. 持久化矩阵结果到 SQLite `app_settings`（`matrix:{id}`），并调用 `ForgeExporter` 导出 `.xlsx` / `.pdf`。

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::cards::scheme_matrix::prompts::{build_matrix_synthesis_prompt, MATRIX_SYSTEM_PROMPT};
use crate::core::evidence_rebuttal::EvidenceRebuttalService;
use crate::core::runtime::llm_router::{builtin_nine_providers, LlmProviderSpec, LlmRouter};
use crate::core::runtime::{CompileDeliverableRequest, CompiledDeliverable, ForgeExporter};
use crate::core::vault::models::{
    DeliverableFormat, DeliverableGenre, FilingStatus, KnowledgeDoc, StitchingAlert,
};
use crate::core::vault::VaultService;
use crate::routes::AppState;

const MATRIX_SETTING_PREFIX: &str = "matrix:";
const MAX_DIFF_CHARS: usize = 3000;
const MAX_PREVIEW_CHARS: usize = 160;

/// 矩阵三重视角枚举
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatrixMode {
    #[default]
    EngineeringSpec,
    PaperBaseline,
    ClaimChart,
}

impl MatrixMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EngineeringSpec => "engineering_spec",
            Self::PaperBaseline => "paper_baseline",
            Self::ClaimChart => "claim_chart",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim() {
            "paper_baseline" => Self::PaperBaseline,
            "claim_chart" => Self::ClaimChart,
            _ => Self::EngineeringSpec,
        }
    }
}

/// 单个对比对象输入源（支持 `doc_id` / `run_id` / 内联文本）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemeSourceInput {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub doc_id: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub inline_text: Option<String>,
    #[serde(default)]
    pub metrics: Option<serde_json::Value>,
    #[serde(default)]
    pub role: Option<String>,
}

/// 归一化后的对比方案实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedScheme {
    pub key: String,
    pub label: String,
    pub source_kind: String,
    pub doc_id: Option<String>,
    pub run_id: Option<String>,
    pub role: String,
    pub venue_or_status: String,
    pub filing_status: FilingStatus,
    pub doi_verified: bool,
    pub content_text: String,
    pub metrics: serde_json::Value,
    pub feature_anchors: Vec<String>,
    pub stitching_probe_tokens: Vec<String>,
}

/// Hirschberg 字符级差异片段
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HirschbergDiffOp {
    pub op: String,
    pub text: String,
}

/// 两两方案语义差异摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairwiseDiffSummary {
    pub base_key: String,
    pub target_key: String,
    pub base_label: String,
    pub target_label: String,
    pub similarity_ratio: f64,
    pub added_chars: usize,
    pub removed_chars: usize,
    pub equal_chars: usize,
    pub ops: Vec<HirschbergDiffOp>,
}

/// 单元格引证锚点（与前端 `<if-matrix-table>` 契约对齐）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatrixCellAnchor {
    pub doc_id: String,
    pub anchor_tag: String,
    pub verdict: String,
    pub summary: String,
}

/// 矩阵单元格值（与前端 `<if-matrix-table>` 契约对齐）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixCellValue {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone: Option<String>,
    #[serde(default)]
    pub anchors: Vec<MatrixCellAnchor>,
}

/// 矩阵列定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixColumn {
    pub key: String,
    pub label: String,
    pub role: String,
}

/// 矩阵行定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatrixRow {
    pub id: String,
    pub dimension: String,
    pub cross_embodiment_warning: bool,
    pub cells: serde_json::Map<String, serde_json::Value>,
}

/// 单视角结构化矩阵
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerspectiveMatrix {
    pub mode: String,
    pub title: String,
    pub columns: Vec<MatrixColumn>,
    pub rows: Vec<MatrixRow>,
}

/// 多方案对决完整结果（持久化于 SQLite `app_settings` `matrix:{id}`）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemeMatrixResult {
    pub matrix_id: String,
    pub title: String,
    pub active_mode: String,
    pub created_at: String,
    pub schemes: Vec<ResolvedScheme>,
    pub perspectives: BTreeMap<String, PerspectiveMatrix>,
    pub diffs: Vec<PairwiseDiffSummary>,
    pub stitching_alerts: Vec<StitchingAlert>,
    pub fusion_recommendations: Vec<String>,
    pub ai_synthesis: Option<String>,
}

/// 安全按字符数截断（绝不使用字节切片）
fn safe_truncate_chars(input: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (count, ch) in input.chars().enumerate() {
        if count >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

/// 滚动两行计算 LCS DP 最后一行（空间复杂度 O(n)）
fn lcs_last_row(a: &[char], b: &[char]) -> Vec<u32> {
    let n = b.len();
    let mut prev = vec![0u32; n + 1];
    let mut cur = vec![0u32; n + 1];
    for &ca in a {
        for j in 1..=n {
            let b_ch = b.get(j - 1).copied().unwrap_or('\0');
            cur[j] = if ca == b_ch {
                prev[j - 1] + 1
            } else {
                prev[j].max(cur[j - 1])
            };
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev
}

/// Hirschberg 线性空间分治重建 LCS 编辑序列（`=` / `-` / `+`）
///
/// 全程使用 `.split_at(...)` 与 `.get(...)`，绝不使用 `[..` 切片语法。
fn lcs_diff_ops(a: &[char], b: &[char], ops: &mut Vec<(char, char)>) {
    if b.is_empty() {
        for &ch in a {
            ops.push(('-', ch));
        }
        return;
    }
    if a.is_empty() {
        for &ch in b {
            ops.push(('+', ch));
        }
        return;
    }
    if a.len() == 1 {
        let a0 = a.first().copied().unwrap_or('\0');
        match b.iter().position(|&c| c == a0) {
            Some(j) => {
                let (b_left, b_rest) = b.split_at(j);
                let (_, b_right) = b_rest.split_at(1);
                for &c in b_left {
                    ops.push(('+', c));
                }
                ops.push(('=', a0));
                for &c in b_right {
                    ops.push(('+', c));
                }
            }
            None => {
                for &c in b {
                    ops.push(('+', c));
                }
                ops.push(('-', a0));
            }
        }
        return;
    }

    let mid = a.len() / 2;
    let n = b.len();
    let (a_left, a_right) = a.split_at(mid);
    let left = lcs_last_row(a_left, b);
    let a_tail_rev: Vec<char> = a_right.iter().rev().copied().collect();
    let b_rev: Vec<char> = b.iter().rev().copied().collect();
    let right = lcs_last_row(&a_tail_rev, &b_rev);

    let mut best_k = 0usize;
    let mut best_score = left.first().copied().unwrap_or(0) + right.get(n).copied().unwrap_or(0);
    for k in 1..=n {
        let s = left.get(k).copied().unwrap_or(0) + right.get(n - k).copied().unwrap_or(0);
        if s > best_score {
            best_score = s;
            best_k = k;
        }
    }

    let (b_left, b_right) = b.split_at(best_k);
    lcs_diff_ops(a_left, b_left, ops);
    lcs_diff_ops(a_right, b_right, ops);
}

/// 基于纯 `Vec<char>` Hirschberg 算法的语义差异比对
pub fn compute_hirschberg_diff(
    base_key: &str,
    target_key: &str,
    base_label: &str,
    target_label: &str,
    a: &str,
    b: &str,
) -> PairwiseDiffSummary {
    let a_chars: Vec<char> = a.chars().take(MAX_DIFF_CHARS).collect();
    let b_chars: Vec<char> = b.chars().take(MAX_DIFF_CHARS).collect();

    let mut raw_ops: Vec<(char, char)> = Vec::new();
    lcs_diff_ops(&a_chars, &b_chars, &mut raw_ops);

    let mut ops: Vec<HirschbergDiffOp> = Vec::new();
    let mut buf_eq = String::new();
    let mut buf_rm = String::new();
    let mut buf_add = String::new();

    let mut equal_chars = 0usize;
    let mut removed_chars = 0usize;
    let mut added_chars = 0usize;

    let flush_rm_add =
        |ops: &mut Vec<HirschbergDiffOp>, buf_rm: &mut String, buf_add: &mut String| {
            if !buf_rm.is_empty() {
                ops.push(HirschbergDiffOp {
                    op: "remove".to_string(),
                    text: buf_rm.clone(),
                });
                buf_rm.clear();
            }
            if !buf_add.is_empty() {
                ops.push(HirschbergDiffOp {
                    op: "add".to_string(),
                    text: buf_add.clone(),
                });
                buf_add.clear();
            }
        };

    let flush_eq = |ops: &mut Vec<HirschbergDiffOp>, buf_eq: &mut String| {
        if !buf_eq.is_empty() {
            ops.push(HirschbergDiffOp {
                op: "equal".to_string(),
                text: buf_eq.clone(),
            });
            buf_eq.clear();
        }
    };

    for (code, ch) in raw_ops {
        match code {
            '=' => {
                equal_chars += 1;
                flush_rm_add(&mut ops, &mut buf_rm, &mut buf_add);
                buf_eq.push(ch);
            }
            '-' => {
                removed_chars += 1;
                flush_eq(&mut ops, &mut buf_eq);
                buf_rm.push(ch);
            }
            '+' => {
                added_chars += 1;
                flush_eq(&mut ops, &mut buf_eq);
                buf_add.push(ch);
            }
            _ => {}
        }
    }
    flush_eq(&mut ops, &mut buf_eq);
    flush_rm_add(&mut ops, &mut buf_rm, &mut buf_add);

    let denom = a_chars.len().max(b_chars.len()).max(1);
    let similarity_ratio = ((equal_chars as f64) / (denom as f64) * 1000.0).round() / 1000.0;

    PairwiseDiffSummary {
        base_key: base_key.to_string(),
        target_key: target_key.to_string(),
        base_label: base_label.to_string(),
        target_label: target_label.to_string(),
        similarity_ratio,
        added_chars,
        removed_chars,
        equal_chars,
        ops,
    }
}

/// 从本地 SQLite 解析单个对比对象
fn resolve_single_source(
    conn: &Connection,
    idx: usize,
    src: &SchemeSourceInput,
) -> Result<ResolvedScheme> {
    let key = format!("scheme_{}", idx + 1);
    let default_label = if src.label.trim().is_empty() {
        format!("方案 {}", idx + 1)
    } else {
        src.label.trim().to_string()
    };
    let mut metrics = src.metrics.clone().unwrap_or_else(|| json!({}));

    if let Some(doc_id) = src
        .doc_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let doc = VaultService::get_document(conn, doc_id)?
            .ok_or_else(|| anyhow!("未在知识金库中找到文档 doc_id={}", doc_id))?;
        let atoms = VaultService::list_feature_atoms(conn, doc_id).unwrap_or_default();
        let numerals = VaultService::get_figure_numerals(conn, doc_id).unwrap_or_default();

        let title = if !src.label.trim().is_empty() {
            src.label.trim().to_string()
        } else if !doc.title_zh.trim().is_empty() {
            doc.title_zh.clone()
        } else if !doc.title_en.trim().is_empty() {
            doc.title_en.clone()
        } else {
            doc.external_id.clone()
        };

        let mut parts = Vec::new();
        if !doc.abstract_text.trim().is_empty() {
            parts.push(doc.abstract_text.trim().to_string());
        }
        if !doc.claims_json.trim().is_empty() && doc.claims_json.trim() != "[]" {
            parts.push(doc.claims_json.trim().to_string());
        }
        if !doc.fulltext_raw.trim().is_empty() {
            parts.push(safe_truncate_chars(doc.fulltext_raw.trim(), 2000));
        }
        if let Some(extra) = src
            .inline_text
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            parts.push(extra.to_string());
        }

        let mut feature_anchors = Vec::new();
        let mut probe_tokens = Vec::new();
        for atom in &atoms {
            feature_anchors.push(format!("{} · {}", atom.anchor_tag, atom.tech_means));
            probe_tokens.push(atom.anchor_tag.clone());
            if !atom.tech_means.trim().is_empty() {
                probe_tokens.push(atom.tech_means.clone());
            }
        }
        for num in &numerals {
            probe_tokens.push(num.numeral_code.clone());
            if !num.part_name.trim().is_empty() {
                probe_tokens.push(num.part_name.clone());
            }
        }

        if let Some(obj) = metrics.as_object_mut() {
            obj.entry("doc_kind")
                .or_insert_with(|| json!(doc.doc_kind.as_str()));
            obj.entry("external_id")
                .or_insert_with(|| json!(doc.external_id));
            obj.entry("doi_verified")
                .or_insert_with(|| json!(doc.doi_verified));
        }

        return Ok(ResolvedScheme {
            key,
            label: title,
            source_kind: "vault_doc".to_string(),
            doc_id: Some(doc.doc_id),
            run_id: None,
            role: src
                .role
                .clone()
                .unwrap_or_else(|| doc.asset_role.as_str().to_string()),
            venue_or_status: format!(
                "{} / {}",
                doc.venue_or_authority,
                if doc.doi_verified {
                    "DOI Verified"
                } else {
                    doc.filing_status.as_str()
                }
            ),
            filing_status: doc.filing_status,
            doi_verified: doc.doi_verified,
            content_text: parts.join("\n"),
            metrics,
            feature_anchors,
            stitching_probe_tokens: probe_tokens,
        });
    }

    if let Some(run_id) = src
        .run_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let run = VaultService::get_dag_run(conn, run_id)?
            .ok_or_else(|| anyhow!("未找到推演运行记录 run_id={}", run_id))?;
        let nodes = VaultService::list_dag_nodes(conn, run_id).unwrap_or_default();

        let mut parts = vec![run.scheme_baseline_json.clone()];
        for node in &nodes {
            if !node.output_json.trim().is_empty() && node.output_json.trim() != "{}" {
                parts.push(format!("[{}] {}", node.step_key, node.output_json));
            }
        }
        if let Some(extra) = src
            .inline_text
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            parts.push(extra.to_string());
        }

        return Ok(ResolvedScheme {
            key,
            label: if src.label.trim().is_empty() {
                format!("推演方案 {}", run.run_id)
            } else {
                src.label.trim().to_string()
            },
            source_kind: "dag_run".to_string(),
            doc_id: run.mounted_own_doc_ids.first().cloned(),
            run_id: Some(run.run_id),
            role: src.role.clone().unwrap_or_else(|| "candidate".to_string()),
            venue_or_status: format!("DAG {} ({})", run.tier_mode, run.status.as_str()),
            filing_status: FilingStatus::UnfiledSecret,
            doi_verified: false,
            content_text: parts.join("\n"),
            metrics,
            feature_anchors: nodes
                .iter()
                .map(|n| format!("RunStep:{}", n.step_key))
                .collect(),
            stitching_probe_tokens: Vec::new(),
        });
    }

    let inline = src.inline_text.as_deref().unwrap_or("").trim();
    let metrics_text = if metrics.is_object() && !metrics.as_object().is_some_and(|m| m.is_empty())
    {
        metrics.to_string()
    } else {
        String::new()
    };
    if inline.is_empty() && metrics_text.is_empty() {
        return Err(anyhow!(
            "第 {} 个对比方案缺少有效内容（需提供 doc_id、run_id 或内联方案文本/指标）",
            idx + 1
        ));
    }

    let content_text = if inline.is_empty() {
        metrics_text
    } else {
        inline.to_string()
    };

    Ok(ResolvedScheme {
        key,
        label: default_label,
        source_kind: "inline".to_string(),
        doc_id: None,
        run_id: None,
        role: src.role.clone().unwrap_or_else(|| "candidate".to_string()),
        venue_or_status: "内联方案 / Inline Scheme".to_string(),
        filing_status: FilingStatus::UnfiledSecret,
        doi_verified: false,
        content_text,
        metrics,
        feature_anchors: Vec::new(),
        stitching_probe_tokens: Vec::new(),
    })
}

fn metric_str(metrics: &serde_json::Value, keys: &[&str], fallback: &str) -> String {
    if let Some(obj) = metrics.as_object() {
        for key in keys {
            if let Some(v) = obj.get(*key) {
                if let Some(s) = v.as_str() {
                    if !s.trim().is_empty() {
                        return s.trim().to_string();
                    }
                } else if !v.is_null() {
                    return v.to_string();
                }
            }
        }
    }
    fallback.to_string()
}

fn build_row(
    id: &str,
    dimension: &str,
    cross_embodiment_warning: bool,
    schemes: &[ResolvedScheme],
    cell_fn: impl Fn(usize, &ResolvedScheme) -> MatrixCellValue,
) -> MatrixRow {
    let mut cells = serde_json::Map::new();
    cells.insert(
        "dimension".to_string(),
        json!(MatrixCellValue {
            text: dimension.to_string(),
            badge: None,
            tone: None,
            anchors: Vec::new(),
        }),
    );
    for (idx, scheme) in schemes.iter().enumerate() {
        let val = cell_fn(idx, scheme);
        cells.insert(scheme.key.clone(), json!(val));
    }
    MatrixRow {
        id: id.to_string(),
        dimension: dimension.to_string(),
        cross_embodiment_warning,
        cells,
    }
}

fn build_columns(schemes: &[ResolvedScheme]) -> Vec<MatrixColumn> {
    let mut cols = vec![MatrixColumn {
        key: "dimension".to_string(),
        label: "对比维度 / Dimension".to_string(),
        role: "dimension".to_string(),
    }];
    for s in schemes {
        cols.push(MatrixColumn {
            key: s.key.clone(),
            label: s.label.clone(),
            role: s.role.clone(),
        });
    }
    cols
}

/// 生成三重视角矩阵（工程性能成本 / 学术 SOTA Baseline / 专利 Claim Chart）
fn build_perspectives(
    schemes: &[ResolvedScheme],
    diffs: &[PairwiseDiffSummary],
    stitching_alerts: &[StitchingAlert],
) -> BTreeMap<String, PerspectiveMatrix> {
    let cols = build_columns(schemes);
    let diff_by_target: BTreeMap<&str, &PairwiseDiffSummary> =
        diffs.iter().map(|d| (d.target_key.as_str(), d)).collect();
    let alert_by_doc: BTreeMap<&str, &StitchingAlert> = stitching_alerts
        .iter()
        .map(|a| (a.doc_id.as_str(), a))
        .collect();

    // 1. 工程性能与成本视角 (engineering_spec)
    let eng_rows = vec![
        build_row(
            "eng_source",
            "来源与资产角色",
            false,
            schemes,
            |_, s| MatrixCellValue {
                text: s.venue_or_status.clone(),
                badge: Some(s.role.clone()),
                tone: Some(if s.role == "own_asset" {
                    "verified".to_string()
                } else {
                    "neutral".to_string()
                }),
                anchors: Vec::new(),
            },
        ),
        build_row("eng_means", "核心技术手段", false, schemes, |_, s| {
            MatrixCellValue {
                text: safe_truncate_chars(&s.content_text, MAX_PREVIEW_CHARS),
                badge: None,
                tone: None,
                anchors: Vec::new(),
            }
        }),
        build_row(
            "eng_perf",
            "性能 / 时延 / 吞吐",
            false,
            schemes,
            |_, s| {
                let perf = metric_str(
                    &s.metrics,
                    &["performance", "latency", "throughput", "fps", "qps"],
                    "待补测工程基准指标",
                );
                MatrixCellValue {
                    text: perf,
                    badge: None,
                    tone: None,
                    anchors: Vec::new(),
                }
            },
        ),
        build_row(
            "eng_cost",
            "BOM 成本与资源开销",
            false,
            schemes,
            |_, s| {
                let cost = metric_str(
                    &s.metrics,
                    &["cost", "bom_cost", "memory", "power"],
                    "待评估物料/算力成本",
                );
                MatrixCellValue {
                    text: cost,
                    badge: None,
                    tone: None,
                    anchors: Vec::new(),
                }
            },
        ),
        build_row(
            "eng_risk",
            "可制造性与工程风险",
            false,
            schemes,
            |_, s| {
                let risk = metric_str(
                    &s.metrics,
                    &["risk", "manufacturability", "reliability"],
                    "需补充环境鲁棒性与量产验证",
                );
                MatrixCellValue {
                    text: risk,
                    badge: None,
                    tone: None,
                    anchors: Vec::new(),
                }
            },
        ),
    ];

    // 2. 学术 SOTA Baseline 增益视角 (paper_baseline)
    let paper_rows = vec![
        build_row(
            "paper_venue",
            "文献来源与 DOI 核验",
            false,
            schemes,
            |_, s| MatrixCellValue {
                text: s.venue_or_status.clone(),
                badge: Some(if s.doi_verified {
                    "DOI 已核验".to_string()
                } else {
                    "待核验".to_string()
                }),
                tone: Some(if s.doi_verified {
                    "verified".to_string()
                } else {
                    "warning".to_string()
                }),
                anchors: Vec::new(),
            },
        ),
        build_row(
            "paper_delta",
            "Hirschberg 方法差异度",
            false,
            schemes,
            |idx, s| {
                if idx == 0 {
                    MatrixCellValue {
                        text: "基准对照组 (Baseline)".to_string(),
                        badge: Some("Baseline".to_string()),
                        tone: Some("verified".to_string()),
                        anchors: Vec::new(),
                    }
                } else if let Some(d) = diff_by_target.get(s.key.as_str()) {
                    MatrixCellValue {
                        text: format!(
                            "相似度 {:.1}%（新增 {} 字 / 删减 {} 字）",
                            d.similarity_ratio * 100.0,
                            d.added_chars,
                            d.removed_chars
                        ),
                        badge: Some(format!("+{}/-{}", d.added_chars, d.removed_chars)),
                        tone: Some("neutral".to_string()),
                        anchors: Vec::new(),
                    }
                } else {
                    MatrixCellValue {
                        text: "未计算差异".to_string(),
                        badge: None,
                        tone: None,
                        anchors: Vec::new(),
                    }
                }
            },
        ),
        build_row(
            "paper_gain",
            "实验增益 / SOTA 指标",
            false,
            schemes,
            |_, s| {
                let gain = metric_str(
                    &s.metrics,
                    &["gain", "sota_gain", "accuracy", "f1", "bleu"],
                    "待关联 Exp 实测账本数据",
                );
                MatrixCellValue {
                    text: gain,
                    badge: None,
                    tone: None,
                    anchors: Vec::new(),
                }
            },
        ),
        build_row(
            "paper_ablation",
            "消融与局限性",
            false,
            schemes,
            |_, s| {
                let lim = metric_str(
                    &s.metrics,
                    &["ablation", "limitation", "limitations"],
                    "需补充消融实验对照与边界条件",
                );
                MatrixCellValue {
                    text: lim,
                    badge: None,
                    tone: None,
                    anchors: Vec::new(),
                }
            },
        ),
    ];

    // 3. 专利 Claim Chart 覆盖度视角 (claim_chart)
    let has_any_stitching = !stitching_alerts.is_empty();
    let claim_rows = vec![
        build_row(
            "claim_anchors",
            "独立权项与特征锚点",
            false,
            schemes,
            |_, s| {
                let anchors: Vec<MatrixCellAnchor> = s
                    .feature_anchors
                    .iter()
                    .take(4)
                    .map(|tag| MatrixCellAnchor {
                        doc_id: s.doc_id.clone().unwrap_or_default(),
                        anchor_tag: tag.clone(),
                        verdict: if s.doc_id.is_some() {
                            "verified".to_string()
                        } else {
                            "summary".to_string()
                        },
                        summary: tag.clone(),
                    })
                    .collect();
                let text = if s.feature_anchors.is_empty() {
                    safe_truncate_chars(&s.content_text, 120)
                } else {
                    format!("已提取 {} 条特征锚点", s.feature_anchors.len())
                };
                MatrixCellValue {
                    text,
                    badge: Some(format!("{} 锚点", s.feature_anchors.len())),
                    tone: Some(if s.feature_anchors.is_empty() {
                        "warning".to_string()
                    } else {
                        "verified".to_string()
                    }),
                    anchors,
                }
            },
        ),
        build_row(
            "claim_coverage",
            "区别特征与覆盖度",
            false,
            schemes,
            |idx, s| {
                if idx == 0 {
                    MatrixCellValue {
                        text: "我方/基准权利要求特征集".to_string(),
                        badge: Some("基准权项".to_string()),
                        tone: Some("verified".to_string()),
                        anchors: Vec::new(),
                    }
                } else if let Some(d) = diff_by_target.get(s.key.as_str()) {
                    let coverage = (d.similarity_ratio * 100.0).round();
                    let tone = if coverage >= 75.0 {
                        "danger"
                    } else if coverage >= 45.0 {
                        "warning"
                    } else {
                        "verified"
                    };
                    MatrixCellValue {
                        text: format!(
                            "文本重叠度 {:.1}%；差异新增 {} 字符",
                            coverage, d.added_chars
                        ),
                        badge: Some(format!("重叠 {:.0}%", coverage)),
                        tone: Some(tone.to_string()),
                        anchors: Vec::new(),
                    }
                } else {
                    MatrixCellValue {
                        text: "待比对".to_string(),
                        badge: None,
                        tone: None,
                        anchors: Vec::new(),
                    }
                }
            },
        ),
        build_row(
            "claim_stitching",
            "跨实施例拼凑风险预警",
            has_any_stitching,
            schemes,
            |_, s| {
                if let Some(doc_id) = s.doc_id.as_deref() {
                    if let Some(alert) = alert_by_doc.get(doc_id) {
                        return MatrixCellValue {
                            text: alert.rebuttal_rationale.clone(),
                            badge: Some("跨实施例拼凑预警".to_string()),
                            tone: Some("danger".to_string()),
                            anchors: Vec::new(),
                        };
                    }
                }
                MatrixCellValue {
                    text: "未检出跨实施例强行拼凑".to_string(),
                    badge: Some("正常".to_string()),
                    tone: Some("verified".to_string()),
                    anchors: Vec::new(),
                }
            },
        ),
        build_row(
            "claim_priority",
            "公开时序与申请状态",
            false,
            schemes,
            |_, s| {
                let tone = match s.filing_status {
                    FilingStatus::UnfiledSecret => "warning",
                    FilingStatus::PatentFiled => "verified",
                    FilingStatus::PublicDisclosed => "neutral",
                };
                MatrixCellValue {
                    text: format!("状态：{}", s.filing_status.as_str()),
                    badge: Some(s.filing_status.as_str().to_string()),
                    tone: Some(tone.to_string()),
                    anchors: Vec::new(),
                }
            },
        ),
    ];

    let mut map = BTreeMap::new();
    map.insert(
        MatrixMode::EngineeringSpec.as_str().to_string(),
        PerspectiveMatrix {
            mode: MatrixMode::EngineeringSpec.as_str().to_string(),
            title: "工程性能与成本对决矩阵".to_string(),
            columns: cols.clone(),
            rows: eng_rows,
        },
    );
    map.insert(
        MatrixMode::PaperBaseline.as_str().to_string(),
        PerspectiveMatrix {
            mode: MatrixMode::PaperBaseline.as_str().to_string(),
            title: "学术 SOTA Baseline 增益对决矩阵".to_string(),
            columns: cols.clone(),
            rows: paper_rows,
        },
    );
    map.insert(
        MatrixMode::ClaimChart.as_str().to_string(),
        PerspectiveMatrix {
            mode: MatrixMode::ClaimChart.as_str().to_string(),
            title: "专利 Claim Chart 覆盖度与抗辩矩阵".to_string(),
            columns: cols,
            rows: claim_rows,
        },
    );
    map
}

/// 生成确定性融合优化建议
fn build_fusion_recommendations(
    schemes: &[ResolvedScheme],
    diffs: &[PairwiseDiffSummary],
    stitching_alerts: &[StitchingAlert],
) -> Vec<String> {
    let mut recs = Vec::new();
    if let Some(first) = schemes.first() {
        recs.push(format!(
            "以「{}」为基准架构，优先保留其核心约束与已核验技术特征。",
            first.label
        ));
    }
    for d in diffs {
        if d.similarity_ratio >= 0.75 {
            recs.push(format!(
                "「{}」与基准「{}」文本相似度达 {:.1}%，建议在 Claim Chart 视角中重点剥离区别技术特征并防抵触申请。",
                d.target_label,
                d.base_label,
                d.similarity_ratio * 100.0
            ));
        } else {
            recs.push(format!(
                "「{}」相对基准「{}」引入了 {} 字符增量差异，可评估将高收益模块吸收至融合方案。",
                d.target_label, d.base_label, d.added_chars
            ));
        }
    }
    for alert in stitching_alerts {
        recs.push(format!(
            "【跨实施例预警】文献 {} 存在跨实施例拼凑风险（{}），审查答复或 FTO 评估时严禁直接组合。",
            alert.doc_id,
            alert.conflicting_embodiment_groups.join(" vs ")
        ));
    }
    let has_unfiled = schemes
        .iter()
        .any(|s| s.filing_status == FilingStatus::UnfiledSecret);
    if has_unfiled {
        recs.push("检测到未递交专利的保密方案（unfiled_secret），对外公开论文或代码前请先锁定专利申请日。".to_string());
    }
    recs
}

/// 尝试从 `app_settings` / `AppConfig` 构造可用 `LlmRouter`（无可用 Key 时静默返回 `None`）
fn build_optional_router(state: &AppState) -> Option<LlmRouter> {
    let settings = state.db.get_all_settings().unwrap_or_default();
    let mut providers: Vec<LlmProviderSpec> = builtin_nine_providers();

    let key_mapping = [
        ("deepseek", "DEEPSEEK_API_KEY"),
        ("qwen", "QWEN_API_KEY"),
        ("zhipu", "ZHIPU_API_KEY"),
        ("moonshot", "MOONSHOT_API_KEY"),
        ("openai", "OPENAI_API_KEY"),
        ("anthropic", "ANTHROPIC_API_KEY"),
        ("gemini", "GEMINI_API_KEY"),
        ("siliconflow", "SILICONFLOW_API_KEY"),
        ("openrouter", "OPENROUTER_API_KEY"),
    ];

    for prov in &mut providers {
        for (id, env_key) in &key_mapping {
            if prov.id == *id {
                if let Some(val) = settings.get(*env_key).filter(|v| !v.trim().is_empty()) {
                    prov.api_key = val.trim().to_string();
                } else if let Ok(val) = std::env::var(env_key) {
                    if !val.trim().is_empty() {
                        prov.api_key = val.trim().to_string();
                    }
                }
            }
        }
    }

    if !providers.iter().any(|p| p.is_usable()) {
        return None;
    }
    LlmRouter::new(providers, 200).ok()
}

/// 核心服务：执行 2～6 方案对决、Hirschberg 比对、跨实施例拼凑检测并持久化到 SQLite
pub async fn compare_schemes(
    state: &AppState,
    title: Option<String>,
    mode: Option<String>,
    sources: Vec<SchemeSourceInput>,
    include_ai_synthesis: bool,
) -> Result<SchemeMatrixResult> {
    if sources.len() < 2 || sources.len() > 6 {
        return Err(anyhow!(
            "对比方案数量需在 2 至 6 个之间（当前 {} 个） / Must provide 2 to 6 schemes",
            sources.len()
        ));
    }

    let active_mode = mode
        .as_deref()
        .map(MatrixMode::parse)
        .unwrap_or_default()
        .as_str()
        .to_string();

    let (schemes, stitching_alerts) = {
        let conn = state.db.conn();
        let mut resolved = Vec::with_capacity(sources.len());
        for (idx, src) in sources.iter().enumerate() {
            resolved.push(resolve_single_source(&conn, idx, src)?);
        }

        let mut alerts = Vec::new();
        let mut seen_docs = BTreeSet::new();
        for s in &resolved {
            if let Some(doc_id) = s.doc_id.as_deref() {
                if seen_docs.insert(doc_id.to_string()) && s.stitching_probe_tokens.len() >= 2 {
                    if let Ok(Some(alert)) =
                        EvidenceRebuttalService::detect_cross_embodiment_stitching(
                            &conn,
                            doc_id,
                            &s.stitching_probe_tokens,
                        )
                    {
                        alerts.push(alert);
                    }
                }
            }
        }
        (resolved, alerts)
    };

    let mut diffs = Vec::new();
    if let Some(base) = schemes.first() {
        for target in schemes.iter().skip(1) {
            diffs.push(compute_hirschberg_diff(
                &base.key,
                &target.key,
                &base.label,
                &target.label,
                &base.content_text,
                &target.content_text,
            ));
        }
    }

    let perspectives = build_perspectives(&schemes, &diffs, &stitching_alerts);
    let fusion_recommendations = build_fusion_recommendations(&schemes, &diffs, &stitching_alerts);

    let ai_synthesis = if include_ai_synthesis {
        if let Some(router) = build_optional_router(state) {
            let schemes_summary = schemes
                .iter()
                .map(|s| {
                    format!(
                        "- {} ({}): {}",
                        s.label,
                        s.role,
                        safe_truncate_chars(&s.content_text, 400)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let diff_summary = diffs
                .iter()
                .map(|d| {
                    format!(
                        "- {} vs {}: 相似度 {:.1}% (+{}/-{})",
                        d.base_label,
                        d.target_label,
                        d.similarity_ratio * 100.0,
                        d.added_chars,
                        d.removed_chars
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let stitching_summary = if stitching_alerts.is_empty() {
                "无跨实施例拼凑预警".to_string()
            } else {
                stitching_alerts
                    .iter()
                    .map(|a| format!("- {}: {}", a.doc_id, a.rebuttal_rationale))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            let prompt = build_matrix_synthesis_prompt(
                &active_mode,
                &schemes_summary,
                &diff_summary,
                &stitching_summary,
            );
            match router
                .chat_with_failover(MATRIX_SYSTEM_PROMPT, &prompt, false)
                .await
            {
                Ok((reply, _provider)) => Some(reply),
                Err(err) => {
                    tracing::warn!("方案矩阵 AI 融合建议生成降级跳过: {}", err);
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    let matrix_id = format!("mtx_{}", Uuid::new_v4().simple());
    let created_at = chrono::Utc::now().to_rfc3339();
    let final_title = title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            schemes
                .iter()
                .map(|s| s.label.as_str())
                .collect::<Vec<_>>()
                .join(" vs ")
        });

    let result = SchemeMatrixResult {
        matrix_id: matrix_id.clone(),
        title: final_title,
        active_mode,
        created_at,
        schemes,
        perspectives,
        diffs,
        stitching_alerts,
        fusion_recommendations,
        ai_synthesis,
    };

    let setting_key = format!("{MATRIX_SETTING_PREFIX}{matrix_id}");
    let serialized = serde_json::to_string(&result)?;
    state.db.set_setting(&setting_key, &serialized)?;

    Ok(result)
}

/// 从 SQLite `app_settings` 读取已保存的方案对决矩阵
pub fn get_matrix(state: &AppState, matrix_id: &str) -> Result<Option<SchemeMatrixResult>> {
    let setting_key = format!("{MATRIX_SETTING_PREFIX}{}", matrix_id.trim());
    match state.db.get_setting(&setting_key)? {
        Some(raw) => {
            let parsed: SchemeMatrixResult = serde_json::from_str(&raw)?;
            Ok(Some(parsed))
        }
        None => Ok(None),
    }
}

/// 调用 `ForgeExporter` 导出 `.xlsx` 或 `.pdf` 文件流
pub fn export_matrix(
    state: &AppState,
    matrix_id: &str,
    format_str: Option<&str>,
    mode_str: Option<&str>,
    acknowledge_priority_warning: bool,
) -> Result<CompiledDeliverable> {
    let matrix = get_matrix(state, matrix_id)?
        .ok_or_else(|| anyhow!("未找到矩阵记录 matrix_id={}", matrix_id))?;

    let format = match format_str
        .unwrap_or("xlsx")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "pdf" => DeliverableFormat::Pdf,
        _ => DeliverableFormat::Xlsx,
    };

    let target_mode = mode_str
        .map(MatrixMode::parse)
        .unwrap_or_else(|| MatrixMode::parse(&matrix.active_mode))
        .as_str()
        .to_string();

    let mut sections: Vec<(String, String)> = Vec::new();
    if let Some(persp) = matrix.perspectives.get(&target_mode) {
        let mut lines = Vec::new();
        for row in &persp.rows {
            let mut row_parts = Vec::new();
            for col in &persp.columns {
                if col.key == "dimension" {
                    continue;
                }
                let cell_text = row
                    .cells
                    .get(&col.key)
                    .and_then(|v| v.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                row_parts.push(format!("{}: {}", col.label, cell_text));
            }
            lines.push(format!("- 【{}】 {}", row.dimension, row_parts.join(" | ")));
        }
        sections.push((persp.title.clone(), lines.join("\n")));
    }

    if !matrix.diffs.is_empty() {
        let diff_lines = matrix
            .diffs
            .iter()
            .map(|d| {
                format!(
                    "- {} vs {}：相似度 {:.1}%（+{} / -{} 字符）",
                    d.base_label,
                    d.target_label,
                    d.similarity_ratio * 100.0,
                    d.added_chars,
                    d.removed_chars
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(("Hirschberg 语义差异摘要".to_string(), diff_lines));
    }

    if !matrix.stitching_alerts.is_empty() {
        let alert_lines = matrix
            .stitching_alerts
            .iter()
            .map(|a| format!("- [{}] {}", a.doc_id, a.rebuttal_rationale))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(("跨实施例拼凑预警清单".to_string(), alert_lines));
    }

    if !matrix.fusion_recommendations.is_empty() {
        sections.push((
            "多方案融合优化建议".to_string(),
            matrix.fusion_recommendations.join("\n"),
        ));
    }

    if let Some(ai) = &matrix.ai_synthesis {
        sections.push(("AI 深度融合建议".to_string(), ai.clone()));
    }

    let mut related_doc_ids = Vec::new();
    let mut reference_docs: Vec<KnowledgeDoc> = Vec::new();
    {
        let conn = state.db.conn();
        for s in &matrix.schemes {
            if let Some(doc_id) = &s.doc_id {
                related_doc_ids.push(doc_id.clone());
                if let Ok(Some(doc)) = VaultService::get_document(&conn, doc_id) {
                    reference_docs.push(doc);
                }
            }
        }
    }

    let req = CompileDeliverableRequest {
        title: matrix.title,
        genre: DeliverableGenre::SurveyFtoReport,
        format,
        sections,
        original_text_for_diff: None,
        amended_text_for_diff: None,
        related_doc_ids,
        reference_docs,
        knowhow_markers: Vec::new(),
        acknowledge_priority_warning,
    };

    let conn = state.db.conn();
    ForgeExporter::compile_deliverable(&conn, &req)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hirschberg_diff_multibyte_chinese_zero_panic() {
        let a = "一种基于自适应滤波器的高精度电压检测电路，包括采样电阻与差分放大器。";
        let b = "一种基于多级自适应滤波器的超低功耗电压检测电路，包括采样电容与差分放大器。";
        let diff = compute_hirschberg_diff("s1", "s2", "方案A", "方案B", a, b);
        assert!(diff.similarity_ratio > 0.5);
        assert!(diff.added_chars > 0);
        assert!(diff.removed_chars > 0);
        assert!(!diff.ops.is_empty());
    }

    #[test]
    fn hirschberg_diff_handles_empty_and_single_char() {
        let d1 = compute_hirschberg_diff("s1", "s2", "A", "B", "", "abc");
        assert_eq!(d1.added_chars, 3);
        assert_eq!(d1.removed_chars, 0);

        let d2 = compute_hirschberg_diff("s1", "s2", "A", "B", "x", "axb");
        assert_eq!(d2.equal_chars, 1);
        assert_eq!(d2.added_chars, 2);
    }

    #[test]
    fn build_perspectives_outputs_all_three_modes() {
        let s1 = ResolvedScheme {
            key: "scheme_1".to_string(),
            label: "我方方案".to_string(),
            source_kind: "inline".to_string(),
            doc_id: None,
            run_id: None,
            role: "own_asset".to_string(),
            venue_or_status: "Internal".to_string(),
            filing_status: FilingStatus::PatentFiled,
            doi_verified: false,
            content_text: "自适应动态阈值调度算法".to_string(),
            metrics: json!({"latency": "12ms", "cost": "$4.2"}),
            feature_anchors: vec!["Claim:1".to_string()],
            stitching_probe_tokens: vec![],
        };
        let s2 = ResolvedScheme {
            key: "scheme_2".to_string(),
            label: "竞品 SOTA".to_string(),
            source_kind: "inline".to_string(),
            doc_id: None,
            run_id: None,
            role: "competitor_wall".to_string(),
            venue_or_status: "IEEE 2025".to_string(),
            filing_status: FilingStatus::PublicDisclosed,
            doi_verified: true,
            content_text: "固定阈值静态调度算法".to_string(),
            metrics: json!({"latency": "28ms", "cost": "$6.8"}),
            feature_anchors: vec![],
            stitching_probe_tokens: vec![],
        };
        let diff = compute_hirschberg_diff(
            &s1.key,
            &s2.key,
            &s1.label,
            &s2.label,
            &s1.content_text,
            &s2.content_text,
        );
        let perspectives = build_perspectives(&[s1, s2], &[diff], &[]);
        assert!(perspectives.contains_key("engineering_spec"));
        assert!(perspectives.contains_key("paper_baseline"));
        assert!(perspectives.contains_key("claim_chart"));
    }
}
