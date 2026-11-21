# 研发创新与专利智能体（InnoForge `< 1.0`）6 版实战演进规划（v0.9.64 ～ v0.9.69）

> **规划定性与版本红线**：
> 1. **严格锚定 `< 1.0`（`0.9.x` `main` 主线）原生规划框架**：本规划完全处于 `< 1.0` 原生规划体系内（版本号 **`v0.9.64` ～ `v0.9.69`**），严格以 `main` 分支 `docs/plans/product-vision.md`、`docs/plans/2026-10-24-direction-realignment.md`、`docs/plans/archive/2026-09-22-gap-analysis-improvement-plan.md`、`docs/plans/archive/milestones.md`、`docs/plans/archive/task-breakdown.md` 与 `docs/plans/archive/研发助手升级规划-多方讨论.md` 为依据，**绝不越界到 `1.0` 重构线**。
> 2. **务实减法与反过度工程化原则**：
>    - **已剔除 `v0.9.20 ~ v0.9.63` 已完工项**：`patent_chunks` + `chunks_fts` + RRF 混合检索、OpenAlex、Agentic Search P1~P3、6 个巨石文件拆分、`amendment_gate` 接入 `fact_check`、Batch 4~16 专项工具等均已落地，绝不重复充数。
>    - **已砍掉/暂缓 5 项非刚需重型工程**：① 纯 Rust 硬编码 5 类物理公式算子；② 后端 `services/` 层大搬家重构；③ 前端大拆迁 + P50/P95/P99 云端式性能监控面板；④ 自造自动化定量跑分台（验收以真实案件与实测为准）；⑤ 9 家服务商硬编码单价表（精简为全链路 Token 记账 + 单日/单创意调用次数与 Token 硬熔断，并入 `v0.9.65`）。
>    - **核心业务能力完整保留**：完整保留 **`PubMed` 学术检索源**、**`CRAAP` 五维来源可信度评分**（`v0.9.67`）以及 **四角色模型路由收口**（`v0.9.69`），将原 12 版浓缩为 **6 版刀刀见血的实战序列（`v0.9.64` ～ `v0.9.69`）**。

---

## §〇 6 版实战队列总览（`v0.9.64` ～ `v0.9.69`）

| 目标版本 | `< 1.0` 原规划依据 | `v0.9.63` 源码真实痛点与行号证据（`file:line`） | 核心交付目标 |
| :--- | :--- | :--- | :--- |
| **`v0.9.64`** | `2026-09-22` §2.2/§2.4 + `2026-10-24` §1.1 + `c0d8fdd`/`198190b` 回归修复 + `:3001` 实测修复 | ① **专利 PDF 附图假提取（`c0d8fdd` / PR #243）**：当 Google Patents 仅提供 `pdf_url` 时，未从 PDF 提取真实单张附图，而是用整本 PDF iframe 顶替图纸页签；<br>② **OCR 通道两张皮（`src/routes/upload.rs` vs `src/ai/extraction.rs:240-276`）**：`extraction.rs` 硬编码调 `pdftoppm + tesseract`，未优先复用已配置的 `Umi-OCR`（`UMI_OCR_BASE_URL`）；<br>③ **`src/pipeline/steps/finalize.rs:29-31, 43-49` 门禁假闭环**：`fact_check_rejected` 与 `!allow_decision` 命中时仅打 `tracing::warn!`，未真正拦截或降级决策输出；<br>④ **剩余 6 处裸截断**：`src/ai/patent.rs:146,171,216,217`、`src/routes/ai.rs:584,655`、`src/routes/upload.rs:307` 仍用裸 `truncate_for_ai` 硬截断；<br>⑤ 合入 `:3001` 实测 9 项有效修复 + `AGENTS.md` §2.8 与 `docs/errors.md`、`docs/feedback.md` 复盘落盘。 | **文档真实附图提取 + Umi-OCR 统一接入 + `finalize.rs` 证据门禁真阻断 + 剩余 6 处裸截断清零** |
| **`v0.9.65`** | `2026-09-22` §4.2/§4.3/§4.6 + `task-breakdown.md` §44 `MB0b` | ① `src/routes/search.rs:1310` 的 `/api/search/vector` 在前端模板中引用数为 0（前后端断链）；<br>② `src/vector/mod.rs` 缺语料 DF/IDF 权重表与存量专利可断点后台回填器（`MB0b`）；<br>③ 流式接口（`api_ai_chat_stream` / `api_idea_chat_stream`）与 Pipeline 部分步骤漏记 Token，且缺单日/单创意调用次数与 Token 硬熔断保护。 | **`MB0b` 向量检索前端闭环（`/api/search/vector`）+ 存量断点回填 + 精简版全链路 Token 记账与预算硬熔断** |
| **`v0.9.66`** | `task-breakdown.md` `T2.5` + `2026-09-22` §3.3/§3.4 + `研发助手升级规划.md` `v0.7.0` V-RAG | ① 大扫描件 PDF OCR 仍在单请求内同步等待易超时，缺基于 `Umi-OCR` 优先的异步 SSE 逐页进度流与 4 类文件错误码（加密/损坏/无OCR引擎/超大）；<br>② `patent_detail.html` 与 `office_action_response.html` 缺 `v0.7.0` V-RAG 正文标号 ↔ 附图部件双向点击高亮联动及交底书“同号异名/有号无图”一致性核查。 | **`T2.5` 异步 SSE OCR 管道（优先对接 Umi-OCR + 4 类错误码）+ V-RAG 附图标号双向高亮 & 交底书标号一致性核查** |
| **`v0.9.67`** | `研发助手升级规划.md` §五 `v0.6.0` & §六 | ① `src/search/agent.rs` 缺 `v0.6.0` 核心的 **IPC 子空间自动锚定（锁定 Top-3 IPC 主分类过滤跨领域同词噪声）**；<br>② `src/search/providers/` 学术源缺 **`Semantic Scholar`（含高引影响力指标）与 `PubMed`**；<br>③ 检索结果缺 **CRAAP 五维来源可信度评分、多源三角互证标记** 与 **检索空间透明化面板**（各源耗时/命中数/失败源一键单源补搜）。 | **IPC 子空间自动锚定 + `Semantic Scholar` + `PubMed` 学术源 + `CRAAP` 五维评分 + 检索空间透明化面板** |
| **`v0.9.68`** | `研发助手升级规划.md` §五.4 + `2026-10-24` §1.1 Q5/§3.1 + `2026-09-22` §五 P2-E/G | ① `src/db/research_state.rs` 在 `pipeline/engine.rs` 中只写不读——下一轮迭代未读取 `excluded_paths` 做负向约束，导致反复重走死胡同；<br>② 缺 `2026-10-24` Q5 Pipeline 步骤级检查点快照回滚（Checkpoint Rollback）；<br>③ 缺基于 `open_questions` 的限次自动补搜闭环（上限 2 轮）。 | **`ResearchState` 跨轮次读取 `excluded_paths` 负向约束 + 步骤级快照回滚 + 限次自动补搜（上限 2 轮）** |
| **`v0.9.69`** | `研发助手升级规划.md` §五.5 / `v0.7.0` + `2026-10-24` §1.1/§2 + `milestones.md` `M2` | ① `decompose.rs` 缺 `v0.7.0` 研发问题结构化四元组拆解器（技术对象 + 量化目标效果 + 工程边界约束 + 核心矛盾对）；<br>② `MinimalDiff` 缺创造性实质贡献度分级；<br>③ `GET /api/idea/:id/report` 缺原规划 §五.5 定义的第三类核心报告——**`type=patent`（专利代理师版）**；<br>④ 模型调用需收口为表驱动的四角色模型路由器（`FastRewrite / DeepReasoning / VisionMultimodal / StrictJsonVerifier`）。 | **研发问题结构化拆解 + `Minimal Diff 2.0` + 第三类「专利代理师版 (`type=patent`)」报告 + 四角色模型路由收口** |

---

## §一 逐版落地规格与验收标准（`v0.9.64` ～ `v0.9.69`）

### 1. `v0.9.64` · 文档真实附图提取、Umi-OCR 统一接入、证据门禁真阻断与裸截断清零

1. **修复 `c0d8fdd` / PR #243 导致的专利 PDF 真实单张附图提取**：
   - 新增纯 Rust 专利 PDF 真实附图提取引擎（`src/pdf_drawings.rs`）与接口 `POST /api/patent/:id/extract-pdf-drawings`（并在 `enrich_patent_free` / `api_enrich_patent` 当 HTML 无单张图片但有 `pdf_url` 时自动调用，同时支持对用户上传的本地专利 PDF 提取附图）：
     - **策略 A（优先提取 PDF 内真实嵌入的图像对象 Image XObjects）**：遍历 PDF 各页资源字典 `/Resources -> /XObject`，提取 `/Subtype /Image`（支持 `/DCTDecode` JPEG 原生流、`/FlateDecode` RGB/灰度/索引色位图转标准 PNG）；自动按宽高与像素面积阈值（如过滤 `width < 160` 或 `height < 120` 或 `width * height < 25,000` 或高宽比极端失衡的顶部条形码/专利局徽标/小公式符号），并**严格跳过封面纯文字区、`权利要求书` 及纯文字 `说明书` 页**；
     - **策略 B（扫描件 PDF 仅定位并提取 `说明书附图` 专属页/附图区域）**：对整页扫描图像（如 `/CCITTFaxDecode` Group 4 传真流或全页位图），结合页文本层 / `Umi-OCR` 标题识别与版面行密度特征，**仅识别并提取 `说明书附图` 专属页中的附图区域**，绝不把封面、权利要求书和纯文字说明书页转成图片充数；
   - **前端 `templates/patent_detail.html` 还原真实附图展示**：移除 `renderImages` 中用整本 PDF `<iframe>` 顶替图纸页签的应付逻辑；「📄 查看/下载原版 PDF」仅作为标题栏/图纸栏旁边的**独立辅助按钮**；
   - **负面反例单测（Anti-Cheat Test）**：构造“纯文字页（含权利要求书/说明书正文及小尺寸印章/条码）+ 真实附图页（含 `说明书附图` 大尺寸图像 XObject）”的合成多页 PDF，断言提取结果**仅包含真实附图页中的图像，纯文字页与小印章 100% 被排除**。
2. **统一接通 `Umi-OCR`（根治 `upload.rs` 与 `extraction.rs` 两张皮）**：
   - 将 `src/routes/upload.rs` 中的 `Umi-OCR`（`UMI_OCR_BASE_URL`，默认 `http://127.0.0.1:1224`）PDF/图片识别能力抽为公共复用的同步/异步桥接入口；
   - 改造 `src/ai/extraction.rs:228-288` 的 `extract_ocr()`（以及 `src/routes/upload.rs:675-699` 的 OCR 顺序）：**全仓统一优先调用已配置的 `Umi-OCR`**，仅在 `Umi-OCR` 未运行或不可达时再回退 `pdftoppm + tesseract`。
3. **修复 `src/pipeline/steps/finalize.rs:28-49` 证据门禁“光打 `warn` 不阻断”**：
   - 当 `ctx.fact_check_rejected` 为真或 `evidence_kernel::should_suppress_decision(&kernel_report)`（`!allow_decision`）为真时，不再仅打 `tracing::warn!`，而是**实质阻断/降级决策输出**：将 `ctx.action_plan` 降级并注入显式 `【⚠️ 证据门禁拦截声明：核心事实核验未通过或证据不足，已抑制立项/申请正面结论，请核对以下未证实断言与待补证据】`，同时在 `ctx.ai_analysis` 头部写入不可忽略的硬降级警示并持久化至数据库。
4. **清零剩余 6 处裸截断（兑现 `2026-09-22` §2.4）**：
   - 将 `src/ai/patent.rs:146`（`summarize_patent`）、`src/ai/patent.rs:171`（`analyze_claims`）、`src/ai/patent.rs:216-217`（`assess_infringement_risk`）、`src/routes/ai.rs:584,655`（`resolve_compare_item` / `resolve_references`）、`src/routes/upload.rs:307` 的硬编码 `2000/4000` 字裸截断全部升级为 **`structured_patent_excerpt`（100% 保留全部权利要求 + 摘要 + 说明书首尾关键实施例）**。
5. **合入 `:3001` 实测有效修复与规约/复盘落盘**：
   - 保留 `:3001` 实测有效的修复（`api_serpapi_balance` 异步化与 Key 脱敏、跨服务商残留模型自动纠偏与非对话模型过滤、AI 对话与专利详情对话 `📎` 附件及粘贴图片、多专利对比选中主专利自动带出引证/相似对比文献、检索栏控件对齐）；
   - `AGENTS.md` 新增 `§2.8 工程真实性与熔断铁律`、`§3.2` `< v1.0` 版本红线、`Step 7` 强制版本复盘门禁；`docs/errors.md`（#80、#81）与 `docs/feedback.md` 完成落盘。

---

### 2. `v0.9.65` · `MB0b` 向量检索前端闭环、存量断点回填与精简版全链路 Token 记账及预算硬熔断

1. **`MB0b` 语料 IDF 权重、存量断点回填与 `/api/search/vector` 前端接线**：
   - 在 `src/vector/mod.rs` 引入基于本地语料文档频率（DF/IDF）的特征词加权，压低专利高频套话词权重；
   - 提供 `POST /api/patents/backfill-index` 与 `GET /api/patents/backfill-status` 按批游标为存量专利补齐 `patent_chunks` 与 `patents_embedding`；
   - 在 `templates/search.html` 正式接通 `/api/search/vector`（「🧠 语义向量相似检索」入口与结果渲染），修复前后端断链。
2. **精简版全链路 Token 记账与单日/单创意硬熔断保护（不搞 9 服务商硬编码单价表）**：
   - 补齐 SSE 流式对话（`api_ai_chat_stream`、`api_idea_chat_stream`）与 Pipeline 全步骤（`expand / analysis / experiment / oa_assist`）的调用次数与 Input/Output Token 计数落库；
   - 在 `templates/idea.html` 显示「本创意累计调用 N 次 / 共消耗 M tokens」；支持在设置页配置「单创意最大 Token / 单日最大 AI 调用次数」硬熔断阈值，超限即阻断并提示确认，防止失控消耗。

---

### 3. `v0.9.66` · `T2.5` 异步 SSE OCR 管道（优先对接 Umi-OCR + 4 类错误码）与 V-RAG 附图标号双向联动

1. **`T2.5` 异步 SSE OCR 进度流与 4 类结构化错误码**：
   - 大扫描件 PDF 解析优先调用 `Umi-OCR` 分批/分页识别并通过 SSE 推送进度（`page X/Y`），杜绝单请求同步长阻塞超时；
   - 对**加密 PDF（`ERR_PDF_ENCRYPTED`）、损坏文件（`ERR_FILE_CORRUPTED`）、无文本层且无可用 OCR 引擎（`ERR_OCR_UNAVAILABLE`）、超体积上限（`ERR_FILE_TOO_LARGE`）**返回 4 类明确错误码与排障提示。
2. **`v0.7.0` V-RAG 附图标号 ↔ 正文双向高亮与交底书标号一致性核查**：
   - 在 `patent_detail.html` 与 `office_action_response.html` 实现点击正文/权要中的部件标号自动高亮对应附图与部件卡，点击附图部件卡反向筛选高亮正文引用段；
   - 新增交底书/权利要求标号一致性静态核查器，自动检出“图中有号正文未提、正文有号附图缺失、同一标号前后部件名称不一致”三类低级错误。

---

### 4. `v0.9.67` · 检索升级（`v0.6.0`）：IPC 子空间自动锚定 + `Semantic Scholar` / `PubMed` + `CRAAP` 五维评分 + 检索空间透明化

1. **IPC 子空间自动锚定降噪（`src/search/ipc_anchor.rs`）**：检索改写阶段自动推导并锁定 Top-1~3 IPC 主分类子空间（前端可勾选微调），过滤跨学科同名词汇噪声。
2. **新增 `Semantic Scholar` 与 `PubMed` 学术检索源（`src/search/providers/`）**：接入 Semantic Scholar Graph API（含高引影响力指标）与 NCBI PubMed E-utilities，与现有专利/论文源协同召回。
3. **`CRAAP` 五维来源可信度评分与「检索空间透明化面板」**：为每条文献计算确定性 `CRAAP` 五维评分（时效性/相关性/权威度/实体核验准确度/文献类型）与多源三角互证标记；在检索页顶部展示已搜源、各源耗时、命中数、去重数及失败源一键单源补搜。

---

### 5. `v0.9.68` · `ResearchState` 跨轮次负向约束闭环、步骤级快照回滚与限次自动补搜

1. **`ResearchState` 跨轮次读取 `excluded_paths` 负向约束**：打通 `src/db/research_state.rs` 的“只写不读”断链，在下一轮推演各步启动前强制读取 `excluded_paths` 作为硬性负向约束注入 Prompt 与后置校验器，禁止重走已证伪死胡同。
2. **Pipeline 步骤级检查点快照回滚（Checkpoint Rollback）**：每步完成保存上下文快照，支持在 `templates/idea.html` 选择任意历史步骤修改局部输入并从该步向后分支重跑。
3. **限次自动补搜闭环（上限 2 轮）**：当 `analysis` 步检测到 `open_questions` 存在核心对比证据缺口时，自动派生针对性子查询回跳 `search` 补搜（严格硬控最多 2 轮并受 `v0.9.65` 预算熔断约束）。

---

### 6. `v0.9.69` · 研发问题结构化拆解、`Minimal Diff 2.0`、第三类「专利代理师版 (`type=patent`)」报告与四角色模型路由收口

1. **`v0.7.0` 研发问题结构化四元组拆解器**：在检索前将模糊想法拆解为可交互微调的「技术对象 + 量化目标效果 + 工程边界约束 + 核心矛盾对」。
2. **`Minimal Diff 2.0` 创造性贡献分级**：为 8 字段最小差异表增加「常规公知置换 / 跨领域转用 / 协同增效 / 预料不到的技术效果」实质贡献判定与三步法映射。
3. **补齐第三类核心报告 `GET /api/idea/:id/report?type=patent`（专利代理师版）**：输出含 D1/D2 Claim Chart、区别特征-技术问题-技术效果三元映射表、独立/从属权利要求攻防梯度布局树、规避设计包裹建议与专利法第 26.4/22.3 条合规自检表的专业报告。
4. **表驱动四角色模型路由收口（`src/ai/router.rs`）**：将散落模型选择统一收口为 `FastRewrite / DeepReasoning / VisionMultimodal / StrictJsonVerifier` 四角色表驱动路由器，完成 `0.9.x` 主线能力闭环。
