# M-B 施工规格书审查记录（2026-09-28）

> 审查人：架构伙伴 AI（Huawei Cloud IaC Architecture Partner）
> 审查对象：`docs/plan/2026-09-28-mb-construction-spec.md`（commit `6e8813f`）
> 审查方法：逐条用 `grep`/`sed` 核对规格书中的"硬事实"与实际代码，验证行号、调用关系、包切分逻辑
> 结论：**规格书整体质量高，硬事实大部分可核。发现 3 个实质性问题已修正，2 个建议增强已标注。**

---

## 一、已验证正确的硬事实（12 条，逐条带证据）

| # | 规格书声称 | 验证结果 | 证据 |
|---|-----------|---------|------|
| 1 | TF-IDF 三份拷贝按字节切片（`vector/mod.rs:32`、`rag/chunker.rs:62`、`routes/search.rs:859`） | ✅ | 三处均为 `cleaned[i..i+n]`，`cleaned` 是 `String`，`i` 按 `len()` 字节长度迭代 ⇒ 中文 3 字节/字必踩非字符边界 |
| 2 | `compute_and_save_embedding` 全仓零调用 | ✅ | `grep -rn "compute_and_save_embedding" src/` 仅返回定义处 `vector/mod.rs:159` |
| 3 | `save_patent_chunks` 全仓零调用点 | ✅ | `grep -rn "save_patent_chunks" src/` 仅返回定义处 `db/rag.rs:17` |
| 4 | XHR `description/claims` 写空串（`google_patents_xhr.rs:498-500`） | ✅ | 实际在 499-500 行：`description: String::new()` / `claims: String::new()` |
| 5 | EPO `description/claims` 写空串（`epo_ops.rs:1021-1022`） | ✅ | 1021 行：`description: String::new()`，1022 行：`claims: String::new()` |
| 6 | `fact_check` 唯一生产接线在 `routes/ai.rs:1433`（OA 流式） | ✅ | `grep -rn "check_oa_analysis" src/` 仅返回 `routes/ai.rs:1433` 调用 + `ai/mod.rs` re-export |
| 7 | `enrich-free`（`routes/patent.rs:221`）不查冷却表 | ✅ | 函数体内无 `breaker`/`global_table`/`cools_down` 引用；20s 超时确认 |
| 8 | RAG 在 `src/rag` 之外零引用 | ✅ | `grep -rn "rag::" src/ --include="*.rs" | grep -v "^src/rag/" | grep -v "^src/lib.rs"` 返回空 |
| 9 | `fact_check.rs:14` 注释"未接入主流程"已过时 | ✅ | 实际在 `:14`：`"注：本模块为 OA 分析增强功能的预留层，当前未接入主流程"`，但 `routes/ai.rs:1433` 已调用 |
| 10 | `vector/mod.rs:77` 注释 "no IDF without corpus" | ✅ | `:77`：`"// Normalize to unit vector (simplified: no IDF without corpus)"` |
| 11 | `INSERT INTO patents` 只出现在 `db/patent.rs:40` 的 `insert_patent` 内部 | ✅ | `grep -rn "INSERT.*INTO patents" src/` 确认唯一写入点；FTS 同步在 `:35`(DELETE)/`:44`(INSERT) |
| 12 | 五个非空写入来源 | ✅ | 逐个验证：`routes/patent.rs:28`(SerpAPI)、`:221`(enrich-free)、`:714`(PDF)、`serpapi.rs:517`(provider)、`settings.rs:681`(导入) |

---

## 二、已修正的实质性问题（3 条）

### 修正 1：`compute_and_save_embedding` 行号错误

- **位置**：规格书 §1.2
- **原文**："`vector/mod.rs:166` 的 `compute_and_save_embedding`"
- **实际**：`compute_and_save_embedding` 定义在 `vector/mod.rs:159`
- **影响**：规格书开篇声明"行号可核"，行号错误会动摇执行棒对规格书事实的信任
- **修正**：166 → 159

### 修正 2：截断点遗漏 6 处（§1.3 + MB3 scope ③）

- **位置**：规格书 §1.3 现状描述 + MB3 scope ③
- **原文**："两处截断违反 AGENTS.md §2.5：`analysis.rs:50` 与 `deep_reasoning.rs:138`"
- **实际**：用 `grep -rn "chars().take" src/pipeline/ src/routes/ai.rs` 扫出 **8 处**喂给 AI 上下文的 `chars().take` 截断，全部不走 `truncate_for_ai`，全部违反 AGENTS.md §2.5

| # | 文件:行号 | 截断对象 | 目标 | 字符数 |
|---|----------|---------|------|--------|
| 1 | `analysis.rs:50` | `m.snippet` | AI prompt | 150 |
| 2 | `analysis.rs:156` | `ctx.ai_analysis` | AI prompt | 500 |
| 3 | `analysis.rs:180` | `m.snippet` | FeatureCard description | 300 |
| 4 | `analysis.rs:193` | `m.snippet` | FeatureCard core_structure | 200 |
| 5 | `analysis.rs:250` | `ctx.ai_analysis` | AI prompt | 2000 |
| 6 | `deep_reasoning.rs:138` | `m.snippet` | AI prompt | 120 |
| 7 | `oa_response.rs:137` | `m.snippet` | AI prompt | 80 |
| 8 | `claim_tree.rs:24` | `ctx.ai_analysis` | AI prompt | 800 |

- **影响**：MB3 scope ③ 原文说"两处同批改"，只改 2 处会留下 6 处继续违规
- **修正**：§1.3 从"两处"改为"八处"逐条列出；MB3 scope ③ 从"两处同批改"改为"全部八处同批改"

### 修正 3：MB0 缺少 `VectorIndex` 可用性架构提示

- **位置**：规格书 MB0 scope ③
- **问题**：`compute_and_save_embedding` 需 `&VectorIndex`，而 `insert_patent` 只有 `&Database`，`AppState` 也不持有 `VectorIndex`
- **修正**：追加架构提示，给出 (a) 调用方构造（推荐）(b) 内联两方案，明确 embedding 失败不回滚

---

## 三、建议增强（未修改，供规划会话斟酌）

### 建议 1：MB2 的 N=5 默认值缺少超时预算论证
### 建议 2：MB4 的"≥3 条理由"阈值缺少论证

---

## 四、验证方法（供规划会话复核）

```bash
# 修正 1 验证
grep -n "pub fn compute_and_save_embedding" src/vector/mod.rs  # 预期：159:

# 修正 2 验证
grep -rn "chars().take" src/pipeline/ src/routes/ai.rs  # 预期：8 行 AI 上下文截断

# 修正 3 验证
grep -n "VectorIndex" src/routes/mod.rs  # AppState 中无 VectorIndex
grep -n "struct AppState" src/routes/mod.rs  # 确认在 routes/mod.rs
```
