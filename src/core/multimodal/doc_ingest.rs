//! 三级自适应多模态文档与图像解析引擎（`src/core/multimodal/doc_ingest.rs`）
//!
//! 根治 `ERR-09` / `#67`：
//! 1. 一级 `NativeTextLayer`：对 PDF 按页（或全文）执行字符密度与乱码率判定（去空白后有效字符 ≥ 80 且乱码率 < 0.05）。
//! 2. 二级 `LocalUmiOcr`：对扫描页/低密度页以 500ms `connect_timeout` 探活本机 `Umi-OCR` (`/api/ocr/get_options` / `/api/ocr`)。
//! 3. 三级 `VisionLlmFallback`：当 Umi-OCR 未运行且 `!privacy_offline_only` 且配置了看图模型时，纯 Rust 扫描 PDF 流内嵌 JPEG/PNG 字节块，转 base64 调 OpenAI 兼容 `image_url` 接口转录（60s 超时）。
//! 4. 优雅降级 `DegradedPlaceholder`：涉密离线锁开启或无看图模型时，绝不报错崩溃，返回结构化降级提示。

use crate::core::multimodal::figure_marks;
use crate::core::vault::models::FigureNumeral;
use anyhow::{anyhow, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};
use std::time::Duration;
use uuid::Uuid;

/// 三级自适应解析实际命中的层级
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IngestTierUsed {
    NativeTextLayer,
    LocalUmiOcr,
    VisionLlmFallback,
    DegradedPlaceholder,
}

/// 多模态解析选项（含涉密离线锁）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IngestOptions {
    /// 本机 Umi-OCR 基础地址（如 `http://127.0.0.1:1224`）
    pub umi_ocr_url: Option<String>,
    /// OpenAI 兼容看图模型端点：`(base_url, api_key, model)`
    pub vision_endpoint: Option<(String, String, String)>,
    /// 涉密离线锁：为 `true` 时绝对禁止向任何外部 Vision 大模型发起网络调用
    pub privacy_offline_only: bool,
}

/// 统一解析后的附件载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestedAttachment {
    pub attachment_id: String,
    pub filename: String,
    pub mime_type: String,
    pub extracted_text: String,
    pub tier_used: IngestTierUsed,
    pub page_count: usize,
    pub figure_numerals: Vec<FigureNumeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_summary_json: Option<serde_json::Value>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// 评估单页或整篇文本的有效字符密度与乱码率。
///
/// 返回 `(valid_char_count, garbled_ratio, is_native_ok)`：
/// - 去除空白字符后统计有效字符数与乱码字符（`\u{FFFD}` 或不可打印控制字符）比例；
/// - 有效字符 `>= 80` 且乱码率 `< 0.05` 时判定可直接走一级 `NativeTextLayer`。
pub fn evaluate_page_text_quality(text: &str) -> (usize, f64, bool) {
    let mut non_ws_total: usize = 0;
    let mut garbled_count: usize = 0;

    for ch in text.chars() {
        if ch.is_whitespace() {
            continue;
        }
        non_ws_total += 1;
        if ch == '\u{FFFD}' || ch.is_control() {
            garbled_count += 1;
        }
    }

    if non_ws_total == 0 {
        return (0, 0.0, false);
    }

    let valid_chars = non_ws_total.saturating_sub(garbled_count);
    let garbled_ratio = garbled_count as f64 / non_ws_total as f64;
    let is_native_ok = valid_chars >= 80 && garbled_ratio < 0.05;
    (valid_chars, garbled_ratio, is_native_ok)
}

/// 统一入口：根据 MIME 与文件扩展名解析 PDF / DOCX / CSV / TSV / 图像 / 文本代码。
pub async fn ingest_attachment(
    filename: &str,
    mime_type: Option<&str>,
    bytes: &[u8],
    options: &IngestOptions,
) -> Result<IngestedAttachment> {
    let attachment_id = format!("att_{}", Uuid::new_v4().simple());
    let resolved_mime = infer_mime_type(filename, mime_type);
    let lower_name = filename.to_ascii_lowercase();

    let mut result = if resolved_mime == "application/pdf" || lower_name.ends_with(".pdf") {
        ingest_pdf(&attachment_id, filename, &resolved_mime, bytes, options).await?
    } else if resolved_mime
        == "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        || lower_name.ends_with(".docx")
    {
        ingest_docx(&attachment_id, filename, &resolved_mime, bytes).await?
    } else if resolved_mime == "text/csv"
        || resolved_mime == "text/tab-separated-values"
        || lower_name.ends_with(".csv")
        || lower_name.ends_with(".tsv")
    {
        let is_tsv = resolved_mime == "text/tab-separated-values" || lower_name.ends_with(".tsv");
        ingest_delimited_table(&attachment_id, filename, &resolved_mime, bytes, is_tsv)?
    } else if resolved_mime.starts_with("image/")
        || lower_name.ends_with(".png")
        || lower_name.ends_with(".jpg")
        || lower_name.ends_with(".jpeg")
        || lower_name.ends_with(".webp")
    {
        ingest_image(&attachment_id, filename, &resolved_mime, bytes, options).await?
    } else {
        ingest_plain_text(&attachment_id, filename, &resolved_mime, bytes)
    };

    if !result.extracted_text.trim().is_empty() {
        result.figure_numerals =
            figure_marks::extract_figure_numerals(&result.attachment_id, &result.extracted_text);
    }

    Ok(result)
}

fn infer_mime_type(filename: &str, provided: Option<&str>) -> String {
    if let Some(m) = provided {
        let trimmed = m.trim().to_ascii_lowercase();
        if !trimmed.is_empty() && trimmed != "application/octet-stream" {
            return trimmed;
        }
    }
    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".pdf") {
        "application/pdf".to_string()
    } else if lower.ends_with(".docx") {
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_string()
    } else if lower.ends_with(".csv") {
        "text/csv".to_string()
    } else if lower.ends_with(".tsv") {
        "text/tab-separated-values".to_string()
    } else if lower.ends_with(".png") {
        "image/png".to_string()
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg".to_string()
    } else if lower.ends_with(".webp") {
        "image/webp".to_string()
    } else if lower.ends_with(".md") {
        "text/markdown".to_string()
    } else if lower.ends_with(".json") {
        "application/json".to_string()
    } else {
        "text/plain".to_string()
    }
}

async fn ingest_pdf(
    attachment_id: &str,
    filename: &str,
    mime_type: &str,
    bytes: &[u8],
    options: &IngestOptions,
) -> Result<IngestedAttachment> {
    let owned_bytes = bytes.to_vec();
    let pages_res = tokio::task::spawn_blocking(move || {
        match pdf_extract::extract_text_from_mem_by_pages(&owned_bytes) {
            Ok(pages) if !pages.is_empty() => Ok(pages),
            _ => pdf_extract::extract_text_from_mem(&owned_bytes).map(|full| {
                if full.contains('\x0c') {
                    full.split('\x0c').map(str::to_string).collect::<Vec<_>>()
                } else {
                    vec![full]
                }
            }),
        }
    })
    .await
    .map_err(|e| anyhow!("PDF 解析任务调度失败: {e}"))?;

    let pages = match pages_res {
        Ok(p) if !p.is_empty() => p,
        Ok(_) => vec![String::new()],
        Err(e) => {
            tracing::warn!("pdf_extract failed for {filename}: {e}");
            vec![String::new()]
        }
    };

    let page_count = pages.len().max(1);
    let mut native_pages = Vec::new();
    let mut low_density_indices = Vec::new();

    for (idx, page_text) in pages.iter().enumerate() {
        let (_valid, _ratio, is_ok) = evaluate_page_text_quality(page_text);
        if is_ok {
            native_pages.push(page_text.trim().to_string());
        } else {
            low_density_indices.push(idx + 1);
        }
    }

    // 全部页面均满足一级原生文字层条件
    if low_density_indices.is_empty() && !native_pages.is_empty() {
        return Ok(IngestedAttachment {
            attachment_id: attachment_id.to_string(),
            filename: filename.to_string(),
            mime_type: mime_type.to_string(),
            extracted_text: native_pages.join("\n\n---\n\n"),
            tier_used: IngestTierUsed::NativeTextLayer,
            page_count,
            figure_numerals: Vec::new(),
            structured_summary_json: Some(serde_json::json!({
                "kind": "pdf",
                "page_count": page_count,
                "native_pages": page_count,
                "low_density_pages": []
            })),
            warnings: Vec::new(),
        });
    }

    // 存在扫描页/低密度页：提取 PDF 内嵌图像供二级 Umi-OCR 或三级 Vision 回退使用
    let embedded_images = extract_embedded_images_from_pdf(bytes);
    let mut warnings = Vec::new();
    if !low_density_indices.is_empty() {
        warnings.push(format!(
            "检测到 {} 个低密度/扫描页（页码 {:?}），已触发多级 OCR 自适应解析链",
            low_density_indices.len(),
            low_density_indices
        ));
    }

    // 二级：探活本机 Umi-OCR（500ms connect_timeout）
    let umi_base = options
        .umi_ocr_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("http://127.0.0.1:1224")
        .trim_end_matches('/');

    if probe_umi_ocr(umi_base).await {
        let mut ocr_texts = Vec::new();
        for (idx, (_img_mime, img_bytes)) in embedded_images.iter().take(12).enumerate() {
            match call_umi_ocr(umi_base, img_bytes).await {
                Ok(txt) if !txt.trim().is_empty() => {
                    ocr_texts.push(format!("[Umi-OCR 图像 {}]\n{}", idx + 1, txt.trim()));
                }
                Ok(_) => {}
                Err(e) => warnings.push(format!("Umi-OCR 图像 {} 识别失败: {e}", idx + 1)),
            }
        }
        if !ocr_texts.is_empty() {
            let mut combined = native_pages;
            combined.extend(ocr_texts);
            return Ok(IngestedAttachment {
                attachment_id: attachment_id.to_string(),
                filename: filename.to_string(),
                mime_type: mime_type.to_string(),
                extracted_text: combined.join("\n\n---\n\n"),
                tier_used: IngestTierUsed::LocalUmiOcr,
                page_count,
                figure_numerals: Vec::new(),
                structured_summary_json: Some(serde_json::json!({
                    "kind": "pdf",
                    "page_count": page_count,
                    "embedded_images_found": embedded_images.len(),
                    "low_density_pages": low_density_indices
                })),
                warnings,
            });
        }
    }

    // 三级：仅在涉密离线锁关闭 (!privacy_offline_only) 且配置了看图模型时，才允许调用外部 Vision LLM
    if !options.privacy_offline_only {
        if let Some((base_url, api_key, model)) = &options.vision_endpoint {
            if !embedded_images.is_empty() {
                let mut vision_texts = Vec::new();
                for (idx, (img_mime, img_bytes)) in embedded_images.iter().take(6).enumerate() {
                    match call_vision_llm(base_url, api_key, model, img_mime, img_bytes).await {
                        Ok(txt) if !txt.trim().is_empty() => {
                            vision_texts.push(format!(
                                "[Vision 转录图像 {}]\n{}",
                                idx + 1,
                                txt.trim()
                            ));
                        }
                        Ok(_) => {}
                        Err(e) => {
                            warnings.push(format!("Vision 看图图像 {} 转录失败: {e}", idx + 1));
                        }
                    }
                }
                if !vision_texts.is_empty() {
                    let mut combined = native_pages;
                    combined.extend(vision_texts);
                    return Ok(IngestedAttachment {
                        attachment_id: attachment_id.to_string(),
                        filename: filename.to_string(),
                        mime_type: mime_type.to_string(),
                        extracted_text: combined.join("\n\n---\n\n"),
                        tier_used: IngestTierUsed::VisionLlmFallback,
                        page_count,
                        figure_numerals: Vec::new(),
                        structured_summary_json: Some(serde_json::json!({
                            "kind": "pdf",
                            "page_count": page_count,
                            "embedded_images_found": embedded_images.len(),
                            "low_density_pages": low_density_indices
                        })),
                        warnings,
                    });
                }
            }
        }
    } else {
        warnings.push(
            "涉密离线锁 (privacy_offline_only=true) 已开启：已拦截外部 Vision 大模型网络调用。"
                .to_string(),
        );
    }

    // 降级占位：优雅返回结构化提示而非报错崩溃
    let mut combined = native_pages;
    let placeholder = if options.privacy_offline_only {
        format!(
            "[扫描件降级占位] 文件 `{filename}` 含低密度扫描页（{page_count} 页，内嵌图 {} 张）。涉密离线锁已启用且本机 Umi-OCR 未在线，已跳过外部看图大模型调用；请启动本机 Umi-OCR 或补充文本摘要。",
            embedded_images.len()
        )
    } else {
        format!(
            "[扫描件降级占位] 文件 `{filename}` 含低密度扫描页（{page_count} 页，内嵌图 {} 张）。未检测到本机 Umi-OCR，且未配置可用的看图大模型或无可转录内嵌图；请启动 Umi-OCR 或配置 Vision 模型后重试。",
            embedded_images.len()
        )
    };
    combined.push(placeholder);

    Ok(IngestedAttachment {
        attachment_id: attachment_id.to_string(),
        filename: filename.to_string(),
        mime_type: mime_type.to_string(),
        extracted_text: combined.join("\n\n---\n\n"),
        tier_used: IngestTierUsed::DegradedPlaceholder,
        page_count,
        figure_numerals: Vec::new(),
        structured_summary_json: Some(serde_json::json!({
            "kind": "pdf",
            "page_count": page_count,
            "embedded_images_found": embedded_images.len(),
            "low_density_pages": low_density_indices,
            "privacy_offline_only": options.privacy_offline_only,
            "degraded": true
        })),
        warnings,
    })
}

/// 纯 Rust 扫描 PDF 字节流内嵌 JPEG (`FF D8 FF`..`FF D9`) 与 PNG (`89 50 4E 47`..`IEND`) 字节块。
pub fn extract_embedded_images_from_pdf(pdf_bytes: &[u8]) -> Vec<(&'static str, Vec<u8>)> {
    let mut images = Vec::new();

    // 1. 扫描 JPEG (FF D8 FF ... FF D9)
    let mut pos = 0usize;
    while pos + 3 < pdf_bytes.len() {
        let found_start = pdf_bytes
            .get(pos..)
            .and_then(|tail| {
                tail.windows(3).position(|w| {
                    w.first() == Some(&0xFF) && w.get(1) == Some(&0xD8) && w.get(2) == Some(&0xFF)
                })
            })
            .map(|rel| pos + rel);

        let Some(start) = found_start else {
            break;
        };

        let search_from = start + 3;
        let found_end = pdf_bytes.get(search_from..).and_then(|tail| {
            tail.windows(2)
                .position(|w| w.first() == Some(&0xFF) && w.get(1) == Some(&0xD9))
                .map(|rel| search_from + rel + 2)
        });

        if let Some(end) = found_end {
            let len = end.saturating_sub(start);
            if (16..=10 * 1024 * 1024).contains(&len) {
                if let Some(slice) = pdf_bytes.get(start..end) {
                    images.push(("image/jpeg", slice.to_vec()));
                }
            }
            pos = end;
        } else {
            break;
        }
    }

    // 2. 扫描 PNG (89 50 4E 47 0D 0A 1A 0A ... 49 45 4E 44 AE 42 60 82)
    const PNG_SIG: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    const PNG_IEND: [u8; 8] = [0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82];
    let mut png_pos = 0usize;
    while png_pos + 8 < pdf_bytes.len() {
        let found_start = pdf_bytes
            .get(png_pos..)
            .and_then(|tail| tail.windows(8).position(|w| w == PNG_SIG))
            .map(|rel| png_pos + rel);

        let Some(start) = found_start else {
            break;
        };

        let search_from = start + 8;
        let found_end = pdf_bytes
            .get(search_from..)
            .and_then(|tail| tail.windows(8).position(|w| w == PNG_IEND))
            .map(|rel| search_from + rel + 8);

        if let Some(end) = found_end {
            let len = end.saturating_sub(start);
            if (24..=10 * 1024 * 1024).contains(&len) {
                if let Some(slice) = pdf_bytes.get(start..end) {
                    images.push(("image/png", slice.to_vec()));
                }
            }
            png_pos = end;
        } else {
            break;
        }
    }

    images
}

async fn probe_umi_ocr(base_url: &str) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(500))
        .timeout(Duration::from_millis(800))
        .build()
    else {
        return false;
    };
    let url = format!("{}/api/ocr/get_options", base_url.trim_end_matches('/'));
    match client.get(url).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

async fn call_umi_ocr(base_url: &str, image_bytes: &[u8]) -> Result<String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(500))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| anyhow!("构建 Umi-OCR HTTP 客户端失败: {e}"))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(image_bytes);
    let url = format!("{}/api/ocr", base_url.trim_end_matches('/'));
    let resp = client
        .post(url)
        .json(&serde_json::json!({
            "base64": b64,
            "options": {
                "data.format": "text",
                "tbpu.parser": "multi_para"
            }
        }))
        .send()
        .await
        .map_err(|e| anyhow!("Umi-OCR 请求失败: {e}"))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow!("Umi-OCR 响应解析失败: {e}"))?;

    match body.get("code").and_then(|v| v.as_i64()) {
        Some(100) => Ok(body
            .get("data")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()),
        Some(code) => Err(anyhow!("Umi-OCR 返回状态码 {code}")),
        None => Err(anyhow!("Umi-OCR 响应缺少 code 字段")),
    }
}

fn normalize_chat_completions_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v1/chat/completions")
    }
}

async fn call_vision_llm(
    base_url: &str,
    api_key: &str,
    model: &str,
    image_mime: &str,
    image_bytes: &[u8],
) -> Result<String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| anyhow!("构建 Vision HTTP 客户端失败: {e}"))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(image_bytes);
    let data_url = format!("data:{image_mime};base64,{b64}");
    let endpoint = normalize_chat_completions_url(base_url);

    let payload = serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": "请准确转录图中的全部技术文字、公式、实验表格数值以及专利附图部件标号（格式如 10-壳体）。"
                    },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": data_url
                        }
                    }
                ]
            }
        ],
        "temperature": 0.1
    });

    let mut req = client.post(endpoint).json(&payload);
    if !api_key.trim().is_empty() {
        req = req.bearer_auth(api_key.trim());
    }

    let resp = req
        .send()
        .await
        .map_err(|e| anyhow!("Vision LLM 请求失败: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Vision LLM 返回异常状态 {status}: {text}"));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow!("Vision LLM 响应 JSON 解析失败: {e}"))?;

    let content = json
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|first| first.get("message"))
        .and_then(|msg| msg.get("content"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| anyhow!("Vision LLM 响应缺少 choices[0].message.content"))?;

    Ok(content.to_string())
}

async fn ingest_docx(
    attachment_id: &str,
    filename: &str,
    mime_type: &str,
    bytes: &[u8],
) -> Result<IngestedAttachment> {
    let owned = bytes.to_vec();
    let extracted = tokio::task::spawn_blocking(move || -> Result<String> {
        let reader = Cursor::new(owned);
        let mut archive =
            zip::ZipArchive::new(reader).map_err(|e| anyhow!("DOCX ZIP 解析失败: {e}"))?;
        let mut doc_xml = archive
            .by_name("word/document.xml")
            .map_err(|e| anyhow!("DOCX 缺少 word/document.xml: {e}"))?;
        let mut xml_str = String::new();
        doc_xml
            .read_to_string(&mut xml_str)
            .map_err(|e| anyhow!("读取 word/document.xml 失败: {e}"))?;
        Ok(extract_text_from_word_xml(&xml_str))
    })
    .await
    .map_err(|e| anyhow!("DOCX 解析任务调度失败: {e}"))??;

    Ok(IngestedAttachment {
        attachment_id: attachment_id.to_string(),
        filename: filename.to_string(),
        mime_type: mime_type.to_string(),
        extracted_text: extracted,
        tier_used: IngestTierUsed::NativeTextLayer,
        page_count: 1,
        figure_numerals: Vec::new(),
        structured_summary_json: Some(serde_json::json!({ "kind": "docx" })),
        warnings: Vec::new(),
    })
}

fn extract_text_from_word_xml(xml: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    let mut current_tag = String::new();
    let mut text_buf = String::new();

    for ch in xml.chars() {
        if ch == '<' {
            if !text_buf.is_empty() {
                out.push_str(&decode_xml_entities(&text_buf));
                text_buf.clear();
            }
            in_tag = true;
            current_tag.clear();
        } else if ch == '>' && in_tag {
            in_tag = false;
            let tag_name = current_tag.split_whitespace().next().unwrap_or("");
            if tag_name == "/w:p"
                || tag_name == "/w:tr"
                || tag_name == "w:br"
                || tag_name == "w:br/"
            {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
            } else if (tag_name == "/w:tc" || tag_name == "w:tab" || tag_name == "w:tab/")
                && !out.ends_with('\t')
                && !out.ends_with('\n')
            {
                out.push('\t');
            }
        } else if in_tag {
            current_tag.push(ch);
        } else {
            text_buf.push(ch);
        }
    }
    if !text_buf.is_empty() {
        out.push_str(&decode_xml_entities(&text_buf));
    }

    out.lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn decode_xml_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn ingest_delimited_table(
    attachment_id: &str,
    filename: &str,
    mime_type: &str,
    bytes: &[u8],
    is_tsv: bool,
) -> Result<IngestedAttachment> {
    let text = decode_text_safe(bytes);
    let delimiter = if is_tsv { '\t' } else { ',' };
    let mut rows: Vec<Vec<String>> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        rows.push(parse_delimited_row(trimmed, delimiter));
    }

    if rows.is_empty() {
        return Err(anyhow!("实验数据表 `{filename}` 为空"));
    }

    let headers = rows.first().cloned().unwrap_or_default();
    let data_rows = rows.get(1..).unwrap_or(&[]);
    let mut numeric_stats = serde_json::Map::new();
    let mut metric_lines = Vec::new();

    for (col_idx, header) in headers.iter().enumerate() {
        let mut vals = Vec::new();
        for row in data_rows {
            if let Some(cell) = row.get(col_idx) {
                let cleaned = cell.trim().trim_end_matches('%');
                if let Ok(num) = cleaned.parse::<f64>() {
                    if num.is_finite() {
                        vals.push(num);
                    }
                }
            }
        }
        if !vals.is_empty() {
            let count = vals.len();
            let sum: f64 = vals.iter().sum();
            let mean = sum / count as f64;
            let min = vals.iter().copied().fold(f64::INFINITY, f64::min);
            let max = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            numeric_stats.insert(
                header.clone(),
                serde_json::json!({
                    "count": count,
                    "min": min,
                    "max": max,
                    "mean": mean
                }),
            );
            metric_lines.push(format!(
                "{header}: n={count}, min={min:.4}, max={max:.4}, mean={mean:.4}"
            ));
        }
    }

    let summary_json = serde_json::json!({
        "kind": if is_tsv { "tsv" } else { "csv" },
        "headers": headers,
        "row_count": data_rows.len(),
        "numeric_stats": numeric_stats,
        "sample_rows": data_rows.iter().take(5).cloned().collect::<Vec<_>>()
    });

    let extracted_text = format!(
        "实验数据表 `{filename}`（共 {} 行，列：{}）\n统计摘要：\n{}",
        data_rows.len(),
        headers.join(" | "),
        if metric_lines.is_empty() {
            text.lines().take(10).collect::<Vec<_>>().join("\n")
        } else {
            metric_lines.join("\n")
        }
    );

    Ok(IngestedAttachment {
        attachment_id: attachment_id.to_string(),
        filename: filename.to_string(),
        mime_type: mime_type.to_string(),
        extracted_text,
        tier_used: IngestTierUsed::NativeTextLayer,
        page_count: 1,
        figure_numerals: Vec::new(),
        structured_summary_json: Some(summary_json),
        warnings: Vec::new(),
    })
}

fn parse_delimited_row(line: &str, delimiter: char) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;

    for ch in line.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
        } else if ch == delimiter && !in_quotes {
            cells.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    cells.push(cur.trim().to_string());
    cells
}

fn validate_image_magic(bytes: &[u8], mime_type: &str) -> Result<&'static str> {
    let is_png = bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    let is_jpeg = bytes.starts_with(&[0xFF, 0xD8, 0xFF]);
    let is_webp = bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP");

    if is_png {
        Ok("image/png")
    } else if is_jpeg {
        Ok("image/jpeg")
    } else if is_webp {
        Ok("image/webp")
    } else {
        Err(anyhow!(
            "图像魔数校验失败（期望 {mime_type}，但文件头不匹配 PNG/JPEG/WEBP）"
        ))
    }
}

async fn ingest_image(
    attachment_id: &str,
    filename: &str,
    mime_type: &str,
    bytes: &[u8],
    options: &IngestOptions,
) -> Result<IngestedAttachment> {
    let actual_mime = validate_image_magic(bytes, mime_type)?;
    let mut warnings = Vec::new();

    let umi_base = options
        .umi_ocr_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("http://127.0.0.1:1224")
        .trim_end_matches('/');

    if probe_umi_ocr(umi_base).await {
        if let Ok(txt) = call_umi_ocr(umi_base, bytes).await {
            if !txt.trim().is_empty() {
                return Ok(IngestedAttachment {
                    attachment_id: attachment_id.to_string(),
                    filename: filename.to_string(),
                    mime_type: actual_mime.to_string(),
                    extracted_text: txt,
                    tier_used: IngestTierUsed::LocalUmiOcr,
                    page_count: 1,
                    figure_numerals: Vec::new(),
                    structured_summary_json: Some(serde_json::json!({
                        "kind": "image",
                        "mime": actual_mime,
                        "byte_size": bytes.len()
                    })),
                    warnings,
                });
            }
        }
    }

    if !options.privacy_offline_only {
        if let Some((base_url, api_key, model)) = &options.vision_endpoint {
            match call_vision_llm(base_url, api_key, model, actual_mime, bytes).await {
                Ok(txt) if !txt.trim().is_empty() => {
                    return Ok(IngestedAttachment {
                        attachment_id: attachment_id.to_string(),
                        filename: filename.to_string(),
                        mime_type: actual_mime.to_string(),
                        extracted_text: txt,
                        tier_used: IngestTierUsed::VisionLlmFallback,
                        page_count: 1,
                        figure_numerals: Vec::new(),
                        structured_summary_json: Some(serde_json::json!({
                            "kind": "image",
                            "mime": actual_mime,
                            "byte_size": bytes.len()
                        })),
                        warnings,
                    });
                }
                Ok(_) => {}
                Err(e) => warnings.push(format!("Vision 看图转录失败: {e}")),
            }
        }
    } else {
        warnings.push(
            "涉密离线锁已开启：图像未发送至外部看图模型，仅记录图像元信息供多轮会话挂载。"
                .to_string(),
        );
    }

    Ok(IngestedAttachment {
        attachment_id: attachment_id.to_string(),
        filename: filename.to_string(),
        mime_type: actual_mime.to_string(),
        extracted_text: format!(
            "[图像附件元信息] 文件：{filename}，格式：{actual_mime}，大小：{} 字节",
            bytes.len()
        ),
        tier_used: IngestTierUsed::DegradedPlaceholder,
        page_count: 1,
        figure_numerals: Vec::new(),
        structured_summary_json: Some(serde_json::json!({
            "kind": "image",
            "mime": actual_mime,
            "byte_size": bytes.len(),
            "privacy_offline_only": options.privacy_offline_only
        })),
        warnings,
    })
}

fn ingest_plain_text(
    attachment_id: &str,
    filename: &str,
    mime_type: &str,
    bytes: &[u8],
) -> IngestedAttachment {
    let text = decode_text_safe(bytes);
    IngestedAttachment {
        attachment_id: attachment_id.to_string(),
        filename: filename.to_string(),
        mime_type: mime_type.to_string(),
        extracted_text: text,
        tier_used: IngestTierUsed::NativeTextLayer,
        page_count: 1,
        figure_numerals: Vec::new(),
        structured_summary_json: Some(
            serde_json::json!({ "kind": "text", "byte_size": bytes.len() }),
        ),
        warnings: Vec::new(),
    }
}

/// UTF-8 优先、失败回退 GB18030 的安全文本解码。
pub fn decode_text_safe(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            let (cow, _enc, _had_errors) = encoding_rs::GB18030.decode(bytes);
            cow.into_owned()
        }
    }
}
