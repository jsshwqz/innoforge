# Patent-Hub 开发进度状态追踪
# Patent-Hub Development Progress Tracker

> 本文档追踪项目的所有重大功能开发进度、状态变更和技术债务处理。
> This document tracks all major feature development progress, status changes, and technical debt handling.

---

## 状态变更日志 (Status Change Log)

### 2026-09-28 — MA6a 源级熔断冷却（`cools_down()` 收口）+ SerpAPI 超时 15s + PR #24 合并

- **状态 / Status**: ✅ 已完成（MA6 第一片）/ Completed
- **范围 / Scope**: 第十七棒单包直出（108 轮 / 89 分钟，含一次自定位的死锁修复，**未止损、无需中继**）。**缺口定位（规划会话派包前实测）**：`FailKind::cools_down()` 自 MA1 写定后**从未有生产消费点**（全仓仅 model.rs 自身测试、chain.rs:25 与 epo_ops.rs:94 的 TODO 注释、search.html:540 的前端镜像注释引用它），而 `Retry-After` **早已在 XHR/EPO 源内被尊重**（`google_patents_xhr.rs:138/347/440`、`epo_ops.rs:212/475/544` 的 `retry_after.unwrap_or_else(backoff_for)`）——所以熔断层**不需要**新增 `AttemptReport` 字段（加字段会动 serde 形状 → 动 MA5b 字面量锁 → 动前端，本包禁止）。交付：`4a04930` `src/search/breaker.rs`（**三段分离**：`CooldownTable` 纯结构显式收 `now` / `SourceKind×FailKind` 命名常量表 / `OnceLock<Mutex<..>>` 全局薄壳，零新依赖）+ SerpAPI `UPSTREAM_TIMEOUT_SECS` 30→15 与常量锁用例；`da230af` `routes/search.rs::api_search_online` 接线（链前 `filter_cooled_providers` 摘链=结构上零请求、链后 `note_attempts` 只消费真实链上 attempts、`weave_cooled_attempts` 把冷却源按**原登记序**织回 `Skipped` 记账）；`67e45e2` 四处「属 MA5/MA6」悬置口径收口；`8799d07` clippy lint + 兜底用例修正。
- **冷却时长表（进程内、源级，重启清零）**：SerpAPI Quota 300s / Auth 900s；**XHR Quota 120s（全表最短）**——它是唯一免费无 Key 源，冷却过长等于把在线检索整体下线，但必须够长以打断「连打→封更久」；EPO Quota 300s / Auth 900s；`LocalFts` 恒 `None`（本地源不参与熔断）。XHR 的 503 反爬**既有分类就是 `Quota`**（`real_503_bot_page_classifies_as_quota`），因此本熔断天然覆盖 Google 封禁场景——正是本仓两次让执行 agent 空转的成因。
- **一处有意偏离（已在文档留痕，非静默）**：规格书 §6 早期设想「连续 3 次 Quota/Auth 才冷却 10 分钟」，实现改为**单次即冷却**。理由（实证）：本仓实测同 IP 连打 3 发（<8s）即触发 Google 503，第 2、3 次「确认性」计数恰好落在恶性循环最疼的位置，上游已返回 Quota/Auth 时限流即成立，多打两发省不下任何封禁时长。§6 原文以删除线保留并写明偏离依据。
- **验证 / Verification（规划会话独立复跑，不只采信 agent 与 CI）**：GitHub CI lint/test/e2e 三绿（run `36343375726`）；本地 tip `8799d07` 上 `cargo fmt --check` exit 0、`cargo clippy --all-targets -- -D warnings` exit 0（touch 7 文件后真编译 1m11s）、`cargo test` **711 passed = lib 330 + bin 332 + 集成 49**，与执行棒声明逐字对账闭合（= 基线 687 + 12 新单测 × lib/bin 双计）；关键新用例逐条抽验绿（`quota_and_auth_cool_down_but_network_and_parse_do_not`、`cooled_source_leaves_chain_but_keeps_skipped_attempt_in_place`、`all_sources_cooled_yields_empty_chain_and_identical_local_shape`、`cooldown_constants_are_locked`、`upstream_timeout_is_tightened_to_spec_1`），且既有两处形状锁 `attempts_json_locks_frontend_panel_literals` / `online_chain_registers_epo_only_with_credentials` **零改动继续绿**（未改期望值「过」）。**审计要点（规划会话逐项核）**：① 全局锁**未跨 `.await`**——链前过滤包在 `let (providers, cooled) = { … }` 块内、guard 于块尾析构后才 `SourceChain::run(...).await`；② `cools_down()` 的消费**是真的**（`breaker.rs:67` `if !fail.cools_down() { return None }`，不在别处复刻第二套 match），且 model.rs:77 那条 `#[allow(dead_code)]` **确实存在**并被撤销（与 MA4a 那次「撤一条不存在的标注」的假事实相反，本次为真）；③ `SourceKind` 只加 `Hash` derive，**变体集合与 serde 表示逐字未动**；④ 冷却 `Skipped` 的原因走 `error` 字段、`hint` 恒 `None`——避免经 `SearchOutcome::hint()` 泄漏改出参形状；⑤ 红线全零 diff（`common.rs` 118 条 `.route()` / `Cargo.*` / migrations / templates / static / main.rs / lib.rs）；⑥ 新增 `.expect()` 全部落在 `routes/search.rs` `#[cfg(test)]`（:1003 起）与 breaker 测试模块内；⑦ `CooldownTable` 不实现 Serialize/Deserialize 是对 AGENTS.md 2.2 的**有意偏离**（`Instant` 本不可跨进程序列化），已在结构注释中说明并保留。templates/static 零 diff ⇒ e2e/eslint/HTML 扫描按 DoD 不适用（沿用上包 56/56）。
- **过程记（诚实留痕）**：本棒首版全局薄壳在测试里**持锁重入** `remaining()` 触发 std Mutex 不可重入死锁，空转约 25 分钟后自查定位并在 `da230af` 注释与 PR §七 留痕（清零改走无锁路径）——这是「里程碑即提交」第二次让我们能在任意时刻止损的原因。
- **仍开放 / Remaining**: ① 诊断面板展示「冷却中·剩余 Ns」与 SerpAPI 配额余额预警（`breaker::remaining()` 单源查询口已预留，当前带诚实的 `#[allow(dead_code)]` 标注待 MA6b 消费）；② `lookup_exact` 专利号直查路径**未纳入熔断覆盖**（MA6b）；③ 真实上游冒烟（本机无 SERPAPI_KEY + 本包禁止出网）；④ `MergedPatent.key` 跨源去重裁决、形态 (c) 引号独立参数在反爬窗口外重测、「在线命中→入库→断网复检」全链路取证、M-A 用户可见变更一次性补入 CHANGELOG——均归 MA6c。
- **同步 / Sync**: origin/main、origin/dev、gitee/main、gitee/dev、本地 main 五端对齐 `6627f50`（PR #24 merge commit；dev 两端均显式 `git push <remote> main:dev`）。M-A 只剩 MA6b/MA6c 两片即收口。

### 2026-09-28 — MA4b 申请人精确匹配 + PR #23 合并（M-A 第 5 包收口）

- **状态 / Status**: ✅ 已完成（本包范围内）/ Completed within scope
- **范围 / Scope**: 由**两棒接力**交付（第十五棒实现棒因外部反爬空转被止损，第十六棒收尾棒直出 PR）。**缺口定位（规划会话派包前实测）**：MA2a 起 `SearchQuery` 就预留了 `assignee`/`exact_assignee` 两键，但 `routes/search.rs` 构造点**恒写 `None/false`**，且 `exact_assignee` 全仓零生产读点——真缺口是请求侧入口与本地等值通道，而非上游语法。交付：`07b3b3f` `SearchRequest` 新增 `assignee: Option<String>` + `exact_assignee: bool`（均 `#[serde(default)]`：**缺键 = 出网 URL 与本地 SQL 与 MA4b 前逐字一致**）；XHR 源 `inner_url` 按开关渲染独立参数（false ⇒ 裸值，MA2a 逐字符锁 `inner_url_emits_spec_2_param_set` 未动；true ⇒ 引号 `assignee="…"`，另补 3 条形态/空白值/**引号只编码一次** `%22` 而非 `%2522` 锁）；本地侧新增 `db::search_smart_exact(…, exact_assignee)`，旧 `search_smart` 降为其 `false` 特化兼容包装（既有 4 个调用点零改动），`search_by_field(exact=true)` 把 `applicant LIKE %词%` 换成 `applicant = ?1` 等值（**字段白名单与参数化绑定不削弱**，inventor 域硬编码 `false` 防开关外溢）；SerpAPI/EPO **故意不消费**该键（上游入参语法不同，禁止串语法）。`aacedc7`/`9f80156` 两份文档取证回填。
- **取证 / Evidence（M-A 首个「双半边」真实验收包）**：① **在线形态真过滤确证两类**（规格书 §8 第 2 项，免费 XHR 真实出网）——形态 (a) `q=assignee:"西南交通大学"` → `total_num_results=542`、首页 **10/10 行 assignee 全等**；形态 (b) `q=固态电池&assignee=西南交通大学`（= 本仓缺省下发的裸值独立参数）→ `total=58`（对照该词十万级全量）、10/10 行为 `<b>西南交通大学</b>`，出网 URL 逐字符入文，原始 JSON 存仓库外 `D:\Temp\ma4b-probe\`。**未取证如实标注**：形态 (c) 引号独立参数（即 `exact_assignee=true` 的在线形状）与英文机构名样本全部只拿到 1103 字节 Google `Sorry...` 反爬页，文档明写「**不得据此对形态 (c) 下任何生效性结论**」，待限频窗口外/换 IP 重测。② **本地等值通道真实实例取证**——临时全新空库实例（`D:\Temp\ma4b-hit-probe`，用户 `innoforge.db` 未被触碰）把形态 (b) 真实抓回的 10 行 + 1 行构造对照「西南交通大学宜宾研究院」入库后打 `/api/search/online`：`exact_assignee=true` → attempts 末条 `local_fts Success hits=10` 且 10/10 全等（等值通道**排除了变体**）；不带开关 → `hits=11` 含变体（旧 LIKE 逐字不变）。
- **一处隐患核查后排除**: 怀疑上游 `<b>` 高亮标签会污染入库——查 `xhr_to_patent` 对 title/snippet/assignee 已统一走 `strip_highlight_tags`（`google_patents_xhr.rs:463/473/474/477`，用例已锁），**无需改代码**，结论写入规格书。
- **验证 / Verification（规划会话独立复跑，不只采信 agent 与 CI）**：GitHub CI lint/test/e2e 三绿（run `36331887838`）；本地 tip `9f80156` 上 `cargo fmt --check` exit 0、`cargo clippy --all-targets -- -D warnings` exit 0（touch 8 个改动文件后真编译 1m11s）、`cargo test` **687 passed = lib 318 + bin 320 + 集成 49**，与收尾棒声明逐字对账闭合（= 基线 669 + 9 新单测 × lib/bin 双计）；关键锁用例逐条抽验绿（`attempts_json_locks_frontend_panel_literals`、`inner_url_emits_spec_2_param_set`、`local_fallback_exact_assignee_switches_the_equal_channel`、3 条 exact_assignee 形态锁、`search_request_accepts_assignee_keys_with_snake_case_defaults`）；HTML 函数扫描「8 模板基线一致」。**红线**：`src/common.rs`（118 条 `.route()`）/`Cargo.toml`+`lock`/`src/db/migrations.rs`/`templates/`/`static/`/`src/main.rs`+`lib.rs` 对 main **全零 diff**；新增 26 处 `.expect()` 经逐行映射**全部落在 4 个 `#[cfg(test)]` 块内**（生产路径零 unwrap）；本次未动前端 ⇒ e2e/eslint 按 DoD 条件不适用（沿用上棒 56/56）。
- **过程止损 / Relay discipline**: 第十五棒实现全部落地但**零 commit、423 行未保存**，且反复重打被限频的上游端点（命中已知失败模式 #5「活着空转」）。处置：`git stash create` 生成 WIP commit `24e2da3`（**不触碰 agent 工作树**）+ `refs/wip/exec/ma4b-assignee` + bundle + 推远端做异地保护 → TaskStop → 派**极窄收尾棒**（brief 直接交出已核实数字、**明令禁止再出网重试**、范围限定「文档 + 门禁 + PR」）。收尾棒 110 轮 / 64 分钟完成「文档 + 门禁 + PR」四件事，未触顶、无需二次止损——验证了「交出已核实数字 + 明令禁止再出网」的收窄派包对「活着空转」型失败有效。第十六棒完成后自述「前任 WIP 逐字保留、本棒未做任何修正」，与 `24e2da3` 对 `src/` 零 diff 已由规划会话核对。
- **新踩坑（已入库 docs/errors.md）**: 规划会话与执行 agent **共用同一 `target/`** 并发跑 cargo → `incremental/` 缓存写坏，表现为 `cargo test` rustc `0xc0000409`（首跑 exit 101）。清理后单进程复跑全绿，**未改一行代码**。预防条款：独立门禁复跑必须等 agent 结束（判据 = 树干净 + 无 cargo/rustc + 无 server 进程），否则用 worktree + 独立 `CARGO_TARGET_DIR`。
- **仍开放 / Remaining**: ① 形态 (c) `exact_assignee=true` 的**在线**引号形态仍未取证（反爬阻断，非代码缺陷）；② 「在线命中→本实例入库→断网复检」**全链路**仍缺（收尾时点上游 503），本地等值半边已实证；③ MA4 的**跨源合并去重**——`MergedPatent.key` 仍无消费点，且现行「链按登记顺序择单一胜者」架构是否需要跨源合并本身待 MA6 前重估；④ M-A 系列（MA5b/MA3/MA4a/MA4b）用户可见变更**尚未合并写入 CHANGELOG**，按约定在 MA6 收口时一次性补记。
- **同步 / Sync**: origin/main、origin/dev、gitee/main、gitee/dev、本地 main 五端对齐 `962f1a5`（PR #23 merge commit `2679c36` + 本条文档回写）。**同步手法更正（实测）**：`dev` 两端都**必须显式 `git push <remote> main:dev`**——`git push origin dev` 会被本地陈旧 `dev` ref（`aeb0b7a`）撞 non-fast-forward 拒绝，Gitee 侧本轮也未自动跟上（首次只到 `2679c36`，显式推后才对齐），不可假定镜像自动同步。

### 2026-09-27 — MA4a 本地 FTS 兜底链上记账 + PR #22 合并

- **状态 / Status**: ✅ 已完成 / Completed（本包范围内；MA4 整体转 ◐）
- **范围 / Scope**: 第十三棒触顶后由第十四棒收尾（同一分支续跑，零返工）。交付：`68ff948` 把 `/api/search/online` 的本地兜底从内联块抽成 `local_fallback_json()`，每次兜底**追加**一条 `SourceKind::LocalFts` 的 `AttemptReport` 到 attempts 末位——本地跑过即如实记 `Success`（hits 可为 0），`search_smart` 报错记 `Failed(FailKind::Parse)`；**出参逐字不变**（`source:"local"`、hint 文案、`patents[]/total/page/page_size/google_url/message` 键集合与取值口径全部由新增 3 条内存库单测锁死），MA5b 的 `attempts_json_locks_frontend_panel_literals` 未红；`6043e72` dead_code 对账；`310b07f` 补 MA3 语言过滤器的 2 条 e2e 用例（54→56）；`bdd084c` 文档回写。**两个有意设计**：① `LocalFts` 故意**不**注册进 `online_chain_providers`——注册会让 `winning_source().as_str()` 吐 `"local_fts"`，破坏出参 `source` 且集体变红「四源链形状」用例；② 不扩 `FailKind`（Network/Quota/Auth 是远端源语义，Parse 的「记 bug、不切换、错误原样抛出」恰与「最后一档无源可切」一致；扩枚举会破坏 MA5b 字面量锁与前端 `DIAG_FAIL_KEYS`）。
- **验收 / Acceptance**: 「检索过的专利断网可复检」的 **attempts 记账半边**已在真实实例取证（非内存库单测冒充）：临时全新空库 `D:\Temp\innoforge-e2e-run`（未触碰用户 4.3GB `innoforge.db`）起服，生僻词 `zzyyxx_offline_probe_qwertyuiop9x` 在线零命中自然回退 → attempts 末条 `{"source":"local_fts","status":"Success","hits":0,"latency_ms":0}`，前置 `serpapi Skipped` → `google_patents_xhr Success hits=0`。取证形态说明：未注入网络故障，走的是与「禁用两在线源」同一兜底代码路径。
- **一处任务书前提被纠正（agent 做对了）**: 派包时我写了「撤掉 `SourceKind::LocalFts` 的 `#[allow(dead_code)]`」——实测该枚举**从未有**该属性（lib crate 的 pub 变体不触发 dead_code lint）。agent 未做假改动、未静默删除，而是在 `6043e72` 注释与 PR 中如实对账。同类：`SearchQuery.assignee` 的 `allow` 亦已陈旧（`google_patents_xhr.rs:239` 早有生产读点），MA4b 派包前须核实。
- **验证 / Verification**: CI lint/test/e2e 三绿（run `36319845393`）；规划会话独立复跑 fmt exit 0 / clippy `--all-targets -D warnings` exit 0（touch 后真编译 59s）/ test 全绿，**669 = lib 309 + bin 311 + 集成 49 对账闭合**（基线 663 + 3 新单测 × lib/bin 双计，与 PR 口径逐项一致）；红线零 diff（`src/common.rs` 118 条 `.route()` / `Cargo.toml`+`lock` / `src/db/` / `templates/` / `static/` 对 main 全等，6 文件 +397/−50）；新增 `unwrap/expect` 全部 10 处经行号映射落在 `#[cfg(test)]`（marker 887）之后；`templates/static` 零改动 ⇒ 无需 `--refresh`，且 `DIAG_SOURCE_LABELS.local_fts` 已在 main 存在（MA5b 交付），面板无需改动即可长出该行；e2e 新增 2 用例经核实为真实页面求值（读 `#language-filter` + 调 `buildRequest`，检查后恢复默认，非空断言）。本地复跑前按已知处置清 `target/debug/incremental`（D: 余 1.8G→5.2G）。
- **仍开放 / Remaining**: MA4 剩三项如实标未完成——**申请人精确匹配**（规格书 §8 第 2 项 XHR `assignee=` 引号 vs 裸值精确性验证仍开放）、**跨源合并去重**（`MergedPatent.key` 无消费点）、**结果强制入库 + FTS 自动同步（§7 单一写入口）**；验收锚点「本人姓名可搜到自己名下专利」**未达成**。本地命中档（hits>0）只有单测形状、无真实实例取证。下一包按 M-A 顺序为 **MA4b**（assignee 精确匹配 + §8 第 2 项实测 + 回归用例集），**MA6**（SerpAPI 超时收紧 + 服务端熔断消费 `cools_down()`）随后；M-B 规格书补发未启动。
- **同步 / Sync**: origin/main、origin/dev、gitee/main、gitee/dev、本地 main 五端对齐 `3edade2`

### 2026-09-27 — MA3 补半程（显式语言/辖区过滤）+ PR #21 合并

- **状态 / Status**: ✅ 已完成 / Completed
- **范围 / Scope**: 第十二棒单包直出（99 轮 / 34 分钟，未触顶）。**缺口定位（规划会话派包前实测）**：MA3 的「半程」实际已被 MA1/MA2a/MA2b 顺做掉——`resolve_lang` 已生产消费、三源都已带 language 闸门；真缺口是 `SearchRequest` 没有显式 `language` 字段、检索页没有语言过滤器。交付：`e747b44` 后端 `SearchRequest.language: Option<Lang>`（`#[serde(default)]`，`chinese|english|all` 与前端 select value 逐字对齐）+ 新函数 `resolve_lang_with_explicit(explicit, region, country, query_trimmed)`，优先级 **显式 language > region(cn/intl) > 自动判定(auto_cn)**，`None` 分支一行委托旧 `resolve_lang`（旧函数与其两条迁移前逐用例等价对拍测试零改动）+ 优先级契约单测；`83f5577` 前端 `#language-filter`（自动/中文/英文/不限，默认「自动」）+ zh/en 双字典各 5 键，`buildRequest` 仅在显式选择时下发键（不选=不发，旧请求体逐字不变）；`5351bfb` task-breakdown MA3 行 ✅ + 规格书 §8 冒烟回写。
- **验收 / Acceptance**: MA3 验收口径「默认配置中文关键词首页以中文专利为主」由**真实出网免费源取证**（同时收掉 MA2a 遗留的 XHR 真实 IP 冒烟）：`{"query":"固态电池"}` → SerpAPI Skipped → XHR 胜出，出网 URL 带 `language=CHINESE`，首页 5/5 CN 号 + 中文标题（total=123942 量级）；显式值可反向覆盖（英文词给 CHINESE、中文词给 ENGLISH、`all` 省略参数）。
- **一处有意取舍**: 未把 `country` 硬默认为 `CN`（规格书 §1 原设想），而是保留「空值=不发」+ `auto_cn` 判定——硬默认会全局改变既有英文用户检索形状，与「不传时行为逐字一致」红线冲突；`§8` 冒烟第 1 项按实测形态记录该差异，未改规格语义。
- **验证 / Verification**: CI lint/test/e2e 三绿（run `36315601516`）；规划会话独立复跑 fmt/clippy（touch 后真编译 2m）/test 全绿，**663 = lib 306 + bin 308 + 集成 49 对账闭合**；HTML 函数扫描「基线一致」（无新增 on* 函数，基线不需 refresh）；`src/common.rs`（118 条 `.route()`）/`Cargo.*`/`src/db`/`static/` 对 main 零 diff；新增 `unwrap/expect` 仅 1 处且落在 `#[cfg(test)] mod serde_snapshot`（整文件测试专用）；`api_recommend_similar` 构造点补 `language: None` 属字段新增的必要连带，非范围外改动
- **仍开放 / Remaining**: 规格书 §8 第 3 项 SerpAPI 命中率对比仍受本机无 `SERPAPI_KEY` 阻塞（凭证侧为用户动作）；§8 第 2 项 `assignee` 精确性验证归 MA4；**新语言过滤器无 e2e 用例覆盖**（`e2e_test.mjs` 零引用），已并入下一包（MA4）的前端门禁要求
- **同步 / Sync**: origin/main、origin/dev、gitee/main、gitee/dev、本地 main 五端对齐 `50c5265`

### 2026-09-27 — MA5b 检索诊断面板 + PR #20 合并

- **状态 / Status**: ✅ 已完成 / Completed
- **范围 / Scope**: 第十棒执行 agent 单包直出（86 轮未触顶，无需中继）。**关键前置发现（规划会话实测）**：`/api/search/online` 一直只在链内记账 `outcome.attempts`，四个 return 分支的 JSON 从未带出该键——诊断面板的真正缺口是后端序列化，而非前端渲染。交付：`91e31b8` 后端三条链后 return 路径（在线命中/本地兜底/空结果）纯新增 `attempts` 键 + 出参字面量锁单测 `attempts_json_locks_frontend_panel_literals`（serde 外部标签形状：`"Success"` / `"Skipped"` / `{"Failed":"quota"}`）；`0372fac` 前端 search 页折叠诊断面板（逐源徽标/耗时/命中/error+hint 摘要，全程 createElement+textContent，截断仅展示、title 保全全文；`diag.*` 15 键 zh/en 双字典；quota|auth「冷却中」徽标为后端 `FailKind::cools_down()` 的展示镜像，注释指向 model.rs）+ SerpAPI 行复用既有 `/api/settings/serpapi/balance` 静默降级；`1868312` 规格书 ✅ 注记。空查询早退路径刻意不带 attempts（链未跑），前端零 attempts 即不渲染。
- **验证 / Verification**: GitHub CI lint/test/e2e 三绿（run `36311268138`）；规划会话独立复跑 fmt/clippy（touch 后真编译 2m05s）/test 全绿，**659 = lib 304 + bin 306 + 集成 49 对账闭合**（656 passed + 3 ignored）；`check_html_functions.mjs` 独立扫描「基线一致」（6 新函数已随 `--refresh` 与模板同提交入基线）；e2e 54/54；红线零 diff（common.rs 118 条 `.route()` / Cargo / src/db / 其余模板）；生产路径无 unwrap（新 helper `unwrap_or_else` 受控降级）；agent 另做真实降级链冒烟（SerpAPI network 失败 → XHR 胜出，面板徽标/耗时正确）。本地 test 曾被 D: 盘满（os error 112）阻断，清 `target/debug/incremental`（3.3GB，可再生）后复跑通过
- **仍开放 / Remaining**: 真实凭证成功路冒烟（SerpAPI/EPO）仍缺（凭证侧为用户动作）；服务端熔断（`cools_down()` 生产消费点 + SerpAPI 30s 收紧）归 MA6；下一棒按 M-A 顺序为 **MA3 补半程**（language/辖区过滤，默认 CN+zh）
- **同步 / Sync**: origin/main、origin/dev、gitee/main、gitee/dev、本地 main 五端对齐 `aeb0b7a`

### 2026-09-27 — AI 成本落账闭环（gap 4.3 本批收口）+ PR #19 合并

- **状态 / Status**: ✅ 已完成 / Completed（本批范围内）
- **范围 / Scope**: `exec/cost-closure` 未提交 WIP 经收尾棒核实交付（PR #19，merge `0350c8b`）。`Database::log_ai_call` 成为唯一落账入口（`save_cost_record_from_client` 收敛为其内部封装），落账点 2→21（ai.rs 14 / idea.rs 5 / upload.rs 1 / claim_tree.rs 1；19 新增 + 2 存量改接）。收尾棒真实增量：修复 `compress_history` 长历史压缩调用漏账、修 idea-chat 把调用类型误填 provider 字段的错账、补 3 条锁定测试（读后即清 / 缺 usage 静默跳过 / 单路径 provider 与类型正确性）；并把前任文档口径（"22 处 / ai.rs 21"，分项加总不自洽）修正为逐处核实真值。并发语义经核实无跨请求串账（client 每请求新建、任务内串行 await）。
- **验证 / Verification**: 本地 fmt / clippy --all-targets / test 全绿（lib 299→302；清 `target/debug/incremental` 解掉前任遗留的 rustc 0xc0000409 增量产物损坏崩溃）；GitHub CI lint/test/e2e 三绿（run `36306173156`）；红线零 diff（common.rs / Cargo / migrations / templates / static）
- **仍开放 / Remaining**: 流式 SSE 不解析 usage 无法落账（需 client 层随响应返回 usage 的架构改法）、批量摘要单次近似、pipeline 其余 6 个 LLM 步骤未接线、per-model 单价编辑表、创意页「本创意花费」chip——均登记于 gap 计划 4.3「仍开放」
- **同步 / Sync**: origin/main、origin/dev、gitee/main、gitee/dev、本地 main 五端对齐 `0350c8b`

### 2026-09-22 — MA5a EPO OPS 凭证设置 + PR #14 合并 + 查漏补缺计划

- **状态 / Status**: ✅ 已完成 / Completed
- **范围 / Scope**: MA5a（EPO OPS 凭证后端读写复用搜索源端点 ba8d56f + 设置页配置位 5fe9558）经 PR #14 合并，CI lint/test/e2e 全绿；新增查漏补缺提升计划 docs/plans/2026-09-22-gap-analysis-improvement-plan.md（含远端审计：双远端无本地缺失工作）；docs/errors.md 记录 PowerShell 环境踩坑
- **同步 / Sync**: origin/main、origin/dev、gitee/main、gitee/dev、本地 main 五端全部对齐到 1a101f1
- **发现 / Finding**: RAG 模块（src/rag/）代码完整但全链路未接通（建块/检索/注入/前端提示四环节断链），属死代码，已记入提升计划 §四待办

### 2026-08-12 — 多服务商模型检测 + 抗幻觉策略 + 创意页功能完整性恢复

- **状态 / Status**: ✅ 已完成 / Completed
- **范围 / Scope**:
  - 内置 10 家主流 AI 服务商，模型列表改为按选中服务商 API 实时查询（不再硬编码）；商汤端点修正为 `https://token.sensenova.cn/v1`，5 个真实模型（含看图模型）可检出
  - 抗幻觉：分场景温度策略（创作 0.6 / 普通 0.5 / OA 0.35 / 专利分析 0.2–0.3）+ 创意与事实分离 / 事实纪律条款
  - 创意页功能完整性恢复：12 个核心函数、会话删除、滚动到底、粘贴图片、TXT 附件、4 个标签页数据加载、概览页正文渲染、证据 API 路由
  - 防再犯硬保障：`check_html_functions.mjs` 静态扫描（编译前拦截"按钮在、函数没了"）+ e2e 创意页功能完整性用例（54/54）+ AGENTS.md 强制流程
- **提交 / Commits**: `e28a787` `835e039` `fa10d97` `6116277` `d4ab6dd` `a2d6e71` `3886970` `04f5887` `7a8f268` `9a908af` `414140f` `956b8b0`
- **验证 / Verification**: Puppeteer E2E 54/54；`check_html_functions.mjs` 8/8 模板通过（含破坏性灵敏度验证）；Rust fmt/clippy/test 通过
- **记录 / Records**: `docs/errors.md` 已记录 42c726e / 9f1a14b 两次误删事故复盘

### 2026-08-12 — FreeCAD 可视化对话与 AionCAD Rust 桥交付

- **状态 / Status**: ✅ 已完成并进入 PR 交付 / Completed and ready for PR handoff
- **范围 / Scope**: 在研创台、专利详情 AI 对话和 OA 讨论接入共享 FreeCAD 图卡；支持自然语言创建、显式继续修改、历史恢复、PNG 预览及 FCStd/STEP 下载。InnoForge 只调用独立 AionCAD Rust HTTP 桥，不引入 Python 服务、额外沙箱或 FreeCAD Skill 代码。
- **InnoForge 提交 / Commits**: `f0cbc80`（v18 产物存储）、`59cc1cd`（AionCAD 适配层）、`c54ac46`（CAD API）、`2de4e8b`（共享图卡）、`757643a`（三处对话与设置页）、`036191f`（启动、桥身份和续改加固）；审查阻断项修复见 PR #8 最新提交。
- **AionCAD / Bridge**: PR #3 已合并，merge commit `039eac9`；Rust bridge schema 为 `aioncad.rust-bridge.v2`，bootstrap 支持分阶段切换、旧桥恢复和同 PID 端口回收验证。
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings` 通过；Rust 全量测试通过；ESLint 0 errors；Puppeteer 51/51；Forge 脱敏核心审查 10/10；真实 AionCAD/FreeCAD 冒烟完成创建、二次打孔修订、历史排序、PNG/FCStd/STEP 下载、自动启动关闭回退和图像目检。
- **版本 / Version**: 保持 `0.7.4`，按用户要求不放大版本号；变更记录保留在 `[Unreleased]`。
- **记录 / Records**: `docs/plans/2026-08-11-freecad-visual-chat-design.md`；`docs/plans/2026-08-11-freecad-visual-chat-implementation.md`。

### 2026-08-02 — 前端全面重新设计 + Linear 式高级深色视觉升级

- **状态 / Status**: ✅ 已完成 / Completed
- **范围 / Scope**: 前端从赛博毛玻璃风全面重设计为 Linear/Raycast 式高级科技深色；导航移回页面右侧；修复 OA 讨论既有问题（附件随消息发送、历史讨论 onclick/data-id 注入面、onmouseleave 语法与 DOM 闭合）
- **提交 / Commits**:
  - `42c726e` feat: 前端全面重新设计为精致深色专业风（设计系统 token + 组件库、8 页模板重构、内联样式收敛）
  - `5f365d6` feat: 增强前端视觉辨识度（渐变品牌/模式卡片/页面标题区）
  - `6bc537a` feat: 首页功能卡入口与检索详情视觉细节增强
  - `5465371` feat: 升级 Linear 式高级深色视觉并将导航移回右侧
  - `d091dd8` fix: 清除 Linear 主题残留的旧蓝色硬编码
  - `56f10e4` fix: 专利详情导航激活态映射与论点看板初始隐藏修复
  - `00c9e9a` docs: 记录专利详情激活态与论点看板显示修复
  - `e7defd4` fix: OA讨论附件随消息发送与历史讨论入口安全修复
  - `51388ec` fix: 彻底消除历史讨论 data-id 属性注入面并修复 DOM 闭合
- **验证 / Verification**: Puppeteer E2E 48/48（release 实测）；`cargo test --lib` 137 passed；`cargo fmt --check`/`cargo clippy -D warnings` 通过；security_review 与 review 无阻断；浏览器冒烟确认右侧导航与 Linear token 生效
- **记录 / Records**: `docs/plans/2026-08-02-frontend-redesign.md`

### 2026-07-17 — v0.7.4 双平台发布完成

- **状态 / Status**: ✅ 已完成 / Completed
- **范围 / Scope**: 将 `feat/oa-fact-check` 中尚未进入 `main` 的 OA 事实核查、讨论持久化、完整记录导出、DOCX 修复、数据完整性和安全加固整理为 `v0.7.4`。
- **版本决策 / Version decision**: 既有 `v0.7.3` 已在 GitHub/Gitee 发布且指向旧 `dev` 提交；本次主要是 OA 模块的连续修复和增强，没有形成新的产品阶段，按用户确认采用补丁版本 `0.7.4`，不移动旧标签。
- **门禁 / Gates**: Forge 记录、Rust 格式/Clippy/测试/正式构建、JS/ESLint、Puppeteer 48/48、核心流程 17/17 和真实 AI 深度门禁 4/4 全部通过。
- **发布 / Release**: 发布提交 `0ff7fc9` 与标签 `v0.7.4` 已同步 GitHub/Gitee；GitHub Actions `29546527695` 成功，两个 Release 均反查确认 Linux 双架构、macOS 双架构和 Windows x86_64 共 5 个附件。
- **发布修复 / Release fix**: `528416d` 补齐 Gitee Release 的 `target_commitish`、附件 `release_id` 参数，并为缺失 `GITEE_TOKEN` 提供明确失败提示；Actions Secret 已配置。
- **记录 / Records**: `docs/records/2026-07-17-v0.7.4-release-forge-worklog.md`；`docs/plans/2026-07-17-v0.7.4-release-plan.md`。

### 2026-07-16 — OA 动态讨论面板完整记录导出入口

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `df23b92` (`fix: 恢复OA讨论记录导出入口`)
- **补充提交 / Follow-up**: `3f07e15` (`fix: 保留讨论导出的分析上下文`)
- **修复 / Fix**: 分析后动态讨论面板缺少完整记录导出按钮，原有功能仅存在于被隐藏的旧面板。现将按钮直接置入动态讨论面板；完成一轮 AI 回复后可导出 OA 原文、已确认分析、全部讨论原文与时间戳。讨论视图初始化时同步保存分析原文，确保恢复/导入等路径也不会遗漏分析上下文。
- **验证 / Verification**: `cargo fmt --all --check`、`cargo clippy --all-targets -- -D warnings` 与 `cargo test --lib`（137 项）通过；真实浏览器模拟一轮讨论并拦截下载，确认按钮可见、文件名正确，且 OA 原文、分析、用户/AI 原文和时间戳全部存在。

### 2026-07-16 — OA 表格显示与 Word 原生表格导出

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `6bf1e46` (`fix: 完整显示OA表格并导出Word表格`)
- **修复 / Fix**: OA 结果区的 Markdown 表格改为独立横向滚动容器，长单元格可换行且不被裁剪。DOCX 导出器会把规范的 Markdown 表格转换为带表头底纹、边框和自动换行的 Word 原生表格，保留长依据文本。
- **验证 / Verification**: DOCX 原生表格专项测试通过；浏览器 E2E 48/48 通过；`cargo fmt --all --check`、`cargo clippy --all-targets -- -D warnings` 和 `cargo test`（137 + 139 + 6 + 37 项）通过。

### 2026-07-16 — OA 普通分析路径答复书入口回归

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `69a82ea` (`fix: 恢复OA分析后的答复书入口`)
- **修复 / Fix**: 普通分析接口成功时误调用旧的只读分析渲染函数，导致“生成意见陈述书”入口消失；现统一调用讨论视图渲染函数，分析完成后固定展示生成入口、第五部分入口和讨论区入口。
- **验证 / Verification**: 浏览器 E2E 48/48 通过；`cargo fmt --all --check`、`cargo clippy --all-targets -- -D warnings` 与 `cargo test --lib`（136 项）通过。

### 2026-07-16 — OA 答复书专业论证与反幻觉约束

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `2aa8633` (`feat: 提升OA答复书专业约束`)
- **改进 / Improvement**: 修复了答复书生成阶段丢弃既有专业 system prompt 的问题。现实际使用专利代理师级指令，要求逐项按“审查意见概述—申请人答复—证据出处—论证结论”写作；创造性论证须交代区别特征、技术效果、技术问题及缺少结合动机。讨论记录降为待核验证据，无法支撑的内容明确标为【需申请人确认】，禁止虚构技术特征、对比文件内容、页段号和法条。
- **验证 / Verification**: `cargo fmt --all --check`、`cargo clippy --all-targets -- -D warnings` 及 `cargo test --lib`（136 项）通过。

### 2026-07-15 — OA Word 导出正文空白修复

- **状态 / Status**: ✅ 已完成 / Completed
- **修复 / Fix**: 修正 `word/document.xml` 多余的 `</w:p>`，补齐 `w:sectPr` 页面节属性，避免 Word 修复文档时丢弃正文。
- **验证 / Verification**: DOCX 导出模块回归测试 2 项通过；`cargo fmt --check`、`cargo test --lib`（136 项）通过；完整构建通过。

### 2026-07-15 — OA 答复书流式生成与导出修复

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `cc5bfba`
- **修复 / Fix**: 前端 SSE 解析兼容标准 `data:` 事件格式并保留正文换行；导入讨论流程恢复 OA 原文、讨论记录与讨论 ID；生成结果不足时阻止空白 DOCX 导出并提示用户；OA 讨论数据库 v17 迁移改为幂等执行。
- **界面补充 / UI follow-up**: 正常“分析 → 讨论”流程在分析摘要下方提供固定可见的“生成意见陈述书”入口；生成后在第五部分直接显示 Word 导出按钮。
- **补充提交 / Follow-up commit**: `a49116b` (`fix: 显示OA答复书生成入口`)
- **后续修复 / Follow-up fix**: 导入讨论的 AI 回复容器、可关闭生成面板及非空 Word 导出已修复；生成内容新增逐项权利要求修改建议，并要求对证据不足处明确标示待确认。
- **后续提交 / Follow-up commit**: `c326f45` (`fix: 修复OA讨论回显与Word导出`)
- **验证 / Verification**: 浏览器实测生成 2,748 字符中文答复并成功导出有效 DOCX；Puppeteer E2E 48/48；`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`（135 + 137 + 6 + 37 项）全部通过。AI 全局超时保持 300 秒。

### 2026-07-14 — OA 讨论持久化（讨论历史面板 + 后端 API + 超时恢复 + Google 认证原子化）

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `1efef3d`
- **修复 / Fix**:
  - 超时层级恢复（全局 300s / 分析 180s / 富化 300s / HTTP 客户端 300s），OA 分析不再超时
  - OA 讨论 SSE `done` 事件返回 `discussion_id`；`api_ai_oa_discuss` 保存完整 JSON 历史到数据库
  - 新增 `GET /api/ai/oas/{patent}/discussions`（列表）和 `GET /api/ai/oas/{patent}/discussions/{id}`（详情）API
  - 前端讨论历史面板：显示历史讨论列表，支持恢复/新讨论；`generateResponseLetter` 传入 `discussion_id`
  - Google 认证原子化持久化（`set_settings_batch`）
- **验证 / Verification**: `cargo fmt --check` ✅, `cargo clippy -- -D warnings` ✅, `cargo test --lib` ✅ (135 passed), JS `--check` ✅

### 2026-07-13 — AI 超时分级恢复 / AI timeout tier restoration (reverted from 60s ceiling)

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `1efef3d` (合并到 OA 讨论持久化提交)
- **修复 / Fix**: `cade390` 将 `CHAT`、`ANALYSIS`、`ENRICHMENT`、`PROVIDER_HTTP_TIMEOUT_SECS`、`GLOBAL_TIMEOUT_SECS` 统一为 60 秒，导致 OA 三步→一步合并后的单步大上下文分析超时失败。已恢复分级：`CHAT` 60s、`ANALYSIS` 180s、`ENRICHMENT` 300s、HTTP 客户端 300s、全局守卫 300s。
  Reverted the 60-second ceiling introduced by `cade390`. Restored tiered timeouts: `CHAT` 60s, `ANALYSIS` 180s, `ENRICHMENT` 300s, provider HTTP client 300s, global `tokio` guard 300s. OA analysis no longer times out.
- **验证 / Verification**: `cargo fmt --check` ✅, `cargo clippy --all-targets -- -D warnings` ✅, `cargo test` ✅ (137 unit + 6 orchestration + 37 integration passed; 1 doctest ignored). `git diff --check` ✅.
  已同步 `src/ai/client.rs` 模块注释、`src/ai/chat.rs` 注释、`src/routes/mod.rs` 测试断言。
  `src/ai/client.rs` module comment, `src/ai/chat.rs` comments, and `src/routes/mod.rs` test assertions synchronized.

### 2026-07-13 — Google 认证状态原子持久化 / Google authentication-state atomic persistence

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `1efef3d` (合并到 OA 讨论持久化提交)
- **修复 / Fix**: gcloud CLI、ADC 文件、OAuth 授权码交换及三类后台 Token 刷新统一通过 `persist_google_auth_state()` 写入 SQLite 批量事务；事务成功后才更新运行时内存。OAuth 初次授权原子保存 access token、expiry、refresh token 与 `oauth` 模式；gcloud/ADC 初次认证明确清空旧 refresh token 并写入 `gcloud` 模式；后台刷新只替换 access token/expiry，保留既有 refresh token 与认证模式。
  gcloud CLI, ADC-file, OAuth authorization-code exchange, and all three background token-refresh paths now use `persist_google_auth_state()` to write a SQLite batch transaction before updating runtime memory. Initial OAuth atomically saves access token, expiry, refresh token, and `oauth` mode; initial gcloud/ADC explicitly clears a stale refresh token and writes `gcloud` mode; background refresh replaces only access token/expiry while retaining the existing refresh token and mode.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`（137 单元测试 + 6 编排集成测试 + 37 集成测试通过；1 个 doctest 按设计忽略）及 `git diff --check` 通过。新增 OAuth 完整保存、OAuth→gcloud 模式切换和后台刷新字段保留测试。

### 2026-07-13 — SerpAPI 多 Key 原子保存 / Atomic multi-key SerpAPI saves

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `5bbedbc`
- **修复 / Fix**: SerpAPI 请求完成全量输入验证后，现在把兼容单 Key 槽位和编号 `_1` 至 `_5` 槽位作为一次完整替换提交给 SQLite 事务；写入或提交失败时不会清空旧配置、不会改变内存，并返回友好错误。提交成功后才更新运行时 Key，`.env` 后备逐项失败会记录 warning 而不会泄露 Key 内容。
  After complete request validation, SerpAPI saves now submit legacy and numbered `_1` through `_5` slots as one SQLite transaction. A write or commit failure cannot clear the old configuration or change memory, and returns a friendly error. Runtime keys update only after commit; each failed `.env` backup is warned without exposing key material.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（309 passed, 1 ignored）和正式二进制构建通过；`/`、`/settings`、`/oa-response` 返回 HTTP 200，向 `POST /api/settings/serpapi` 提交非数组值得到受控错误且不写入配置。

### 2026-07-13 — AI 配置原子保存 / AI configuration atomic persistence

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `7628984`
- **修复 / Fix**: `api_save_ai` 先把 AI 地址、服务商与通用 Key、模型、Google 凭据和 Gemini CLI 开关共 8 项设置写入一个 SQLite 事务；任何执行或提交错误都会回滚、记录服务端诊断并返回友好 JSON，运行中的内存配置不会被提前切换。事务成功后才更新内存，`.env` 只保留为失败可记录的桌面后备。
  `api_save_ai` now writes the base URL, provider/general keys, models, Google credentials, and Gemini CLI switch as eight SQLite settings in one transaction. Any execution or commit error rolls back, logs server diagnostics, returns friendly JSON, and never switches the running configuration early. Memory updates only after success; `.env` is a warning-logged desktop backup.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（307 passed, 1 ignored）和正式二进制构建通过；`/`、`/settings`、`/oa-response` 均返回 HTTP 200，向 `POST /api/settings/ai` 提交无效协议返回受控错误且不写入配置。

### 2026-07-13 — SerpAPI Key 保存完整性 / SerpAPI Key save integrity

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `6be94b6`
- **修复 / Fix**: SerpAPI Key 保存改为先完整解析和校验、再执行清空/写入/内存更新。缺失或非数组、超过 5 项、空白或超长项、短 Key、非法字符和无法唯一还原的掩码均在任何持久化前返回用户友好错误；空数组仍代表用户明确清空。合法完整 Key 与唯一掩码保持原成功响应。
  SerpAPI Key saves now fully parse and validate before clearing, writing, or updating memory. Missing/non-array input, more than five entries, blank/oversized entries, short or malformed keys, and masks that cannot be uniquely restored return friendly errors before persistence; an empty array remains an explicit clear. Valid full keys and unique masks retain the prior success response.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（305 passed, 1 ignored）和正式二进制构建通过；实际 `POST /api/settings/serpapi` 提交短 Key 返回受控错误，未写入配置；`/`、`/settings`、`/oa-response` 均返回 HTTP 200。

### 2026-07-13 — 正则初始化 panic 加固 / Regex initialization panic hardening

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `26e20b2`
- **修复 / Fix**: 创意报告行内 Markdown、专利说明书 HTML 清理与 Sogou 法律状态页面解析的静态正则均改为缓存 `Result`，删除生产路径 `expect()`。异常时分别保留已转义原文、返回用户友好 JSON 或传递受控错误以触发既有降级链；诊断仅写入服务端日志。行内 Markdown 统一在函数入口转义用户文本，并新增格式和 XSS 回归。
  Static regexes used by idea-report inline Markdown, patent-description HTML cleanup, and Sogou legal-status parsing now cache a `Result`, removing production-path `expect()`. Failures respectively preserve escaped source text, return friendly JSON, or propagate a controlled error into the existing fallback chain; diagnostics stay server-side. Inline Markdown now consistently escapes user text at its entry point, with rendering and XSS regressions added.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（299 passed, 1 ignored）与正式二进制构建通过；`/`、`/idea`、`/oa-response` 均返回 HTTP 200。

### 2026-07-13 — DOCX 导出安全加固 / DOCX export safety hardening

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `b8b6e89`
- **修复 / Fix**: OA 意见陈述书的 DOCX 生成改为返回受控 `Result`；所有 ZIP 写入和收尾失败均不再触发生产路径 panic。专利号、申请人、审查意见类型和答复正文均经 XML 文本转义，避免 `&`、`<`、`>` 损坏 `word/document.xml`。接口在导出失败时记录详细错误并返回用户友好提示，成功响应格式保持不变。
  OA response-letter DOCX generation now returns a controlled `Result`; every ZIP write/finalization failure avoids production-path panic. Patent number, applicant, office-action type, and response text are XML-text escaped so `&`, `<`, and `>` cannot corrupt `word/document.xml`. The API logs detailed failures and returns a friendly message while preserving the successful response shape.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（295 passed, 1 ignored）和正式二进制构建通过；`/`、`/oa-response` 及携带特殊字符的本地 `POST /api/oa/export-docx` 均返回 HTTP 200。内存 DOCX 回归测试解压并验证所有四个非可信字段的 XML 转义。

### 2026-07-13 — 移动端嵌入服务生命周期加固 / Mobile embedded-server lifecycle hardening

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `6f193ec`
- **修复 / Fix**: FFI 启动路径将 Tokio runtime 和线程创建错误传播回调用方，返回码为 `1` 而非 panic；服务器状态 Mutex 中毒同样返回受控错误。重复启动不会再替换或丢弃已有句柄，关闭流程先在短锁作用域取出句柄，再在锁外发送关闭信号并等待线程退出。
  The FFI start path now propagates Tokio runtime and thread-creation failures to the caller as return code `1` rather than panicking; a poisoned server-state mutex also returns a controlled error. Duplicate starts no longer replace or lose an existing handle, and shutdown takes the handle in a short lock scope before signalling and joining outside the lock.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings` 和 `cargo test`（293 passed, 1 ignored）通过。正式二进制重新构建后，`/` 与 `/oa-response` 均返回 HTTP 200。

### 2026-07-13 — 专利图片代理响应边界 / Patent image-proxy response boundaries

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `421df26`
- **修复 / Fix**: 既有的可信 HTTPS 域名校验之外，图片代理现在关闭环境代理，限制上游响应为 PNG/JPEG/GIF/WebP/BMP/AVIF 等安全栅格 MIME，拒绝缺失类型、SVG、HTML 和其它类型。通过 `Content-Length` 预检与 `chunk()` 流式累计双重检查，将单张图片限制为 20 MiB；同时移除每次响应 `leak()` MIME 字符串的内存泄漏。
  In addition to existing allowlisted-HTTPS URL validation, the image proxy now disables environment proxies and accepts only safe raster MIME types (PNG/JPEG/GIF/WebP/BMP/AVIF), rejecting missing types, SVG, HTML, and other types. A `Content-Length` precheck plus streamed `chunk()` accumulation limits each image to 20 MiB and removes the per-response MIME-string memory leak.
- **验证 / Verification**: 新增 MIME 正规化/危险类型、声明长度和流式边界回归；`cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（293 passed, 1 ignored）通过。正式二进制的 `/` 和 `/oa-response` 返回 HTTP 200；不可信 HTTP 图片 URL 返回 HTTP 403。

### 2026-07-13 — 本地上传 PDF 文件签名校验 / Local PDF-upload signature validation

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `8170576`
- **修复 / Fix**: `api_upload_pdf_store`、文档对比上传、通用文本提取和专利 PDF 专用提取现在都会在写入磁盘或调用 PDF 解析、OCR、AI 视觉兜底前检查首 1024 字节内的 `%PDF-` 文件签名。仅把文本或 HTML 改名为 `.pdf` 的请求会得到用户可见的错误；专利专用入口还会对直传和远程下载在汇合后的字节统一复检。
  `api_upload_pdf_store`, document comparison upload, general text extraction, and patent-specific PDF extraction now inspect the `%PDF-` signature in the first 1024 bytes before disk writes, PDF parsing, OCR, or AI vision fallback. Text or HTML merely renamed to `.pdf` receives a user-visible error, and the patent-specific endpoint rechecks both direct and remote bytes at their shared boundary.
- **验证 / Verification**: 纯函数回归覆盖有效签名、前导空白、HTML、普通文本伪装和 1024 字节边界；`cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（285 passed, 1 ignored）以及正式二进制 HTTP 回归均通过。伪造 PDF 被 `/api/upload/pdf-store` 明确拒绝，`/` 与 `/oa-response` 均返回 HTTP 200。

### 2026-07-13 — AI 单次调用 60 秒上限 / AI single-call 60-second ceiling

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `cade390`
- **修复 / Fix**: Chat、Analysis、Enrichment 的调用时钟、默认提供商 HTTP 客户端、全局 `tokio` 守卫和 OA 流式调用均统一为 60 秒。保留原有重试和错误降级，但全局守卫确保单次调用不会因重试超出上限。
  Chat, Analysis, and Enrichment clocks, the default provider HTTP client, the global `tokio` guard, and OA streaming now use 60 seconds. Existing retries and error fallback remain, while the global guard prevents a single call from exceeding the ceiling.
- **验证 / Verification**: 新增常量回归并更新全局守卫回归；`cargo fmt --check`、`cargo clippy -- -D warnings` 和 `cargo test`（285 passed, 1 ignored）通过。正式二进制重建后 `/` 与 `/oa-response` 均返回 HTTP 200。

### 2026-07-13 — AI 提示词输入边界加固 / AI prompt-input boundary hardening

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `be815cb`
- **修复 / Fix**: `/api/ai/chat` 现在仅接受 `user` 与 `assistant` 历史角色，伪造的 `system`/未知角色会在请求上游前得到友好错误。专利记录、联网搜索、OA 分析与审查意见、讨论、最新意见和结论导出材料均用转义后的 `<user_input>` 边界隔离；原始自定义角色只作为受限偏好，服务端预设仍可作为可信角色。
  `/api/ai/chat` now accepts only `user` and `assistant` history roles; forged `system` or unknown roles receive a friendly error before any upstream request. Patent records, web results, OA analysis/office actions/discussion/latest input, and conclusion-export material use escaped `<user_input>` boundaries; raw custom roles are bounded preferences while server presets remain trusted roles.
- **验证 / Verification**: 新增 4 条边界/角色回归；`cargo fmt --check`、`cargo clippy -- -D warnings` 和 `cargo test`（283 passed, 1 ignored）通过。重新构建后 `/` 与 `/oa-response` 均为 HTTP 200；真实 POST 请求携带伪造 `system` 历史时返回本地友好错误且不调用 AI。

### 2026-07-13 — D 盘运行期 PDF 临时文件治理 / D-drive runtime PDF temporary-file remediation

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `dcb446d`
- **修复 / Fix**: 所有外部 PDF 提取路径（视觉回退、pdftotext、PyMuPDF、MinerU、OCR）现将临时输入及视觉 PNG 输出限制在项目工作目录的 `data/runtime-temp`。文件使用 UUID 和 `create_new` 独占创建，并由 RAII 守卫覆盖成功、失败和提前返回的清理；`pdftotext` 改为捕获标准输出，Umi-OCR 删除未使用的磁盘副本。
  External PDF extraction paths (vision fallback, pdftotext, PyMuPDF, MinerU, and OCR) now keep temporary inputs and vision PNG outputs under project-working-directory `data/runtime-temp`. UUID plus `create_new` prevents collisions, while an RAII guard cleans on success, failure, and early returns; pdftotext captures stdout and Umi-OCR no longer writes an unused disk copy.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings` 和 `cargo test`（275 passed, 1 ignored）全部通过；临时文件回归确认路径和清理行为，目录无残留；新二进制的 `/` 与 `/oa-response` 均返回 HTTP 200。

### 2026-07-13 — 远程专利 PDF 下载安全加固 / Remote patent-PDF download hardening

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `2fd64ab`
- **修复 / Fix**: 专利记录中的远程 `pdf_url` 现在仅允许 HTTPS 主机名、默认 443
  端口和无凭据 URL。服务端解析并固定全部公网 DNS 地址，关闭环境代理和重定向，
  拒绝非 2xx、超过 20 MB、流式超限和无有效 PDF 签名的响应，防止 SSRF、DNS
  重绑定及内存耗尽。
  Remote `pdf_url` values now require an HTTPS hostname on the default port with no credentials.
  The server resolves and pins only public DNS addresses, disables proxies and redirects, and
  rejects non-2xx, oversized, streaming-overlimit, and invalid-PDF responses to prevent SSRF,
  DNS rebinding, and memory exhaustion.
- **验证 / Verification**: 新增 URL/IP/大小/PDF 签名/localhost DNS 回归；`cargo fmt
  --check`、`cargo clippy -- -D warnings` 和 `cargo test`（273 passed, 1 ignored）通过；
  新版服务的 `/` 与 `/oa-response` 均返回 HTTP 200。


### 2026-07-13 — OA 可审计完整讨论记录导出 / OA auditable full discussion-record export

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `5588968`
- **新增 / Added**: OA 讨论区新增“导出完整讨论记录”，在浏览器本地下载 UTF-8 Markdown，不调用 AI 或新增后端接口。记录保留起始系统上下文、每轮用户/AI 原文、角色、ISO 时间戳和导出时间；含反引号的原文会使用动态 Markdown 代码围栏完整保存。
  The OA discussion panel now provides “Export Full Discussion Record”, a local UTF-8 Markdown download with no AI call or new backend endpoint. It preserves the initial system context, every user/AI source message, role, ISO timestamp, and export time; source text with backticks is retained using dynamic Markdown fences.
- **改进 / Improvement**: 原“导出结论”明确更名为“AI 总结结论”，避免把二次 AI 摘要误认为最终答复或讨论全过程。
  The former “Export Conclusions” action is explicitly relabelled “AI Summary” so a second AI summary is not mistaken for a final response or the complete discussion.
- **验证 / Verification**: Puppeteer E2E 48/48 真实点击导出按钮并读取受控 Blob 内容，确认全文尾部、反引号、角色、时间戳、原始记录声明和零 AI 请求；JS/Rust 门禁全部通过。
  Puppeteer E2E 48/48 clicks the export button and reads the controlled Blob content, verifying full tails, backticks, roles, timestamps, the original-record notice, and zero AI requests; all JS/Rust gates passed.

### 2026-07-13 — 八页面浏览器回归与搜索页初始化修复 / Eight-page browser regression and search initialization fix

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `aad56d5`
- **修复 / Fix**: 搜索页在定义 `updatePdfFileList()` 前调用它，导致页面加载时出现 `ReferenceError`；现已保持原有 PDF 恢复语义并在函数声明后调用。
  Search called `updatePdfFileList()` before defining it and produced a load-time `ReferenceError`; it now preserves the original PDF-restoration behavior and invokes it after the declaration.
- **测试 / Tests**: `e2e_test.mjs` 扩展为 42 项真实浏览器回归：8 个页面的 HTTP/关键节点/浏览器异常/失败请求/无副作用交互，并保留 OA 参数校验与长文本尾标记完整性。专利详情在有数据时验证标签交互，空库时验证明确的 404 提示；不写入测试数据或调用真实 AI。
  `e2e_test.mjs` now has 42 real-browser regressions: HTTP/critical node/browser-error/failed-request/side-effect-free interaction checks across eight pages, plus OA validation and long-payload tail integrity. Patent detail exercises tabs with data and validates the explicit 404 prompt in an empty library; it neither seeds data nor calls real AI.
- **验证 / Verification**: `node --check`、ESLint 无配置模式、Puppeteer 42/42、`cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（265 passed，1 ignored）通过。
  `node --check`, ESLint without repository config, Puppeteer 42/42, `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` (265 passed, 1 ignored) passed.

### 2026-07-13 — OA 后端数据完整性加固 / OA backend data integrity hardening

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `ce303d2`
- **修复 / Fix**: OA 讨论及答复书生成路径改为保留完整输入；超过容量时按 Unicode 字符计数返回用户可见的字段级错误，不再静默截断 OA 原文、分析或讨论历史。
  Discussion and response-letter paths now retain complete input and return visible field-specific Unicode-character capacity errors rather than silently truncating OA material, analysis, or discussion history.
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings` 与 `cargo test` 通过（245 项通过，1 项文档测试按设计忽略）。
  `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` passed (245 passed; one doc test intentionally ignored).

### 2026-07-13 — OA 可重复端到端回归 / OA reproducible end-to-end regression

- **状态 / Status**: ✅ 已完成 / Completed
- **文件 / File**: `e2e_test.mjs`
- **覆盖 / Coverage**: 首页与 OA 页面加载、浏览器页面异常/失败请求、审核修改方案接口参数校验，以及超长请求体三字段尾标记保留；不调用真实 AI 服务。
  Home/OA page loading, browser page errors/request failures, amendment-check parameter validation, and tail-marker preservation across all three long-payload fields; no real AI call.
- **验证 / Verification**: `node --check e2e_test.mjs`、ESLint 无配置模式、`node e2e_test.mjs`（6/6）及 Rust 全量门禁通过。
  `node --check e2e_test.mjs`, ESLint without repository config, `node e2e_test.mjs` (6/6), and the full Rust gates passed.

### 2026-07-13 — 本地服务 CORS 收紧 / Local-service CORS hardening

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `15f134a`
- **修复 / Fix**: 默认跨域来源从全开放改为本机 `http://127.0.0.1:3000` 与 `http://localhost:3000`；通过 `INNOFORGE_CORS_ORIGINS` 可安全添加 HTTP/HTTPS 来源，无效配置项被忽略。
  Default CORS changed from open access to local `http://127.0.0.1:3000` and `http://localhost:3000`; `INNOFORGE_CORS_ORIGINS` can safely add HTTP/HTTPS origins while invalid entries are ignored.
- **验证 / Verification**: 允许来源预检返回对应 `access-control-allow-origin`，不受信任来源无该响应头；fmt、clippy、245 项 Rust 测试和 E2E 6/6 通过。
  The allowed-origin preflight returns its `access-control-allow-origin`, while an untrusted origin receives none; fmt, clippy, 245 Rust tests, and E2E 6/6 passed.

### 2026-07-13 — 专利图片代理 SSRF 加固 / Patent image-proxy SSRF hardening

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `1ee38f1`
- **修复 / Fix**: 图片代理改为结构化 URL 校验，仅允许 HTTPS、精确白名单主机、默认端口且不含凭据；禁用自动重定向，防止借白名单主机跳转到内网。
  The image proxy now validates structured URLs: HTTPS, exact allowlisted host, default port, and no credentials; redirects are disabled to prevent allowlisted hosts from reaching internal targets.
- **验证 / Verification**: 协议、子域、用户名伪装与非默认端口均 HTTP 403；新增 4 项 URL 回归测试，Rust 全量门禁与 E2E 6/6 通过。
  HTTP, subdomain, username-spoofing, and non-default-port inputs all return 403; four URL regressions, full Rust gates, and E2E 6/6 passed.

### 2026-07-13 — Windows 启动脚本修复 / Windows launcher fix

- **状态 / Status**: ✅ 已完成 / Completed
- **文件 / File**: `start.bat`
- **修复 / Fix**: 移除 debug 构建括号代码块内 `echo` 文本的未转义圆括号，避免 CMD 报“此时不应有 ...”。
  Removed unescaped parentheses from the debug-build echo inside a CMD block.
- **验证 / Verification**: 通过 `start.bat` 完成 debug 编译和后台启动；`/` 与 `/oa-response` 均 HTTP 200。
  `start.bat` compiled and started the server; both `/` and `/oa-response` returned HTTP 200.

### 2026-07-13 — OA 前端数据完整性修复 / OA frontend data integrity remediation

- **状态 / Status**: ✅ 已完成 / Completed
- **提交 / Commit**: `f496648`
- **核心改动 / Core changes**:
  - `templates/office_action_response.html`: 移除修改校验与讨论上下文中的 6 个正文截断表达式，覆盖 5 个逻辑问题组
    Removed six body truncations across five logical issue groups in amendment checking and discussion context construction
  - 保留日期、SSE 协议解析、消息数组和选区操作等非数据用途的 `slice`
    Preserved non-data slices used for dates, SSE parsing, message arrays, and text selection
- **验证 / Verification**: `cargo fmt --check`、clippy、245 项 Rust 测试及定制 Puppeteer 数据完整性回归通过
  `cargo fmt --check`, clippy, 245 Rust tests, and a focused Puppeteer integrity regression passed
- **已知后续项 / Follow-ups**: OA 讨论后端仍有 60k/40k/15k 字符静默截断；模板存在与 HEAD 相同的 16 个历史 ESLint `no-redeclare` 错误；仓库缺少规约引用的 `e2e_test.mjs`
  The OA discussion backend still silently truncates at 60k/40k/15k characters; the template retains 16 baseline `no-redeclare` errors identical to HEAD; the referenced `e2e_test.mjs` is absent

### 2026-07-09 — OA 分析模块：三步→一步+缓存+超时移除 (v0.7.4)

- **PR**: 三步→一步 prompt 重构、OA 缓存、超时移除、论点看板修复
- **状态**: ✅ 开发完成，待提交
- **核心改动**:
  - `ai/patent.rs`: OA 分析从 3 步串行合并为 1 步，deep mode 精简输出
  - `routes/ai.rs`: OA 缓存（patent_number + oa_type + depth）、超时移除
  - `office_action_response.html`: 论点看板修复（本地 AI chat 调用）
  - `ARCHITECTURE.md`: 补充 OA 模块章节（5a 节）
- **关联 PR 号**: v0.7.4
- **技术债务**: ⏳ OA 数据库存储方案（长期）

### 2026-07-08 — 文件解析器重构 (v0.7.3)

- **PR**: 重构文件解析器，提升 PDF/DOCX/DOC 解析准确性
- **状态**: ✅ 已完成
- **核心改动**:
  - `file-parser.rs`: 重构 `parse_file_to_markdown` 和 `get_preview_text`
  - PDF 解析器: OCR 模式支持、文字层检测优化
  - DOCX 解析器: 表格解析、图像提取、结构化内容处理
  - DOC 解析器: `docx2txt-js` 替代方案
- **关联 PR 号**: v0.7.3
- **技术债务**: 无

### 2026-07-07 — 数据库 schema 优化 (v0.7.2)

- **PR**: 扩展 `documents` schema 以支持文件预览信息
- **状态**: ✅ 已完成
- **核心改动**:
  - `schema.sql`: 新增 `file_content`, `file_ext`, `is_processed`, `last_processed_at` 字段
  - `db/document.rs`: 新增文档处理状态查询接口
  - `routes/document.rs`: 文档处理 API 完善
  - 修复 `documents` 与 `case_documents` 关系
- **关联 PR 号**: v0.7.2
- **技术债务**: 无

### 2026-07-06 — OCR 模式支持

- **PR**: 添加文件上传 OCR 模式
- **状态**: ✅ 已完成
- **核心改动**:
  - `routes/upload.rs`: OCR 模式参数处理
  - `file-parser.rs`: OCR 模式 PDF/DOC/DOCX 解析
  - 支持 Tesseract OCR 引擎
- **关联 PR 号**: v0.7.1
- **技术债务**: OCR 性能优化（异步处理）

### 2026-07-05 — 研创台 AI 分析模块（InnoForge 核心功能）

- **PR**: 实现研创台 AI 分析全链路
- **状态**: ✅ 已完成
- **核心改动**:
  - `routes/ai.rs`: `/api/ai/innovation/analyze`, `/api/ai/innovation/analyze-stream`, `/api/ai/innovation/compare` 等端点
  - `ai/innovation.rs`: 分析引擎（专利地图 + 对比分析 + 策略建议）
  - `templates/innovation_analysis.html`: 前端展示
  - **AI 分析样板文档**: `docs/研创台 AI 分析样板.doc` — 提供 4 个场景的详细分析报告
- **关联 PR 号**: v0.7.0
- **技术债务**: 无

### 2026-07-04 — 研创台 UI 增强

- **PR**: 修复研创台样式问题、添加批量操作、新增搜索功能
- **状态**: ✅ 已完成
- **核心改动**:
  - 批量删除/批量重命名功能
  - 高级搜索（标题/类型/日期/关键词）
  - UI 样式统一
- **关联 PR 号**: v0.6.9
- **技术债务**: 无

---

## 当前版本 (Current Version)

**版本**: v0.7.4（已发布）；main 已积压 MA1/MA2a/MA2b/MA5a 等未发版变更，拟发 v0.8.0（见 docs/plans/2026-09-22-gap-analysis-improvement-plan.md §七）
**发布日期**: 2026-07-09
**主要特性**:
- OA 分析三步→一步重构，消除超时风险
- OA 缓存机制，减少重复 API 调用
- 超时移除，让 provider 300s 兜底
- 论点看板修复，使用本地 AI chat 服务
- ARCHITECTURE.md 补充 OA 模块描述

**版本历史**:
- v0.7.4 (2026-07-09): OA 分析重构
- v0.7.3 (2026-07-08): 文件解析器重构
- v0.7.2 (2026-07-07): 数据库 schema 优化
- v0.7.1 (2026-07-06): OCR 模式支持
- v0.7.0 (2026-07-05): 研创台 AI 分析模块

---

## 技术债务 (Technical Debt)

1. **OA 数据库存储方案**: 长期，当前 OA 数据仅存储在 `case_documents` 中，未来可扩展专用 OA 数据库表
2. **OCR 性能优化**: 异步处理，避免阻塞主线程
3. **文件解析器错误处理**: 需要更完善的错误处理和用户提示
4. **前端验证基线**: 补齐 `e2e_test.mjs`，并清理 `office_action_response.html` 现有 16 个 ESLint `no-redeclare` 错误

---

## 下一步计划 (Next Steps)

1. **OA 数据库表**: 设计并实现 OA 专用表结构（审查意见历史、答复历史、审批流程）
2. **性能监控**: 为关键 API 端点添加性能监控和日志
3. **测试覆盖**: 为 OA 分析模块和文件解析器添加单元测试
4. **文档补全**: 为 OA 模块前端页面添加使用说明
