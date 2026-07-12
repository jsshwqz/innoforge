# 2026-07-12 OA 数据完整性修复计划

## 背景与目标

InnoForge 面向研发人员提供专利分析与 OA（审查意见）答复支持。本轮对当前功能链进行基线审计后，确认 `templates/office_action_response.html` 有多处将上传的专利全文、对比文件、审查意见或既有分析结果截断后再提交给 AI 的逻辑。该行为会丢失证据上下文，与 `AGENTS.md` 的“用于提交给后端或传给 AI 的数据必须保留全文”强制规范冲突。

本轮目标：先消除已确认的数据完整性缺陷，并以最小改动验证 OA 相关请求仍可用；不增加依赖、不改数据库 schema、不删改 API。

## 执行前基线（2026-07-12）

- 当前包版本：`Cargo.toml` 为 `0.7.2`；`STATUS.md` 和 `CHANGELOG.md` 存在待后续统一核对的版本记录差异。本轮不擅自发布或改版本号。
- Git 工作区：无已跟踪的未提交改动；仓库根目录有本地运行产物与敏感配置，均不纳入本轮变更。
- 已确认问题：OA 页面在修改校验、讨论初始化、讨论消息上下文中使用 `.slice(0, N)` 截断将发送给 AI 的材料。
- Aion Forge 协作能力核验：`.mcp.json` 仅配置 MinerU，当前可用 MCP 工具中也不存在 Aion Forge；不具备可调用能力。
- 决策：本轮弃用 Aion Forge 协作，不修改其配置（无可验证的项目内缺陷，也避免将无关的工具链改动带入核心仓库）。
- Skill 决策：遵照用户要求，未调用任何电脑现有 Skill。此任务可由项目规约、源码审计和 Codex 协作完成，不在核心仓库创建 MCP Skill/Plugin（项目规约要求二者置于独立仓库）。

## 任务拆分与责任

| 编号 | 任务 | 责任 | 完成标准 |
| --- | --- | --- | --- |
| T1 | 复核 OA 页面中每一处截断的数据用途，区分展示截断与 AI 请求截断 | 审计代理 | 列出受影响调用点与保留/修改理由 |
| T2 | 移除确认会进入 AI 请求的截断；保持展示用截断和导出文件名逻辑不变 | Codex 实现代理 | 仅修改必要模板代码，不新增依赖/API/schema |
| T3 | 审查补丁的 XSS/i18n 影响和请求载荷构造 | 主代理 | 无新增不安全 HTML 注入；不影响中英文文案 |
| T4 | 执行格式、静态检查、Rust 检查和可用的自动化回归 | 主代理 + Codex 实现代理 | 记录每条命令结果；失败必须修复或明确阻塞 |
| T5 | 更新状态、CHANGELOG/错误复盘与本计划复盘 | 主代理 | 文档保留问题、决策、证据与后续项 |

## 变更边界与风险控制

- 不新增 crate、npm 包或运行服务。
- 不更改数据库 schema、迁移、路由和公共 API。
- 不处理与本缺陷无关的历史 `innerHTML` 技术债务；单独列为后续审计项，避免未经确认的大范围重构。
- 由 Codex 实现代理执行代码编辑；主代理只负责规划、审查、验证和记录。
- AI 上下文可能变大：这是为保证材料完整性的必要结果；后端已有各端点的输入控制与 provider 超时策略，若验证发现请求体上限问题，停止扩大范围并另行规划。

## 验证方案

1. 检查 OA 页面所有 AI 请求字段，确认不再对正文资料使用 `.slice()` / `substring()`。
2. 运行 `cargo fmt --check`。
3. 运行 `cargo clippy -- -D warnings`；若失败，先处理警告再继续。
4. 运行 `cargo test`。
5. 因修改模板，运行 ESLint；若存在 `e2e_test.mjs` 才运行 Puppeteer。当前仅检测到 Puppeteer 依赖，未检测到该脚本，因此该项应记录为不可执行而非伪造通过。
6. 手动核查 OA 页面中的“修改校验”和“讨论”请求体保留全文。真实 AI 调用需要用户凭据，不以真实外部请求作为本轮通过条件。

## 复盘模板（完成后填写）

- 实际修改 / Actual changes：✅ 已完成。移除 6 个进入 AI 请求的数据截断表达式，覆盖计划中的 5 个逻辑问题组；未改依赖、API、schema 或用户文案。
  Completed. Removed six AI-bound truncation expressions across the five planned logical issue groups; no dependencies, APIs, schema, or user-facing copy changed.
- 验证证据 / Verification evidence：`cargo fmt --check` 通过；`cargo clippy -- -D warnings` 通过；`cargo test` 245 项通过、0 失败、1 个既有文档测试忽略；定制 Puppeteer 回归 6 项完整性断言全部通过。
  Formatting and clippy passed; 245 Rust tests passed with zero failures and one existing ignored doc test; all six focused Puppeteer integrity assertions passed.
- 发现的偏差与处理 / Deviations：仓库有 Puppeteer 依赖但缺少 `e2e_test.mjs`，因此建立 D 盘临时回归脚本并在验证后清理；模板 ESLint 当前与 HEAD 均为 16 errors / 387 warnings，本补丁新增 0。
  Puppeteer is installed but `e2e_test.mjs` is missing, so a temporary D-drive regression was created and cleaned up; template ESLint is 16 errors / 387 warnings both before and after, with zero new findings.
- 后续建议 / Follow-ups：另行规划 `src/routes/ai.rs` 的 60k/40k/15k 字符静默截断治理，并恢复官方 E2E 脚本、清理 OA 模板 ESLint 基线。
  Plan a separate remediation for the backend 60k/40k/15k silent truncations, restore the official E2E script, and clean the OA template ESLint baseline.
- 提交号 / Commit：`f496648`

## 任务状态 / Task status

✅ T1~T5 已完成 / T1–T5 completed（代码提交 / code commit: `f496648`）。
