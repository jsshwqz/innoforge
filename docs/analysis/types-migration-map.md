# 类型迁移映射表（T1.1 施工图纸）

> 2026-08-15 机械提取自源码。**执行 agent 按本表落位，不自行发明归属**；发现表外散落类型时按文末裁决规则处理并回填本表。
> 归属总规则：**被 ≥2 个 crate 引用的领域类型进 `crates/types`；单 crate 内部类型留在原 crate**。

## 1. src/patent.rs 的 29 个类型（现状：无域边界的杂物桶）

### CAD 族 → `crates/types/src/cad.rs`
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

### 检索族 → `crates/types/src/search.rs`
SearchType(L182)、SearchRequest(L191)、**SearchResult(L218)** ⚠️、CategoryGroup(L232)、PatentSummary(L238)

### 创意域 → `crates/types/src/idea.rs`
Idea(L365)、IdeaSubmitRequest(L382)、TextAttachment(L390)、IdeaChatRequest(L396)、IdeaSummary(L513)、FeatureCard(L437)、CreateFeatureCardRequest(L460)、ClaimNode(L482)、ClaimType(L496)、TechnicalFeature(L504)

### AI 对话族 → `crates/types/src/chat.rs`
AiChatRequest(L282)、AiResponse(L347)、ChatMessage（现居 db/chat.rs:11，一并迁入）

## 2. src/pipeline/context.rs 的 20 个类型

| 行号 | 类型 | 去向 | 理由 |
|---|---|---|---|
| L10 | AiCostRecord | **types/cost.rs** | 被 db/cost.rs 反向引用（T4.2 解依赖的关键） |
| L27 | ReferenceChunk | types/search.rs | rag/db 双方使用 |
| L222 | Evidence | **types/idea.rs** | 被 db/evidence.rs 反向引用 |
| L248 | ResearchState | **types/idea.rs** | 被 db/research_state.rs 反向引用 |
| L142 | PipelineProgress | pipeline crate 内部 | 仅 SSE 推流用 |
| L114 | **SearchResult（重名第二定义）** | **消灭** | 字段并入 types/search::SearchResult 或改名 PipelinePatentHit；T1.1 验收项 |
| 其余 | SimilarityEntry/RankedMatch/PriorArtCluster/Contradiction/ScoreBreakdown/StepResult/StepStatus/DimensionInsight/DeepReasoningResult/AgentOutput/ReflectionResult/DebateResult/ExperimentResult/PipelineContext | pipeline crate 内部 | 单 crate 使用 |

## 3. 配置与状态类型

| 现位置 | 类型 | 去向 |
|---|---|---|
| routes/mod.rs:64 | AppConfig（10 个 per-provider Key 字段族） | crates/config（T1.2），同时按 T2.4 表驱动化 |
| routes/mod.rs | AppState / PipelineChannelEntry | crates/server/state.rs（T3.0 后） |

## 4. 已知同形异名/重复实现对照（合并时逐条销账）

| # | 内容 | 销账任务 |
|---|---|---|
| 1 | SearchResult 双定义（patent.rs:218 vs context.rs:114） | T1.1 |
| 2 | TF-IDF 三实现（vector/mod.rs / search.rs 私有 / rag/chunker） | T2.2 重写合一 |
| 3 | contains_cjk ×2（search.rs:1026 / steps/search.rs:20） | T2.2 归并 text_util |
| 4 | Jaccard ×2（text_util pub 版 / idea.rs:1700 私有复刻） | T3.1 新写时用 text_util |
| 5 | SerpAPI 客户端 ×3（steps/search.rs:100 / ai.rs:84 / patent.rs:67） | T2.1 |
| 6 | 服务商知识三处（AppConfig 字段族 / settings provider_db_key / idea.rs 注册表）+ settings.html 前端预设表 | T2.4 |

## 5. 全部公共类型须带 derive

新增/迁移的 struct/enum 统一 `#[derive(Debug, Clone, Serialize, Deserialize)]`（AGENTS.md §2.2）；带数值比较需求的追加 PartialOrd 等，不得省略 Debug/Clone。
