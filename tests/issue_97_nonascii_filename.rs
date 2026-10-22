//! Regression for issue #97: non-ASCII filename PDF upload rejected as
//! "文件编码不支持" (encoding unsupported) instead of being identified by
//! content magic bytes.
//!
//! Root cause: `api_upload_extract` derived file type solely from the filename
//! extension.  When the multipart parser returns None or a mangled file_name
//! for non-ASCII names, the fallback `"unknown.txt"` pushes the request into
//! the text-decode branch, which fails on binary PDF content and reports
//! "文件编码不支持" — a misleading error.
//!
//! Fix: detect file type by content magic bytes (%PDF-, PK\x03\x04) when the
//! filename extension is missing or unreliable.
//!
//! Negative control: if the magic-byte detection is removed (reverting to
//! filename-only), the non-ASCII and .txt-renamed-PDF cases will get
//! "文件编码不支持" again, and these assertions will fail.

mod common;
use common::{free_port, spawn_test_server};

/// Minimal one-page PDF with extractable text.
/// Built by hand — no external tools needed.
fn minimal_pdf() -> Vec<u8> {
    // A minimal valid PDF with text "Test content for issue 97"
    let pdf = b"%PDF-1.4\n\
1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n\
4 0 obj\n<< /Length 52 >>\nstream\nBT /F1 12 Tf 100 700 Td (Test content for issue 97) Tj ET\nendstream\nendobj\n\
5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n\
xref\n0 6\n\
0000000000 65535 f \n\
0000000009 00000 n \n\
0000000058 00000 n \n\
0000000115 00000 n \n\
0000000266 00000 n \n\
0000000378 00000 n \n\
trailer\n<< /Root 1 0 R /Size 6 >>\nstartxref\n448\n%%EOF\n";
    pdf.to_vec()
}

/// Random binary content that is neither valid UTF-8, valid GBK, nor has any
/// known magic bytes.
fn random_binary() -> Vec<u8> {
    // Deliberately not starting with %PDF- or PK\x03\x04.
    // Must be invalid UTF-8 AND invalid GBK so both decoders report errors.
    // GBK: lead byte 0x81-0xFE must be followed by trail byte 0x40-0x7E or
    // 0x80-0xFE.  We pair each lead byte with 0x3F (below 0x40) to break GBK.
    vec![
        0x81, 0x3F, 0x82, 0x3F, 0x83, 0x3F, 0x84, 0x3F, 0x85, 0x3F, 0x86, 0x3F, 0x87, 0x3F, 0x88,
        0x3F, 0x89, 0x3F, 0x8A, 0x3F, 0x8B, 0x3F, 0x8C, 0x3F, 0x8D, 0x3F, 0x8E, 0x3F, 0x8F, 0x3F,
        0x90, 0x3F, 0x91, 0x3F, 0x92, 0x3F, 0x93, 0x3F, 0x94, 0x3F,
    ]
}

/// Upload a file with a given filename to /api/upload/extract.
fn upload_extract(
    client: &reqwest::blocking::Client,
    port: u16,
    filename: &str,
    content: &[u8],
) -> serde_json::Value {
    let form = reqwest::blocking::multipart::Form::new().part(
        "file",
        reqwest::blocking::multipart::Part::bytes(content.to_vec()).file_name(filename.to_string()),
    );
    let resp = client
        .post(format!("http://127.0.0.1:{}/api/upload/extract", port))
        .multipart(form)
        .send()
        .expect("upload must respond");
    assert!(resp.status().is_success(), "upload HTTP status");
    resp.json().expect("upload response must be JSON")
}

/// #97 positive: PDF with non-ASCII filename must NOT be rejected with
/// "文件编码不支持" or "无法识别文件类型".  It should enter the PDF branch
/// (either succeed or fail with a PDF-specific error).
#[test]
fn issue_97_nonascii_filename_pdf_not_rejected_as_encoding() {
    let port = free_port();
    let server = spawn_test_server(port, &[]);
    let client = reqwest::blocking::Client::new();
    let pdf = minimal_pdf();

    // Non-ASCII (Chinese) filename
    let resp_cn = upload_extract(&client, port, "测试文档.pdf", &pdf);
    let err_cn = resp_cn["error"].as_str().unwrap_or("");
    assert!(
        !err_cn.contains("文件编码不支持"),
        "#97: Chinese filename must not get encoding error, got: {err_cn}"
    );
    assert!(
        !err_cn.contains("无法识别文件类型"),
        "#97: Chinese filename must not get unrecognised type error, got: {err_cn}"
    );

    // Filename with spaces
    let resp_space = upload_extract(&client, port, "test file.pdf", &pdf);
    let err_space = resp_space["error"].as_str().unwrap_or("");
    assert!(
        !err_space.contains("文件编码不支持"),
        "#97: space filename must not get encoding error, got: {err_space}"
    );

    // ASCII filename
    let resp_ascii = upload_extract(&client, port, "document.pdf", &pdf);
    let err_ascii = resp_ascii["error"].as_str().unwrap_or("");
    assert!(
        !err_ascii.contains("文件编码不支持"),
        "#97: ASCII filename must not get encoding error, got: {err_ascii}"
    );

    // All three should have the same outcome: either all have text or all
    // have the same error (PDF-specific, not encoding).
    let has_text_cn = resp_cn["text"].as_str().is_some();
    let has_text_space = resp_space["text"].as_str().is_some();
    let has_text_ascii = resp_ascii["text"].as_str().is_some();
    assert_eq!(
        has_text_cn, has_text_ascii,
        "#97: Chinese and ASCII filename must have same extraction outcome"
    );
    assert_eq!(
        has_text_space, has_text_ascii,
        "#97: space and ASCII filename must have same extraction outcome"
    );

    // If text was extracted, lengths must match
    if has_text_cn {
        let len_cn = resp_cn["char_length"].as_u64().unwrap_or(0);
        let len_ascii = resp_ascii["char_length"].as_u64().unwrap_or(0);
        assert_eq!(
            len_cn, len_ascii,
            "#97: Chinese and ASCII filename must extract same char count"
        );
    }

    drop(server);
}

/// #97 negative control A: random binary content uploaded as .txt must return
/// "无法识别文件类型" (unrecognised type), NOT "文件编码不支持" (encoding
/// unsupported).  The error must match the real cause.
#[test]
fn issue_97_random_binary_as_txt_must_say_unrecognised_type() {
    let port = free_port();
    let server = spawn_test_server(port, &[]);
    let client = reqwest::blocking::Client::new();
    let bin = random_binary();

    let resp = upload_extract(&client, port, "data.txt", &bin);
    let err = resp["error"].as_str().unwrap_or("");

    assert!(
        !err.is_empty(),
        "#97 neg-A: must return an error for random binary, got: {resp}"
    );
    assert!(
        !err.contains("文件编码不支持"),
        "#97 neg-A: must NOT say 'encoding unsupported' for unrecognised binary, got: {err}"
    );
    assert!(
        err.contains("无法识别文件类型"),
        "#97 neg-A: must say 'unrecognised file type' for random binary, got: {err}"
    );

    drop(server);
}

/// #97 negative control B: a file with %PDF- magic bytes renamed to .txt must
/// NOT be silently treated as text and rejected with "文件编码不支持".
/// It must either be identified as PDF (by magic bytes) and processed, or
/// explicitly rejected — but not with an encoding error.
#[test]
fn issue_97_pdf_magic_renamed_txt_must_not_say_encoding_error() {
    let port = free_port();
    let server = spawn_test_server(port, &[]);
    let client = reqwest::blocking::Client::new();
    let pdf = minimal_pdf();

    // Upload a PDF with .txt extension — magic bytes should detect it as PDF
    let resp = upload_extract(&client, port, "not_a_pdf.txt", &pdf);
    let err = resp["error"].as_str().unwrap_or("");

    assert!(
        !err.contains("文件编码不支持"),
        "#97 neg-B: PDF magic renamed .txt must NOT get encoding error, got: {err}"
    );
    assert!(
        !err.contains("无法识别文件类型"),
        "#97 neg-B: PDF magic renamed .txt must NOT get unrecognised type error, got: {err}"
    );

    // The file_type in response should be "pdf" (detected by magic bytes),
    // not "txt" (from the filename).
    if let Some(ft) = resp["file_type"].as_str() {
        assert_eq!(
            ft, "pdf",
            "#97 neg-B: file_type must be 'pdf' (magic byte detection), got: {ft}"
        );
    }

    drop(server);
}
