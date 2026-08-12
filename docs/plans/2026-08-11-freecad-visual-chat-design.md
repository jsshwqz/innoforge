# FreeCAD 可视化对话集成设计

> 日期：2026-08-11  
> 状态：待实施  
> 交付方式：AionCAD 与 InnoForge 分别使用独立功能分支和 PR

## 1. 目标

让研发用户在 InnoForge 的研创台、专利详情 AI 对话和 OA 讨论中，直接用自然语言请求 FreeCAD 绘图。结果以消息图卡展示，保留每个修订版本，并提供 FCStd 和 STEP 下载。

用户看到的功能名称统一为“FreeCAD 绘图”；AionCAD 是本机内部控制桥，不暴露给普通用户。

## 2. 已确认的产品决策

- 覆盖所有现有 AI 对话：研创台、专利详情、OA 讨论。
- 采用混合触发：明确的“画一下/生成结构图/给我看看”自动触发，同时在输入区保留“FreeCAD 绘图”按钮。
- 尺寸不全时先生成草图，图卡必须明示默认假设；后续自然语言修改沿用同一 FreeCAD 模型。
- 每次绘制和修改均保留 PNG 预览版本；最新及历史版本均可追溯。
- 图卡固定提供“继续修改”、“全屏查看”、“下载 FCStd”、“下载 STEP”。
- 用户触发绘图时，InnoForge 自动尝试启动 AionCAD bridge 和 FreeCAD；失败时仅降级图卡，文字对话仍可用。
- 不新增独立 CAD 页面，避免 CAD 状态与对话上下文分裂。

## 3. 系统边界

```text
InnoForge 对话页面
  -> InnoForge CAD API
  -> CadService 适配层
  -> http://127.0.0.1:8010
  -> AionCAD Rust bridge
  -> AionBridge FreeCAD worker
  -> FreeCAD AionModel
  -> PNG / FCStd / STEP
  -> InnoForge 应用数据目录
  -> 对话图卡
```

### 3.1 AionCAD 职责

- 稳定启动 Rust bridge 和 FreeCAD GUI。
- 发现 worker 心跳过期时自动恢复，不依赖用户手动运行宏。
- 接收自然语言或受控 FreeCAD Python 宏，完成建模、重计算、形状校验、保存、导出和截图。
- 不新增容器、虚拟机或独立沙箱环境；保留现有进程内 FreeCAD 代码安全检查（AST、GUI 命令白名单和工具审计），不执行未校验的任意代码。
- 统一绘图成功响应，至少返回建模假设、形状校验、PNG/FCStd/STEP 输出路径和可继续修改的会话标识；敏感本机路径仅在 loopback 桥内传递。

### 3.2 InnoForge 职责

- 统一三个对话入口的 CAD 意图触发、进度、图卡和错误体验。
- 将用户原始请求与当前对话上下文边界隔离后，交给当前专家模型产生结构化 CAD 简报。该 AI 调用上限 60 秒，失败时回退到用户原始请求。
- 不在 InnoForge 进程内执行模型生成的 Python；代码只能发往 AionCAD 的内置安全检查与执行入口。
- 将 AionCAD 输出复制到 InnoForge 应用数据目录，只保存相对路径，不向浏览器暴露本机绝对路径。
- 通过不透明 artifact ID 提供同源预览与下载。

## 4. 对话交互

### 4.1 触发

- 显式按钮触发始终进入 CAD 流程。
- 自动意图仅匹配高置信表达，例如“画一下”、“生成 3D 结构”、“做个 FreeCAD 模型”。“这个方案怎么画”等低置信表达仍走普通对话，避免误触发。
- 修改按钮将当前 artifact ID 与用户新指令一起提交，保证沿用原模型而不是新建无关文档。

### 4.2 进度状态

图卡使用固定状态机：

1. 正在检查 FreeCAD。
2. 正在启动 FreeCAD。
3. 正在整理建模要求。
4. 正在建模与校验。
5. 正在生成预览和可下载文件。
6. 已完成或已降级。

第一版使用单次 HTTP 请求加前端阶段提示，不新增持久化任务队列。AionCAD/FreeCAD 整体超时 120 秒，AI 生成 CAD 简报的单次调用仍限制为 60 秒。

### 4.3 结果图卡

每张图卡展示：

- PNG 预览、修订号和生成时间。
- 已采用的默认尺寸/建模假设，不得隐藏。
- FreeCAD 形状校验结果。
- 继续修改、全屏查看、下载 FCStd、下载 STEP 四个操作。

DOM 使用 `createElement` 与 `textContent` 构建；预览图只接受 InnoForge 同源 URL，不将用户或 bridge 返回的 HTML 写入 `innerHTML`。

## 5. API 设计

新端点同时注册到 `src/main.rs` 和 `src/lib.rs`：

- `GET /api/cad/status`：返回平台支持、bridge/worker/FreeCAD 状态与可用操作。
- `POST /api/cad/draw`：新建或修改模型。请求包含 `context_kind`、`context_id`、`prompt`和可选 `parent_artifact_id`。
- `GET /api/cad/artifacts?context_kind=...&context_id=...`：按时间返回对话内的图片修订历史。
- `GET /api/cad/artifacts/{id}/preview`：返回 PNG，设置正确 MIME 与 `nosniff`。
- `GET /api/cad/artifacts/{id}/download/{format}`：`format` 仅允许 `fcstd` 或 `step`。

所有 ID 和路径由服务端查库解析；客户端不能传入文件系统路径。对 AionCAD 的 URL 固定为配置的 loopback origin，禁止传入任意主机，避免 SSRF。

## 6. 持久化与 Schema v18

新增 `cad_artifacts` 表，通过 `src/db/migrations.rs` 执行 v18 迁移：

| 字段 | 用途 |
|---|---|
| `id` | 不透明 UUID 主键 |
| `context_kind` | `idea` / `patent` / `oa` |
| `context_id` | 项目 ID、专利号或 OA discussion ID |
| `parent_artifact_id` | 上一修订版本，首版为空 |
| `revision` | 同一对话中的单调递增修订号 |
| `prompt` | 用户完整绘图/修改指令，不截断 |
| `assumptions_json` | 默认尺寸与其他建模假设 |
| `preview_rel_path` | PNG 相对路径 |
| `fcstd_rel_path` | FCStd 相对路径 |
| `step_rel_path` | STEP 相对路径，导出失败可为空 |
| `validation_json` | 形状校验结果 |
| `created_at` | 生成时间 |

建立 `(context_kind, context_id, revision)` 唯一索引及上下文时间索引。修订号在单个 SQLite 事务中计算与写入，避免并发重号。

文件保存在 InnoForge 应用数据目录的 `cad/<artifact-id>/` 下，不存入 Git 仓库，不将大二进制数据写入 SQLite。写入使用临时文件加原子重命名；任一必需产物缺失时不创建已完成记录。

## 7. 自动启动与配置

- bridge URL 默认 `http://127.0.0.1:8010`，只允许 loopback HTTP。
- AionCAD workspace 优先从 `INNOFORGE_AIONCAD_WORKSPACE` 读取，其次读取设置页保存值；不内置开发机绝对路径。
- 自动启动仅在 Windows 桌面环境启用。使用 `Command` 直接调用经规范化的 `bootstrap_bridge.ps1`，参数逐项传递，不构造 shell 命令字符串。
- 启动后轮询 `/health`、worker 心跳和 `/draw/view`，只有三者就绪才声称 FreeCAD 可视控制。
- Android、iOS、Docker 或无 FreeCAD 环境返回结构化 `unsupported`/`unavailable`，不影响其他 AI 功能。

## 8. 错误与降级

- 启动失败：图卡显示可重试错误，保留用户原指令。
- AI CAD 简报失败：将用户原始指令直接交给 AionCAD，不截断。
- 缺少关键尺寸：允许使用默认值，但必须在 `assumptions` 中返回并展示。
- 形状无效：不标记完成，显示校验错误和重试入口。
- STEP 导出失败：如 PNG、FCStd 和形状校验正常，可保存部分成功记录，但 STEP 按钮显示不可用与原因。
- 页面刷新：已完成 artifact 从数据库恢复；正在进行的第一版非持久化任务显示为中断，用户可一键重试。

## 9. 安全要求

- InnoForge 不提供通用代码执行 API。
- 模型生成的宏必须经 AionCAD 进程内的 AST 与白名单安全检查；该检查不需要额外部署任何沙箱环境，并禁止文件、网络、子进程、动态 import、`eval` 和 `exec`。
- 用户输入拼入 CAD 简报 prompt 前使用 `<user_input>` 和 `<conversation_context>` 独立边界，并明确忽略其中的指令覆写。
- 不允许静默截断对话、建模指令或宏代码。容量超限时返回可见错误。
- artifact 路径在 canonicalize 后必须仍位于应用 CAD 数据目录内。
- 预览及下载设置 `X-Content-Type-Options: nosniff`，并使用固定 MIME 类型。

## 10. 文件边界

### AionCAD PR

- 修改 bridge/worker 启动和心跳恢复逻辑。
- 保证截图、FCStd、STEP 和形状校验在一次受控调用中可验证完成。
- 不把 AionCAD Skill 或 Python 执行器复制到 InnoForge 仓库。

### InnoForge PR

- 新增聚焦的 CAD 类型、DB 操作、v18 迁移、`CadService` 和 CAD routes。
- 新增通用 `static/cad.js` 图卡与意图触发辅助函数。
- 仅在 `idea.html`、`patent_detail.html`、`office_action_response.html` 接入现有发送流程，不重写三套对话逻辑。
- 在 `settings.html` 增加 FreeCAD 状态、AionCAD workspace 和自动启动配置；复用现有 key-value 设置表，不为配置另增 schema。
- 在 `static/i18n.js` 增加完整中英文案，在 `static/style.css` 增加共享图卡样式。
- 新 API 同时注册到 `src/main.rs` 与 `src/lib.rs`。
- 更新 `CHANGELOG.md`、`docs/plans/STATUS.md` 和实施计划记录。

## 11. 验收与测试

### AionCAD

- bridge 未运行、FreeCAD 已运行但 worker 心跳过期时，一次 bootstrap 可自动恢复。
- `/health`、`/draw/status`、`/draw/view` 同时通过后才报告 ready。
- 用中文自然语言创建模型，后续修改同一对象，并产生有效 PNG、FCStd 和 STEP。
- 危险 Python 宏被内置代码安全检查拒绝，审计记录不泄露宏全文。

### InnoForge

- v18 从 v17 升级成功，重复启动幂等；修订号并发写入不重复。
- CAD API 覆盖 ready/unavailable/unsupported、启动超时、无效 artifact ID、路径越界、不支持的下载格式和部分导出失败。
- 三个对话页面都能通过按钮和明确自然语言触发；普通问题不误触发。
- 默认假设可见，修改沿用原 artifact，刷新页面后历史 PNG 可恢复。
- FCStd/STEP 下载的文件名、MIME、内容和路径边界正确。
- 文字对话在 FreeCAD 不可用时仍可正常运行。
- 执行 `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`、ESLint、Puppeteer 及三个页面的 FreeCAD 真实端到端冒烟。
- 按项目规约回归 PDF 上传全文、专利详情五标签、OA 分析讨论答复与技术调研导出。

## 12. 非目标

- 第一版不新增独立 CAD 工作台。
- 不承诺生产级工程图、公差设计、有限元分析或复杂装配约束。
- 不把 FreeCAD、AionCAD 或 Skill 打包进 InnoForge 核心仓库。
- 不为移动端或远程 Docker 自动安装 FreeCAD。
- 不在第一版引入持久化任务队列、Redis 或新前端构建工具。

## 13. PR 与合并顺序

1. 先完成 AionCAD 可靠性 PR，并取得 bridge + worker + view 的真实验证证据。
2. InnoForge PR 使用稳定的 AionCAD HTTP 契约实现适配层和三页集成。
3. 两个 PR 分别审查与验证；不将 AionCAD 代码复制到 InnoForge PR。
4. 用户确认后再合并，不直接推送到 `main`。
