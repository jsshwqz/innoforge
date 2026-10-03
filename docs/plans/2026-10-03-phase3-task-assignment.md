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
- [ ] 6 维度各有 0-10 评分
- [ ] 雷达图正确展示
- [ ] 总分和等级合理
- [ ] 薄弱点分析具体
- [ ] 改进建议可操作
