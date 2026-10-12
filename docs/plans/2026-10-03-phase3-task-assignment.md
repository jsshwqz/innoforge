# 第三阶段分工计划书 — v0.9.9

> 创建日期：2026-10-03
> 目标版本：v0.9.9
> 前置条件：第二阶段 v0.9.7 集成完成
> 功能主题：智能决策支持

---

## 任务总览

| 任务ID | 功能 | 工时 | 可并行 |
|--------|------|------|--------|
| P3-T1 | 答复策略智能推荐 | 2天 | 独立 |
| P3-T2 | 权利要求修改模拟器 | 3天 | 独立 |
| P3-T3 | 答复质量量化评估 | 2天 | 独立 |
| P3-T4 | 集成+验证+提交 | 0.5天 | 依赖 P3-T1~T3 |

---

## P3-T1：答复策略智能推荐

**分支**：`feat/oa-strategy-recommend`
**预计**：2 天

### 需求

根据 OA 类型、对比文献特征、专利技术领域，自动推荐最优答复策略。

### 后端

新建 `src/ai/patent.rs` 函数：

```rust
fn build_strategy_recommend_prompt(
    oa_type: &str,        // OA 类型（first_rejection/second_rejection 等）
    my_patent: &str,
    refs: &str,           // 对比文献
    oa_text: &str,
) -> (String, String) {
    // 系统提示：你是专利答复策略专家
    // 分析维度：
    // 1. OA 类型 → 答复紧迫度（一审 vs 二审 vs 复审）
    // 2. 对比文献数量和相关性 → 翻案难度
    // 3. 权利要求 vs 对比文献 → 哪些权项有救
    // 4. 技术领域 → 该领域审查标准
    // 输出：
    // - 推荐策略（修改权项 / 增加从属权 / 争辩创造性 / 组合策略）
    // - 每个策略的成功概率估计
    // - 策略优先级排序
    // - 不推荐的策略及原因
}
```

新增 handler `src/routes/ai.rs`：

```rust
/// POST /api/ai/oa-strategy-recommend
/// 请求: { oa_type, my_patent, refs, oa_text }
/// 响应: { strategies: [{ name, probability, priority, reason, not_recommended: [{name, reason}] }] }
pub async fn api_ai_oa_strategy_recommend(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value>
```

路由注册 `src/common.rs`：
```rust
.route("/api/ai/oa-strategy-recommend", post(routes::api_ai_oa_strategy_recommend))
```

### 前端

在 `templates/office_action_response.html`：
1. OA 分析前显示"💡 智能推荐策略"按钮
2. 点击后调用 API，展示策略卡片列表
3. 每个策略卡片：名称、成功概率（进度条）、优先级（星标）、推荐理由
4. 不推荐策略灰色折叠显示
5. 用户选择策略后，自动填充到分析 prompt 中

关键 JS：
```javascript
async function recommendStrategy() {
    // 收集 OA 类型、专利文本、对比文献
    // POST /api/ai/oa-strategy-recommend
    // 渲染策略卡片
}

function selectStrategy(strategyName) {
    // 将选中策略注入分析上下文
    // 高亮选中卡片
}
```

### i18n keys（前缀 `oa.strategy.`）
- `oa.strategy.btn` = "智能推荐策略" / "Recommend Strategy"
- `oa.strategy.probability` = "成功概率" / "Success Probability"
- `oa.strategy.priority` = "优先级" / "Priority"
- `oa.strategy.reason` = "推荐理由" / "Reason"
- `oa.strategy.notRecommended` = "不推荐" / "Not Recommended"
- `oa.strategy.select` = "采用此策略" / "Use This Strategy"

### 验收
- [ ] 能根据 OA 类型推荐不同策略
- [ ] 每个策略有成功概率估计
- [ ] 策略按优先级排序
- [ ] 不推荐策略有明确原因
- [ ] 选择策略后能注入分析流程

---

## P3-T2：权利要求修改模拟器

**分支**：`feat/oa-claim-simulator`
**预计**：3 天

### 需求

用户输入修改后的权利要求，AI 模拟审查员审查，判断是否可能授权。

### 后端

新建 handler：

```rust
/// POST /api/ai/oa-claim-simulate
/// 请求: { original_claims, modified_claims, refs, oa_text }
/// 响应: { 
///   support_check: String,      // 修改是否有说明书支持
///   novelty_check: String,      // 修改后是否新颖
///   inventiveness_check: String,// 修改后是否创造性
///   scope_check: String,        // 保护范围评估
///   overall: String,            // 总体判断
///   suggestions: String         // 进一步修改建议
/// }
pub async fn api_ai_oa_claim_simulate(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value>
```

Prompt 设计：
```rust
fn build_claim_simulate_prompt(
    original: &str,
    modified: &str,
    refs: &str,
    oa: &str,
) -> (String, String) {
    // 系统提示：你是专利审查员，申请人提交了修改后的权利要求
    // 请逐项检查：
    // 1. 说明书支持：修改后的权项是否能在说明书中找到依据？
    // 2. 新颖性：对比 D1/D2/...，修改后是否新颖？
    // 3. 创造性：对比文件组合能否显而易见得到？
    // 4. 保护范围：修改后范围是否合理？是否过度缩小？
    // 5. 总体判断：可能授权 / 需要进一步修改 / 仍有问题
    // 6. 修改建议：如果需要进一步修改，具体建议
}
```

### 前端

在 `templates/office_action_response.html` 新增"🔧 权利要求修改模拟器"区域：
1. 左右分栏：左侧原始权利要求（只读），右侧修改后权利要求（可编辑）
2. "模拟审查"按钮
3. 结果区域：5 个检查项各一个卡片，通过/不通过用颜色标注
4. 总体判断大字号显示
5. 修改建议折叠区

### i18n keys（前缀 `oa.claimSim.`）
- `oa.claimSim.title` = "权利要求修改模拟器" / "Claim Modification Simulator"
- `oa.claimSim.original` = "原始权利要求" / "Original Claims"
- `oa.claimSim.modified` = "修改后权利要求" / "Modified Claims"
- `oa.claimSim.simulate` = "模拟审查" / "Simulate Examination"
- `oa.claimSim.support` = "说明书支持" / "Specification Support"
- `oa.claimSim.novelty` = "新颖性" / "Novelty"
- `oa.claimSim.inventiveness` = "创造性" / "Inventiveness"
- `oa.claimSim.scope` = "保护范围" / "Protection Scope"
- `oa.claimSim.overall` = "总体判断" / "Overall Assessment"
- `oa.claimSim.suggestions` = "修改建议" / "Suggestions"

### 验收
- [ ] 能输入修改后的权利要求
- [ ] 5 项检查各有明确结论
- [ ] 通过/不通过有颜色区分
- [ ] 给出进一步修改建议
- [ ] 总体判断明确

---

## P3-T3：答复质量量化评估

**分支**：`feat/oa-quality-score`
**预计**：2 天

### 需求

对生成的答复文本进行多维度量化评分，帮助用户判断答复质量。

### 后端

新增 handler：

```rust
/// POST /api/ai/oa-quality-score
/// 请求: { response_text, oa_text, my_patent }
/// 响应: {
///   scores: {
///     logic: f32,        // 逻辑严密性 0-10
///     evidence: f32,     // 证据引用充分性 0-10
///     distinction: f32,  // 区别技术特征论证 0-10
///     modification: f32, // 修改合理性 0-10
///     tone: f32,         // 措辞专业度 0-10
///     completeness: f32  // 回复完整性 0-10
///   },
///   total: f32,           // 加权总分
///   grade: String,        // 等级 A/B/C/D
///   weaknesses: String,   // 薄弱点分析
///   suggestions: String   // 改进建议
/// }
pub async fn api_ai_oa_quality_score(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value>
```

Prompt 设计：
```rust
fn build_quality_score_prompt(
    response: &str,
    oa: &str,
    patent: &str,
) -> (String, String) {
    // 系统提示：你是专利答复质量评审专家
    // 请从 6 个维度评分（每项 0-10 分）：
    // 1. 逻辑严密性：论证链条是否完整，有无跳跃
    // 2. 证据引用充分性：是否引用了具体段落、附图
    // 3. 区别技术特征论证：是否清晰区分了本申请与对比文件
    // 4. 修改合理性：权利要求修改是否恰当
    // 5. 措辞专业度：用语是否符合专利答复规范
    // 6. 回复完整性：是否回应了所有驳回理由
    // 输出 JSON 格式评分 + 薄弱点分析 + 改进建议
}
```

### 前端

在答复生成后显示"📊 质量评估"按钮：
1. 点击后调用 API
2. 雷达图展示 6 维评分（用 CSS 画或 SVG）
3. 总分 + 等级（A/B/C/D）大字号
4. 薄弱点列表
5. 改进建议

### i18n keys（前缀 `oa.quality.`）
- `oa.quality.btn` = "质量评估" / "Quality Score"
- `oa.quality.total` = "总分" / "Total Score"
- `oa.quality.grade` = "等级" / "Grade"
- `oa.quality.logic` = "逻辑严密性" / "Logic"
- `oa.quality.evidence` = "证据引用" / "Evidence"
- `oa.quality.distinction` = "区别特征论证" / "Distinction"
- `oa.quality.modification` = "修改合理性" / "Modification"
- `oa.quality.tone` = "措辞专业度" / "Tone"
- `oa.quality.completeness` = "回复完整性" / "Completeness"
- `oa.quality.weaknesses` = "薄弱点" / "Weaknesses"
- `oa.quality.suggestions` = "改进建议" / "Suggestions"

### 验收
- [x] 6 维度各有 0-10 评分
- [x] 雷达图正确展示
- [x] 总分和等级合理
- [x] 薄弱点分析具体
- [x] 改进建议可操作

### ✅ 完成记录（Agent-2 / AI-2，commit `5887599`）

- **分支**：main（工作区已有其他AI写入的后端代码，直接在此基础上补完前端）
- **改动**：4 文件，+549 -2 行
  - `src/routes/ai.rs`：`api_ai_oa_quality_score` handler（其他AI已写）
  - `src/common.rs`：路由注册（其他AI已写）
  - `static/i18n.js`：`oa.quality.*` 中英文 key（其他AI已写）
  - `templates/office_action_response.html`：**本次新增** `qualityScore()` JS 函数 + `renderRadarChart()` SVG 雷达图 + 评分卡片 UI + 修复 defenseAnalysis() 内误插入的按钮 HTML bug
- **验证**：cargo fmt ✅ / cargo clippy ✅ / cargo test 37 passed ✅ / HTML 函数扫描 ✅
- **未完成事项**：无

---

## 附录：T3 一审-二审对比分析 — 交接记录

> 原 `2026-10-03-T3-handoff.md`，已合并至本文档并删除原文件。
> **交接时间**: 2026-10-03
> **负责人**: AI-2 (Architecture Partner)
> **状态**: ✅ 后端+前端代码完成并推送，遗留工作已由主AI补完（commit `1a07c2e`）

### A.1 任务概述

**T3 一审-二审对比分析**：当专利有多轮 OA 时，自动关联历史记录，展示一审/二审差异，并将历史上下文注入 AI 分析 prompt 用于二审驳回答复。

### A.2 已完成的工作

#### A.2.1 后端 — diff API 端点

**新增文件**: `src/routes/oa_diff.rs`（258 行）

- 端点: `GET /api/oa/history/:patent_number/diff`
- 逻辑: 查询同一专利号的所有历史 OA 记录（按版本排序），生成相邻轮次差异对比
- 差异维度: OA 类型变化、分析深度变化、驳回理由新增/消失、对比文件增减、权利要求变化
- 返回 `first_exam_context` 字段：一审分析摘要，供前端注入 AI prompt
- 辅助函数: `extract_rejection_reasons`, `extract_reference_numbers`, `extract_claim_changes`, `safe_preview`

**路由注册**: `src/common.rs` 添加 `.route("/api/oa/history/:patent_number/diff", get(routes::api_oa_history_diff))`
**模块注册**: `src/routes/mod.rs` 添加 `mod oa_diff;` 和 `pub use oa_diff::*;`

#### A.2.2 AI Prompt 注入

**修改文件**: `src/ai/patent.rs`

- `build_second_rejection_prompt` 签名新增 `first_exam_context: Option<&str>` 参数
- 新增 `build_first_exam_context_section` 辅助方法
- 两个调用点均已更新（非流式 + SSE 流式）

**修改文件**: `src/routes/ai.rs` — 两个 handler 提取 `first_exam_context` 字段并透传

#### A.2.3 前端 UI

**修改文件**: `templates/office_action_response.html`

- `doOAAnalysis()` 中二审驳回时自动调用 diff API
- 新增 `renderOaDiff(diffs)` 函数：差异对比 UI（新增驳回理由标红、已克服问题标绿）
- 使用 `DOMPurify.sanitize()` 做 XSS 防护

#### A.2.4 i18n

**修改文件**: `static/i18n.js` — 新增 `oa.diff.*` 系列 keys（中英文）

#### A.2.5 测试

- `tests/ai_smoke_test.rs`：调用签名更新
- **新增 15 个单元测试**（主AI补完，commit `1a07c2e`）：辅助函数 + API 端点全覆盖

#### A.2.6 已通过的验证

cargo check ✅ | cargo clippy -D warnings ✅ | cargo test 全通过 ✅ | cargo fmt ✅ | HTML 扫描 ✅ | ESLint 0 errors ✅

### A.3 原未完成的工作（已全部补完）

| 遗留事项 | 状态 | 补完者 | Commit |
|----------|------|--------|--------|
| CHANGELOG.md 未更新 | ✅ 已补 | 主AI | `1a07c2e` |
| 无 oa_diff 单元测试 | ✅ 已补（15 个测试） | 主AI | `1a07c2e` |
| HTML 函数完整性扫描未跑 | ✅ 已跑+基线刷新 | 主AI | `1a07c2e` |
| ESLint 检查未跑 | ✅ 已跑（0 errors） | 主AI | `1a07c2e` |
| Puppeteer e2e 未跑 | ⏭ 环境未安装，按规约跳过 | — | — |
| 端到端手动测试 | ⏭ 需浏览器环境 | — | — |

### A.4 重要注意事项

#### A.4.1 签名变更影响

`build_second_rejection_prompt` 和 `office_action_response` / `office_action_response_stream` 的签名都多了一个参数 `first_exam_context: Option<&str>`。如果其它 agent 也要改这些函数的调用点，注意要传此参数（通常传 `None`）。

受影响的调用路径：
```
api_ai_office_action_response (ai.rs) → office_action_response (patent.rs) → build_second_rejection_prompt
api_ai_office_action_response_stream (ai.rs) → office_action_response_stream (patent.rs) → build_second_rejection_prompt
```

#### A.4.2 rebase 冲突说明

T3 commit `aa8d019` 在 rebase 时与远程已有的其它 agent 提交有冲突，已解决。冲突点:
- `src/ai/patent.rs`: T2 的 `{mech}` 占位符与 T3 的 `{}` + `first_exam_context` 参数 → 合并保留两者
- `static/i18n.js`: 其它 agent 的 `oa.panel.*` / `oa.deadline.*` keys 与 T3 的 `oa.diff.*` keys → 全部保留
- `docs/plans/STATUS.md`: 任务看板更新 → 合并

#### A.4.3 已知问题

- 远程 `4010c0a` 提交标题含 "T3"，可能是另一个 agent 也做了 T3 的部分实现。以 `src/routes/oa_diff.rs` 为标志。
- 前端 `renderOaDiff` 中专利号提取逻辑：正则匹配 `(CN|US|EP|JP|KR|WO)\d{7,12}`，格式不标准时静默 catch 不报错。

### A.5 文件清单

| 文件 | 状态 | 说明 |
|------|------|------|
| `src/routes/oa_diff.rs` | 新增 | diff API 端点 + 15 单元测试 |
| `src/routes/mod.rs` | 修改 | 注册 oa_diff 模块 |
| `src/common.rs` | 修改 | 注册路由 |
| `src/ai/patent.rs` | 修改 | prompt 注入 first_exam_context |
| `src/routes/ai.rs` | 修改 | 两个 handler 提取 first_exam_context |
| `templates/office_action_response.html` | 修改 | 前端 diff UI + 自动获取 |
| `static/i18n.js` | 修改 | 新增 oa.diff.* keys |
| `tests/ai_smoke_test.rs` | 修改 | 测试签名更新 |
| `CHANGELOG.md` | 修改 | v0.9.6 版本段（主AI补完） |
| `docs/functions-manifest.json` | 修改 | HTML 基线刷新（主AI补完） |
