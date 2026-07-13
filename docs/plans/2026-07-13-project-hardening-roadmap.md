# 2026-07-13 InnoForge 项目加固路线图
# 2026-07-13 InnoForge Project Hardening Roadmap

## 目标 / Goal

在不重写 Axum + 纯 HTML 架构、不引入新 crate、不立即修改数据库 schema 的前提下，先提升研发用户最关心的可靠性：数据不静默丢失、回归可重复、安全边界可验证。

Improve reliability for research users without rewriting the Axum + static HTML architecture, adding crates, or immediately changing the database schema: no silent data loss, reproducible regression, and verifiable security boundaries.

## 当前基线 / Baseline

- Rust 单元与集成测试：245 项通过。
  Rust unit and integration tests: 245 passing.
- OA 前端 5 个逻辑问题组的浏览器截断已修复；`start.bat` 的 CMD 解析错误已修复。
  Five logical frontend OA truncation groups and the `start.bat` CMD parse error are fixed.
- `src/routes/ai.rs` 的 OA 讨论链路仍存在 60k/40k/15k 字符静默截断。
  The OA discussion path in `src/routes/ai.rs` still silently truncates at 60k/40k/15k characters.
- 仓库缺少规约引用的 `e2e_test.mjs`；模板有与 HEAD 相同的 ESLint 基线错误。
  The referenced `e2e_test.mjs` is missing, and the template has baseline ESLint errors identical to HEAD.
- 版本、STATUS、CHANGELOG 和架构文档存在漂移，作为发布治理项处理。
  Version, STATUS, CHANGELOG, and architecture documents have drift and need release governance.

## 执行原则 / Execution principles

1. 每个阶段先写任务单，再由 CODEX 实现；主代理负责审查和门禁。
   Write a task sheet before each phase; CODEX implements, and the primary agent reviews and gates.
2. 数据用途的文本禁止静默截断；容量限制必须显式报错或采用可追踪分段策略。
   Data-bearing text must never be silently truncated; capacity limits must be visible errors or traceable chunking.
3. 本路线图第一阶段不新增 crate、npm 包、数据库表或公共 API。
   Phase 1 adds no crate, npm package, database table, or public API.
4. 每阶段必须运行 Rust 门禁；改模板或 JS 时必须运行静态检查和可重复 E2E。
   Each phase runs Rust gates; template/JS changes require static checks and reproducible E2E.

## 阶段规划 / Phases

### P0-A：OA 后端数据完整性 / OA backend data integrity

**范围 / Scope**

- 盘点 `src/routes/ai.rs`、`src/ai/patent.rs`、`src/ai/client.rs` 中 OA 相关所有截断、请求体限制和错误响应。
- Inventory all OA truncation, payload limits, and error responses in the three AI modules.
- 优先移除静默截断；对超限输入返回用户可理解的结构化错误，只有在能保留证据边界时才采用分段分析。
- Remove silent truncation first; return understandable structured errors for oversized input, using chunking only when evidence boundaries remain traceable.
- 增加路由级测试：短文本成功、超限可见失败、Unicode 字符计数、SSE 错误事件。
- Add route-level tests for success, visible overflow failure, Unicode character counting, and SSE error events.

**完成标准 / Exit criteria**

- OA 原文、分析结果和讨论历史不再被静默截断。
- Office action text, analysis, and discussion history are never silently truncated.
- 超限响应不泄露 panic/内部堆栈，并给出下一步操作建议。
- Overflow responses expose no panic/internal stack and provide a next action.

### P0-B：可重复前端回归 / Reproducible frontend regression

- 恢复仓库内官方 `e2e_test.mjs` 或等价的 `tests/e2e/` 入口，不依赖会话临时脚本。
- Restore a repository-owned `e2e_test.mjs` or equivalent `tests/e2e/` entry point instead of session-only scripts.
- 覆盖首页、OA 页面加载、长文本请求体尾部保留、讨论请求、启动/停止健康检查。
- Cover home/OA loading, long-text payload tails, discussion requests, and startup/health checks.
- 将 E2E 纳入 CI，失败时输出页面错误、请求 URL 和响应状态。
- Run E2E in CI with page errors, request URLs, and response statuses on failure.

### P0-C：安全基线 / Security baseline

- 将 CORS 从全开放改为配置化 allowlist，同时保留本地桌面和移动端所需来源。
- Replace open CORS with a configurable allowlist while preserving desktop/mobile origins.
- 审计上传大小、扩展名/MIME、临时文件清理、路径穿越和外部 URL 抓取的 SSRF 风险。
- Audit upload size/type, temp-file cleanup, path traversal, and SSRF in external URL fetching.
- 统一 Prompt 用户输入边界，补充恶意输入回归样例；保持 DOMPurify 全局保护。
- Standardize prompt user-input boundaries, add malicious-input regression cases, and preserve the global DOMPurify guard.

### P1：AI 稳定性与可观测性 / AI resilience and observability

- 每次 AI 调用独立超时、重试退避、Provider 熔断和 trace ID。
- Per-call timeout, retry backoff, provider circuit breaking, and trace IDs.
- 记录耗时、输入/输出字符数、Provider、失败分类和成本估算，不记录密钥和原始敏感材料。
- Record latency, character counts, provider, failure class, and cost estimates without secrets or raw sensitive material.
- 统一结构化输出校验和降级格式。
- Standardize structured-output validation and fallback formats.

### P1：OA 版本与审计 / OA versioning and audit

在单独的数据库迁移计划中设计原始 OA、分析版本、讨论记录、用户修改和最终答复的不可变链路；此阶段需要单独确认 schema 设计后再实施。

Design an immutable chain for original OA, analysis versions, discussion, user edits, and final responses in a separate migration plan; schema work requires separate approval.

### P1：前端维护性 / Frontend maintainability

- 将超长 OA 页面拆为无构建工具也能加载的 ES modules。
- Split the large OA page into ES modules that still work without a bundler.
- 清理 16 个历史 `no-redeclare` 错误，统一 fetch、错误屏障和 DOM 渲染边界。
- Remove the 16 baseline `no-redeclare` errors and unify fetch, error-barrier, and DOM-rendering boundaries.

### P2：发布与质量治理 / Release and quality governance

- 统一 Cargo、CHANGELOG、STATUS、Schema 的版本来源。
- Make Cargo, CHANGELOG, STATUS, and schema versions derive from one source.
- 为 `start.bat`、Docker、移动 FFI 增加健康检查和兼容矩阵。
- Add health checks and a compatibility matrix for `start.bat`, Docker, and mobile FFI.
- 建立专利搜索人工标注集，持续评估召回率、相关性和去重质量。
- Establish a labeled patent-search set for recall, relevance, and deduplication evaluation.

## 当前执行任务 / Current execution task

**P0-A：OA 后端数据完整性。** 先只改 OA 相关后端和测试；不扩展到 schema、Provider 重构或前端模块化。

**P0-A: OA backend data integrity.** Modify only OA backend paths and tests first; do not expand into schema, provider rewrites, or frontend modularization.

状态：✅ 已完成 / Completed

- **代码提交 / Code commit**: `ce303d2` (`fix: OA 后端超限改为显式错误`)
- **结果 / Result**: OA 讨论和答复书路径保留全文；超出容量时返回带字段、实际 Unicode 字符数和上限的可见错误。新增短文本、Unicode 边界和流式错误回归测试。
  OA discussion and response-letter paths retain full input; oversized fields return visible errors with field name, actual Unicode character count, and limit. Added short-input, Unicode-boundary, and streaming-error regressions.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test` 全部通过。
  `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` all passed.

## 下一执行任务 / Next execution task

**P0-B：可重复前端回归 / Reproducible frontend regression。** 恢复仓库内可执行的 `e2e_test.mjs`，不新增 npm 依赖，覆盖服务健康检查、OA 页面加载和关键请求连通性。

### P0-B 实施计划 / Implementation plan

1. 新建仓库根目录 `e2e_test.mjs`（项目规约指定的入口），仅使用现有 `puppeteer`。
   Create the required repository-root `e2e_test.mjs` using only the existing `puppeteer` dependency.
2. 测试脚本只连接已运行的本地服务，不负责停止用户服务；默认地址为 `http://127.0.0.1:3000`，可用环境变量覆盖。
   The script connects to a running local server and never stops a user-owned service; default to `http://127.0.0.1:3000` with an environment override.
3. 覆盖首页与 OA 页 HTTP/页面加载、前端控制台异常、`/api/ai/check-amendments` 的参数校验连通性，以及 OA 长文本提交体尾标记不被前端截断。
   Cover home/OA HTTP and page loading, frontend console exceptions, amendment-check connectivity, and preservation of a long-text end marker in OA request payload construction.
4. 失败时输出请求 URL、HTTP 状态、页面错误；成功时给出稳定的通过计数。不得调用真实 AI Provider 或写入数据库。
   On failure report request URL, HTTP status, and page errors; on success report stable pass counts. Do not call a real AI provider or write to the database.
5. CODEX 实现后由主代理运行脚本、ESLint（新增 JS）和全部 Rust 门禁；验证完成才提交。
   After CODEX implements it, the primary agent runs the script, ESLint for the new JS, and all Rust gates before committing.

状态：✅ 已完成 / Completed

- **代码提交 / Code commit**: `d03c975` (`test: 补齐 OA 端到端回归`)
- **结果 / Result**: 新增根目录 `e2e_test.mjs`，默认连接本地服务并可用环境变量覆盖；6 项回归覆盖页面、接口和长请求体尾部完整性，真实 AI 请求被浏览器拦截。
  Added root `e2e_test.mjs` with an overridable local-server URL; six regressions cover page, endpoint, and long-payload-tail integrity while real AI requests are intercepted in the browser.
- **验证 / Verification**: `node --check`、ESLint 无配置模式、E2E 6/6 与 Rust 全量门禁通过。
  `node --check`, ESLint without repository config, E2E 6/6, and full Rust gates passed.

## 下一执行任务 / Next execution task

**P0-C：安全基线 / Security baseline。** 先做只读审计与最小变更方案；CORS 策略或任何影响移动端来源的改动需要在实施前单独确认。
