# API 文档 / API Documentation

## 基础 URL / Base URL

```
http://127.0.0.1:3000
```

> 默认端口为 `3000`，可通过环境变量 `INNOFORGE_PORT` 覆盖。

## 说明 / Notes

- 搜索历史为前端 `localStorage`，没有 `/api/search/history` 接口。
- 详情页是页面路由 `GET /patent/:id`，不是 JSON API。
- AI 分析相关接口均为 OpenAI 兼容下游能力封装。
- 全局请求超时：300 秒（防止大上下文提前超时）。
- SSE（Server-Sent Events）端点使用标准 `text/event-stream` 格式。

---

## 页面路由 / Page Routes

| Method | Path | Description |
|---|---|---|
| GET | `/` | 首页 / Home |
| GET | `/search` | 搜索页 / Search page |
| GET | `/compare` | 专利对比页 / Patent comparison page |
| GET | `/ai` | AI 助手页 / AI assistant page |
| GET | `/settings` | 设置页 / Settings page |
| GET | `/patent/:id` | 专利详情页 / Patent detail page |
| GET | `/test` | 调试测试页 / Debug test page |
| GET | `/import` | 样例数据导入页 / Sample data import page |

---

## 搜索接口 / Search APIs

### 1) 本地搜索 / Local Search

**POST** `/api/search`

Request:

```json
{
  "query": "人工智能",
  "page": 1,
  "page_size": 20,
  "country": "CN",
  "date_from": "2020-01-01",
  "date_to": "2024-12-31",
  "search_type": "inventor",
  "sort_by": "relevance"
}
```

Fields:
- `query` (required)
- `page` (optional, default `1`)
- `page_size` (optional, default `20`)
- `country` (optional)
- `date_from`, `date_to` (optional, `YYYY-MM-DD`)
- `search_type` (optional): `applicant | inventor | patent_number | keyword`
- `sort_by` (optional): `relevance | new | old`

Response:

```json
{
  "patents": [
    {
      "id": "uuid",
      "patent_number": "CN1234567A",
      "title": "示例标题",
      "abstract_text": "摘要...",
      "applicant": "示例申请人",
      "inventor": "示例发明人",
      "filing_date": "2024-01-01",
      "country": "CN",
      "relevance_score": 92.5,
      "score_source": "发明人包含匹配"
    }
  ],
  "total": 123,
  "page": 1,
  "page_size": 20,
  "search_type": "inventor"
}
```

### 2) 在线搜索 / Online Search

**POST** `/api/search/online`

Request 与 `/api/search` 相同。优先走 SerpAPI（多 Key 轮询），失败自动回落本地搜索。

Response（SerpAPI 命中）:

```json
{
  "patents": [],
  "total": 0,
  "page": 1,
  "page_size": 10,
  "source": "serpapi"
}
```

Response（本地回落）:

```json
{
  "patents": [],
  "total": 0,
  "page": 1,
  "page_size": 20,
  "source": "local"
}
```

### 3) 统计 / Stats

**POST** `/api/search/stats`

Request 与 `/api/search` 相同（用于保持筛选一致）。

Response:

```json
{
  "total": 100,
  "applicants": [["公司A", 20], ["公司B", 15]],
  "countries": [["CN", 50], ["US", 30]],
  "years": [["2020", 10], ["2021", 20]]
}
```

### 4) 导出 CSV / Export CSV

**POST** `/api/search/export`

Request 与 `/api/search` 相同，返回 `text/csv` 文件流。

### 5) AI 分析搜索结果 / AI Analyze Search Results

**POST** `/api/search/analyze`

Request:

```json
{
  "query": "机器视觉",
  "patents": [
    { "title": "A", "abstract_text": "..." },
    { "title": "B", "abstract_text": "..." }
  ]
}
```

Response:

```json
{
  "status": "ok",
  "analysis": {}
}
```

---

## 专利接口 / Patent APIs

### 1) 按专利号抓取 / Fetch Patent by Number

**POST** `/api/patent/fetch`

Request:

```json
{
  "patent_number": "EP1234567",
  "source": "epo"
}
```

`source`: `epo | uspto`（默认 `epo`）

### 2) 批量导入 / Import Patents

**POST** `/api/patents/import`

Request:

```json
{
  "patents": []
}
```

> 推荐配合 `tools/import_public_patents.py` 使用，可将公开公告数据包（CSV/JSON/JSONL）批量导入本地库。

### 3) 丰富专利信息 / Enrich Patent

**GET** `/api/patent/enrich/:id`

### 4) 相似专利推荐 / Similar Patents

**GET** `/api/patent/similar/:id`

### 5) 上传文档对比 / Upload Compare

**POST** `/api/upload/compare` (`multipart/form-data`)

Fields:
- `file` (`.txt` 等文本文件)
- `patent_id`

---

## AI 接口 / AI APIs

### 1) 对话 / Chat

**POST** `/api/ai/chat`

```json
{
  "message": "请分析该专利创新点",
  "patent_id": "uuid-optional"
}
```

### 2) 流式对话 / Chat Stream (SSE)

**POST** `/api/ai/chat/stream`

与 `/api/ai/chat` 相同的请求体，返回 `text/event-stream` 流式响应。

### 3) 对话结论 / Chat Conclusions

**POST** `/api/ai/chat/conclusions`

```json
{
  "message": "请分析该专利创新点",
  "patent_id": "uuid-optional"
}
```

返回 AI 对话的关键结论摘要。

### 4) 摘要 / Summarize

**POST** `/api/ai/summarize`

```json
{
  "patent_number": "CN1234567A"
}
```

### 5) 对比 / Compare

**POST** `/api/ai/compare`

```json
{
  "patent_id1": "uuid-or-number-1",
  "patent_id2": "uuid-or-number-2"
}
```

### 6) 威胁评估 / Threat Assessment

**POST** `/api/ai/threat-assessment`

```json
{
  "patent_id": "uuid",
  "query": "评估该专利对本创意的威胁"
}
```

### 7) 权利要求图表 / Claim Chart

**POST** `/api/ai/claim-chart`

```json
{
  "patent_id": "uuid",
  "idea_id": "uuid"
}
```

### 8) 多专利对比矩阵 / Compare Matrix

**POST** `/api/ai/compare-matrix`

```json
{
  "patent_ids": ["uuid-1", "uuid-2", "uuid-3"]
}
```

---

## 创意验证 / Idea Validation & Pipeline APIs

> 创研台核心功能：16 步创新验证管道，支持断点续跑、状态机编排、SSE 进度推送。

### 1) 提交创意 / Submit Idea

**POST** `/api/idea/submit`

Request (`IdeaSubmitRequest`):

```json
{
  "title": "基于深度学习的图像识别方法",
  "description": "本发明涉及一种...",
  "technical_domain": "人工智能",
  "industry_field": "信息技术"
}
```

Response:

```json
{
  "status": "ok",
  "idea_id": "uuid"
}
```

### 2) 快速分析 / Quick Analyze

**POST** `/api/idea/analyze`

```json
{
  "idea_id": "uuid"
}
```

### 3) 启动管道 / Start Pipeline

**POST** `/api/idea/pipeline`

启动完整 16 步验证管道（搜索 → 解析 → 相似度 → 矛盾检测 → 深度推理 → 综合报告）。

```json
{
  "idea_id": "uuid"
}
```

Response:

```json
{
  "status": "ok",
  "idea_id": "uuid",
  "message": "管道已启动，请通过 SSE 监听进度"
}
```

### 4) 断点续跑 / Resume Pipeline

**POST** `/api/idea/:id/resume`

从上一次失败或中断的步骤继续执行管道。

Response:

```json
{
  "status": "ok",
  "idea_id": "uuid",
  "resumed_from_step": "StepX"
}
```

### 5) SSE 进度流 / Pipeline Progress (SSE)

**GET** `/api/idea/:id/progress`

返回 `text/event-stream`，推送管道执行进度。每条事件包含当前步骤名称、完成百分比、中间结果。

### 6) 获取创意详情 / Get Idea

**GET** `/api/idea/:id`

### 7) 列出所有创意 / List Ideas

**GET** `/api/idea/list`

### 8) 删除创意 / Delete Idea

**POST** `/api/idea/:id/delete`

级联删除：创意 + 消息 + 特征卡片 + 管道快照 + 证据链。

### 9) 证据链 / Evidence Chain

**GET** `/api/idea/:id/evidence`

返回管道执行过程中积累的全部证据条目。

### 10) 创意对话 / Idea Multi-Round Chat

**POST** `/api/idea/:id/chat`

```json
{
  "message": "这个创意的差异化优势在哪里？"
}
```

### 11) 获取对话消息 / Get Idea Messages

**GET** `/api/idea/:id/messages`

### 12) 对话结论 / Chat Conclusions

**GET** `/api/idea/:id/chat/conclusions`

### 13) 总结讨论 / Summarize Discussion

**POST** `/api/idea/:id/summarize`

### 14) 验证报告 / Report

**GET** `/api/idea/:id/report?type=executive`

Query 参数 `type`: `executive | technical`

返回 JSON 格式的验证报告。

### 15) HTML 报告 / Report HTML

**GET** `/api/idea/:id/report.html`

返回可直接渲染的 HTML 报告。

### 16) 研发状态 / Research State

**GET** `/api/idea/:id/research-state`

**POST** `/api/idea/:id/research-state`

获取或更新研发状态机的当前状态。

Request:

```json
{
  "current_hypothesis": "当前假设",
  "excluded_paths": ["已排除路径"],
  "open_questions": ["待验证问题"],
  "verified_claims": ["已验证声明"]
}
```

### 17) 管道重定向 / Redirect Pipeline

**POST** `/api/idea/:id/redirect`

从指定步骤重新启动管道，可切换技术领域或追加查询词。

Request:

```json
{
  "restart_from": "Step3",
  "technical_domain": "新领域",
  "add_queries": ["额外查询词"],
  "reason": "重定向原因"
}
```

### 18) 批量对比 / Batch Compare Ideas

**POST** `/api/ideas/batch-compare`

```json
{
  "idea_ids": ["uuid-1", "uuid-2"]
}
```

### 19) 权利要求树 / Claim Tree

**GET** `/api/idea/:id/claim-tree`

返回可视化权利要求层级结构。

### 20) 迭代优化 / Iterate

**POST** `/api/idea/:id/iterate`

基于当前验证结果，自动迭代优化创意。

### 21) 版本历史 / Versions

**GET** `/api/idea/:id/versions`

### 22) 分支列表 / Branches

**GET** `/api/idea/:id/branches`

### 23) 关键发现 / Findings

**GET** `/api/idea/:id/findings`

---

## 特征卡片接口 / Feature Cards APIs

> 创意方案的结构化拆解与对比工具。每张卡片记录一个技术方案的核心结构、工艺步骤等维度。

### 1) 获取特征卡片列表 / Get Feature Cards

**GET** `/api/ideas/:id/feature-cards`

Response:

```json
{
  "status": "ok",
  "cards": [
    {
      "id": "uuid",
      "idea_id": "uuid",
      "title": "方案 A",
      "description": "描述...",
      "novelty_score": 0.85,
      "technical_problem": "技术问题...",
      "core_structure": "核心结构...",
      "key_relations": "关键关系...",
      "process_steps": "工艺步骤...",
      "application_scenarios": "应用场景...",
      "created_at": "2026-01-01T00:00:00Z"
    }
  ]
}
```

### 2) 创建特征卡片 / Create Feature Card

**POST** `/api/ideas/:id/feature-cards`

Request (`CreateFeatureCardRequest`):

```json
{
  "title": "方案 A：基于卷积神经网络的检测方法",
  "description": "详细描述...",
  "novelty_score": 0.85,
  "technical_problem": "现有技术精度不足",
  "core_structure": "CNN + 注意力机制",
  "key_relations": "输入→卷积→池化→全连接→输出",
  "process_steps": "数据采集→预处理→模型训练→推理→后处理",
  "application_scenarios": "工业质检、医疗影像"
}
```

校验规则：
- `title` 必填，最长 500 字符

### 3) 卡片差异对比 / Card Diff

**GET** `/api/feature-cards/diff?a=ID_A&b=ID_B`

使用字符级 LCS（最长公共子序列）算法对比两张卡片的全部字段，返回结构化差异列表。

Response:

```json
{
  "status": "ok",
  "card_a": { "id": "uuid-a", "title": "方案 A" },
  "card_b": { "id": "uuid-b", "title": "方案 B" },
  "diff": {
    "title": [
      { "type": "equal", "text": "方案 " },
      { "type": "remove", "text": "A" },
      { "type": "add", "text": "B" }
    ],
    "description": [...],
    "novelty_score": { "a": "85.0", "b": "72.3", "delta": "-12.7" },
    "technical_problem": [...],
    "core_structure": [...],
    "key_relations": [...],
    "process_steps": [...],
    "application_scenarios": [...]
  },
  "diff_type": "structure",
  "novelty_significance": true,
  "novelty_reason": "核心结构采用了不同的技术方案"
}
```

差异类型（`diff_type`）：
- `structure` — 核心结构不同（新颖性意义高）
- `method` — 工艺/实施步骤不同（新颖性意义高）
- `parameter` — 参数级差异（新颖性意义低）
- `none` — 无显著差异

---

## 收藏夹接口 / Collections APIs

### 1) 列出所有收藏夹 / List Collections

**GET** `/api/collections`

Response:

```json
{
  "collections": [
    {
      "id": "uuid",
      "name": "核心专利",
      "description": "竞品核心专利集",
      "patent_count": 15,
      "created_at": "2026-01-01T00:00:00Z"
    }
  ]
}
```

### 2) 创建收藏夹 / Create Collection

**POST** `/api/collections`

```json
{
  "name": "核心专利",
  "description": "竞品核心专利集"
}
```

校验规则：
- `name` 必填，最长 100 字符

### 3) 删除收藏夹 / Delete Collection

**DELETE** `/api/collections/:id`

### 4) 获取收藏夹内专利 / Get Collection Patents

**GET** `/api/collections/:id/patents`

### 5) 添加专利到收藏夹 / Add Patent to Collection

**POST** `/api/collections/:id/add`

```json
{
  "patent_id": "uuid-or-number"
}
```

> 会先验证专利 ID 是否存在于数据库中。

### 6) 从收藏夹移除专利 / Remove Patent from Collection

**DELETE** `/api/collections/:id/remove/:patent_id`

### 7) 获取专利所属收藏夹 / Get Patent's Collections

**GET** `/api/patents/:patent_id/collections`

---

## 标签接口 / Tag APIs

### 1) 添加标签 / Add Tag

**POST** `/api/patents/:patent_id/tags`

```json
{
  "tag": "AI"
}
```

校验规则：
- `tag` 必填，最长 50 字符

### 2) 删除标签 / Remove Tag

**DELETE** `/api/patents/:patent_id/tags/:tag`

### 3) 获取专利标签 / Get Patent Tags

**GET** `/api/patents/:patent_id/tags`

Response:

```json
{
  "tags": ["AI", "机器学习"]
}
```

### 4) 列出所有标签 / List All Tags

**GET** `/api/tags`

Response:

```json
{
  "tags": [
    { "tag": "AI", "count": 10 },
    { "tag": "机器学习", "count": 5 }
  ]
}
```

---

## 聊天同步接口 / Chat Sync APIs

> 跨设备聊天记录同步，替代前端 `localStorage`，通过后端 SQLite 持久化。

### 1) 获取消息（分页）/ Get Messages

**GET** `/api/chat/:session_key?limit=50&offset=0`

Query 参数：
- `limit`（可选，默认 50，范围 1-200）
- `offset`（可选，默认 0）

校验规则：
- `session_key` 长度 1-255 字符

Response:

```json
{
  "status": "ok",
  "messages": [
    { "id": 1, "role": "user", "content": "Hello", "created_at": "..." }
  ],
  "total": 100,
  "limit": 50,
  "offset": 0
}
```

### 2) 保存消息 / Save Message

**POST** `/api/chat/:session_key/save`

```json
{
  "role": "user",
  "content": "Hello"
}
```

校验规则：
- `role` 必须是 `user | assistant | system`
- `content` 最长 50000 字符

### 3) 删除消息 / Delete Messages

**POST** `/api/chat/:session_key/delete`

删除指定会话的全部消息。

Response:

```json
{
  "status": "ok",
  "deleted": 5
}
```

---

## IPC 分类接口 / IPC Classification APIs

### 1) IPC 建议 / Suggest

**GET** `/api/ipc/suggest`

### 2) IPC 树 / Tree

**GET** `/api/ipc/tree`

---

## 配置接口 / Settings APIs

### 1) 读取配置 / Get Settings

**GET** `/api/settings`

返回脱敏密钥与配置状态（支持多服务商独立 Key）:

```json
{
  "serpapi_key": "abcd****wxyz",
  "serpapi_key_configured": true,
  "ai_base_url": "https://open.bigmodel.cn/api/paas/v4",
  "ai_api_key": "abcd****wxyz",
  "ai_api_key_configured": true,
  "ai_model": "glm-4-flash",
  "ai_api_key_deepseek": "****",
  "ai_api_key_gemini": "****",
  "ai_api_key_zhipu": "****",
  "ai_api_key_anthropic": "****",
  "ai_api_key_xiaomi": "****",
  "ai_api_key_sensetime": "****",
  "ai_api_key_openrouter": "****",
  "gemini_cli_enabled": false
}
```

### 2) 保存 SerpAPI Key / Save SerpAPI

**POST** `/api/settings/serpapi`

```json
{ "api_key": "your-serpapi-key" }
```

### 3) 保存 AI 配置 / Save AI Config

**POST** `/api/settings/ai`

```json
{
  "base_url": "https://open.bigmodel.cn/api/paas/v4",
  "api_key": "your-ai-key",
  "model": "glm-4-flash"
}
```

---

## MCP Server（信息性说明 / Informational）

> MCP Server **不是** HTTP 端点，通过 stdio JSON-RPC 通信（`src/bin/mcp-server.rs`）。
> 作为轻量代理，内部调用主 Web 服务器 HTTP API。

### 可用工具 / Available Tools

| Tool Name | 内部调用 / Internal Call |
|---|---|
| `patent_search` | `POST /api/search` 或 `POST /api/search/online` |
| `patent_detail` | `GET /api/patent/enrich/{id}` |
| `patent_analyze` | `POST /api/ai/summarize` |
| `patent_compare` | `POST /api/ai/analyze-results` |
| `idea_validate` | `POST /api/idea/submit` |
| `patent_chat` | `POST /api/ai/chat` |
| `patent_threat_assessment` | `POST /api/ai/threat-assessment` |
| `patent_claim_chart` | `POST /api/ai/claim-chart` |
| `patent_multi_compare` | `POST /api/ai/compare-matrix` |

---

## 通用错误 / Common Errors

```json
{
  "status": "error",
  "message": "..."
}
```

常见错误码 / HTTP Status Codes：

| Status | 含义 / Meaning |
|---|---|
| 200 | 成功 / Success |
| 400 | 请求参数错误 / Bad Request |
| 404 | 资源不存在 / Not Found |
| 500 | 服务器内部错误 / Internal Server Error |
