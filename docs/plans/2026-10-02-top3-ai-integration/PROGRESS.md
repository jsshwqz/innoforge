# 御三家 AI 接入 — 进度记录

> 最后更新：2026-10-02
> 负责人：executor（架构搭档 AI）
> 仓库：github.com/jsshwqz/innoforge
> 分支：dev

---

## 已完成 ✅

### Phase 1 核心：API 预设配置

| commit | 文件 | 说明 |
|--------|------|------|
| `1e60414` | `src/ai/providers.rs`（新建） | 三家 ProviderPreset 定义 + 查找函数 + 单元测试 |
| `eae084f` | `src/ai/mod.rs`（更新） | 注册 `mod providers` + 导出 `TOP3_PRESETS` 等 |
| `eae084f` | `.env.example`（更新） | 新增三家注释示例（取消注释填 Key 即可切换） |

### 规划文档

| commit | 文件 |
|--------|------|
| `c5e30d3` | `docs/plans/2026-10-02-top3-ai-integration/README.md` |
| `9454b8c` | `01-status-analysis.md` |
| `eca7a18` | `02-subscription-design.md` |
| `23ae364` | `03-implementation.md` |
| `598cfd9` | `04-integration-roadmap.md` |
| `70c2aea` | `05-risks-summary.md` |

### 已完成的具体内容

**`src/ai/providers.rs`** 定义了三个预设：

| 预设 | base_url | 默认模型 | 专家模型 |
|------|----------|----------|----------|
| OpenAI | `https://api.openai.com/v1` | gpt-4o | o3-mini |
| Anthropic | `https://api.anthropic.com/v1`（OpenAI 兼容端点） | claude-sonnet-4-6 | claude-opus-4 |
| Gemini | `https://generativelanguage.googleapis.com/v1beta/openai/` | gemini-2.0-flash | gemini-2.5-pro |

关键设计决策：
- 三家全部走 OpenAI 兼容 API 格式，**无需写格式适配器**
- Anthropic 利用 2025 年后推出的 OpenAI 兼容端点
- Gemini 利用 v1beta/openai/ 兼容端点
- 每个预设包含：id、display_name、base_url、api_key_env、default_model、expert_model、available_models、signup_url

---

## 未完成 ❌

### Phase 1 收尾（预计 0.5 天）

| # | 任务 | 文件 | 说明 | 状态 |
|---|------|------|------|------|
| 1 | 设置页面加三家快捷选择 UI | `static/settings.html` | 在 AI 配置区新增三个预设按钮，点击后自动填入 base_url + model | 未开始 |
| 2 | 连接测试端点 | `src/routes/ai.rs` | 新增 `POST /api/ai/test-connection` 端点，接收 base_url + api_key + model，发一个简单请求验证连通性 | 未开始 |
| 3 | 前端调用测试端点 | `static/settings.html`（JS 部分） | 点击"测试"按钮时调 test-connection，显示成功/失败 | 未开始 |

**修改方式提示**：这两个文件较大（settings.html ~?KB, ai.rs ~86KB），需通过 git clone + push 方式修改。git 代理地址：`http://127.0.0.1:9800/git/github/jsshwqz/innoforge.git`，已验证可 push。

### Phase 2：订阅代理核心（预计 5-7 天）

| # | 任务 | 文件 | 说明 | 状态 |
|---|------|------|------|------|
| 4 | SubscriptionProvider trait | `src/ai/subscription/mod.rs`（新建） | 定义统一 trait：from_cookies / check_login / get_plan / chat / chat_stream / list_models / refresh | 未开始 |
| 5 | ChatGPT Cookie 代理 | `src/ai/subscription/openai.rs`（新建） | 提取 `__Secure-next-auth.session-token` → 获取 accessToken → 代理调 `backend-api/conversation` | 未开始 |
| 6 | Claude.ai Cookie 代理 | `src/ai/subscription/anthropic.rs`（新建） | 提取 `sessionKey` → 获取 org_id → 代理调 `chat_conversations/{id}/completion` | 未开始 |
| 7 | 数据库表 | `src/db/`（迁移） | `ai_subscription_sessions` 表 + `ai_subscription_usage` 日志表 | 未开始 |
| 8 | 订阅 API 端点 | `src/routes/ai.rs`（更新） | 登录/状态/退出/刷新 端点 | 未开始 |
| 9 | SSE 流式解析 | `src/ai/subscription/sse.rs`（新建） | 统一 SSE 解析器，适配各家格式差异 | 未开始 |

### Phase 3：WebView 登录集成（预计 3-5 天）

| # | 任务 | 文件 | 说明 | 状态 |
|---|------|------|------|------|
| 10 | 内嵌 WebView 登录窗口 | 前端组件 | 打开 chat.openai.com / claude.ai，用户在 WebView 中登录 | 未开始 |
| 11 | Cookie 提取 | 前端 JS Bridge | 登录成功后通过 WebView Cookie API 提取会话 Cookie | 未开始 |
| 12 | Cookie 传递给后端 | `src/routes/ai.rs` | 前端提取 Cookie 后 POST 给后端，后端存入 ai_subscription_sessions | 未开始 |
| 13 | 登录状态管理 | 前端 + 后端 | 显示已登录/未登录/已过期状态，过期时提示重新登录 | 未开始 |

### Phase 4：Failover 整合（预计 2-3 天）

| # | 任务 | 文件 | 说明 | 状态 |
|---|------|------|------|------|
| 14 | ProviderSource 枚举 | `src/ai/client.rs`（更新） | 扩展为 ApiKey / OAuth / Subscription 三种来源 | 未开始 |
| 15 | 跨来源 Failover | `src/ai/client.rs`（更新） | 主 Provider 失败时自动切换到备用，支持跨来源（如订阅→API Key→OAuth） | 未开始 |
| 16 | 优先级配置 | `src/routes/ai.rs` + 前端 | 用户可拖拽排序 Failover 优先级 | 未开始 |

---

## 关键技术决策记录

### 决策 1：三家全部走 OpenAI 兼容端点
- **原因**：InnoForge 的 `src/ai/client.rs` 本就是 OpenAI 兼容客户端
- **Anthropic**：2025 年后推出 OpenAI 兼容端点 `https://api.anthropic.com/v1/chat/completions`
- **Gemini**：已有 `v1beta/openai/` 兼容端点
- **影响**：API 方式接入开发量几乎为零

### 决策 2：订阅方式用 WebView + Cookie 代理
- **原因**：ChatGPT Plus / Claude Pro 不提供 OAuth 给订阅用户
- **方案**：内嵌 WebView 让用户在官方页面登录 → 提取 Cookie → 代理调后端 API
- **借鉴**：Cursor / Monica / ChatGPTNextWeb 的实现方式
- **风险**：可能违反第三方 ToS，标注为"实验性功能"

### 决策 3：Gemini 订阅方式无需开发
- **原因**：InnoForge 已实现 gcloud OAuth 登录（`src/routes/auth.rs`）
- **影响**：Gemini 的 API + 订阅两种方式都已覆盖

---

## 给其他 AI 协作者的说明

### 接手 Phase 1 收尾

1. clone 仓库：`git clone http://127.0.0.1:9800/git/github/jsshwqz/innoforge.git`
2. 切到 dev 分支
3. 读 `src/ai/providers.rs` 了解预设结构
4. 在 `static/settings.html` 的 AI 配置区加三个按钮：
   - 点击"OpenAI 预设" → 自动填入 base_url=`https://api.openai.com/v1`, model=`gpt-4o`
   - 点击"Claude 预设" → 自动填入 base_url=`https://api.anthropic.com/v1`, model=`claude-sonnet-4-6`
   - 点击"Gemini 预设" → 自动填入 base_url=`https://generativelanguage.googleapis.com/v1beta/openai/`, model=`gemini-2.0-flash`
5. 在 `src/routes/ai.rs` 加 `POST /api/ai/test-connection` 端点
6. push 到 dev 分支

### 接手 Phase 2

1. 先读 `docs/plans/2026-10-02-top3-ai-integration/03-implementation.md` 了解技术细节
2. 新建 `src/ai/subscription/` 目录
3. 按 03-implementation.md 中的 Rust 代码模板实现
4. 注意：Cookie 代理有合规风险，需标注"实验性功能"

### 注意事项

- **不要修改 `src/ai/client.rs` 的 failover 逻辑**（Phase 4 才动）
- **providers.rs 中的模型名可能需要更新**（AI 模型迭代快，确认最新可用模型）
- **Anthropic 兼容端点的确切 URL 需实测确认**（文档搜索受区域限制，未能在本会话中 100% 确认）
- git push 代理偶尔 502，重试即可

---

## 文件变更清单

### 新增文件
```
src/ai/providers.rs
docs/plans/2026-10-02-top3-ai-integration/README.md
docs/plans/2026-10-02-top3-ai-integration/01-status-analysis.md
docs/plans/2026-10-02-top3-ai-integration/02-subscription-design.md
docs/plans/2026-10-02-top3-ai-integration/03-implementation.md
docs/plans/2026-10-02-top3-ai-integration/04-integration-roadmap.md
docs/plans/2026-10-02-top3-ai-integration/05-risks-summary.md
docs/plans/2026-10-02-top3-ai-integration/PROGRESS.md  ← 本文件
```

### 修改文件
```
src/ai/mod.rs          ← 新增 mod providers + 导出
.env.example           ← 新增三家预设注释示例
```

### 待修改文件（后续 Phase）
```
static/settings.html   ← Phase 1 收尾 + Phase 3
src/routes/ai.rs       ← Phase 1 收尾 + Phase 2 + Phase 4
src/ai/client.rs       ← Phase 4
src/db/                ← Phase 2
```
