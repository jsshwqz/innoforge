# Patent-Hub 开发进度状态追踪
# Patent-Hub Development Progress Tracker

> 本文档追踪项目的所有重大功能开发进度、状态变更和技术债务处理。
> This document tracks all major feature development progress, status changes, and technical debt handling.

---

## 状态变更日志 (Status Change Log)

### 2026-07-13 — AI 超时分级恢复 / AI timeout tier restoration (reverted from 60s ceiling)

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: 待用户终审后填写 / To be filled after user final review
- **修复 / Fix**: `cade390` 将 `CHAT`、`ANALYSIS`、`ENRICHMENT`、`PROVIDER_HTTP_TIMEOUT_SECS`、`GLOBAL_TIMEOUT_SECS` 统一为 60 秒，导致 OA 三步→一步合并后的单步大上下文分析超时失败。已恢复分级：`CHAT` 60s、`ANALYSIS` 180s、`ENRICHMENT` 300s、HTTP 客户端 300s、全局守卫 300s。
  Reverted the 60-second ceiling introduced by `cade390`. Restored tiered timeouts: `CHAT` 60s, `ANALYSIS` 180s, `ENRICHMENT` 300s, provider HTTP client 300s, global `tokio` guard 300s. OA analysis no longer times out.
- **验证 / Verification**: `cargo fmt --check` ✅, `cargo clippy --all-targets -- -D warnings` ✅, `cargo test` ✅ (137 unit + 6 orchestration + 37 integration passed; 1 doctest ignored). `git diff --check` ✅.
  已同步 `src/ai/client.rs` 模块注释、`src/ai/chat.rs` 注释、`src/routes/mod.rs` 测试断言。
  `src/ai/client.rs` module comment, `src/ai/chat.rs` comments, and `src/routes/mod.rs` test assertions synchronized.

### 2026-07-13 — Google 认证状态原子持久化 / Google authentication-state atomic persistence

- **状态 / Status**: ✅ 已完成，待用户终审与提交 / Completed, pending user final review and commit
- **提交 / Commit**: 待用户终审后填写 / To be filled after user final review
- **修复 / Fix**: gcloud CLI、ADC 文件、OAuth 授权码交换及三类后台 Token 刷新统一通过 `persist_google_auth_state()` 写入 SQLite 批量事务；事务成功后才更新运行时内存。OAuth 初次授权原子保存 access token、expiry、refresh token 与 `oauth` 模式；gcloud/ADC 初次认证明确清空旧 refresh token 并写入 `gcloud` 模式；后台刷新只替换 access token/expiry，保留既有 refresh token 与认证模式。
  gcloud CLI, ADC-file, OAuth authorization-code exchange, and all three background token-refresh paths now use `persist_google_auth_state()` to write a SQLite batch transaction before updating runtime memory. Initial OAuth atomically saves access token, expiry, refresh token, and `oauth` mode; initial gcloud/ADC explicitly clears a stale refresh token and writes `gcloud` mode; background refresh replaces only access token/expiry while retaining the existing refresh token and mode.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`（137 单元测试 + 6 编排集成测试 + 37 集成测试通过；1 个 doctest 按设计忽略）及 `git diff --check` 通过。新增 OAuth 完整保存、OAuth→gcloud 模式切换和后台刷新字段保留测试。

### 2026-07-13 — SerpAPI 多 Key 原子保存 / Atomic multi-key SerpAPI saves

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `5bbedbc`
- **修复 / Fix**: SerpAPI 请求完成全量输入验证后，现在把兼容单 Key 槽位和编号 `_1` 至 `_5` 槽位作为一次完整替换提交给 SQLite 事务；写入或提交失败时不会清空旧配置、不会改变内存，并返回友好错误。提交成功后才更新运行时 Key，`.env` 后备逐项失败会记录 warning 而不会泄露 Key 内容。
  After complete request validation, SerpAPI saves now submit legacy and numbered `_1` through `_5` slots as one SQLite transaction. A write or commit failure cannot clear the old configuration or change memory, and returns a friendly error. Runtime keys update only after commit; each failed `.env` backup is warned without exposing key material.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（309 passed, 1 ignored）和正式二进制构建通过；`/`、`/settings`、`/oa-response` 返回 HTTP 200，向 `POST /api/settings/serpapi` 提交非数组值得到受控错误且不写入配置。

### 2026-07-13 — AI 配置原子保存 / AI configuration atomic persistence

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `7628984`
- **修复 / Fix**: `api_save_ai` 先把 AI 地址、服务商与通用 Key、模型、Google 凭据和 Gemini CLI 开关共 8 项设置写入一个 SQLite 事务；任何执行或提交错误都会回滚、记录服务端诊断并返回友好 JSON，运行中的内存配置不会被提前切换。事务成功后才更新内存，`.env` 只保留为失败可记录的桌面后备。
  `api_save_ai` now writes the base URL, provider/general keys, models, Google credentials, and Gemini CLI switch as eight SQLite settings in one transaction. Any execution or commit error rolls back, logs server diagnostics, returns friendly JSON, and never switches the running configuration early. Memory updates only after success; `.env` is a warning-logged desktop backup.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（307 passed, 1 ignored）和正式二进制构建通过；`/`、`/settings`、`/oa-response` 均返回 HTTP 200，向 `POST /api/settings/ai` 提交无效协议返回受控错误且不写入配置。

### 2026-07-13 — SerpAPI Key 保存完整性 / SerpAPI Key save integrity

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `6be94b6`
- **修复 / Fix**: SerpAPI Key 保存改为先完整解析和校验、再执行清空/写入/内存更新。缺失或非数组、超过 5 项、空白或超长项、短 Key、非法字符和无法唯一还原的掩码均在任何持久化前返回用户友好错误；空数组仍代表用户明确清空。合法完整 Key 与唯一掩码保持原成功响应。
  SerpAPI Key saves now fully parse and validate before clearing, writing, or updating memory. Missing/non-array input, more than five entries, blank/oversized entries, short or malformed keys, and masks that cannot be uniquely restored return friendly errors before persistence; an empty array remains an explicit clear. Valid full keys and unique masks retain the prior success response.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（305 passed, 1 ignored）和正式二进制构建通过；实际 `POST /api/settings/serpapi` 提交短 Key 返回受控错误，未写入配置；`/`、`/settings`、`/oa-response` 均返回 HTTP 200。

### 2026-07-13 — 正则初始化 panic 加固 / Regex initialization panic hardening

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `26e20b2`
- **修复 / Fix**: 创意报告行内 Markdown、专利说明书 HTML 清理与 Sogou 法律状态页面解析的静态正则均改为缓存 `Result`，删除生产路径 `expect()`。异常时分别保留已转义原文、返回用户友好 JSON 或传递受控错误以触发既有降级链；诊断仅写入服务端日志。行内 Markdown 统一在函数入口转义用户文本，并新增格式和 XSS 回归。
  Static regexes used by idea-report inline Markdown, patent-description HTML cleanup, and Sogou legal-status parsing now cache a `Result`, removing production-path `expect()`. Failures respectively preserve escaped source text, return friendly JSON, or propagate a controlled error into the existing fallback chain; diagnostics stay server-side. Inline Markdown now consistently escapes user text at its entry point, with rendering and XSS regressions added.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（299 passed, 1 ignored）与正式二进制构建通过；`/`、`/idea`、`/oa-response` 均返回 HTTP 200。

### 2026-07-13 — DOCX 导出安全加固 / DOCX export safety hardening

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `b8b6e89`
- **修复 / Fix**: OA 意见陈述书的 DOCX 生成改为返回受控 `Result`；所有 ZIP 写入和收尾失败均不再触发生产路径 panic。专利号、申请人、审查意见类型和答复正文均经 XML 文本转义，避免 `&`、`<`、`>` 损坏 `word/document.xml`。接口在导出失败时记录详细错误并返回用户友好提示，成功响应格式保持不变。
  OA response-letter DOCX generation now returns a controlled `Result`; every ZIP write/finalization failure avoids production-path panic. Patent number, applicant, office-action type, and response text are XML-text escaped so `&`, `<`, and `>` cannot corrupt `word/document.xml`. The API logs detailed failures and returns a friendly message while preserving the successful response shape.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（295 passed, 1 ignored）和正式二进制构建通过；`/`、`/oa-response` 及携带特殊字符的本地 `POST /api/oa/export-docx` 均返回 HTTP 200。内存 DOCX 回归测试解压并验证所有四个非可信字段的 XML 转义。

### 2026-07-13 — 移动端嵌入服务生命周期加固 / Mobile embedded-server lifecycle hardening

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `6f193ec`
- **修复 / Fix**: FFI 启动路径将 Tokio runtime 和线程创建错误传播回调用方，返回码为 `1` 而非 panic；服务器状态 Mutex 中毒同样返回受控错误。重复启动不会再替换或丢弃已有句柄，关闭流程先在短锁作用域取出句柄，再在锁外发送关闭信号并等待线程退出。
  The FFI start path now propagates Tokio runtime and thread-creation failures to the caller as return code `1` rather than panicking; a poisoned server-state mutex also returns a controlled error. Duplicate starts no longer replace or lose an existing handle, and shutdown takes the handle in a short lock scope before signalling and joining outside the lock.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings` 和 `cargo test`（293 passed, 1 ignored）通过。正式二进制重新构建后，`/` 与 `/oa-response` 均返回 HTTP 200。

### 2026-07-13 — 专利图片代理响应边界 / Patent image-proxy response boundaries

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `421df26`
- **修复 / Fix**: 既有的可信 HTTPS 域名校验之外，图片代理现在关闭环境代理，限制上游响应为 PNG/JPEG/GIF/WebP/BMP/AVIF 等安全栅格 MIME，拒绝缺失类型、SVG、HTML 和其它类型。通过 `Content-Length` 预检与 `chunk()` 流式累计双重检查，将单张图片限制为 20 MiB；同时移除每次响应 `leak()` MIME 字符串的内存泄漏。
  In addition to existing allowlisted-HTTPS URL validation, the image proxy now disables environment proxies and accepts only safe raster MIME types (PNG/JPEG/GIF/WebP/BMP/AVIF), rejecting missing types, SVG, HTML, and other types. A `Content-Length` precheck plus streamed `chunk()` accumulation limits each image to 20 MiB and removes the per-response MIME-string memory leak.
- **验证 / Verification**: 新增 MIME 正规化/危险类型、声明长度和流式边界回归；`cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（293 passed, 1 ignored）通过。正式二进制的 `/` 和 `/oa-response` 返回 HTTP 200；不可信 HTTP 图片 URL 返回 HTTP 403。

### 2026-07-13 — 本地上传 PDF 文件签名校验 / Local PDF-upload signature validation

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `8170576`
- **修复 / Fix**: `api_upload_pdf_store`、文档对比上传、通用文本提取和专利 PDF 专用提取现在都会在写入磁盘或调用 PDF 解析、OCR、AI 视觉兜底前检查首 1024 字节内的 `%PDF-` 文件签名。仅把文本或 HTML 改名为 `.pdf` 的请求会得到用户可见的错误；专利专用入口还会对直传和远程下载在汇合后的字节统一复检。
  `api_upload_pdf_store`, document comparison upload, general text extraction, and patent-specific PDF extraction now inspect the `%PDF-` signature in the first 1024 bytes before disk writes, PDF parsing, OCR, or AI vision fallback. Text or HTML merely renamed to `.pdf` receives a user-visible error, and the patent-specific endpoint rechecks both direct and remote bytes at their shared boundary.
- **验证 / Verification**: 纯函数回归覆盖有效签名、前导空白、HTML、普通文本伪装和 1024 字节边界；`cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（285 passed, 1 ignored）以及正式二进制 HTTP 回归均通过。伪造 PDF 被 `/api/upload/pdf-store` 明确拒绝，`/` 与 `/oa-response` 均返回 HTTP 200。

### 2026-07-13 — AI 单次调用 60 秒上限 / AI single-call 60-second ceiling

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `cade390`
- **修复 / Fix**: Chat、Analysis、Enrichment 的调用时钟、默认提供商 HTTP 客户端、全局 `tokio` 守卫和 OA 流式调用均统一为 60 秒。保留原有重试和错误降级，但全局守卫确保单次调用不会因重试超出上限。
  Chat, Analysis, and Enrichment clocks, the default provider HTTP client, the global `tokio` guard, and OA streaming now use 60 seconds. Existing retries and error fallback remain, while the global guard prevents a single call from exceeding the ceiling.
- **验证 / Verification**: 新增常量回归并更新全局守卫回归；`cargo fmt --check`、`cargo clippy -- -D warnings` 和 `cargo test`（285 passed, 1 ignored）通过。正式二进制重建后 `/` 与 `/oa-response` 均返回 HTTP 200。

### 2026-07-13 — AI 提示词输入边界加固 / AI prompt-input boundary hardening

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `be815cb`
- **修复 / Fix**: `/api/ai/chat` 现在仅接受 `user` 与 `assistant` 历史角色，伪造的 `system`/未知角色会在请求上游前得到友好错误。专利记录、联网搜索、OA 分析与审查意见、讨论、最新意见和结论导出材料均用转义后的 `<user_input>` 边界隔离；原始自定义角色只作为受限偏好，服务端预设仍可作为可信角色。
  `/api/ai/chat` now accepts only `user` and `assistant` history roles; forged `system` or unknown roles receive a friendly error before any upstream request. Patent records, web results, OA analysis/office actions/discussion/latest input, and conclusion-export material use escaped `<user_input>` boundaries; raw custom roles are bounded preferences while server presets remain trusted roles.
- **验证 / Verification**: 新增 4 条边界/角色回归；`cargo fmt --check`、`cargo clippy -- -D warnings` 和 `cargo test`（283 passed, 1 ignored）通过。重新构建后 `/` 与 `/oa-response` 均为 HTTP 200；真实 POST 请求携带伪造 `system` 历史时返回本地友好错误且不调用 AI。

### 2026-07-13 — D 盘运行期 PDF 临时文件治理 / D-drive runtime PDF temporary-file remediation

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `dcb446d`
- **修复 / Fix**: 所有外部 PDF 提取路径（视觉回退、pdftotext、PyMuPDF、MinerU、OCR）现将临时输入及视觉 PNG 输出限制在项目工作目录的 `data/runtime-temp`。文件使用 UUID 和 `create_new` 独占创建，并由 RAII 守卫覆盖成功、失败和提前返回的清理；`pdftotext` 改为捕获标准输出，Umi-OCR 删除未使用的磁盘副本。
  External PDF extraction paths (vision fallback, pdftotext, PyMuPDF, MinerU, and OCR) now keep temporary inputs and vision PNG outputs under project-working-directory `data/runtime-temp`. UUID plus `create_new` prevents collisions, while an RAII guard cleans on success, failure, and early returns; pdftotext captures stdout and Umi-OCR no longer writes an unused disk copy.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings` 和 `cargo test`（275 passed, 1 ignored）全部通过；临时文件回归确认路径和清理行为，目录无残留；新二进制的 `/` 与 `/oa-response` 均返回 HTTP 200。

### 2026-07-13 — 远程专利 PDF 下载安全加固 / Remote patent-PDF download hardening

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `2fd64ab`
- **修复 / Fix**: 专利记录中的远程 `pdf_url` 现在仅允许 HTTPS 主机名、默认 443
  端口和无凭据 URL。服务端解析并固定全部公网 DNS 地址，关闭环境代理和重定向，
  拒绝非 2xx、超过 20 MB、流式超限和无有效 PDF 签名的响应，防止 SSRF、DNS
  重绑定及内存耗尽。
  Remote `pdf_url` values now require an HTTPS hostname on the default port with no credentials.
  The server resolves and pins only public DNS addresses, disables proxies and redirects, and
  rejects non-2xx, oversized, streaming-overlimit, and invalid-PDF responses to prevent SSRF,
  DNS rebinding, and memory exhaustion.
- **验证 / Verification**: 新增 URL/IP/大小/PDF 签名/localhost DNS 回归；`cargo fmt
  --check`、`cargo clippy -- -D warnings` 和 `cargo test`（273 passed, 1 ignored）通过；
  新版服务的 `/` 与 `/oa-response` 均返回 HTTP 200。


### 2026-07-13 — OA 可审计完整讨论记录导出 / OA auditable full discussion-record export

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `5588968`
- **新增 / Added**: OA 讨论区新增“导出完整讨论记录”，在浏览器本地下载 UTF-8 Markdown，不调用 AI 或新增后端接口。记录保留起始系统上下文、每轮用户/AI 原文、角色、ISO 时间戳和导出时间；含反引号的原文会使用动态 Markdown 代码围栏完整保存。
  The OA discussion panel now provides “Export Full Discussion Record”, a local UTF-8 Markdown download with no AI call or new backend endpoint. It preserves the initial system context, every user/AI source message, role, ISO timestamp, and export time; source text with backticks is retained using dynamic Markdown fences.
- **改进 / Improvement**: 原“导出结论”明确更名为“AI 总结结论”，避免把二次 AI 摘要误认为最终答复或讨论全过程。
  The former “Export Conclusions” action is explicitly relabelled “AI Summary” so a second AI summary is not mistaken for a final response or the complete discussion.
- **验证 / Verification**: Puppeteer E2E 48/48 真实点击导出按钮并读取受控 Blob 内容，确认全文尾部、反引号、角色、时间戳、原始记录声明和零 AI 请求；JS/Rust 门禁全部通过。
  Puppeteer E2E 48/48 clicks the export button and reads the controlled Blob content, verifying full tails, backticks, roles, timestamps, the original-record notice, and zero AI requests; all JS/Rust gates passed.

### 2026-07-13 — 八页面浏览器回归与搜索页初始化修复 / Eight-page browser regression and search initialization fix

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `aad56d5`
- **修复 / Fix**: 搜索页在定义 `updatePdfFileList()` 前调用它，导致页面加载时出现 `ReferenceError`；现已保持原有 PDF 恢复语义并在函数声明后调用。
  Search called `updatePdfFileList()` before defining it and produced a load-time `ReferenceError`; it now preserves the original PDF-restoration behavior and invokes it after the declaration.
- **测试 / Tests**: `e2e_test.mjs` 扩展为 42 项真实浏览器回归：8 个页面的 HTTP/关键节点/浏览器异常/失败请求/无副作用交互，并保留 OA 参数校验与长文本尾标记完整性。专利详情在有数据时验证标签交互，空库时验证明确的 404 提示；不写入测试数据或调用真实 AI。
  `e2e_test.mjs` now has 42 real-browser regressions: HTTP/critical node/browser-error/failed-request/side-effect-free interaction checks across eight pages, plus OA validation and long-payload tail integrity. Patent detail exercises tabs with data and validates the explicit 404 prompt in an empty library; it neither seeds data nor calls real AI.
- **验证 / Verification**: `node --check`、ESLint 无配置模式、Puppeteer 42/42、`cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（265 passed，1 ignored）通过。
  `node --check`, ESLint without repository config, Puppeteer 42/42, `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` (265 passed, 1 ignored) passed.

### 2026-07-13 — OA 后端数据完整性加固 / OA backend data integrity hardening

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `ce303d2`
- **修复 / Fix**: OA 讨论及答复书生成路径改为保留完整输入；超过容量时按 Unicode 字符计数返回用户可见的字段级错误，不再静默截断 OA 原文、分析或讨论历史。
  Discussion and response-letter paths now retain complete input and return visible field-specific Unicode-character capacity errors rather than silently truncating OA material, analysis, or discussion history.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings` 与 `cargo test` 通过（245 项通过，1 项文档测试按设计忽略）。
  `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` passed (245 passed; one doc test intentionally ignored).

### 2026-07-13 — OA 可重复端到端回归 / OA reproducible end-to-end regression

- **状态 / Status**: ✅ 已完成 / Completed
- **文件 / File**: `e2e_test.mjs`
- **覆盖 / Coverage**: 首页与 OA 页面加载、浏览器页面异常/失败请求、审核修改方案接口参数校验，以及超长请求体三字段尾标记保留；不调用真实 AI 服务。
  Home/OA page loading, browser page errors/request failures, amendment-check parameter validation, and tail-marker preservation across all three long-payload fields; no real AI call.
- **验证 / Verification**: `node --check e2e_test.mjs`、ESLint 无配置模式、`node e2e_test.mjs`（6/6）及 Rust 全量门禁通过。
  `node --check e2e_test.mjs`, ESLint without repository config, `node e2e_test.mjs` (6/6), and the full Rust gates passed.

### 2026-07-13 — 本地服务 CORS 收紧 / Local-service CORS hardening

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `15f134a`
- **修复 / Fix**: 默认跨域来源从全开放改为本机 `http://127.0.0.1:3000` 与 `http://localhost:3000`；通过 `INNOFORGE_CORS_ORIGINS` 可安全添加 HTTP/HTTPS 来源，无效配置项被忽略。
  Default CORS changed from open access to local `http://127.0.0.1:3000` and `http://localhost:3000`; `INNOFORGE_CORS_ORIGINS` can safely add HTTP/HTTPS origins while invalid entries are ignored.
- **验证 / Verification**: 允许来源预检返回对应 `access-control-allow-origin`，不受信任来源无该响应头；fmt、clippy、245 项 Rust 测试和 E2E 6/6 通过。
  The allowed-origin preflight returns its `access-control-allow-origin`, while an untrusted origin receives none; fmt, clippy, 245 Rust tests, and E2E 6/6 passed.

### 2026-07-13 — 专利图片代理 SSRF 加固 / Patent image-proxy SSRF hardening

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `1ee38f1`
- **修复 / Fix**: 图片代理改为结构化 URL 校验，仅允许 HTTPS、精确白名单主机、默认端口且不含凭据；禁用自动重定向，防止借白名单主机跳转到内网。
  The image proxy now validates structured URLs: HTTPS, exact allowlisted host, default port, and no credentials; redirects are disabled to prevent allowlisted hosts from reaching internal targets.
- **验证 / Verification**: 协议、子域、用户名伪装与非默认端口均 HTTP 403；新增 4 项 URL 回归测试，Rust 全量门禁与 E2E 6/6 通过。
  HTTP, subdomain, username-spoofing, and non-default-port inputs all return 403; four URL regressions, full Rust gates, and E2E 6/6 passed.

### 2026-07-13 — Windows 启动脚本修复 / Windows launcher fix

- **状态 / Status**: ✅ 已完成 / Completed
- **文件 / File**: `start.bat`
- **修复 / Fix**: 移除 debug 构建括号代码块内 `echo` 文本的未转义圆括号，避免 CMD 报“此时不应有 ...”。
  Removed unescaped parentheses from the debug-build echo inside a CMD block.
- **验证 / Verification**: 通过 `start.bat` 完成 debug 编译和后台启动；`/` 与 `/oa-response` 均 HTTP 200。
  `start.bat` compiled and started the server; both `/` and `/oa-response` returned HTTP 200.

### 2026-07-13 — OA 前端数据完整性修复 / OA frontend data integrity remediation

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `f496648`
- **核心改动 / Core changes**:
  - `templates/office_action_response.html`: 移除修改校验与讨论上下文中的 6 个正文截断表达式，覆盖 5 个逻辑问题组
    Removed six body truncations across five logical issue groups in amendment checking and discussion context construction
  - 保留日期、SSE 协议解析、消息数组和选区操作等非数据用途的 `slice`
    Preserved non-data slices used for dates, SSE parsing, message arrays, and text selection
- **验证 / Verification**: `cargo fmt --check`、clippy、245 项 Rust 测试及定制 Puppeteer 数据完整性回归通过
  `cargo fmt --check`, clippy, 245 Rust tests, and a focused Puppeteer integrity regression passed
- **已知后续项 / Follow-ups**: OA 讨论后端仍有 60k/40k/15k 字符静默截断；模板存在与 HEAD 相同的 16 个历史 ESLint `no-redeclare` 错误；仓库缺少规约引用的 `e2e_test.mjs`
  The OA discussion backend still silently truncates at 60k/40k/15k characters; the template retains 16 baseline `no-redeclare` errors identical to HEAD; the referenced `e2e_test.mjs` is absent

### 2026-07-09 — OA 分析模块：三步→一步+缓存+超时移除 (v0.7.4)

- **PR**: 三步→一步 prompt 重构、OA 缓存、超时移除、论点看板修复
- **状态**: ✅ 开发完成，待提交
- **核心改动**:
  - `ai/patent.rs`: OA 分析从 3 步串行合并为 1 步，deep mode 精简输出
  - `routes/ai.rs`: OA 缓存（patent_number + oa_type + depth）、超时移除
  - `office_action_response.html`: 论点看板修复（本地 AI chat 调用）
  - `ARCHITECTURE.md`: 补充 OA 模块章节（5a 节）
- **关联 PR 号**: v0.7.4
- **技术债务**: ⏳ OA 数据库存储方案（长期）

### 2026-07-08 — 文件解析器重构 (v0.7.3)

- **PR**: 重构文件解析器，提升 PDF/DOCX/DOC 解析准确性
- **状态**: ✅ 已完成
- **核心改动**:
  - `file-parser.rs`: 重构 `parse_file_to_markdown` 和 `get_preview_text`
  - PDF 解析器: OCR 模式支持、文字层检测优化
  - DOCX 解析器: 表格解析、图像提取、结构化内容处理
  - DOC 解析器: `docx2txt-js` 替代方案
- **关联 PR 号**: v0.7.3
- **技术债务**: 无

### 2026-07-07 — 数据库 schema 优化 (v0.7.2)

- **PR**: 扩展 `documents` schema 以支持文件预览信息
- **状态**: ✅ 已完成
- **核心改动**:
  - `schema.sql`: 新增 `file_content`, `file_ext`, `is_processed`, `last_processed_at` 字段
  - `db/document.rs`: 新增文档处理状态查询接口
  - `routes/document.rs`: 文档处理 API 完善
  - 修复 `documents` 与 `case_documents` 关系
- **关联 PR 号**: v0.7.2
- **技术债务**: 无

### 2026-07-06 — OCR 模式支持

- **PR**: 添加文件上传 OCR 模式
- **状态**: ✅ 已完成
- **核心改动**:
  - `routes/upload.rs`: OCR 模式参数处理
  - `file-parser.rs`: OCR 模式 PDF/DOC/DOCX 解析
  - 支持 Tesseract OCR 引擎
- **关联 PR 号**: v0.7.1
- **技术债务**: OCR 性能优化（异步处理）

### 2026-07-05 — 研创台 AI 分析模块（InnoForge 核心功能）

- **PR**: 实现研创台 AI 分析全链路
- **状态**: ✅ 已完成
- **核心改动**:
  - `routes/ai.rs`: `/api/ai/innovation/analyze`, `/api/ai/innovation/analyze-stream`, `/api/ai/innovation/compare` 等端点
  - `ai/innovation.rs`: 分析引擎（专利地图 + 对比分析 + 策略建议）
  - `templates/innovation_analysis.html`: 前端展示
  - **AI 分析样板文档**: `docs/研创台 AI 分析样板.doc` — 提供 4 个场景的详细分析报告
- **关联 PR 号**: v0.7.0
- **技术债务**: 无

### 2026-07-04 — 研创台 UI 增强

- **PR**: 修复研创台样式问题、添加批量操作、新增搜索功能
- **状态**: ✅ 已完成
- **核心改动**:
  - 批量删除/批量重命名功能
  - 高级搜索（标题/类型/日期/关键词）
  - UI 样式统一
- **关联 PR 号**: v0.6.9
- **技术债务**: 无

---

## 当前版本 (Current Version)

**版本**: v0.7.4 (开发中)
**发布日期**: 2026-07-09
**主要特性**:
- OA 分析三步→一步重构，消除超时风险
- OA 缓存机制，减少重复 API 调用
- 超时移除，让 provider 300s 兜底
- 论点看板修复，使用本地 AI chat 服务
- ARCHITECTURE.md 补充 OA 模块描述

**版本历史**:
- v0.7.4 (2026-07-09): OA 分析重构
- v0.7.3 (2026-07-08): 文件解析器重构
- v0.7.2 (2026-07-07): 数据库 schema 优化
- v0.7.1 (2026-07-06): OCR 模式支持
- v0.7.0 (2026-07-05): 研创台 AI 分析模块

---

## 技术债务 (Technical Debt)

1. **OA 数据库存储方案**: 长期，当前 OA 数据仅存储在 `case_documents` 中，未来可扩展专用 OA 数据库表
2. **OCR 性能优化**: 异步处理，避免阻塞主线程
3. **文件解析器错误处理**: 需要更完善的错误处理和用户提示
4. **前端验证基线**: 补齐 `e2e_test.mjs`，并清理 `office_action_response.html` 现有 16 个 ESLint `no-redeclare` 错误

---

## 下一步计划 (Next Steps)

1. **OA 数据库表**: 设计并实现 OA 专用表结构（审查意见历史、答复历史、审批流程）
2. **性能监控**: 为关键 API 端点添加性能监控和日志
3. **测试覆盖**: 为 OA 分析模块和文件解析器添加单元测试
4. **文档补全**: 为 OA 模块前端页面添加使用说明
