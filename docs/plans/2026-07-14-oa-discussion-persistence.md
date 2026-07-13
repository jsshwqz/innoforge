# 2026-07-14 OA 完整讨论持久化与恢复计划

## 目标

让研发用户在 OA 页面进行多轮 AI 讨论后，即使刷新页面、关闭浏览器或数日后重新进入，也能恢复同一案件的完整讨论上下文，并继续讨论或基于已确认讨论生成答复书。

## 已核实现状

- `oa_discussions` 表及 `OaDiscussion`、`save_oa_discussion()`、`get_oa_discussion()`、`list_oa_discussions()` 已存在。
- 前端仅用页面内存数组保存讨论；刷新即丢失。
- 前端不持有/发送固定 `discussion_id`；后端会为每轮请求新建 UUID。
- 后端当前只保存当前轮的 AI 流式输出，不保存完整 user/assistant 历史。
- 既有历史 OA 分析可在侧栏加载，但没有讨论会话列表、加载接口或恢复 UI。

## 范围

1. 使用现有 `oa_discussions` 表，不修改 schema 或新增迁移。
2. 将每个讨论会话稳定绑定到一个 `discussion_id`。
3. 后端保存完整、结构化、可验证的讨论历史 JSON。
4. 增加读取讨论列表与单个讨论的 API。
5. 前端增加讨论会话列表、加载恢复与继续讨论能力。
6. 恢复后，生成意见陈述书使用恢复后的完整讨论上下文。
7. 添加 Rust 单元/集成测试，以及前端变更需要的 ESLint 和 E2E 验证。

## 数据契约

### discussion_history

存储 JSON 数组，单项严格为：

```json
{"role":"user","content":"..."}
```

或：

```json
{"role":"assistant","content":"..."}
```

不接受客户端 `system`、`tool` 等角色作为 OA 讨论持久化内容；服务端系统提示词不入库。

### discussion_id

- 首次发送讨论：前端生成 UUID；后端在缺省时兼容生成 UUID。
- 后续讨论：前端始终提交同一个 ID。
- 后端响应在 SSE `done` 事件携带 ID，确保旧页面/兼容客户端可回填。

## API 设计

| 方法 | 路径 | 职责 |
|---|---|---|
| POST | `/api/ai/oas/{id}/discuss-stream` | 保持现有流式讨论；保存完整历史；`done` 返回 discussion_id |
| GET | `/api/ai/oas/{id}/discussions` | 返回该 OA 的讨论会话摘要列表 |
| GET | `/api/ai/oas/{id}/discussions/{discussion_id}` | 返回并校验属于该 OA 的完整讨论会话 |

> **路由注册说明**：当前桌面端与移动端均通过 `src/common.rs::build_router()` 构建路由；新增 API 在此统一注册即可同步覆盖 `main.rs` 和 `lib.rs`，不分别重复注册。

## 实施步骤

1. 扩展 `OaDiscussion` 数据模型，明确完整 JSON 历史与最后更新时间语义；增加按 `oa_analysis_id` 查询/归属验证的 DB 方法和测试。
2. 在讨论流路由中校验 JSON 历史，追加本轮 user/assistant，保存同一 `discussion_id` 的完整会话；使用用户友好错误处理，禁止 `unwrap()`。
3. 新增讨论列表/详情路由，避免跨 OA 读取。
4. 前端保存当前 discussion ID，渲染会话列表，加载时恢复消息和讨论面板；所有用户/AI内容使用既有 `renderMarkdown()` + DOMPurify 防护，不用用户数据拼接未过滤 `innerHTML`。
5. 增加 i18n 中英文本。
6. 更新 API、架构、STATUS、CHANGELOG 和本计划完成记录；复盘保存到仓库外 memory。
7. 执行 fmt、clippy、test、ESLint、Puppeteer E2E，并手工验证 OA：首次讨论→刷新→恢复→继续讨论→生成答复书。

## 风险与约束

- 本轮不做跨案件 RAG、自动模型训练或“自我学习”；先保证可审计的数据基础。
- 保持历史客户端兼容：旧客户端无 `discussion_id` 仍能获得后端生成的会话 ID。
- 讨论历史不得为节省 token 而截断；超过模型上下文时，应采用可见的结构化摘要/用户确认机制，不能静默丢数据。
- 当前工作树已有未提交认证和超时修复，实施不得还原或覆盖这些修改。

## 验收标准

- 同一讨论每轮使用同一 `discussion_id`。
- 刷新 OA 页面后可以加载完整讨论并继续对话。
- 恢复讨论后生成答复书能带入完整恢复历史。
- 无法通过 URL/ID 读取其他 OA 的讨论。
- Rust 与前端全部质量门禁通过。

## 完成记录 / Completion

- **状态 / Status**: ⏳ 实施中 / In progress
- **代码提交 / Code commit**: 待完成后填写 / To be filled after completion
