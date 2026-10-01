# M-B 施工规格书 · 可信深度分析（2026-09-28 派包基线）

> 承接 M-A（检索员上岗）2026-09-28 收口：PR #20→#27 全合并，五端对齐 `1af09fd`，门禁基线 **721 = lib 335 + bin 337 + 集成 49**（cad_template 3 + cad_ui 3 + orchestrator 6 + patent_hub 37），`e2e_test.mjs:5` `expectedPasses = 60`。
> 本文是执行棒的**唯一派包依据**。所有「现状」条目均为规划会话在 tip 上实测（grep / 读码 / 复解析原始件），行号可核；执行棒若发现与现实冲突，**以证据为准并反驳本文件**，禁止假服从。

---

## 0. 本轮已拍板的产品决策（不再讨论，除非可验证事实推翻）

| 决策 | 结论 | 影响 |
|------|------|------|
| **嵌入路线**（gap 4.6 的 A/B 二选一） | **选本地方案 = gap 4.6 的 B 案**：先把 TF-IDF 修对，**不上嵌入模型** | M-B 期间禁止引入 ONNX / candle / tract / `AiClient::embed()` / 外部 embedding API 等新依赖；语义检索**只承诺「可用 + 不 panic + 单一实现」**，不承诺检索质量；现有向量空间的语义局限必须写进代码注释与文档，禁止伪造分数让它「看起来有效」。⚠️ **对 B 案原文的有意偏离**：B 案的「补 IDF / 语料统计」本轮**不做**（`patents_embedding` 生产恒 0 行、全文供给尚未落地时补 IDF 等于给空表调权重，收益不可验证），待 MB3 有真实切片语料后再评估；若用户要求 B 案全量落地须另派一包（MB0b）。偏离理由已登记在 gap 计划 §4.6 |
| **付费凭据** | **只测免费链，SerpAPI / EPO 留占位** | 所有 M-B 包的 DoD 必须能在**无 `SERPAPI_KEY`、无 `EPO_KEY/EPO_SECRET`** 的环境下闭环；付费路径只做「有 Key 才启用 + 失败如实记账」，不派需要真实付费冒烟才能验收的包 |

被这两条决策**继续冻结**的项（M-B 收口前不派包）：真嵌入模型接入、SerpAPI 真实冒烟、配额预警的真实数据触发。

---

## 1. 现状硬事实（派包基线，逐条带证据）

### 1.1 全文供给（M-B 的真地基，也是 M-A 遗留的边界）

- `patents` 表的 `description` / `claims` 只有 5 个非空写入来源：`routes/patent.rs:28`（SerpAPI details 付费富化，无 Key 直接报 `SERPAPI_KEY not configured`，`:56-58`，**无降级**）、`routes/patent.rs:221`（**免费** `api_enrich_patent_free`：直抓 `https://patents.google.com/patent/{号}/{lang}` HTML 剥标签，20s 超时）、`routes/patent.rs:714`（PDF 页自动富化，先免费爬再 SerpAPI）、SerpAPI provider 直查（`search/providers/serpapi.rs:517-526` → 入库 `:551`）、`routes/settings.rs:681`（导入原样入库）。
- **M-A 在线命中入库的专利只有摘要**：`xhr_to_patent` 把 `description`/`claims` 写成 `String::new()`（`google_patents_xhr.rs:498-500`），EPO 同（`epo_ops.rs:1021-1022`）；降级链 `search/chain.rs` 全程无入库/富化钩子。
- 全仓 `INSERT INTO patents` 只出现在 `db/patent.rs:40` 的 `insert_patent` 内部，同函数 `:35/:44` 同步 `patents_fts`（MA4 复核已确认「单一写入口」成立）。
- PDF 上传产出的全文**不回写 `patents` 表**（`routes/upload.rs:534` 六级降级提取，文本只进 prompt / 返回体）。
- `enrich-free` 与 XHR **同 Host、同反爬风险**，但它**不查 MA6a 冷却表**（独立 20s 超时、零记账）——分析前批量富化一旦接入，等于给熔断器开了一个旁路。

### 1.2 嵌入 / 向量（MB0 的对象）

- TF-IDF / n-gram 实现**三份互不共享的拷贝**：`src/vector/mod.rs:26-40`（`CharNGramTokenizer::tokenize`）、`src/rag/chunker.rs:57-66`、`src/routes/search.rs:852-863`。三者都是 `cleaned[i..i+n]` **按字节索引切 String**，而 `cleaned.len()` 是字节长度 ⇒ 中文（3 字节/字）必然踩非字符边界，**panic**。全仓无 `char_indices` / `floor_char_boundary` 防护。
- `patents_embedding`（迁移 v20）的写入函数 `db::save_patent_embedding`（`db/vector.rs:6`）唯一调用方是 `vector/mod.rs:159` 的 `compute_and_save_embedding`，而后者**全仓零调用** ⇒ 生产库该表恒 0 行。
- `/api/search/vector`（`common.rs:239` 注册）全表扫 blob 算 cosine（`routes/search.rs:771-799`），且 `count_embeddings()`（`:767`）**>0 才启用向量档**，0 行时静默只回 BM25 RRF 并如实带 `vector_count: 0`（`:847`）。
- 现「向量」是把 term 分数排序后填充 512 维、**丢弃 term→维度映射**（`vector/mod.rs:77` 自注 "no IDF without corpus"，`:81-91`）⇒ 查询向量与文档向量不在同一可比空间，相似度数值不构成语义相关性。**这是已知局限，本轮不修，但必须写明。**

### 1.3 RAG（MB3 的对象，彻底未接线）

- `src/rag/` 四文件（`mod.rs` `build_chunks_from_patent` / `rag_search`；`chunker.rs`；`retriever.rs` `retrieve_chunks` + `retrieve_chunks_by_keyword`（空占位）；`assembler.rs`）。`patent_chunks`（迁移 v21）的写入口 `db::save_patent_chunks`（`db/rag.rs:17`）**全仓零调用点**；`rag::` 在 `src/rag` 之外零引用（仅 `lib.rs:20` 声明）；`build_router` 无任何 rag 路由。
- 深度分析喂给 AI 的内容**不含专利全文**：`pipeline/steps/analysis.rs:37 deep_analysis_simple`（拼装 `:77-113`）与 `pipeline/steps/deep_reasoning.rs:126 build_user_context`（`:164-179`）只喂用户创意的 title/description + `top_matches` 的摘要（snippet 源自 `pipeline/steps/search.rs:164` 的 `p.abstract_text`）。
- **八处** `chars().take` 截断**违反 AGENTS.md §2.5 的数据完整性纪律**（不走 `ai::client::truncate_for_ai`（`:267`，带完整性提示的既有工具））：
  - `analysis.rs:50`（snippet→AI prompt，150 字符）、`:156`（ai_analysis→AI prompt，500）、`:180`（snippet→FeatureCard description，300）、`:193`（snippet→core_structure，200）、`:250`（ai_analysis→AI prompt，2000）
  - `deep_reasoning.rs:138`（snippet→AI prompt，120）
  - `oa_response.rs:137`（snippet→AI prompt，80）
  - `claim_tree.rs:24`（ai_analysis→AI prompt，800）
  
  其中 `analysis.rs:50`/`:180`/`:193`、`deep_reasoning.rs:138`、`oa_response.rs:137` 截断的是专利摘要喂给 AI 的内容；`analysis.rs:156`/`:250`、`claim_tree.rs:24` 截断的是 AI 自身产出再喂回。两类都违反 §2.5「传给 AI 的数据必须保留全文」纪律。

### 1.4 幻觉防线与出处（MB1 / MB4 的对象）

- 核查模块真实位置是 `src/ai/fact_check.rs`（不是顶层）：公开 `check_oa_analysis(:79)`、`format_report(:119)`、`FactWarning(:20)`、`FactCheckReport(:31)`；核查 4 项（技术领域一致 / 段落引用存在 / 无来源数据 / A33 术语），扣分致命 35 / 高 15 / 中 5（`:105-116`）。
- **唯一生产接线**：`routes/ai.rs:1433`（OA **流式**答复 handler），结果作为「AI 事实核查」标注事件追加在流尾（`:1434-1437`），**不拦截**，并随全文存 `save_oa_analysis`（`:1441`）；非流式版（`:1123`）不核查。模块头 `:14` 自称「未接入主流程」——**注释已过时**，需顺手更正。
- `src/pipeline/`、`src/routes/idea.rs` **零 fact_check 引用**（证实）。
- 出处结构现状：`types/idea.rs:78-81`（`Evidence` 的 `source_*` 字段）与 `:184`（`claim.evidence_ids`）已存在；`evidence_chain`（迁移 v8）由 `pipeline/steps/contradiction.rs:91`、`scoring.rs:77`、`rank.rs:73` 写 → `finalize.rs:19` 批量插入（`db/evidence.rs:9`），读侧 `db/evidence.rs:38` → `routes/idea.rs:394`。但 **`ctx.ai_analysis` 是纯字符串、无出处结构**——MB4 的强制标注必须落在有结构的那一侧。

### 1.5 上下文压缩与容量（MB5 的对象，派包时未取证，本轮补）

- 压缩路径唯一实现于 `src/routes/ai.rs:196 compress_history(ai, db, history: Vec<(String,String)>, max_tokens)`：触发是**双条件**（`:202-205`）——估算 token 总和 > `max_tokens`（两个调用点都传 **8000**：`:407` 多模态、`:450` 文本）**且** `history.len() > 10`；估算函数 `:186-192`（CJK 计 3、ASCII 计 1，再 `/2+1`）。被裁的是最早的 `len-8` 条（`keep_recent=8`，`:208-210`），经**一次真实计费的 AI 调用**压成 ≤800 字摘要（`:223-230`），产出**单条 role=assistant** + 最近 8 条（`:241-247`）；压缩失败降级为全量送（`:249-252`）。多模态分支阈值另算（`:402-413`，`len>5`）。
- system prompt 不被裁（`src/ai/chat.rs:40-47` 把 system 单独放 `messages[0]`，history 只含 user/assistant）；但**首轮用户约束在 history 里 ⇒ 必被摘要吞掉**，这就是 MB5 的风险源。
- **第二套互不共享的摘要实现**：`src/routes/idea.rs:500-501`（`auto_summary_threshold=8` / `keep_recent=6`）、增量分段摘要 `:516-551`、二级压缩 2000 字 `:554`、窗口截取 `:738-741`。与 ai.rs 那套阈值/形状都不同 ⇒ 只改一处必留行为分裂。
- 关键事实现在的载体**全是自由文本**：`ai.rs:59-67 patent_reference_material()` 把专利号/标题/摘要/claims 拼成 `"Patent: {}\nTitle: ..."`，`:354` 进 system prompt；claims 侧走 `safe_truncate`（`ai/patent.rs:37/:62/:318-320`）。既有可复用的结构化容器 = `ai.rs:38-43 bounded_reference_material(label, content)`（固定 label + `<user_input>` 包裹 + `escape_prompt_material` `:29-34`），即「结构化直传」的最小改造点。
- OA 容量：`ai/client.rs:314-322 oa_capacity_error(field, value, max_chars) -> Option<String>`（报错串含 `actual_chars/max_chars`），上限常量 `:307-312`（60_000 / 2_000_000 / 15_000 / 600_000 / 400_000 / 150_000）。**真正「报错不截断」的入口只有两处**：讨论 `ai.rs:1631-1653`（SSE error `:1651`）、答复书流式 `ai/patent.rs:803-815`（由 `ai.rs:1449` 调）。其余全部静默截断，缺口见 MB5 范围③。
- ⚠️ 新发现的**生产路径中文 panic**（与 MB0 同类缺陷）：`ai.rs:1698-1699` 用**字节索引**硬切讨论历史字符串（前面 `:1670-1684` 才是 `chars().take`），超长中文讨论历史越限即 panic。
- 记忆注入预算与压缩无关：`src/context.rs:16 CONTEXT_MAX_CHARS=6000`，截断 `:53-61`，注入在 system 层（`ai.rs:360-365`、`idea.rs:638`）⇒ 复用这条通道承载关键事实可绕开 `compress_history`。
- 测试基础：全仓**无**任何对话轮数/压缩回归用例。最接近的是 `tests/patent_hub_integration.rs:423/:450`（只断 2 轮顺序）、`src/ai/tests.rs:25 ai_data_truncation_tests`（5 例）、`:69 oa_capacity_tests`（`:76/:83/:95`）、`src/routes/ai.rs:2201/:2219`；`e2e_test.mjs` 只有 `:255 checkLongPayloadIntegrity`（断言名 `:265`、`:291`），无 20 轮用例。

### 1.6 现状校正清单（本轮实测推翻/补强 §1.1–§1.4，执行棒以本节为准）

| # | §1 原述 | 实测校正 | 影响哪个包 |
|---|---------|---------|-----------|
| a | 「共用 MA6a 的 `breaker::global_table()` 判据（同表同函数）」易被读成 DB 表 | 它是**进程内** `OnceLock` 的 `&'static Mutex<CooldownTable>`（`search/breaker.rs:155`，`:10-12` 明写刻意不落库）——无表名、无迁移版本。判据 `CooldownTable::remaining(source, now) :121`；写 `record(source, fail, now) :106` / `note_attempts :132` / 全局薄壳 `breaker::note_attempts(&[AttemptReport]) :164`；时长 `:38-60`（SerpAPI 300/900s、XHR 120s、EPO 300/900s）。消费点**仅在搜索路径**：`routes/search.rs:667-670`（`filter_cooled_providers :284`）、链后回写 `:674`、精确直查只读门 `:621-629`（`exact_lookup_allowed :359`）。`routes/patent.rs` 三个富化入口（`:28/:221/:729`）**不查不写** | MB2（禁止为此新建 DB 表，否则破 migrations 红线） |
| b | §1.1 称全文来源「只有 5 个非空写入来源」 | **漏记第三份抓取实现**：`routes/patent.rs:729-759`（PDF 页自动富化）自带一份同 URL + 20s 超时的复制，未复用 `api_enrich_patent_free`/`extract_section` | MB2（「不复制第二份 HTML 解析」实际是三份） |
| c | — | `enrich-free` 有 **handler 层幂等**判据 `:238-240`（desc>50 且 claims>50 → `Already enriched`）+ 逐列覆写 `:293/:300`；DB 层无去重 | MB2 |
| d | §1.2 三处字节切片行号 | MB0 工作区已删两处拷贝：现 `rag/chunker.rs:59` 单行委托、`routes/search.rs:779` 复用同函数（§1.2 行号为派包时取证，已漂移） | MB3 |
| e | §1.3 称 `analysis.rs:37 deep_analysis_simple` 与 `deep_reasoning.rs:126` 两条路径并列喂 AI | **`deep_analysis_simple` 全仓零调用点**（`orchestrator/engine.rs:310-312` 只派 `deep_analysis`）⇒ 是「降级路径实为死码」。真正影响验收的只有 `deep_reasoning.rs` 一处 | MB3 |
| f | §1.3 只说 RAG 未接线 | 接线即错的两处缺陷：`db/rag.rs:55 search_chunks` 的 SELECT 只取 `id,content`，却在 `:66 row.get(2)` 读第三列 embedding ⇒ **运行必错**；`PatentChunk`（`db/rag.rs:4-11`）缺 `model_name`/`created_at`，与 v21 表列不对齐。另 `assembler::build_citations :86` 是孤儿，`rag_search` 走自己的 `build_fallback_citations`（`mod.rs:133`） | MB3 |
| g | §1.3「`rag::` 仅 `lib.rs:20` 声明」 | `src/main.rs` 模块清单（`:15-36`）**无 `mod rag`** ⇒ MB3 接线必须再破一次 main.rs 红线（同 MB0 偏离①性质），并改变门禁计数（见 §3 基线漂移预告） | MB3 |
| h | MB3 验收「每条引用能反查到 `patent_chunks` 的行」 | `RankedMatch`（`pipeline/context.rs:57-66`）字段只有 `source_id`（本地命中形如 `patent_local_{patent_number}`，`steps/search.rs:162`）与 snippet，**无 patent_id、无段落号** ⇒ 反查前必须补可反查 id | MB3 |
| i | §1.4 称核查结果「作为『AI 事实核查』标注事件追加在流尾」 | 实际是**匿名 data 事件**（`routes/ai.rs:1435-1437`，无专用 event name），前端靠文本标记识别 ⇒ MB1 若沿用即违反 MA6b「只发结构化键」纪律 | MB1 |
| j | MB1 要「拒绝并给可读原因」（未定机制） | 无需新造：`orchestrator/engine.rs:186-205`（critical 步 → `StepStatus::Error` + `record_failure` + 回滚快照 + Err；非 critical → Skipped 继续）、`OrchestratorCommand::Abort{reason} :72-76`、`is_critical`（`pipeline/state.rs:108`）、idea 侧 Err 落库 `routes/idea.rs:995-999` + 进度广播 `:1066` | MB1 |
| k | MB1「禁止留第二份判定」缺依据 | 现确定性判定全仓唯一。`routes/ai.rs:2000 api_ai_check_amendments` 走 LLM（`ai.check_claim_amendments`）属**非确定性**、不同层；`ai/patent.rs:314/376/468/926` 与 `ai.rs:1724/1729` 仅是 prompt 约束；`fact_check.rs:314` 段落号正则别处无重复。现有 fact_check 单测 15 条（`:621-814`） | MB1 |
| l | §2 统一红线禁改 `templates/`、`static/` | **MB4 必然破线**：出处/决策段的可视化落点只有 `templates/idea.html:436-470 loadEvidence`（现只渲染 claim/置信度/`来源: source_type·source_id·produced_by·relation`，excerpt 在 `:458`，**不渲染 `source_url`/`claim_number`**）与报告区 `:1807-1811`、`:1952-1953 renderMarkdown`；不破线的唯一替代（往 `ai_analysis` Markdown 贴补丁）正是 MB4 禁止项。`check_html_functions.mjs:6` 注明「新增引用 → INFO 不报错」，故只加不减不需 `--refresh` | MB4 |
| m | §1.4 `ctx.ai_analysis` 纯字符串改造面 | 写侧 `steps/analysis.rs:23/:116/:118`、`steps/finalize.rs:13-14`；读侧 `steps/debate.rs:76-86`、`reflection.rs:60/103/109`、`claim_tree.rs:24`、`oa_response.rs:19/26/91`、`experiment/generator.rs:31`、`engine.rs:274`、`routes/idea.rs:930/1042/1138/1863/2008`，最终落 `ideas.analysis`；呈现 `api_idea_report :1117` 三分支（`:1160/:1195/:1226`）+ `api_idea_report_html :1240`（`:1339` 直接 dump）⇒ **禁止改类型**，只加结构化旁路 | MB4 |
| n | — | `routes/idea.rs:1176-1180` 的 executive 档已有**硬编码「建议」3 条**，与 MB4 的「决策建议段」语义重叠，须显式裁定取代/并存 | MB4 |

---

## 2. 包切分（每包 = 一棒 = 一次门禁，串行派，禁止同仓并行）

> 统一红线（每包都适用，逐文件 `git diff main..<branch> -- <path>` 必须为 0）：`src/common.rs`、`Cargo.toml`、`Cargo.lock`（**M-B 全程禁止新 crate 依赖**）、`src/db/migrations.rs`（M-B 前 4 包**不动 schema**，`patent_chunks`/`patents_embedding` 表已存在）、`src/main.rs`、`src/lib.rs`、`AGENTS.md`、`templates/`、`static/`、`e2e_test.mjs`、`check_html_functions.mjs`、`docs/functions-manifest.json`；生产路径零 `unwrap()/expect()`；禁止读写用户库 `D:\test\patent-hub-backup\innoforge.db`（取证一律在 `D:\Temp\*-probe\` 的临时空库实例）；禁止仓库根目录临时文件；`gh` 写操作一律 `env -u GITHUB_TOKEN` 前缀；门禁输出全量落盘后统计，**禁止 `| head`**。

### 2.1 红线例外预授权（本轮实测后的唯一合法破口，超出即审计打回）

| 红线文件 | 哪包可破 | 允许到什么程度 | 依据 |
|---|---|---|---|
| `src/main.rs` | MB0（已发生）、MB3 | **仅限模块声明行**（`pub mod vector;` / `pub mod rag;`），函数体、路由注册、`main` 逻辑一律零改动 | bin 与 lib 双入口模块清单必须一致（AGENTS.md 2.4）；MB0 后 `db/patent.rs`/`routes/search.rs` 调 `crate::vector::*`，缺声明即 `innoforge-server` 编译失败（§1.6 g） |
| `src/common.rs` | 无 | 零破口。MB3 若想要 rag 路由，**本包不派路由**（rag 由 pipeline 内部自动调用，对外无新端点需求） | 派包时无对外检索端点诉求，加了即无人消费的死路由 |
| `templates/` + `static/` | 仅 MB4 | 最小范围见 MB4 范围⑤：只改 `templates/idea.html` 的 `loadEvidence` 函数体与报告面板渲染、`static/i18n.js` 双表加键；**禁止新增内联 `on*` 引用**（不动函数基线即不动 `functions-manifest.json`） | §1.6 l：出处/决策段不破前端就没有任何可见落点 |
| `e2e_test.mjs` | 仅 MB5（且须先获用户裁定） | 只允许新增用例并同步 `expectedPasses`（`:5` 当前 60），禁止改/删既有用例 | MB5 验收若要 e2e 取证必破此线，否则一律走单测 |
| `src/db/migrations.rs` | 无 | M-B 全程零 schema。冷却表是进程内表（§1.6 a），chunks/embedding 表 v21/v20 已存在，MB4 的决策段持久化改走 `PipelineContext` → 既有 `idea_versions.context_json` | 加列即破两包红线且无可验证收益 |

> 越界判定口径：审计会话逐文件跑 `git diff main..<branch> -- <path>`，命中上表未授权项即打回，不接受「顺手修的」——顺手修的东西另起一包（见 §4）。

### MB0 · Embedder 归一 + char-boundary 安全 + 写入链接通（第一棒，无前置）

- **范围**：① 三份 n-gram/TF 拷贝收敛为**单一出处**（建议留在 `src/vector/mod.rs`，另两处改为调用它；删除重复实现而不是加 `#[allow(dead_code)]`）；② 字节切片全部改字符边界安全（`Vec<char>` 窗口或 `char_indices`）；③ `compute_and_save_embedding` 接上真实调用点——**只接「新入库顺手算」**（挂在 `db/patent.rs::insert_patent` 之后的单一写入口侧），**禁止**在本包做全表批量回填（4.3GB 用户库，风险与耗时都不可控）；
  - ⚠️ **架构提示**：`compute_and_save_embedding` 签名需要 `&VectorIndex`，而 `insert_patent`（`db/patent.rs:7`）只有 `&self`（Database），`AppState`（`routes/mod.rs:385`）也不持有 `VectorIndex`。执行棒须在以下方案中选一并落注释：(a) 在调用方（如 `routes/patent.rs` / `search/providers/serpapi.rs`）于 `insert_patent` 返回后用默认参数构造 `VectorIndex` 再调 `compute_and_save_embedding`——**推荐**，不改 `insert_patent` 签名、不碰红线文件；(b) 在 `insert_patent` 内部内联嵌入计算——需让 `db/patent.rs` 依赖 `vector` 模块，且 tokenizer 参数硬编码在 DB 层。无论选哪个，embedding 失败**不得回滚**专利入库（embedding 是附带优化，不是入库前提），失败只 `tracing::warn!` 记账；④ `/api/search/vector` 的查询向量与写入端**同函数同实现**（不留第二套标准）；⑤ 更正 `vector/mod.rs:77` 一带注释与规格书：写清「当前向量丢弃 term 映射 ⇒ 相似度不构成语义能力，本轮有意不修」。
- **必做取证**：中文 panic 回归用例（纯中文 3 字节 / 中英混排 / emoji / 对 `0..len` 全起点切片的循环用例），**先在旧实现上跑红、新实现跑绿**，把红→绿证据写进 PR body（这是本包唯一的「证明我改对了」的锚）。
- **验收**：临时空库实例入库一条中文专利 → `count_embeddings()` 由 0 变 ≥1、`/api/search/vector` 的 `vector_count` 如实翻转（此前恒 0）；同一中文长文本反复 tokenize 零 panic；`routes/search.rs:852` 与 `rag/chunker.rs:57` 两处拷贝消失。
- **门禁**：fmt / `clippy --all-targets -D warnings` / `cargo test`（基线 721，本包新增 N 条 ⇒ 721 + 2N 对账）；templates/static 零改动 ⇒ e2e 与 HTML 扫描按 DoD 不适用。
- **落地记录（MB0 执行后，行号为派包时 tip `6e8813f` 取证、施工后已漂移）**：单一出处 = `src/vector/mod.rs::compute_char_tfidf_embedding`（写入/查询/切片三侧共用）；`CharNGramTokenizer::tokenize` 改 `Vec<char>::windows`，`rag/chunker.rs` 与 `routes/search.rs` 的字节切片拷贝已删；写入链接在 `db/patent.rs::insert_patent` 单一写入口（顺手算、静默降级 warn、无全表回填）；门控两档各一用例锁定。**「本轮不修 IDF / 不修 term→维度映射丢失」口径不变**，局限已写明于 `compute_char_tfidf_embedding` 函数头注释；实测既有浮点求和顺序随 HashMap 迭代抖动（1 ulp 级），属公式既有行为，未改。**门禁实测（2026-09-28 审计会话复跑）**：`cargo fmt --check` 通过；`cargo clippy --all-targets -- -D warnings` 零告警通过；`cargo test` 全绿 **746 passed / 0 failed**（lib 347 +12、bin 348 +11、集成 49 不变 + 红绿取证 2）⇒ 对账 `721 + 2N − 1 = 744`（N=12），**少的那 1 条是 `rag/chunker.rs` 的委托用例只在 lib 侧跑**——`src/main.rs` 的模块清单本就没有 `mod rag`（AGENTS.md 2.4 双入口既有漂移，MB0 未扩大触碰范围去补）。红→绿锚已真跑：临时副本 `tests/mb0_red_proof_tmp.rs`（逐字复刻 HEAD 旧字节切片实现）2 passed —— 旧实现对中文专利文本 panic、新实现零 panic 且维度 512，取证输出落 `D:\Temp\mb0-verify-red.txt`；该副本按本包约定**不入提交**，红侧证据以本行 + PR body 为准。**两处登记偏离**：① `src/main.rs` +4 行（`pub mod vector;`）破了 §2 红线「main.rs 零 diff」，理由为 MB0 后 bin 侧 `db/patent.rs`/`routes/search.rs` 调 `crate::vector::*` 缺声明即 `innoforge-server` 编译失败，属红线与现实冲突、按 §前言「以证据为准并反驳本文件」处理；② 验收项「`/api/search/vector` 的 `vector_count` 如实翻转」以 handler 层函数 `vector_hybrid_search_json` 的 `:memory:` 空库双档用例取证（0 行档 `vector_count: 0` + 出参 7 键形状不变 / 有行档向量档启用），未另起真实 HTTP server 冒烟。

### MB2 · 免费全文供给链式化 + 冷却表共用（第二棒，无 schema 变更，依赖 MB0 合并）

- **现状锚点**：`routes/patent.rs:221-224 api_enrich_patent_free(Path(id), State(s)) -> Json<Value>`（路由 `common.rs:244-247` GET `/api/patent/enrich-free/:id`）；专利号/语言取自 DB（`get_patent :226`，CN→`zh` 否则 `en` `:243-247`）；URL `:250`、超时 20s `:253-256`、浏览器 UA `:260-263`；解析走**非正则**手写剥标签 `extract_section :334-386`（`find(itemprop=…)` 定位 + `>`/`<` 扫描）与 `extract_classifications :389`；落库 `s.db.insert_patent(&updated) :314`（单一写入口）；成功 `{"status":"ok","patent":…} :330`，失败 `{"status":"error","message":…} :228/:268/:272/:277`；**全程零冷却记账**。冷却判据真实形态见 §1.6 a（进程内 `Mutex<CooldownTable>`）。前端消费现仅手动/单篇：`templates/patent_detail.html:653`（按钮，先 free 后 SerpAPI `:666`）、`:723`（加载自动，先 SerpAPI `:713` 失败回落 free）。
- **范围**：让「分析前自动有全文」在**无付费 Key** 下可达且不加剧反爬：
  ① 抽可复用富化核心（建议 `enrich_free_core(&Database, &Patent) -> EnrichOutcome`），内部复用 `api_enrich_patent_free` 的抓取 + `extract_section` 解析，handler 改为薄壳调用它；**同批把 `routes/patent.rs:729-759` PDF 自动富化里那份复制实现也改成调用它**（§1.6 b：现实是三份不是两份，只抽一处等于留两份）。
  ② 进网络前查 `breaker::global_table()` 的 `remaining(source, now)`（`:121`），**source 必须复用搜索路径已登记的同一个标识**（free 抓取与 Google Patents XHR 同 Host，用不同 source 值等于给熔断开旁路——这正是 §1.1 第 27 行的缺陷本体）；冷却中 → 直接降级摘要档、零出网，并如实给原因码；抓取失败按既有写法回写 `record(:106)` / `note_attempts(:132/:164)`，禁止新建表、禁止新增 `FailKind` 语义。
  ③ 单次分析最多富化 top-N（参数化，默认 5）；每条沿用 20s 超时；**禁止无界循环里逐条出网**（AGENTS.md 2.7 禁循环查库同源纪律）。
  ④ 出参**只发结构化键**（沿 MA6b 范式：`routes/search.rs:232-243 cooldowns_json`、空则整键省略 `:161-163/:178-180`、秒数向上取整 `:221`；前端消费范式 `templates/search.html:599-605` 只认键不猜文案）。建议形状：`enrichment: {requested, enriched, failed: [{patent_id, reason_code, remaining_secs}]}` + 顶层 `full_text_available_count`；`reason_code` 为稳定枚举（`cooldown` / `blocked` / `timeout` / `not_found` / `parse_empty`），**禁止把中文文案塞进键让前端判字符串**。
  ⑤ 幂等沿用 handler 层既有判据 `:238-240`（desc>50 且 claims>50 → Already enriched），写库仍**只走 `insert_patent`**（禁止新增 `UPDATE patents SET description…`，那会绕掉 `db/patent.rs:35/:44` 的 FTS 同步与 MB0 的 embedding 挂点——全仓无触发器，见 MB3 现状锚点）。
  ⑥ 断言富化结果不被截断（AGENTS.md §2.5）：入库前后对账 `description`/`claims` 字符数不减；本包禁止调用 `truncate_for_ai`/`safe_truncate` 于写入路径。
- **不做**：不改 SerpAPI 付费路径行为（`routes/patent.rs:28`、`search/providers/serpapi.rs:517-526/:551`）；不引入新源；不动 `patents` 表列；不把 XHR/EPO 入库改成带全文（那是 §1.1 记录的上游限制，本包用富化链补，不改 provider）。
- **必做取证**：
  - 红→绿锚：**冷却表预置一条 XHR 档冷却记录后，旧代码仍出网、新代码零出网**。取证方式=在临时实例里预置 `record(...)` 再调富化，以 `tracing` 日志/请求计数证明零网络发起（禁止靠「返回快」推断）。
  - 中文 panic 不受影响：`extract_section` 是字节扫描实现，须补一条中文长文用例证明 MB2 改动没引入越界（与 MB0 同类风险）。
  - 真实阻断档：无 Key 环境下对 M-A 在线命中入库的中文专利跑一次，抓到什么就记什么（拿到全文 / 反爬阻断），**两种结果都算取证通过**，禁止为过验收伪造 `status: ok`。
- **验收**：① 临时实例（无 Key）里 `description/claims` 由空变非空，**或**如实返回阻断原因码且冷却表被正确写入/读取；② 冷却期内二次调用零出网（有计数/日志证据）；③ 富化结果字符数对账不减；④ 出参结构化键集合有单测锁定（含「空则省略键」）；⑤ `routes/patent.rs` 里同 URL 抓取实现从三份收敛为一份（grep 断言 `patents.google.com/patent/` 构造点只剩一处 + 注释）。
- **门禁**：fmt / `clippy --all-targets -D warnings` / `cargo test`（基线见 §3.1，本包新增 N 条 ⇒ 基线 + 2N）；templates/static 零改动 ⇒ e2e 与 HTML 扫描按 DoD 不适用。

### MB3 · RAG 接线（切片→检索→组装→引用）（第三棒，依赖 MB0 + MB2）

- **范围**：把已有但未接线的 `src/rag/` 真正挂上：① 入库/富化时写 `patent_chunks`（单一写入口侧，与 MB0 同一挂点纪律）；② 深度分析前自动取 top-N 专利的全文切片进 prompt，每条切片带**可回溯引用编号**（专利号 + 段/权号）；③ **全部八处** `chars().take` 截断改走 `truncate_for_ai`（`analysis.rs:50/156/180/193/250`、`deep_reasoning.rs:138`、`oa_response.rs:137`、`claim_tree.rs:24`），**八处同批改**（少改一处则该处仍违规）；④ 无全文时降级为摘要档并在上下文里如实标注（不得静默把「没取到」变成「没有相关内容」）。
- **验收（无 Key 环境）**：一份真实中文专利库状态下，深度模式报告上下文里出现 **≥5 篇专利全文片段引用**，每条引用能反查到 `patent_chunks` 的行；`retriever::retrieve_chunks_by_keyword` 的空占位要么实现要么删除并销账（**禁止留装饰性空函数**）。

### MB4 · 出处标注体系（第四棒，依赖 MB3 的引用编号；**唯一获准破前端红线的包**）

- **现状锚点**：`types/idea.rs:71-94 pub struct Evidence` 字段齐全（`id:73`、`idea_id:74`、`claim:76`、`source_type:78`（`patent|web|contradiction|scoring`）、`source_id:79`、`source_title:80`、`source_url:81`、`claim_number:84 Option + skip_serializing_if`、`excerpt:86`、`relation:88`（`supports|contradicts|partial`）、`confidence:90`（注释明写「算法计算非 AI 生成」）、`produced_by:92`、`created_at:93`）；`evidence_ids: Vec<String>` 在 `TechnicalFeature`（`:179-185`，`novelty_flag:183`），存 `technical_features.evidence_ids TEXT DEFAULT '[]'`（`db/migrations.rs:310`），写侧 `steps/claim_tree.rs:83`。表 `evidence_chain` 迁移 **v8**（`migrations.rs:191-218`，列 `:195-207`，索引 `:210-211`）。写：`steps/contradiction.rs:91`（`produced_by="DetectContradictions"`，`source_url` 恒空、`claim_number: None`）、`scoring.rs:77`（`"ScoreNovelty"`、`confidence: 1.0`）、`rank.rs:73`（`"RankAndFilter"`、透传 `m.source_url`）→ `steps/finalize.rs:17-24`（先 `delete_evidence_by_idea:18` 再 `insert_evidence_batch:19`，失败仅 `warn:20`）→ `db/evidence.rs:9`；读 `db/evidence.rs:38 get_evidence_by_idea`（`:43 ORDER BY confidence DESC`）→ `routes/idea.rs:394 api_idea_evidence`（`:400` 出参 `{status, evidence[...全字段], count}`，路由 `common.rs:390`）。`ctx.ai_analysis` = `pipeline/context.rs:266 pub ai_analysis: String`（改造面清单见 §1.6 m，**禁止改类型**）。前端：证据 tab `templates/idea.html:86-89`、`loadEvidence :436-470`（`:456` 置信度硬编码、`:459-462` 只拼 `来源: source_type · source_id · produced_by · relation`、`:458` excerpt，**`source_url`/`claim_number` 无渲染位置**）；报告区 `:1807-1811`、`:1952-1953 renderMarkdown`；`idea.html:1763-1772` 的 md→html 只支持 h1-h3/加粗/列表（表格/链接渲染不了）。i18n：`static/i18n.js:2 I18N_COMMON = {zh:{…}, en:{…}}`，键风格 `页名.驼峰`（例 `:96 'idea.title'`，英文表 `:447`），取值 `:710 function t(key, vars)`，`idea.html:237` 已引 i18n.js、全页 131 处 `t(`。**既有【推测】约定只活在 prompt 纪律层**：`routes/idea.rs:671-673`、`ai/patent.rs:14`、`ai/client.rs:274`、`routes/ai.rs:1728`；检测器 `ai/fact_check.rs:437-472 unsourced_data`（用例 `:706`）在 pipeline/idea 侧零接线。导出：**idea 无 docx 导出**（`docx_export/export.rs:10/:18` 唯一调用方是 OA `routes/ai.rs:2103-2110`），创意报告 = `steps/finalize.rs:149-236` Markdown 拼装 + `routes/idea.rs:1160-1236` 三档 + `report.html`（`:1297-1360`，浏览器打印转 PDF）。
- **范围**：
  ① 事实性结论附源**只走 `Evidence` 侧**：新增 `produced_by` 取值（如 `"AiDeepAnalysis"` / `"RagCitation"`）与 `source_type` 的 `chunk` 一档（值域是应用层字符串，CHECK 只在 `evidence_chain` 的 `source_type NOT NULL` 上，**无 DB CHECK 约束**，故不需迁移）；每条证据必须能反查 MB3 的 `patent_chunks` 行（`source_id` = chunk id）与专利号 + 段/权号（`claim_number` 现有 Option 字段转正，`contradiction.rs:91`/`rank.rs:73` 的 `claim_number: None` 须补齐或注明为何留空）。
  ② 无源结论标【推测】：结构化承载，即「该结论无 `evidence_ids`」本身即事实，前端据键渲染标签；**禁止往 `ctx.ai_analysis` 纯字符串贴补丁**（§1.6 m 的 12 个读点会互相打脸）。
  ③ 决策建议段：新增 `#[serde(default)] decision: Option<DecisionAdvice>` 进 `PipelineContext`（`pipeline/context.rs:228-316`），随既有 `idea_versions.context_json` 持久化（`routes/idea.rs:1136-1138` 反序列化点已就位）——**不加列、不加表**；结构 `{stance: 申请|放弃|转向, rationale: [{text, evidence_ids}]}`，理由必须引用报告内已有证据编号。报告文本从结构化字段渲染，不许反向解析字符串。
  ④ executive 档冲突裁定（§1.6 n）：`routes/idea.rs:1176-1180` 的硬编码「建议」3 条与本包决策段语义重叠，**规划裁定 = 取代**（改为从 `decision` 渲染，编号可反查），PR body 须注明该项被取代。
  ⑤ 前端最小破线（§2.1 授权）：`templates/idea.html` 限 `loadEvidence`（`:436-470`）+ 报告面板（`:1807` 附近）；`static/i18n.js` zh 表（`:96` 区）与 en 表（`:447` 区）**双表同时加键** `idea.evidence*` / `idea.decision*`。**禁止新增内联 `on*` 引用**（新增引用扫描只 INFO，但删除基线已有 = ERROR）；DOM 写入必须 `createElement + textContent`，需富文本时经 `i18n.js` 全局 DOMPurify 保护（AGENTS.md 2.5）；`source_url` 渲染为链接时禁止 `innerHTML` 拼用户/AI 文本。既有证据卡文案 `:456/:459-462` 是硬编码中文——**转 i18n 属扩大范围，须登记偏离或另包**，本包只保证新增文案走 i18n。
  ⑥ 出参只发结构化键（`api_idea_evidence :400` 已是全字段直发，本包沿用；决策段经 `api_idea_report :1117` / `api_idea_report_html :1240` 出，前端不猜文案）。
- **不做**：不动 OA 侧 `docx_export`（创意报告无 docx）；不改 `ai_analysis` 类型为结构化；不动 `functions-manifest.json`（只加不减不需 `--refresh`；若被迫需要 refresh，必须与模板同提交并说明原因）。
- **必做取证**：三份真实创意报告的 `get_evidence_by_idea` 导出 + 结论逐条对照表（10 条结论 → 有无 `evidence_ids` → 编号能否反查到 `evidence_chain` 行 → 再反查 `patent_chunks`/`patents`）；`source_url`/`claim_number` 在 UI 上可见的截图或 DOM 断言（e2e 红线未授权 ⇒ 用 Puppeteer 手动跑 `e2e_test.mjs` 现有 60 条，不新增用例）；红→绿锚 = 破线前 `loadEvidence` 无 `source_url` 渲染位置（grep 证明），破线后有且 XSS 路径走 `textContent`。
- **验收**：① 抽查 10 条结论 ≥9 条有源，且编号可反查；② 决策段存在、stance 三选其一、理由 ≥3 条且每条带 `evidence_ids`；③ 无源结论显示【推测】标签而非伪造来源；④ `cargo test` + ESLint `static/i18n.js` + `node check_html_functions.mjs` + `node e2e_test.mjs`（60/60）**四项全过**——本包动了模板，Step 5 的 e2e/HTML 扫描不再是不适用项；⑤ zh/en 双表键数一致（缺 en 键即视为未完成）。
- **门禁**：fmt / clippy / `cargo test`（+ 上述前端四项）；红线自检须额外跑 `git diff main..<branch> -- templates/ static/`，**且逐文件确认只落在 §2.1 授权范围内**。

### MB1 · 幻觉防线扩面（可插入棒，无前置，建议在 MB0 合并后的任一空档）

- **现状锚点**：`ai/fact_check.rs:20 FactWarning{category,description,severity}`、`:31 FactCheckReport{warnings, score: f64}`（`Default` = 100 分 `:37`）；入口 `:79 pub fn check_oa_analysis(ai_output: &str, references: &str, my_patent: &str) -> FactCheckReport`（三参数全是原文文本，**无专利号**）；扣分 `compute_score(:105-116)` 致命 35 / 高 15 / 中 5 / 其他 2；`:119 format_report` 输出「可信度评分：X/100 + 编号警告列表 + 人工核实提示」纯文本；`:14` 注释原文＝「注：本模块为 OA 分析增强功能的预留层，当前未接入主流程，已标记 `#[allow(dead_code)]`」（**已过时**）。四项核查的场景耦合度：①技术领域 `:243`（依赖「D1/对比文件」句式 `:226` + 领域词表 `:269`）、②段落引用 `:362`（正则 `[00XX]`/第 X 段 `:314-341`）、④A33 `:545`（`HIGH_RISK_AMENDMENT_TERMS :489` + 「修改为」定位 `:510`）= **OA 专属**；③无来源数据 `:405`（数字 + 单位模式 `:409`）= **通用**。接线只有一处：流式 `api_ai_office_action_response_stream(:1295)` 的 `routes/ai.rs:1433` 调用 → `:1434` 拼「## AI 事实核查（请人工复核）」→ sanitize 后以**匿名 data 事件**追加流尾（`:1435-1437`，无专用 event name，前端靠文本标记识别，见 §1.6 i）、不拦截、拼进 `full_text` 随 `save_oa_analysis(:1441)` 落库；非流式 `api_ai_office_action_response(:1123)` 零核查。测试基础：现有单测 **15 条**（`fact_check.rs:621-814`：四项校验 10 + 综合 2 + format 2 + 边界 1），`tests/` 四个集成文件均无 fact_check。可复用中止机制见 §1.6 j。
- **范围**：
  ① 签名泛化：保留 `check_oa_analysis` 作为 OA 薄壳入口（不动现有 OA 行为），新增通用 `check_analysis(claimed_output: &str, sources: &[(label: &str, text: &str)]) -> FactCheckReport`；把 ②段落引用 泛化为「引用存在性」（创意侧对 `ctx.top_matches` 的标题/URL/`patent_chunks` 引用逐条验存在），④泛化为「引入外部来源未记载的术语」，①按场景分档（无「D1/对比文件」句式时**不得判 0 命中即通过**，要如实标 `not_applicable`）。禁止把 OA 词表当通用词表硬套创意场景。
  ② 接线点择一：`steps/finalize.rs:13-14`（`ai_analysis` 定稿处，`:13` 已有空值兜底）或 `orchestrator/engine.rs:311` 派发前后；idea 侧对应 `routes/idea.rs:877 api_idea_pipeline`（后台 spawn `:920`，落库 `:930-992`）。**OA 与非 OA 必须共用同一判定实现**——确定性判定现全仓唯一（§1.6 k），本包禁止在 route/pipeline 里另写校验。
  ③ 处置：默认**标注**，但改为**结构化键**出（不再沿 `ai.rs:1435-1437` 的文本标记做法；OA 流式那处本包一并改造为发键，属既有行为变更须在 PR body 声明）；致命档（扣分 ≥35）**升级为拒绝并给可读原因**——复用 `engine.rs:186-205`（critical → `StepStatus::Error` + `record_failure` + 回滚快照 + Err）或 `OrchestratorCommand::Abort{reason}`（`:72-76`），前端经 `routes/idea.rs:1066 api_idea_progress` 收进度事件；**禁止新造中止/降级机制**。
  ④ 更正 `fact_check.rs:14` 过时注释（改为如实写明 OA 流式 + 创意 pipeline 两处接线）。
- **不做**：不接 `templates/`/`static/`（UI 呈现留 MB4，本包只保证结构化键可消费）；不动 schema；不引入 LLM 判定（`routes/ai.rs:2000 api_ai_check_amendments` 那条 LLM 路线保持独立，注明它与确定性核查是两层）。
- **必做取证**：用例集按 4 类诱导（编造法条 / 编造页码段落号 / 编造引用 URL / 无来源数字）各 ≥2 条，**OA 与创意两条路径各一套 ⇒ 新增 ≥16 条**；红→绿锚 = 接线前把同一份含编造引用的创意分析文本喂给 pipeline，断言 fact_check **未被调用**（warnings 恒空 / 无核查字段），接线后同一输入必须被标记或拒绝。**OA 侧回归**：现有 15 条单测零删改，流式改造后前端仍能拿到核查结果（以结构化键断言，不以文案断言）。
- **验收**：① 诱导用例集 100% 被标记或拒绝（列出每条用例的 category/severity/最终处置）；② 两条路径共用同一实现（grep 断言判定函数唯一）；③ 致命档拒绝真的走 `StepStatus::Error`/`Abort` 且用户能读到原因（非 Rust panic 文本，AGENTS.md 2.7）；④ `:14` 注释已更正。
- **门禁**：fmt / clippy / `cargo test`（基线 + 2N，N ≥16；lib 与 bin 双入口各计一次）；templates/static 零改动 ⇒ e2e / HTML 扫描不适用。

### MB5 · 上下文治理（收口棒，可最后；现状全量见 §1.5）

- **范围**：
  ① 关键事实不进压缩摘要：现状是 `ai.rs:59-67 patent_reference_material()` 把专利号/标题/摘要/claims 拼成自由文本、`:354` 进 system prompt，而多轮正文走 `compress_history`（`:196`，触发双条件 `:202-205`，裁掉最早 `len-8` 条 `:208-210`，压成**单条 assistant 摘要** `:241-247`）⇒ **首轮用户约束必被摘要吞掉**（system 不受影响，`ai/chat.rs:40-47`）。改造 = 首轮约束与关键事实改由 system 层结构化通道固定注入，复用既有 `ai.rs:38-43 bounded_reference_material(label, content)`（已带 `<user_input>` 包裹 + `escape_prompt_material :29-34`），与记忆注入（`context.rs:16 CONTEXT_MAX_CHARS=6000` → `ai.rs:360-365`）同一条通道。**禁止破坏 `keep_recent=8` 与「单条 assistant 摘要」的现有形状**（前端已按此消费，且改形状需动 templates ⇒ 破线）。
  ② 双套摘要实现收敛：`routes/idea.rs:500-501`（`auto_summary_threshold=8`/`keep_recent=6`）+ 增量分段 `:516-551` + 二级压缩 2000 字 `:554` + 窗口 `:738-741` 与 ai.rs 那套（`>10 条且 >8000 token`）行为不同 ⇒ 本包要么统一策略，要么**显式登记为何保留两套**并让两套都满足「首轮约束不丢」，禁止只改 ai.rs 留 idea 分裂。
  ③ OA 容量统一为「报错不截断」：现只有两处真报错（讨论 `ai.rs:1631-1653`、答复书流式 `ai/patent.rs:803-815`），**静默截断缺口清单**（本包逐项改为 `ai/client.rs:314-322 oa_capacity_error`，上限常量 `:307-312`）：非流式 OA `ai.rs:1123` → `patent.rs:243-245 safe_truncate(300000/200000/300000，按字节且无尾注)`；流式 `ai.rs:1295` → `patent.rs:666-668`；深度模式二次自审 `patent.rs:756-757`(8000/12000)；修改核查 `ai.rs:2000` → `patent.rs:318-320`；讨论历史 capacity 通过后**仍二次截断** `ai.rs:1661 MAX_DISCUSSION_FOR_AI=120_000` + `:1670-1684 chars().take`；OA 讨论导入 `ai.rs:1931 api_oa_discussion_import` **零容量校验**。另注明：`ai/patent.rs` 还有 `:37/:62/:107/:108/:151/:201/:202/:643/:756/:757/:1000/:1001/:1046/:1047` 等静默截断点，本包只改**与 OA/claims/专利号相关**的，其余逐条登记「为何不改」。
  ④ **中文 panic 修复（本包唯一硬 bug，与 MB0 同类）**：`ai.rs:1698-1699` 按**字节索引**硬切讨论历史字符串，超长中文越限即 panic（生产路径）；改为 `truncate_for_ai`（`client.rs:267`）或 `Vec<char>` 窗口，必须有红→绿锚（旧切法对中文长讨论历史 panic、新写法零 panic，取证方式沿 MB0 的「全起点循环 + 纯中文/混排/emoji」用例形状）。
  ⑤ 20 轮回归：判据**必须改写为可测形式**——现设计 `keep_recent=8` 必然丢弃首轮 user 原文，所以验收口径是「20 轮后首轮约束仍进入 AI 上下文（system 层结构化字段仍在）」，而非「首轮原文仍在 history」。取证主路 = 单测（`compress_history` 是 `routes/ai.rs` 私有 async fn，同文件已有 `#[cfg(test)] mod tests`（`:2201/:2219` 附近），可直接构 40 条 `Vec<(String,String)>` 断言返回长度/角色；fake `AiClient` 参照 `ai/tests.rs:95`，临时库参照 `tests/patent_hub_integration.rs` 惯例）；辅路 = e2e `setRequestInterception` 抓 messages（沿 `e2e_test.mjs:255-296` 模式，provider 侧 mock 不真调）——**但新增 e2e 用例须先获用户裁定并同步 `expectedPasses`（`:5` 现 60）**，否则 MB5 只做单测取证、e2e 另包（§2.1）。**陷阱**：用例消息必须写到 >8000 估算 token（`:203` 双条件），否则压都没压，「约束仍生效」是假绿。多模态分支 `ai.rs:406` 与文本分支 `:450` 两处调用都要覆盖。
  ⑥ pipeline 截断余量销账：MB3 遗留的 `analysis.rs:156/:180/:193/:250`（300/500/2000）等送入 AI 前的截断，逐条判定是显示用途还是数据用途，数据用途一律改 `truncate_for_ai`（AGENTS.md §2.5）。
- **不做**：不动 `messages` 构造的条数上限设计（`chat.rs:40-47/:56-63` 无上限是有意为之）；不改输出预算（`client.rs:647 max_tokens=8192`、`chat.rs:159/:253 16384`）；不做真实计费的多轮压测（压缩本身会调 AI 花钱，取证一律 mock）。
- **验收**：① `ai.rs:1699` 字节切片消失且有红→绿锚；② 缺口清单里每个 OA 入口都改为「超限即 `OA_INPUT_TOO_LARGE` 报错」，且有断言证明**不再静默丢内容**（沿 `ai/tests.rs:95 response_letter_stream_emits_visible_overflow_error` 的做法）；③ 20 轮回归用例可跑且真触发了压缩（断言摘要事件确实发生，否则用例无效）；④ ai.rs 与 idea.rs 两套摘要在「首轮约束不丢」上行为一致或差异有登记理由；⑤ MB3 遗留截断点逐条有结论。
- **门禁**：fmt / clippy / `cargo test`（+2N）；若裁定动 e2e ⇒ `node e2e_test.mjs` 全过 + `expectedPasses` 同步 + 该破线须有用户裁定记录。

---

## 3. 排期、基线对账与风险

```
MB0（embedder 安全 + 写入链） → MB2（免费全文 + 冷却共用） → MB3（RAG 接线） → MB4（出处 + 决策段）
                              ↘ MB1（幻觉防线扩面，独立，可在 MB0 之后任一空档插入）
MB5（上下文治理）：M-B 收口棒
```

| 包 | 规模 | 硬前置 | 破线项（§2.1 之外一律打回） | e2e / HTML 扫描 |
|---|---|---|---|---|
| MB0 | M（已完工待收口） | — | `src/main.rs` 模块声明 1 行 | 不适用 |
| MB2 | M | MB0 合并 | 无 | 不适用 |
| MB3 | **L**（含两处前置缺陷修复） | MB0 + MB2 | `src/main.rs` 模块声明 1 行 | 不适用 |
| MB4 | **L** | MB3 引用编号 | `templates/idea.html` + `static/i18n.js`（最小范围见 MB4⑤） | **必跑** |
| MB1 | M | MB0 合并 | 无（OA 流式出参由文本标记改结构化键 = 既有行为变更，须在 PR body 声明） | 不适用 |
| MB5 | M+ | MB3（截断余量销账） | `e2e_test.mjs` **仅在用户裁定后** | 视裁定 |

### 3.1 门禁基线对账口径（每包 PR body 必须逐条归因，禁止 `| head` 截断统计）

- 派包基线：**721 = lib 335 + bin 337 + 集成 49**（tip `6e8813f`）。
- **MB0 合并后基线 = 744**（实测 746 全绿，其中 2 条属仓库外临时取证件 `tests/mb0_red_proof_tmp.rs`，不计入）：lib 347 passed（+12）、bin 348 passed（+11）、集成 49 不变。差额 `721 + 2N − 1 = 744`（N=12），**少的那 1 条 = `rag/chunker.rs::compute_chunk_embedding_delegates_to_single_vector_impl` 只在 lib 侧跑**，因为 `src/main.rs` 没有 `mod rag`（§1.6 g）。
- **MB3 加 `pub mod rag;` 后基线会再 +1**（那条委托用例在 bin 侧现身），属**非新增测试导致的跳变** ⇒ MB3 对账公式为 `744 + 2N + 1`（再加 rag 其余既有单测的 bin 侧现身，逐条列）。审计时若看到「测试数涨了但没写测试」，按本条判定为正常，不得为此删测试。
- 此后各包一律按「上一包合并后的实测总数 + 2N」对账，N = 本包新增 `#[test]` 条数（双入口各计一次，rag/ 侧新增除外，见上）。

### 3.2 风险与规避（派包时评估，执行棒不必复述但必须遵守）

- **同仓禁止并行开两棒**：共享 `target/` 会损坏 `incremental/`（已记 `docs/errors.md` 2026-09-28），且 D 盘余量现测 **5.9GB**（`df -h /d`，641G/646G 已用 100%），两个构建同时跑必爆盘。清盘只允许删 `target/debug/incremental` 与 `target/debug/deps/*.pdb`，**禁止 `cargo clean`**。
- **编译耗时**：本机 MB0 实测 `cargo test` 冷编 **5m48s**、clippy **3m27s** ⇒ 每棒门禁预留 ≥20 分钟，禁止中途重复起编。
- **独立 crate 编译会崩**：`D:\Temp\mb0-red-proof\red_err.txt` 实测 standalone crate 触发 rustc `0xc0000409 STATUS_STACK_BUFFER_OVERRUN` ⇒ **红→绿取证一律写在仓内 `tests/` 集成测试或 `#[cfg(test)]`，禁止另建临时 crate**。
- **中文 locale 链接器告警**：本机 lib 构建有 `linker stdout: Non-UTF-8`（GBK）warning，`clippy -D warnings` 实测**不受影响**（本机与 CI 均通过）；出现即忽略，禁止为此加 `#[allow]`。
- **反爬**：MB2 的批量富化与 XHR 同 Host ⇒ top-N 即使限流仍可能触发冷却；处置=**冷却即降级，禁止重试**，更禁止为此绕过 `chain.rs` 的降级链。
- **行号漂移**：本文所有行号基于「tip `6e8813f` + MB0 未合并工作区」的现网工作树；MB0 合并后行号会再漂移，**执行棒以函数名/符号为准重新定位**，发现与现实冲突按前言反驳本文件。

### 3.3 交付定义（每棒一一对应，缺一不得标完成）

`exec/<包名>` 分支 → PR（body 必含：红→绿取证原文、门禁全量统计、破线自检 `git diff main..<branch> -- <红线文件>` 逐文件 0 证明、§2.1 授权项的偏离说明）→ 规划会话只读审计 + **独立复跑门禁** → GitHub 原生 merge（`env -u GITHUB_TOKEN gh pr merge`）→ 五端推送对齐 → 回写 STATUS / task-breakdown / MASTER §6 / 本文件对应包的「落地记录」。

### 3.4 待用户裁定的决策门（不裁定则相应包不得开工，其余包不受影响）

1. **MB4 前端破线**：出处/决策段没有任何不破线的可见落点（§1.6 l）。放行「`templates/idea.html` + `static/i18n.js` 最小改动」，还是把 MB4 缩为纯后端包（结构化字段 + 出参就位，UI 可见性另开一包）？
2. **MB4 executive 档硬编码「建议」被取代**（§1.6 n）：现 `routes/idea.rs:1176-1180` 的固定 3 条建议将改为从结构化决策段渲染——用户可见文案会变，需确认。
3. **MB1 致命档「拒绝」**：核查扣分 ≥35 从「标注」升级为「整条流水线中止」，对用户是体验倒退还是保护？若不接受，退路是只标注 + 前端强提示（须同步放开 UI 线）。
4. **MB5 是否新增 e2e 用例**：新增即破 `e2e_test.mjs` 红线并要同步 `expectedPasses`；不新增则 20 轮验收只靠单测。
5. **MB0b 触发条件**（§0 的 B 案全量「补 IDF / 语料统计」）：待 MB3 产出真实 `patent_chunks` 语料后再评估——评估时点 = MB3 合并后的首个规划会话。

## 4. 遗留开放项（登记不派包）

**M-A 遗留**：

1. 英文机构名 assignee 形态仍受上游反爬阻断，未取证；
2. SerpAPI / EPO 真实凭证冒烟 + 配额预警真实数据触发（等用户配 Key，见 §0 决策 B）；
3. OS 级断网注入取证（现以通道级证据替代，已如实入文）；
4. MA6「错误信息再润色」（体验打磨）。

**M-B 规划本轮新发现（刻意不进任何包，越界即审计打回）**：

5. `pipeline/steps/analysis.rs:37 deep_analysis_simple` 是**零调用点死码**（§1.6 e）——MB3 要求「两处同改」但禁止顺手删；删除它需另评估（它是否仍是 quick 模式的预期降级路径）。
6. `patents_embedding` / `patent_chunks` 的**全表批量回填**（历史专利永远拿不到 embedding/切片，除非重新入库）——4.3GB 用户库，需单独设计分批 + 断点 + 可取消方案，不在 M-B 六包内。
7. `ai/client.rs::truncate_for_ai` 之外的展示型截断（`safe_truncate*` 全仓散落点）与 `context.rs:53-61` 记忆截断——MB5 只改「送入 AI 且属数据用途」的那部分。
8. `templates/idea.html:456/:459-462` 既有硬编码中文证据文案转 i18n（AGENTS.md 2.5 双语纪律的历史欠账）——MB4 只保证新增文案走 i18n。
9. `routes/ai.rs:2000 api_ai_check_amendments` 的 LLM 判定与 `fact_check.rs` 确定性判定两层并存是否合并（§1.6 k 判定为不同层，暂不并）。
10. MB0 实测的既有浮点求和顺序抖动（HashMap 迭代序 ⇒ 1 ulp 级差异，测试以 1e-6 容差锁定）：属打分公式既有行为，本轮不改；若要确定性需换 BTreeMap 或固定排序，另议。

## 5. M-B 跨包结构化出参契约（先定协议，再写包，禁止各包自造）

### 5.1 命名与形状纪律

沿用仓内既有范式，不新造风格：snake_case 顶层键；**空结果整键省略**（先例 `routes/search.rs:161-163/:178-180`）；秒数**向上取整**（先例 `:221`）；数组元素一律带稳定枚举 `reason_code`，**前端只认键不猜文案**（先例 `templates/search.html:599-605` 的 `cooldownBySource` 映射）；既有出参的键集合**必须被单测锁定**（MB0 已立此范：`routes/search.rs` 断「7 键不增减」）。反面教材：`routes/ai.rs:1434-1437` 把核查结果拼成「## AI 事实核查（请人工复核）」文本 + 匿名 data 事件让前端按文本识别——MB1 要消灭这种做法，**新增一律发键，存量按包内声明改造**。

### 5.2 键与生产者/消费者对照表

| 键（顶层/嵌套路径） | 生产者 | 消费者 | 形状 | 出处包 | 备注 |
|---|---|---|---|---|---|
| `cooldowns[]` | 既有 MA6b | `templates/search.html` | `[{source, remaining_secs}]` | — | 现网先例，本表仅作风格基线 |
| `enrichment` | MB2 | 分析页 / pipeline 内部 | `{requested, enriched, failed:[{patent_id, reason_code, remaining_secs}]}` | MB2④ | 空则整键省略 |
| `full_text_available_count` | MB2 | 报告与 UI | `u32` | MB2④ | 与 MB3 的「降级为摘要档」判据共用 |
| `patent_id`（`RankedMatch` 字段） | MB3② | MB3③ 反查、MB4① 出处 | `Option<String>`，`#[serde(default)]` | MB3 | 在线命中无库 id 时如实 `None`，禁止伪造 |
| `rag`（若外露） | MB3③ | UI（MB4 之后才渲染） | `{chunks_used, distinct_patents, citations:[{ref_no, patent_id, chunk_id, source_type, claim_number}]}` | MB3 | 本包默认只进 prompt 与 `ctx`，**不加路由**（§2.1 `common.rs` 零破口） |
| `evidence[]` 的 `source_url` / `claim_number` | MB4① | `templates/idea.html:436-470` | 既有字段转正（非新增） | MB4 | 悬空 id 视为缺陷不是特性 |
| `decision` | MB4③ | 报告三档 + UI 决策段 | `{stance: "file"\|"abandon"\|"redirect", rationale:[{text, evidence_ids[]}]}` | MB4 | 存 `idea_versions.context_json`，不加列 |
| `speculative`（或 `has_source: false`） | MB4② | UI 打【推测】标签 | bool/键存在性 | MB4 | **禁止**把「无源」写进 `ai_analysis` 字符串 |
| `fact_check` | MB1③ | UI（呈现留 MB4，键先行） | `{score, warnings:[{category, severity, description}], disposition: "annotated"\|"rejected", reject_reason_code}` | MB1 | 取代文本标记做法 |
| `OA_INPUT_TOO_LARGE` → `{code, field, actual_chars, max_chars}` | MB5③ | 前端可读错误 | 结构化错误体 | MB5 | 现 `ai/client.rs:314-322` 已含 `actual/max` 文本，本包结构化它 |

### 5.3 `reason_code` 统一枚举（跨包共享，新增取值必须同 PR 回写本表）

`cooldown`（同 Host 冷却中，零出网）｜`blocked`（上游反爬/非 200）｜`timeout`（20s 抓取超时）｜`not_found`（号不存在）｜`parse_empty`（抓到但剥标签为空）｜`no_fulltext`（MB3 降级摘要档）｜`db_error`（写库失败）｜`already_enriched`（幂等命中 `routes/patent.rs:238-240`）。

规则：**`not_found` 与 `parse_empty` 与 `cooldown` 三者不得混用**——把「没拿到」写成「没有相关内容」正是 MB3⑥ 禁止的假象；`cooldown` 必须同时带 `remaining_secs`；任何包新增 reason_code 而未回写本表，审计打回。

---

## 6. 逐包用例清单（执行棒可直接抄的测试名与断言点）

> 计数规则：lib/bin 双入口 ⇒ 每条 `src/` 内单测计 2 次；`rag/` 侧在 MB3 前只计 1 次（§3.1）。断言点写的是**必须为真**的判据，不是实现提示。测试一律离线（`:memory:` 或临时文件库），**禁止真出网、禁止真调 AI、禁止碰 `innoforge.db`**。

### MB2（新增 ≥6，计 12）

| 测试名 | 位置 | 断言点 |
|---|---|---|
| `enrich_url_lang_selection_unchanged_for_cn_and_other` | `routes/patent.rs` tests | 抽出 `build_enrich_url(pn, lang)` 后 CN→`zh`、非 CN→`en` 行为与 `:243-250` 逐字一致；URL 构造点全仓唯一（配合源码 grep 自检） |
| `cooldown_hit_short_circuits_before_request` | 同上 | 预置 `breaker::record(...)` 后，富化前置判定返回「不进网络」+ `reason_code=cooldown` + `remaining_secs>0`。**这是「零出网」的离线可证形态，比数请求次数更硬** |
| `cooldown_source_matches_search_path_source` | 同上 | free 抓取用的 source 值必须与 `routes/search.rs:667-670` 过滤所用的同一档一致（防旁路，§1.6 a） |
| `enrich_outcome_keys_snapshot` | 同上 | `enrichment` 键集合与 §5.2 逐字相等；`failed[]` 元素键集合锁定；空结果时整键省略 |
| `enriched_text_not_truncated_on_write` | 同上 | stub 一段超长中文 description（≥6000 字），入库后 `get_patent` 取回字符数不减；断言写入路径未调用 `truncate_for_ai`/`safe_truncate` |
| `extract_section_chinese_long_html_no_panic` | 同上 | 手写剥标签实现 `:334-386` 在超长中文 + emoji + 中英混排输入下零 panic（MB2 新引入风险，与 MB0 同类） |

### MB3（新增 ≥11，计 22；`pub mod rag;` 落地后含 `rag/` 侧全部双计，见 §3.1 跳变说明）

| 测试名 | 位置 | 断言点 |
|---|---|---|
| `search_chunks_reads_all_selected_columns` | `db/rag.rs` tests | **红→绿锚**：旧实现 SELECT 只有 `id,content` 却 `row.get(2)` 取 embedding（§1.6 f）⇒ 修前 Err/panic，修后 embedding 可反序列化为 `Vec<f32>` |
| `patent_chunk_struct_covers_v21_columns` | 同上 | `PatentChunk` 与 v21 表列对齐（`model_name`/`created_at` 补齐），往返写入读出等值 |
| `insert_patent_writes_chunks_for_chinese_patent` | `db/patent.rs` tests | `:memory:` 入一条中文专利 ⇒ `count_chunks` 0→≥3，且 `source_type` 覆盖 abstract/claim/description 三档 |
| `insert_patent_skips_chunks_for_textless_patent` | 同上 | 无文本专利零切片，且不报错 |
| `insert_patent_survives_chunk_write_failure` | 同上 | 切片写失败只 `warn`，`insert_patent` 仍返回 `Ok(final_id)`（静默降级纪律，同 MB0） |
| `chunks_are_reverse_lookupable_by_id` | `rag/mod.rs` 或 `db/rag.rs` | `{patent_id}-{index}` 形态的每个 id 都能 `get_chunk` 取回，且 `patent_id` 一致 |
| `rag_reuses_single_cosine_impl` | `rag/retriever.rs` tests | `retrieve_chunks` 的相似度与 `vector::VectorIndex::cosine_similarity`（`vector/mod.rs:118`）逐点相等（禁第二套，§1.6 f 的 `db/rag.rs:149` 私有份须收敛） |
| `citation_blocks_appear_in_deep_prompt` | `pipeline/steps/deep_reasoning.rs` tests | **主红→绿锚**：接线前 prompt 内 `### [引用 ` 计数为 0，接线后 ≥5，且 **distinct `patent_id` ≥5**（见 §7 MB3-3 假绿） |
| `downgrade_labels_reason_not_absence` | 同上 | 无全文时上下文里必须带 §5.3 的 reason_code，且全文里 grep 不到「没有相关内容」这类把未取到说成不存在的表述 |
| `snippet_truncation_uses_truncate_for_ai` | 同上 + `analysis.rs` | 两处超限（`analysis.rs:49-50`/`deep_reasoning.rs:137-138`）改后：超限片段带 `truncate_for_ai` 的「数据完整性提示」尾注，未超限片段逐字不变 |
| `chunk_text_all_char_starts_no_panic` | `rag/chunker.rs` tests | `chunk_text` 仍是字节切法 + 边界回退：对纯中文串做「全起点 × 全长度」切片循环零 panic，且切片边界只落在字符边界 |

### MB4（新增 ≥6 + 前端四项）

| 测试名 | 位置 | 断言点 |
|---|---|---|
| `evidence_ids_resolve_to_real_rows` | `db/evidence.rs` 或 `routes/idea.rs` tests | 每条 `TechnicalFeature.evidence_ids` 与 `decision.rationale[].evidence_ids` 都能在 `evidence_chain` 命中行；命中行再能反查 `patent_chunks`/`patents`（悬空即失败） |
| `decision_struct_roundtrips_context_json` | `routes/idea.rs` tests | 写 `idea_versions.context_json` → `:1136-1138` 反序列化后 `stance` 与 ≥3 条理由完整无损 |
| `executive_report_renders_decision_not_hardcoded` | 同上 | `routes/idea.rs:1176-1180` 的固定 3 条建议不再出现；决策段来自 `decision` 字段（§3.4 决策门 2 裁定后生效） |
| `claim_without_evidence_is_flagged_structurally` | 同上 | 无源结论出参有 `speculative`/`has_source:false` 键；**断言 `ai_analysis` 字符串本身没被插入标注标记**（禁往纯字符串贴补丁） |
| `evidence_api_key_set_locked` | 同上 | `api_idea_evidence`（`:400`）出参键集合快照锁定，新增键须显式登记 |
| `i18n_keys_parity_zh_en` | Node 侧（沿 `check_html_functions.mjs` 风格，禁改该扫描器规则） | `static/i18n.js` zh 表与 en 表键集合相等；本包新增的 `idea.evidence*`/`idea.decision*` 两侧都有 |

前端四项（非 cargo）：`cargo fmt`+`clippy`+`test`、`node node_modules/.bin/eslint static/i18n.js` 无 error、`node check_html_functions.mjs` 退出 0（**只加不减即无需 `--refresh`**）、`node e2e_test.mjs` 全过现有条数。

### MB1（新增 ≥9 条测试；诱导编造用例按 OA/创意双路径参数化，实例总数 ≥16）

| 测试名 | 位置 | 断言点 |
|---|---|---|
| `induce_fabricated_law_article_oa_flagged` / `..._idea_flagged` | `ai/fact_check.rs` tests | 编造法条在两条路径各 ≥1 条被标记或拒绝；`category` 命中期望值 |
| `induce_fabricated_paragraph_ref_...` | 同上 | 编造段落/页码；**泛化后创意侧走「引用存在性」而非 OA 段落正则** |
| `induce_fabricated_citation_url_...` | 同上 | 编造 URL：对 `ctx.top_matches` 的标题/URL 逐条验存在，命中不了即警告 |
| `induce_unsourced_number_...` | 同上 | 无来源数字（校验③ `:405/:409` 通用，双路径同一实现） |
| `idea_pipeline_invokes_analysis_check` | `pipeline/steps/finalize.rs` 或 `orchestrator` tests | **红→绿锚**：接线前同一输入下核查函数零调用（或 `fact_check` 字段恒默认），接线后必被调用 |
| `fatal_score_aborts_with_readable_reason` | 同上 | 扣分 ≥35 走 `StepStatus::Error`/`Abort{reason}`（`engine.rs:186-205`/`:72-76`），`disposition="rejected"`；可读原因里 grep 不到 Rust panic/`Expect` 文本（AGENTS.md 2.7） |
| `oa_and_idea_share_single_verdict` | 同上 | 判定入口唯一：泛化后的 `check_analysis` 被 OA 薄壳与创意侧共同调用；grep 断言 `routes/idea.rs`/`pipeline/` 内无本地新写的校验逻辑 |
| `oa_stream_emits_structured_factcheck_keys` | `routes/ai.rs` tests | 流式尾包改为发 `fact_check` 键；现有 15 条 `fact_check.rs:621-814` 单测**零删改**且仍全绿 |
| `non_applicable_not_counted_as_pass` | `ai/fact_check.rs` tests | 创意场景缺「D1/对比文件」句式时，校验① 返回 `not_applicable` 而**不是**「0 警告 = 通过」（§7 MB1-1 假绿） |

### MB5（新增 ≥7）

| 测试名 | 位置 | 断言点 |
|---|---|---|
| `compress_history_byte_slice_no_panic_on_chinese` | `routes/ai.rs` tests | **红→绿锚**：`ai.rs:1698-1699` 旧写法对超长中文讨论历史 panic，改后零 panic；全起点循环用例形状沿 MB0 |
| `first_turn_constraint_survives_compression` | 同上 | 40 条消息 + 估算 >8000 token（必须真触发压缩，见下条）后，发给 AI 的 messages 里 system 层仍含首轮约束/专利号/日期/claims 的结构化字段 |
| `compression_actually_triggered_in_test` | 同上 | 同一用例断言压缩**确实发生**（mock client 调用计数 ≥1 或摘要事件存在）；否则上一条是假绿 |
| `oa_entries_reject_oversized_not_truncate` | `src/ai/tests.rs`（沿 `:95` 范式） | §1.5③ 缺口清单逐入口参数化：超限返回 `OA_INPUT_TOO_LARGE`，且 body 不再被 `safe_truncate` 静默裁剪 |
| `oa_discussion_import_capacity_guarded` | `routes/ai.rs` tests | `api_oa_discussion_import`（`:1931`）超限被拒（现零校验） |
| `discussion_history_not_double_truncated` | 同上 | capacity 通过后不再二次截（`:1661`/`:1670-1684`）：断言送入内容字符数 = 校验通过时的字符数 |
| `idea_and_ai_summary_policy_consistent` | `routes/idea.rs` tests | 两套摘要（`idea.rs:500-501` vs `ai.rs:202-205`）在「首轮约束不丢」上同结论；若故意保留差异，测试须断言差异被文档化（§2 MB5②） |

---

## 7. 反假绿清单（每包最可能的「看起来过了」，审计逐条识破）

**MB0（已合并前须防）**：只把 tokenize 改安全却没删两份拷贝 → `grep -rn "cleaned\[i\.\.i" src/` 必须为空；用 `#[allow(dead_code)]` 保留旧实现冒充「收敛」；用全表回填冒充「写入链接通」（MB0 明令禁止）；给分数加归一化让相似度「看起来更准」（§0 决策禁止）。

**MB2**：
1. **先抓后判**：代码调了 `remaining()` 但放在出网之后，冷却形同虚设——审计看调用顺序，要求「判据在 `reqwest` 发起之前」且 `cooldown_hit_short_circuits_before_request` 用的是纯判定函数而非整链 handler。
2. 用**另一个 source 值**查冷却表（等价于没查，就是 §1.1 第 27 行的旁路本体）——`cooldown_source_matches_search_path_source` 专治此条。
3. 把「未拿到全文的原因」写成中文字符串塞进 message，前端 `includes('冷却')` 猜——出参键快照断言。
4. 幂等判据 `>50` 字符被放宽成「只要有内容就跳过」，导致半截 HTML 冒充全文——要求 `enriched_text_not_truncated_on_write` 与 `parse_empty` reason 同时存在。

**MB3**：
1. `retrieve_chunks` 错误时返回 `Ok(vec![])`（现状 `retriever.rs:20`）被当作「降级成功」——要求降级带 reason_code，不得静默变空。
2. 切片只写 `abstract` 一档（`source_type CHECK` 允许三档）就宣称接线——`insert_patent_writes_chunks_for_chinese_patent` 断三档齐全。
3. **≥5 篇引用被实现成同一片专利的 5 个 chunk**——`citation_blocks_appear_in_deep_prompt` 必须断 **distinct `patent_id` ≥5**，这条是 MB3 最容易蒙混的验收点。
4. 测试里手搓 `PatentChunk` 塞进内存库，绕过 `insert_patent` 挂点，冒充「写入链接通」——要求同时有「入库即自动切片」路径的用例。
5. 只改 `deep_reasoning.rs` 不改 `analysis.rs`，拿死码 `deep_analysis_simple` 说「两处都改过了」——grep 两处 `chars().take` 均消失。

**MB4**：
1. `evidence.source_id` 指向不存在行（悬空引用看起来很美）——`evidence_ids_resolve_to_real_rows` 逐级反查到 `patents`。
2. 【推测】写进 `ai_analysis` Markdown（省事但违反 MB4②）——断言字符串未被插入标记、结构化键存在。
3. 后端键齐了但 UI 一个像素没变（「接口已支持」式交付）——要求 DOM 渲染取证，且 §2 MB4⑤ 授权范围就是这个目的。
4. i18n 只加 zh 键（en 走 fallback 也能跑）——键集合相等断言。
5. `functions-manifest.json` 被顺手 `--refresh` 掩盖函数删除——本包只加不减，出现基线变更即视为红旗，要求说明。

**MB1**：
1. **`not_applicable` 当通过**：泛化校验① 后创意侧因缺「D1」句式恒 0 命中，报告「无幻觉」——`non_applicable_not_counted_as_pass` 专治。
2. 致命档只标不改：`is_critical` 若为假，失败会被 `engine.rs:203-205` 吞成 `Skipped` 继续跑，等于没拒——要求断言状态机真的进 `Error`/`Abort`。
3. 在 idea 侧另写一份校验函数冒充「接入」——`oa_and_idea_share_single_verdict`。
4. 删改现有 15 条 OA 单测来让改造通过——用例数对账 + 逐条名比对。

**MB5**：
1. **压缩没触发就断「约束仍在」**：消息写得太短，`:203` 双条件不满足，`compress_history` 根本没跑，测试自然全绿——必须同测断言压缩真发生。
2. 用真 AI 调用跑压缩（花钱且不确定）——一律 mock；本包禁止真实计费。
3. capacity 报错修了，但后续 `:1670-1684`/`:1698-1699` 仍二次截断——`discussion_history_not_double_truncated`。
4. 只改 `ai.rs` 不碰 `idea.rs`，留两套行为——两包同断言。
5. 验收口径被偷偷改回「首轮原文仍在 history」（不可能达成，会被判为假实现）——本包口径是「结构化字段仍进上下文」。

**通用（审计会话自查）**：门禁统计禁止 `| head`/小值 `-A`（已记 `docs/errors.md` 2026-09-28，退出码来自 `head` 不是 cargo）；`#[ignore]` 数量必须与基线一致（现 lib 1 / bin 1 / doc 1）；「测试数涨了但没写测试」按 §3.1 归因，不得为此删测试；行号断言以符号名为准（§3.2 行号漂移）。

---

## 8. 审计复跑命令卡（规划会话只读核验用，执行棒无需照抄但结果必须可复现）

```bash
df -h /d | tail -1                       # 先确认余量 ≥5GB，否则先清 target/debug/incremental（禁 cargo clean）
cargo fmt --check                                  > D:/Temp/<包>-fmt.txt     2>&1; echo EXIT=$?
cargo clippy --all-targets -- -D warnings           > D:/Temp/<包>-clippy.txt 2>&1; echo EXIT=$?
cargo test                                          > D:/Temp/<包>-test.txt   2>&1; echo EXIT=$?
grep "^test result" D:/Temp/<包>-test.txt          # 全量逐行求和，禁止 head；同时记录 ignored 数
git diff main..<branch> -- src/common.rs Cargo.toml Cargo.lock src/db/migrations.rs src/lib.rs  # 红线自检
git diff main..<branch> -- src/main.rs             # 只允许模块声明行（§2.1）
git diff main..<branch> -- templates/ static/ e2e_test.mjs check_html_functions.mjs docs/functions-manifest.json  # 非 MB4 必须为空
```

MB4 额外：`node node_modules/.bin/eslint static/i18n.js`、`node check_html_functions.mjs`、`node e2e_test.mjs`（现有条数全过）。时间预算：本机实测冷编 `cargo test` **5m48s**、`clippy --all-targets` **3m27s**，clippy 之后 test 需重链 DLL（见 `docs/errors.md`）⇒ 顺序建议 fmt → clippy → test 全后台跑，勿用截断命令求快。**审计结论必须能贴出 `test result` 行集合的原文**，否则不写「N passed」。

---

## 9. M-B 期间的对外口径边界（写进注释、UI 文案、CHANGELOG 的承诺上限）

| 能力 | 可以承诺 | 禁止承诺 | 依据 |
|---|---|---|---|
| 向量/语义检索 | 本地字符 n-gram TF 相似度补充档：可用、不 panic、全仓单一实现、0 行时如实关闭 | 「语义检索」「相似专利识别」「向量召回质量」 | §0 决策；`vector/mod.rs` 函数头注释（丢 term→维度映射 ⇒ 相似度不构成语义相关性） |
| 全文供给（MB2 后） | 「分析前尽力从公开源补全文；补不到会告诉你为什么」 | 「所有专利都有全文」「支持 EPO/SerpAPI 全文」（付费链未冒烟） | §1.1、§0 决策 B |
| RAG（MB3 后） | 报告里的引用可逐条反查到库内切片行 | 「引用即正确」「模型不会编造」；引用相似度数值不得当语义分数展示 | §5.3、`assembler.rs:40/:52` 引用编号形状 |
| 出处与决策段（MB4 后） | 事实性结论有源、无源标【推测】、建议可反查编号 | 「律师意见」「授权/无效结论」——项目面向研发用户不是代理人 | AGENTS.md §一 |
| 幻觉核查（MB1 后） | 确定性判定（法条/段落/无来源数据/术语）+ 致命档处置 | 「已消除幻觉」「零误报」 | `fact_check.rs:105-116` 是启发式扣分非证明 |
| 上下文治理（MB5 后） | 关键事实结构化直传、超限报错不静默截 | 「任意长度 OA 都能处理」 | `client.rs:307-312` 上限是硬阈值 |

补充纪律：① 新增/修改面向用户文案必须 `static/i18n.js` zh+en 双键（AGENTS.md 2.5），且用词不得超出上表右列；② CHANGELOG 只写用户可感知项，M-B 内部收敛（拷贝删除、挂点接通）归「改进」且措辞按上表；③ 代码注释里凡写了「本轮有意不修」的局限，**禁止**在 UI/文档用乐观措辞覆盖它——口径冲突时以代码注释与本表为准，并回写 §1.6 校正。

---

## 10. 派单卡（执行棒直接复制：分支 / 白名单 / DoD 勾选 / PR body 模板）

### 10.1 统一 PR body 六段模板（缺任一段 = 审计不打回也先补齐再看）

```markdown
## 1 包号与范围
M-B / MB<n> · 一句话说明本包改了什么、为什么改（对照规格书 §2 对应包）

## 2 红→绿取证原文
命令 + 输出原文粘贴（禁止只写"已验证"）。红侧必须是旧实现失败的真实报错文本，
绿侧必须是新实现通过的 `test result` 行原文。

## 3 门禁全量统计
fmt / clippy --all-targets -D warnings / cargo test 三条命令的 exit code，
+ `grep "^test result"` 的**全部行**（逐二进制），+ ignored 计数。
对账公式：基线 <上一包合并后数字> + 2N ± 已归因跳变 = <本次实测>

## 4 红线自检
逐文件 `git diff main..HEAD --stat -- <红线文件>` 输出为空的证据；
若命中 §2.1 预授权破口，贴出 diff 原文并标注"授权项：§2.1 第 n 行"。

## 5 偏离登记
本包与规格书写不一致之处（含反驳，见 §13）。无偏离写"无"。

## 6 验收逐条对照
引用规格书 §2 MB<n> 验收条目 + §6 用例名，逐条标 ✅/⚠️/❌ 与证据位置。
```

### 10.2 各包派单卡

**MB0 收口卡（特殊：代码已在 `exec/mb0-embedder` 工作区，未提交）** ✅ **已执行完毕（2026-10-01 回写）**：收口棒已按本卡提交代码 `f66912c`（+12 测试），开出 **PR #28**（⚠️ 远端 head 仅 `f66912c`；errors 代记 `667064a` 当时未推送，其内容随 PR #29 入库），远端 CI `test` / `lint` / `e2e` 三项 SUCCESS、`mergeable=MERGEABLE`，取证件 `tests/mb0_red_proof_tmp.rs` 未入库。本卡转为留档，余下动作 = 规划会话按 §16 九步审计后合并。

| 项 | 内容 |
|---|---|
| 可提交白名单 | `src/vector/mod.rs`、`src/rag/chunker.rs`、`src/routes/search.rs`、`src/db/patent.rs`、`src/main.rs`（**仅 `pub mod vector;` 声明行**）、`docs/plan/2026-09-28-mb-construction-spec.md`、`docs/plans/STATUS.md`、`docs/feedback.md`、`docs/errors.md`、`CHANGELOG.md` |
| 取证件处置 | `tests/mb0_red_proof_tmp.rs` **不入提交**（包约定：临时文件）。但删除前必须把它 `cargo test --test mb0_red_proof_tmp` 的**两条 test 结果行原文**贴进 PR body 第 2 段；删除动作用 `git status` 证明工作区干净 |
| 禁止 | 全表回填；给向量分数加归一化/伪 IDF；`#[allow(dead_code)]` 保留旧拷贝；为消 GBK 链接器告警加 `#[allow]` |
| DoD | §2 MB0 验收 5 条 + §7 MB0 段 4 条假绿自查 |
| 门禁预算 | fmt 秒级 / clippy `--all-targets` **3m27s** / test **5m48s**（本机冷编实测，`-j 2` 更稳） |

**MB2 卡**｜分支 `exec/mb2-fulltext-chain`｜白名单：`src/routes/patent.rs`、`src/search/breaker.rs`（**只读调用，禁改表结构与时长常量**）、`src/routes/search.rs`（仅为 source 判据对齐所需的最小改动，须在 PR body 声明）、新增测试同文件内｜禁改：`Cargo.*`、`src/db/migrations.rs`、`src/common.rs`、`templates/`、`static/`、`src/main.rs`｜关键决策点（须在 PR body 二选一写明）：富化核心放 `routes/patent.rs` 内还是抽 `src/search/enrich.rs` 新模块（抽新模块须同步 `lib.rs`/`main.rs` 模块清单 ⇒ **会破 §2.1 授权范围，默认不抽**）｜DoD：§2 MB2 五条验收 + §6 六条用例全绿。

**MB3 卡**｜分支 `exec/mb3-rag-wiring`｜白名单：`src/db/rag.rs`（前置缺陷修复）、`src/rag/*.rs`、`src/db/patent.rs`（切片挂点）、`src/pipeline/context.rs`（`patent_id` + `rag_chunks` 两个 `#[serde(default)]` 字段）、`src/pipeline/steps/{scoring,deep_reasoning,analysis}.rs`、`src/main.rs`（**仅 `pub mod rag;`**）｜禁改：`src/common.rs`（**禁止新增 rag 路由**）、migrations、`templates/`、`static/`、`Cargo.*`｜三个必须显式裁定的点（写进 PR body）：切片取数挂点选 `scoring.rs:13` 还是 `engine.rs:310` 之前（**只能选一个**）；`retrieve_chunks_by_keyword` 实现还是删除；`build_citations` 与 `build_fallback_citations` 留哪个｜门禁特殊：基线跳变须按 §3.1 逐条归因｜DoD：§2 MB3 五条验收 + §6 十一条用例。

**MB4 卡**｜分支 `exec/mb4-provenance`｜白名单（**唯一放开前端的包**）：`src/pipeline/context.rs`、`src/pipeline/steps/finalize.rs`、`src/routes/idea.rs`、`src/db/evidence.rs`、`templates/idea.html`（限 `loadEvidence` 函数体 + 报告面板渲染）、`static/i18n.js`（zh/en 双表加键）｜禁改：`e2e_test.mjs`、`check_html_functions.mjs`、`docs/functions-manifest.json`（新增函数引用扫描只 INFO，不需 `--refresh`；若出现需要 `--refresh` 的情形 = 你动了函数增删，须专门说明）、migrations（决策段走 `idea_versions.context_json`，**不加列**）｜硬纪律：新 DOM 一律 `createElement + textContent`，富文本走 `i18n.js` 全局 DOMPurify 保护；截断只能用于显示，出处/AI 输入数据保留全文（AGENTS.md 2.5）｜门禁额外四项：ESLint、HTML 扫描、`node e2e_test.mjs` 现有条数、fmt/clippy/test｜前置：**§3.4 决策门 1、2 已由用户裁定**，未裁定不得开工。

**MB1 卡**｜分支 `exec/mb1-factcheck-breadth`｜白名单：`src/ai/fact_check.rs`、`src/routes/ai.rs`（流式出参改结构化键）、`src/pipeline/steps/finalize.rs` 或 `src/orchestrator/engine.rs`（核查挂点二选一）、`src/routes/idea.rs`（Err/进度事件透传）｜禁改：templates/static（呈现留 MB4）、migrations、`Cargo.*`｜硬约束：不新增中止机制（复用 `engine.rs:186-205`/`:72-76`）；OA 与非 OA 共用同一判定；现有 15 条单测零删改；`:14` 过时注释顺手更正｜前置：**§3.4 决策门 3 已裁定**｜DoD：§6 十条用例（≥16 条新测试）。

**MB5 卡**｜分支 `exec/mb5-context`｜白名单：`src/routes/ai.rs`（`compress_history`、讨论历史截断、OA 入口 capacity）、`src/routes/idea.rs`（第二套摘要策略）、`src/ai/patent.rs`（`safe_truncate` → `oa_capacity_error`）、`src/ai/client.rs`（capacity 错误结构化，**不动上限常量值**）、`src/pipeline/steps/analysis.rs`（MB3 遗留截断余量）｜禁改：`Cargo.*`、migrations、`e2e_test.mjs`（除非 §3.4 决策门 4 放行）、templates/static（错误文案只发键）｜两条不可让步项：`ai.rs:1698-1699` 中文 panic 必须有红→绿锚；20 轮用例必须**同测断言压缩真触发**｜一律 mock，禁止真调 AI 计费。

---

## 11. 用户视角走查剧本（M-B 收口必跑；对应 AGENTS.md Step 5「改了什么就测什么」）

> 环境准备（硬条件）：**无 `SERPAPI_KEY`、无 `EPO_KEY/EPO_SECRET`**；数据库用临时空库实例（`D:\Temp\mb-probe\`，禁碰 `D:\test\patent-hub-backup\innoforge.db`）；端口用 `INNOFORGE_PORT=3921` 避让；浏览器走查产物（截图/结论）留仓外，仓内只提交结论行。

| # | 步骤 | 必须看到 | 看不到时对应哪个包的缺陷 |
|---|---|---|---|
| 1 | 首页 → 检索（本地库 FTS + `/api/search/vector`） | 中文关键词能出结果、页面不报错；`vector_count` 与实际入库量一致（0 行时如实 0） | MB0（若向量档恒 0 或 panic） |
| 2 | 上传一份中文 PDF 专利 → 详情页「加载全文」 | 5 个标签页全文完整、**末尾不被截断** | 非 M-B 范围（`upload.rs` 不回写 `patents`，见 §1.1）——登记不假装通过 |
| 3 | 详情页点「免费富化」（`enrich-free`） | `description/claims` 由空变非空，**或**明确显示「未拿到的原因」徽标（走 `reason_code`） | MB2 |
| 4 | 冷却期内再点一次富化 | **零出网**、UI 显示剩余冷却秒数、不白烧配额 | MB2（旁路就是 §1.1 第 27 行那个洞） |
| 5 | idea 页创建创意 → 跑深度分析（长文本，≥150 字创意描述） | 报告里出现 ≥5 条专利全文切片引用，每条可点开反查 | MB3 |
| 6 | 同份报告逐条看结论 | 事实性结论带（专利号 + 段/权号）；无源的显示【推测】标签而非编造来源；`source_url`/`claim_number` 在 UI 可见 | MB4 |
| 7 | 报告末尾 | 决策建议段：申请/放弃/转向 + ≥3 条理由，理由编号能反查回上文证据 | MB4 |
| 8 | 故意在创意描述里塞诱导句（如「参见专利法实施细则第 84 条第 2 款，见说明书第 999 段」） | 核查标记或拒绝，且拒绝给的是**可读原因**不是 Rust panic 文本 | MB1 |
| 9 | OA 答复页粘贴超长中文 OA（超过 `client.rs:307-312` 上限） | 明确报错「超长 + 实际/上限字符数」，**不是**静默吃掉后半段 | MB5 |
| 10 | OA 讨论连打 20 轮（每轮正文写长到能触发压缩） | 首轮设定的专利号/日期约束在第 20 轮回答里仍生效 | MB5 |
| 11 | 技术调研导出报告 | 导出全文完整（不得为省 token 截数据） | MB3/MB4 的截断改造若误伤数据用途即在此暴露 |
| 12 | 切换英文界面重复步骤 5-7 | 新增文案中英双语，无裸中文/无 `undefined` 键 | MB4（i18n 双表纪律） |

**关键原则**：走查是「收口判据」第 4 条（milestones.md M-B），不是可选加分项；任何一步失败必须在 STATUS 记为未收口，禁止用「单测全绿」替代。

---

## 12. 收口回写责任表（防止上一棒的账漏到下一棒）

| 时机 | 谁写 | 写什么 |
|---|---|---|
| 施工完成 | 执行棒 | PR（六段 body，§10.1）+ 代码 + 测试；`CHANGELOG.md` 的 `[Unreleased]` 条目（用户可见项才写） |
| 审计与合并 | 规划会话 | 独立复跑门禁（§8 命令卡）+ 逐文件红线 diff + §6 用例名与 §2 验收逐条对照；核验通过后 `env -u GITHUB_TOKEN gh pr merge`（merge commit，用户 2026-09-20 授权） |
| 合并后 | 规划会话 | 规格书对应包补「落地记录」+ §1.6 校正条目关闭；`docs/plans/STATUS.md` 新条；`docs/plan/task-breakdown.md` 行标 ✅ + commit hash；`milestones.md` M-B 判据打勾；五端推送对齐 |
| 踩坑 | 命中谁谁写 | `docs/errors.md`（工具/构建/CI）；执行棒白名单若不含该文件，由规划会话代记（既有惯例） |
| 反馈 | 规划会话 | `docs/feedback.md`（用户纠正/认可/边界澄清，含 why） |

> 历史教训（MA6d 曾一次性补账 M-A 全部 CHANGELOG 条目）：**每包合并当天就把用户可见项写进 `[Unreleased]`**，不留到里程碑收口时回忆式补写。草案见 §14。

---

## 13. 反驳与仲裁流程（执行棒发现规格书与现实冲突时怎么走，禁止假服从）

1. **不许假服从**。规格书前言已写死：现实与本文冲突时以证据为准并反驳本文。遇到「按规格书写了编译不过 / 判据不可能成立 / 授权范围不够」三类情况，**停下来登记偏离**，不要静默改方案，也不要硬凑一个能过的测试。
2. **证据门槛（三选一即可采信）**：① 编译器/测试的真实错误原文（含文件:行）；② grep 结果证明规格书点名的符号/行号不存在或已移位；③ 一条红→绿反向用例（照规格书写法跑红、按替代方案跑绿）。截图、口头描述、「我觉得」不算。
3. **登记位置**：PR body 第 5 段「偏离登记」，写清「规格书 §x 说 A，实测是 B，我按 B 做了 C，影响是 D」。规划会话在合并前把该偏离回写进 §1.6 校正表或 §2.1 授权表，**再合并**——避免同一坑被下一棒再踩。
4. **升级给用户的判定线**（只有这三类才打断用户，其余规划会话自行裁定）：改变包的验收口径｜改变红线范围（新增破线文件）｜需要新依赖/schema/对外 API 变更。其余按 §10.2 卡内「二选一/三选一」自行择一并在 PR body 声明。
5. **不可协商项**（任何反驳都不接受）：新 crate 依赖、schema 变更、全表回填、`unwrap/expect` 进生产路径、伪造 reason_code 或相似度分数、删改既有测试来让门禁变绿、动 `check_html_functions.mjs` 规则或静默删基线。

---

## 14. CHANGELOG `[Unreleased]` 草案（合并当天由执行棒搬运，措辞受 §9 口径约束）

> 版本动作：M-B 收口时按 **MINOR** 递增（新功能、非破坏性），`Cargo.toml` 的 version 同步。中文在前、英文随后（AGENTS.md 3.1）。以下措辞已刻意避开「语义检索」「已消除幻觉」等超出实现的表述。

```markdown
## [未发布] / [Unreleased]

### 新增 / Added
- 深度分析现在会自动为最相关的若干篇专利抓取公开全文（无需付费 API Key），并在报告中引用专利原文片段，每条引用可反查回库内该专利的切片原文（M-B / MB2、MB3）
- 创意分析报告新增「决策建议段」：明确给出申请 / 放弃 / 转向，并附不少于 3 条可溯源理由（M-B / MB4）
- 分析报告的事实性结论强制附来源（专利号 + 段落/权利要求号），无来源的结论标注为推测（M-B / MB4）
- AI 事实核查从 OA 答复扩展到创意分析：编造法条、页码、引用或无来源数据会被标注，达到致命档时中止并给出可读原因（M-B / MB1）
- 新入库专利自动生成检索向量与文本切片（M-B / MB0、MB3）

### 修复 / Fixed
- 修复中文专利文本在向量检索时分词越界导致的服务崩溃（M-B / MB0）
- 修复超长中文 OA 讨论历史在裁剪时导致的服务崩溃（M-B / MB5）

### 改进 / Improved
- OA 输入超长时改为明确报错并显示实际/上限字符数，不再静默丢弃超出部分（M-B / MB5）
- 长对话中的关键事实（专利号、日期、权利要求）改走结构化传递，不再被历史摘要吞掉（M-B / MB5）
- 全文抓取与检索共用同一套反爬冷却判断，被限流时不再重复出网（M-B / MB2）

### 已知限制 / Known limitations
- 本地向量检索为字符 n-gram 相似度补充档，不构成语义理解能力；0 行向量时该档自动关闭并如实上报（M-B §9）
- 历史存量专利未做向量与切片批量回填，仅新入库与新富化的记录生效（M-B §4.6）
```

---

## 15. M-B 施工看板（唯一进度事实源，状态变化当天更新）

> 规则：`STATUS.md` 记事件流水，本表记状态快照；两者冲突时以本表为准并查因。状态枚举只有六个：`未开工` / `施工中` / `待审计` / `待裁定` / `打回重做` / `已合并`，**禁止**出现枚举外的词（「基本完成」「快了」「差不多」一律不算状态）。本表由规划会话维护，执行棒在 PR body 里自报的状态不直接改表，由审计落账。更新于 2026-10-01。

| 包 | 分支 | 状态 | PR / merge | 门禁实测 | 基线对账 | 决策门 | 备注 |
|---|---|---|---|---|---|---|---|
| MB0 | `exec/mb0-embedder` | **待审计**（收口棒已提交 `f66912c` 代码 + 12 测试，取证件未入库。⚠️ **远端 #28 head 仅 `f66912c`**；本地 docs 提交 `667064a` 当时未推送，其内容随 PR #29 分支 ancestry 入库） | **PR #28** OPEN，`mergeable=MERGEABLE` | 远端 CI `test`/`lint`/`e2e` 三项 SUCCESS；规划会话代跑 fmt 0 / clippy 0 / test 746（含仓外取证件 2 条） | 744 = 721 + 2×12 − 1 | 无 | 下一步 = §16 九步审计后合并；合并前 OA-U 不开工（OA-U §0.1）、PR #29（docs）不得先合 |
| MB2 | 未建 | 未开工 | — | — | 公式 744 + 2N | 无 | 下一棒；串行纪律 §3.2 |
| MB3 | 未建 | 未开工 | — | — | 744 + 2N + 1（`pub mod rag;` 跳变，§3.1） | 无 | 依赖 MB2 合并 |
| MB4 | 未建 | 未开工 | — | — | 上一包合并值 + 2N | 门1、门2 | 门未裁不开工 |
| MB1 | 未建 | 未开工 | — | — | 上一包合并值 + 2N | 门3 | 可插棒，任一空档 |
| MB5 | 未建 | 未开工 | — | — | 上一包合并值 + 2N | 门4 | 收口棒 |
| MB0b | 未建 | 未开工（**未立项**，等门5） | — | — | — | 门5 | 评估时点 = MB3 合并后首个规划会话 |

## 16. 审计作业流程（规划会话收到执行棒 PR 后的固定 9 步，禁止跳步、禁止跳过失败步骤继续）

每步带通过判据；任一步不过即停，直接走第 9 步出结论，**禁止**「先看完再综合判断」——综合判断是把缺陷摊薄成措辞的来路。

1. **环境预检**：`df -h /d` 余量 ≥ 6GB；`git status` 无第二棒的工作区改动；确认无并行构建（§3.2）。不过 ⇒ 先清 `target/debug/incremental` 与 `deps/*.pdb`，仍不过 ⇒ 挂起审计。
2. **改动面白名单审**：`git diff main...HEAD --stat` 逐文件与 §10.2 白名单比对，多一个少一个都不过；白名单内文件还要看 diff 是否越出该卡声明的能力面（如 MB0 的 `main.rs` 只许 +`pub mod vector;` 声明行）。
3. **PR body 六段齐**（§10.1）：缺段 ⇒ 有条件通过，补齐前不进第 4 步之后的重活。
4. **红→绿锚复核**：red 侧必须是旧实现可复现的真实报错原文（必要时在临时 worktree 检出旧代码单独验证，**禁止**动执行棒工作区）；green 侧必须是新实现的 `test result` 行原文。仓外 standalone crate 取证一律不认（§3.2 rustc `0xc0000409`）。
5. **门禁独立复跑**（§8 命令卡）：全量落盘统计，`test result` 行逐二进制抄录，禁止 `| head`。本机预算：fmt <1min、clippy ~3.5min、test ~6min（冷编上限 8min，超时先查并行污染）。
6. **计数对账**（§3.1 公式）：对不上且无 §3.1 预登记跳变归因 ⇒ 打回，**不得**「数字差不多就过」。
7. **反假绿抽查**（§7）：该包「硬判据」级条目逐条验断言点写在测试里（如 MB3 的 distinct `patent_id` ≥5 是断言，不是口头）。
8. **安全纪律抽查**：新增 `unwrap()/expect()` 全仓 diff 比对（必须全落 `#[cfg(test)]`）；生产路径新增 `panic!` 同查；动了 templates/static 则 eslint / HTML 扫描 / e2e 三件套齐跑。
9. **结论（四态之一，写进 PR 审计评论 + §15 看板）**：**通过**（全步绿）｜**有条件通过**（仅文档/账面级缺口，限期补）｜**打回**（代码/测试级缺陷，列「文件:行 + 复现方式」清单回给执行棒，状态改打回重做）｜**升级用户**（命中 §13 三线之一）。

## 17. 回归风险矩阵（每包「不许弄坏」的既有资产，具名到测试）

> 用法：执行棒动手前过一遍本包行；审计第 7 步按本行抽查。以下名字全部现网存在（2026-10-01 复核），改名/删除走 §13 反驳。

| 包 | 具名资产 | 破坏方式 | 必做验证 |
|---|---|---|---|
| MB2 | `inner_url_emits_spec_2_param_set`（XHR URL 形态锁） | 动 URL 构造时改了既有参数渲染 | 该测试零改动仍绿；真要改 URL 语义先走 §13 |
| MB2 | `enrich-free` 幂等早退（`routes/patent.rs:238-240` "Already enriched"） | 把幂等判定挪到冷却判定之后，导致冷却中仍重复出网 | 早退必须保持在冷却判定**之前**；`reason_code=already_enriched` 只作记账不改早退语义 |
| MB2 | `attempts_json_locks_frontend_panel_literals` / `online_chain_registers_epo_only_with_credentials` | 在 `routes/search.rs` 顺手改出参或链注册 | 两锁零改动仍绿 |
| MB3 | `fts_search_finds_matching_patent`（集成） | 在 `insert_patent` 里把切片写入挪进 FTS 同步之间，破坏时序 | 集成全绿；切片写入沿 MB0 纪律：放既有写入全部落库**之后**、函数返回之前 |
| MB3 | `insert_patent_writes_embedding_for_chinese_patent` / `..._skips_embedding_for_textless_patent` / `..._survives_when_embedding_write_fails`（`db/patent.rs:940/:966/:983`） | 改同一函数尾部 | 三条零改动仍绿 |
| MB3 | `mb0_vector_layer_gated_off_when_no_embeddings` / `mb0_vector_layer_engages_when_embeddings_exist`（`routes/search.rs:2077/:2128`） | 动向量门控条件 | 两门控测试零改动仍绿；向量档 7 键形状不增减 |
| MB4 | `e2e_test.mjs` 全量 60 条（`expectedPasses = 60`，`e2e_test.mjs:5`） | 改 `templates/idea.html` 结构破坏既有断言 | 60/60；任何计数变化属破线须升级用户——门4 只覆盖 MB5 的新增用例，不覆盖 MB4 |
| MB4 | `docs/functions-manifest.json` 基线 | 新增 on* 函数忘 `--refresh`，或误删函数 | 扫描退出 0；基线只加不减，`--refresh` 与模板变更同提交 |
| MB4 | `api_idea_evidence`（`routes/idea.rs:400`）出参既有消费方 | 改出参键名 | §6 `evidence_api_key_set_locked` 快照锁兜底；改键 = 破坏性变更须升级 |
| MB1 | `fact_check.rs` 既有 15 条测试（2026-10-01 实测 `grep -c '#\[test\]'` = 15） | 泛化判定入口时改了既有判定语义 | 15 条零删改仍绿（§6 MB1 表已锁） |
| MB1 | OA 流式旧前端消费（oa-response 页按「## AI 事实核查」文本识别，`routes/ai.rs:1434-1437`） | 直接删旧文本标记 | 新旧并行期：结构化键先行，旧文本标记保留至呈现层落地后由后续包删；跨包时两 PR 各声明半边 |
| MB5 | `truncate_for_ai` 既有用例（`src/ai/tests.rs:29-62`：`short_text_passes_through_untouched` / `text_at_limit_is_not_rewritten` / `overflow_keeps_head_and_marks_omission_explicitly` / `limit_counts_characters_not_bytes` / `zero_budget_returns_only_the_notice`） | 「顺手统一」删掉数据完整性尾注 | 五条零改动仍绿；MB5 只增不减提示面 |
| MB5 | OA 容量既有真报错用例（`src/ai/tests.rs:76-95`：`short_oa_input_passes_without_truncation` / `unicode_capacity_uses_character_count` / `response_letter_stream_emits_visible_overflow_error`） | 改容量错误响应形状 | 三条零改动仍绿；结构化错误体按 §5.2 增键不删键 |
| 通用 | `git config core.hooksPath .githooks` + 远端 CI（fmt/clippy/test/扫描/e2e） | 本地 `--no-verify` 跳钩子 | CI 是硬关卡，本地绿不算数；审计以 §16 第 5 步独立复跑为准 |

## 18. MB4 新增文案预审稿（zh/en 草案，执行棒照抄或走 §13 反驳，禁止自由发挥措辞）

键名沿 `static/i18n.js` 既有风格（小写点分域前缀，zh/en 同键，先例 `cad.*`、`diag.*`）。措辞上限受 §9：不得出现「语义」「授权概率」「律师」「保证」；【推测】标签只说明「本条无出处」，不得写成「无相关内容」。

| 键 | zh | en |
|---|---|---|
| `idea.evidence.speculative` | 【推测 · 本条未找到出处】 | [Speculative – no source found] |
| `idea.evidence.refFormat` | 出处：{patent_id} {section} | Source: {patent_id} {section} |
| `idea.decision.title` | 决策建议 | Decision recommendation |
| `idea.decision.stance.file` | 建议申请 | Recommend filing |
| `idea.decision.stance.abandon` | 建议放弃 | Recommend abandoning |
| `idea.decision.stance.redirect` | 建议转向 | Recommend pivoting |
| `idea.decision.rationaleTitle` | 理由（编号可在上方报告中反查） | Rationale (IDs traceable in the report above) |
| `idea.decision.noEvidence` | 证据不足，暂不给出决策建议，仅列事实。 | Insufficient evidence; listing facts only instead of a recommendation. |

约束：① 每键 zh/en 同 PR 同批次；② 决策段的存在性不因证据不足缺席——`noEvidence` 是**替代渲染**，不是删除段落；③ stance 三值与 §5.2 `decision.stance` 枚举逐字对应，前端不做第四种解释；④ 本表为预审稿，执行棒有更好措辞走 §13 反驳并附理由，**禁止**合并后私自换词。

## 19. 术语表 + 接棒首查清单（给第一次进本仓的执行棒 AI）

**术语**（AGENTS.md 未定义、本规格书与审计在用的都在这里）：

- **棒 / 派单**：一个执行 AI 会话承接的一个包 = 一次分支 + 一次 PR + 一次审计。串行，禁止两棒同仓并行（§3.2）。
- **门禁**：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`；动了 templates/static 再加 HTML 函数扫描 + eslint + e2e。CI 是硬关卡，本地绿不算数。
- **红线**：声明「零 diff」的文件清单（逐包见 §10.2 卡；公共底线 `common.rs`/`Cargo.*`/migrations）。例外必须落在 §2.1 预授权表内。
- **对账**：测试计数公式 `上一包合并值 + 2N（±已登记跳变）`，逐二进制核对（§3.1）。
- **红→绿锚**：先证旧实现在真实输入上失败（报错原文），再证新实现通过（`test result` 原文）。防「测试写出来就是绿的」。
- **假绿**：测试通过但断言的不是验收口径（§7 逐包清单）。
- **形状锁 / 快照锁**：断言出参键集合或字面量不变的测试，防「顺手改接口」。
- **静默降级**：可选增强失败时主流程继续 + `warn` 记账 + 出参如实（先例：embedding 写失败不阻断入库）。
- **单一写入口**：`db/patent.rs::insert_patent` 是 `patents`/FTS/embedding/chunks 的唯一写路径，派生数据挂在它尾部，禁止旁路。

**接棒首查（10 分钟，做完才有资格写代码）**：

1. `git status` 干净、基于 main 最新；按 §10.2 卡建分支（分支名逐字照抄）。
2. `git config core.hooksPath .githooks`（AGENTS.md Step 0）。
3. `df -h /d` 查余量；确认无并行棒。
4. 顺序读：AGENTS.md → STATUS.md 顶 3 条 → 本规格书 §0、§1.6、本包节、§10.2 本包卡、§13、§17 本包行。
5. `node_modules/puppeteer` 不存在 ⇒ e2e 条件不适用（PR body 声明），**不得**现场安装（新依赖须升级）。

## 20. 决策门裁定记录表（用户统一裁定后回填；裁定前对应包不得开工，与 §3.4 同源）

| 门 | 主题 | 影响包 | 用户裁定（原文照录，禁止 AI 转述） | 日期 | 规格书回写位置 |
|---|---|---|---|---|---|
| 1 | MB4 前端最小破线放行否（`templates/idea.html` + `static/i18n.js`） | MB4 | ⏳ 待裁定 | — | §2.1 / §10.2 MB4 卡 |
| 2 | executive 档硬编码 3 条建议被结构化决策段取代（用户可见文案变） | MB4 | ⏳ 待裁定 | — | §1.6 n / §18 |
| 3 | MB1 致命档处置：中止流水线 vs 只标注+前端强提示 | MB1 | ⏳ 待裁定 | — | §2 MB1 / §5.2 `fact_check` |
| 4 | MB5 是否新增 e2e 用例（破 `e2e_test.mjs` 线 + 同步 `expectedPasses`） | MB5 | ⏳ 待裁定 | — | §3.4 / §17 通用行 |
| 5 | MB0b（B 案补 IDF/语料统计）是否立项 | MB0b | ⏳ 待裁定 | — | §0 / §3.4 |

规则：裁定后本表为事实源，§3.4 相应条目加删除线并注明「已裁定，见 §20」；**本表只做记录不做解释**，裁定的解释权在用户。

## 21. M-C 消费面预告（M-B 必须留对的缝，M-B 收口时逐条验）

MC1（检索↔创意打通）与 MC2（OA 链路复用核查）将来直接消费 M-B 的产出。M-B 各包落地时把下面这些留成**函数级可复用**，禁止埋死在 idea 页 handler 内部：

| M-C 任务 | 消费的 M-B 产物 | M-B 侧约束 |
|---|---|---|
| MC1 `SearchPatents`/`PriorArtCluster` 步骤替换 | MB2 `enrichment` / `full_text_available_count`（§5.2）；MB3 top-N 切片检索（`rag/retriever`） | 检索与富化保持「入参出参纯函数化」调用形态，pipeline 步骤替换时不需要重写查询逻辑 |
| MC1 创意报告引用 | MB3 引用编号（`{ref_no, patent_id, chunk_id, ...}`） | 引用形状不加 idea 专属字段；MB4 的 `decision`/`evidence` 对 pipeline 透明（存 `context_json` 不加列） |
| MC2 OA 分析/讨论/答复书全走核查 | MB1 `fact_check` 结构化键 + `check_analysis` 单一判定入口 | 判定入口必须是普通函数（可被 OA 流与答复书生成直接调用），禁止只活在流式 handler 里 |
| MC2 出处体系复用 | MB4 `Evidence` 转正字段（`source_url`/`claim_number`） | OA 侧复用不需要改 `Evidence` 定义；缺字段场景走 `#[serde(default)]` |

收口验法：M-B 收口走查（§11）通过后，规划会话另做一次「假想 MC1/MC2 接线走读」——只读代码，确认上表每行调用点存在且不依赖 UI 层；发现埋死即在 M-B 剩余包内顺手纠正（改函数位置不算扩包）。

---

## 22. 执行棒派单提示词（用户复制粘贴到执行棒 AI 会话，逐字用；占位符只有一处）

> 用法：整段复制对应代码块，把开头 `{执行棒名}` 换成棒号即可（如「MB2 执行棒」）。六份共用同一纪律骨架，差异只在包号/分支/范围/决策门前置。**MB4/MB1/MB5 三份带决策门前置行：对应门未裁定（§20 仍 ⏳）时不得开工，提示词贴了也不行。**

### 22.1 MB0 收口棒（特殊：接手工作区已有改动，只做提交收口，禁止重写）

> ✅ **本份提示词已执行完毕（2026-10-01 回写）**：收口棒已提交代码 `f66912c`、开出 PR #28（远端 head 仅 `f66912c`，`667064a` 未推入而随 PR #29 入库；CI `test`/`lint`/`e2e` 三绿、`mergeable=MERGEABLE`）。以下正文留档不再派发；下一份可用提示词是 §22.2（MB2），但其开工前置是 PR #28 审计合并。

```text
{执行棒名}：你是 InnoForge / patent-hub 仓库（D:\test\patent-hub-backup）的执行棒。
使命：把工作区里上一棒已完成的 MB0 改动收口提交，禁止重写任何实现。
必读（顺序）：AGENTS.md → docs/plans/STATUS.md 顶 3 条 → docs/plan/2026-09-28-mb-construction-spec.md 的 §0、§1.6、MB0 节（§2）、§10.2 MB0 收口卡、§13、§17 MB3 行（insert_patent 回归面）。
操作：工作区改动已在 src/vector/mod.rs、src/rag/chunker.rs、src/routes/search.rs、src/db/patent.rs、src/main.rs（仅 pub mod vector;），先 cargo fmt --check / clippy --all-targets -- -D warnings / cargo test 全量落盘统计（禁止 | head）确认三绿；cargo test --test mb0_red_proof_tmp 跑临时取证件，把两条 test 结果行原文贴进 PR body 第 2 段，然后删除该文件并用 git status 证明工作区干净；按 §10.2 MB0 卡白名单逐文件提交（分支 exec/mb0-embedder），PR body 用规格书 §10.1 六段模板，对账公式 744 = 721 + 2×12 − 1。
红线：不加 crate；不碰 innoforge.db；不新增测试；白名单外文件零 diff；全表回填与伪 IDF 禁止。
与现实冲突时按规格书 §13 反驳并贴证据，禁止假服从。
```

### 22.2 MB2 执行棒

```text
{执行棒名}：你是 InnoForge / patent-hub 仓库（D:\test\patent-hub-backup）的执行棒。
使命：完成规格书 MB2 包——免费全文供给链式化 + 冷却表共用（enrich-free 前置冷却判定、三份抓取拷贝收敛、§5.2 enrichment 出参键）。
必读（顺序）：AGENTS.md → docs/plans/STATUS.md 顶 3 条 → docs/plan/2026-09-28-mb-construction-spec.md 的 §0、§1.6、MB2 节、§5.2/§5.3、§6 MB2 表、§10.2 MB2 卡、§13、§17 MB2 行。
分支名：exec/mb2-fulltext-chain。测试按 §6 MB2 表实现（≥6 条，含红→绿锚），基线对账 744 + 2N。
红线：Cargo.* 零 diff（禁新依赖）；migrations/common.rs/lib.rs 零 diff；不碰 innoforge.db；生产路径禁 unwrap/expect；幂等早退 routes/patent.rs:238-240 必须保持在冷却判定之前（§17）；门禁三绿全量落盘统计，禁止 | head。
交付：PR body 用 §10.1 六段模板；在线取证按 §24 规程。与现实冲突按 §13 反驳，禁止假服从。
```

### 22.3 MB3 执行棒

```text
{执行棒名}：你是 InnoForge / patent-hub 仓库（D:\test\patent-hub-backup）的执行棒。
使命：完成规格书 MB3 包——RAG 接线（insert_patent 写 chunks、db/rag.rs SELECT 缺列修对、深度分析 prompt 注入 ≥5 篇 distinct patent_id 全文切片、analysis.rs:50 与 deep_reasoning.rs:138 两处同批改走 truncate_for_ai、无全文降级如实标注 reason_code）。
必读（顺序）：AGENTS.md → docs/plans/STATUS.md 顶 3 条 → docs/plan/2026-09-28-mb-construction-spec.md 的 §0、§1.6（f/g 两条必读）、MB3 节、§5.2/§5.3、§6 MB3 表、§10.2 MB3 卡、§13、§17 MB3 行。
分支名：exec/mb3-rag-wiring。src/main.rs 补 pub mod rag;（§2.1 预授权，仅声明行），基线对账 744 + 2N + 1（mod rag 跳变 §3.1）。
红线：Cargo.* 零 diff；migrations/common.rs/lib.rs 零 diff；不碰 innoforge.db；生产路径禁 unwrap/expect；insert_patent 尾部时序不得破坏 FTS 同步与 MB0 wiring 三条测试（§17）；门禁三绿全量落盘统计，禁止 | head。
交付：PR body 用 §10.1 六段模板。与现实冲突按 §13 反驳，禁止假服从。
```

### 22.4 MB4 执行棒

```text
{执行棒名}：你是 InnoForge / patent-hub 仓库（D:\test\patent-hub-backup）的执行棒。
使命：完成规格书 MB4 包——出处标注体系（Evidence 转正、无源标【推测】走结构化键、决策建议段取代 executive 档硬编码 3 条建议、idea.html + i18n.js 最小破线渲染，文案照抄 §18 预审稿）。
前置条件：规格书 §20 门1、门2 必须已裁定（⏳ 未裁定则本包不开工，请退回派单方）。
必读（顺序）：AGENTS.md → docs/plans/STATUS.md 顶 3 条 → docs/plan/2026-09-28-mb-construction-spec.md 的 §0、§1.6（l/m/n 三条必读）、MB4 节、§5.2、§6 MB4 表、§10.2 MB4 卡、§17 MB4 行、§13。
分支名：exec/mb4-provenance。本包是唯一获准破前端红线的包：templates/idea.html 与 static/i18n.js 仅限 §10.2 卡白名单范围；e2e 60/60 必须保持（计数变化须升级用户）。
红线：Cargo.* 零 diff；migrations/common.rs/lib.rs 零 diff；不碰 innoforge.db；生产路径禁 unwrap/expect；门禁三绿 + eslint + HTML 扫描 + e2e 全量落盘统计，禁止 | head。
交付：PR body 用 §10.1 六段模板。与现实冲突按 §13 反驳，禁止假服从。
```

### 22.5 MB1 执行棒

```text
{执行棒名}：你是 InnoForge / patent-hub 仓库（D:\test\patent-hub-backup）的执行棒。
使命：完成规格书 MB1 包——幻觉防线扩面（check_analysis 泛化为单一判定入口、创意 pipeline 真实接线、OA 流式改发 fact_check 结构化键、诱导编造用例 ≥16 实例，致命档处置按门3 裁定执行）。
前置条件：规格书 §20 门3 必须已裁定（⏳ 未裁定则本包不开工，请退回派单方）。
必读（顺序）：AGENTS.md → docs/plans/STATUS.md 顶 3 条 → docs/plan/2026-09-28-mb-construction-spec.md 的 §0、§1.4、MB1 节、§5.2、§6 MB1 表、§10.2 MB1 卡、§13、§17 MB1 行。
分支名：exec/mb1-factcheck-breadth。fact_check.rs 既有 15 条测试零删改（§17）。
红线：Cargo.* 零 diff；migrations/common.rs/lib.rs/templates/static 零 diff；不碰 innoforge.db；生产路径禁 unwrap/expect；门禁三绿全量落盘统计，禁止 | head。
交付：PR body 用 §10.1 六段模板。与现实冲突按 §13 反驳，禁止假服从。
```

### 22.6 MB5 执行棒

```text
{执行棒名}：你是 InnoForge / patent-hub 仓库（D:\test\patent-hub-backup）的执行棒。
使命：完成规格书 MB5 包——上下文治理（routes/ai.rs:1698-1699 字节切片红→绿修字符安全、首轮关键事实结构化直传、OA 容量全入口「报错不截断」、双套摘要策略对齐）。
前置条件：规格书 §20 门4 已裁定（⏳ 未裁定则不加 e2e 用例，其余范围照常开工）。
必读（顺序）：AGENTS.md → docs/plans/STATUS.md 顶 3 条 → docs/plan/2026-09-28-mb-construction-spec.md 的 §0、§1.5、MB5 节、§5.2、§6 MB5 表、§10.2 MB5 卡、§13、§17 MB5 行。
分支名：exec/mb5-context。truncate_for_ai 五条与 OA 容量三条既有用例零删改（§17）。
红线：Cargo.* 零 diff；migrations/common.rs/lib.rs 零 diff；不碰 innoforge.db；生产路径禁 unwrap/expect；门禁三绿全量落盘统计，禁止 | head。
交付：PR body 用 §10.1 六段模板。与现实冲突按 §13 反驳，禁止假服从。
```

## 23. 审计评论四态模板（规划会话贴 PR 用，禁止写成散文）

```text
【审计结论：通过】
门禁独立复跑：fmt exit 0 ｜ clippy --all-targets -D warnings exit 0 ｜ cargo test exit 0（逐二进制：<原文行>）。
对账：<基线> + 2×<N> <±跳变归因> = <实测>，闭合。红→绿锚复核：通过（<一句话>）。
反假绿抽查：§7 本包条目全过。白名单 diff：零越界。结论：可合并。

【审计结论：有条件通过（限期补，合并前补齐）】
仅账面缺口：1) <条目，如 PR body 缺第 4 段红线自检原文> 2) <条目>。
门禁数字本身已闭合，代码不动。补齐后本会话复核即转「通过」。

【审计结论：打回重做】
缺陷清单（文件:行 + 复现方式）：1) <缺陷> 2) <缺陷>。
对应规格书条目：§<节>. 修复后重新走 §16 流程，勿在旧 PR 上混合无关改动。

【审计结论：升级用户】
命中 §13 判定线：<三线之一 + 事实>。已暂停合并，等用户裁定后按 §20 回填继续。
```

## 24. 在线取证操作规程（免费链真实出网怎么做才合规；MB2/MB3 验收要用）

- **实例纪律**：临时空库实例一律放 `D:\Temp\<包名>-probe\`，**禁止读写用户库 `innoforge.db`**；取证完实例可删，证据 JSON 留存至 PR 合并。
- **证据落盘**：原始响应存仓库外 `D:\Temp\`，PR body 贴关键 JSON 原文（URL、状态码、字节数、首条结果字段），**禁止**截图代替文本、禁止「已验证」三个字代替证据。
- **已知工具坑**（`docs/errors.md` 在案）：Git-Bash curl 中文体内联会被 GBK 编码打坏（实测 400）⇒ 一律 `--data-binary @utf8文件.json`；XHR 检索响应的结果数组在 `results.cluster[0].result`，不在 `results.patents`。
- **反爬处置**：同 Host 批量动作前先看冷却状态；遇 `Sorry...`/503 ⇒ 停手等窗口外重测并登记开放项，**禁止**重试绕过、禁止伪造数据填充缺口。
- **免费链边界**：SerpAPI/EPO 无 Key 环境**禁止**真调（§0 决策 B）；「断网复检」用通道级证据（实例无凭据 + 仅本地端点）并如实注明，OS 级断网注入属 §4 开放项，**不得**冒充已做。
- **取证不改仓**：取证动作零改动仓库文件；需要临时验证代码行为时用仓内 `#[cfg(test)]` 或临时 worktree，**禁止** standalone crate（§3.2 rustc 崩溃）。

## 25. 固定中文测试语料（各包测试共用的确定性 fixtures，字节级一致）

> 用法：各包测试模块内联**同字节串**常量（不建共享 fixture 文件，避免动 lib 结构）；改一个字节都算改判定口径，走 §13。语料只进 `#[cfg(test)]`，禁止进生产路径。

**`FIXTURE_CN_PATENT_TITLE`**（沿 MB0 红→绿锚同串）：

```
一种基于深度学习的图像识别方法及装置
```

**`FIXTURE_CN_PATENT_BODY`**（纯中文正文，覆盖 char-boundary / 切片 / chunk 场景）：

```
本发明公开了一种基于深度学习的图像识别方法及装置，涉及人工智能技术领域。所述方法包括：获取待识别图像；对所述待识别图像进行灰度化与归一化预处理，得到标准化图像；将所述标准化图像输入预先训练完成的卷积神经网络模型，提取图像特征向量；根据所述图像特征向量与预设分类阈值进行比较，输出识别结果；所述装置包括图像采集模块、预处理模块、特征提取模块与结果输出模块。本发明能够提升复杂背景下目标识别的准确率，降低误报率，适用于工业质检与安防监控场景。
```

**`FIXTURE_MIXED`**（中英混排 + emoji + 数字，测 UTF-8 边界与 token 窗口）：

```
The present invention relates to 一种图像处理装置（image processing apparatus），包括：GPU 加速模块 🚀、NPU 推理模块；所述 GPU 模块运行 CUDA 内核 v12.3，处理 4096×2160 分辨率输入；实验表明准确率提升 12.7%。
```

**`FIXTURE_LONG`**（超长语料，MB2 入库不截断 / MB5 压缩用）：**不落字面量**，由测试按确定规则生成——`FIXTURE_CN_PATENT_BODY` 整体重复拼接直至 ≥ 6200 字符（`chars()` 计数），末尾补「补」字至 6200 整。重复单元确定 ⇒ 字节确定。

**`FIXTURE_OA`**（OA 决定文书样例，MB1 诱导编造 / MB5 容量用）：

```
应申请人于 2025 年 3 月 2 日提交的意见陈述，审查员认为：权利要求 1 相对于对比文件 1（CN110123456A，说明书第[0034]段）不具备专利法第 22 条第 3 款规定的创造性。对比文件 1 公开了图像特征提取步骤，权利要求 1 与其区别仅在于预处理参数的常规选择。
```

## 26. M-B 收口报告模板 + 发布 runbook

**收口报告**（M-B 六包全合并后由规划会话产出，落 `docs/plan/`）：

```markdown
# M-B 收口报告（<日期>）
## 1 六包终态
拷贝规格书 §15 看板终态（逐包 merge hash + 门禁实测）。
## 2 门禁数字演进链
744 → <每包合并后实测，逐格挂 PR 号> → <最终值>；每步对账公式闭合证明。
## 3 用户视角走查（规格书 §11 剧本）
12 步逐行：现象 ✅/❌ + 证据（截图存仓外，文本摘录入报告）。
## 4 假想 MC1/MC2 接线走读（规格书 §21）
四行逐行结论：调用点存在且不依赖 UI 层 ✅/❌；发现埋死时的处置记录。
## 5 开放项移交
规格书 §4 未清项 + §20 裁定记录 → 作为 M-C 规划输入。
## 6 对外口径终检
CHANGELOG [Unreleased] 与规格书 §9 口径表逐条比对，零越界措辞。
```

**发布 runbook**（收口报告通过后执行）：

1. `Cargo.toml` version 按语义化升 **MINOR**（M-B 新功能非破坏，AGENTS 3.2），并与 CHANGELOG 同步；
2. CHANGELOG `[Unreleased]` 标题改为版本号 + 日期（内容按规格书 §14 草案定稿）；
3. 全门禁三绿 + e2e 60/60 后打 tag `v<新版本>`（先例 `v0.7.4`，tag 打在 merge commit 上）；
4. 五端推送对齐（沿 M-A PR #20→#27 同法：合并后同步各远端与平台仓）；
5. STATUS / milestones / task-breakdown 的 M-B 行标 ✅ + 收口报告链接；dependency-graph 的 M-B 节点标完成。

---

## 27. 变更记录

- 2026-09-28：规划会话依 M-A 收口后的 tip `1af09fd` 实测撰写；用户对嵌入路线（A：先修对 TF-IDF）与凭证（只测免费链）两项决策已记入 §0。现状事实由只读取证 + 规划会话亲自复核（`check_oa_analysis` 接线点 `routes/ai.rs:1433`、三处字节切片 `vector/mod.rs:32`/`rag/chunker.rs:62`/`routes/search.rs:859`、`enrich-free` 免费爬取 `routes/patent.rs:221`）逐条验证后写入。
- 2026-09-28（同日第二轮，规划会话）：**MB1–MB5 逐包扩写到 MB0 同等粒度**（现状锚点 / 范围 / 不做 / 必做取证 / 验收 / 门禁），并新增 §1.5（MB5 现状：`compress_history`、双套摘要、OA 容量缺口、`ai.rs:1699` 字节切片 panic）、§1.6（**14 条现状校正 a–n**，其中 f/g/h/l 四条会改变施工面）、§2.1（红线例外预授权表）、§3.1（基线 721 → MB0 后 744、MB3 后 +1 的对账口径）、§3.2（风险：5.9GB 余量、5m48s 冷编、standalone crate rustc 崩溃、GBK 链接器告警）、§3.4（5 个待用户裁定的决策门）、§4.5–4.10（新发现 6 项登记不派包）。取证由只读子代理并行完成，MB0 未合并的工作区改动已排除在外。**本轮规划会话未修改任何产品代码**（`src/` 下 MB0 的 5 个文件改动属上一轮执行棒遗留，保持原样）。
- 2026-09-28（同日第三轮，规划会话，追加可抄层）：新增 **§5 跨包结构化出参契约**（键/生产者/消费者对照表 + `reason_code` 统一枚举 `cooldown|blocked|timeout|not_found|parse_empty|no_fulltext|db_error|already_enriched`，禁各包自造）、**§6 逐包用例清单**（MB2 6 / MB3 11 / MB4 6 + 前端四项 / MB1 9 条·诱导编造双路径参数化 ≥16 / MB5 7 条测试名与断言点，全部离线、`:memory:` 或临时实例，含每条的红→绿锚归属）、**§7 反假绿清单**（逐包「看起来过了」的蒙混路径与审计识破手段，其中 MB3「≥5 篇引用被做成同一片专利的 5 个切片」、MB1「`not_applicable` 当通过」、MB5「压缩根本没触发就断言约束仍在」三条为本轮新立的硬判据）、**§8 审计复跑命令卡**（含 `df` 预检、全量落盘统计、逐文件红线 diff、MB4 额外四项、本机冷编时间预算）、**§9 对外口径边界**（六项能力「可承诺 / 禁止承诺 / 依据」对照，含 CHANGELOG 与 i18n 措辞纪律）。原 §5 变更记录顺延为 §10。**仍未改动任何产品代码**；`docs/feedback.md` 已登记「规划会话只写文档」分工边界，`docs/errors.md` 已代记仓外 crate rustc 崩溃与 GBK 链接器告警两条踩坑。
- 2026-09-28（同日第四轮，规划会话，追加派单执行层）：新增 **§10 派单卡**（§10.1 统一 PR body 六段模板——包号与范围 / 红→绿取证原文 / 门禁全量统计 / 红线自检 / 偏离登记 / 验收逐条对照，缺段打回；§10.2 逐包派单卡——分支名、可提交白名单、禁止事项、DoD 勾选、MB0 收口卡含 `tests/mb0_red_proof_tmp.rs` 的「贴原文后删除」处置规则）、**§11 用户视角走查剧本**（12 步无 Key 走查，每步标注「不显示该现象⇒哪一包没做成」）、**§12 收口回写责任表**（施工完成 / 审计通过 / 合并 / 回写四态各有唯一责任人，CHANGELOG 当日入账不留收口回忆式补写）、**§13 反驳与仲裁流程**（证据阈值、三类必须升级用户的判定线、五条不可协商项，禁止假服从）、**§14 CHANGELOG `[Unreleased]` 草案**（措辞受 §9 约束，执行棒合并当天搬运）。同批把 §6 各包标题的计数改到与用例表逐行一致（MB3 11 / MB4 6 / MB1 9 条·参数化 ≥16），消除「标题 ≥N 表里 M 行」的可钻空子。**本节自 §16 重排为 §15**（上一轮追加时 §10 锚点被整段替换未重排号，本轮审计自查发现后统一顺延，正文交叉引用 §10.1/§10.2/§13/§14 已同批改齐）。仍未改动任何产品代码。
- 2026-10-01（规划会话，追加执行层与收口层，用户指示「先不管杂碎事，继续写」）：新增 **§15 M-B 施工看板**（六态状态枚举 + 逐包快照，唯一进度事实源，STATUS 只记流水）、**§16 审计作业流程**（收到 PR 后固定 9 步，每步带通过判据，任一步不过即停出结论，结论四态：通过/有条件通过/打回/升级用户）、**§17 回归风险矩阵**（每包「不许弄坏」的既有资产具名到测试，全部 2026-10-01 现网复核：`inner_url_emits_spec_2_param_set`、幂等早退 `routes/patent.rs:238-240`、两把 attempts 形状锁、`fts_search_finds_matching_patent`、`mb0_wiring_tests` 三条（`db/patent.rs:940/:966/:983`）、向量双门控（`routes/search.rs:2077/:2128`）、e2e `expectedPasses=60`（`e2e_test.mjs:5`）、`fact_check.rs` 15 条、`truncate_for_ai` 五条 + OA 容量三条（`src/ai/tests.rs:29-95`）、`functions-manifest` 基线）、**§18 MB4 文案预审稿**（8 个 i18n 键 zh/en 草案，措辞受 §9 约束，`noEvidence` 是替代渲染不是删段落，stance 枚举与 §5.2 逐字对应）、**§19 术语表 + 接棒首查清单**（9 个术语定义 + 接棒 10 分钟 5 步，给首次进仓的执行棒 AI）、**§20 决策门裁定记录表**（门1–门5 占位 ⏳，裁定原文照录禁止 AI 转述，裁定后取代 §3.4 为事实源）、**§21 M-C 消费面预告**（MC1/MC2 将消费的 M-B 产物与「函数级可复用、禁止埋死在 idea handler」约束，收口时做假想接线走读）。**本节自 §15 重排为 §22**。写前锚点均经实测核实（测试名 grep、`e2e_test.mjs:5`、幂等早退原文），未改动任何产品代码；决策门仍全部待用户统一裁定。
- 2026-10-01（同日第二轮，规划会话，用户指示「继续写」）：新增 **§22 执行棒派单提示词**（6 份可直接复制粘贴的开工指令——MB0 收口棒「只做提交收口禁止重写」、MB2/MB3 常规棒、MB4/MB1/MB5 带决策门前置行「门未裁定贴了也不开工」，共用纪律骨架：必读顺序 / 分支名 / 红线 / §10.1 六段交付 / §13 反驳）、**§23 审计评论四态模板**（通过/有条件通过/打回/升级用户的可粘贴骨架，禁止散文式审计评论）、**§24 在线取证操作规程**（临时空库实例纪律、证据落盘仓外、GBK curl 坑与 XHR 响应结构、反爬停手规则、免费链边界与通道级证据的诚实标注）、**§25 固定中文测试语料**（5 个确定性 fixtures：标题/纯中文正文/中英混排+emoji/6200 字生成规则/OA 样例，各包内联同字节串，改字节走 §13）、**§26 M-B 收口报告模板 + 发布 runbook**（六段收口报告骨架 + MINOR 升版/tag `v<新版本>` 沿 `v0.7.4` 先例/五端对齐/回写清单）。插入时一度吞掉「§22 变更记录」锚点造成 §21→§23 号断，自查发现后整体下移一号：**本节现编 §27**。tag 格式与「五端」惯例经 `git tag` 与 M-A 文档核实（`v0.7.0`–`v0.7.4`；PR #20→#27 merge 后同步法）。未改动任何产品代码。
- 2026-10-01（同日第三轮，规划会话，回写 + 自查修号）：① **MB0 状态回写**——收口棒已按 §10.2 / §22.1 提交代码 `f66912c`（+12 测试），PR **#28**（远端 head 仅 `f66912c`；docs 提交 `667064a` 未推入 #28，内容改随 PR #29 入库）OPEN、远端 CI `test`/`lint`/`e2e` 三项 SUCCESS、`mergeable=MERGEABLE`，取证件未入库；§15 看板 MB0 行由「施工中（未提交）」改「**待审计**」，§10.2 收口卡与 §22.1 提示词加 ✅ 已执行完毕标记转留档，下一份可派发提示词为 §22.2（前置 = #28 审计合并）。② **审计自查修三处**：§22 的六个子节标题沿用上轮误号 `23.1–23.6`（与真正的 §23 撞号）⇒ 改 `22.1–22.6`；§15 看板 `MB0b` 行用了枚举外的「未立项」⇒ 改「未开工（未立项，等门5）」；`docs/errors.md` 里「补充实测（MB0 会话）」一条被重复粘贴成两行 ⇒ 删重。③ **紧急包 OA-U 下发**（用户真实复审任务驱动）：新文件 `docs/plan/2026-10-01-oa-reexam-urgent-spec.md`，UA1–UA6 六项；用户同日拍板三条交付硬标准（专家级流程 / 零幻觉 / 可直接提交终稿）已软件化为 **UA5 六角度（含⑥预判合议组质疑并预先回应）**、**UA6 出处锚点纪律（逐断言【依据：…】+ 文末出处对照表 + 无出处断言禁写）**、**UA2 终稿形态（可直接抄入官方理由栏 + 文末固定「提交前清单」段）**，并改写 §0.4 口径纪律（禁止「初稿供人工改写」类降级表述）。未改动任何产品代码。
- 2026-10-01（同日第四轮，规划会话，docs 下发 = PR #29）：本轮仍**未改任何产品代码**。① §10–§27 可抄层 + OA-U 紧急包（UA1–UA6）随分支 `docs/oa-u-dispatch` 提交 `cea4722` 下发：GitHub **PR #29**、Gitee 同名分支同步。该分支基于 `exec/mb0-embedder`，故 **#29 必须等 #28 合并后再合**（否则等于把未过审计的 MB0 代码带进 main），PR body 第 4 段已写明。② **推送通道踩坑代记**（`docs/errors.md` 新条）：`github.com` 的 git-over-HTTPS 连续 `Connection was reset` / 443 拒连，而 `api.github.com`（gh REST）与 `gitee.com` 同时可达 ⇒ 三条通道相互独立，处置顺序是「重试 → 换远端 → 最后才走 REST 造 blob/tree/commit」；REST 重放会改 commit SHA，本次即因「本地比远端多一个未推的 docs 提交」撞到 `git/commits/667064a` 404。③ **精度回写（审计自查）**：查证 **PR #28 远端 head 只有 `f66912c`**，此前六处文档把未推送的 docs 提交 `667064a` 写成「已入 #28」，已逐处改为「远端 head 仅 `f66912c`，`667064a` 内容随 #29 入库」（§10.2 收口卡 / §15 看板 / §22.1 留档行 / §27 本表 / OA-U §0.1 / STATUS / task-breakdown / errors）。
