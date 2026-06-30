# OA 答复功能加强规划

> **版本**：v0.7.2+ 专项规划（不绑定发版节奏）
> **日期**：2026-06-30
> **状态**：规划中 / 待评审
> **关联文件**：
> - 前端：`templates/office_action_response.html`（约 2197 行，单文件 SPA）
> - 后端：`src/db/oa.rs`、`src/routes/ai.rs`、`src/routes/upload.rs`、`src/pipeline/steps/oa_response.rs`
> - 总览：`docs/plans/STATUS.md`（当前 v0.7.1，实际已 v0.7.2）

---

## 一、背景与目标

用户反馈"OA 页面功能太弱"。经逐行通读前端 `office_action_response.html` 与数据层 `db/oa.rs` 后发现：**该页面实际已相当成熟**（输入/分析/讨论/答复/修改/矩阵/论点/期限/历史九大模块俱全），所谓"弱"更多源于若干**产出物可用性**与**纵深能力**的硬伤，而非功能缺失。

本规划目标：
1. 把答复产出物从"能看"升级到"能直接提交"（docx + 国知局模板）；
2. 把分析从"一坨文本整体喂 AI"升级到"结构化拆条 + 逐条分析"；
3. 把持久化从"只存分析文本"升级到"全工作流断点续作"；
4. 把历史从"扁平列表"升级到"案件维度时间线"；
5. 补齐扫描件 OCR、段落级编辑、修改超范围检测等纵深短板。

**非目标**：不重写页面，不引入新的前端框架（继续 vanilla JS 单文件）。

---

## 二、现状盘点（基于真实代码，非推测）

### 2.1 已实现能力（15 类）

| 模块 | 关键函数 / 路由 | 能力 | 成熟度 |
|---|---|---|---|
| 文件上传解析 | `extractFile` → `/api/upload/extract` | PDF/DOCX/TXT 提取文本 | ★★★ |
| PDF 原件预览 | `/api/upload/pdf-store` + iframe | 存 PDF 并可预览原件 | ★★ |
| 本地专利库查找 | `/api/patent/lookup/{number}` | 按申请号/公开号查库 | ★★★ |
| 在线抓取 | `/api/patent/lookup-or-fetch` | 在线补抓（含额度提示） | ★★ |
| 多份对比文件 | 动态 `ref-row` | 动态增删对比文件 | ★★★ |
| 自动识别专利号/发文日 | `autoDetectPatentFromOA` | 正则匹配 CN/ZL + 发文日 | ★★ |
| OA 分析 | `doOAAnalysis` → `/api/ai/office-action-response/stream`（SSE）+ `/api/ai/office-action-response`（回退） | 5 部分分段流式输出 | ★★★ |
| 5 段解析渲染 | `parseOASections` / `showOAAnalysisSections` | 概述/法条/特征对比/分层反驳/意见陈述书 | ★★★ |
| 讨论模式 | `showOAAnalysisDiscuss` + `sendOADiscussion` → `/api/ai/oa-discuss`（SSE） | section1-4 + section5 占位 + 讨论区 + 生成按钮 | ★★ |
| 通用讨论 | `sendDiscussionMessage` → `/api/ai/chat`（非流式） | 带代理师指令、避开角色扮演审查、AbortController 中断、引用选中文本 | ★★ |
| 答复书生成 | `generateResponseLetter` → `/api/ai/oa-generate-response-letter`（SSE） | 流式生成 section5 | ★★ |
| 权利要求修改 | `claims-editor` + `checkAmendments` → `/api/ai/check-amendments` | AI 合规检查 | ★★ |
| 权利要求 diff | `computeLineDiff`(LCS) + `showClaimDiff` | +/- 行高亮 | ★★ |
| 特征映射矩阵 | `extractMappingMatrix` | 正则事后提取 权项/理由/法条/对比文件 → 表格 | ★ |
| 对比文件锚点 | `numberRefParagraphs`([§n]) + `enableAnchorJumps` + `scrollToRef` | D1/D2 可点击跳转 | ★★ |
| 论据模板库 | `buildArgLibrary` | A22.2/A22.3/A26.4/A33 三类静态模板，点击复制 | ★ |
| 论点板 | `argList` + `parseArgsFromAI` + `renderArgBoard` | confirmed/pending/risk 三态、AI 自动提取、采纳/拒绝 | ★★ |
| 论点提取 | `extractArgConclusions` / `exportDiscussionConclusions` → `/api/ai/chat/conclusions` | 讨论结论提炼 | ★★ |
| 期限倒计时 | `initDeadline` / `updateDeadline` | 4 月/复审 3 月、剩余天数、逾期红/临期黄、进度条、localStorage 持久化 | ★★★ |
| 历史持久化 | `loadOaHistory` → `/api/oa/history/all` + `/api/oa/history/detail/{id}` | 侧边栏历史列表、点击恢复分析 | ★★ |
| 状态持久化 | `localStorage('innoforge_oa_data')` + Proxy 自动保存 | 上传内容/输入框刷新不丢 | ★★ |
| Checklist | `showChecklist` | 6 项勾选 + 进度 | ★ |
| XSS 防护 | DOMPurify | 渲染前消毒 | ★★★ |
| i18n | `t()` | 多语言 | ★★★ |

### 2.2 数据层现状（`db/oa.rs`）

`oa_analyses` 表字段：`id, patent_number, patent_title, oa_type, depth, analysis_text, created_at, version`

- ✅ **版本管理数据层已具备**：`save_oa_analysis` 自动算 `next_version = MAX(version)+1 WHERE patent_number`，`list_oa_analyses(patent_number)` 按版本倒序返回同专利所有记录。
- ❌ **持久化范围窄**：仅存 `analysis_text`。讨论记录、答复书全文、修改后权利要求、checkAmendments 结果、论点板、对比文件内容**均不持久化**，刷新/换机即丢。
- ❌ **无独立案件表**：`patent_number` 是事实上的案件键，但无案件元信息（申请人、代理机构、当前 OA 轮次、答复期限、案件状态）。

---

## 三、真正的薄弱点（12 条，基于代码）

| # | 薄弱点 | 依据 | 影响 |
|---|---|---|---|
| W1 | **答复书仅导出 .txt** | `exportGeneratedResponse`/`exportResponseDraft` 用 `text/plain`，剥 markdown 后纯文本，只补"申请人/日期"落款 | 产出物无法直接提交国知局，必须人工重排 |
| W2 | **无 OCR 扫描件支持**（待确认） | `/api/upload/extract` 走文本提取；后端虽有 PDF 6 级降级含 MinerU OCR（见 STATUS.md），但 upload.rs 是否接入未确认 | 盖章扫描件审查意见提取不出文字 |
| W3 | **分析输入是一坨文本** | `doOAAnalysis` 把 OA 全文 + 专利全文整体喂 AI；`extractMappingMatrix` 是**事后**从 AI 自由文本正则抓取 | 分析深度不足、矩阵漏抓、AI 表述不标准即失效 |
| W4 | **历史扁平、无案件时间线** | 前端 `loadOaHistory` 调 `list_all_oa_analyses`（全量时间倒序），未用已有的 `list_oa_analyses(patent_number)` 分组 | 看不到同一专利一审→二审→复审的答复演进 |
| W5 | **持久化范围窄** | 表只存 `analysis_text` | 刷新丢讨论/答复/修改稿/论点，无法真正断点续作 |
| W6 | **两套讨论逻辑并存割裂** | `sendOADiscussion`(oa-discuss SSE) vs `sendDiscussionMessage`(chat 非流式)，两套消息容器/历史/输入框 | 技术债、易混乱、论点提取只挂其中一套 |
| W7 | **答复书整体生成、无段落级编辑** | `generateResponseLetter` 一次流式全文，生成后只能导出 | 某段不满意只能整篇重跑 |
| W8 | **权利要求无逐条超范围检测** | `checkAmendments` 整体调一次 AI；`showClaimDiff` 仅行级文本 diff，不区分修改方式、不联动从权 | 修改超范围（A33）是答复失败高频原因，未精细防护 |
| W9 | **论据库是静态模板** | `buildArgLibrary` 写死字符串，不按本案审查意见动态生成、不沉淀历史成功答复 | 复用价值低 |
| W10 | **论点/矩阵提取靠 emoji 正则** | `parseArgsFromAI` 靠 ✅⚠️📌 + "："；`extractMappingMatrix` 靠"权利要求 X/新颖性/A22.2"正则 | AI 输出格式微变即失效 |
| W11 | **期限是单值** | 一个 `oa-date` 对应一次，多轮 OA 的多个期限无管理，历史也不关联期限 | 多次审查意见期限混乱 |
| W12 | **无协作/状态流转** | 纯单人工具 | 无代理人→复核→客户确认流 |

---

## 四、设计原则

1. **不重写，渐进增强**：在现有单文件 vanilla JS 上扩展，保持风格一致。
2. **结构化优先于自由文本**：凡是 AI 产出的关键数据（审查意见条目、特征矩阵、论点），改用结构化 JSON 输出，前端按 schema 渲染，告别正则事后抓取。
3. **产出物可用性优先**：docx + 国知局模板是 P0，决定工具能否真正交付。
4. **数据层先行**：持久化范围扩展是其他增强（断点续作、时间线、段落编辑）的地基。
5. **复用已有能力**：版本管理数据层、PDF 6 级 OCR 降级、MinerU、DOMPurify 等已具备，优先接入而非另造。

---

## 五、分阶段加强计划

### P0 — 产出物可用性 + 全流程持久化（最高优先）

#### P0-1 答复书 Word 导出 + 国知局标准模板
- **目标**：导出可直接提交的 `.docx`，带国知局意见陈述书标准格式。
- **现状**：W1，仅 `.txt`。
- **方案**：
  - 后端新增 `/api/oa/export-docx`（POST，入参 `response_text, patent_number, applicant, oa_type`），用 `docx-rs` 或 `headless` 生成；
  - 内置标准模板：标题"意见陈述书"居中 → 申请人/代理机构/申请号栏 → 正文（AI 内容按"一、二、三"分级编号）→ 落款（申请人签字/日期）；
  - 前端 `exportGeneratedResponse` / `exportResponseDraft` 改为调该接口下载 `.docx`，保留 `.txt` 作为备选。
- **改动点**：`src/routes/ai.rs`（新路由）、新增 `src/oa/export.rs` 或复用现有 docx 工具、前端两处导出函数。
- **验收**：生成的 docx 用 Word 打开样式正确、段落编号合规、可二次编辑；`.txt` 仍可用。

#### P0-2 持久化范围扩展（全工作流断点续作）
- **目标**：讨论、答复书、修改稿、论点板、对比文件、checkAmendments 结果均可存可恢复。
- **现状**：W5，仅存 `analysis_text`。
- **方案**：
  - `oa_analyses` 表新增列（迁移）：`discussion_json TEXT`、`response_letter TEXT`、`amended_claims TEXT`、`amendment_check TEXT`、`arg_board_json TEXT`、`references_json TEXT`、`oa_date TEXT`、`case_status TEXT`；
  - `db/oa.rs` 增 `save_oa_full_snapshot(id_or_patent, payload)` / `get_oa_full_snapshot(id)`；
  - 前端在关键节点（分析完成、讨论每轮、生成答复、checkAmendments、采纳论点）调 `save_oa_full_snapshot`；`loadOaAnalysis` 恢复全部状态而非仅文本。
- **改动点**：`src/db/oa.rs`（迁移 + CRUD）、`src/routes/ai.rs`（快照路由）、前端 `loadOaAnalysis` + 各节点埋点。
- **验收**：分析→讨论→生成答复→修改稿后刷新页面，全部状态恢复；换机登录同账号可续作。

#### P0-3 统一讨论逻辑（消除两套并存）
- **目标**：合并 `sendOADiscussion` 与 `sendDiscussionMessage` 为单一 SSE 流式讨论通道。
- **现状**：W6。
- **方案**：
  - 统一走 `/api/ai/oa-discuss`（SSE），废弃 `sendDiscussionMessage`→`/api/ai/chat` 的非流式分支；
  - 统一消息容器为 `#oa-discuss-messages`、历史为 `oaDiscussMessages`、输入框为 `#oa-discuss-input`；
  - 保留"代理师指令放 user message 避开角色扮演审查"的策略；
  - 论点提取 `extractArgConclusions` 挂到统一通道。
- **改动点**：前端 `sendDiscussionMessage` 删除/合并、`startDiscussion` 适配、`extractArgConclusions` 切到统一历史。
- **验收**：讨论全程流式、可中断、论点可提取；无两套 DOM 残留。

---

### P1 — 结构化分析 + 案件维度 + 修改纵深

#### P1-1 审查意见结构化拆条
- **目标**：分析前先把 OA 拆成结构化条目（每条：权项/法条/对比文件/引用特征/结论类型），再逐条分析。
- **现状**：W3，整体喂 AI + 事后正则。
- **方案**：
  - 新增后端 `/api/ai/oa-parse-issues`（SSE 或 JSON），输入 OA 文本，输出 `[{claim, article, ref_doc, cited_features, issue_type, conclusion}]` JSON；
  - 前端先调拆条 → 渲染条目卡片列表（可勾选/编辑）→ 再按勾选条目驱动 `doOAAnalysis` 逐条或批分析；
  - `extractMappingMatrix` 改为直接消费该 JSON 渲染表格（W10 一并解决）。
- **改动点**：`src/routes/ai.rs`（新路由）、`src/pipeline/steps/oa_response.rs`（拆条步骤）、前端拆条 UI + 矩阵改造。
- **验收**：矩阵表格条目与拆条 JSON 一一对应，AI 表述变化不影响渲染；条目可手动修正后重分析。

#### P1-2 案件维度时间线
- **目标**：以案件（专利）为维度，展示一审→二审→复审的多轮 OA 答复演进。
- **现状**：W4，前端调全量 `list_all_oa_analyses`，未用已有 `list_oa_analyses(patent_number)`。
- **方案**：
  - 前端 `loadOaHistory` 改为按 `patent_number` 分组：侧边栏先列案件，点案件展开其下各版本（v1 一审/v2 二审…）时间线；
  - 每个版本节点显示：oa_type 标签、created_at、期限状态、case_status；
  - 新增案件列表路由 `/api/oa/cases`（按 patent_number 聚合，取最新版本 + 案件元信息）。
- **改动点**：`src/db/oa.rs`（`list_cases` 聚合查询）、`src/routes/ai.rs`、前端 `renderOaHistory` 重构。
- **验收**：同专利多次 OA 聚合到同一案件下，时间线清晰；点击版本恢复对应全量快照（依赖 P0-2）。

#### P1-3 权利要求逐条超范围检测 + 修改方式标注
- **目标**：逐条标记新增内容是否超范围（A33），区分删除/增加/替换，联动从权。
- **现状**：W8，整体 AI 检查 + 行级 diff。
- **方案**：
  - `showClaimDiff` 升级：按"权利要求 N"分块 diff，每块标注修改方式（删/增/替换/重排），新增片段单独高亮并附"是否超范围"待 AI 判定标记；
  - `checkAmendments` 改为逐条输入（`[{claim_no, original, amended}]`），AI 逐条返回 `{over_range: bool, reason, suggestion}`；
  - 修改独权时提示从权是否需同步。
- **改动点**：前端 `showClaimDiff`/`checkAmendments`、`/api/ai/check-amendments` 入参出参 schema 化。
- **验收**：每条权项独立显示超范围结论与理由；修改方式可标注。

#### P1-4 OCR 扫描件接入（待确认后端）
- **目标**：盖章扫描件审查意见能提取文字。
- **现状**：W2，待确认 `/api/upload/extract` 是否已接 PDF 6 级 OCR 降级。
- **方案**（二选一，取决于确认结果）：
  - 若 `upload.rs` 已接 OCR：前端仅需在提取失败/为空时提示"扫描件，尝试 OCR 重试"按钮 → 调 OCR 强制路径；
  - 若未接：`upload.rs` 的 extract 在文本层为空时回落到 `extract_pdf_text` 6 级降级（含 MinerU 云端），前端透传进度。
- **改动点**：`src/routes/upload.rs`、前端 `extractFile` 失败分支。
- **验收**：上传盖章扫描件 PDF 能提取出可读文本；进度可见。
- **⚠ 待确认**：通读 `src/routes/upload.rs` 的 extract 实现，确认 OCR 接入状态后再定子方案。

---

### P2 — 答复书段落级编辑 + 论据动态化

#### P2-1 答复书段落级重生成与编辑
- **目标**：生成后可逐段"重新生成 / 手动编辑 / 追加论述"。
- **现状**：W7，整体生成。
- **方案**：
  - 生成后按段落（一、二、三…）切分为可编辑块，每块附 [重新生成][编辑][上移/下移]；
  - "重新生成"调 `/api/ai/oa-regenerate-paragraph`（入参：段落上下文 + 该段原有 + 讨论结论），SSE 单段流式替换；
  - 支持插入"参见修改后权利要求第 X 条"等交叉引用。
- **改动点**：前端 section5 渲染改造、`src/routes/ai.rs`（段落重生成路由）、`src/pipeline/steps/oa_response.rs`。
- **验收**：任一段可单独重生成且不破坏其他段；手动编辑可保存入快照（依赖 P0-2）。

#### P2-2 论据库动态化 + 历史复用
- **目标**：论据模板按本案审查意见动态生成，并沉淀历史成功答复。
- **现状**：W9，静态字符串。
- **方案**：
  - `buildArgLibrary` 改为调 `/api/ai/oa-suggest-arguments`（入参：拆条 JSON + 本案权项特征），返回针对性论据草稿；
  - 新增"历史成功答复库"：标注某次答复最终授权的案件，其论据可被检索复用（依赖 P0-2 快照 + 案件结果字段）。
- **改动点**：前端 `buildArgLibrary`、`src/routes/ai.rs`、`db/oa.rs`（结果字段）。
- **验收**：论据草稿引用本案具体权项/特征；历史授权案件论据可检索引用。

---

### P3 — 协作与长期沉淀

#### P3-1 多人协作与状态流转
- **目标**：代理人初稿 → 律师复核 → 客户确认。
- **方案**：案件 `case_status` 字段 + 简单角色（基于现有 auth.rs）；状态流转带批注。
- **改动点**：`db/oa.rs`（status 流转）、`auth.rs`、前端案件卡状态操作。

#### P3-2 多轮 OA 期限管理
- **目标**：每个版本独立的期限，案件级期限总览。
- **方案**：期限随版本快照存（P0-2 的 `oa_date` 列），案件卡显示最近到期版本红色提醒。

#### P3-3 策略成功率统计
- **目标**：记录每条答复策略对应的最终授权/驳回，反哺推荐。
- **方案**：案件结果字段 + 按法条/策略聚合统计。

---

## 六、数据层迁移（DDL 草案）

```sql
-- oa_analyses 扩展（P0-2），向后兼容，新列允许 NULL
ALTER TABLE oa_analyses ADD COLUMN discussion_json TEXT;
ALTER TABLE oa_analyses ADD COLUMN response_letter TEXT;
ALTER TABLE oa_analyses ADD COLUMN amended_claims TEXT;
ALTER TABLE oa_analyses ADD COLUMN amendment_check TEXT;
ALTER TABLE oa_analyses ADD COLUMN arg_board_json TEXT;
ALTER TABLE oa_analyses ADD COLUMN references_json TEXT;
ALTER TABLE oa_analyses ADD COLUMN oa_date TEXT;
ALTER TABLE oa_analyses ADD COLUMN case_status TEXT DEFAULT 'draft';
ALTER TABLE oa_analyses ADD COLUMN final_result TEXT; -- P3-3: granted/rejected/pending

-- 案件聚合查询（P1-2）示例
-- SELECT patent_number, patent_title, MAX(version) latest_ver,
--        MAX(created_at) latest_at, MIN(oa_date) nearest_deadline
-- FROM oa_analyses GROUP BY patent_number;
```

---

## 七、风险与依赖

| 风险 | 影响 | 缓解 |
|---|---|---|
| 前端单文件已达 2197 行，继续膨胀难维护 | 可读性下降 | P0/P1 优先做；后续可考虑抽 JS 到独立文件（非本规划范围） |
| docx 生成依赖（docx-rs / headless）增加体积/构建复杂度 | 构建 | 先调研现有项目是否已有 docx 生成路径复用 |
| 结构化拆条依赖 AI 输出稳定 JSON | 偶发格式漂移 | 后端做强 schema 校验 + 失败重试 + 自由文本兜底 |
| OCR 走 MinerU 云端有额度/隐私考量 | 扫描件场景 | 本地 OCR 优先，云端降级；提示用户 |
| 两套讨论合并可能影响既有用户习惯 | 体验 | 保留交互形态，仅底层统一；灰度 |
| 表迁移在生产库 | 数据安全 | 全新列允许 NULL，向后兼容；先备份 |

---

## 八、里程碑（建议）

| 里程碑 | 内容 | 依赖 |
|---|---|---|
| M1 | P0-1 docx 导出 + P0-3 讨论统一 | 无外部依赖，可立即开工 |
| M2 | P0-2 全流程持久化 + 表迁移 | M1 |
| M3 | P1-1 拆条 + P1-2 案件时间线 | 依赖 M2 快照 |
| M4 | P1-3 修改超范围 + P1-4 OCR（待确认后） | 可并行 M3 |
| M5 | P2 段落编辑 + 论据动态化 | 依赖 M3 拆条 |
| M6 | P3 协作/期限/统计 | 长期 |

---

## 九、待确认事项（动工前需核实）

1. **`src/routes/upload.rs` 的 `/api/upload/extract` 是否已接入 PDF 6 级 OCR 降级（含 MinerU）**？——决定 P1-4 子方案。⚠ **本轮因 Grep/命令行在本项目环境失效未能确认，需通读 `upload.rs`。**
2. **后端 OA 路由注册位置**：`/api/ai/office-action-response(/stream)`、`/api/ai/oa-discuss`、`/api/ai/oa-generate-response-letter`、`/api/ai/check-amendments`、`/api/oa/history/*` 分别在哪个文件注册？——`src/routes/ai.rs`？还是 `mod.rs`/`main.rs`？⚠ 本轮 Grep 在 `src` 全局无匹配（疑似工具在该项目失效），需直接通读确认。
3. **项目是否已有 docx 生成路径**可复用？——影响 P0-1 选型。
4. **STATUS.md 落后一版**（记 v0.7.1，实际 v0.7.2）——动工前同步。

---

## 十、与已有规划的关系

- 本规划与 `docs/plans/pdf-extraction-enhancement.md` 在 OCR 侧有交集（P1-4），可复用其 PDF 6 级降级链成果。
- 本规划不冲突 `2026-06-22-v0.6.3-plan.md` 的 OA 一审结构化分析（已完成），是在其 5 段结构基础上的纵深加强。
