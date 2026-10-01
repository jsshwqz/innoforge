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

---

## 2. 包切分（每包 = 一棒 = 一次门禁，串行派，禁止同仓并行）

> 统一红线（每包都适用，逐文件 `git diff main..<branch> -- <path>` 必须为 0）：`src/common.rs`、`Cargo.toml`、`Cargo.lock`（**M-B 全程禁止新 crate 依赖**）、`src/db/migrations.rs`（M-B 前 4 包**不动 schema**，`patent_chunks`/`patents_embedding` 表已存在）、`src/main.rs`、`src/lib.rs`、`AGENTS.md`、`templates/`、`static/`、`e2e_test.mjs`、`check_html_functions.mjs`、`docs/functions-manifest.json`；生产路径零 `unwrap()/expect()`；禁止读写用户库 `D:\test\patent-hub-backup\innoforge.db`（取证一律在 `D:\Temp\*-probe\` 的临时空库实例）；禁止仓库根目录临时文件；`gh` 写操作一律 `env -u GITHUB_TOKEN` 前缀；门禁输出全量落盘后统计，**禁止 `| head`**。

### MB0 · Embedder 归一 + char-boundary 安全 + 写入链接通（第一棒，无前置）

- **范围**：① 三份 n-gram/TF 拷贝收敛为**单一出处**（建议留在 `src/vector/mod.rs`，另两处改为调用它；删除重复实现而不是加 `#[allow(dead_code)]`）；② 字节切片全部改字符边界安全（`Vec<char>` 窗口或 `char_indices`）；③ `compute_and_save_embedding` 接上真实调用点——**只接「新入库顺手算」**（挂在 `db/patent.rs::insert_patent` 之后的单一写入口侧），**禁止**在本包做全表批量回填（4.3GB 用户库，风险与耗时都不可控）；
  - ⚠️ **架构提示**：`compute_and_save_embedding` 签名需要 `&VectorIndex`，而 `insert_patent`（`db/patent.rs:7`）只有 `&self`（Database），`AppState`（`routes/mod.rs:385`）也不持有 `VectorIndex`。执行棒须在以下方案中选一并落注释：(a) 在调用方（如 `routes/patent.rs` / `search/providers/serpapi.rs`）于 `insert_patent` 返回后用默认参数构造 `VectorIndex` 再调 `compute_and_save_embedding`——**推荐**，不改 `insert_patent` 签名、不碰红线文件；(b) 在 `insert_patent` 内部内联嵌入计算——需让 `db/patent.rs` 依赖 `vector` 模块，且 tokenizer 参数硬编码在 DB 层。无论选哪个，embedding 失败**不得回滚**专利入库（embedding 是附带优化，不是入库前提），失败只 `tracing::warn!` 记账；④ `/api/search/vector` 的查询向量与写入端**同函数同实现**（不留第二套标准）；⑤ 更正 `vector/mod.rs:77` 一带注释与规格书：写清「当前向量丢弃 term 映射 ⇒ 相似度不构成语义能力，本轮有意不修」。
- **必做取证**：中文 panic 回归用例（纯中文 3 字节 / 中英混排 / emoji / 对 `0..len` 全起点切片的循环用例），**先在旧实现上跑红、新实现跑绿**，把红→绿证据写进 PR body（这是本包唯一的「证明我改对了」的锚）。
- **验收**：临时空库实例入库一条中文专利 → `count_embeddings()` 由 0 变 ≥1、`/api/search/vector` 的 `vector_count` 如实翻转（此前恒 0）；同一中文长文本反复 tokenize 零 panic；`routes/search.rs:852` 与 `rag/chunker.rs:57` 两处拷贝消失。
- **门禁**：fmt / `clippy --all-targets -D warnings` / `cargo test`（基线 721，本包新增 N 条 ⇒ 721 + 2N 对账）；templates/static 零改动 ⇒ e2e 与 HTML 扫描按 DoD 不适用。

### MB2 · 免费全文供给链式化 + 冷却表共用（第二棒，无 schema 变更）

- **范围**：让「分析前自动有全文」在**无付费 Key** 下可达且不加剧反爬：① 抽出一个可复用的富化入口（内部复用 `api_enrich_patent_free` 的抓取+解析逻辑，不复制第二份 HTML 解析）；② 富化请求**必须共用 MA6a 的 `breaker::global_table()` 判据**（同表同函数，禁止旁路），命中冷却则**直接降级为摘要档**并如实说明「未拿到全文的原因」；③ 每次分析最多富化 top-N 篇（N 参数化、默认 5，**禁止无界循环里逐条出网**）；④ 响应侧新增结构化字段列出「哪些篇有全文 / 哪些没有 / 为什么没有」（沿 MA6b 的纪律：**只发结构化键，不发文案让前端猜**）。
- **不做**：不改 SerpAPI 付费路径的行为；不引入新源；不动 `patents` 表列。
- **验收**：临时实例（无 Key）里，对 M-A 真实在线命中入库的中文专利调用富化 → `description/claims` 由空变非空（或如实返回反爬阻断且冷却表被正确写入/读取）；冷却期内二次调用**零出网**（用请求计数或日志证明）；断言富化结果不被截断（AGENTS.md §2.5）。

### MB3 · RAG 接线（切片→检索→组装→引用）（第三棒，依赖 MB0+MB2）

- **范围**：把已有但未接线的 `src/rag/` 真正挂上：① 入库/富化时写 `patent_chunks`（单一写入口侧，与 MB0 同一挂点纪律）；② 深度分析前自动取 top-N 专利的全文切片进 prompt，每条切片带**可回溯引用编号**（专利号 + 段/权号）；③ **全部八处** `chars().take` 截断改走 `truncate_for_ai`（`analysis.rs:50/156/180/193/250`、`deep_reasoning.rs:138`、`oa_response.rs:137`、`claim_tree.rs:24`），**八处同批改**（少改一处则该处仍违规）；④ 无全文时降级为摘要档并在上下文里如实标注（不得静默把「没取到」变成「没有相关内容」）。
- **验收（无 Key 环境）**：一份真实中文专利库状态下，深度模式报告上下文里出现 **≥5 篇专利全文片段引用**，每条引用能反查到 `patent_chunks` 的行；`retriever::retrieve_chunks_by_keyword` 的空占位要么实现要么删除并销账（**禁止留装饰性空函数**）。

### MB4 · 出处标注体系（第四棒，依赖 MB3 的引用编号）

- **范围**：事实性结论强制附源（专利号 + 段/权号），无源标【推测】；结构化承载沿用 `types/idea.rs` 的 `Evidence`，**不再往纯字符串 `ctx.ai_analysis` 上贴补丁**；报告末尾新增「决策建议段」（申请/放弃/转向 + ≥3 条理由，理由引用报告内已有证据编号）。
- **验收**：三份真实创意报告抽查 10 条结论 ≥9 条有源；决策段存在且理由编号可反查。

### MB1 · 幻觉防线扩面（可插入棒，无前置）

- **范围**：`check_oa_analysis` 的等价能力接入创意分析 pipeline（当前 pipeline / `routes/idea.rs` 零调用）；建「诱导编造法条 / 页码 / 引用」用例集；处置策略默认**标注**，致命档（扣分 ≥35）升级为**拒绝并给可读原因**（对齐 AGENTS.md 的失败降级要求）；顺手更正 `ai/fact_check.rs:14` 过时注释。
- **验收**：诱导用例集全部被标记或拒绝；OA 与创意两条路径共用同一核查实现（不留第二份判定）。

### MB5 · 上下文治理（收口棒，可最后）

- **范围**：关键事实（专利号 / 日期 / claims）不进压缩摘要路径（走结构化直传）；OA 容量策略统一为「报错不截断」（核实既有 `oa_capacity_error` 覆盖面）；20 轮对话后首轮约束仍生效的回归用例。
- **验收**：20 轮对话后首轮约束仍生效——须有可跑的回归用例或真实实例取证，二者缺一不得标完成。

---

## 3. 排期与依赖

```
MB0（embedder 安全 + 写入链） → MB2（免费全文 + 冷却共用） → MB3（RAG 接线） → MB4（出处 + 决策段）
                              ↘ MB1（幻觉防线扩面，独立，可在 MB0 之后任一空档插入）
MB5（上下文治理）：M-B 收口棒
```

- **禁止并行开两棒**：同仓 `target/` 共享会损坏 `incremental/`（已记 `docs/errors.md` 2026-09-28），且 D 盘余量长期 4-7GB，两个构建同时跑必爆盘。清盘只允许删 `target/debug/incremental` 与 `target/debug/deps/*.pdb`，**禁止 `cargo clean`**。
- 每棒交付：`exec/<包名>` 分支 → PR → 规划会话只读审计 + 独立复跑门禁 → GitHub 原生 merge（`env -u GITHUB_TOKEN gh pr merge`）→ 五端推送对齐 → 回写 STATUS / task-breakdown / MASTER §6。

## 4. M-A 遗留的开放项（不在 M-B 包内，登记不派包）

1. 英文机构名 assignee 形态仍受上游反爬阻断，未取证；
2. SerpAPI / EPO 真实凭证冒烟 + 配额预警真实数据触发（等用户配 Key，见 §0 决策 B）；
3. OS 级断网注入取证（现以通道级证据替代，已如实入文）；
4. MA6「错误信息再润色」（体验打磨）。

## 5. 变更记录

- 2026-09-28：规划会话依 M-A 收口后的 tip `1af09fd` 实测撰写；用户对嵌入路线（A：先修对 TF-IDF）与凭证（只测免费链）两项决策已记入 §0。现状事实由只读取证 + 规划会话亲自复核（`check_oa_analysis` 接线点 `routes/ai.rs:1433`、三处字节切片 `vector/mod.rs:32`/`rag/chunker.rs:62`/`routes/search.rs:859`、`enrich-free` 免费爬取 `routes/patent.rs:221`）逐条验证后写入。
