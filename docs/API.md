# API 文档 / API Documentation

> **已覆盖 24/140 端点**（R23 文档覆盖重生：`/api/cad/*`、`/api/auth/*`、`/api/ai/oa-*` 三组）。
> 计数由 `tests/api_doc_guard.rs` 守卫：文档声明但代码路由表里不存在的端点会让 `cargo test --test api_doc_guard` 变红。
> 路由表共 140 条，源自 `src/common.rs` 的 `.route(...)` 调用，字段名一律从 handler 真实代码读取，未编造。

## 基础 URL / Base URL

```
http://127.0.0.1:3000
```

> 默认端口 `3000`，可通过环境变量 `INNOFORGE_PORT` 覆盖。

## 通用约定 / Conventions

- 未标注 Content-Type 的 POST/PUT 请求体均为 `application/json`。
- 成功响应以各端点示例为准；失败响应统一形如 `{"error": "<message>"}` 或 `{"code":"<code>","error":"<message>"}`，HTTP 状态码见各端点说明。
- SSE 端点使用 `text/event-stream`。
- 示例专利号一律使用合成号（如 `CN990001001A`），不含任何真实案件号或人名。
- AI 类端点依赖 OpenAI 兼容下游配置（`api_key` / `base_url` / `model`，于设置页或 `/api/settings/ai` 写入）；Google 类端点依赖 OAuth Client ID/Secret 或 gcloud ADC。

---

## 覆盖索引 / Coverage Index

| Method | Path | 说明 |
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


---

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

## 待覆盖 / Not Yet Covered

以下端点尚未在本轮 R23 文档中展开（计数 140-24=116），后续迭代补充：页面路由、`/api/search/*`、`/api/patent/*`、`/api/chat/*`、`/api/settings/*`、`/api/idea/*`、`/api/ideas/*`、`/api/oa/*`（非 ai 前缀的 OA 历史/导出/轮次）、`/api/ai/*`（非 oa 前缀的 AI 端点）、`/api/ipc/*`、`/api/collections/*`、`/api/patents/*`、`/api/tags`、`/api/upload/*`、`/api/version`、`/static/*path`。
