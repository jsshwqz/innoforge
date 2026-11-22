# InnoForge 核心能力与单一权威入口地图（CODE_MAP.md）

> 开工必读（≤ 80 行）：修改或新增任何功能前，先在本表定位**唯一权威入口**与**秒级定向单测**，严禁另起炉灶搞“两张皮”。

| 专家角色 / 领域 | 核心业务能力 | 唯一权威入口文件 | 关联路由 / 数据表 | 秒级定向单测命令 |
| :--- | :--- | :--- | :--- | :--- |
| **全局底座** | 路由统一注册与鉴权中间件 | `src/common.rs` (`build_router_with_token`) | 全路由 (`docs/API-ROUTES.md`) | `cargo test --lib common` |
| **全局底座** | 公共领域类型与 DTO | `src/patent.rs`、`src/types/` | 全模块共用结构体 | `cargo test --lib patent` |
| **全局底座** | SQLite 版本化幂等迁移 | `src/db/migrations.rs` (`v1`~`v26`) | `schema_version` | `cargo test --lib db::tests` |
| **全局底座** | AI 客户端、降级链与安全截断 | `src/ai/client.rs`、`src/routes/mod.rs` | `/api/ai/models`、`app_settings` | `cargo test --lib ai::client` |
| **全局底座** | 全链路 Token 记账与单日/单创意硬熔断 | `src/db/cost.rs` (`check_ai_budget_circuit_breaker`) | `ai_cost_log`、`/api/settings/ai-budget` | `cargo test --lib db::cost` |
| **检索员** | 多源检索编排、冷却熔断与去重 | `src/search/mod.rs`、`src/search/breaker.rs` | `POST /api/search/online` | `cargo test --lib search` |
| **检索员** | Agentic 检索改写、精排与推荐理由 | `src/search/agent.rs`、`src/routes/search.rs` | `POST /api/search`、`ai_cost_ledger` | `cargo test --lib search::agent` |
| **检索员** | 专利/学术数据源 Providers | `src/search/providers/` (`serpapi`/`epo_ops`/`openalex`等) | `/api/patent/:id/legal-status` | `cargo test --lib search::providers` |
| **检索员** | 专利分块、`chunks_fts`、向量回填与 RRF 混合检索 | `src/patent_chunker.rs`、`src/db/rag.rs`、`src/db/vector.rs` | `POST /api/search/vector`、`patents_embedding` | `cargo test --lib db::vector` |
| **检索员+分析师** | 免费全文补全与 HTML 解析 | `src/search/enrichment.rs` (`enrich_patent_free`) | `GET /api/patent/enrich-free/:id` | `cargo test --lib search::enrichment` |
| **检索员+分析师** | **专利 PDF 真实单张附图提取** | `src/pdf_drawings.rs` (`extract_drawings_from_pdf_bytes`) | `POST /api/patent/:id/extract-pdf-drawings` | `cargo test --lib pdf_drawings` |
| **检索员+分析师** | **PDF/图片文本提取与 Umi-OCR 统一通道** | `src/routes/upload.rs`、`src/ai/extraction.rs` | `POST /api/upload/extract` (`UMI_OCR_BASE_URL`) | `cargo test --lib ai::extraction` |
| **技术分析师** | 16 步创新验证流水线编排与状态机 | `src/pipeline/engine.rs`、`src/db/research_state.rs` | `ideas`、`research_states` | `cargo test --lib pipeline` |
| **技术分析师** | 事实校验、EK-v2 证据核验与决策门禁 | `src/pipeline/steps/fact_check.rs`、`finalize.rs`、`src/evidence_kernel/` | `evidence_chain`、`ideas.analysis` | `cargo test --lib pipeline::steps::finalize` |
| **技术分析师** | 特征卡、3-Sheet BOM Excel 与工程归档 | `src/routes/feature_cards.rs`、`src/pipeline/steps/deliverables.rs` | `feature_cards`、`/api/ideas/:id/archive/zip` | `cargo test --lib pipeline::steps::deliverables` |
| **技术分析师** | FreeCAD 3D 建模与实验沙箱安全门禁 | `src/cad.rs`、`src/experiment/sandbox.rs` | `/api/cad/*` (`INNOFORGE_ALLOW_HOST_EXEC`) | `cargo test --lib experiment` |
| **专利代理师** | 结构化专利摘录（100%保留权要+选段） | `src/ai/patent.rs` (`structured_patent_excerpt`) | `/api/ai/claims`、`/api/ai/compare*` | `cargo test --lib ai::patent::structured_excerpt_tests` |
| **专利代理师** | 权利要求树编辑与第 26.4 条合规自检 | `src/ai/claim_validator.rs`、`src/routes/idea_ops.rs` | `POST /api/ideas/:id/claims/validate` | `cargo test --lib ai::claim_validator` |
| **专利代理师** | 批量 FTO 侵权风险扫描与规避设计 | `src/ai/fto_scanner.rs`、`src/routes/idea_reports.rs` | `POST /api/ideas/:id/fto-scan` | `cargo test --lib ai::fto_scanner` |
| **专利代理师** | OA 5 阶段答复管线、三维矩阵与修订 `.docx` | `src/ai/response_pipeline.rs`、`src/ai/oa_matrix.rs`、`src/routes/oa_export.rs` | `GET /api/oa/:case_id/export/docx` | `cargo test --lib ai::oa_matrix` |
| **专利代理师** | 多轮 OA 观点漂移与禁止反悔硬门禁 | `src/ai/oa_drift.rs`、`src/ai/pre_submission_check.rs` | `oa_rounds`、`/api/oa/:case_id/rounds/diff` | `cargo test --lib ai::oa_drift` |
| **学术论文助手** | 真实 DOI `.bib`、仿真 SVG 与 LaTeX/Word 论文包 | `src/pipeline/steps/deliverables.rs`、`src/routes/idea_reports.rs` | `GET /api/ideas/:id/paper/latex-bundle` | `cargo test --lib routes::idea_reports` |
| **前端与门禁** | 中英双语词典与全局 `DOMPurify` 保护 | `static/i18n.js`、`templates/*.html` | `docs/functions-manifest.json` | `node scripts/check_html_functions.mjs` |
| **安全与守卫** | 案件隐私红线、整顿令标记与裸切片守卫 | `tests/no_case_data_in_repo.rs`、`tests/no_unsafe_byte_slice.rs` | 全仓 git 跟踪文件 | `cargo test --test no_case_data_in_repo` |
