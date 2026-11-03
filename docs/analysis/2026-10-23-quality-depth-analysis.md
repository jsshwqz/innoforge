# InnoForge（创研台）质量深度分析报告

> **分析对象**: https://github.com/jsshwqz/innoforge `main` 分支（commit `e1e6bf0`，v0.9.19）
> **分析日期**: 2026-10-23
> **委托背景**: 该工具由 GLM5.2 持续开发（99 commits），用户反馈"效率和质量太差"，要求以仓库版本为主做质量分析并给出改进计划
> **方法**: GitHub API 全量文件树 + 关键源文件逐个抓取分析 + 25 个 open issues 全量 + 项目自有的分析文档/流程文档，三路并行独立调查（架构质量 / 运行时性能 / 开发流程）。本报告只做分析，不改代码

---

## 一、项目现状画像

### 1.1 仓库形态

- **技术栈**: Rust（Axum 0.6 + tokio + rusqlite 0.31 bundled + reqwest 0.11）单体 Web 应用，前端为内联 JS 的 HTML 模板 + 少量静态文件；无 sqlx、无 rayon、无 `[profile.release]` 调优
- **双入口**: `main.rs`（桌面启动，仅调用 `common::{build_router, init_app_state}`）+ `lib.rs`（uniffi FFI，4 个 `extern "C"` 导出供移动端 WebView 壳调用）；`src/bin/mcp-server.rs` 为纯 HTTP 客户端的 MCP 服务
- **核心能力**: 16 步自动化查新流水线（串行状态机 + SSE 进度 + 断点续跑）、多源专利检索（SerpAPI / Google Patents / EPO OPS / 本地库级联）、9 个 AI provider 预设自动 failover、OA 答复辅助、CSV/Excel/PDF 导出
- **规模**: src/ 下约 40 万字节 Rust 源码；99 commits；25 个 open issues；3 个 PR
- **治理现状**: 三套并行优先级体系 —— `docs/progress/MASTER.md`（v4 里程碑主控，停在 2026-09-28，M-B 0/6、M-C 0/5 未开工）、issue #90 看板（活跃，滚动到 R32 轮次）、`DESIGN_v5_engine.md`（设计稿，标注"待用户拍板"）

### 1.2 模块体量（问题最突出的文件）

| 文件 | 大小 | 内部构成 |
|---|---|---|
| `templates/office_action_response.html` | 262 KB | 单个 HTML 模板 |
| `src/routes/ai.rs` | 151 KB | **31+ 个 axum handler**（chat 3 / 摘要对比分析 9 / OA 答复 7 / OA 历史持久化 7 / 风险导出成本 5） |
| `src/search/providers/epo_ops.rs` | 111 KB | OAuth + 重试退避 + CQL 渲染 + 手写 JSON 容错遍历 + 字段映射 + 限流记账，一体 |
| `src/routes/idea.rs` | 109 KB | 25 个 handler + 15 个辅助函数，另含约 600–700 行 prompt 文案与内置 Markdown 渲染器 |
| `src/ai/patent.rs` | 108 KB | 13 个 AI 方法 + 11 个 prompt builder；含 300+ 行的 `generate_response_letter_stream` |
| `src/routes/search.rs` | 90 KB | |
| `src/ai/response_pipeline.rs` | 68 KB | |
| `src/routes/settings.rs` / `upload.rs` | 各 ~58 KB | |
| `src/ai/fact_check.rs` / `ai/client.rs` | 52 KB / 51 KB | |
| `src/common.rs` | 36 KB | 6 类职责混杂的"杂物抽屉"（详见 Q4） |
| **`src/error.rs`** | **1.6 KB（约 60 行）** | **全部错误处理只有 3 个变体** —— 与上面 151 KB 的路由文件形成刺眼对比 |

对照：项目自有评估（`docs/analysis/comprehensive-code-analysis-2026-07-05.md`）自评 架构 4 / 性能 4 / 安全 4 / 文档 4 / **错误处理 3 / 可维护性 3 / 测试 2 / 可观测性 2**（满分 10）。本报告结论：错误处理与可维护性的实际状况比自评更差（理由见 Q1/Q3）。

---

## 二、运行时性能问题清单（P1–P7）

> 每项按"现象 → 根因与证据 → 关联 issue → 修复建议 → 改动面/收益"组织。

### P1 同步 SQLite 直接跑在 tokio worker 上，全仓库零 `spawn_blocking`【严重度: 高】

**现象**: SSE 流卡顿、请求尾延迟大、并发场景下 worker 线程被饿死；数据库操作全局串行。

**根因与证据**:
- `src/db/mod.rs`: `pub struct Database { conn: Mutex<Connection> }` —— **全局单连接 + 互斥锁，无连接池**。启动时 `Connection::open` 一次，之后所有 DB 访问（SSE 订阅、pipeline 后台任务、普通 CRUD）争抢同一把锁
- 全仓库搜索不到任何 `spawn_blocking`：所有 rusqlite 同步调用（`execute`/`query`/`prepare`）都在 async 上下文里直接执行
- `src/orchestrator/engine.rs`: pipeline 每步完成后同步"三连写"——`save_pipeline_snapshot` + `upsert_research_state` + `save_version_snapshot`，全部在 async 状态机循环内，且 `let _ =` 吞掉所有错误
- `src/routes/idea.rs`: async handler 里直接 `s.db.get_idea(&id)`；`api_idea_claim_tree` 直接拿裸连接 `conn.prepare(...)/query_map`；`api_idea_delete` 是"同步 purge + sleep 重试"循环
- `src/db/mod.rs` 仅设 `journal_mode=WAL`，**无 `busy_timeout`、无 `synchronous` 调整**

**关联 issue**: #77（SQLite busy_timeout + spawn_blocking，项目已立案未修）

**修复建议**:
1. 启动 PRAGMA 补齐：`busy_timeout(5000)` + `synchronous=NORMAL`（WAL 下安全且明显提升写吞吐）
2. 为 `Database` 增加异步包装层（如 `db.spawn<F>(move |conn| ...)` 内部 `tokio::task::spawn_blocking`），先迁移 orchestrator 三连写与 routes/idea.rs 热点
3. 中期：读多写少场景引入只读连接或 `r2d2` 池

**改动面/收益**: 中等（新增包装层 + 约 10 处调用点改造，业务语义不变）；直接消除 async runtime 阻塞，写吞吐典型提升数倍。

### P2 数据库迁移并发竞态，双开即崩【严重度: 高】

**现象**: 两个进程并发启动（用户手动双开）时报 `duplicate column name` 退出（issue #60 原文场景）。

**根因与证据**（`src/db/migrations.rs`）:
- 好消息：每个迁移各自包在 `unchecked_transaction()` 里，DDL 与 `schema_version` 更新同事务提交；`ALTER` 均有 `column_exists`（`PRAGMA table_info`）守卫 —— issue #13 的老问题已部分修复
- 坏消息：`run()` 整体**无 `BEGIN IMMEDIATE`、无文件锁**；且 `current_version` 是在 `Database::init` 里**事务外**读取后传参进来的。两个进程并发启动时都读到旧版本、同时进入迁移，`column_exists` 检查与后续 `ALTER` 之间存在竞态窗口，SQLite 的 `ALTER ADD COLUMN` 无 `IF NOT EXISTS`，后提交者即报 `duplicate column name`；叠加 P1 的无 `busy_timeout`，写锁竞争直接放大为 `SQLITE_BUSY`

**关联 issue**: #60（并发启动迁移撞 duplicate column）、#13（迁移未在事务内，残留）

**修复建议**: `migrations::run` 开头 `BEGIN IMMEDIATE` 并把版本读取移进锁内（或启动前文件锁）。单函数改造，schema 不变。

**验收标准**: 脚本并发启动两个实例 ×100 次无 duplicate column / SQLITE_BUSY。

### P3 pipeline 快照全量覆盖写，写入量 O(步数 × 最终体积)【严重度: 中高】

**现象**: 16 步 pipeline 跑一次，数据库累计写入量约为最终快照体积的 16–32 倍；长会话下越来越慢。

**根因与证据**:
- `orchestrator/engine.rs` 每步成功（及回滚）都调 `save_pipeline_snapshot`；`src/db/` 中该函数为 `INSERT OR REPLACE INTO pipeline_snapshots (idea_id, context_json, ...)`
- `context_json` 是整个 `PipelineContext`（`src/pipeline/context.rs`）序列化结果，包含 `patent_results`、`rag_chunks: Vec<ReferenceChunk>`（**RAG 全文切片**）、`agent_outputs`（每条含字符串化 JSON）、`ai_analysis/action_plan/oa_response` 等 —— 快照体积随步骤单调增长，每步全量重写

**修复建议**: 快照节流（仅关键步 / 每 N 步 / 尺寸超阈值时增量）；`rag_chunks`、`agent_outputs` 拆独立表存主键引用；大字段 zstd 压缩。需要一次 schema 迁移（配合 P2 的锁一起做）。

**收益**: 快照写入量从 O(16×N) 降到近线性；resume 反序列化也同步变快。

### P4 AiClient 每个 HTTP 请求重建，且存在 4 处"无超时 client"回退【严重度: 中高】

**现象**: 高频 AI 调用（chat、批量摘要）每次多付出一次 TLS 握手 + 连接池建立的延迟；极端情况下请求只剩 300s 全局兜底超时。

**根因与证据**:
- `src/routes/mod.rs` 的 `AppConfig::ai_client()/ai_client_expert()/ai_client_fast()` 每次调用都 `AiClient::with_config(...)` 新建实例，几乎所有 handler（`api_ai_chat`、`api_ai_chat_stream`、`api_ai_compare` 等）都这么干 —— **跨请求零连接复用**
- 4+ 处"builder 失败 → 回退无超时 client"（issue #55 立案未修）:
  - `src/ai/client.rs` `with_config`: 两级回退，最终 `Client::new()`（无超时）
  - `src/ai/client.rs` `clone_with_timeout`: `build().unwrap_or_else(|_| Client::new())`
  - `src/search/providers/serpapi.rs`: `client()` 与 `lookup_exact` 各一处（其 15s 上游超时只在 builder 成功时生效）
  - `src/search/providers/google_patents_xhr.rs`: transport 同样回退默认 client
- 次级问题: `serpapi.rs` 每次 `search` 新建 Client；每条命中额外做一次 `r.to_string()` 双重 JSON 序列化；`last_usage` 是单个 `Arc<RwLock<Option>>` take-即清，并发调用下用量归属错位（正确性瑕疵）

**关联 issue**: #55（上游客户端无超时兜底，4 处）

**修复建议**: `AppState` 按"配置指纹"缓存 `Arc<AiClient>`（settings 变更时失效）；4 处回退改为"保守默认超时（如 120s）重建"或直接报错；serpapi transport 复用。

**改动面/收益**: 小（三个工厂方法 + 4 处回退，接口不变）；高频调用延迟降低数百 ms 量级，消除 300s 尾部风险。

### P5 AI 链路系统性串行【严重度: 中】

**现象**: 批量摘要等 AI 聚合操作延迟线性叠加；provider 故障时 failover 最坏路径分钟级。

**根因与证据**:
- `src/ai/client.rs` `send_chat_inner`: primary → fallbacks 是 **for 循环串行尝试**；单 provider 内 3 次重试（固定 2s 退避；429 指数 2/4/8s 且尊重 Retry-After；401/403 不重试）。最坏 (1+3)×(1+N) 个请求串行排队，直到 300s 全局超时。无并发竞速、无失败熔断记忆
- `src/ai` 的 `batch_summarize` 注释自认"批量摘要内部逐条串行调用 AI"（最多 20 条专利 → 20×LLM 延迟累加）
- `routes/ai.rs` 的 `try_online_search` 也在 for 循环里逐条 await
- `src/pipeline/runner.rs` / `engine.rs`: 16 步**完全串行状态机**（`loop { execute_step().await; decide_next() }`），全文件无 `join_all`/`FuturesUnordered`；reflection/debate 的多 agent 调用也是顺序执行（部分步骤有业务依赖，串行合理，但无依赖的检索/展开类步骤可以并行）
- 次级: `PipelineContext` 大量字段是"字符串化 JSON"，存在 结构体→JSON String→再 parse 的双重序列化模式

**修复建议**: `batch_summarize` 改 `buffer_unordered(4~8)`；failover 引入 hedged request（primary 1–2s 未返回即并发 fallback）+ 短期失败熔断跳过已知坏 provider；`try_online_search` 并发化。注意修 `last_usage` 归属错位。

**收益**: 批量摘要延迟从 Σ(20×LLM) 降到 max/并发度；failover 最坏路径从分钟级降到 10s 级。

### P6 检索链"白发请求"+ 向量索引冷启动为空【严重度: 中】

**现象**: 每次多源检索把所有配置的源（含按次计费的 SerpAPI）都请求一遍，但只采用一个源的结果；向量检索功能冷启动时形同虚设。

**根因与证据**:
- `src/search/chain.rs`: 用 `futures::future::join_all` **并发发起所有 provider**（并发本身是对的），但 `resolve_by_precedence` **只取一个胜出源的结果，不做跨源拼接** —— 其他源的请求（含付费配额与带宽）全部浪费
- `src/search/merge.rs`: 只做本地 `/api/search` 路径的同源去重（key 为规范公开号，空号退化为"大写标题"），**无 top-N 截断**
- `src/search/providers/google_patents_xhr.rs`: `MIN_REQUEST_INTERVAL=2s` + 700ms jitter + 指数退避，请求串行 —— 批量场景逐个排队
- `src/vector/mod.rs`: 512 维**字符 n-gram(2~4)+TF+L2 的本地哈希向量**（非语义 embedding，代码注释自认"M-B 阶段临时方案，无 IDF，query/doc 不同空间"）；`search()` 对内存 `RwLock<HashMap>>` 全量暴力余弦遍历；**`build_index()` 是零向量占位骨架；DB 持久化与内存索引之间无预加载/同步机制 —— 冷启动内存索引为空**，检索要等到某处重新 compute 才有数据

**修复建议**: chain 改"按优先级级联 + 短超时竞速"（高优先级源 2s 无响应再放低优先级并发），避免付费源白发；vector 先做"启动时从 DB 预加载 + 写入时增量更新内存索引"，中期评估真 embedding（API 或本地模型）+ `sqlite-vec`。注意现有测试断言 `primary_wins...` 需同步调整。

### P7 构建配置无调优【严重度: 低】

`Cargo.toml` 无 `[profile.release]`。建议加 `lto="thin"`、`codegen-units=1`（对 JSON 密集型 workload 有免费增益）；`futures`/`async-stream` 依赖已就位，P5 改造成本低。

---

## 三、代码质量 / 架构问题清单（Q1–Q6）

### Q1 错误处理双轨制，AppError 形同虚设【最严重】

**证据**:
- `src/error.rs` 全部内容约 60 行：`AppError` 仅 **3 个变体**（NotFound→404 / BadRequest→400 / Internal→500），手写 Display（未用 thiserror）；仅 `From<anyhow::Error>` 与 `From<rusqlite::Error>` 两个转换，**一律塌缩为 500** —— 数据库唯一约束冲突与真正的内部错误不可区分；无 401/403/409/422/429/502 变体；`#[allow(dead_code)]` 表明部分变体甚至无人使用
- **AppError 实际被整体绕过**: `routes/ai.rs`、`routes/idea.rs` 的 handler 签名返回 `Json<serde_json::Value>`，非法输入统一返回 **HTTP 200 + `{"error": "..."}`**（如 `api_ai_threat_assessment`、`api_ai_claim_chart`、OA 系列、"创意不存在"等）；流式端点则返回 SSE `"[ERROR] ..."` 事件。`main.rs` 无任何错误映射中间件

**后果**: 调用方无法用状态码区分"参数错"与"服务挂"，重试/告警/监控逻辑无从建立；且该"空值校验返 200"模式被当作模板逐个 handler 复制，仍在持续增殖。

**关联 issue**: #102（12 个 AI 端点非法输入返回 200）、#76（AppError 分类缺失）—— 均已立案未修。项目自有报告也已建议"P1 结构化错误码体系"。

**修复建议**: thiserror 扩容变体（Validation→422、Unauthorized→401、RateLimited→429、Upstream→502、Conflict→409…）；提供统一 `ApiResult` 提取器或中间件；12 个端点迁移；**前端 fetch 包装需同步适配**（当前前端靠 `body.error` 判断失败）。

**验收标准**: CI 增加 curl 探测脚本，对全部 AI 端点发非法输入断言非 200。

### Q2 无类型 JSON 契约，与巨型文件互为因果

`routes/ai.rs`、`routes/idea.rs` **不定义任何 Request struct**，全部 `Json<serde_json::Value>` + `req["xxx"].as_str().unwrap_or("")` 逐字段掏取 —— 字段名拼错、类型不符在编译期零提示，静默变成空串，再被 Q1 的"空值返 200"模板吞掉。pipeline 层的 `HashMap<String, Value>`（项目自有报告问题 5）是同一病灶。类型缺失使 handler 无法提取公共抽象，文件因此越长越大。**修复路径**: 逐端点引入 serde 类型化 Request struct（与 Q4 拆分同 PR 做）。

### Q3 系统性复制粘贴（修一处要同步 N 处，已实际造成过事故）

| 重复点 | 证据 | 后果 |
|---|---|---|
| SSE 解析两份 ~70% 重复 | `ai/chat.rs` 与 `ai/patent.rs` 各一份 | **曾真实造成线上空回复事故**（reasoning_content 字段解析分歧，项目报告 🔴1） |
| OA 流式/非流式 handler | `api_ai_office_action_response`（~200 行）与 `_stream`（~250 行）**60–70% 重复**：my_patent 解析、oa_date 注入、refs 组装、容量校验、白名单过滤、冲突检测、持久化全部两份 | 修 bug 需人肉双写 |
| `get_idea` None 分支 | `routes/idea.rs` 至少 **8 个 handler 逐字重复** `Json(json!({"error": "创意不存在"}))` | 同上 |
| 后台任务模式 | `tokio::spawn + 写回 novelty_score/status` 在 idea.rs 重复 **4 次** | 同上 |
| `CaseInput` 构造 | `ai/patent.rs` 文件内重复 **3 次**；`spawn_oa_stream_worker` 两个几乎相同的 SSE `while let` 循环 | 同上 |
| 硬编码文案 | "在线搜索未配置" 等 **3 处** | issue #83（P1，PR #121 待复核） |
| 跨文件刻意双份 | `epo_ops.rs` 的 `find_first`/`find_all` 是同一深度搜索的两次实现；`kind_label` 注释明说与 `google_patents_xhr.rs` "刻意各留一份" | 同上 |

**修复建议**: 抽 `SseParser` 统一 trait；OA 两个 handler 提取共享 prepare 阶段；`get_idea_or_404` helper；`CaseInput::from_context` 构造器；文案常量单源。

### Q4 杂物抽屉式公共层 + 120 条路由平铺单链

- `src/common.rs`（36 KB）一个文件混合 **6 类职责**: `init_app_state`（状态组装）、`build_router`（**约 120+ 条路由全部平铺在一条 `.route()` 链上，独占约 350–400 行**）、DB 路径解析与 `auto_migrate_db`、CORS 白名单解析、`StaticAssets` rust-embed 静态服务、临时文件/OAuth URL/上游 client 杂项
- `src/routes/mod.rs` 又塞入 `AppConfig`（含 AI client 工厂、SerpAPI 多 key 轮询）和 `efld`/`image_data_uri`/`html_escape` 工具
- **没有统一的 `ok_json`/`err_json` 响应 helper** —— 这是 Q1"每个 handler 自造错误响应"的组织性根因；新增一个端点必须触碰单点大文件，也是巨型文件持续生长的结构性原因

**修复建议**: `common.rs` 拆为 `router.rs`/`state.rs`/`config.rs`/`static_assets.rs`；路由按域（ai/idea/search/oa/settings）分文件嵌套注册；AppConfig 移出 routes 层。

### Q5 可靠性护栏缺失

- **pipeline 无回滚**: 失败后部分完成状态被持久化但不可恢复（项目报告 🔴2，与 P3 快照问题耦合）
- **双入口行为漂移史**: lib.rs 曾漏 `reset_stale_analyzing()`、shutdown `.ok()` 吞错误（项目报告 🔴3）—— 两个入口复制启动逻辑，已实际产生过漂移
- **假绿测试史**: #59（node 缺失时打印 ok 的假绿）、#68（首启假绿首轮被识破为自证断言）、#82（测试恒真，TempDir drop 掩盖泄漏）、#57（自报数字与 CI 日志不符）—— 测试自评 ⭐2 与此互相印证
- **无自动 E2E**（仅手工清单）、**无请求追踪 ID**、CSRF 未防护、无 rate limiting（绑定 0.0.0.0:3000 无鉴权，README 自认不可公网暴露）

**修复建议**: pipeline 步骤幂等化 + 失败可重入（配合 P3）；启动路径收敛为单一函数双入口调用；为每条 guard 测试加"负控必红"自动化（项目已在 #90 流程里手工做，应固化进 CI）。

### Q6 前端债务

- `office_action_response.html` **262 KB** 单模板、`idea.html` 115 KB、`i18n.js` 67 KB、`style.css` 62 KB —— 模板内联大量 JS（仓库专门有 `check_html_functions.mjs`、`inline_scripts_parse.rs` 等守卫测试围着它转，说明痛点已知）
- README 声称"Chart.js 走 CDN、离线不渲染"，但仓库已有 `static/vendor/chart.umd.min.js` 本地副本与 `no_cdn_scripts.rs` 测试 —— 文档与现状疑似不符，需核实后修正
- i18n 文案散落 HTML 模板（项目报告问题 8）

---

## 四、开发流程问题清单（F1–F4）—— "GLM5.2 效率和质量差"的流程根因

> 代码层问题解释"质量差的一半"，流程层问题解释"效率差"与"质量差的另一半"。以下证据来自仓库 `AGENTS.md`、`docs/plans/STATUS.md`、`docs/progress/MASTER.md`、issue #90 及其 R25–R32 评论。

### F1 每轮流程税约 140KB 阅读 + 四处登记，编码只占轮次小部分（效率差的直接来源）

- **每轮必读**: `AGENTS.md`（约 240 行、**80–100 条硬性规则**）+ `docs/plans/STATUS.md`（**137 KB**，被规定为"每次必读"）——单这两份即可吃掉数万 token 上下文；首次进入还要读 CHANGELOG、errors.md
- **提交前测试门**: `cargo fmt` + `clippy -D warnings` + `cargo test`（含守卫测试）；改模板另加 `node check_html_functions.mjs` + Puppeteer e2e 54/54 + 回退检测 + release build；改 JS 加 ESLint；改基线刷新 functions-manifest.json
- **变更后登记义务（四处）**: CHANGELOG.md、docs/plans/ 标 ✅+commit hash、feedback.md（用户反馈）、errors.md（踩坑）、STATUS.md 写回 —— 即每轮结束还要再写一遍 137KB 大文档
- **#90 侧 ACK 纪律**: 每轮开工先回 issue（容器号/认领号/预计时间）、每件一 commit、禁 `git add -A`、draft PR 等维护方裁定

**结论**: 一个典型修复轮次中，真正编码可能只占小部分时间/token；重流程的初衷是防 AI 犯错，但对文档规模没有对等治理。

### F2 三套优先级体系无单一权威 + 状态源自相矛盾（做偏/返工的来源）

- `docs/progress/MASTER.md`（v4 主控，**停在 2026-09-28**，M-B 0/6、M-C 0/5 未开工）仍位于"权威文档链"之首；`docs/plans/STATUS.md` 更新到 10-23；`DESIGN_v5_engine.md` 待拍板 —— 三者互不同步，agent 每轮需自行裁定"做什么才对"
- STATUS.md **开头"当前版本"仍写 v0.7.4，文末已是 v0.9.19**；条目顺序混乱（R24–R31 在文档后部）—— agent 若信了开头就会基于错误假设工作
- 目录双轨: `docs/plan/`（单数）与 `docs/plans/`（复数）并存；同一变更要在 CHANGELOG、STATUS、issue 评论、PR 描述多处重复登记

### F3 假修与负控的军备竞赛，每件事验证两遍

评论区多次记录: #69 首轮"假修被识破（配套测试是自证断言）"、#82 首版测试恒真、#83 断言不敏感暂缓。流程被迫用"负控植入必红"双门反制，导致每件事验证两遍、多轮返工 —— 既是质量差的表象，也进一步拉低效率。根因是测试门槛只检查"绿不绿"，不检查"测试本身是否有效"；应把负控固化进 CI 自动化，而不是靠人工纪律。

### F4 追加式文档膨胀正反馈："越慢越记、越记越慢"

STATUS.md 137KB、CHANGELOG 134KB、errors.md 183KB、MASTER 74KB，全部只增不减；每轮阅读成本递增，形成恶性循环。

---

## 五、分阶段改进路线图

### 阶段一 · 快赢（约 2–3 个 agent 工作日，改动面小、不动业务语义）

| # | 事项 | 对应问题 | 验收标准 |
|---|---|---|---|
| 1 | 迁移加 `BEGIN IMMEDIATE` + 版本读取移进锁内 | P2（#60/#13） | 并发双开 ×100 无 duplicate column |
| 2 | PRAGMA 补 `busy_timeout(5000)` + `synchronous=NORMAL` | P1（#77 第一步） | 现有测试全绿 |
| 3 | AiClient 按配置指纹缓存 + 修 4 处无超时回退 | P4（#55） | 单测断言复用实例；grep 无 `Client::new()` 裸回退 |
| 4 | AppError 扩容（thiserror）+ 12 个 200 端点迁移 + 前端 fetch 包装适配 | Q1（#102/#76） | CI 加非法输入探测脚本，全部端点非 200 |
| 5 | 流程瘦身: AGENTS.md 240 行 → 核心约 80 行；STATUS.md 旧轮次归档至 `docs/plans/archive/` 只留最近 + 修正头部版本号；MASTER.md 标记归档；**单一优先级权威 = #90 看板** | F1/F2/F4 | agent 每轮必读量 < 20KB；文档无版本矛盾 |

### 阶段二 · 结构治理（1–2 周，防质量继续劣化）

| # | 事项 | 对应问题 | 验收标准 |
|---|---|---|---|
| 1 | `Database::spawn` 包装层，迁移 orchestrator 三连写 + idea.rs 热点 | P1（#77） | 全仓库 async 上下文无裸 rusqlite 调用（clippy 自定义 lint 或 grep 守卫） |
| 2 | 快照节流 + rag_chunks/agent_outputs 拆表 + 压缩 | P3 | 单次 pipeline 全程快照写入量 < 3× 最终体积 |
| 3 | 六处去重: SseParser trait、OA 共享 prepare、`get_idea_or_404`、CaseInput 构造器、文案常量、epo_ops 工具函数合并 | Q3 | 重复块消除的 grep 证据；SSE 行为回归测试 |
| 4 | 拆 `routes/ai.rs` → `routes/ai/{chat,analyze,oa,history,export}.rs`；`common.rs` → router/state/config/static_assets；AppConfig 移出 routes | Q4 | 单文件 < 30KB；路由注册按域嵌套 |
| 5 | 逐端点类型化 Request struct（与拆分同 PR） | Q2 | ai/idea 路由无 `Json<Value>` 裸掏取 |
| 6 | 批量 AI 并发化 + failover 竞速/熔断（先修 last_usage 归属） | P5 | batch_summarize 20 条总延迟 < 3× 单条 P95 |

### 阶段三 · 长期（按 #90 看板排期）

1. 检索链级联竞速（省付费配额）+ vector 启动预加载；评估真 embedding + sqlite-vec（P6，对应 #78 的 FTS5+RRF 方向）
2. pipeline 步骤幂等 + 可重入 resume；启动路径收敛单入口（Q5，对应项目报告 🔴2/🔴3）
3. 请求追踪 ID + CSRF + 限流（Q5）
4. `office_action_response.html` 262KB 模板拆分（Q6）
5. v5 引擎设计稿的 5 个待拍板决策点给出建议后拍板，避免第四套优先级体系出现

---

## 六、与项目自身认知的对齐

**项目已识别但截至 main 仍未解决**（自有报告/issue 立案）: 错误处理 ⭐3、SSE 重复 🔴、pipeline 无回滚 🔴、双入口漂移 🔴、#76/#102（错误码与 200）、#77（spawn_blocking/busy_timeout）、#55（超时兜底）、#60/#13（迁移）、#62（API 文档覆盖 8%）、#83（硬编码文案）。

**本次新发现**（此前无人立案）:
1. **AiClient 每请求重建** —— 全仓库最大的隐性性能浪费，issue 列表里没有它
2. **快照 O(16×N) 写放大** —— #77 只提了 spawn_blocking，没提写入量本身
3. **检索链白发请求** —— join_all 全源并发却只采用一个源，付费配额纯浪费
4. **向量索引冷启动为空** —— build_index 是占位骨架，功能实际不可用
5. **流程税定量**（每轮 140KB+ 必读 + 四处登记）与 **三套优先级无单一权威** —— 解释"GLM5.2 效率差"的流程层根因，#90 里未被当作问题
6. STATUS.md 头部 v0.7.4 vs 文末 v0.9.19 的版本矛盾；`docs/plan/` 与 `docs/plans/` 目录双轨
7. serpapi `last_usage` 并发归属错位（正确性 bug）；merge 无 top-N 截断

**双方一致且互相印证**: 测试自评 ⭐2 与假绿史（#59/#68/#82/#57）说明问题不在缺测试，而在测试有效性无自动化把关。

---

## 七、数据来源与方法说明

- 分析基准: 2026-10-23，`main` @ `e1e6bf0`，通过 GitHub REST API（git/trees recursive）取得全量文件清单与逐文件大小
- 源码证据: raw.githubusercontent.com 逐文件抓取分析（lib/main/error/common/routes-ai/routes-idea 摘要/db-mod/migrations/orchestrator-engine/pipeline-context-state/search-chain/serpapi/vector/ai-client 等）
- issue 证据: 25 个 open issues 全量 + issue #90 看板及 R25–R32 评论
- 文档证据: `docs/analysis/comprehensive-code-analysis-2026-07-05.md`、`ARCHITECTURE.md`、`AGENTS.md`、`docs/plans/STATUS.md`、`docs/progress/MASTER.md`、`DESIGN_v5_engine.md`、README
- 调查方式: 三路并行子代理独立调查（架构质量 / 运行时性能 / 开发流程），交叉汇总
- **局限**: 超大文件（如 151KB 的 routes/ai.rs）经摘要模型分析，handler 数为下限（"31+"），行号级细节以本地 clone 复核为准；未实际编译运行；前端模板内部质量未逐行深入；#90 看板中 #73/#71 等 P1 修复可能已在 R25–R31 轮次完成（截至 R32 剩余 OPEN: #83(PR #121 待复核)、#76、#110、#62 剩余、#51/#52），采信前建议以 issue 实时状态复核
