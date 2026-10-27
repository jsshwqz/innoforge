# 证据可核验的专利分析内核 v2（Evidence Kernel v2）— 设计

> 分支：`r33/evidence-kernel`（基于 origin/main @ e1e6bf0）
> 状态：设计稿（本文）→ 垂直切片实现 → 验收负控 → draft PR
> 一句话目标：把「一段看似专业的文本」变成「每句结论可追溯（evidence id）、可反驳（核验器会红）、可评分（评分区分『看起来好』与『有证据』）」的产物。

---

## 0. 背景与问题清单（现状证据，全部来自 main@e1e6bf0 实测）

16 步 pipeline 中第 1–10 步是代码算法、证据（source_id）强绑定；**从第 11 步 AiDeepAnalysis 起，证据只被"塞进 prompt"，AI 回写的只有自由 Markdown**，此后没有任何程序化核验把结论与证据重新接上。具体问题：

| # | 问题 | 证据（file:line） |
|---|---|---|
| EK-P1 | **结论与证据脱钩**：ai_analysis（最终报告主体）是自由文本，prompt 口头要求标 `[引用N]`，但输出里引用是否存在、编号是否越界，零校验 | `deep_reasoning.rs:192`（要求）、`analysis.rs:33`（写入）；全链无校验代码 |
| EK-P2 | **检索为空却下结论**：空检索时 scoring 正确给出 InsufficientEvidence（`scoring.rs:24-47`），但 AI 步骤（11/12/14/16）不感知该状态照常产出完整"新颖性/风险"论述并成为最终报告（`routes/idea.rs:990`）；executive 渲染把 None 折成 0 分再输出"低新颖性"结论（`routes/idea.rs:1230,1244-1257`） | 见左 |
| EK-P3 | **引用格式四套并存且互不兼容**：`[Dx:第Z段]`（FACT_DISCIPLINE，仅拼入 4 处 prompt）、`D1第[00XX]段`（一审表格）、`【依据：类型：定位】`（驳回复审）、`[引用N]`（idea 链路）；OA 主 builder 不带 FACT_DISCIPLINE | `patent.rs:12-22/176/222/317/1643、526-582、763-777`；`deep_reasoning.rs:192` |
| EK-P4 | **现有"核验"十项空壳**：output_guard 结果被丢弃（`patent.rs:1378` 只发正文）、UA3/UA6 检测器写好未接线（`fact_check.rs:1197/1335` dead_code）、rag_search 死码、fact_check_rejected 无消费者、claim_tree 的 evidence_ids 恒 `"[]"`（`claim_tree.rs:85`） | 详见调研盘点 |
| EK-P5 | **弱匹配假绿**：段落核验用 `references.contains("[num]")` 子串包含（`fact_check.rs:496-511`），所有对比文件拼接后无 doc 归因（D1 引 D2 的段号也过）；provenance 的专利号↔证据匹配也是 contains（`provenance.rs:70-74`）——issue #79 点名这类"数字包含检查是假绿机器" | issue #79 |
| EK-P6 | **引用无结构化通道**：OA 流 SSE 逐 chunk 把换行替换为空格（`routes/ai.rs:1721`）破坏句对齐；核验报告与正文混存同一 full_text（`routes/ai.rs:1736`）；存储无引用结构 | 见左 |
| EK-P7 | **D 编号映射脆弱**：refs 编号来自数组顺序、自动抓取公开号独立从 D1 递增、AI 被要求按审查员的 D 编号引用——三套编号无对齐表 | `routes/ai.rs:1534,1567-1581` |
| EK-P8 | **评分不能区分"看起来好"**：novelty_score 是纯代码公式（`scoring.rs:78-93`）与 AI 文本质量无关；reflection 的 evidence 分只是 evidence_chain 均值置信度（自评）；没有任何"这段结论有没有证据支撑"的度量 | `reflection.rs:127-131` |

**外部实践参照**：ALCE（引用生成评测）、RAGAS faithfulness（断言分解→对照上下文逐条核验）、CiteGuard（2025-10，引用归因忠实性）、TRACE（2026，检索对齐引用评测）；共识做法 = 断言级分解 + 引用解析为 (doc, locator) + 与源文精确/归一化匹配 + 支持/不支持/不足三分类 + 未支撑断言列表驱动修复。issue #79 的三元组精确匹配标准与该共识一致，本设计直接采用。

---

## 1. 目标与非目标

**目标（本切片）**
1. claim/evidence 数据模型：`claim、source_id、locator、quote、relation∈{support,contradict,insufficient}`。
2. 引用核验器（纯代码，零 AI）：锚点解析为 (doc_id, kind, number) 三元组**精确匹配**；引文原文归一化后逐字/近似匹配；失败即 FAIL，禁止子串包含判据。
3. 结论必须绑定证据：事实句无已核验引用 → 进「未支撑清单」→ 修复或**降级标注**，不得静默通过。
4. 检索为空 → 输出 `InsufficientEvidence`，禁止推断；评分被 grounding 封顶。
5. rubric 五维（grounding/coverage/legal_reasoning/clarity/consistency）+ critique-repair loop（生成→核验→未支撑句清单→修复或降级）。
6. 垂直切片接入 idea pipeline 的 Finalize 步（一个接缝、非破坏性），并在该链路验证。
7. 合成案件夹具 + ≥3 个负控 + 逐句带 evidence id 的示例报告。

**非目标（本切片不做，防 scope 爆炸）**
- 不改 OA 流式主链路（SSE/答复书生成）——SSE 换行破坏问题（EK-P6）留待后续把内核挂到 OA 落库前。
- 不动数据库 schema（ADR-8）；不统一前端渲染组件；不做语义 NLI 级"支持性判定"（v1 只做可确定性核验的锚点+引文匹配，语义支持性列为 v3 方向）。
- 不替换现有 fact_check/C 系列检查（并存，见 ADR-7）。

---

## 2. 数据契约（核心类型，serde 可序列化）

```rust
/// 证据宇宙：一次分析实际持有的全部可寻址文本。
/// 只由代码从 ctx 构造（patent_results/rag_chunks/OA 文本），编号可信（解决 EK-P7）。
pub struct SourceCorpus {
    pub docs: Vec<SourceDoc>,
}
pub struct SourceDoc {
    pub doc_id: String,            // "D1"、"引用3"、"本申请"
    pub kind: SourceDocKind,       // Patent{pub_number} / OfficeAction / OwnApplication / Web
    pub title: String,
    pub sections: Vec<SourceSection>,
}
pub struct SourceSection {
    pub section_id: String,        // "D1:para:0035"、"D1:claim:2"、"引用3:chunk:0"
    pub kind: SectionKind,         // Claim(u32) | Paragraph(u32) | Chunk(u32)
    pub text: String,              // 必须是手头真实文本（摘要/切片/原文段）
}

/// 单条证据引用（AI 产出或代码抽取）。
pub struct EvidenceRef {
    pub raw: String,               // 原始锚点串，如 "[D1:第0035段]"
    pub anchor: Option<Anchor>,    // 解析失败 = None（unparsed，与 unverified 分开计）
    pub quote: String,             // 锚点旁的引文（可空）
}
pub struct Anchor {                // issue #79 三元组
    pub doc_id: String,
    pub kind: SectionKind,
    pub number: u32,
}

/// 结论句（claim）。
pub struct Claim {
    pub id: String,                // "C1"..
    pub sentence_index: usize,     // 在原文中的句序（可回贴）
    pub text: String,
    pub kind: ClaimKind,           // FactFromSource | AnalysisInference | LegalArgument | Meta(标题/建议等豁免类)
    pub evidence: Vec<EvidenceRef>,
}

/// 核验结果（每 claim × 每 evidence）。
pub enum VerifyStatus {
    VerifiedExact { section_id: String },                    // 锚点命中 + 引文归一化逐字匹配
    VerifiedApprox { section_id: String, score: f64 },       // 锚点命中 + 引文 3-gram Jaccard ≥ 0.85
    AnchorNotFound,                                          // 三元组在 corpus 中无精确命中
    QuoteMismatch { best_score: f64 },                       // 锚点命中但引文对不上
    UnparsedAnchor,                                          // 原始串无法解析为三元组
}
pub enum Relation { Support, Contradict, Insufficient }

/// 内核输出。
pub struct KernelReport {
    pub status: KernelStatus,      // Ok | Degraded | InsufficientEvidence
    pub claims: Vec<ClaimRecord>,  // 含每条 claim 的核验明细与最终 relation
    pub metrics: GroundingMetrics, // verified/unverified/unparsed/unsupported 计数与 hallucination_rate（分母=可解析锚点，#79）
    pub rubric: RubricScore,       // 五维
    pub critique: Vec<Finding>,    // 未支撑句清单（修复循环的输入与产物）
    pub repair_rounds: u32,
}
```

**不变量（可被测试证伪的硬约束）**
- I1：任何 `Verified*` 必须同时满足锚点三元组精确相等（含零填充）与引文匹配；**全链禁止对数字/编号用子串包含判据**。
- I2：`hallucination_rate = (AnchorNotFound + QuoteMismatch) / 可解析锚点数`；分母为 0 时 rate 必须输出 `None`（不许除零，也不许默认 0 分绿）。
- I3：corpus 为空（无任何 section）→ `status = InsufficientEvidence`，claims 不产生事实断言，grounding = 0，总分被 grounding 封顶。
- I4：`FactFromSource` 类 claim 无任何 `Verified*` 证据 → 必须出现在 critique 清单，且在修复轮耗尽后被降级标注（不得静默保留原样进最终报告）。
- I5：核验器不调用任何 AI/网络（`run_verify` 是纯函数）。

---

## 3. 架构与流程

```
                    ┌──────────────────────────────────────────────┐
                    │                SourceCorpus（代码构造，可信） │
                    │  patent_results(摘要) + rag_chunks + OA 文本  │
                    └───────────────┬──────────────────────────────┘
                                    │
  AI 生成文本（ai_analysis 等）      ▼
 ┌──────────────┐   ┌──────────────────────────┐   ┌───────────────────┐
 │ 生成（现状）  │→ │ anchor.rs 解析（四套格式  │→ │ verifier.rs       │
 │ 不改 prompt   │   │ →统一三元组；解析失败     │   │ 锚点精确匹配       │
 │ 机制）        │   │ = UnparsedAnchor）       │   │ + 引文归一化匹配   │
 └──────────────┘   └──────────────────────────┘   └─────────┬─────────┘
                                                             ▼
 ┌──────────────┐   ┌──────────────────────────┐   ┌───────────────────┐
 │ critic.rs    │←─ │ classifier.rs            │← │ VerifyStatus 汇总 │
 │ 未支撑句清单  │   │ 句子切分+claim 分类       │   │ per-claim relation│
 │ 修复/降级驱动 │   │（确定性启发式）           │   └───────────────────┘
 └──────┬───────┘   └──────────────────────────┘
        │ 修复轮（≤2，RepairFn 注入，可 fake）        ▼
        └──────────→ 重写文本 → 重走 解析/核验 ──→ rubric.rs 五维评分
                                                     │
                                                     ▼
                                    insufficient.rs 空证据硬门（I3）
                                                     │
                                                     ▼
                                   report.rs 渲染：逐句 [E#] + 证据表 + 核验表 + 评分表
```

**接线（垂直切片，一个接缝）**：`pipeline/steps/finalize.rs` 在现有 fact_check 之后：
1. 从 ctx 构造 SourceCorpus（rag_chunks → `引用N:chunk:M` sections；patent_results 摘要 → `DN:abstract` 伪段；如有 OA 文本 → OfficeAction doc）。
2. `run_verify(ai_analysis, corpus)`（纯代码）→ KernelReport。
3. critique-repair：v1 切片用**零 AI 的降级路径**（不修句，只降级标注 `【未证实】`）+ 预留 `RepairFn` trait 注入点（AI 修复在后续版本接，测试用 fake repair）。
4. 产物：ai_analysis 尾部追加「证据核验报告」段（Markdown，前端无需改动）；`ctx.kernel_report`（JSON，进快照，不进新表）；**决策门**：grounding < 0.5 或 status=InsufficientEvidence → `ctx.decision = None` 并记录原因（I4 的产品语义）。
5. **修 P2 渲染折零**：executive 报告 `novelty_score.unwrap_or(0.0)` 改为按 `novelty_status` 输出「证据不足，无法评分」（`routes/idea.rs:1230` 一处 + version 分支一处）。

---

## 4. ADR（取舍记录，可反驳）

- **ADR-1 核验是确定性代码，永远不调 AI。** 理由：可证伪、零成本、可负控；仓库史上三次假绿（#79 数字包含、vacuous 正则空转、guard 结果丢弃）全部来自"用启发式/流程代替硬判据"。AI 只出现在生成与修复轮，且修复产物仍须过同一核验器。反驳点：语义级"引文是否真的支持断言"（NLI）代码做不了——接受，v1 明确只承诺"锚点真实 + 引文真实存在于源文"，语义支持性标为 v3（见失败模式 F8）。
- **ADR-2 单一规范锚点语法 + 遗留格式适配器。** 规范格式 `[doc_id:权利要求N]` / `[doc_id:第XXXX段]` / `[引用N]`（chunk 容器）；适配器把 `D1第[0035]段`、`【依据：对比文件 D1第35段】` 解析进同一三元组，解析失败计 UnparsedAnchor（不静默丢，#79 要求分开统计）。反驳点：为何不强制改所有 prompt？——改 prompt 不改核验器会重演 EK-P1；本切片先保证"已产出文本可核验"，prompt 统一在 OA 链路接入时做。
- **ADR-3 claim 粒度 = 句。** 逐句切分（CJK 句号/问号/叹号 + 长句分号二切），事实性判定用确定性启发式（含"公开了/记载了/相比现有技术/区别在于…"断言模式 → FactFromSource；含"建议/因此可以考虑"→ LegalArgument/Meta 豁免）。反驳点：启发式会误分类——接受，误分类的代价是不对称的：把推论误判为事实 → 进核验 → 无引用 → 降级标注（偏保守，可接受）；把事实误判为推论 → 漏网（v1 用高召回的事实模式表缓解，负控 F5 盯住）。
- **ADR-4 引文匹配两级：归一化逐字优先，近似兜底。** 归一化 = NFKC + 去空白 + 中西引号统一 + 标点全半角统一（复用 `whitespace_search` 思路扩展）；近似 = 字符 3-gram Jaccard ≥ 0.85（复用 golden_regression 算法），`VerifiedApprox` 必须带分数明示。**禁止任何形式的"数字包含即通过"。**
- **ADR-5 空证据硬门。** corpus 无 section 或 novelty_status=InsufficientEvidence → KernelReport.status=InsufficientEvidence：不产出事实 claim、grounding=0、总分=min(各维, grounding×5)（数值上不可能高分）、渲染层不得折零（修 routes/idea.rs:1230）。
- **ADR-6 修复循环有界且可注入。** `RepairFn = Fn(text, findings) -> text`；生产 v1 = NoRepair（直接降级）；测试 = fake repair；AI repair 后续接。未修复完的句**降级标注而非删除**（保留用户可见性，防止内核悄悄改写结论）。
- **ADR-7 与现有检查并存不替换。** fact_check/C 系列、provenance 照旧运行；内核报告独立追加。理由：现有检查各有价值但均文本级（EK-P4/P5），替换它们会把本切片变成大重构；后续内核稳定后再收编（消空壳清单见 §6）。
- **ADR-8 不加数据库 schema。** KernelReport 以 JSON 存 pipeline snapshot（ctx 字段），evidence_chain 表不动。理由：schema 变更需用户确认（AGENTS Step 3），且快照已随步骤持久化可恢复；独立表等内核定型后再议。

---

## 5. 失败模式（FMEA，每条配对策）

| # | 失败模式 | 对策 |
|---|---|---|
| F1 | AI 伪造锚点（`[D1:第9999段]`） | 三元组精确匹配必 FAIL → AnchorNotFound → hallucination_rate 上升 + claim 降级。**负控 NC1 直接盯这条** |
| F2 | AI 伪造引文（编一段"原文"） | 引文归一化后与命中 section 匹配失败 → QuoteMismatch。近似阈值 0.85 防止改写洗白（NC1 同覆盖） |
| F3 | 把核验器改回子串包含（回归 #79 假绿） | 负控 NC1 的红通道 = 把 verifier 临时替换为 contains 版本跑测试必须翻红（真实执行，见 §7） |
| F4 | 证据在修复轮被删（上游 bug/竞态） | claim 无可核验证据 → 进 critique + 降级 + grounding 下降 → 决策门关闭。**负控 NC2** |
| F5 | 分类器漏判事实句（漏网） | 高召回模式表 + 负控（构造无模式词但含专利号断言的句子，至少被"含锚点/专利号"规则捕获）；残余风险记录在报告 limitations |
| F6 | corpus 构造错（把摘要标成原文段） | SectionKind 与来源强绑定（abstract 伪段单独 kind），报告中明示证据类型；绝不把摘要冒充 claim/para |
| F7 | 超大 corpus 性能（万级 section） | 索引 = HashMap<(doc_id, kind), BTreeMap<number, section_idx>>，锚点 O(1)；引文匹配只在命中 section 内做，天然受限 |
| F8 | 锚点+引文都真但断言语义不成立（引非所证） | v1 不能解（ADR-1 边界）；报告 limitation 明示"核验=存在性，非支持性"；v3 方向 NLI/对照表 |
| F9 | 仅锚点引用被当支撑计分（AnchorOnly 冒充核验；PR #125 复核 Blocking 抓出） | `verified_weight()` 只认逐字核验（Exact/Approx ⇒ 1.0，AnchorOnly ⇒ None）；grounding 分子只计 Exact/Approx；critic 清单点名"仅有锚点、无逐字引文"；`anchor_only` 计量保留为诊断。**负控 NC4 直接盯这条** |

---

## 6. 与现有代码的复用映射（不重复造轮子）

| 复用 | 位置 | 用途 |
|---|---|---|
| 归一化去空格思路 | `ai/whitespace_search.rs` | 扩展成 verifier 的归一化管线 |
| 3-gram Jaccard | `ai/golden_regression.rs:122-146` | 近似引文匹配 |
| 零填充段落匹配的教训 | `ai/fact_check.rs:496-511` | 反例：v1 用 BTreeMap 精确键，不用 contains |
| chunk 基础设施 | `patent_chunks` 表 + `rag/retriever.rs` | corpus 的 chunk sections 来源 |
| evidence_chain 表 | `db/evidence.rs` | 不动；内核报告独立（ADR-8） |
| FACT_DISCIPLINE 锚点语法 | `patent.rs:12-22` | `[Dx:权利要求Y]/[Dx:第Z段]` 即规范语法的事实来源 |

**后续收编清单**（本切片不动）：output_guard 拦截语义、UA3/UA6 接线、rag_search 死码删除、provenance 弱匹配替换、claim_tree evidence_ids 填充。

---

## 7. 验收标准与负控（写进 PR）

**正控**
- AC1：合成案件（2 专利 + 12 段原文 + 若干 chunk + 一份混合引用的分析文本）端到端跑通，示例报告逐句带 `[E#]`，证据表/核验表/评分表齐全（`tests/fixtures/evidence_kernel/`）。
- AC2：好引用（真实锚点+逐字引文）→ VerifiedExact；轻微改写 → VerifiedApprox(score)；均计 grounding。
- AC3：`cargo test evidence_kernel` 全绿；clippy `-D warnings` 零警告；示例报告由测试生成（可复现）。

**负控（每条先绿后红验证测试有效性）**
- NC1 伪造引文必须红：`[D1:第9999段]` + 编造引文 → AnchorNotFound/QuoteMismatch、hallucination_rate 上升、claim 降级。**红通道**：临时把 verifier 换成 contains 版（模拟 #79 被否决的检查器）重跑 → 测试必须 FAIL（真实执行并记录输出）。
- NC2 删证据必须降级：从 corpus 移除被引用的 doc → 原 Verified claim 变 Unsupported → grounding 跌破决策门 → decision=None + 报告出现降级标注。红通道：把降级逻辑改成"跳过无证据 claim"重跑 → FAIL。
- NC3 检索为空不得高分：空 corpus → status=InsufficientEvidence、grounding=0、五维总分被封顶（数值断言 < 阈值）、无事实 claim。红通道：去掉封顶规则重跑 → FAIL。
- NC4 仅锚点不得冒充支撑（existence ≠ support；PR #125 复核 Blocking）：只含裸 `[引用N]`（锚点命中、无引文）的事实句 → relation 必须非 Support、grounding 分子必须为 0、决策门必须关闭、句子必须降级标注；`anchor_only` 计量保留为诊断。**红通道**：`verified_weight()` 把 AnchorOnly 记 0.7 分（修复前实现）重跑 → 测试必须 FAIL（真实执行：relation=Support、grounding=0.7 越过 0.5 决策门）。

**命令**（PR 正文照抄）：
```bash
cargo test --lib evidence_kernel          # 单元+夹具
cargo test --test evidence_kernel_slice   # 接缝集成（finalize 接线）
cargo clippy --all-targets -- -D warnings
```

---

## 8. 开放问题（需拍板才阻塞，其余按上述默认执行）

无阻塞性开放问题。两个已按默认处理、可推翻的决策：修复轮 v1 用零 AI 降级（ADR-6）；决策门阈值 grounding<0.5 关闭 decision（ADR 见 §3-4）。若你想让 v1 就带 AI 修复轮或调整阈值，在 draft PR 里留言即可。
