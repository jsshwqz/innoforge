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

### P0-B.2 浏览器回归矩阵（第一批）/ Browser regression matrix (first batch)

- **状态 / Status**: ✅ 已完成 / Completed
- **代码提交 / Code commit**: `aad56d5` (`fix: 修复搜索页初始化并扩展浏览器回归`)
- **验证 / Verification**: `node --check`、ESLint 无配置模式、Puppeteer E2E 42/42、`cargo fmt --check`、`cargo clippy -- -D warnings` 与 `cargo test`（265 passed，1 个文档测试按设计忽略）通过。
  `node --check`, ESLint without repository config, Puppeteer E2E 42/42, `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` (265 passed; one doc test intentionally ignored) passed.

1. 只扩展已有 `e2e_test.mjs`，继续使用现有 Puppeteer；不新增依赖、后端路由、数据库写入或真实 AI 调用。
   Extend only the existing `e2e_test.mjs` with the current Puppeteer dependency; add no dependencies, backend routes, database writes, or real AI calls.
2. 对 8 个用户页面逐一验证 HTTP 成功、关键根节点存在、页面错误和失败请求为空：`/`、`/search`、`/patent/1`、`/idea`、`/ai`、`/compare`、`/settings`、`/oa-response`。
   Verify HTTP success, a critical root element, no page errors, and no failed requests for all eight user pages: `/`, `/search`, `/patent/1`, `/idea`, `/ai`, `/compare`, `/settings`, and `/oa-response`.
3. 只执行无需外部服务且不写数据库的关键交互，例如导航、标签切换、输入校验与本地渲染；AI、上传、删除、保存等有副作用的操作使用拦截或留给单独的受控用例。
   Execute only safe, side-effect-free key interactions such as navigation, tab switching, input validation, and local rendering; intercept or defer AI, uploads, deletion, and persistence to separate controlled cases.
4. 将失败信息统一为页面 URL、HTTP 状态、缺失选择器、浏览器异常和失败请求，成功计数可重复；在本机 Chrome/Edge 未自动下载 Chromium 时继续使用现有浏览器。
   Standardize failure output around page URL, HTTP status, missing selector, browser errors, and failed requests, with a repeatable pass count; keep using installed Chrome/Edge when Puppeteer Chromium is absent.
5. CODEX 实现后由主代理复跑语法、ESLint、浏览器 E2E 和完整 Rust 门禁；通过后提交与归档。
   After CODEX implements it, the primary agent reruns syntax, ESLint, browser E2E, and full Rust gates before committing and documenting.

### P0-B.3 OA 可审计讨论记录导出 / OA auditable discussion transcript export

1. 明确分离两种产物：现有 `exportDiscussionConclusions()` 保留其调用 AI 生成摘要的行为，但按钮改名为“AI 总结结论”；新增“导出完整讨论记录”按钮，避免用户把摘要误当成最终答复或完整过程。
   Clearly separate the two outputs: retain `exportDiscussionConclusions()` as an AI-generated summary but relabel it “AI Summary”; add an “Export Full Discussion Record” button so a summary is not mistaken for a final response or complete process.
2. 仅修改 `templates/office_action_response.html` 与 `static/i18n.js`，不新增依赖、API、数据库迁移或 AI 调用。导出在浏览器本地以 UTF-8 Markdown 下载，完整保留发给讨论的起始上下文及每一轮用户/助手原文；严禁 `slice`、`substring` 或其他数据截断。
   Change only `templates/office_action_response.html` and `static/i18n.js`; add no dependencies, APIs, migrations, or AI calls. Export a UTF-8 Markdown file locally in the browser, preserving the full initial discussion context and every original user/assistant message; prohibit `slice`, `substring`, or any other data truncation.
3. 为每条讨论历史消息记录 ISO 时间戳；保持发送到现有聊天和总结 API 的载荷为既有 `[role, content]` 双元组，确保兼容性。导出文件包含生成时间、产品提示、完整上下文、按顺序排列的角色/时间/原文，并明确标记为“原始记录，未由 AI 二次改写”。
   Record an ISO timestamp for each discussion-history message; preserve the existing `[role, content]` payload sent to chat and summary APIs for compatibility. The export contains generation time, product note, complete context, ordered role/timestamp/source text, and an explicit “original record, not AI-rewritten” marker.
4. 在 `e2e_test.mjs` 增加受控浏览器验证：构造本地讨论记录，点击导出按钮，读取下载内容并确认尾部标记、消息原文、角色与时间均完整；拦截所有网络下载以外的副作用，不调用真实 AI。
   Add a controlled browser test in `e2e_test.mjs`: construct a local discussion record, click the export button, inspect the download, and verify tail markers, full message source text, roles, and timestamps; intercept all effects except the local download and never call real AI.
5. CODEX 完成后由主代理依次执行 `node --check`、ESLint、浏览器 E2E、`cargo fmt --check`、`cargo clippy -- -D warnings` 与 `cargo test`；通过后更新 CHANGELOG、STATUS、errors 与本计划，再提交。
   After CODEX completes implementation, the primary agent runs `node --check`, ESLint, browser E2E, `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test`; after passing, update CHANGELOG, STATUS, errors, and this plan before committing.

### P0-B.2.1 浏览器矩阵基线修复 / Browser-matrix baseline remediation

- **状态 / Status**: ✅ 已完成 / Completed
- **代码提交 / Code commit**: `aad56d5` (`fix: 修复搜索页初始化并扩展浏览器回归`)
- **结果 / Result**: 修复搜索页 `updatePdfFileList()` 的加载顺序；专利详情在有数据时测试详情交互，空库时验证明确 404 提示，二者均不写入测试数据。
  Fixed the `updatePdfFileList()` load order on search; patent detail exercises interactions with data and verifies the explicit 404 prompt in an empty library, without writing test data in either case.

1. 修复 `templates/search.html` 的初始化顺序：不得在后续 `<script>` 中定义 `updatePdfFileList()` 前调用它；保留既有 PDF 元数据恢复与文件列表渲染语义，不改动上传、存储或网络请求。
   Fix the initialization order in `templates/search.html`: do not call `updatePdfFileList()` before its later `<script>` definition; preserve the existing PDF metadata restoration and file-list rendering behavior without changing uploads, storage, or network requests.
2. 调整 `e2e_test.mjs` 的专利详情页断言：数据库中存在目标专利时验证 HTTP 200、页面根节点和标签切换；空数据库时把 `/patent/1` 的明确 HTTP 404 视为受控无数据分支，验证其“专利未找到”提示并跳过详情 DOM 交互。两种分支均保持固定、可读的断言计数，绝不写入测试数据。
   Adjust the patent-detail assertion in `e2e_test.mjs`: when the target patent exists, verify HTTP 200, its root node, and tab switching; in an empty database, treat the explicit HTTP 404 from `/patent/1` as a controlled no-data branch, verify its “not found” message, and skip detail-DOM interaction. Both paths keep a fixed, readable assertion count and never seed test data.
3. 仅修改 `templates/search.html` 与 `e2e_test.mjs`，不新增依赖、后端路由、数据库 schema 或真实 AI 调用。CODEX 完成后，主代理复跑 JS 语法、ESLint、全量浏览器 E2E 和 Rust 三项门禁，再进入 P0-B.3。
   Change only `templates/search.html` and `e2e_test.mjs`; add no dependencies, backend routes, database schema, or real AI calls. After CODEX completes, the primary agent reruns JS syntax, ESLint, full browser E2E, and all three Rust gates before proceeding to P0-B.3.

## 下一执行任务 / Next execution task

**P0-C：安全基线 / Security baseline。** 先做只读审计与最小变更方案；CORS 策略或任何影响移动端来源的改动需要在实施前单独确认。

### P0-C.1 CORS 实施计划 / CORS implementation plan

1. 只修改 `src/common.rs`：移除 `CorsLayer::allow_origin(Any)`，不改路由、数据库或公开 API。
   Modify only `src/common.rs`: remove `CorsLayer::allow_origin(Any)` without changing routes, database, or public APIs.
2. 默认 allowlist 固定包含 `http://127.0.0.1:3000` 与 `http://localhost:3000`，保证桌面本地服务可用。
   The default allowlist includes `http://127.0.0.1:3000` and `http://localhost:3000` to preserve desktop local access.
3. 额外来源由 `INNOFORGE_CORS_ORIGINS` 提供（逗号分隔）；仅接受有效 HTTP Header 值，非法项忽略且不扩大默认权限。
   Additional origins come from comma-separated `INNOFORGE_CORS_ORIGINS`; accept only valid HTTP header values and ignore invalid values without widening default permissions.
4. 保留现有允许的方法和请求头；增加单元测试，验证默认来源、有效额外来源和非法来源处理。
   Preserve the existing allowed methods/headers; add unit tests for defaults, valid additional origins, and invalid-origin handling.
5. 完成后运行 CORS preflight/拒绝来源的本地 HTTP 验证、完整 Rust 门禁及 E2E；不新增 crate。
   Afterward run local HTTP checks for CORS preflight/denied origins, full Rust gates, and E2E; add no crates.

状态：✅ 已完成 / Completed

- **代码提交 / Code commit**: `15f134a` (`fix: 收紧本地服务 CORS 来源`)
- **结果 / Result**: CORS 默认仅放行两个本机来源；`INNOFORGE_CORS_ORIGINS` 支持以逗号分隔添加经过校验的 HTTP/HTTPS 来源，路径、查询、片段、用户信息、通配符和 `file:` 等无效值会被忽略。
  CORS now permits only two local origins by default; `INNOFORGE_CORS_ORIGINS` adds validated comma-separated HTTP/HTTPS origins, while paths, queries, fragments, user-info, wildcards, and `file:` values are ignored.
- **验证 / Verification**: 3 项 CORS 单元测试、允许/拒绝来源预检、Rust 全量门禁与 E2E 6/6 均通过。
  Three CORS unit tests, allowed/rejected-origin preflight checks, full Rust gates, and E2E 6/6 passed.

## 后续审计项 / Follow-up audit items

- 图片代理目前已有域名白名单，但仍使用字符串前缀解析主机；下一阶段应改用结构化 URL 解析并明确端口/重定向策略。
  The image proxy has a domain allowlist but still parses the host with string prefixes; a follow-up should use structured URL parsing and define port/redirect policy.

### P0-C.2 图片代理 SSRF 加固计划 / Image-proxy SSRF hardening plan

1. 只修改 `src/routes/patent.rs`，使用现有 `reqwest` 的 URL 类型解析请求地址；不新增依赖、路由或数据库变更。
   Modify only `src/routes/patent.rs`, using the existing `reqwest` URL type to parse request URLs; add no dependencies, routes, or database changes.
2. 仅允许 HTTPS、精确白名单主机、默认 HTTPS 端口；拒绝用户名/密码和非默认端口。保留图片 URL 的路径与查询参数，以免破坏合法的签名图片链接。
   Allow only HTTPS, exact allowlisted hosts, and the default HTTPS port; reject credentials and non-default ports. Preserve paths and query strings for valid signed image links.
3. 禁止自动跟随重定向，防止白名单域名重定向到内网地址；请求客户端构建失败返回友好 502，不使用生产 `unwrap`。
   Disable automatic redirects so an allowlisted host cannot redirect to an internal address; return a friendly 502 if client construction fails, with no production `unwrap`.
4. 增加同文件测试覆盖合法 URL、大小写/子域/用户名伪装、HTTP、非默认端口和无效 URL；运行 HTTP 验证、Rust 门禁和 E2E。
   Add same-file tests for valid URLs, case/subdomain/credential impersonation, HTTP, non-default ports, and malformed URLs; run HTTP verification, Rust gates, and E2E.

状态：✅ 已完成 / Completed

- **代码提交 / Code commit**: `1ee38f1` (`fix: 加固专利图片代理 SSRF 防护`)
- **结果 / Result**: 图片代理仅接受合法 HTTPS 白名单 URL，拒绝子域和用户名伪装、HTTP、非默认端口与无效地址；关闭重定向，防止 DNS/上游重定向绕过。
  The image proxy accepts only valid HTTPS allowlisted URLs and rejects subdomain/credential spoofing, HTTP, non-default ports, and malformed input; redirects are disabled to prevent DNS/upstream bypasses.
- **验证 / Verification**: 4 个 URL 单元回归、4 个恶意 URL 的 HTTP 403、Rust 全量门禁和 E2E 6/6 全部通过。
  Four URL unit regressions, HTTP 403 for four malicious URLs, full Rust gates, and E2E 6/6 all passed.
