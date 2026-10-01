# M-C 施工规格书 — 全流程贯通与交付

> 日期：2026-10-01  
> 依据：`task-breakdown.md` M-C 段 + `2026-09-28-mb-construction-spec.md` §21 接线走读结果 + §4 开放项  
> 前置：M-B ✅（v0.8.0，六包全绿，§21 接线走读通过）

## 0. 里程碑目标

**用户视角一句话**：研发用户从创意萌发到专利答复书，全流程跑一遍，每一步的结论都有出处、编造会被拦、检索结果有全文——不需要任何付费 Key。

**双轨意义**：
- MC1+MC2 是**能力贯通**——把 M-B 造好的零件（RAG 切片、出处标注、事实核查）接到创意流水线和 OA 链路里，让用户真正用得上
- MC3+MC4+MC5 是**交付收口**——安全底线、文档同步、真实冒烟，从"功能完成"到"可交钥匙"

## 1. 现状锚点（2026-10-01 实测）

### MC1 现状：创意流水线检索

- `pipeline/steps/search.rs:137 search_patents` — 当前用本地 FTS + SerpAPI，**未接 M-A 多源检索**，**未接 M-B 全文富化**
- `pipeline/steps/prior_art_cluster.rs` — 按 title 聚类，**未用 RAG 切片**
- `pipeline/steps/analysis.rs:325 enrich_top_n` — M-B2 已接，但只在深度分析路径，**创意流水线主路径未调**
- `pipeline/steps/analysis.rs:393 retrieve_rag_chunks` — M-B3 已接，同上
- `routes/idea.rs` — idea handler 通过 `PipelineContext` 驱动流水线，检索步骤由 `PipelineStep::SearchPatents` 枚举派发

### MC2 现状：OA 链路核查

- `routes/ai.rs:1530 check_oa_analysis` — OA 响应路径已接 M-B1 事实核查 ✅
- `routes/ai.rs:1546 api_ai_oa_generate_response_letter` — 答复书生成，**未接 fact_check**
- OA 讨论路径（`api_ai_oa_discussion_response` 等）— **未接 provenance**
- `pipeline/steps/finalize.rs:27 run_fact_check` — pipeline 末尾已接 fact_check ✅
- `pipeline/steps/finalize.rs:33 run_provenance_pipeline` — pipeline 末尾已接 provenance ✅

### MC3 现状：innerHTML 安全

- `templates/` 下 `innerHTML` 赋值：254 处
- `escapeHtml`/`sanitize`/`textContent`：403 处
- 需审计：254 处中哪些涉及用户输入路径（非纯 AI 生成或静态模板）

### M-B §4 开放项移交

| # | 开放项 | M-C 处置建议 |
|---|--------|-------------|
| 5 | `deep_analysis_simple` 零调用点死码 | MC1 顺手评估：是 quick 模式降级路径还是可删 |
| 6 | `patents_embedding`/`patent_chunks` 全表回填 | MC4 登记为已知限制，不派包（需单独设计分批方案） |
| 7 | 展示型截断 `safe_truncate*` 散落点 | MC4 登记为技术债，不影响功能 |
| 8 | `idea.html:456` 硬编码中文证据文案转 i18n | MC3 顺手修（MC3 本就动 templates） |
| 9 | `check_amendments` LLM 判定与 `fact_check` 确定性判定并存 | MC2 评估是否合并 |
| 10 | 浮点求和顺序抖动（1 ulp） | 不动，属既有行为 |

## 2. 逐包定义

### MC1 — 检索↔创意验证打通

**范围**：创意流水线的 `SearchPatents` 步骤改走 M-A 多源检索 + M-B 全文富化；`PriorArtCluster` 步骤引入 RAG 切片上下文。

**必做**：
1. `search_patents` 改调 `routes/search.rs` 的多源检索逻辑（或提取为共享函数），使创意流水线复用 M-A 的在线降级链 + 本地兜底
2. `enrich_top_n` 调用从 `analysis.rs` 提到 `search_patents` 之后（检索完即富化，而非等到深度分析）
3. `PriorArtCluster` 聚类时，对每个 cluster 的 top-1 专利调 `retrieve_chunks`，将切片摘要附到 cluster 上
4. 创意报告引用使用 MB3 的 `{ref_no, patent_id, chunk_id}` 形状，不加 idea 专属字段

**不做**：
- 不改 `SearchProvider` 端口定义（M-A 已定）
- 不改 `rag/retriever.rs` 函数签名（M-B 已定）
- 不加新 DB 表

**验收**：
- 创意流水线跑完，`PipelineContext.patent_results` 里至少 1 条带 `full_text_available: true`
- `PriorArtCluster` 结构体新增 `chunks: Vec<ChunkRef>` 字段，非空时有切片摘要
- 无 Key 环境下创意报告里出现专利原文片段引用（≥1 条，不要求 ≥5，创意检索命中数通常少于深度分析）

**红线**：`Cargo.*` 零 diff、`migrations/` 零 diff、`e2e_test.mjs expectedPasses` 只增不减

### MC2 — OA 链路复用核查

**范围**：OA 讨论路径 + 答复书生成路径接上 M-B1 fact_check + M-B4 provenance。

**必做**：
1. `api_ai_oa_generate_response_letter` 生成答复书前调 `check_oa_analysis`，致命档拒绝生成并返回原因
2. OA 讨论路径（`api_ai_oa_discussion_response` 等）的 AI 输出走 `check_oa_analysis`，结果附到讨论消息的结构化字段
3. 答复书引用使用 MB4 的 `Evidence` 结构（`source_url`/`claim_number`），不新建引用格式
4. 评估 `check_amendments`（`ai.rs:2000`）与 `check_oa_analysis` 是否合并——若合并，`check_amendments` 改调 `check_oa_analysis`；若不合并，登记理由

**不做**：
- 不改 `check_oa_analysis` 函数签名（M-B 已定）
- 不改 `Evidence` 结构体定义（M-B 已定）
- 不改 `fact_check.rs` 既有测试

**验收**：
- OA 讨论中 AI 编造法条时，讨论消息带 `fact_check.warnings` 非空
- 答复书生成时若 fact_check 致命档触发，返回结构化错误而非生成含编造的答复书
- `check_oa_analysis` 既有测试零改动仍绿

**红线**：`Cargo.*` 零 diff、`migrations/` 零 diff

### MC3 — innerHTML 高危审计

**范围**：仅安全底线项——254 处 innerHTML 中涉及用户输入路径的，改为显式 sanitize 或 textContent。

**必做**：
1. 枚举 254 处 `innerHTML` 赋值，分类：① 纯静态模板 ② AI 生成内容 ③ 用户输入路径
2. 第③类改为 `escapeHtml` + `innerHTML` 或纯 `textContent`
3. `idea.html:456/459-462` 硬编码中文证据文案转 i18n（M-B §4 开放项 8）
4. 同步 `docs/functions-manifest.json` 基线（`--refresh`）

**不做**：
- 不做前端组件化重构（v3 归档任务）
- 不改既有 e2e 用例（只增不删）

**验收**：
- 用户输入路径的 innerHTML 零处无 sanitize（grep 断言）
- `node check_html_functions.mjs` 退出 0
- `e2e_test.mjs` 60/60（或新增后 N/N）
- zh/en 键数一致

**红线**：`Cargo.*` 零 diff、`migrations/` 零 diff

### MC4 — 文档与版本

**范围**：API/ARCHITECTURE/AGENTS 同步实际形态；CHANGELOG 定稿；版本升 MINOR。

**必做**：
1. `AGENTS.md` 同步当前模块结构（`src/rag/`、`src/pipeline/steps/provenance.rs`、`src/pipeline/steps/fact_check.rs` 等新模块）
2. `ARCHITECTURE.md`（若存在）同步 pipeline 全步骤清单
3. CHANGELOG `[Unreleased]` 标题改为版本号 + 日期
4. `Cargo.toml` version 0.8.0 → 0.9.0（MINOR）
5. M-B §4 开放项 6/7/10 登记为已知限制/技术债

**验收**：
- 文档与代码零漂移（模块清单、函数签名逐条核对）
- CHANGELOG 措辞不越界（不称语义检索/已消除幻觉）

### MC5 — 交钥匙最后一公里

**范围**：真实冒烟 + 文档复核 + tag 演练。

**必做**：
1. 真实冒烟：无 Key 环境下跑通完整流程——新建创意 → 检索 → 深度分析 → OA 讨论 → 答复书，确认全文切片引用、出处标注、核查处置、决策段四样东西可见
2. 补中文检索场景冒烟用例（M-A 遗留）
3. 文档复核：README/AGENTS/CHANGELOG 逐页读，确认无过期信息
4. tag v0.9.0 演练：打 tag、推送、确认 CI 绿

**验收**：
- 冒烟报告含截图或文本取证
- tag v0.9.0 在远端存在且 CI 绿

## 3. 依赖与施工序

```
MC1（检索贯通）─┐
                ├→ MC4（文档版本）→ MC5（交钥匙）
MC2（OA 核查） ─┘
MC3（innerHTML）─→ MC4（独立，可并行）
```

- MC1 和 MC2 可并行（改不同文件）
- MC3 独立，可与 MC1/MC2 并行
- MC4 必须在 MC1+MC2+MC3 全合并后
- MC5 必须在 MC4 后

**建议施工序**：MC1 → MC2 → MC3 → MC4 → MC5（串行，降低并行冲突风险）

## 4. 红线

- `Cargo.*` 零 diff（MC1/MC2/MC3 均不碰依赖）
- `migrations/` 零 diff（不加新 DB 表）
- `e2e_test.mjs expectedPasses` 只增不减
- `fact_check.rs` 既有测试零删改
- `truncate_for_ai` 既有用例零删改
- 无新增 `unwrap()/expect()` 在生产路径
- 无真实 SerpAPI/EPO 调用（无 Key 环境）

## 5. M-B §4 开放项处置汇总

| # | 开放项 | 处置 | 落地包 |
|---|--------|------|--------|
| 5 | `deep_analysis_simple` 死码 | MC1 评估后决定删或保留 | MC1 |
| 6 | 全表回填 | 登记已知限制，不派包 | MC4 |
| 7 | 展示截断散落点 | 登记技术债，不派包 | MC4 |
| 8 | 硬编码中文文案转 i18n | 顺手修 | MC3 |
| 9 | LLM/确定性判定并存 | MC2 评估，登记理由 | MC2 |
| 10 | 浮点抖动 | 不动 | — |

## 6. 版本规划

| 节点 | 版本 | 动作 |
|------|------|------|
| M-B 收口 | v0.8.0 ✅ | 已打 tag |
| MC1+MC2 合并 | — | 不单独升版 |
| MC3 合并 | — | 不单独升版 |
| MC4 完成 | v0.9.0 | MINOR 升 |
| MC5 完成 | v0.9.0 | tag 打在 MC4 merge commit 上 |

> 注：milestones.md 写"发 v0.1.0"是"全新版本线"表述，当前已在 0.x 线上，按语义化续升 MINOR 到 0.9.0。
