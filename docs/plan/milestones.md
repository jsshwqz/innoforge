# 重构里程碑 / Milestones

> 判定标准全部为客观可验证条件；日期为相对工期（净工作日），非日历承诺。

## M0 — 开工就绪（Phase 0 完成）
- git status 干净；预处理修复已入库（冲突标记/CAD 接线/cad.rs 清理）
- **本地门禁全绿**：fmt/clippy/test（2026-08-15 实测曾三红：fmt 漂移、clippy 32 错、磁盘满致链接失败——T0.10 恢复）
- **双远端 CI 对最新提交全绿**：推送 main 与 dev（当前领先远端 60+/36 提交未经 CI——T0.6）
- dev 包含 main 全部提交（用户决策：直接在 dev 执行重构）
- 磁盘水位 ≥5GB（target/ 曾占 8.4GB，cargo clean 已释放）
- SCHEMA_VERSION=23 且空库初始化验证通过
- 文档漂移清零（AGENTS.md 双入口条款/v11/15步/AI超时 全部修正）

## M1 — 地基成型（Phase 1 完成）
- SearchResult 唯一定义；types/ 领域模块就位
- AppConfig/AppState 脱离 routes/mod.rs；common.rs 无反向依赖
- 全部路由统一 AppError 错误契约（含错误码）
- 生产路径 unwrap/expect = 0

## M2 — 端口贯通（Phase 2 完成）⭐ 核心里程碑
- SearchProvider / Embedder / PdfExtractor / ProviderMode adapter 四大端口落地
- SerpAPI 实现 ×3→×1；TF-IDF ×3→×1（char-boundary 安全版）；服务商知识 ×3→×1（表驱动）
- ai/client.rs 语义拆为 FailoverClient + 三 adapter，容灾循环测试 ≥8 个
- **意义**：此后所有新写都是"把调用方接到已有端口上"，不再产生新轮子

## M3 — 新骨架成型（Phase 3+4 完成）
- **workspace 化完成**：crates/{types,config,db,ai,search,pipeline,server} 就位，src/routes/ 整体消失
- routes 万能层被薄 handler + service 层替代；idea 域测试 ≥15 个
- rag/vector/fact_check 三块死代码有明确归宿并接线【裁决已定】
- db 层无 pipeline 类型反向依赖；Pipeline 写入侧强类型化

## M4 — 前端收敛（Phase 5 完成）
- escapeHtml 等 ×7 重复定义清零；i18n.js 只管翻译
- OA 页与 idea 页 JS 完成分段外置；innerHTML 255 处审计闭环、高危清零
- 函数完整性基线已 --refresh 并随代码提交

## M5 — 发布 v0.1.0（Phase 6 完成）
- 覆盖率数字落档（对比重构前）
- e2e ≥60 项；文档与代码零漂移
- CHANGELOG 完整、双远端 CI 绿、tag v0.1.0 发布（全新版本线，用户决策；若实意为 1.0.0 仅改此处）
- **交付边界确认**：delivery-boundary.md 的"最后一公里"三项（真实冒烟/用户文档复核/tag 演练）执行完毕——这是从"重构完成"到"可交钥匙"的必经步骤

---

## 产品轨里程碑（与上方 M0–M5 重构轨编号不通用）

> 上方 `M0–M5` 是重构轨（Phase 0-6）；产品轨记在 `task-breakdown.md` 的 `M-A / M-B / M-C`。本节只补 M-B 的客观收口判据，判定标准全部可验证，措辞不超出规格书 §9 口径表。

### M-A — 检索员上岗（✅ 2026-09-28 收口）
- PR #20→#27 全合并，五端对齐 tip `1af09fd`；门禁基线 **721 = lib 335 + bin 337 + 集成 49**，`e2e_test.mjs` 60/60
- 独立查证成立：`relevance.rs:246` 先入库后闸门 ⇒ 「结果非注入」声明成立；「在线命中→本实例入库→断网复检」全链路取证闭合（通道级证据，OS 级断网未注入已如实入文）
- 四项开放项保留登记（英文机构名 assignee / 付费凭证冒烟 / OS 级断网 / 错误信息润色），不阻断收口

### M-B — 可信深度分析（🚧 进行中：MB0 待审计（PR #28 / CI 三绿），MB1–MB5 规格就绪待派）

**里程碑目标（用户视角一句话）**：研发用户跑一次深度分析，能在**没有付费 Key** 的条件下拿到「有出处的结论」——引用的每句话能反查回库内专利原文片段，没有出处的话会被标成推测，编造法条/页码会被拦下来。

**逐包客观判据（合并即达成，缺一不得计入收口）**：

| 包 | 判据（可验证条件，非形容词） |
|---|---|
| MB0 | `grep -rn "cleaned\[i\.\.i" src/` 为空；`db/patent.rs::insert_patent` 为 embedding 与 chunks 的唯一挂点；`:memory:` 实例入库一条中文专利 ⇒ `count_embeddings()>0` 且 `/api/search/vector` 的 `vector_count` 由 0 翻正；红→绿锚两条测试绿 |
| MB2 | 同 URL 抓取实现全仓唯一；冷却判据在 `reqwest` 发起**之前**；预置冷却后富化零出网（有函数级证据）；出参含 `enrichment`/`full_text_available_count` 且空则整键省略；写入路径字符数不减 |
| MB3 | `patent_chunks` 由入库自动生成（三档 source_type 齐全）；深度 prompt 内 `### [引用 ` 计数 ≥5 且 **distinct patent_id ≥5**，逐条 `get_chunk` 可反查；`retrieve_chunks_by_keyword` 空占位与 `build_citations` 孤儿二选一销账；`search_chunks` SELECT 缺列缺陷修复有红→绿用例 |
| MB4 | 抽查 10 条结论 ≥9 条有源且编号可反查至 `patents`；决策段 `stance + ≥3 条 rationale` 存在；无源结论走结构化标记而非字符串补丁；`templates` + `static` 改动落在 §2.1 授权范围且 zh/en 键数相等；e2e + HTML 扫描 + ESLint 三绿 |
| MB1 | 诱导用例集（法条/页码/URL/无来源数字 × OA 与创意双路径）100% 被标记或拒绝；`not_applicable` 不计为通过；两条路径共用同一判定实现；现有 15 条 fact_check 单测零删改 |
| MB5 | `ai.rs:1698-1699` 字节切片消失且有中文 panic 红→绿锚；§1.5③ 六个静默截断入口全部改为超限报错；20 轮回归用例**同测断言压缩真触发**；`ai.rs` 与 `idea.rs` 两套摘要在「首轮约束不丢」上同结论或有登记理由 |

**M-B 整体收口判据**（六包全绿之外还须满足）：
1. 基线从 721 单调上升到「721 + Σ各包 2N ± 已归因跳变」，且**每一跳都能逐条归因**（禁止靠删测试凑数，§3.1）；
2. 规格书 §9 口径表逐条落到代码注释 / UI 文案 / CHANGELOG，**无任何超出可承诺范围的表述**（尤其：向量档不得称「语义检索」）；
3. `docs/plan/2026-09-28-mb-construction-spec.md` §1.6 的 14 条校正**全部关闭**——要么被包修掉，要么移入 §4 登记不派包；
4. 无 Key 环境下跑通一次用户视角全流程（规格书 §11 走查剧本），产出报告里能看到全文切片引用、出处标注、核查处置三样东西；
5. CHANGELOG `[Unreleased]` 有 M-B 用户可见条目（草案见规格书 §14），版本号按语义化升 **MINOR**（新功能，非破坏性）；
6. 五端对齐 + STATUS / task-breakdown / dependency-graph 回写完成。

**冻结项（M-B 期间明确不做，解冻条件写在括号里）**：真嵌入模型与 `AiClient::embed()`（用户配 Key 或明确改路线）｜SerpAPI/EPO 真实冒烟与配额真实触发（用户配 Key）｜embedding/chunks 全表历史回填（需单独设计分批可断点方案，§4.6）｜专业 prompt 库收拢与专家模型分工（M-B 六包未覆盖，见 task-breakdown M-B 段第 3 笔账）｜IDF/语料统计补全 = MB0b（MB3 有真实切片语料后评估）。

---

### 风险提示
- M2 是整个方案的枢纽：若 T2.3（AI client 拆分）工作量超预期，允许裁剪为"仅抽 StreamParser + 保持容灾循环原样"，其余端口照常——M3 起不受影响。
- M3 的死代码决断（T4.1）是唯一需要用户在方案确认时一并拍板的事项。
