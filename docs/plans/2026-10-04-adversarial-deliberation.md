# 多轮对抗推演 — OA 答复质变升级规划

> **创建时间**: 2026-10-04
> **版本目标**: v0.10.0
> **状态**: 规划中
> **总指挥**: IaC Architecture Partner

---

## 一、背景与动机

### 1.1 当前系统的根本局限

v0.9.10 已完成 12 个 OA 增强功能，全部经过真实 API 调用验证。但存在一个**架构层面的局限**：

> 专利审查是一个**博弈问题**，不是一个方向的表达问题。

现有功能（包括多角色会诊 #1、审查员预判 #6、对方防御 #10）都是**单次独立调用**：
- 每个角色调一次 AI，各自输出独立分析
- 角色之间**没有交互、没有迭代**
- 审查员提出质疑后，代理师不会回应；代理师回应后，审查员不会再追问

这就像下棋只走一步就结束——真正的博弈需要**来回交锋**，在压力测试中打磨出经得起审查的答复。

### 1.2 目标专利

用一个真实的二审驳回案做端到端驱动测试（案件数据经 data/ 目录注入，不入库、不进 git）
- 答复期限紧迫
- 驳回理由：权利要求 1 相对 D1 缺乏创造性
- 区别特征：辅助火焰通道设有预热段
- 审查员认为：该区别属于本领域常规技术手段

### 1.3 核心转变

```
现有: 单向分析 → 输出答复 → 完
目标: 审查员质疑 → 代理师回应 → 审查员追问 → 代理师完善 → ... → 压力测试通过的终稿
```

---

## 二、功能设计

### 2.1 多轮对抗推演引擎

**端点**: `POST /api/ai/oa-adversarial`

**输入参数**:
```json
{
  "my_patent": "专利全文",
  "claims": "权利要求书",
  "oa_text": "审查意见全文",
  "refs": "对比文件摘要",
  "rounds": 3,
  "focus": "创造性"
}
```

**输出**:
```json
{
  "status": "ok",
  "transcript": [
    {
      "round": 1,
      "role": "examiner",
      "content": "审查员质疑内容",
      "challenges": ["质疑点1", "质疑点2"],
      "severity": "high"
    },
    {
      "round": 1,
      "role": "agent",
      "content": "代理师回应",
      "arguments": ["论点1", "论点2"],
      "evidence_used": ["证据1"],
      "confidence": 0.75
    }
  ],
  "final_response": "经过压力测试的终稿答复书",
  "confidence_score": 0.82,
  "weak_points": ["仍存在的弱点"],
  "rounds_completed": 3,
  "converged": true
}
```

### 2.2 对抗流程

```
第1轮:
  审查员 → 分析驳回理由，提出核心质疑（2-3个挑战点）
  代理师 → 针对每个质疑点逐一回应，引用证据，构建论证链

第2轮:
  审查员 → 审查代理师回应，找出薄弱环节，追问
  代理师 → 补强薄弱环节，可能修改权利要求，重新论证

第3轮:
  审查员 → 最终审查，如果代理师回应充分则收敛
  代理师 → 整合三轮论证，输出终稿

收敛判定:
  - 审查员第N轮的质疑严重程度 <= 第N-1轮 → 收敛
  - 或达到最大轮数 → 强制结束，标注未收敛弱点
```

### 2.3 上下文管理

多轮对话的上下文会快速增长。策略：

- **每轮摘要**: 每轮结束后，用一次额外 AI 调用生成 200 字摘要
- **累积上下文**: 第 N 轮的输入 = 原始材料 + 前 N-1 轮的摘要（不是全文）
- **最终整合**: 最后一轮不受摘要限制，能看到所有轮次的完整内容

上下文预算（以 4K 模型为例）:
```
原始材料:    ~2000 tokens
每轮摘要:    ~300 tokens x 5轮 = 1500 tokens
当前轮对话:  ~500 tokens
总计:        ~4000 tokens（在预算内）
```

### 2.4 Prompt 设计

**审查员 System Prompt**（每轮通用，动态注入上文摘要）:
```
你是一位严谨的中国专利审查员，正在审查申请人针对你发出的驳回决定的答复。

这是第 {round} 轮对抗。前 {round-1} 轮的摘要如下：
{previous_rounds_summary}

你的任务：
1. 仔细阅读申请人的最新回应
2. 找出论证中的薄弱环节（逻辑跳跃、证据缺失、法条误用）
3. 提出具体的追问，每个追问指向一个具体弱点
4. 评估申请人的回应在多大程度上回应了你的质疑

输出格式：
- 质疑点（2-3个，每个标注严重程度: high/medium/low）
- 具体追问
- 本轮评估：申请人回应的充分程度（0-100%）
```

**代理师 System Prompt**:
```
你是一位资深专利代理师，正在为申请人撰写答复意见书。

这是第 {round} 轮对抗。审查员在前一轮提出了以下质疑：
{examiner_challenges}

前 {round-1} 轮的摘要如下：
{previous_rounds_summary}

你的任务：
1. 针对审查员的每个质疑，构建严密的论证链
2. 论证链结构：区别特征 → 技术作用 → 技术效果 → 非显而易见性
3. 引用具体证据（说明书段落、对比文件缺失内容）
4. 如果当前权利要求无法克服质疑，提出修改建议
5. 评估你自己本轮回应的信心度（0-100%）
```

**终稿整合 Prompt**（最后一轮后调用）:
```
你是资深专利代理师。经过 {rounds} 轮与审查员的对抗推演，以下是完整对话记录：
{full_transcript}

请整合所有轮次的论证，撰写一份完整的、可以直接提交的答复意见书。

要求：
1. 格式符合正式答复意见书规范
2. 论证逻辑连贯，不重复
3. 对审查员提出的所有质疑都已回应
4. 标注仍存在的风险点（诚实披露）
5. 按可直接提交成稿生成、提交前请人工核对事实与程序信息
```

---

## 三、前端设计

### 3.1 对抗推演面板

在 `templates/office_action_response.html` 新增区域，位于现有功能下方：

- 标题：多轮对抗推演
- 控件：对抗轮数下拉框(1-5, 默认3) + 焦点问题输入框 + 开始推演按钮
- 第1轮区域：红色审查员质疑框 + 绿色代理师回应框
- 第2轮区域：同上
- 第3轮区域：同上
- 终稿区域：综合信心度 + 收敛状态 + 完整答复书 + 残留风险点 + 复制/导出按钮

### 3.2 交互流程

1. 用户填入 OA 文本、专利文本、权利要求（复用现有表单）
2. 选择对抗轮数（默认3，可选1-5）
3. 点击"开始推演"
4. 前端显示进度：第1轮 审查员分析中... → 代理师回应中... → 第2轮...
5. 每轮完成后实时显示（不需要等全部完成）
6. 全部完成后显示终稿 + 信心度 + 风险点
7. 可"复制"或"导出"终稿

### 3.3 实时显示

使用流式响应（SSE 或分块返回）：
- 后端每完成一个角色的一轮，立即推送该轮结果
- 前端逐轮追加显示，不需要等全部完成
- 每轮之间有视觉分隔，像聊天记录

---

## 四、后端实现规格

### 4.1 新增文件

| 文件 | 用途 | 大小估计 |
|------|------|---------|
| `src/ai/adversarial.rs` | 对抗推演引擎核心 | ~400行 |

### 4.2 修改文件

| 文件 | 改动 | 说明 |
|------|------|------|
| `src/routes/ai.rs` | 新增 `api_ai_oa_adversarial` handler | ~80行 |
| `src/common.rs` | 注册路由 `POST /api/ai/oa-adversarial` | 1行 |
| `templates/office_action_response.html` | 新增对抗推演面板 UI + JS | ~200行 |
| `static/i18n.js` | 新增 `oa.adversarial.*` keys | ~30行 |

### 4.3 核心函数签名

```rust
// src/ai/adversarial.rs

/// 执行多轮对抗推演
pub async fn run_adversarial_deliberation(
    ai: &dyn AiClient,
    params: &AdversarialParams,
) -> Result<AdversarialResult, AiError>;

/// 单轮对抗：审查员质疑
async fn examiner_challenge(
    ai: &dyn AiClient,
    round: usize,
    previous_summary: &str,
    agent_last_response: &str,
    params: &AdversarialParams,
) -> Result<ExaminerTurn, AiError>;

/// 单轮对抗：代理师回应
async fn agent_respond(
    ai: &dyn AiClient,
    round: usize,
    previous_summary: &str,
    examiner_challenges: &ExaminerTurn,
    params: &AdversarialParams,
) -> Result<AgentTurn, AiError>;

/// 轮次摘要（控制上下文增长）
async fn summarize_round(
    ai: &dyn AiClient,
    examiner: &ExaminerTurn,
    agent: &AgentTurn,
) -> Result<String, AiError>;

/// 终稿整合
async fn synthesize_final(
    ai: &dyn AiClient,
    transcript: &[RoundRecord],
    params: &AdversarialParams,
) -> Result<String, AiError>;

/// 收敛判定
fn check_convergence(current_severity: &str, previous_severity: &str) -> bool;
```

### 4.4 数据结构

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdversarialParams {
    pub my_patent: String,
    pub claims: String,
    pub oa_text: String,
    pub refs: String,
    pub rounds: usize,
    pub focus: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExaminerTurn {
    pub round: usize,
    pub content: String,
    pub challenges: Vec<String>,
    pub severity: String,    // high/medium/low
    pub assessment: f32,     // 0.0-1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurn {
    pub round: usize,
    pub content: String,
    pub arguments: Vec<String>,
    pub evidence_used: Vec<String>,
    pub confidence: f32,     // 0.0-1.0
    pub modification: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdversarialResult {
    pub transcript: Vec<RoundRecord>,
    pub final_response: String,
    pub confidence_score: f32,
    pub weak_points: Vec<String>,
    pub rounds_completed: usize,
    pub converged: bool,
}
```

### 4.5 超时与降级

- **单轮超时**: 每个 AI 调用 60 秒上限（遵循 AGENTS.md 2.6）
- **总超时**: 3轮约需 7 次 AI 调用（3x审查员+3x代理师+1x整合），约 7 分钟
- **降级**: 如果某一轮 AI 调用失败，保留已完成轮次，标注"推演中断于第N轮"，输出已完成的最佳版本
- **上下文溢出**: 如果某轮 AI 返回上下文超长错误，自动缩减前文摘要后重试

---

## 五、i18n Keys

```javascript
// 中文
"oa.adversarial.title": "多轮对抗推演",
"oa.adversarial.rounds": "对抗轮数",
"oa.adversarial.focus": "焦点问题",
"oa.adversarial.start": "开始推演",
"oa.adversarial.progress": "第{round}轮 {role}分析中...",
"oa.adversarial.examiner": "审查员质疑",
"oa.adversarial.agent": "代理师回应",
"oa.adversarial.confidence": "信心度",
"oa.adversarial.final": "终稿答复书",
"oa.adversarial.weakPoints": "残留风险点",
"oa.adversarial.converged": "已收敛",
"oa.adversarial.notConverged": "未完全收敛",
"oa.adversarial.severity.high": "严重",
"oa.adversarial.severity.medium": "中等",
"oa.adversarial.severity.low": "轻微",
"oa.adversarial.copy": "复制终稿",
"oa.adversarial.export": "导出",

// English
"oa.adversarial.title": "Adversarial Deliberation",
"oa.adversarial.rounds": "Rounds",
"oa.adversarial.focus": "Focus Issue",
"oa.adversarial.start": "Start Deliberation",
"oa.adversarial.progress": "Round {round}: {role} analyzing...",
"oa.adversarial.examiner": "Examiner Challenge",
"oa.adversarial.agent": "Agent Response",
"oa.adversarial.confidence": "Confidence",
"oa.adversarial.final": "Final Response",
"oa.adversarial.weakPoints": "Remaining Weak Points",
"oa.adversarial.converged": "Converged",
"oa.adversarial.notConverged": "Not Fully Converged",
"oa.adversarial.severity.high": "High",
"oa.adversarial.severity.medium": "Medium",
"oa.adversarial.severity.low": "Low",
"oa.adversarial.copy": "Copy Final",
"oa.adversarial.export": "Export",
```

---

## 六、任务分解与派工

### 6.1 任务清单

| 任务ID | 内容 | 估时 | 依赖 |
|--------|------|------|------|
| AD-T1 | 后端对抗引擎 `src/ai/adversarial.rs` | 2天 | 无 |
| AD-T2 | 后端路由 + handler `src/routes/ai.rs` | 0.5天 | AD-T1 |
| AD-T3 | 前端对抗面板 UI + JS | 1天 | AD-T2 |
| AD-T4 | i18n keys | 0.5天 | 无 |
| AD-T5 | 集成测试 + 真实API验证 | 1天 | AD-T1~T4 |
| AD-T6 | HTML函数完整性扫描 + 基线更新 | 0.5天 | AD-T3 |

**总工期**: 5.5 个工作日

### 6.2 派工建议

| 任务 | 指派 | 说明 |
|------|------|------|
| AD-T1 | 主AI | 核心引擎，逻辑复杂，需理解博弈流程 |
| AD-T2 | 主AI | 路由注册，与AD-T1接口对齐 |
| AD-T3 | AI-1 | 前端UI，参照现有OA面板风格 |
| AD-T4 | AI-2 | i18n，机械工作 |
| AD-T5 | 总指挥 | 真实API逐个验证，不委托 |
| AD-T6 | AI-1 | 基线更新 |

### 6.3 验收标准

- [ ] `POST /api/ai/oa-adversarial` 端点注册并可调用
- [ ] 输入正确参数返回 `transcript` + `final_response` + `confidence_score`
- [ ] 3轮对抗中每轮有 examiner + agent 两个 turn
- [ ] `final_response` 是整合后的完整答复书（非空、非简单拼接）
- [ ] `confidence_score` 在 0-1 之间
- [ ] `converged` 字段正确反映收敛状态
- [ ] 单轮 AI 失败时降级输出已完成轮次，不 panic
- [ ] 前端有"开始推演"按钮，点击后显示逐轮结果
- [ ] 前端显示终稿 + 信心度 + 风险点
- [ ] i18n 双语 keys 全部存在
- [ ] `cargo fmt` + `cargo clippy -- -D warnings` + `cargo test` 全通过
- [ ] `node check_html_functions.mjs` 通过
- [ ] 生产路径无 `unwrap()` / `expect()`

---

## 七、与现有功能的关系

| 现有功能 | 对抗推演中的角色 |
|---------|----------------|
| #1 多角色会诊 | 会诊的4角色 → 对抗推演只用审查员+代理师，但更深入 |
| #6 审查员预判 | 预判是1轮 → 对抗推演是N轮预判的迭代版 |
| #10 对方防御 | 对方律师视角 → 可作为第4轮的额外挑战者 |
| #5 策略推荐 | 策略推荐 → 代理师每轮可选择策略 |
| #8 质量评分 | 质量评分 → 终稿自动跑一次质量评分 |
| #9 证据收集 | 证据 → 代理师每轮可引用已收集的证据 |

对抗推演不是替代现有功能，而是把它们**串联成一个迭代闭环**。

---

## 八、风险与注意事项

1. **AI 调用成本**: 3轮对抗 = 7次 AI 调用，是单次分析的 7 倍。用户需知晓。
2. **延迟**: 3轮约需 7 分钟，前端必须显示进度，不能让用户干等。
3. **上下文溢出**: 5轮时上下文可能超限，摘要策略是关键。
4. **收敛不保证**: 审查员可能每轮都提出新质疑，3轮不一定收敛。这是正常的——真实审查也不保证一次答复就通过。
5. **AI 角色一致性**: 同一个 AI 模型扮演审查员和代理师，可能出现"自己跟自己下棋"的局限。可通过不同 system prompt 强化角色差异，但根本上这是单模型的局限。
