//! OA 答复书 docx 导出模块
//!
//! 使用纯手写 XML 生成简单 docx，依赖已有的 zip crate

use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// 导出参数
pub struct ExportParams {
    pub response_text: String,
    pub patent_number: String,
    pub applicant: String,
    pub oa_type: String,
}

/// 生成 docx 字节流
pub fn generate_docx(params: &ExportParams) -> Result<Vec<u8>, String> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);

    // 1. [Content_Types].xml
    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#;
    write_docx_file(&mut zip, "[Content_Types].xml", content_types)?;

    // 2. _rels/.rels
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;
    write_docx_file(&mut zip, "_rels/.rels", rels)?;

    // 3. word/document.xml
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="Title"/></w:pPr>
      <w:r><w:t>意见陈述书</w:t></w:r></w:p>
    <w:p>
      <w:r><w:t>申请号：{patent_number}</w:t></w:r></w:p>
    <w:p>
      <w:r><w:t>申请人：{applicant}</w:t></w:r></w:p>
    <w:p>
      <w:r><w:t>OA类型：{oa_type}</w:t></w:r></w:p>
    <w:p/>
    {content}
    <w:p>
      <w:r><w:t>申请人（签字）：_________________</w:t></w:r></w:p>
    <w:p>
      <w:r><w:t>日期：________年____月____日</w:t></w:r></w:p>
    <w:sectPr>
      <w:pgSz w:w="11906" w:h="16838"/>
      <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/>
    </w:sectPr>
  </w:body>
</w:document>"#,
        patent_number = sanitize_xml(&params.patent_number),
        applicant = sanitize_xml(&params.applicant),
        oa_type = sanitize_xml(&params.oa_type),
        content = format_paragraphs(&params.response_text)
    );
    write_docx_file(&mut zip, "word/document.xml", &document)?;

    // 4. word/_rels/document.xml.rels
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;
    write_docx_file(&mut zip, "word/_rels/document.xml.rels", doc_rels)?;

    // 5. word/styles.xml
    let styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="Title">
    <w:name w:val="Title"/>
    <w:basedOn w:val="Normal"/>
    <w:next w:val="Normal"/>
    <w:pPr>
      <w:spacing w:before="0" w:after="0" w:line="360" w:lineRule="auto"/>
      <w:justify w:val="center"/>
    </w:pPr>
    <w:rPr>
      <w:rFonts w:ascii="SimSun" w:hAnsi="SimSun" w:eastAsia="宋体"/>
      <w:b w:val="true"/>
      <w:sz w:val="32"/>
    </w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Normal">
    <w:name w:val="Normal"/>
    <w:pPr>
      <w:spacing w:before="0" w:after="0" w:line="360" w:lineRule="auto"/>
    </w:pPr>
    <w:rPr>
      <w:rFonts w:ascii="SimSun" w:hAnsi="SimSun" w:eastAsia="宋体"/>
      <w:sz w:val="24"/>
    </w:rPr>
  </w:style>
</w:styles>"#;
    write_docx_file(&mut zip, "word/styles.xml", styles)?;

    let cursor = zip
        .finish()
        .map_err(|error| format!("无法完成 DOCX 文件: {error}"))?;
    Ok(cursor.into_inner())
}

fn write_docx_file(
    zip: &mut ZipWriter<Cursor<Vec<u8>>>,
    path: &str,
    contents: &str,
) -> Result<(), String> {
    zip.start_file(path, SimpleFileOptions::default())
        .map_err(|error| format!("无法创建 DOCX 文件 {path}: {error}"))?;
    zip.write_all(contents.as_bytes())
        .map_err(|error| format!("无法写入 DOCX 文件 {path}: {error}"))
}

/// 通用 Markdown 报告转 Word 参数
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MarkdownDocxParams {
    pub title: String,
    pub subtitle: String,
    pub markdown: String,
}

/// 生成通用结构化 Markdown 报告 `.docx` 字节流（支持标题、加粗/行内代码、列表、引用块、代码块与表格）
pub fn generate_markdown_docx(params: &MarkdownDocxParams) -> Result<Vec<u8>, String> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);

    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#;
    write_docx_file(&mut zip, "[Content_Types].xml", content_types)?;

    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;
    write_docx_file(&mut zip, "_rels/.rels", rels)?;

    let safe_title = if params.title.trim().is_empty() {
        "创研台技术分析报告"
    } else {
        params.title.trim()
    };
    let subtitle_xml = if params.subtitle.trim().is_empty() {
        String::new()
    } else {
        format!(
            r#"<w:p><w:pPr><w:jc w:val="center"/><w:spacing w:after="200"/></w:pPr><w:r><w:rPr><w:color w:val="595959"/><w:sz w:val="20"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
            sanitize_xml(params.subtitle.trim())
        )
    };
    let body_xml = format_rich_markdown(&params.markdown);

    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="Title"/><w:jc w:val="center"/><w:spacing w:after="120"/></w:pPr>
      <w:r><w:t xml:space="preserve">{title}</w:t></w:r>
    </w:p>
    {subtitle}
    {content}
    <w:sectPr>
      <w:pgSz w:w="11906" w:h="16838"/>
      <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/>
    </w:sectPr>
  </w:body>
</w:document>"#,
        title = sanitize_xml(safe_title),
        subtitle = subtitle_xml,
        content = body_xml
    );
    write_docx_file(&mut zip, "word/document.xml", &document)?;

    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;
    write_docx_file(&mut zip, "word/_rels/document.xml.rels", doc_rels)?;

    let styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="Title">
    <w:name w:val="Title"/>
    <w:basedOn w:val="Normal"/>
    <w:rPr>
      <w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:eastAsia="微软雅黑"/>
      <w:b w:val="true"/>
      <w:color w:val="1B365D"/>
      <w:sz w:val="36"/>
    </w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Normal">
    <w:name w:val="Normal"/>
    <w:pPr>
      <w:spacing w:before="40" w:after="100" w:line="320" w:lineRule="auto"/>
    </w:pPr>
    <w:rPr>
      <w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:eastAsia="微软雅黑"/>
      <w:sz w:val="21"/>
    </w:rPr>
  </w:style>
</w:styles>"#;
    write_docx_file(&mut zip, "word/styles.xml", styles)?;

    let cursor = zip
        .finish()
        .map_err(|error| format!("无法完成 DOCX 文件: {error}"))?;
    Ok(cursor.into_inner())
}

fn format_inline_markdown_runs(text: &str) -> String {
    // 轻量解析 **加粗** 与 `行内代码`
    let mut out = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        let next_bold = rest.find("**");
        let next_code = rest.find('`');
        let choose_bold = match (next_bold, next_code) {
            (Some(b), Some(c)) => b <= c,
            (Some(_), None) => true,
            _ => false,
        };
        if choose_bold {
            let Some(b) = next_bold else { break };
            if b > 0 {
                if let Some(prefix) = rest.get(..b) {
                    out.push_str(&format!(
                        r#"<w:r><w:t xml:space="preserve">{}</w:t></w:r>"#,
                        sanitize_xml(prefix)
                    ));
                }
            }
            let after = rest.get(b + 2..).unwrap_or("");
            if let Some(end) = after.find("**") {
                let inner = after.get(..end).unwrap_or("");
                out.push_str(&format!(
                    r#"<w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
                    sanitize_xml(inner)
                ));
                rest = after.get(end + 2..).unwrap_or("");
            } else {
                out.push_str(&format!(
                    r#"<w:r><w:t xml:space="preserve">{}</w:t></w:r>"#,
                    sanitize_xml(rest.get(b..).unwrap_or(""))
                ));
                break;
            }
        } else if let Some(c) = next_code {
            if c > 0 {
                if let Some(prefix) = rest.get(..c) {
                    out.push_str(&format!(
                        r#"<w:r><w:t xml:space="preserve">{}</w:t></w:r>"#,
                        sanitize_xml(prefix)
                    ));
                }
            }
            let after = rest.get(c + 1..).unwrap_or("");
            if let Some(end) = after.find('`') {
                let inner = after.get(..end).unwrap_or("");
                out.push_str(&format!(
                    r#"<w:r><w:rPr><w:rFonts w:ascii="Consolas" w:hAnsi="Consolas"/><w:color w:val="0B5394"/><w:shd w:val="clear" w:fill="F2F4F8"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
                    sanitize_xml(inner)
                ));
                rest = after.get(end + 1..).unwrap_or("");
            } else {
                out.push_str(&format!(
                    r#"<w:r><w:t xml:space="preserve">{}</w:t></w:r>"#,
                    sanitize_xml(rest.get(c..).unwrap_or(""))
                ));
                break;
            }
        } else {
            out.push_str(&format!(
                r#"<w:r><w:t xml:space="preserve">{}</w:t></w:r>"#,
                sanitize_xml(rest)
            ));
            break;
        }
    }
    out
}

fn format_rich_markdown(text: &str) -> String {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    let mut in_code_fence = false;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        if trimmed.starts_with("```") {
            in_code_fence = !in_code_fence;
            i += 1;
            continue;
        }
        if in_code_fence {
            blocks.push(format!(
                r#"<w:p><w:pPr><w:shd w:val="clear" w:fill="F4F6F8"/><w:ind w:left="240"/><w:spacing w:before="0" w:after="20" w:line="260" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Consolas" w:hAnsi="Consolas"/><w:sz w:val="18"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
                sanitize_xml(lines[i])
            ));
            i += 1;
            continue;
        }

        if i + 1 < lines.len() {
            if let Some(header_cells) = markdown_table_cells(lines[i]) {
                if is_markdown_table_separator(lines[i + 1], header_cells.len()) {
                    let mut rows = Vec::new();
                    let mut data_i = i + 2;
                    while data_i < lines.len() {
                        let Some(cells) = markdown_table_cells(lines[data_i]) else {
                            break;
                        };
                        if cells.len() != header_cells.len() {
                            break;
                        }
                        rows.push(cells);
                        data_i += 1;
                    }
                    if !rows.is_empty() {
                        blocks.push(format_markdown_table(&header_cells, &rows));
                        blocks.push("<w:p/>".to_string());
                        i = data_i;
                        continue;
                    }
                }
            }
        }

        if trimmed.is_empty() {
            blocks.push("<w:p/>".to_string());
        } else if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            blocks.push(
                r#"<w:p><w:pPr><w:pBdr><w:bottom w:val="single" w:sz="6" w:space="1" w:color="CCCCCC"/></w:pBdr></w:pPr></w:p>"#
                    .to_string(),
            );
        } else if let Some(h) = trimmed.strip_prefix("#### ") {
            blocks.push(format!(
                r#"<w:p><w:pPr><w:keepNext/><w:spacing w:before="140" w:after="60"/></w:pPr><w:r><w:rPr><w:b/><w:color w:val="2F5597"/><w:sz w:val="22"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
                sanitize_xml(h.trim())
            ));
        } else if let Some(h) = trimmed.strip_prefix("### ") {
            blocks.push(format!(
                r#"<w:p><w:pPr><w:keepNext/><w:spacing w:before="180" w:after="80"/></w:pPr><w:r><w:rPr><w:b/><w:color w:val="1F4E79"/><w:sz w:val="24"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
                sanitize_xml(h.trim())
            ));
        } else if let Some(h) = trimmed.strip_prefix("## ") {
            blocks.push(format!(
                r#"<w:p><w:pPr><w:keepNext/><w:spacing w:before="220" w:after="100"/><w:pBdr><w:bottom w:val="single" w:sz="4" w:space="4" w:color="D0D7DE"/></w:pBdr></w:pPr><w:r><w:rPr><w:b/><w:color w:val="1B365D"/><w:sz w:val="28"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
                sanitize_xml(h.trim())
            ));
        } else if let Some(h) = trimmed.strip_prefix("# ") {
            blocks.push(format!(
                r#"<w:p><w:pPr><w:keepNext/><w:spacing w:before="260" w:after="120"/></w:pPr><w:r><w:rPr><w:b/><w:color w:val="1B365D"/><w:sz w:val="32"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
                sanitize_xml(h.trim())
            ));
        } else if let Some(q) = trimmed.strip_prefix("> ") {
            blocks.push(format!(
                r#"<w:p><w:pPr><w:shd w:val="clear" w:fill="F6F8FA"/><w:pBdr><w:left w:val="single" w:sz="12" w:space="8" w:color="1F6FEB"/></w:pBdr><w:ind w:left="240"/></w:pPr>{}</w:p>"#,
                format_inline_markdown_runs(q.trim())
            ));
        } else if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("• "))
        {
            blocks.push(format!(
                r#"<w:p><w:pPr><w:ind w:left="360" w:hanging="180"/></w:pPr><w:r><w:t xml:space="preserve">• </w:t></w:r>{}</w:p>"#,
                format_inline_markdown_runs(item.trim())
            ));
        } else {
            blocks.push(format!(
                "<w:p>{}</w:p>",
                format_inline_markdown_runs(trimmed)
            ));
        }

        i += 1;
    }

    blocks.join("\n    ")
}

/// 将文本转换为 docx 段落 XML
fn format_paragraphs(text: &str) -> String {
    let mut paragraphs = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut line_index = 0;

    while line_index < lines.len() {
        if line_index + 1 < lines.len() {
            if let Some(header_cells) = markdown_table_cells(lines[line_index]) {
                if is_markdown_table_separator(lines[line_index + 1], header_cells.len()) {
                    let mut rows = Vec::new();
                    let mut data_index = line_index + 2;
                    while data_index < lines.len() {
                        let Some(cells) = markdown_table_cells(lines[data_index]) else {
                            break;
                        };
                        if cells.len() != header_cells.len() {
                            break;
                        }
                        rows.push(cells);
                        data_index += 1;
                    }

                    if !rows.is_empty() {
                        paragraphs.push(format_markdown_table(&header_cells, &rows));
                        line_index = data_index;
                        continue;
                    }
                }
            }
        }

        let line = lines[line_index];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            paragraphs.push("<w:p/>".to_string());
        } else {
            // 简单处理标题（以"一、""二、"等开头）
            let _is_heading = trimmed.starts_with("一、")
                || trimmed.starts_with("二、")
                || trimmed.starts_with("三、")
                || trimmed.starts_with("四、")
                || trimmed.starts_with("五、")
                || trimmed.starts_with("（一）")
                || trimmed.starts_with("（二）")
                || trimmed.starts_with("（三）");

            paragraphs.push(format!(
                r#"<w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
                sanitize_xml(trimmed)
            ));
        }
        line_index += 1;
    }
    paragraphs.join("\n    ")
}

fn markdown_table_cells(line: &str) -> Option<Vec<&str>> {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
        return None;
    }

    let cells: Vec<&str> = trimmed[1..trimmed.len() - 1]
        .split('|')
        .map(str::trim)
        .collect();
    (cells.len() >= 2).then_some(cells)
}

fn is_markdown_table_separator(line: &str, expected_columns: usize) -> bool {
    let Some(cells) = markdown_table_cells(line) else {
        return false;
    };
    cells.len() == expected_columns
        && cells.iter().all(|cell| {
            let marker = cell.trim_matches(':').trim();
            marker.len() >= 3 && marker.chars().all(|character| character == '-')
        })
}

fn format_markdown_table(header_cells: &[&str], rows: &[Vec<&str>]) -> String {
    let column_width = 9000 / header_cells.len().max(1);
    let mut table = String::from(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblLayout w:type="autofit"/><w:tblBorders><w:top w:val="single" w:sz="4" w:color="808080"/><w:left w:val="single" w:sz="4" w:color="808080"/><w:bottom w:val="single" w:sz="4" w:color="808080"/><w:right w:val="single" w:sz="4" w:color="808080"/><w:insideH w:val="single" w:sz="4" w:color="B0B0B0"/><w:insideV w:val="single" w:sz="4" w:color="B0B0B0"/></w:tblBorders></w:tblPr>"#,
    );

    table.push_str(&format_docx_table_row(header_cells, column_width, true));
    for row in rows {
        table.push_str(&format_docx_table_row(row, column_width, false));
    }
    table.push_str("</w:tbl>");
    table
}

fn format_docx_table_row(cells: &[&str], column_width: usize, is_header: bool) -> String {
    let mut row = String::from("<w:tr>");
    for cell in cells {
        let shading = if is_header {
            r#"<w:shd w:val="clear" w:fill="D9EAF7"/>"#
        } else {
            ""
        };
        let bold = if is_header { "<w:b/>" } else { "" };
        row.push_str(&format!(
            r#"<w:tc><w:tcPr><w:tcW w:w="{column_width}" w:type="dxa"/>{shading}</w:tcPr><w:p><w:r><w:rPr>{bold}</w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p></w:tc>"#,
            sanitize_xml(cell)
        ));
    }
    row.push_str("</w:tr>");
    row
}

/// 转义 XML 特殊字符
fn sanitize_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::{generate_docx, ExportParams};
    use std::io::{Cursor, Read};
    use zip::ZipArchive;

    #[test]
    fn generated_docx_escapes_all_untrusted_xml_text() {
        let params = ExportParams {
            response_text: "正文&<RESPONSE>".to_string(),
            patent_number: "PATENT&<NUMBER>".to_string(),
            applicant: "APPLICANT&<NAME>".to_string(),
            oa_type: "OA&<TYPE>".to_string(),
        };

        let docx = generate_docx(&params).expect("DOCX should be generated");
        let mut archive = ZipArchive::new(Cursor::new(docx)).expect("DOCX should be a valid ZIP");
        let mut document = archive
            .by_name("word/document.xml")
            .expect("DOCX document XML should exist");
        let mut document_xml = String::new();
        document
            .read_to_string(&mut document_xml)
            .expect("DOCX document XML should be readable");

        for escaped_value in [
            "正文&amp;&lt;RESPONSE&gt;",
            "PATENT&amp;&lt;NUMBER&gt;",
            "APPLICANT&amp;&lt;NAME&gt;",
            "OA&amp;&lt;TYPE&gt;",
        ] {
            assert!(document_xml.contains(escaped_value));
        }
        assert!(!document_xml.contains("PATENT&<NUMBER>"));
        assert!(!document_xml.contains("APPLICANT&<NAME>"));
        assert!(!document_xml.contains("OA&<TYPE>"));
        assert!(!document_xml.contains("正文&<RESPONSE>"));
    }

    #[test]
    fn generated_docx_keeps_response_body_in_document_xml() {
        let params = ExportParams {
            response_text: "一、答复意见\n\n申请人认为，本申请具备创造性。".to_string(),
            patent_number: "202610000001.0".to_string(),
            applicant: "测试申请人".to_string(),
            oa_type: "第一次审查意见通知书".to_string(),
        };

        let docx = generate_docx(&params).expect("DOCX should be generated");
        let mut archive = ZipArchive::new(Cursor::new(docx)).expect("DOCX should be a valid ZIP");
        let mut document = archive
            .by_name("word/document.xml")
            .expect("DOCX document XML should exist");
        let mut document_xml = String::new();
        document
            .read_to_string(&mut document_xml)
            .expect("DOCX document XML should be readable");

        assert!(document_xml.contains("一、答复意见"));
        assert!(document_xml.contains("本申请具备创造性。"));
        assert!(document_xml.contains("<w:sectPr>"));
        assert!(document_xml.contains("</w:sectPr>\n  </w:body>"));
        assert!(!document_xml.contains("</w:p>\n    </w:p>\n  </w:body>"));
        assert_eq!(
            document_xml.matches("<w:t").count(),
            document_xml.matches("</w:t>").count()
        );
        assert_eq!(
            document_xml.matches("<w:r>").count(),
            document_xml.matches("</w:r>").count()
        );
    }

    #[test]
    fn generated_docx_converts_markdown_table_to_native_word_table() {
        let params = ExportParams {
            response_text: "| 特征 | 原始申请文件依据 | 合规性 |\n| :--- | :--- | :--- |\n| 电动执行器为伺服电机 | 说明书第[0025]段：所述驱动电机的伸缩轴与执行构件连接。 | ✅ |\n| 智能模块控制电缸行程 | 说明书第[0028]段：使用智能算法控制电动执行器行程。 | ✅ |".to_string(),
            patent_number: "202610000001.0".to_string(),
            applicant: "测试申请人".to_string(),
            oa_type: "第一次审查意见通知书".to_string(),
        };

        let docx = generate_docx(&params).expect("DOCX should be generated");
        let mut archive = ZipArchive::new(Cursor::new(docx)).expect("DOCX should be a valid ZIP");
        let mut document = archive
            .by_name("word/document.xml")
            .expect("DOCX document XML should exist");
        let mut document_xml = String::new();
        document
            .read_to_string(&mut document_xml)
            .expect("DOCX document XML should be readable");

        assert!(document_xml.contains("<w:tbl>"));
        assert!(document_xml.contains("<w:shd w:val=\"clear\" w:fill=\"D9EAF7\"/>"));
        assert!(document_xml.contains("原始申请文件依据"));
        assert!(document_xml.contains("说明书第[0028]段：使用智能算法控制电动执行器行程。"));
        assert!(!document_xml.contains("| :--- | :--- | :--- |"));
    }

    #[test]
    fn generated_markdown_docx_renders_headings_bold_bullets_and_tables() {
        let params = super::MarkdownDocxParams {
            title: "跨文档深度对比报告".to_string(),
            subtitle: "InnoForge 实测验证".to_string(),
            markdown: "# 总览\n\n## 核心差异\n\n- **关键特征**：采用 `SiC` 逆变器与双风道冷却\n> 引用：详见说明书第[0018]段\n\n| 文献 | 差异点 | 风险 |\n| --- | --- | --- |\n| D1 | 未公开螺旋导流筋 | 低 |\n| D2 | 仅限风冷结构 | 中 |\n".to_string(),
        };
        let docx = super::generate_markdown_docx(&params).expect("markdown DOCX should generate");
        let mut archive = ZipArchive::new(Cursor::new(docx)).expect("valid zip");
        let mut doc = archive
            .by_name("word/document.xml")
            .expect("word/document.xml");
        let mut xml = String::new();
        doc.read_to_string(&mut xml).expect("readable xml");

        assert!(xml.contains("跨文档深度对比报告"));
        assert!(xml.contains("核心差异"));
        assert!(xml.contains("<w:b/>"));
        assert!(xml.contains("SiC"));
        assert!(xml.contains("<w:tbl>"));
        assert!(xml.contains("未公开螺旋导流筋"));
        assert_eq!(
            xml.matches("<w:t ").count() + xml.matches("<w:t>").count(),
            xml.matches("</w:t>").count()
        );
    }
}
