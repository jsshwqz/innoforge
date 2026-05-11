# 错误记录 / Error Record

> 日期：2026-05-11 | 版本：v0.5.8（待发） | 分支：claude/lucid-engelbart-8f807d

---

## 错误一：死代码删除误删活动函数

### 现象

在 `src/routes/search.rs` 中删除 6 个"死代码"函数块时，误删了三个仍在被 SerpAPI 搜索路径调用的函数：
- `calculate_online_relevance()` — 计算搜索结果与查询的相关性分数
- `is_online_result_relevant()` — 判断搜索结果是否相关
- `contains_cjk()` — 检测字符串是否包含中日韩文字

### 根因

"死代码"区域的界定不够准确，将相邻的活动函数也划入了删除范围。没有在删除前 grep 确认每个函数的引用关系。

### 修复

从 git HEAD 恢复这三个函数并重新插入到文件中，同时确认了它们的 5 参数 inventor bonus 签名。

### 教训

删除代码前，必须对每个函数做全代码库引用 grep。尤其对于 search.rs 这种 2000+ 行的大文件，"看起来像死代码"不等于"真的是死代码"。

---

## 错误二：`is_ascii_punct()` 编译错误

### 现象

恢复后的代码使用 `char::is_ascii_punct()` 编译失败：
```
error[E0599]: no method named `is_ascii_punct` found for type `char` in the current scope
```

### 根因

Rust 标准库中的 `char` 类型没有 `is_ascii_punct()` 方法，正确方法是 `is_ascii_punctuation()`。

### 修复

将 `is_ascii_punct()` 改为 `is_ascii_punctuation()`。

### 教训

恢复旧代码时不能机械地 checkout，需确认 API 兼容性。这个错误可能是因为旧代码基于不同的 Rust 版本或来自不同的 fork。

---

## 错误三：Edit 工具长文本匹配失败

### 现象

用 Edit 工具删除大段代码（如 Google Patents Direct 整块功能）时，`old_string` 参数与文件内容匹配失败，Edit 工具报错无法定位。

### 根因

编码差异或不可见字符差异（Bash 输出与文件实际内容之间可能存在制表符/空格/换行符差异）。200+ 行的 old_string 越界概率大。

### 修复

改用 `sed` 行范围删除操作绕过。例如 `sed -i '568,584d' src/routes/patent.rs`。

### 教训

大段删除（超过 10 行）应优先使用行号定位的 `sed` 或直接 `Write` 重写文件，Edit 工具适合精确定位的短文本替换。

---

## 错误四：运行中的进程锁定 Release 二进制

### 现象

`cargo build --release` 编译成功，但在部署时发现无法覆盖 `innoforge.exe`，因为服务器进程正在运行持有文件锁。

### 修复

先用 `taskkill /F /IM innoforge.exe` 终止进程，再拷贝部署。

### 教训

构建部署脚本中应先 kill 旧进程，后拷贝，再启动。标准顺序：`kill → copy → start`。

---

## 错误五：Claude Code 设置使用无效键 `allowMode`

### 现象

Claude Code 反复弹出工具权限确认窗口，尽管 `.claude/settings.local.json` 中已经配置了 `allowMode: "always"` 和 `defaultMode: "bypassPermissions"`。

### 根因

`allowMode` 不是一个有效的配置键。在 Claude Code 官方 schema 中，权限模式控制只通过 `defaultMode` 字段实现。`allowMode: "always"` 被运行时静默忽略，导致系统回退到默认模式，逐条匹配 `allow` 列表中的 263 条规则，漏掉的工具就弹权限确认。

### 修复

删除无效的 `allowMode` 字段，按 [官方文档](https://code.claude.com/docs/en/settings) 重写为：

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

同时更新了项目根目录和工作区的两个 `settings.local.json` 文件。

### 教训

1. Claude Code 的 settings 配置必须严格参照官方文档，不可凭猜测添加字段。
2. 修改 settings 后应主动验证格式正确性，而不是等到用户反馈才排查。
3. 官方文档路径：https://code.claude.com/docs/en/settings

---

## 错误六：AI 对话持久化缺失

### 现象

AI 助手页 (`/ai`) 和专利详情页 (`/patent/:id`) 的聊天内容在页面刷新后丢失，只有创意推演页 (`/idea/:id`) 的对话有持久化。

### 根因

`/ai` 和 `/patent/:id` 都调用 `api/ai/chat` 端点，该端点只返回 AI 响应数据，不做任何持久化存储。前端仅在 JS 内存中维护 `chatHistory` 数组。三个页面中只有 `/idea/:id` 的 `api/idea/:id/chat` 端点将消息写入 SQLite `idea_messages` 表。

### 修复

前端使用 `localStorage` 保存对话历史，页面刷新时自动加载渲染。

### 教训

新增 AI 对话页面时，必须从一开始就考虑持久化方案。对无 session/idea 概念的通用页面，`localStorage` 是最轻量的方案。

---

## 错误七：临时文件污染工作区

### 现象

`git status` 显示大量未跟踪文件（`*.tar.gz`、`test_*.json`、`search_*.json`、`server_*.txt`、`start_proxy.bat` 等），使状态检查变得困难。

### 原因

测试 API 时将响应数据保存到仓库根目录，编译产物拷贝到仓库根目录，没有使用临时目录或 `.gitignore` 管理。

### 建议

在 `.gitignore` 中添加常见测试产物模式，或在测试时使用系统临时目录。

---

## 错误八：专利详情页验证使用无效 patent ID

### 现象

验证 `/patent/:id` 页面是否正确渲染时，使用了不存在的 patent ID "test"，服务器返回 58 字节的"专利未找到"错误页面，导致验证代码认为模板修改未生效。

### 修复

改用 `curl` 获取已知存在的专利页面路径验证，或直接 grep 模板源文件确认代码存在。

### 教训

测试 URL 路径时，确保使用能正常渲染的实际数据路径。对静态模板内容，直接 grep 源文件比访问服务器更可靠。
