# 2026-07-13 D 盘 PDF 临时文件修复计划

## 背景

当前 Windows 环境的 `TEMP`/`TMP` 位于 C 盘。`src/routes/upload.rs` 的 PDF
提取、视觉回退、OCR 与 MinerU 路径会调用 `std::env::temp_dir()`，其中多个文件
仅以进程 PID 命名；并发请求可能覆盖彼此的材料，且 Umi-OCR 的失败分支会遗留
文件。这既占用 C 盘，也可能造成跨请求内容串扰。

## 范围

仅修改 `src/routes/upload.rs`（包括该文件内测试）。不新增 crate、路由、公开 API、
数据库 schema 或前端改动。

## 实施步骤

1. 在模块内实现受控临时文件工具：运行时目录固定为项目工作目录下的
   `data/runtime-temp`。现有 `start.bat` 先切换到项目根目录，因此该目录位于项目所在
   D 盘；路径由服务端生成，绝不采纳文件名或请求输入。
2. 临时 PDF 使用 UUID 命名和 `OpenOptions::create_new(true)` 创建，并由 `Drop`
   守卫清理，以覆盖所有 `?`、早退和外部命令失败分支。视觉回退生成的至多十张
   PNG 使用同一随机会话前缀并在守卫中清理。
3. 将 pdftotext 输出改为捕获标准输出，消除额外 `.txt` 临时文件；删除 Umi-OCR
   当前未被上传流程使用的临时 PDF 写入及其分散清理代码。PyMuPDF、Tesseract、
   MinerU 和视觉回退统一使用受控工具。
4. 添加同文件回归，验证运行时路径在项目 `data/runtime-temp` 下、UUID 临时文件在
   drop 后被删除、路径不落入系统临时目录；保留成功与失败的解析语义。

## 验收与归档

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`

完成编译后，以运行中的新版服务验证 `/` 与 `/oa-response` 健康响应；三项门禁通过
后更新 CHANGELOG、STATUS、错误复盘和本计划，并按规范提交。

## 完成记录 / Completion

- **状态 / Status**: ✅ 已完成 / Completed
- **代码提交 / Code commit**: `dcb446d` (`fix: 临时 PDF 文件改存项目数据目录`)
- **结果 / Result**: 临时文件统一落在项目 `data/runtime-temp`；UUID 独占创建与 RAII 清理覆盖所有已识别的外部 PDF 工具路径，且 Umi-OCR 不再写入无用副本。
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（275 passed, 1 ignored）均通过；`data/runtime-temp` 无测试残留，重新构建后的 `/` 与 `/oa-response` 均为 HTTP 200。
