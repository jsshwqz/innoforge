# T1 交接文档 — 对比文献自动获取与全文分析

> 创建时间：2026-10-03
> 作者：T1 负责 Agent
> 目的：供其他 AI 接手 T1 后续工作

---

## 一、任务概述

T1：对比文献自动获取与全文分析（第一阶段分工计划书）

- 后端端点：`POST /api/ai/oa-fetch-refs`
- 功能：从 OA 文本提取公开号 → 查本地库 → 在线搜索链 → 返回全文

---

## 二、已完成的工作

### 2.1 基础实现（commit `dedca85`，已在 main 上）

- **后端** `src/routes/ai.rs`：`api_ai_oa_fetch_refs` 函数
  - 正则提取公开号（CN/US/EP/JP/KR/WO）
  - `get_patent` 直接查本地 + `search_smart_exact` 本地 FTS5
- **前端** `templates/office_action_response.html`：
  - `autoFetchRefs()` 函数 + "自动获取对比文献"按钮
  - 文献卡片渲染 + 全文注入 `uploadedData['ref_N']`
  - DOMPurify.sanitize() 用于 innerHTML
- **i18n** `static/i18n.js`：5 个 `oa.refs.*` key 中英双语
- **路由** `src/common.rs:324`：已注册

### 2.2 在线搜索链 fallback（commit `6427a03`，在 `feat/oa-deadline-enhance` 分支）

**这是我补的工作。** 原实现缺少规划要求的"本地未找到则尝试在线搜索链"。

新增内容（仅 `src/routes/ai.rs`，+115 行）：

- 新增 `try_online_search()` 函数
- 降级链：`get_patent` → `search_smart_exact` → SerpAPI → Google Patents → EPO OPS
- 10 秒超时保护
- 失败优雅降级为"未找到"
- 搜索结果标记 `source: "online"`

---

## 三、未完成 / 待集成

### 3.1 代码未合并到集成分支

我的 T1 代码在 `feat/oa-deadline-enhance` 分支上（commit `6427a03`），**尚未合并到 main 或集成分支**。

```bash
# cherry-pick（推荐）
git cherry-pick 6427a03
```

### 3.2 可能的冲突点

- `src/routes/ai.rs`：T3 也改了此文件。我的改动在 `api_ai_oa_fetch_refs` 函数内和新增 `try_online_search` 函数，与 T3 改动区域不重叠，但 cherry-pick 时可能遇到上下文偏移。

### 3.3 验收状态

| 验收项 | 状态 |
|--------|------|
| 输入含 CN103133144A 等公开号能自动提取并搜索 | ✅ |
| 搜索结果含标题和摘要 | ✅ |
| 未找到显示"未找到全文" | ✅ |
| 全文能参与后续 AI 分析 | ✅ |
| 不破坏现有手动添加对比文献功能 | ✅ |

---

## 四、关键技术笔记（接手者必读）

### 4.1 Future Send 问题

`try_online_search` **不能**接收 `&AppState`——跨 `.await` 持有引用导致 future 不是 `Send`，axum Handler trait 编译失败。必须传入 cloned `Arc<Database>` 和 `Arc<RwLock<AppConfig>>`。

### 4.2 RwLock guard 作用域

`std::sync::RwLock` guard 必须在 `.await` 前释放。用作用域块限定（不要用 `drop(cfg)`，编译器有时无法证明）：

```rust
let (key, cred) = {
    let cfg = config.read().unwrap_or_else(|e| e.into_inner());
    // 读取配置...
    (key, cred)
}; // guard 在这里释放
```

### 4.3 在线搜索链构建

直接内联构建 providers，不调用 `search.rs` 的 `online_chain_providers`（private 函数 + private 模块）：

```rust
let mut providers: Vec<Arc<dyn SearchProvider>> = Vec::new();
if let Some(api_key) = serpapi_key {
    providers.push(Arc::new(SerpApiProvider::new(api_key, db.clone())));
}
providers.push(Arc::new(GooglePatentsXhrProvider::new(db.clone())));
if let Some((k, s)) = epo_credentials {
    providers.push(Arc::new(EpoOpsProvider::new(k, s, db.clone())));
}
```

---

## 五、职责边界

- 我只做 T1 代码，不更新共享看板（STATUS.md 其他任务行）、不替协调人做事
- 集成 / 报告上传 / 状态看板由协调人 / T5 负责

---

## 六、Commit 索引

| Commit | 分支 | 说明 |
|--------|------|------|
| `dedca85` | main | T1 基础实现（主 AI） |
| `6427a03` | feat/oa-deadline-enhance | T1 在线搜索链 fallback（我） |
