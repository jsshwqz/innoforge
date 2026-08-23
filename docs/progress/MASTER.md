# InnoForge 完整重构 — 主控文档 / MASTER

> **本文件是所有执行 agent 的唯一入口。每次会话开始必读本文档，从"当前状态"继续，禁止重开炉灶。**
> 任务：InnoForge（D:\test\patent-hub-backup）整体重构 —— 原地分层重组，功能零变化
> 制定日期：2026-08-15 ｜ 分支策略：直接在 dev ｜ 发布版本：0.1.0（全新版本线）

---

## 1. 权威文档链（按需阅读，不必全读）

| 文档 | 内容 | 何时读 |
|------|------|--------|
| `docs/plan/task-breakdown.md` | 27+ 项任务、验收标准、用户决策记录 | **每个任务开工前** |
| `docs/plan/milestones.md` | M0-M5 客观判定标准 | 每阶段收尾时 |
| `docs/plan/dependency-graph.md` | 阶段依赖与并行泳道 | 认领任务前 |
| `docs/analysis/module-inventory.md` | 模块评分、行号级病灶证据 | 动对应模块前 |
| `docs/analysis/risk-assessment.md` | 风险 Top10 与缓解措施 | 每阶段开工前 |
| `docs/analysis/project-overview.md` | 架构全景、技术栈、118 路由、26 表清单 | 首次进入项目时 |
| `AGENTS.md` | 项目强制规约（提交格式/验证流程/禁止行为） | **始终有效** |

## 2. 用户已拍板的决策（不得重新讨论）

1. 未提交改动：审阅后提交 ✅ 已执行
2. 死代码按质量裁决：fact_check=保留接线；rag=重写并入；vector/search 内联版=重写合一（详见 task-breakdown Phase 0.5）
3. 分支：**直接在 dev 上做**（dev 需先合并 main 最新状态）
4. 版本号：全新版本线，发布 **0.1.0**
5. **重构路线：A 重建式重构**——终态为 workspace 多 crate 全新架构（crates/{types,config,db,ai,search,pipeline,server}），病变代码一律新写替代而非搬运，旧结构零残留后发 0.1.0；workspace 物理化一次性放在 T3.0
6. 执行方式：其它 agent 依据本方案自动执行，无需逐项请示；仅在验收标准无法达成或发现方案性错误时上报

## 3. 阶段总览

- [ ] Phase 0 地基与卫生 (5/13 — T0.1✅ T0.10✅ +三项预处理✅；剩 T0.2-T0.9) → [details](./phase-0-foundation.md)
- [ ] Phase 1 类型与错误地基 (0/5) → [details](./phase-1-types-errors.md)
- [ ] Phase 2 端口层建设 (0/5) ⭐枢纽里程碑 M2 → [details](./phase-2-ports.md)
- [ ] Phase 3 routes 巨型文件拆分 (0/6) → [details](./phase-3-routes-split.md)
- [ ] Phase 4 数据层与死代码收尾 (0/4) → [details](./phase-4-data-deadcode.md)
- [ ] Phase 5 前端重构 (0/6，可与 Phase 3 并行) → [details](./phase-5-frontend.md)
- [ ] Phase 6 收尾发布 v0.1.0 (0/5) → [details](./phase-6-release.md)

## 4. 执行协议（每个 agent 必须遵守）

### 4.1 每个任务的完成定义（DoD）
1. 功能行为与重构前完全一致（除任务明确要求的行为修复）
2. 全套门禁绿：`cargo fmt --check` ＋ `cargo clippy --all-targets -- -D warnings` ＋ `cargo test`（≥161 通过）＋ 改动 templates 时 `node check_html_functions.mjs` ＋ `node e2e_test.mjs`（54+ 通过）
3. 新代码遵守 AGENTS.md：生产路径零 unwrap/expect；新类型查重后放 types/ 或模块内并带 derive(Debug,Clone,Serialize,Deserialize)；API 错误走 AppError；prompt 留在 handler 所在模块
4. 提交格式 `refactor|fix|feat|chore|docs: 中文描述`；**一个任务一个 commit，完成后秒级提交**（防并行互扫）
5. 回写进度：勾选对应 phase 文件的 checkbox + 更新本文件第 3 节计数与"当前状态"

### 4.2 并行泳道
- 泳道 A（src/ 后端）：Phase 1→2→3→4 串行
- 泳道 B（templates/+static/ 前端）：Phase 5 的 T5.1-T5.5 可与泳道 A 并行
- 两泳道文件零交集；同泳道内禁止两个 agent 同时改同一文件

### 4.3 禁止事项（违反即返工）
- 禁止顺手改业务逻辑/文案/API 形状（行为变化另开 fix 任务）
- 禁止引入新 crate 依赖（确需则停下上报）
- 禁止改 DB schema（本轮零迁移；SCHEMA_VERSION 对齐除外）
- 禁止削弱测试/扫描器/基线来"转绿"
- 禁止 `git add -A`（只 add 自己任务触碰的文件）；禁止动他人未提交工作

### 4.4 异常处理
- 门禁红了先定位：是本次改动引起 → 修复；是既有问题 → 记入 phase 文件 Notes 并上报，不得跳过
- 发现方案与代码现实冲突：以可验证的最新代码为准，更新对应文档并在 commit message 说明

## 5. 当前状态（每 session 开始/结束更新此节）

- **日期**：2026-08-15（第二次更新）
- **已完成**：
  1. 全量扫描与三份分析文档 + 完整方案 v2（commits 7f7d5be）
  2. 预处理修复并入库：模板冲突解决 + idea 页 CAD 接线恢复（c6e3139）、cad.rs 调试代码剥离（620b263）
  3. **本地门禁全绿恢复**（413db81）：fmt=0、clippy --all-targets=0（32 项清零）、cargo test **369 通过/0 失败**、HTML 函数扫描通过
- **⚠️ 环境警报**：D 盘曾剩 0.7GB（target 占 8.4GB，已 cargo clean → 7.5GB）。**执行 agent 开工前必查 `Get-PSDrive D`，<5GB 先 clean**
- **下一步（按序）**：
  1. T0.5 切 dev 合 main（dev 落后 main，先 `git checkout dev && git merge main`）
  2. T0.6 推送 main+dev 双远端激活 CI（领先远端 60+/36 提交从未过 CI）
  3. T0.2 / T0.3 / T0.4 / T0.7 / T0.8 并行认领；T0.9 提醒用户
  4. Phase 1 启动：T1.1 types/ 拆域
- **已知风险提醒**：/api/search/vector 中文查询 panic（T2.2 修）；Docker 出口损坏（T0.7 修）；.env 明文密钥（T0.9 用户动作）；mcp-server 未接线函数带 allow(dead_code)+T6.4 标注

## 6. 会话记录（追加式，保留历史）

- 2026-08-15：扫描+方案制定+预处理修复。分析产物 docs/analysis/*，计划产物 docs/plan/*。
