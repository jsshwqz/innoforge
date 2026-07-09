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
pub fn generate_docx(params: &ExportParams) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut cursor);

    // 1. [Content_Types].xml
    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#;
    zip.start_file("[Content_Types].xml", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(content_types.as_bytes()).unwrap();

    // 2. _rels/.rels
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;
    zip.start_file("_rels/.rels", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(rels.as_bytes()).unwrap();

    // 3. word/document.xml
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="Title"/></w:pPr>
      <w:r><w:t>意见陈述书</w:r></w:p>
    <w:p>
      <w:r><w:t>申请号：{patent_number}</w:r></w:p>
    <w:p>
      <w:r><w:t>申请人：{applicant}</w:r></w:p>
    <w:p>
      <w:r><w:t>OA类型：{oa_type}</w:r></w:p>
    <w:p/>
    {content}
    <w:p>
      <w:r><w:t>申请人（签字）：_________________</w:r></w:p>
    <w:p>
      <w:r><w:t>日期：________年____月____日</w:r></w:p>
    </w:p>
  </w:body>
</w:document>"#,
        patent_number = params.patent_number,
        applicant = params.applicant,
        oa_type = params.oa_type,
        content = format_paragraphs(&params.response_text)
    );
    zip.start_file("word/document.xml", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(document.as_bytes()).unwrap();

    // 4. word/_rels/document.xml.rels
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;
    zip.start_file("word/_rels/document.xml.rels", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(doc_rels.as_bytes()).unwrap();

    // 5. word/styles.xml
    let styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="Title">
    <w:name w:val="Title"/>
    <w:basedOn w:val="Normal"/>
    <w:next w:val="Normal"/>
    <w:pPr>
      <w:spacing w:before="0" w:after="0" w:line="1.5"/>
      <w:justify w:val="center"/>
    </w:pPr>
    <w:rPr>
      <w:b w:val="true"/>
      <w:sz w:val="32"/>
    </w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Normal">
    <w:name w:val="Normal"/>
    <w:pPr>
      <w:spacing w:before="0" w:after="0" w:line="1.5"/>
    </w:pPr>
    <w:rPr>
      <w:sz w:val="24"/>
    </w:rPr>
  </w:style>
</w:styles>"#;
    zip.start_file("word/styles.xml", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(styles.as_bytes()).unwrap();

    zip.finish().unwrap();
    cursor.into_inner()
}

/// 将文本转换为 docx 段落 XML
fn format_paragraphs(text: &str) -> String {
    let mut paragraphs = Vec::new();
    for line in text.lines() {
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
    }
    paragraphs.join("\n    ")
}

/// 转义 XML 特殊字符
fn sanitize_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
