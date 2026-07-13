# 2026-07-13 DOCX 导出安全加固计划

## 背景

OA 答复书导出会将用户与外部材料字段写入 DOCX 的 ZIP/XML 容器。原实现使用生产路径 `unwrap()` 写入 ZIP；任何 I/O 异常都可能使请求处理 panic。答复正文已有 XML 转义，但专利号、申请人与审查意见类型仍直接插入 XML，特殊字符会损坏 Word 文档结构。

## 范围

仅修改 `src/docx_export/export.rs` 与 `src/routes/ai.rs`。

- 不新增 crate、API 路由、数据库迁移或前端改动。
- 保持 `/api/oa/export-docx` 成功响应的 JSON 结构不变。
- 导出失败只返回用户可理解的信息，详细错误仅写入服务端日志。

## 实施步骤

1. 让 `generate_docx` 返回 `Result<Vec<u8>, String>`，将所有 ZIP 创建、写入和收尾错误显式传播。
2. 对全部四个外部文本字段（答复正文、专利号、申请人、审查意见类型）使用 XML 文本转义。
3. 在 API 层记录内部导出错误，并向客户端返回稳定的友好错误 JSON。
4. 添加内存 DOCX 解包测试，确认所有字段在 `word/document.xml` 中已转义且导出仍为有效 ZIP。

## 验收与归档

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- 构建正式二进制，并验证 `/`、`/oa-response` 与携带特殊字符的 `POST /api/oa/export-docx`。

## 完成记录 / Completion

- **状态 / Status**: ✅ 已完成 / Completed
- **代码提交 / Code commit**: `b8b6e89` (`fix: 加固 DOCX 导出错误处理`)
- **结果 / Result**: ZIP 写入失败改为受控错误；四个外部 XML 文本字段均经转义，接口失败时不暴露底层 panic 或 I/O 细节。
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（295 passed, 1 ignored）、正式二进制构建和本地 HTTP 导出验证均通过。
