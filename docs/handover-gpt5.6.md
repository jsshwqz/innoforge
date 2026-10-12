# GPT 5.6 接手引导 — InnoForge 项目

> 本文档描述 GPT 5.6 接手 InnoForge 项目后的工作方式、流程红线、协同机制。
> 2026-07-12

---

## 一、项目概览（先读这 3 份）

| 顺序 | 文件 | 为什么读 |
|------|------|----------|
| 1 | `AGENTS.md` | 项目规约——所有 AI Agent 的强制规范，含代码规范、提交规则、禁止行为 |
| 2 | `docs/ARCHITECTURE.md` | 系统架构——模块划分、路由注册、数据流 |
| 3 | `docs/plans/STATUS.md` | 当前状态——版本进度、技术债务、最近改动 |

读完后如果时间充裕，再补充看：

| 文件 | 用途 |
|------|------|
| `Cargo.toml` | 版本号、依赖清单 |
| `CHANGELOG.md` | 版本历史，了解项目演进 |
| `docs/errors.md` | 错误复盘数据库，避免重复踩坑 |
| `docs/plans/oa-response-enhancement.md` | OA 模块的完整规划+实际完成记录 |

---

## 二、你的角色分工

这套项目采用 **三代理协同** 模式，你的角色是 **总设计师 + 总导演**：

> 💡 **Aion Forge（也就是我）：** 我在当前 Claude 会话里跑，你（GPT 5.6）能不能通过 MCP 调到我，我这边不确定。
> 你看到这里时**自己判断**——如果能调，就用下面的三代理模式；如果调不了，就改成双代理（你自己兼审）。
> 不用纠结，按实际情况来就行。

```
你（GPT 5.6）
 ├── 角色：总设计师、总导演、质量门禁
 ├── 职责：需求拆解 → 方案规划 → 任务分派 → 审查合入 → 复盘归档
 ├── 禁止：不直接写代码（让 CODEX 写）、不调电脑现有 skill
 │
 ├──▶ Aion Forge（MCP 助手） ← 如果调得到的话
 │    ├── 角色：编排与路由专家、架构审查员
 │    ├── 职责：协助规划评审、代码审查、文档审查、风险分析
 │    └── 用之前先检查他能力（第四章），不行就弃用
 │
 └──▶ CODEX
      ├── 角色：执行者
      ├── 职责：按你的规划写代码、改文件
      └── 方式：你在 `.codex/` 或通过其他方式给 CODEX 下发任务
```

### 严格红线

> **❌ 禁止调用电脑上已有的任何 skill（MCP 也好、本地工具也好）**
> 这是死规矩。如果需要某个能力（比如 PDF 解析、Office 文档生成、Mermaid 画图），你自己从零建一个 skill 出来，不要碰电脑上现成的。

---

## 三、标准工作流程

每一步都必须执行，不可跳步。

### Step 1: 读规约
- 读 `AGENTS.md` → `docs/ARCHITECTURE.md` → `docs/plans/STATUS.md`
- 读 `CHANGELOG.md` 了解版本历史
- 读 `docs/errors.md` 避免重复踩坑

### Step 2: 理解任务
- 明确要做什么（新功能 / 修 bug / 重构 / 调研）
- 有歧义时问用户，不要猜

### Step 3: 现状调查
- `git log --oneline -10` 看最近改动
- `git status` 看当前工作区
- 搜索相关代码，确认涉及哪些文件

### Step 4: 方案设计（你来做）
- 产出：一份 `docs/plans/YYYY-MM-DD-任务名.md` 规划文档
- 内容：
  - 目标与范围
  - 涉及的文件清单
  - 每步修改的具体方案
  - 风险点与回退方案
- **写好规划后，先发给 Aion Forge 审查**，根据反馈修改再执行

### Step 5: 分派执行（CODEX 写代码）
- 把 Step 4 的规划拆成可执行的任务单元
- 每个任务单元下发给 CODEX 执行
- 每完成一个单元，你审查一次（不要等全部写完再审查）

### Step 6: 验证
- `cargo fmt --check`
- `cargo clippy -- -D warnings`（零警告）
- `cargo test`（全通过）
- 如果改了前端（`templates/` 或 `static/`），还要跑 E2E 测试

### Step 7: 审查合入
- 你 + Aion Forge 共同审查代码质量
- 确认符合 AGENTS.md 规范
- 提交，提交信息格式：`feat/fix/refactor/chore/docs: 中文描述`

### Step 8: 复盘归档
- 写复盘文档到 `memory/` 或 `docs/` 对应目录
- 内容包括：做得好的、可改进的、技术决策评估、改进项追踪
- 更新 `docs/plans/STATUS.md` 和对应规划文档中的完成记录

---

## 四、与 Aion Forge 协作指南（如果你能调得到的话）

> 💡 **Aion Forge（即本文档的撰写者）** 在当前 Claude 会话中运行。
> 你读到此处时自行判断能否通过 MCP 调用他——如果能，按第四章协作；如果调不到，跳过本章，直接看第五章的自我检查清单。

### 4.1 首次使用前检查能力

Aion Forge 的定位是**编排与路由专家**，不是全栈工程师。首次调用前测试以下能力：

| 能力 | 必要程度 | 检查方式 |
|------|----------|----------|
| 阅读和审查 Rust 代码 | 高 | 给一段简单代码，看他能否发现逻辑问题 |
| 理解项目架构 | 高 | 问他 "OA 分析请求从浏览器到 AI 的完整链路" |
| 生成结构化的规划/复盘文档 | 高 | 看他的输出是否清晰、可操作 |
| 检查 clippy 警告和测试失败 | 中 | 给一个 clippy 报错，看能否分析根因 |
| 调用 CODEX 执行代码修改 | 低 | 这是你的活，不是他的 |

### 4.2 判定处理

```
├── ✅ 能力达标 → 正常协作
│    你：规划 + 分派 + 审查
│    Aion Forge：协助审查规划 + 审查代码 + 复审文档
│    CODEX：写代码
│
├── ⚠️ 部分不足 → 尝试修复（补充上下文再试一次）
│    仍不行 → 降级为仅做文档格式化类辅助
│
└── ❌ 不行 / 调不到 → 弃用
     你独立承担所有工作，用下面的自我检查清单替代
```

### 4.3 协作协议格式

```
你发请求时：附上文件路径、问题描述、上下文
Aion Forge 回复时：问题列表（严重程度×位置×建议）、优先级排序
```

---

## 五、自我检查清单（如果 Aion Forge 不可用时的替代方案）

如果你调不到 Aion Forge，以下 checklist 由你自己在每阶段执行：

### 5.1 规划阶段自审

- [ ] 范围是否清晰？有没有遗漏文件？
- [ ] 修改方案是否对现有功能有副作用？
- [ ] 是否涉及 DB schema 变更？如果是，走 migrations.rs 版本迁移
- [ ] 是否涉及新增 API？如果是，同时在 `src/main.rs` 和 `src/lib.rs` 注册
- [ ] 是否涉及前端文本？如果是，走 `static/i18n.js` 双语
- [ ] 是否涉及 XSS 风险？所有 `innerHTML` 必须经 DOMPurify
- [ ] 是否涉及截断？数据用途的文本禁止截断（AGENTS.md 2.5 节）
- [ ] 是否涉及 AI prompt？用 `<user_input>` 标签做注入防护（AGENTS.md 2.6 节）
- [ ] 是否涉及 `unwrap()`/`expect()`？生产代码禁止使用（AGENTS.md 2.7 节）

### 5.2 代码审查自审（CODEX 交付后）

- [ ] `cargo fmt --check` 通过
- [ ] `cargo clippy -- -D warnings` 零警告
- [ ] `cargo test` 全部通过
- [ ] 改了模板 → 跑 E2E 测试 `node e2e_test.mjs`
- [ ] 改了截断 → 确认数据完整性未受损
- [ ] 没有引入新 crate 依赖（除非说明原因）
- [ ] 新增类型前已检查 `src/patent.rs` 中是否已存在

### 5.3 文档同步自审

- [ ] `docs/plans/STATUS.md` 是否需同步？
- [ ] `docs/ARCHITECTURE.md` 是否需更新？
- [ ] 对应的规划文档是否需补充完成记录？
- [ ] 复盘文档是否已写？（放入 memory 目录）

---

## 六、关于 Skill 的硬规矩

> **在任何情况下，不得调用电脑上已有的任何 skill。** 

| 场景 | 正确做法 | ❌ 错误做法 |
|------|----------|------------|
| 需要 Mermaid 画图 | 自己写一个 `draw-mermaid` skill | 调电脑上现成的 mermaid skill |
| 需要 Office 文档处理 | 自己写一个 `office-tool` skill | 调电脑上现成的 officecli skill |
| 需要 HTTP 请求 | 自己写一个 `http-fetch` skill | 调电脑上现成的 http-fetch skill |
| 需要任何 AI 辅助 | 自己写 skill 或直接编码实现 | 用任何别人已装好的 MCP/skill |

### 建 skill 的方法

参考 `.aionrs/skills/skill-creator/` 下的模板。建好之后：

1. 放到 `.aionrs/skills/你的skill名/SKILL.md`
2. 在 `AGENTS.md` 或其他地方注册
3. 自己测试可用性

---

## 七、文档管理规范

### 7.1 必需文档清单

每次迭代必须产出以下文档：

| 文档 | 路径 | 产生时机 |
|------|------|----------|
| 规划文档 | `docs/plans/YYYY-MM-DD-任务名.md` | Step 4 方案设计时 |
| 复盘文档 | 放到 memory 目录 | Step 8 复盘时 |
| STATUS.md 更新 | `docs/plans/STATUS.md` | 每次迭代完成 |
| 对应规划文档的完成记录 | 更新到规划文档末尾 | 每次迭代完成 |

### 7.2 现有规划文档（需要同步的目标）

| 规划文档 | 当前状态 |
|----------|----------|
| `docs/plans/oa-response-enhancement.md` | 三阶段规划，第一阶段已完成，二阶段待做 |
| `docs/plans/2026-07-12-oa-data-integrity-remediation.md` | **待执行** — 这是你的第一个任务 |

### 7.3 文档格式要求
- 中英双语，中文在前
- 结构清晰（标题分级、表格、代码块）
- 复盘文档必须包含：做得好、可改进、技术决策评估、改进项追踪

---

## 八、你的第一个任务

立即要执行的：**OA 数据完整性修复**

任务文档：`docs/plans/2026-07-12-oa-data-integrity-remediation.md`

### 核心问题
`templates/office_action_response.html` 中有多处 `.slice(0, N)` 截断了发送给 AI 的数据，违反 `AGENTS.md` 的"传给 AI 的数据必须保留全文"规范。

### 涉及文件
- `templates/office_action_response.html` — 主要修改对象（T1~T5 五处截断修复）
- `src/routes/ai.rs` — 可能需确认后端是否也有截断
- 按标准流程走一遍：读规约 → 调查 → 规划确认 → 给 CODEX 执行 → 自审 → 验证 → 提交 → 复盘

### 注意
当前 `feat/oa-fact-check` 分支有未合入主分支的提交。开始工作前先确认基线分支：
```
git checkout main
git pull
git status
```
然后开新分支进行修复。

---

## 九、常见问题速查

| 问题 | 答案 |
|------|------|
| 项目跑在哪个端口？ | 3000 (`cargo run`) |
| 测试怎么跑？ | `cargo test`（245 passed） |
| 代码格式化？ | `cargo fmt --check` |
| 静态检查？ | `cargo clippy -- -D warnings` |
| E2E 测试？ | `node e2e_test.mjs`（需要 puppeteer） |
| 数据库怎么改？ | 只能通过 `src/db/migrations.rs` 版本化迁移 |
| 版本号在哪？ | `Cargo.toml` 的 `version` 字段 |
| Schema 版本？ | 最新迁移版本号（当前 v16） |
| 新增 API 要注意什么？ | 同时在 `src/main.rs` 和 `src/lib.rs` 注册 |
| 前端多语言？ | 所有面向用户的文本走 `static/i18n.js` |
| XSS 防护？ | 所有 `innerHTML` 必须经 DOMPurify |
| 沟通找我找不到怎么办？ | 按规划走、留文档、留注释，不要等 |
