//! 一源四出（含三栖抗辩）多体裁导出编译器（`src/core/runtime/forge_export.rs` · T-205）
//!
//! 覆盖六大体裁（`ProductPrdTrd`、`SurveyFtoReport`、`AcademicPaper`、`PatentDisclosure`、
//! `OaPackage`、`PaperRebuttalPackage`）× 五种格式（`.docx` 含修订留痕 / `.tex`+`.bib` /
//! `.xlsx` / `.md` / `.pdf`），并在导出前强制执行：
//! 1. 先锁专利申请日、后公开论文/开源时序守卫与商业秘密脱敏裁剪；
//! 2. 引证锚点核验与五维质量门禁硬阻断。

use std::io::{Cursor, Write};

use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::core::evidence_rebuttal::{
    anchor_verifier, priority_guard, quality_gate, GateEvaluationInput,
};
use crate::core::vault::models::{
    DeliverableFormat, DeliverableGenre, FiveDimGateReport, GateDecision, KnowledgeDoc,
    PriorityCheckResult,
};

const DOWNGRADE_BANNER: &str = "【待人工补证草稿 / Needs Manual Evidence Verification】";
const REDACTED_PLACEHOLDER: &str = "[商业秘密脱敏保留 / Know-how Redacted]";

/// 一源四出成果编译请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileDeliverableRequest {
    pub title: String,
    pub genre: DeliverableGenre,
    pub format: DeliverableFormat,
    /// 章节标题与正文草稿列表
    pub sections: Vec<(String, String)>,
    /// OA 权项修改或论文返修时的修订前原稿（用于生成修订留痕）
    pub original_text_for_diff: Option<String>,
    /// OA 权项修改或论文返修时的修订后文稿
    pub amended_text_for_diff: Option<String>,
    /// 关联的本地知识金库文档 ID 列表
    pub related_doc_ids: Vec<String>,
    /// 参考文献集（`AcademicPaper` 下强制仅导出 `doi_verified == true` 且非空 `bibtex_raw` 的真实文献）
    pub reference_docs: Vec<KnowledgeDoc>,
    /// 商业秘密脱敏词条
    pub knowhow_markers: Vec<String>,
    /// 是否人工确认跳过时序红灯导出内部草稿
    pub acknowledge_priority_warning: bool,
}

/// 一源四出编译产物
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledDeliverable {
    pub filename: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
    pub companion_bibtex: Option<String>,
    pub priority_check: PriorityCheckResult,
    pub gate_report: FiveDimGateReport,
}

/// 统一多体裁成果导出编译器
#[derive(Debug, Default, Clone, Copy)]
pub struct ForgeExporter;

impl ForgeExporter {
    /// 执行时序守卫、商业秘密脱敏、五维门禁裁决，并编译目标格式字节流
    pub fn compile_deliverable(
        conn: &Connection,
        req: &CompileDeliverableRequest,
    ) -> Result<CompiledDeliverable> {
        let mut combined_parts = Vec::new();
        for (heading, body) in &req.sections {
            combined_parts.push(format!("{}\n{}", heading, body));
        }
        if let Some(amended) = &req.amended_text_for_diff {
            combined_parts.push(amended.clone());
        }
        let combined_draft = combined_parts.join("\n\n");

        // 第一步：公开时序红线检查与商业秘密脱敏裁剪
        let (priority_check, redacted_text) = priority_guard::check_disclosure_priority(
            conn,
            &req.related_doc_ids,
            req.genre,
            &req.knowhow_markers,
            &combined_draft,
        )?;

        let default_doc_id = req.related_doc_ids.first().cloned();
        let _verified_anchors =
            anchor_verifier::verify_all_citations(conn, &redacted_text, default_doc_id.as_deref())?;

        let gate_input = GateEvaluationInput {
            draft_text: redacted_text,
            default_doc_id,
            require_citations: false,
            stitching_alerts: Vec::new(),
            scope_report: None,
            priority_check: Some(priority_check.clone()),
        };
        let mut gate_report = quality_gate::evaluate_submission_gate(conn, &gate_input)?;

        // 参考文献核验过滤：AcademicPaper 强制仅保留 doi_verified == true 且非空 bibtex_raw 的真实文献
        let (accepted_refs, rejected_ref_notes) =
            filter_references_for_genre(req.genre, &req.reference_docs);
        for note in rejected_ref_notes {
            gate_report.downgrade_warnings.push(note);
        }

        if gate_report.decision == GateDecision::HardBlocked && !req.acknowledge_priority_warning {
            let reason = if gate_report.blocking_reasons.is_empty() {
                priority_check.advisory_zh.clone()
            } else {
                gate_report.blocking_reasons.join("；")
            };
            return Err(anyhow!("硬门禁拦截: {}", reason));
        }

        // 对各章节及修订文本执行一致的商业秘密脱敏裁剪
        let mut rendered_sections: Vec<(String, String)> = req
            .sections
            .iter()
            .map(|(h, b)| {
                (
                    redact_knowhow(h, &req.knowhow_markers),
                    redact_knowhow(b, &req.knowhow_markers),
                )
            })
            .collect();

        if gate_report.decision == GateDecision::DowngradeNeedsEvidence {
            let warning_body = if gate_report.downgrade_warnings.is_empty() {
                DOWNGRADE_BANNER.to_string()
            } else {
                format!(
                    "{}\n{}",
                    DOWNGRADE_BANNER,
                    gate_report.downgrade_warnings.join("\n")
                )
            };
            rendered_sections.insert(0, (DOWNGRADE_BANNER.to_string(), warning_body));
        }

        let orig_diff = req
            .original_text_for_diff
            .as_deref()
            .map(|s| redact_knowhow(s, &req.knowhow_markers));
        let amended_diff = req
            .amended_text_for_diff
            .as_deref()
            .map(|s| redact_knowhow(s, &req.knowhow_markers));

        let safe_stem = sanitize_filename(&req.title);
        let bibtex_entries: Vec<String> = accepted_refs
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

        // 第二步：按目标格式编译真实可用的二进制/文本字节流
        let (filename, mime_type, bytes) = match req.format {
            DeliverableFormat::Docx => {
                let bytes = compile_docx(
                    &req.title,
                    req.genre,
                    &rendered_sections,
                    orig_diff.as_deref(),
                    amended_diff.as_deref(),
                    &gate_report,
                )?;
                (
                    format!("{}.docx", safe_stem),
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                        .to_string(),
                    bytes,
                )
            }
            DeliverableFormat::Tex => {
                let tex_src = compile_tex(
                    &req.title,
                    req.genre,
                    &rendered_sections,
                    orig_diff.as_deref(),
                    amended_diff.as_deref(),
                    &accepted_refs,
                    &gate_report,
                );
                (
                    format!("{}.tex", safe_stem),
                    "application/x-tex".to_string(),
                    tex_src.into_bytes(),
                )
            }
            DeliverableFormat::Xlsx => {
                let bytes = compile_xlsx(
                    &req.title,
                    req.genre,
                    &rendered_sections,
                    &req.reference_docs,
                    &accepted_refs,
                    &priority_check,
                    &gate_report,
                )?;
                (
                    format!("{}.xlsx", safe_stem),
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string(),
                    bytes,
                )
            }
            DeliverableFormat::Md => {
                let md_src = compile_markdown(
                    &req.title,
                    req.genre,
                    &rendered_sections,
                    orig_diff.as_deref(),
                    amended_diff.as_deref(),
                    &accepted_refs,
                    &priority_check,
                    &gate_report,
                );
                (
                    format!("{}.md", safe_stem),
                    "text/markdown; charset=utf-8".to_string(),
                    md_src.into_bytes(),
                )
            }
            DeliverableFormat::Pdf => {
                let pdf_bytes = compile_pdf(
                    &req.title,
                    req.genre,
                    &rendered_sections,
                    orig_diff.as_deref(),
                    amended_diff.as_deref(),
                    &gate_report,
                );
                (
                    format!("{}.pdf", safe_stem),
                    "application/pdf".to_string(),
                    pdf_bytes,
                )
            }
        };

        Ok(CompiledDeliverable {
            filename,
            mime_type,
            bytes,
            companion_bibtex,
            priority_check,
            gate_report,
        })
    }
}

fn filter_references_for_genre(
    genre: DeliverableGenre,
    refs: &[KnowledgeDoc],
) -> (Vec<KnowledgeDoc>, Vec<String>) {
    let mut accepted = Vec::new();
    let mut rejected_notes = Vec::new();

    for doc in refs {
        if genre == DeliverableGenre::AcademicPaper {
            if doc.doi_verified && !doc.bibtex_raw.trim().is_empty() {
                accepted.push(doc.clone());
            } else {
                let title = if !doc.title_zh.trim().is_empty() {
                    doc.title_zh.trim()
                } else {
                    doc.title_en.trim()
                };
                rejected_notes.push(format!(
                    "已自动剔除未核验参考文献（AcademicPaper 强校验）：doc_id={}, external_id={}, title={}",
                    doc.doc_id, doc.external_id, title
                ));
            }
        } else if doc.doi_verified || !doc.bibtex_raw.trim().is_empty() {
            accepted.push(doc.clone());
        }
    }

    (accepted, rejected_notes)
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

fn sanitize_filename(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "innoforge_deliverable".to_string()
    } else {
        trimmed.chars().take(64).collect()
    }
}

fn genre_label(genre: DeliverableGenre) -> &'static str {
    match genre {
        DeliverableGenre::ProductPrdTrd => "Product PRD/TRD",
        DeliverableGenre::SurveyFtoReport => "Survey & FTO Report",
        DeliverableGenre::AcademicPaper => "Academic Paper",
        DeliverableGenre::PatentDisclosure => "Patent Disclosure",
        DeliverableGenre::OaPackage => "OA Response Package",
        DeliverableGenre::PaperRebuttalPackage => "Paper Rebuttal Package",
    }
}

fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

fn write_zip_entry(zip: &mut ZipWriter<Cursor<Vec<u8>>>, path: &str, content: &str) -> Result<()> {
    zip.start_file(path, SimpleFileOptions::default())
        .with_context(|| format!("创建 ZIP 条目失败: {}", path))?;
    zip.write_all(content.as_bytes())
        .with_context(|| format!("写入 ZIP 条目失败: {}", path))?;
    Ok(())
}

fn compile_docx(
    title: &str,
    genre: DeliverableGenre,
    sections: &[(String, String)],
    original_diff: Option<&str>,
    amended_diff: Option<&str>,
    gate_report: &FiveDimGateReport,
) -> Result<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);

    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-officedocument.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;
    write_zip_entry(&mut zip, "[Content_Types].xml", content_types)?;

    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;
    write_zip_entry(&mut zip, "_rels/.rels", rels)?;

    let mut body_xml = String::new();
    body_xml.push_str(&format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Title\"/></w:pPr><w:r><w:t>{}</w:t></w:r></w:p>\n",
        escape_xml(title)
    ));
    body_xml.push_str(&format!(
        "<w:p><w:r><w:t>Genre: {} | Gate: {:?} (Score: {:.1})</w:t></w:r></w:p>\n",
        escape_xml(genre_label(genre)),
        gate_report.decision,
        gate_report.overall_score
    ));

    for (heading, content) in sections {
        body_xml.push_str(&format!(
            "<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>{}</w:t></w:r></w:p>\n",
            escape_xml(heading)
        ));
        for line in content.lines() {
            body_xml.push_str(&format!(
                "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>\n",
                escape_xml(line)
            ));
        }
    }

    if let (Some(orig), Some(amended)) = (original_diff, amended_diff) {
        body_xml.push_str(
            "<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Tracked Changes / 修订留痕</w:t></w:r></w:p>\n",
        );
        body_xml.push_str(&format!(
            "<w:p>\
             <w:del w:id=\"1\" w:author=\"InnoForge\" w:date=\"2026-11-18T00:00:00Z\"><w:r><w:delText xml:space=\"preserve\">{}</w:delText></w:r></w:del>\
             <w:ins w:id=\"2\" w:author=\"InnoForge\" w:date=\"2026-11-18T00:00:00Z\"><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:ins>\
             </w:p>\n",
            escape_xml(orig),
            escape_xml(amended)
        ));
    }

    let document_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    {body_xml}
    <w:sectPr>
      <w:pgSz w:w="11906" w:h="16838"/>
      <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/>
    </w:sectPr>
  </w:body>
</w:document>"#
    );
    write_zip_entry(&mut zip, "word/document.xml", &document_xml)?;

    let cursor = zip.finish().context("完成 DOCX ZIP 打包失败")?;
    Ok(cursor.into_inner())
}

fn extract_bibtex_cite_key(bibtex_raw: &str) -> Option<String> {
    let start = bibtex_raw.find('{')?;
    let rest = bibtex_raw.get(start + 1..)?;
    let comma = rest.find(',')?;
    let key = rest.get(..comma)?.trim();
    if key.is_empty() {
        None
    } else {
        Some(key.to_string())
    }
}

fn escape_latex(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str(r"\textbackslash{}"),
            '{' => out.push_str(r"\{"),
            '}' => out.push_str(r"\}"),
            '%' => out.push_str(r"\%"),
            '$' => out.push_str(r"\$"),
            '&' => out.push_str(r"\&"),
            '#' => out.push_str(r"\#"),
            '_' => out.push_str(r"\_"),
            _ => out.push(ch),
        }
    }
    out
}

fn compile_tex(
    title: &str,
    genre: DeliverableGenre,
    sections: &[(String, String)],
    original_diff: Option<&str>,
    amended_diff: Option<&str>,
    accepted_refs: &[KnowledgeDoc],
    gate_report: &FiveDimGateReport,
) -> String {
    let mut out = String::new();
    out.push_str("\\documentclass{article}\n");
    out.push_str("\\usepackage[UTF8]{ctex}\n");
    out.push_str(&format!("\\title{{{}}}\n", escape_latex(title)));
    out.push_str("\\author{InnoForge}\n");
    out.push_str("\\begin{document}\n");
    out.push_str("\\maketitle\n");
    out.push_str(&format!(
        "% Genre: {} | Gate: {:?} | Overall Score: {:.1}\n\n",
        genre_label(genre),
        gate_report.decision,
        gate_report.overall_score
    ));

    for (heading, body) in sections {
        out.push_str(&format!("\\section{{{}}}\n", escape_latex(heading)));
        out.push_str(&escape_latex(body));
        out.push_str("\n\n");
    }

    if let (Some(orig), Some(amended)) = (original_diff, amended_diff) {
        out.push_str("\\section{Revision Comparison}\n");
        out.push_str(&format!("\\textbf{{Original:}} {}\n\n", escape_latex(orig)));
        out.push_str(&format!(
            "\\textbf{{Amended:}} {}\n\n",
            escape_latex(amended)
        ));
    }

    let cite_keys: Vec<String> = accepted_refs
        .iter()
        .filter_map(|doc| extract_bibtex_cite_key(&doc.bibtex_raw))
        .collect();
    if !cite_keys.is_empty() {
        out.push_str("\\section{Verified References}\n");
        out.push_str(&format!(
            "Verified citations: \\cite{{{}}}.\n\n",
            cite_keys.join(",")
        ));
        out.push_str("\\bibliographystyle{plain}\n");
        out.push_str("\\bibliography{references}\n");
    }

    out.push_str("\\end{document}\n");
    out
}

fn compile_xlsx(
    title: &str,
    genre: DeliverableGenre,
    sections: &[(String, String)],
    all_refs: &[KnowledgeDoc],
    accepted_refs: &[KnowledgeDoc],
    priority_check: &PriorityCheckResult,
    gate_report: &FiveDimGateReport,
) -> Result<Vec<u8>> {
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let header_fmt = rust_xlsxwriter::Format::new().set_bold();

    // Sheet 1: Sections & Summary
    let sheet_sections = workbook.add_worksheet();
    sheet_sections.set_name("Deliverable")?;
    sheet_sections.write_string_with_format(0, 0, "Title", &header_fmt)?;
    sheet_sections.write_string(0, 1, title)?;
    sheet_sections.write_string_with_format(1, 0, "Genre", &header_fmt)?;
    sheet_sections.write_string(1, 1, genre_label(genre))?;
    sheet_sections.write_string_with_format(2, 0, "Priority Lamp", &header_fmt)?;
    sheet_sections.write_string(2, 1, &priority_check.lamp)?;
    sheet_sections.write_string_with_format(3, 0, "Gate Decision", &header_fmt)?;
    sheet_sections.write_string(3, 1, format!("{:?}", gate_report.decision))?;

    sheet_sections.write_string_with_format(5, 0, "Section", &header_fmt)?;
    sheet_sections.write_string_with_format(5, 1, "Content", &header_fmt)?;
    for (idx, (heading, body)) in sections.iter().enumerate() {
        let row = (idx + 6) as u32;
        sheet_sections.write_string(row, 0, heading)?;
        sheet_sections.write_string(row, 1, body)?;
    }

    // Sheet 2: Five-Dim Gate Metrics
    let sheet_metrics = workbook.add_worksheet();
    sheet_metrics.set_name("GateMetrics")?;
    sheet_metrics.write_string_with_format(0, 0, "Dimension", &header_fmt)?;
    sheet_metrics.write_string_with_format(0, 1, "Score", &header_fmt)?;
    let metrics = [
        ("Anchor Accuracy", gate_report.anchor_accuracy_score),
        ("Scope Compliance", gate_report.scope_compliance_score),
        ("Logic Completeness", gate_report.logic_completeness_score),
        (
            "Embodiment Consistency",
            gate_report.embodiment_consistency_score,
        ),
        ("Priority & Privacy", gate_report.priority_privacy_score),
        ("Overall Score", gate_report.overall_score),
    ];
    for (idx, (name, score)) in metrics.iter().enumerate() {
        let row = (idx + 1) as u32;
        sheet_metrics.write_string(row, 0, *name)?;
        sheet_metrics.write_number(row, 1, *score)?;
    }

    // Sheet 3: References Audit
    let sheet_refs = workbook.add_worksheet();
    sheet_refs.set_name("ReferencesAudit")?;
    sheet_refs.write_string_with_format(0, 0, "Doc ID", &header_fmt)?;
    sheet_refs.write_string_with_format(0, 1, "External ID", &header_fmt)?;
    sheet_refs.write_string_with_format(0, 2, "DOI Verified", &header_fmt)?;
    sheet_refs.write_string_with_format(0, 3, "Export Status", &header_fmt)?;
    for (idx, doc) in all_refs.iter().enumerate() {
        let row = (idx + 1) as u32;
        let is_accepted = accepted_refs.iter().any(|a| a.doc_id == doc.doc_id);
        sheet_refs.write_string(row, 0, &doc.doc_id)?;
        sheet_refs.write_string(row, 1, &doc.external_id)?;
        sheet_refs.write_boolean(row, 2, doc.doi_verified)?;
        sheet_refs.write_string(row, 3, if is_accepted { "Accepted" } else { "Rejected" })?;
    }

    let buf = workbook.save_to_buffer().context("生成 XLSX 字节流失败")?;
    Ok(buf)
}

#[allow(clippy::too_many_arguments)]
fn compile_markdown(
    title: &str,
    genre: DeliverableGenre,
    sections: &[(String, String)],
    original_diff: Option<&str>,
    amended_diff: Option<&str>,
    accepted_refs: &[KnowledgeDoc],
    priority_check: &PriorityCheckResult,
    gate_report: &FiveDimGateReport,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", title));
    out.push_str(&format!(
        "- **Genre**: {}\n- **Priority Lamp**: {}\n- **Gate Decision**: {:?} (Overall: {:.1})\n\n",
        genre_label(genre),
        priority_check.lamp,
        gate_report.decision,
        gate_report.overall_score
    ));

    for (heading, body) in sections {
        out.push_str(&format!("## {}\n\n{}\n\n", heading, body));
    }

    if let (Some(orig), Some(amended)) = (original_diff, amended_diff) {
        out.push_str("## Tracked Changes\n\n");
        out.push_str(&format!("~~{}~~\n\n**Amended:** {}\n\n", orig, amended));
    }

    if !gate_report.verified_citations.is_empty() {
        out.push_str("## Citation Audit\n\n");
        out.push_str("| Mention | Doc ID | Anchor | Verdict |\n");
        out.push_str("| --- | --- | --- | --- |\n");
        for c in &gate_report.verified_citations {
            out.push_str(&format!(
                "| {} | {} | {} | {:?} |\n",
                c.raw_mention, c.doc_id, c.anchor_tag, c.verdict
            ));
        }
        out.push('\n');
    }

    if !accepted_refs.is_empty() {
        out.push_str("## Verified References\n\n");
        for doc in accepted_refs {
            out.push_str(&format!("- `{}` ({})\n", doc.doc_id, doc.external_id));
        }
        out.push('\n');
    }

    out
}

fn escape_pdf_ascii(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
            _ => out.push('?'),
        }
    }
    out
}

fn compile_pdf(
    title: &str,
    genre: DeliverableGenre,
    sections: &[(String, String)],
    original_diff: Option<&str>,
    amended_diff: Option<&str>,
    gate_report: &FiveDimGateReport,
) -> Vec<u8> {
    let mut text_lines = Vec::new();
    text_lines.push(format!("InnoForge Deliverable: {}", title));
    text_lines.push(format!(
        "Genre: {} | Gate: {:?} | Score: {:.1}",
        genre_label(genre),
        gate_report.decision,
        gate_report.overall_score
    ));
    for (heading, body) in sections {
        text_lines.push(format!("-- {} --", heading));
        for line in body.lines() {
            text_lines.push(line.to_string());
        }
    }
    if let (Some(orig), Some(amended)) = (original_diff, amended_diff) {
        text_lines.push(format!("DEL: {}", orig));
        text_lines.push(format!("INS: {}", amended));
    }

    let mut stream = String::from("BT\n/F1 11 Tf\n50 760 Td\n14 TL\n");
    for (idx, line) in text_lines.iter().take(45).enumerate() {
        let escaped = escape_pdf_ascii(line);
        if idx == 0 {
            stream.push_str(&format!("({}) Tj\n", escaped));
        } else {
            stream.push_str(&format!("T* ({}) Tj\n", escaped));
        }
    }
    stream.push_str("ET\n");

    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");

    let mut offsets: Vec<usize> = Vec::with_capacity(6);
    offsets.push(0); // object 0

    let objs = [
        "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_string(),
        "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".to_string(),
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 5 0 R /Resources << /Font << /F1 4 0 R >> >> >>\nendobj\n".to_string(),
        "4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n".to_string(),
        format!(
            "5 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj\n",
            stream.len(),
            stream
        ),
    ];

    for obj in &objs {
        offsets.push(pdf.len());
        pdf.extend_from_slice(obj.as_bytes());
    }

    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", offsets.len()).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            offsets.len(),
            xref_offset
        )
        .as_bytes(),
    );
    pdf
}
