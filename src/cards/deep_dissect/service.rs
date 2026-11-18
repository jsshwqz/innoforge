//! 卡片 ③ 单篇文献扒皮与深析核心服务（`src/cards/deep_dissect/service.rs`）
//!
//! 职责：
//! 1. **无截断入库切分**：支持纯文本/结构化字段或上传文件（PDF/DOCX/CSV/图像）经 `MultimodalService`
//!    三级自适应解析后，完整写入 `KnowledgeDoc` 并调用 `VaultService::upsert_document_with_chunks`
//!    重建全量切片与附图部件标号词典（绝不做 5000 字硬截断）；
//! 2. **双模式结构化深析**：支持专利（核心方案+附图连接关系 / 逐权项 FTO 排雷）与学术论文
//!    （机理白话拆解+复现超参数表 / 局限性与改进挖掘），优先走 `LlmRouter` 9 服务商容灾路由，
//!    离线或无可用 Key 时自动降级为确定性结构化提取；自动调用 `EvidenceRebuttalService::verify_all_citations`
//!    完成 O(1) 原文/DOI 锚点核验，并将技术问题-手段-效果沉淀写入 `feature_atoms`；
//! 3. **多轮带图局部追问**：基于 `VaultService::hybrid_search_chunks` + `VaultService::get_figure_numerals`
//!    召回高相关切片与部件标号，经 `wrap_user_input` 隔离防注入，支持附带截图与局部框选追问。

use std::collections::HashSet;

use anyhow::{anyhow, Result};
use base64::Engine;
use chrono::Utc;
use regex::Regex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::evidence_rebuttal::EvidenceRebuttalService;
use crate::core::multimodal::{IngestOptions, IngestTierUsed, MultimodalService};
use crate::core::runtime::llm_router::{
    builtin_nine_providers, ollama_local_provider, wrap_user_input, LlmProviderSpec, LlmRouter,
};
use crate::core::vault::models::{
    AssetRole, DocChunk, DocKind, FeatureAtom, FigureNumeral, FilingStatus, KnowledgeDoc,
    SearchSceneMode, SectionKind, VerifiedCitation,
};
use crate::core::vault::VaultService;
use crate::routes::AppConfig;

use super::prompts::{
    DISSECT_ASK_SYSTEM_PROMPT, PAPER_LIMITATION_PROMPT, PAPER_MECHANISM_REPRO_PROMPT,
    PATENT_FTO_PROMPT, PATENT_SCHEME_FIG_PROMPT,
};

/// 单篇深析入库输入载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissectIngestInput {
    pub doc_id: Option<String>,
    pub doc_kind: Option<String>,
    pub asset_role: Option<String>,
    pub filing_status: Option<String>,
    pub external_id: Option<String>,
    pub title: Option<String>,
    pub authors_or_assignees: Option<String>,
    pub pub_or_filing_date: Option<String>,
    pub venue_or_authority: Option<String>,
    pub abstract_text: Option<String>,
    pub claims_json: Option<String>,
    pub fulltext_raw: Option<String>,
    /// 可选：上传文件的文件名与 Base64 内容（支持 PDF/DOCX/CSV/图像/文本）
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub file_base64: Option<String>,
    /// 涉密离线锁：为 true 时严禁外部 Vision 大模型调用
    #[serde(default)]
    pub privacy_offline_only: bool,
}

/// 单篇深析入库返回结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissectIngestResult {
    pub doc: KnowledgeDoc,
    pub chunks: Vec<DocChunk>,
    pub figure_numerals: Vec<FigureNumeral>,
    pub existing_feature_atoms: Vec<FeatureAtom>,
    pub ingest_tier_used: Option<IngestTierUsed>,
    pub warnings: Vec<String>,
}

/// 单篇结构化深析输入载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissectAnalyzeInput {
    pub doc_id: String,
    /// `auto` / `patent` / `paper`
    pub mode: Option<String>,
    /// 子类型列表：`patent_scheme`, `patent_fto`, `paper_mechanism`, `paper_limitations`
    #[serde(default)]
    pub sub_types: Vec<String>,
    #[serde(default)]
    pub privacy_offline_only: bool,
}

/// 单个深析标签页分节结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissectSectionResult {
    pub sub_type: String,
    pub title_zh: String,
    pub title_en: String,
    pub markdown: String,
    pub source: String,
    pub provider_id: Option<String>,
}

/// 单篇结构化深析返回结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissectAnalyzeResult {
    pub doc_id: String,
    pub detected_mode: String,
    pub sections: Vec<DissectSectionResult>,
    pub report_markdown: String,
    pub verified_citations: Vec<VerifiedCitation>,
    pub saved_feature_atoms: Vec<FeatureAtom>,
    pub figure_numerals: Vec<FigureNumeral>,
}

/// 多轮带图局部追问输入载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissectAskInput {
    pub doc_id: String,
    pub question: String,
    pub selected_excerpt: Option<String>,
    pub selected_anchor: Option<String>,
    pub figure_numeral: Option<String>,
    /// 可选：局部截图 Data URL（如 `data:image/png;base64,...`）或纯 Base64
    #[serde(default)]
    pub image_data_urls: Vec<String>,
    #[serde(default)]
    pub privacy_offline_only: bool,
}

/// 多轮带图局部追问返回结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissectAskResult {
    pub doc_id: String,
    pub answer_markdown: String,
    pub verified_citations: Vec<VerifiedCitation>,
    pub matched_chunks: Vec<DocChunk>,
    pub matched_numerals: Vec<FigureNumeral>,
    pub answer_source: String,
    pub provider_id: Option<String>,
}

/// 字符边界安全截断（严禁裸字节切片）
fn safe_char_limit(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let taken: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{taken}...")
    } else {
        taken
    }
}

/// 剥离 Data URL 前缀或解码 Base64 字节
fn decode_base64_payload(raw: &str) -> Result<Vec<u8>> {
    let trimmed = raw.trim();
    let b64_part = match trimmed.find("base64,") {
        Some(pos) => trimmed.get(pos + 7..).unwrap_or(trimmed),
        None => trimmed,
    };
    base64::engine::general_purpose::STANDARD
        .decode(b64_part.trim())
        .map_err(|e| anyhow!("Base64 解码失败 / Invalid base64 payload: {e}"))
}

/// 从 `AppConfig` 构造 9 服务商容灾路由器（无可用 Key 时返回 None，触发确定性离线回退）
pub fn build_llm_router_from_config(cfg: &AppConfig) -> Option<LlmRouter> {
    let mut providers: Vec<LlmProviderSpec> = builtin_nine_providers()
        .into_iter()
        .map(|mut spec| {
            spec.api_key = match spec.id.as_str() {
                "deepseek" => cfg.ai_api_key_deepseek.clone(),
                "sensenova" => cfg.ai_api_key_sensetime.clone(),
                "zhipu" => cfg.ai_api_key_zhipu.clone(),
                "qwen" => cfg.ai_api_key_qwen.clone(),
                "openai" => cfg.ai_api_key_openai.clone(),
                "anthropic" => cfg.ai_api_key_anthropic.clone(),
                "gemini" => cfg.ai_api_key_gemini.clone(),
                "openrouter" => cfg.ai_api_key_openrouter.clone(),
                "xiaomi" => cfg.ai_api_key_xiaomi.clone(),
                _ => String::new(),
            };
            spec
        })
        .collect();

    if !cfg.ai_base_url.trim().is_empty() && !cfg.ai_api_key.trim().is_empty() {
        let model = if cfg.ai_model.trim().is_empty() {
            "gpt-4o-mini".to_string()
        } else {
            cfg.ai_model.clone()
        };
        let expert = if cfg.ai_model_expert.trim().is_empty() {
            model.clone()
        } else {
            cfg.ai_model_expert.clone()
        };
        providers.insert(
            0,
            LlmProviderSpec {
                id: "custom".to_string(),
                display_name: "Custom Endpoint".to_string(),
                base_url: cfg.ai_base_url.clone(),
                default_model: model.clone(),
                expert_model: expert,
                vision_model: Some(model),
                supports_vision: true,
                max_context_tokens: 128_000,
                api_key: cfg.ai_api_key.clone(),
            },
        );
    } else if cfg.ai_base_url.trim().starts_with("http://127.0.0.1")
        || cfg.ai_base_url.trim().starts_with("http://localhost")
    {
        let model = if cfg.ai_model.trim().is_empty() {
            None
        } else {
            Some(cfg.ai_model.as_str())
        };
        providers.push(ollama_local_provider(model, false));
    }

    if !providers.iter().any(|p| p.is_usable()) {
        return None;
    }
    LlmRouter::new(providers, 500).ok()
}

/// 单语句查询指定文献的全部切片（按 `chunk_id` 升序）
pub fn list_doc_chunks(conn: &Connection, doc_id: &str) -> Result<Vec<DocChunk>> {
    let mut stmt = conn.prepare(
        "SELECT chunk_id, doc_id, section_kind, section_tag, claim_no, parent_claim_no,
                para_start, para_end, embodiment_group, raw_content, fts_tokens,
                embedding_blob, char_count
         FROM doc_chunks
         WHERE doc_id = ?1
         ORDER BY chunk_id ASC",
    )?;
    let rows = stmt.query_map(params![doc_id], |row| {
        let kind_str: String = row.get(2)?;
        let char_count_i64: i64 = row.get(12)?;
        Ok(DocChunk {
            chunk_id: row.get(0)?,
            doc_id: row.get(1)?,
            section_kind: SectionKind::parse(&kind_str),
            section_tag: row.get(3)?,
            claim_no: row.get(4)?,
            parent_claim_no: row.get(5)?,
            para_start: row.get(6)?,
            para_end: row.get(7)?,
            embodiment_group: row.get(8)?,
            raw_content: row.get(9)?,
            fts_tokens: row.get(10)?,
            embedding_blob: row.get(11)?,
            char_count: usize::try_from(char_count_i64).unwrap_or(0),
        })
    })?;

    let mut out = Vec::new();
    for item in rows {
        out.push(item?);
    }
    Ok(out)
}

/// 自动识别文本是专利还是学术论文
fn infer_doc_kind(explicit: Option<&str>, text: &str, title: &str, external_id: &str) -> DocKind {
    if let Some(raw) = explicit {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return DocKind::parse(trimmed);
        }
    }
    let combined = format!("{title}\n{external_id}\n{}", safe_char_limit(text, 2000));
    let lower = combined.to_ascii_lowercase();
    if lower.contains("10.")
        && (lower.contains("doi") || lower.contains("abstract") || lower.contains("arxiv"))
    {
        return DocKind::JournalPaper;
    }
    if combined.contains("权利要求")
        || combined.contains("说明书")
        || combined.contains("附图说明")
        || lower.contains("claims")
        || lower.contains("embodiment")
        || external_id.to_ascii_uppercase().starts_with("CN")
        || external_id.to_ascii_uppercase().starts_with("US")
        || external_id.to_ascii_uppercase().starts_with("EP")
        || external_id.to_ascii_uppercase().starts_with("WO")
    {
        return DocKind::Patent;
    }
    DocKind::JournalPaper
}

/// 预处理上传文件附件（纯异步，不持有 SQLite 锁）
#[derive(Debug, Clone)]
pub struct PreparedIngestAttachment {
    pub extracted_text: String,
    pub ingest_tier_used: IngestTierUsed,
    pub warnings: Vec<String>,
}

pub async fn prepare_ingest_attachment(
    cfg: &AppConfig,
    input: &DissectIngestInput,
) -> Result<Option<PreparedIngestAttachment>> {
    let Some(ref b64) = input.file_base64 else {
        return Ok(None);
    };
    if b64.trim().is_empty() {
        return Ok(None);
    }
    let bytes = decode_base64_payload(b64)?;
    let filename = input
        .filename
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("document.txt");
    let vision_endpoint = if !input.privacy_offline_only {
        build_llm_router_from_config(cfg).and_then(|r| {
            r.select_vision_provider().map(|p| {
                (
                    p.base_url.clone(),
                    p.api_key.clone(),
                    p.select_vision_model().to_string(),
                )
            })
        })
    } else {
        None
    };
    let opts = IngestOptions {
        umi_ocr_url: None,
        vision_endpoint,
        privacy_offline_only: input.privacy_offline_only,
    };
    let mm = MultimodalService::new(opts);
    let ingested = mm
        .ingest_attachment(filename, input.mime_type.as_deref(), &bytes, None)
        .await?;
    Ok(Some(PreparedIngestAttachment {
        extracted_text: ingested.extracted_text,
        ingest_tier_used: ingested.tier_used,
        warnings: ingested.warnings,
    }))
}

/// 卡片 ③ 入口 1（同步入库阶段）：无截断入库或从金库加载单篇文献
pub fn commit_ingest_document(
    conn: &Connection,
    input: DissectIngestInput,
    prepared: Option<PreparedIngestAttachment>,
) -> Result<DissectIngestResult> {
    VaultService::ensure_tables(conn)?;

    let has_new_content = input
        .fulltext_raw
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
        || prepared.is_some();

    if !has_new_content {
        if let Some(ref doc_id) = input.doc_id {
            let trimmed_id = doc_id.trim();
            if !trimmed_id.is_empty() {
                if let Some(existing) = VaultService::get_document(conn, trimmed_id)? {
                    let chunks = list_doc_chunks(conn, trimmed_id)?;
                    let figure_numerals = VaultService::get_figure_numerals(conn, trimmed_id)?;
                    let existing_feature_atoms =
                        VaultService::list_feature_atoms(conn, trimmed_id)?;
                    return Ok(DissectIngestResult {
                        doc: existing,
                        chunks,
                        figure_numerals,
                        existing_feature_atoms,
                        ingest_tier_used: None,
                        warnings: Vec::new(),
                    });
                }
            }
        }
    }

    let mut extracted_text = input.fulltext_raw.clone().unwrap_or_default();
    let mut ingest_tier_used: Option<IngestTierUsed> = None;
    let mut warnings: Vec<String> = Vec::new();

    if let Some(att) = prepared {
        ingest_tier_used = Some(att.ingest_tier_used);
        warnings.extend(att.warnings);
        if !att.extracted_text.trim().is_empty() {
            if extracted_text.trim().is_empty() {
                extracted_text = att.extracted_text;
            } else {
                extracted_text = format!("{extracted_text}\n\n{}", att.extracted_text);
            }
        }
    }

    let abstract_text = input.abstract_text.clone().unwrap_or_default();
    let claims_json = input.claims_json.clone().unwrap_or_default();
    if extracted_text.trim().is_empty()
        && abstract_text.trim().is_empty()
        && claims_json.trim().is_empty()
    {
        return Err(anyhow!(
            "请提供文献正文、摘要、权利要求或上传有效文件 / Please provide document text or upload a file"
        ));
    }

    let title_raw = input
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            input
                .filename
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })
        .unwrap_or("未命名深析文献 / Untitled Document");

    let external_id = input.external_id.clone().unwrap_or_default();
    let doc_kind = infer_doc_kind(
        input.doc_kind.as_deref(),
        &extracted_text,
        title_raw,
        &external_id,
    );
    let doc_id = input
        .doc_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("dissect_{}", Uuid::new_v4().simple()));

    let asset_role = input
        .asset_role
        .as_deref()
        .map(AssetRole::parse)
        .unwrap_or(AssetRole::GeneralRef);
    let filing_status = input
        .filing_status
        .as_deref()
        .map(FilingStatus::parse)
        .unwrap_or(FilingStatus::PublicDisclosed);

    let doi_verified = external_id.trim().starts_with("10.");
    let doc = KnowledgeDoc {
        doc_id: doc_id.clone(),
        doc_kind,
        asset_role,
        external_id,
        title_zh: title_raw.to_string(),
        title_en: String::new(),
        authors_or_assignees: input.authors_or_assignees.clone().unwrap_or_default(),
        pub_or_filing_date: input.pub_or_filing_date.clone().unwrap_or_default(),
        venue_or_authority: input.venue_or_authority.clone().unwrap_or_default(),
        filing_status,
        doi_verified,
        bibtex_raw: String::new(),
        abstract_text,
        claims_json,
        fulltext_raw: extracted_text,
        fulltext_status: "fulltext".to_string(),
        chunk_ver: 2,
    };

    // 无截断全量切分入库 + 附图部件词典重建
    VaultService::upsert_document_with_chunks(conn, &doc)?;

    let chunks = list_doc_chunks(conn, &doc_id)?;
    let figure_numerals = VaultService::get_figure_numerals(conn, &doc_id)?;
    let existing_feature_atoms = VaultService::list_feature_atoms(conn, &doc_id)?;

    Ok(DissectIngestResult {
        doc,
        chunks,
        figure_numerals,
        existing_feature_atoms,
        ingest_tier_used,
        warnings,
    })
}

/// 组装长文献深析上下文（覆盖全部重要切片而非仅取前 5000 字）
fn build_analysis_context(
    doc: &KnowledgeDoc,
    chunks: &[DocChunk],
    numerals: &[FigureNumeral],
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "【文献元数据】\ndoc_id: {}\ndoc_kind: {}\nexternal_id: {}\ntitle: {}\nauthors_or_assignees: {}\ndate: {}\n\n",
        doc.doc_id,
        doc.doc_kind.as_str(),
        doc.external_id,
        if doc.title_zh.is_empty() { &doc.title_en } else { &doc.title_zh },
        doc.authors_or_assignees,
        doc.pub_or_filing_date,
    ));

    if !doc.abstract_text.trim().is_empty() {
        out.push_str(&format!("【摘要】\n{}\n\n", doc.abstract_text.trim()));
    }

    if !numerals.is_empty() {
        out.push_str("【附图部件标号词典】\n");
        for item in numerals.iter().take(60) {
            out.push_str(&format!(
                "- {} | 标号 `{}` -> {} [{}#{}]\n",
                item.figure_label, item.numeral_code, item.part_name, doc.doc_id, item.anchor_tag
            ));
        }
        out.push('\n');
    }

    out.push_str("【结构化原文切片（附标准锚点）】\n");
    let mut used_chars: usize = 0;
    const MAX_PROMPT_CHARS: usize = 48_000;
    for chunk in chunks {
        let entry = format!(
            "[{}#{}] ({}):\n{}\n\n",
            doc.doc_id,
            chunk.section_tag,
            chunk.section_kind.as_str(),
            chunk.raw_content.trim()
        );
        let entry_chars = entry.chars().count();
        if used_chars + entry_chars > MAX_PROMPT_CHARS {
            out.push_str(&format!(
                "...（后续尚有切片，已优先保留核心权项/实施例/方法/实验切片，总切片数={}）\n",
                chunks.len()
            ));
            break;
        }
        out.push_str(&entry);
        used_chars += entry_chars;
    }
    out
}

/// 离线/无 Key 时的确定性结构化深析回退（保证永不空跑报错）
fn deterministic_dissect_section(
    sub_type: &str,
    doc: &KnowledgeDoc,
    chunks: &[DocChunk],
    numerals: &[FigureNumeral],
) -> String {
    let doc_id = &doc.doc_id;
    match sub_type {
        "patent_scheme" => {
            let mut md = String::from("### 专利核心方案与附图连接关系（确定性结构化提取）\n\n");
            let key_chunks: Vec<&DocChunk> = chunks
                .iter()
                .filter(|c| {
                    matches!(
                        c.section_kind,
                        SectionKind::ClaimIndep
                            | SectionKind::Summary
                            | SectionKind::Embodiment
                            | SectionKind::Abstract
                    )
                })
                .take(6)
                .collect();
            md.push_str("#### 1. 核心技术特征与锚点摘录\n");
            if key_chunks.is_empty() {
                md.push_str("- 暂无结构化核心切片，请检查原文内容。\n");
            } else {
                for ch in key_chunks {
                    md.push_str(&format!(
                        "- [{doc_id}#{}] {}\n",
                        ch.section_tag,
                        safe_char_limit(ch.raw_content.trim(), 220)
                    ));
                }
            }
            md.push_str("\n#### 2. 附图部件标号映射字典\n");
            if numerals.is_empty() {
                md.push_str("- 原文未检测到显式附图部件标号对（如 `10-壳体`）。\n");
            } else {
                for n in numerals.iter().take(30) {
                    md.push_str(&format!(
                        "- **{}** `{}`：{}（锚点 [{doc_id}#{}]）\n",
                        n.figure_label, n.numeral_code, n.part_name, n.anchor_tag
                    ));
                }
            }
            md
        }
        "patent_fto" => {
            let mut md = String::from("### 逐权项 FTO 侵权风险排雷（确定性结构化提取）\n\n");
            let claims: Vec<&DocChunk> = chunks
                .iter()
                .filter(|c| {
                    matches!(
                        c.section_kind,
                        SectionKind::ClaimIndep | SectionKind::ClaimDep
                    )
                })
                .collect();
            if claims.is_empty() {
                md.push_str("- 当前文献未切分出独立/从属权利要求，无法执行逐权项比对。\n");
            } else {
                md.push_str(
                    "| 权项锚点 | 类型 | 初步风险定级 | 关键限定特征摘要 | 规避切入提示 |\n",
                );
                md.push_str("| --- | --- | --- | --- | --- |\n");
                for c in claims.iter().take(15) {
                    let is_indep = c.section_kind == SectionKind::ClaimIndep;
                    let kind_label = if is_indep {
                        "独立权项"
                    } else {
                        "从属权项"
                    };
                    let risk = if is_indep { "high" } else { "medium" };
                    let summary = safe_char_limit(&c.raw_content.replace('\n', " "), 120);
                    let advice = if is_indep {
                        "核对是否可省略或替换其中任一必要技术特征（All-Elements Rule）"
                    } else {
                        "仅当落入其引用之独立权项时才叠加本附加特征风险"
                    };
                    md.push_str(&format!(
                        "| [{doc_id}#{}] | {kind_label} | `{risk}` | {summary} | {advice} |\n",
                        c.section_tag
                    ));
                }
            }
            md
        }
        "paper_mechanism" => {
            let mut md = String::from("### 论文核心机理与复现条件核查（确定性结构化提取）\n\n");
            if doc.doi_verified && !doc.external_id.trim().is_empty() {
                md.push_str(&format!("- 核验文献标识：DOI:{}\n", doc.external_id.trim()));
            }
            let method_chunks: Vec<&DocChunk> = chunks
                .iter()
                .filter(|c| {
                    matches!(
                        c.section_kind,
                        SectionKind::Abstract
                            | SectionKind::PaperMethod
                            | SectionKind::PaperExperiment
                            | SectionKind::PaperTableFig
                    )
                })
                .take(6)
                .collect();
            for ch in method_chunks {
                md.push_str(&format!(
                    "- [{doc_id}#{}] {}\n",
                    ch.section_tag,
                    safe_char_limit(ch.raw_content.trim(), 240)
                ));
            }
            md
        }
        _ => {
            let mut md = String::from("### 论文局限性与改进切入点（确定性结构化提取）\n\n");
            let lim_chunks: Vec<&DocChunk> = chunks
                .iter()
                .filter(|c| {
                    matches!(
                        c.section_kind,
                        SectionKind::PaperLimitation
                            | SectionKind::PaperExperiment
                            | SectionKind::Fallback
                    )
                })
                .take(5)
                .collect();
            if lim_chunks.is_empty() {
                md.push_str(
                    "- 论文未在独立 Limitation 章节披露局限，建议核查实验边界条件与对照组设置。\n",
                );
            } else {
                for ch in lim_chunks {
                    md.push_str(&format!(
                        "- [{doc_id}#{}] {}\n",
                        ch.section_tag,
                        safe_char_limit(ch.raw_content.trim(), 220)
                    ));
                }
            }
            md
        }
    }
}

/// 从结构化切片与报告中提取并去重技术特征原子（P-M-E 三元组），自动沉淀入 `feature_atoms`
fn build_feature_atoms(
    doc: &KnowledgeDoc,
    chunks: &[DocChunk],
    report_markdown: &str,
) -> Vec<FeatureAtom> {
    let now = Utc::now().to_rfc3339();
    let mut atoms: Vec<FeatureAtom> = Vec::new();
    let mut seen_anchors: HashSet<String> = HashSet::new();

    // 1. 优先从报告中匹配带标准锚点的技术描述行
    if let Ok(re) = Regex::new(r"\[([A-Za-z0-9_.\-]+)#([^\]]+)\]") {
        for line in report_markdown.lines() {
            let trimmed = line.trim();
            if trimmed.chars().count() < 16 {
                continue;
            }
            if let Some(cap) = re.captures(trimmed) {
                if let (Some(d), Some(tag)) = (cap.get(1), cap.get(2)) {
                    if d.as_str() == doc.doc_id {
                        let anchor = tag.as_str().to_string();
                        if seen_anchors.insert(anchor.clone()) {
                            atoms.push(FeatureAtom {
                                atom_id: format!("atom_{}_{}", doc.doc_id, atoms.len() + 1),
                                doc_id: doc.doc_id.clone(),
                                anchor_tag: anchor,
                                tech_problem: format!(
                                    "针对 {} 的关键技术瓶颈",
                                    safe_char_limit(&doc.title_zh, 40)
                                ),
                                tech_means: safe_char_limit(trimmed, 220),
                                tech_effect: "经深析抽取并绑定原文锚点的可复用技术特征".to_string(),
                                created_at: now.clone(),
                            });
                            if atoms.len() >= 8 {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. 若报告未命中足够锚点，从核心切片补充确定性特征原子
    if atoms.len() < 3 {
        for ch in chunks.iter().filter(|c| {
            matches!(
                c.section_kind,
                SectionKind::ClaimIndep
                    | SectionKind::Summary
                    | SectionKind::Embodiment
                    | SectionKind::PaperMethod
                    | SectionKind::PaperExperiment
            )
        }) {
            if seen_anchors.insert(ch.section_tag.clone()) {
                atoms.push(FeatureAtom {
                    atom_id: format!("atom_{}_{}", doc.doc_id, atoms.len() + 1),
                    doc_id: doc.doc_id.clone(),
                    anchor_tag: ch.section_tag.clone(),
                    tech_problem: format!("围绕 {} 的技术问题", safe_char_limit(&doc.title_zh, 40)),
                    tech_means: safe_char_limit(ch.raw_content.trim(), 220),
                    tech_effect: format!("来自 {} 切片的核实特征", ch.section_kind.as_str()),
                    created_at: now.clone(),
                });
                if atoms.len() >= 5 {
                    break;
                }
            }
        }
    }

    atoms
}

fn sub_type_meta(sub_type: &str) -> (&'static str, &'static str, &'static str) {
    match sub_type {
        "patent_scheme" => (
            "核心方案与附图字典",
            "Core Scheme & Figure Glossary",
            PATENT_SCHEME_FIG_PROMPT,
        ),
        "patent_fto" => (
            "逐权项 FTO 排雷",
            "Claim-by-Claim FTO Risk",
            PATENT_FTO_PROMPT,
        ),
        "paper_mechanism" => (
            "机理白话拆解与复现表",
            "Mechanism & Reproduction Table",
            PAPER_MECHANISM_REPRO_PROMPT,
        ),
        _ => (
            "局限性与改进切入点",
            "Limitations & Improvements",
            PAPER_LIMITATION_PROMPT,
        ),
    }
}

/// 深析上下文（同步加载，跨 `.await` 不持有 SQLite 锁）
#[derive(Debug, Clone)]
pub struct AnalyzeContext {
    pub doc: KnowledgeDoc,
    pub chunks: Vec<DocChunk>,
    pub figure_numerals: Vec<FigureNumeral>,
    pub detected_mode: String,
    pub sub_types: Vec<String>,
    pub context_prompt: String,
    pub privacy_offline_only: bool,
}

pub fn load_analyze_context(
    conn: &Connection,
    input: &DissectAnalyzeInput,
) -> Result<AnalyzeContext> {
    VaultService::ensure_tables(conn)?;
    let doc_id = input.doc_id.trim();
    let doc = VaultService::get_document(conn, doc_id)?
        .ok_or_else(|| anyhow!("金库中未找到文献 doc_id={doc_id} / Document not found"))?;
    let chunks = list_doc_chunks(conn, doc_id)?;
    let figure_numerals = VaultService::get_figure_numerals(conn, doc_id)?;

    let detected_mode = match input.mode.as_deref().map(str::trim) {
        Some("patent") => "patent",
        Some("paper") => "paper",
        _ => {
            if doc.doc_kind == DocKind::Patent {
                "patent"
            } else {
                "paper"
            }
        }
    };

    let sub_types: Vec<String> = if input.sub_types.is_empty() {
        if detected_mode == "patent" {
            vec!["patent_scheme".to_string(), "patent_fto".to_string()]
        } else {
            vec![
                "paper_mechanism".to_string(),
                "paper_limitations".to_string(),
            ]
        }
    } else {
        input.sub_types.clone()
    };

    let context_prompt = build_analysis_context(&doc, &chunks, &figure_numerals);
    Ok(AnalyzeContext {
        doc,
        chunks,
        figure_numerals,
        detected_mode: detected_mode.to_string(),
        sub_types,
        context_prompt,
        privacy_offline_only: input.privacy_offline_only,
    })
}

/// 纯异步执行 AI 路由或确定性结构化提取（不持有 SQLite 锁）
pub async fn run_analyze_sections(
    cfg: &AppConfig,
    ctx: &AnalyzeContext,
) -> Vec<DissectSectionResult> {
    let router = if ctx.privacy_offline_only {
        None
    } else {
        build_llm_router_from_config(cfg)
    };

    let mut sections: Vec<DissectSectionResult> = Vec::with_capacity(ctx.sub_types.len());
    for st in &ctx.sub_types {
        let (title_zh, title_en, sys_prompt) = sub_type_meta(st);
        let (md, source, provider_id) = if let Some(ref llm) = router {
            match llm
                .chat_with_failover(sys_prompt, &ctx.context_prompt, true)
                .await
            {
                Ok((reply, pid)) => (reply, "llm_router".to_string(), Some(pid)),
                Err(err) => {
                    tracing::warn!(doc_id = %ctx.doc.doc_id, sub_type = %st, error = %err, "AI 深析调用失败，降级为确定性结构化提取");
                    (
                        deterministic_dissect_section(
                            st,
                            &ctx.doc,
                            &ctx.chunks,
                            &ctx.figure_numerals,
                        ),
                        "deterministic_fallback".to_string(),
                        None,
                    )
                }
            }
        } else {
            (
                deterministic_dissect_section(st, &ctx.doc, &ctx.chunks, &ctx.figure_numerals),
                "deterministic_fallback".to_string(),
                None,
            )
        };

        sections.push(DissectSectionResult {
            sub_type: st.clone(),
            title_zh: title_zh.to_string(),
            title_en: title_en.to_string(),
            markdown: md,
            source,
            provider_id,
        });
    }
    sections
}

/// 卡片 ③ 入口 2（同步收尾阶段）：O(1) 锚点核验 + `feature_atoms` 自动沉淀
pub fn finalize_analyze_document(
    conn: &Connection,
    ctx: AnalyzeContext,
    sections: Vec<DissectSectionResult>,
) -> Result<DissectAnalyzeResult> {
    let mut combined_md = String::new();
    for sec in &sections {
        combined_md.push_str(&format!("## {}\n\n{}\n\n", sec.title_zh, sec.markdown));
    }

    let verified_citations =
        EvidenceRebuttalService::verify_all_citations(conn, &combined_md, Some(&ctx.doc.doc_id))?;

    let atoms = build_feature_atoms(&ctx.doc, &ctx.chunks, &combined_md);
    if !atoms.is_empty() {
        VaultService::save_feature_atoms(conn, &atoms)?;
    }

    Ok(DissectAnalyzeResult {
        doc_id: ctx.doc.doc_id,
        detected_mode: ctx.detected_mode,
        sections,
        report_markdown: combined_md,
        verified_citations,
        saved_feature_atoms: atoms,
        figure_numerals: ctx.figure_numerals,
    })
}

/// 追问上下文（同步加载，跨 `.await` 不持有 SQLite 锁）
#[derive(Debug, Clone)]
pub struct AskContext {
    pub doc_id: String,
    pub prompt_ctx: String,
    pub matched_chunks: Vec<DocChunk>,
    pub matched_numerals: Vec<FigureNumeral>,
    pub image_data_urls: Vec<String>,
    pub privacy_offline_only: bool,
}

pub fn load_ask_context(conn: &Connection, input: &DissectAskInput) -> Result<AskContext> {
    VaultService::ensure_tables(conn)?;
    let doc_id = input.doc_id.trim();
    let doc = VaultService::get_document(conn, doc_id)?
        .ok_or_else(|| anyhow!("金库中未找到文献 doc_id={doc_id} / Document not found"))?;
    let question = input.question.trim();
    if question.is_empty() {
        return Err(anyhow!("请输入追问内容 / Question cannot be empty"));
    }

    let scene_mode = if doc.doc_kind == DocKind::Patent {
        SearchSceneMode::PatentCompare
    } else {
        SearchSceneMode::PaperSurvey
    };
    let search_query = match input.selected_excerpt.as_deref() {
        Some(sel) if !sel.trim().is_empty() => format!("{question} {}", safe_char_limit(sel, 200)),
        _ => question.to_string(),
    };

    let doc_filter = vec![doc_id.to_string()];
    let bundle = VaultService::hybrid_search_chunks(
        conn,
        &search_query,
        scene_mode,
        Some(&doc_filter),
        None,
        None,
        6,
        6000,
    )?;

    let all_numerals = VaultService::get_figure_numerals(conn, doc_id)?;
    let needle_text = format!(
        "{question} {} {}",
        input.selected_excerpt.as_deref().unwrap_or(""),
        input.figure_numeral.as_deref().unwrap_or("")
    );
    let matched_numerals: Vec<FigureNumeral> = all_numerals
        .iter()
        .filter(|n| {
            needle_text.contains(&n.numeral_code)
                || needle_text.contains(&n.part_name)
                || input.figure_numeral.as_deref().is_some_and(|f| {
                    !f.trim().is_empty() && n.numeral_code.eq_ignore_ascii_case(f.trim())
                })
        })
        .cloned()
        .collect();

    let matched_chunks: Vec<DocChunk> = bundle.hits.iter().map(|h| h.chunk.clone()).collect();

    let isolated_question = wrap_user_input(&format!(
        "追问问题：{question}\n框选原文：{}\n指定锚点：{}\n指定标号：{}",
        input.selected_excerpt.as_deref().unwrap_or("无"),
        input.selected_anchor.as_deref().unwrap_or("无"),
        input.figure_numeral.as_deref().unwrap_or("无"),
    ));

    let mut prompt_ctx = format!("【当前文献】doc_id={doc_id}, title={}\n\n", doc.title_zh);
    if !all_numerals.is_empty() {
        prompt_ctx.push_str("【附图部件标号词典】\n");
        let numerals_iter: Box<dyn Iterator<Item = &FigureNumeral>> = if matched_numerals.is_empty()
        {
            Box::new(all_numerals.iter().take(30))
        } else {
            Box::new(matched_numerals.iter())
        };
        for n in numerals_iter {
            prompt_ctx.push_str(&format!(
                "- {} `{}`: {} [{}#{}]\n",
                n.figure_label, n.numeral_code, n.part_name, doc_id, n.anchor_tag
            ));
        }
        prompt_ctx.push('\n');
    }
    prompt_ctx.push_str("【检索命中的切片上下文】\n");
    for hit in &bundle.hits {
        prompt_ctx.push_str(&format!(
            "[{}#{}]\n{}\n\n",
            doc_id, hit.chunk.section_tag, hit.assembled_context
        ));
    }
    prompt_ctx.push_str(&format!("【用户隔离输入】\n{isolated_question}"));

    Ok(AskContext {
        doc_id: doc_id.to_string(),
        prompt_ctx,
        matched_chunks,
        matched_numerals,
        image_data_urls: input.image_data_urls.clone(),
        privacy_offline_only: input.privacy_offline_only,
    })
}

/// 纯异步执行带图局部追问（不持有 SQLite 锁）
pub async fn run_ask_llm(cfg: &AppConfig, ctx: &AskContext) -> (String, String, Option<String>) {
    let router = if ctx.privacy_offline_only {
        None
    } else {
        build_llm_router_from_config(cfg)
    };

    if let Some(ref llm) = router {
        let call_res = if !ctx.image_data_urls.is_empty() && llm.select_vision_provider().is_some()
        {
            llm.vision_chat_with_failover(
                DISSECT_ASK_SYSTEM_PROMPT,
                &ctx.prompt_ctx,
                &ctx.image_data_urls,
            )
            .await
        } else {
            llm.chat_with_failover(DISSECT_ASK_SYSTEM_PROMPT, &ctx.prompt_ctx, false)
                .await
        };

        match call_res {
            Ok((reply, pid)) => (reply, "llm_router".to_string(), Some(pid)),
            Err(err) => {
                tracing::warn!(doc_id = %ctx.doc_id, error = %err, "带图追问 AI 调用失败，降级为切片检索直答");
                (
                    build_deterministic_ask_reply(
                        &ctx.doc_id,
                        &ctx.matched_chunks,
                        &ctx.matched_numerals,
                    ),
                    "deterministic_fallback".to_string(),
                    None,
                )
            }
        }
    } else {
        (
            build_deterministic_ask_reply(&ctx.doc_id, &ctx.matched_chunks, &ctx.matched_numerals),
            "deterministic_fallback".to_string(),
            None,
        )
    }
}

/// 卡片 ③ 入口 3（同步收尾阶段）：对追问回答执行 O(1) 锚点核验
pub fn finalize_ask_document(
    conn: &Connection,
    ctx: AskContext,
    answer_markdown: String,
    answer_source: String,
    provider_id: Option<String>,
) -> Result<DissectAskResult> {
    let verified_citations =
        EvidenceRebuttalService::verify_all_citations(conn, &answer_markdown, Some(&ctx.doc_id))?;

    Ok(DissectAskResult {
        doc_id: ctx.doc_id,
        answer_markdown,
        verified_citations,
        matched_chunks: ctx.matched_chunks,
        matched_numerals: ctx.matched_numerals,
        answer_source,
        provider_id,
    })
}

fn build_deterministic_ask_reply(
    doc_id: &str,
    chunks: &[DocChunk],
    numerals: &[FigureNumeral],
) -> String {
    let mut out = String::from("已为您从本机金库切片与附图词典中检索到以下直接证据：\n\n");
    if !numerals.is_empty() {
        out.push_str("**命中附图部件标号：**\n");
        for n in numerals {
            out.push_str(&format!(
                "- `{}` {}（{}，[{doc_id}#{}]）\n",
                n.numeral_code, n.part_name, n.figure_label, n.anchor_tag
            ));
        }
        out.push('\n');
    }
    if chunks.is_empty() {
        out.push_str("当前文献切片中未检索到直接匹配的段落。\n");
    } else {
        out.push_str("**命中原文切片：**\n");
        for ch in chunks {
            out.push_str(&format!(
                "- [{doc_id}#{}] {}\n",
                ch.section_tag,
                safe_char_limit(ch.raw_content.trim(), 240)
            ));
        }
    }
    out
}
