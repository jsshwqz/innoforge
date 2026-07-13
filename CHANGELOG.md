# 更新日志 / Changelog

所有重要变更都会记录在此文件中。格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/)。
All notable changes are documented here. Format based on [Keep a Changelog](https://keepachangelog.com/).

---

## [Unreleased]

### 新增 / Added
- **OA 完整讨论记录导出** — OA 讨论区新增纯本地 Markdown 导出，保留起始上下文、每轮用户/AI 原文、角色和 ISO 时间戳；导出不发起 AI 请求，且能安全保留含 Markdown 反引号的原文
  OA full discussion-record export: a local-only Markdown export preserves the initial context, every user/AI source message, role, and ISO timestamp; it makes no AI request and safely retains source text containing Markdown backticks

### 安全修复 / Security Fixes
- **专利图片代理响应边界** — 图片代理现在禁用环境代理、仅接受 PNG/JPEG/GIF/WebP/BMP/AVIF 等安全栅格图片 MIME，拒绝缺失类型、SVG 与 HTML；通过声明长度预检和流式累计双重限制，将单张上游图片限制为 20 MiB，且不再为每次响应泄漏 MIME 字符串
  Patent image-proxy response boundaries: the proxy now disables environment proxies, accepts only safe raster image MIME types (PNG/JPEG/GIF/WebP/BMP/AVIF), rejects missing types, SVG, and HTML, and enforces a 20 MiB per-image limit via both declared-length and streamed-body checks without leaking a MIME string per response
- **本地上传 PDF 文件签名校验** — 四个本地 PDF 上传入口现在都会在保存文件、调用解析器、OCR 或视觉兜底前验证首 1024 字节内的 `%PDF-` 签名，拒绝仅伪装为 `.pdf` 的文本或 HTML；专利专用提取入口对直传和远程下载汇合后的内容统一复检
  Local PDF-upload signature validation: all four local PDF ingress paths now require a `%PDF-` signature within the first 1024 bytes before storage, parsing, OCR, or vision fallback. Text or HTML merely named `.pdf` is rejected, and the patent-specific extractor revalidates both direct-upload and remote-download bytes at their shared boundary
- **AI 提示词输入边界** — 聊天历史现仅接受 `user` 和 `assistant` 角色，拒绝客户端伪造的 `system`/未知角色；专利记录、联网搜索、OA 材料、讨论记录和用户自定义角色偏好均以不可逃逸的 `<user_input>` 数据边界传给 AI。原始自定义角色不再拥有系统指令权限，服务端预设角色保持可用
  AI prompt input boundaries: chat history now accepts only `user` and `assistant`, rejecting client-forged `system` or unknown roles. Patent records, web results, OA material, discussion records, and raw custom role preferences use non-escapable `<user_input>` data boundaries; raw custom roles no longer have system-instruction authority while server presets remain available

### 修复 / Fixed
- **DOCX 导出错误处理与 XML 转义** — OA 意见陈述书导出不再在 ZIP 写入失败时 panic；所有写入失败会记录服务端诊断信息并向用户返回可理解的导出失败提示。专利号、申请人、审查意见类型和答复正文均会按 XML 文本节点转义，避免特殊字符损坏 Word 文档结构。
  DOCX export error handling and XML escaping: OA response-letter export no longer panics when ZIP writing fails; failures are logged server-side and returned as a friendly export error. Patent number, applicant, office-action type, and response text are XML-escaped as text nodes so special characters cannot corrupt the Word document.
- **移动端嵌入服务生命周期** — Android/iOS FFI 启停入口不再因 Tokio 运行时创建、线程创建或服务器状态锁失败而 panic；重复启动会明确拒绝且保留原服务句柄，关闭操作在释放状态锁后再等待线程退出
  Mobile embedded-server lifecycle: Android/iOS FFI start and shutdown no longer panic on Tokio runtime creation, thread creation, or server-state lock failure. Duplicate starts are explicitly refused without replacing the original handle, and shutdown waits for the thread only after releasing the state lock
- **AI 单次调用时限** — 聊天、分析、增强处理、默认 HTTP、多模态与流式 OA 调用现统一受 60 秒上限约束；现有重试与用户友好的超时错误保留，但不会让一次请求等待数分钟
  AI single-call timeout: chat, analysis, enrichment, default HTTP, multimodal, and streaming OA calls now share a 60-second ceiling. Existing retries and user-friendly timeout errors remain, but a request can no longer wait for minutes
- **D 盘运行期 PDF 临时文件** — PDF 解析、视觉回退、OCR 和 MinerU 不再使用 Windows 系统临时目录；文件现在以不可预测的 UUID 名称在项目 `data/runtime-temp` 下独占创建，并由作用域守卫在成功、失败和提前返回时清理。`pdftotext` 直接捕获标准输出，Umi-OCR 不再写入未使用的副本
  D-drive runtime PDF temporary files: PDF extraction, vision fallback, OCR, and MinerU no longer use the Windows system temp directory. UUID-named files are created exclusively under project `data/runtime-temp` and cleaned by a scope guard on success, failure, and early return; `pdftotext` captures stdout directly and Umi-OCR no longer writes an unused copy
- **远程专利 PDF 下载安全** — 从专利记录下载 PDF 时，现仅允许 HTTPS 主机名、默认
  443 端口和无凭据 URL；服务会校验并固定公网 DNS 结果、禁用代理和重定向、拒绝非
  2xx/超过 20 MB/非 PDF 响应，防止 SSRF、DNS 重绑定和内存耗尽
  Remote patent-PDF download security: only HTTPS hostname URLs on the default port without
  credentials are accepted; public DNS answers are validated and pinned, proxies and redirects
  are disabled, and non-2xx, oversized, or non-PDF responses are rejected to prevent SSRF,
  DNS rebinding, and memory exhaustion
- **OA 导出语义澄清** — 原“导出结论”实际会调用 AI 生成二次摘要，现改名为“AI 总结结论”，并与可审计的完整原始记录导出分开
  OA export semantics clarified: the former “Export Conclusions” action actually invokes AI to generate a second summary; it is now labelled “AI Summary” and separated from the auditable full-source-record export
- **搜索页 PDF 初始化顺序** — 将 `updatePdfFileList()` 移至其定义之后执行，消除搜索页加载时的 `ReferenceError`，不改变已保存 PDF 元数据的恢复逻辑
  Search-page PDF initialization order: `updatePdfFileList()` now runs after its definition, eliminating the load-time `ReferenceError` without changing persisted-PDF restoration
- **专利图片代理 SSRF 防护** — 图片 URL 改用结构化解析，限制 HTTPS、精确白名单主机和默认端口，禁止凭据与自动重定向；保留合法签名图片链接的路径和查询参数
  Patent image-proxy SSRF hardening: image URLs now use structured parsing with HTTPS, exact-host, and default-port restrictions; credentials and redirects are blocked while valid signed-image paths and queries are preserved
- **本地服务 CORS 边界** — 移除全开放跨域来源；默认仅允许本机 `127.0.0.1:3000` 与 `localhost:3000`，移动端或桌面壳的额外来源可通过 `INNOFORGE_CORS_ORIGINS` 显式配置
  Local-service CORS boundary: removed open cross-origin access; only local `127.0.0.1:3000` and `localhost:3000` are allowed by default, while mobile/desktop-shell origins can be explicitly configured with `INNOFORGE_CORS_ORIGINS`
- **OA 后端数据完整性** — OA 讨论和答复书生成不再为控制上下文而静默截断原文；超出明确容量时返回带字段、实际字符数和上限的可见错误，并按 Unicode 字符计数
  OA backend integrity: discussion and response-letter flows no longer silently truncate source material; oversized inputs return a visible field-specific Unicode-character capacity error
- **OA AI 上下文数据完整性** — 移除 OA 修改校验和讨论流程中 6 个前端正文截断表达式，覆盖审查意见、本专利、对比文件和既有分析结果，避免长材料在提交 AI 前静默丢失
  OA AI context integrity: removed six frontend body truncations from amendment checking and discussion flows so office actions, the subject patent, references, and existing analysis reach AI requests intact
- **Windows 启动脚本解析** — 修复 `start.bat` debug 构建分支中未转义圆括号导致的 CMD 解析错误，快捷方式现在可以完成编译并启动服务
  Windows launcher parsing: removed unescaped parentheses from the debug build echo line so the `start.bat` shortcut can compile and start the server

### 测试 / Tests
- **八页面浏览器回归矩阵** — `e2e_test.mjs` 扩展至 42 项无副作用真实浏览器检查，覆盖 8 个用户页面的 HTTP、关键节点、浏览器异常、失败请求和本地交互；专利详情同时支持有数据与空库 404 的受控分支，OA 长文本完整性和接口参数校验仍受覆盖
  Eight-page browser regression matrix: `e2e_test.mjs` now has 42 side-effect-free browser checks covering HTTP, critical nodes, browser errors, failed requests, and local interactions across all eight user pages; patent detail supports both populated and empty-library 404 branches while OA long-payload integrity and endpoint validation remain covered
- **OA 可重复端到端回归** — 新增仓库内 `e2e_test.mjs`，覆盖首页/OA 页面加载、审核修改方案接口连通性，以及超长 OA 请求体尾部标记保留；脚本不调用真实 AI 服务
  Reproducible OA E2E regression: added repository-owned `e2e_test.mjs` covering home/OA loading, amendment-check connectivity, and long-payload tail preservation without calling a real AI service

## [v0.7.3] - 2026-07-04

### 修复 / Fixed
- **DeepSeek reasoning_content 兼容** — DeepSeek v4-flash 流式 SSE 将文本放在 `delta.reasoning_content` 而非 `delta.content`，服务端 SSE 解析器只读取 `content`，导致讨论/分析等流式调用返回空内容。`ResponseMessage` 新增 `reasoning_content` 字段，SSE 解析器增加 `reasoning_content` 兜底
  DeepSeek streaming SSE compatibility: added `reasoning_content` fallback in both struct deserialization and streaming parser

- **OA 渲染乱码根因修复** — DeepSeek API 请求缺失 `max_tokens` 导致默认 4K 截断，流式输出被切断后浏览器显示 `*`/`D`/`★` 乱码。`src/ai/chat.rs` 流式请求补全 `"max_tokens": 16384`，彻底解决截断问题
  OA garbled text root cause fixed: missing `max_tokens` in DeepSeek requests caused 4K truncation; added `max_tokens: 16384` to all streaming calls

- **DOMPurify 崩溃修复** — `static/purify.min.js` 文件损坏（18KB，缺少函数体）导致 `DOMPurify.sanitize()` 调用报 `Uncaught SyntaxError`，OA 页面 JS 整块不执行。重新下载完整版（21KB + sourcemap）替换
  DOMPurify crash fixed: corrupted `purify.min.js` (18KB, missing function body) caused `SyntaxError` on entire OA page JS; replaced with complete version (21KB + sourcemap)

- **OA 上下文扩容** — `src/ai/patent.rs` OA 分析与讨论截断上限从 120K/80K/120K 提升到 300K/200K/300K，确保长 OA 分析不丢上下文
  OA context expanded: truncation limits raised from 120K/80K/120K to 300K/200K/300K for complete analysis

### 改进 / Improved
- **讨论 SSE 解析增强** — `sendOADiscussion()` 统一使用 `event:` 行解析模式（与 `doOAAnalysis()` 一致），更稳定地处理 SSE error/done 事件
  Discussion SSE parser enhanced: consistent `event:` line handling across all streaming functions

- **讨论传入 `oa_type`** — 讨论 API 请求增加 `oa_type` 字段，提供更丰富的上下文给 AI
  `oa_type` field added to discussion API requests for richer AI context

### 新增 / Added
- **Fusion 多模型辩论引擎** — 支持多模型协同辩论，流水线每步独立模型配置，输出格式校验，S.U.P.E.R 架构规范写入 CLAUDE.md
  Fusion multi-model debate engine: multi-model collaboration, per-step model config, output format validation, S.U.P.E.R architecture spec in CLAUDE.md

---

## [v0.7.2] - 2026-07-01

### 新增 / Added
- **SenseNova V6.7 Flash Lite / DeepSeek V4 Flash 模型支持** — 设置页新增 SenseNova 6.7 Flash Lite 和 DeepSeek V4 Flash 模型选项，同时更新 SenseNova 模型标签（Flash/Turbo/Pro/Ultra → Flash Lite/Flash/Turbo/Pro）
  New model support: SenseNova 6.7 Flash Lite, DeepSeek V4 Flash; SenseNova model labels updated
- **OA 答复 Markdown 无分隔行表格支持** — `office_action_response.html` 模板升级 Markdown 渲染器，支持不含分隔行的表格语法，兼容更多 AI 输出格式
  OA response template: Markdown renderer enhanced to support separator-less tables
- **OA 答复增强规划文档** — `docs/plans/oa-response-enhancement.md` 新增 P0-P3 规划（docx 导出 / 全流程持久化 / 拆条分析等）
  OA enhancement roadmap document added (P0-P3 planning for docx export, persistence, structured analysis)

### 改进 / Improved
- **JS 错误屏障系统** — 全站 8 个页面引入全局 JS 错误屏障，统一拦截运行时错误并显示友好提示，杜绝白屏/黑屏
  Global JS error barrier across all 8 pages — catches runtime errors with user-friendly messages, no more blank screens
- **DOMPurify 全局保护** — 将 DOMPurify 初始化移至 `i18n.js` 全局模块，覆盖全部 8 个页面，新页面不再需要单独加保护
  DOMPurify protection centralized in `i18n.js`, covers all 8 pages automatically
- **Puppeteer 端到端测试** — 新增 Puppeteer 测试框架 + 基础页面加载测试，为后续自动化回归测试奠基
  New Puppeteer test framework with basic page-load tests for E2E regression testing
- **ESLint 代码规范** — 新增 package.json + ESLint 配置，JS 代码规范检查工具链
  ESLint config added for JavaScript code quality enforcement
- **start.bat debug 加速** — 调试模式使用 `cargo run` 而非 `cargo build --release`，启动速度大幅提升
  start.bat debug mode uses `cargo run` instead of `cargo build --release` for faster startup
- **AI 上下文预览大幅提升** — claims 预览上限从 3K→8K、content 从 2K→15K、description 从 3K→30K，让 AI 获取更完整的专利上下文提高分析质量
  AI context window expanded: claims 3K→8K, content 2K→15K, description 3K→30K for richer patent context
- **流式 debug 日志** — `chat.rs` 新增首块日志和 total_chars 追踪，便于排查流式响应异常
  Streaming debug logging: first-chunk log + total_chars tracking for easier streaming diagnostics
- **区别特征标记优化** — 专利分析结果中区别特征标记从 ✅ 改为 ★，更醒目直观
  Distinguishing feature marker changed from ✅ to ★ for better visual clarity
- **Google Patents 回捞增强** — 回捞逻辑优化，专利号提取和格式化更加健壮
  Google Patents refetch improvements: more robust patent number extraction and formatting
- **AI 上下文 10 倍扩容** — `patent.rs` 3 函数 6 处 `safe_truncate` 上限提升（30K→300K / 20K→200K / 60K→600K），`ai.rs` 10 处 `.take()` 预览上限同步放大（5K→50K / 15K→150K / 20K→200K），AI 能获取完整专利全文
  AI context expanded 10x across patent analysis pipeline (30K→300K chars etc.) for complete patent document access
- **GitFlow 分支策略** — 切换为 main/dev/feature 分支模型，main 受保护需 PR 合并，CI 覆盖 dev 分支
  GitFlow branch strategy adopted: main protected (PR only), dev with CI, feature branches
- **分离 Gitee remote** — 修复 origin 同时推 GitHub+Gitee 的配置问题，origin 专推 GitHub
  Separated Gitee remote from origin to fix dual-push configuration issues

### 修复 / Fixed
- **patent_detail 页全部按钮失效** — `const box` 与 `var box` 命名冲突导致 JS 整块不执行，修复后按钮正常响应
  All patent_detail page buttons were dead due to `const box` vs `var box` naming conflict; fixed
- **patent_detail 标签页与按钮全部失效** — JavaScript 加载顺序问题导致标签页切换和操作按钮全部失灵，修复 DOM 就绪检测
  Fixed JavaScript load order issue causing tab switching and action buttons to fail
- **PDF 导出图片兜底** — 专利 PDF 导出时若图片 URL 不存在，不再崩溃，改为嵌入 PDF 链接文本
  PDF export image fallback: when image URL is missing, embeds PDF link text instead of crashing
- **Umi-OCR 异步 + 所有权修复** — Umi-OCR 调用中异步处理不当导致所有权错误，修复 reqwest multipart 特性补全
  Umi-OCR async + ownership fix; reqwest multipart feature flag completed
- **search 页 PDF 上传错误屏障** — `updatePdfFileList` 未定义时加入错误屏障忽略，防止干扰其他功能
  Error barrier for undefined `updatePdfFileList` on search page
- **未使用变量警告** — `upload.rs` 中 `tmp_pdf_str` 加 `_` 前缀消除 clippy 警告
  Unused variable warning in upload.rs fixed (`tmp_pdf_str` → `_tmp_pdf_str`)
- **OA 错误信息被覆盖** — SSE 流式错误后 `showError` 被通用"分析失败"覆盖，改 `} else if (!errored)` 保留详细错误
  OA error message overwrite bug fixed: detailed error no longer replaced by generic "分析失败"

### 文档 / Documentation
- **防回退机制** — CLAUDE.md 验证强化 + errors.md 复盘补录，防止已修复问题重新出现
  Anti-regression documentation: enhanced CLAUDE.md verification rules + errors.md retrospective records

---

## [v0.7.0] - 2026-06-26

### 新增 / Added
- **AI 对话角色预设系统** — 新增 5 种角色预设（专利审查专家、OA 答复专家、权利要求分析师、发明人头脑风暴），用户可在 AI 对话侧边栏切换角色，AI 回复风格和专业知识自动适配。支持自定义 system_prompt 传入
  AI chat role preset system: 5 presets (Patent Examiner, OA Expert, Claim Analyst, Inventor Brainstorm) with auto-adapted response style and expertise; custom system_prompt support
- **MCP 服务器新增 3 个专利分析工具** — `patent_threat_assessment`（X/Y/A 威胁分类）、`patent_claim_chart`（权利要求逐项对照表）、`patent_multi_compare`（3+ 专利多维对比矩阵）
  3 new MCP tools: threat assessment (X/Y/A classification), claim chart (element-by-element mapping), multi-compare (3+ patents)
- **专利威胁评估 API** — 后端 `/api/ai/threat-assessment`，批量评估多篇对比专利的威胁等级，输出威胁矩阵 + 组合威胁分析 + 答辩策略
  Threat assessment API: bulk X/Y/A classification with similarity matrix and response strategy
- **权利要求对照图表 API** — 后端 `/api/ai/claim-chart`，逐元素映射权利要求到对比文件，含等效/等同分析
  Claim chart API: element-by-element mapping with equivalency analysis
- **流水线新增第 16 步：OA 答复辅助分析** — `GenerateOaResponse` 作为流水线最终步骤，整合权利要求分析、现有技术聚类、新颖性评分、AI 深度分析，自动生成 OA 答复辅助分析报告（5 部分结构化输出）
  Pipeline step 16: GenerateOaResponse — integrates claim analysis, prior art clusters, novelty scoring, AI deep analysis to generate structured OA response report

### 改进 / Improved
- **AI 对话流式响应支持 system_prompt** — `api_ai_chat` 和 `api_ai_chat_stream` 均支持传入自定义 system_prompt，可覆盖默认角色设定
  AI chat streaming now supports custom system_prompt override
- **流水线进度条更新** — 从 15 步同步更新为 16 步，前端进度显示适配新步骤
  Pipeline progress bar synced from 15 to 16 steps
- **PDF 专利文本提取增强** — `extract_pdf_text()` 新增 Rust 原生逐页提取（`extract_text_from_mem_by_pages`）作为第二级降级，在多栏中文专利排版下比标准模式效果更好。新增 `/api/patent/pdf/extract-text` 端点，返回按页分段的 JSON 数组，方便 AI 逐页分析。零新依赖
  PDF text extraction enhanced: added page-by-page extraction as 2nd fallback (better for multi-column Chinese patents); new `/api/patent/pdf/extract-text` endpoint returns per-page JSON array; zero new dependencies

---

## [v0.6.3] - 2026-06-22

### 新增 / Added
- **OA 一审结构化分析** — Prompt 重写，强制 AI 输出 5 部分固定结构：权利要求逐项解析 / 审查员驳回逻辑还原 / 特征对比总表 / 逐权利要求反驳论点 / 意见陈述书草稿。每项权利要求单独论述，引用对比文献具体段落
  OA first exam structured analysis: 5-part fixed output (claim breakdown / examiner logic / feature comparison table / counter-arguments / response draft)
- **分析结果分段展示** — 前端将 5 部分分析渲染为彩色卡片，特征对比表（绿色边框）突出显示，意见陈述书草稿可独立导出 `.txt` 直接粘贴提交
  Analysis result displayed in 5 color-coded cards; response draft exportable as `.txt`
- **深度模式增强** — `extract_oa_response_section` 辅助函数将 critique 审查范围缩小到第五部分意见陈述书，避免冗余分析
  Deep mode: critique now targets only the response draft section

### 改进 / Improved
- **OA 审查员自检增强** — `oa_critique` prompt 增加特征对比准确性检查维度，要求 AI 从审查员视角核实对比文献公开内容认定的准确性
  OA critique prompt enhanced with feature comparison accuracy verification
- **OA 分析 SSE 流式支持** — 新增 `send_chat_stream` 通用流式方法 + `office_action_response_stream` OA 流式端点 `/api/ai/office-action-response/stream`。前端实时显示文本生成过程，deep 模式两阶段流式（主分析 + 审查员自检），失败自动回退普通 POST
  SSE streaming support for OA analysis: real-time text display, two-phase streaming for deep mode (analysis + critique), automatic fallback to regular POST

### 修复 / Fixed
- **DOMPurify OA 页面遗漏** — `office_action_response.html` 仍使用 CDN 引用，导致断网时 JS 崩溃全黑。已替换为 `/static/purify.min.js`
  DOMPurify CDN → local in OA page (was missed in v0.6.2)
- **系统代理导致 API 连接失败** — Windows HTTP_PROXY 环境变量（`http://127.0.0.1:10808`）被 reqwest 库自动读取，拦截了 DeepSeek 请求。reqwest 客户端强制 `.no_proxy()` 绕过系统代理
  Windows system proxy (`HTTP_PROXY=127.0.0.1:10808`) blocked DeepSeek API calls; fixed with `.no_proxy()` on reqwest client
- **AI 请求超时 45s 过短** — `PROVIDER_HTTP_TIMEOUT_SECS` 从 45 秒提升至 180 秒，解决 DeepSeek v4-pro 推理模型处理长 prompt 超时问题
  HTTP timeout increased from 45s to 180s for reasoning model response times
- **start.bat 找不到 cargo** — `cargo` 不在 Windows 系统 PATH 中时编译失败。加入 `set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"`
  start.bat: added cargo to PATH for systems where it's not in Windows environment
- **start.bat 启动慢** — 每次运行都执行 `cargo build --release`（3-4分钟）。改为检测到已有二进制时跳过编译直达启动
  start.bat: skip build if release binary already exists (instant startup)
- **getRefs 重复推送** — `refs.push()` 对同一数据推送两次（`_uploadedData` + `uploadedData` Proxy），删除重复行
  Fixed duplicate refs.push() in getRefs function
- **dev.bat 同样缺少 cargo 路径** — 加入相同的 PATH 修复
  dev.bat: same cargo PATH fix

---

## [v0.6.2] - 2026-06-26

### 新增 / Added
- **搜索页 PDF 上传** — 搜索页新增 PDF 上传区，支持拖拽上传 + 点击选择，文件送至 `/api/upload/extract` 提取文本后结合搜索分析
  Search page PDF upload zone with drag-and-drop, text extraction via backend API
- **首页文件上传持久化** — 上传的文件信息通过 localStorage 持久化，页面刷新后自动恢复，不丢失文件引用
  Homepage file upload persistence: files survive page refresh via localStorage

### 改进 / Improved
- **DOMPurify 本地化** — 6 个模板（index/idea/ai/settings/patent_detail/oa-response）从 CDN 引用替换为 `/static/purify.min.js`，编译进二进制，完全消除网络依赖和断网黑屏
  DOMPurify localized: 6 templates use embedded `/static/purify.min.js`, no CDN dependency
- **启动脚本重构** — `start.bat` 重写为始终编译（删除旧二进制跳过逻辑），新增 `dev.bat` 调试模式快速启动脚本
  start.bat rewritten to always recompile; new dev.bat for debug-mode fast startup
- **代码质量加固** — 修复 3 个 clippy 警告（`manual_flatten` / `unnecessary_map_or` × 2），零警告通过
  3 clippy fixes: manual_flatten, 2x unnecessary_map_or → is_some_and
- **.gitignore 增强** — 增加 `.reasonix/` / 临时工具脚本 / 测试适配器 / 备份文件等保护规则
  .gitignore strengthened with entries for temp scripts, test adapters, backup files

### 修复 / Fixed
- **search.html CSS 重复** — 移除搜索页 PDF 上传区样式定义重复，修复多余 `</script>` 标签
  Fixed duplicate CSS definitions and extra `</script>` tag in search page

---

## [v0.6.1] - 2026-05-23

### 改进 / Improved
- **代码质量加固** — 消除生产路径 14 处 `unwrap()` 调用（`idea.rs` / `pages.rs` / `patent.rs` / `db/patent.rs` / `upload.rs`）
  Production code quality: eliminated 14 `unwrap()` calls across 5 source files
- **Pipeline 步骤同步** — 前端进度条从 14 步更新为 15 步（补齐 PriorArtCluster），子步骤显示从 AiDeepAnalysis 独享泛化为所有步骤通用
  Pipeline visualization: synced to 15 steps, sub-step progress generalized to all steps
- **聊天消息分段加载** — 后端 `GET /api/chat/:key` 支持 `?limit=N&offset=M` 分页，前端默认加载最近 50 条，提供"加载更多"按钮
  Chat pagination: backend paginated API + frontend load-more UI
- **文档治理** — STATUS.md / CLAUDE.md / docs/plans 版本同步，根目录冗余文件清理
  Documentation governance: STATUS.md, CLAUDE.md, plans synced to v0.6.1

### 修复 / Fixed
- **prior_art_cluster unwrap** — `best_cluster.unwrap().1` → `is_none_or()`（clippy 建议）
  prior_art_cluster unwrap eliminated via is_none_or
- **CI/CD release.yml** — 支持 `workflow_dispatch` 手动触发 + 按平台重命名二进制（解决 Linux/macOS 同名冲突）
  CI/CD: workflow_dispatch support + platform-specific binary naming

### 修复 / Fixed
- **Gitee Release 同步** — 补齐 v0.5.9 tag，创建 v0.5.10 / v0.6.0 Release 条目
  Gitee release sync: added missing v0.5.9 tag, created v0.5.10/v0.6.0 releases

---

## [v0.6.0] - 2026-05-23

### 新增 / Added
- **OA 答复分析页面** — 全新 `/oa-response` 独立页面，支持三种审查意见类型：一审/二审答复、非正常申请答辩、驳回后复审请求，各有专属 system prompt
  New OA Response page (`/oa-response`): supports 3 office action types — 1st/2nd OA, abnormal application, re-examination after rejection
- **三档分析深度** — OA 分析支持快速概览（单次简要）、标准分析（特征对比表+分层策略）、深度穷追（标准+AI 自检反驳，两次串行调用），用户按需选择
  Three-level analysis depth: Quick (single-pass brief), Standard (feature table + layered strategy), Deep (Standard + AI self-critique, two-pass serial)
- **AI 自检反驳机制** — 深度模式下 AI 先提答复方案，再换审查员视角逐条挑刺，输出「审查员视角预判」附在方案后
  AI self-critique: Deep mode generates response first, then re-analyzes from examiner's perspective, outputting "examiner's predicted objections"
- **特征对比表 + 分层策略** — 标准/深度模式下 AI 强制输出权利要求逐特征对比表，以及「否定技术启示→强调协同效果→保底修改」三层论证
  Feature comparison table + layered strategy: Standard/Deep modes output claim-by-claim feature comparison table and 3-tier argument structure
- **侧边栏导航 OA 入口** — 全局导航增加「OA答复」链接，方便快速访问
  Sidebar nav: added "OA Reply" link for quick access

### 改进 / Improved
- **AI 通俗表达规则** — 深度/中度模式新增第 8/7 条规则，要求 AI 保留全部技术分析能力但用通俗语言呈现结论和推理过程，避免堆砌公式
  Plain language rule added to deep/medium prompts: AI retains full analytical capability but explains conclusions in accessible language
- **代码质量加固** — 消除生产路径 10+ 处 `unwrap()` 调用（`idea.rs` / `pages.rs` / `patent.rs` / `db/patent.rs` / `upload.rs`），包括 `Response::builder().unwrap()` → match 模式、`text_result.unwrap()` → 统一 match、`country.unwrap()` → `country_val` 提前绑定、`unwrap_or_default()` 替代
  Production code quality: eliminated 10+ `unwrap()` calls across 5 source files, replaced with match/`unwrap_or_default()`/`expect()` patterns
- **文档同步** — STATUS.md 版本同步到 v0.6.0、CLAUDE.md Pipeline 步数修正为 15 步、v0.6.0 计划文档创建
  Documentation sync: STATUS.md updated to v0.6.0, CLAUDE.md pipeline step count corrected to 15, v0.6.0 plan doc created
- **仓库清理** — 删除根目录冗余 `main.rs`（过时副本）和 `write_start.py` 工具脚本，`.gitignore` 增加防护
  Repo cleanup: removed stale root `main.rs` and `write_start.py`, added .gitignore entries

### 修复 / Fixed
- **引用按钮不显示** — ai.html 的 getQuoteBtn 添加 document.body.contains 检测，修复 renderAllMessages 清空 DOM 后引用按钮丢失的问题
  Fixed quote button disappearing after renderAllMessages clears DOM — added document.body.contains check in getQuoteBtn
- **时间显示为中国时区** — idea.html 版本列表、侧边栏创意列表、对话消息三处时间从 UTC 改为 CST (UTC+8)，新增 toCST() 辅助函数
  Fixed timezone: idea page timestamps (version list, sidebar, chat messages) now display in CST (UTC+8) via new toCST() helper

### 新增 / Added
- **讨论深度选择器** — IDEA 创意讨论支持浅度/中度/深度三档，控制 AI 回答的分析深度和严谨程度
  Discussion depth selector: shallow/medium/deep modes for idea discussion, controls AI analysis depth
- **全站暂停按钮** — AI 助手、专利详情、创意讨论、方案对比四个页面统一增加 ■ 停止按钮，可中止 AI 回复后立即输入新话题
  Universal stop button: all 4 chat pages (AI Chat, Patent Detail, Idea Discussion, Compare) support AbortController-based stop
- **全站导出结论** — 所有对话页面统一支持将讨论历史导出为结构化结论（已定决策/达成的结论/待解决问题/风险项及等级）
  Universal export conclusions: all chat pages can export structured conclusions (decisions/conclusions/pending issues/risks)
- **通用导出结论 API** — 新增 `POST /api/ai/chat/conclusions`，支持 session_key 和 history 双源
  New API: POST /api/ai/chat/conclusions supporting session_key (DB) or history (in-memory) sources

### 改进 / Improved
- **深度模式 Prompt 重构** — AI 从「提问者」改为「深度回答者」，三档均不再向用户提问，改为控制回答深度
  Depth prompt refactored: AI changed from questioner to deep-answerer across all 3 modes
- **AI 超时提升至 300 秒** — 解决大上下文讨论超时问题
  AI timeout increased from 60s to 300s to handle large context discussions
- **Gemini CLI 误用修复** — 非 Gemini 服务商不再错误启用 Gemini CLI 模式
  Gemini CLI no longer applies to non-Gemini providers via is_gemini guard
- **启动日志增强** — 启动时输出 AI 服务商、模式、超时信息，便于排查
  Startup config log: prints AI provider, mode, timeout for easier debugging

### 修复 / Fixed
- **DeepSeek 模型名称修正** — 前端模型下拉列表移除无效的 deepseek-chat-v4-flash，仅保留有效模型
  Fixed DeepSeek model list: removed invalid model names, only deepseek-v4-flash and deepseek-v4-pro shown
- **设置页 Gemini CLI 联动** — 切换非 Gemini 服务商时自动禁用 Gemini CLI 开关
  Settings page: auto-disables Gemini CLI toggle when switching away from Gemini provider

### 技术 / Technical
- **新增自动化测试** — 3 个测试覆盖 Gemini CLI 模式保护和超时配置（共 121 测试通过）
  3 automated tests for Gemini CLI guard and timeout config (121 total tests passing)

## [v0.5.9] - 2026-05-14

### 新增 / Added
- **设置页 AI 服务商预设** — 支持 DeepSeek / 小米 MiMo / 商汤 SenseNova / OpenRouter / Google Gemini 一键切换，模型下拉随服务商动态变化
  Settings page: AI provider presets (DeepSeek/Xiaomi/SenseTime/OpenRouter/Gemini) with dynamic model dropdowns
- **聊天图片粘贴支持** — AI 助手页面支持 Ctrl+V 粘贴图片，基于 Gemini 等多模态模型识别图片内容
  AI Chat: paste image support (Ctrl+V), uses multimodal models (Gemini) for image recognition
- **专家模型配置（深度分析）** — 设置页新增"专家模型"下拉框，独立于默认模型，用于创造性分析、侵权评估等高推理任务
  Expert model config: separate dropdown for deep analysis tasks (inventiveness, infringement, claims, office action)
- **SerpAPI 余额/用量显示** — 设置页新增 SerpAPI 余额进度条，显示 plan_name、searches_per_month、当月用量、剩余次数
  Settings page: SerpAPI balance indicator with progress bar (plan, quota, usage, remaining)
- **对话跨设备同步** — AI 助手和专利详情页的聊天记录从 localStorage 迁移到后端 SQLite 持久化，换设备不丢失
  Cross-device chat sync: chat history migrated from localStorage to backend SQLite persistence
- **聊天记录 CRUD API** — 新增 `GET/POST /api/chat/:key` 和 `POST /api/chat/:key/delete` 三个端点
  Chat records CRUD API: 3 new endpoints for get/save/delete chat messages
- **CI/CD 自动化** — GitHub Actions: push/PR 到 main 自动运行 fmt + clippy + test；打 v* tag 自动构建全平台 Release
  CI/CD: GitHub Actions CI (fmt + clippy + test) and Release (build for ubuntu/windows/macos, publish)

### 改进 / Improved
- **AI 专家深度增强** — 4 个深度分析端点（创造性三步法分析、权利要求分析、侵权评估、审查意见答复）自动使用专家模型（默认 deepseek-reasoner），提升分析质量；inventiveness_analysis 提示词增加结构化输出（结论/置信度/证据/反论/下一步）
  AI expert enhancement: 4 deep analysis endpoints now use expert model (default deepseek-reasoner); inventiveness prompt enhanced with structured output (conclusion/confidence/evidence/counterargument/next-step)
- **分析页面模型标识** — 创造性分析、一审答复、权利要求分析、侵权评估按钮添加 (Reasoner) 标签，明确标识使用深度推理模型
  Analysis page model badges: (Reasoner) label added to inventiveness, OA, claims analysis, and risk assessment buttons
- **SerpAPI 中文搜索精度** — 中文查询自动附加 `hl=zh-cn&gl=cn&lr=lang_zh-CN` 参数，提升中文专利搜索命中率
  SerpAPI Chinese search accuracy: auto-attach hl/gl/lr params for Chinese queries
- **本地 FTS5 排序权重** — BM25 权重调优：标题 10.0、专利号 5.0、摘要 5.0、权利要求 3.0、申请人 2.0、发明人 2.0、IPC 1.0
  FTS5 BM25 ranking weights tuned: title 10.0, patent_number 5.0, abstract 5.0, claims 3.0, applicant 2.0, inventor 2.0, IPC 1.0
- **数据库 schema 升级到 v13** — 新增 `chat_records` 表用于聊天记录持久化
  Database schema upgraded to v13 (chat_records table)
- **聊天输入框升级** — input 替换为 textarea，支持 Shift+Enter 换行、Enter 发送，支持图片粘贴预览
  Chat input upgraded to textarea with Shift+Enter for newlines, Enter to send, image paste preview

### 修复 / Fixed
- **Gemini API 双斜杠 URL 导致 404** — AI client URL 拼接时 trim 尾斜杠，修复 Gemini base_url 尾部 `/` 导致 `openai//chat/completions` 404 问题
  Fixed Gemini API 404 due to double-slash URL (trailing slash in base_url)
- **Gemini 模型配额选择** — 默认模型从 gemini-2.0-flash（免费配额耗尽）改为 gemini-2.5-flash（有可用配额）
  Default Gemini model changed from gemini-2.0-flash (quota=0) to gemini-2.5-flash (working)

### 测试 / Tests
- **聊天记录集成测试** — 新增 6 个测试覆盖 chat CRUD、会话隔离、边界条件
  6 new chat record integration tests (CRUD, isolation, edge cases)
- **Schema 版本断言更新** — 3 个集成测试中的版本断言从 12 更新为 13
  3 integration tests updated schema version assertion from 12 to 13

---

## [v0.5.8] - 2026-05-11

### 新增 / Added
- **SerpAPI 多 Key 轮询** -- 支持配置最多 5 个 SerpAPI Key 自动轮询，突破单账号每月 250 次限制
  SerpAPI multi-key round-robin -- supports up to 5 keys to bypass monthly 250-query limit
- **设置页多 Key UI** -- 设置页面支持动态增删 SerpAPI Key 行
  Settings page multi-key UI with dynamic add/delete rows
- **AI 对话持久化** — AI 助手页 (`/ai`) 和专利详情页 (`/patent/:id`) 的聊天记录通过 localStorage 持久化保存，页面刷新后自动恢复
  AI chat persistence via localStorage for AI Assistant and Patent Detail pages (survives page refresh)
- **网络错误重试按钮** — 三个 AI 对话页面（AI 助手 / 专利详情 / 创意推演）在网络错误时显示 🔄 重试按钮，一键重发上一条消息，无需手动复制
  Retry button on network errors for all 3 AI chat pages, one-click resend of last message

### 改进 / Improved
- **搜索源精简** — 仅保留 SerpAPI + 本地数据库，移除 Firecrawl/Bing/Lens.org/CNIPR/搜狗，代码减少约 1100 行
  Search sources simplified to SerpAPI + local DB only (removed Firecrawl/Bing/Lens.org/CNIPR/Sogou, ~1100 lines removed)
- **AI 服务商精简** — 仅保留 DeepSeek，移除 6 个备用 AI 服务商（智谱/OpenRouter/Gemini/OpenAI/NVIDIA/Ollama）
  AI providers simplified to DeepSeek only (removed 6 fallback providers)
- **设置页 UI 简化** — 只显示 SerpAPI 多 Key 管理和 DeepSeek 配置，删除已移除服务的配置项
  Settings page UI simplified to show only SerpAPI + DeepSeek configuration
- **`.claude/settings.local.json` 权限配置修正** — 删除无效的 `allowMode` 键，按官方文档使用 `defaultMode: "bypassPermissions"`
  Fixed Claude Code permissions config: removed invalid `allowMode` key, uses official `defaultMode: "bypassPermissions"`

### 修复 / Fixed
- 设置页删除 Key 行后保存丢失数据（`querySelectorAll` 修复）
  Fixed key loss after deleting a row on settings page (querySelectorAll fix)
- AI 备用服务商自动启用 bug（`ai_client()` 回退到主 AI 优先）
  Fixed auto-promotion of backup AI providers (ai_client() now prefers primary)
- 生产路径 `unwrap()` 消除（search.rs 等）
  Removed unwrap() from production code paths (search.rs, etc.)
- `api_patent_pdf` Handler 编译错误（内联方法链拆解）
  Fixed api_patent_pdf Handler trait compilation error

### 移除 / Removed
- 搜索源：Firecrawl / Bing / Lens.org / CNIPR / 搜狗（已移除全部代码）
- AI 服务商：智谱 GLM / OpenRouter / Gemini / OpenAI / NVIDIA / Ollama（仅保留 DeepSeek）
- 配置项：`bing_api_key` / `lens_api_key` / `firecrawl_api_key` / `cnipr_*` / `ai_fallbacks`

---

## [v0.5.7] - 2026-05-02

### 新增 / Added
- **Firecrawl 专利兜底搜索** -- 新增 Firecrawl 作为 SerpAPI 超时/无结果时的降级源
  Added Firecrawl patent fallback search when SerpAPI times out or returns no results

### 改进 / Improved
- 启动与搜索质量优化
  Startup and search quality improvements

---

## [v0.5.6] - 2026-04-18

### 新增 / Added
- **研发状态机持久化（ResearchState Persistence）** -- 新增 `idea_research_state` 表，研发假设/排除路径/开放问题/已验证结论可独立持久化并跨续跑保留
  Added persistent `idea_research_state` table for hypothesis/excluded paths/open questions/verified claims across runs
- **ResearchState API** -- 新增读取与更新接口（`GET/POST /api/idea/:id/research-state`）
  Added read/update API for research state (`GET/POST /api/idea/:id/research-state`)
- **中途重定向重跑（Redirect Run）** -- 新增 `POST /api/idea/:id/redirect`，支持注入新约束并从指定步骤跳转重跑
  Added redirect endpoint to inject constraints and restart from a specified step (`POST /api/idea/:id/redirect`)

### 改进 / Improved
- Orchestrator 每步成功后自动落盘 `ResearchState`，删除创意时级联清理对应研发状态
  Orchestrator now persists research state after each successful step and cleans it up on idea deletion
- 数据库 schema 升级到 v12，并补齐集成测试覆盖 `research_state` CRUD
  Database schema upgraded to v12 with integration coverage for `research_state` CRUD

---

## [v0.5.4] - 2026-04-18

### 新增 / Added
- **移动端 C ABI 启动入口** -- 主仓新增/补齐 innoforge_start_server + patent_hub_start_server 兼容别名
  Mobile C ABI startup entry -- main repo adds/completes innoforge_start_server + patent_hub_start_server compatible aliases
- **Desktop 壳入口对齐** -- 窗口入口改为本地 http://127.0.0.1:3000，核心品牌字段对齐
  Desktop shell entry alignment -- window entry changed to local http://127.0.0.1:3000, core brand fields aligned
- **iOS 壳对齐** -- 改用 innoforge.db 与 innoforge_start_server，Bundle Identifier/显示名对齐
  iOS shell alignment -- switched to innoforge.db and innoforge_start_server, Bundle ID/display name aligned
- **Harmony 品牌对齐** -- app 元数据与页面文案品牌版本对齐至 0.5.4
  Harmony brand alignment -- app metadata and page text brand version aligned to 0.5.4

---

## [v0.4.4] - 2026-04-03

### 新增 / Added
- **多维深度推演引擎** -- AI 深度分析升级为 7 轮多维推演（科学推导/辩证批判/知识审计/本质还原/减法思维/跨域映射 + 跨维度合成）
  Multi-dimensional deep reasoning engine -- 7-round analysis (scientific deduction / dialectical critique / knowledge audit / essence reduction / subtractive thinking / cross-domain mapping + synthesis)

### 修复 / Fixed
- 侵权评估结果字段名修正（assessment → analysis，前端可正确显示）
  Risk assessment field name fix (assessment → analysis)
- 创意删除级联清理特征卡片（FK constraint 修复）
  Idea deletion cascades to feature cards (FK constraint fix)
- 13 步文案全面修正（模板 + 注释中残留的 "12 步" 全部更新）
  All "12-step" text updated to "13-step" across templates and comments
- 设置页备用 AI 表单 XSS 加固（innerHTML → createElement）
  Settings page fallback AI form XSS hardening
- gitignore 清理（忽略临时测试文件）
  gitignore cleanup for temp test files

---

## [v0.4.3] - 2026-04-02

### 新增 / Added
- **Pipeline 13 步** -- 新增 PriorArtCluster 步骤（现有技术按主题聚类），优化矛盾检测上游数据
  Pipeline upgraded to 13 steps -- PriorArtCluster groups similar prior art by topic
- **特征卡片系统** -- Feature Card CRUD + 差异对比 API（`/api/ideas/:id/feature-cards`, `/api/feature-cards/diff`）
  Feature Cards system -- CRUD + diff comparison API for structured idea features
- **AI 流式聊天** -- SSE 方式逐字返回 AI 回答（`/api/ai/chat/stream`）
  AI streaming chat -- SSE-based token-by-token response
- **Pipeline 断点续跑** -- 中断后可从快照恢复（`/api/idea/:id/resume`）
  Pipeline resume -- restore from snapshot after interruption
- **HTML 验证报告** -- 可视化验证结果页面（`/api/idea/:id/report.html`）
  HTML validation report -- visual report page
- **批量创意对比** -- 2-10 个创意同时对比（`/api/ideas/batch-compare`）
  Batch idea comparison -- compare 2-10 ideas simultaneously
- **CORS 中间件** -- 支持跨域 API 调用（MCP 客户端等）
  CORS middleware -- enables cross-origin API calls for MCP clients

### 安全修复 / Security Fixes
- **XSS 防护** -- 全部 5 个 HTML 模板加入 DOMPurify，innerHTML 改为 createElement + textContent
  XSS protection -- DOMPurify added to all 5 HTML templates, innerHTML replaced with safe DOM APIs
- **SSRF 防护** -- 图片代理添加域名白名单（仅允许 googleapis.com / espacenet.com / sogou.com）
  SSRF protection -- image proxy now validates URL against domain allowlist
- **AI 全局超时** -- 从 24 分钟最坏情况降为 60 秒全局上限
  AI global timeout -- worst case reduced from 24 min to 60s global cap

### 改进 / Improved
- Pipeline 通道自动清理（超 5 分钟自动移除，防止内存泄漏）
  Pipeline channel auto-cleanup (removes stale entries after 5 min)
- 数据库 schema 升级到 v6（新增 feature_cards 表）
  Database schema upgraded to v6 (feature_cards table)
- Skill Router 安全规则：允许代码审查类任务讨论漏洞
  Skill Router security: allow code review tasks that discuss vulnerabilities

---

## [v0.4.2] - 2026-03-30

### 新增 / Added
- **历史记录增强** -- 显示精确时间（HH:MM）而非仅日期，同一天多条记录可清晰区分
  History records enhancement -- show precise time (HH:MM), clearly distinguish same-day entries
- **内容摘要预览** -- 历史列表显示创意描述前 40 字，快速识别每条记录
  Description preview -- show first 40 chars in history list for quick identification
- **对话计数标识** -- 显示每条创意的对话消息数，一目了然
  Chat count indicator -- show conversation message count per idea
- **自动滚动** -- 点击历史记录后自动滚动到对话区域
  Auto-scroll to discussion panel when clicking history items

### 改进 / Improved
- IdeaSummary 增加 description 和 message_count 字段
  IdeaSummary includes description and message_count fields
- list_ideas 查询优化，子查询统计消息数
  list_ideas query optimized with subquery for message count

---

## [v0.4.1] - 2026-03-30

### 新增 / Added
- **首页重塑** -- 从搜索框改为研发助手入口，三种模式（AI 对话 / 快速验证 / 深度验证）
  Homepage redesign -- from search box to R&D assistant entry with 3 modes
- **AI 对话增强** -- 专家级 prompt + 思维框架（第一性原理 / TRIZ / 逆向工程）
  AI conversation enhancement -- expert-level prompt with thinking frameworks
- **智能上下文管理** -- 超 8 轮自动压缩 + 摘要长记忆
  Smart context management -- auto-compress after 8 rounds + summary long memory
- **历史记录管理** -- 支持删除创意记录（级联清理对话）
  History management -- delete ideas with cascading message cleanup
- **文件上传** -- 首页拖拽上传 + 聊天区 📎 附件按钮（PDF/Word/图片）
  File upload -- drag & drop on homepage + chat attachment button

### 修复 / Fixed
- 评分显示精度（97.19... → 97.2）/ Score display precision fix
- AI 设置回显（不再重置为默认智谱）/ AI settings dropdown reflects saved value
- SerpAPI 免费额度（100 → 250 次/月）/ SerpAPI free quota corrected
- BAT 启动脚本中文编码 / BAT script Chinese encoding fix
- 讨论总结错误改为友好内联提示 / Discussion summary error as inline message

---

## [v0.4.0] - 2026-03-30

### 重大变更 / Breaking Changes
- **Pipeline 统一** -- api_idea_analyze 改为 pipeline 快捷模式，删除 220 行重复代码
  Pipeline unification -- api_idea_analyze uses quick_mode wrapper, removed 220 lines of duplicate code
- **仓库架构重组** -- 主仓瘦身为纯 Rust 核心，iOS/鸿蒙/Tauri 拆至独立仓库
  Repository restructuring -- core repo is pure Rust, iOS/HarmonyOS/Tauri split to independent repos

### 新增 / Added
- PipelineRunner 支持 quick_mode，跳过非必要步骤加速验证
  PipelineRunner supports quick_mode, skipping non-essential steps
- 前端「快速验证」（6 步）/ 「深度验证」（12 步）切换
  Frontend quick validation (6 steps) / deep validation (12 steps) toggle
- Android 固定签名证书，后续更新可直接覆盖安装
  Android fixed signing certificate for seamless updates
- CHANGELOG.md + CONTRIBUTING.md / Changelog and contributing guide

### 改进 / Improved
- cargo fmt 全量格式化 + cargo clippy 零警告 / Full formatting + zero clippy warnings
- .gitattributes 优化 GitHub 语言统计（Rust 占比 90%+）/ GitHub language stats optimized
- 清理冗余文件（Node.js 依赖、过时 CI、构建产物）/ Cleanup redundant files

---

## [v0.3.5] - 2026-03-29

### 新增 / Added
- 搜狗搜索内置免费方案，零配置开箱即用（国内无需 VPN 无需 Key）
  Built-in Sogou search, zero-config for China (no VPN, no API key needed)
- Bing Web Search API 支持（国内可用）/ Bing Web Search API support
- Lens.org 专利搜索 API 支持 / Lens.org patent search API support
- 搜索降级链：SerpAPI → Google Patents → Bing → Lens.org → 搜狗 → 本地 DB
  Search degradation chain: SerpAPI → Google Patents → Bing → Lens.org → Sogou → Local DB
- 设置页面新增「国内搜索配置」区域 / Settings page: domestic search configuration section
- 使用免费搜索时自动提示升级 / Auto-prompt to upgrade when using free search

---

## [v0.3.4] - 2026-03-29

### 修复 / Fixed
- APP 端创意分析 AI 失败时降级评分 / Fallback scoring when AI analysis fails on APP
- Tauri 前端浏览器测试路由 / Tauri frontend browser test routing
- 文档上传支持 .docx + GBK 编码处理 / Document upload: .docx support + GBK encoding
- AI 错误提示改善 / Improved AI error messages
- Pipeline SSE 时序修复 / Pipeline SSE timing fix
- 空 query 搜索校验 + 收藏专利 ID 验证 / Empty query validation + favorite patent ID check
- 收藏标签前端 UI 优化 / Favorites tag UI polish

---

## [v0.3.3] - 2026-03-27

### 新增 / Added
- 12 步创新验证流水线（ParseInput → ScoreNovelty → AI 分析 → 报告生成）
  12-step innovation validation pipeline
- 设置持久化（SQLite 存储，重启不丢失）/ Settings persistence in SQLite
- 鸿蒙 APP 构建配置 / HarmonyOS APP build configuration
- 多平台 APP 支持框架 / Multi-platform APP framework
- 全面中文化 / Full Chinese localization

### 修复 / Fixed
- 测试断言修复 / Test assertion fixes
- 引用准确性提升 / Citation accuracy improvements
- i18n 补全 / i18n completeness
- Clippy 错误修复 / Clippy error fixes

---

## [v0.3.2] - 2026-03-26

### 新增 / Added
- 设置页面改造：多 AI 预设 + 注册引导 + 自定义支持
  Settings page redesign: multi-AI presets + registration guide + custom support
- 纯 Rust Android APP 方案（无 Java 依赖）
  Pure Rust Android APP (no Java dependency)

### 修复 / Fixed
- 设置保存优化（先更新内存，.env 写入可选）/ Settings save optimization
- AI 未配置时友好中文提示 / Friendly Chinese prompt when AI not configured
- Android APK cdylib 共享库替代可执行文件 / Android APK: cdylib shared library
- wry 0.46 API 变更适配 / wry 0.46 API changes adaptation

---

## [v0.3.1] - 2026-03-26

### 新增 / Added
- Android APP 支持（ARM64 + x86_64 双架构）
  Android APP support (ARM64 + x86_64 dual architecture)
- Dioxus Mobile 移动端方案 / Dioxus Mobile solution
- 纯 Java WebView 方案（最终采用）/ Pure Java WebView approach (final choice)

### 修复 / Fixed
- APK 签名路径和上传条件 / APK signing path and upload conditions
- Android APP 闪退和图标问题 / Android APP crash and icon issues
- 静态文件内嵌二进制（Android 兼容）/ Static files embedded in binary
- Android 9+ 允许 localhost 明文 HTTP / Android 9+ cleartext HTTP for localhost
- CI 构建流程优化 / CI build workflow optimization

---

## [v0.3.0] - 2026-03-25

### 新增 / Added
- IPC 分类浏览 API / IPC classification browsing API
- 混合相关性评分算法（TF-IDF + 位置加权）/ Hybrid relevance scoring (TF-IDF + position weighting)
- Chart.js 可视化统计图表 / Chart.js visualization and statistics
- 对比矩阵 + 侵权风险评估 UI / Comparison matrix + infringement risk assessment UI
- 权利要求分析 + 批量摘要 / Claims analysis + batch summarize
- PWA 支持（可安装为桌面/移动应用）/ PWA support (installable as app)
- MCP Server（AI Agent 集成）/ MCP Server for AI Agent integration

---

## [v0.2.0] - 2026-03-24

### 新增 / Added
- AI 多服务商自动容灾切换（智谱 GLM、OpenRouter、Gemini、OpenAI、NVIDIA、DeepSeek）
  AI multi-provider automatic failover (6 providers)
- 专利技术附图查看 + 本地图片代理 / Patent drawings viewer + local image proxy
- PDF 导出（含附图）/ PDF export with drawings
- 中英双语国际化（i18n）/ Bilingual internationalization
- 搜索结果智能去重 / Smart search result deduplication

---

## [v0.1.0] - 2026-02-24

### 新增 / Added
- 在线专利搜索（SerpAPI + Google Patents）/ Online patent search
- 本地 SQLite 数据库 + FTS5 全文搜索 / Local SQLite + FTS5 full-text search
- AI 智能分析（OpenAI 兼容 API）/ AI analysis (OpenAI compatible API)
- 专利对比分析 / Patent comparison analysis
- 相似专利推荐 / Similar patent recommendations
- 文件上传对比 / File upload comparison
- 搜索历史管理 / Search history management
- 统计图表展示 / Statistics charts
- Excel 数据导出 / Excel data export
- 跨平台支持（Windows/Linux/macOS）/ Cross-platform support
- 设置页面（网页配置 API Key）/ Settings page (web-based API key configuration)
