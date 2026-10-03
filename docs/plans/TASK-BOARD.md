# 总指挥任务板 — OA 答复功能全面增强

> 最后更新：2026-10-03
> 总指挥：主 AI（只规划+审核+调度，不写代码）

---

## 一、AI 花名册

| AI 编号 | 当前状态 | 当前任务 | 完成数 |
|---------|---------|---------|--------|
| AI-1 | 🟢 空闲 → 待派 P3-T2 | — | T2✅ |
| AI-2 | 🟢 空闲 → 待派 P3-T3 | — | 0 |
| AI-3 | 🟢 空闲 → 待派 P4-T1 | — | T4✅ |
| 主 AI | 🟡 指挥 | 监控+审核+调度 | — |

---

## 二、全量任务板

### 第一阶段 v0.9.6 — ✅ 全部完成

| 任务 | 功能 | 状态 | commit |
|------|------|------|--------|
| P1-T1 | 对比文献自动获取 | ✅ | dedca85 |
| P1-T2 | 技术机制深度对比 | ✅ | d3a9590 |
| P1-T3 | 一审-二审对比分析 | ✅ | 4010c0a |
| P1-T4 | 答复期限管理 | ✅ | dedca85 |
| P1-T5 | 集成 | ✅ | 4010c0a |

### 第二阶段 v0.9.7 — ✅ 全部完成

| 任务 | 功能 | 状态 | commit |
|------|------|------|--------|
| P2-T1 | 多角色会诊面板 | ✅ | e8c1126 |
| P2-T2 | 模拟审查员预判 | ✅ | 4010c0a |
| P2-T3 | 对方视角防御 | ✅ | 4010c0a |

### 第三阶段 v0.9.9 — 🔵 进行中

| 任务 | 功能 | 负责人 | 状态 | 分支 |
|------|------|--------|------|------|
| P3-T1 | 答复策略智能推荐 | 主AI(已完成) | ✅ | 9f98c6b |
| P3-T2 | 权利要求修改模拟器 | → AI-1 | 📋 待认领 | feat/oa-claim-simulator |
| P3-T3 | 答复质量量化评估 | → AI-2 | 📋 待认领 | feat/oa-quality-score |
| P3-T4 | 集成+验证+提交 | 待分配 | ⏳ 等T2/T3 | — |

### 第四阶段 v0.10.0

| 任务 | 功能 | 负责人 | 状态 | 分支 |
|------|------|--------|------|------|
| P4-T1 | 证据自动收集 | → AI-3 | 📋 待认领 | feat/oa-evidence-collector |
| P4-T2 | 多轮答复全流程追踪 | 待分配 | 📋 | feat/oa-workflow-tracker |
| P4-T3 | 集成+验证+提交 | 待分配 | ⏳ | — |

---

## 三、当前需要用户操作的

把以下指令分别发给对应 AI：

**发给 AI-1：**
> 认领 P3-T2：权利要求修改模拟器。规格在 `docs/plans/2026-10-03-phase3-task-assignment.md`，分支 `feat/oa-claim-simulator`，基于 main 创建。完成后跑 cargo fmt + clippy + test + check_html_functions.mjs，报告 commit hash 和修改文件列表。

**发给 AI-2：**
> 认领 P3-T3：答复质量量化评估。规格在 `docs/plans/2026-10-03-phase3-task-assignment.md`，分支 `feat/oa-quality-score`，基于 main 创建。完成后跑 cargo fmt + clippy + test + check_html_functions.mjs，报告 commit hash 和修改文件列表。

**发给 AI-3：**
> 认领 P4-T1：证据自动收集与整理。规格在 `docs/plans/2026-10-03-phase4-task-assignment.md`，分支 `feat/oa-evidence-collector`，基于 main 创建。完成后跑 cargo fmt + clippy + test + check_html_functions.mjs，报告 commit hash 和修改文件列表。

---

## 四、审核标准（总指挥收到完成报告后执行）

1. `git fetch && git log --oneline [分支]` — 确认分支存在
2. `git diff main...[分支] --stat` — 确认修改范围合理
3. 在分支上跑 `cargo fmt --check` — 格式
4. 在分支上跑 `cargo clippy -- -D warnings` — 零警告
5. 在分支上跑 `cargo test` — 测试通过
6. `node check_html_functions.mjs` — HTML 函数完整
7. 检查代码规范：无 unwrap()、无未消毒 innerHTML、i18n 双语
8. 通过 → 合并 main + 更新任务板 + 派下一任务
9. 不通过 → 列出问题发回修复
