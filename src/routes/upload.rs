use super::AppState;
use axum::{extract::State, Json};
use serde_json::json;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::PathBuf,
    time::Duration,
};

const MAX_FILE_SIZE: usize = 10 * 1024 * 1024; // 10 MB
const MAX_PDF_STORE_SIZE: usize = 20 * 1024 * 1024; // 20 MB

/// Umi-OCR 默认 HTTP 地址（端口可在 Umi-OCR 全局设置中修改）
const UMI_OCR_BASE_URL: &str = "http://127.0.0.1:1224";

/// Files created for external PDF tools. They stay under the application
/// working directory so the Windows launcher keeps them on the project drive.
/// The guard owns both input files and subprocess output paths, including
/// paths created immediately before a subprocess fails.
#[derive(Debug, Default)]
struct RuntimeTempFiles {
    paths: Vec<PathBuf>,
}

impl RuntimeTempFiles {
    fn new() -> Self {
        Self::default()
    }

    fn create_file(&mut self, extension: &str) -> Result<(PathBuf, File), String> {
        let extension = extension.trim_start_matches('.');
        if extension.is_empty() {
            return Err("temporary file extension must not be empty".to_string());
        }

        let directory = runtime_temp_dir()?;
        for _ in 0..16 {
            let path = directory.join(format!("innoforge-{}.{}", uuid::Uuid::new_v4(), extension));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    self.paths.push(path.clone());
                    return Ok((path, file));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!("failed to create runtime temporary file: {error}"));
                }
            }
        }

        Err("failed to allocate a unique runtime temporary file".to_string())
    }

    fn write_file(&mut self, extension: &str, data: &[u8]) -> Result<PathBuf, String> {
        let (path, mut file) = self.create_file(extension)?;
        file.write_all(data)
            .map_err(|error| format!("failed to write runtime temporary file: {error}"))?;
        file.flush()
            .map_err(|error| format!("failed to flush runtime temporary file: {error}"))?;
        Ok(path)
    }

    fn track_output(&mut self, path: PathBuf) {
        self.paths.push(path);
    }
}

impl Drop for RuntimeTempFiles {
    fn drop(&mut self) {
        for path in self.paths.iter().rev() {
            if let Err(error) = fs::remove_file(path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(path = %path.display(), %error, "failed to remove runtime temporary file");
                }
            }
        }
    }
}

fn runtime_temp_dir() -> Result<PathBuf, String> {
    let directory = std::env::current_dir()
        .map_err(|error| format!("failed to resolve application working directory: {error}"))?
        .join("data")
        .join("runtime-temp");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("failed to create runtime temporary directory: {error}"))?;
    Ok(directory)
}
/// Umi-OCR HTTP 请求超时（秒）
const UMI_OCR_TIMEOUT_SECS: u64 = 120;

/// POST /api/upload/pdf-store — 上传 PDF 文件并存储，返回可预览的 URL
pub async fn api_upload_pdf_store(
    _state: State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Json<serde_json::Value> {
    let mut file_bytes: Vec<u8> = Vec::new();
    let mut file_name = String::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            file_name = field.file_name().unwrap_or("unknown.pdf").to_lowercase();
            match field.bytes().await {
                Ok(data) => {
                    if data.len() > MAX_PDF_STORE_SIZE {
                        return Json(
                            json!({"status": "error", "message": "文件大小超过 20MB 限制"}),
                        );
                    }
                    file_bytes = data.to_vec();
                }
                Err(_) => return Json(json!({"status": "error", "message": "文件读取失败"})),
            }
        }
    }

    if file_bytes.is_empty() {
        return Json(json!({"status": "error", "message": "缺少文件"}));
    }

    // 仅允许 PDF 文件
    let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    if ext != "pdf" {
        return Json(json!({"status": "error", "message": "仅支持 PDF 文件"}));
    }

    // 确保上传目录存在
    let upload_dir = "data/uploads";
    if let Err(e) = std::fs::create_dir_all(upload_dir) {
        return Json(json!({"status": "error", "message": format!("创建上传目录失败: {}", e)}));
    }

    // 用 UUID 命名文件
    let uuid = uuid::Uuid::new_v4().to_string();
    let filename = format!("{}.pdf", uuid);
    let filepath = format!("{}/{}", upload_dir, filename);

    if let Err(e) = std::fs::write(&filepath, &file_bytes) {
        return Json(json!({"status": "error", "message": format!("保存文件失败: {}", e)}));
    }

    let url = format!("/uploads/{}", filename);
    Json(json!({
        "status": "ok",
        "url": url,
        "filename": filename,
        "size": file_bytes.len(),
    }))
}

pub async fn api_upload_compare(
    State(s): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Json<serde_json::Value> {
    let mut file_bytes: Vec<u8> = Vec::new();
    let mut file_name = String::new();
    let mut patent_id = String::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();

        if name == "file" {
            file_name = field.file_name().unwrap_or("unknown.txt").to_lowercase();
            match field.bytes().await {
                Ok(data) => {
                    if data.len() > MAX_FILE_SIZE {
                        return Json(json!({"error": "文件大小超过 10MB 限制"}));
                    }
                    file_bytes = data.to_vec();
                }
                Err(_) => return Json(json!({"error": "文件读取失败"})),
            }
        } else if name == "patent_id" {
            if let Ok(text) = field.text().await {
                patent_id = text;
            }
        }
    }

    if file_bytes.is_empty() || patent_id.is_empty() {
        return Json(json!({"error": "缺少文件或专利 ID"}));
    }

    let patent = match s.db.get_patent(&patent_id) {
        Ok(Some(p)) => p,
        _ => return Json(json!({"error": "专利不存在"})),
    };

    // Extract text content based on file type
    let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();

    let is_image = matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp"
    );

    let file_content = if is_image {
        // For images, use AI vision to describe the content
        let ai_client = s
            .config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .ai_client();
        match describe_image_with_fallback(&ai_client, &file_bytes, &ext).await {
            Ok(description) => description,
            Err(e) => return Json(json!({"error": format!("图片识别失败: {}", e)})),
        }
    } else if ext == "pdf" {
        match extract_pdf_text(&file_bytes).await {
            Ok(t) if !t.trim().is_empty() => t,
            _ => {
                // 文字提取失败，用 AI 视觉模型兜底
                let is_deepseek = {
                    let cfg = s.config.read().unwrap_or_else(|e| e.into_inner());
                    cfg.ai_base_url.contains("deepseek")
                };
                if is_deepseek {
                    return Json(
                        json!({"error": "PDF 文字提取失败。当前 AI 为 DeepSeek 不支持图片识别，\n建议：1) 上传可编辑的文本文件(.txt/.docx) 2) 在设置页切换至 Gemini 后重试 3) 直接粘贴文字内容"}),
                    );
                }
                tracing::info!("[UPLOAD] PDF 文字提取失败，尝试 AI 视觉识别...");
                let ai_client = s
                    .config
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .ai_client();
                match extract_pdf_via_ai_vision(&file_bytes, &ai_client).await {
                    Ok(t) => t,
                    Err(e) => {
                        return Json(
                            json!({"error": format!("PDF 提取失败（含 AI 视觉兜底）: {}", e)}),
                        )
                    }
                }
            }
        }
    } else if ext == "docx" {
        // DOCX = ZIP containing XML; extract text from word/document.xml
        match extract_docx_text(&file_bytes) {
            Ok(text) if !text.trim().is_empty() => text,
            Ok(_) => return Json(json!({"error": "DOCX 文件无可提取的文字内容"})),
            Err(e) => return Json(json!({"error": format!("DOCX 解析失败: {}", e)})),
        }
    } else if ext == "doc" {
        return Json(
            json!({"error": "暂不支持旧版 .doc 格式，请将文件另存为 .docx、.txt 或 .pdf 后重试"}),
        );
    } else {
        // TXT, CSV, etc. — try UTF-8, then GBK
        match String::from_utf8(file_bytes.clone()) {
            Ok(text) => text,
            Err(_) => {
                // Try GBK/GB18030 for Chinese text files
                let (text, _encoding, had_errors) = encoding_rs::GBK.decode(&file_bytes);
                if had_errors {
                    return Json(
                        json!({"error": "文件编码不支持，请上传 UTF-8 或 GBK 编码的文本文件、.docx、PDF 或图片"}),
                    );
                }
                text.into_owned()
            }
        }
    };

    if file_content.trim().is_empty() {
        return Json(json!({"error": "文件内容为空"}));
    }

    let ai_client = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client();

    let file_type_label = if is_image {
        "图片识别内容"
    } else {
        "上传文档"
    };

    let prompt = format!(
        "请对比以下两份技术文档，分析它们的相似性和差异：\n\n\
        【专利文档】\n标题：{}\n摘要：{}\n权利要求：{}\n\n\
        【{}】\n{}\n\n\
        请从以下方面分析：\n\
        1. 技术领域是否相同\n\
        2. 解决的技术问题是否相似\n\
        3. 技术方案的相似度（百分比）\n\
        4. 是否存在侵权风险\n\
        5. 主要差异点",
        patent.title,
        patent.abstract_text,
        patent.claims.chars().take(2000).collect::<String>(),
        file_type_label,
        file_content.chars().take(3000).collect::<String>()
    );

    match ai_client.chat(&prompt, None).await {
        Ok(response) => Json(json!({
            "success": true,
            "analysis": response,
            "file_type": ext,
            "content_length": file_content.len()
        })),
        Err(e) => Json(json!({"error": format!("AI 分析失败: {}", e)})),
    }
}

/// 通用文件内容提取（首页上传附件用）
pub async fn api_upload_extract(
    State(s): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Json<serde_json::Value> {
    let mut file_bytes: Vec<u8> = Vec::new();
    let mut file_name = String::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            file_name = field.file_name().unwrap_or("unknown.txt").to_lowercase();
            match field.bytes().await {
                Ok(data) => {
                    if data.len() > MAX_FILE_SIZE {
                        return Json(json!({"error": "文件大小超过 10MB 限制"}));
                    }
                    file_bytes = data.to_vec();
                }
                Err(_) => return Json(json!({"error": "文件读取失败"})),
            }
        }
    }

    if file_bytes.is_empty() {
        return Json(json!({"error": "缺少文件"}));
    }

    let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    let is_image = matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp"
    );

    let text = if is_image {
        let ai_client = s
            .config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .ai_client();
        match describe_image_with_fallback(&ai_client, &file_bytes, &ext).await {
            Ok(desc) => desc,
            Err(e) => return Json(json!({"error": format!("图片识别失败: {}", e)})),
        }
    } else if ext == "pdf" {
        match extract_pdf_text(&file_bytes).await {
            Ok(t) if !t.trim().is_empty() => t,
            _ => {
                // 文字提取失败，用 AI 视觉模型兜底
                let is_deepseek = {
                    let cfg = s.config.read().unwrap_or_else(|e| e.into_inner());
                    cfg.ai_base_url.contains("deepseek")
                };
                if is_deepseek {
                    return Json(
                        json!({"error": "PDF 文字提取失败。当前 AI 为 DeepSeek 不支持图片识别，\n建议：1) 上传可编辑的文本文件(.txt/.docx) 2) 在设置页切换至 Gemini 后重试 3) 直接粘贴文字内容"}),
                    );
                }
                tracing::info!("[UPLOAD] PDF 文字提取失败，尝试 AI 视觉识别...");
                let ai_client = s
                    .config
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .ai_client();
                match extract_pdf_via_ai_vision(&file_bytes, &ai_client).await {
                    Ok(t) => t,
                    Err(e) => {
                        return Json(
                            json!({"error": format!("PDF 提取失败（含 AI 视觉兜底）: {}", e)}),
                        )
                    }
                }
            }
        }
    } else if ext == "docx" {
        match extract_docx_text(&file_bytes) {
            Ok(t) if !t.trim().is_empty() => t,
            Ok(_) => return Json(json!({"error": "DOCX 无可提取文字"})),
            Err(e) => return Json(json!({"error": format!("DOCX 解析失败: {}", e)})),
        }
    } else if ext == "doc" {
        return Json(json!({"error": "暂不支持 .doc 格式，请另存为 .docx 或 .pdf"}));
    } else {
        match String::from_utf8(file_bytes.clone()) {
            Ok(t) => t,
            Err(_) => {
                let (t, _, had_errors) = encoding_rs::GBK.decode(&file_bytes);
                if had_errors {
                    return Json(json!({"error": "文件编码不支持"}));
                }
                t.into_owned()
            }
        }
    };

    Json(json!({
        "text": text.chars().take(50000).collect::<String>(),
        "file_type": ext,
        "length": text.len()
    }))
}

/// AI 视觉兜底：将 PDF 每页转 PNG，用 AI 视觉模型识别文字
/// Fallback: convert PDF pages to PNG images and use AI vision to extract text
async fn extract_pdf_via_ai_vision(
    data: &[u8],
    ai_client: &crate::ai::AiClient,
) -> Result<String, String> {
    let mut temp_files = RuntimeTempFiles::new();
    let tmp_pdf = temp_files
        .write_file("pdf", data)
        .map_err(|error| format!("failed to prepare PDF for vision fallback: {error}"))?;
    let tmp_pdf_str = tmp_pdf.to_string_lossy().to_string();
    let out_prefix = tmp_pdf.with_extension("");
    let out_prefix_str = out_prefix.to_string_lossy().to_string();
    for page in 0..10 {
        temp_files.track_output(PathBuf::from(format!("{}_{}.png", out_prefix_str, page)));
    }

    // Convert PDF pages to PNG using PyMuPDF (max 10 pages)
    let python = r"C:\Users\Administrator\AppData\Local\Programs\Python\Python313\python.exe";
    let script = "import fitz,sys\n\
         doc=fitz.open(sys.argv[1])\n\
         n=min(len(doc),10)\n\
         for i in range(n):\n\
             doc[i].get_pixmap(dpi=200).save(f'{sys.argv[2]}_{i}.png')\n\
         print(n)"
        .to_string();

    let output = std::process::Command::new(python)
        .args(["-c", &script, &tmp_pdf_str, &out_prefix_str])
        .output();

    let page_count: usize = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .trim()
            .parse()
            .unwrap_or(0),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            return Err(format!(
                "PDF 转图片失败: {}",
                stderr.chars().take(200).collect::<String>()
            ));
        }
        Err(e) => return Err(format!("无法调用 Python: {}", e)),
    };

    if page_count == 0 {
        return Err("PDF 转图片失败：无可用页面".into());
    }

    // Read each PNG and use AI vision to describe it
    let mut all_text = String::new();
    for i in 0..page_count {
        let png_path = format!("{}_{}.png", out_prefix_str, i);
        let png_bytes = match std::fs::read(&png_path) {
            Ok(b) => b,
            Err(_) => continue,
        };

        match describe_image_with_fallback(ai_client, &png_bytes, "png").await {
            Ok(text) => {
                if !all_text.is_empty() {
                    all_text.push_str("\n\n---\n\n");
                }
                all_text.push_str(&format!("【第 {} 页】\n{}", i + 1, text));
            }
            Err(e) => {
                tracing::warn!("AI 视觉识别第 {} 页失败: {}", i + 1, e);
            }
        }
    }

    if all_text.trim().is_empty() {
        Err("AI 视觉识别也无法提取 PDF 内容".into())
    } else {
        Ok(all_text)
    }
}

/// Extract text from a PDF file: pdf-extract → pdf-extract by-pages → pdftotext → PyMuPDF → Tesseract OCR
async fn extract_pdf_text(data: &[u8]) -> Result<String, String> {
    // Step 1: Rust pdf-extract (standard mode, good for simple layouts)
    if let Ok(text) = pdf_extract::extract_text_from_mem(data) {
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }
    // Step 2: Rust pdf-extract by-pages (better for multi-column Chinese patents)
    if let Ok(text) = extract_pdf_text_by_pages(data) {
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }
    // Step 3: pdftotext (poppler, handles malformed PDFs well)
    if let Ok(text) = extract_pdf_text_pdftotext(data) {
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }
    // Step 4: PyMuPDF (Python fitz)
    if let Ok(text) = extract_pdf_text_pymupdf(data) {
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }
    // Step 5: Tesseract OCR (handles scanned/special font PDFs)
    if let Ok(text) = extract_pdf_text_ocr(data) {
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }
    // Step 6: Umi-OCR 本地离线 OCR（高精度中文识别，替代依赖云端/外部 Python 环境的方案）
    let data_vec = data.to_vec();
    if let Ok(text) = extract_pdf_text_umi_ocr(data_vec).await {
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }
    // Step 7: MinerU 云端 API（OCR+版面还原，中文专利优化）
    if let Ok(text) = extract_pdf_text_mineru(data) {
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }
    Err("所有 PDF 提取方法均失败".into())
}

/// Extract text from PDF using page-by-page extraction (better for multi-column layouts)
fn extract_pdf_text_by_pages(data: &[u8]) -> Result<String, String> {
    let pages = pdf_extract::extract_text_from_mem_by_pages(data)
        .map_err(|e| format!("逐页提取失败: {}", e))?;
    let mut result = String::new();
    for (i, page_text) in pages.iter().enumerate() {
        let trimmed = page_text.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !result.is_empty() {
            result.push_str(&format!("\n\n--- 第 {} 页 ---\n\n", i + 1));
        }
        result.push_str(trimmed);
    }
    if result.trim().is_empty() {
        return Err("逐页提取结果为空".into());
    }
    Ok(result)
}

/// Fallback: use pdftotext (poppler) to extract text from PDF
fn extract_pdf_text_pdftotext(data: &[u8]) -> Result<String, String> {
    let mut temp_files = RuntimeTempFiles::new();
    let tmp_path = temp_files
        .write_file("pdf", data)
        .map_err(|error| format!("failed to prepare PDF for pdftotext: {error}"))?;
    let tmp_str = tmp_path.to_string_lossy().to_string();

    // Try common pdftotext locations
    let pdftotext_candidates = [
        r"C:\Program Files\poppler\Library\bin\pdftotext.exe",
        r"C:\msys64\mingw64\bin\pdftotext.exe",
        r"C:\Users\Administrator\scoop\apps\poppler\current\Library\bin\pdftotext.exe",
        "/mingw64/bin/pdftotext",
    ];

    for pdftotext in &pdftotext_candidates {
        if !std::path::Path::new(pdftotext).exists() {
            continue;
        }
        let output = std::process::Command::new(pdftotext)
            .args(["-nopgbrk", "-enc", "UTF-8", &tmp_str, "-"])
            .output();

        match output {
            Ok(o) if o.status.success() => {
                let text = String::from_utf8_lossy(&o.stdout).into_owned();
                if !text.trim().is_empty() {
                    return Ok(text);
                }
                break;
            }
            Ok(_) => continue,
            Err(_) => continue,
        }
    }

    Ok(String::new())
}

/// Fallback: use Python PyMuPDF (fitz) to extract text from PDF
fn extract_pdf_text_pymupdf(data: &[u8]) -> Result<String, String> {
    let mut temp_files = RuntimeTempFiles::new();
    let tmp_path = temp_files
        .write_file("pdf", data)
        .map_err(|error| format!("failed to prepare PDF for PyMuPDF: {error}"))?;
    let tmp_str = tmp_path.to_string_lossy().to_string();

    let python = r"C:\Users\Administrator\AppData\Local\Programs\Python\Python313\python.exe";
    let script = "import fitz,sys\nsys.stdout.reconfigure(encoding='utf-8')\ndoc=fitz.open(sys.argv[1])\nfor p in doc:\n print(p.get_text())".to_string();

    let output = std::process::Command::new(python)
        .args(["-c", &script, &tmp_str])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout).to_string();
            if text.trim().is_empty() {
                Ok(String::new()) // empty — let caller try next method
            } else {
                Ok(text)
            }
        }
        Ok(_) | Err(_) => Ok(String::new()), // failed — let caller try next method
    }
}

/// MinerU 云端 API 提取（中文专利优化，需 MINERU_API_TOKEN 环境变量）
/// 免费 token：https://mineru.net/apiManage/token
/// 通过 Python mineru-open-sdk 调用，已在环境中安装
fn extract_pdf_text_mineru(data: &[u8]) -> Result<String, String> {
    let token = std::env::var("MINERU_API_TOKEN").unwrap_or_default();
    if token.is_empty() {
        return Err("MINERU_API_TOKEN 未配置".into());
    }

    let mut temp_files = RuntimeTempFiles::new();
    let tmp_pdf = temp_files
        .write_file("pdf", data)
        .map_err(|error| format!("failed to prepare PDF for MinerU: {error}"))?;
    let tmp_pdf_str = tmp_pdf.to_string_lossy().to_string();

    // Python script to call mineru-open-sdk and output markdown
    let script = format!(
        r#"import sys, json
sys.stdout.reconfigure(encoding='utf-8')
from mineru import MinerU
client = MinerU('{}')
result = client.extract(r'{}')
if result and result.markdown:
    print(result.markdown)
else:
    print('MINERU_EMPTY')
"#,
        token, tmp_pdf_str
    );

    let python = r"C:\Users\Administrator\AppData\Local\Programs\Python\Python313\python.exe";
    let output = std::process::Command::new(python)
        .args(["-c", &script])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout).to_string();
            if text.trim() == "MINERU_EMPTY" || text.trim().is_empty() {
                let stderr = String::from_utf8_lossy(&o.stderr);
                Err(format!(
                    "MinerU 提取结果为空: {}",
                    stderr.chars().take(200).collect::<String>()
                ))
            } else {
                Ok(text)
            }
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            Err(format!(
                "MinerU 提取失败: {}",
                stderr.chars().take(300).collect::<String>()
            ))
        }
        Err(e) => Err(format!("无法调用 Python: {}", e)),
    }
}

/// Fallback: use Tesseract OCR via Python to extract text from scanned PDFs
fn extract_pdf_text_ocr(data: &[u8]) -> Result<String, String> {
    let mut temp_files = RuntimeTempFiles::new();
    let tmp_path = temp_files
        .write_file("pdf", data)
        .map_err(|error| format!("failed to prepare PDF for OCR: {error}"))?;
    let tmp_str = tmp_path.to_string_lossy().to_string();

    let python = r"C:\Users\Administrator\AppData\Local\Programs\Python\Python313\python.exe";
    let script = r#"
import pytesseract, fitz, sys
from PIL import Image
import io

sys.stdout.reconfigure(encoding='utf-8')
pytesseract.pytesseract.tesseract_cmd = r'C:\Program Files\Tesseract-OCR\tesseract.exe'
doc = fitz.open(sys.argv[1])
for page in doc:
    mat = fitz.Matrix(2.0, 2.0)
    pix = page.get_pixmap(matrix=mat)
    img = Image.open(io.BytesIO(pix.tobytes('png')))
    text = pytesseract.image_to_string(img, lang='chi_sim+eng')
    print(text)
"#;

    let output = std::process::Command::new(python)
        .args(["-c", script, &tmp_str])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout).to_string();
            if text.trim().is_empty() {
                Err("OCR 也无法识别文字".into())
            } else {
                Ok(text)
            }
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            Err(format!(
                "OCR 失败: {}",
                stderr.chars().take(200).collect::<String>()
            ))
        }
        Err(e) => Err(format!("无法调用 Python OCR: {}", e)),
    }
}

/// Extract text from a DOCX file (ZIP containing XML)
fn extract_docx_text(data: &[u8]) -> Result<String, String> {
    use std::io::{Cursor, Read};
    let reader = Cursor::new(data);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| format!("非有效DOCX: {}", e))?;
    let mut xml = String::new();
    if let Ok(mut file) = archive.by_name("word/document.xml") {
        file.read_to_string(&mut xml)
            .map_err(|e| format!("读取失败: {}", e))?;
    } else {
        return Err("DOCX 中找不到 word/document.xml".into());
    }
    // Strip XML tags to get plain text
    let mut text = String::new();
    let mut in_tag = false;
    for ch in xml.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            text.push(ch);
        }
    }
    Ok(text)
}

/// Use AI vision (GLM-4V or compatible) to describe an image
async fn describe_image_with_ai(
    ai_client: &crate::ai::AiClient,
    image_bytes: &[u8],
    ext: &str,
) -> Result<String, String> {
    use base64::Engine;

    let b64 = base64::engine::general_purpose::STANDARD.encode(image_bytes);
    let mime = match ext {
        "png" => "image/png",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        _ => "image/jpeg",
    };
    let data_url = format!("data:{};base64,{}", mime, b64);

    ai_client
        .describe_image(&data_url)
        .await
        .map_err(|e| format!("{}", e))
}

// ──────────────────────────────────────────────
// Umi-OCR 集成：本地离线 OCR 引擎
// 下载：https://github.com/hiroi-sora/Umi-OCR
// 启动后默认监听 http://127.0.0.1:1224
// ──────────────────────────────────────────────

/// 通过 Umi-OCR HTTP API 识别 PDF 文档的文本内容（处理扫描件/不可选文字 PDF）
///
/// 使用 Umi-OCR 的文档识别接口：
///   上传 → 轮询 → 获取结果
async fn extract_pdf_text_umi_ocr(data: Vec<u8>) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(UMI_OCR_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Umi-OCR 客户端创建失败: {}", e))?;

    // Upload via multipart; the request body already owns the PDF bytes.
    let upload_url = format!("{}/api/doc/upload", UMI_OCR_BASE_URL);
    let file_part = reqwest::multipart::Part::bytes(data)
        .file_name("document.pdf")
        .mime_str("application/pdf")
        .map_err(|e| format!("Umi-OCR 创建 multipart 失败: {}", e))?;
    let form = reqwest::multipart::Form::new()
        .part("file", file_part)
        .text(
            "json",
            r#"{"doc.extractionMode":"fullPage","data.format":"text"}"#,
        );

    let upload_resp = client
        .post(&upload_url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Umi-OCR 上传失败 (服务未启动?): {}", e))?;

    let upload_json: serde_json::Value = upload_resp
        .json()
        .await
        .map_err(|e| format!("Umi-OCR 响应解析失败: {}", e))?;

    let task_id = match upload_json["code"].as_i64() {
        Some(100) => upload_json["data"].as_str().map(|s| s.to_string()),
        _ => {
            return Err(format!(
                "Umi-OCR 上传失败: code={}, msg={}",
                upload_json["code"], upload_json["data"]
            ));
        }
    };

    let task_id = match task_id {
        Some(id) => id,
        None => {
            return Err("Umi-OCR 上传成功但未返回任务 ID".into());
        }
    };

    // Step 3: Poll for result (wait up to ~90s)
    let result_url = format!("{}/api/doc/result", UMI_OCR_BASE_URL);
    let mut all_text = String::new();
    let max_polls = 90; // 90 * 1s = 90s max wait
    for _poll in 0..max_polls {
        let poll_resp = client
            .post(&result_url)
            .json(&serde_json::json!({
                "id": task_id,
                "is_data": true,
                "is_unread": true,
                "format": "text"
            }))
            .send()
            .await;

        let poll_json: serde_json::Value = match poll_resp {
            Ok(r) => r.json().await.unwrap_or_default(),
            Err(_) => {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        if poll_json["code"] != 100 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        }

        let is_done = poll_json["is_done"].as_bool().unwrap_or(false);
        let state = poll_json["state"].as_str().unwrap_or("");

        // Collect incremental text
        if let Some(data_str) = poll_json["data"].as_str() {
            if !data_str.trim().is_empty() {
                all_text.push_str(data_str);
            }
        }

        if is_done {
            break;
        }

        // state == "failure" — stop early
        if state == "failure" {
            break;
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    // Step 4: Cleanup — clear task on Umi-OCR
    let clear_url = format!("{}/api/doc/clear/{}", UMI_OCR_BASE_URL, task_id);
    let _ = client.get(&clear_url).send().await;

    if all_text.trim().is_empty() {
        Err("Umi-OCR 未能识别 PDF 中的文字".into())
    } else {
        Ok(all_text)
    }
}

/// 通过 Umi-OCR HTTP API 识别图片文字（替代 AI 视觉模型，免费且离线）
///
/// 使用 Umi-OCR 的图片 OCR 接口：POST /api/ocr
async fn extract_image_text_umi_ocr(image_bytes: &[u8], _ext: &str) -> Result<String, String> {
    use base64::Engine;

    let b64 = base64::engine::general_purpose::STANDARD.encode(image_bytes);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(UMI_OCR_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Umi-OCR 客户端创建失败: {}", e))?;

    let resp = client
        .post(format!("{}/api/ocr", UMI_OCR_BASE_URL))
        .json(&serde_json::json!({
            "base64": b64,
            "options": {
                "data.format": "text",
                "tbpu.parser": "multi_para",
            }
        }))
        .send()
        .await
        .map_err(|e| format!("Umi-OCR 请求失败 (服务未启动?): {}", e))?;

    let result: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Umi-OCR 响应解析失败: {}", e))?;

    match result["code"].as_i64() {
        Some(100) => {
            let text = result["data"].as_str().unwrap_or("");
            if text.trim().is_empty() {
                Err("Umi-OCR 图片识别结果为空".into())
            } else {
                Ok(text.to_string())
            }
        }
        Some(101) => Err("Umi-OCR 图片中未识别到文字".into()),
        _ => Err(format!("Umi-OCR 识别失败: code={}", result["code"])),
    }
}

/// Try Umi-OCR for image text extraction; fall back to AI vision on failure.
async fn describe_image_with_fallback(
    ai_client: &crate::ai::AiClient,
    image_bytes: &[u8],
    ext: &str,
) -> Result<String, String> {
    // 优先尝试本地 Umi-OCR（免费、离线、快速）
    match extract_image_text_umi_ocr(image_bytes, ext).await {
        Ok(text) => {
            tracing::info!("[Umi-OCR] 图片 OCR 成功，提取 {} 字符", text.len());
            return Ok(text);
        }
        Err(e) => {
            tracing::warn!("[Umi-OCR] 图片识别失败，回退到 AI 视觉模型: {}", e);
        }
    }
    // 回退：云端 AI 视觉模型
    describe_image_with_ai(ai_client, image_bytes, ext).await
}

/// POST /api/patent/pdf/extract-text — 针对专利 PDF 的专用文本提取，返回按页分段结果
///
/// 接收 patent_id（从 DB 查找专利获取 PDF）或直接上传 PDF 文件。
/// 返回每页文本的 JSON 数组，方便 AI 按页分析。
pub async fn api_patent_pdf_extract_text(
    State(s): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Json<serde_json::Value> {
    let mut file_bytes: Vec<u8> = Vec::new();
    let mut patent_id = String::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "patent_id" {
            if let Ok(text) = field.text().await {
                patent_id = text;
            }
        } else if name == "file" {
            match field.bytes().await {
                Ok(data) => {
                    if data.len() > MAX_PDF_STORE_SIZE {
                        return Json(json!({"error": "文件大小超过 20MB 限制"}));
                    }
                    file_bytes = data.to_vec();
                }
                Err(_) => return Json(json!({"error": "文件读取失败"})),
            }
        }
    }

    // Try to get PDF bytes from patent_id if no file uploaded
    if file_bytes.is_empty() && !patent_id.is_empty() {
        // Look up patent PDF from enrichment
        if let Ok(Some(patent)) = s.db.get_patent(&patent_id) {
            if !patent.pdf_url.is_empty() {
                match download_pdf(&patent.pdf_url).await {
                    Ok(bytes) => file_bytes = bytes,
                    Err(error) => tracing::warn!(
                        patent_id = %patent_id,
                        error = %error,
                        "Remote patent PDF download was rejected or failed"
                    ),
                }
            }
        }
        if file_bytes.is_empty() {
            return Json(json!({"error": "未找到专利 PDF，请直接上传 PDF 文件"}));
        }
    }

    if file_bytes.is_empty() {
        return Json(json!({"error": "缺少文件或 patent_id"}));
    }

    // Extract text using page-by-page extraction
    let pages = match pdf_extract::extract_text_from_mem_by_pages(&file_bytes) {
        Ok(pages) => pages,
        Err(e) => {
            // Fallback: try standard extraction and wrap as single page
            match pdf_extract::extract_text_from_mem(&file_bytes) {
                Ok(text) if !text.trim().is_empty() => {
                    return Json(json!({
                        "status": "ok",
                        "pages": [{
                            "page": 1,
                            "text": text.trim(),
                            "char_count": text.trim().len()
                        }],
                        "page_count": 1,
                        "method": "standard_fallback"
                    }));
                }
                _ => return Json(json!({"error": format!("PDF 文本提取失败: {}", e)})),
            }
        }
    };

    let page_count = pages.len();
    let page_list: Vec<serde_json::Value> = pages
        .into_iter()
        .enumerate()
        .filter(|(_, text)| !text.trim().is_empty())
        .map(|(i, text)| {
            json!({
                "page": i + 1,
                "text": text.trim(),
                "char_count": text.trim().len()
            })
        })
        .collect();

    let total_chars: usize = page_list
        .iter()
        .filter_map(|p| p["char_count"].as_u64())
        .sum::<u64>() as usize;

    // Also return full text concatenated for convenience
    let full_text: String = page_list
        .iter()
        .map(|p| {
            format!(
                "【第 {} 页】\n{}\n",
                p["page"].as_u64().unwrap_or(0),
                p["text"].as_str().unwrap_or("")
            )
        })
        .collect();

    Json(json!({
        "status": "ok",
        "pages": page_list,
        "page_count": page_count,
        "total_chars": total_chars,
        "full_text": full_text,
        "method": "by_pages"
    }))
}

/// Validate a remotely stored patent PDF URL before resolving or downloading it.
///
/// This accepts only host names so the subsequent DNS result can be checked and
/// pinned to the request. IP-literal URLs bypass that protection and are refused.
fn validate_pdf_download_url(raw_url: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(raw_url).map_err(|_| "invalid PDF URL".to_string())?;

    if url.scheme() != "https" {
        return Err("PDF URL must use HTTPS".to_string());
    }
    if url.port_or_known_default() != Some(443) {
        return Err("PDF URL must use port 443".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("PDF URL must not include user credentials".to_string());
    }

    let host = url
        .host_str()
        .ok_or_else(|| "PDF URL must include a host name".to_string())?;
    if is_ip_literal(host) {
        return Err("PDF URL must use a host name instead of an IP literal".to_string());
    }

    Ok(url)
}

fn is_ip_literal(host: &str) -> bool {
    let unbracketed = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    unbracketed.parse::<IpAddr>().is_ok()
}

async fn resolve_public_pdf_host(host: &str) -> Result<Vec<SocketAddr>, String> {
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host, 443))
        .await
        .map_err(|error| format!("DNS lookup failed: {error}"))?
        .collect();

    if addresses.is_empty() {
        return Err("DNS lookup returned no addresses".to_string());
    }
    if addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err("DNS lookup returned a non-public address".to_string());
    }

    Ok(addresses)
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => is_public_ipv4(ipv4),
        IpAddr::V6(ipv6) => is_public_ipv6(ipv6),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let [first, second, third, _fourth] = ip.octets();

    !matches!(
        (first, second, third),
        (0, _, _)
            | (10, _, _)
            | (100, 64..=127, _)
            | (127, _, _)
            | (169, 254, _)
            | (172, 16..=31, _)
            | (192, 0, _)
            | (192, 88, 99)
            | (192, 168, _)
            | (198, 18..=19, _)
            | (198, 51, 100)
            | (203, 0, 113)
            | (224..=255, _, _)
    )
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if ip.is_unspecified() || ip.is_loopback() || ip.is_multicast() {
        return false;
    }

    let octets = ip.octets();
    if octets[0] & 0xfe == 0xfc
        || (octets[0] == 0xfe && (octets[1] & 0xc0 == 0x80 || octets[1] & 0xc0 == 0xc0))
        || octets.starts_with(&[0x20, 0x01, 0x0d, 0xb8])
        || octets.starts_with(&[0x20, 0x01, 0x00, 0x00])
        || octets.starts_with(&[0x20, 0x02])
    {
        return false;
    }

    let is_ipv4_embedded = octets[..12].iter().all(|byte| *byte == 0)
        || (octets[..10].iter().all(|byte| *byte == 0) && octets[10] == 0xff && octets[11] == 0xff);
    let embedded_ipv4 = if is_ipv4_embedded {
        Some(Ipv4Addr::new(
            octets[12], octets[13], octets[14], octets[15],
        ))
    } else {
        None
    };

    embedded_ipv4.map(is_public_ipv4).unwrap_or(true)
}

fn validate_pdf_content_length(content_length: Option<u64>) -> Result<(), &'static str> {
    if let Some(length) = content_length {
        if length > MAX_PDF_STORE_SIZE as u64 {
            return Err("PDF response exceeds the 20 MB limit");
        }
    }
    Ok(())
}

fn append_pdf_chunk(buffer: &mut Vec<u8>, chunk: &[u8]) -> Result<(), &'static str> {
    if buffer.len().saturating_add(chunk.len()) > MAX_PDF_STORE_SIZE {
        return Err("PDF response exceeds the 20 MB limit");
    }
    buffer.extend_from_slice(chunk);
    Ok(())
}

fn has_pdf_header(data: &[u8]) -> bool {
    data[..data.len().min(1024)]
        .windows(b"%PDF-".len())
        .any(|window| window == b"%PDF-")
}

/// Download a PDF from a URL (for patent PDFs from enrichment).
///
/// DNS is resolved once, checked for public addresses, and pinned to this request
/// to prevent redirect and DNS-rebinding based SSRF.
async fn download_pdf(raw_url: &str) -> Result<Vec<u8>, String> {
    let url = validate_pdf_download_url(raw_url)?;
    let host = url
        .host_str()
        .ok_or_else(|| "validated PDF URL has no host".to_string())?
        .to_string();
    let addresses = resolve_public_pdf_host(&host).await?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(&host, &addresses)
        .build()
        .map_err(|error| format!("HTTP client setup failed: {error}"))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("PDF download request failed: {error}"))?;

    if !resp.status().is_success() {
        return Err(format!("PDF upstream returned HTTP {}", resp.status()));
    }
    validate_pdf_content_length(resp.content_length()).map_err(str::to_string)?;

    let mut data = Vec::new();
    let mut response = resp;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("PDF response stream failed: {error}"))?
    {
        append_pdf_chunk(&mut data, &chunk).map_err(str::to_string)?;
    }

    if !has_pdf_header(&data) {
        return Err(
            "PDF response did not contain a valid %PDF- header in its first 1024 bytes".to_string(),
        );
    }

    Ok(data)
}

#[cfg(test)]
mod remote_pdf_download_tests {
    use super::{
        append_pdf_chunk, has_pdf_header, is_public_ip, is_public_ipv4, is_public_ipv6,
        resolve_public_pdf_host, validate_pdf_content_length, validate_pdf_download_url,
        MAX_PDF_STORE_SIZE,
    };
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn accepts_only_https_hostname_pdf_urls_without_credentials() {
        assert!(validate_pdf_download_url("https://example.com/patent.pdf").is_ok());

        for url in [
            "http://example.com/patent.pdf",
            "https://example.com:8443/patent.pdf",
            "https://user@example.com/patent.pdf",
            "https://127.0.0.1/patent.pdf",
            "https://[::1]/patent.pdf",
            "not a URL",
        ] {
            assert!(
                validate_pdf_download_url(url).is_err(),
                "should reject {url}"
            );
        }
    }

    #[test]
    fn rejects_private_loopback_shared_and_reserved_ip_ranges() {
        for raw_ip in [
            "127.0.0.1",
            "10.0.0.1",
            "100.64.0.1",
            "169.254.1.1",
            "172.16.0.1",
            "192.168.0.1",
            "192.0.2.1",
            "198.18.0.1",
            "203.0.113.1",
            "255.255.255.255",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
        ] {
            let ip = raw_ip
                .parse::<IpAddr>()
                .expect("test input must be an IP address");
            assert!(!is_public_ip(ip), "should reject {raw_ip}");
        }

        assert!(is_public_ipv4(Ipv4Addr::new(1, 1, 1, 1)));
        assert!(is_public_ipv6(
            "2606:4700:4700::1111"
                .parse::<Ipv6Addr>()
                .expect("test input must be an IPv6 address")
        ));
    }

    #[test]
    fn validates_pdf_header_and_response_size_for_streaming_downloads() {
        assert!(has_pdf_header(b"%PDF-1.7\n"));
        let mut pdf_with_leading_bytes = vec![b' '; 128];
        pdf_with_leading_bytes.extend_from_slice(b"%PDF-1.7\n");
        assert!(has_pdf_header(&pdf_with_leading_bytes));

        let mut pdf_after_header_window = vec![b' '; 1024];
        pdf_after_header_window.extend_from_slice(b"%PDF-1.7\n");
        assert!(!has_pdf_header(&pdf_after_header_window));
        assert!(!has_pdf_header(b"<html>not a PDF"));
        assert!(validate_pdf_content_length(Some(MAX_PDF_STORE_SIZE as u64)).is_ok());
        assert!(validate_pdf_content_length(Some(MAX_PDF_STORE_SIZE as u64 + 1)).is_err());

        let mut data = vec![0; MAX_PDF_STORE_SIZE - 1];
        assert!(append_pdf_chunk(&mut data, &[1]).is_ok());
        assert!(append_pdf_chunk(&mut data, &[2]).is_err());
    }

    #[tokio::test]
    async fn rejects_localhost_dns_before_any_download_request() {
        assert!(resolve_public_pdf_host("localhost").await.is_err());
    }
}

#[cfg(test)]
mod runtime_temp_file_tests {
    use super::{runtime_temp_dir, RuntimeTempFiles};

    #[test]
    fn runtime_temp_files_are_unique_under_project_data_and_removed_on_drop() {
        let expected_directory = std::env::current_dir()
            .expect("test working directory should be available")
            .join("data")
            .join("runtime-temp");
        assert_eq!(
            runtime_temp_dir().expect("runtime temp directory"),
            expected_directory
        );

        let paths = {
            let mut temp_files = RuntimeTempFiles::new();
            let first = temp_files
                .write_file("pdf", b"first")
                .expect("first temporary file");
            let second = temp_files
                .write_file("pdf", b"second")
                .expect("second temporary file");

            assert_ne!(first, second);
            for path in [&first, &second] {
                assert!(path.starts_with(&expected_directory));
                assert!(path.is_file());
            }
            assert_eq!(
                std::fs::read(&first).expect("read first temporary file"),
                b"first"
            );
            assert_eq!(
                std::fs::read(&second).expect("read second temporary file"),
                b"second"
            );
            vec![first, second]
        };

        for path in paths {
            assert!(
                !path.exists(),
                "temporary file should be removed: {}",
                path.display()
            );
        }
    }
}
