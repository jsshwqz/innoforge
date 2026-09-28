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
- `patents_embedding`（迁移 v20）的写入函数 `db::save_patent_embedding`（`db/vector.rs:6`）唯一调用方是 `vector/mod.rs:166` 的 `compute_and_save_embedding`，而后者**全仓零调用** ⇒ 生产库该表恒 0 行。
- `/api/search/vector`（`common.rs:239` 注册）全表扫 blob 算 cosine（`routes/search.rs:771-799`），且 `count_embeddings()`（`:767`）**>0 才启用向量档**，0 行时静默只回 BM25 RRF 并如实带 `vector_count: 0`（`:847`）。
- 现「向量」是把 term 分数排序后填充 512 维、**丢弃 term→维度映射**（`vector/mod.rs:77` 自注 "no IDF without corpus"，`:81-91`）⇒ 查询向量与文档向量不在同一可比空间，相似度数值不构成语义相关性。**这是已知局限，本轮不修，但必须写明。**

### 1.3 RAG（MB3 的对象，彻底未接线）

- `src/rag/` 四文件（`mod.rs` `build_chunks_from_patent` / `rag_search`；`chunker.rs`；`retriever.rs` `retrieve_chunks` + `retrieve_chunks_by_keyword`（空占位）；`assembler.rs`）。`patent_chunks`（迁移 v21）的写入口 `db::save_patent_chunks`（`db/rag.rs:17`）**全仓零调用点**；`rag::` 在 `src/rag` 之外零引用（仅 `lib.rs:20` 声明）；`build_router` 无任何 rag 路由。
- 深度分析喂给 AI 的内容**不含专利全文**：`pipeline/steps/analysis.rs:37 deep_analysis_simple`（拼装 `:77-113`）与 `pipeline/steps/deep_reasoning.rs:126 build_user_context`（`:164-179`）只喂用户创意的 title/description + `top_matches` 的摘要（snippet 源自 `pipeline/steps/search.rs:164` 的 `p.abstract_text`）。
- 两处截断**违反 AGENTS.md §2.5 的数据完整性纪律**：`analysis.rs:50` 与 `deep_reasoning.rs:138` 用 `chars().take(150/120)`，不走 `ai::client::truncate_for_ai`（`:267`，带完整性提示的既有工具）。

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

- **范围**：① 三份 n-gram/TF 拷贝收敛为**单一出处**（建议留在 `src/vector/mod.rs`，另两处改为调用它；删除重复实现而不是加 `#[allow(dead_code)]`）；② 字节切片全部改字符边界安全（`Vec<char>` 窗口或 `char_indices`）；③ `compute_and_save_embedding` 接上真实调用点——**只接「新入库顺手算」**（挂在 `db/patent.rs::insert_patent` 之后的单一写入口侧），**禁止**在本包做全表批量回填（4.3GB 用户库，风险与耗时都不可控）；④ `/api/search/vector` 的查询向量与写入端**同函数同实现**（不留第二套标准）；⑤ 更正 `vector/mod.rs:77` 一带注释与规格书：写清「当前向量丢弃 term 映射 ⇒ 相似度不构成语义能力，本轮有意不修」。
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

- **现状锚点**：`rag/mod.rs:16-18` 常量 `CHUNK_SIZE=800`/`CHUNK_OVERLAP=100`/`TOP_K=3`；`:21 build_chunks_from_patent(patent_id, _title, abstract_text, claims, description) -> Vec<PatentChunk>`（三段循环 abstract `:32` / claim `:48` / description `:64`，chunk id 形如 `{patent_id}-{index}`，`_title` 未用）；`:83 async rag_search(&AiClient,&Database,patent_id,query,system_prompt,max_tokens) -> Result<RagResult,String>`，`RagResult(:150)` = answer/citations/chunk_count/rag_enabled/rag_failed，AI 失败降级 `:111`；`chunker.rs:4 chunk_text(text,chunk_size,overlap)`（**按字节 `text[start..end]`**，边界回退找 `。/；/./\n`）、`:46 chunk_summary`、`:59 compute_chunk_embedding` 已委托 MB0 单一出处；`retriever.rs:8 retrieve_chunks(db,patent_id,query,top_k)`（走 `db.search_chunks`+`get_chunk`，错误 `Ok(vec![]) :20`）、`:49-56 retrieve_chunks_by_keyword` 参数全带 `_` 前缀、函数体只有 `Ok(vec![])`、注释 "placeholder for future use"、**连 TODO 都没有**；`assembler.rs:6 assemble_rag_prompt(query,&[ReferenceChunk],system_prompt,max_tokens)`（每条 `### [引用 N]（来源: X, 相似度: 0.xx）` `:40/:52`，剩余预算 <100 早停 `:33`）、`:86 build_citations`（取前 5、预览 50 字，孤儿函数）。表：`patent_chunks` 迁移 **v21**（`db/migrations.rs:529-551`；现最新 v23），列 `id/patent_id/chunk_index/source_type CHECK('abstract','claim','description')/content/embedding/model_name DEFAULT 'char-tfidf-v1'/created_at` + FK CASCADE，索引 `idx_chunk_patent`/`idx_chunk_source`；`db/rag.rs:17 save_patent_chunks(&self,patent_id,&[PatentChunk],model_name)`（先 DELETE 再事务 INSERT，blob 为 f32 LE）、`:96 get_chunk`、`:137 count_chunks`、`:149` 私有 `cosine_similarity`。引用类型 `types/search.rs:127 ReferenceChunk{id,patent_id,chunk_index,source_type,content,relevance_score}`（经 `pipeline/context.rs:29` re-export）。挂点候选：`pipeline/state.rs:6-24` 16 步，`ScoreNovelty(9)` 为 critical 且 quick 模式不跳（`is_critical :108`、`skipped_in_quick_mode :116-129`），派发在 `orchestrator/engine.rs:284-333`（`:309` ScoreNovelty、`:310` AiDeepAnalysis）；`PipelineContext` 定义 `pipeline/context.rs:228-316`，**现成字段无一可承载切片**（`evidence_chain :279`、`memory_entries :283`、`agent_outputs :295` 语义都不对）。检索复用：`routes/search.rs:755 vector_hybrid_search_json`（BM25 `:764`、向量 `:778-819`、cosine `:807`、RRF `:822-836`）、`vector/mod.rs:118 cosine_similarity`/`:133 search`/`:184 compute_and_save_embedding`/`:198 rrf_fuse`、`db/patent.rs:547 search_fts`（`:565 bm25(...)` 权重表）；`patents_fts` **无触发器**，同步只发生在 `db/patent.rs:35/:44` ⇒ 任何绕过 `insert_patent` 的全文 UPDATE 都会让 FTS 漂移。
- **前置缺陷（必须先修，否则接线即运行错误，见 §1.6 f）**：`db/rag.rs:55 search_chunks` 的 SELECT 未取 `embedding` 却在 `:66 row.get(2)` 读第三列；`PatentChunk`（`db/rag.rs:4-11`）缺 `model_name`/`created_at`，与 v21 表列不对齐。修法是补 SELECT 列或改 row 索引 + struct 字段补齐，**禁止动迁移**（表已存在，加列即破红线）。
- **范围**：
  ① 写入：`db/patent.rs::insert_patent` 收尾（MB0 embedding 顺手算之后、`Ok(final_id)` 之前——守卫已 `drop`，可再取锁）调 `build_chunks_from_patent` + `save_patent_chunks`；失败静默降级只 `warn`；**禁止全表批量回填**（与 MB0 同纪律，用户库 4.3GB）。富化路径更新全文也会走到这里（`routes/patent.rs:314`/`:198` 均调 `insert_patent`），**无需另设挂点**；`save_patent_chunks` 的「先 DELETE 再建块」天然幂等。
  ② 引用可反查（§1.6 h）：给 `RankedMatch`（`pipeline/context.rs:57-66`）加 `#[serde(default)] patent_id: Option<String>`，本地命中从 `steps/search.rs:162` 的 `patent_local_{patent_number}` 反解或直接取库 id；在线/SerpAPI 命中（`steps/search.rs:117/:208`）无库 id 时如实留 `None`，**禁止伪造**。新增字段若进对外 JSON，必须 `skip_serializing_if` 或保证既有键集合不变（沿 MB0「出参 7 键形状锁定」单测的做法）。
  ③ 检索 + 组装：在 `steps/scoring.rs:13 execute`（critical、quick 不跳、此时 `ctx.top_matches` 已就绪）取 top-N 专利切片，塞进 `PipelineContext` 新字段 `#[serde(default)] rag_chunks: Vec<ReferenceChunk>`；prompt 组装**必须走 `assembler::assemble_rag_prompt`（`assembler.rs:6`）**，相似度**必须复用 `vector::VectorIndex::cosine_similarity`（`vector/mod.rs:118`）**——禁止在 rag 侧或 pipeline 侧新写第二套拼装/打分（`db/rag.rs:149` 那份私有 cosine 本包一并收敛或注明为何保留）。不想动 scoring 的替代挂点是 `engine.rs:310` 分支前取切片，**二选一写进 PR body，禁止两处都接**。
  ④ 空占位与孤儿销账：`retriever::retrieve_chunks_by_keyword`（`:49-56`）要么真实现要么删除并连带删调用点，**禁止留装饰性空函数**；`assembler::build_citations :86` 与 `mod.rs:133 build_fallback_citations` 二者取一（建议 assembler 转正、fallback 退役），禁止留孤儿。
  ⑤ 截断同批改：`analysis.rs:49-50`（`snippet.len() > 150` 后 `chars().take(150)`，注意 `len()` 是字节）与 `deep_reasoning.rs:137-138`（120）→ `ai::client::truncate_for_ai`（`:267`，超限自动追加「原文共 N 字符…禁止推测补全」提示 `:273-276`）。**两处同改**，但按 §1.6 e，`analysis.rs:37 deep_analysis_simple` 是零调用点死码，验收只以 `deep_reasoning` 侧为准；死码**本包禁止顺手删除**（删了改变门禁计数），登记进 §4 另议。
  ⑥ 无全文降级：取不到切片时退回摘要档并在上下文如实标注「未取到全文的原因」（沿用 MB2 的 `reason_code`），**禁止把「没取到」写成「没有相关内容」**。
  ⑦ 红线破口预告：`src/main.rs` 加 `pub mod rag;`（§1.6 g + §2.1 授权，仅限模块声明行）。
- **必做取证**：红→绿锚 = 接线前深度模式 prompt 字符串断言**不含**任何 `### [引用 N]` 切片段，接线后含 ≥5 段且逐条能 `db.get_chunk(id)` 反查到 `patent_chunks` 行（`:memory:` 实例即可）；切片端中文 panic 回归（`chunk_text` 仍是字节切法 + 边界回退，须跑纯中文/中英混排/emoji 与「越界起点」循环用例，证明本包没引入 MB0 同类缺陷）；`count_chunks` 接线前后对账（0 → >0）。
- **验收（无 Key 环境）**：① 深度模式报告上下文出现 ≥5 篇专利全文片段引用，每条引用能反查到 `patent_chunks` 的行且带专利号 + 段/权号；② 一条专利入库 ⇒ 同时产生 embedding **和** chunks（两档都在单一写入口顺手算）；③ 空占位/孤儿函数 grep 为零；④ rag 侧无第二套 cosine/拼装实现；⑤ `analysis.rs` + `deep_reasoning.rs` 两处 `chars().take` 全部消失（grep 断言）。其余 `chars().take(300/500/2000)`（`analysis.rs:156/:180/:193/:250`）**本包不动**，登记进 MB5 改造面。
- **门禁**：fmt / clippy / `cargo test`；**基线跳变须逐条归因**：加了 `mod rag` 后，`rag/` 既有单测（含 MB0 那条委托用例）会在 bin 侧现身 ⇒ 计数出现「非新增测试导致的 +N」，按 §3.1 口径写明，**禁止为凑基线增删测试**。templates/static 零改动 ⇒ e2e / HTML 扫描不适用。

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

## 5. 变更记录

- 2026-09-28：规划会话依 M-A 收口后的 tip `1af09fd` 实测撰写；用户对嵌入路线（A：先修对 TF-IDF）与凭证（只测免费链）两项决策已记入 §0。现状事实由只读取证 + 规划会话亲自复核（`check_oa_analysis` 接线点 `routes/ai.rs:1433`、三处字节切片 `vector/mod.rs:32`/`rag/chunker.rs:62`/`routes/search.rs:859`、`enrich-free` 免费爬取 `routes/patent.rs:221`）逐条验证后写入。
- 2026-09-28（同日第二轮，规划会话）：**MB1–MB5 逐包扩写到 MB0 同等粒度**（现状锚点 / 范围 / 不做 / 必做取证 / 验收 / 门禁），并新增 §1.5（MB5 现状：`compress_history`、双套摘要、OA 容量缺口、`ai.rs:1699` 字节切片 panic）、§1.6（**14 条现状校正 a–n**，其中 f/g/h/l 四条会改变施工面）、§2.1（红线例外预授权表）、§3.1（基线 721 → MB0 后 744、MB3 后 +1 的对账口径）、§3.2（风险：5.9GB 余量、5m48s 冷编、standalone crate rustc 崩溃、GBK 链接器告警）、§3.4（5 个待用户裁定的决策门）、§4.5–4.10（新发现 6 项登记不派包）。取证由只读子代理并行完成，MB0 未合并的工作区改动已排除在外。**本轮规划会话未修改任何产品代码**（`src/` 下 MB0 的 5 个文件改动属上一轮执行棒遗留，保持原样）。
