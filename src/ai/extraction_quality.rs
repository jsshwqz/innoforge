//! C0 文本提取质量门 / Text Extraction Quality Gate
//!
//! 任何 PDF 进来，先试文本层提取（poppler/pdftotext），检测到文本层存在就用文本层；
//! 只有检测到无文本层（扫描件/图片件）才回退 OCR。
//!
//! 输出必须带提取方式标记（TEXT_LAYER / OCR_FALLBACK）。
//! OCR_FALLBACK 的文本在 C1/C2/C11 这类计数断言前必须标红提示风险。
//!
//! C0 是 C1-C13 的前置依赖：输入脏了下游计数全错。

use std::path::Path;

/// 提取方式标记
#[derive(Debug, Clone, PartialEq)]
pub enum ExtractionMethod {
    /// pdftotext 文本层提取（干净，可信）
    TextLayer,
    /// OCR 回退提取（可能有字符替换错误，计数断言需标红）
    OcrFallback,
}

impl ExtractionMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExtractionMethod::TextLayer => "TEXT_LAYER",
            ExtractionMethod::OcrFallback => "OCR_FALLBACK",
        }
    }

    /// 是否可信（TEXT_LAYER 可信，OCR_FALLBACK 不可信）
    pub fn is_reliable(&self) -> bool {
        matches!(self, ExtractionMethod::TextLayer)
    }
}

/// 提取结果
#[derive(Debug, Clone)]
pub struct ExtractionResult {
    /// 提取的文本
    pub text: String,
    /// 提取方式
    pub method: ExtractionMethod,
    /// 文本字符数（去空白后）
    pub char_count: usize,
    /// PDF 页数（如果能获取）
    pub page_count: Option<usize>,
    /// 警告信息
    pub warnings: Vec<String>,
}

impl ExtractionResult {
    /// 生成风险提示
    pub fn risk_warning(&self) -> Option<String> {
        if !self.method.is_reliable() {
            Some(format!(
                "⚠️ OCR_FALLBACK：文本通过 OCR 提取，可能存在字符替换错误。\
                 C1/C2/C11 等计数断言不可信，需人工核对原文。"
            ))
        } else {
            None
        }
    }
}

/// 检测 PDF 是否有文本层
///
/// 通过运行 `pdftotext -enc UTF-8 -raw` 并检查输出是否有足够的中文内容来判断。
/// 规则：提取的文本字符数 > 页数 × 50（每页至少 50 个字符）则认为有文本层。
pub fn detect_text_layer(pdf_path: &Path) -> Option<(String, usize)> {
    let path_str = pdf_path.to_str()?;

    // 运行 pdftotext
    let output = std::process::Command::new("pdftotext")
        .args(["-enc", "UTF-8", "-raw", path_str, "-"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout).to_string();

    // 获取页数
    let page_count = get_pdf_page_count(pdf_path).unwrap_or(1);

    // 去空白后字符数
    let char_count: usize = text.chars().filter(|c| !c.is_whitespace()).count();

    // 判断标准：每页至少 50 个非空白字符
    if char_count > page_count * 50 {
        Some((text, char_count))
    } else {
        None
    }
}

/// 获取 PDF 页数
fn get_pdf_page_count(pdf_path: &Path) -> Option<usize> {
    let path_str = pdf_path.to_str()?;

    let output = std::process::Command::new("pdfinfo")
        .arg(path_str)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let info = String::from_utf8_lossy(&output.stdout);
    for line in info.lines() {
        if let Some(rest) = line.strip_prefix("Pages:") {
            return rest.trim().parse().ok();
        }
    }

    None
}

/// C0 质量门：从 PDF 提取文本，自动选择最佳提取方式
///
/// 1. 先试 pdftotext 文本层提取
/// 2. 如果有足够文本 → 返回 TEXT_LAYER
/// 3. 如果无文本层 → 返回 OCR_FALLBACK（调用方需自行调用 OCR）
///
/// 返回 ExtractionResult，带提取方式标记和风险警告。
pub fn extract_pdf_text(pdf_path: &Path) -> ExtractionResult {
    let page_count = get_pdf_page_count(pdf_path);
    let mut warnings = Vec::new();

    // Step 1: 尝试文本层提取
    if let Some((text, char_count)) = detect_text_layer(pdf_path) {
        return ExtractionResult {
            text,
            method: ExtractionMethod::TextLayer,
            char_count,
            page_count,
            warnings,
        };
    }

    // Step 2: 无文本层，需要 OCR 回退
    warnings.push(
        "PDF 无文本层（扫描件/图片件），需 OCR 回退提取。\
         OCR 文本可能存在字符替换错误，计数断言不可信。"
            .to_string(),
    );

    ExtractionResult {
        text: String::new(), // 调用方需自行调用 OCR 填充
        method: ExtractionMethod::OcrFallback,
        char_count: 0,
        page_count,
        warnings,
    }
}

/// 对已提取的文本标记提取方式
///
/// 用于从外部 OCR 管道接收文本时标记为 OCR_FALLBACK。
pub fn mark_ocr_text(text: String, page_count: Option<usize>) -> ExtractionResult {
    let char_count = text.chars().filter(|c| !c.is_whitespace()).count();
    let mut warnings = Vec::new();

    if char_count < 100 {
        warnings.push(format!("提取文本极短（{}字符），可能提取失败", char_count));
    }

    ExtractionResult {
        text,
        method: ExtractionMethod::OcrFallback,
        char_count,
        page_count,
        warnings,
    }
}

/// 对已提取的文本标记为文本层提取
pub fn mark_text_layer_text(text: String, page_count: Option<usize>) -> ExtractionResult {
    let char_count = text.chars().filter(|c| !c.is_whitespace()).count();

    ExtractionResult {
        text,
        method: ExtractionMethod::TextLayer,
        char_count,
        page_count,
        warnings: Vec::new(),
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  测试
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_method_str() {
        assert_eq!(ExtractionMethod::TextLayer.as_str(), "TEXT_LAYER");
        assert_eq!(ExtractionMethod::OcrFallback.as_str(), "OCR_FALLBACK");
    }

    #[test]
    fn extraction_method_reliability() {
        assert!(ExtractionMethod::TextLayer.is_reliable());
        assert!(!ExtractionMethod::OcrFallback.is_reliable());
    }

    #[test]
    fn text_layer_result_no_risk_warning() {
        let result = mark_text_layer_text("测试文本".to_string(), Some(1));
        assert!(result.risk_warning().is_none());
    }

    #[test]
    fn ocr_fallback_result_has_risk_warning() {
        let result = mark_ocr_text("测试文本".to_string(), Some(1));
        assert!(result.risk_warning().is_some());
        let warning = result.risk_warning().unwrap();
        assert!(warning.contains("OCR_FALLBACK"));
        assert!(warning.contains("计数断言不可信"));
    }

    #[test]
    fn mark_text_layer_counts_chars() {
        let result = mark_text_layer_text("  你 好 世 界  ".to_string(), Some(1));
        assert_eq!(result.char_count, 4); // 4 个非空白字符
        assert_eq!(result.method, ExtractionMethod::TextLayer);
    }

    #[test]
    fn mark_ocr_counts_chars() {
        let result = mark_ocr_text("测试 OCR".to_string(), Some(2));
        assert_eq!(result.char_count, 5); // "测试OCR" = 5 chars
        assert_eq!(result.method, ExtractionMethod::OcrFallback);
    }

    #[test]
    fn ocr_with_short_text_warns() {
        let result = mark_ocr_text("短".to_string(), Some(1));
        assert!(!result.warnings.is_empty());
        assert!(result.warnings[0].contains("极短"));
    }

    #[test]
    fn ocr_with_sufficient_text_no_length_warning() {
        let text = "这是一段足够长的OCR提取文本用于测试质量门功能是否正常工作它需要超过一百个字符才能不触发长度警告所以这里要写很多字这是一段足够长的OCR提取文本用于测试质量门功能是否正常工作它需要超过一百个字符才能不触发长度警告所以这里要写很多字".to_string();
        let result = mark_ocr_text(text, Some(1));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn risk_warning_format() {
        let result = mark_ocr_text("测试".to_string(), Some(1));
        let warning = result.risk_warning().unwrap();
        assert!(warning.starts_with("⚠️"));
        assert!(warning.contains("C1/C2/C11"));
    }
}
