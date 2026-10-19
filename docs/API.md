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

> 注：`/test`（调试页）与 `/import`（样例导入页）两条页面路由已在代码中移除，故不再列出（issue #62）。
| GET | `/patent/:id` | 专利详情页 / Patent detail page |

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
  "search_type": "inventor",
  "matched_fields": ["inventor"],
  "applied_filter": "inventor_only",
  "dedup_removed": 0
}
```

**多词语义（issue #72）**: 当 `query` 含空格分出多个 token 时，本地搜索走 **OR 语义**——
每个 token 命中 `title / abstract_text / applicant / inventor / patent_number` 任一字段即返回该行，
按相关性评分排序。单 token 时行为与旧整串 `%query%` LIKE 一致。

**自证字段（issue #72）**:
- `matched_fields`: 本次实际查询的数据库字段列表（如 `["inventor"]` 或 `["title","abstract_text","applicant","inventor","patent_number"]`）。
- `applied_filter`: 检索策略标识：`inventor_only` / `applicant_only` / `patent_number` / `single_token_like` / `multi_token_or`。
- `reason`（仅 0 命中出现）: 解释为何无结果，如 `"查询被判定为发明人检索，但发明人列无匹配"` / `"多词 OR 检索：库中无任一 token 命中"` / `"库中无匹配该词的专利"`。

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

> ⚠️ 该端点在当前代码路由表中不存在（历史条目，issue #62）。以实际路由为准，勿据此调用。

---

## 标签接口 / Tag APIs

### 1) 添加标签 / Add Tag

**POST** `/api/patents/:patent_id/tags`

> ⚠️ 该端点在当前代码路由表中不存在（历史条目，issue #62）。以实际路由为准，勿据此调用。

```json
{
  "tag": "AI"
}
```

校验规则：
- `tag` 必填，最长 50 字符

### 2) 删除标签 / Remove Tag

**DELETE** `/api/patents/:patent_id/tags/:tag`

> ⚠️ 该端点在当前代码路由表中不存在（历史条目，issue #62）。以实际路由为准，勿据此调用。

### 3) 获取专利标签 / Get Patent Tags

**GET** `/api/patents/:patent_id/tags`

> ⚠️ 该端点在当前代码路由表中不存在（历史条目，issue #62）。以实际路由为准，勿据此调用。

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

> ⚠️ 该端点在当前代码路由表中不存在（历史条目，issue #62）。以实际路由为准，勿据此调用。

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

---

## 环境变量 / Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `INNOFORGE_PORT` | `3000` | HTTP 监听端口。更改后 OAuth 回调地址自动跟随。/ HTTP listen port. OAuth callback URL follows automatically. |
| `INNOFORGE_OAUTH_CALLBACK_URL` | *(由 `INNOFORGE_PORT` 推导)* | Google OAuth 重定向 URI。设置后覆盖 `INNOFORGE_PORT` 推导的地址。必须与 Google Cloud Console 中登记的授权重定向 URI 逐字匹配。/ Google OAuth redirect URI. Overrides `INNOFORGE_PORT`-derived URL. Must match the authorized redirect URI in Google Cloud Console. |
| `INNOFORGE_UPSTREAM_TIMEOUT_SECS` | `30` | 上游 HTTP 调用超时秒数（专利 API、Google OAuth、AI 模型探测）。/ Timeout in seconds for upstream HTTP calls. |

---

## 覆盖索引（R23 新增 24 条） / Coverage Index

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/api/cad/status` | 查询 FreeCAD 引擎就绪状态 |
| POST | `/api/cad/start` | 启动/预热 FreeCAD 引擎 |
| POST | `/api/cad/draw` | 提交绘图指令，生成 CAD 产物 |
| GET | `/api/cad/artifacts` | 列出某上下文的 CAD 产物历史 |
| GET | `/api/cad/artifacts/:id/preview` | 取产物预览 PNG |
| GET | `/api/cad/artifacts/:id/download/:format` | 下载产物（fcstd/step） |
| GET | `/api/auth/google/url` | 生成 Google OAuth 授权跳转 URL |
| GET | `/api/auth/google/callback` | OAuth 回调 HTML（前端中转） |
| POST | `/api/auth/google/exchange` | 用授权码交换 access/refresh token |
| GET | `/api/auth/google/status` | 查询 Google OAuth 凭据/令牌状态 |
| GET | `/api/auth/gcloud/status` | 查询 gcloud CLI / ADC 状态 |
| POST | `/api/auth/gcloud/login` | 通过 gcloud CLI 或 ADC 登录获取令牌 |
| POST | `/api/ai/oa-discuss` | 审查意见多轮讨论 |
| POST | `/api/ai/oa-fetch-refs` | 从审查意见抽取对比文献标识 |
| POST | `/api/ai/oa-panel` | OA 分析面板聚合数据 |
| POST | `/api/ai/oa-generate-response-letter` | 生成正式答复函草稿 |
| POST | `/api/ai/oa-history-compare` | 对比当前与历史 OA 轮次 |
| POST | `/api/ai/oa-examiner-preview` | 预览审查员视角下的答复 |
| POST | `/api/ai/oa-defense-analysis` | 针对权项的抗辩分析 |
| POST | `/api/ai/oa-strategy-recommend` | 答复策略推荐 |
| POST | `/api/ai/oa-quality-score` | 答复质量评分 |
| POST | `/api/ai/oa-claim-simulate` | 修改权项的侵权模拟 |
| POST | `/api/ai/oa-collect-evidence` | 收集支持答复的证据 |
| POST | `/api/ai/oa-adversarial` | 对抗性反驳压力测试 |


## 1. CAD 接口 / `/api/cad/*`

> handler：`src/routes/cad.rs`；域类型：`src/patent.rs`（`CadDrawRequest`/`CadDrawResponse`/`CadArtifact`/`CadContextKind`/`CadAvailability`/`CadStatus`）。
> 所需配置：FreeCAD/AionCAD 引擎（`freecad_auto_start` 设置项控制是否自动启动）。

### GET `/api/cad/status`

- 请求：无 body。
- 成功响应（`200`）：

```json
{ "availability": "ready", "message": "..." }
```

`availability` ∈ `ready` | `starting` | `unavailable` | `unsupported`。

- 失败：本端点不返回错误体（始终 200）。

### POST `/api/cad/start`

- 请求：无 body。
- 成功响应（`200`）：同 `/api/cad/status` 的 `CadStatus` 对象。

### POST `/api/cad/draw`

- 请求（`CadDrawRequest`）：

```json
{
  "context_kind": "idea",
  "context_id": "CN990001001A",
  "prompt": "画一个 40x30x10 的长方体",
  "conversation_context": "可选上下文文本",
  "parent_artifact_id": null
}
```

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `context_kind` | enum `idea`/`patent`/`oa` | 是 | 上下文类型 |
| `context_id` | string | 是 | 上下文 ID（非空） |
| `prompt` | string | 是 | 绘图指令（非空） |
| `conversation_context` | string | 否 | 上下文文本，默认空 |
| `parent_artifact_id` | string\|null | 否 | 父版本 ID，用于派生新版本 |

- 成功响应（`200`，`CadDrawResponse`）：

```json
{
  "artifact": {
    "id": "<uuid>",
    "context_kind": "idea",
    "context_id": "CN990001001A",
    "parent_artifact_id": null,
    "revision": 0,
    "prompt": "画一个 40x30x10 的长方体",
    "assumptions": ["单位毫米"],
    "preview_rel_path": "...",
    "fcstd_rel_path": "...",
    "step_rel_path": null,
    "validation": { "valid": true, "fixed": false, "issues": [] },
    "created_at": ""
  },
  "warnings": []
}
```

- 失败响应（`CadApiError` → `{ "code": "...", "error": "..." }`）：

| HTTP | code | 触发条件 |
|---|---|---|
| 400 | `invalid_request` | prompt 或 context_id 为空 |
| 400 | `parent_not_found` | parent_artifact_id 不存在 |
| 400 | `parent_context_mismatch` | 父版本不属于当前上下文 |
| 503 | `freecad_unavailable` | 引擎无法就绪 |
| 503 | `draw_failed` | 绘图调用失败 |
| 500 | `save_failed` / `parent_read_failed` | DB 读写失败 |

### GET `/api/cad/artifacts`

- 请求（query，`CadHistoryQuery`）：`context_kind`（enum）、`context_id`（string，非空）。
- 成功响应（`200`）：`{ "artifacts": [CadArtifact, ...] }`。
- 失败：`400 invalid_request`（context_id 空）/ `500 history_failed`。

### GET `/api/cad/artifacts/:id/preview`

- 路径参数：`id`（产物 ID）。
- 成功响应（`200`）：`image/png` 二进制。
- 失败：`404 artifact_not_found` / `404 file_not_found` / `400 invalid_path` / `500 root_unavailable`。

### GET `/api/cad/artifacts/:id/download/:format`

- 路径参数：`id`、`format` ∈ `fcstd` | `step`。
- 成功响应（`200`）：对应 MIME 的二进制，带 `Content-Disposition: attachment`。
- 失败：`404 step_unavailable`（无 STEP 文件）/ `400 invalid_format` / `404 artifact_not_found` / `404 file_not_found`。


---

## 2. 认证接口 / `/api/auth/*`

> handler：`src/routes/auth.rs`。所需配置：Google OAuth Client ID/Secret（设置页或 `/api/settings`），或本机 gcloud CLI / ADC 文件。

### GET `/api/auth/google/url`

- 请求：无 body。
- 成功响应（`200`）：

```json
{ "url": "https://accounts.google.com/o/oauth2/v2/auth?...", "redirect_uri": "http://127.0.0.1:3000/api/auth/google/callback" }
```

- 失败响应（`200`，业务错误内联）：`{ "error": "请先在设置页填写 Google OAuth Client ID" }`（未配置 client_id 时）。

### GET `/api/auth/google/callback`

- 请求：URL query `code`（由 Google 回跳带入）。
- 响应（`200`）：`text/html` 页面，内嵌 JS 将 `code` POST 到 `/api/auth/google/exchange` 并展示结果。无 JSON body。

### POST `/api/auth/google/exchange`

- 请求：

```json
{ "code": "<授权码>" }
```

- 成功响应（`200`）：

```json
{ "status": "ok", "message": "Google 账号认证成功！", "expires_in": 3600, "has_refresh_token": true }
```

- 失败响应（`200`，业务错误内联）：

| error 文案 | 触发条件 |
|---|---|
| `无效的 JSON 请求体` | body 非合法 JSON |
| `缺少授权码` | 无 `code` 字段 |
| `未配置 Google OAuth 凭据，请先在设置页填写` | client_id/secret 为空 |
| `令牌交换失败: <详情>` | Google token 端点返回非 2xx |
| `网络请求失败: <详情>` | 上游请求异常 |

### GET `/api/auth/google/status`

- 请求：无 body。
- 成功响应（`200`）：

```json
{
  "has_credentials": true,
  "has_token": true,
  "token_expired": false,
  "token_expiry": 1730000000,
  "client_id_set": true,
  "client_secret_set": true,
  "refresh_token_set": true,
  "auth_mode": "oauth"
}
```

- 无独立失败响应（始终 200，字段反映当前状态）。

### GET `/api/auth/gcloud/status`

- 请求：无 body。
- 成功响应（`200`）：

```json
{
  "gcloud_available": true,
  "adc_file_exists": true,
  "adc_file_path": "/root/.config/gcloud/application_default_credentials.json",
  "adc_type": "authorized_user",
  "adc_email": "robot@example.com",
  "is_gcloud_mode": false,
  "has_token": true,
  "token_expired": false
}
```

- 无独立失败响应（始终 200）。

### POST `/api/auth/gcloud/login`

- 请求：无 body。
- 成功响应（`200`）：

```json
{ "status": "ok", "message": "✅ gcloud CLI 认证成功！", "method": "gcloud_cli", "expires_in": 3600 }
```

- 失败响应（`200`，业务错误内联）：`{ "status": "error", "message": "无法保存 Google 认证状态，请稍后重试。" }`（DB 持久化失败时）；gcloud CLI 不可用且 ADC 不可用时返回对应错误文案。


---

## 3. 审查意见 AI 接口 / `/api/ai/oa-*`

> handler：`src/routes/ai.rs`。请求体均为 `application/json`（`Json<serde_json::Value>`，字段名取自代码中 `req["..."]` / `req.get("...")` 访问）。
> 所需配置：OpenAI 兼容下游（`api_key`/`base_url`/`model`）；部分端点额外读取 DB 中的专利/历史记录。
> 通用失败：缺关键字段时返回 `200` + `{ "error": "<message>" }`（业务级，非 HTTP 错误码）。

### POST `/api/ai/oa-discuss`

- 请求字段（从代码读取）：`message`、`patent_number`、`oa_text`、`oa_type`、`office_action`、`analysis`、`discussion`、`discussion_id`、`applicant_name`、`file_name`、`file_type`、`content`。
- 成功响应（`200`）：`{ "reply": "...", "discussion_id": "...", ... }`（AI 回复与讨论状态）。
- 失败：缺 `message` 或上下文时返回 `{ "error": "..." }`。

### POST `/api/ai/oa-fetch-refs`

- 请求字段：`oa_text`（审查意见全文）。
- 成功响应（`200`）：`{ "references": [ { "number": "...", "title": "..." }, ... ] }`。
- 失败：`oa_text` 为空时返回 `{ "error": "..." }`。

### POST `/api/ai/oa-panel`

- 请求字段：`my_patent`、`oa_text`、`refs`。
- 成功响应（`200`）：面板聚合对象（含分析摘要、轮次、建议等）。
- 失败：缺关键字段时返回 `{ "error": "..." }`。

### POST `/api/ai/oa-generate-response-letter`

- 请求字段：`analysis`、`discussion`、`discussion_id`、`oa_date`、`oa_full_text`、`oa_type`、`office_action`、`case_meta`。
- 成功响应（`200`）：`{ "letter": "...", ... }`（正式答复函草稿）。
- 失败：缺关键字段时返回 `{ "error": "..." }`。

### POST `/api/ai/oa-history-compare`

- 请求字段：`patent_id`、`current_oa_text`、`current_oa_type`。
- 成功响应（`200`）：`{ "diff": "...", "rounds": [...], ... }`。
- 失败：缺 `patent_id` 时返回 `{ "error": "..." }`。

### POST `/api/ai/oa-examiner-preview`

- 请求字段：`my_patent`、`oa_text`、`response_text`。
- 成功响应（`200`）：`{ "preview": "...", ... }`（审查员视角预览）。
- 失败：缺关键字段时返回 `{ "error": "..." }`。

### POST `/api/ai/oa-defense-analysis`

- 请求字段：`my_patent`、`claims`。
- 成功响应（`200`）：`{ "defense": [...], ... }`。
- 失败：缺关键字段时返回 `{ "error": "..." }`。

### POST `/api/ai/oa-strategy-recommend`

- 请求字段：`my_patent`、`oa_text`、`oa_type`、`refs`。
- 成功响应（`200`）：`{ "strategies": [...], ... }`。
- 失败：缺关键字段时返回 `{ "error": "..." }`。

### POST `/api/ai/oa-quality-score`

- 请求字段：`my_patent`、`oa_text`、`response_text`。
- 成功响应（`200`）：`{ "score": <number>, "dimensions": {...}, ... }`。
- 失败：缺关键字段时返回 `{ "error": "..." }`。

### POST `/api/ai/oa-claim-simulate`

- 请求字段：`original_claims`、`modified_claims`、`oa_text`、`refs`。
- 成功响应（`200`）：`{ "simulation": [...], ... }`。
- 失败：缺关键字段时返回 `{ "error": "..." }`。

### POST `/api/ai/oa-collect-evidence`

- 请求字段：`my_patent`、`oa_text`、`refs`。
- 成功响应（`200`）：`{ "evidence": [...], ... }`。
- 失败：缺关键字段时返回 `{ "error": "..." }`。

### POST `/api/ai/oa-adversarial`

- 请求字段：`my_patent`、`oa_text`、`claims`、`refs`、`rounds`、`focus`、`goal_threshold`。
- 成功响应（`200`）：`{ "rounds": [...], "survived": <bool>, ... }`。
- 失败：缺关键字段时返回 `{ "error": "..." }`。

---


---

---

## 4. OA 期限 / 轮次 / 历史 / 讨论 / 导出

> 这一组是审查意见（OA）工作流的数据面。期限计算以 `src/oa_deadline.rs` 为唯一数据源；轮次记录以 `oa_rounds` 表为准。

### GET `/api/oa/deadline-table`

返回各类 OA 的法定答复期限（唯一数据源，前端不应自行硬编码）。

```json
{
  "deadline_table": {
    "first_exam":             { "kind": "Months",  "value": 4 },
    "second_rejection":       { "kind": "Months",  "value": 2 },
    "reject_review":          { "kind": "Months",  "value": 3 },
    "reexamination_request":  { "kind": "Months",  "value": 3 },
    "reexamination_decision": { "kind": "Months",  "value": 3 },
    "admin_lawsuit":          { "kind": "Months",  "value": 3 },
    "abnormal":               { "kind": "WorkDays", "value": 15 }
  }
}
```

### GET `/api/oa/compute-deadline?oa_type=<type>&date=YYYY-MM-DD`

按“发文日”推算届满日。`date` 不合法时返回 `{ "error": "Invalid date format, expected YYYY-MM-DD" }`。

响应字段（`DeadlineResult`）：

| 字段 | 含义 |
|---|---|
| `dispatch_date` | 发文日（原样返回） |
| `service_date` | 送达日（电子送达＝发文日，不加邮路推定） |
| `deadline` | 届满日 |
| `days_left` | 剩余天数，负数表示已逾期 |
| `total_days` | 发文日到届满日的总天数 |
| `is_urgent` | 是否临近期限（≤7 天） |
| `spec` | 期限规格（`Months(n)` / `WorkDays(n)`） |

### GET `/api/oa/rounds/:patent_id`

返回某专利的全部 OA 轮次：`{ "status": "ok", "patent_id": "...", "rounds": [...], "count": N }`。

### POST `/api/oa/rounds`

登记一轮 OA。必填：`patent_id`、`round_type`；`round_number` 由服务端自动计算。

- `round_type` 仅允许：`first_rejection`、`second_rejection`、`reexamination`、`office_action`、`first_exam`、`abnormal`、`reject_review`；非法值返回 `{ "status": "error", "message": "Invalid round_type: '…'. Allowed values: …" }`。
- 可选：`oa_date`、`oa_text`、`response_date`、`response_text`、`result`、`result_date`、`strategy_used`、`quality_score`（数值）、`notes`。
- 成功：`{ "status": "ok", "round": {…}, "message": "OA round created" }`。

### PUT `/api/oa/rounds/update/:id`

按 id 局部更新一轮 OA，只更新请求里出现的字段；记录不存在返回 `{ "status": "error", "message": "OA round not found" }`。

### GET `/api/oa/history/all` 与 GET `/api/oa/history/:patent_number`

均返回 `{ "status": "ok", "analyses": [...] }`。`/history/all` 最多返回 100 条（按时间倒序）；`/history/:patent_number` 按专利号过滤。

> ⚠️ `GET /api/oa/history/detail/:id` 目前被错误绑定到“列出全部”的 handler，`:id` 会被忽略（见 issue #88）。排查时不要依赖它。

### POST `/api/oa/export-docx`

把答复正文导出为 DOCX（服务端生成，返回 base64）。

- 请求：`{ "response_text": "…", "patent_number": "…", "applicant": "…", "oa_type": "…" }`；`response_text` 必填且不超过 50,000 字符。
- 成功：`{ "status": "ok", "docx": "<base64>" }`；失败：`{ "error": "…" }`。
- 注意：响应是 JSON 里的 base64，不是二进制流。

## 覆盖索引（R23 续：OA 17 条） / Coverage Index

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/api/oa/deadline-table` | 法定答复期限表（唯一数据源） |
| GET | `/api/oa/compute-deadline` | 按发文日推算届满日 |
| GET | `/api/oa/history/all` | 全部 OA 分析历史（≤100 条） |
| GET | `/api/oa/history/:patent_number` | 某专利的 OA 分析历史 |
| GET | `/api/oa/history/detail/:id` | 单条详情（当前实现有缺陷，见 #88） |
| POST | `/api/oa/history/:id/delete` | 删除一条 OA 分析记录 |
| GET | `/api/oa/history/:patent_number/diff` | 一审/二审差异对比 |
| GET | `/api/oa/discussions/:patent_number` | 某专利的 OA 讨论会话列表 |
| GET | `/api/oa/discussions/:patent_number/:discussion_id` | 单个讨论会话详情 |
| POST | `/api/oa/discussions/import` | 导入讨论会话 |
| POST | `/api/oa/export-docx` | 答复正文导出 DOCX（base64） |
| POST | `/api/oa/export` | 分析结果服务端导出 |
| GET | `/api/oa/export/:filename` | 下载已导出的文件 |
| GET | `/api/oa/rounds/:patent_id` | 某专利的 OA 轮次列表 |
| POST | `/api/oa/rounds` | 登记一轮 OA |
| PUT | `/api/oa/rounds/update/:id` | 局部更新一轮 OA |
| GET | `/api/oa/timeline/:patent_id` | 某专利的 OA 时间线 |


## 覆盖说明 / Coverage

- 既有章节（搜索 / 专利 / 创意验证 / 特征卡片 / 收藏夹 / 标签 / 聊天 / IPC / 配置 / 错误 / 环境变量）：59 条 API 端点说明；
- 页面路由表：6 条（`/test`、`/import` 已从代码移除，见上表下方注记）；
- 本轮 R23 新增（CAD / 认证 / 审查意见 AI）：24 条；
- 本轮续补（OA 期限 / 轮次 / 历史 / 讨论 / 导出）：17 条。
- 合计 **111 条**（按 method+path 去重），覆盖 145 个 method+path 路由组合中的 111 条（约 77%）。剩余 39 条由 issue #62 跟踪。

## 待覆盖 / Not Yet Covered

以下范围尚未逐条展开：页面级交互细节、`/api/search/*` 中尚未给出字段级示例的部分、`/api/chat/*`、`/api/settings/*` 其余项、`/api/idea/*`、`/api/ideas/*`、`/api/oa/*`（非 `ai` 前缀）、`/api/ai/*`（非 `oa` 前缀）、`/api/ipc/*`、`/api/collections/*`、`/api/tags`、`/api/upload/*`、`/api/version`、`/static/*path`。

> 注：`/api/search/*`、`/api/patent/*`、`/api/upload/compare` 的说明已在上文既有章节中，不属于“待覆盖”。
