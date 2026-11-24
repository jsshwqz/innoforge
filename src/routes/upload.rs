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
    const MAX_COMPARE_FILES: usize = 5;
    let mut uploaded_raw: Vec<(String, Vec<u8>)> = Vec::new();
    let mut patent_id = String::new();
    let mut compare_focus = String::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();

        if name == "file" || name == "files" {
            if uploaded_raw.len() >= MAX_COMPARE_FILES {
                return Json(json!({
                    "error": format!("最多支持同时上传 {} 份对比文件", MAX_COMPARE_FILES)
                }));
            }
            let fname = field
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("document_{}.txt", uploaded_raw.len() + 1));
            match field.bytes().await {
                Ok(data) => {
                    if data.len() > MAX_FILE_SIZE {
                        return Json(json!({
                            "error": format!("文件「{}」大小超过 10MB 限制", fname)
                        }));
                    }
                    if !data.is_empty() {
                        uploaded_raw.push((fname, data.to_vec()));
                    }
                }
                Err(_) => return Json(json!({"error": format!("文件「{}」读取失败", fname)})),
            }
        } else if name == "patent_id" {
            if let Ok(text) = field.text().await {
                patent_id = text;
            }
        } else if name == "compare_focus" {
            if let Ok(text) = field.text().await {
                compare_focus = text.trim().to_string();
            }
        }
    }

    if uploaded_raw.is_empty() || patent_id.is_empty() {
        return Json(json!({"error": "缺少对比文件或专利 ID"}));
    }

    let patent = match s.db.get_patent(&patent_id) {
        Ok(Some(p)) => p,
        _ => return Json(json!({"error": "专利不存在"})),
    };

    let mut extracted_docs: Vec<UploadedCompareDoc> = Vec::new();
    let mut first_ext = String::new();
    let mut total_content_len = 0usize;

    for (raw_name, file_bytes) in uploaded_raw {
        let lower_name = raw_name.to_lowercase();
        let ext_from_name = lower_name.rsplit('.').next().unwrap_or("").to_lowercase();
        // #97: filename may be missing or mangled for non-ASCII names.
        let ext = match detect_type_by_magic(&file_bytes) {
            Some(magic_type) => magic_type.to_string(),
            None => ext_from_name,
        };
        if ext == "pdf" && !has_pdf_header(&file_bytes) {
            return Json(json!({
                "error": format!("文件「{}」不是有效 PDF，未检测到 %PDF- 文件签名", raw_name)
            }));
        }

        let is_image = matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp"
        );

        let file_content = if is_image {
            let ai_client = s
                .config
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .ai_client();
            match describe_image_with_fallback(&ai_client, &file_bytes, &ext).await {
                Ok(description) => description,
                Err(e) => {
                    return Json(json!({
                        "error": format!("图片「{}」识别失败: {}", raw_name, e)
                    }))
                }
            }
        } else if ext == "pdf" {
            match extract_pdf_text(&file_bytes).await {
                Ok(t) if t.trim().len() >= 100 => t,
                _ => {
                    let is_deepseek = {
                        let cfg = s.config.read().unwrap_or_else(|e| e.into_inner());
                        cfg.ai_base_url.contains("deepseek")
                    };
                    if is_deepseek {
                        return Json(json!({
                            "error": format!(
                                "PDF「{}」文字提取失败。当前 AI 为 DeepSeek 不支持图片识别，\n建议：1) 上传可编辑文本(.txt/.docx) 2) 在设置页切换至 Gemini 后重试 3) 直接粘贴文字内容",
                                raw_name
                            )
                        }));
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
                                "error": format!("PDF「{}」提取失败（含 AI 视觉兜底）: {}", raw_name, e)
                            }))
                        }
                    }
                }
            }
        } else if ext == "docx" {
            match extract_docx_text(&file_bytes) {
                Ok(text) if !text.trim().is_empty() => text,
                Ok(_) => {
                    return Json(json!({
                        "error": format!("DOCX 文件「{}」无可提取的文字内容", raw_name)
                    }))
                }
                Err(e) => {
                    return Json(json!({
                        "error": format!("DOCX「{}」解析失败: {}", raw_name, e)
                    }))
                }
            }
        } else if ext == "pptx" {
            match extract_pptx_text(&file_bytes) {
                Ok(text) if !text.trim().is_empty() => text,
                Ok(_) => {
                    return Json(json!({
                        "error": format!("PPTX 文件「{}」无可提取的幻灯片文字", raw_name)
                    }))
                }
                Err(e) => {
                    return Json(json!({
                        "error": format!("PPTX「{}」解析失败: {}", raw_name, e)
                    }))
                }
            }
        } else if ext == "xlsx" {
            match extract_xlsx_text(&file_bytes) {
                Ok(text) if !text.trim().is_empty() => text,
                Ok(_) => {
                    return Json(json!({
                        "error": format!("XLSX 表格「{}」无可提取的单元格内容", raw_name)
                    }))
                }
                Err(e) => {
                    return Json(json!({
                        "error": format!("XLSX「{}」解析失败: {}", raw_name, e)
                    }))
                }
            }
        } else if ext == "ofd" {
            match extract_ofd_text(&file_bytes) {
                Ok(text) if !text.trim().is_empty() => text,
                Ok(_) => {
                    return Json(json!({
                        "error": format!("OFD 文档「{}」无可提取文字（若为纯扫描件请另存 PDF 后上传）", raw_name)
                    }))
                }
                Err(e) => {
                    return Json(json!({
                        "error": format!("OFD「{}」解析失败: {}", raw_name, e)
                    }))
                }
            }
        } else if ext == "stl" {
            match extract_stl_summary(&file_bytes) {
                Ok(text) if !text.trim().is_empty() => text,
                Ok(_) => {
                    return Json(json!({
                        "error": format!("STL 三维文件「{}」未提取到有效三角面片", raw_name)
                    }))
                }
                Err(e) => {
                    return Json(json!({
                        "error": format!("STL「{}」解析失败: {}", raw_name, e)
                    }))
                }
            }
        } else if let Some(hint) = unsupported_proprietary_format_hint(&ext, &raw_name) {
            return Json(json!({ "error": hint }));
        } else {
            let decoded = match String::from_utf8(file_bytes.clone()) {
                Ok(text) => text,
                Err(_) => {
                    let (text, _encoding, had_errors) = encoding_rs::GBK.decode(&file_bytes);
                    if had_errors {
                        return Json(json!({
                            "error": format!("无法识别文件「{}」类型，请上传 PDF、DOCX、PPTX、XLSX、OFD、TXT/MD/LaTeX/HTML 或图片文件", raw_name)
                        }));
                    }
                    text.into_owned()
                }
            };
            postprocess_text_by_ext(&ext, &decoded)
        };

        if file_content.trim().is_empty() {
            return Json(json!({
                "error": format!("文件「{}」内容为空", raw_name)
            }));
        }
        if first_ext.is_empty() {
            first_ext = ext.clone();
        }
        total_content_len += file_content.len();
        let doc_kind = infer_uploaded_doc_kind(&raw_name, &file_content, is_image);
        extracted_docs.push(UploadedCompareDoc {
            file_name: raw_name,
            doc_kind: doc_kind.to_string(),
            ext,
            content: file_content,
        });
    }

    if let Err(breaker_msg) = s.db.check_ai_budget_circuit_breaker(None) {
        return Json(json!({ "error": breaker_msg }));
    }

    let ai_client = s
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ai_client_expert();

    let (system_prompt, prompt) = build_multi_upload_compare_prompt(
        &patent,
        &extracted_docs,
        if compare_focus.is_empty() {
            None
        } else {
            Some(compare_focus.as_str())
        },
    );

    let files_meta: Vec<serde_json::Value> = extracted_docs
        .iter()
        .map(|d| {
            json!({
                "file_name": d.file_name,
                "doc_kind": d.doc_kind,
                "file_type": d.ext,
                "content_length": d.content.len()
            })
        })
        .collect();

    match ai_client.chat(&prompt, Some(&system_prompt)).await {
        Ok(response) => {
            let _ =
                s.db.log_ai_call(&ai_client, "upload-compare-deep", None, None);
            Json(json!({
                "success": true,
                "analysis": response,
                "file_type": first_ext,
                "file_count": extracted_docs.len(),
                "files": files_meta,
                "content_length": total_content_len
            }))
        }
        Err(e) => Json(json!({ "error": format!("AI 分析失败: {}", e) })),
    }
}

struct UploadedCompareDoc {
    file_name: String,
    doc_kind: String,
    ext: String,
    content: String,
}

/// 根据文件名与正文特征自动识别对比文献体裁（专利 / 期刊学术论文 / 图片图纸 / 技术方案文档）
fn infer_uploaded_doc_kind(file_name: &str, content: &str, is_image: bool) -> &'static str {
    if is_image {
        return "图片/图纸识别内容";
    }
    let lower_name = file_name.to_lowercase();
    let sample: String = content.chars().take(2500).collect();
    let lower_sample = sample.to_lowercase();
    if lower_name.ends_with(".step")
        || lower_name.ends_with(".stp")
        || lower_name.ends_with(".stl")
        || lower_name.ends_with(".iges")
        || lower_name.ends_with(".igs")
        || sample.contains("【STEP 三维 CAD 模型结构化解析】")
        || sample.contains("【STL 三维网格模型解析】")
        || sample.contains("【IGES 三维 CAD 模型解析】")
    {
        return "三维 CAD 模型";
    }
    if lower_sample.contains("doi:")
        || (lower_sample.contains("10.")
            && (lower_sample.contains("abstract")
                || lower_sample.contains("keywords")
                || lower_sample.contains("references")
                || lower_sample.contains("journal")
                || lower_sample.contains("ieee")
                || lower_sample.contains("elsevier")
                || lower_sample.contains("springer")))
        || sample.contains("参考文献")
        || sample.contains("基金项目")
        || sample.contains("收稿日期")
        || sample.contains("中图分类号")
        || sample.contains("期刊")
        || lower_name.contains("paper")
        || lower_name.contains("journal")
        || lower_name.contains("论文")
        || lower_name.contains("期刊")
    {
        "期刊/学术论文"
    } else if sample.contains("权利要求书")
        || sample.contains("说明书附图")
        || sample.contains("公开号")
        || sample.contains("申请号")
        || lower_sample.contains("what is claimed is")
        || lower_name.contains("patent")
        || lower_name.contains("专利")
    {
        "对比专利文献"
    } else {
        "自定义技术文档/交底书"
    }
}

/// 单文件上传对比提示词入口（委托给多文献统一构造器，保持调用兼容）
#[cfg(test)]
fn build_upload_compare_prompt(
    patent: &crate::patent::Patent,
    file_type_label: &str,
    file_content: &str,
) -> (String, String) {
    let doc = UploadedCompareDoc {
        file_name: file_type_label.to_string(),
        doc_kind: file_type_label.to_string(),
        ext: "txt".to_string(),
        content: file_content.to_string(),
    };
    build_multi_upload_compare_prompt(patent, &[doc], None)
}

/// 构造专利详情页「多文件自定义对比（支持专利 + 期刊论文 + 技术文档 + 图纸混合）」的全文档深度提示词
fn build_multi_upload_compare_prompt(
    patent: &crate::patent::Patent,
    docs: &[UploadedCompareDoc],
    compare_focus: Option<&str>,
) -> (String, String) {
    let patent_signals =
        crate::ai::patent::extract_whole_document_signals(&patent.claims, &patent.description);

    let legal_precheck = if crate::ai::fto_scanner::is_patent_inactive(&patent.legal_status) {
        format!(
            "【程序预检 · 失效/未授权状态：{}】该专利已失效、终止、撤回或被驳回，原则上不构成有效字面侵权风险（仅作现有技术与公知设计参考）。",
            patent.legal_status.trim()
        )
    } else if patent.legal_status.trim().is_empty() {
        "未标注法律状态（请提示用户核实专利局最新有效状态后再下最终 FTO 放行结论）".to_string()
    } else {
        format!("有效/在审状态：{}", patent.legal_status.trim())
    };

    let patent_excerpt = crate::ai::patent::structured_patent_excerpt(
        &format!(
            "专利号：{}\n标题：{}\n申请人：{}\nIPC分类：{}\n法律状态：{}\n摘要：{}\n\n{}\n\n权利要求书全文：\n{}\n\n说明书（前部分）：\n{}",
            patent.patent_number,
            patent.title,
            patent.applicant,
            patent.ipc_codes,
            legal_precheck,
            patent.abstract_text,
            patent_signals.format_prompt_block(),
            patent.claims,
            patent.description
        ),
        22_000,
    );

    let doc_count = docs.len().max(1);
    let per_doc_budget = (26_000 / doc_count).max(4_500);
    let mut has_academic_doc = false;
    let mut docs_blocks = Vec::with_capacity(docs.len());

    for (idx, doc) in docs.iter().enumerate() {
        if doc.doc_kind.contains("论文") || doc.doc_kind.contains("期刊") {
            has_academic_doc = true;
        }
        let doc_signals = crate::ai::patent::extract_whole_document_signals("", &doc.content);
        let excerpt = crate::ai::patent::structured_patent_excerpt(
            &format!(
                "{}\n\n原文内容：\n{}",
                doc_signals.format_prompt_block(),
                doc.content
            ),
            per_doc_budget,
        );
        docs_blocks.push(format!(
            "【对比文献 D{}｜体裁：{}｜文件名：{}】\n{}",
            idx + 1,
            doc.doc_kind,
            doc.file_name,
            excerpt
        ));
    }

    let focus_clause = match compare_focus.map(str::trim).filter(|s| !s.is_empty()) {
        Some(f) => format!(
            "\n【用户自定义重点对比诉求】\n<user_input>{f}</user_input>\n请务必在报告中正面、定量地优先回应上述自定义诉求。\n"
        ),
        None => String::new(),
    };

    let academic_clause = if has_academic_doc {
        "\n- **期刊/学术论文专项交叉分析**：明确对比论文的理论模型、实验平台、实测性能数据（标注原文真实 DOI/期刊/年份，原文未给出的严禁编造）与本专利权利要求覆盖范围的差异；评估该论文若早于申请日公开，是否可能构成本专利的新颖性/创造性现有技术威胁（专利法第 22 条），以及论文中的实验验证方法有哪些可直接吸收用于工程验证。\n"
    } else {
        "\n- 若上传文献中包含学术论文、行业标准或实验报告内容，同步核验其理论/实验依据与本专利权利要求的新颖性/创造性及工程边界差异（原文未给出的 DOI/指标严禁编造）。\n"
    };

    let system_prompt = "你是兼具 15 年一线总工程师、资深专利代理师与跨学科科研审稿人经验的深度文献对比专家。\
         你的核心职责是打通专利文献、期刊学术论文、技术交底书与工程图纸之间的体裁壁垒，结合完整权利要求、说明书实施例、\
         法律状态预检与程序预挖的定量参数/部件拓扑，产出高信息密度、可直接指导研发决策、专利攻防与论文实验设计的深度对比报告。\
         严禁只凭摘要泛泛而谈；严禁将已失效/撤回/驳回专利误判为高侵权风险；原文未披露的定量指标、DOI 或权要细节必须显式标注「原文未披露」，绝不编造。"
        .to_string();

    let prompt = format!(
        "请将【目标专利】与用户自定义的 **{doc_count} 份对比文献**（覆盖专利、期刊论文、技术文档或图纸）进行全维度跨体裁深度对比分析：\n\
         {focus_clause}\n\
         <user_input>\n\
         【目标专利】\n{patent_excerpt}\n\n\
         {docs_joined}\n\
         </user_input>\n\n\
         请严格按以下 4 个硬核维度输出标准 Markdown 报告：\n\n\
         ### 一、多文献核心物理机理与定量参数横向对照表\n\
         - 使用标准 Markdown 表格（列：`对比维度 | 目标专利 | D1 | ... | 本质异同与工程含义`），覆盖至少 5 个实质维度：底层痛点机理、核心拓扑/算法架构、定量参数与实测指标窗口、隐含前提与失效边界、落地代价/成熟度。\n\n\
         ### 二、独立权利要求逐特征核验、FTO 风险与学术现有技术交叉评估\n\
         - 先明确复核上述法律状态预检结论；若目标专利已失效/撤回/驳回，直接标明「无有效专利权侵权风险，仅作公知技术参考」。\n\
         - 逐项拆解目标专利独立权利要求必要技术特征，对照每份对比文献标注「字面覆盖 / 等同风险 / 明确缺失 / 现有技术公开威胁 / 原文未披露」，给出最终风险定级（高 / 中 / 低 / 无）与量化依据。{academic_clause}\n\
         ### 三、说明书隐藏细节、跨文献互补嫁接与工程规避路径\n\
         - 结合程序预挖的未权要隐藏部件、独立权利要求薄弱点及各篇对比文献（含论文方法/结构）的长板，给出至少 2 条可落地的工程绕行或跨方案嫁接路径，并诚实说明各自对成本、可靠性或性能的副作用。\n\n\
         ### 四、面向未来 3–5 年的差异化创新与专利/论文协同布局建议\n\
         - 提炼这批文献共同未攻克的研发空白点，给出外围高价值专利布局与学术实验验证切入点，明确每条建议必须先做的实验验证边界。",
        docs_joined = docs_blocks.join("\n\n")
    );

    (system_prompt, prompt)
}

/// POST /api/upload/ingest-local
/// 将「技术调研」页上传解析后的本地文件（专利 / 期刊论文 / 技术文档）自动识别体裁并收入本地数据库
/// （同步触发 FTS5 全文索引与本地语义向量），使本地检索立即可查，并提取核心检索词供下游衔接。
pub async fn api_upload_ingest_local(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let filename = req["filename"]
        .as_str()
        .unwrap_or("local_document.txt")
        .trim();
    let content = req["content"].as_str().unwrap_or("").trim();
    if content.is_empty() {
        return Json(json!({ "status": "error", "error": "文档内容为空，无法入库" }));
    }

    let (patent, doc_kind, keywords) = build_local_patent_record(filename, content);
    match s.db.insert_patent(&patent) {
        Ok(id) => {
            crate::db::vector::try_compute_and_save_embedding(
                &s.db,
                &id,
                &format!("{} {}", patent.title, patent.abstract_text),
            );
            Json(json!({
                "status": "ok",
                "id": id,
                "patent_number": patent.patent_number,
                "title": patent.title,
                "doc_kind": doc_kind,
                "source": patent.source,
                "keywords": keywords,
                "detail_url": format!("/patent/{}", id),
            }))
        }
        Err(e) => Json(json!({
            "status": "error",
            "error": format!("收入本地文献库失败: {}", e)
        })),
    }
}

fn build_local_patent_record(
    filename: &str,
    content: &str,
) -> (crate::patent::Patent, &'static str, Vec<String>) {
    let doc_kind = infer_uploaded_doc_kind(filename, content, false);
    let sample: String = content.chars().take(4000).collect();

    let stem = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename)
        .rsplit_once('.')
        .map(|(base, _)| base)
        .unwrap_or(filename)
        .trim()
        .to_string();

    // 确定性短指纹（同内容重复上传生成同一编号，便于幂等去重）
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in content.as_bytes().iter().take(4096) {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let short_hash = format!("{:08X}", (hash & 0xFFFF_FFFF) as u32);

    // 1. 提取标题：优先命中「发明名称/题目/Title」，否则取首个有效长行或文件名
    let mut title = String::new();
    for line in sample.lines().take(30) {
        let trimmed = line.trim();
        for prefix in [
            "发明名称：",
            "发明名称:",
            "实用新型名称：",
            "名称：",
            "题目：",
            "Title:",
            "TITLE:",
        ] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let candidate = rest.trim();
                if candidate.chars().count() >= 4 {
                    title = candidate.chars().take(120).collect();
                    break;
                }
            }
        }
        if !title.is_empty() {
            break;
        }
    }
    if title.is_empty() {
        title = sample
            .lines()
            .map(str::trim)
            .find(|l| {
                let c = l.chars().count();
                (6..=90).contains(&c)
                    && !l.starts_with("http")
                    && !l.contains("申请号")
                    && !l.contains("公开号")
                    && !l.contains("授权公告号")
            })
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                if stem.is_empty() {
                    "本地上传文献".to_string()
                } else {
                    stem.clone()
                }
            });
    }

    // 2. 按体裁确定编号、来源与国家标签
    let (patent_number, source, country) = if doc_kind == "对比专利文献" {
        let pat_no = regex::Regex::new(r"\b(?:CN|US|EP|WO|JP|KR)\s*[0-9]{6,13}\s*[A-Z][0-9]?\b")
            .ok()
            .and_then(|re| re.find(&sample).map(|m| m.as_str().replace(' ', "")))
            .unwrap_or_else(|| format!("LOCAL-PAT-{}", short_hash));
        let ctry = if pat_no.starts_with("LOCAL-") {
            "LOCAL".to_string()
        } else {
            pat_no.chars().take(2).collect()
        };
        (pat_no, "local_patent".to_string(), ctry)
    } else if doc_kind == "期刊/学术论文" {
        let doi = regex::Regex::new(r"10\.\d{4,9}/[^\s,;)\]>，。]+")
            .ok()
            .and_then(|re| {
                re.find(&sample)
                    .map(|m| m.as_str().trim_end_matches('.').to_string())
            })
            .unwrap_or_else(|| format!("LOCAL-PAPER-{}", short_hash));
        (doi, "local_paper".to_string(), "PAPER".to_string())
    } else {
        (
            format!("LOCAL-DOC-{}", short_hash),
            "local_doc".to_string(),
            "DOC".to_string(),
        )
    };

    // 3. 拆分摘要 / 权利要求 / 说明书
    let abstract_text = extract_section_between(
        content,
        &["摘要", "摘 要", "Abstract", "ABSTRACT"],
        &[
            "关键词",
            "Keywords",
            "权利要求书",
            "1.",
            "技术领域",
            "引言",
            "Introduction",
        ],
        600,
    )
    .unwrap_or_else(|| content.chars().take(420).collect());

    let (claims, description) = if doc_kind == "对比专利文献" {
        let claims_part = extract_section_between(
            content,
            &["权利要求书", "What is claimed is", "CLAIMS"],
            &["说明书", "技术领域", "DESCRIPTION", "具体实施方式"],
            30_000,
        )
        .unwrap_or_default();
        let desc_part = extract_section_between(
            content,
            &["说明书", "技术领域", "具体实施方式", "DESCRIPTION"],
            &[],
            80_000,
        )
        .unwrap_or_else(|| content.chars().take(80_000).collect());
        (claims_part, desc_part)
    } else {
        (String::new(), content.chars().take(80_000).collect())
    };

    let ipc_codes = regex::Regex::new(r"\b[A-H][0-9]{2}[A-Z]\s*[0-9]{1,4}/[0-9]{2,4}\b")
        .ok()
        .map(|re| {
            let mut codes: Vec<String> = Vec::new();
            for m in re.find_iter(&sample) {
                let code = m.as_str().replace(' ', "");
                if !codes.contains(&code) {
                    codes.push(code);
                }
                if codes.len() >= 4 {
                    break;
                }
            }
            codes.join("; ")
        })
        .unwrap_or_default();

    let applicant = sample
        .lines()
        .find_map(|l| {
            let t = l.trim();
            for prefix in [
                "申请人：",
                "申请人:",
                "专利权人：",
                "专利权人:",
                "作者单位：",
            ] {
                if let Some(rest) = t.strip_prefix(prefix) {
                    let v = rest.trim();
                    if !v.is_empty() {
                        return Some(v.chars().take(80).collect::<String>());
                    }
                }
            }
            None
        })
        .unwrap_or_else(|| match doc_kind {
            "期刊/学术论文" => "本地上传期刊论文".to_string(),
            "对比专利文献" => "本地上传专利".to_string(),
            _ => "本地上传技术文档".to_string(),
        });

    let mut keywords = crate::search::relevance::extract_query_words(&title);
    if keywords.len() < 3 {
        for kw in crate::search::relevance::extract_query_words(&abstract_text) {
            if !keywords.contains(&kw) {
                keywords.push(kw);
            }
            if keywords.len() >= 6 {
                break;
            }
        }
    }
    keywords.truncate(6);

    let patent = crate::patent::Patent {
        id: uuid::Uuid::new_v4().to_string(),
        patent_number,
        title,
        abstract_text,
        description,
        claims,
        applicant,
        inventor: String::new(),
        filing_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        publication_date: String::new(),
        grant_date: None,
        ipc_codes,
        cpc_codes: String::new(),
        priority_date: String::new(),
        country,
        kind_code: String::new(),
        family_id: None,
        legal_status: match doc_kind {
            "期刊/学术论文" => "学术文献（本地上传）".to_string(),
            "对比专利文献" => "本地上传专利".to_string(),
            _ => "本地技术文档".to_string(),
        },
        citations: "[]".to_string(),
        cited_by: "[]".to_string(),
        source,
        raw_json: "{}".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        images: "[]".to_string(),
        pdf_url: String::new(),
    };

    (patent, doc_kind, keywords)
}

fn extract_section_between(
    text: &str,
    starts: &[&str],
    ends: &[&str],
    max_chars: usize,
) -> Option<String> {
    let mut start_pos: Option<usize> = None;
    for marker in starts {
        if let Some(idx) = text.find(marker) {
            let after = idx + marker.len();
            if start_pos.is_none_or(|prev| after < prev) {
                start_pos = Some(after);
            }
        }
    }
    let from = start_pos?;
    let tail = text
        .get(from..)?
        .trim_start_matches([':', '：', ' ', '\t', '\r', '\n']);
    let mut end_pos = tail.len();
    for marker in ends {
        if let Some(idx) = tail.find(marker) {
            if idx > 20 && idx < end_pos {
                end_pos = idx;
            }
        }
    }
    let slice = tail.get(..end_pos)?.trim();
    if slice.is_empty() {
        return None;
    }
    Some(slice.chars().take(max_chars).collect())
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

    match extract_universal_text_from_bytes(&s, &file_name, &file_bytes).await {
        Ok((text, ext)) => {
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
        Err(err_json) => Json(err_json),
    }
}

/// 统一全格式解析入口（供 `/api/upload/extract` 与 `/api/ai/oa-discuss` 等内部复用）：
/// 支持 PDF / DOCX / PPTX / XLSX / OFD / STEP / STL / IGES / HTML / XML / RTF / LaTeX / Markdown / TXT 及图片视觉识别。
pub(crate) async fn extract_universal_text_from_bytes(
    s: &AppState,
    file_name: &str,
    file_bytes: &[u8],
) -> Result<(String, String), serde_json::Value> {
    let ext_from_name = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    // #97: filename may be missing or mangled for non-ASCII names.
    // Use content magic bytes to detect type when the extension is unreliable.
    let ext = match detect_type_by_magic(file_bytes) {
        Some(magic_type) => magic_type.to_string(),
        None => ext_from_name,
    };
    if ext == "pdf" {
        if let Err(err) = inspect_pdf_integrity(file_bytes) {
            return Err(err.to_json());
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
        match describe_image_with_fallback(&ai_client, file_bytes, &ext).await {
            Ok(desc) => desc,
            Err(e) => return Err(StructuredUploadError::ocr_unavailable(e).to_json()),
        }
    } else if ext == "pdf" {
        match extract_pdf_text(file_bytes).await {
            Ok(t) if t.trim().len() >= 100 => t,
            _ => {
                // 文字提取失败，用 AI 视觉模型兜底
                let is_deepseek = {
                    let cfg = s.config.read().unwrap_or_else(|e| e.into_inner());
                    cfg.ai_base_url.contains("deepseek")
                };
                if is_deepseek {
                    return Err(StructuredUploadError::ocr_unavailable(
                        "本地 Umi-OCR/Tesseract 未命中，且当前 AI 为 DeepSeek 不支持图片识别",
                    )
                    .to_json());
                }
                let ai_client = s
                    .config
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .ai_client();
                match extract_pdf_via_ai_vision(file_bytes, &ai_client).await {
                    Ok(t) => t,
                    Err(e) => {
                        return Err(StructuredUploadError::ocr_unavailable(format!(
                            "全部 OCR 与视觉兜底均失败: {e}"
                        ))
                        .to_json());
                    }
                }
            }
        }
    } else if ext == "docx" {
        match extract_docx_text(file_bytes) {
            Ok(t) if !t.trim().is_empty() => t,
            Ok(_) => {
                return Err(StructuredUploadError::file_corrupted("DOCX 无可提取文字").to_json());
            }
            Err(e) => {
                return Err(
                    StructuredUploadError::file_corrupted(format!("DOCX 解析失败: {e}")).to_json(),
                );
            }
        }
    } else if ext == "pptx" {
        match extract_pptx_text(file_bytes) {
            Ok(t) if !t.trim().is_empty() => t,
            Ok(_) => {
                return Err(
                    StructuredUploadError::file_corrupted("PPTX 无可提取的幻灯片文字").to_json(),
                );
            }
            Err(e) => {
                return Err(
                    StructuredUploadError::file_corrupted(format!("PPTX 解析失败: {e}")).to_json(),
                );
            }
        }
    } else if ext == "xlsx" {
        match extract_xlsx_text(file_bytes) {
            Ok(t) if !t.trim().is_empty() => t,
            Ok(_) => {
                return Err(
                    StructuredUploadError::file_corrupted("XLSX 无可提取的表格内容").to_json(),
                );
            }
            Err(e) => {
                return Err(
                    StructuredUploadError::file_corrupted(format!("XLSX 解析失败: {e}")).to_json(),
                );
            }
        }
    } else if ext == "ofd" {
        match extract_ofd_text(file_bytes) {
            Ok(t) if !t.trim().is_empty() => t,
            Ok(_) => {
                return Err(StructuredUploadError::file_corrupted(
                    "OFD 无可提取文字（若为扫描件请另存 PDF 后上传）",
                )
                .to_json());
            }
            Err(e) => {
                return Err(
                    StructuredUploadError::file_corrupted(format!("OFD 解析失败: {e}")).to_json(),
                );
            }
        }
    } else if ext == "stl" {
        match extract_stl_summary(file_bytes) {
            Ok(t) if !t.trim().is_empty() => t,
            Ok(_) => {
                return Err(
                    StructuredUploadError::file_corrupted("STL 三维文件未提取到有效面片").to_json(),
                );
            }
            Err(e) => {
                return Err(
                    StructuredUploadError::file_corrupted(format!("STL 解析失败: {e}")).to_json(),
                );
            }
        }
    } else if let Some(hint) = unsupported_proprietary_format_hint(&ext, file_name) {
        return Err(json!({
            "error": hint,
            "error_code": "ERR_UNSUPPORTED_FORMAT",
            "hint": hint
        }));
    } else {
        let decoded = match String::from_utf8(file_bytes.to_vec()) {
            Ok(t) => t,
            Err(_) => {
                let (t, _, had_errors) = encoding_rs::GBK.decode(file_bytes);
                if had_errors {
                    return Err(StructuredUploadError::file_corrupted(
                        "无法识别文件类型，请上传 PDF、DOCX、PPTX、XLSX、OFD、STEP/STL/IGES、TXT/MD/LaTeX/HTML 或图片文件",
                    )
                    .to_json());
                }
                t.into_owned()
            }
        };
        postprocess_text_by_ext(&ext, &decoded)
    };

    Ok((text, ext))
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
    Ok(decode_basic_xml_entities(&text))
}

fn decode_basic_xml_entities(input: &str) -> String {
    input
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// 针对私有/老旧专有格式（知网 CAJ、旧版 Office 97-2003、WPS、AutoCAD DWG）返回可执行的中文转换提示
fn unsupported_proprietary_format_hint(ext: &str, file_name: &str) -> Option<String> {
    match ext {
        "caj" | "nh" | "kdh" => Some(format!(
            "文件「{}」为知网私有格式（.{}），受私有编码限制暂不支持直读。请在知网下载页改选「PDF 下载」，或在 CAJViewer 中「打印 → 另存为 PDF」后上传。",
            file_name, ext
        )),
        "doc" | "ppt" | "xls" => Some(format!(
            "文件「{}」为旧版 Office 97-2003 二进制格式（.{}），请用 Word/PowerPoint/Excel 或 WPS「另存为 .docx / .pptx / .xlsx 或 .pdf」后上传。",
            file_name, ext
        )),
        "wps" | "et" | "dps" => Some(format!(
            "文件「{}」为 WPS 老旧专有格式（.{}），请用 WPS「另存为 .docx / .xlsx / .pptx 或 .pdf」后上传。",
            file_name, ext
        )),
        "dwg" => Some(format!(
            "文件「{}」为 AutoCAD 私有二进制图纸（.dwg），请在 CAD 软件中导出为 .pdf、.dxf 或高分辨率 .png 截图后上传。",
            file_name
        )),
        _ => None,
    }
}

/// 按文本类后缀做清洗（STEP/IGES 三维结构化提取、HTML/XML 去标签、RTF 去控制字，其余源码文本原样保留）
fn postprocess_text_by_ext(ext: &str, raw: &str) -> String {
    match ext {
        "step" | "stp" => extract_step_summary(raw),
        "iges" | "igs" => extract_iges_summary(raw),
        "html" | "htm" | "xml" => extract_html_or_xml_text(raw),
        "rtf" => extract_rtf_text(raw),
        _ => raw.to_string(),
    }
}

/// 解码 STEP (ISO 10303-21) 中的 `\X2\XXXX...\X0\` UTF-16BE 中文/Unicode 转义序列
fn decode_step_unicode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(pos) = rest.find("\\X2\\") {
        if let Some(prefix) = rest.get(..pos) {
            out.push_str(prefix);
        }
        let after = rest.get(pos + 4..).unwrap_or("");
        if let Some(end_pos) = after.find("\\X0\\") {
            let hex_str = after.get(..end_pos).unwrap_or("");
            let mut u16s = Vec::new();
            let bytes = hex_str.as_bytes();
            let mut idx = 0;
            while idx + 4 <= bytes.len() {
                if let Some(chunk) = hex_str.get(idx..idx + 4) {
                    if let Ok(val) = u16::from_str_radix(chunk, 16) {
                        u16s.push(val);
                    }
                }
                idx += 4;
            }
            out.push_str(&String::from_utf16_lossy(&u16s));
            rest = after.get(end_pos + 4..).unwrap_or("");
        } else {
            out.push_str("\\X2\\");
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// 从 ISO 10303-21 STEP (`.step`/`.stp`) 三维 CAD 文件提取高信息密度工程结构摘要：
/// 装配体与零件清单（含中文解码）、精确三维外廓包络尺寸、典型圆孔/圆柱直径分布、几何拓扑统计。
fn extract_step_summary(raw: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut schema = String::new();
    let mut file_meta = String::new();
    let mut unit_label = "mm（默认）";

    if raw.contains("SI_UNIT(.MILLI.,.METRE.)") {
        unit_label = "mm";
    } else if raw.contains("SI_UNIT($,.METRE.)") {
        unit_label = "m";
    } else if raw.contains("CONVERSION_BASED_UNIT('INCH'") || raw.contains("INCH") {
        unit_label = "inch";
    }

    if let Ok(re_schema) = regex::Regex::new(r"FILE_SCHEMA\s*\(\s*\(\s*'([^']+)'") {
        if let Some(c) = re_schema.captures(raw) {
            schema = c[1].to_string();
        }
    }
    if let Ok(re_fname) = regex::Regex::new(r"FILE_NAME\s*\(\s*'([^']*)'") {
        if let Some(c) = re_fname.captures(raw) {
            file_meta = decode_step_unicode(c[1].trim());
        }
    }

    if let Ok(re_part) = regex::Regex::new(
        r"(?:PRODUCT|MANIFOLD_SOLID_BREP|NEXT_ASSEMBLY_USAGE_OCCURRENCE)\s*\(\s*'([^']*)'(?:\s*,\s*'([^']*)')?",
    ) {
        for cap in re_part.captures_iter(raw) {
            for group_idx in [1usize, 2usize] {
                if let Some(m) = cap.get(group_idx) {
                    let decoded = decode_step_unicode(m.as_str().trim());
                    let trimmed = decoded.trim();
                    if trimmed.chars().count() >= 2
                        && !trimmed.eq_ignore_ascii_case("NONE")
                        && !trimmed.eq_ignore_ascii_case("NA")
                        && !trimmed.eq_ignore_ascii_case("OPEN CASCADE")
                        && !parts.contains(&trimmed.to_string())
                    {
                        parts.push(trimmed.to_string());
                        if parts.len() >= 60 {
                            break;
                        }
                    }
                }
            }
            if parts.len() >= 60 {
                break;
            }
        }
    }

    // 计算三维外廓包络尺寸（扫描 CARTESIAN_POINT）
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut min_z = f64::INFINITY;
    let mut max_z = f64::NEG_INFINITY;
    let mut point_count = 0usize;
    if let Ok(re_pt) = regex::Regex::new(
        r"CARTESIAN_POINT\s*\(\s*'[^']*'\s*,\s*\(\s*([-+]?\d*\.?\d+(?:[eE][-+]?\d+)?)\s*,\s*([-+]?\d*\.?\d+(?:[eE][-+]?\d+)?)\s*,\s*([-+]?\d*\.?\d+(?:[eE][-+]?\d+)?)\s*\)\s*\)",
    ) {
        for cap in re_pt.captures_iter(raw) {
            if let (Ok(x), Ok(y), Ok(z)) = (
                cap[1].parse::<f64>(),
                cap[2].parse::<f64>(),
                cap[3].parse::<f64>(),
            ) {
                if x.is_finite() && y.is_finite() && z.is_finite() {
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                    min_z = min_z.min(z);
                    max_z = max_z.max(z);
                    point_count += 1;
                }
            }
        }
    }

    // 提取圆柱面与圆特征半径分布
    let mut radii: Vec<f64> = Vec::new();
    if let Ok(re_rad) = regex::Regex::new(
        r"(?:CYLINDRICAL_SURFACE|CIRCLE)\s*\(\s*'[^']*'\s*,\s*#\d+\s*,\s*([-+]?\d*\.?\d+(?:[eE][-+]?\d+)?)\s*\)",
    ) {
        for cap in re_rad.captures_iter(raw) {
            if let Ok(r) = cap[1].parse::<f64>() {
                if r.is_finite() && r > 0.0 && r < 1e7 {
                    let rounded = (r * 100.0).round() / 100.0;
                    if !radii.iter().any(|v| (*v - rounded).abs() < 0.005) {
                        radii.push(rounded);
                    }
                }
            }
        }
    }
    radii.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let solid_cnt = raw.matches("MANIFOLD_SOLID_BREP").count();
    let shell_cnt = raw.matches("CLOSED_SHELL").count();
    let face_cnt = raw.matches("ADVANCED_FACE").count();
    let plane_cnt = raw.matches("PLANE(").count();
    let cyl_cnt = raw.matches("CYLINDRICAL_SURFACE(").count();
    let cone_cnt = raw.matches("CONICAL_SURFACE(").count();
    let bspline_cnt = raw.matches("B_SPLINE_SURFACE").count();

    let mut md = String::from("【STEP 三维 CAD 模型结构化解析】\n");
    if !file_meta.is_empty() {
        md.push_str(&format!("- 模型名称：{}\n", file_meta));
    }
    if !schema.is_empty() {
        md.push_str(&format!("- 协议标准：{}\n", schema));
    }
    md.push_str(&format!("- 长度单位：{}\n", unit_label));
    if point_count > 0 {
        md.push_str(&format!(
            "- 三维外廓包络尺寸（基于 {} 个几何控制点）：X = {:.2} {}（{:.2} ~ {:.2}），Y = {:.2} {}（{:.2} ~ {:.2}），Z = {:.2} {}（{:.2} ~ {:.2}）\n",
            point_count,
            max_x - min_x,
            unit_label,
            min_x,
            max_x,
            max_y - min_y,
            unit_label,
            min_y,
            max_y,
            max_z - min_z,
            unit_label,
            min_z,
            max_z,
        ));
    }
    md.push_str(&format!(
        "- 几何拓扑统计：实体(MANIFOLD_SOLID_BREP) {} 个、封闭壳体 {} 个、高级面 {} 个（平面 {}、圆柱面 {}、锥面 {}、自由曲面 {}）\n",
        solid_cnt, shell_cnt, face_cnt, plane_cnt, cyl_cnt, cone_cnt, bspline_cnt
    ));
    if !radii.is_empty() {
        let sample_dims: Vec<String> = radii
            .iter()
            .take(12)
            .map(|r| format!("R={:.2}/Ø{:.2}", r, r * 2.0))
            .collect();
        md.push_str(&format!(
            "- 典型圆孔/圆柱特征半径与直径（共 {} 种规格，单位 {}）：{}\n",
            radii.len(),
            unit_label,
            sample_dims.join("、")
        ));
    }
    if !parts.is_empty() {
        md.push_str(&format!(
            "\n### 装配体与零部件清单（共 {} 项）\n{}\n",
            parts.len(),
            parts
                .iter()
                .enumerate()
                .map(|(i, p)| format!("{}. {}", i + 1, p))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    md.push_str("\n> 说明：已自动过滤原始海量坐标点文本，仅保留装配体/零部件树、精确外廓尺寸、圆孔/轴径规格与拓扑特征供 AI 技术剖析与对比；如需同时评估三维外观视角，可补充三视图截图或调用本机 FreeCAD 渲染。\n");
    md
}

/// 从 STL（同时支持 Binary STL 与 ASCII STL）提取三维外廓包络尺寸与三角面片数
fn extract_stl_summary(data: &[u8]) -> Result<String, String> {
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;

    // 优先检测标准 Binary STL：80B header + 4B u32 triangle_count + 50B * count
    if data.len() >= 84 {
        let tri_count = u32::from_le_bytes([data[80], data[81], data[82], data[83]]) as usize;
        let expected = 84usize.saturating_add(tri_count.saturating_mul(50));
        if tri_count > 0 && expected == data.len() {
            let header_text = String::from_utf8_lossy(data.get(..80).unwrap_or(data))
                .trim_matches(|c: char| c == '\0' || c.is_whitespace())
                .to_string();
            for i in 0..tri_count {
                let base = 84 + i * 50;
                // 跳过 12 字节法向量，读取 3 个顶点 (x, y, z)
                for v in 0..3 {
                    let off = base + 12 + v * 12;
                    let x = f32::from_le_bytes([
                        data[off],
                        data[off + 1],
                        data[off + 2],
                        data[off + 3],
                    ]);
                    let y = f32::from_le_bytes([
                        data[off + 4],
                        data[off + 5],
                        data[off + 6],
                        data[off + 7],
                    ]);
                    let z = f32::from_le_bytes([
                        data[off + 8],
                        data[off + 9],
                        data[off + 10],
                        data[off + 11],
                    ]);
                    if x.is_finite() && y.is_finite() && z.is_finite() {
                        min_x = min_x.min(x);
                        max_x = max_x.max(x);
                        min_y = min_y.min(y);
                        max_y = max_y.max(y);
                        min_z = min_z.min(z);
                        max_z = max_z.max(z);
                    }
                }
            }
            return Ok(format!(
                "【STL 三维网格模型解析】\n- 编码格式：Binary STL\n- 文件头备注：{}\n- 三角面片总数：{}\n- 三维外廓包络尺寸：X = {:.2}（{:.2} ~ {:.2}），Y = {:.2}（{:.2} ~ {:.2}），Z = {:.2}（{:.2} ~ {:.2}）\n",
                if header_text.is_empty() { "无" } else { &header_text },
                tri_count,
                max_x - min_x, min_x, max_x,
                max_y - min_y, min_y, max_y,
                max_z - min_z, min_z, max_z,
            ));
        }
    }

    // 回退 ASCII STL
    let text = String::from_utf8_lossy(data);
    let mut vertex_count = 0usize;
    let mut facet_count = 0usize;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("facet ") {
            facet_count += 1;
        } else if let Some(rest) = t.strip_prefix("vertex ") {
            let nums: Vec<f32> = rest
                .split_whitespace()
                .filter_map(|s| s.parse::<f32>().ok())
                .collect();
            if nums.len() >= 3 && nums[0].is_finite() && nums[1].is_finite() && nums[2].is_finite()
            {
                min_x = min_x.min(nums[0]);
                max_x = max_x.max(nums[0]);
                min_y = min_y.min(nums[1]);
                max_y = max_y.max(nums[1]);
                min_z = min_z.min(nums[2]);
                max_z = max_z.max(nums[2]);
                vertex_count += 1;
            }
        }
    }
    if vertex_count == 0 {
        return Err("未检测到有效的 Binary 或 ASCII STL 三维网格顶点".into());
    }
    Ok(format!(
        "【STL 三维网格模型解析】\n- 编码格式：ASCII STL\n- 三角面片总数：{}（顶点数 {}）\n- 三维外廓包络尺寸：X = {:.2}（{:.2} ~ {:.2}），Y = {:.2}（{:.2} ~ {:.2}），Z = {:.2}（{:.2} ~ {:.2}）\n",
        facet_count.max(vertex_count / 3),
        vertex_count,
        max_x - min_x, min_x, max_x,
        max_y - min_y, min_y, max_y,
        max_z - min_z, min_z, max_z,
    ))
}

/// 从 IGES (`.iges`/`.igs`) 三维 CAD 文件提取全局头信息与实体统计
fn extract_iges_summary(raw: &str) -> String {
    let mut start_lines = Vec::new();
    let mut global_lines = Vec::new();
    let mut entity_lines = 0usize;
    for line in raw.lines() {
        if line.len() >= 73 {
            let sec = line.as_bytes()[72] as char;
            let content = line.get(..72).unwrap_or("").trim();
            match sec {
                'S' if !content.is_empty() => start_lines.push(content),
                'G' if !content.is_empty() => global_lines.push(content),
                'D' => entity_lines += 1,
                _ => {}
            }
        }
    }
    let entity_count = entity_lines / 2;
    let global_preview: String = global_lines.join("").chars().take(600).collect();
    let start_preview: String = start_lines.join(" ").chars().take(300).collect();
    format!(
        "【IGES 三维 CAD 模型解析】\n- 目录实体总数：{} 个\n- Start 说明：{}\n- Global 参数摘要：{}\n",
        entity_count,
        if start_preview.is_empty() { "无" } else { &start_preview },
        if global_preview.is_empty() { "无" } else { &global_preview },
    )
}

/// 从 PPTX 幻灯片（ZIP 内 `ppt/slides/slide*.xml`）按页顺序原生提取文字
fn extract_pptx_text(data: &[u8]) -> Result<String, String> {
    use std::io::{Cursor, Read};
    let mut archive =
        zip::ZipArchive::new(Cursor::new(data)).map_err(|e| format!("非有效 PPTX: {}", e))?;
    let mut slide_names: Vec<(usize, String)> = Vec::new();
    for i in 0..archive.len() {
        if let Ok(file) = archive.by_index(i) {
            let name = file.name().to_string();
            if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
                let num = name
                    .trim_start_matches("ppt/slides/slide")
                    .trim_end_matches(".xml")
                    .parse::<usize>()
                    .unwrap_or(usize::MAX);
                slide_names.push((num, name));
            }
        }
    }
    slide_names.sort_by_key(|(n, _)| *n);
    if slide_names.is_empty() {
        return Err("PPTX 中未找到 ppt/slides/slide*.xml".into());
    }

    let mut sections = Vec::new();
    for (idx, (_, name)) in slide_names.into_iter().enumerate() {
        let mut xml = String::new();
        if let Ok(mut f) = archive.by_name(&name) {
            let _ = f.read_to_string(&mut xml);
        }
        let body = extract_xml_tag_texts(&xml, &["a:t", "t"]).join(" ");
        let trimmed = body.trim();
        if !trimmed.is_empty() {
            sections.push(format!("【幻灯片 {}】\n{}", idx + 1, trimmed));
        }
    }
    Ok(sections.join("\n\n"))
}

/// 从 XLSX 表格（ZIP 内 `xl/sharedStrings.xml` + `xl/worksheets/sheet*.xml`）原生提取可读表格文本
fn extract_xlsx_text(data: &[u8]) -> Result<String, String> {
    use std::io::{Cursor, Read};
    let mut archive =
        zip::ZipArchive::new(Cursor::new(data)).map_err(|e| format!("非有效 XLSX: {}", e))?;

    let mut shared_strings: Vec<String> = Vec::new();
    if let Ok(mut ss_file) = archive.by_name("xl/sharedStrings.xml") {
        let mut xml = String::new();
        let _ = ss_file.read_to_string(&mut xml);
        for si in xml.split("<si").skip(1) {
            let end = si.find("</si>").unwrap_or(si.len());
            let chunk = si.get(..end).unwrap_or(si);
            let joined = extract_xml_tag_texts(chunk, &["t"]).join("");
            shared_strings.push(joined);
        }
    }

    let mut sheet_names: Vec<(usize, String)> = Vec::new();
    for i in 0..archive.len() {
        if let Ok(file) = archive.by_index(i) {
            let name = file.name().to_string();
            if name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml") {
                let num = name
                    .trim_start_matches("xl/worksheets/sheet")
                    .trim_end_matches(".xml")
                    .parse::<usize>()
                    .unwrap_or(usize::MAX);
                sheet_names.push((num, name));
            }
        }
    }
    sheet_names.sort_by_key(|(n, _)| *n);
    if sheet_names.is_empty() {
        return Err("XLSX 中未找到 xl/worksheets/sheet*.xml".into());
    }

    let mut out = Vec::new();
    for (idx, (_, name)) in sheet_names.into_iter().enumerate() {
        let mut xml = String::new();
        if let Ok(mut f) = archive.by_name(&name) {
            let _ = f.read_to_string(&mut xml);
        }
        let mut rows_out = Vec::new();
        for row_chunk in xml.split("<row").skip(1) {
            let row_end = row_chunk.find("</row>").unwrap_or(row_chunk.len());
            let row_slice = row_chunk.get(..row_end).unwrap_or(row_chunk);
            let mut cells = Vec::new();
            for cell_chunk in row_slice.split("<c").skip(1) {
                if !cell_chunk
                    .chars()
                    .next()
                    .is_some_and(|ch| ch == '>' || ch.is_whitespace())
                {
                    continue;
                }
                let cell_end = cell_chunk.find("</c>").unwrap_or(cell_chunk.len());
                let cell_slice = cell_chunk.get(..cell_end).unwrap_or(cell_chunk);
                let tag_close = cell_slice.find('>').unwrap_or(0);
                let header = cell_slice.get(..tag_close).unwrap_or("");
                let is_shared = header.contains("t=\"s\"") || header.contains("t='s'");
                let is_inline = header.contains("t=\"inlineStr\"");
                let val = if is_inline {
                    extract_xml_tag_texts(cell_slice, &["t"]).join("")
                } else {
                    let raw_v = extract_xml_tag_texts(cell_slice, &["v"]).join("");
                    if is_shared {
                        raw_v
                            .trim()
                            .parse::<usize>()
                            .ok()
                            .and_then(|i| shared_strings.get(i).cloned())
                            .unwrap_or(raw_v)
                    } else {
                        raw_v
                    }
                };
                let cleaned = val.trim();
                if !cleaned.is_empty() {
                    cells.push(cleaned.to_string());
                }
            }
            if !cells.is_empty() {
                rows_out.push(cells.join(" | "));
            }
        }
        if !rows_out.is_empty() {
            out.push(format!("【工作表 {}】\n{}", idx + 1, rows_out.join("\n")));
        }
    }
    Ok(out.join("\n\n"))
}

/// 从国产版式文档 OFD（常见于国知局电子发文/专利证书：ZIP 内 `Pages/*/Content.xml` 的 `<ofd:TextCode>`）原生提取文字
fn extract_ofd_text(data: &[u8]) -> Result<String, String> {
    use std::io::{Cursor, Read};
    let mut archive =
        zip::ZipArchive::new(Cursor::new(data)).map_err(|e| format!("非有效 OFD: {}", e))?;
    let mut page_files: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        if let Ok(file) = archive.by_index(i) {
            let name = file.name().to_string();
            let lower = name.to_ascii_lowercase();
            if lower.contains("page") && lower.ends_with("content.xml") {
                page_files.push(name);
            }
        }
    }
    page_files.sort();
    if page_files.is_empty() {
        return Err("OFD 中未找到页面内容 XML（Pages/*/Content.xml）".into());
    }
    let mut pages_out = Vec::new();
    for (idx, name) in page_files.into_iter().enumerate() {
        let mut xml = String::new();
        if let Ok(mut f) = archive.by_name(&name) {
            let _ = f.read_to_string(&mut xml);
        }
        let tokens = extract_xml_tag_texts(&xml, &["ofd:TextCode", "TextCode"]);
        let joined = tokens.join("");
        let trimmed = joined.trim();
        if !trimmed.is_empty() {
            pages_out.push(format!("【第 {} 页】\n{}", idx + 1, trimmed));
        }
    }
    Ok(pages_out.join("\n\n"))
}

/// 提取指定 XML 标签对内的文本节点（跳过属性，按 UTF-8 字符边界安全扫描）
fn extract_xml_tag_texts(xml: &str, tag_names: &[&str]) -> Vec<String> {
    let mut results = Vec::new();
    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        let after_lt = match rest.get(lt + 1..) {
            Some(s) => s,
            None => break,
        };
        let mut matched_tag: Option<&str> = None;
        for &tag in tag_names {
            if let Some(after_tag) = after_lt.strip_prefix(tag) {
                if after_tag
                    .chars()
                    .next()
                    .is_some_and(|c| c == '>' || c.is_whitespace())
                {
                    matched_tag = Some(tag);
                    break;
                }
            }
        }
        if let Some(tag) = matched_tag {
            if let Some(gt) = after_lt.find('>') {
                let body_start = match after_lt.get(gt + 1..) {
                    Some(s) => s,
                    None => break,
                };
                let close_pat = format!("</{}>", tag);
                if let Some(close_idx) = body_start.find(&close_pat) {
                    if let Some(inner) = body_start.get(..close_idx) {
                        let decoded = decode_basic_xml_entities(inner);
                        if !decoded.is_empty() {
                            results.push(decoded);
                        }
                    }
                    rest = body_start.get(close_idx + close_pat.len()..).unwrap_or("");
                    continue;
                }
            }
        }
        rest = after_lt;
    }
    results
}

/// 清理 HTML / XML 文档中的 `<script>` / `<style>` 与标签，保留纯正文
fn extract_html_or_xml_text(raw: &str) -> String {
    let cleaned = regex::Regex::new(r"(?is)<(?:script|style)[^>]*>.*?</(?:script|style)>")
        .map(|re| re.replace_all(raw, " ").into_owned())
        .unwrap_or_else(|_| raw.to_string());
    let mut text = String::with_capacity(cleaned.len() / 2);
    let mut in_tag = false;
    for ch in cleaned.chars() {
        if ch == '<' {
            in_tag = true;
            text.push('\n');
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            text.push(ch);
        }
    }
    let decoded = decode_basic_xml_entities(&text);
    decoded
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 轻量提取 RTF 富文本中的可读文字（去除控制字与花括号）
fn extract_rtf_text(raw: &str) -> String {
    let no_ctrl = regex::Regex::new(r"\\[a-zA-Z]+-?\d* ?|\\[{}\\]")
        .map(|re| re.replace_all(raw, " ").into_owned())
        .unwrap_or_else(|_| raw.to_string());
    no_ctrl
        .replace(['{', '}'], " ")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
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
/// Supports PDF, Office Open XML (`docx` / `pptx` / `xlsx`), CNIPA `ofd`,
/// images (`png` / `jpg` / `gif` / `webp` / `bmp`), `rtf`, and CNKI `caj`.
fn detect_type_by_magic(data: &[u8]) -> Option<&'static str> {
    if has_pdf_header(data) {
        return Some("pdf");
    }
    if has_docx_header(data) {
        // 细查 ZIP 内部清单，精确区分 DOCX / PPTX / XLSX / OFD（均以 PK\x03\x04 开头）
        if let Ok(mut archive) = zip::ZipArchive::new(std::io::Cursor::new(data)) {
            let mut is_ofd = false;
            let mut is_pptx = false;
            let mut is_xlsx = false;
            let mut is_docx = false;
            for i in 0..archive.len().min(64) {
                if let Ok(entry) = archive.by_index(i) {
                    let lower = entry.name().to_ascii_lowercase();
                    if lower == "ofd.xml" || lower.ends_with("/ofd.xml") {
                        is_ofd = true;
                        break;
                    } else if lower.starts_with("ppt/") {
                        is_pptx = true;
                    } else if lower.starts_with("xl/") {
                        is_xlsx = true;
                    } else if lower.starts_with("word/") {
                        is_docx = true;
                    }
                }
            }
            if is_ofd {
                return Some("ofd");
            }
            if is_pptx {
                return Some("pptx");
            }
            if is_xlsx {
                return Some("xlsx");
            }
            if is_docx {
                return Some("docx");
            }
        }
        return Some("docx");
    }
    if data.len() >= 8 && data[0..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return Some("png");
    }
    if data.len() >= 3 && data[0..3] == [0xFF, 0xD8, 0xFF] {
        return Some("jpg");
    }
    if data.len() >= 6 && (&data[0..6] == b"GIF87a" || &data[0..6] == b"GIF89a") {
        return Some("gif");
    }
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return Some("webp");
    }
    if data.len() >= 2 && &data[0..2] == b"BM" {
        return Some("bmp");
    }
    if data.len() >= 5 && &data[0..5] == b"{\\rtf" {
        return Some("rtf");
    }
    if data.len() >= 4 && (&data[0..4] == b"CAJ\x00" || &data[0..4] == b"HN\x00\x00") {
        return Some("caj");
    }
    let head = data.get(..data.len().min(128)).unwrap_or(data);
    if head
        .windows(b"ISO-10303-21".len())
        .any(|w| w == b"ISO-10303-21")
    {
        return Some("step");
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

    #[test]
    fn upload_compare_prompt_includes_spec_signals_inactive_legal_precheck_and_user_input_isolation(
    ) {
        let patent = crate::patent::Patent {
            id: "pat-uc-1".into(),
            patent_number: "CN90002001B".into(),
            title: "双流道油冷驱动电机".into(),
            abstract_text: "摘要文本".into(),
            legal_status: "Withdrawn".into(),
            claims: "1. 一种双流道油冷电机，包括定子铁芯(10)与螺旋油道(20)。".into(),
            description:
                "实施例1：冷却油流量 12 L/min，定子绕组峰值温度降至 115℃；导流环(30)位于端盖内侧。"
                    .into(),
            ..Default::default()
        };
        let (sys, prompt) = build_upload_compare_prompt(
            &patent,
            "上传文档",
            "研发交底书：采用轴向微通道冷却，工作压力 0.35 MPa，绕组温升降低 18%。",
        );
        assert!(sys.contains("已失效/撤回/驳回专利误判"));
        assert!(prompt.contains("<user_input>") && prompt.contains("</user_input>"));
        assert!(prompt.contains("失效/未授权状态：Withdrawn"));
        assert!(prompt.contains("115℃"), "应带入说明书定量参数预挖");
        assert!(prompt.contains("导流环"), "应带入说明书未权要隐藏部件");
        assert!(prompt.contains("0.35 MPa"), "应带入上传文档定量参数");
        assert!(prompt.contains("未来 3–5 年"));
    }

    #[test]
    fn multi_upload_compare_prompt_handles_mixed_patent_and_journal_papers_with_custom_focus() {
        let patent = crate::patent::Patent {
            id: "pat-multi-1".into(),
            patent_number: "CN90003001A".into(),
            title: "低频交流电场辅助贫燃燃烧器".into(),
            abstract_text: "利用低频交流电场强化天然气贫燃稳定。".into(),
            claims: "1. 一种电场辅助燃烧器，包括环形高压电极(10)和旋流喷口(20)。".into(),
            description: "实施例：施加 50Hz、4.5 kV 交流电压，NOx 排放降至 28 mg/m3。".into(),
            ..Default::default()
        };
        let docs = vec![
            UploadedCompareDoc {
                file_name: "ieee_flame_paper_2024.pdf".into(),
                doc_kind: infer_uploaded_doc_kind(
                    "ieee_flame_paper_2024.pdf",
                    "Abstract: Electric-field assisted lean combustion. DOI: 10.1109/TPS.2024.000123. Experimental NOx dropped by 35%.",
                    false,
                )
                .to_string(),
                ext: "pdf".into(),
                content: "Abstract: Electric-field assisted lean combustion. DOI: 10.1109/TPS.2024.000123. Experimental NOx dropped by 35%.".into(),
            },
            UploadedCompareDoc {
                file_name: "competitor_patent.txt".into(),
                doc_kind: infer_uploaded_doc_kind(
                    "competitor_patent.txt",
                    "权利要求书：1. 一种等离子体稳燃装置，采用直流脉冲电源。",
                    false,
                )
                .to_string(),
                ext: "txt".into(),
                content: "权利要求书：1. 一种等离子体稳燃装置，采用直流脉冲电源。".into(),
            },
        ];
        let (_sys, prompt) = build_multi_upload_compare_prompt(
            &patent,
            &docs,
            Some("重点对比论文的实测 NOx 降幅与两件专利电极方案的寿命差异"),
        );
        assert!(prompt.contains("2 份对比文献"));
        assert!(prompt.contains("体裁：期刊/学术论文"));
        assert!(prompt.contains("体裁：对比专利文献"));
        assert!(prompt.contains("10.1109/TPS.2024.000123"));
        assert!(prompt.contains("期刊/学术论文专项交叉分析"));
        assert!(prompt.contains("重点对比论文的实测 NOx 降幅"));
    }

    #[tokio::test]
    async fn ingest_local_indexes_patent_and_journal_paper_for_local_search() {
        let state = crate::common::init_app_state(":memory:").expect("init app state");

        // 1) 上传本地专利 PDF 文本 -> 识别为专利文献并进入本地搜索索引
        let pat_resp = api_upload_ingest_local(
            axum::extract::State(state.clone()),
            axum::Json(serde_json::json!({
                "filename": "dual_channel_cooling_patent.pdf",
                "content": "公开号：CN90008881B\n发明名称：一种双流道油冷扁线驱动电机定子总成\n申请人：示例电驱研究院\nInt.Cl. H02K 9/19\n摘要：本发明公开一种双流道油冷扁线定子冷却结构。\n权利要求书\n1. 一种双流道油冷定子总成，包括定子铁芯(10)和轴向微通道(20)。\n说明书\n具体实施方式：定子铁芯(10)外周设置轴向微通道(20)。"
            })),
        )
        .await;
        assert_eq!(pat_resp.0["status"], "ok");
        assert_eq!(pat_resp.0["source"], "local_patent");
        assert_eq!(pat_resp.0["patent_number"], "CN90008881B");
        assert!(pat_resp.0["detail_url"]
            .as_str()
            .unwrap_or("")
            .starts_with("/patent/"));

        // 验证本地数据库搜索立刻能命中刚才上传的专利
        let (hits, total) = state
            .db
            .search_like("双流道油冷扁线", None, None, None, 1, 10)
            .expect("local search");
        assert!(total >= 1 && hits.iter().any(|h| h.patent_number == "CN90008881B"));

        // 2) 上传期刊学术论文 -> 识别为期刊/学术论文、提取真实 DOI 并入库可检
        let paper_resp = api_upload_ingest_local(
            axum::extract::State(state.clone()),
            axum::Json(serde_json::json!({
                "filename": "thermal_journal_2025.pdf",
                "content": "Thermal Management of Hairpin PMSM via Spray Oil Cooling\nDOI: 10.1109/TIE.2025.998877\nAbstract: Experimental study of hairpin motor winding oil spray cooling with 12 L/min flow.\nKeywords: hairpin motor, oil spray cooling\nReferences: [1] ..."
            })),
        )
        .await;
        assert_eq!(paper_resp.0["status"], "ok");
        assert_eq!(paper_resp.0["source"], "local_paper");
        assert_eq!(paper_resp.0["patent_number"], "10.1109/TIE.2025.998877");
        let (paper_hits, paper_total) = state
            .db
            .search_like("Hairpin PMSM", None, None, None, 1, 10)
            .expect("local paper search");
        assert!(
            paper_total >= 1
                && paper_hits
                    .iter()
                    .any(|h| h.patent_number == "10.1109/TIE.2025.998877")
        );
    }

    #[test]
    fn universal_extractors_parse_pptx_xlsx_ofd_html_and_guide_caj() {
        use std::io::Write;

        let make_zip = |files: &[(&str, &str)]| -> Vec<u8> {
            let mut cursor = std::io::Cursor::new(Vec::new());
            {
                let mut zw = zip::ZipWriter::new(&mut cursor);
                let opts = zip::write::SimpleFileOptions::default();
                for (name, body) in files {
                    zw.start_file(*name, opts).expect("start zip file");
                    zw.write_all(body.as_bytes()).expect("write zip entry");
                }
                zw.finish().expect("finish zip");
            }
            cursor.into_inner()
        };

        // 1) PPTX
        let pptx = make_zip(&[
            ("ppt/presentation.xml", "<p:presentation/>"),
            (
                "ppt/slides/slide1.xml",
                "<p:sld><a:t>第一页：油冷电机温升测试</a:t><a:t>热点降低 34℃</a:t></p:sld>",
            ),
        ]);
        assert_eq!(detect_type_by_magic(&pptx), Some("pptx"));
        let pptx_text = extract_pptx_text(&pptx).expect("pptx extract");
        assert!(pptx_text.contains("第一页：油冷电机温升测试"));
        assert!(pptx_text.contains("热点降低 34℃"));

        // 2) XLSX
        let xlsx = make_zip(&[
            ("xl/workbook.xml", "<workbook/>"),
            (
                "xl/sharedStrings.xml",
                "<sst><si><t>流量(L/min)</t></si><si><t>峰值温度(℃)</t></si></sst>",
            ),
            (
                "xl/worksheets/sheet1.xml",
                "<worksheet><sheetData><row><c t=\"s\"><v>0</v></c><c t=\"s\"><v>1</v></c></row><row><c><v>10.5</v></c><c><v>134</v></c></row></sheetData></worksheet>",
            ),
        ]);
        assert_eq!(detect_type_by_magic(&xlsx), Some("xlsx"));
        let xlsx_text = extract_xlsx_text(&xlsx).expect("xlsx extract");
        assert!(xlsx_text.contains("流量(L/min) | 峰值温度(℃)"));
        assert!(xlsx_text.contains("10.5 | 134"));

        // 3) OFD（国知局电子发文）
        let ofd = make_zip(&[
            ("OFD.xml", "<ofd:OFD/>"),
            (
                "Doc_0/Pages/Page_0/Content.xml",
                "<ofd:Page><ofd:TextCode>第一次审查意见通知书：权利要求1不具备创造性</ofd:TextCode></ofd:Page>",
            ),
        ]);
        assert_eq!(detect_type_by_magic(&ofd), Some("ofd"));
        let ofd_text = extract_ofd_text(&ofd).expect("ofd extract");
        assert!(ofd_text.contains("第一次审查意见通知书"));

        // 4) HTML stripping
        let html = "<html><head><style>.x{color:red}</style><script>alert(1)</script></head><body><h1>专利摘要</h1><p>温度 &le; 120&deg;C &amp; 压力 0.2MPa</p></body></html>";
        let html_text = extract_html_or_xml_text(html);
        assert!(html_text.contains("专利摘要"));
        assert!(!html_text.contains("alert(1)"));
        assert!(!html_text.contains("color:red"));

        // 5) CAJ / DWG 友好引导
        assert_eq!(detect_type_by_magic(b"CAJ\x00dummy"), Some("caj"));
        let caj_hint = unsupported_proprietary_format_hint("caj", "paper.caj").expect("caj hint");
        assert!(caj_hint.contains("PDF 下载") && caj_hint.contains("CAJViewer"));
        let dwg_hint = unsupported_proprietary_format_hint("dwg", "motor.dwg").expect("dwg hint");
        assert!(dwg_hint.contains(".dxf"));
    }

    #[test]
    fn step_and_stl_3d_extractors_decode_chinese_parts_and_bounding_box() {
        let step_sample = "ISO-10303-21;\n\
            HEADER;\n\
            FILE_NAME('stator_assembly.stp','2026-11-24',('Engineer'),('R&D'),'SolidWorks','','');\n\
            FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\n\
            ENDSEC;\n\
            DATA;\n\
            #10=SI_UNIT(.MILLI.,.METRE.);\n\
            #20=PRODUCT('Stator_Housing','\\X2\\5B9A5B5094C182AF\\X0\\','',(#1));\n\
            #21=MANIFOLD_SOLID_BREP('Cooling_Jacket',#30);\n\
            #30=CLOSED_SHELL('',(#40));\n\
            #40=ADVANCED_FACE('',(),#50,.T.);\n\
            #50=CYLINDRICAL_SURFACE('',#60,60.0);\n\
            #60=CARTESIAN_POINT('',(0.0,0.0,0.0));\n\
            #61=CARTESIAN_POINT('',(240.0,240.0,180.0));\n\
            ENDSEC;\n\
            END-ISO-10303-21;\n";

        assert_eq!(detect_type_by_magic(step_sample.as_bytes()), Some("step"));
        let summary = extract_step_summary(step_sample);
        assert!(
            summary.contains("定子铁芯"),
            "应解码 STEP UTF-16BE 中文部件名: {summary}"
        );
        assert!(summary.contains("Cooling_Jacket"));
        assert!(summary.contains("X = 240.00 mm"));
        assert!(summary.contains("Z = 180.00 mm"));
        assert!(summary.contains("R=60.00/Ø120.00"));
        assert_eq!(
            infer_uploaded_doc_kind("motor.stp", &summary, false),
            "三维 CAD 模型"
        );

        let ascii_stl = "solid box\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 100 0 0\nvertex 100 50 25\nendloop\nendfacet\nendsolid box\n";
        let stl_sum = extract_stl_summary(ascii_stl.as_bytes()).expect("ascii stl");
        assert!(
            stl_sum.contains("X = 100.00")
                && stl_sum.contains("Y = 50.00")
                && stl_sum.contains("Z = 25.00")
        );
    }

    #[tokio::test]
    async fn shared_universal_extractor_handles_step_and_caj_for_oa_discuss() {
        let state = crate::common::init_app_state(":memory:").expect("init in-memory state");
        let step_bytes = b"ISO-10303-21;\nHEADER;\nFILE_NAME('valve.stp','2026-11-24',('Eng'),('Lab'),'CAD','','');\nENDSEC;\nDATA;\n#10=SI_UNIT(.MILLI.,.METRE.);\n#20=PRODUCT('Valve','\\X2\\96004F53\\X0\\','',(#1));\n#60=CARTESIAN_POINT('',(0.0,0.0,0.0));\n#61=CARTESIAN_POINT('',(80.0,40.0,20.0));\nENDSEC;\nEND-ISO-10303-21;\n";
        let (text, ext) = extract_universal_text_from_bytes(&state, "valve.stp", step_bytes)
            .await
            .expect("step extract should succeed");
        assert_eq!(ext, "step");
        assert!(
            text.contains("阀体") && text.contains("X = 80.00 mm"),
            "unexpected STEP summary: {text}"
        );

        let err_json = extract_universal_text_from_bytes(&state, "paper.caj", b"CAJ\x00dummy")
            .await
            .expect_err("caj should return conversion hint");
        assert_eq!(
            err_json.get("error_code").and_then(|v| v.as_str()),
            Some("ERR_UNSUPPORTED_FORMAT")
        );
    }
}
