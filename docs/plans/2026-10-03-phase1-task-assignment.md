# 第一阶段分工计划书 — v0.9.6 并行开发指南

> 创建日期：2026-10-03
> 目标版本：v0.9.6
> 用途：供多个 AI Agent 并行认领任务，独立开发，最后合并

---

## 一、任务总览

第一阶段共 4 个功能任务 + 1 个集成任务。

| 任务ID | 功能 | 工时 | 可并行 |
|--------|------|------|--------|
| T1 | 对比文献自动获取与全文分析 | 2天 | 后端独立 |
| T2 | 技术机制深度对比 | 1.5天 | 改 prompt 独立 |
| T3 | 一审-二审对比分析 | 1天 | 后端已有 history 基础 |
| T4 | 答复期限管理增强 | 0.5天 | 前端已有 initDeadline |
| T5 | 集成+验证+提交 | 0.5天 | 依赖 T1-T4 完成 |


---

## 二、文件冲突分析与分区策略

### 独占文件（各任务改自己的，无冲突）

| 任务 | 独占后端 | 独占前端区域 |
|------|---------|-------------|
| T1 | `src/routes/oa_refs.rs`（新建） | OA 页对比文献区域 |
| T2 | `src/ai/patent.rs`（改 prompt） | OA 页特征矩阵区域 |
| T3 | `src/routes/oa_diff.rs`（新建） | OA 页一审二审 diff 区域 |
| T4 | 无后端 | OA 页期限区域 + 首页 |

### 共享文件合并策略

| 文件 | 涉及任务 | 策略 |
|------|---------|------|
| `src/common.rs` | T1/T3 各加路由 | T5 统一注册 |
| `templates/office_action_response.html` | T1/T2/T3/T4 | 分区域插入 |
| `static/i18n.js` | T1/T2/T3/T4 | 各加自己的 key 前缀 |
| `src/db/migrations.rs` | T3 可能加迁移 | T3 独占 v24 |

### 前端分区（关键！各任务只改自己的区域）

```
区域 A（T1）：对比文献 — 自动获取按钮 + 文献卡片
区域 B（T2）：特征矩阵 — 机制列 + 结合动机列
区域 C（T3）：一审二审 diff — 差异对比区块
区域 D（T4）：期限管理 — 倒计时增强 + 首页提醒
区域 E（已有）：分析结果 — 不改
```

规则：每个任务用 `<!-- TX: xxx -->` 标记自己的区域，只在该区域内操作。


---

## 三、任务详细规格

### T1：对比文献自动获取与全文分析

**分支**：`feat/oa-fetch-refs`

#### 现有基础设施（不要重复造！）

- `autoExtractPublicationNumbers(text)` 已存在于 `templates/office_action_response.html:777`
- `lookupRef(btn)` 已存在于 `templates/office_action_response.html:515`
- `src/search/provider.rs:30` 有 `lookup_exact` trait 方法
- `src/routes/search.rs:338` 有精确直查逻辑

#### 后端

新建 `src/routes/oa_refs.rs`：

```rust
/// POST /api/ai/oa-fetch-refs
/// 请求: { oa_text: String }
/// 响应: { refs: [{ pub_number, title, abstract, found: bool }] }
///
/// 逻辑：
/// 1. 正则提取所有公开号（CN\d{7,13}[A-Z]）
/// 2. 每个公开号先查本地 SQLite（db.search_smart_exact）
/// 3. 本地未找到则尝试在线搜索链
/// 4. 返回结果列表
pub async fn api_ai_oa_fetch_refs(
    State(s): State<AppState>,
    Json(req): Json<OaFetchRefsRequest>,
) -> Json<serde_json::Value>
```

路由注册（`src/common.rs`）：
```rust
.route("/api/ai/oa-fetch-refs", post(routes::api_ai_oa_fetch_refs))
```

#### 前端

在对比文献区域增加：
1. "自动获取对比文献全文"按钮 + spinner
2. 文献卡片（标题、摘要、全文可折叠）
3. 全文注入 `uploadedData['ref_N'].content` 参与后续分析

关键 JS：
```javascript
async function autoFetchRefs() {
    // 1. POST /api/ai/oa-fetch-refs { oa_text: getOA().content }
    // 2. 显示结果卡片
    // 3. 全文注入 uploadedData
}
```

#### i18n keys（前缀 `oa.refs.`）
- `oa.refs.autoFetch` = "自动获取对比文献" / "Auto-fetch references"
- `oa.refs.fetching` = "正在获取对比文献全文..." / "Fetching reference full text..."
- `oa.refs.fetchSuccess` = "成功获取 {count} 篇" / "Fetched {count} references"
- `oa.refs.fetchFail` = "获取失败，请手动上传" / "Fetch failed, please upload manually"
- `oa.refs.notFound` = "未找到全文" / "Full text not found"

#### 验收
- [ ] 输入含 CN103133144A 等公开号能自动提取并搜索
- [ ] 搜索结果含标题和摘要
- [ ] 未找到显示"未找到全文"
- [ ] 全文能参与后续 AI 分析
- [ ] 不破坏现有手动添加对比文献功能


---

### T2：技术机制深度对比

**分支**：`feat/oa-mechanism-compare`

#### 后端

改造 `src/ai/patent.rs` 所有 7 个 `build_*_prompt` 函数，增加机制分析指令。

在现有 prompt 特征对比部分之后追加：

```
## 技术机制深度分析

除了结构特征对比，还需进行技术机制层面深度分析：

1. 技术领域分析：每篇对比文件解决的技术问题属于什么领域？是否与本申请相同？
2. 工作原理对比：对比文件的技术方案如何工作？与本申请有何本质区别？
3. 结合动机分析：本领域技术人员是否有动机组合这些对比文件？组合后是否面临技术矛盾？
4. 协同效应分析：本申请各技术特征之间是否存在协同关系？是否被对比文件公开？

请输出技术机制对比表：技术领域 | 解决问题 | 工作原理 | 与本申请关系
```

7 个函数（只改 prompt 内容，不改签名）：
1. `build_first_exam_prompt` (line 452)
2. `build_abnormal_prompt` (line 584)
3. `build_reject_review_prompt` (line 652)
4. `build_second_rejection_prompt` (line 752)
5. `build_reexamination_request_prompt` (line 782)
6. `build_reexamination_decision_prompt` (line 812)
7. `build_admin_lawsuit_prompt` (line 842)

#### 前端

在特征矩阵区域：
1. `showOAAnalysisSections` 中识别"技术机制对比"段落
2. 在现有特征矩阵下方渲染机制对比表
3. 高亮显示结合动机分析

#### i18n keys（前缀 `oa.mechanism.`）
- `oa.mechanism.title` = "技术机制深度对比" / "Technical Mechanism Comparison"
- `oa.mechanism.domain` = "技术领域" / "Technical Domain"
- `oa.mechanism.principle` = "工作原理" / "Working Principle"
- `oa.mechanism.motivation` = "结合动机" / "Combination Motivation"
- `oa.mechanism.synergy` = "协同效应" / "Synergistic Effect"

#### 验收
- [ ] AI 输出含"技术机制深度分析"段落
- [ ] 对 D3 能输出"解决的是不同领域问题"类分析
- [ ] 前端能渲染机制对比表
- [ ] 不破坏现有特征矩阵

---

### T3：一审-二审对比分析

**分支**：`feat/oa-history-diff`

#### 现有基础设施

- `GET /api/oa/history/:patent_number` 已存在（`src/routes/ai.rs:2040`）
- `GET /api/oa/history/all` 已存在（`src/routes/ai.rs:2051`）
- 前端 `loadOaState()` 已有 localStorage 管理

#### 后端

新建 `src/routes/oa_diff.rs`：

```rust
/// GET /api/oa/history/:patent_number/diff
/// 返回该专利号所有 OA 轮次的对比分析
///
/// 逻辑：
/// 1. 查询该专利号所有历史 OA 记录
/// 2. >= 2 条则生成相邻轮次 diff
/// 3. diff：驳回理由变化、对比文件变化、权利要求变化
pub async fn api_oa_history_diff(
    State(s): State<AppState>,
    Path(patent_number): Path<String>,
) -> Json<serde_json::Value>
```

改造 `src/ai/patent.rs`：`build_second_rejection_prompt` 中如传入一审 OA 文本，追加：

```
## 一审历史对比

一审驳回理由：{first_exam_reasons}
二审新增驳回理由：{new_reasons}
一审已克服的问题：{overcome_issues}

请在分析中考虑一审答复策略和效果，说明哪些论点有效、哪些需调整。
```

路由注册：
```rust
.route("/api/oa/history/:patent_number/diff", get(routes::api_oa_history_diff))
```

#### 前端

在一审二审 diff 区域：
1. OA 分析时自动调用 `/api/oa/history/:pn/diff`
2. 差异对比 UI：驳回理由变化（新增标红、消失标绿）、对比文件增减、权利要求修改标黄
3. 历史 OA 文本注入 AI 分析 prompt

#### i18n keys（前缀 `oa.diff.`）
- `oa.diff.title` = "一审-二审对比分析" / "First-Second Examination Diff"
- `oa.diff.newReasons` = "新增驳回理由" / "New Rejection Reasons"
- `oa.diff.overcome` = "已克服问题" / "Overcome Issues"
- `oa.diff.refChange` = "对比文件变化" / "Reference Changes"
- `oa.diff.noHistory` = "无历史 OA 记录" / "No History OA Records"

#### 验收
- [ ] 同专利号多轮 OA 时自动关联并显示差异
- [ ] 新增驳回理由标红
- [ ] 无历史记录时优雅降级
- [ ] 历史上下文能注入 AI 分析

---

### T4：答复期限管理增强

**分支**：`feat/oa-deadline-enhance`

#### 现有基础设施

- `initDeadline()` 已存在（`templates/office_action_response.html:2157`）
- `updateDeadline()` 已存在，支持一审/二审 4 月、复审 3 月
- 已有进度条和颜色警告

#### 需要增强

1. **二审特殊期限**：二审驳回答复期限是 **2 个月**（非 4 个月），需修正
2. **首页提醒**：`templates/index.html` 增加临期 OA 提醒卡片
3. **多 OA 追踪**：多个专利的 OA 在跟踪时，首页显示列表

#### 前端改动

`templates/office_action_response.html`：
- 修正 `updateDeadline()`：`oaType === 'second_rejection' ? 2 : oaType === 'reject_review' ? 3 : 4`

`templates/index.html`：
- 新增"OA 答复到期提醒"区块
- 从 localStorage 读取所有跟踪的 OA 期限
- 按紧急程度排序，<30 天红色，<60 天黄色

关键 JS：
```javascript
function renderOaDeadlineReminders() {
    // 1. 从 localStorage 读取所有 oa_date 记录
    // 2. 计算剩余天数
    // 3. 按紧急程度排序渲染卡片
}
```

#### i18n keys（前缀 `oa.deadline.`）
- `oa.deadline.reminder` = "OA 答复到期提醒" / "OA Response Deadline Reminder"
- `oa.deadline.urgent` = "紧急" / "Urgent"
- `oa.deadline.expired` = "已逾期" / "Expired"

#### 验收
- [ ] 二审期限正确显示为 2 个月
- [ ] 首页显示 OA 到期提醒
- [ ] <30 天红色警告
- [ ] 不破坏现有期限功能


---

### T5：集成 + 验证 + 提交

**分支**：`feat/oa-phase1-integrate`
**前置**：T1-T4 全部完成

#### 步骤

1. 合并所有分支：
   ```bash
   git merge feat/oa-fetch-refs feat/oa-mechanism-compare feat/oa-history-diff feat/oa-deadline-enhance
   ```

2. 统一注册路由（`src/common.rs`）：
   - T1 的 `/api/ai/oa-fetch-refs`
   - T3 的 `/api/oa/history/:patent_number/diff`

3. 更新函数基线：
   ```bash
   node check_html_functions.mjs --refresh
   ```

4. 验证：
   ```bash
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo test
   node check_html_functions.mjs
   ```

5. 端到端测试：用 CN115076688A 做完整 OA 流程

6. 更新版本号：`Cargo.toml` → `0.9.6`，更新 `CHANGELOG.md`

7. 提交并推送

---

## 四、并行执行时序

```
时间 →  Day1   Day2   Day3   Day4   Day5
T1      ├──────┤      │      │      │
T2      ├──────┼──────┤      │      │
T3      │      ├──────┤      │      │
T4      │      │      ├──────┤      │
T5      │      │      │      ├──────┤
```

T1/T2/T3/T4 可完全并行（改不同文件/不同区域）。T5 等全部完成。

---

## 五、AI Agent 认领规则

1. **认领前**：在 `docs/plans/STATUS.md` 标注 `TASK-TX: 认领 by Agent-XXX`
2. **开发中**：在自己分支开发，不直接 push 到 main
3. **完成后**：在 STATUS.md 标注 `TASK-TX: 完成 by Agent-XXX (commit-hash)`
4. **代码规范**：遵守 `AGENTS.md` 全部规范（fmt/clippy/test/i18n/DOMPurify）
5. **不碰共享文件**：`src/common.rs` 和 `docs/functions-manifest.json` 由 T5 统一改

---

## 六、各任务独立验证方法

每个任务完成后可独立验证（不需要等其他任务）：

```bash
# 后端验证
cargo fmt --check
cargo clippy -- -D warnings
cargo test

# 前端验证
node check_html_functions.mjs

# 功能验证（手动）
# 1. 启动服务：cargo run
# 2. 打开 OA 答复页
# 3. 测试该任务的新功能
```
