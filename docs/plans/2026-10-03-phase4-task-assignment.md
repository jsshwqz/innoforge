# 第四阶段分工计划书 — v0.10.0

> 创建日期：2026-10-03
> 目标版本：v0.10.0
> 前置条件：第三阶段 v0.9.9 集成完成
> 功能主题：全流程追踪与自动化

---

## 任务总览

| 任务ID | 功能 | 工时 | 可并行 |
|--------|------|------|--------|
| P4-T1 | 证据自动收集与整理 | 3天 | 独立 |
| P4-T2 | 多轮答复全流程追踪 | 3天 | 独立 |
| P4-T3 | 集成+验证+提交 | 0.5天 | 依赖 P4-T1~T2 |

---

## P4-T1：证据自动收集与整理

**分支**：`feat/oa-evidence-collector`
**预计**：3 天

### 需求

从专利全文、对比文献、OA 文本中自动提取证据段落，按类型分类整理，生成证据清单。

### 后端

新建 `src/ai/evidence.rs` 模块：

```rust
/// 证据类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvidenceType {
    /// 说明书支持段落
    SpecSupport,
    /// 对比文件区别点
    Distinction,
    /// 技术效果证据
    TechnicalEffect,
    /// 现有技术缺陷
    PriorArtDeficiency,
    /// 实验数据
    ExperimentalData,
}

/// 证据条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub evidence_type: EvidenceType,
    pub source: String,      // 来源（说明书第X段 / D1第Y页 / OA第Z段）
    pub content: String,     // 证据内容
    pub relevance: f32,      // 相关性 0-1
    pub used_in_response: bool, // 是否已用于答复
}

pub async fn collect_evidence(
    ai: &AiClient,
    patent: &str,
    refs: &str,
    oa: &str,
) -> Result<Vec<Evidence>>
```

新增 handler：

```rust
/// POST /api/ai/oa-collect-evidence
/// 请求: { my_patent, refs, oa_text }
/// 响应: { evidence: [Evidence], summary: String }
pub async fn api_ai_oa_collect_evidence(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value>
```

Prompt 设计：
```rust
fn build_evidence_collect_prompt(
    patent: &str,
    refs: &str,
    oa: &str,
) -> (String, String) {
    // 系统提示：你是专利证据收集专家
    // 从以下材料中提取所有可用证据：
    // 1. 说明书支持：哪些段落支持权利要求中的技术特征？
    // 2. 区别点：本申请与对比文件在技术上的具体区别
    // 3. 技术效果：说明书提到的效果数据/实验
    // 4. 现有技术缺陷：对比文件不能解决什么问题？
    // 5. 实验数据：实施例/对比实验
    // 每条证据标注来源、内容、相关性
}
```

### 前端

在 `templates/office_action_response.html` 新增"📁 证据中心"区域：
1. "自动收集证据"按钮
2. 证据列表按类型分组（可折叠）
3. 每条证据：来源标签、内容预览、相关性进度条
4. "用于答复"复选框，勾选后自动插入答复上下文
5. 证据统计：各类型数量、总相关度

### i18n keys（前缀 `oa.evidence.`）
- `oa.evidence.btn` = "自动收集证据" / "Collect Evidence"
- `oa.evidence.title` = "证据中心" / "Evidence Center"
- `oa.evidence.specSupport` = "说明书支持" / "Spec Support"
- `oa.evidence.distinction` = "区别点" / "Distinction"
- `oa.evidence.techEffect` = "技术效果" / "Technical Effect"
- `oa.evidence.priorArt` = "现有技术缺陷" / "Prior Art Deficiency"
- `oa.evidence.experimental` = "实验数据" / "Experimental Data"
- `oa.evidence.relevance` = "相关性" / "Relevance"
- `oa.evidence.useInResponse` = "用于答复" / "Use in Response"
- `oa.evidence.summary` = "证据统计" / "Summary"

### 验收
- [ ] 能从专利/对比文献/OA 中提取证据
- [ ] 证据按 5 种类型分类
- [ ] 每条证据有来源标注
- [ ] 相关性评分合理
- [ ] 勾选后能插入答复上下文

---

## P4-T2：多轮答复全流程追踪

**分支**：`feat/oa-workflow-tracker`
**预计**：3 天

### 需求

追踪一件专利从一审→二审→复审→授权/驳回的完整流程，每轮记录 OA 内容、答复文本、审查结果，形成时间线。

### 后端

#### 数据库迁移

新增 `oa_rounds` 表（迁移版本递增）：

```sql
CREATE TABLE oa_rounds (
    id TEXT PRIMARY KEY,
    patent_id TEXT NOT NULL,          -- 关联专利
    round_number INTEGER NOT NULL,    -- 第几轮（1=一审, 2=二审, 3=复审）
    round_type TEXT NOT NULL,         -- first_rejection / second_rejection / reexamination
    oa_date TEXT,                     -- OA 发文日期
    oa_text TEXT,                     -- OA 全文
    response_date TEXT,               -- 答复提交日期
    response_text TEXT,               -- 答复全文
    result TEXT,                      -- 结果：pending / accepted / rejected / partially_accepted
    result_date TEXT,                 -- 结果通知日期
    strategy_used TEXT,               -- 使用的策略
    quality_score REAL,               -- 质量评分
    notes TEXT,                       -- 备注
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (patent_id) REFERENCES patents(id)
);
```

#### API 端点

```rust
/// GET /api/oa/rounds/:patent_id — 获取某专利的所有 OA 轮次
pub async fn api_oa_rounds_get(
    State(s): State<AppState>,
    Path(patent_id): Path<String>,
) -> Json<serde_json::Value>

/// POST /api/oa/rounds — 新增一轮 OA
pub async fn api_oa_rounds_create(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value>

/// PUT /api/oa/rounds/:id — 更新某轮（如补充结果）
pub async fn api_oa_rounds_update(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value>

/// GET /api/oa/timeline/:patent_id — 获取时间线视图数据
pub async fn api_oa_timeline(
    State(s): State<AppState>,
    Path(patent_id): Path<String>,
) -> Json<serde_json::Value>
```

### 前端

#### OA 答复页改造

在 `templates/office_action_response.html`：
1. 顶部新增"历史轮次"选择器（下拉框）
2. 选择某轮后加载该轮的 OA 和答复
3. "新建轮次"按钮，选择 OA 类型后创建新轮次

#### 新增时间线视图

在 `templates/patent_detail.html` 新增"OA 时间线"标签页：
1. 横向时间线，每个节点代表一轮 OA
2. 节点颜色：进行中(蓝)、已授权(绿)、已驳回(红)、部分授权(黄)
3. 点击节点展开该轮详情
4. 轮次间对比：显示每轮策略变化、范围变化

关键 JS：
```javascript
async function loadOaTimeline(patentId) {
    // GET /api/oa/timeline/:patent_id
    // 渲染时间线
}

async function createOaRound(patentId, roundType) {
    // POST /api/oa/rounds
    // 切换到新轮次
}

async function updateOaResult(roundId, result) {
    // PUT /api/oa/rounds/:id
    // 更新结果状态
}
```

### i18n keys（前缀 `oa.workflow.`）
- `oa.workflow.timeline` = "OA 时间线" / "OA Timeline"
- `oa.workflow.round` = "第{n}轮" / "Round {n}"
- `oa.workflow.newRound` = "新建轮次" / "New Round"
- `oa.workflow.result` = "审查结果" / "Result"
- `oa.workflow.pending` = "待回复" / "Pending"
- `oa.workflow.accepted` = "已授权" / "Accepted"
- `oa.workflow.rejected` = "已驳回" / "Rejected"
- `oa.workflow.partial` = "部分授权" / "Partially Accepted"
- `oa.workflow.strategy` = "使用策略" / "Strategy"
- `oa.workflow.compare` = "轮次对比" / "Compare Rounds"

### 验收
- [ ] 能记录多轮 OA 及答复
- [ ] 时间线正确显示各轮状态
- [ ] 节点颜色区分结果
- [ ] 能更新每轮结果
- [ ] 轮次间能对比策略变化
- [ ] 数据库迁移正确执行
