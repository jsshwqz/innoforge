# OA 答复书导入 → 生成流程：当前状态报告

> 日期：2026-07-15
> 作者：Aion Forge
> 分支：`feat/oa-fact-check`
> 修复验证：2026-07-15（本地端口 3000，服务已在验证后停止）

---

## 一、核心目标

**用户的原始诉求（原话）**："导入就是为了绕过分析，直接生成答复书"

用户想走的路径是：上传 OA PDF → 导入讨论记录 →（可选聊几句）→ 生成答复书 → 导出 Word。

**不依赖**：先做 AI 分析（三步/一步合并后的分析流程）。

---

## 二、当前进度总览

### ✅ 已完成

| 序号 | 项目 | 说明 |
|------|------|------|
| 1 | OA 三步→一步分析重构 | `src/ai/patent.rs` 合并为单步，消除超时 |
| 2 | OA 缓存机制 | 按专利号+OA类型+深度缓存分析结果 |
| 3 | OA 讨论持久化 | `POST /api/ai/oas/{patent}/discuss` SSE 流式保存完整 JSON 历史到 SQLite |
| 4 | OA 讨论历史恢复 | `GET /api/ai/oas/{patent}/discussions` 列表 + 详情 API |
| 5 | 讨论导出 | 纯本地 Markdown 导出，不调 AI，保留原文+角色+时间戳 |
| 6 | 导入按钮 UI 改造 | 从"先点分析才出现"移到页面顶部，上传 OA 后即可使用 |
| 7 | 导入 API 路径修复 | 后端 `POST /api/ai/oas/{patent}/discussions/import` 返回 `{"ok":true, "status":"ok"}` |
| 8 | 专利号提取修复 | 支持从 URL、localStorage、OA 文件名（正则 `\b\d{10,}\b`）提取 |
| 9 | DB 迁移 v17 | `ALTER TABLE oa_discussions ADD COLUMN oa_text TEXT` + `SCHEMA_VERSION: 17` |
| 10 | 导入流程 OA 文本传递 | 前端 POST 时把 `uploadedData['oa'].content` 传入 `oa_text` 字段 |
| 11 | 生成答复书绕过分析检查 | 移除了 `analysis_text.len() < 50` 的硬性拒绝；无分析时 fallback 用 OA 文本作为伪分析 |
| 12 | 导入后讨论面板渲染 | 新 `renderImportDiscussionPanel()` 内联渲染讨论历史 + 输入框 + 生成按钮（非弹窗） |
| 13 | 生成输出全屏面板 | `generateResponseLetter()` 在无 `#section5-content` div 时降级为全屏 overlay |
| 14 | AI prompt 分流（导入 vs 正常） | 新增 `is_import_flow` 分支：导入流程 prompt 让 AI 基于 OA 原文独立完成分析+撰写 |

### ✅ 本次修复并验证

| 序号 | 问题 | 严重程度 | 详细说明 |
|------|------|----------|----------|
| A | **答复书显示为空或仅有符号** | 🔴 已修复 | 后端 SSE 使用标准 `data:` 格式；前端原先只接受 `data: `（多一个空格），从而丢弃完整正文。现已兼容两种格式并保留换行。 |
| B | **DOCX 导出空白** | 🔴 已修复 | 生成正文现能完整进入前端；实际调用导出接口得到有效、非空 DOCX（ZIP 文件头 `PK`）。 |
| C | **迁移重复执行失败** | 🟡 已修复 | v17 迁移改为检查 `oa_text` 列是否存在，再写入版本号；同一数据库重复初始化通过。 |

### 实测结论

- 浏览器实际请求生成接口，收到 2,748 个字符的中文答复，其中 2,433 个为实质中文、字母或数字字符。
- 浏览器端到端自动化回归：48/48 通过。
- Rust 全量测试：135 + 137 + 6 + 37 项通过；`cargo clippy --all-targets -- -D warnings` 通过。
- AI 全局超时维持 300 秒，未改回 60 秒。

---

## 三、原问题的详细分析（已由实测修正）

### 问题根因链路

```
用户操作: 上传 OA PDF → 导入讨论记录 → 点"生成意见陈述书"
        ↓
前端调用: POST /api/ai/oas/{patent}/generate-response-letter
  参数: analysis_text = "" (空,因为没有做分析)
        office_action = "{OCR 识别的乱码文本}" (OA PDF 提取的文字)
        discussion_json = "[{'role':'user','content':'...'}]" (导入的讨论)
        oa_text = "{OCR 识别的乱码文本}" (已正确传递)
        is_import_flow = false (这个字段目前没用上)
        ↓
后端: generate_response_letter_stream()
  检测到 is_import_flow = true (analysis_text 为空)
  使用新 prompt: "基于审查意见通知书原文和用户讨论内容，独立分析并撰写答复书"
  ↓
AI 调用: 发送给 DeepSeek
  system prompt: 长篇指令，要求独立分析+撰写
  user prompt: OA 原文(乱码) + 讨论内容(用户说"帮我生成"等) + 输出格式模板
  ↓
AI 输出: 只生成了 209 字符装饰符号
```

### 实际根因：SSE 前端解析不兼容

后端实际已返回完整中文答复；问题出在浏览器端的流式解析只匹配 `data: `，而 Axum SSE 发送的是同样符合规范的 `data:`。前端因此忽略了正文事件，后续导出自然为空。现已改为接受所有 `data:` 前缀，并只移除可选的一个空格。

### OCR 质量仍是输入质量风险（不是本次空白的根因）

1. **OCR 质量差**：用户上传的 OA PDF 是扫描件，OCR 提取出的文本是乱码（含大量 `【】`、`*`、`=` 等符号），AI 无法从中解析出有意义的审查意见
2. **prompt 太长但有效信息太少**：系统指令 400+ 字符 + 用户输入（大量乱码 OCR 文本 + 简短讨论） = 有效信号被噪声淹没
3. **AI 模型可能能力不足**：DeepSeek 面对这种"OCR 乱码 + 无分析基础"的输入，无法可靠地"独立分析+撰写"

### DOCX 导出的问题（已修复）

- 之前导出功能接收到的是前端丢失后的空白/符号内容。
- 现在使用实际生成结果调用导出接口，已得到有效的非空 DOCX。

---

## 四、数据流实证（前端 → 后端 → AI）

### 生成请求的前端参数（导入流程）

```javascript
// 导入流程中，以下变量值：
lastAnalysisRawText = ''             // 未执行分析，始终为空
oaDiscussOAText = "{OCR乱码}"        // OA 文件名: "王青芝 2022104773249 二审.pdf"
oaDiscussOAType = 'second'          // 二审
oaDiscussMessages = [
    { role: 'user', content: '帮我生成...', ... },
    { role: 'assistant', content: '好的...', ... },
    // ... 导入的讨论记录
]
```

### 后端接收到的 JSON

```json
{
  "analysis": "",
  "discussion": "[{\"role\":\"user\",\"content\":\"...\"}]",
  "office_action": "{OCR乱码文本}",
  "oa_type": "second",
  "discussion_id": ""
}
```

### AI 实际收到的内容

- **system prompt**（导入流程版本）：要求 AI "基于审查意见通知书原文，独立完成分析并撰写"
- **user prompt**：
  - "## 原始审查意见通知书" → OCR 乱码（大量 `【】`、`*`、`=`）
  - "## 讨论记录" → 用户简短对话（如"帮我生成"）
  - "输出格式" → 长篇模板（意见陈述书结构）

### 关键问题：prompt 中有效信息占比

| 内容 | 字符数（估） | 有效信息 |
|------|------------|----------|
| 系统指令 | ~500 | 格式要求 |
| OA 原文（OCR） | ~3000 | 几乎全是乱码 |
| 讨论记录 | ~500 | 用户说"帮我生成" |
| 输出格式模板 | ~600 | 格式要求 |
| **总计** | ~4600 | **实质内容 < 200 字符** |

### 为什么 AI 输出装饰符号

DeepSeek 模型面对大量 OCR 乱码输入，无法从中提取有意义的审查意见，于是生成了"看起来像文档"的占位内容——这就是为什么结果只有 `=`、`*`、`【】` 等装饰符号，没有实质论述。

### DOCX 导出为什么空白

```
AI 生成文本（209 字符装饰符号）
  → window._generatedResponseText = "============..."
  → exportGeneratedResponse() 调用：
      text.replace(/^#+\s*/gm, '')  // 移除标题标记
      text.replace(/\*+/g, '')       // 移除星号（把装饰符号也删了）
  → 传给后端 /api/ai/oa-export-docx
  → 后端生成 DOCX，内容为空白或接近空白
```

---

## 五、后续优化建议

### 优先级 1：解决 OCR 输入质量问题

**方案 A（推荐）**：导入流程中增加一个"OCR 文本预览/手动修正"步骤
- 用户导入讨论后，先显示一个文本编辑框，展示 OCR 提取的内容
- 用户可手动删除乱码、补充关键信息
- 然后把修正后的文本作为 `oa_text` 传给生成接口

**方案 B（快速）**：在 prompt 中显式告知 AI "以下 OCR 文本可能有大量识别误差，请只关注能识别的部分，忽略乱码"

### 优先级 2：改善导入流程的 prompt

当前 prompt 在导入流程中仍然要求 AI"独立分析并撰写答复书"，但传入的是 OCR 乱码。建议：
- 区分"有讨论内容"和"无讨论内容"两种情况
- 如果讨论中有用户的明确意图（如"帮我修改权利要求1"），重点围绕讨论内容生成
- 如果讨论内容也很少，降级为"根据能识别的 OA 内容生成框架模板"

### 优先级 3：持续确保数据流完整

- 确认 `is_import_flow` 字段从前端传到后端并被正确使用（目前只是后端自行判断 analysis_text 是否为空）
- 回归测试应覆盖 SSE `data:` 与 `data: ` 两种格式。
- 确认生成响应中的 `response_text` 被正确传递到前端并可导出。

## 六、代码修改清单（未提交）

### 后端修改

| 文件 | 修改内容 | 变更量 |
|------|----------|--------|
| `src/ai/patent.rs` | `generate_response_letter_stream()` 增加 `is_import_flow` 分支，两条 prompt 路径 | +100/-38 |
| `src/db/mod.rs` | `SCHEMA_VERSION: 15 → 17` | +1/-1 |
| `src/db/migrations.rs` | 新增 v17 迁移：添加 `oa_text TEXT` 列 | +13 |
| `src/db/oa.rs` | `OaDiscussion` 结构体新增 `oa_text` 字段；5 个 SQL 查询同步更新 | +13/-14 |
| `src/routes/ai.rs` | `api_oa_discussion_import` 提取 `oa_text`；`api_ai_oa_discuss` 同步字段；`api_ai_oa_generate_response_letter` 移除硬检查+fallback | +24/-2 |

### 前端修改

| 文件 | 修改内容 | 变更量 |
|------|----------|--------|
| `templates/office_action_response.html` | 导入按钮移到顶部；`getPatentNumberFromUrl` 扩展；导入 POST body 含 `oa_text`；新 `renderImportDiscussionPanel()` 函数；生成输出全屏 overlay；修复 `uploadedData` 代理访问（3处 `_uploadedData` → `uploadedData`） | +141/-30 |

---

## 七、已知工作区临时文件

以下文件是调试过程中产生的，应当删除或归入 `.gitignore`：

```
temp_fix_gen.js
test_import.ps1
test_syntax.js
```

---

## 八、运行环境

- 操作系统：Windows x86_64
- Rust 版本：通过 `cargo build --release` 编译
- 验证服务端口：3000
- AI 提供商：DeepSeek（为主）
- DB：SQLite，`data/innoforge.db`
- 数据库当前 schema 版本：17
- 分支：`feat/oa-fact-check`（领先 origin 1 commit）

---

*报告生成时间：2026-07-15*
*Aion Forge — 如有任何遗漏，欢迎补充。*
