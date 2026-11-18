# 创研台 InnoForge 全新一代架构与全谱系功能重构详细设计总书（多 Agent 可直接施工版）

- **文档编号**：`docs/plans/2026-11-18-v1-overhaul-plan.md`
- **配套宪章**：`docs/plans/product-vision.md`（六大永久业务公理与 §3 验收口径）、根目录 `AGENTS.md` 与 `CLAUDE.md`
- **适用仓库**：[jsshwqz/innoforge](https://github.com/jsshwqz/innoforge)（演进起点 `v0.9.46`，本机路径 `D:\test\aionui\innoforge`）
- **目标版本定级**：**直接定标 `v1.0.0`**（跨越 `v0.10`，按 `0.9.46 → 1.0.0-alpha.1 [Phase 1-2] → 1.0.0-beta.1 [Phase 3-4] → 1.0.0-rc.1 → 1.0.0 [Phase 5 全门禁封版]` 演进）
- **文档定位**：**纯架构、数据模型、接口契约、交互规格与多 Agent 原子任务拆解总书（零代码片段）**。任何开发 Agent 读取本文档与 `product-vision.md` 后，无需向用户重复询问业务背景或设计细节，即可按第六章任务编号直接认领施工。

## 第一章：项目规则合规衔接与 12 项存量问题清零台账

### 1.1 规约换挡与全程强制工程红线
1. **到期整顿令结项换挡**：原 [2026-10-27 整顿令](https://github.com/jsshwqz/innoforge/blob/main/docs/plans/2026-10-27-rectification.md) 及 [Issue #90](https://github.com/jsshwqz/innoforge/issues/90) 冻结期（至 2026-11-17）已届满收官。由任务 `T-101` 正式归档旧令，将根目录 [AGENTS.md](https://github.com/jsshwqz/innoforge/blob/main/AGENTS.md)、[CLAUDE.md](https://github.com/jsshwqz/innoforge/blob/main/CLAUDE.md) 与静态守卫测试切换至守护 `docs/plans/product-vision.md` 的**六大永久公理**（① 表层每张卡片独立自治；② 自有存量成果与新产品实验并行；③ 专利+期刊论文+开源+标准四维检索；④ 多轮对话+图文文件复合共创；⑤ 已有专利 OA + 新专利预审 + 论文期刊审稿回复三栖抗辩；⑥ 产品/报告/论文/专利一源四出与先专利后论文公开时序守卫）。
2. **七条不可触碰的施工红线**：
   - **隐私红线**：在办案件、未发表论文手稿、涉密图纸与原始实验数据严禁进入 Git、测试夹具、Issue 或 CI 日志。
   - **单机零配置红线**：保持 Rust + Axum + 内嵌 SQLite 单可执行文件形态，离线可查本地库。
   - **前端零打包工具红线**：严禁引入 Webpack、Vite、React 等构建工具；仅使用原生 HTML/CSS、原生 ES Modules 与浏览器原生 Custom Elements；全部用户可见文案经 `static/i18n.js` 保持中英双语键值 100% 齐平；严禁未清洗 `innerHTML`。
   - **Rust 安全红线**：生产代码零 `unwrap` / `expect`、零 UTF-8 裸字节切片（须用字符边界安全方法，过 `no_unsafe_byte_slice` 守卫）、零循环逐条查库（N+1）、零异步运行时同步阻塞。
   - **确定性核验红线**：AI 单次调用 60 秒超时上限、9 服务商自动容灾、用户输入 `<user_input>` 隔离；专利锚点、附图部件、论文真实 DOI 及实验数值必须经确定性逻辑 O(1) 回查，核查未过必降级阻断（严禁仅打警告放行）。
   - **卡片垂直隔离红线**：`src/cards/*` 各卡片目录之间严禁直接相互引用，仅允许依赖 `src/core/*` 并通过本机金库数据表共享资产。
   - **契约与发版红线**：路由统一挂载并与 API 契约文档保持零漂移；发版必过全套静态检查与 7 张卡片逐按钮动态端到端实测（[CONTRIBUTING.md](https://github.com/jsshwqz/innoforge/blob/main/CONTRIBUTING.md)）。

### 1.2 存量错误、安全漏洞与技术债清零明细表（12 项全覆盖）

| 编号 | 类别 | 现存具体问题与定位 | 根治验收标准（对应施工任务号） |
| :--- | :--- | :--- | :--- |
| **ERR-01** | 安全高危 | `src/main.rs` 默认绑定 `0.0.0.0:3000` 且零访问鉴权，局域网暴露本地库与 9 家 AI 密钥 | 桌面端默认改绑 `127.0.0.1:3000`；非回环绑定强制开启同源 `HttpOnly` Cookie / Bearer 令牌鉴权；设置接口密钥掩码化（`T-103`） |
| **ERR-02** | 质量漏放 | `src/pipeline/steps/finalize.rs:30-32` 中 `fact_check_rejected` 仅打 `warn` 不阻断幻觉 | 接入 `core/evidence_rebuttal/quality_gate`：存在无锚点断言或假引证时强制降级标注“待人工补证”，禁止签发“可提交”（`T-204`） |
| **ERR-03** | 截断失真 | `src/ai/patent.rs:307-308` 对长文本硬截断前 5000 字，丢失中后段实施例与实验表 | 以结构化切片 + BM25 与向量 RRF 混合检索 + 父权项/邻接段智能组装替代硬截断（`T-201`、`T-203`） |
| **ERR-04** | 守卫死锁 | 整顿令冻结期已过，`AGENTS.md` §〇 与 `tests/rectification_order_present.rs` 仍锁死旧口令 | 归档旧整顿令，升级规约文档与守卫测试为检查六大永久公理与字符预算上限（`T-101`） |
| **ERR-05** | 文档失步 | `AGENTS.md` 迁移版本写 `v11`（实为 `v24`）、与 `CLAUDE.md` 双份重复、英文 README 仍写 CNIPR | `CLAUDE.md` 改为精炼同源引用，动态对齐迁移号，全面校准中英文 README 与 API 导航（`T-101`、`T-504`） |
| **ERR-06** | 上下文污染 | `docs/errors.md` 超 280KB（[docs/README.md](https://github.com/jsshwqz/innoforge/blob/main/docs/README.md)），挤爆 `src/context.rs` 6000 字注入窗口 | 拆出 4KB 内的高频铁律摘要 `docs/errors-digest.md` 供 `context.rs` 注入，历史长文移入 `docs/archive/`（`T-101`） |
| **ERR-07** | 阻塞死锁 | [Cargo.toml](https://github.com/jsshwqz/innoforge/blob/main/Cargo.toml) 开启 `reqwest` 的 `blocking` 特性，异步线程误调易致死锁 | 移除 `blocking` 特性，全仓网络请求统一改为纯异步 `reqwest 0.12`（`T-102`） |
| **ERR-08** | 端口竞态 | 测试辅助残留 `free_port` 先放再绑 TOCTOU 竞态（`#110`），引发 CI 偶发假红 | 测试服务统一绑定 `127.0.0.1:0` 后直接传入已绑定 `TcpListener` 实例（`T-102`） |
| **ERR-09** | 扫描件报错 | 上传扫描版 PDF 在未装外部 `Umi-OCR` 时直接报错中断（[#67](https://github.com/jsshwqz/innoforge/issues/67)） | 落地“原生文字层 → 本机 Umi-OCR → 纯 Rust 抽内嵌页图接看图大模型自动回退”三级解析链（`T-202`） |
| **ERR-10** | 路由泛滥 | `/api/search/vector` 前端零调用悬空；130+ 条碎片路由混塞业务 Prompt | 收编悬空接口进混合检索内核；剥离全部路由层 Prompt；收敛为约 38 条正交 REST 接口（`T-302`～`T-403`） |
| **ERR-11** | 内核分裂 | `src/ai/` 31 文件平铺，3 套编排器并存，22 个核查文件重复，`patent.rs` 与 `types/` 分裂 | 重铸为 `src/core/` 五大唯一共享内核 × `src/cards/` 七大独立卡片分片（Phase 2～Phase 4） |
| **ERR-12** | 工具链旧债 | [package.json](https://github.com/jsshwqz/innoforge/blob/main/package.json) 并存 `playwright` 与 `puppeteer`；Web 栈停留在 `axum 0.6`；哈希守卫未覆盖外置模块 | 升级 `axum 0.8` + `tower-http 0.6`；清退 `puppeteer`；升级模板与原生 ES 模块联合哈希守卫（`T-102`、`T-301`、`T-502`） |

## 第二章：全仓库目录架构与存量模块 100% 迁移映射表

### 2.1 目标目录层级结构定义
1. **入口与装配层**：`src/main.rs`（桌面端进程入口）、`src/lib.rs`（移动端与桌面壳 C ABI / JNI 可重入入口）、`src/bin/mcp-server.rs`（本地 AionUI MCP 协议入口）、`src/app.rs`（全局 `AppState`、同源鉴权中间件、静态资源内嵌与 7 张卡片顶层路由挂载）。
2. **下层五大共享超级内核（`src/core/`）**：
   - `src/core/vault/`：`models.rs`（全域模型）、`schema_mig.rs`（WAL 连接与版本化迁移）、`chunker.rs`（专利与论文双栖结构化切分）、`local_rag.rs`（中英双字分词 FTS5 + 向量 RRF 混合检索与关联上下文组装）、`repo.rs`（文献、特征原子、附图词典与实验账本仓储）。
   - `src/core/multimodal/`：`doc_ingest.rs`（PDF/DOCX/图片/表格三级解析与看图 OCR 回退）、`figure_marks.rs`（专利附图部件标号提取与局部图联动）、`session.rs`（多轮图文附件会话与《结构化技术方案基线卡》增量收敛）。
   - `src/core/omni_search/`：`query_planner.rs`（意图识别与中英专业检索式改写）、`patent_sources.rs`（SerpAPI / Google Patents / EPO 适配）、`scholar_sources.rs`（[OpenAlex API](https://help.openalex.org/api/) / [Crossref REST API](https://www.crossref.org/documentation/retrieve-metadata/rest-api/) / [arXiv API](https://info.arxiv.org/help/api/user-manual.html) / [Semantic Scholar API](https://www.semanticscholar.org/product/api) 免 Key 学术源适配与真实 DOI/BibTeX 校验）、`code_std_sources.rs`（开源仓库与技术标准检索适配）、`fusion_rerank.rs`（熔断回退、跨源去重合并与推荐理由生成）。
   - `src/core/evidence_rebuttal/`：`anchor_verifier.rs`（专利段落/权项、论文真实 DOI/图表、实验记录 O(1) 回查）、`embodiment_guard.rs`（跨实施例强行拼凑特征确定性识破与特征覆盖矩阵计算）、`legal_academic_guard.rs`（专利法第 33 条修改超范围守卫 + 论文返修数据一致性守卫 + 涉密禁词一票否决）、`priority_guard.rs`（先专利申请日后论文/开源公开时序冲突检测与披露颗粒度裁剪）、`quality_gate.rs`（五维量化评分与硬阻断总闸门）。
   - `src/core/runtime/`：`llm_router.rs`（9 服务商自动容灾、看图能力路由、60s 超时、日预算与输入隔离）、`dag_runner.rs`（支持三档速选、任意节点分叉 Fork、人工卡点暂停恢复与 SSE 流推送的统一任务引擎）、`forge_export.rs`（PRD/TRD、调研/FTO 报告、真实题录论文 DOCX/LaTeX/MD、专利交底与修订留痕 OA DOCX 统一编译器）。
3. **上层七大零耦合独立卡片分片（`src/cards/`）**：每个子目录内部采用标准四件套结构（`mod.rs` 导出、`routes.rs` 薄 HTTP 处理器、`service.rs` 业务编排、`prompts.rs` 专属结构化提示词，按需含 `repo.rs` 卡片轻量会话存取）：
   - `src/cards/omni_search/`（卡片 ①：全域情报检索卡）
   - `src/cards/idea_studio/`（卡片 ②：多模态想法共创与可分叉推演卡）
   - `src/cards/deep_dissect/`（卡片 ③：单篇文献扒皮与深析卡）
   - `src/cards/scheme_matrix/`（卡片 ④：多方案对决与矩阵卡）
   - `src/cards/rebuttal_hub/`（卡片 ⑤：审查与审稿抗辩卡）
   - `src/cards/deliverable_forge/`（卡片 ⑥：全谱系成果锻造工坊卡）
   - `src/cards/settings_vault/`（卡片 ⑦：系统配置与知识金库卡）
4. **前端零构建目录（`templates/` + `static/`）**：
   - `templates/`：`index.html`（7 卡片独立导航台）+ `search.html`、`studio.html`、`dissect.html`、`matrix.html`、`rebuttal.html`、`forge.html`、`settings.html`（及独立详情阅读视图 `doc_viewer.html`）。
   - `static/js/core/`：`api-client.js`、`sse-client.js`、`dom-safe.js`、`toast.js`。
   - `static/js/widgets/`：`if-multimodal-box.js`、`if-citation-chip.js`、`if-vault-picker.js`、`if-matrix-table.js`。
   - `static/js/cards/`：与上述模板一一对应的独立 ES Module 入口文件。

### 2.2 原仓库现有模块（`v0.9.46`）精准处置对照表

| 原模块/文件集合 | 处置动作 | 目标模块路径与具体重构要求 |
| :--- | :--- | :--- |
| `src/common.rs` | 重构替换 | 升级为 `src/app.rs`，保留桌面与移动壳共享路由构建契约，剥离碎片路由实现 |
| `src/context.rs` | 升级保留 | 留任顶层或 `core/runtime/`，改为读取 `AGENTS.md` + `product-vision.md` + `feedback.md` + `errors-digest.md`（总长硬控 6000 字符内） |
| `src/patent.rs` + `src/types/*` | 合并下沉 | 全量归入 `src/core/vault/models.rs`；过渡期保留兼容类型重导出直至卡片迁移完毕后移除 |
| `src/ai/{client, providers, output_guard}.rs` | 提纯下沉 | 迁入 `src/core/runtime/llm_router.rs`，增加服务商 `supports_vision` 与学术/专利长上下文窗口元数据 |
| `src/ai/` 11 个核查文件 + `src/evidence_kernel/` 11 个文件 | 去重合流 | 合并浓缩至 `src/core/evidence_rebuttal/` 下 5 个子模块，消除重复结构体与重复正则扫描 |
| `src/ai/{idea, whitespace_search, adversarial}.rs` + `src/pipeline/*` + `src/orchestrator/*` + `src/experiment/*` | 拆底座归卡片 | 通用状态机与分叉调度归 `core/runtime/dag_runner.rs`；共创、16 步算子与实验回填归 `cards/idea_studio/` |
| `src/ai/{patent, chat, extraction, extraction_quality}.rs` | 按职责归卡 | 通用抽取下沉 `core/multimodal/`；单篇专利/论文深析、FTO 风险与带图问答归 `cards/deep_dissect/` |
| `src/routes/feature_cards.rs` + `src/ai/feature_attribution.rs` | 整合升维 | Hirschberg 线性空间差异算法保留并扩展为多维对比，归入 `cards/scheme_matrix/` |
| `src/ai/{case_input, letter_header, response_pipeline, strategy_arbitration, targeted_claims}.rs` + `src/routes/{oa_rounds, oa_diff, oa_export}.rs` + `src/oa_deadline.rs` | 整合泛化 | 归入 `cards/rebuttal_hub/`，在保留原有专利 OA 全部深度能力基础上，扩展新专利魔鬼预审与期刊论文 Rebuttal 模式 |
| `src/ai/search_report.rs` + `src/docx_export/*` + PDF/XLSX 导出路由 | 拆底座归卡片 | 格式渲染器下沉 `core/runtime/forge_export.rs`；四体裁裁剪编排与时序守卫交互归 `cards/deliverable_forge/` |
| `src/search/*` + `src/rag/*` + `src/vector/*` | 合并扩源 | 在线源与新增四大开放学术源归 `core/omni_search/`；结构化切分与混合检索归 `core/vault/`（闭环 [#78](https://github.com/jsshwqz/innoforge/issues/78)）；检索页业务归 `cards/omni_search/` |
| `src/cad.rs` + `src/routes/upload.rs` | 下沉共享 | 归入 `core/multimodal/{doc_ingest, figure_marks}.rs` |
| `src/db/*`（18 个文件） | 收敛迁移 | 公共资产归 `core/vault/repo.rs`；卡片会话归各 `cards/*/repo.rs`；迁移统一集中在 `core/vault/schema_mig.rs` |

## 第三章：数据库 Schema 演进与五大内核接口契约规范

### 3.1 数据库版本化平滑迁移与核心表字段字典（`v24 → v26` 无损演进）

迁移脚本只执行毫秒级建表、加列与触发器注册，历史存量专利数据通过后台增量事务按需回填，严禁启动时全表阻塞扫描：

| 表名 | 字段名与存储类型 | 必填/默认值与枚举约束 | 字段职责与索引设计 |
| :--- | :--- | :--- | :--- |
| **`knowledge_docs`**<br>（统一文献资产表，兼容承接原 `patents` 表） | `doc_id` (`TEXT PK`)<br>`doc_kind` (`TEXT NOT NULL`)<br>`asset_role` (`TEXT NOT NULL`)<br>`external_id` (`TEXT`)<br>`title_zh` / `title_en` (`TEXT`)<br>`authors_or_assignees` (`TEXT`)<br>`pub_or_filing_date` (`TEXT`)<br>`venue_or_authority` (`TEXT`)<br>`filing_status` (`TEXT NOT NULL`)<br>`doi_verified` (`INTEGER NOT NULL`)<br>`bibtex_raw` (`TEXT`)<br>`abstract_text` (`TEXT`)<br>`claims_json` (`TEXT`)<br>`fulltext_status` (`TEXT NOT NULL`)<br>`chunk_ver` (`INTEGER NOT NULL`) | • `doc_kind`: `patent / journal_paper / conf_paper / preprint / code_repo / standard / internal_scheme`<br>• `asset_role`: `own_asset / competitor_wall / review_cited / general_ref`（默认 `general_ref`）<br>• `filing_status`: `unfiled_secret / patent_filed / public_disclosed`<br>• `doi_verified`: `0` 或 `1`<br>• `chunk_ver`: 默认 `0`，结构化切分后置为 `2` | 唯一索引 `(doc_kind, external_id)`（非空时）；二级索引 `(asset_role, pub_or_filing_date)`。老库 `patents` 表记录无损映射为 `doc_kind='patent'` |
| **`doc_chunks`**<br>（双栖结构化切片主表） | `chunk_id` (`INTEGER PK AUTO`)<br>`doc_id` (`TEXT NOT NULL FK`)<br>`section_kind` (`TEXT NOT NULL`)<br>`section_tag` (`TEXT NOT NULL`)<br>`claim_no` (`INTEGER NULL`)<br>`parent_claim_no` (`INTEGER NULL`)<br>`para_start` / `para_end` (`INTEGER NULL`)<br>`embodiment_group` (`TEXT NULL`)<br>`raw_content` (`TEXT NOT NULL`)<br>`fts_tokens` (`TEXT NOT NULL`)<br>`embedding_blob` (`BLOB NULL`)<br>`char_count` (`INTEGER NOT NULL`) | • `section_kind`: `claim_indep / claim_dep / abstract / tech_field / background / summary / drawings / embodiment / paper_intro / paper_method / paper_experiment / paper_table_fig / paper_limitation / fallback`<br>• `section_tag` 标准示例：`Claim:1`、`Desc:[0024]`、`Desc:[0023-0024]`、`Paper:Sec3.1`、`Paper:Table2` | 联合唯一索引 `(doc_id, section_tag)` 支撑证据内核 **O(1) 锚点核查**；过滤索引 `(doc_id, claim_no)` 与 `(doc_id, section_kind)` |
| **`chunks_fts`**<br>（FTS5 全文索引虚拟表） | `section_tag`、`fts_tokens`、`doc_id UNINDEXED`、`section_kind UNINDEXED` | 绑定外部内容表 `content='doc_chunks', content_rowid='chunk_id'`，分词器 `unicode61 remove_diacritics 2`，配增删改自动触发器 | `fts_tokens` 由纯 Rust 生成（英文/数字/IPC/段号原样保留，连续汉字按“单字+相邻双字”空格展开），兼顾单双字专业词精度与移动端零外部 C 依赖 |
| **`figure_numerals`**<br>（附图部件标号词典表） | `id` (`INTEGER PK AUTO`)<br>`doc_id` (`TEXT NOT NULL`)<br>`figure_label` (`TEXT`)<br>`numeral_code` (`TEXT NOT NULL`)<br>`part_name` (`TEXT NOT NULL`)<br>`embodiment_group` (`TEXT NULL`)<br>`anchor_tag` (`TEXT NOT NULL`) | 联合索引 `(doc_id, numeral_code)` | 支撑阅读时标号悬停解释、附图联动高亮，以及与 `embodiment_group` 配合判定“跨实施例强行拼凑” |
| **`feature_atoms`**<br>（沉淀技术特征原子表） | `atom_id` (`TEXT PK`)<br>`doc_id` (`TEXT NOT NULL`)<br>`anchor_tag` (`TEXT NOT NULL`)<br>`tech_problem` (`TEXT`)<br>`tech_means` (`TEXT NOT NULL`)<br>`tech_effect` (`TEXT`)<br>`created_at` (`TEXT NOT NULL`) | 索引 `(doc_id)` | 各卡片深析、对比、抗辩提取出的经核实特征自动沉淀，供跨卡片复用 |
| **`experiment_records`**<br>（对照实验与实测账本表） | `exp_id` (`TEXT PK`)<br>`run_id` (`TEXT NULL`)<br>`title` (`TEXT NOT NULL`)<br>`hypothesis` (`TEXT NOT NULL`)<br>`control_group_spec` (`TEXT NOT NULL`)<br>`test_group_spec` (`TEXT NOT NULL`)<br>`sandbox_output_json` (`TEXT NULL`)<br>`measured_data_json` (`TEXT NULL`)<br>`status` (`TEXT NOT NULL`) | `status`: `designed / sandbox_ran / real_data_filled / verified` | 当用户回填真实样机或实验数据（`real_data_filled`）后，自动生成 `Exp:{exp_id}` 铁证锚点供专利实施例、OA 争辩与论文实验表直接引用 |
| **`dag_runs` & `dag_nodes`**<br>（可分叉任务与会话持久化表） | `run_id` (`TEXT PK`)、`card_kind` (`TEXT`)、`parent_run_id` (`TEXT NULL`)、`fork_from_step` (`TEXT NULL`)、`tier_mode` (`TEXT`)、`scheme_baseline_json` (`TEXT`)、`mounted_own_doc_ids` (`TEXT`)、`status` (`TEXT`)；节点表存每步输入输出快照与卡点待确认载荷 | `status`: `chatting / running / paused_checkpoint / completed / failed` | 统一承载多轮共创前舱会话、三档速选状态、任意步骤分叉（Fork）父子谱系及断点续跑恢复 |

### 3.2 五大共享内核对上层卡片暴露的核心服务契约

1. **`VaultService`（`src/core/vault/`）**
   - `upsert_document`：写入或更新四维文献并自动触发双栖结构化切分与附图部件词典抽取（单事务批处理，零 N+1）。
   - `hybrid_search_chunks`：入参含查询词、检索场景模式（`PatentCompare / PaperSurvey / AnchorExact`）、限定文献集或资产角色过滤、预算字符上限；内部执行 BM25 + 向量加权 RRF（背景技术章节自动降权 0.75，独立权项与核心实验自动提权 1.15～1.20），并自动回溯补全从属权利要求的父权项链及邻接段落，返回 `HybridEvidenceBundle`。
   - `save_and_verify_experiment`：保存对照实验设计或写入用户回填的实测指标，返回可引用实验锚点标识。
2. **`MultimodalService`（`src/core/multimodal/`）**
   - `ingest_attachment`：接收上传文件或剪贴板图像字节流；对 PDF 按页判定有效字符密度（去噪 ≥80 字符且乱码率低于 5% 走原生文字层；否则先探活 500ms 本机 `Umi-OCR`；不在线且隐私模式允许时纯 Rust 提取扫描页内嵌图像调用看图模型转录并抽取附图部件表/论文图表）。
   - `advance_cocreation_turn`：处理一轮图文附件混合输入，返回“启发式追问回复 + 增量更新后的《结构化技术方案基线卡》+ 可选原理框图描述”。
3. **`OmniSearchService`（`src/core/omni_search/`）**
   - `search_across_realms`：按勾选圈层（专利 / 期刊论文 / 开源工程 / 行业标准）并发调用已启用的检索源适配器，经熔断器保护、公开号/DOI 归一化去重及相关性重排，返回统一结构列表及每条结果的推荐命中证据说明；对学术文献强制固化经远端元数据源核实的真实 DOI 与标准 BibTeX。
4. **`EvidenceRebuttalService`（`src/core/evidence_rebuttal/`）**
   - `verify_all_citations`：扫描任意草稿中的专利锚点、论文引用与实验引用，逐一在 `doc_chunks`、`knowledge_docs`、`experiment_records` 中执行 O(1) 回查，输出每处引用的绿/黄/红核验状态与原文片段。
   - `detect_cross_embodiment_stitching`：针对审查员或竞品比对结论，校验所引用的多个技术特征是否跨越了对比文献中互不兼容的不同 `embodiment_group`，输出反驳依据。
   - `check_scope_and_consistency`：专利模式下执行第 33 条修改超范围与同义偷换核查；论文模式下执行返修回复信与修改后手稿的页码/图表/实验数据一致性核查。
   - `check_disclosure_priority`：检查当前方案关联的成果导出记录与 `filing_status`，若存在“专利未获申请日而拟导出公开论文/开源文档”则输出强制预警载荷。
   - `evaluate_submission_gate`：汇总输出五维得分与最终门禁裁决（`PassSubmittable` / `DowngradeNeedsEvidence` / `HardBlocked`）。
5. **`RuntimeForgeService`（`src/core/runtime/`）**
   - `start_or_fork_dag` / `resume_from_checkpoint`：驱动多步任务执行、节点分叉与人工卡点确认，通过统一 SSE 通道推送进度与增量结果。
   - `compile_deliverable`：按指定体裁（`ProductPrdTrd` / `SurveyFtoReport` / `AcademicPaper` / `PatentDisclosure` / `OaPackage` / `PaperRebuttalPackage`）与目标格式（`docx / md / tex / xlsx / pdf`）执行体裁裁剪、商业秘密脱敏提示、五维门禁校验并生成最终文件流。

## 第四章：前端四大原生积木与 7 张独立卡片交互规格

### 4.1 浏览器原生 Web Components 四大积木契约（零打包工具）

| 自定义标签名 | 标准输入属性（Attributes / Properties） | 派发标准事件（CustomEvents） | 渲染与安全规范 |
| :--- | :--- | :--- | :--- |
| **`if-multimodal-box`** | `placeholder-key`（双语键）、`accept-types`、`allow-vault-mount`（布尔） | `if-submit`（携带文本、粘贴图片列表、已解析附件 ID 列表、已挂载本机文献 ID 列表） | 原生监听 `paste` 剪贴板图像与拖拽文件；全部文件名与预览摘要使用 `textContent` 渲染防 XSS |
| **`if-citation-chip`** | `doc-id`、`anchor-tag`（如 `Claim:1`、`Desc:[0024]`、`DOI:10.xxxx`、`Exp:01`）、`verdict`（`verified / summary / unverified`） | `if-open-source`（通知宿主页面右侧滑出原文与附图对照抽屉并滚动高亮目标锚点） | 绿/黄/红高对比度视觉语义统一，鼠标悬停显示部件词典或题录摘要 |
| **`if-vault-picker`** | `filter-roles`（如筛选 `own_asset` 自有专利/论文）、`multi-select`（布尔） | `if-select-change`（返回选中的本机文献 ID 与实验账本 ID 集合） | 以非侵入式轻量抽屉/模态框挂载在任意独立卡片上，不选时零界面干扰 |
| **`if-matrix-table`** | `matrix-mode`（`claim_chart / engineering_spec / paper_baseline`）、`columns-data`、`rows-data` | `if-cell-anchor-click`、`if-export-request`（导出 Excel 或 PDF） | 单元格内置 `if-citation-chip` 联动，数值与判定标签不折行 |

### 4.2 七张独立卡片页面布局与双语命名空间规范

1. **首页独立卡片导航台（`templates/index.html` · 命名空间 `nav.*`）**：清爽网格呈现 7 张独立功能卡片（清晰标注每张卡片解决的具体场景与输入输出），顶栏右侧常驻一颗非打扰的“系统健康状态胶囊灯”（点击展开四维自检详情）。
2. **卡片 ① 全域情报检索卡（`templates/search.html` + `doc_viewer.html` · `card_search.*`）**：顶部四维圈层勾选与搜索栏（支持自然语言与专业布尔式切换）；左栏结果卡列表（带文献类型徽章、推荐命中理由折叠行、一键打标 `[我方已有成果]` 入库按钮）；右栏同屏结构化精读器（专利权利要求树/段落目录/附图部件对照，论文摘要/方法指标/BibTeX 复制）。
3. **卡片 ② 多模态想法共创与推演卡（`templates/studio.html` · `card_studio.*`）**：采用“前舱共创 + 后舱可分叉流水线”双视图切换——前舱左侧为 `if-multimodal-box` 多轮图文对话流，右侧实时刷新《结构化技术方案基线卡》与可选挂载的 `[我方已有专利/论文]`；点击锁定基线后进入后舱，顶部可选**三档运行深度**（快速探路档 / 研发与对照实验档 / 全套保护与抗辩预演档），步骤轨支持在检索词确认、文献筛选节点暂停微调，支持任意步点击「分叉推演（对比路线 B）」，并在实验步内置结构化**实测数据回填表单**。
4. **卡片 ③ 单篇文献扒皮与深析卡（`templates/dissect.html` · `card_dissect.*`）**：顶部拖入单篇专利/论文或从金库点选；左栏原文与附图/图表对照阅读器（支持局部截图圈选追问）；右栏分标签页展示结构化扒皮结论（专利核心方案与附图连接字典 / 逐权项 FTO 侵权风险定级与规避建议；或论文机理公式白话拆解 / 复现超参数表 / 局限性与改进切入点）。
5. **卡片 ④ 多方案对决与矩阵卡（`templates/matrix.html` · `card_matrix.*`）**：顶部槽位支持并排添加 2～6 个对比对象（混合上传文档、贴架构图或选本机自有前代专利/SOTA 论文）；主体为 `if-matrix-table` 三重视角切换（工程性能成本对决 / 学术 Baseline 增益对决 / 专利 Claim Chart 覆盖度与跨实施例拼凑预警），底部配融合优化建议与导出栏。
6. **卡片 ⑤ 审查与审稿抗辩卡（`templates/rebuttal.html` · `card_rebuttal.*`）**：顶部清晰切换三大独立子模式（**① 已有专利官方 OA/复审**、**② 新专利提交前魔鬼预审**、**③ 期刊/论文模拟审稿与返修回复 Rebuttal**）；三栏专业攻坚布局——左栏案件文献/手稿与引证锚点对照，中栏逐条质疑拆解、策略裁决与权项/正文修订编辑器（支持一通→二通多轮历史切换与绝限倒计时），右栏实时运行五维硬阻断门禁红黄灯面板与一键三件套导出。
7. **卡片 ⑥ 全谱系成果锻造工坊卡（`templates/forge.html` · `card_forge.*`）**：左栏勾选本次编译的素材源（技术方案基线 + 已验证实验记录 + 核实参考文献集）与目标体裁（产品 PRD/TRD / 全域调研与 FTO 报告 / 期刊论文初稿 / 专利技术交底书）及商业秘密保留项；顶部内置醒目的**「公开时序冲突守卫灯」**；右栏为结构化大纲与分节预览器（所有引用内嵌 `if-citation-chip` 核验），确认过门禁后一键导出 `.docx` / `.md` / `.tex` + `.bib` / `.pdf`。
8. **卡片 ⑦ 系统配置与知识金库卡（`templates/settings.html` · `card_settings.*`）**：分四个清晰面板——① 四维环境健康自检与安全监听状态；② 9 家 AI 服务商容灾排序、看图模型指定与日预算控制；③ 四维检索凭据与**免 Python 原生公开专利/文献数据包拖拽流式导入**；④ 本机知识金库总表（管理 `[我方已有专利/论文]` 资产状态、特征原子、实验账本与 SQLite 一键备份）。

## 第五章：正交 API 接口契约总表与统一 SSE 事件规范

将原先杂乱膨胀的 130+ 条历史路由全面收敛归并为以下 **8 组共 36 条正交 REST 与流式接口**（对高频历史路径保留薄转发别名并标注废弃，同步通过自动化脚本生成 `docs/API-ROUTES.md` 与 `docs/API-CONTRACTS.md`）：

| 归属模块 | 标准方法与路径 | 核心入参契约 | 核心出参 / 流式契约 |
| :--- | :--- | :--- | :--- |
| **系统与鉴权**<br>(`app` & `settings_vault`) | `GET /api/healthz`<br>`POST /api/auth/unlock`<br>`GET /api/settings/health`<br>`GET / POST /api/settings/config`<br>`POST /api/settings/probe-provider`<br>`POST /api/vault/import-pack`<br>`GET / PATCH / DELETE /api/vault/docs` | 免鉴权探针；访问码换同源 `HttpOnly` Cookie；配置更新（含服务商容灾链、OCR 回退策略）；公开包流式上传；金库文献角色（`own_asset`）与申请状态（`filing_status`）更新 | 存活状态；四维健康诊断灯；掩码脱敏后的配置快照；SSE 数据包灌库与切片进度流；分页文献与实验资产列表 |
| **卡片 ① 全域检索**<br>(`omni_search`) | `POST /api/omni-search/query`<br>`POST /api/omni-search/chunks`<br>`GET /api/omni-search/doc/{id}`<br>`POST /api/omni-search/export` | 查询词、勾选圈层列表、资产角色过滤、是否开启 AI 改写与重排；段落混合检索模式；导出格式（`csv / xlsx / pdf`） | 跨四维归一化结果列表（含真实 DOI/公开号、BibTeX 与推荐命中证据）；RRF 段落命中列表；完整文献结构（权项树 + 段落 + 附图词典） |
| **卡片 ② 共创推演**<br>(`idea_studio`) | `POST /api/idea-studio/session`<br>`POST /api/idea-studio/session/{id}/turn`<br>`POST /api/idea-studio/run/start`<br>`POST /api/idea-studio/run/{id}/fork`<br>`POST /api/idea-studio/run/{id}/checkpoint`<br>`POST /api/idea-studio/run/{id}/experiment`<br>`GET /api/idea-studio/run/{id}/events` | 创建多模态会话；发送图文附件轮次；选定档位（1/2/3 档）与挂载的自有资产 ID 启动流水线；从指定步骤分叉新路线；提交人工卡点微调确认；回填真实实验测量数据 | 增量更新的《技术方案基线卡》与启发式追问；运行实例与父子分叉谱系；实测证据锚点 `Exp:{id}`；统一 SSE 进度与节点产出流 |
| **卡片 ③ 单篇深析**<br>(`deep_dissect`) | `POST /api/deep-dissect/ingest`<br>`POST /api/deep-dissect/analyze`<br>`POST /api/deep-dissect/ask` | 上传单篇专利/论文或传本机 `doc_id`；分析子类型（专利方案与附图拆解 / FTO 逐权项排雷 / 论文机理与复现拆解 / 局限性与改进挖掘）；多轮带图追问载荷 | 结构化拆解报告（全部断言带核验后的锚点状态）、自动沉淀的特征原子列表、流式问答响应 |
| **卡片 ④ 方案矩阵**<br>(`scheme_matrix`) | `POST /api/scheme-matrix/compare`<br>`GET /api/scheme-matrix/{id}`<br>`POST /api/scheme-matrix/{id}/export` | 2～6 个对比源（上传附件或本机 `doc_id` / `run_id`）及对比视角（工程指标 / 学术 Baseline / 专利 Claim Chart） | 结构化三维矩阵行列数据、Hirschberg 差异高亮、跨实施例拼凑预警清单、Excel/PDF 导出文件流 |
| **卡片 ⑤ 三栖抗辩**<br>(`rebuttal_hub`) | `POST /api/rebuttal-hub/case`<br>`POST /api/rebuttal-hub/case/{id}/round`<br>`POST /api/rebuttal-hub/case/{id}/mock-review`<br>`POST /api/rebuttal-hub/case/{id}/draft`<br>`POST /api/rebuttal-hub/case/{id}/verify`<br>`POST /api/rebuttal-hub/case/{id}/export` | 抗辩模式（`patent_oa / patent_pre_mock / paper_rebuttal`）；上传通知书/审稿信、原稿与对比文献；新增一通/二通/返修轮次；触发策略裁决与答复生成；触发五维硬门禁核验；导出三件套 | 质疑点结构化拆解表、修改 vs 纯争辩双轨策略、逐条绑定锚点的答复草稿、第 33 条/数据一致性与五维门禁详细体检单、带修订痕迹的 `.docx` 压缩包 |
| **卡片 ⑥ 成果工坊**<br>(`deliverable_forge`) | `POST /api/deliverable-forge/priority-check`<br>`POST /api/deliverable-forge/compile`<br>`POST /api/deliverable-forge/export` | 关联的方案基线 ID、实验记录 ID 集、核实参考文献 ID 集、目标体裁（产品 / 报告 / 论文 / 专利）、商业秘密脱敏字段标记、导出文件格式（`docx / md / tex / pdf`） | 公开时序冲突守卫检查结果（红/黄/绿灯与处置建议）；分节可核验草稿与五维门禁评分；目标格式成品文档与 `.bib` 题录包 |
| **多模态公共上传**<br>(`multimodal`) | `POST /api/multimodal/upload`<br>`GET /api/multimodal/task/{id}/events` | 多格式文件或剪贴板图像上传、隐私解析策略覆盖项 | 解析后附件标识、三级 OCR 逐页识别 SSE 进度流、提取出的附图部件词典预览 |

**统一 SSE 流式事件信封规范**：全仓所有 SSE 端点统一采用标准事件名与载荷字段——`stage_update`（当前阶段、完成百分比、耗时）、`token_delta`（流式文本增量）、`checkpoint_required`（卡点类型、待人工确认的可编辑候选数据）、`citation_audit`（实时锚点与 DOI 回查结果）、`gate_result`（五维评分与通过/降级/阻断状态）、`completed`（最终产物摘要 ID）、`error`（双语错误码与可恢复指引）。

## 第六章：多 Agent 可直接认领的原子任务施工看板（`T-101` ～ `T-505`）

本看板替代旧历史队列，供单个或多个 AI 开发 Agent 按序认领施工。**每个任务均严格划定了读写目录边界与必过验收命令**，支持同阶段无依赖任务多 Agent 并行推进且零合并冲突：

| 阶段 | 任务号 | 前置依赖 | 原子任务内容（严禁跨边界混改） | 允许修改路径范围 | 准出验收硬门禁 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Phase 1**<br>规约与地基 | **`T-101`** | 无 | 归档旧整顿令；落盘 `product-vision.md`、新版 `AGENTS.md` 与 `CLAUDE.md`；将 `docs/errors.md` 拆出 4KB `errors-digest.md`；升级 `src/context.rs` 与宪章守卫测试 | `AGENTS.md`, `CLAUDE.md`, `docs/`, `src/context.rs`, `tests/rectification_order_present.rs` | `cargo test --test rectification_order_present` 及 `context` 单测全绿 |
| **Phase 1** | **`T-102`** | `T-101` | 升级 `Cargo.toml` 至 `axum 0.8`、`tower-http 0.6`、纯异步 `reqwest 0.12`（彻底移除 `blocking`）；全量测试服务器改为传递已绑定 `TcpListener`；适配契约生成脚本路由正则 | `Cargo.toml`, `src/`, `tests/`, `scripts/gen_*.js` | `cargo clippy --all-targets -- -D warnings` 与全量 `cargo test` 零警告通过 |
| **Phase 1** | **`T-103`** | `T-102` | 建立 `src/app.rs`；落地桌面默认 `127.0.0.1` 绑定、非回环自动同源 `HttpOnly` Cookie + Token 鉴权中间件、密钥掩码回显；加固 `Dockerfile` 与 `src/lib.rs` 移动端 C ABI `catch_unwind` 可重入启停状态机 | `src/main.rs`, `src/lib.rs`, `src/app.rs`, `Dockerfile`, `tests/ffi_*.rs` | 鉴权拦截单测全绿；C ABI 同一进程双周期启停冒烟测试通过 |
| **Phase 2**<br>五大共享核 | **`T-201`** | `T-103` | 铸造 `src/core/vault/`：合并全仓模型，完成 `v25+` 平滑迁移建立 `knowledge_docs`、`doc_chunks`、`chunks_fts`、`figure_numerals`、`feature_atoms`、`experiment_records`、`dag_runs` 表，实现双栖结构化切分与纯 Rust 双字分词 + RRF 混合检索（闭环 [#78](https://github.com/jsshwqz/innoforge/issues/78)） | `src/core/vault/`, `tests/` | 老库平滑迁移无损测试、单双字中文术语 BM25 命中测试、从属权项自动拼父权项测试全绿 |
| **Phase 2** | **`T-202`** | `T-201` | 铸造 `src/core/multimodal/`：实现原生文字层 → 本机 Umi-OCR → 纯 Rust 抽扫描页内嵌图接看图模型三级自适应解析、专利附图部件标号提取、以及多轮图文附件共创会话管理 | `src/core/multimodal/`, `tests/` | 混合文字/扫描页按需分流单测、附图部件词典提取单测与涉密离线锁测试通过 |
| **Phase 2** | **`T-203`** | `T-201` | 铸造 `src/core/omni_search/`：整合专利级联检索源，新增 OpenAlex、Crossref、arXiv、Semantic Scholar 四大免 Key 学术论文源与开源/标准检索适配器，实现跨源去重合并与真实 DOI/BibTeX 校验 | `src/core/omni_search/`, `tests/` | 四维并发检索、熔断降级及真实 DOI 归一化校验单测全绿 |
| **Phase 2** | **`T-204`** | `T-201` | 铸造 `src/core/evidence_rebuttal/`：深度去重合并原 22 个核查文件，实现专利锚点/真实 DOI/实验记录 O(1) 回查、跨实施例拼凑识破、第 33 条超范围与论文一致性守卫、先专利后论文公开时序守卫、五维硬阻断总闸门（根治 `ERR-02`） | `src/core/evidence_rebuttal/`, `tests/` | 伪造段落号、伪造论文 DOI、跨实施例拼凑、超范围修改、公开时序冲突 5 类负控测试 100% 触发拦截降级 |
| **Phase 2** | **`T-205`** | `T-201`～`T-204` | 铸造 `src/core/runtime/`：封装 9 服务商自动容灾与看图路由、支持三档速选/节点分叉 Fork/卡点暂停恢复与统一 SSE 信封的 `dag_runner`、以及一源四出多体裁导出编译器 `forge_export` | `src/core/runtime/`, `tests/` | 任务卡点暂停恢复、节点分叉并行谱系、以及 `.docx`/`.tex`/`.xlsx`/`.md` 导出完整性单测通过 |
| **Phase 3**<br>前端底座与前四卡 | **`T-301`** | `T-103` | 建立零构建前端底座 `static/js/core/` 与四大浏览器原生组件 `static/js/widgets/{if-multimodal-box, if-citation-chip, if-vault-picker, if-matrix-table}.js`；从 `package.json` 移除 `puppeteer`，升级 `eslint` 与模板模块联合完整性校验脚本 | `static/style.css`, `static/i18n.js`, `static/js/core/`, `static/js/widgets/`, `package.json`, `scripts/` | `npm run lint` 与函数完整性脚本零报错；组件防 XSS 测试通过 |
| **Phase 3** | **`T-302`** | `T-205`, `T-301` | 交付**卡片 ① 全域情报检索卡**（可与 `T-303`～`T-305` 并行施工）：四维检索页、左搜右读同屏精读器、附图部件悬停与一键打标入库 | `src/cards/omni_search/`, `templates/search.html`, `templates/doc_viewer.html`, `static/js/cards/search.js` | 卡片 ① 接口契约、双语词条与端到端检索入库回归全绿 |
| **Phase 3** | **`T-303`** | `T-205`, `T-301` | 交付**卡片 ② 多模态想法共创与推演卡**：前舱多轮图文文件共创与基线卡收敛 + 后舱挂载自有前序成果、三档速选、路线 A/B 分叉推演与实测实验数据回填闭环 | `src/cards/idea_studio/`, `templates/studio.html`, `static/js/cards/studio.js` | 多模态轮次收敛、三档运行、节点分叉与实测回填生成 `Exp` 锚点测试全绿 |
| **Phase 3** | **`T-304`** | `T-205`, `T-301` | 交付**卡片 ③ 单篇文献扒皮与深析卡**：单篇专利（方案还原、附图字典、逐权项 FTO 风险）与单篇论文（机理拆解、复现条件、局限性与改进点）双模式深析及局部图追问 | `src/cards/deep_dissect/`, `templates/dissect.html`, `static/js/cards/dissect.js` | 长专利与长论文无截断深析、特征原子自动入库及带图问答测试全绿 |
| **Phase 3** | **`T-305`** | `T-205`, `T-301` | 交付**卡片 ④ 多方案对决与矩阵卡**：跨形态多方案（前代/新方案/竞品专利/SOTA 论文）三维对决矩阵、跨实施例拼凑高亮与 Excel/PDF 导出 | `src/cards/scheme_matrix/`, `templates/matrix.html`, `static/js/cards/matrix.js` | 三维矩阵切换、Hirschberg 差异比对与导出测试全绿 |
| **Phase 4**<br>后三卡与清场 | **`T-401`** | `T-205`, `T-301` | 交付**卡片 ⑤ 审查与审稿抗辩卡**（可与 `T-402`、`T-403` 并行施工）：已有专利官方 OA 多轮攻坚、新专利提交前魔鬼预审、期刊论文模拟审稿与返修逐条回复（Rebuttal）三模式及修订留痕导出 | `src/cards/rebuttal_hub/`, `templates/rebuttal.html`, `static/js/cards/rebuttal.js` | 三大子模式全流程、第 33 条与数据一致性硬阻断、修订留痕 DOCX 导出全绿 |
| **Phase 4** | **`T-402`** | `T-205`, `T-301` | 交付**卡片 ⑥ 全谱系成果锻造工坊卡**：一源四出体裁编译器（产品 PRD/TRD、调研/FTO 报告、真实 BibTeX 期刊论文 `.docx`/`.tex`、专利交底书）与公开时序冲突拦截守卫 | `src/cards/deliverable_forge/`, `templates/forge.html`, `static/js/cards/forge.js` | 四类成果导出格式完整、零假 DOI 引用、未交专利先出论文 100% 触发时序预警 |
| **Phase 4** | **`T-403`** | `T-205`, `T-301` | 交付**卡片 ⑦ 系统配置与知识金库卡**及新版首页导航台：9 服务商容灾配置、首启四维健康自检面板、免 Python 原生公开文献/专利包流式导入（闭环 [#67](https://github.com/jsshwqz/innoforge/issues/67)）、本机金库资产管理 | `src/cards/settings_vault/`, `templates/index.html`, `templates/settings.html`, `static/js/cards/settings.js` | 首启诊断响应低于 1.5s、原生数据包流式灌库与金库管理测试全绿 |
| **Phase 4** | **`T-404`** | `T-302`～`T-403` | 彻底清场收口：移除已迁空的旧平铺目录（旧 `src/ai/`、`pipeline/`、`orchestrator/`、`vector/` 及冗余路由表），配置关键旧路由兼容转发别名，生成最新 `docs/API-ROUTES.md`、`docs/API-CONTRACTS.md` 与 `docs/API.md` | `src/`, `templates/`, `docs/API*.md` | 全仓零死代码告警、中英双语键值 100% 齐平、API 三维契约校验零差异 |
| **Phase 5**<br>多端与封版 | **`T-501`** | `T-404` | 升级 `src/bin/mcp-server.rs`：基于 WAL 无锁并发模式向本机 AionUI 暴露四维检索、混合证据召回、多模态入库、三栖核查与四体裁编译全套标准 MCP 工具 | `src/bin/mcp-server.rs`, `tests/` | Web 主服务与 MCP 进程并发读写同一 SQLite 库零锁冲突测试通过 |
| **Phase 5** | **`T-502`** | `T-404` | 升级 `.github/workflows/` 五大工作流（`ci.yml`、`template-hash-check.yml`、`ai-review.yml`、`guard-tag.yml`、`release.yml`），纳入 ES 模块联合哈希、API 契约防漂移与 AI Review 禁词脱敏保护 | `.github/workflows/`, `scripts/` | 工作流语法校验与本地模拟门禁脚本全绿 |
| **Phase 5** | **`T-503`** | `T-404` | 编写覆盖首页与全部 7 张独立卡片核心按钮、剪贴板贴图、节点分叉、三栖抗辩及四类导出的 Playwright 动态逐按钮回归套件（闭环 [#7](https://github.com/jsshwqz/innoforge/issues/7)），接入 `scripts/release_verify.ps1` | `scripts/e2e_test.mjs`, `scripts/release_verify.ps1` | `/api/version` 提交哈希一致，7 张卡片全部交互按钮 HTTP 状态码与导出断言 100% 通过 |
| **Phase 5** | **`T-504`** | `T-501`～`T-503` | 全面更新中英文 `README.md`、`docs/README.md`、`CHANGELOG.md`，更新 [#90](https://github.com/jsshwqz/innoforge/issues/90) 看板收官状态，完成 `v1.0.0` 正式版标签封版与交付验收 | `README.md`, `CHANGELOG.md`, `docs/` | `scripts/release_verify.ps1` 全项通过，达到正式发版状态 |
