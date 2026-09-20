# Phase 1 — 基础设施增强执行规划 / Infrastructure Enhancement Execution Plan

> 规划日期：2026-08-15 | 基于 v0.7.4 + 前端重设计后代码
> Planning Date: 2026-08-15 | Based on post-v0.7.4 + frontend-redesign codebase
> 目标读者：可独立接力的 AI Agent
> 状态：🔄 待执行 / Pending

---

## 一、背景与动机 / Context

当前系统有 FTS5 关键词检索、AI 对话、创意管理，但存在三个硬缺口直接限制 AI 回答质量与用户信任：

1. **没有语义检索** — FTS5 只匹配字面词，同义词、隐含语义、跨语言都查不到。用户搜"电池寿命改进"查不到"提升续航"的内容。
2. **没有 RAG** — AI 回答时看不到用户上传的专利/技术材料，只能凭模型参数知识回答，必然产生幻觉或遗漏材料中的关键事实。
3. **没有成本追踪** — 不知道每次 AI 调用花多少 token/钱，无法做用量管理或按创意归因成本。

这三件事直接决定"AI 回答准不准、可信不可信"，是 Phase 1 的核心。

---

## 二、总览 / Overview

| 编号 | 任务 | 优先级 | 预估工期 | 依赖 |
|---|---|---|---|---|
| **B** | 向量嵌入 + 混合语义搜索 | P0-1 | 2 周 | 无（可独立开工） |
| **C** | AI 调用成本追踪 | P1 | 1 周 | 无（可独立开工，与 B 并行） |
| **D** | RAG 管道 | P1 | 2 周 | **依赖 B**（需要 B 的嵌入能力） |

**执行顺序**：B 和 C 可并行开工；D 等 B 完成后再开始。

---

## 三、架构设计决策 / Architecture Decisions

### 3.1 嵌入向量从哪来？（B 的关键决策）

**决策：复用现有的 AI 服务商 API 生成嵌入向量，不引入新的 embedding 模型依赖。**

理由：项目已经集成多家 AI 服务商，多数提供 /embeddings 端点；不新增 crate 依赖，符合 AGENTS.md "禁止新增 crate 依赖"；用户换服务商时嵌入能力自动跟随。

实现要点：在 AI 服务商模块中新增 embed() 方法，返回 Vec<f32>；优先选 OpenAI 兼容的 /v1/embeddings 格式；不支持的服务商在 UI 上禁用或提示。嵌入向量存为 TEXT（逗号分隔浮点字符串），不预设固定维度。

### 3.2 向量存在哪？最近邻怎么算？

**决策：向量化数据存入 SQLite 新表，最近邻用 Rust 端暴力余弦相似度计算（不做 HNSW 索引）。**

理由：项目规模远小于 10 万条向量，暴力计算毫秒级；不引入额外索引 crate；SQLite 存 TEXT 即可。

余弦相似度：cos_sim(a,b) = dot(a,b) / (norm(a) * norm(b))
暴力流程：SQL 查出候选集 → Rust 反序列化所有候选向量 → 对每个候选算余弦 → 取 top-K。

> 若未来超过 10 万条，再考虑 candle 或 hnsw-rs。当前不需要。

### 3.3 混合搜索权重

**决策：BM25（FTS5）+ 余弦相似度，等权重（0.5:0.5）线性融合，可配置。**

1. FTS5 关键词检索返回 top-50 候选
2. 对每条算余弦相似度
3. BM25 排名和余弦分别做 min-max 归一化到 [0,1]
4. 融合 = weight_bm25 * norm_bm25 + weight_cosine * norm_cosine
5. 按融合降序取 top-20

权重放设置页可配，默认 0.5:0.5。

### 3.4 RAG 检索注入方式

**决策：在 AI 提示词中注入检索到的上下文块，不做复杂两阶段架构。**

1. 上传材料 → 文本分块（500-800 字符，20% 重叠）→ 嵌入 → 存 rag_chunks
2. 提问时，对问题做嵌入 → 在 rag_chunks 中按 idea_id/patent_id 过滤取 top-5
3. 拼进 prompt：【参考材料】{块列表} 请基于以上参考材料回答：{query}
4. 参考材料块最多 5 个，总长不超过 2000 字符

### 3.5 成本追踪数据模型

**决策：每次 AI 调用结束后记录一行日志，成本按 token 数 × 单价计算。**

表 ai_call_log：provider / model / call_type / input_tokens / output_tokens / cost / idea_id / session_id / created_at

模型单价存在设置中，默认提供主流模型参考价，用户可编辑。

---

## 四、Task B — 向量嵌入 + 混合语义搜索

**优先级**: P0-1  **预估**: 2 周  **状态**: ⬜ 待执行

### 验收标准

1. 搜索能体现语义相似度（搜同义词也能命中）
2. 设置页可开关语义搜索、调节权重、选择嵌入服务商
3. 嵌入失败时有明确提示，不阻断关键词搜索
4. cargo test 覆盖嵌入、余弦、混合排序
5. e2e：上传含"续航改进"的文档，搜索"电池寿命"能命中

### 实施步骤

**B1：迁移 v18 — 新增 semantic_chunks 表**
文件：src/db/migrations.rs

    CREATE TABLE semantic_chunks (
        id TEXT PRIMARY KEY,
        idea_id TEXT NULL,
        patent_id TEXT NULL,
        content TEXT NOT NULL,
        embedding TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX idx_semantic_idea ON semantic_chunks(idea_id);
    CREATE INDEX idx_semantic_patent ON semantic_chunks(patent_id);

**B2：嵌入能力接入 AI 服务商**
文件：src/ai/ 下对应模块。新增 fn embed(&self, text: &str) -> Result<Vec<f32>, AiError>
请求体 {model: "...", input: text}，取 data[0].embedding 的 f64 数组转 Vec<f32>。
不支持的服务商返回明确错误。

**B3：余弦相似度工具函数**
文件：src/lib.rs 或 src/util/mod.rs
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32
维度不等或空返回 0，否则 dot/(na*nb)。

**B4：语义搜索 API 端点**
文件：src/routes/search.rs
- GET /api/search/semantic?q=&scope=&top_k= — 嵌入查询 → 暴力搜 → 混合排序 → 返回
- POST /api/idea/:id/embed — 首次触发嵌入

**B5：嵌入触发点**
创意创建/更新时嵌入 description/expanded_queries/discussion_summary；专利加载时嵌入摘要/权利要求/说明书段落；上传材料时分块嵌入。幂等：同一 content hash 不重复。

**B6：设置页配置**
文件：templates/settings.html
- 复选框：启用语义搜索
- 下拉：嵌入服务商
- 数字输入：BM25 权重、余弦权重（提示"两者之和应为 1"）
- 显示：已嵌入块数、最近嵌入时间

**B7：测试**
- cargo test：余弦（正交=0、相同=1、反向=-1、异维度=0）
- cargo test：混合排序（3-4 条构造数据）
- check_html_functions.mjs

---

## 五、Task C — AI 调用成本追踪

**优先级**: P1  **预估**: 1 周  **状态**: ⬜ 待执行

### 验收标准

1. 每次 AI 调用后自动记录 token 数和成本
2. 设置页可看：本月总花费、按服务商/模型/类型分组、趋势图
3. 创意页显示"本创意累计 AI 花费"
4. 模型单价可编辑

### 实施步骤

**C1：迁移 v19 — 新增 ai_call_log 表**
文件：src/db/migrations.rs

    CREATE TABLE ai_call_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        provider TEXT NOT NULL,
        model TEXT NOT NULL,
        call_type TEXT NOT NULL,
        input_tokens INTEGER NOT NULL DEFAULT 0,
        output_tokens INTEGER NOT NULL DEFAULT 0,
        cost REAL NOT NULL DEFAULT 0.0,
        idea_id TEXT NULL,
        session_id TEXT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX idx_ai_cost_idea ON ai_call_log(idea_id);
    CREATE INDEX idx_ai_cost_time ON ai_call_log(created_at);

**C2：成本计算工具**
文件：src/ai/ 或 src/lib.rs
fn calc_cost(provider, model, input, output, price_map) -> f64
PriceMap: HashMap<String, {input:f64, output:f64}>，从 settings 加载，默认含主流模型参考价，单位跟随用户设置。

**C3：调用日志拦截**
找到集中做 AI 请求的位置，在成功响应后解析 usage.input_tokens/output_tokens，调用 log_ai_call(...)。日志失败静默降级，不影响主流程。

**C4：统计 API**
文件：src/routes/settings.rs
- GET /api/stats/ai-cost?period=month — 月/周聚合
- GET /api/idea/:id/ai-cost — 该创意累计花费

**C5：前端展示**
文件：templates/settings.html + templates/idea.html
- 设置页"AI 使用统计"标签页：大字总花费、Chart.js 柱状趋势图、用量明细表、模型单价编辑表
- 创意页概览区小 chip："本创意 AI 花费：$X.XX"

**C6：测试**
- cargo test：成本计算（0 token、不同单价）
- cargo test：日志写入和聚合查询
- check_html_functions.mjs
---

## 六、Task D — RAG 管道

**优先级**: P1  **预估**: 2 周  **状态**: ⬜ 待执行（**依赖 B 完成**）

### 验收标准

1. 上传 PDF/文档后内容自动分块并嵌入
2. AI 对话/分析时自动检索相关块并注入提示词
3. AI 回答能引用参考材料的具体内容
4. 检索失败时降级为无 RAG 的正常回答
5. 用户可查看/管理已嵌入的材料块

### 实施步骤

**D1：迁移 v20 — 新增 rag_chunks 表**
文件：src/db/migrations.rs

    CREATE TABLE rag_chunks (
        id TEXT PRIMARY KEY,
        idea_id TEXT NULL,
        patent_id TEXT NULL,
        source_type TEXT NOT NULL,
        source_name TEXT NULL,
        chunk_index INTEGER NOT NULL,
        content TEXT NOT NULL,
        embedding TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX idx_rag_idea ON rag_chunks(idea_id);
    CREATE INDEX idx_rag_patent ON rag_chunks(patent_id);
    CREATE INDEX idx_rag_source ON rag_chunks(source_type, source_name);

**D2：文本分块工具**
文件：src/util/rag.rs
pub fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String>
按句子（。！？.!?）分割，累积到 chunk_size 后切分，保留 overlap 字符；单句超长则硬切。
默认 chunk_size=600, overlap=120（20% 重叠）。

**D3：嵌入触发（复用 B 的 embed）**
上传 PDF/文档 → 文本提取 → chunk_text → 对每块 embed() → 存 rag_chunks
幂等：同一 source_name + chunk_index 不重复。
触发点：创意页上传附件时；专利详情页加载全文时（可选）；设置页提供"重新嵌入全部材料"入口。

**D4：检索函数**
文件：src/util/rag.rs
pub async fn retrieve_relevant(db, query_embedding, idea_id, patent_id, top_k) -> Result<Vec<(String,f32)>, _>
SQL 查出候选 → Rust 端算余弦 → 取 top_k。

**D5：注入 AI 提示词**
文件：src/routes/idea.rs、patent_detail.rs、oa.rs 中的 AI 调用点
构造 prompt 前调用 retrieve_relevant，拼进参考材料块（最多 5 块，总长 ≤ 2000 字符）。
检索失败时不注入，正常走原 prompt。

**D6：前端提示**
- AI 回复后如引用了参考材料，显示"📚 引用了 N 个参考材料"，点击展开
- 上传材料时显示"正在分块并嵌入…"进度

**D7：测试**
- cargo test：分块（空文本、超短句、超长句、无标点）
- cargo test：检索（空库、精确匹配、部分匹配）
- check_html_functions.mjs
- e2e：上传含特定技术描述的文档，问相关问题，验证 AI 回答引用了文档内容

---

## 七、风险与注意事项 / Risks

1. **零新增依赖**：三个任务都不引入新的 Rust crate。嵌入走现有 AI 服务商 API，向量存 SQLite TEXT。
2. **嵌入服务商兼容性**：不是所有服务商都提供 /embeddings。设置页须明确告知"当前服务商不支持语义搜索/RAG，请切换"。
3. **大数据量**：暴力余弦在 10 万条以上变慢。当前预估远低于此，暂不优化。
4. **迁移顺序**：v18（semantic_chunks）→ v19（ai_call_log）→ v20（rag_chunks），不可乱。
5. **提示词污染**：RAG 注入的参考材料必须用【参考材料】标签包裹，防止用户输入污染系统指令（AGENTS.md 2.6）。
6. **成本计算准确性**：token 数来自服务商响应的 usage 字段，不同服务商计费方式不同，需要 per-provider 适配。

---

## 八、执行检查清单 / Execution Checklist

- [ ] Task B1：迁移 v18
- [ ] Task B2：嵌入能力
- [ ] Task B3：余弦工具
- [ ] Task B4：语义搜索 API
- [ ] Task B5：嵌入触发
- [ ] Task B6：设置页配置
- [ ] Task B7：测试
- [ ] Task C1：迁移 v19
- [ ] Task C2：成本计算
- [ ] Task C3：调用日志
- [ ] Task C4：统计 API
- [ ] Task C5：前端展示
- [ ] Task C6：测试
- [ ] Task D1：迁移 v20（B 完成后）
- [ ] Task D2：分块工具
- [ ] Task D3：嵌入触发
- [ ] Task D4：检索函数
- [ ] Task D5：提示词注入
- [ ] Task D6：前端提示
- [ ] Task D7：测试
- [ ] 全部完成后更新 CHANGELOG.md [Unreleased] 和 docs/plans/STATUS.md
- [ ] git commit 分多个提交
- [ ] node e2e_test.mjs 全部通过



