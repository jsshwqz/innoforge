# 类型迁移映射表（T1.1 施工图纸）

> 2026-08-15 机械提取自源码。**执行 agent 按本表落位，不自行发明归属**；发现表外散落类型时按文末裁决规则处理并回填本表。
> 归属总规则：**被 ≥2 个 crate 引用的领域类型进 `crates/types`；单 crate 内部类型留在原 crate**。
>
> **2026-09-20 T1.1（缩水版）落地回填**：本表原以「先拆 crates/、再迁类型」为前提；T0.x 阶段仓库仍是单 crate
> （Cargo.toml 只有一个 `[lib]` + 两个 `[[bin]]`），无 `crates/types` 可落。故本表全部 `crates/types/src/*.rs`
> 去向按等价原则落在 **`src/types/*.rs`**（同文件名、同归属、同规则），未来 T1.2/T3.x 抽 crate 时整目录平移即可。
> 详见文末 §6。

## 1. src/patent.rs 的 29 个类型（现状：无域边界的杂物桶）

### CAD 族 → `crates/types/src/cad.rs`（T1.1 未迁：见 §6 裁决 4）
| 行号 | 类型 |
|---|---|
| L10 | CadContextKind |
| L27 | CadValidation |
| L36 | CadArtifact |
| L52 | CadDrawRequest |
| L63 | CadAvailability |
| L71 | CadStatus |
| L77 | CadDrawResponse |

### 专利核心与检索族 → `crates/types/src/patent.rs`
Patent(L85)、FetchPatentRequest(L352)、ImportRequest(L358)、LegalStatusResult(L414)、LegalEvent(L426)
**✅ 已落 `src/types/patent.rs`**（含 `canonical_patent_key` 与其 4 个单测随类型同迁）。

### 检索族 → `crates/types/src/search.rs`
SearchType(L182)、SearchRequest(L191)、**SearchResult(L218)** ⚠️、CategoryGroup(L232)、PatentSummary(L238)
**✅ 已落 `src/types/search.rs`**；⚠️ 双定义按 §4 第 1 项以「改名 `PipelinePatentHit`」方式销账。

### 创意域 → `crates/types/src/idea.rs`
Idea(L365)、IdeaSubmitRequest(L382)、TextAttachment(L390)、IdeaChatRequest(L396)、IdeaSummary(L513)、FeatureCard(L437)、CreateFeatureCardRequest(L460)、ClaimNode(L482)、ClaimType(L496)、TechnicalFeature(L504)
**✅ 已落 `src/types/idea.rs`**。

### AI 对话族 → `crates/types/src/chat.rs`
AiChatRequest(L282)、AiResponse(L347)、ChatMessage（现居 db/chat.rs:11，一并迁入）
**✅ 已落 `src/types/chat.rs`**（`SYSTEM_PROMPT_PRESETS` / `effective_system_prompt()` 随 AiChatRequest 同迁）。

## 2. src/pipeline/context.rs 的 20 个类型

| 行号 | 类型 | 去向 | 理由 | 状态 |
|---|---|---|---|---|
| L10 | AiCostRecord | **types/cost.rs** | 被 db/cost.rs 反向引用（T4.2 解依赖的关键） | ⏸ 留原处：`types/cost.rs` 是第五个域，超出 T1.1 四域范围（见 §6 裁决 5） |
| L27 | ReferenceChunk | types/search.rs | rag/db 双方使用 | ✅ `src/types/search.rs` |
| L222 | Evidence | **types/idea.rs** | 被 db/evidence.rs 反向引用 | ✅ `src/types/idea.rs` |
| L248 | ResearchState | **types/idea.rs** | 被 db/research_state.rs 反向引用 | ✅ `src/types/idea.rs` |
| L142 | PipelineProgress | pipeline crate 内部 | 仅 SSE 推流用 | 留原处（范围外） |
| L114 | **SearchResult（重名第二定义）** | **消灭** | 字段并入 types/search::SearchResult 或改名 PipelinePatentHit；T1.1 验收项 | ✅ 已消灭：改名 `PipelinePatentHit` 落 `src/types/search.rs`（见 §6 裁决 2） |
| 其余 | SimilarityEntry/RankedMatch/PriorArtCluster/Contradiction/ScoreBreakdown/StepResult/StepStatus/DimensionInsight/DeepReasoningResult/AgentOutput/ReflectionResult/DebateResult/ExperimentResult/PipelineContext | pipeline crate 内部 | 单 crate 使用 | 留原处 |

## 3. 配置与状态类型

| 现位置 | 类型 | 去向 |
|---|---|---|
| routes/mod.rs:64 | AppConfig（10 个 per-provider Key 字段族） | crates/config（T1.2），同时按 T2.4 表驱动化 |
| routes/mod.rs | AppState / PipelineChannelEntry | crates/server/state.rs（T3.0 后） |

## 4. 已知同形异名/重复实现对照（合并时逐条销账）

| # | 内容 | 销账任务 | 状态 |
|---|---|---|---|
| 1 | SearchResult 双定义（patent.rs:218 vs context.rs:114） | T1.1 | ✅ 2026-09-20 销账：改名 `PipelinePatentHit`，全仓 `grep -c "pub struct SearchResult"` = 1（src/types/search.rs） |
| 2 | TF-IDF 三实现（vector/mod.rs / search.rs 私有 / rag/chunker） | T2.2 重写合一 | 未开工 |
| 3 | contains_cjk ×2（search.rs:1026 / steps/search.rs:20） | T2.2 归并 text_util | 部分收敛：MA1 抽出 `search::relevance::contains_cjk` 供 routes 侧复用，steps/search.rs 版留 T2.2 |
| 4 | Jaccard ×2（text_util pub 版 / idea.rs:1700 私有复刻） | T3.1 新写时用 text_util | 未开工 |
| 5 | SerpAPI 客户端 ×3（steps/search.rs:100 / ai.rs:84 / patent.rs:67） | T2.1 | 部分收敛：MA1 已把 routes/search.rs 的调用链抽为 `SerpApiProvider`；其余两处副本属 T2.1，未开工 |
| 6 | 服务商知识三处（AppConfig 字段族 / settings provider_db_key / idea.rs 注册表）+ settings.html 前端预设表 | T2.4 | 未开工 |

## 5. 全部公共类型须带 derive

新增/迁移的 struct/enum 统一 `#[derive(Debug, Clone, Serialize, Deserialize)]`（AGENTS.md §2.2）；带数值比较需求的追加 PartialOrd 等，不得省略 Debug/Clone。
**✅ T1.1 已核查**：src/types/ 下 27 个类型全部满足（原样迁移，derive 未增删）；`pipeline/context.rs::SearchResult`（迁前无 Clone）是唯一例外，改名 `PipelinePatentHit` 时按本条补齐 `Debug, Clone, Serialize, Deserialize`。

## 6. T1.1（缩水版）落地记录与裁决

**范围**：按 M-A 开工包指令，只做 search / patent / chat / idea 四域的公共类型地基，不做全量拆分。

| 落位 | 内容 | 行数 |
|---|---|---|
| `src/types/mod.rs` | 四域声明 + 门面说明 | 33 |
| `src/types/patent.rs` | Patent、FetchPatentRequest、ImportRequest、LegalStatusResult、LegalEvent、canonical_patent_key | 166 |
| `src/types/search.rs` | SearchType、SearchRequest、SearchResult、CategoryGroup、PatentSummary、PipelinePatentHit、ReferenceChunk | 132 |
| `src/types/chat.rs` | AiChatRequest、SYSTEM_PROMPT_PRESETS、AiResponse、ChatMessage | 87 |
| `src/types/idea.rs` | Idea、IdeaSubmitRequest、TextAttachment、IdeaChatRequest、IdeaSummary、Evidence、ResearchState、FeatureCard、CreateFeatureCardRequest、ClaimNode、ClaimType、TechnicalFeature | 185 |
| `src/types/serde_snapshot.rs` | 16 个「字段逐字不变」快照测试（验收证据，见下） | 715 |

**行为保持证明（两阶段取证）**：
1. 迁移**前**先写 `src/types/serde_snapshot.rs`，对当时的 `src/patent.rs` / `src/db/chat.rs` / `src/pipeline/context.rs` 定义跑绿（快照期望值因此必然出自旧代码）；
2. 迁移定义后重跑同一文件，仍 16/16 绿；
3. 断言三层：`json_field_names` 比字段名集合（serde_json 未开 `preserve_order`，`to_value` 落 BTreeMap 会按字母序，故字段名字段序无关）+ `assert_exact_json` 逐字节比字符串（含 key 输出顺序，覆盖对外接口的三个类型）+ `assert_roundtrip_stable` 反序列化再序列化稳定；
4. 默认值语义一并固化（`Patent::default()` 的 `citations == ""` 而非 `"[]"`、`SearchResult` 的 `Option<f64>` 默认 `None` 输出 `null`），避免「迁移顺手改默认值」。

**兼容门面**：`src/patent.rs` 保留 CAD 族并 `pub use crate::types::{chat,idea,patent,search}` 全量再导出；
`src/db/chat.rs`、`src/pipeline/context.rs` 就地 `pub use` 迁移后类型。
收益：约 20 处 `use crate::patent::*` 与集成测试 `innoforge::patent::*` 零改动通过。
副作用与修复：main.rs 原以 `mod patent;`（私有）声明，门面 `pub use` 在 bin 里被判 unused_imports → 改 `pub mod patent;`（与 lib.rs 对齐，已注释说明）。

**裁决**：
1. `crates/types` → `src/types`：仓库尚未拆 crate，按等价目录名落位（顶部说明）。
2. SearchResult 双定义取「改名 `PipelinePatentHit`」而非「字段并入」：两形状字段集不同（旧 pipeline 版无 `Clone`、无 `source`/`provider` 语义），合并会改变 `/api/search` 与 `/api/search/online` 的 JSON，违反「API 形状零变化」。改名后编译期强制暴露全部使用点（steps/search.rs 7 处、context.rs 2 处），并补 `search_cache` 历史 JSON 仍可反序列化的回归测试。
3. `CreateFeatureCardRequest` 未补 `Clone`、`ClaimNode/ClaimType/TechnicalFeature` 保留 `#[allow(dead_code)]`：均为原样迁移，改动属新需求，另案。
4. CAD 族不迁：不在四域范围。
5. `AiCostRecord` 不迁：去向 `types/cost.rs` 属第五域，且其价值在 T4.2 解 db 反向依赖时兑现。
6. 表外散落类型：本次未发现四域内未登记的公共类型；`ChatMessage` 已在 §1 预登记。
