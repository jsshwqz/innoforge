# InnoForge 完整重构 — 主控文档 / MASTER

> **本文件是所有执行 agent 的唯一入口。每次会话开始必读本文档，从"当前状态"继续，禁止重开炉灶。**
> 任务：InnoForge（D:\test\patent-hub-backup）整体重构 —— 路线 A 重建式重构，终态 workspace 全新多 crate 架构
> 制定日期：2026-08-15 ｜ 分支策略：直接在 dev ｜ 发布版本：0.1.0（全新版本线）
>
> ## 角色分工（硬约束）
> - **规划会话**：只写/维护方案与文档，不写产品代码；方案变更以 docs commit 形式下发
> - **执行 agent**：按本文件与 phase 清单施工，只勾选进度、追加 Notes，**不得改写任务定义或验收标准**；发现方案与现实冲突时停下上报，由规划会话修订

---

## 1. 权威文档链（按需阅读，不必全读）

| 文档 | 内容 | 何时读 |
|------|------|--------|
| **`docs/plan/product-vision.md`** | **产品愿景与九段旅程到位标准（验收解释基准）** | **任何验收判断时** |
| **`docs/plan/prd-v1.md`** | **产品需求与验收口径** | **任何施工前** |
| **`docs/plan/2026-09-19-capability-uplift-plan.md`** | **实测复核版开工指令**：清场前置、MA3/MB1 半程校准、M-A/M-B 顺序调整、防停摆机制 | **接手第一步（先于 task-breakdown 原序）** |
| `docs/plan/task-breakdown.md` | v4 功能里程碑主轴（M-A/B/C 任务+验收）+ 结构支撑件挂载表 | **每个任务开工前** |
| `docs/progress/GATES.md` | 门禁速查卡（可复制命令） | **每个任务收尾时** |
| `docs/plan/milestones.md` | 里程碑判定（M0-M5，结构版措辞以 v4 为准） | 每里程碑收尾时 |
| `docs/plan/delivery-boundary.md` | 0.1.0 交付边界声明 | 发布前 |
| `docs/analysis/search-sources-spec.md` | **M-A 施工规格书**（统一契约/各源端点与坑位/执行链策略/冒烟清单） | **M-A 任何任务开工前** |
| `docs/analysis/routes-inventory.md` | 118 条路由防丢失基准表 | 动路由的任务 |
| `docs/analysis/types-migration-map.md` | 类型迁移施工图纸 | T1.1 及动类型前 |
| `docs/analysis/module-inventory.md` | 模块评分、行号级病灶证据 | 动对应模块前 |
| `docs/analysis/risk-assessment.md` | 风险 Top10 + 已复核缺陷清单 | 每里程碑开工前 |
| `docs/analysis/project-overview.md` | 架构全景、技术栈、26 表清单 | 首次进入项目时 |
| `AGENTS.md` | 项目强制规约 | **始终有效** |

## 2. 用户已拍板的决策（不得重新讨论）

1. 未提交改动：审阅后提交 ✅ 已执行
2. 死代码按质量裁决：fact_check=保留接线；rag=重写并入；vector/search 内联版=重写合一（详见 task-breakdown Phase 0.5）
3. 分支：**直接在 dev 上做**（dev 需先合并 main 最新状态）
4. 版本号：全新版本线，发布 **0.1.0**
5. 结构路线：A 重建式重构——**但 2026-08-15 方向修正后降级为支撑件**：用户澄清重构动机是产品功能不达预期（检索失败/中文给英文/召回不全；分析浅/幻觉/丢上下文），目标形态=个人本地利器，全流程主线全要
6. **最高依据：`docs/plan/prd-v1.md`** —— 一切施工以 PRD 验收口径为准绳；task-breakdown v4 为功能里程碑主轴（M-A 可信中文检索 → M-B 可信深度分析 → M-C 全流程贯通）；v3 结构任务除挂载件外全部归档
7. 执行方式：其它 agent 依据本方案自动执行，无需逐项请示；仅在验收标准无法达成或发现方案性错误时上报

## 3. 阶段总览（v4 功能主轴）

- [ ] **M-A 可信中文检索**（1/6，PRD N1-N4；MA1 ✅ 2026-09-20）→ 施工明细见 task-breakdown.md「里程碑 M-A」表
- [ ] **M-B 可信深度分析**（0/6，PRD N5-N8）→ 见 task-breakdown.md「里程碑 M-B」
- [ ] **M-C 全流程贯通与交付**（0/5）→ 见 task-breakdown.md「里程碑 M-C」
- [ ] Phase 0 地基与卫生（5/13，继续有效）→ [details](./phase-0-foundation.md)
- 📦 归档：phase-1~6 结构版文件保留作参考，**以 task-breakdown v4 为准**；结构支撑件挂载关系见其"结构支撑件"表

## 4. 执行协议（每个 agent 必须遵守）

### 4.1 每个任务的完成定义（DoD）
1. 功能行为与重构前完全一致（除任务明确要求的行为修复）
2. 全套门禁绿：`cargo fmt --check` ＋ `cargo clippy --all-targets -- -D warnings` ＋ `cargo test`（≥161 通过）＋ 改动 templates 时 `node check_html_functions.mjs` ＋ `node e2e_test.mjs`（54+ 通过）
3. 新代码遵守 AGENTS.md：生产路径零 unwrap/expect；新类型查重后放 types/ 或模块内并带 derive(Debug,Clone,Serialize,Deserialize)；API 错误走 AppError；prompt 留在 handler 所在模块
4. 提交格式 `refactor|fix|feat|chore|docs: 中文描述`；**一个任务一个 commit，完成后秒级提交**（防并行互扫）
5. 回写进度：勾选对应 phase 文件的 checkbox + 更新本文件第 3 节计数与"当前状态"

### 4.2 并行泳道
- 泳道 A（src/ 后端）：Phase 1→2→3→4 串行
- 泳道 B（templates/+static/ 前端）：Phase 5 的 T5.1-T5.5 可与泳道 A 并行
- 两泳道文件零交集；同泳道内禁止两个 agent 同时改同一文件

### 4.3 禁止事项（违反即返工）
- 禁止顺手改业务逻辑/文案/API 形状（行为变化另开 fix 任务）
- 禁止引入新 crate 依赖（确需则停下上报）
- 禁止改 DB schema（本轮零迁移；SCHEMA_VERSION 对齐除外）
- 禁止削弱测试/扫描器/基线来"转绿"
- 禁止 `git add -A`（只 add 自己任务触碰的文件）；禁止动他人未提交工作

### 4.4 异常处理
- 门禁红了先定位：是本次改动引起 → 修复；是既有问题 → 记入 phase 文件 Notes 并上报，不得跳过
- 发现方案与代码现实冲突：以可验证的最新代码为准，更新对应文档并在 commit message 说明

## 5. 当前状态（每 session 开始/结束更新此节）

> **🤝 交接状态（2026-09-20 第十二次更新）：MA2a（Google Patents XHR 直抓源 + SourceChain 多源执行链）已完成，分支 `exec/ma2a-xhr`，代码由第六棒写完、第七棒收尾（第六棒在准备收尾阶段 43 分钟零心跳挂死，监工已先推其提交保护）。提交链 `0c37569`(接手第五棒契约半成品并清零其编译告警) → `0ac73e7`(源 + SourceChain) → `4bca304`(降级链三用例 + XHR 源离线实证单测) → `52948ad`(`/api/search/online` 接上链路)；变更 8 文件全在 `src/` 下，+2054/−133。**验收数字（第七棒 tip `52948ad` 复跑，rustc/clippy 1.98.0）**：`cargo fmt --check` exit 0 ｜ `cargo clippy --all-targets -- -D warnings` exit 0（六个改动文件 `touch` 后强制复检，非缓存假绿）｜ `cargo test` **525 passed / 0 failed / 3 ignored**（main 基线 457 → 525；净增 68 = 新增 **34** 条 `#[test]`/`#[tokio::test]`（18+16）在 lib(203→237) 与 bin(205→239) 两个测试二进制各计一次，逐名对账闭合）；**红线自查全过**：`src/common.rs` 零 diff（71 条 `.route()` 未动）、templates/ 与 static/ 零 diff ⇒ HTML 扫描与 Puppeteer e2e 按 DoD 条件不适用、`Cargo.toml`/`Cargo.lock` 零 diff（无新依赖）、`src/db/` 零 diff（无 schema 变更）、生产路径零 `unwrap`/`expect`（20 处命中点全部落在 `#[cfg(test)]` 内）。**MA2a 验收锚点**：spec §6 三用例（①SerpAPI 挂→XHR 降级仍非空且 `winning_source` 如实标注；②XHR 撞实测 503 反爬→退避重试→链路不判死；③双源挂→空结果但 `attempts` 按源各带 `FailKind` 且 error 非空点名来源），**全程注入假传输/假时钟、零联网**。上游超时按 spec §1 收紧为 XHR **15s**，SerpAPI 维持旧 30s（MA6 口径）。**下一位执行 agent 认领：MA2b EPO OPS（规格书 §4）**，随后 MA5 诊断面板（消费 `attempts` 全量）。**发包纪律重申：一个任务 + 一轮门禁 = 一个包**；第六棒的死因是收尾阶段一次性大放送，收尾者应把门禁/文档/推送拆成多次小动作各自动作落盘。

> **🤝 交接状态（2026-09-20 第十一次更新·施工明细）：MA1 施工取证（原"提交待审"块，随 PR #11 已合并，合并后全量数字见上方块）。** 本分支取到的三个"前任遗产"事实：① `07bab1b` 的 `src/search/` 五文件**从未接入模块树**（`lib.rs`/`main.rs` 均无 `mod search;`），所以 `cargo check` 通过是零信息量的假绿，那批代码根本没被编译过；② `b08f1c5` 的 T1.1 类型地基经门禁复跑确认仍绿，**视为已完成**成立；③ 接线即成片报错（`PatentSummary` 缺 `PartialEq`/`Clone`）反向坐实 ② 之外那份 WIP 从未编译。**处置=续建不重写**（契约建模忠实 spec §1，且已自带 29 个用例；MA1 全部代码与文档改动见本分支 commit 与 PR 正文）。**门禁数字（rustc/clippy 1.98.0）**：`cargo fmt --check` exit 0 ｜ `cargo clippy --all-targets -- -D warnings` exit 0 ｜ `cargo test` **457 passed / 0 failed / 3 ignored**（8 个测试二进制：lib 203 + bin 205 + 集成 49）｜ templates/ 与 static/ **零改动**，故 HTML 函数扫描与 Puppeteer e2e 两道门禁按 DoD 条件（"若动 templates/ 或 static/"）**不适用**，未跑。
>
> **🤝 交接状态（2026-09-20 第十一次更新）：MA1 + T1.1 已由 PR #11 合并落地（merge commit `58fdabe`）。施工历经三棒两救：agent1 完成 `b08f1c5`(T1.1) 后死于模型超时、agent2 完成 serpapi 迁入+四域接线后死于 150 轮上限（两棒均由监工 `WIP 原样保存` commit 代存防丢），agent3 以 39 轮完成收尾——**证明包尺寸是本轮瓶颈，后续一律"一个任务+一轮门禁"一个包**。验收数字：fmt/clippy --all-targets 双 0；test **457 passed / 0 failed / 3 ignored**（369→457 = T1.1 +36、MA1 +52）；`common.rs` 71 条 `.route()` 与 main 逐条 diff=0（端点形状零变化）；templates/static 零改动（e2e 按 DoD 条件免跑）；GitHub CI 三 job 全绿后合并。`src/search/` 已备 MA2/MA5 消费点（`SourceKind::GooglePatentsXhr/EpoOps/LocalFts`、`AttemptStatus::Skipped`、`MergedPatent`、`SearchOutcome::attempts` 带 allow(dead_code)）。**下一位执行 agent 认领"下一步"第 3a 项：MA2a Google Patents XHR 直抓（规格书 §2，含限速退避；验收=禁用 SerpAPI 后降级仍出结果）**；MA2b EPO OPS（§4）随后单独发包。上游超时收紧 15s 归 MA2 顺带处理（MA1 遗留已登记）。
>
> **🤝 交接状态（2026-09-20 第十次更新）：PR #10 已合并（merge commit `faf60f6`，GitHub 记录 2026-09-20T03:05Z）——main 取得史上首个 CI 绿色基线（合并前 GitHub 三 job 实测全绿：lint 18s / test 1m54s / e2e 2m1s，run `35485167886`）。T0.5/T0.6 已由规划会话直接完成（均为纯快进、零冲突，不构成施工）：dev `7fc1aec→faf60f6`（origin 已推，dev 分支 CI 已被点亮，run `35486017267`）；gitee main `8e9b6dc→faf60f6`、dev 同步（落后 73 提交的一次性收齐）。至此 origin/main == origin/dev == gitee/main == gitee/dev == 本地 main == `faf60f6`。陈旧开放 PR #1/#2 已关闭（基点过期，如需保留基于当前 main 重开）。下一位执行 agent 从"下一步"第 3 项 **M-A 开工（MA1 + T1.1 缩水版类型地基）** 开始，从当前 main 开出 `exec/*` 分支即自带全套绿色门禁基线。**

> **🤝 交接状态（2026-09-20 第九次更新）：CI 绿色基线包已交付为 PR #10（分支 `exec/ci-green`，自 `adae26f` 开出），未合并、未动 main/gitee、未开工 M-A。六项收口：① `package-lock.json` 入库 + AGENTS.md §4.3 改写（消 npm ci 红）；② clippy 存量 4 处惯用法修复（`--all-targets -D warnings` 首次 exit 0）；③ `rust-toolchain.toml` 钉 1.98.0（防漂移根因）；④ `.gitignore` 收编 226 个根目录调试产物（未跟踪项归零，已核实 0 个已跟踪文件被新规则命中）；⑤ HTML 函数基线 `--refresh`（消待刷新提示，零函数消失）；⑥ ESLint 迁移 flat config + 修 `static/i18n.js` 2 处真实 no-redeclare（消掉被 npm ci 掩盖多年的假绿门禁）。本地门禁全绿：fmt exit 0 / clippy --all-targets exit 0 / test 369 passed 0 failed 3 ignored / HTML 扫描 exit 0 / Puppeteer e2e 54/54 PASSED / eslint 0 error。下一位仍按"下一步"第 1、2 项（T0.5 切 dev 合 main、T0.6 双远端推送）接手；**PR #10 的 GitHub CI 已三 job 全绿**（lint 18s / test 1m54s / e2e 2m1s，run `35485167886`，明细见 §6），合并后 main 即取得首个绿色基线。**
>
> **🤝 交接状态（2026-09-20 第八次更新）：PR #9 已由用户授权、规划会话代为合并（merge commit `4ed7f3e`，GitHub 记录 2026-09-20T01:21Z）。main 血统彻底收编：origin/main == 本地 main == `4ed7f3e`（含 74 提交 + 清场分诊 + PR#8 对齐），fmt 绿基线，工作区干净。合并前经用户裁决修改了 ruleset「研创台」：移除 `required_linear_history`（它禁 merge commit 且无人可 bypass，与"保留完整历史"规约冲突），保留 deletion/non_fast_forward/creation/code_quality 四条。下一位执行 agent 从"下一步"第 1 项（T0.5 切 dev 合 main）接手；M-A 分支从现在的 main 开出即带干净基线。CI 仍有两个存量红（npm ci 缺锁文件、clippy 4 项）待用户裁决修法，见风险 1/4。**

- **日期**：2026-09-20（第十二次更新：MA2a 完成——Google Patents XHR 直抓源 + SourceChain 降级链接线 /api/search/online，M-A 进度 2/6（MA1+MA2 半程），分支 `exec/ma2a-xhr` 待 PR #12 审核）
- **已完成**：
  1. 扫描分析五件套（含 routes-inventory/types-migration-map）+ 基线修复入库 + 门禁全绿（fmt=0/clippy=0/test 369 通过）
  2. **方向修正落地**：用户痛点口述存档 feedback.md → `docs/plan/prd-v1.md`（痛点根因诊断+可测验收口径）→ task-breakdown v4 功能里程碑主轴（M-A 可信中文检索 / M-B 可信深度分析 / M-C 全流程贯通）；v3 结构任务归档或挂载
  3. GATES.md 门禁速查卡；规划/执行角色分工硬约束
  4. **2026-09-19 实测复核（能力提升方案下发）**：交棒后零实现提交确认；发现文档进度双向失真——**MA3 半程**（SerpAPI 已带 country 透传+auto_cn，`routes/search.rs:259/:315`）、**MB1 半程**（OA 路径 fact_check 已接线，`routes/ai.rs:1416`；idea/流水线仍零调用）；rag 确认仍无调用方；D 盘 25GB 警报解除
  5. **2026-09-19 第 0 段清场（执行 agent，PR #9）**：14 个未提交 src 文件全部分诊收编（7 纯 fmt + 7 fmt/clippy 惯用法，零功能改动、零丢弃、零 WIP 挂起）；`cargo fmt --check` 由红（main 178 处）转绿；test 369 保持；MA3/MB1 半程状态回写 task-breakdown；新查出处环境事实——clippy 1.98 存量 4 处红、GitHub main 与本地 main 分叉（见新增风险节）
- **⚠️ 环境警报**：D 盘剩 25GB（2026-09-19 实测，警报解除但保留规则）。**执行 agent 开工前必查 `Get-PSDrive D`，<5GB 先 clean**
- **执行 agent 下一步（按序，详细依据见能力提升方案 §3）**：
  0. **第 0 段清场（一切施工前置）——✅ 已完成（PR #9，分支 `exec/stage0-triage`）**
     - **处置清单（14 个未提交 src 文件 → 2 个 chore commit；另有 docs commit 若干，全在分支上，main 未动）**：PR #9 的 commit 清单见下表的 `ef647cd` / `9ba1473` / `848eb59` / `148d4a8`，以及本行所在的 hash 补记提交；净增量对比 https://github.com/jsshwqz/innoforge/compare/4f316a2...exec/stage0-triage
       | commit | 文件 | 判定 |
       |---|---|---|
       | `ef647cd` chore: 清场分诊(1/2) | lib.rs / main.rs / orchestrator/engine.rs / pipeline/steps/{finalize,parse}.rs / routes/{pages,upload}.rs（7 个） | **纯 fmt**：与 `rustfmt(HEAD)` 逐字节一致（git archive HEAD → cargo fmt → diff=0 证明） |
       | `9ba1473` chore: 清场分诊(2/2) | db/{memory,vector}.rs / pipeline/steps/{debate,reflection}.rs / routes/{ai,idea,mod}.rs（7 个） | **fmt + clippy 惯用法**：manual_flatten×2、collapsible_if×4、删未用导入×3、去多余 `&….to_string()`×3、`name`→`_name`、测试模块整体移至文件末尾 |
       | `848eb59` docs: 进度回写 | task-breakdown.md（MA3/MB1 半程加注）、MASTER §5/§6 | 进度回写 |
       | `148d4a8` docs: 错误复盘 | docs/errors.md ×3 条 | gh PR 权限绕行 / clippy 1.98 工具链漂移 / 分诊只读取证法 |
     - **零 WIP 保存**：三处担心的"上百行实质改动"（reflection 自动重试 / debate / parse）实测**全部是 fmt 展开造成的行数放大**（如 parse.rs 把领域指标数组一项一行展开）；reflection 自动重试逻辑在 HEAD 内已完整存在。14 个文件无一例业务逻辑/API 形状变化，因此无"WIP 原样保存"commit、无需用户定夺的半成品；未跟踪的 226 个 `_*.txt` / `.firecrawl/` 调试产物按约定未碰未提交。
     - **门禁数字（分支已提交态，rustc/clippy 1.98.0）**：`cargo fmt --check` **绿**（exit 0；对照 main@4f316a2 **红**，178 处 diff / 13 文件）｜ `cargo test` **绿**（369 passed / 0 failed / 3 ignored，8 个测试二进制含 doc-test）｜ `node check_html_functions.mjs` **绿**（8 模板，基线一致，1 条非阻断待刷新提示）｜ `cargo clippy --all-targets -- -D warnings` **红**（4 处，全部为存量，见已知风险）
     - **✅ main 血统对齐（2026-09-19 追加，merge commit `844143f`，父：`fe9705f` + `3a50c49`）**：把 `origin/main`（PR #8 GitHub 合并提交）并入 `exec/stage0-triage`，PR #9 由 CONFLICTING 转 **MERGEABLE**。实际冲突 **12 处**（`merge-tree` 预演记 14，本轮 `src/lib.rs`/`src/main.rs` 自动合并），逐文件 `git diff HEAD...origin/main -- <file>` 核查后**全取本地侧**：远端独有内容本地均已存在——CHANGELOG 3 条 FreeCAD 条目逐字已在（:13/:49/:51）、STATUS `2026-08-12 FreeCAD` 条目已在、`e2e_test.mjs` 两个 CAD 用例已在且本地 `expectedPasses=54 > 远端 51`（取远端即回退）、`src/cad.rs`(add/add) 的端口 `8010`/preview 强校验/无告警系 `9f1a14b` 旧态（本地 `620b263` 刻意改为 `8080` + warn 日志 + 无头 preview 放宽，见 `docs/analysis/risk-assessment.md:128`）、`db/mod.rs` `mod cad;` 已有且 `SCHEMA_VERSION` 22/断言 23 > 远端 18、`db/migrations.rs` v18 已含、两模板 CAD 接线与两处测试断言均已含。**零内容变化的铁证**：合并后树哈希 == 合并前本地树（`627a03f9672fd06fa66927b1dde2213d7fa5a9d6`），`git diff fe9705f HEAD` 输出 0 行。**门禁复跑（merge 态）**：fmt 绿 ｜ test 369 passed/0 failed/3 ignored（持平基线）｜ HTML 扫描绿 ｜ clippy 仍 4 处存量红（未新增、未修、未 allow）。处置明细已贴 PR #9 评论。
  1. T0.5 切 dev 合 main ——**✅ 2026-09-20 规划会话完成**（纯快进 `7fc1aec→faf60f6`，无冲突）
  2. T0.6 推送双远端激活 CI ——**✅ 2026-09-20 规划会话完成**（origin dev 已推、dev CI 点亮 run `35486017267`；gitee main+dev 同步至 `faf60f6`）
  3. **M-A 开工（顺序按能力提升方案调整）**：~~MA1 SearchProvider 契约+SerpAPI 迁入（前置：T1.1 缩水版类型地基）~~ **✅ 2026-09-20 已合并（PR #11，`58fdabe`）** → ~~3a. MA2a Google Patents XHR 直抓（含限速退避）~~ **✅ 2026-09-20 完成（分支 `exec/ma2a-xhr`，提交链 `0ac73e7`/`4bca304`/`52948ad`，待 PR #12 审核；XHR 超时已按 spec §1 收紧为 15s）** → **3b. MA2b EPO OPS（规格书 §4，当前下一步）** → MA5 诊断面板 → MA3 补半程（language/辖区过滤/默认 CN+zh）→ MA4 召回增强 → MA6 SerpAPI 稳健化；**同步建"用户名下专利号"中文检索回归用例集**。**发包纪律：一个任务+一轮门禁=一个包（150 轮上限实证）**
     - **① MA1 ——✅ 2026-09-20 完成，已开 PR #11 待审**（分支 `exec/ma1-searchprovider`，接手链 `b08f1c5`(T1.1)→`07bab1b`(监工代存 WIP)→`fdbaf11`(监工代存 WIP·续建完成)→收尾 docs commit；取证见 §6 同日两条）。**收尾 agent 在 tip `fdbaf11` 复跑门禁**：`cargo fmt --check` exit 0 ｜ `cargo clippy --all-targets -- -D warnings` exit 0 ｜ `cargo test` **457 passed / 0 failed / 3 ignored**（main 基线 369 → 457 = T1.1 类型地基 +36、MA1 接线 +52，与 §6 逐名对账一致）｜ 端点形状对 `main` 逐条比对 **71/71 `.route()` 路径完全一致**（路由单源在 `src/common.rs::build_router`，双入口共用）｜ templates/ 与 static/ 零改动（`git diff main --stat -- templates static` 空）⇒ HTML 扫描与 e2e 不适用。**MA1 无需返工**。下一位从 **② MA2（Google Patents XHR 直抓）** 接手，其余顺序不变。MA2 施工要点（MA1 已铺好的接口，不必再造）：新 provider 落 `src/search/providers/<源名>.rs` 并注册进同目录 `mod.rs`（该处已留注释锚点）；`SearchProvider` 是手工 `Pin<Box<dyn Future>>` 对象安全写法，可直接放进 `Vec<Box<dyn SearchProvider>>` 做链式执行；`SourceKind::GooglePatentsXhr`/`EpoOps`/`LocalFts` 与 `AttemptStatus::Skipped`、`MergedPatent`、`SearchOutcome::attempts` 目前是 `#[allow(dead_code)]` 的**待消费点**，MA2/MA5 落地时删标注即可；规格书§2 的限速退避与 §1 的 15s 超时收紧（MA1 刻意保持旧 30s 未动）都在 MA2 口径内。
     - **② MA2a Google Patents XHR 直抓 ——✅ 2026-09-20 完成（分支 `exec/ma2a-xhr`，待 PR #12 审核）**：按上一行留的施工要点落地——provider 落 `src/search/providers/google_patents_xhr.rs` 并注册进 `providers/mod.rs`（锚点已用掉），`SourceChain` 在 `src/search/chain.rs`（spec §6 并行发起、按登记顺序择胜），`/api/search/online` 在 `routes/search.rs` 登记 `[SerpAPI(有 Key 时) → XHR]`；`SourceKind::GooglePatentsXhr` 与 `SearchOutcome::attempts` 的 `#[allow(dead_code)]` 待消费标注已随本次接线删除（`providers/mod.rs` 的 `mod` 注册锚点亦已用掉）；**仍留待下棒的消费点**：`MergedPatent.key`（MA2a 只有单源胜出、无跨源合并 → MA2b 消费）、`FailKind::cools_down()`（MA2b/MA5 熔断器消费）、`SourceKind::EpoOps`/`LocalFts`（MA2b/MA4 消费）。**上游超时本次按 spec §1 收紧为 XHR 15s，SerpAPI 保持旧 30s**（原要点最后一句的两处口径均已办）。下一位从 **③ MA2b EPO OPS（规格书 §4）** 接手，其余顺序不变。
     - **③ MA3 的证据行号需按 MA1 回写的新位置核对**（旧 `routes/search.rs:259/:315` 已迁移），见 task-breakdown.md MA1 行末注。
  4. T0.2 / T0.3 / T0.4 / T0.7 / T0.8 穿插并行认领；T0.9 提醒用户
- **已知风险提醒**：/api/search/vector 中文 panic（MB3 修）；Docker 出口损坏（T0.7 修）；.env 明文密钥（T0.9 用户动作）；mcp-server 未接线函数带 allow(dead_code) 标注保留
- **🆕 第 0 段清场新增风险（2026-09-19，PR #9）**：
  1. **clippy 门禁在 main 上就是红的（存量 4 处，工具链漂移所致）**：`rag/chunker.rs:22`（unnecessary_min_or_max）、`rag/chunker.rs:72`、`routes/search.rs:688`、`vector/mod.rs:73`（for_kv_map×3）。当前工具链 rustc/clippy **1.98.0**（2026-08-18），本节上方"已完成"第 1 条记的"clippy=0"是旧工具链结论；仓库无 `rust-toolchain` 钉版。清场**未修也未加 allow 掩盖**（越界 + 违禁），修复建议留用户定夺：search.rs 属 MA3/MA4 施工对象、rag/ 属 MB2 重写对象，顺势修比孤立补丁更划算。
     **→ ✅ 2026-09-20 已消解（CI 绿色基线包，分支 `exec/ci-green` / PR #10）**：两处同时收口——① 4 处按 clippy 建议做惯用法替换（去恒真 `.max(0)`；`iter_mut()`→`values_mut()`×3），零逻辑变化，`rag/`、`search.rs` 的后续重写不受影响（改的是迭代器写法不是业务逻辑）；② 新增 `rust-toolchain.toml` 钉 `channel = "1.98.0"`（+ components clippy/rustfmt、profile minimal），门禁语义从此与"本机/runner 恰好装了什么"解耦，MA3/MA4/MB2 期间不会再凭空多出一批无关红。`cargo clippy --all-targets -- -D warnings` 现 **exit 0**。踩坑：钉版后本机 rustup 把 `1.98.0` 当新工具链要联网重装而 TLS 中途断，须走镜像补装（`docs/errors.md` 2026-09-20 同名条目）。
  2. **GitHub main 与本地 main 已分叉**：`origin/main=3a50c49`（PR #8 接入 FreeCAD 可视化对话，本地无此提交），本地 `main=4f316a2` 领先远端 60+ 提交。合并 PR #9 会在 11 个文件冲突（CHANGELOG.md、docs/plans/STATUS.md、e2e_test.mjs、src/cad.rs（add/add）、src/common.rs、src/db/{migrations,mod,tests}.rs、templates/{idea,patent_detail}.html、tests/{orchestrator_integration,patent_hub_integration}.rs）；**冲突源是两条 main 的分叉，不是本 PR 的施工**（本 PR 的 lib.rs/main.rs/routes/mod.rs/ai.rs 均可自动合并）。复核命令：`git merge-tree --write-tree exec/stage0-triage origin/main`。处置属用户动作（先对齐 main 或先在 GitHub 上处理 #8 线）。
     **→ ✅ 2026-09-19 已在分支侧消解（本项原文保留）**：`exec/stage0-triage` 追加 merge `844143f` 并入 `origin/main`，PR #9 转 MERGEABLE；核查证明远端侧无独有内容（合并后树哈希不变），**但本地 `main` 与 GitHub `main` 的分叉仍未解**——合并 PR #9 时 GitHub 侧会一次性把 PR#8 血统对齐收进 main，此后 `main` 才与本地同线；T0.5/T0.6（dev 合 main、双远端推送）仍按序执行。
  3. **HTML 函数基线待刷新（非阻断）**：`settings.html` 有 2 个新定义（`formatNumber`、`loadAiCostStats`）未纳入 `docs/functions-manifest.json`；扫描仍绿，需与模板变更同提交时跑 `--refresh`，本次未动模板故不改基线。
     **→ ✅ 2026-09-20 已刷新（PR #10）**：`node check_html_functions.mjs --refresh` 后 manifest 仅新增这 2 个定义 + `generatedAt`（numstat 3 增 1 删，删除行即时间戳），**无任何函数从基线消失**，扫描规则未动；待刷新提示归零，`node check_html_functions.mjs` 干净 exit 0。
  4. **🆕 CI `npm ci` 缺锁文件（2026-09-19 血统对齐实测发现，非本分支引起）**：lint job 9 秒即挂（`npm error code EUSAGE`），因 `package-lock.json` 在本地存在但**从未提交**（`git log --all -- package-lock.json` 为空）；test job 的 node deps 与 e2e 亦走 `npm ci`，同样受影响。修法与 AGENTS.md §4.3「Node.js 相关文件不应出现在本仓库」相互冲突（提交锁文件 vs 改 CI 用 `npm install`），**属用户决策，执行 agent 未擅自处理**。详见 `docs/errors.md` 同名条目。
     **→ ✅ 2026-09-20 已消解（PR #10，按 feedback.md 2026-09-20「小问题按建议直接办」取推荐方案）**：走"提交锁文件"这条路，同时把 §4.3 该条改写清楚——本意是**禁止前端构建工具链（webpack/vite）**，开发工具的依赖清单（`package.json`/`package-lock.json`）必须入库以保证 CI 确定性；`.gitignore` 同步移除对锁文件的忽略项并留注释。锁文件与 `package.json` 已核对同步（`npm ci --dry-run` exit 0，根节点四项依赖逐字一致），CI 仍用 `npm ci`，未削弱可复现性。
  5. **🆕 CI 的 `npx eslint` 步骤是"假绿门禁"，修好 npm ci 后必现红（2026-09-20 本地复现 CI 命令时发现）**：ESLint 10 不再读取 `.eslintrc.json`，`npx eslint static/i18n.js` 直接 exit 2（该不兼容 2026-07-13 已在 `docs/records/2026-07-13-oa-data-integrity-retrospective.md` 记过，但从未修）；配置修好后又暴露 `static/i18n.js` 内 2 处**真实** `no-redeclare`（§2.5 禁止形态）。**处置**：规则逐条等价迁移到 `eslint.config.js`（不新增/不降级任何检查）+ 三处 `var len`→`let`（零行为变化）+ 删除失效的 `.eslintrc.json`；GitHub 与 Gitee 两份 workflow 的调用命令未动。取证与预防见 `docs/errors.md` 2026-09-20 条目。**另注**：AGENTS.md Step 5 的本地命令路径 `templates/static/i18n.js` 不存在（真实路径 `static/i18n.js`），本轮按规约未改 AGENTS.md 该节，留待规划会话裁决。
  6. **ℹ️ 工作区共享带来的分支串扰（2026-09-20 实测）**：本分支施工期间，规划会话的 2 个 docs 提交（`c71d854`、`56e8691`，均只动 `docs/feedback.md`）落在同一棵工作树的 HEAD 上，因此随 `exec/ci-green` 一起进入 PR #10 diff。已在 PR 正文与交接说明中标注，审阅时可只看本人 8 个提交；后续多 agent 并行建议用独立 worktree（`using-git-worktrees`）而非共享目录。

## 6. 会话记录（追加式，保留历史）

- 2026-08-15：扫描+方案制定+预处理修复。分析产物 docs/analysis/*，计划产物 docs/plan/*。
- 2026-09-19：规划会话实测复核（git log/代码 grep/磁盘/工作区四处取证），下发 docs/plan/2026-09-19-capability-uplift-plan.md（清场前置+MA3/MB1 半程校准+M-A/M-B 顺序调整+防停摆机制），再交棒执行 agent。
- 2026-09-19：执行 agent 完成第 0 段清场（分支 `exec/stage0-triage` → PR #9，未合并待审）。取证方式：`git archive HEAD` 导出副本 + 副本内 `cargo fmt` + 逐文件 `diff` 证明"纯 fmt"；`git merge-tree` 预演与 GitHub main 的合并冲突。结论：未提交工作区不含金银——是一次未提交的全量 fmt + 局部 clippy 清理，"完成即止"未触碰 M-A。
- 2026-09-19：执行 agent 完成 main 血统对齐（同分支 merge `844143f`，父 `fe9705f` + `3a50c49`；PR #9 CONFLICTING → MERGEABLE，处置表见 PR 评论）。任务单一，未合并任何 PR、未写 main、未碰 gitee/dev/226 个未跟踪产物。取证方式：逐冲突文件 `git diff HEAD...origin/main -- <file>` 看远端独有内容 + `git log -S` 定代际（cad.rs 的 `8010→8080`/warn 日志/preview 放宽 系本地 `620b263`(08-24) 对远端 `3a50c49`(08-12) 的后继修复）+ `git cat-file` 比 blob + 合并前后**树哈希全等**（`627a03f`，`git diff fe9705f HEAD` 0 行）。结论：远端侧零可取内容，12 处冲突取 ours 无任何丢失；门禁 fmt 绿 / test 369 持平 / HTML 绿 / clippy 仍 4 处存量。新查出处环境事实：CI lint job 因 `npm ci` 无锁文件必挂（见风险 4），另 `gh` 写操作须 `env -u GITHUB_TOKEN`。
- 2026-09-20：用户授权规划会话代执行合并：PR #9 以 merge commit 合并（`4ed7f3e`），本地 main fast-forward 拉齐，origin/main == main，血统分叉终结。合并路径排障：repo 设置与 GraphQL 均显示允许 merge commit 但 REST/GraphQL 合并仍 405——真凶是 ruleset「研创台」的 `required_linear_history`（bypass_actors 空，无人可绕）。经用户裁决 PUT rulesets/18487639 移除该条（保留 deletion/non_fast_forward/creation/code_quality）。gitee/main 落后 50 提交，待 T0.6 一并处理。
- 2026-09-20：执行 agent 完成 **CI 绿色基线包**（分支 `exec/ci-green` 自 `adae26f` 开出 → PR #10，未合并、未动 main/gitee、未开工 M-A）。处置清单：① 锁文件 `package-lock.json` 入库（`npm ci --dry-run` 先验同步）+ AGENTS.md §4.3 该条改写为"禁构建工具链，开发工具依赖清单必须入库" + `.gitignore` 去掉对锁文件的忽略；② clippy 存量 4 处（`rag/chunker.rs` 恒真 `.max(0)` + 三处 `for_kv_map`→`values_mut()`）逐处最小修，`--all-targets -D warnings` 首次 exit 0；③ `rust-toolchain.toml` 钉 channel 1.98.0（含 components clippy/rustfmt、profile minimal）；④ `.gitignore` 收编 `_*.txt`/`_*.json`/`_*.py`/`.firecrawl/`（先核实 239 个已跟踪路径 0 命中，再用 `git check-ignore --no-index` 复验）；⑤ `check_html_functions.mjs --refresh`（manifest 仅 +2 定义，零删除）；⑥ 本地复现 CI 命令时发现**新暴露的第 5 项红**：ESLint 10 不读 `.eslintrc.json`（该步骤自建仓起从未真的执行过），等价迁移 flat config 后 `static/i18n.js` 又露出 2 处真实 `no-redeclare`，按 var→let 零行为变化修掉。取证方式：逐命令复跑并记 exit code；e2e 在**独立临时 cwd**（空库）跑，避免动用户 4.3GB `innoforge.db`。门禁数字：fmt 0 / clippy --all-targets 0 / test 369 passed・0 failed・3 ignored / HTML 扫描 0 / eslint 0 error / e2e 54/54 PASSED。踩坑与证据入 `docs/errors.md` 2026-09-20 两条（rustup 钉版需镜像补装；eslint 假绿门禁）。**GitHub CI 首绿（run `35485167886`，PR #10）**：lint ✓ 18s（`npm ci` + HTML 扫描 + `npx eslint` 三步全过，eslint 0 error/5 warn）、test ✓ 1m54s（fmt/clippy/test 全过，`Install Rust stable` 步打印 `the toolchain '1.98.0-x86_64-unknown-linux-gnu' is currently in use` —— 钉版在 Linux runner 上确实生效）、e2e ✓ 2m1s（`chrome@150.0.7871.24` 安装成功、空库起服务、日志打印 `E2E PASSED (54/54)`）。CI 侧 test 计数 367 passed / 0 failed，比本机 369 少 2：经逐名 diff 定位为**同一个** `#[cfg(target_os = "windows")]` 测试 `cad::tests::startup_retry_is_blocked_only_during_the_cooldown` 在 lib 与 bin 两个测试二进制里各计一次，Linux runner 上本就不编译，**非本分支削弱或跳过任何测试**（另 1 项差异是 `#[ignore]` 的 `experiment::sandbox::tests::test_timeout_terminates_script`，两端都忽略）。
- 2026-09-20：规划会话代执行 **PR #10 合并 + T0.5/T0.6 收口**（均为可验证的安全操作，按"小问题自主决策"授权办理）。① 合并前只读核验：GitHub 三 job 全绿（run `35485167886`）、PR MERGEABLE、10 提交逐一对账工作包清单；merge commit `faf60f6`（含执行 agent 追加的 CI 结果回写 `52768a`）。② T0.5：dev 为 main 严格祖先（`main..dev`=0），纯快进 `7fc1aec→faf60f6` 并推 origin，dev 分支 CI 点亮（run `35486017267`）。③ T0.6：gitee/main 为严格祖先（落后 73、超前 0），`git push gitee main dev` 快进收齐。至此五端（origin main/dev、gitee main/dev、本地 main）全部 == `faf60f6`。④ 卫生：关闭基点过期的陈旧 PR #1/#2（留关闭说明）。⑤ 裁决执行 agent 留置项：AGENTS.md Step 5 的 ESLint 命令路径 `templates/static/i18n.js` 系笔误，改为真实路径 `static/i18n.js`。**M-A 开工条件全部就绪**，下一位执行 agent 认领"下一步"第 3 项（MA1 + T1.1 缩水版）。
- 2026-09-20：执行 agent 接手失联前任、完成 **MA1 SearchProvider 契约 + SerpAPI 迁入**（分支 `exec/ma1-searchprovider`，在 `b08f1c5`(T1.1) + `07bab1b`(WIP 原样保存) 之上**续建**，未另起炉灶；main/dev/gitee 未动）。
  - **"接手前任遗产"的取证方式（为什么判定 WIP 是未经任何验证的死代码，而不是"能跑的半成品"）**：① 在 `07bab1b` 上 `cargo check` **通过**——但 `grep -n "mod search" src/lib.rs src/main.rs` 结果为空，即 `src/search/` 五文件从不在模块树内，check 绿是**零信息量**的假绿；② 接线（`lib.rs`/`main.rs` 双入口各补 `pub mod search;`，AGENTS.md 2.4）后立刻成片 E0369/E0599（`PatentSummary` 缺 `PartialEq`，而 spec §1 的 `MergedPatent`/`SearchOutcome` 必须持有并比较它），**反证 `07bab1b` 从未编译过**；③ 四个 WIP 文件（merge/model/relevance/query）里存在 `rustfmt` 会折叠的单行长调用，也是"从未进过门禁"的旁证；④ 逐字段对 spec §1 契约与 `provider.rs` 的手工 `Pin<Box<dyn Future>>` 写法，确认建模忠实、trait 保持 object-safe，且自带 29 个用例，**故续建**：删掉的是它的错误断言与未接线部分，保留的是它的契约与测试骨架。
  - **发现并修掉 WIP 两处真实缺陷（都是它的测试自证失败，非本次改动引起）**：① `query.rs::render_q_matches_pre_migration_implementation` 的用例构造用 `q(keyword, st)` 辅助函数，`date_from/date_to` 在新代码侧永远为 `None`、旧代码侧却有值 → 对拍形同虚设，改为逐字段构造 `SearchQuery`；② `render_q_golden_cases` 断言 `SearchType::Keyword` 输出**带引号**的 `"iPhone 15"`，而旧实现 `match _ => q` 分支根本不加引号 → 断言本身写错，按旧代码真实行为改为"透传 + 去引号"三类用例。**未削弱任何断言**（两处均为把错断言改成正确断言）。
  - **施工内容**：新建 `src/search/{mod.rs,providers/mod.rs,providers/serpapi.rs}`（serpapi.rs 1121 行，routes 侧 SerpAPI 调用链整体抽出为首个 `impl SearchProvider`）；`routes/search.rs` **1449→820 行**，8 个函数（`sort_by_relevance`/`dedup_patent_summaries`/`calculate_online_relevance`/`contains_cjk`/`is_online_result_relevant`/`serp_to_patent`/`try_exact_patent_lookup`/`find_publication_patent_id`）迁入 `crate::search` 单一出处，`routes/mod.rs` 的 `build_online_query` 删除（−81 行含 2 个测试，断言并入 `query.rs` 金样例）——按 AGENTS.md 2.2 消除双定义；`types/search.rs` 的 `PatentSummary` 仅加 `PartialEq` 派生（serde 形状与字段零变化）。
  - **行为保持的证明方式（关键：不用自证式包装）**：把**迁移前的旧实现逐字复制**进 `#[cfg(test)]` 参考模块（`query::test_support::legacy_build_online_query` / `legacy_region_flags`、`providers::serpapi::legacy_url_from_q` / `legacy_map`），新代码与它逐用例对拍——URL 参数 10 例矩阵 + CN 查询整串金样例 + 结果映射逐字段（抹平自增 id、按 patent_number 排序消 HashMap 序不确定）；`resolve_lang` 与旧 region flags 做 **4 region × 4 country × 14 关键词全组合**交叉验证；端点侧新增 2 个形状测试锁死四条终止响应（在线命中 / 无 Key 跳过 / 专利号直查 / 回退 google_url）与全部旧中文提示文案（含"首个非空生效"的 `upstream_hint` 语义）。**对外 API 端点形状、字段名、提示文案零变化**；上游超时刻意保持旧 30s（spec §1 的 15s 收紧属 MA2，避免混入行为变化）。
  - **门禁数字（rustc/clippy 1.98.0）**：`cargo fmt --check` **exit 0** ｜ `cargo clippy --all-targets -- -D warnings` **exit 0** ｜ `cargo test` **457 passed / 0 failed / 3 ignored**（8 个测试二进制：lib 203 + bin 205 + 集成 49；其中 `search::*` 29 例在 lib 与 bin 两个测试二进制各计一次 = 58 例；净增与 `b08f1c5` 记录的 405 逐名对账：+58 新接线、−2 `routes::search` 重复用例、−4 `routes::tests` 已迁移用例 = +52）。**templates/ 与 static/ 未触碰** ⇒ HTML 函数扫描与 Puppeteer e2e 两道门禁按 DoD 条件不适用，未跑（e2e 需要改前端，属 MA5）。
  - **留痕**：`types-migration-map.md` 里 WIP 预先写下的两条"部分收敛"注记（`contains_cjk` 抽出、`SerpApiProvider` 抽取）在本次接线后**由声明变为事实**；MA3 的行号级证据（`routes/search.rs:259/:315`）因本次迁移失效，新位置已同时回写到 task-breakdown MA1 行与 §5。MA2/MA4/MA5 的消费点以 `#[allow(dead_code)]` + 注释就地标注。
- 2026-09-20：**MA1 收尾（第三个 agent，scope 收窄为"只到可开 PR"）——门禁复跑全绿，开 PR #11**。
  - **"三接手机制"经过（MA1 的真实交付路径，记此以免重蹈）**：本执行包先后由 **3 个 agent** 承担。第 1 个交付 `b08f1c5`（T1.1 缩水版类型地基）后继续 MA1，**33 分钟零心跳**（末次写盘 14:02）撞 150 轮上限，监工以 `07bab1b` **WIP 原样代存**（`src/search` 五文件，从未接线、从未编译）；第 2 个在其上续建（SerpAPI 抽出 + 四域接线 + docs 回写），死亡瞬间原话 **"Found both bugs (one was latent in the WIP): the matrix test dropped the date fields, and the `Keyword` golden expectation was wrong."**，修复半途撞上限，监工第二次以 `fdbaf11` **WIP 原样代存**；第 3 个（本轮）复跑门禁、逐项取证那两处 bug、补齐前任留在 §5/§6 的 `@@TESTS@@` 占位后推送开 PR。**教训：执行包应按"一个任务 + 一轮门禁"切分；跨 agent 续建时前任须把未完成判定写进 commit message，监工代存只保真不修，收尾者只负责复跑 + 取证 + 交付。**
  - **两处测试 bug 的最终处置（复核结论：把错断言改成对断言，非削弱；`fdbaf11` 已含修复，本轮零代码改动）**：① **矩阵对拍丢 date 字段**——`query.rs::render_q_matches_pre_migration_implementation` 的 `RenderCase` 现为 `(keyword, search_type, date_from, date_to)` 四元组，循环内把 `*from`/`*to` **同时**喂给迁移前参考实现 `legacy_build_online_query` 与新实现 `render_q`（`src/search/query.rs:252-309`），含 5 条带日期用例（`2024-01-01/2024-12-31`、`20200101`、`20190101/20240101`、`20250101`）；旧代码侧有值而新代码侧恒 `None` 的"假对拍"消除，属**断言加强**。② **`Keyword` golden 期望值写错**——原断言 `assert_eq!("\"iPhone 15\"", render_q(&q("iPhone 15", Keyword)))` 期望输出**带引号**，但 `render_q` 首行 `keyword.trim().replace('"', "")` 与 `_ => q` 分支**从不补引号**（`src/search/query.rs:64`、`:102`，与逐字符复制的迁移前实现同源），断言本身与"行为保持"目标矛盾；已按旧实现真实行为改为三条：`"  iPhone 15  "`→`iPhone 15`（trim 透传）、`Mixed` 同形、`带"iPhone 15"的查询`→`带iPhone 15的查询`（去引号不转义），并由 ① 矩阵中的 `带"引号"的查询` 用例交叉自证。
  - **端点形状零变化的取证**：`git diff main --stat -- templates static` 输出为空 ⇒ HTML 函数扫描与 Puppeteer e2e 按 DoD 条件不适用；`src/common.rs::build_router` 的 **71 条** `.route(...)` 路径与 `main` 逐条 diff 全等（`/api/search`、`/api/search/online`、`/api/search/vector`、`/api/search/stats`、`/api/search/export`、`/api/search/export/xlsx`、`/api/search/analyze` 均在列）；`src/main.rs`/`src/lib.rs` 仅改模块声明（`pub mod search`/`types`、`patent` 可见性对齐），AGENTS.md 2.4 的"双入口一致"由两入口共用 `build_router` 结构性保证。
  - **本轮改动范围**：`docs/progress/MASTER.md`（§5 第 3 项 MA1 状态标注 + §5/§6 两处 `@@TESTS@@` 补真值）+ `docs/plan/task-breakdown.md`（MA1 行补门禁数字与 PR 号），**未触碰任何 src/ 代码、未改测试期望、未动扫描/函数基线**。
- 2026-09-20：**MA2a 完成（第六棒写码 + 第七棒收尾，分支 `exec/ma2a-xhr` → 开 PR #12）**。
  - **交付路径（第四棒起的两棒接力）**：第五棒留下 `src/search/` 契约半成品（`model`/`serpapi`/`relevance`）后失联，监工以 `9bd8559` **WIP 原样代存**（未编译验证）；第六棒自 `0c37569` 接手并清零其唯一编译告警，随后 `0ac73e7`（`GooglePatentsXhrProvider` + `SourceChain`）、`4bca304`（spec §6 三用例 + XHR 源离线实证单测）、`52948ad`（`/api/search/online` 接链）；第六棒在**准备收尾阶段** 43 分钟零心跳挂死，监工先推其 4 个提交到远端保护，第七棒（本轮）**只做收尾**：跑门禁 + 回写文档 + 推送 + 开 PR，**零功能代码改动、零 `fix(search)` 提交**（三门禁一次全绿，无失败可修）。
  - **门禁复跑（tip `52948ad`，rustc/clippy 1.98.0）**：`cargo fmt --check` **exit 0** ｜ `cargo clippy --all-targets -- -D warnings` **exit 0** ｜ `cargo test` **525 passed / 0 failed / 3 ignored**。取全量绿的一处方法记此备用：**`cargo clippy` 秒级返回时可能是缓存复用，虽然缓存会重放诊断，但为了排除"未复检"的歧义，收尾时对 6 个改动文件 `touch` 后强制复检，仍 exit 0、零告警**。测试计数对账：main 基线 457 → 525，净增 68 = 本分支新增 **34** 条测试函数（`git diff main...HEAD` 统计 `#[test]`×18 + `#[tokio::test]`×16），每条在 lib 与 bin 两个测试二进制各计一次（lib 203→237、bin 205→239、集成 49 持平）——**与 MA1 的 "×2 计数" 现象同源，非重复测试或注水**。
  - **红线自查（逐条命令取证）**：`git diff main...HEAD --stat -- src/common.rs` 空 ⇒ 71 条 `.route()` 零改动（AGENTS.md 2.4 的双入口一致由共用 `build_router` 结构性保证，本任务不需要动它）｜ `-- templates static` 空 ⇒ HTML 函数扫描与 Puppeteer e2e 按 DoD 条件**不适用**（本轮与 MA1 同样未改前端；MA2a 的"降级可见"要等 MA5 面板才落到 UI）｜ `-- Cargo.toml Cargo.lock` 空 ⇒ 无新 crate 依赖 ｜ `-- src/db/` 空 ⇒ 无 schema 变更 ｜ `unwrap`/`expect` 在 8 个改动文件共 20 处命中，**行号全部大于该文件 `#[cfg(test)]` 起始行**（chain.rs 115→命中 374、model.rs 259→270、serpapi.rs 644→657/1048、google_patents_xhr.rs 651→762+），生产路径零命中。
  - **验收口径的对齐说明（一处与发包口径的差异，按代码事实记录）**：发包时估计"新增约 27 条测试"，实际落地 **34** 条（第六棒多写了 8 条离线实证用例）。按"通过数应 ≥457"的判据取真值 **525**，不回填估计值。
  - **本轮改动范围**：`docs/plan/task-breakdown.md`（MA2 行改为半程 + 新增 **MA2a 独立行**记 ✅ 与门禁数字）+ `docs/progress/MASTER.md`（§5 第十二次交接块、§5 日期行、§5 下一步 3a→3b 指针与新增 ② 子项、§6 本条）。**未触碰任何 src/ 代码。**下一位执行 agent 认领 **MA2b EPO OPS（规格书 §4）**。
