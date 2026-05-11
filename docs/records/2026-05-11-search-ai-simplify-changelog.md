# 改动记录 / Changelog

> 日期：2026-05-11 | 版本：v0.5.8（待发） | 分支：claude/lucid-engelbart-8f807d

---

## 修改一：搜索源精简 — 仅保留 SerpAPI

移除五个搜索源：Firecrawl / Bing / Lens.org / CNIPR / 搜狗，只保留 SerpAPI + 本地数据库。

### 文件变更

| 文件 | 操作 | 说明 |
|------|------|------|
| `src/routes/mod.rs` | **修改** | 删除 `bing_api_key` / `lens_api_key` / `firecrawl_api_key` / `firecrawl_api_url` / `cnipr_*` 字段及对应方法 |
| `src/routes/search.rs` | **修改** | 删除 Firecrawl 助手函数、Bing 搜索、Google Patents 直连、Sogou 免费搜索、CNIPR 搜索、extract 助手函数共 6 个死代码块；保留 `calculate_online_relevance()` / `is_online_result_relevant()` / `contains_cjk()` |
| `src/routes/settings.rs` | **修改** | 只保留 `serpapi_keys` + `ai_*` 的读写端点，删除 firecrawl/bing/lens/cnipr 的 save 端点 |
| `src/routes/idea.rs` | **修改** | `PipelineRunner::new` 和 `Orchestrator::new` 的构造调用移除 2 个废弃参数 |
| `src/routes/patent.rs` | **修改** | 删除 `fetch_legal_from_lens()` 调用，注释"Lens.org 已屏蔽" |
| `src/pipeline/steps/search.rs` | **修改** | `search_web()` 和 `search_patents()` 只走 SerpAPI，删除 Bing/Sogou/Lens.org 降级分支 |
| `src/pipeline/runner.rs` | **修改** | 删除 `bing_api_key` / `lens_api_key` 字段，构造函数改为 4 参数 |
| `src/orchestrator/engine.rs` | **修改** | 同步删除 `bing_api_key` / `lens_api_key` 字段 |
| `templates/settings.html` | **修改** | 状态检查只显示 SerpAPI + AI 两项，删除 Firecrawl/Bing/CNIPR 引用 |

### 涉及文件数：9 个 Rust 文件 + 1 个 HTML 模板（共 10 文件）

---

## 修改二：AI 服务商精简 — 仅保留 DeepSeek

删除 `AppConfig::ai_fallbacks` 字段，`ai_client()` 不再加载备用服务商列表（智谱 / OpenRouter / Gemini / OpenAI / NVIDIA / Ollama）。

### 文件变更

| 文件 | 操作 | 说明 |
|------|------|------|
| `src/routes/mod.rs` | **修改** | 删除 `AiFallback` 结构体、`ai_fallbacks` 字段、`from_db_and_env()` 中加载 fallback 的逻辑 |
| `src/ai/client.rs` | **修改** | 简化 `AiClient::with_config()`，不再接收 fallback 列表 |

---

## 修改三：AI 对话持久化 + 网络错误重试

三个 AI 对话入口中，之前只有创意推演 (`/idea/:id`) 的聊天保存到 SQLite 数据库。AI 助手 (`/ai`) 和专利详情 (`/patent/:id`) 的对话**仅存 JS 内存**，刷新即丢。且三个页面网络错误时都没有重试机制。

### 解决方案

**持久化**：使用浏览器 `localStorage` 保存对话历史，页面刷新后自动恢复。

| 页面 | localStorage Key | 说明 |
|------|-----------------|------|
| `/ai` | `innoforge_ai_chat` | 全局 AI 助手对话 |
| `/patent/:id` | `innoforge_chat_patent_{patent_number}` | 按专利号分别存储 |
| `/idea/:id` | 不变（已有 DB 持久化） | — |

**重试按钮**：三个页面在网络/服务器错误时，均显示 **🔄 重试** 按钮，点击自动重发上一条消息（带完整上下文）。

### 文件变更

| 文件 | 操作 | 说明 |
|------|------|------|
| `static/i18n.js` | **新增** | 添加 `ai.retry` 中英双语翻译键 |
| `templates/ai.html` | **修改** | 添加 localStorage 持久化（保存/加载/渲染）+ 重试按钮 |
| `templates/patent_detail.html` | **修改** | 添加 localStorage 持久化 + 重试按钮 |
| `templates/idea.html` | **修改** | 添加重试按钮（持久化已有 DB 支持） |

---

## 修改四：Claude Code 默认权限修复

### 根因

`.claude/settings.local.json` 中使用了 **无效配置键** `allowMode: "always"`，该键名在 Claude Code 官方 schema 中不存在，被运行时静默忽略。实际只有 `defaultMode: "bypassPermissions"` 是有效字段。

### 修正

依据 [Claude Code 官方设置文档](https://code.claude.com/docs/en/settings) 重写配置：

```json
{
  "permissions": {
    "allow": [],
    "deny": [],
    "defaultMode": "bypassPermissions",
    "skipDangerousModePermissionPrompt": true
  }
}
```

### 文件变更

| 文件 | 操作 |
|------|------|
| `D:\test\patent-hub-backup\.claude\settings.local.json` | **重写** — 删除无效 `allowMode`，采用官方格式 |
| `.claude/settings.local.json`（工作区） | **重写** — 同上 |

> **注意**：设置文件在 Claude Code 启动时加载，重启 `claude` 新会话后生效。
