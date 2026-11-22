//! 专利 PDF 真实单张附图提取引擎 / Real Patent PDF Drawing Extractor
//!
//! 严格遵循 AGENTS.md §2.8 工程真实性铁律：
//! 1. 优先从 PDF 页面资源字典 (`/Resources -> /XObject`) 中提取真实嵌入的图像对象 (`Subtype /Image`)；
//! 2. 自动按尺寸与面积阈值过滤顶部条形码、二维码、专利局徽标、印章及行内微小公式符号；
//! 3. 严格识别并跳过封面文字区、`权利要求书` 及纯文字 `说明书` 页，绝不把文字页当成附图输出；
//! 4. 对无文本层的旧版扫描件专利 PDF（如 `/CCITTFaxDecode` Group 4 传真流），先在纯 Rust 中解码位图，
//!    通过版面文字行周期性密度特征剔除封面、权利要求书与说明书文字页，仅提取末尾 `说明书附图` 页的附图区域。

use base64::Engine;
use pdf_extract::{Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};

/// 单张提取出的专利附图
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtractedPdfDrawing {
    /// 所在 PDF 页码（1-based）
    pub page_number: u32,
    /// 附图序号（1-based）
    pub figure_index: usize,
    /// 图像像素宽度
    pub width: u32,
    /// 图像像素高度
    pub height: u32,
    /// MIME 类型 (`image/jpeg` 或 `image/png`)
    pub mime_type: String,
    /// 可直接用于 `<img>` 或看图模型的 Base64 Data URL
    pub data_url: String,
}

/// 最小附图宽度（过滤小印章、公式符号、小图标）
const MIN_DRAWING_WIDTH: u32 = 160;
/// 最小附图高度（过滤顶部一维条形码、分隔横线）
const MIN_DRAWING_HEIGHT: u32 = 120;
/// 最小附图像素面积（过滤二维码与页眉徽标）
const MIN_DRAWING_AREA: u64 = 25_000;
/// 最大长宽比（过滤细长条形码或装饰线）
const MAX_ASPECT_RATIO: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageRole {
    /// 明确的「说明书附图 / DRAWINGS」页
    DrawingSheet,
    /// 明确的封面、权利要求书或纯文字说明书页（严禁当成附图）
    TextOrCover,
    /// 未带显式页眉标签的普通页
    Neutral,
}

/// 根据单页提取文本判定该页的角色
fn classify_page_text(page_num: u32, raw_text: &str) -> PageRole {
    let compact: String = raw_text.chars().filter(|c| !c.is_whitespace()).collect();
    let char_count = compact.chars().count();
    let upper = compact.to_ascii_uppercase();

    // 1. 明确标有「说明书附图」页眉（且非正文中“详见说明书附图”长段落）
    if compact.contains("说明书附图") && char_count < 320 {
        return PageRole::DrawingSheet;
    }
    if (upper.contains("DRAWINGS") || upper.contains("DRAWING"))
        && !upper.contains("BRIEFDESCRIPTIONOFTHEDRAWINGS")
        && char_count < 260
    {
        return PageRole::DrawingSheet;
    }

    // 2. 明确的权利要求书页
    if compact.contains("权利要求书") || upper.contains("WHATISCLAIMED") || upper.contains("CLAIMS")
    {
        return PageRole::TextOrCover;
    }

    // 3. 明确的专利扉页/封面页（含 INID 代码或发明名称/摘要著录项）
    if page_num == 1
        && (compact.contains("(19)")
            || compact.contains("(12)")
            || compact.contains("(54)")
            || compact.contains("(57)")
            || compact.contains("申请公布号")
            || compact.contains("授权公告号")
            || compact.contains("实用新型名称")
            || compact.contains("发明名称"))
    {
        return PageRole::TextOrCover;
    }

    // 4. 明确的纯文字说明书页（含“技术领域/背景技术/发明内容/具体实施方式”或文字量 > 240 字）
    if compact.contains("技术领域")
        || compact.contains("背景技术")
        || compact.contains("具体实施方式")
        || compact.contains("附图说明")
        || (compact.contains("说明书") && !compact.contains("说明书附图"))
        || char_count > 240
    {
        return PageRole::TextOrCover;
    }

    // 5. 仅含「图1 / 图2 / FIG. 1」短标签的附图页
    if (compact.contains("图1")
        || compact.contains("图2")
        || compact.contains("图3")
        || upper.contains("FIG.1")
        || upper.contains("FIG.2")
        || upper.contains("FIGURE1"))
        && char_count > 0
        && char_count <= 180
    {
        return PageRole::DrawingSheet;
    }

    PageRole::Neutral
}

/// 判断尺寸是否符合专利附图要求（剔除条形码、印章、二维码、公式小字符）
pub fn is_valid_drawing_dimensions(width: u32, height: u32) -> bool {
    if width < MIN_DRAWING_WIDTH || height < MIN_DRAWING_HEIGHT {
        return false;
    }
    let area = (width as u64) * (height as u64);
    if area < MIN_DRAWING_AREA {
        return false;
    }
    if width > height.saturating_mul(MAX_ASPECT_RATIO)
        || height > width.saturating_mul(MAX_ASPECT_RATIO)
    {
        return false;
    }
    true
}

/// 从 PDF 字节流中提取真实的单张专利附图
pub fn extract_drawings_from_pdf_bytes(
    pdf_bytes: &[u8],
) -> Result<Vec<ExtractedPdfDrawing>, String> {
    if pdf_bytes.len() < 8 || !pdf_bytes.starts_with(b"%PDF-") {
        return Err("无效的 PDF 文件头".to_string());
    }

    let doc = Document::load_mem(pdf_bytes).map_err(|e| format!("PDF 解析失败: {e}"))?;
    let pages = doc.get_pages();
    if pages.is_empty() {
        return Ok(Vec::new());
    }

    let total_pages = pages.len() as u32;
    let mut page_roles: Vec<(u32, ObjectId, PageRole, usize)> = Vec::with_capacity(pages.len());
    let mut total_text_chars = 0usize;

    for (&page_num, &page_id) in &pages {
        let page_text = doc.extract_text(&[page_num]).unwrap_or_default();
        let non_ws_chars = page_text.chars().filter(|c| !c.is_whitespace()).count();
        total_text_chars += non_ws_chars;
        let role = classify_page_text(page_num, &page_text);
        page_roles.push((page_num, page_id, role, non_ws_chars));
    }

    let has_explicit_drawing_sheets = page_roles
        .iter()
        .any(|(_, _, role, _)| *role == PageRole::DrawingSheet);

    // 收集每页上的候选 Image XObject
    struct CandidateImage {
        page_num: u32,
        page_role: PageRole,
        page_text_chars: usize,
        obj_id: ObjectId,
        width: u32,
        height: u32,
        is_ccitt_full_page: bool,
    }

    let mut candidates: Vec<CandidateImage> = Vec::new();
    for &(page_num, page_id, role, text_chars) in &page_roles {
        let image_ids = collect_page_image_xobjects(&doc, page_id);
        for obj_id in image_ids {
            if let Ok(Object::Stream(ref stream)) = doc.get_object(obj_id) {
                let (w, h) = extract_stream_dimensions(stream);
                if !is_valid_drawing_dimensions(w, h) {
                    continue;
                }
                let filter_name = extract_filter_name(&doc, stream);
                let is_ccitt_full_page =
                    filter_name.contains("CCITTFaxDecode") && w >= 1200 && h >= 1600;
                candidates.push(CandidateImage {
                    page_num,
                    page_role: role,
                    page_text_chars: text_chars,
                    obj_id,
                    width: w,
                    height: h,
                    is_ccitt_full_page,
                });
            }
        }
    }

    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    // 判断是否为「全页无文本层扫描件 PDF」（每页 1 张全页扫描图且全书无文本层）
    let all_ccitt_or_fullpage_scan = total_text_chars < 50
        && total_pages >= 2
        && candidates
            .iter()
            .all(|c| c.is_ccitt_full_page || (c.width >= 1200 && c.height >= 1600));

    let mut results: Vec<ExtractedPdfDrawing> = Vec::new();
    let mut seen_objects: HashSet<ObjectId> = HashSet::new();

    if all_ccitt_or_fullpage_scan {
        // 策略 B：无文本层扫描件专利 PDF —— 严格剔除扉页（第 1 页）与文字密集页，仅提取说明书附图页
        for cand in &candidates {
            if cand.page_num == 1 && total_pages > 1 {
                // 专利第 1 页固定为著录项目扉页，绝不当作说明书附图页输出
                continue;
            }
            let Ok(Object::Stream(ref stream)) = doc.get_object(cand.obj_id) else {
                continue;
            };
            if let Some((mime, data_url, out_w, out_h)) =
                decode_scanned_drawing_page_if_non_text(&doc, stream, cand.width, cand.height)
            {
                let fig_idx = results.len() + 1;
                results.push(ExtractedPdfDrawing {
                    page_number: cand.page_num,
                    figure_index: fig_idx,
                    width: out_w,
                    height: out_h,
                    mime_type: mime,
                    data_url,
                });
            }
        }
        return Ok(results);
    }

    // 策略 A：带文本层或嵌入式单张附图的 PDF
    for cand in &candidates {
        if has_explicit_drawing_sheets {
            // 若文档中已识别出明确的「说明书附图」专属页，则仅提取说明书附图页上的图像
            if cand.page_role != PageRole::DrawingSheet {
                continue;
            }
        } else {
            // 否则严格排除封面页、权利要求书页及文字量 > 240 字的纯文字说明书页
            if cand.page_role == PageRole::TextOrCover || cand.page_text_chars > 240 {
                continue;
            }
        }

        if !seen_objects.insert(cand.obj_id) {
            continue;
        }

        let Ok(Object::Stream(ref stream)) = doc.get_object(cand.obj_id) else {
            continue;
        };

        if let Some((mime, bytes, out_w, out_h)) =
            decode_pdf_image_stream(&doc, stream, cand.width, cand.height)
        {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            let data_url = format!("data:{mime};base64,{b64}");
            let fig_idx = results.len() + 1;
            results.push(ExtractedPdfDrawing {
                page_number: cand.page_num,
                figure_index: fig_idx,
                width: out_w,
                height: out_h,
                mime_type: mime,
                data_url,
            });
        }
    }

    Ok(results)
}

/// 收集指定 PDF 页（含其 Form XObject 子资源）引用的所有 `/Subtype /Image` 对象 ID
fn collect_page_image_xobjects(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let mut image_ids = Vec::new();
    let mut visited = BTreeSet::new();
    let Ok((resource_dict, resource_ids)) = doc.get_page_resources(page_id) else {
        return image_ids;
    };

    if let Some(dict) = resource_dict {
        collect_xobjects_from_resources_dict(doc, dict, &mut image_ids, &mut visited);
    }
    for res_id in resource_ids {
        if let Ok(Object::Dictionary(ref dict)) = doc.get_object(res_id) {
            collect_xobjects_from_resources_dict(doc, dict, &mut image_ids, &mut visited);
        }
    }
    image_ids
}

fn collect_xobjects_from_resources_dict(
    doc: &Document,
    res_dict: &pdf_extract::Dictionary,
    out: &mut Vec<ObjectId>,
    visited: &mut BTreeSet<ObjectId>,
) {
    let Ok(xobj_entry) = res_dict.get(b"XObject") else {
        return;
    };
    let xobj_dict = match xobj_entry {
        Object::Reference(id) => match doc.get_object(*id) {
            Ok(Object::Dictionary(ref d)) => Some(d),
            _ => None,
        },
        Object::Dictionary(ref d) => Some(d),
        _ => None,
    };
    let Some(xobj_dict) = xobj_dict else {
        return;
    };

    // 按键值名排序以确保提取顺序稳定
    let mut entries: Vec<(&Vec<u8>, &Object)> = xobj_dict.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    for (_name, obj_ref) in entries {
        let Object::Reference(obj_id) = obj_ref else {
            continue;
        };
        if !visited.insert(*obj_id) {
            continue;
        }
        let Ok(Object::Stream(ref stream)) = doc.get_object(*obj_id) else {
            continue;
        };
        let subtype = stream
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| o.as_name_str().ok())
            .unwrap_or("");
        if subtype == "Image" {
            out.push(*obj_id);
        } else if subtype == "Form" {
            if let Ok(form_res) = stream.dict.get(b"Resources") {
                match form_res {
                    Object::Dictionary(ref d) => {
                        collect_xobjects_from_resources_dict(doc, d, out, visited);
                    }
                    Object::Reference(rid) => {
                        if let Ok(Object::Dictionary(ref d)) = doc.get_object(*rid) {
                            collect_xobjects_from_resources_dict(doc, d, out, visited);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn extract_stream_dimensions(stream: &Stream) -> (u32, u32) {
    let width = stream
        .dict
        .get(b"Width")
        .or_else(|_| stream.dict.get(b"Columns"))
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0)
        .max(0) as u32;
    let height = stream
        .dict
        .get(b"Height")
        .or_else(|_| stream.dict.get(b"Rows"))
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0)
        .max(0) as u32;
    (width, height)
}

fn extract_filter_name(doc: &Document, stream: &Stream) -> String {
    let Ok(filter_obj) = stream.dict.get(b"Filter") else {
        return String::new();
    };
    resolve_filter_names(doc, filter_obj)
}

fn resolve_filter_names(doc: &Document, obj: &Object) -> String {
    match obj {
        Object::Name(n) => String::from_utf8_lossy(n).to_string(),
        Object::Array(arr) => arr
            .iter()
            .map(|item| resolve_filter_names(doc, item))
            .collect::<Vec<_>>()
            .join(","),
        Object::Reference(id) => doc
            .get_object(*id)
            .map(|o| resolve_filter_names(doc, o))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// 将 PDF `/Subtype /Image` 流解码为浏览器可直接渲染的 `(mime_type, image_bytes, width, height)`
fn decode_pdf_image_stream(
    doc: &Document,
    stream: &Stream,
    width: u32,
    height: u32,
) -> Option<(String, Vec<u8>, u32, u32)> {
    let filter = extract_filter_name(doc, stream);

    // 1. DCTDecode (JPEG)：流内容本身即为标准 JPEG
    if filter.contains("DCTDecode") {
        let raw = if filter.starts_with("FlateDecode") {
            stream.decompressed_content().ok()?
        } else {
            stream.content.clone()
        };
        if raw.len() > 4 && raw[0] == 0xFF && raw[1] == 0xD8 {
            return Some(("image/jpeg".to_string(), raw, width, height));
        }
    }

    // 2. FlateDecode 或无压缩原始像素流：解码并封装为标准 PNG
    if filter.is_empty() || filter == "FlateDecode" {
        let raw_pixels = if filter == "FlateDecode" {
            stream.decompressed_content().ok()?
        } else {
            stream.content.clone()
        };

        let bpc = stream
            .dict
            .get(b"BitsPerComponent")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(8) as u8;
        let channels = infer_color_channels(doc, stream, &raw_pixels, width, height, bpc);
        let png_bytes = raw_pixels_to_png(&raw_pixels, width, height, bpc, channels)?;
        return Some(("image/png".to_string(), png_bytes, width, height));
    }

    // 3. CCITTFaxDecode (Group 4 1-bit 单色图)
    if filter.contains("CCITTFaxDecode") {
        let bitmap = decode_ccitt_group4(&stream.content, width as usize, height as usize)?;
        let png_bytes = bitmap_1bit_to_png(
            &bitmap,
            width as usize,
            height as usize,
            0,
            width as usize,
            0,
            height as usize,
        )?;
        return Some(("image/png".to_string(), png_bytes, width, height));
    }

    None
}

/// 针对无文本层扫描件页：先解码位图，检测是否为文字密集页（权利要求/说明书）；
/// 若非文字页（即真正的 `说明书附图` 页），裁掉顶部页眉与底部页码后输出 PNG。
fn decode_scanned_drawing_page_if_non_text(
    doc: &Document,
    stream: &Stream,
    width: u32,
    height: u32,
) -> Option<(String, String, u32, u32)> {
    let filter = extract_filter_name(doc, stream);
    if !filter.contains("CCITTFaxDecode") {
        return None;
    }
    let w = width as usize;
    let h = height as usize;
    let bitmap = decode_ccitt_group4(&stream.content, w, h)?;

    // 统计正文区（y: 12%..88%, x: 10%..90%）的水平文字行带数量
    let text_rows = count_text_rows_in_1bit_page(&bitmap, w, h);
    if text_rows >= 11 {
        // 权利要求书或说明书文字页（通常有 18~38 行密集汉字水平排布），严禁当成附图输出
        return None;
    }

    // 裁掉顶部 9% 页眉（「说明书附图」字样及横线）和底部 7% 页脚页码后，进一步自动紧贴墨迹外接框去除多余 A4 留白
    let base_top = (h * 9) / 100;
    let base_bottom = (h * 93) / 100;
    if base_bottom <= base_top + 100 {
        return None;
    }
    let (crop_x0, crop_x1, crop_y0, crop_y1) =
        auto_crop_1bit_ink_bounds(&bitmap, w, h, base_top, base_bottom);
    let out_w = (crop_x1 - crop_x0) as u32;
    let out_h = (crop_y1 - crop_y0) as u32;
    let png_bytes = bitmap_1bit_to_png(&bitmap, w, h, crop_x0, crop_x1, crop_y0, crop_y1)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    let data_url = format!("data:image/png;base64,{b64}");
    Some(("image/png".to_string(), data_url, out_w, out_h))
}

/// 在给定垂直区间内寻找有效墨迹外接框并保留 36px 舒朗白边，大幅提升老旧扫描件附图的可读性
fn auto_crop_1bit_ink_bounds(
    bitmap: &[u8],
    width: usize,
    height: usize,
    y_min_bound: usize,
    y_max_bound: usize,
) -> (usize, usize, usize, usize) {
    let x_margin_ignore = (width * 2) / 100; // 避开扫描仪边沿黑线噪点
    let x_scan_start = x_margin_ignore;
    let x_scan_end = width.saturating_sub(x_margin_ignore).max(x_scan_start + 1);
    let y_scan_end = y_max_bound.min(height);

    let mut min_x = x_scan_end;
    let mut max_x = x_scan_start;
    let mut min_y = y_scan_end;
    let mut max_y = y_min_bound;

    for y in y_min_bound..y_scan_end {
        let row = y * width;
        let mut row_black = 0usize;
        let mut row_min_x = x_scan_end;
        let mut row_max_x = x_scan_start;
        for x in x_scan_start..x_scan_end {
            if bitmap[row + x] != 0 {
                row_black += 1;
                if x < row_min_x {
                    row_min_x = x;
                }
                if x > row_max_x {
                    row_max_x = x;
                }
            }
        }
        // 至少 3 个黑像素才算有效内容行（过滤单点灰尘）
        if row_black >= 3 {
            if y < min_y {
                min_y = y;
            }
            if y > max_y {
                max_y = y;
            }
            if row_min_x < min_x {
                min_x = row_min_x;
            }
            if row_max_x > max_x {
                max_x = row_max_x;
            }
        }
    }

    if max_x <= min_x + 80 || max_y <= min_y + 80 {
        return (0, width, y_min_bound, y_scan_end);
    }
    let pad = 36usize;
    let x0 = min_x.saturating_sub(pad);
    let x1 = (max_x + pad + 1).min(width);
    let y0 = min_y.saturating_sub(pad).max(y_min_bound);
    let y1 = (max_y + pad + 1).min(y_scan_end);
    (x0, x1, y0, y1)
}

/// 统计 1-bit 页面位图正文区中具有印刷文字特征的水平文字行数
fn count_text_rows_in_1bit_page(bitmap: &[u8], width: usize, height: usize) -> usize {
    let y_start = (height * 12) / 100;
    let y_end = (height * 88) / 100;
    let x_start = (width * 10) / 100;
    let x_end = (width * 90) / 100;
    if y_end <= y_start || x_end <= x_start {
        return 0;
    }

    // 一行印刷汉字在扫描线上的黑白跳变次数通常 >= 38
    let min_transitions_for_text_line = ((x_end - x_start) / 48).max(28);
    let mut text_row_bands = 0usize;
    let mut current_band_height = 0usize;
    let mut gap_height = 0usize;

    for y in y_start..y_end {
        let row_offset = y * width;
        let mut transitions = 0usize;
        let mut prev = bitmap[row_offset + x_start];
        // 步长 2 采样加速且保留笔画跳变特征
        let mut x = x_start + 2;
        while x < x_end {
            let cur = bitmap[row_offset + x];
            if cur != prev {
                transitions += 1;
                prev = cur;
            }
            x += 2;
        }

        if transitions >= min_transitions_for_text_line {
            current_band_height += 1;
            gap_height = 0;
        } else {
            gap_height += 1;
            if gap_height >= 6 {
                // 标准 300DPI 印刷文字行高在 14..95 像素之间
                if (12..=100).contains(&current_band_height) {
                    text_row_bands += 1;
                }
                current_band_height = 0;
            }
        }
    }
    if (12..=100).contains(&current_band_height) {
        text_row_bands += 1;
    }

    text_row_bands
}

fn infer_color_channels(
    doc: &Document,
    stream: &Stream,
    raw: &[u8],
    width: u32,
    height: u32,
    bpc: u8,
) -> u8 {
    let pixels = (width as usize).saturating_mul(height as usize);
    if pixels > 0 && bpc == 8 {
        if raw.len() == pixels * 3 || raw.len() == pixels * 3 + height as usize {
            return 3;
        }
        if raw.len() == pixels * 4 || raw.len() == pixels * 4 + height as usize {
            return 4;
        }
        if raw.len() == pixels || raw.len() == pixels + height as usize {
            return 1;
        }
    }
    if let Ok(cs) = stream.dict.get(b"ColorSpace") {
        let cs_name = resolve_filter_names(doc, cs);
        if cs_name.contains("DeviceGray") {
            return 1;
        }
        if cs_name.contains("DeviceCMYK") {
            return 4;
        }
    }
    3
}

/// 纯 Rust RFC 1950 Zlib 封装器（零外部依赖，兼容所有标准 PNG 解码器）
fn zlib_compress_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + (data.len() / 65535 + 1) * 5 + 6);
    // Zlib header: CMF=0x78 (Deflate, 32K window), FLG=0x01 (check bits so 0x7801 % 31 == 0)
    out.push(0x78);
    out.push(0x01);

    if data.is_empty() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    } else {
        let chunks: Vec<&[u8]> = data.chunks(65_535).collect();
        for (idx, chunk) in chunks.iter().enumerate() {
            let is_last = idx + 1 == chunks.len();
            out.push(if is_last { 0x01 } else { 0x00 });
            let len = chunk.len() as u16;
            let nlen = !len;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&nlen.to_le_bytes());
            out.extend_from_slice(chunk);
        }
    }

    // Adler-32 checksum (big-endian)
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;
    for &b in data {
        s1 = (s1 + b as u32) % 65_521;
        s2 = (s2 + s1) % 65_521;
    }
    let adler = (s2 << 16) | s1;
    out.extend_from_slice(&adler.to_be_bytes());
    out
}

/// 将解压后的像素缓冲区封装为标准 PNG（支持 8-bit 灰度 / RGB / RGBA 及 1-bit 单色）
fn raw_pixels_to_png(
    raw: &[u8],
    width: u32,
    height: u32,
    bpc: u8,
    channels: u8,
) -> Option<Vec<u8>> {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return None;
    }

    if bpc == 1 {
        let row_bytes = w.div_ceil(8);
        if raw.len() < row_bytes * h {
            return None;
        }
        let mut gray = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let byte = raw[y * row_bytes + (x / 8)];
                let bit = (byte >> (7 - (x % 8))) & 1;
                gray.push(if bit == 0 { 0 } else { 255 });
            }
        }
        return encode_png_buffer(&gray, width, height, 1);
    }

    let bytes_per_pixel = (channels as usize).max(1);
    let row_len = w * bytes_per_pixel;

    // 兼容带 PNG Predictor 行过滤字节（每行首字节为 filter type）或裸像素缓冲
    let clean_pixels = if raw.len() == (row_len + 1) * h {
        let mut buf = Vec::with_capacity(row_len * h);
        for y in 0..h {
            let start = y * (row_len + 1) + 1;
            buf.extend(raw.iter().copied().skip(start).take(row_len));
        }
        buf
    } else if raw.len() >= row_len * h {
        raw.iter().copied().take(row_len * h).collect()
    } else {
        return None;
    };

    if channels == 4 {
        // CMYK 或 RGBA 转 RGB
        let mut rgb = Vec::with_capacity(w * h * 3);
        for chunk in clean_pixels.as_chunks::<4>().0 {
            rgb.push(chunk[0]);
            rgb.push(chunk[1]);
            rgb.push(chunk[2]);
        }
        return encode_png_buffer(&rgb, width, height, 3);
    }

    encode_png_buffer(&clean_pixels, width, height, channels.min(3))
}

/// 将 1-bit 位图（0=白, 1=黑）指定行区间 `[y_start..y_end)` 降采样（2x2 抗锯齿灰度）并编码为 PNG
fn bitmap_1bit_to_png(
    bitmap: &[u8],
    width: usize,
    height: usize,
    x_start: usize,
    x_end: usize,
    y_start: usize,
    y_end: usize,
) -> Option<Vec<u8>> {
    let x_end = x_end.min(width);
    let y_end = y_end.min(height);
    if y_end <= y_start || x_end <= x_start || width == 0 {
        return None;
    }
    let crop_w = x_end - x_start;
    let crop_h = y_end - y_start;
    let scale = if crop_w >= 1800 { 2 } else { 1 };
    let out_w = crop_w / scale;
    let out_h = crop_h / scale;
    if out_w == 0 || out_h == 0 {
        return None;
    }

    let mut gray = Vec::with_capacity(out_w * out_h);
    for oy in 0..out_h {
        let sy0 = y_start + oy * scale;
        for ox in 0..out_w {
            let sx0 = x_start + ox * scale;
            let mut black_count = 0u32;
            let mut total = 0u32;
            for dy in 0..scale {
                let y = (sy0 + dy).min(height - 1);
                let row = y * width;
                for dx in 0..scale {
                    let x = (sx0 + dx).min(width - 1);
                    if bitmap[row + x] != 0 {
                        black_count += 1;
                    }
                    total += 1;
                }
            }
            let lum = 255u8.saturating_sub(((black_count * 255) / total.max(1)) as u8);
            gray.push(lum);
        }
    }

    encode_png_buffer(&gray, out_w as u32, out_h as u32, 1)
}

/// 纯 Rust 极简标准 PNG 编码器（基于 `flate2::write::ZlibEncoder` + IEEE CRC32）
pub fn encode_png_buffer(pixels: &[u8], width: u32, height: u32, channels: u8) -> Option<Vec<u8>> {
    let w = width as usize;
    let h = height as usize;
    let ch = channels as usize;
    if w == 0 || h == 0 || (ch != 1 && ch != 3) || pixels.len() < w * h * ch {
        return None;
    }

    let color_type: u8 = if ch == 1 { 0 } else { 2 }; // 0 = Grayscale, 2 = Truecolor RGB
    let mut raw_scanlines = Vec::with_capacity((w * ch + 1) * h);
    let row_stride = w * ch;
    for y in 0..h {
        raw_scanlines.push(0u8); // Filter type 0 (None)
        let start = y * row_stride;
        raw_scanlines.extend_from_slice(&pixels[start..start + row_stride]);
    }

    let idat_data = zlib_compress_stored(&raw_scanlines);

    let mut png = Vec::with_capacity(idat_data.len() + 64);
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");

    // IHDR
    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&width.to_be_bytes());
    ihdr[4..8].copy_from_slice(&height.to_be_bytes());
    ihdr[8] = 8; // bit depth = 8
    ihdr[9] = color_type;
    ihdr[10] = 0; // compression
    ihdr[11] = 0; // filter
    ihdr[12] = 0; // interlace
    write_png_chunk(&mut png, b"IHDR", &ihdr);
    write_png_chunk(&mut png, b"IDAT", &idat_data);
    write_png_chunk(&mut png, b"IEND", &[]);

    Some(png)
}

fn write_png_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    let crc = crc32_ieee(chunk_type, data);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32_ieee(chunk_type: &[u8], data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in chunk_type.iter().chain(data.iter()) {
        crc ^= b as u32;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

// ═══════════════════════════════════════════════════════════════════════════
//  纯 Rust ITU-T T.6 (CCITT Group 4, K = -1) 传真图像流解码器
//  用于解码 CNIPA / USPTO / EPO 历史专利 PDF 中的 `/CCITTFaxDecode` 页面
// ═══════════════════════════════════════════════════════════════════════════

struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    fn peek_bits(&self, count: usize) -> Option<u32> {
        if self.bit_pos + count > self.data.len() * 8 {
            return None;
        }
        let mut val = 0u32;
        for i in 0..count {
            let pos = self.bit_pos + i;
            let byte = self.data[pos / 8];
            let bit = (byte >> (7 - (pos % 8))) & 1;
            val = (val << 1) | (bit as u32);
        }
        Some(val)
    }

    fn consume(&mut self, count: usize) {
        self.bit_pos += count;
    }
}

/// 解码 CCITT Group 4 (`K = -1`) 1-bit 图像流，输出 `width * height` 字节（`0` = 白, `1` = 黑）
fn decode_ccitt_group4(data: &[u8], width: usize, height: usize) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || width > 10_000 || height > 15_000 {
        return None;
    }
    let mut reader = BitReader::new(data);
    let mut bitmap = vec![0u8; width * height];
    let mut ref_line = vec![0u8; width];
    let mut cur_line = vec![0u8; width];

    for y in 0..height {
        cur_line.fill(0);
        let mut a0: isize = -1;
        let mut color: u8 = 0; // 0 = white, 1 = black

        while (a0 as usize) < width || a0 < 0 {
            let a0_pos = if a0 < 0 { 0usize } else { a0 as usize };
            if a0_pos >= width {
                break;
            }

            // 解析 2D 模式码字
            let mode = read_2d_mode(&mut reader)?;
            match mode {
                Mode2D::Eofb => {
                    return Some(bitmap);
                }
                Mode2D::Pass => {
                    let b1 = find_b1(&ref_line, width, a0, color);
                    let b2 = find_next_change(&ref_line, width, b1);
                    fill_run(&mut cur_line, a0_pos, b2, color);
                    a0 = b2 as isize;
                }
                Mode2D::Horizontal => {
                    let run1 = read_1d_run(&mut reader, color, width)?;
                    let end1 = (a0_pos + run1).min(width);
                    fill_run(&mut cur_line, a0_pos, end1, color);
                    let run2 = read_1d_run(&mut reader, 1 - color, width)?;
                    let end2 = (end1 + run2).min(width);
                    fill_run(&mut cur_line, end1, end2, 1 - color);
                    a0 = end2 as isize;
                }
                Mode2D::Vertical(offset) => {
                    let b1 = find_b1(&ref_line, width, a0, color) as isize;
                    let a1 = (b1 + offset).clamp(a0_pos as isize, width as isize) as usize;
                    fill_run(&mut cur_line, a0_pos, a1, color);
                    a0 = a1 as isize;
                    color = 1 - color;
                }
            }
            if a0 as usize >= width {
                break;
            }
        }

        let dst_offset = y * width;
        bitmap[dst_offset..dst_offset + width].copy_from_slice(&cur_line);
        ref_line.copy_from_slice(&cur_line);
    }

    Some(bitmap)
}

#[derive(Debug, Clone, Copy)]
enum Mode2D {
    Pass,
    Horizontal,
    Vertical(isize),
    Eofb,
}

fn read_2d_mode(reader: &mut BitReader<'_>) -> Option<Mode2D> {
    if let Some(v) = reader.peek_bits(1) {
        if v == 0b1 {
            reader.consume(1);
            return Some(Mode2D::Vertical(0));
        }
    }
    if let Some(v) = reader.peek_bits(3) {
        if v == 0b001 {
            reader.consume(3);
            return Some(Mode2D::Horizontal);
        }
        if v == 0b011 {
            reader.consume(3);
            return Some(Mode2D::Vertical(1));
        }
        if v == 0b010 {
            reader.consume(3);
            return Some(Mode2D::Vertical(-1));
        }
    }
    if let Some(v) = reader.peek_bits(4) {
        if v == 0b0001 {
            reader.consume(4);
            return Some(Mode2D::Pass);
        }
    }
    if let Some(v) = reader.peek_bits(6) {
        if v == 0b000011 {
            reader.consume(6);
            return Some(Mode2D::Vertical(2));
        }
        if v == 0b000010 {
            reader.consume(6);
            return Some(Mode2D::Vertical(-2));
        }
    }
    if let Some(v) = reader.peek_bits(7) {
        if v == 0b0000011 {
            reader.consume(7);
            return Some(Mode2D::Vertical(3));
        }
        if v == 0b0000010 {
            reader.consume(7);
            return Some(Mode2D::Vertical(-3));
        }
    }
    if let Some(v) = reader.peek_bits(12) {
        if v == 0b0000_0000_0001 {
            reader.consume(12);
            return Some(Mode2D::Eofb);
        }
    }
    None
}

fn fill_run(line: &mut [u8], start: usize, end: usize, color: u8) {
    let s = start.min(line.len());
    let e = end.min(line.len());
    if e > s && color != 0 {
        line[s..e].fill(1);
    }
}

/// 在参考行 `ref_line` 中找 `a0` 右侧第一个与 `color` 相反颜色的跳变点 `b1`
fn find_b1(ref_line: &[u8], width: usize, a0: isize, color: u8) -> usize {
    let target_color = 1 - color;
    let start = if a0 < 0 { 0 } else { (a0 as usize) + 1 };
    let mut i = start;
    // 若起点已在相同颜色块内，先跨过前一段
    if a0 < 0 {
        if !ref_line.is_empty() && ref_line[0] == target_color {
            return 0;
        }
    } else {
        while i < width && ref_line[i] == target_color {
            if ref_line[i - 1] != target_color {
                return i;
            }
            i += 1;
        }
    }
    while i < width {
        let prev = if i == 0 { 0 } else { ref_line[i - 1] };
        if ref_line[i] == target_color && prev != target_color {
            return i;
        }
        i += 1;
    }
    width
}

fn find_next_change(ref_line: &[u8], width: usize, from: usize) -> usize {
    if from >= width {
        return width;
    }
    let c = ref_line[from];
    let mut i = from + 1;
    while i < width {
        if ref_line[i] != c {
            return i;
        }
        i += 1;
    }
    width
}

fn read_1d_run(reader: &mut BitReader<'_>, color: u8, max_width: usize) -> Option<usize> {
    let mut total = 0usize;
    loop {
        let (run, is_terminating) = if color == 0 {
            match_huffman_code(reader, WHITE_CODES)?
        } else {
            match_huffman_code(reader, BLACK_CODES)?
        };
        total += run;
        if is_terminating || total >= max_width {
            return Some(total);
        }
    }
}

fn match_huffman_code(
    reader: &mut BitReader<'_>,
    table: &[(u8, u16, u16, bool)],
) -> Option<(usize, bool)> {
    for &(bit_len, code, run_len, is_term) in table {
        if let Some(v) = reader.peek_bits(bit_len as usize) {
            if v == code as u32 {
                reader.consume(bit_len as usize);
                return Some((run_len as usize, is_term));
            }
        }
    }
    None
}

// (bit_len, code, run_len, is_terminating)
const WHITE_CODES: &[(u8, u16, u16, bool)] = &[
    (4, 0b0111, 2, true),
    (4, 0b1000, 3, true),
    (4, 0b1011, 4, true),
    (4, 0b1100, 5, true),
    (4, 0b1110, 6, true),
    (4, 0b1111, 7, true),
    (5, 0b10011, 8, true),
    (5, 0b10100, 9, true),
    (5, 0b00111, 10, true),
    (5, 0b01000, 11, true),
    (5, 0b11011, 64, false),
    (5, 0b10010, 128, false),
    (6, 0b000111, 1, true),
    (6, 0b001000, 12, true),
    (6, 0b000011, 13, true),
    (6, 0b110100, 14, true),
    (6, 0b110101, 15, true),
    (6, 0b101010, 16, true),
    (6, 0b101011, 17, true),
    (6, 0b010111, 192, false),
    (6, 0b011000, 1664, false),
    (7, 0b0100111, 18, true),
    (7, 0b0001100, 19, true),
    (7, 0b0001000, 20, true),
    (7, 0b0010111, 21, true),
    (7, 0b0000011, 22, true),
    (7, 0b0000100, 23, true),
    (7, 0b0101000, 24, true),
    (7, 0b0101011, 25, true),
    (7, 0b0010011, 26, true),
    (7, 0b0100100, 27, true),
    (7, 0b0011000, 28, true),
    (7, 0b0110111, 256, false),
    (8, 0b00110101, 0, true),
    (8, 0b00000010, 29, true),
    (8, 0b00000011, 30, true),
    (8, 0b00011010, 31, true),
    (8, 0b00011011, 32, true),
    (8, 0b00010010, 33, true),
    (8, 0b00010011, 34, true),
    (8, 0b00010100, 35, true),
    (8, 0b00010101, 36, true),
    (8, 0b00010110, 37, true),
    (8, 0b00010111, 38, true),
    (8, 0b00101000, 39, true),
    (8, 0b00101001, 40, true),
    (8, 0b00101010, 41, true),
    (8, 0b00101011, 42, true),
    (8, 0b00101100, 43, true),
    (8, 0b00101101, 44, true),
    (8, 0b00000100, 45, true),
    (8, 0b00000101, 46, true),
    (8, 0b00001010, 47, true),
    (8, 0b00001011, 48, true),
    (8, 0b01010010, 49, true),
    (8, 0b01010011, 50, true),
    (8, 0b01010100, 51, true),
    (8, 0b01010101, 52, true),
    (8, 0b00100100, 53, true),
    (8, 0b00100101, 54, true),
    (8, 0b01011000, 55, true),
    (8, 0b01011001, 56, true),
    (8, 0b01011010, 57, true),
    (8, 0b01011011, 58, true),
    (8, 0b01001010, 59, true),
    (8, 0b01001011, 60, true),
    (8, 0b00110010, 61, true),
    (8, 0b00110011, 62, true),
    (8, 0b00110100, 63, true),
    (8, 0b00110110, 320, false),
    (8, 0b00110111, 384, false),
    (8, 0b01100100, 448, false),
    (8, 0b01100101, 512, false),
    (8, 0b01101000, 576, false),
    (8, 0b01100111, 640, false),
    (9, 0b011001100, 704, false),
    (9, 0b011001101, 768, false),
    (9, 0b011010010, 832, false),
    (9, 0b011010011, 896, false),
    (9, 0b011010100, 960, false),
    (9, 0b011010101, 1024, false),
    (9, 0b011010110, 1088, false),
    (9, 0b011010111, 1152, false),
    (9, 0b011011000, 1216, false),
    (9, 0b011011001, 1280, false),
    (9, 0b011011010, 1344, false),
    (9, 0b011011011, 1408, false),
    (9, 0b010011000, 1472, false),
    (9, 0b010011001, 1536, false),
    (9, 0b010011010, 1600, false),
    (9, 0b010011011, 1728, false),
    (11, 0b00000001000, 1792, false),
    (11, 0b00000001100, 1856, false),
    (11, 0b00000001101, 1920, false),
    (12, 0b000000010010, 1984, false),
    (12, 0b000000010011, 2048, false),
    (12, 0b000000010100, 2112, false),
    (12, 0b000000010101, 2176, false),
    (12, 0b000000010110, 2240, false),
    (12, 0b000000010111, 2304, false),
    (12, 0b000000011100, 2368, false),
    (12, 0b000000011101, 2432, false),
    (12, 0b000000011110, 2496, false),
    (12, 0b000000011111, 2560, false),
];

const BLACK_CODES: &[(u8, u16, u16, bool)] = &[
    (2, 0b11, 2, true),
    (2, 0b10, 3, true),
    (3, 0b010, 1, true),
    (3, 0b011, 4, true),
    (4, 0b0011, 5, true),
    (4, 0b0010, 6, true),
    (5, 0b00011, 7, true),
    (6, 0b000101, 8, true),
    (6, 0b000100, 9, true),
    (7, 0b0000100, 10, true),
    (7, 0b0000101, 11, true),
    (7, 0b0000111, 12, true),
    (8, 0b00000100, 13, true),
    (8, 0b00000111, 14, true),
    (9, 0b000011000, 15, true),
    (10, 0b0000110111, 0, true),
    (10, 0b0000010111, 16, true),
    (10, 0b0000011000, 17, true),
    (10, 0b0000001000, 18, true),
    (10, 0b0000001111, 64, false),
    (11, 0b00001100111, 19, true),
    (11, 0b00001101000, 20, true),
    (11, 0b00001101100, 21, true),
    (11, 0b00000110111, 22, true),
    (11, 0b00000101000, 23, true),
    (11, 0b00000010111, 24, true),
    (11, 0b00000011000, 25, true),
    (11, 0b00000001000, 1792, false),
    (11, 0b00000001100, 1856, false),
    (11, 0b00000001101, 1920, false),
    (12, 0b000011001010, 26, true),
    (12, 0b000011001011, 27, true),
    (12, 0b000011001100, 28, true),
    (12, 0b000011001101, 29, true),
    (12, 0b000001101000, 30, true),
    (12, 0b000001101001, 31, true),
    (12, 0b000001101010, 32, true),
    (12, 0b000001101011, 33, true),
    (12, 0b000011010010, 34, true),
    (12, 0b000011010011, 35, true),
    (12, 0b000011010100, 36, true),
    (12, 0b000011010101, 37, true),
    (12, 0b000011010110, 38, true),
    (12, 0b000011010111, 39, true),
    (12, 0b000001101100, 40, true),
    (12, 0b000001101101, 41, true),
    (12, 0b000011011010, 42, true),
    (12, 0b000011011011, 43, true),
    (12, 0b000001010100, 44, true),
    (12, 0b000001010101, 45, true),
    (12, 0b000001010110, 46, true),
    (12, 0b000001010111, 47, true),
    (12, 0b000001100100, 48, true),
    (12, 0b000001100101, 49, true),
    (12, 0b000001010010, 50, true),
    (12, 0b000001010011, 51, true),
    (12, 0b000000100100, 52, true),
    (12, 0b000000110111, 53, true),
    (12, 0b000000111000, 54, true),
    (12, 0b000000100111, 55, true),
    (12, 0b000000101000, 56, true),
    (12, 0b000001011000, 57, true),
    (12, 0b000001011001, 58, true),
    (12, 0b000000101011, 59, true),
    (12, 0b000000101100, 60, true),
    (12, 0b000001011010, 61, true),
    (12, 0b000001100110, 62, true),
    (12, 0b000001100111, 63, true),
    (12, 0b000011001000, 128, false),
    (12, 0b000011001001, 192, false),
    (12, 0b000001011011, 256, false),
    (12, 0b000000110011, 320, false),
    (12, 0b000000110100, 384, false),
    (12, 0b000000110101, 448, false),
    (12, 0b000000010010, 1984, false),
    (12, 0b000000010011, 2048, false),
    (12, 0b000000010100, 2112, false),
    (12, 0b000000010101, 2176, false),
    (12, 0b000000010110, 2240, false),
    (12, 0b000000010111, 2304, false),
    (12, 0b000000011100, 2368, false),
    (12, 0b000000011101, 2432, false),
    (12, 0b000000011110, 2496, false),
    (12, 0b000000011111, 2560, false),
    (13, 0b0000001101100, 512, false),
    (13, 0b0000001101101, 576, false),
    (13, 0b0000001001010, 640, false),
    (13, 0b0000001001011, 704, false),
    (13, 0b0000001001100, 768, false),
    (13, 0b0000001001101, 832, false),
    (13, 0b0000001110010, 896, false),
    (13, 0b0000001110011, 960, false),
    (13, 0b0000001110100, 1024, false),
    (13, 0b0000001110101, 1088, false),
    (13, 0b0000001110110, 1152, false),
    (13, 0b0000001110111, 1216, false),
    (13, 0b0000001010010, 1280, false),
    (13, 0b0000001010011, 1344, false),
    (13, 0b0000001010100, 1408, false),
    (13, 0b0000001010101, 1472, false),
    (13, 0b0000001011010, 1536, false),
    (13, 0b0000001011011, 1600, false),
    (13, 0b0000001100100, 1664, false),
    (13, 0b0000001100101, 1728, false),
];

/// 规范化 Google Patents 的 `pdf_url`（修复 SerpAPI 返回的相对路径 `xx/yy/zz/CNxxx.pdf`）
pub fn normalize_patent_pdf_url(raw_pdf_url: &str) -> Option<String> {
    let trimmed = raw_pdf_url.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        return Some(trimmed.to_string());
    }
    if trimmed.ends_with(".pdf") || trimmed.ends_with(".PDF") {
        let clean = trimmed.trim_start_matches('/');
        return Some(format!(
            "https://patentimages.storage.googleapis.com/{clean}"
        ));
    }
    None
}

/// 从远程 `pdf_url` 下载专利 PDF 并提取真实单张附图（返回 `data:image/...;base64,...` 列表）
pub async fn fetch_and_extract_drawings_from_pdf_url(
    pdf_url: &str,
) -> Result<Vec<ExtractedPdfDrawing>, String> {
    let Some(full_url) = normalize_patent_pdf_url(pdf_url) else {
        return Err("无效的专利 PDF 链接".to_string());
    };
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(25))
        .build()
        .map_err(|e| format!("HTTP 客户端初始化失败: {e}"))?;

    let resp = client
        .get(&full_url)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await
        .map_err(|e| format!("下载专利 PDF 失败: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("专利 PDF 上游返回 HTTP {}", resp.status()));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取专利 PDF 响应失败: {e}"))?;

    if bytes.len() > 20 * 1024 * 1024 {
        return Err("专利 PDF 超过 20MB 体积上限".to_string());
    }

    extract_drawings_from_pdf_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_extract::{dictionary, Document, Object, Stream};

    /// 构造一个包含「第1页：权利要求书 + 小尺寸印章」与「第2页：纯文字说明书」与
    /// 「第3页：说明书附图 + 真实大图 XObject」的合成多页 PDF，
    /// 验证反作弊能力：纯文字页（权利要求书/说明书）与小尺寸印章绝不被当作附图输出！
    fn build_synthetic_patent_pdf() -> Vec<u8> {
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });

        // 1. 小尺寸印章/条码图像 (60x40, 应当被尺寸门禁和文字页门禁双重过滤)
        let small_stamp_pixels = vec![128u8; 60 * 40 * 3];
        let stamp_stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 60,
                "Height" => 40,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            small_stamp_pixels,
        );
        let stamp_id = doc.add_object(stamp_stream);

        // 2. 放置在权利要求书页上的伪大图 (300x200, 应当因为所在页是 CLAIMS 权利要求书页而被拦截)
        let text_page_img_pixels = vec![200u8; 300 * 200 * 3];
        let text_page_img_stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 300,
                "Height" => 200,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            text_page_img_pixels,
        );
        let text_page_img_id = doc.add_object(text_page_img_stream);

        // 3. 放置在「DRAWINGS / 说明书附图」专属页上的真实专利附图 (320x240 RGB)
        let mut real_fig_pixels = Vec::with_capacity(320 * 240 * 3);
        for y in 0..240u32 {
            for x in 0..320u32 {
                let is_line = (x == 40 || x == 280 || y == 40 || y == 200)
                    && (40..=280).contains(&x)
                    && (40..=200).contains(&y);
                let v = if is_line { 20u8 } else { 250u8 };
                real_fig_pixels.push(v);
                real_fig_pixels.push(v);
                real_fig_pixels.push(v);
            }
        }
        let real_fig_stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 320,
                "Height" => 240,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            real_fig_pixels,
        );
        let real_fig_id = doc.add_object(real_fig_stream);

        // Page 1: CLAIMS (权利要求书纯文字页 + 小印章 + 伪图)
        let p1_content = b"BT /F1 12 Tf 50 750 Td (CLAIMS 1. A solid-state battery thermal runaway barrier comprising aerogel layers.) Tj ET";
        let p1_content_id = doc.add_object(Stream::new(dictionary! {}, p1_content.to_vec()));
        let p1_res = dictionary! {
            "Font" => dictionary! { "F1" => font_id },
            "XObject" => dictionary! {
                "ImStamp" => stamp_id,
                "ImFakeOnClaims" => text_page_img_id,
            },
        };
        let p1_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => p1_content_id,
            "Resources" => p1_res,
        });

        // Page 2: Pure Specification Text Page (> 260 non-whitespace chars)
        let long_spec = format!(
            "BT /F1 10 Tf 50 750 Td (SPECIFICATION BACKGROUND OF THE INVENTION {}) Tj ET",
            "DetailedProcessStepAndEmbodimentDescription".repeat(8)
        );
        let p2_content_id = doc.add_object(Stream::new(dictionary! {}, long_spec.into_bytes()));
        let p2_res = dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        };
        let p2_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => p2_content_id,
            "Resources" => p2_res,
        });

        // Page 3: DRAWINGS Page (含真实附图 real_fig_id + 小印章 stamp_id)
        let p3_content = b"BT /F1 12 Tf 250 800 Td (DRAWINGS FIG.1) Tj ET";
        let p3_content_id = doc.add_object(Stream::new(dictionary! {}, p3_content.to_vec()));
        let p3_res = dictionary! {
            "Font" => dictionary! { "F1" => font_id },
            "XObject" => dictionary! {
                "ImStamp" => stamp_id,
                "ImRealFig1" => real_fig_id,
            },
        };
        let p3_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => p3_content_id,
            "Resources" => p3_res,
        });

        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(p1_id), Object::Reference(p2_id), Object::Reference(p3_id)],
                "Count" => 3,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        let mut buf = Vec::new();
        doc.save_to(&mut buf).expect("save synthetic pdf");
        buf
    }

    #[test]
    fn anti_cheat_pdf_drawing_extractor_excludes_text_pages_and_stamps() {
        let pdf_bytes = build_synthetic_patent_pdf();
        let drawings = extract_drawings_from_pdf_bytes(&pdf_bytes)
            .expect("extract drawings from synthetic pdf");

        // 反作弊核心断言：
        // 1. 共 3 页 PDF（第 1 页权利要求书、第 2 页纯文字说明书、第 3 页说明书附图），
        //    只允许提取出第 3 页上的 1 张真实附图！
        assert_eq!(
            drawings.len(),
            1,
            "Must extract ONLY the 1 real drawing from page 3, excluding claims page, text page, and small stamps"
        );
        let fig = &drawings[0];
        assert_eq!(
            fig.page_number, 3,
            "Drawing must come from page 3 (DRAWINGS sheet)"
        );
        assert_eq!(fig.width, 320);
        assert_eq!(fig.height, 240);
        assert_eq!(fig.mime_type, "image/png");
        assert!(fig.data_url.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn dimension_filter_rejects_stamps_barcodes_and_thin_lines() {
        // 小印章/二维码
        assert!(!is_valid_drawing_dimensions(74, 74));
        // 顶部一维条形码 (宽 160, 高 18)
        assert!(!is_valid_drawing_dimensions(160, 18));
        // 极端细长分隔线 (宽 1200, 高 150 -> 长宽比 8 > 5)
        assert!(!is_valid_drawing_dimensions(1200, 150));
        // 真实专利附图 (宽 430, 高 463)
        assert!(is_valid_drawing_dimensions(430, 463));
    }

    #[test]
    fn normalize_patent_pdf_url_handles_relative_serpapi_paths() {
        assert_eq!(
            normalize_patent_pdf_url("d1/3d/69/0a8a51e99c3cb2/CN108626463B.pdf").as_deref(),
            Some("https://patentimages.storage.googleapis.com/d1/3d/69/0a8a51e99c3cb2/CN108626463B.pdf")
        );
        assert_eq!(
            normalize_patent_pdf_url("https://patentimages.storage.googleapis.com/aa/bb/CN1.pdf")
                .as_deref(),
            Some("https://patentimages.storage.googleapis.com/aa/bb/CN1.pdf")
        );
        assert!(normalize_patent_pdf_url("   ").is_none());
    }
}
