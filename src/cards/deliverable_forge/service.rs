//! 卡片 ⑥ 核心业务逻辑：公开时序守卫、一源四出分节编译与多格式导出（`src/cards/deliverable_forge/service.rs`）

use std::io::{Cursor, Write};

use anyhow::{anyhow, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::cards::deliverable_forge::prompts::{build_compile_system_prompt, section_blueprint};
use crate::core::evidence_rebuttal::{
    priority_guard, EvidenceRebuttalService, GateEvaluationInput,
};
use crate::core::runtime::llm_router::{builtin_nine_providers, LlmProviderSpec, LlmRouter};
use crate::core::runtime::{CompileDeliverableRequest, CompiledDeliverable, ForgeExporter};
use crate::core::vault::models::{
    DeliverableFormat, DeliverableGenre, ExperimentRecord, FiveDimGateReport, KnowledgeDoc,
    PriorityCheckResult, VerifiedCitation,
};
use crate::core::vault::VaultService;
use crate::routes::AppConfig;

const REDACTED_PLACEHOLDER: &str = "[商业秘密脱敏保留 / Know-how Redacted]";

/// 预览编译输入参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilePreviewInput {
    pub title: String,
    pub genre: DeliverableGenre,
    pub related_doc_ids: Vec<String>,
    pub reference_doc_ids: Vec<String>,
    pub experiment_ids: Vec<String>,
    pub knowhow_markers: Vec<String>,
    pub custom_notes: String,
    pub use_ai: bool,
}

/// 单个预览章节
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledSection {
    pub key: String,
    pub heading: String,
    pub body: String,
    pub citations: Vec<VerifiedCitation>,
}

/// 被剔除文献的审计记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectedReferenceInfo {
    pub doc_id: String,
    pub title: String,
    pub reason: String,
}

/// 预览编译完整返回体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilePreviewOutput {
    pub title: String,
    pub genre: DeliverableGenre,
    pub sections: Vec<CompiledSection>,
    pub priority_check: PriorityCheckResult,
    pub gate_report: FiveDimGateReport,
    pub accepted_references: Vec<KnowledgeDoc>,
    pub rejected_references: Vec<RejectedReferenceInfo>,
    pub bound_experiments: Vec<ExperimentRecord>,
    pub companion_bibtex: Option<String>,
    pub ai_provider_used: Option<String>,
}

/// 多格式导出输入参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportDeliverableInput {
    pub title: String,
    pub genre: DeliverableGenre,
    pub format: DeliverableFormat,
    pub sections: Vec<(String, String)>,
    pub original_text_for_diff: Option<String>,
    pub amended_text_for_diff: Option<String>,
    pub related_doc_ids: Vec<String>,
    pub reference_doc_ids: Vec<String>,
    pub knowhow_markers: Vec<String>,
    pub acknowledge_priority_warning: bool,
    pub bundle_zip: bool,
}

/// 卡片 ⑥ 全谱系成果锻造服务
#[derive(Debug, Default, Clone, Copy)]
pub struct DeliverableForgeService;

impl DeliverableForgeService {
    /// 执行「先锁专利申请日、后公开论文/开源」时序冲突检测与商业秘密参数脱敏预检
    pub fn check_priority(
        conn: &Connection,
        related_doc_ids: &[String],
        target_genre: DeliverableGenre,
        knowhow_markers: &[String],
        draft_text: &str,
    ) -> Result<(PriorityCheckResult, String)> {
        priority_guard::check_disclosure_priority(
            conn,
            related_doc_ids,
            target_genre,
            knowhow_markers,
            draft_text,
        )
    }

    /// 编译分节预览：加载基线资产、核验实验锚点、过滤真 DOI 文献、裁剪 Know-how 并跑五维门禁
    pub async fn compile_preview(
        db: &crate::db::Database,
        config: &AppConfig,
        input: &CompilePreviewInput,
    ) -> Result<CompilePreviewOutput> {
        let (related_docs, all_ref_docs, experiments, priority_check) = {
            let conn = db.conn();
            let mut related_docs = Vec::new();
            for id in &input.related_doc_ids {
                if let Some(doc) = VaultService::get_document(&conn, id)? {
                    related_docs.push(doc);
                }
            }

            let mut all_ref_docs = Vec::new();
            for id in &input.reference_doc_ids {
                if let Some(doc) = VaultService::get_document(&conn, id)? {
                    all_ref_docs.push(doc);
                }
            }

            let mut experiments = Vec::new();
            for id in &input.experiment_ids {
                if let Some(exp) = VaultService::get_experiment(&conn, id)? {
                    experiments.push(exp);
                }
            }

            let (priority_check, _) = priority_guard::check_disclosure_priority(
                &conn,
                &input.related_doc_ids,
                input.genre,
                &input.knowhow_markers,
                &input.custom_notes,
            )?;

            (related_docs, all_ref_docs, experiments, priority_check)
        };

        // 学术论文体裁下强制仅绑定 doi_verified == true 且 bibtex_raw 非空的真实题录
        let (accepted_references, rejected_references) =
            filter_references_strict(input.genre, &all_ref_docs);

        let blueprint = section_blueprint(input.genre);
        let mut raw_sections: Vec<(String, String, String)> = Vec::new();
        let mut ai_provider_used: Option<String> = None;

        if input.use_ai {
            if let Ok(router) = build_llm_router(config) {
                let sys_prompt = build_compile_system_prompt(input.genre, &input.knowhow_markers);
                let context_prompt = build_ai_context_prompt(
                    input,
                    &related_docs,
                    &accepted_references,
                    &experiments,
                );
                if let Ok((ai_text, provider_id)) = router
                    .chat_with_failover(&sys_prompt, &context_prompt, false)
                    .await
                {
                    ai_provider_used = Some(provider_id);
                    raw_sections = parse_ai_sections(&ai_text, blueprint);
                }
            }
        }

        if raw_sections.is_empty() {
            raw_sections = build_deterministic_sections(
                input,
                &related_docs,
                &accepted_references,
                &experiments,
            );
        }

        // 统一商业秘密脱敏裁剪
        let redacted_sections: Vec<(String, String, String)> = raw_sections
            .into_iter()
            .map(|(key, heading, body)| {
                (
                    key,
                    redact_knowhow(&heading, &input.knowhow_markers),
                    redact_knowhow(&body, &input.knowhow_markers),
                )
            })
            .collect();

        let combined_draft = redacted_sections
            .iter()
            .map(|(_, h, b)| format!("{}\n{}", h, b))
            .collect::<Vec<_>>()
            .join("\n\n");

        // 再次用数据库执行引证回查与五维门禁评分（不跨 .await 持有 MutexGuard）
        let (sections, mut gate_report) = {
            let conn = db.conn();
            let default_doc_id = input.related_doc_ids.first().cloned();
            let mut compiled_sections = Vec::with_capacity(redacted_sections.len());
            for (key, heading, body) in redacted_sections {
                let citations = EvidenceRebuttalService::verify_all_citations(
                    &conn,
                    &body,
                    default_doc_id.as_deref(),
                )?;
                compiled_sections.push(CompiledSection {
                    key,
                    heading,
                    body,
                    citations,
                });
            }

            let gate_input = GateEvaluationInput {
                draft_text: combined_draft,
                default_doc_id,
                require_citations: false,
                stitching_alerts: Vec::new(),
                scope_report: None,
                priority_check: Some(priority_check.clone()),
            };
            let gate_report =
                EvidenceRebuttalService::evaluate_submission_gate(&conn, &gate_input)?;
            (compiled_sections, gate_report)
        };

        for rej in &rejected_references {
            gate_report
                .downgrade_warnings
                .push(format!("已剔除未核验文献 {}: {}", rej.doc_id, rej.reason));
        }

        let bibtex_entries: Vec<String> = accepted_references
            .iter()
            .filter_map(|doc| {
                let raw = doc.bibtex_raw.trim();
                if raw.is_empty() {
                    None
                } else {
                    Some(raw.to_string())
                }
            })
            .collect();
        let companion_bibtex = if bibtex_entries.is_empty() {
            None
        } else {
            Some(bibtex_entries.join("\n\n"))
        };

        Ok(CompilePreviewOutput {
            title: redact_knowhow(&input.title, &input.knowhow_markers),
            genre: input.genre,
            sections,
            priority_check,
            gate_report,
            accepted_references,
            rejected_references,
            bound_experiments: experiments,
            companion_bibtex,
            ai_provider_used,
        })
    }

    /// 调用内核 `ForgeExporter::compile_deliverable` 导出目标格式（支持 `.tex` 内嵌或 `.zip` 打包 `.bib`）
    pub fn export_deliverable(
        conn: &Connection,
        input: &ExportDeliverableInput,
    ) -> Result<CompiledDeliverable> {
        let mut reference_docs = Vec::new();
        for doc_id in &input.reference_doc_ids {
            if let Some(doc) = VaultService::get_document(conn, doc_id)? {
                reference_docs.push(doc);
            }
        }

        let compile_req = CompileDeliverableRequest {
            title: input.title.clone(),
            genre: input.genre,
            format: input.format,
            sections: input.sections.clone(),
            original_text_for_diff: input.original_text_for_diff.clone(),
            amended_text_for_diff: input.amended_text_for_diff.clone(),
            related_doc_ids: input.related_doc_ids.clone(),
            reference_docs,
            knowhow_markers: input.knowhow_markers.clone(),
            acknowledge_priority_warning: input.acknowledge_priority_warning,
        };

        let mut compiled = ForgeExporter::compile_deliverable(conn, &compile_req)?;

        if input.format == DeliverableFormat::Tex {
            if let Some(bib) = compiled.companion_bibtex.as_deref() {
                if input.bundle_zip {
                    let zip_bytes = bundle_tex_and_bib_zip(&compiled.bytes, bib)?;
                    let stem = compiled
                        .filename
                        .strip_suffix(".tex")
                        .unwrap_or(&compiled.filename)
                        .to_string();
                    compiled.filename = format!("{}_tex_bundle.zip", stem);
                    compiled.mime_type = "application/zip".to_string();
                    compiled.bytes = zip_bytes;
                } else {
                    let mut tex_text = String::from_utf8_lossy(&compiled.bytes).into_owned();
                    tex_text.push_str("\n\n% === references.bib ===\n");
                    tex_text.push_str(bib);
                    tex_text.push('\n');
                    compiled.bytes = tex_text.into_bytes();
                }
            }
        }

        Ok(compiled)
    }
}

fn bundle_tex_and_bib_zip(tex_bytes: &[u8], bib_text: &str) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default();
        zip.start_file("manuscript.tex", opts)?;
        zip.write_all(tex_bytes)?;
        zip.start_file("references.bib", opts)?;
        zip.write_all(bib_text.as_bytes())?;
        zip.finish()?;
    }
    Ok(buf.into_inner())
}

fn filter_references_strict(
    genre: DeliverableGenre,
    docs: &[KnowledgeDoc],
) -> (Vec<KnowledgeDoc>, Vec<RejectedReferenceInfo>) {
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();

    for doc in docs {
        if genre == DeliverableGenre::AcademicPaper
            && (!doc.doi_verified || doc.bibtex_raw.trim().is_empty())
        {
            let reason = if !doc.doi_verified {
                "未通过真实 DOI 核验（禁止写入学术论文 BibTeX，防幻觉伪造）".to_string()
            } else {
                "缺少有效 BibTeX 题录原文".to_string()
            };
            rejected.push(RejectedReferenceInfo {
                doc_id: doc.doc_id.clone(),
                title: display_doc_title(doc),
                reason,
            });
            continue;
        }
        accepted.push(doc.clone());
    }

    (accepted, rejected)
}

fn display_doc_title(doc: &KnowledgeDoc) -> String {
    if !doc.title_zh.trim().is_empty() {
        doc.title_zh.trim().to_string()
    } else if !doc.title_en.trim().is_empty() {
        doc.title_en.trim().to_string()
    } else {
        doc.doc_id.clone()
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut out: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        out.push('…');
    }
    out
}

fn redact_knowhow(text: &str, markers: &[String]) -> String {
    let mut out = text.to_string();
    for marker in markers {
        let trimmed = marker.trim();
        if !trimmed.is_empty() && out.contains(trimmed) {
            out = out.replace(trimmed, REDACTED_PLACEHOLDER);
        }
    }
    out
}

fn build_deterministic_sections(
    input: &CompilePreviewInput,
    related_docs: &[KnowledgeDoc],
    accepted_refs: &[KnowledgeDoc],
    experiments: &[ExperimentRecord],
) -> Vec<(String, String, String)> {
    let blueprint = section_blueprint(input.genre);
    let doc_summary = if related_docs.is_empty() {
        "（未挂载自有/关联方案基线文档）".to_string()
    } else {
        related_docs
            .iter()
            .map(|d| {
                format!(
                    "- [{}] {}（状态: {}）: {}",
                    d.doc_id,
                    display_doc_title(d),
                    d.filing_status.as_str(),
                    truncate_chars(&d.abstract_text, 220)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let exp_summary = if experiments.is_empty() {
        "（暂未绑定对照实验记录）".to_string()
    } else {
        experiments
            .iter()
            .map(|e| {
                let anchor = e
                    .evidence_anchor()
                    .unwrap_or_else(|| format!("Exp:{} [待实测]", e.exp_id));
                let measured = e
                    .measured_data_json
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| truncate_chars(s, 180))
                    .unwrap_or_else(|| "尚未回填实测 JSON".to_string());
                format!(
                    "- {} · {} | 假设: {} | 对照组: {} | 实验组: {} | 实测: {}",
                    anchor,
                    e.title,
                    e.hypothesis,
                    e.control_group_spec,
                    e.test_group_spec,
                    measured
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let ref_summary = if accepted_refs.is_empty() {
        "（无已核验参考文献）".to_string()
    } else {
        accepted_refs
            .iter()
            .map(|r| {
                format!(
                    "- [{}] {} ({}) | DOI核验: {}",
                    r.doc_id,
                    display_doc_title(r),
                    r.external_id,
                    if r.doi_verified {
                        "已核验"
                    } else {
                        "未核验"
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let notes = if input.custom_notes.trim().is_empty() {
        "（无额外补充说明）".to_string()
    } else {
        input.custom_notes.trim().to_string()
    };

    blueprint
        .iter()
        .enumerate()
        .map(|(idx, spec)| {
            let body = match idx {
                0 => format!(
                    "编制要点：{}\n\n核心方案基线：\n{}\n\n补充构想：\n{}",
                    spec.guidance, doc_summary, notes
                ),
                1 => format!(
                    "编制要点：{}\n\n关联技术与文献支撑：\n{}\n\n参考文献清单：\n{}",
                    spec.guidance, doc_summary, ref_summary
                ),
                2 => format!(
                    "编制要点：{}\n\n对照实验与实测铁证：\n{}",
                    spec.guidance, exp_summary
                ),
                _ => format!(
                    "编制要点：{}\n\n参考文献核验集：\n{}\n\n实验锚点汇总：\n{}",
                    spec.guidance, ref_summary, exp_summary
                ),
            };
            (spec.key.to_string(), spec.heading_zh.to_string(), body)
        })
        .collect()
}

fn build_ai_context_prompt(
    input: &CompilePreviewInput,
    related_docs: &[KnowledgeDoc],
    accepted_refs: &[KnowledgeDoc],
    experiments: &[ExperimentRecord],
) -> String {
    let blueprint_lines = section_blueprint(input.genre)
        .iter()
        .map(|s| format!("## {} [key={}]\n指导：{}", s.heading_zh, s.key, s.guidance))
        .collect::<Vec<_>>()
        .join("\n");

    let docs_block = related_docs
        .iter()
        .map(|d| {
            format!(
                "- {} | {} | 摘要: {}",
                d.doc_id,
                display_doc_title(d),
                truncate_chars(&d.abstract_text, 400)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let refs_block = accepted_refs
        .iter()
        .map(|r| {
            format!(
                "- {} | {} | ext_id={} | doi_verified={}",
                r.doc_id,
                display_doc_title(r),
                r.external_id,
                r.doi_verified
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let exps_block = experiments
        .iter()
        .map(|e| {
            format!(
                "- Exp:{} | {} | 对照: {} | 实验: {} | 实测: {}",
                e.exp_id,
                e.title,
                e.control_group_spec,
                e.test_group_spec,
                e.measured_data_json.as_deref().unwrap_or("无")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "文稿标题：{}\n\n请严格按以下章节标题（使用 `## 标题` 分隔）撰写正文：\n{}\n\n\
         方案基线：\n{}\n\n已核验参考文献：\n{}\n\n对照实验记录：\n{}\n\n补充说明：\n{}",
        input.title, blueprint_lines, docs_block, refs_block, exps_block, input.custom_notes
    )
}

fn parse_ai_sections(
    ai_text: &str,
    blueprint: &[crate::cards::deliverable_forge::prompts::GenreSectionSpec],
) -> Vec<(String, String, String)> {
    let mut parsed: Vec<(String, String, String)> = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut current_body = Vec::new();

    for line in ai_text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("## ") {
            if let Some(prev_h) = current_heading.take() {
                let idx = parsed.len();
                let key = blueprint
                    .get(idx)
                    .map(|s| s.key.to_string())
                    .unwrap_or_else(|| format!("section_{}", idx + 1));
                parsed.push((key, prev_h, current_body.join("\n").trim().to_string()));
                current_body.clear();
            }
            current_heading = Some(rest.trim().to_string());
        } else if current_heading.is_some() {
            current_body.push(line.to_string());
        }
    }

    if let Some(last_h) = current_heading {
        let idx = parsed.len();
        let key = blueprint
            .get(idx)
            .map(|s| s.key.to_string())
            .unwrap_or_else(|| format!("section_{}", idx + 1));
        parsed.push((key, last_h, current_body.join("\n").trim().to_string()));
    }

    parsed
}

fn build_llm_router(config: &AppConfig) -> Result<LlmRouter> {
    let mut providers: Vec<LlmProviderSpec> = builtin_nine_providers();
    for p in &mut providers {
        p.api_key = match p.id.as_str() {
            "deepseek" => config.ai_api_key_deepseek.clone(),
            "sensenova" => config.ai_api_key_sensetime.clone(),
            "zhipu" => config.ai_api_key_zhipu.clone(),
            "qwen" => config.ai_api_key_qwen.clone(),
            "openai" => config.ai_api_key_openai.clone(),
            "anthropic" => config.ai_api_key_anthropic.clone(),
            "gemini" => config.ai_api_key_gemini.clone(),
            "openrouter" => config.ai_api_key_openrouter.clone(),
            "xiaomi" => config.ai_api_key_xiaomi.clone(),
            _ => String::new(),
        };
        if p.api_key.trim().is_empty() && !config.ai_api_key.trim().is_empty() && p.id == "deepseek"
        {
            p.api_key = config.ai_api_key.clone();
        }
    }
    if !providers.iter().any(|p| p.is_usable()) {
        return Err(anyhow!("未配置可用 AI Key"));
    }
    LlmRouter::new(providers, 200)
}
