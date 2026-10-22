//! Regression for issue #97: two REAL failure modes that triggered
//! "文件编码不支持" at baseline 42027cc.
//!
//! The existing `issue_97_nonascii_filename.rs` tests a裸 UTF-8 Chinese
//! filename, which was ALREADY passing at baseline — it is a "three forms
//! agree" guard, not a fault reproduction.
//!
//! The two forms that actually broke at baseline:
//!   (a) Only `filename*=UTF-8''<percent-encoded>` — no `filename=` field.
//!       The multipart parser returns None for file_name → fallback
//!       "unknown.txt" → ext="txt" → PDF binary fails text decode →
//!       "文件编码不支持".
//!   (b) No `filename` / `filename*` at all.
//!       Same fallback path → same encoding error.
//!
//! Fix: `detect_type_by_magic()` in `api_upload_extract` checks content
//! magic bytes (%PDF-) and overrides the filename extension.
//!
//! Falsifiability: reverting `src/routes/upload.rs` to 42027cc (removing
//! `detect_type_by_magic`) makes both tests RED because the response
//! contains "文件编码不支持".
//!
//! IMPORTANT: these tests hand-craft the multipart request body byte-by-byte
//! (Content-Disposition headers written manually) to control exactly which
//! filename parameters are present.  We do NOT use reqwest's multipart
//! builder because it may add its own `filename=` or encode filenames
//! differently, which would mask the real failure mode.

mod common;
use common::{free_port, spawn_test_server};

/// Synthetic PDF with >100 chars of extractable text so the handler returns
/// `file_type` in the response body (the PDF branch only emits `file_type`
/// when text extraction yields ≥100 chars; otherwise it falls through to
/// AI-vision fallback which returns an error without `file_type`).
///
/// The PDF is minimal but valid: one page, one text object with a long string.
/// We omit a valid xref table (startxref 0) — `pdf_extract` recovers by
/// scanning for objects, which is standard behaviour for PDF parsers.
fn synthetic_pdf() -> Vec<u8> {
    let text = "Synthetic test document for issue 97 filename encoding regression test. \
This content is intentionally longer than one hundred characters to ensure \
the upload extract handler returns file_type in the JSON response body.";
    assert!(
        text.len() >= 100,
        "test text must be >=100 chars for file_type to appear, got {}",
        text.len()
    );
    let content = format!("BT /F1 12 Tf 100 700 Td ({}) Tj ET", text);
    let pdf = format!(
        "%PDF-1.4\n\
1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n\
4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n\
5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n\
trailer\n<< /Root 1 0 R /Size 6 >>\nstartxref\n0\n%%EOF\n",
        content.len(),
        content
    );
    pdf.into_bytes()
}

/// Send a raw multipart/form-data request with a hand-crafted
/// Content-Disposition header.  This gives us full control over which
/// filename parameters are present (filename, filename*, or neither).
fn raw_multipart_upload(
    client: &reqwest::blocking::Client,
    port: u16,
    content_disposition: &str,
    file_bytes: &[u8],
) -> serde_json::Value {
    let boundary = "----issue97formboundary";
    let mut body = Vec::new();
    // Part header
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(format!("Content-Disposition: {}\r\n", content_disposition).as_bytes());
    body.extend_from_slice(b"\r\n");
    // File content
    body.extend_from_slice(file_bytes);
    // Closing boundary
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());

    let resp = client
        .post(format!("http://127.0.0.1:{}/api/upload/extract", port))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(body)
        .send()
        .expect("upload must respond");
    assert!(
        resp.status().is_success(),
        "#97: upload HTTP status must be 2xx, got {}",
        resp.status()
    );
    resp.json().expect("upload response must be JSON")
}

/// (a) Only `filename*=UTF-8''<percent-encoded>`, NO `filename=` field.
///
/// At baseline 42027cc the multipart parser returns None for file_name
/// (it does not decode RFC 5987 `filename*`), so the handler falls back to
/// "unknown.txt" and tries to decode the PDF binary as text, producing
/// "文件编码不支持".  With the fix, `detect_type_by_magic` identifies the
/// content as PDF by magic bytes and enters the PDF branch.
#[test]
fn issue_97_filename_star_only_not_rejected_as_encoding() {
    let port = free_port();
    let server = spawn_test_server(port, &[]);
    let client = reqwest::blocking::Client::new();
    let pdf = synthetic_pdf();

    // "测试文档.pdf" percent-encoded as UTF-8:
    //   测=E6B58B 试=E8AF95 文=E69687 档=E6A1A3 .pdf=2E706466
    let disposition =
        r#"form-data; name="file"; filename*=UTF-8''%E6%B5%8B%E8%AF%95%E6%96%87%E6%A1%A3.pdf"#;
    let resp = raw_multipart_upload(&client, port, disposition, &pdf);

    let err = resp["error"].as_str().unwrap_or("");
    assert!(
        !err.contains("文件编码不支持"),
        "#97(a): filename* only must NOT get encoding error, got: {err}"
    );
    assert!(
        !err.contains("无法识别文件类型"),
        "#97(a): filename* only must NOT get unrecognised type error, got: {err}"
    );

    // Must enter the PDF branch: either file_type=="pdf" in the success
    // response, or a PDF-specific error (not an encoding/type error).
    if let Some(ft) = resp["file_type"].as_str() {
        assert_eq!(
            ft, "pdf",
            "#97(a): file_type must be 'pdf' (magic byte detection), got: {ft}"
        );
    } else if !err.is_empty() {
        // Extraction may fail in a test env without AI, but the error must
        // be PDF-specific (contains "PDF" or "pdf"), not an encoding error.
        assert!(
            err.to_lowercase().contains("pdf"),
            "#97(a): error must be PDF-specific, got: {err}"
        );
    }

    drop(server);
}

/// (b) No `filename` and no `filename*` at all.
///
/// At baseline 42027cc the multipart parser returns None for file_name,
/// so the handler falls back to "unknown.txt" → same encoding error.
/// With the fix, magic bytes detect the content as PDF.
#[test]
fn issue_97_no_filename_at_all_not_rejected_as_encoding() {
    let port = free_port();
    let server = spawn_test_server(port, &[]);
    let client = reqwest::blocking::Client::new();
    let pdf = synthetic_pdf();

    // Content-Disposition with only name="file", no filename parameter.
    let disposition = r#"form-data; name="file""#;
    let resp = raw_multipart_upload(&client, port, disposition, &pdf);

    let err = resp["error"].as_str().unwrap_or("");
    assert!(
        !err.contains("文件编码不支持"),
        "#97(b): no filename must NOT get encoding error, got: {err}"
    );
    assert!(
        !err.contains("无法识别文件类型"),
        "#97(b): no filename must NOT get unrecognised type error, got: {err}"
    );

    if let Some(ft) = resp["file_type"].as_str() {
        assert_eq!(
            ft, "pdf",
            "#97(b): file_type must be 'pdf' (magic byte detection), got: {ft}"
        );
    } else if !err.is_empty() {
        assert!(
            err.to_lowercase().contains("pdf"),
            "#97(b): error must be PDF-specific, got: {err}"
        );
    }

    drop(server);
}
