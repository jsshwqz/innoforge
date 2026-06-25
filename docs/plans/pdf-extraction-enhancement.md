# PDF 专利文本提取增强计划

> 在零新依赖的前提下，增强研创台的 PDF 文本提取能力，解决中文多栏专利排版乱序问题。

## 修改清单

| 文件 | 改动 |
|------|------|
| `src/routes/upload.rs` | `extract_pdf_text()` 新增逐页提取 Step 2；新增 `extract_pdf_text_by_pages()` 函数；新增 `api_patent_pdf_extract_text()` 端点处理函数；新增 `download_pdf()` 辅助函数 |
| `src/main.rs` | 注册 `/api/patent/pdf/extract-text` 路由 |
| `src/lib.rs` | 同步注册 `/api/patent/pdf/extract-text` 路由 |
| `CHANGELOG.md` | 更新 v0.7.0 条目，新增 PDF 提取增强说明 |

## 架构

### 提取降级链（增强后）

```
extract_pdf_text(data)
  ├── Step 1: pdf_extract::extract_text_from_mem()          [Rust, 标准模式]
  ├── Step 2: pdf_extract::extract_text_from_mem_by_pages()  [Rust, 逐页/多栏更好] ✅ 新增
  ├── Step 3: pdftotext (poppler)                            [外部工具]
  ├── Step 4: PyMuPDF (Python fitz)                          [Python]
  └── Step 5: Tesseract OCR                                  [Python, 扫描件]
```

### 新端点

```
POST /api/patent/pdf/extract-text
  参数: patent_id (可选，从 DB 获取 PDF URL) 或 file (直接上传 PDF)
  返回: {
    "pages": [
      {"page": 1, "text": "...", "char_count": 1234},
      {"page": 2, "text": "...", "char_count": 5678}
    ],
    "page_count": 12,
    "total_chars": 60000,
    "full_text": "【第 1 页】\n...\n【第 2 页】\n...",
    "method": "by_pages"
  }
```

## 完成状态

| 任务 | 状态 |
|------|------|
| `extract_pdf_text()` 新增逐页提取 Step 2 | ✅ 已完成 |
| `extract_pdf_text_by_pages()` 函数 | ✅ 已完成 |
| `api_patent_pdf_extract_text()` 端点 | ✅ 已完成 |
| `download_pdf()` 辅助函数 | ✅ 已完成 |
| `src/main.rs` 注册路由 | ✅ 已完成 |
| `src/lib.rs` 同步注册路由 | ✅ 已完成 |
| CHANGELOG.md 更新 | ✅ 已完成 |
| `cargo fmt` | ⚠️ 需用户本地执行（沙箱不可用） |
| `cargo clippy` | ✅ 零警告 |
| `cargo test` | ⚠️ 需用户本地执行（沙箱不可用） |

## 能力对比

| 能力 | 改造前 | 改造后 |
|------|--------|--------|
| Rust 原生提取 | 1 种模式 | 2 种模式（标准 + 逐页） |
| 中文多栏排版 | 易乱序 | 逐页提取，多栏效果更好 |
| 专利 PDF 专用端点 | 无 | 有，返回按页 JSON |
| AI 逐页分析 | 需要手动分段 | 直接传入 pages 数组 |
| 新依赖 | — | 零（全用已引入的 crate） |
