# InnoForge 完整重构任务分解 / Task Breakdown

> 版本：v2（已纳入用户四项决策与 2026-08-15 全量扫描/修复结果）
> 策略总纲：**原地分层重组（Strangler Fig 渐进式）** —— 保持 axum + SQLite + 纯静态前端技术栈与全部既有功能不变，先立端口抽象、再拆巨型文件、最后治理前端。每阶段结束都是"可发布状态"。
> 用户决策记录：①未提交改动审阅后提交（已完成）；②死代码按质量裁决（见 Phase 0.5）；③**直接在 dev 分支执行**；④版本号走**全新版本线，发布 0.1.0**（若实意为 1.0.0，仅影响 M5 发布时的 Cargo.toml/tag，不影响任何前置任务）。
> **本文档是自动执行的唯一权威输入**：执行 agent 按阶段顺序认领任务，逐条对照验收标准，全程遵守下方"总原则"。

---

## 总原则（每个任务隐含遵守）

1. **门禁不降级**：每任务完成必须过 fmt/clippy -D warnings/cargo test(159+)/ESLint/check_html_functions/e2e(54+) 全套
2. **只搬不写**：拆分阶段禁止顺手改逻辑；行为变化一律另开 fix 任务
3. **每任务一提交**：`refactor: 中文描述` 格式，秒级提交防多会话互扫
4. **S.U.P.E.R Quick Check**：每任务完成前自检其标注的设计约束
5. **测试先行搬家**：被搬函数的既有测试随代码同移；目标模块补齐最低测试密度

---

## Phase 0 — 地基与卫生（预计 0.5-1 天）🟢 无风险前置

> ✅ 2026-08-15 预处理已完成三项（见 git log）：①patent_detail.html 未解决合并冲突标记解决（保留 CAD 拦截、舍弃被队列机制取代的旧中止块）；②idea.html 补回 FreeCAD 图卡接线（sendChatMessage 拦截 + loadMessages 历史重绘后 renderHistory），cad_template_contract 3/3 转绿；③src/cad.rs 剥离硬编码调试日志（改为 tracing::warn），保留端口 8080 默认值/无头预览放宽/db 路径 canonicalize 三处真修复。

| # | 任务 | 优先级 | 工作量 | 验收标准 |
|---|------|--------|--------|----------|
| T0.1 | ~~处置未提交改动~~ | P0 | S | ✅ 已完成（审阅后剥离调试代码并提交） |
| T0.2 | 工作区垃圾清理：根目录 ~250 个 `_*.txt`/`server_v*.log` 直删；**innoforge.db(4.1GB)/docs.zip(118MB)/src.zip 复制到仓库外 `_archive_20260815/` 再删**；游离根级 main.rs(17KB 旧单体) 删除 | P1 | S | 根目录仅剩源码+配置+文档；删除前有备份 |
| T0.3 | SCHEMA_VERSION 对齐：db/mod.rs 常量 22→23，migrations::run 增加 current > target 时 warn 日志 | P0 | S | cargo test 全绿；新建空库 init 后 query_schema_version()==23 |
| T0.4 | 文档漂移修正：AGENTS.md §2.4 双入口条款→"统一在 common.rs::build_router 注册"、§6 15步→16步、"当前 v11"→"见 migrations.rs"、AI 超时条款与代码分级(60/180/300s)对齐、"7 个页面"→8；ARCHITECTURE.md 模板引擎描述改 include_str!+replace | P1 | M | 文档与代码一致 |
| T0.5 | **分支切换（用户决策：直接在 dev）**：`git checkout dev && git merge main`（dev 目前落后 main 多个提交，必须先合入最新状态含本轮修复），确认工作区干净 | P0 | S | dev 包含 main 全部提交；cargo test 在 dev 上绿 |
| T0.6 | **推送激活远端关卡**：本地 main 领先 gitee/main 36 提交、领先 origin/main 60 提交——最新代码从未经过远端 CI！推送 main 与 dev 到双远端，等待 GitHub+Gitee CI 全绿 | P0 | M | 双远端 CI 对最新提交三 job（test/lint/e2e）全绿 |
| T0.7 | Dockerfile 修复：`--bin innoforge` → `--bin innoforge-server`（两处：RUN build 与 COPY），删冗余 COPY templates/static（已编译期嵌入）；本地 docker build 验证 | P1 | S | docker build 成功产出镜像 |
| T0.8 | functions-manifest 基线刷新：settings.html 新增定义 formatNumber/loadAiCostStats 纳入基线（`node check_html_functions.mjs --refresh`），基线变更与说明同提交 | P1 | S | 扫描器零待更新提示 |
| T0.9 | **【用户动作】.env 密钥轮换**：工作区 .env 含多个明文真实密钥（DeepSeek/Gemini/SenseNova/Zhipu/Google Secret），虽被 gitignore 但建议轮换；确认从未被历史提交泄露（git log -p 扫描） | P1 | S | 用户确认轮换完成或接受风险 |

## Phase 0.5 — 死代码质量裁决（用户决策：按质量定去留）

> 用户规则："你看看他的质量，强过你就保留，你的强过他就重写"。裁决结果（2026-08-15 完成评审）：

| 模块 | 质量评审结论 | 裁决 |
|------|--------------|------|
| ai/fact_check.rs (700行) | 全场最佳设计（24/25）：纯函数层、15 个测试、无外部依赖 | **保留并接线**（接入 OA 分析流程） |
| rag/ (362行) | 结构清晰但依赖 pipeline::context 类型；chunker 的 embedding 与另外两份实现同源 | **重写并入**：接线时按新 types/ 依赖重写为独立 service，不原样保留 |
| vector/ vs routes/search.rs 内联版 | **同源双胞胎且共享同一缺陷**：`cleaned[i..i+n]` 字节切片在多字节字符（中文）上 panic——即 /api/search/vector 收到中文查询会打崩 handler；另有语义缺陷（embedding 按 value 排序丢失 token 身份）与 build_index 桩实现。rrf_fuse 函数本身正确 | **重写合一**：抽取唯一 `search_vector` 模块（char-boundary 安全 + 修排序缺陷），删两份旧拷贝，补中文回归测试；同时决断 patents_embedding 写入路径（当前全仓无写入方，向量层空转） |

## Phase 1 — 类型与错误地基（解反向依赖）（预计 2-3 天）

| # | 任务 | 优先级 | 工作量 | S.U.P.E.R 约束 | 验收标准 |
|---|------|--------|--------|----------------|----------|
| T1.1 | 建 `src/types/` 领域模块：把 patent.rs 29 类型按域拆为 types/{patent,search,idea,cad,chat,legal}；**消灭 SearchResult 双定义**（pipeline/context.rs 版本改名 PipelineSearchResult 或合并字段） | P0 | L | P/R | 全仓 use 路径更新；cargo test 绿；patent.rs 变薄壳 re-export 或删除 |
| T1.2 | AppConfig 从 routes/mod.rs 上提到 `src/config.rs`；AppState 同步上提 | P0 | M | U/P | routes/mod.rs 不再含配置类型；common.rs 不再 use crate::routes 类型 |
| T1.3 | AppError 启用：补错误码字段（code: &'static str），全部路由签名迁移为 `Result<Json<T>, AppError>`，删除手写 (StatusCode, Json) 元组与 dead_code 标记 | P0 | L | P/U | grep 无 `(StatusCode::INTERNAL_SERVER_ERROR, Json(` 手写模式；错误响应统一 {status,message,code} |
| T1.4 | 生产路径 unwrap/expect 清零（~25 处逐一甄别替换） | P0 | M | E | 自写脚本扫描非 test 区 =0 |
| T1.5 | PathConfig 集中：data/uploads、data/runtime-temp、cad 目录、docs 记忆路径收敛到 config 提供；mcp-server BASE_URL 可配化 | P1 | M | E | grep 无硬编码路径字面量（测试除外）；cad.rs D:\test\aionui 日志路径改配置 |

## Phase 2 — 端口层建设（P 原则补课）（预计 3-4 天）

| # | 任务 | 优先级 | 工作量 | S.U.P.E.R 约束 | 验收标准 |
|---|------|--------|--------|----------------|----------|
| T2.1 | `SearchProvider` trait：统一三套 SerpAPI 实现（steps/search.rs / ai.rs / patent.rs）为一个 client + 配置注入 | P0 | L | P/R | SerpAPI HTTP 调用全仓唯一；三调用方改走 trait 对象 |
| T2.2 | `Embedder` trait：按 Phase 0.5 裁决重写合一——新建 char-boundary 安全的统一向量模块（修 CJK panic + 排序语义缺陷），routes/search.rs 私有版与 vector/mod.rs 旧拷贝删除；**决断 patents_embedding 写入路径**（专利保存/富化时写入，否则向量层继续空转）；contains_cjk/jaccard 工具归并 text_util | P0 | L | P/S/E | 中文查询回归测试通过；全仓仅一份 TF-IDF 实现；api_search_vector e2e 保护 |
| T2.3 | ai/client.rs 拆分：HttpOpenAI / AnthropicApi / GeminiCli 三个 adapter struct + FailoverClient 编排器 + 统一 StreamParser（SSE 解析去重） | P0 | XL | P/S/R | client.rs ≤400 行；容灾循环新增单测 ≥8 个（现 1 个） |
| T2.4 | ProviderConfig 表驱动：服务商元数据（id/名称/base_url/key 字段名/默认模型）收为单一注册表，AppConfig 的 10 个 per-provider Key 字段改为 map；settings.rs provider_db_key 与 idea.rs 注册表副本删除 | P0 | L | P/E | 新增服务商仅需在注册表加一行 + DB 一条 key |
| T2.5 | upload.rs 提取器策略化：六路 PDF 提取各自独立文件实现 `PdfExtractor` trait，注册表顺序降级；SSRF 防护独立 `net_guard.rs` 纯函数模块 | P1 | L | S/P/E | upload.rs 缩至 <400 行；net_guard 单测覆盖现有回归 |

## Phase 3 — routes 巨型文件拆分（预计 4-6 天，部分可与 Phase 5 并行）

| # | 任务 | 优先级 | 工作量 | S.U.P.E.R 约束 | 验收标准 |
|---|------|--------|--------|----------------|----------|
| T3.1 | idea.rs(2194) 五拆：idea_crud / idea_chat(压缩算法下沉 service) / research_state / report_render(Markdown 渲染器独立 util) / claim_tree(SQL 下沉 db 层) | P0 | XL | S/U | 单文件 ≤500 行；idea.rs 测试 2→≥15；e2e 创意页 54 项全过 |
| T3.2 | ai.rs(2038) 分组拆：oa.rs(OA 分析/讨论/答复书) / chat.rs / compare_analyze.rs；prompt 文本集中各文件头部常量区（遵守规约 prompt 放 handler 所在模块） | P0 | L | S/U | 单文件 ≤600 行；OA e2e 全过 |
| T3.3 | patent.rs 外部客户端外迁 `patent_sources/{epo,uspto,google_patents}.rs` 实现 trait；路由只剩编排 | P1 | L | P/U | patent.rs 缩至 <500 行；SSRF 回归保持绿 |
| T3.4 | search.rs 导出职责独立 export.rs（CSV/XLSX）；向量检索改调 Embedder port | P1 | M | S | 导出单测保留通过 |
| T3.5 | common.rs 减负：build_router 按 API 族拆 register_xxx(router) 私有函数群（仍在 common.rs 内分组即可，不强拆文件） | P1 | M | U/S | build_router 主函数可读性恢复；118 条路由数量断言测试防丢路由 |
| T3.6 | pages.rs 渲染统一：通用 render_template(path, &[(k,v)]) 函数替代逐字段 replace 链 | P2 | S | P | patent_detail 16 字段渲染行为不变 |

## Phase 4 — 数据层与死代码决断（预计 2-3 天）⚠️ 含需用户决策项

| # | 任务 | 优先级 | 工作量 | S.U.P.E.R 约束 | 验收标准 |
|---|------|--------|--------|----------------|----------|
| T4.1 | **死代码处置【已按用户质量规则裁决，见 Phase 0.5】**：fact_check 保留并接入 OA 分析流程；rag 按新 types/ 重写为独立 service 后接线（聊天引用场景）；vector 与 search.rs 内联版重写合一（T2.2 已完成合一，此处收尾 rag 接线与 embeddings 写入链路） | P0 | L | R/S | 三者不再处于"无人负责"状态；各自有测试与调用方 |
| T4.2 | db 层按域拆 trait（PatentRepo/IdeaRepo/SettingRepo…）或最小化：先把 db→pipeline::context 的类型依赖反转为依赖 types/ DTO | P1 | L | U/P/R | db/*.rs 不再 use pipeline:: |
| T4.3 | Pipeline results 强类型化第一步：定义 StepOutput 枚举（每步一个变体）替代 HashMap<String,Value> 的写入侧；读取侧渐进迁移 | P1 | L | P/U | 编译期保证步骤产物字段；现有 16 步流水线 e2e 全过 |
| T4.4 | Database 临界区审计：锁内不放慢操作（外部 HTTP/AI 调用绝不持锁），必要时 clone 后锁外处理 | P1 | M | R | code review 清单核对；并发冒烟测试 |

## Phase 5 — 前端重构（预计 4-5 天，T5.1-T5.2 可与 Phase 3 并行）

| # | 任务 | 优先级 | 工作量 | S.U.P.E.R 约束 | 验收标准 |
|---|------|--------|--------|----------------|----------|
| T5.1 | 共享 JS 层 `static/js/common.js`（原生 ES module，无构建工具）：escapeHtml/renderMarkdown/fetchJson 封装/toast/showPdfPreview 收敛；8 页改为 `<script type="module">` 引用 | P0 | L | S/R | 重复定义清零；check_html_functions 基线 --refresh 并提交说明；54 e2e 全过 |
| T5.2 | i18n.js 减负：renderNavbar/renderSidebar/initChatHistory 剥离到 layout.js；DOMPurify 全局保护保持在最先加载位 | P0 | M | S | 加载顺序契约写进 AGENTS.md；全页面冒烟正常 |
| T5.3 | OA 页(153KB) JS 按流程分段外置 static/js/oa/*.js：analyze/discuss/generate/export 四段 | P0 | XL | S/R | 只搬不写；分段期间每段跑 OA 相关 e2e；函数基线同步刷新 |
| T5.4 | idea 页(111KB) 同法外置 | P1 | XL | S/R | 同上 |
| T5.5 | innerHTML 审计：255 处逐条标注数据来源（纯服务端可信/含用户输入/富文本渲染）；含用户输入路径显式 DOMPurify.sanitize 或改 textContent | P0 | L | 安全 | 审计表落盘 docs/analysis/innerHTML-audit.md；高危点清零 |
| T5.6 | settings/compare/search/detail 页 JS 外置 | P2 | L | S | 各页 e2e 正常 |

## Phase 6 — 收尾与防线升级（预计 2 天）

| # | 任务 | 优先级 | 工作量 | 验收标准 |
|---|------|--------|--------|----------|
| T6.1 | 覆盖率度量引入（cargo llvm-cov，生成 JSON 忽略入库）并记录重构前后对比 | P1 | M | README/AGENTS 附覆盖率数字 |
| T6.2 | e2e 补强：新拆模块的边界路径（报告 HTML、导出文件、向量检索开关） | P1 | M | e2e ≥60 项 |
| T6.3 | 文档终同步：ARCHITECTURE.md 重画分层图、API.md 校对 118 条路由、AGENTS.md 规约更新（types/ 模块约定等）、CHANGELOG 完整记录 | P0 | M | 文档与代码零漂移抽查通过 |
| T6.4 | **MCP 出口决断**：6 个声明工具仅 patent_search 可用且 patent_compare 指向不存在的 /api/ai/analyze-results——要么补全 5 个工具（映射到既有 API），要么把 tools/list 裁剪到真实可用的集合 | P2 | M | tools/list 与 tools/call 一一对应；每个工具冒烟通过 |
| T6.5 | 版本发布：Cargo.toml 0.7.4→**0.1.0（全新版本线，用户决策）**，CHANGELOG 开新版本段记录全部重构内容，tag 推送双远端触发 release.yml 5 平台矩阵 | P0 | S | CI 双远端绿；Release 附件齐全 |

---

## 工期汇总

| 阶段 | 内容 | 预计 | 累计 |
|------|------|------|------|
| Phase 0 | 地基卫生 | 0.5-1d | 1d |
| Phase 1 | 类型与错误地基 | 2-3d | 4d |
| Phase 2 | 端口层建设 | 3-4d | 8d |
| Phase 3 | routes 拆分 | 4-6d | 14d |
| Phase 4 | 数据层与死代码 | 2-3d | 17d |
| Phase 5 | 前端重构 | 4-5d | 22d（并行后 ~18d） |
| Phase 6 | 收尾发布 | 2d | **~20 天净工期** |

> 并行泳道：Phase 3（后端拆分）与 Phase 5 T5.1-T5.4（前端外置）互不碰文件，可双会话并行；Phase 4 依赖 Phase 1/2 完成。
