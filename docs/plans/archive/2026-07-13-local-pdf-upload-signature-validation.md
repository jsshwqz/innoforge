# 2026-07-13 本地上传 PDF 文件签名校验计划

## 背景

远程专利 PDF 下载已在读取响应后验证 `%PDF-` 文件签名，但本地上传入口仍只依据客户端提供的 `.pdf` 扩展名选择处理分支。伪装文本或 HTML 因而可能被写入上传目录，或被交给 PDF 解析、OCR 和 AI 视觉回退，浪费资源并返回不准确的错误。

## 范围

仅修改 `src/routes/upload.rs` 及其同文件单元测试：

- `api_upload_pdf_store`
- `api_upload_compare`
- `api_upload_extract`
- `api_patent_pdf_extract_text`

复用既有 `has_pdf_header`，不新增 crate、路由、数据库迁移或前端改动。

## 实施步骤

1. 先扩展现有 PDF 签名单元回归，覆盖普通文本伪装为 PDF 的拒绝情形，同时保留有效签名、前导空白、HTML 和 1024 字节边界覆盖。
2. 在 PDF 存储入口写入 `data/uploads` 前验证签名。
3. 在对比上传和通用提取入口调用 PDF 解析、OCR 或视觉回退前验证签名。
4. 在专利专用提取入口的直传/远程下载汇合点统一验证签名。
5. 对未通过校验的请求返回明确、用户可理解的 JSON 错误，不让无效内容进入后续处理。

## 验收与归档

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- 运行正式二进制后，以真实 multipart HTTP 请求确认伪装 PDF 被拒绝，且首页和 OA 页面可正常访问。

## 完成记录 / Completion

- **状态 / Status**: ✅ 已完成 / Completed
- **代码提交 / Code commit**: `8170576` (`fix: 校验本地上传 PDF 文件签名`)
- **结果 / Result**: 四条本地 PDF 入口在落盘、解析、OCR 或视觉处理之前复用 `%PDF-` 签名校验；专利专用入口统一校验直传与远程下载的汇合数据。
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（285 passed, 1 ignored）通过。运行正式二进制后，`/api/upload/pdf-store` 明确拒绝伪装 PDF，`/` 与 `/oa-response` 均返回 HTTP 200。
