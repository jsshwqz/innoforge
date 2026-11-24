//! OA 分析结果服务端导出通路 / Server-side export for OA analysis results
//!
//! A1 (R10): 解决内嵌 webview 丢失 `<a download>` 文件名的问题。
//! POST /api/oa/export  — 接收文本/二进制内容，落盘到 data/oa-exports/
//! GET  /api/oa/export/:filename — 带正确 Content-Disposition 的下载

use super::AppState;
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
};
use base64::Engine;
use serde_json::json;
use std::{
    fs,
    path::Path as StdPath,
    time::{SystemTime, UNIX_EPOCH},
};

/// 扩展名白名单
const ALLOWED_EXTS: &[&str] = &["md", "txt", "csv", "json", "html", "pdf", "docx"];

/// 文本上限：400,000 字
const TEXT_MAX_CHARS: usize = 400_000;

/// 二进制上限：20 MB
const BINARY_MAX_BYTES: usize = 20 * 1024 * 1024;

/// 导出目录
const EXPORT_DIR: &str = "data/oa-exports";

/// 扩展名 → Content-Type
fn content_type_for_ext(ext: &str) -> &'static str {
    match ext {
        "md" => "text/markdown; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        _ => "application/octet-stream",
    }
}

/// 判断扩展名是否为文本类
fn is_text_ext(ext: &str) -> bool {
    matches!(ext, "md" | "txt" | "csv" | "json" | "html")
}

/// 剥掉路径分隔符和控制字符，保留 CJK
fn sanitize_filename_component(raw: &str) -> String {
    raw.chars()
        .filter(|c| {
            // 保留：字母、数字、CJK、常见标点
            // 剥掉：路径分隔符 / \ 、控制字符、null
            !matches!(c, '/' | '\\' | '\0' | '\n' | '\r' | '\t') && !c.is_control()
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// 生成时间戳后缀 YYYYmmdd-HHMMSS
fn timestamp_suffix() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // 使用 chrono 如果可用，否则手动计算
    let secs = now;
    let days = secs / 86400;
    let remainder = secs % 86400;
    let hour = remainder / 3600;
    let minute = (remainder % 3600) / 60;
    let second = remainder % 60;

    // 从 1970-01-01 计算年月日
    let (year, month, day) = days_to_ymd(days as i64);
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        year, month, day, hour, minute, second
    )
}

/// 将 Unix epoch 天数转换为 (年, 月, 日)
fn days_to_ymd(days: i64) -> (i64, i64, i64) {
    // 算法来自 Howard Hinnant 的 date 算法
    let z = days + 719468; // days since 0000-03-01
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

/// ASCII 回退文件名：非 ASCII 字符替换为 _
fn ascii_fallback(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii() && !c.is_control() {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// 百分号编码（RFC 3986），用于 filename*=UTF-8'' 编码
fn percent_encode_utf8(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            result.push(byte as char);
        } else {
            result.push_str(&format!("%{:02X}", byte));
        }
    }
    result
}

/// 剥掉 Windows \\?\ 长路径前缀（Linux 上 no-op，但保持跨平台一致）
fn display_path(p: &StdPath) -> String {
    let s = p.to_string_lossy().to_string();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        if let Some(unc) = stripped.strip_prefix(r"UNC\") {
            format!(r"\\{}", unc)
        } else {
            stripped.to_string()
        }
    } else {
        s
    }
}

/// 确保导出目录存在
fn ensure_export_dir() -> Result<std::path::PathBuf, String> {
    let dir = StdPath::new(EXPORT_DIR);
    if !dir.exists() {
        fs::create_dir_all(dir).map_err(|e| format!("创建导出目录失败: {}", e))?;
    }
    let canonical = dir
        .canonicalize()
        .map_err(|e| format!("解析导出目录路径失败: {}", e))?;
    Ok(canonical)
}

/// POST /api/oa/export 核心逻辑（纯函数，便于单测）
///
/// 请求体: { filename: string, content: string, ext: string, binary?: string(base64) }
/// 返回: (StatusCode, JSON body)
fn oa_export_post_logic(req: &serde_json::Value) -> (StatusCode, serde_json::Value) {
    let raw_filename = req["filename"].as_str().unwrap_or("OA分析报告");
    let ext = req["ext"].as_str().unwrap_or("md").to_lowercase();

    // 扩展名白名单检查
    if !ALLOWED_EXTS.contains(&ext.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            json!({
                "status": "error",
                "error": format!("不允许的扩展名: {}，仅支持: {}", ext, ALLOWED_EXTS.join(", "))
            }),
        );
    }

    // 获取内容：优先 text 字段，其次 binary(base64)
    let (content_bytes, char_count) = if let Some(binary_b64) = req["binary"].as_str() {
        if binary_b64.is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                json!({ "status": "error", "error": "内容为空" }),
            );
        }
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(binary_b64)
            .map_err(|e| format!("base64 解码失败: {}", e));
        match decoded {
            Ok(bytes) => {
                if bytes.len() > BINARY_MAX_BYTES {
                    return (
                        StatusCode::BAD_REQUEST,
                        json!({
                            "status": "error",
                            "error": format!("文件过大: {} 字节，上限 {} 字节", bytes.len(), BINARY_MAX_BYTES)
                        }),
                    );
                }
                let chars = String::from_utf8_lossy(&bytes).chars().count();
                (bytes, chars)
            }
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    json!({ "status": "error", "error": e }),
                )
            }
        }
    } else {
        let text = req["content"].as_str().unwrap_or("");
        if text.is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                json!({ "status": "error", "error": "内容为空" }),
            );
        }
        let chars = text.chars().count();
        if is_text_ext(&ext) && chars > TEXT_MAX_CHARS {
            return (
                StatusCode::BAD_REQUEST,
                json!({
                    "status": "error",
                    "error": format!("文本过长: {} 字，上限 {} 字", chars, TEXT_MAX_CHARS)
                }),
            );
        }
        if text.len() > BINARY_MAX_BYTES {
            return (
                StatusCode::BAD_REQUEST,
                json!({
                    "status": "error",
                    "error": format!("内容过大: {} 字节，上限 {} 字节", text.len(), BINARY_MAX_BYTES)
                }),
            );
        }
        (text.as_bytes().to_vec(), chars)
    };

    // 清洗文件名主干
    let stem = sanitize_filename_component(raw_filename);
    let stem = if stem.is_empty() {
        "OA分析报告".to_string()
    } else {
        stem
    };

    // 组装最终文件名: <主干>_<时间戳>.<ext>
    let ts = timestamp_suffix();
    let final_filename = format!("{}_{}.{}", stem, ts, ext);

    // 路径穿越检查：文件名中不得包含 .. / \
    if final_filename.contains("..")
        || final_filename.contains('/')
        || final_filename.contains('\\')
    {
        return (
            StatusCode::BAD_REQUEST,
            json!({ "status": "error", "error": "文件名包含非法字符" }),
        );
    }

    // 确保导出目录存在
    let export_dir = match ensure_export_dir() {
        Ok(d) => d,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "status": "error", "error": e }),
            )
        }
    };

    // 最终路径
    let file_path = export_dir.join(&final_filename);

    // canonicalize 后必须仍在导出目录内
    let parent_dir = file_path.parent().unwrap_or(&export_dir);
    if let Ok(canonical_parent) = parent_dir.canonicalize() {
        if !canonical_parent.starts_with(&export_dir) {
            return (
                StatusCode::BAD_REQUEST,
                json!({ "status": "error", "error": "路径穿越检测失败" }),
            );
        }
    }

    // 落盘
    if let Err(e) = fs::write(&file_path, &content_bytes) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({ "status": "error", "error": format!("写入文件失败: {}", e) }),
        );
    }

    let abs_path = display_path(&file_path);
    let byte_count = content_bytes.len();
    let url = format!("/api/oa/export/{}", final_filename);

    (
        StatusCode::OK,
        json!({
            "status": "ok",
            "filename": final_filename,
            "url": url,
            "path": abs_path,
            "bytes": byte_count,
            "chars": char_count
        }),
    )
}

/// POST /api/oa/export
///
/// 请求体: { filename: string, content: string, ext: string, binary?: string(base64) }
/// 响应: { status, filename, url, path, bytes, chars }
pub async fn api_oa_export_post(
    State(_s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Response {
    let (status, body) = oa_export_post_logic(&req);
    (status, Json(body)).into_response()
}

/// GET /api/oa/export/:filename
///
/// 返回文件内容，带正确 Content-Disposition
pub async fn api_oa_export_get(
    State(_s): State<AppState>,
    Path(filename): Path<String>,
) -> Response {
    // 路径穿越检查
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return (StatusCode::BAD_REQUEST, "非法文件名 / Invalid filename").into_response();
    }

    // 提取扩展名
    let ext = StdPath::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if !ALLOWED_EXTS.contains(&ext.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            "不允许的扩展名 / Extension not allowed",
        )
            .into_response();
    }

    let export_dir = match ensure_export_dir() {
        Ok(d) => d,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "导出目录不可用 / Export directory unavailable",
            )
                .into_response();
        }
    };

    let file_path = export_dir.join(&filename);

    // canonicalize 检查
    match file_path.canonicalize() {
        Ok(canonical) => {
            if !canonical.starts_with(&export_dir) {
                return (
                    StatusCode::BAD_REQUEST,
                    "路径穿越 / Path traversal detected",
                )
                    .into_response();
            }
        }
        Err(_) => {
            return (StatusCode::NOT_FOUND, "文件不存在 / File not found").into_response();
        }
    }

    // 读取文件
    let file_bytes = match fs::read(&file_path) {
        Ok(b) => b,
        Err(_) => {
            return (StatusCode::NOT_FOUND, "文件不存在 / File not found").into_response();
        }
    };

    // 构建 Content-Disposition
    let ascii_name = ascii_fallback(&filename);
    let encoded_name = percent_encode_utf8(&filename);
    let disposition = format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        ascii_name, encoded_name
    );

    let ct = content_type_for_ext(&ext);

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(ct).unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition)
            .unwrap_or_else(|_| HeaderValue::from_str("attachment").unwrap()),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&file_bytes.len().to_string()).unwrap(),
    );

    (StatusCode::OK, headers, file_bytes).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        // CJK 保留
        assert_eq!(sanitize_filename_component("OA分析报告"), "OA分析报告");
        // 路径分隔符剥掉
        assert_eq!(sanitize_filename_component("a/b\\c"), "abc");
        // 控制字符剥掉
        assert_eq!(sanitize_filename_component("a\nb\tc"), "abc");
        // 空字符串
        assert_eq!(sanitize_filename_component(""), "");
    }

    #[test]
    fn test_content_type() {
        assert_eq!(content_type_for_ext("md"), "text/markdown; charset=utf-8");
        assert_eq!(content_type_for_ext("pdf"), "application/pdf");
        assert_eq!(
            content_type_for_ext("docx"),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
        assert_eq!(content_type_for_ext("unknown"), "application/octet-stream");
    }

    #[test]
    fn test_ascii_fallback() {
        assert_eq!(ascii_fallback("test.md"), "test.md");
        assert_eq!(ascii_fallback("OA分析报告.md"), "OA____.md");
    }

    #[test]
    fn test_percent_encode() {
        assert_eq!(percent_encode_utf8("test.md"), "test.md");
        // 中文字符应该被编码
        let encoded = percent_encode_utf8("分析");
        assert!(encoded.starts_with("%"));
        assert!(!encoded.contains('分'));
    }

    #[test]
    fn test_display_path_linux() {
        // Linux 上不应有 \\?\ 前缀
        assert_eq!(display_path(StdPath::new("/tmp/test.md")), "/tmp/test.md");
    }

    #[test]
    fn test_timestamp_format() {
        let ts = timestamp_suffix();
        // 格式应为 YYYYmmdd-HHMMSS
        assert_eq!(ts.len(), 15);
        assert_eq!(ts.as_bytes()[8], b'-');
    }

    #[test]
    fn test_days_to_ymd() {
        // 1970-01-01 = epoch
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
        // 2026-01-01 = 20454 days from epoch
        assert_eq!(days_to_ymd(20454), (2026, 1, 1));
    }

    #[test]
    fn test_ext_whitelist() {
        assert!(ALLOWED_EXTS.contains(&"md"));
        assert!(ALLOWED_EXTS.contains(&"docx"));
        assert!(!ALLOWED_EXTS.contains(&"exe"));
        assert!(!ALLOWED_EXTS.contains(&"sh"));
    }

    #[test]
    fn test_illegal_filename_returns_400() {
        let req = json!({"filename": "../../tmp/qoder_probe2", "content": "test", "ext": "md"});
        let (status, body) = oa_export_post_logic(&req);
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["status"], "error");
        let err = body["error"].as_str().unwrap_or("");
        assert!(
            err.contains("文件名包含非法字符"),
            "error should contain 文件名包含非法字符, got: {}",
            err
        );
    }

    #[test]
    fn oa_docx_export_contains_strike_and_underline_xml() {
        let bytes = super::build_oa_docx_bytes(
            "CN119900001A",
            "1. 一种普通电磁阀，包括线圈。",
            "1. 一种双波纹管隔离电磁阀，包括线圈。",
            "[0024]",
            "一、修改说明：依据说明书第[0024]段进行修改。\n二、创造性争辩：本申请具有显著进步。\n三、结论：请求授予专利权。",
        )
        .expect("build oa docx");
        assert_eq!(&bytes[0..2], b"PK", "docx must be a valid ZIP archive");

        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("open docx zip");
        let mut doc_file = archive.by_name("word/document.xml").expect("document.xml");
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut doc_file, &mut xml).expect("read document.xml");
        assert!(xml.contains("<w:strike/>"), "must contain <w:strike/>");
        assert!(xml.contains("<w:u "), "must contain <w:u underline tag");
        assert!(xml.contains("[0024]"), "must preserve A33 anchor");
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 基于字符级 LCS 对比修改前后文本，生成带 OOXML 修订标记的 `<w:r>` 片段列表。
/// 返回 `Vec<(is_del, is_add, segment_text)>`。
fn diff_claim_segments(old_text: &str, new_text: &str) -> Vec<(bool, bool, String)> {
    let a: Vec<char> = old_text.chars().collect();
    let b: Vec<char> = new_text.chars().collect();
    if a.len() * b.len() > 2_500_000 {
        return vec![
            (true, false, old_text.to_string()),
            (false, true, new_text.to_string()),
        ];
    }
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0u32; n + 1]; m + 1];
    for i in (0..m).rev() {
        for j in (0..n).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut ops: Vec<(bool, bool, char)> = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < m && j < n {
        if a[i] == b[j] {
            ops.push((false, false, a[i]));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            ops.push((true, false, a[i]));
            i += 1;
        } else {
            ops.push((false, true, b[j]));
            j += 1;
        }
    }
    while i < m {
        ops.push((true, false, a[i]));
        i += 1;
    }
    while j < n {
        ops.push((false, true, b[j]));
        j += 1;
    }

    let mut merged: Vec<(bool, bool, String)> = Vec::new();
    for (del, add, ch) in ops {
        if let Some(last) = merged.last_mut() {
            if last.0 == del && last.1 == add {
                last.2.push(ch);
                continue;
            }
        }
        merged.push((del, add, ch.to_string()));
    }
    merged
}

/// 生成带《权利要求书修改对照页》（红删线+蓝下划线+A33锚点）与《意见陈述书》的标准 `.docx` 字节流。
pub fn build_oa_docx_bytes(
    case_label: &str,
    old_claims: &str,
    new_claims: &str,
    anchor_note: &str,
    statement_body: &str,
) -> Result<Vec<u8>, String> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let mut runs_xml = String::new();
    if old_claims.trim() != new_claims.trim() && !old_claims.trim().is_empty() {
        for (is_del, is_add, seg) in diff_claim_segments(old_claims, new_claims) {
            let esc = escape_xml(&seg);
            if is_del {
                runs_xml.push_str(&format!(
                    "<w:r><w:rPr><w:color w:val=\"C00000\"/><w:strike/></w:rPr><w:t xml:space=\"preserve\">{esc}</w:t></w:r>"
                ));
            } else if is_add {
                runs_xml.push_str(&format!(
                    "<w:r><w:rPr><w:color w:val=\"0052CC\"/><w:u w:val=\"single\"/></w:rPr><w:t xml:space=\"preserve\">{esc}</w:t></w:r>"
                ));
            } else {
                runs_xml.push_str(&format!(
                    "<w:r><w:t xml:space=\"preserve\">{esc}</w:t></w:r>"
                ));
            }
        }
    } else {
        let esc = escape_xml(if new_claims.trim().is_empty() {
            "（本次答复未修改权利要求书）"
        } else {
            new_claims
        });
        runs_xml.push_str(&format!(
            "<w:r><w:t xml:space=\"preserve\">{esc}</w:t></w:r>"
        ));
    }

    let safe_anchor = if anchor_note.trim().is_empty() {
        "【A33提示：尚未绑定说明书段落锚点，提交前请核对依据】".to_string()
    } else {
        format!("修改依据（专利法第33条锚点）：{}", anchor_note.trim())
    };

    let mut body_paras = String::new();
    body_paras.push_str(&format!(
        "<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>一、权利要求书修改对照页（{}）</w:t></w:r></w:p>",
        escape_xml(case_label)
    ));
    body_paras.push_str(&format!("<w:p>{runs_xml}</w:p>"));
    body_paras.push_str(&format!(
        "<w:p><w:r><w:rPr><w:i/></w:rPr><w:t>{}</w:t></w:r></w:p>",
        escape_xml(&safe_anchor)
    ));
    body_paras.push_str("<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>二、意见陈述书</w:t></w:r></w:p>");
    let stmt_text = if statement_body.trim().is_empty() {
        "一、修改说明（专利法第33条）\n二、创造性/新颖性争辩（三步法+证据锚点）\n三、结论：恳请审查员继续审查并授予专利权。"
    } else {
        statement_body
    };
    for line in stmt_text.lines() {
        body_paras.push_str(&format!(
            "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
            escape_xml(line)
        ));
    }

    let doc_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
         <w:body>{body_paras}<w:sectPr/></w:body></w:document>"
    );

    let content_types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
        <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
        <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
        <Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
        </Types>";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
        <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
        </Relationships>";
    let doc_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"/>";

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default();
        zip.start_file("[Content_Types].xml", opts)
            .map_err(|e| e.to_string())?;
        zip.write_all(content_types.as_bytes())
            .map_err(|e| e.to_string())?;
        zip.start_file("_rels/.rels", opts)
            .map_err(|e| e.to_string())?;
        zip.write_all(rels.as_bytes()).map_err(|e| e.to_string())?;
        zip.start_file("word/_rels/document.xml.rels", opts)
            .map_err(|e| e.to_string())?;
        zip.write_all(doc_rels.as_bytes())
            .map_err(|e| e.to_string())?;
        zip.start_file("word/document.xml", opts)
            .map_err(|e| e.to_string())?;
        zip.write_all(doc_xml.as_bytes())
            .map_err(|e| e.to_string())?;
        zip.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

#[derive(serde::Deserialize, Default)]
pub struct OaDocxQuery {
    #[serde(default)]
    pub old_claims: String,
    #[serde(default)]
    pub new_claims: String,
    #[serde(default)]
    pub anchor_note: String,
    #[serde(default)]
    pub statement: String,
}

/// POST /api/export/docx — 通用 Markdown 专业报告导出 Word (`.docx`)
pub async fn api_export_markdown_docx(
    Json(req): Json<serde_json::Value>,
) -> axum::response::Response {
    use axum::http::header;
    use axum::response::IntoResponse;

    let title = req
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("创研台技术分析报告")
        .trim()
        .to_string();
    let subtitle = req
        .get("subtitle")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let markdown = req
        .get("markdown")
        .or_else(|| req.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if markdown.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "导出内容为空" })),
        )
            .into_response();
    }

    let raw_name = req
        .get("filename")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(if title.is_empty() {
            "innoforge_report"
        } else {
            title.as_str()
        });
    let safe_stem: String = raw_name
        .trim_end_matches(".docx")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' || ('\u{4E00}'..='\u{9FFF}').contains(&c)
            {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    let final_stem = if safe_stem.trim_matches('_').is_empty() {
        "innoforge_report".to_string()
    } else {
        safe_stem.trim_matches('_').to_string()
    };
    let final_filename = format!("{final_stem}.docx");
    let encoded_name = urlencoding::encode(&final_filename);

    let default_subtitle = if subtitle.is_empty() {
        format!(
            "由创研台 InnoForge 生成 · {}",
            chrono::Utc::now().format("%Y-%m-%d %H:%M UTC")
        )
    } else {
        subtitle
    };

    let params = crate::docx_export::MarkdownDocxParams {
        title,
        subtitle: default_subtitle,
        markdown,
    };
    match crate::docx_export::generate_markdown_docx(&params) {
        Ok(bytes) => {
            let disposition = format!(
                "attachment; filename=\"innoforge_report.docx\"; filename*=UTF-8''{}",
                encoded_name
            );
            (
                StatusCode::OK,
                [
                    (
                        header::CONTENT_TYPE,
                        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                            .to_string(),
                    ),
                    (header::CONTENT_DISPOSITION, disposition),
                ],
                bytes,
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("生成 Word 文档失败: {e}") })),
        )
            .into_response(),
    }
}

/// GET /api/oa/:case_id/export/docx — 导出带红删线/蓝下划线修改对照页与意见陈述书的标准 `.docx`
pub async fn api_oa_case_export_docx(
    State(s): State<AppState>,
    Path(case_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<OaDocxQuery>,
) -> Response {
    let old_claims = q.old_claims;
    let new_claims = q.new_claims;
    let mut anchor_note = q.anchor_note;
    let mut statement = q.statement;

    if statement.trim().is_empty() {
        if let Ok(numeric_id) = case_id.parse::<i64>() {
            if let Ok(Some(record)) = s.db.get_oa_analysis(numeric_id) {
                statement = record.analysis_text;
            }
        }
    }
    if anchor_note.trim().is_empty() && !case_id.trim().is_empty() {
        anchor_note = format!("案件号: {}", case_id.trim());
    }

    match build_oa_docx_bytes(&case_id, &old_claims, &new_claims, &anchor_note, &statement) {
        Ok(bytes) => {
            let safe_id: String = case_id
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            let filename = format!(
                "oa-response-{}.docx",
                if safe_id.is_empty() { "case" } else { &safe_id }
            );
            (
                StatusCode::OK,
                [
                    (
                        header::CONTENT_TYPE,
                        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                            .to_string(),
                    ),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{filename}\""),
                    ),
                ],
                bytes,
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("DOCX build error: {e}"),
        )
            .into_response(),
    }
}
