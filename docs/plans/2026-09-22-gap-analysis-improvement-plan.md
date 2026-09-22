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

- [ ] **1.1 更新 AGENTS.md**
  - §2.4 路由注册规则改为「统一在 `src/common.rs::build_router` 注册；`main.rs`/`lib.rs` 共用」，删除"必须同时在 main.rs 和 lib.rs 注册"的过期表述
  - §六 状态速查：DB schema v11→v23、搜索源 2→3+向量、pipeline 步数以 `src/pipeline/steps/` 实态为准
- [ ] **1.2 更新 `docs/plans/STATUS.md`**
  - 「当前版本」一节停留在 2026-07-09 口径；补记 MA2a/MA2b/MA5a 已在变更日志但「版本历史/主要特性」未更新
  - 「技术债务」4 条逐条核销或更新（见 §三）
- [ ] **1.3 对账两份 roadmap 与代码实态**
  - `2026-08-05-advanced-features-roadmap.md` 与 `2026-08-15-phase1-infrastructure-plan.md` 中 B/C/D 标 ⬜ 但代码已有：逐条按验收标准核实（见 §四核实清单），完成后勾选 + commit hash，**禁止只改文档不验证**
- [ ] **1.4 依赖清单自检**：`npm ci --dry-run` 确认 `package-lock.json` 与 `package.json` 同步（规约 §4.3 的 CI 硬性要求）
- [ ] **1.5 推送 MA5a 并合回主线**（远端审计结论，2026-09-22）：本地 `exec/ma5a-settings` 领先 origin/main **2 个提交**（`ba8d56f` + `5fe9558`，EPO OPS 凭证设置页）尚未推送；双远端 main 一致、无 open PR、CI 最新全绿。**本地是最前沿，远端无本地缺失的工作**。按既有 MA 流程走 PR 合并 + 五端同步回写
- [ ] **1.6 陈旧分支清理**（可选）：`feat/freecad-visual-chat`、`fix/ai-image-upload-bug` 等分支内容已通过 PR 合并进主线（同内容双哈希分叉，errors.md 有案），但远端分支仍在，易误导后续 agent 以为有未合并工作；确认后删除远端分支

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

## 二、P0 — 数据完整性审计（规约红线）

> 动机：AGENTS.md §2.5「截断不得破坏数据完整性」；STATUS.md 曾记录 OA 后端 60k/40k/15k 静默截断为遗留项。

- [ ] **2.1 审计 `src/routes/ai.rs` 三处 `chars().take(150000)`**（行 612 / 883 / 1030）
  - 确认 `preview` 变量是**显示用途**还是**喂给 AI 的数据用途**
  - 若喂 AI：改为全文/分块策略，或在 UI 明确提示「内容过长已截取前 15 万字符」，禁止静默截断
- [ ] **2.2 全仓截断点复查**：`take(` / `substring` / `slice` / `safe_truncate` 在生产路径逐一标注「显示用/数据用」，数据用的一律保全文
- [ ] **2.3 历史遗留核销**：确认 OA 讨论链路旧截断（60k/40k/15k）是否已随 150k 调整消解，在 STATUS.md 回写结论

## 三、P1 — 技术债务清理（STATUS.md 已挂账）

- [ ] **3.1 `office_action_response.html` JS 规范化**：现存约 498 处 `var`、历史遗留 16 个 ESLint `no-redeclare` 错误 → 逐步改 `let`/`const`。风险：该文件 156KB，改动后必须跑 ESLint + `check_html_functions.mjs` + 完整 e2e + OA 核心流程手动回退检测（分析→讨论→答复书导出）
- [ ] **3.2 e2e 覆盖度核实**：AGENTS.md 声称 54/54；当前 `e2e_test.mjs`（27KB）需跑一遍确认真实通过数，并补新增功能的用例：语义搜索开关、成本统计页、记忆 API、FreeCAD 图卡、EPO OPS 降级链（离线 mock）
- [ ] **3.3 OCR 异步化**：上传大 PDF 时 OCR 阻塞请求线程 → 改后台任务 + 轮询/SSE 进度（先确认现状是否仍阻塞）
- [ ] **3.4 文件解析器错误处理**：解析失败给用户可读提示（非 500/静默空），覆盖 PDF/DOCX/DOC/图片 OCR 四类
- [ ] **3.5 OA 专用表长期方案**：`src/db/oa.rs` 已存在，核对与 `case_documents` 的边界，决定是否需要 v24 迁移收敛（涉及 schema 变更，执行前必须用户确认）

## 四、P1 — 已建能力的功能闭环核实（查漏补缺核心）

> 动机：Phase 1 代码已写但验收未做，可能存在「后端有 API、前端没入口」「能力建了、没接到主流程」的半拉子工程。

**核实清单（每项先验证再定工作量）：**

- [ ] **4.1 RAG 提示词注入（roadmap D5）**：检查 `src/routes/idea.rs` / `patent.rs` / `ai.rs` 的 AI 调用点是否都接了 `retrieve_relevant`；前端是否有「📚 引用了 N 个参考材料」提示（D6）；注入材料是否用边界标签包裹（防注入，规约 §2.6）
- [ ] **4.2 语义搜索前端闭环**：`/api/search/vector` 已有路由，核实搜索页是否有语义开关/权重调节入口（B6）；验收用例「搜『电池寿命』命中『续航改进』」实测（B7 e2e）
- [ ] **4.3 成本追踪闭环**：设置页统计 UI 已有（settings.html:231+），核实：创意页「本创意 AI 花费」chip（C5）、模型单价编辑表、token 数来自各服务商 usage 字段的 per-provider 适配（风险清单第 6 条）
- [ ] **4.4 记忆系统闭环**：`/api/idea/:id/memory` API 已有，核实创意页是否有记忆查看/管理 UI；`src/context.rs` 的项目记忆注入与创意记忆是否各司其职、无重复
- [ ] **4.5 嵌入失败降级**：服务商不支持 `/embeddings` 时 UI 是否明确提示且不阻断关键词搜索（B 验收第 3 条）

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

## 八、执行顺序建议

```
第 1 批（纯文档，0 风险）：§一 全部
第 2 批（审计，不改行为）：§二 截断审计 → 出结论后再定修复
第 3 批（核实闭环）：§四 4.1–4.5 → 缺什么补什么
第 4 批（技术债务）：§三（3.1 风险最高，单独提交）
第 5 批（新功能）：§五 按用户确认的优先级逐项开工
每批走完 AGENTS.md Step 5 全部门禁（fmt/clippy/test/ESLint/check_html_functions/e2e）再提交
```

## 九、明确不做 / 边界

- 不引入新 crate 依赖（嵌入走现有服务商 API 的既定决策不变）
- 不引入前端构建工具链
- 不动 DB schema 除非 §3.5 经用户确认
- §五 新功能未确认前不写任何实现代码
