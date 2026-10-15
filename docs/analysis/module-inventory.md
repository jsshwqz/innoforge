# InnoForge 模块清单 / Module Inventory

> 分析日期：2026-08-15（基于 HEAD `69b6a85`）
> 方法：逐文件行数统计 → pub fn/struct 清单提取 → use crate:: 依赖图重建 → 巨型文件证据采样（行号级）→ 前端静态分析
> 评分：S.U.P.E.R 五原则各 1-5 分（满分 25）。**全项目加权均分约 13.6/25**
> 关键结论均已二次人工复核（含与子代理初判冲突处的源码验证）

---

## 1. 模块总表

| 模块 | 职责 | 行数 | pub API | 主要依赖 | S | U | P | E | R | 总分 |
|---|---|---|---|---|---|---|---|---|---|---|
| **routes/**（14 文件） | HTTP 层 + 业务逻辑 + 外部客户端混杂层 | **11,960 (46%)** | ~117 fn | ai, db, pipeline, patent, cad | 2 | 2 | 2 | 3 | 2 | **11** |
| ├ routes/idea.rs | 7 种职责混居（见§2） | 2,194 | 29 fn+15 私有 | pipeline, patent, **rusqlite 直连** | 1 | 2 | 2 | 2 | 1 | **8** ⚠️全场最低 |
| ├ routes/ai.rs | AI 对话/OA handler + 24 处 prompt + 私建搜索爬取 | 2,038 | 28 fn+17 私有 | ai, patent | 1 | 2 | 2 | 2 | 2 | **9** |
| ├ routes/patent.rs | 详情 + EPO/USPTO/GP 外部客户端内嵌 | 1,448 | 9 fn+21 私有 | patent, reqwest | 2 | 2 | 2 | 3 | 2 | **11** |
| ├ routes/search.rs | 搜索+向量检索(私有重写)+导出 | 1,363 | 6 fn+12 私有 | patent, rusqlite 直连 | 2 | 2 | 2 | 3 | 2 | **11** |
| ├ routes/upload.rs | PDF 六路提取+OCR+SSRF 防护 | 1,298 | 4 fn+33 私有 | reqwest, fs, 外部进程 | 2 | 3 | 2 | 2 | 2 | **11** |
| ├ routes/auth.rs | Google OAuth + gcloud 子进程 | 720 | 6 fn+11 私有 | db, reqwest | 3 | 3 | 2 | 2 | 2 | **12** |
| ├ routes/mod.rs | AppState/AppConfig + 解析/HTML 工具杂烩 | 665 | 8 fn+3 类型 | ai, db, pipeline, patent | 2 | 3 | 2 | 3 | 2 | **12** |
| ├ routes/settings.rs | 设置 CRUD + 服务商 key 映射硬编码 | 601 | 6 fn+11 私有 | db | 3 | 3 | 2 | 3 | 2 | **13** |
| ├ 其余 6 文件(feature_cards/cad/collections/chat/ipc/pages) | 单页面 API，职责较清晰 | ~2,091 | ~33 fn | 少量 | 3 | 3 | 3 | 3 | 3 | **15** |
| **ai/**（6 文件） | 多服务商容灾客户端 + prompt 库 | 3,341 | 41 fn | 自洽（不碰 DB ✅） | 3 | 4 | 3 | 3 | 3 | **16** |
| ├ ai/client.rs | 容灾循环+Anthropic 协议+Gemini CLI 子进程+超时分级，四职一体 | 1,069 | 14 fn+7 类型 | reqwest | 2 | 3 | 3 | 3 | 3 | **14** |
| ├ ai/patent.rs | 专利分析方法 + 大段中文 prompt | 997 | 12 fn | client | 2 | 4 | 4 | 4 | 3 | **17** |
| ├ ai/fact_check.rs | OA 事实校验纯函数层（**未接线，#[allow(dead_code)] 预留**） | 700 | 6 fn+15 测 | 无 | 4 | 5 | 5 | 5 | 5 | **24** 🏆设计最佳却闲置 |
| **pipeline/**（22 文件） | 16 步研究流水线 | 3,281 | ~40 fn | ai, db, experiment | 3 | 3 | 2 | 3 | 3 | **14** |
| ├ pipeline/context.rs | **20 个 pub 类型的上帝上下文对象** | 342 | 20 类型 | state | 2 | 3 | 2 | 4 | 2 | **13** |
| ├ pipeline/steps/*（18 个） | 单步骤一文件，模式统一 ✅ | 2,689 | ~25 fn | context, ai, db | 4 | 3 | 3 | 4 | 3 | **17** |
| ├ steps/search.rs | 步骤内直连 SerpAPI + DB 缓存 | 208 | 2 fn | db, reqwest | 2 | 2 | 2 | 3 | 2 | **11** |
| **db/**（18 文件） | SQLite 门面(Mutex\<Connection\>) + 17 子域 + v23 迁移 | ~4,300 | ~100 fn | patent, **pipeline::context 反向** | 3 | 2 | 2 | 3 | 2 | **12** |
| **orchestrator/**（3 文件） | 命令编排 + 分支/版本快照 | 463 | 6 fn | ai, db, pipeline | 4 | 4 | 3 | 4 | 3 | **18** |
| **patent.rs** | **29 个 pub 类型杂物桶**（CAD/Patent/AiChat/Idea/LegalEvent/FeatureCard/ClaimNode 无域边界） | 474 | 3 fn+29 类型 | serde | 1 | 3 | 3 | 4 | 2 | **13** |
| **rag/**（4 文件） | 完整 RAG 管道 —— **零引用死代码**（已复核：全仓无外部调用方） | 362 | 9 fn | ai, db | 4 | 4 | 2 | 4 | 2 | **16** |
| **vector/** | VectorIndex + RRF —— **零引用；逻辑被 routes/search.rs 私有重写替代** | 175 | 8 fn | db | 4 | 4 | 2 | 4 | 1 | **15** |
| **experiment/**（4 文件） | AI 生成脚本 + 沙箱执行（WIP，仅 2 处引用） | 315 | 3 fn | common, pipeline, ai | 4 | 4 | 3 | 3 | 3 | **17** |
| **docx_export/** | 手写 XML→docx，单一纯粹 ✅ | 314 | 1 fn+1 类型 | zip | 5 | 4 | 4 | 4 | 4 | **21** |
| **common.rs** | CORS+静态资源+init/build_router+临时文件 | 596 | 5 fn | **routes::AppState（反向!）** | 2 | 1 | 3 | 3 | 2 | **11** |
| **cad.rs** | AionCAD 桥适配（**硬编码 D:\test\aionui 日志路径 L120/L153，已复核**） | 415 | 8 fn+3 类型 | reqwest | 3 | 3 | 2 | 1 | 2 | **11** |
| **error.rs** | AppError 统一错误设计良好但**未被路由采用**（dead_code 标记在字段级） | 53 | 1 类型 | axum, rusqlite | 5 | 4 | 4 | 4 | 4 | **21** |
| **context.rs** | 项目记忆注入（相对路径读 docs/*.md） | 70 | 1 fn | fs | 4 | 4 | 3 | 2 | 3 | **16** |
| bin/mcp-server.rs | MCP server（BASE_URL 硬编码 localhost:3000，HTTP 自调） | 243 | — | lib | 4 | 3 | 2 | 2 | 3 | **14** |

## 2. 巨型文件职责混杂证据（行号级）

### routes/idea.rs — 2,194 行，44 函数，≥7 种职责
| 职责 | 证据位置 |
|---|---|
| Idea CRUD | L160-392 |
| AI 聊天 + 上下文压缩算法 | L408-805 `api_idea_chat` 约 **400 行单函数**（读历史、压缩 prompt、双次调 AI、合并摘要、写库） |
| Pipeline 编排启动 | L235/L866/L982 三处直接 `PipelineRunner::new(...)`——路由层持有编排器构造权 |
| 报告生成 + 内嵌 Markdown→HTML 渲染器 | L1071-1490（`simple_md_to_html` L1338、`inline_md` L1441） |
| 文本相似度算法 | L1700-1730 `char_trigrams`+`jaccard_trigram`（与 text_util.rs 的 pub 版重复造轮子） |
| **绕过 db 层直连 rusqlite** | L1995、L2029 手写 SQL |
| **AI 服务商注册表第三副本** | L2196-2205 八家服务商 URL+key 名硬编码 |

### 其他要点
- routes/ai.rs：L84 直连 serpapi.com、L115 爬 sogou.com——路由层自建两套搜索源
- routes/patent.rs：fetch_epo(L620)/fetch_uspto(L661)/Google Patents(L745) 三套外部客户端无 trait 抽象
- routes/search.rs：L552-720 私有重写 TF-IDF(L676)+cosine(L710)，直连 patents_embedding SQL(L595)；CSV(L787)/XLSX(L838) 导出职责混入
- routes/upload.rs：六路 PDF 提取平铺函数（pdftotext L593/pymupdf L633/mineru L663/OCR L720/Umi-OCR L829/AI vision L437）；SSRF 防护子系统 L1131-1233；Umi-OCR 地址硬编码 L20
- ai/client.rs：单一 struct 身兼容灾循环+Anthropic 协议适配(L582-701)+Gemini CLI 子进程(L903)+超时策略(L26-40)

## 3. 类型重复问题清单

| # | 问题 | 位置 | 影响 |
|---|---|---|---|
| T1 | **SearchResult 双定义同名**（已复核） | patent.rs:218 vs pipeline/context.rs:114 | 违反规约 §2.2 最核心禁令 |
| T2 | **TF-IDF 向量化三实现** | vector/mod.rs:61（零引用）/ search.rs:676（实际在用）/ rag/chunker.rs | 同一算法三份代码 |
| T3 | contains_cjk 双定义 | search.rs:1026 vs steps/search.rs:20 | 逐字重复 |
| T4 | Jaccard/trigram 双实现 | text_util.rs:12(pub 正确版) vs idea.rs:1700 | 工具层存在被无视 |
| T5 | **AI 服务商知识三处编码** | AppConfig 字段族(mod.rs:64-99) + settings.rs provider_db_key + idea.rs:2196 注册表 | 新增服务商改三处 |
| T6 | patent.rs 29 类型杂物桶 | CAD/Patent/AiChat/Idea/LegalEvent/FeatureCard/ClaimNode 混居 | 无领域边界 |
| T7 | SerpAPI 客户端三套 | steps/search.rs:100 / ai.rs:84 / patent.rs:67 | P 原则缺失活标本 |

## 4. 可疑依赖与架构违例（重构靶点）

1. **基础设施反向依赖路由层**：common.rs:112,188 → routes::{AppState, AppConfig}
2. **routes 绕过 db 直连 rusqlite**：idea.rs:1995,2029 / search.rs:595 / steps/claim_tree.rs:9
3. **db 层反向依赖 pipeline 业务类型**：db/cost.rs→AiCostRecord、db/evidence.rs→Evidence、db/research_state.rs→ResearchState——pipeline::context 成为全局类型枢纽
4. **Schema 版本漂移（已复核为命名漂移而非功能 bug）**：db/mod.rs:45 SCHEMA_VERSION=22 vs 实际迁移至 v23；target_version 参数只用于日志不参与门控，属"定时炸弹级不一致"
5. **死代码群 ~1,200 行**：rag/(362) + vector/(175) 全零引用；fact_check.rs(700) 设计最佳(24/25)却未接线
6. **环境硬编码**：cad.rs:120,153 开发机绝对路径；upload.rs Umi-OCR 写死 127.0.0.1:1224；mcp-server BASE_URL 写死；context.rs 相对路径依赖 CWD
7. （好消息）双入口已收敛于 common.rs::build_router，AGENTS.md"两处注册"条款过时

## 5. 前端页面盘点

| 页面 | 体积 | function 声明 | on* 处理器 | innerHTML= | fetch | 备注 |
|---|---|---|---|---|---|---|
| office_action_response.html | **152.9KB** | 90 | 71 | 68 | 20 | 最大单体前端 |
| idea.html | 111.2KB | 29 | 33 | 68 | 33 | fetch 密度最高 |
| settings.html | 62.5KB | 12 | 13 | 21 | 18 | 函数少而巨大 |
| compare.html | 43.2KB | 41 | 34 | 22 | 9 | |
| patent_detail.html | 46.4KB | 20 | 22 | 19 | 22 | |
| search.html | 39.9KB | 23 | 23 | 35 | 7 | |
| ai.html | 24.8KB | 22 | 6 | 8 | 6 | |
| index.html | 23.3KB | 11 | 11 | 6 | 6 | |

跨页重复（重构最大红利）：escapeHtml ×7 次/6 页（已复核）；renderMarkdown ×3 页；showPdfPreview/closePdfPreview ×3 页。
static/ 组织：style.css 63KB 有 Design Tokens 与分区注释，组织良好仅体积大；i18n.js 46KB 里藏着 renderNavbar/renderSidebar/initChatHistory（导航渲染器藏在翻译文件里）+ DOMPurify 全局 fallback，隐性加载顺序耦合。

## 6. 测试覆盖分布（危险失衡）

| 区域 | 测试密度 |
|---|---|
| tests/ 集成 4 文件 974 行 ~49 用例 + 内嵌 cfg(test) 36 处 ~162 用例（合计 159 绿 ✅） | — |
| routes/idea.rs 2,194 行 | **仅 2 测（1/1097 行）🔴** |
| ai/client.rs 1,069 行 | **仅 1 测 🔴** |
| routes/ai.rs 2,038 行 | 4 测 🔴 |
| routes/search.rs / upload.rs / patent.rs | 3/5/8 测 |
| 密集区：fact_check 15、chat 15、feature_cards 15、collections 14 | 小模块健康 |

结论：测试集中在易测小模块，最需要回归保护的**编排/容灾/提取逻辑恰好裸奔**——混合职责导致不可测的直接后果。

## 7. 重构优先目标 Top 10

| # | 目标 | 一句话理由 |
|---|---|---|
| 1 | routes/idea.rs 拆分 | 总分 8/25 全场最低；拆为 idea_crud/idea_chat/research_state/report_render/claim_tree 五模块 |
| 2 | routes/ai.rs 拆分 | 24 处 prompt + 两套私建搜索源 |
| 3 | 统一 SearchProvider/Embedder 端口 | 三套 SerpAPI + 三套 TF-IDF 是最大重复税 |
| 4 | routes/upload.rs 策略模式化 | 六路提取后端注册表 + SSRF 工具独立（纯函数拆完即可测） |
| 5 | routes/patent.rs 外部客户端外迁 | EPO/USPTO/GP 实现 trait，路由只剩编排 |
| 6 | ai/client.rs 按 ProviderMode 拆 adapter | Http/Anthropic/GeminiCli 三 adapter + failover 编排器 |
| 7 | 类型体系重组 | patent.rs 按域拆 + 消灭 SearchResult 双定义 + AppConfig 上提 |
| 8 | db 层端口化 + SCHEMA_VERSION 修复 | Repository trait 或按域拆；v22→v23 对齐 |
| 9 | 死代码决断 | rag/vector/fact_check 要么接线要么摘除，拖着最差 |
| 10 | 前端公共层抽取 | escapeHtml×7 等收敛到原生 ES module；i18n.js 剥离导航渲染 |
