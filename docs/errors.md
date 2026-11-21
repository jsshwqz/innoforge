# InnoForge 高频跨模块避坑活索引（Top-15 Active Anti-Pitfall Index）

> **三层防再犯机制（`AGENTS.md` §四）**：
> 1. **第一层（机器拦截，首选）**：凡能用测试或静态守卫拦住的坑，必须转化为 `tests/` 负面单测或 CI 守卫；
> 2. **第二层（就近撞脸）**：模块专属致命禁区直接写入目标 `.rs` 文件顶部 3 行 `//!` 契约注释；
> 3. **第三层（本文件极简活索引）**：本文件永远只保留 **Top-15 高频跨模块避坑索引**（供 `src/context.rs` 自动注入与开工速查）；历史 #1~#79 完整流水账已归档至 [`docs/archive/errors-history-1-79.md`](archive/errors-history-1-79.md)。

---

## Top-15 跨模块高频避坑铁律

### 1. [2026-11-21] PDF 专利附图假提取陷阱（`c0d8fdd` / PR #243）：严禁整本 PDF 全页转图或套 iframe 冒充附图
- **现象与根因**：部分中国专利在 Google Patents HTML 中不提供预切片 PNG，仅提供官方 `pdf_url`。旧代码一方面把 `!pdf_url.is_empty()` 误判为已有附图跳过提取，另一方面在 `patent_detail.html` 用 `<iframe src="...pdf">` 顶替图纸页签，或将整本 PDF 封面/权要/文字页全转成图片充数。
- **防再犯铁律**：① 专利附图统一由 `src/pdf_drawings.rs` 从 PDF 内部真实提取嵌入图像对象（Image XObjects，按尺寸/面积/长宽比过滤条形码与小印章，并严格跳过封面、`权利要求书` 及纯文字 `说明书` 页）；扫描件仅解码并提取末尾 `说明书附图` 专属页附图区；② 「查看原版 PDF」只能作为独立辅助按钮；③ 由 `pdf_drawings::tests::anti_cheat_pdf_drawing_extractor_excludes_text_pages_and_stamps` 反作弊单测强制守卫。

### 2. [2026-11-21] OCR 通道“两张皮”陷阱（`src/routes/upload.rs` vs `src/ai/extraction.rs`）
- **现象与根因**：`upload.rs` 已接入可配置的 `Umi-OCR`（`UMI_OCR_BASE_URL`），但 `src/ai/extraction.rs` 仍另起炉灶硬编码调 `pdftoppm + tesseract`，导致无 poppler/tesseract 的 Windows 环境扫描件提取直接失败。
- **防再犯铁律**：全仓任何 PDF/图片 OCR 回退必须统一优先调用 `crate::routes::extract_pdf_text_umi_ocr_with_base_url`（`UMI_OCR_BASE_URL`，默认 `http://127.0.0.1:1224`），仅在 `Umi-OCR` 未运行或不可达时回退 `pdftoppm + tesseract`（由 `test_extract_ocr_prioritizes_umi_ocr_when_available` 守卫）。

### 3. [2026-11-21] 证据门禁“只打 `tracing::warn!` 不阻断”假闭环陷阱（`src/pipeline/steps/finalize.rs`）
- **现象与根因**：`fact_check_rejected` 或 `should_suppress_decision(&kernel_report)` 命中时，旧代码仅打印 `warn!` 日志，未清空 `ctx.decision` 也未在报告顶部注入阻断声明。
- **防再犯铁律**：任何核验/风控/超范围修改门禁命中时，必须产生实质状态阻断或硬降级（清空 `ctx.decision = None`、抑制申请建议并置顶 `【证据与事实核验门禁拦截声明】`，由 `finalize_blocks_decision_and_injects_banner_when_fact_check_rejected` 守卫）。

### 4. [2026-11-21] 核心业务文本硬编码裸截断陷阱（`src/ai/patent.rs` / `src/routes/ai.rs` / `upload.rs`）
- **现象与根因**：对送入 AI 的权利要求书或对比专利直接调 `truncate_for_ai(..., 2000/4000)` 或 `.slice(0, N)`，导致从属权利要求与末尾核心实施例静默丢失。
- **防再犯铁律**：凡涉及专利文本送入 AI，一律走 `crate::ai::patent::structured_patent_excerpt`（100% 保留全部权利要求 + 说明书按相关性选段并附带显式数据完整性提示），严禁对权利要求做硬字符截断。

### 5. [2026-11-07] 审查基准权利要求（`current_claims`）半接线陷阱（#72）
- **现象与根因**：复审案件中 `current_claims` 只接了修改超范围核验门，未接入 Stage A 生成管线，导致复审前新增的核心区别特征被当作非原始申请 verbatim 静默丢弃。
- **防再犯铁律**：新字段接入必须枚举全链路所有消费点；优先使用 `resolve_claims_under_review`（优先级：`CaseInput` 自带 > 请求 `current_claims` > `my_patent` 兜底）；特征被丢弃时必须在产物内显式提示，严禁静默丢弃。

### 6. [2026-11-06] 驳回决定首页勾选表法条误收陷阱（#71）
- **现象与根因**：驳回决定首页印有全部候选法条勾选表，简单正则会把未勾选的 15+ 条法条全部当成审查依据。
- **防再犯铁律**：提取法律依据必须区分「模板泛指（无具体权利要求编号）」与「实际特指认定（点名权利要求 X-Y 不具备……）」，由 `legal_basis_ignores_rejection_form_checklist` 双向负控守卫。

### 7. [2026-11-05] 前端控件与后端在线路径“假接线”陷阱（#69 / #70）
- **现象与根因**：前端传了 `sort_by` / `page_size` 或选择了 `custom` 模型网关，但后端在线检索分支或单源探测分支写死忽略/硬拦截。
- **防再犯铁律**：每接一个前端控件或配置项，必须配一条「改变该入参 → 出参/上游请求真实改变」的语义测试，禁止只测静态路由形状。

### 8. [2026-11-05] 模板内联 JS 暂时性死区（TDZ）与函数丢失陷阱（#67）
- **现象与根因**：在 `const/let` 声明前引用变量，或重构 HTML 时漏掉 `on*` 绑定函数，`node --check` 静态语法检查无法发现运行时 TDZ。
- **防再犯铁律**：改 `templates/*.html` 或 `static/*.js` 必跑 `node scripts/check_html_functions.mjs` 与 `npx eslint static/i18n.js`，并在真实浏览器/隔离端口走通渲染路径。

### 9. [2026-11-04] 中文无空格长查询相关性门禁误杀陷阱（#66）
- **现象与根因**：中文长句无空格，`split_whitespace()` 词级匹配得 0 分，且「一种/包括/装置」等模板词稀释 bigram 分母导致在线检索结果被门禁清零。
- **防再犯铁律**：中文查询必须经 `extract_query_words` 剥离专利套话停用词并抽取 2~4 字核心技术词；放宽召回判据时必须同步保留 0 命中无关噪声被拦截的负面单测。

### 10. [2026-11-08] 外联/探活 HTTP 客户端无 `connect_timeout` 悬挂陷阱（#55 / #74 / #76）
- **现象与根因**：裸 `reqwest::Client::new()` 或未设 `connect_timeout` 的客户端在死端口或跨国代理异常时悬挂数分钟；异步路由内调 `reqwest::blocking` 会直接触发 Tokio panic 或卡死工作线程。
- **防再犯铁律**：全仓 HTTP 客户端必须显式设置 `.connect_timeout(...)` 与 `.timeout(...)`（由 `tests/no_silent_client_fallback.rs` 强制静态扫描），异步上下文严禁调用 `reqwest::blocking`；检索链内嵌 LLM 编排步必须包裹独立短超时（≤ 20s）。

### 11. [2026-11-15] 测试端口裸 `bind(":0")` 竞态与 Windows 保留端口段漂移陷阱（#75 / #78）
- **现象与根因**：Windows Hyper-V/WSL 动态保留端口段成块漂移（100 端口一段），硬编码端口必撞 `WSAEACCES (10013)`；测试桩若绕过 `port_alloc` 记账分配器直接 `bind(":0")`，会被并行用例复用到刚释放的端口引发偶发红。
- **防再犯铁律**：测试内一切服务与 Mock 桩统一走 `tests/common::spawn_test_server_on_free_port` 与 `bind_stub_on_free_port`（带 +2000 探测窗口与互斥记账）。

### 12. [2026-11-15] 宿主机全局环境变量与 `.env` 污染测试隔离陷阱（#79）
- **现象与根因**：宿主机全局 `AI_API_KEY` / `AI_BASE_URL` 或仓库根目录 `.env` 泄漏进测试进程，把“全新未配置环境”伪装成“已配置”，或测试写 `.env` 污染工作区。
- **防再犯铁律**：测试子进程必须 `env_clear` + 独立临时 `INNOFORGE_HOME` / `INNOFORGE_ENV_PATH`；进程内读写环境变量的测试必须加全局 `Mutex` 串行执行。

### 13. [2026-11-09] SQLite 日期统计跨午夜 UTC × 本地时区错位陷阱（#77）
- **现象与根因**：时间戳以 UTC RFC3339 写入，查询用 `date('now', 'localtime')` 对比未换算列，导致每天 00:00~08:00 统计窗口错位 8 小时。
- **防再犯铁律**：凡「今日/本月」SQL 统计，列侧与当前时间侧必须同为本地时区：`date(timestamp, 'localtime') = date('now', 'localtime')`。

### 14. [2026-07-13] UTF-8 多字节字符裸字节切片 `&s[..n]` 崩溃陷阱（#73）
- **现象与根因**：对含中文的 `&str` 直接写 `&s[..n]` 或 `&s[mid..]`，当字节下标落在 3 字节汉字内部时直接 `panic`。
- **防再犯铁律**：全仓严禁裸字节切片，统一用 `safe_truncate` / `safe_truncate_chars` / `.chars().take(N)`（由 `tests/no_unsafe_byte_slice.rs` 静态守卫强制拦截）。

### 15. [2026-07-13] 图片代理 SSRF 与外部 API Key 报错泄漏陷阱
- **现象与根因**：图片代理仅用字符串前缀判断域名或跟随重定向存在 SSRF 风险；上游请求报错时 `reqwest::Error` 默认携带完整 URL（含 `?api_key=...`）导致密钥明文回显。
- **防再犯铁律**：外部图片代理必须经 `validate_patent_image_url` 校验 HTTPS + 精确白名单主机 + 禁凭据 + `redirect::Policy::none()`；任何含 Key 的上游 URL 报错在入日志或返回前端前必须 `.replace(&api_key, "***")` 脱敏。
