# DeepScientist 借鉴分析 — InnoForge 架构演进参考

> 分析日期：2026-04-08
> 对比版本：InnoForge v0.5.1 (Rust, 12,800 行) vs DeepScientist v1.5.17 (Python+Node.js)
> 对比仓库：[ResearAI/DeepScientist](https://github.com/ResearAI/DeepScientist) (ICLR 2026 Top 10, 1.8k stars)
> 参考输入：`INTEGRATION_SPEC.md`（深度集成技术规范）
> 协作分析：Claude + Gemini 交叉审查，Aion-Forge 多引擎研究

---

## 1. 项目定位对比

| 维度 | InnoForge (创研台) | DeepScientist |
|------|-------------------|---------------|
| **定位** | 专利/创新验证平台 | 自主研究工作室 |
| **核心流程** | 13 步创新验证管道 | 持续实验循环 (baseline→hypothesis→experiment→paper) |
| **AI 角色** | 辅助分析（代码编排 LLM） | 自主执行（AI 驱动全流程） |
| **数据持久化** | SQLite + 管道快照 | Git 仓库 (one quest = one repo) |
| **语言** | Rust + Vanilla JS | Python + React |
| **部署** | 零依赖单体二进制 | npm 全局安装 + daemon |
| **目标用户** | 研发人员（发明人、工程师、技术负责人） | 研究生、实验室 |

**核心差异**：DeepScientist 面向学术研究全流程；InnoForge 面向研发人员的创新验证与 IP 保护。两者用户画像高度重叠——都是**会写代码、懂 Git、需要跑实验的技术人员**。这意味着 DeepScientist 的很多理念可以直接移植。

**DeepScientist 七大契约**（AGENTS.md）：
1. One Quest, One Repository — 每个研究课题一个 Git 仓库
2. Python runtime + npm launcher — 双语言混合架构
3. MCP 三命名空间 — `memory`, `artifact`, `bash_exec`
4. Prompt 定义工作流 — Skill 提供执行行为
5. Registry 注册模式 — 扩展点统一用 register/get/list API
6. 客户端平权 — Web UI 和 TUI 消费相同 daemon API
7. QQ 集成是核心产品形态 — 不是 hack

**DeepScientist 八阶段 Skill**：`scout` → `baseline` → `idea` → `experiment` → `analysis-campaign` → `write` → `finalize` → `decision`

---

## 2. 值得借鉴的理念

DeepScientist 的核心哲学：**「研究是持续积累的过程，不是一次性查询」**

InnoForge 当前是「跑一次管道 → 出报告 → 结束」，缺乏迭代和积累机制。

### 2.1 Findings Memory（发现记忆系统）⭐⭐⭐

**DeepScientist 做法**：失败路径不丢弃，总结保存，供后续实验复用。

**InnoForge 现状**：`ResearchState.excluded_paths` 只是内存中的临时字符串列表，Pipeline 结束即消失。

**改进方案**：
- 在 `evidence_chain` 表增加 `finding_type` 字段（`discovery` / `dead_end` / `insight`）
- 新 Idea 提交时，自动检索历史排除路径中的相似记录
- 跨 Idea 的 Findings 检索 API：`GET /api/findings?q=关键词`

**涉及文件**：`src/db/evidence.rs`, `src/pipeline/context.rs`, `src/routes/idea.rs`

### 2.2 持续迭代分析 ⭐⭐⭐

**DeepScientist 做法**：每轮实验结果自动喂入下一轮假设生成，形成闭环。

**InnoForge 现状**：Pipeline 跑一次就结束，用户需手动重新提交。

**改进方案**：
- 新增 `POST /api/idea/:id/iterate`：基于上一次分析 + 用户反馈，自动调整关键词重跑 Pipeline
- `ResearchState.open_questions` 作为下一轮输入种子
- 迭代计数器：`iteration_count` 字段，展示方案经过几轮验证

**涉及文件**：`src/pipeline/runner.rs`, `src/routes/idea.rs`

### 2.3 Research Map 可视化 ⭐⭐⭐

**DeepScientist 做法**：Canvas 可视化研究分支、已完成路径、死胡同。

**InnoForge 现状**：只有线性 13 步进度条。

**改进方案**：
- 在报告页增加「创新探索地图」：以 Idea 为中心，展示 prior_art_clusters → contradictions → novelty 关系
- 用 Mermaid.js（已有 Markdown 渲染能力）或 D3.js 渲染证据链网络图
- 红色节点 = 反对证据，绿色节点 = 支持证据，灰色 = 排除路径

**涉及文件**：`templates/` (新增), `src/routes/idea.rs` (report.html 增强)

### 2.4 假设演化链 ⭐⭐

**DeepScientist 做法**：贝叶斯优化驱动假设选择。

**InnoForge 现状**：`current_hypothesis` 在 ParseInput 设置一次，后续不更新。

**改进方案**：
- 每步结束后根据新证据更新假设
- `hypothesis_history: Vec<(step, old_hypothesis, new_hypothesis, reason)>`
- Step 11 (DeepReasoning) 的 `novel_directions` 自动转为新假设候选

**涉及文件**：`src/pipeline/context.rs`, `src/pipeline/steps/deep_reasoning.rs`

### 2.5 人机协作 — 中途重定向 ⭐⭐

**DeepScientist 做法**："Human takeover anytime"，研究者可随时暂停、编辑、重定向。

**InnoForge 现状**：已有 resume 能力，但只能从断点续跑，无法修改方向。

**改进方案**：
- `POST /api/idea/:id/redirect`：注入新约束（如追加排除关键词、修改技术领域）
- Pipeline resume 时检查 override 参数

**涉及文件**：`src/pipeline/runner.rs`

### 2.6 Webhook 通知 ⭐⭐

**DeepScientist 做法**：WeChat/Telegram/飞书多通道推送。

**InnoForge 现状**：仅 Web SSE。

**改进方案**：
- `POST /api/settings/webhook` 配置回调 URL
- Pipeline 完成时 POST JSON 到用户 URL（标题、评分、结论摘要）

**涉及文件**：`src/routes/settings.rs`, `src/pipeline/runner.rs`

---

## 3. Gemini 集成规范（INTEGRATION_SPEC.md）评审

Gemini 提出了 4 个模块（A/B/C/D），以下是务实评估：

### 模块 A：自主实验验证引擎 — 采纳 70% ⭐

| Gemini 方案 | 与研发用户的契合度 | 实施建议 |
|------------|-------------------|---------|
| 自动生成验证脚本 | **高** — 研发用户会写代码、懂实验 | AI 生成 Python/Rust 验证脚本，用户可审查修改 |
| 沙箱运行 | 中 — Docker 可接受但非必须 | 第一步用简单的子进程隔离，后期可选 Docker |
| 指标捕获（吞吐量/延迟） | **高** — 研发用户关心性能数据 | 捕获标准输出中的数值指标，自动结构化 |
| 报告注入到 docx | 高 | 将真实实验数据填入交底书的「实施例」章节 |

**结论**：研发用户有能力审查和运行实验代码，模块 A 的价值远高于之前评估。关键是让人保持控制权（Human Takeover）。

### 模块 B：Git 驱动版本管理 — 采纳 80% ⭐

| Gemini 方案 | 与研发用户的契合度 | 实施建议 |
|------------|-------------------|---------|
| 每个 Quest 创建 Git 子仓库 | **高** — 研发用户天天用 Git | 使用 `git2-rs` 管理，UI 提供可视化 |
| `failed-path` 分支 | **高** — 研发人员理解分支概念 | 失败路径作为「非显而易见性」证据保留 |
| `design-around-01` 分支 | **高** — 规避设计天然适合分支模型 | 每条规避路径一个分支，diff 可视化对比 |

**结论**：研发用户懂 Git，直接采用 Quest-Git 模型。用 `git2-rs` 实现，前端用分支图可视化。

### 模块 C：权利要求特征拆解 — 采纳 90% ⭐

| Gemini 方案 | 与现有架构的契合度 | 改进点 |
|------------|-------------------|--------|
| Feature Matrix 拆解 | 完美对接 FeatureCard 5 维体系 | 已有基础，需增加「必要技术特征」提取 |
| Gap 分析 | 完美对接 `classify_diff` | 已做 structure/method/parameter 分类 |
| 授权率预测 | 需新增 | 基于 Gap 类型 + 数量 → 预测授权概率 |

**这是最应该优先做的模块**，因为它与 T1-T3 刚完成的 FeatureCard 增强天然衔接。

**实施路径**：
1. AI 提取对比文件的「必要技术特征」列表 → 存为 FeatureCard
2. 用户 Idea 的 FeatureCard vs 对比文件的 FeatureCard → `classify_diff`
3. 汇总 Gap 数量和类型 → 输出授权率预测
4. 新 API：`GET /api/idea/:id/patentability` 返回授权率 + 理由

### 模块 D：动态技术空间地图 — 采纳 60%

| Gemini 方案 | 问题 | 务实替代 |
|------------|------|---------|
| 向量空间投影 | 当前无 embedding 模型 | 用 `prior_art_clusters` 数据做气泡图 |
| 热力图 | 需要大量专利向量 | 聚类密度图：红区/绿区标注 |
| ECharts/React Flow | 前端是 Vanilla JS | 用 ECharts CDN 或 Mermaid.js |

**结论**：先用现有聚类数据做简化版；embedding 向量化作为后期增强。

---

## 4. 三方分析对比（Forge 工程视角 + 战略 ZL 矛盾视角 + Claude 产品视角）

### 4.1 该舍弃什么（三方共识）

| 现有组件 | Forge (Gemini) | 战略 ZL | Claude | 共识 |
|---------|---------------|---------|--------|------|
| runner.rs 线性编排 | 舍弃，改 Skill 编排器 | 舍弃（严重度 9/10），改 DAG | 舍弃，改状态机 | **三方一致：舍弃** |
| pipeline_snapshots 表 | 舍弃，版本化替代 | 舍弃 | 舍弃 | **三方一致：舍弃** |
| ResearchState 内存模式 | 重构为持久化 | 舍弃（严重度 8/10） | 重构 | **三方一致：改为持久化** |
| FeatureCard 作为顶层模型 | 降级为 ClaimTree 注解 | 降级（严重度 7/10） | 降级为摘要层 | **三方一致：降级不删除** |

### 4.2 该保留什么（三方共识）

| 现有组件 | 判定 | 理由 |
|---------|------|------|
| 13 步的算法逻辑代码 | **保留**，改接口为 Skill | 算法本身没问题，编排方式有问题 |
| SQLite 数据库 | **保留为唯一存储** | 不引入 Git 存储，用表结构模拟版本管理 |
| AI 客户端 | **保留** | 与架构无关 |
| 分层报告生成 | **保留** | 输出格式和存储解耦 |
| diff_strings / LCS 算法 | **保留** | 纯算法，与架构无关 |
| FeatureCard 5 维数据 | **保留为摘要** | 降级但不删除 |

### 4.3 主要矛盾（战略 ZL 判定）

**主要矛盾（严重度 9/10）**：单次线性 Pipeline vs 迭代式分支实验
— 根源：runner.rs 假设 Step(i)→Step(i+1)，不支持回退、分支、循环

**次要矛盾**：
- 严重度 8：ResearchState 内存临时 vs 需要持久化可追溯
- 严重度 7：FeatureCard 通用 5 维 vs 需要权利要求级精确拆解
- 严重度 6：SQLite 单体 vs 实验环境隔离需求

### 4.4 资源分配（综合三方）

| 方向 | 占比 | 说明 |
|------|------|------|
| Pipeline→状态机重构 | **35%** | 主要矛盾，一步到位 |
| 实验验证引擎 | **25%** | 决策：先做 |
| ClaimTree 特征下沉 | **20%** | 对接已有 FeatureCard |
| 状态持久化 + 版本管理 | **15%** | SQLite 表模拟分支 |
| Findings Memory | **5%** | 在上述基础上自然产生 |

---

## 5. 架构决策记录

| 决策项 | 最终选择 | 否决方案 | 理由 |
|--------|---------|---------|------|
| 迁移策略 | **一步到位** | 4 阶段渐进 | 渐进会留大量兼容代码，维护成本高 |
| 存储模型 | **纯 SQLite** | SQLite+Git 混合 / 纯 Git | 混合有同步风险和双写复杂度；纯 Git 跨课题检索慢 |
| 实验验证引擎 | **先做**（P0） | 放到 P1/P2 | 研发用户能跑实验，真实数据对专利价值最大 |
| Git 分支语义 | **SQLite 表模拟** | 真实 Git 子仓库 | 保持单体架构简洁性 |
| FeatureCard 处置 | **降级为 ClaimTree 摘要** | 删除 / 保持原样 | 已有数据和 API 继续可用 |

---

## 6. 不采纳的部分

| 方案 | 不采纳理由 |
|------|-----------|
| Git 子仓库存储课题数据 | 增加同步复杂度，SQLite 表可模拟版本语义 |
| npm/Python daemon 架构 | Rust 单体零依赖是核心优势 |
| TUI 终端界面 | 已有 Web + MCP，投入产出比低 |
| IM 连接器（QQ/微信/Telegram） | 创新验证需要专注环境 |
| 论文 LaTeX 编译 | 专利交底书用 docx |
| 4 阶段渐进迁移 | 一步到位，避免兼容代码积累 |

---

## 7. v0.6.0 执行计划

```
v0.6.0 — 从「一次性验证」到「持续创新研究」
存储：纯 SQLite，一步到位重构
策略：先做实验验证引擎

1. Pipeline 状态机重构（35%）
   → runner.rs 拆为 Skill trait + Orchestrator 状态机
   → 支持迭代、跳转、回退
   → pipeline_snapshots 表废弃，改为 idea_steps 版本表

2. 实验验证引擎（25%）
   → AI 生成验证脚本（Python/Rust）
   → 子进程隔离运行，捕获标准输出中的指标
   → 实验数据自动填入报告

3. ClaimTree 权利要求级特征（20%）
   → 新增 ClaimNode + TechnicalFeature 结构
   → FeatureCard 降级为 ClaimTree 的可读摘要
   → GET /api/idea/:id/patentability 授权率预测

4. SQLite 版本管理表（15%）
   → idea_versions 表：记录创意演进历史
   → idea_branches 表：管理规避设计路径
   → findings 表：跨课题知识积累

5. 持续迭代 API（5%）
   → POST /api/idea/:id/iterate
   → ResearchState 持久化到 idea_research_state 表
```

> **核心定位声明**：InnoForge 面向**研发用户**（发明人、工程师、技术负责人），
> 而非专利代理人或法务人员。研发用户懂代码、懂 Git、能跑实验——
> 这决定了 DeepScientist 的技术理念可以高度移植，但存储模型保持 SQLite 单体简洁。
