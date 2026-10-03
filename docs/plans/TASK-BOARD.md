# 总指挥任务板 — OA 答复功能全面增强

> 最后更新：2026-10-03（持续监控中）
> 总指挥：主 AI（只规划+审核+调度，不写代码）

---

## 一、AI 花名册

| AI 编号 | 当前状态 | 当前任务 | 完成数 |
|---------|---------|---------|--------|
| AI-1 | 🟢 空闲 | 无 | T2✅ |
| AI-2 | 🟢 空闲 | T3✅ T4✅（已审核通过） | 2 |
| AI-3 | 🟢 空闲 | 无 | T4✅ |
| 主 AI | 🟡 指挥 | 监控+审核+调度 | — |

---

## 二、全量任务板

### 第一阶段 v0.9.6 — ✅ 全部完成

| 任务 | 功能 | 状态 | commit |
|------|------|------|--------|
| P1-T1 | 对比文献自动获取 | ✅ | dedca85 |
| P1-T2 | 技术机制深度对比 | ✅ | d3a9590 |
| P1-T3 | 一审-二审对比分析 | ✅ | aa8d019 (AI-2) |
| P1-T4 | 答复期限管理 | ✅ | 4084eb6 (AI-3) |
| P1-T5 | 集成 | ✅ | 已合入 main |

### 第二阶段 v0.9.7 — ✅ 全部完成

| 任务 | 功能 | 状态 | commit |
|------|------|------|--------|
| P2-T1 | 多角色会诊面板 | ✅ | e8c1126 |
| P2-T2 | 模拟审查员预判 | ✅ | 4010c0a |
| P2-T3 | 对方视角防御 | ✅ | 4010c0a |

### 第三阶段 v0.9.9 — 🔵 进行中

| 任务 | 功能 | 负责人 | 状态 | 分支 |
|------|------|--------|------|------|
| P3-T1 | 答复策略智能推荐 | 主AI | ✅ | 9f98c6b |
| P3-T2 | 权利要求修改模拟器 | → AI-1 | 📋 待认领 | feat/oa-claim-simulator |
| P3-T3 | 答复质量量化评估 | → AI-2 | 📋 待认领 | feat/oa-quality-score |
| P3-T4 | 集成 | 待分配 | ⏳ | — |

### 第四阶段 v0.10.0

| 任务 | 功能 | 负责人 | 状态 | 分支 |
|------|------|--------|------|------|
| P4-T1 | 证据自动收集 | → AI-3 | 📋 待认领 | feat/oa-evidence-collector |
| P4-T2 | 多轮答复全流程追踪 | → AI-1 | 📋 待认领 | feat/oa-workflow-tracker |
| P4-T3 | 集成 | 待分配 | ⏳ | — |

---

## 三、待派任务

### 给 AI-1（空闲）
> 认领 P3-T2：权利要求修改模拟器。先 `git fetch && git checkout main && git pull`，再 `git checkout -b feat/oa-claim-simulator`。规格看 `docs/plans/2026-10-03-phase3-task-assignment.md`。**必须用分支，禁止直接 push main。** 完成后跑 cargo fmt + clippy + test + check_html_functions.mjs，报告 commit hash。

### 给 AI-2（空闲，T3/T4 已审核通过）
> 认领 P3-T3：答复质量量化评估。先 `git fetch && git checkout main && git pull`，再 `git checkout -b feat/oa-quality-score`。规格看 `docs/plans/2026-10-03-phase3-task-assignment.md`。**必须用分支，禁止直接 push main。** 完成后跑全部验证，报告 commit hash。

### 给 AI-3（空闲）
> 认领 P4-T1：证据自动收集。先 `git fetch && git checkout main && git pull`，再 `git checkout -b feat/oa-evidence-collector`。规格看 `docs/plans/2026-10-03-phase4-task-assignment.md`。**必须用分支，禁止直接 push main。** 完成后跑全部验证，报告 commit hash。
