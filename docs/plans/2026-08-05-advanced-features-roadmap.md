# InnoForge 先进化执行计划 / InnoForge Advanced Features Execution Roadmap

> 规划日期：2026-08-05 | 基于 v0.7.4 代码审查
> Planning Date: 2026-08-05 | Based on v0.7.4 Code Review
> 目标读者：可独立接力的 AI Agent（含本对话中的子代理）
> Target Audience: Self-contained AI Agents capable of independent execution

---

## 一、项目现状速查 / Current Status

| 项目 | 值 |
|---|---|
| 版本 | v0.7.4（2026-07-17） |
| 核心架构 | Rust + Axum 0.6 / SQLite + FTS5 / 纯 HTML+CSS+JS |
| Pipeline | 16 步（code 先算 + AI 后判） |
| AI 服务商 | 8 个 provider（DeepSeek 主 / Gemini 副） |
| 搜索 | FTS5 BM25 + SerpAPI 网络搜索 |
| 前端 | 8 页面 × Linear 式深色设计 + i18n + DOMPurify |
| DB Schema | v17（22 张表） |
| 已知缺口 | 无向量搜索 / 无 RAG / 无多 Agent / 无成本追踪 / 无记忆 |

---

## 二、总路线 / Master Roadmap

```
Phase 0 — 代码修复（P0 Bugfix, 1 周）✅ 已完成
  └── ✅ sandbox 超时 + ✅ search N+1 + ✅ MCP 健壮性

Phase 1 — 基础设施增强（P0-1, 3-4 周）🔄 进行中
  ├── ✅ A. Fact-Check 接入主流程（1 周）
  ├── ⬜ B. 向量嵌入 + 混合语义搜索（2 周）
  ├── ⬜ C. AI 调用成本追踪（1 周）
  └── ⬜ D. RAG 管道（与 B 并行, 2 周）

Phase 2 — 智能升级（P1, 3-4 周）
  ├── E. 多智能体 Pipeline 升级（3 周）
  └── F. 持久化记忆系统（2 周, 与 E 并行）

Phase 3 — 高级功能（P2, 3-4 周）
  ├── G. Agentic 自主研究（3 周）
  ├── H. 实验沙箱升级（2 周）
  └── I. 专利组合智能分析（3 周）

Phase 4 — 工程化（P3, 持续）
  ├── J. 可观测性基础设施（1-2 周）
  └── K. 插件化架构（4 周, Phase 4 末期）
```

> **执行原则**：每个 Task 独立可完成，有明确验收标准；后续 Task 不依赖前置未完成的 Task（除非显式标注）。

---

## 三、Phase 0 — 代码修复（P0 Bugfix, 1 周）

### Task 0.1 — sandbox.rs 超时修复 + 临时文件迁移

**状态**: ✅ 已完成 (2026-08-05)  **优先级**: P0  **提交**: `ad1e7fe`

**问题**:
1. `_timeout` 变量被创建后从未使用，实验脚本可无限期运行
2. 使用系统临时目录 `temp_dir()`，违反 AGENTS.md 中"禁止使用系统临时目录"规范

**修改文件**: `src/experiment/sandbox.rs`、`src/common.rs`

**实施步骤**:

Step 1: 在 `src/common.rs` 中新增项目临时目录工具函数：

```rust
pub fn project_temp_dir() -> PathBuf {
    let base = PathBuf::from("data").join("runtime-temp");
    std::fs::create_dir_all(&base).expect("failed to create runtime-temp");
    base
}

pub fn new_temp_file(prefix: &str, ext: &str) -> Result<PathBuf, std::io::Error> {
    let dir = project_temp_dir();
    let name = format!("{}_{}.{}", prefix, uuid::Uuid::new_v4(), ext);
    let path = dir.join(name);
    std::fs::File::create_new(&path)?;
    Ok(path)
}
```

Step 2: 重写 `sandbox.rs`，将 `std::env::temp_dir()` 替换为 `project_temp_dir()`，使用 UUID 文件命名。

Step 3: 使用 `tokio::process::Command` + `tokio::time::timeout` 实现真正的超时控制：

```rust
use tokio::process::Command as AsyncCommand;

let timeout = Duration::from_secs(spec.timeout_secs.max(30).min(300));
let output = tokio::time::timeout(timeout, async {
    let mut cmd = AsyncCommand::new(&interpreter);
    cmd.arg(&script_path);
    cmd.env("PYTHONDONTWRITEBYTECODE", "1");
    cmd.output().await
}).await;

match output {
    Ok(Ok(child_output)) => { /* 正常完成 */ }
    Ok(Err(e)) => { /* 子进程启动失败 */ }
    Err(_) => { /* 超时：子进程已被 tokio 自动终止 */ }
}
```

Step 4: 清理临时文件（成功/失败/超时均清理）。

**验收标准**:
- [ ] sandbox.rs 不再使用 `std::env::temp_dir()`
- [ ] 实验脚本有真实超时控制
- [ ] 超时后脚本被终止
- [ ] 临时文件在成功/失败/超时后均被清理
- [ ] `cargo clippy --all-targets -- -D warnings` 通过

---

### Task 0.2 — search.rs IPC/CPC 过滤循环查询修复

**状态**: ✅ 已完成 (2026-08-05)  **优先级**: P0  **提交**: `625ac82`

**问题**: `src/routes/search.rs` 中 IPC/CPC 过滤循环逐条查询数据库，违反 AGENTS.md 2.7 禁止循环逐条查询。

**修改文件**: `src/routes/search.rs`、`src/db/patent.rs`

**实施步骤**:

Step 1: 在 `src/db/patent.rs` 中新增批量查询方法 `get_patents_by_ids(ids: &[String]) -> Result<HashMap<String, Patent>, _>`，使用 `WHERE id IN (...)` 分批次处理（每批 ≤900 条）。

Step 2: 替换 `search.rs` 中的循环查询为单次批量查询。

**验收标准**:
- [ ] 使用 `WHERE id IN (...)` 批量查询
- [ ] `cargo test --lib` 通过

---

### Task 0.3 — MCP Server 健壮性增强

**状态**: ✅ 已完成 (2026-08-05)  **优先级**: P0  **提交**: `625ac82`

**修改文件**: `src/bin/mcp-server.rs`

**实施步骤**:
1. 为外部 API 请求设置 60 秒超时
2. 添加简单速率限制（单 IP 每秒最多 5 个请求）
3. 替换所有 `unwrap_or_default()` 为受控错误返回
4. JSON-RPC 响应中正确返回 error 对象

**验收标准**:
- [ ] 无 `unwrap()`/生产路径 `unwrap_or_default()`
- [ ] 速率限制生效
- [ ] JSON-RPC 错误响应格式正确

---

## 四、Phase 1 — 基础设施增强（P0-1, 3-4 周）

### Task 1.A — Fact-Check 层接入主流程

**状态**: ✅ 已完成 (2026-08-05)  **优先级**: P0  **提交**: `c2d3c3c`
  > **发现**: Fact-Check 实际已在主流程中工作（ai.rs:1393-1394），本次修复仅为清理过时标记和注释

**问题**: `fact_check.rs` 已实现四类校验，但 `check_oa_analysis()` 从未被调用。

**修改文件**:
- `src/ai/mod.rs` — 移除 `#[allow(dead_code)]`
- `src/pipeline/steps/oa_response.rs` — 在 OA 分析后调用 fact-check
- `src/db/oa.rs` — 新增 `quality_score` 和 `fact_warnings` 列
- `src/db/migrations.rs` — 新增 v18 迁移
- `templates/office_action_response.html` — 显示质量评分和警告
- `templates/settings.html` — 新增开关

**v18 迁移**:

```sql
ALTER TABLE oa_analyses ADD COLUMN quality_score REAL DEFAULT 100.0;
ALTER TABLE oa_analyses ADD COLUMN fact_warnings TEXT DEFAULT '[]';
```

**实施步骤**:

Step 1: 在 `oa_response.rs` 中，OA 分析完成后调用 `check_oa_analysis()`，失败时降级为 `FactCheckReport::default()`。

Step 2: 将校验结果写入 DB，随 OA 分析一起保存。

Step 3: 前端显示质量评分（进度条）和警告列表；评分 < 60 时显示红色横幅。

Step 4: 设置页新增"开启 OA 事实校验"开关，通过 `app_settings` 表控制。

**验收标准**:
- [ ] `check_oa_analysis()` 在 OA 分析后被调用
- [ ] 校验结果持久化到 DB
- [ ] 前端显示质量评分和警告
- [ ] 降级不影响 OA 流程
- [ ] 设置页可开关

---

### Task 1.B — 向量嵌入 + 混合语义搜索

**状态**: ⬜ 待办  **优先级**: P0（核心功能升级）  **工作量**: 2 周

**架构**:

```
用户查询
    ├── BM25 层 (FTS5) → 按字段权重排序
    ├── 向量层 (text2vec) → 余弦相似度排序
    └── RRF 融合 (k=60) → 最终排序
```

**修改文件**:
- `Cargo.toml` — 新增 `text2vec = "0.27"`（**需用户确认**，纯 Rust CPU 推理，无需外部服务）
- `src/db/migrations.rs` — v19: `patents_embedding` 表
- `src/db/patent.rs` — 向量 CRUD
- `src/vector/mod.rs`（新建）— Embedding 计算模块
- `src/pipeline/steps/search.rs` — 搜索后计算 embedding
- `src/routes/search.rs` — 新增 `/api/search/vector` 端点
- `src/main.rs` + `src/lib.rs` — 双注册新端点

**v19 迁移**:

```sql
CREATE TABLE IF NOT EXISTS patents_embedding (
    patent_id TEXT NOT NULL UNIQUE,
    embedding BLOB NOT NULL,
    model_name TEXT NOT NULL DEFAULT 'text2vec-base-chinese',
    created_at TEXT NOT NULL,
    FOREIGN KEY (patent_id) REFERENCES patents(id) ON DELETE CASCADE
);
```

**核心实现**:

```rust
// RRF 融合：rank_fusion_result = Σ 1 / (k + rank)
fn rrf_fuse(bm25_ranked: &[(String, f64)], vector_ranked: &[(String, f64)], k: usize) -> Vec<(String, f64)> {
    let mut scores: HashMap<String, f64> = HashMap::new();
    for (i, (id, _)) in bm25_ranked.iter().enumerate() {
        *scores.entry(id.0.clone()).or_insert(0.0) += 1.0 / (k as f64 + (i + 1) as f64);
    }
    for (i, (id, _)) in vector_ranked.iter().enumerate() {
        *scores.entry(id.0.clone()).or_insert(0.0) += 1.0 / (k as f64 + (i + 1) as f64);
    }
    let mut sorted: Vec<(String, f64)> = scores.into_iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    sorted
}
```

**增量计算策略**:
- 导入新专利时：在 `SearchPatents` 步骤后异步计算
- 历史回填：新增 `/api/admin/embeddings/backfill` 端点

**验收标准**:
- [ ] 向量 embedding 可正常计算和存储
- [ ] 混合搜索质量优于纯 BM25
- [ ] `/api/search/vector` 端点可用
- [ ] 历史数据可批量回填
- [ ] 搜索页面无性能退化

---

### Task 1.C — AI 调用成本追踪

**状态**: ⬜ 待办  **优先级**: P0-1  **工作量**: 1 周

**修改文件**:
- `src/db/migrations.rs` — v20: `ai_cost_ledger` 表
- `src/ai/client.rs` — 解析 `usage` 字段
- `src/pipeline/context.rs` — 增加 cost 字段
- `src/pipeline/steps/finalize.rs` — 写入成本汇总
- `templates/idea.html` — 显示成本
- `templates/settings.html` — 预算上限设置

**v20 迁移**:

```sql
CREATE TABLE IF NOT EXISTS ai_cost_ledger (
    id TEXT PRIMARY KEY,
    pipeline_run_id TEXT NOT NULL,
    step TEXT NOT NULL,
    model TEXT NOT NULL,
    provider TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    estimated_cost_cents REAL NOT NULL DEFAULT 0.0,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (pipeline_run_id) REFERENCES ideas(id)
);
```

**成本模型**（在 `ai/client.rs` 维护价格表，每百万 tokens 美分）:

```
("deepseek-chat",       (0.14, 0.28))
("deepseek-reasoner",   (0.55, 2.19))
("deepseek-v4-flash",   (0.10, 0.40))
("gpt-4o",              (2.50, 10.00))
("gpt-4o-mini",         (0.15, 0.60))
```

**验收标准**:
- [ ] 每次 AI 调用后自动记录
- [ ] 前端显示本次分析总成本和 token 用量
- [ ] 超过预算时 pipeline 可终止
- [ ] `cargo test` 全通过

---

### Task 1.D — RAG 管道

**状态**: ⬜ 待办  **优先级**: P0  **工作量**: 2 周（与 1.B 并行）

**架构**:

```
用户提问 → 查询理解 → 向量检索 → 切片组装 → Prompt 组装 → AI 回答（带引用）
```

**修改文件**:
- `src/pipeline/context.rs` — 新增 `ReferenceChunk` 类型
- `src/rag/mod.rs`（新建）— RAG 管道核心
- `src/rag/chunker.rs`（新建）— 语义切片
- `src/rag/retriever.rs`（新建）— 检索器
- `src/rag/assembler.rs`（新建）— Prompt 组装器
- `src/routes/ai.rs` — 接入 RAG
- `src/routes/patent.rs` — 接入 RAG

**v21 迁移**:

```sql
CREATE TABLE IF NOT EXISTS patent_chunks (
    id TEXT PRIMARY KEY,
    patent_id TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    source_type TEXT NOT NULL,  -- 'abstract' | 'claim' | 'description'
    content TEXT NOT NULL,
    embedding BLOB NOT NULL,
    model_name TEXT NOT NULL DEFAULT 'text2vec-base-chinese',
    FOREIGN KEY (patent_id) REFERENCES patents(id) ON DELETE CASCADE
);
CREATE INDEX idx_chunk_patent ON patent_chunks(patent_id);
CREATE INDEX idx_chunk_source ON patent_chunks(source_type);
```

**验收标准**:
- [ ] 专利全文可正确切片（摘要/权利要求/说明书）
- [ ] RAG 检索 < 1 秒
- [ ] AI 回答可引用具体来源
- [ ] RAG 失败时优雅降级
- [ ] AI 聊天和专利分析均接入

---

## 五、Phase 2 — 智能升级（P1, 3-4 周）

### Task 2.E — 多智能体 Pipeline 升级

**状态**: ⬜ 待办  **优先级**: P1（架构级升级）  **工作量**: 3 周

**目标架构**:

```
Search Agent → Cluster Agent → Contradiction Agent → Evidence Agent
        │               │               │                  │
        └───────────────┴───────────────┴──────────────────┘
                            │
                            ▼
                    [Reflection Layer]
                    评估置信度 → 低分触发 Retry
                            │
                            ▼
                    [Debate Layer]
                    分歧合成为更强结论
                            │
                            ▼
                    Judge Agent → Finalize Agent
```

**修改文件**:
- `src/pipeline/context.rs` — 新增 `AgentOutput`、`ReflectionResult` 类型
- `src/pipeline/steps/reflection.rs`（新建）— 反思评估
- `src/pipeline/steps/debate.rs`（新建）— 辩论合成
- `src/orchestrator/command.rs` — 新增 `Reflect`、`Debate` 命令
- `src/orchestrator/engine.rs` — 支持新命令
- `src/pipeline/state.rs` — 扩展

**核心数据结构**:

```rust
pub struct AgentOutput<T> {
    pub content: T,
    pub confidence: f64,              // 0.0-1.0
    pub evidence_refs: Vec<String>,   // 证据引用
    pub uncertainty: Vec<String>,     // 自述不确定点
    pub self_reflection: Option<String>,
    pub duration_ms: u64,
    pub tokens_used: u64,
}

pub struct ReflectionResult {
    pub quality_score: f64,
    pub completeness_score: f64,
    pub consistency_score: f64,
    pub evidence_score: f64,
    pub needs_retry: bool,
    pub retry_reason: Option<String>,
    pub improvement_suggestions: Vec<String>,
}
```

**优先接入反思的步骤**:
- `AiDeepAnalysis`（多维推演后）
- `AiActionPlan`
- `BuildClaimTree`
- `ScoreNovelty`（交叉验证）

**验收标准**:
- [ ] AgentOutput 含 confidence/evidence_refs/uncertainty
- [ ] 反思 Agent 可评估质量
- [ ] 低分步骤自动重试（可配置阈值）
- [ ] 辩论 Agent 合成分歧结论
- [ ] Pipeline 执行时间增加 ≤50%

---

### Task 2.F — 持久化记忆系统

**状态**: ⬜ 待办  **优先级**: P1  **工作量**: 2 周（与 2.E 并行）

**修改文件**:
- `src/db/migrations.rs` — v22: `idea_memory` 表
- `src/db/idea.rs` — memory CRUD
- `src/pipeline/context.rs` — ResearchState 扩展
- `src/pipeline/steps/finalize.rs` — 写入记忆
- `templates/idea.html` — 记忆面板
- `src/routes/idea.rs` — 新增 `/api/idea/{id}/memory`

**v22 迁移**:

```sql
CREATE TABLE IF NOT EXISTS idea_memory (
    id TEXT PRIMARY KEY,
    idea_id TEXT NOT NULL,
    concept_name TEXT NOT NULL,
    concept_type TEXT NOT NULL,  -- 'domain_concept' | 'decision' | 'pattern' | 'question'
    content TEXT NOT NULL,
    confidence REAL DEFAULT 0.5,
    source_step TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (idea_id) REFERENCES ideas(id) ON DELETE CASCADE
);
```

**记忆提取流程**: pipeline 完成后，AI 自动提取领域概念、决策、模式、未解决问题 → 写入记忆表。
**记忆注入流程**: pipeline 启动时，检索相关记忆注入上下文。

**验收标准**:
- [ ] 每次 pipeline 完成后自动提取记忆
- [ ] 下次 pipeline 自动注入既往记忆
- [ ] 前端可浏览和搜索记忆
- [ ] 记忆按类型分类展示

---

## 六、Phase 3 — 高级功能（P2, 3-4 周）

### Task 3.G — Agentic 自主研究工作流

**状态**: ⬜ 待办  **优先级**: P2  **工作量**: 3 周

**核心流程**: 规划 → 并行搜索 → 深度分析 → 自我评估 → 补全搜索 → 合成报告

**修改文件**:
- `src/routes/agent.rs`（新建）
- `src/agent/mod.rs`（新建）
- `src/agent/planner.rs`（新建）
- `src/agent/researcher.rs`（新建）
- `src/agent/evaluator.rs`（新建）
- `templates/idea.html` — 新增入口

**核心 API**:

```
POST /api/agent/research (SSE)
{
  "objective": "调研锂电池固态电解质",
  "depth": "standard",
  "max_rounds": 5
}

SSE Events:
event: progress → {"phase": "planning"|"searching"|"analyzing"|"evaluating"}
event: complete  → {"report": "..."}
```

**验收标准**:
- [ ] 多轮研究可在 < 10 分钟完成
- [ ] 覆盖度评估可识别盲区
- [ ] 最终报告结构化输出，含来源引用
- [ ] 支持 SSE 实时进度推送

---

### Task 3.H — 实验沙箱升级（Notebook 模式）

**状态**: ⬜ 待办  **优先级**: P2  **工作量**: 2 周

**升级方向**: 多 Cell 执行 + 变量持久化 + 图表输出 + 参数优化

**验收标准**:
- [ ] 多 Cell 逐步执行
- [ ] Cell 间变量持久化
- [ ] matplotlib 图表可显示为 base64 图片
- [ ] 参数优化引擎可自动搜索最优参数

---

### Task 3.I — 专利组合智能分析

**状态**: ⬜ 待办  **优先级**: P2  **工作量**: 3 周

**功能列表**:

| 功能 | 描述 | 工作量 |
|---|---|---|
| 引用网络可视化 | D3.js 力导向图展示引用关系 | 2 天 |
| 竞争格局分析 | 按申请人/IPC 分析竞争态势 | 2 天 |
| 技术空白分析 | 识别未覆盖的 IPC 交叉组合 | 3 天 |
| 技术生命周期曲线 | 申请量时间序列 + 阶段判断 | 2 天 |
| FTO 自由实施分析 | 评估侵权风险 | 3 天 |
| 专利组合仪表盘 | 首页新增组合分析入口 | 2 天 |

**新增 API**:
- `GET /api/patent/{id}/citations`
- `GET /api/patent/{id}/cited_by`
- `GET /api/analysis/landscape`
- `GET /api/analysis/gaps`
- `GET /api/analysis/lifecycle/{ipc}`
- `POST /api/analysis/fto`

**验收标准**:
- [ ] 引用网络可视化可交互展示
- [ ] 竞争格局提供聚合视图
- [ ] 技术空白识别 IPC 交叉空白点
- [ ] FTO 提供风险评分和证据引用
- [ ] 所有分析可导出报告

---

## 七、Phase 4 — 工程化（P3, 持续）

### Task 4.J — 可观测性基础设施

**状态**: ⬜ 待办  **优先级**: P3  **工作量**: 1-2 周

**目标**: 结构化日志 + 分布式追踪 + Prometheus 指标

**修改文件**:
- `src/common.rs` — tracing 初始化
- `src/routes/mod.rs` — request_id 中间件
- `src/routes/metrics.rs`（新建）— Prometheus 指标

**新增指标**:
- `innoforge_requests_total`（按路径+状态码）
- `innoforge_request_duration_ms`（直方图）
- `innoforge_pipeline_steps_total`
- `innoforge_ai_calls_total`（按模型）
- `innoforge_ai_tokens_total`（按模型+input/output）

**验收标准**:
- [ ] 每个 API 请求有唯一 request_id
- [ ] Pipeline 步骤可追踪
- [ ] `/api/metrics` 返回有效 Prometheus 格式
- [ ] 日志可通过 request_id 关联

---

### Task 4.K — 插件化架构

**状态**: ⬜ 待办  **优先级**: P3  **工作量**: 4 周

**核心设计**（trait + 注册表模式）:

```rust
pub trait SearchPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn search(&self, query: &SearchRequest) -> Result<SearchResult, PluginError>;
    fn supported_countries(&self) -> Vec<String>;
}

pub trait AiPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn chat(&self, messages: Vec<Message>) -> Result<AiResponse, PluginError>;
    fn models(&self) -> Vec<String>;
}
```

**验收标准**:
- [ ] 新增搜索源可通过实现 trait 注册
- [ ] 新增 AI provider 可通过实现 trait 注册
- [ ] 插件配置通过 settings 表控制

---

## 八、依赖关系图 / Dependency Graph

```
Phase 0（修复）— 全部独立
Phase 1（基础设施）— 1.A 独立 / 1.B 独立 / 1.C 可选依赖 1.B / 1.D 依赖 1.B
Phase 2（智能升级）— 2.E 依赖 1.C / 2.F 独立
Phase 3（高级功能）— 3.G 依赖 2.E / 3.H 独立 / 3.I 独立
Phase 4（工程化）— 4.J 独立 / 4.K 依赖 4.J
```

---

## 九、AI Agent 工作指引 / AI Agent Execution Guide

### 9.1 任务领取原则

1. **独立可完成**：每个 Task 独立领取执行
2. **最小化变更**：优先修改/新增文件，不做大规模重构
3. **向后兼容**：所有变更保持现有 API 兼容
4. **遵守 AGENTS.md**：所有代码修改遵守项目规范

### 9.2 执行模板

每个 AI Agent 开始 Task 时按此模板执行：

```
## Task 执行报告

### 基本信息
- Task ID: (如 1.B)
- 任务名称:
- 开始时间:
- 预计完成:

### Step 0: 读规约
- [ ] 已阅读 AGENTS.md
- [ ] 已阅读 docs/plans/STATUS.md
- [ ] 已确认不与其他 Agent 冲突

### Step 1: 理解任务
- [ ] 理解目标
- [ ] 确认修改范围

### Step 2: 现状调查
- [ ] 搜索现有实现
- [ ] 确认涉及文件
- [ ] 确认已有类型定义

### Step 3: 方案确认
- [ ] 说明修改方案
- [ ] 涉及 DB 变更已准备迁移
- [ ] 涉及新依赖已说明原因
- [ ] 涉及大范围重构已获确认

### Step 4: 实现
- [ ] 按规范编写代码
- [ ] 新增类型先查 src/patent.rs
- [ ] 新增 API 同时注册到 main.rs 和 lib.rs
- [ ] 前端文本走 i18n.js
- [ ] DB 变更走 migrations.rs

### Step 5: 验证
- [ ] cargo fmt --check 通过
- [ ] cargo clippy --all-targets -- -D warnings 通过
- [ ] cargo test 全通过
- [ ] JS ESLint 通过（如修改前端）
- [ ] 核心流程冒烟测试（如修改模板）

### Step 6: 提交
- [ ] 提交信息格式正确 (feat/fix/refactor/chore/docs: 中文)
- [ ] 只提交相关文件

### Step 7: 记录
- [ ] 更新 CHANGELOG.md
- [ ] 在 STATUS.md 中记录
```

### 9.3 关键代码规范

| 规则 | 说明 | 违反后果 |
|---|---|---|
| 禁止 `unwrap()`/`expect()` | 生产路径必须用 `Result + ?` | clippy 检查 |
| 禁止循环逐条查 DB | 必须用 `WHERE id IN (...)` | 性能问题 |
| 禁止 `innerHTML = 用户输入` | 用 `createElement + textContent` 或 DOMPurify | XSS |
| 新增类型先查重 | 在 `src/patent.rs` 搜索 | 代码膨胀 |
| 路由双注册 | `main.rs` + `lib.rs` 同步 | 平台不一致 |
| DB 变更必须迁移 | 通过 `migrations.rs` 版本化 | 数据库损坏 |
| i18n 必须双语 | 所有用户可见文本走 `i18n.js` | 国际化缺失 |
| Prompt 注入防护 | 用户输入必须用 `<user_input>` 包裹 | 安全风险 |

---

## 十、版本映射 / Version Mapping

| 版本 | 包含的 Tasks | 预计发布时间 |
|---|---|---|
| v0.7.5 (Patch) | Task 0.1 + 0.2 + 0.3 + 1.A | ~1 周 |
| v0.8.0 (Minor) | Task 1.B + 1.C + 1.D | ~2 周 |
| v0.9.0 (Minor) | Task 2.E + 2.F | ~2 周 |
| v1.0.0 (Major) | Task 3.G + 3.H + 3.I + 4.J | ~3 周 |
| v1.1.0 (Minor) | Task 4.K + 持续优化 | 持续 |

---

## 附录 A：已知代码缺陷清单 / Known Code Defects

| # | 文件 | 问题 | 严重程度 |
|---|---|---|---|
| D1 | `src/experiment/sandbox.rs` | `_timeout` 变量创建后未使用 | 高 |
| D2 | `src/experiment/sandbox.rs` | 使用 `std::env::temp_dir()` | 中 |
| D3 | `src/routes/search.rs` | IPC/CPC 过滤循环逐条查询 | 中 |
| D4 | `src/bin/mcp-server.rs` | 第 29 行 `unwrap_or_default()` | 高 |
| D5 | `src/ai/mod.rs` | `fact_check` 模块 `#[allow(dead_code)]` | 高 |
| D6 | `src/ai/chat.rs` | system prompt 拼接未用 `<user_input>` 边界隔离 | 中 |
| D7 | `src/pipeline/steps/scoring.rs` | `truncate_title` 截断数据用途文本 | 低 |
| D8 | `src/routes/ai.rs` | `has_only_allowed_history_roles()` 允许 "system" 角色 | 中 |
| D9 | `src/db/mod.rs` | `Mutex<Connection>` 高并发下可能阻塞 | 低 |
| D10 | `src/ai/client.rs` | Anthropic/Gemini CLI provider mode 处理不统一 | 低 |

---

## 附录 B：新增数据库表清单 / New Database Tables

| 版本 | 表名 | 用途 |
|---|---|---|
| v18 | `oa_analyses` 新增列 | quality_score + fact_warnings |
| v19 | `patents_embedding` | 专利全文向量嵌入 |
| v20 | `ai_cost_ledger` | AI 调用成本账本 |
| v21 | `patent_chunks` | 专利全文切片 + embedding |
| v22 | `idea_memory` | 创意级持久化记忆 |

---

*本文档最后更新：2026-08-05*
*Last Updated: 2026-08-05*