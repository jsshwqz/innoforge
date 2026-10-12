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
            response_text: "| 特征 | 原始申请文件依据 | 合规性 |\n| :--- | :--- | :--- |\n| 电动执行器为电缸 | 说明书第[0025]段：所述电缸的伸缩轴与阀芯连接。 | ✅ |\n| 智能模块控制电缸行程 | 说明书第[0028]段：使用智能算法控制电动执行器行程。 | ✅ |".to_string(),
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
}
