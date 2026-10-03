# 第二阶段分工计划书 — v0.9.7 并行开发指南

> 创建日期：2026-10-03
> 目标版本：v0.9.7
> 前置条件：第一阶段 v0.9.6 集成完成

---

## 一、任务总览

| 任务ID | 功能 | 工时 | 可并行 |
|--------|------|------|--------|
| P2-T1 | 多角色 AI 会诊面板 | 3天 | 后端独立 |
| P2-T2 | 模拟审查员预判 | 1天 | 独立 |
| P2-T3 | 对方视角防御 | 2天 | 独立 |
| P2-T4 | 集成+验证+提交 | 0.5天 | 依赖 P2-T1~T3 |

---

## 二、任务详细规格

### P2-T2：模拟审查员预判（推荐给空闲 AI）

**分支**：`feat/oa-examiner-preview`
**预计**：1 天

#### 后端

新建 prompt 函数 `src/ai/patent.rs`：

```rust
fn build_examiner_preview_prompt(
    my_patent: &str,
    oa: &str,
    response: &str,  // 答复文本
) -> (String, String) {
    // 系统提示：你是中国专利审查员，刚收到申请人对你发出的驳回决定的答复
    // 请以审查员视角预判：
    // 1. 哪些论点你可能接受（标绿）
    // 2. 哪些论点你可能仍要反驳（标红），给出反驳理由
    // 3. 下一轮你可能提出的新驳回理由
    // 4. 总体预判：可能授权 / 可能部分授权 / 可能维持驳回
}
```

新增 handler `src/routes/ai.rs`：

```rust
/// POST /api/ai/oa-examiner-preview
/// 请求: { my_patent, oa_text, response_text }
/// 响应: { preview: String }
pub async fn api_ai_oa_examiner_preview(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value>
```

路由注册 `src/common.rs`：
```rust
.route("/api/ai/oa-examiner-preview", post(routes::api_ai_oa_examiner_preview))
```

#### 前端

在 `templates/office_action_response.html` 分析结果区域：
1. 答复生成后显示"🔍 模拟审查员预判"按钮
2. 点击后调用 API，显示预判结果
3. 三色标注：可能接受（绿）、可能反驳（红）、下一轮预测（黄）

关键 JS：
```javascript
async function examinerPreview() {
    var patent = getMyPatent();
    var oa = getOA();
    var response = getLastResponseText();
    // POST /api/ai/oa-examiner-preview
    // 渲染结果
}
```

#### i18n keys（前缀 `oa.preview.`）
- `oa.preview.btn` = "模拟审查员预判" / "Examiner Preview"
- `oa.preview.accept` = "可能接受" / "May Accept"
- `oa.preview.reject` = "可能反驳" / "May Reject"
- `oa.preview.nextRound` = "下一轮预测" / "Next Round Prediction"
- `oa.preview.overall` = "总体预判" / "Overall Prediction"

#### 验收
- [ ] 答复生成后能点击预判按钮
- [ ] 预判结果区分"可能接受"和"可能反驳"
- [ ] 给出下一轮可能新驳回理由
- [ ] 总体预判有明确结论

---

### P2-T3：对方视角防御

**分支**：`feat/oa-defense-analysis`
**预计**：2 天

#### 后端

新建 prompt 函数 `src/ai/patent.rs`：

```rust
fn build_defense_analysis_prompt(
    my_patent: &str,
    claims: &str,  // 权利要求
) -> (String, String) {
    // 系统提示：你是对方律师，目标是无效这件专利
    // 三个子分析并行：
    // 1. 无效宣告预判：权利要求有哪些潜在漏洞？
    // 2. 侵权可执行性：授权后是否容易检测侵权？
    // 3. 商业价值评估：保护范围是否太窄无商业价值？
}
```

新增 handler + 路由：
```rust
/// POST /api/ai/oa-defense-analysis
/// 请求: { my_patent, claims }
/// 响应: { invalidation, infringement, commercial }
pub async fn api_ai_oa_defense_analysis(...)
```

#### 前端

在分析结果区域增加"🛡️ 防御分析"标签页：
1. 无效宣告预判（折叠区块）
2. 侵权可执行性（折叠区块）
3. 商业价值评估（折叠区块）

#### i18n keys（前缀 `oa.defense.`）
- `oa.defense.title` = "对方视角防御分析" / "Defense Analysis"
- `oa.defense.invalidation` = "无效宣告预判" / "Invalidation Preview"
- `oa.defense.infringement` = "侵权可执行性" / "Infringement Enforceability"
- `oa.defense.commercial` = "商业价值评估" / "Commercial Value"

#### 验收
- [ ] 能从对方律师视角分析权利要求漏洞
- [ ] 侵权可执行性评估检测难度
- [ ] 商业价值评估权衡保护范围与授权概率
