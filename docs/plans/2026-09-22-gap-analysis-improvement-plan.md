# InnoForge 项目提升计划（功能查漏补缺）/ Gap-Analysis Improvement Plan

> 规划日期：2026-09-22 | 基于 v0.7.4 + MA5a 分支（exec/ma5a-settings）代码现状
> 性质：仅计划，不含实现。执行前需逐项走 AGENTS.md Step 3 确认。
> 目标读者：可独立接力的 AI Agent

---

## 〇、现状速查（本次调查实证）/ Current State (verified 2026-09-22)

| 项目 | 实测值 | 备注 |
|---|---|---|
| 版本 | 0.7.4（`Cargo.toml`） | `[Unreleased]` 已积压大量变更，未发版 |
| DB Schema | **v23**（migrations.rs 最大迁移号） | AGENTS.md 速查写 v11，已过期 |
| API 路由 | **118 个**，集中在 `src/common.rs::build_router` | `main.rs`/`lib.rs` 均调用 `build_router`，不再各自注册 |
| 搜索源 | 3 个在线源：SerpAPI → EPO OPS → Google Patents XHR（SourceChain 降级链）+ 本地 FTS5 + `/api/search/vector` 语义搜索 | MA2a/MA2b/MA5a 已落地 |
| Phase 1 基础设施 | 代码已存在：`src/rag/`、`src/vector/`、`src/db/cost.rs`、`src/db/memory.rs`、成本 API（`/api/ai/cost*`）、记忆 API（`/api/idea/:id/memory`）、设置页 AI 使用统计 UI | 但 roadmap（2026-08-05/08-15 两份）中 B/C/D 仍标 ⬜，**文档与代码不同步** |
| Pipeline | 18 个 step 文件（含 orchestrator） | AGENTS.md 速查写 15 步，已过期 |
| 前端 | 8 模板，`check_html_functions.mjs` 扫描通过 ✅ | `e2e_test.mjs` 存在 |
| AI 调用 | 60s 单次上限、分场景温度、prompt 注入边界、`<user_input>` 隔离均已落地 | |

---

## 一、P0 — 文档与规约对账（低成本、防误导，先做）

> 动机：AGENTS.md/STATUS.md/roadmap 是 AI Agent 接力的入口，过期信息会直接误导后续开发（本次调查即发现 4 处实证冲突）。

- [x] **1.1 更新 AGENTS.md** ✅（PR #15，commit `edc4d52`）
  - §2.4 路由注册规则改为「统一在 `src/common.rs::build_router` 注册；`main.rs`/`lib.rs` 共用」，删除"必须同时在 main.rs 和 lib.rs 注册"的过期表述
  - §六 状态速查：Pipeline 15→16 步、AI 服务商「多服务商」→「内置 10 家 + API 实时模型查询」、搜索源「2 个」→「在线 3 源降级链 + FTS5 + 向量语义搜索」；DB schema 行保持「以源码为准」口径（实测 v23）
- [x] **1.2 更新 `docs/plans/STATUS.md`** ✅（PR #15，commit `edc4d52`）
  - 新增 2026-09-22 状态变更条目（MA5a/PR #14/五端同步/RAG 死代码发现）
  - 「当前版本」一节由「v0.7.4 (开发中)」改为「v0.7.4（已发布）+ 拟发 v0.8.0」
- [ ] **1.3 对账两份 roadmap 与代码实态**：§四 核实已给出结论（B/C/D 实为「空壳/半拉子」）。**结论：不能简单勾选**——B（向量）与 D（RAG）需先做 §4.6 路线决策；C（成本）为半拉子，需补全落账与单价。roadmap 勾选待相应修复完成后再回写
- [x] **1.4 依赖清单自检** ✅：`npm ci --dry-run` 通过（锁文件与 package.json 同步）
- [x] **1.5 推送 MA5a 并合回主线** ✅：PR #14 已合并（merge commit `1a101f1`），CI lint/test/e2e 全绿；五端同步完成
- [ ] **1.6 陈旧分支清理**（可选）：`feat/freecad-visual-chat`、`fix/ai-image-upload-bug` 等分支内容已通过 PR 合并进主线（同内容双哈希分叉，errors.md 有案），但远端分支仍在，易误导后续 agent 以为有未合并工作；确认后删除远端分支
- [ ] **1.7 本地分支清理**（可选）：已完成分支 `exec/ma5a-settings`、`exec/docs-reconcile`、`exec/gap-verify` 在合并且回写完毕后删除，避免分支堆积

### 远端审计记录（2026-09-22）

| 检查项 | 结果 |
|---|---|
| `git fetch --all` 后逐分支比对 | origin/main 完整包含于本地 HEAD（`merge-base --is-ancestor` 通过） |
| gitee/main vs origin/main | 完全一致，双向无 diff |
| 本地 vs origin/main | 领先 2（MA5a 未推送），落后 0 |
| open PR | 无；最近合并 PR #8–#13（FreeCAD → MA2b），全部已进入主线 |
| 「领先本地」的远端分支 | `feat/freecad-visual-chat`(+12)、`fix/ai-image-upload-bug`(+5) 等：内容是合并前的双哈希分叉（如「先进化执行计划」存在 `2b49d21`/`a612f44` 两个同内容哈希），本地均已包含，**无实际缺失** |
| CI 状态 | 最新 main/dev push 全绿（run 35545978756 等） |
| Tags | 远端最新 v0.7.4，与本地一致 |

## 二、P0 — 数据完整性审计（规约红线）✅ 已完成（2026-09-22 实证）

> 动机：AGENTS.md §2.5「截断不得破坏数据完整性」；STATUS.md 曾记录 OA 后端 60k/40k/15k 静默截断为遗留项。

### 审计结论

- [x] **2.1 三处 `chars().take(150000)`：全部是数据用途（拼进发往 AI 的 prompt），非显示用**
  - `src/routes/ai.rs:612` `resolve_compare_item` → 「上传文件」内容 → 喂 `api_ai_compare`（两两对比）
  - `src/routes/ai.rs:883` `api_ai_compare_matrix` → 喂 `ai.compare_multiple`（多专利对比矩阵）
  - `src/routes/ai.rs:1030` `resolve_references` → 标「对比文件全文」→ 喂创造性/侵权分析
- [x] **2.2 全仓截断点已分类**（PowerShell 全仓扫描）
  - **数据路径（静默截断，需修）**：`ai.rs:623/624/824/895/1005/1046/1047`、`ai/patent.rs:37/62/107/108/151/201/202/318/319/320/756/757/1000/1001/1046/1047`、`idea.rs:1559/1610`、`upload.rs:307/309`（对比分析仅取说明书 2k / 权利要求 3k，额度偏小）
  - **合规范本（照此改造）**：`ai.rs:1644–1661` OA 讨论 —— 120K 上限、从最老消息开始移除、单条超限时截断并追加「[讨论内容已被截断以适配 AI 上下文窗口]」显式标记、DB 侧保存不受限（2M）
  - **显示/日志路径（合规，不动）**：`ai.rs:718`、`search.rs:183/631/675/751`、`idea.rs:1542/1599/1666/1772/1780`、`upload.rs:437`、`context.rs:54`（项目记忆 ≤6000 为规约自身预算）
- [x] **2.3 历史遗留核销**：旧 60k/40k/15k 字面上已不存在（被 120k/150k/300k 取代），但**同一类问题以新额度形式仍在**，不能算已解决

### 待修（新增任务 2.4）

- [ ] **2.4 静默截断显式化**：对 2.2「数据路径」清单统一采用 `ai.rs:1644` 范式——截断时必须
  1. 在拼给 AI 的文本里追加显式标记（原文总字符数 + 已截取长度）
  2. 前端可见提示（避免用户以为全文已送入）
  3. 保留 DB 全文，不因显示/上下文限制改写入内容
  单独成 PR，附逐点对照测试。

## 三、P1 — 技术债务清理（STATUS.md 已挂账）

- [ ] **3.1 `office_action_response.html` JS 规范化**：现存约 498 处 `var`、历史遗留 16 个 ESLint `no-redeclare` 错误 → 逐步改 `let`/`const`。风险：该文件 156KB，改动后必须跑 ESLint + `check_html_functions.mjs` + 完整 e2e + OA 核心流程手动回退检测（分析→讨论→答复书导出）
- [ ] **3.2 e2e 覆盖度核实**：AGENTS.md 声称 54/54；当前 `e2e_test.mjs`（27KB）需跑一遍确认真实通过数，并补新增功能的用例：语义搜索开关、成本统计页、记忆 API、FreeCAD 图卡、EPO OPS 降级链（离线 mock）
- [ ] **3.3 OCR 异步化**：上传大 PDF 时 OCR 阻塞请求线程 → 改后台任务 + 轮询/SSE 进度（先确认现状是否仍阻塞）
- [ ] **3.4 文件解析器错误处理**：解析失败给用户可读提示（非 500/静默空），覆盖 PDF/DOCX/DOC/图片 OCR 四类
- [ ] **3.5 OA 专用表长期方案**：`src/db/oa.rs` 已存在，核对与 `case_documents` 的边界，决定是否需要 v24 迁移收敛（涉及 schema 变更，执行前必须用户确认）

## 四、P1 — 已建能力的功能闭环核实 ✅ 已完成（2026-09-22 实证）

> 动机：Phase 1 代码已写但验收未做，可能存在「后端有 API、前端没入口」「能力建了、没接到主流程」的半拉子工程。
> 核实方式：独立核实员 + 命令行证据复核，逐项给 file:line。

### 核实结论（重要：三项基础设施中有两项是「空壳」）

| 项 | 结论 | 关键证据 |
|---|---|---|
| **4.1 RAG 提示词注入** | ❌ **缺失（死代码）** | `src/rag/` 四文件齐全（`mod.rs:21` build_chunks_from_patent、`retriever.rs:8` retrieve_chunks、`assembler.rs:6` assemble_rag_prompt、`chunker.rs:6/57`），但全仓除模块内自调（`mod.rs:92/103`）外**零外部调用**：建块、检索、注入、前端提示四环节全部断链 |
| **4.2 语义搜索闭环** | ❌ **缺失（三重断链）** | ① 前端零调用：`templates/*.html` 与 `static/*.js` 中无任何 `/api/search/vector` 引用；② 设置页无 B6 要求的任何配置项（无开关/权重/嵌入服务商）；③ 向量层无数据：`src/vector/mod.rs:137 build_index` 为骨架（仅插入 `vec![0.0f32]` 占位），`mod.rs:159 compute_and_save_embedding` 全仓零调用 → `/api/search/vector`（`search.rs:425`）读 `count_embeddings()==0` 恒空转 |
| **4.3 AI 成本追踪** | 🟡 **半拉子** | ✅ `client.rs:217–237` usage 解析兼容 OpenAI(prompt/completion) 与 Anthropic(input/output)；✅ 设置页汇总 UI `settings.html:231–252/652`；❌（审计时）落账仅 2 处调用点（`ai.rs:455` 对话、`idea.rs:750` 创意），OA/对比/创新分析/pipeline 等大量 AI 调用**不落账**；❌ 估算系数硬编码于 `db/cost.rs:85–89`（`(in*0.001+out*0.002)/1000*100*100`），**无 per-model 单价、单位可疑**；❌ 创意页无「本创意花费」chip；❌ 无模型单价编辑表。**本批闭环（routes 层 + claim_tree）**：新增 `db.log_ai_call(&ai, 类型, idea_id, session_id)` 统一落账，落账点 2 → **21 处**（ai.rs 14、idea.rs 5 含 history-compress/summary-merge、upload.rs 1、pipeline claim_tree.rs 1；其中 19 新增——含补 `compress_history` 漏记点，2 处存量改接），并修 idea-chat 原把调用类型误填 provider 的错账；`save_cost_record_from_client` 收敛为 `log_ai_call` 唯一内部封装。**仍开放**：流式 SSE 不解析 usage 无法落账、批量摘要单次近似、pipeline 其余 LLM 步骤（expand/analysis×3/experiment/oa_response）未接线、创意页花费 chip、per-model 单价编辑表 |
| **4.4 创意记忆系统** | ✅ **已修复** | ✅ API 三件套 + 创意页记忆标签页 UI（`idea.html:73/117–123`）；✅ **本批补齐**：记忆条目注入 `api_idea_chat` system context（`idea.rs` `build_memory_context()`，≤20 条/≤2000 字符预算、按更新时间倒序、`<idea_memory>` 边界隔离 + 转义防注入、失败静默降级）；✅ 写入侧已有 pipeline finalize 自动沉淀（`finalize.rs:29`） |
| **4.5 嵌入失败降级** | ❌ **不存在该机制** | 无任何走服务商 `/embeddings` 的路径，故无「服务商不支持」提示；无数据时静默空转返回纯 BM25 结果，用户完全无法感知语义搜索未生效 |

### 新发现：架构决策漂移（需用户确认，属 §三 决策类）

`docs/plans/2026-08-15-phase1-infrastructure-plan.md` §3.1 的决策是「**复用现有 AI 服务商 API 生成嵌入向量，不引入新依赖**」，但代码实态是另一套方案：

- `src/vector/mod.rs`：**本地字符 n-gram(2–4) TF-IDF 伪嵌入**，512 维定长，无 IDF（无语料统计），`build_index` 为 skeleton
- `src/rag/chunker.rs:57 compute_chunk_embedding`：RAG 侧另一份**独立**嵌入实现（与 vector 模块不共用）

后果：语义能力实际不成立（TF-IDF 无 IDF + 只排序不保序 → 检索质量远低于 BM25），且两套嵌入并存埋下重复维护风险。

- [x] **4.6 嵌入路线决策**（需用户拍板，二选一）→ **2026-09-28 已拍板：B 案**：
  - **A. 回到计划路线**：实现 `AiClient::embed()`（OpenAI 兼容 `/v1/embeddings`），向量存 DB，`/api/search/vector` + RAG 共用同一嵌入；服务商不支持时明确降级提示
  - **B. 保留本地方案**：明确放弃「语义」表述，改称「本地相似度补充排序」，补 IDF/语料统计，删除重复嵌入实现，并在 UI 说明其局限
  未拍板前，§4.1/4.2 的接线工作不应开工（否则接的是空壳）。
  - **2026-09-27 用户拍板：暂缓（选项 C「先推完 M-A」）**。A/B 二选一推迟到 M-A（MA4b → MA6 → M-B 规格书补发）收口后再定；在此之前 §4.1 RAG 接线、§4.2 语义搜索三重断链收口、§4.5 嵌入降级提示**一律不派包**，避免接空壳。规划会话按此纪律维持现状不动 `src/search/vector*`/嵌入相关代码。
  - ✅ **2026-09-28 用户拍板：走 B 案（保留本地 TF-IDF 方案，本轮不上嵌入模型）** —— M-A 已于当日收口（PR #27，五端对齐 `1af09fd`），决策门解除。落定为 M-B 规格书 §0 的两条硬约束：**M-B 全程禁止新 crate 依赖**（`AiClient::embed()` / ONNX / 外部 embedding API 一律不派包），并且**语义能力的对外口径降级为「本地相似度补充」**，其「向量丢 term 映射 ⇒ 相似度不构成语义能力」的局限必须写进代码注释与文档，禁止伪造分数。三条解冻项与其归属：§4.2 语义断链收口 + 删重复嵌入实现 → **M-B 包 MB0**（含 char-boundary panic 修复与 `compute_and_save_embedding` 写入链接通，但**全表批量回填有意不做**）；§4.1 RAG 接线 → **M-B 包 MB3**（前置 MB2 免费全文供给）；§4.5 嵌入降级提示 → **M-B 包 MB0/MB3** 的响应结构化字段侧。
    - **与 B 案原文的一处有意偏离（登记不隐瞒）**：B 案含「补 IDF / 语料统计」，本轮 MB0 **只做** panic 安全 + 三份实现归一 + 写入链接通，IDF/语料统计**推迟**——理由：在 `patents_embedding` 生产恒 0 行（写入链断裂）、且全文供给本身尚未落地（`description/claims` 对在线命中恒空）的现状下，先补 IDF 属于给空表调排序权重，收益不可验证；先把「有没有向量」与「同一份实现」两件事做实，IDF 待 MB3 有真实切片语料后再评估。若用户要求 B 案全量落地，须另派一包（MB0b）。
    - 配套第二项决策：**只测免费链、SerpAPI/EPO 留占位** ⇒ M-B 每包 DoD 必须在无 `SERPAPI_KEY`、无 `EPO_KEY/EPO_SECRET` 的环境下闭环（§4.3 成本落账已在 2026-09-26 单独完成，与本决策无关）。

## 五、P2 — 新功能候选（roadmap Phase 2/3 未启动项）

> 均在原 roadmap 中有定义，此处仅列为候选，**开工前需逐个与用户确认优先级**。

| 编号 | 功能 | 价值 | 预估 | 前置 |
|---|---|---|---|---|
| E | 多智能体 Pipeline 升级（orchestrator 已起步） | 分析质量 | 3 周 | 4.x 闭环核实完成 |
| G | Agentic 自主研究（自动多轮检索-分析-反思） | 差异化核心 | 3 周 | E |
| H | 实验沙箱升级 | 研创台验证能力 | 2 周 | 无 |
| I | 专利组合智能分析（多专利批量对比/布局） | 面向研发决策 | 3 周 | 4.2 |
| J | 可观测性（关键 API 性能监控/日志） | 工程化 | 1-2 周 | 无 |

## 六、P2 — 测试与文档补全

- [ ] **6.1 单元测试补强**：OA 分析模块、文件解析器（STATUS.md 下一步计划第 3 条）
- [ ] **6.2 OA 模块使用说明**：`docs/` 补面向研发用户的操作文档（STATUS.md 下一步计划第 4 条）
- [ ] **6.3 真实 AI 冒烟**：语义搜索/RAG/成本统计涉及真实服务商，至少 DeepSeek 主链路人工冒烟一次

## 七、版本与发布建议

- `[Unreleased]` 已积压：10 家服务商、FreeCAD 对话、导出对话、抗幻觉、MA2a/MA2b 搜索降级链、向量/成本/记忆基础设施等 → 建议 §一~§四完成（P0+P1 闭环）后发 **v0.8.0**（MINOR：新功能、非破坏性）
- 发版时同步：`Cargo.toml` version、CHANGELOG 移出 Unreleased、STATUS.md 当前版本节、双远端 Release

---

## 八、执行顺序与进度

```
第 1 批（纯文档，0 风险）：§一 1.1/1.2/1.4/1.5   ✅ 完成（PR #14、#15 已合并，五端同步）
第 2 批（审计，不改行为）：§二                  ✅ 完成（结论见 §二）
第 3 批（核实闭环）：§四 4.1–4.5                ✅ 完成（结论见 §四）
第 4 批（决策→修复）：
   ├── ✅ 决策门：§4.6 嵌入路线 → **2026-09-28 用户拍板 B 案（不上嵌入模型）+ 只测免费链**，M-B 规格书 §0 已固化
   ├── §二 2.4 静默截断显式化（不依赖决策，可先做；其中 pipeline 两处 `chars().take` 已由 M-B 包 MB3 承接）
   ├── ✅ §4.3 成本落账补全（2026-09-26 完成，PR #19，落账点 2→21；per-model 单价与创意页 chip 仍开放）
   ├── §4.4 记忆注入 prompt
   └── §4.1/4.2 RAG 与语义搜索接线 → **决策门已解除，归 M-B 包 MB0（4.2 断链收口 + 删重复实现）与 MB3（4.1 RAG 接线）**，见 `docs/plan/2026-09-28-mb-construction-spec.md`
第 5 批（技术债务）：§三（3.1 OA 页 JS 规范化风险最高，单独提交）
第 6 批（新功能）：§五 按用户确认的优先级逐项开工
每批走完 AGENTS.md Step 5 全部门禁（fmt/clippy/test/ESLint/check_html_functions/e2e）再提交
```

### 执行记录（2026-09-22）

| 时间 | 动作 | 结果 |
|---|---|---|
| 批次 1 | 远端审计 | 双远端无本地缺失工作；本地领先 2 提交（MA5a） |
| 批次 1 | PR #14（MA5a + 计划文档） | 已合并 `1a101f1`；CI lint/test/e2e 全绿；五端同步 |
| 批次 1 | PR #15（AGENTS.md/STATUS.md 对账） | 已合并 `0d626e4`；五端同步 |
| 批次 2/3 | 截断点审计 + 能力闭环核实 | 见 §二 / §四；三项基础设施中 RAG、向量为死代码，成本/记忆为半拉子 |
| 批次 2/3 | 副产物 | 计划文档本批更新；team 核实员因模型日配额（429）停在 4.5，改由命令行复核补齐 |

## 九、明确不做 / 边界

- 不引入新 crate 依赖（嵌入走现有服务商 API 的既定决策不变）
- 不引入前端构建工具链
- 不动 DB schema 除非 §3.5 经用户确认
- §五 新功能未确认前不写任何实现代码
