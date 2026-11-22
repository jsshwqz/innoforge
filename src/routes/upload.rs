//! 模块职责：文件上传与内容提取统一入口（PDF/DOCX/TXT/图片），含 T2.5 异步 SSE OCR 进度流、优先 Umi-OCR 通道与 4 类结构化错误码。
//! 上游调用者：`src/common.rs` (`/api/upload/*`)、`src/ai/extraction.rs` (`extract_pdf_text_umi_ocr`) 与前端各上传入口。
//! 模块专属禁区：严禁硬编码个人 Windows Python/Tesseract 绝对路径；严禁在异步路由中直接执行阻塞子进程（需 `tokio::process` 或 `spawn_blocking`）；上传错误必须归入 4 类结构化错误码且兼容 `error` 键。

use super::AppState;
use crate::ai::truncate_for_ai;
use axum::{
    extract::State,
    response::sse::{Event, Sse},
    Json,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    convert::Infallible,
    fs::{self, File, OpenOptions},
    io::Write,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::PathBuf,
    time::Duration,
};

const MAX_FILE_SIZE: usize = 10 * 1024 * 1024; // 10 MB
const MAX_PDF_STORE_SIZE: usize = 20 * 1024 * 1024; // 20 MB

/// T2.5 4 类结构化文件上传/解析错误码
pub const ERR_PDF_ENCRYPTED: &str = "ERR_PDF_ENCRYPTED";
pub const ERR_FILE_CORRUPTED: &str = "ERR_FILE_CORRUPTED";
pub const ERR_OCR_UNAVAILABLE: &str = "ERR_OCR_UNAVAILABLE";
pub const ERR_FILE_TOO_LARGE: &str = "ERR_FILE_TOO_LARGE";

/// 结构化文件解析错误（向后兼容旧 `error` 字段，并补充机器可判定的 `error_code` 与排障 `hint`）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuredUploadError {
    pub error_code: String,
    pub error: String,
    pub hint: String,
}

impl StructuredUploadError {
    pub fn new(code: impl Into<String>, error: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            error_code: code.into(),
            error: error.into(),
            hint: hint.into(),
        }
    }

    pub fn file_too_large(max_mb: usize) -> Self {
        Self::new(
            ERR_FILE_TOO_LARGE,
            format!("文件大小超过 {max_mb}MB 限制"),
            format!("请先压缩图片清晰度或按章节拆分 PDF 至 {max_mb}MB 以内后重新上传。"),
        )
    }

    pub fn pdf_encrypted() -> Self {
        Self::new(
            ERR_PDF_ENCRYPTED,
            "检测到该 PDF 已加密或受密码/权限保护，无法直接提取文字",
            "请先在 PDF 阅读器中解除密码保护（或“另存为/打印为新 PDF”）后再上传。",
        )
    }

    pub fn file_corrupted(detail: impl Into<String>) -> Self {
        Self::new(
            ERR_FILE_CORRUPTED,
            format!("文件已损坏或格式异常：{}", detail.into()),
            "请确认文件下载完整、未截断，或将其重新导出为标准 PDF / DOCX / TXT 后重试。",
        )
    }

    pub fn ocr_unavailable(detail: impl Into<String>) -> Self {
        Self::new(
            ERR_OCR_UNAVAILABLE,
            format!("PDF 为扫描件且当前无可用 OCR 引擎（{}）", detail.into()),
            "排障建议：1) 启动本机 Umi-OCR 并确认监听端口（可用 UMI_OCR_BASE_URL 配置）；2) 或在设置页配置支持看图的多模态模型（如 Gemini/Qwen-VL）；3) 或直接上传可编辑的 .docx/.txt。",
        )
    }

    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "error_code": self.error_code,
            "error": self.error,
            "hint": self.hint,
        })
    }
}

/// 异步 SSE OCR 进度事件载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrProgressEvent {
    pub stage: String,
    pub page: usize,
    pub total_pages: usize,
    pub percent: u8,
    pub message: String,
}

/// 预检 PDF 字节完整性、加密状态与总页数（确定性纯函数）
pub fn inspect_pdf_integrity(data: &[u8]) -> Result<usize, StructuredUploadError> {
    if !has_pdf_header(data) {
        return Err(StructuredUploadError::file_corrupted(
            "未检测到 %PDF- 文件签名",
        ));
    }
    // 检查未压缩字节中是否包含典型加密字典标记 `/Encrypt`
    let has_encrypt_marker = data.windows(8).any(|w| w == b"/Encrypt");
    match pdf_extract::Document::load_mem(data) {
        Ok(doc) => {
            if doc.is_encrypted() || has_encrypt_marker {
                return Err(StructuredUploadError::pdf_encrypted());
            }
            let pages = doc.get_pages().len();
            if pages == 0 {
                return Err(StructuredUploadError::file_corrupted(
                    "PDF 不包含任何有效页面",
                ));
            }
            Ok(pages)
        }
        Err(e) => {
            let msg = e.to_string();
            let lower = msg.to_lowercase();
            if has_encrypt_marker
                || lower.contains("encrypt")
                || lower.contains("password")
                || lower.contains("decryption")
            {
                return Err(StructuredUploadError::pdf_encrypted());
            }
            let has_eof = data.windows(5).any(|w| w == b"%%EOF");
            if !has_eof {
                return Err(StructuredUploadError::file_corrupted(format!(
                    "缺少 %%EOF 结束标记（{msg}）"
                )));
            }
            // 某些非标 PDF 虽令 lopdf 报轻微语法错，但 poppler/Umi-OCR 仍可读取
            Ok(1)
        }
    }
}

fn resolve_python_bin() -> String {
    if let Ok(p) = std::env::var("INNOFORGE_PYTHON_BIN") {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    for cand in ["python3", "python"] {
        if std::process::Command::new(cand)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
        {
            return cand.to_string();
        }
    }
    "python3".to_string()
}

/// Umi-OCR 出厂默认 HTTP 地址（端口可在 Umi-OCR 全局设置中修改）。
/// issue #33：可用环境变量 `UMI_OCR_BASE_URL` 覆盖（部署环境属性 → env 注入点）。
const UMI_OCR_BASE_URL_DEFAULT: &str = "http://127.0.0.1:1224";

/// 解析 Umi-OCR 服务地址：环境变量 `UMI_OCR_BASE_URL` 非空时覆盖默认值。
/// 纯函数（env 由调用方传入），由 `umi_ocr_base_url_resolution` 锁死优先级/去尾斜杠/空值钳制。
fn resolve_umi_ocr_base_url(env_value: Option<&str>) -> String {
    match env_value {
        Some(v) => {
            let v = v.trim().trim_end_matches('/').to_string();
            if v.is_empty() {
                UMI_OCR_BASE_URL_DEFAULT.to_string()
            } else {
                v
            }
        }
        None => UMI_OCR_BASE_URL_DEFAULT.to_string(),
    }
}

pub(crate) fn umi_ocr_base_url() -> String {
    resolve_umi_ocr_base_url(std::env::var("UMI_OCR_BASE_URL").ok().as_deref())
}

/// 从服务地址推导探测地址（去 scheme、取 host:port）——TCP 探测目标必须与
/// 实际 HTTP 目标同源，写死 1224 会在自定义地址下探错对象。
fn probe_addr_for(base_url: &str) -> String {
    let stripped = base_url
        .strip_prefix("https://")
        .or_else(|| base_url.strip_prefix("http://"))
        .unwrap_or(base_url);
    stripped.split('/').next().unwrap_or(stripped).to_string()
}

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
    if !has_pdf_header(&file_bytes) {
        return Json(json!({
            "status": "error",
            "message": "文件不是有效 PDF，未检测到 %PDF- 文件签名"
        }));
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

    let ext_from_name = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    // #97: filename may be missing or mangled for non-ASCII names.
    // Use content magic bytes to detect type when the extension is unreliable.
    let ext = match detect_type_by_magic(&file_bytes) {
        Some(magic_type) => {
            // Magic bytes override the filename extension — the content is
            // authoritative.  This fixes non-ASCII filenames where the
            // multipart parser may return None or a mangled file_name.
            magic_type.to_string()
        }
        None => ext_from_name,
    };
    if ext == "pdf" && !has_pdf_header(&file_bytes) {
        return Json(json!({
            "error": "文件不是有效 PDF，未检测到 %PDF- 文件签名"
        }));
    }

    let patent = match s.db.get_patent(&patent_id) {
        Ok(Some(p)) => p,
        _ => return Json(json!({"error": "专利不存在"})),
    };

    // Extract text content based on file type
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
            Err(e) => return Json(json!({ "error": format!("图片识别失败: {}", e) })),
        }
    } else if ext == "pdf" {
        match extract_pdf_text(&file_bytes).await {
            Ok(t) if t.trim().len() >= 100 => t,
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
                let ai_client = s
                    .config
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .ai_client();
                match extract_pdf_via_ai_vision(&file_bytes, &ai_client).await {
                    Ok(t) => t,
                    Err(e) => {
                        return Json(json!({
                            "error": format!("PDF 提取失败（含 AI 视觉兜底）: {}", e)
                        }))
                    }
                }
            }
        }
    } else if ext == "docx" {
        // DOCX = ZIP containing XML; extract text from word/document.xml
        match extract_docx_text(&file_bytes) {
            Ok(text) if !text.trim().is_empty() => text,
            Ok(_) => return Json(json!({"error": "DOCX 文件无可提取的文字内容"})),
            Err(e) => return Json(json!({ "error": format!("DOCX 解析失败: {}", e) })),
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
                        json!({"error": "无法识别文件类型，请上传 PDF、DOCX、TXT 或图片文件"}),
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
        crate::ai::patent::structured_patent_excerpt(&patent.claims, 30_000),
        file_type_label,
        crate::ai::patent::structured_patent_excerpt(&file_content, 30_000)
    );

    match ai_client.chat(&prompt, None).await {
        Ok(response) => {
            let _ = s.db.log_ai_call(&ai_client, "upload-compare", None, None);
            Json(json!({
                "success": true,
                "analysis": response,
                "file_type": ext,
                "content_length": file_content.len()
            }))
        }
        Err(e) => Json(json!({ "error": format!("AI 分析失败: {}", e) })),
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
                        return Json(StructuredUploadError::file_too_large(10).to_json());
                    }
                    file_bytes = data.to_vec();
                }
                Err(_) => {
                    return Json(StructuredUploadError::file_corrupted("文件读取失败").to_json())
                }
            }
        }
    }

    if file_bytes.is_empty() {
        return Json(StructuredUploadError::file_corrupted("缺少文件或文件为空").to_json());
    }

    tracing::info!(
        "[EXTRACT] file_name={}, size={}, has_pdf_header={}",
        file_name,
        file_bytes.len(),
        if file_name.to_lowercase().ends_with(".pdf") {
            has_pdf_header(&file_bytes).to_string()
        } else {
            "N/A".to_string()
        }
    );

    let ext_from_name = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    // #97: filename may be missing or mangled for non-ASCII names.
    // Use content magic bytes to detect type when the extension is unreliable.
    let ext = match detect_type_by_magic(&file_bytes) {
        Some(magic_type) => magic_type.to_string(),
        None => ext_from_name,
    };
    if ext == "pdf" {
        if let Err(err) = inspect_pdf_integrity(&file_bytes) {
            return Json(err.to_json());
        }
    }
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
            Err(e) => return Json(StructuredUploadError::ocr_unavailable(e).to_json()),
        }
    } else if ext == "pdf" {
        match extract_pdf_text(&file_bytes).await {
            Ok(t) if t.trim().len() >= 100 => t,
            _ => {
                // 文字提取失败，用 AI 视觉模型兜底
                let is_deepseek = {
                    let cfg = s.config.read().unwrap_or_else(|e| e.into_inner());
                    cfg.ai_base_url.contains("deepseek")
                };
                if is_deepseek {
                    return Json(
                        StructuredUploadError::ocr_unavailable(
                            "本地 Umi-OCR/Tesseract 未命中，且当前 AI 为 DeepSeek 不支持图片识别",
                        )
                        .to_json(),
                    );
                }
                let ai_client = s
                    .config
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .ai_client();
                match extract_pdf_via_ai_vision(&file_bytes, &ai_client).await {
                    Ok(t) => t,
                    Err(e) => {
                        return Json(
                            StructuredUploadError::ocr_unavailable(format!(
                                "全部 OCR 与视觉兜底均失败: {e}"
                            ))
                            .to_json(),
                        )
                    }
                }
            }
        }
    } else if ext == "docx" {
        match extract_docx_text(&file_bytes) {
            Ok(t) if t.trim().len() >= 100 => t,
            Ok(_) => {
                return Json(StructuredUploadError::file_corrupted("DOCX 无可提取文字").to_json())
            }
            Err(e) => {
                return Json(
                    StructuredUploadError::file_corrupted(format!("DOCX 解析失败: {e}")).to_json(),
                )
            }
        }
    } else if ext == "doc" {
        return Json(
            StructuredUploadError::file_corrupted("暂不支持 .doc 格式，请另存为 .docx 或 .pdf")
                .to_json(),
        );
    } else {
        match String::from_utf8(file_bytes.clone()) {
            Ok(t) => t,
            Err(_) => {
                let (t, _, had_errors) = encoding_rs::GBK.decode(&file_bytes);
                if had_errors {
                    return Json(
                        StructuredUploadError::file_corrupted(
                            "无法识别文件类型，请上传 PDF、DOCX、TXT 或图片文件",
                        )
                        .to_json(),
                    );
                }
                t.into_owned()
            }
        }
    };

    tracing::info!(
        "[EXTRACT] result: file_type={}, text_len={}, preview={}",
        ext,
        text.len(),
        text.chars().take(80).collect::<String>()
    );
    const MAX_EXTRACT_CHARS: usize = 50_000;
    let total_chars = text.chars().count();
    let truncated = total_chars > MAX_EXTRACT_CHARS;
    Json(json!({
        "text": truncate_for_ai(&text, MAX_EXTRACT_CHARS),
        "file_type": ext,
        "length": text.len(),
        "char_length": total_chars,
        "truncated": truncated
    }))
}

/// T2.5: POST /api/upload/extract/stream — 异步 SSE 文件提取与分页 OCR 进度流
///
/// 优先使用 `Umi-OCR` 异步文档识别并逐页推送 `event: progress`（`page X/Y`），
/// 杜绝大扫描件 PDF 单请求长时间阻塞超时；出错时严格返回 4 类结构化错误码：
/// `ERR_FILE_TOO_LARGE` / `ERR_PDF_ENCRYPTED` / `ERR_FILE_CORRUPTED` / `ERR_OCR_UNAVAILABLE`。
pub async fn api_upload_extract_stream(
    State(s): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut file_bytes: Vec<u8> = Vec::new();
    let mut file_name = String::new();
    let mut precheck_err: Option<StructuredUploadError> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            file_name = field.file_name().unwrap_or("unknown.txt").to_lowercase();
            match field.bytes().await {
                Ok(data) => {
                    if data.len() > MAX_FILE_SIZE {
                        precheck_err = Some(StructuredUploadError::file_too_large(10));
                        break;
                    }
                    file_bytes = data.to_vec();
                }
                Err(_) => {
                    precheck_err = Some(StructuredUploadError::file_corrupted("文件读取失败"));
                    break;
                }
            }
        }
    }

    if precheck_err.is_none() && file_bytes.is_empty() {
        precheck_err = Some(StructuredUploadError::file_corrupted("缺少文件或文件为空"));
    }

    let stream = async_stream::stream! {
        if let Some(err) = precheck_err {
            yield Ok(Event::default().event("error").data(err.to_json().to_string()));
            yield Ok(Event::default().event("done").data("[DONE]"));
            return;
        }

        let ext_from_name = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
        let ext = match detect_type_by_magic(&file_bytes) {
            Some(magic_type) => magic_type.to_string(),
            None => ext_from_name,
        };
        let is_image = matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp"
        );

        let mut total_pages = 1usize;
        if ext == "pdf" {
            match inspect_pdf_integrity(&file_bytes) {
                Ok(pages) => total_pages = pages,
                Err(err) => {
                    yield Ok(Event::default().event("error").data(err.to_json().to_string()));
                    yield Ok(Event::default().event("done").data("[DONE]"));
                    return;
                }
            }
        }

        let init_evt = OcrProgressEvent {
            stage: "inspect".to_string(),
            page: 0,
            total_pages,
            percent: 5,
            message: format!("正在解析 {}（共 {} 页）...", ext.to_uppercase(), total_pages),
        };
        yield Ok(Event::default().event("progress").data(serde_json::to_string(&init_evt).unwrap_or_default()));

        let mut method = "text_layer".to_string();
        let extracted: Result<String, StructuredUploadError> = if is_image {
            method = "image_ocr".to_string();
            let ai_client = s
                .config
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .ai_client();
            describe_image_with_fallback(&ai_client, &file_bytes, &ext)
                .await
                .map_err(StructuredUploadError::ocr_unavailable)
        } else if ext == "pdf" {
            // 1. 先尝试无 OCR 快速文本层提取
            let quick_text = extract_pdf_text_fast_layers(&file_bytes).await;
            if let Ok(t) = quick_text {
                if t.trim().len() >= 100 {
                    let done_evt = OcrProgressEvent {
                        stage: "text_layer".to_string(),
                        page: total_pages,
                        total_pages,
                        percent: 95,
                        message: format!("已从文本层快速提取 {} 页内容", total_pages),
                    };
                    yield Ok(Event::default().event("progress").data(serde_json::to_string(&done_evt).unwrap_or_default()));
                    Ok(t)
                } else {
                    // 文本层不足，进入流式 Umi-OCR
                    method = "umi_ocr".to_string();
                    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<OcrProgressEvent>();
                    let pdf_for_ocr = file_bytes.clone();
                    let base_url = umi_ocr_base_url();
                    let mut ocr_handle = tokio::spawn(async move {
                        extract_pdf_text_umi_ocr_with_progress(
                            pdf_for_ocr,
                            &base_url,
                            total_pages,
                            Some(tx),
                        )
                        .await
                    });

                    while let Some(evt) = rx.recv().await {
                        yield Ok(Event::default().event("progress").data(serde_json::to_string(&evt).unwrap_or_default()));
                    }

                    match (&mut ocr_handle).await {
                        Ok(Ok(ocr_text)) if !ocr_text.trim().is_empty() => Ok(ocr_text),
                        Ok(Err(umi_err)) => {
                            // Umi-OCR 不可用或失败时依次回退本地 Tesseract -> AI 视觉
                            let fb_evt = OcrProgressEvent {
                                stage: "fallback".to_string(),
                                page: 0,
                                total_pages,
                                percent: 40,
                                message: "Umi-OCR 未命中，尝试本地 OCR / 视觉模型兜底...".to_string(),
                            };
                            yield Ok(Event::default().event("progress").data(serde_json::to_string(&fb_evt).unwrap_or_default()));

                            let data_for_tess = file_bytes.clone();
                            let tess_res = tokio::task::spawn_blocking(move || extract_pdf_text_ocr(&data_for_tess))
                                .await
                                .unwrap_or_else(|_| Err("Tesseract task panicked".into()));
                            if let Ok(t) = tess_res {
                                if !t.trim().is_empty() {
                                    method = "tesseract".to_string();
                                    Ok(t)
                                } else {
                                    Err(StructuredUploadError::ocr_unavailable(umi_err))
                                }
                            } else {
                                let is_deepseek = {
                                    let cfg = s.config.read().unwrap_or_else(|e| e.into_inner());
                                    cfg.ai_base_url.contains("deepseek")
                                };
                                if is_deepseek {
                                    Err(StructuredUploadError::ocr_unavailable(format!(
                                        "{umi_err}；且 DeepSeek 不支持图片视觉兜底"
                                    )))
                                } else {
                                    let ai_client = s
                                        .config
                                        .read()
                                        .unwrap_or_else(|e| e.into_inner())
                                        .ai_client();
                                    match extract_pdf_via_ai_vision(&file_bytes, &ai_client).await {
                                        Ok(v) if !v.trim().is_empty() => {
                                            method = "ai_vision".to_string();
                                            Ok(v)
                                        }
                                        Ok(_) | Err(_) => Err(StructuredUploadError::ocr_unavailable(umi_err)),
                                    }
                                }
                            }
                        }
                        _ => Err(StructuredUploadError::ocr_unavailable("Umi-OCR 异步任务异常中止")),
                    }
                }
            } else {
                // 快速文本层失败，直接走流式 Umi-OCR + 兜底
                method = "umi_ocr".to_string();
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<OcrProgressEvent>();
                let pdf_for_ocr = file_bytes.clone();
                let base_url = umi_ocr_base_url();
                let mut ocr_handle = tokio::spawn(async move {
                    extract_pdf_text_umi_ocr_with_progress(
                        pdf_for_ocr,
                        &base_url,
                        total_pages,
                        Some(tx),
                    )
                    .await
                });

                while let Some(evt) = rx.recv().await {
                    yield Ok(Event::default().event("progress").data(serde_json::to_string(&evt).unwrap_or_default()));
                }

                match (&mut ocr_handle).await {
                    Ok(Ok(ocr_text)) if !ocr_text.trim().is_empty() => Ok(ocr_text),
                    Ok(Err(umi_err)) => {
                        let data_for_tess = file_bytes.clone();
                        let tess_res = tokio::task::spawn_blocking(move || extract_pdf_text_ocr(&data_for_tess))
                            .await
                            .unwrap_or_else(|_| Err("Tesseract task panicked".into()));
                        if let Ok(t) = tess_res {
                            if !t.trim().is_empty() {
                                method = "tesseract".to_string();
                                Ok(t)
                            } else {
                                Err(StructuredUploadError::ocr_unavailable(umi_err))
                            }
                        } else {
                            let is_deepseek = {
                                let cfg = s.config.read().unwrap_or_else(|e| e.into_inner());
                                cfg.ai_base_url.contains("deepseek")
                            };
                            if is_deepseek {
                                Err(StructuredUploadError::ocr_unavailable(format!(
                                    "{umi_err}；且 DeepSeek 不支持图片视觉兜底"
                                )))
                            } else {
                                let ai_client = s
                                    .config
                                    .read()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .ai_client();
                                match extract_pdf_via_ai_vision(&file_bytes, &ai_client).await {
                                    Ok(v) if !v.trim().is_empty() => {
                                        method = "ai_vision".to_string();
                                        Ok(v)
                                    }
                                    Ok(_) | Err(_) => Err(StructuredUploadError::ocr_unavailable(umi_err)),
                                }
                            }
                        }
                    }
                    _ => Err(StructuredUploadError::ocr_unavailable("Umi-OCR 异步任务异常中止")),
                }
            }
        } else if ext == "docx" {
            method = "docx_xml".to_string();
            extract_docx_text(&file_bytes)
                .map_err(|e| StructuredUploadError::file_corrupted(format!("DOCX 解析失败: {e}")))
                .and_then(|t| {
                    if t.trim().is_empty() {
                        Err(StructuredUploadError::file_corrupted("DOCX 无可提取文字"))
                    } else {
                        Ok(t)
                    }
                })
        } else if ext == "doc" {
            Err(StructuredUploadError::file_corrupted(
                "暂不支持旧版 .doc 格式，请另存为 .docx 或 .pdf",
            ))
        } else {
            method = "plain_text".to_string();
            match String::from_utf8(file_bytes.clone()) {
                Ok(t) => Ok(t),
                Err(_) => {
                    let (t, _, had_errors) = encoding_rs::GBK.decode(&file_bytes);
                    if had_errors {
                        Err(StructuredUploadError::file_corrupted(
                            "无法识别文件类型，请上传 PDF、DOCX、TXT 或图片文件",
                        ))
                    } else {
                        Ok(t.into_owned())
                    }
                }
            }
        };

        match extracted {
            Ok(text) => {
                const MAX_EXTRACT_CHARS: usize = 50_000;
                let total_chars = text.chars().count();
                let truncated = total_chars > MAX_EXTRACT_CHARS;
                let payload = json!({
                    "text": truncate_for_ai(&text, MAX_EXTRACT_CHARS),
                    "file_type": ext,
                    "length": text.len(),
                    "char_length": total_chars,
                    "truncated": truncated,
                    "method": method,
                    "total_pages": total_pages,
                });
                yield Ok(Event::default().event("result").data(payload.to_string()));
            }
            Err(err) => {
                yield Ok(Event::default().event("error").data(err.to_json().to_string()));
            }
        }
        yield Ok(Event::default().event("done").data("[DONE]"));
    };

    Sse::new(stream)
}

/// 仅执行快速文本层提取（pdf-extract -> by-pages -> pdftotext -> PyMuPDF），不触发慢速 OCR
async fn extract_pdf_text_fast_layers(data: &[u8]) -> Result<String, String> {
    if let Ok(text) = pdf_extract::extract_text_from_mem(data) {
        let trimmed = text.trim();
        let is_error = trimmed.contains("MuPDF error")
            || trimmed.contains("mupdf error")
            || trimmed.contains("zlib error")
            || trimmed.contains("PDF error");
        if !trimmed.is_empty() && !is_error {
            return Ok(text);
        }
    }
    if let Ok(text) = extract_pdf_text_by_pages(data) {
        let trimmed = text.trim();
        let is_error = trimmed.contains("MuPDF error")
            || trimmed.contains("mupdf error")
            || trimmed.contains("zlib error")
            || trimmed.contains("PDF error");
        if !trimmed.is_empty() && !is_error {
            return Ok(text);
        }
    }
    {
        let data_vec = data.to_vec();
        let result = tokio::task::spawn_blocking(move || extract_pdf_text_pdftotext(&data_vec))
            .await
            .unwrap_or_else(|_| Err("pdftotext task panicked".into()));
        if let Ok(text) = result {
            let trimmed = text.trim();
            let is_error = trimmed.contains("MuPDF error")
                || trimmed.contains("mupdf error")
                || trimmed.contains("zlib error")
                || trimmed.contains("PDF error");
            if !trimmed.is_empty() && !is_error {
                return Ok(text);
            }
        }
    }
    {
        let data_vec = data.to_vec();
        let result = tokio::task::spawn_blocking(move || extract_pdf_text_pymupdf(&data_vec))
            .await
            .unwrap_or_else(|_| Err("PyMuPDF task panicked".into()));
        if let Ok(text) = result {
            let trimmed = text.trim();
            let is_error = trimmed.contains("MuPDF error")
                || trimmed.contains("mupdf error")
                || trimmed.contains("zlib error")
                || trimmed.contains("PDF error");
            if !trimmed.is_empty() && !is_error {
                return Ok(text);
            }
        }
    }
    Err("PDF 无可提取文本层".into())
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

    // Convert PDF pages to PNG using PyMuPDF (max 10 pages, cross-platform & async)
    let python_candidates: Vec<String> = match std::env::var("INNOFORGE_PYTHON_BIN") {
        Ok(p) if !p.trim().is_empty() => vec![p.trim().to_string()],
        _ => vec!["python3".to_string(), "python".to_string()],
    };
    const CONVERT_PDF_PY: &str = r#"import sys, fitz
doc = fitz.open(sys.argv[1])
limit = min(len(doc), 10)
for i in range(limit):
    pix = doc[i].get_pixmap(dpi=150)
    pix.save(f"{sys.argv[2]}_{i}.png")
print(limit)
"#;

    let mut output = Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "python3/python not found in PATH",
    ));
    for py in &python_candidates {
        match tokio::process::Command::new(py)
            .args(["-c", CONVERT_PDF_PY, &tmp_pdf_str, &out_prefix_str])
            .kill_on_drop(true)
            .output()
            .await
        {
            Ok(o) => {
                output = Ok(o);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                output = Err(e);
                break;
            }
        }
    }

    let page_count: usize = match output {
        Ok(o) if o.status.success() => {
            let n = String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse()
                .unwrap_or(0);
            n
        }
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
        let trimmed = text.trim().to_string();
        let is_error = trimmed.contains("MuPDF error")
            || trimmed.contains("mupdf error")
            || trimmed.contains("zlib error")
            || trimmed.contains("PDF error");
        if !trimmed.is_empty() && !is_error {
            return Ok(text);
        }
    }
    // Step 2: Rust pdf-extract by-pages (better for multi-column Chinese patents)
    if let Ok(text) = extract_pdf_text_by_pages(data) {
        let trimmed = text.trim().to_string();
        let is_error = trimmed.contains("MuPDF error")
            || trimmed.contains("mupdf error")
            || trimmed.contains("zlib error")
            || trimmed.contains("PDF error");
        if !trimmed.is_empty() && !is_error {
            return Ok(text);
        }
    }
    // Step 3: pdftotext (poppler, handles malformed PDFs well)
    // §3.3: 外部进程调用用 spawn_blocking 避免阻塞 Tokio 异步运行时
    {
        let data_vec = data.to_vec();
        let result = tokio::task::spawn_blocking(move || extract_pdf_text_pdftotext(&data_vec))
            .await
            .unwrap_or_else(|_| Err("pdftotext task panicked".into()));
        if let Ok(text) = result {
            let trimmed = text.trim();
            let is_error = trimmed.contains("MuPDF error")
                || trimmed.contains("mupdf error")
                || trimmed.contains("zlib error")
                || trimmed.contains("PDF error");
            if !trimmed.is_empty() && !is_error {
                return Ok(text);
            }
        }
    }
    // Step 4: PyMuPDF (Python fitz)
    {
        let data_vec = data.to_vec();
        let result = tokio::task::spawn_blocking(move || extract_pdf_text_pymupdf(&data_vec))
            .await
            .unwrap_or_else(|_| Err("PyMuPDF task panicked".into()));
        if let Ok(text) = result {
            let trimmed = text.trim();
            let is_error = trimmed.contains("MuPDF error")
                || trimmed.contains("mupdf error")
                || trimmed.contains("zlib error")
                || trimmed.contains("PDF error");
            if !trimmed.is_empty() && !is_error {
                return Ok(text);
            }
        }
    }
    // Step 5: Umi-OCR 本地离线 OCR（优先调用已配置的 Umi-OCR，高精度中文识别）
    let data_vec = data.to_vec();
    if let Ok(text) = extract_pdf_text_umi_ocr(data_vec).await {
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }
    // Step 6: Tesseract OCR 回退 (handles scanned/special font PDFs when Umi-OCR is unavailable)
    // §3.3: OCR 是最耗时的阻塞调用，必须用 spawn_blocking 移出异步线程
    {
        let data_vec = data.to_vec();
        let result = tokio::task::spawn_blocking(move || extract_pdf_text_ocr(&data_vec))
            .await
            .unwrap_or_else(|_| Err("OCR task panicked".into()));
        if let Ok(text) = result {
            let trimmed = text.trim();
            let is_error = trimmed.contains("MuPDF error")
                || trimmed.contains("mupdf error")
                || trimmed.contains("zlib error")
                || trimmed.contains("PDF error");
            if !trimmed.is_empty() && !is_error {
                return Ok(text);
            }
        }
    }
    // Step 7: MinerU 云端 API（OCR+版面还原，中文专利优化）
    {
        let data_vec = data.to_vec();
        let result = tokio::task::spawn_blocking(move || extract_pdf_text_mineru(&data_vec))
            .await
            .unwrap_or_else(|_| Err("MinerU task panicked".into()));
        if let Ok(text) = result {
            if !text.trim().is_empty() {
                return Ok(text);
            }
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

    let python = resolve_python_bin();
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

    let python = resolve_python_bin();
    let output = std::process::Command::new(&python)
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

    let python = resolve_python_bin();
    let script = r#"
import pytesseract, fitz, sys
from PIL import Image
import io

sys.stdout.reconfigure(encoding='utf-8')
doc = fitz.open(sys.argv[1])
for page in doc:
    mat = fitz.Matrix(2.0, 2.0)
    pix = page.get_pixmap(matrix=mat)
    img = Image.open(io.BytesIO(pix.tobytes('png')))
    text = pytesseract.image_to_string(img, lang='chi_sim+eng')
    print(text)
"#;

    let output = std::process::Command::new(&python)
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
pub(crate) async fn extract_pdf_text_umi_ocr(data: Vec<u8>) -> Result<String, String> {
    let base_url = umi_ocr_base_url();
    extract_pdf_text_umi_ocr_with_base_url(data, &base_url).await
}

pub(crate) async fn extract_pdf_text_umi_ocr_with_base_url(
    data: Vec<u8>,
    base_url: &str,
) -> Result<String, String> {
    extract_pdf_text_umi_ocr_with_progress(data, base_url, 1, None).await
}

/// 带分页进度回调的 Umi-OCR 文档识别（供同步与 T2.5 SSE 流式接口共用唯一实现）
pub(crate) async fn extract_pdf_text_umi_ocr_with_progress(
    data: Vec<u8>,
    base_url: &str,
    known_total_pages: usize,
    progress_tx: Option<tokio::sync::mpsc::UnboundedSender<OcrProgressEvent>>,
) -> Result<String, String> {
    let base_url = base_url.trim().trim_end_matches('/');
    let probe_addr = probe_addr_for(base_url);
    let probe = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::net::TcpStream::connect(&probe_addr),
    )
    .await;
    if probe.is_err() || probe.unwrap().is_err() {
        return Err(format!(
            "Umi-OCR 未运行（{probe_addr} 不可达）。需要启动 Umi-OCR 或改用可复制文本的 PDF"
        ));
    }

    if let Some(ref tx) = progress_tx {
        let _ = tx.send(OcrProgressEvent {
            stage: "umi_ocr".to_string(),
            page: 0,
            total_pages: known_total_pages.max(1),
            percent: 10,
            message: format!(
                "已连接 Umi-OCR（{base_url}），正在上传扫描件（共 {} 页）...",
                known_total_pages.max(1)
            ),
        });
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(UMI_OCR_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Umi-OCR 客户端创建失败: {}", e))?;

    let upload_url = format!("{}/api/doc/upload", base_url);
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

    // Step 3: Poll for result (up to 180s for large scanned patent PDFs)
    let result_url = format!("{}/api/doc/result", base_url);
    let mut all_text = String::new();
    let max_polls = 180;
    let mut last_reported_page = 0usize;
    for poll in 0..max_polls {
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
        let processed = poll_json["processed_count"]
            .as_u64()
            .map(|n| n as usize)
            .unwrap_or(last_reported_page);
        let total = poll_json["pages_count"]
            .as_u64()
            .map(|n| n as usize)
            .filter(|&n| n > 0)
            .unwrap_or_else(|| known_total_pages.max(1));

        // Collect incremental text
        if let Some(data_str) = poll_json["data"].as_str() {
            if !data_str.trim().is_empty() {
                all_text.push_str(data_str);
            }
        }

        if let Some(ref tx) = progress_tx {
            if processed > last_reported_page || is_done || poll % 3 == 0 {
                last_reported_page = processed;
                let effective_page = if is_done { total } else { processed.min(total) };
                let pct = ((effective_page * 85) / total.max(1) + 10).min(98) as u8;
                let _ = tx.send(OcrProgressEvent {
                    stage: "umi_ocr".to_string(),
                    page: effective_page,
                    total_pages: total,
                    percent: pct,
                    message: format!("Umi-OCR 正在识别第 {effective_page}/{total} 页..."),
                });
            }
        }

        if is_done || state == "failure" {
            break;
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Step 4: Cleanup — clear task on Umi-OCR
    let clear_url = format!("{}/api/doc/clear/{}", base_url, task_id);
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
        .post(format!("{}/api/ocr", umi_ocr_base_url()))
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
    if !has_pdf_header(&file_bytes) {
        return Json(json!({
            "error": "文件不是有效 PDF，未检测到 %PDF- 文件签名"
        }));
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
                _ => return Json(json!({ "error": format!("PDF 文本提取失败: {}", e) })),
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

/// Check if data starts with the ZIP magic bytes (PK\x03\x04), which is the
/// signature for .docx (and other Office Open XML formats).
fn has_docx_header(data: &[u8]) -> bool {
    data.len() >= 4 && data[0..4] == [0x50, 0x4B, 0x03, 0x04]
}

/// Detect file type by content magic bytes when the filename extension is
/// missing or unreliable (e.g. non-ASCII filenames mangled by multipart parsers).
///
/// Returns `Some("pdf")` / `Some("docx")` when magic bytes match, `None` when
/// no known magic is recognised.
fn detect_type_by_magic(data: &[u8]) -> Option<&'static str> {
    if has_pdf_header(data) {
        return Some("pdf");
    }
    if has_docx_header(data) {
        return Some("docx");
    }
    None
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
        assert!(!has_pdf_header(b"plain text disguised as a PDF"));
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

/// §6.1 新增：文件解析器纯函数测试
#[cfg(test)]
mod file_parser_tests {
    use super::{
        append_pdf_chunk, has_pdf_header, is_ip_literal, validate_pdf_content_length,
        MAX_PDF_STORE_SIZE,
    };

    #[test]
    fn is_ip_literal_recognizes_ipv4() {
        assert!(is_ip_literal("1.2.3.4"));
        assert!(is_ip_literal("255.255.255.255"));
    }

    #[test]
    fn is_ip_literal_recognizes_ipv6_bracketed() {
        assert!(is_ip_literal("[::1]"));
        assert!(is_ip_literal("[2001:db8::1]"));
    }

    #[test]
    fn is_ip_literal_recognizes_ipv6_unbracketed() {
        assert!(is_ip_literal("::1"));
        assert!(is_ip_literal("2001:db8::1"));
    }

    #[test]
    fn is_ip_literal_rejects_hostname() {
        assert!(!is_ip_literal("example.com"));
        assert!(!is_ip_literal("patent.google.com"));
        assert!(!is_ip_literal("localhost"));
    }

    #[test]
    fn is_ip_literal_rejects_empty() {
        assert!(!is_ip_literal(""));
    }

    #[test]
    fn has_pdf_header_empty_input() {
        assert!(!has_pdf_header(b""));
    }

    #[test]
    fn has_pdf_header_exact_boundary() {
        // %PDF- 正好在 1024 字节窗口末尾
        let mut data = vec![b' '; 1019];
        data.extend_from_slice(b"%PDF-");
        assert!(has_pdf_header(&data));
    }

    #[test]
    fn has_pdf_header_just_past_boundary() {
        // %PDF- 超出 1024 字节窗口
        let mut data = vec![b' '; 1021];
        data.extend_from_slice(b"%PDF-");
        assert!(!has_pdf_header(&data));
    }

    #[test]
    fn validate_pdf_content_length_none_is_ok() {
        assert!(validate_pdf_content_length(None).is_ok());
    }

    #[test]
    fn validate_pdf_content_length_zero_is_ok() {
        assert!(validate_pdf_content_length(Some(0)).is_ok());
    }

    #[test]
    fn validate_pdf_content_length_exactly_max_is_ok() {
        assert!(validate_pdf_content_length(Some(MAX_PDF_STORE_SIZE as u64)).is_ok());
    }

    #[test]
    fn validate_pdf_content_length_over_max_is_err() {
        assert!(validate_pdf_content_length(Some(MAX_PDF_STORE_SIZE as u64 + 1)).is_err());
    }

    #[test]
    fn append_pdf_chunk_empty_buffer() {
        let mut buf = Vec::new();
        assert!(append_pdf_chunk(&mut buf, b"%PDF-1.7").is_ok());
        assert_eq!(buf, b"%PDF-1.7");
    }

    #[test]
    fn append_pdf_chunk_overflow_rejected() {
        let mut buf = vec![0u8; MAX_PDF_STORE_SIZE - 1];
        // 恰好填满
        assert!(append_pdf_chunk(&mut buf, &[1]).is_ok());
        // 超出
        assert!(append_pdf_chunk(&mut buf, &[2]).is_err());
    }
}

#[cfg(test)]
mod umi_ocr_config_tests {
    use super::*;

    #[test]
    fn umi_ocr_base_url_resolution() {
        // 环境变量未设置 → 出厂默认
        assert_eq!(resolve_umi_ocr_base_url(None), "http://127.0.0.1:1224");
        // 设置 → 覆盖（含去尾斜杠）
        assert_eq!(
            resolve_umi_ocr_base_url(Some("http://192.168.1.8:1224/")),
            "http://192.168.1.8:1224"
        );
        // 空串/纯空白 → 视为未设置（不产生空 URL）
        assert_eq!(resolve_umi_ocr_base_url(Some("")), "http://127.0.0.1:1224");
        assert_eq!(
            resolve_umi_ocr_base_url(Some("   ")),
            "http://127.0.0.1:1224"
        );
    }

    #[test]
    fn probe_addr_follows_base_url() {
        assert_eq!(probe_addr_for("http://127.0.0.1:1224"), "127.0.0.1:1224");
        assert_eq!(
            probe_addr_for("http://192.168.1.8:1224"),
            "192.168.1.8:1224"
        );
        assert_eq!(
            probe_addr_for("http://ocr.lan:9000/api"),
            "ocr.lan:9000",
            "带路径的地址取 host:port"
        );
        assert_eq!(
            probe_addr_for("127.0.0.1:1224"),
            "127.0.0.1:1224",
            "无 scheme 也能取"
        );
    }

    #[test]
    fn t25_structured_upload_error_codes_classify_all_four_cases() {
        // 1. 损坏文件（无 %PDF- 签名 / 截断缺 %%EOF）
        let corrupt = inspect_pdf_integrity(b"not a pdf file").expect_err("corrupted must fail");
        assert_eq!(corrupt.error_code, ERR_FILE_CORRUPTED);
        assert!(!corrupt.hint.is_empty());

        let truncated =
            inspect_pdf_integrity(b"%PDF-1.7\n1 0 obj\n<<>>\nendobj").expect_err("truncated");
        assert_eq!(truncated.error_code, ERR_FILE_CORRUPTED);

        // 2. 加密 PDF（含 /Encrypt 字典项）
        let encrypted = inspect_pdf_integrity(
            b"%PDF-1.7\n1 0 obj\n<< /Filter /Standard /V 2 >>\nendobj\ntrailer\n<< /Encrypt 1 0 R >>\n%%EOF",
        )
        .expect_err("encrypted PDF must fail");
        assert_eq!(encrypted.error_code, ERR_PDF_ENCRYPTED);

        // 3. 超体积上限
        let too_large = StructuredUploadError::file_too_large(10);
        assert_eq!(too_large.error_code, ERR_FILE_TOO_LARGE);

        // 4. OCR 不可用
        let ocr_err = StructuredUploadError::ocr_unavailable("Umi-OCR unreachable");
        assert_eq!(ocr_err.error_code, ERR_OCR_UNAVAILABLE);
        assert!(ocr_err.hint.contains("Umi-OCR"));
    }

    #[tokio::test]
    async fn t25_umi_ocr_progress_stream_emits_page_updates_and_text() {
        use axum::{routing::get, routing::post, Router};
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };

        let poll_counter = Arc::new(AtomicUsize::new(0));
        let poll_counter_clone = poll_counter.clone();
        let app = Router::new()
            .route(
                "/api/doc/upload",
                post(|| async { Json(json!({ "code": 100, "data": "mock-task-42" })) }),
            )
            .route(
                "/api/doc/result",
                post(move || {
                    let c = poll_counter_clone.clone();
                    async move {
                        let step = c.fetch_add(1, Ordering::SeqCst);
                        if step == 0 {
                            Json(json!({
                                "code": 100,
                                "is_done": false,
                                "state": "running",
                                "processed_count": 1,
                                "pages_count": 3,
                                "data": "第1页：固态电解质隔膜"
                            }))
                        } else {
                            Json(json!({
                                "code": 100,
                                "is_done": true,
                                "state": "success",
                                "processed_count": 3,
                                "pages_count": 3,
                                "data": "\n第2-3页：说明书附图与实施例"
                            }))
                        }
                    }
                }),
            )
            .route(
                "/api/doc/clear/:id",
                get(|| async { Json(json!({ "code": 100 })) }),
            );

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock umi-ocr");
        let addr = listener.local_addr().expect("local addr");
        let server = tokio::spawn(async move {
            let _ = axum::Server::from_tcp(listener)
                .expect("server")
                .serve(app.into_make_service())
                .await;
        });

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let base_url = format!("http://{addr}");
        let text = extract_pdf_text_umi_ocr_with_progress(
            b"%PDF-1.4\n%%EOF".to_vec(),
            &base_url,
            3,
            Some(tx),
        )
        .await
        .expect("umi-ocr mock should succeed");

        let mut events = Vec::new();
        while let Ok(evt) = rx.try_recv() {
            events.push(evt);
        }
        server.abort();

        assert!(text.contains("固态电解质隔膜"));
        assert!(text.contains("说明书附图与实施例"));
        assert!(
            events.iter().any(|e| e.page == 1 && e.total_pages == 3),
            "must emit page 1/3 progress, got: {:?}",
            events
        );
        assert!(
            events.iter().any(|e| e.page == 3 && e.total_pages == 3),
            "must emit page 3/3 progress, got: {:?}",
            events
        );
    }
}
