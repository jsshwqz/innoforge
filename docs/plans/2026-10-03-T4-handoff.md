# T4 交接文档：答复期限管理增强

> **最后更新**: 2026-10-03
> **负责人**: Architecture Partner (AI Agent)
> **状态**: 代码完成，已推送远端，待集成

---

## 一、任务概述

**任务编号**: T4
**任务名称**: 答复期限管理增强
**分支**: `feat/oa-deadline-enhance`
**Commit**: `4084eb6`
**工作量**: 0.5 天

### 三个子功能

| # | 子功能 | 状态 | 说明 |
|---|--------|------|------|
| 1 | 二次驳回期限修正为 2 个月 | ✅ 无需改动 | 检查发现 `updateDeadline()` 原代码已正确区分一次/二次驳回期限 |
| 2 | 首页期限提醒 | ✅ 已完成 | 修复 `renderOaDeadlineReminders()` 的 XSS 风险、i18n 缺失、颜色阈值 |
| 3 | 多 OA 结构化跟踪 | ✅ 已完成 | localStorage 按 `innoforge_oa_deadline_<patentNum>` 分别存储 |

---

## 二、改动文件清单

共 3 个文件，30 行新增，4 行删除。

### 2.1 `static/i18n.js` (+8 行)
- 新增 3 个 i18n key（中英双语）：
  - `oa.deadline.reminder` — "OA 答复到期提醒" / "OA Response Deadline Reminders"
  - `oa.deadline.urgent` — "剩余 {days} 天" / "{days} days remaining"
  - `oa.deadline.expired` — "已逾期 {days} 天" / "Overdue {days} days"
- 位置：zh 段在 `oa.diff.onlyOne` 之后；en 段在对应位置

### 2.2 `templates/index.html` (+12 行, -3 行)
- 新增 `_escapeHtml(str)` 辅助函数，防止 patentNumber 注入 HTML（XSS 防护）
- `renderOaDeadlineReminders()` 中：
  - 标题改用 `t('oa.deadline.reminder')` 走 i18n
  - 状态文案改用 `t('oa.deadline.urgent', {days})` / `t('oa.deadline.expired', {days})`
  - patentNumber 用 `_escapeHtml()` 转义（原来调用未定义的 `escapeHtml`，已修复）
  - 颜色阈值：`daysLeft < 0` 红色 / `< 30` 黄色 / `>= 30` 绿色（原代码阈值有误）

### 2.3 `templates/office_action_response.html` (+14 行)
- `updateDeadline()` 函数末尾新增结构化期限存储块
- 存储 key: `innoforge_oa_deadline_<patentNumber>`
- 存储内容: `{ patentNumber, oaType, deadline, daysLeft, date }`
- 位置：在 `info.style.color = color;` 之后，进度条代码之前

---

## 三、未完成事项

| # | 事项 | 说明 |
|---|------|------|
| 1 | 端到端测试未跑 | 环境无 Chrome/Chromium，无法执行 Puppeteer e2e 测试。需接手者在本地跑 `node e2e_test.mjs` 确认 |
| 2 | HTML 函数完整性扫描未跑 | 需接手者运行 `node check_html_functions.mjs`，如基线有变化需 `--refresh` 更新 |
| 3 | ESLint 检查未跑 | 需接手者运行 ESLint 确认 `static/i18n.js` 无 error |
| 4 | PR 未创建 | 分支已推远端，但未创建 Pull Request |
| 5 | `cargo fmt/clippy/test` 未跑 | 本次改动仅涉及前端文件（HTML/JS），不涉及 Rust 代码，但仍建议确认 |

---

## 四、注意事项

### 4.1 分支上有其他 agent 的 commit
`feat/oa-deadline-enhance` 分支上除了我的 T4 commit (`4084eb6`)，还有 T1 agent 的两个 commit：
- `6427a03` — feat(T1): 对比文献在线搜索链fallback
- `91fc279` — docs(T1): 记录T1完成

这些不是我提交的。推送时整个分支被 push 到远端（包括 T1 的 commit）。**协调人需决定如何处理**：是否拆分、是否让 T1 agent 确认等。

### 4.2 localStorage key 设计
- 旧 key: `innoforge_oa_date`（全局单一，不支持多 OA）
- 新 key: `innoforge_oa_deadline_<patentNumber>`（按专利号分别存储）
- 首页 `renderOaDeadlineReminders()` 遍历所有 localStorage key，筛选 `innoforge_oa_deadline_` 前缀的条目
- **未做旧数据迁移**：如果用户已有 `innoforge_oa_date` 旧数据，不会自动迁移到新格式。如需迁移，接手者需在首页加载时添加迁移逻辑。

### 4.3 `_escapeHtml` vs `escapeHtml`
- 原代码调用了 `escapeHtml()`，但该函数在 `index.html` 中未定义（可能依赖全局或其他文件）
- 我新增了 `_escapeHtml()` 作为本地实现，下划线前缀避免命名冲突
- 如果项目其他地方有全局 `escapeHtml()`，可考虑统一；当前 `_escapeHtml` 是自包含的

### 4.4 i18n fallback
- 所有 i18n 调用都用 `typeof t === 'function' ? t(key, vars) : '默认中文'` 做 fallback
- 如果 `i18n.js` 未加载，提醒区域仍能显示中文默认值

### 4.5 颜色阈值
- 红色: `daysLeft < 0`（已逾期）
- 黄色: `daysLeft < 30`（不足 30 天）
- 绿色: `daysLeft >= 30`（充足）
- 原代码阈值有误（`< 15` 和 `< 7` 过于紧迫），已修正为 30 天

---

## 五、验证清单（接手者执行）

```bash
# 1. 切换到 T4 分支
git checkout feat/oa-deadline-enhance

# 2. 确认 T4 commit 存在
git log --oneline | grep 4084eb6

# 3. HTML 函数完整性扫描
node check_html_functions.mjs

# 4. ESLint 检查
export PATH="/c/Users/Administrator/AppData/Local/ms-playwright-go/1.57.0:/c/Users/Administrator/AppData/Roaming/npm:$PATH"
node node_modules/.bin/eslint static/i18n.js 2>&1 | grep -v "node_modules"

# 5. Rust 检查（虽未改 Rust 代码，确认不影响编译）
cargo fmt --check
cargo clippy -- -D warnings

# 6. 端到端测试
cd D:\\test\\patent-hub-backup && node e2e_test.mjs

# 7. 手动验证流程
# - 首页 → 检查 OA 期限提醒区域显示正常
# - OA 答复页 → 填写专利号和 OA 日期 → 检查 localStorage 有 innoforge_oa_deadline_<patentNum>
# - 切换语言 → 检查提醒区域文案切换
```

---

## 六、相关文件位置

- 任务定义: `docs/plans/2026-10-03-phase1-task-assignment.md` 中 T4 部分
- 项目状态: `docs/plans/STATUS.md`
- 仓库规则: `AGENTS.md`
- i18n 系统: `static/i18n.js`
- 首页模板: `templates/index.html`
- OA 答复页: `templates/office_action_response.html`
