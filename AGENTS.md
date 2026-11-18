# 创研台 InnoForge 项目规约（AI Agent 与贡献者强制宪章）

> **定位**：本文件是所有 AI Agent 与人类贡献者的最高工程行为准则；只定规矩与架构边界，不承载阶段流水账（演进蓝图见 `docs/plans/2026-11-18-v1-overhaul-plan.md`，历史记录见 `docs/plans/STATUS.md`）。

## §〇 最高效力宪章：全谱系创研六大公理（暨 2026-10-27 整顿令结项声明）

1. **整顿令结项与基线切换**：原 [2026-10-27 整顿令](docs/plans/2026-10-27-rectification.md)（M1→M2→M3 实案闭环及至 2026-11-17 冻结窗口）已按期完成并归档结项，不再要求旧版 `M1/M2/M3` 开工口令。自 2026-11-18 起，全仓统一以本节宪章、`docs/plans/product-vision.md` 与 `docs/plans/2026-11-18-v1-overhaul-plan.md` 为唯一行动准绳。
2. **效力优先级链**：安全与案件隐私一票否决红线 > 本节六大永久公理与 `docs/plans/product-vision.md` > 本文件工程铁律 > `docs/plans/2026-11-18-v1-overhaul-plan.md` > 历史归档文件。
3. **六大永久业务与架构公理（严禁窄化理解成“单一专利工具”，永不重复解释）**：
   - **公理 1 · 表层每张卡片独立自治**：首页 7 张功能卡片（全域检索、多模态共创推演、单篇深析、多方案对决、审查与审稿抗辩、全谱系成果锻造、配置与知识金库）必须保持 **100% 独立即点即用、互不捆绑前置工程**；水下通过本机 SQLite 知识与证据金库（Vault）按需调取、复利沉淀。
   - **公理 2 · 存量与增量双线并行**：默认服务一线技术开发者“我方先前已有专利/已发论文/在办 OA”与“新想法、新产品开发及实验验证”并行推进的常态，支持挂载自有前序成果做增量创新剥离、防自我公开/抵触申请预警，以及对照实验实测数据双向回填。
   - **公理 3 · 四维全域情报检索**：从新想法查起绝不只查专利库，底层检索必须覆盖**「全球专利 + 学术期刊/会议/预印本论文（OpenAlex / Crossref / arXiv / Semantic Scholar 等）+ 开源代码工程 + 行业标准/白皮书」**四大圈层。
   - **公理 4 · 多模态复合输入与多轮共创**：技术构想沟通与深析绝非单次纯文本盲跑，必须原生支持**粘贴截图/手绘草图/CAD 与电路图 + 拖入 PDF/DOCX/代码/CSV 实验表等复合模态输入 + 多轮追问收敛**为结构化方案基线后再进自动化验证。
   - **公理 5 · 审查与审稿三栖抗辩**：基于原文锚点的核验抗辩引擎一核三用——① **已有专利官方 OA/复审答复**、② **新专利提交前“魔鬼审查员”模拟预审**、③ **学术论文/期刊模拟同行评审与返修逐条回复（Rebuttal / Response to Reviewers）**。
   - **公理 6 · 一源四出与公开时序守卫**：同一经核验技术内核覆盖**「产品工程方案（PRD/TRD）、调研/FTO 报告、学术论文/期刊手稿（强制仅绑定真实核验 DOI/BibTeX）、专利交底与 OA 三件套」**四类成果，并强制内置**“先锁专利申请日，后公开论文/预印本/开源代码”**的防自毁时序守卫。
4. **最高实效验收标尺**：目标用户是一线技术开发者与工程师（非专职专利代理师）。一切开发与重构只认 `docs/plans/product-vision.md` §3（**报告敢用 · 决策敢做 · 答复与成果敢交**），严禁拿代码行数、测试数、PR 数或文档数冒充进度。

## §一 安全与隐私一票否决红线

1. **案件与研发机密不出本机仓**：真实在办 OA/复审材料、未公开新产品图纸、未发表论文手稿与涉密实验数据仅允许存放在本地忽略目录（如 `data/`），**严禁进入 Git 提交、PR、Issue、测试夹具或 CI 日志**（配合 `.githooks/pre-commit` 与禁词表守卫）。
2. **网络绑定与密钥保护**：桌面二进制默认仅绑定回环地址 `127.0.0.1:3000`；非回环绑定（如 Docker `0.0.0.0`）必须强制启用同源会话/Token 鉴权；设置查询接口严禁回显明文 API Key。

## §二 上下文记忆机制与模块分层纪律

1. **运行时上下文自动汇编（`src/context.rs`）**：按优先级读取 `AGENTS.md`、`docs/plans/product-vision.md`、`docs/feedback.md` 与精炼踩坑摘要（`docs/errors-digest.md` ≤4KB，超长历史归入 `docs/archive/`），总注入预算硬控在 `≤6000` 字符内，缺失文件静默降级。
2. **二元正交代码结构（`src/core/` × `src/cards/`）**：
   - 下层 `src/core/` 承载全仓唯一确定性公共底座（`vault` 统一文献/特征/实验模型与混合检索、`multimodal` 自适应三级解析与多轮会话、`omni_search` 四维检索、`evidence_rebuttal` 原文/DOI 回查与硬阻断总闸门、`runtime` 9 服务商容灾与可分叉调度导出）；
   - 上层 `src/cards/` 严格对应 7 张独立功能卡片，**卡片模块之间严禁直接相互引用**（仅可调用 `src/core/`，通过本地 Vault 数据互通）。

## §三 编码、前端、数据库与发布硬规范

1. **Rust 稳定性与并发铁律**：
   - 生产路径严禁 `unwrap()` / `expect()`；
   - **严禁裸字节切片 `&s[..n]`**（防中文 UTF-8 字符边界崩溃，必须使用 `char_indices()` 等安全方法，由 `tests/no_unsafe_byte_slice.rs` 静态扫描）；
   - 严禁循环逐条查库（N+1）与异步运行时内同步阻塞调用（禁用 `reqwest::blocking`）。
2. **AI 通信与防幻觉门禁**：单次调用上限 60 秒并配自动容灾降级；用户输入必须包裹 `<user_input>` 隔离防提示词注入；所有专利引证、论文 DOI 与实验数据必须经确定性代码回查核验，核验被拒必须触发硬降级标注或阻断，**严禁仅打 `warn` 日志放行**。
3. **模型、迁移与路由防漂移**：
   - 禁止跨模块重复定义公共结构体，新增公共 `struct` / `enum` 须派生 `#[derive(Debug, Clone, Serialize, Deserialize)]`；
   - SQLite Schema 变更必须通过版本化迁移文件递增演进（严禁篡改历史迁移版本或启动时全表同步大循环锁库），开启 WAL 并发模式；
   - 所有 HTTP 路由统一在顶层路由构建器（现 `src/common.rs` `build_router` / 重构目标 `src/app.rs`）注册，供桌面端 `src/main.rs` 与移动端 C ABI `src/lib.rs`（须带 `catch_unwind` 防跨 FFI 崩溃）共享；变更路由必须同步运行 `scripts/gen_route_inventory.js` 与 `scripts/gen_api_contracts.js` 更新 API 契约文档。
4. **前端零构建与安全规范**：
   - 坚持原生 HTML/CSS + 原生 ES Modules / Web Components，**严禁引入 Webpack / Vite / React 等任何前端打包构建工具**；
   - 面向用户文案统一经 `static/i18n.js` 做中英双语键值齐平；
   - 严禁 `innerHTML = 未清洗内容`（须用 `createElement + textContent` 或 `static/purify.min.js`）；使用 `const` / `let` 声明变量，保持 `eslint` 与 `scripts/check_html_functions.mjs` 零报错。
5. **提交与发版门禁（commit-first）**：提交前必须跑通 `cargo fmt --check`、`cargo clippy -- -D warnings` 与 `cargo test`；提交格式 `feat/fix/refactor/chore/docs: 中文简要描述`；发版必须执行 `scripts/release_verify.ps1` 与逐卡片全按钮动态 E2E 实测。
