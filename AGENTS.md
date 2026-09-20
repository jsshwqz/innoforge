# InnoForge 项目规约

> 本文件是所有 AI Agent 和人类贡献者的强制规范。修改代码前必须阅读并遵守。

## 记忆
本项目的协作记忆按以下位置维护。进入任务前按 Step 0b 的检索规则读取相关记忆；命中 Step 0c 的存储触发器后，须在最终回复前静默更新对应记忆：
- 用户反馈（纠正 / 认可 / 边界澄清）→ `docs/feedback.md`
- 工具 / 依赖 / CI / 构建踩坑复盘 → `docs/errors.md`（错误复盘数据库）
- 项目阶段事实与共识 → `docs/plans/STATUS.md`
- 用户长期偏好与决策习惯 → 本文件各条款（如温度策略、事实纪律、目标用户定位）

> 记忆与最新代码、文档或运行结果冲突时，以最新可验证来源为准，并在必要时更新记忆。
>
> 后端 `src/context.rs` 会在每次 AI 对话时自动读取本目录的记忆文件，汇编成有字符预算（≤6000 字符）的「项目记忆上下文」注入系统提示词，让 agent 无需自行猜测读什么即可拿到近期反馈/复盘/状态；文件缺失时静默降级，不影响主流程。

---

## 一、目标用户

面向 **研发用户**（发明人、工程师），不是专利工程师或律师。
所有功能设计、文案措辞、报告格式都以研发人员视角为准。

---

## 二、代码规范

### 2.1 语言与格式
- Rust 代码必须通过 `cargo fmt` 和 `cargo clippy -- -D warnings`（零警告）
- 提交前必须 `cargo test` 全通过
- 提交信息格式：`feat/fix/refactor/chore/docs: 中文简要描述`

### 2.2 类型与结构体
- **禁止重复定义类型**。新增数据结构前，先在 `src/patent.rs` 和对应模块中搜索是否已有同类型
- 公共类型统一放在 `src/patent.rs`，模块内部类型放在对应模块文件中
- 新增 struct/enum 必须带 `#[derive(Debug, Clone, Serialize, Deserialize)]`

### 2.3 数据库变更
- Schema 变更必须通过 `src/db/migrations.rs` 的版本化迁移，禁止手动改 DB
- 迁移版本号递增（当前 v11），新增迁移在 `run_migrations` 函数中追加
- 新增表/字段必须在迁移注释中说明用途

### 2.4 路由注册
- 所有 API 端点必须同时在 `src/main.rs` 和 `src/lib.rs` 中注册
  - `main.rs`：桌面端 / Docker 独立运行入口
  - `lib.rs`：Android / iOS FFI 共享库入口
  - 两处路由必须保持一致，新增 API 时不可遗漏任一
- 路由命名规范：`/api/{模块}/{动作}`，RESTful 风格

### 2.5 前端
- 纯 HTML/CSS/JS，禁止引入构建工具（webpack/vite 等）
- 所有面向用户的文本必须通过 `static/i18n.js` 做中英双语
- XSS 防护：禁止 `innerHTML = 用户输入`，使用 `createElement + textContent`；需要渲染富文本时必须经 DOMPurify 过滤
- **DOMPurify 是必选依赖，不是可选增强**：所有 `innerHTML` 赋值必须调用 `DOMPurify.sanitize()`。DOMPurify 加载可能失败（网络/CDN/编译嵌入问题），因此在 `static/i18n.js` 中已设置全局保护（自动 fallback）。如需在其他 JS 中使用 DOMPurify，必须确保 `i18n.js` 已加载在先。**禁止在任何页面直接调用 `DOMPurify.sanitize()` 而不依赖全局保护**。
- **截断不得破坏数据完整性**：凡是截断文本（`substring`/`slice`/`safe_truncate`/`chars().take()`），必须确认被截的是显示用途而非数据用途。用于提交给后端或传给 AI 的数据，必须保留全文，禁止为了省 token 或省空间而截断数据内容。
- **JS 变量声明**：同一作用域内禁止 `const` 在 `var` 之后声明同名变量（`no-redeclare`）。优先使用 `let`/`const` 而非 `var`。

### 2.6 AI 提示词
- AI 相关 prompt 统一放在对应 route handler 中（不单独建 prompt 文件）
- 新增 AI 功能必须设置超时（**单次 AI 调用** 60 秒上限，非整条 pipeline）
- 必须处理 AI 调用失败的降级逻辑
- **Prompt 注入防护**：用户输入拼入 prompt 前必须做边界隔离（如用 `<user_input>` 标签包裹），禁止将用户原始输入直接拼接为系统指令

### 2.7 Rust 编码安全
- 生产路径（非测试代码）**禁止** `unwrap()` / `expect()`，统一用 `Result` + `?` 传播错误
- API 层错误必须转换为用户友好的 JSON 响应，禁止将 Rust panic 信息暴露给前端
- 禁止在循环中逐条查询数据库，优先使用批量操作（`WHERE id IN (...)` 等）

---

## 三、变更追踪

### 3.1 CHANGELOG 规则
- 每个版本发布时**必须**更新 `CHANGELOG.md`
- 格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/)
- 分类：新增 / 修复 / 改进 / 安全修复 / 重大变更
- 中英双语，中文在前

### 3.2 版本号规则
- 遵循语义化版本：`MAJOR.MINOR.PATCH`
- `Cargo.toml` 中的 version 字段必须与 CHANGELOG 一致
- PATCH：bug 修复、小改进
- MINOR：新功能、非破坏性变更
- MAJOR：破坏性 API 变更

### 3.3 任务计划追踪
- 版本规划文档统一放在 `docs/plans/` 目录
- 任务完成后在计划文档中标注 ✅ 和对应 commit hash
- 禁止删除已完成的任务记录（保留完整历史）

---

## 四、文件与目录规范

### 4.1 仓库只保留核心代码
```
src/           # Rust 核心代码
templates/     # HTML 页面模板（7 个页面）
static/        # CSS + JS + 图标（不放图片资源）
tests/         # 集成测试
docs/          # 文档和规划
```

### 4.2 禁止提交到仓库的文件
- 辩论报告、分析报告等 AI 生成的 `.md` / `.txt` 产物
- 测试用的 PDF、图片、OCR 结果等临时文件
- 日志文件（`*.log`）
- 数据库文件（`*.db`）
- 环境变量文件（`.env`）

如果需要保存这类文件，放在仓库外部或添加到 `.gitignore`。

### 4.3 开发工具/依赖分离
- 核心仓库只包含 Rust + 前端静态文件
- 桌面端（Tauri）→ [`innoforge-desktop`](https://gitee.com/jsshwqz/innoforge-desktop) 独立仓库
- iOS → [`innoforge-ios`](https://gitee.com/jsshwqz/innoforge-ios) 独立仓库
- 鸿蒙 → [`innoforge-harmony`](https://gitee.com/jsshwqz/innoforge-harmony) 独立仓库
- MCP 技能/插件 → 独立仓库，不放在本项目中
- 本条本意是**禁止引入前端构建工具链**（webpack / vite / 打包器等），产品前端仍为纯 HTML/CSS/JS。
- **开发工具的依赖清单属于例外，必须入库**：`package.json`、`package-lock.json` 等（供 ESLint / Puppeteer / 扫描脚本等开发工具使用）应当提交，其中**锁文件必须与 `package.json` 同步提交**，否则 CI 的 `npm ci` 在干净 runner 上必然失败（`npm error code EUSAGE`），依赖版本也无法复现。提交前用 `npm ci --dry-run` 自检。
- 仍禁止入库的是：`node_modules/`、构建产物、以及任何真正的构建工具链配置（webpack/vite 配置与插件）。

---

## 五、标准操作流程（所有 AI Agent 必须遵守）

任何 AI Agent 接到任务后，按以下步骤顺序执行。不可跳步。

### Step 0: 读规约
- 读取本文件（`AGENTS.md`），理解项目定位、规范和禁忌
- 读取 `docs/plans/STATUS.md`，了解当前焦点、最近变更、已知问题（**每次必读**）
- 如果是首次进入项目，同时读：
  - `CHANGELOG.md` — 版本历史，了解改了什么
  - `Cargo.toml` — 当前版本号和依赖
  - `docs/errors.md` — 错误复盘数据库，避免重复踩坑
- 其他文档（`docs/ARCHITECTURE.md`、`docs/API.md`）需要时再读
- **首次克隆/进入项目后，设置 git 钩子**（防忘记跑检查）：
  ```bash
  git config core.hooksPath .githooks      # 或运行 sh .githooks/setup-githooks.sh
  ```

  钩子用 `core.hooksPath` 指向 `.githooks/`，随仓库提交、跨机器共享；每次 `git commit` 自动跑 `check_html_functions.mjs`。
- **硬关卡（强制）**：本地钩子可被 `git commit --no-verify` 跳过，它只是方便层；真正的强制关卡是远端 **CI**（`.github/workflows/ci.yml`），push/PR 后自动跑 fmt/clippy/test + HTML 函数完整性扫描 + e2e，不过关无法合并——任何 AI 或开发者想绕过本地检查都拦不住远端。

#### Step 0b: 记忆检索规则（每次任务执行）
- 固定先读 `AGENTS.md`，再读 `docs/plans/STATUS.md`（**每次必读**）。
- **只读与本任务直接相关的记忆全文**；无关的长文档（`docs/errors.md`、`CHANGELOG.md`、计划文档）只看最近若干条或标题，避免把整份历史塞进上下文。
- `docs/errors.md`（工具/构建/CI 复盘）只在当前任务会用到对应工具，或该工具刚刚失败时才展开。
- `docs/feedback.md` 只展开与当前任务最相关的几条；宁缺毋滥，不为凑「全面」继续读。
- 检索到**足够支撑当前判断的证据即停**，不要为措辞或「全面感」继续铺开记忆。
- **记忆与最新代码、文档或运行结果冲突时，以最新可验证来源为准**，并在必要时更新对应记忆。
- 不要为了「看起来读过所有东西」而重复读同一份记忆；同一轮内读过即记。

#### Step 0c: 记忆存储规则（最终回复前静默执行）
- 每轮最终回复前必须静默判断是否需要维护记忆。命中以下任一条件则先写入、再回复，命中后无需向用户解释。
- **用户反馈**：当用户用「你这里不应该 / 这样不对 / 不是 X 而是 Y / 以后遇到这种情况 / 以后都 / 我更喜欢 / 默认按」等话术给出纠正、认可或边界澄清时，写入 `docs/feedback.md`（含：规则 / 原因 / 适用场景 / 时间）；**既记纠正也记肯定**，避免 agent 越来越保守。
- **工具踩坑**：新增的构建/CI/依赖/工具失败解法，写入 `docs/errors.md`（沿用其条目格式）。
- **不应沉淀**：一次性流水账、可从代码直接读取的信息、临时任务摘要。
- 已有同主题记忆可承载时**优先更新旧条目**，不重复新建。

### Step 1: 理解任务
- 明确用户要做什么（新功能 / bug 修复 / 重构 / 分析调研）
- 有歧义时主动问用户，不要猜

### Step 2: 现状调查
- `git log --oneline -10` 了解最近改动
- `git status` 确认当前工作区状态，不覆盖用户未提交的工作
- 搜索相关代码，确认：
  - 是否已有类似实现（禁止重复造轮子）
  - 涉及哪些文件、哪些模块
  - 现有的类型定义、数据库表、API 端点

### Step 3: 方案确认
- 简要说明修改范围和方案（改哪些文件、加什么、改什么）
- 涉及以下情况时**必须等用户确认**再动手：
  - 新增 crate 依赖
  - 修改数据库 schema
  - 删除或重命名现有 API
  - 大范围重构（> 3 个文件）
- 小修复和明确任务可直接执行

### Step 4: 实现
- 按第二章代码规范编写代码
- 新增类型先查 `src/patent.rs`，避免重复定义
- 新增 API 同时注册到 `src/main.rs` 和 `src/lib.rs`
- 前端文本走 `static/i18n.js` 双语
- 数据库变更走 `src/db/migrations.rs` 版本迁移

### Step 5: 验证
- 运行 `cargo fmt --check`（格式检查）
- 运行 `cargo clippy -- -D warnings`（零警告）
- 运行 `cargo test`（测试通过）
- 如果修改了模板（`templates/`）或 JS 文件（`static/`），运行 ESLint 检查 JS 语法：
  ```bash
  export PATH="/c/Users/Administrator/AppData/Local/ms-playwright-go/1.57.0:/c/Users/Administrator/AppData/Roaming/npm:$PATH"
  node node_modules/.bin/eslint static/i18n.js 2>&1 | grep -v "node_modules"
  ```
  无 `error` 级别报错
- 三项全过才算完成，任一失败必须修复后重跑
- 优化：fmt 失败先修格式再跑后续；clippy 失败不必跑 test，先修警告
- **端到端回退检测**：如果修改了模板文件（`templates/`）或静态资源（`static/`），特别是涉及截断、数据流、DOM 操作的改动，完成编译后必须手动跑一遍核心流程确认正常：
  1. 首页 → 上传 PDF → 检查文件内容完整（未被截断）
  2. 专利详情页 → 点「加载全文」→ 检查 5 个标签页内容均显示正常
  3. OA 答复页 → 粘贴 OA → 专利号/日期自动填入 → 点分析 → 讨论 → 生成答复书
  4. 技术调研 → 上传 PDF → 点开始 → 导出报告 → 检查 PDF 全文完整
  **关键原则**：改了什么，就要测什么。改了 PDF 提取就测 PDF 上传，改了截断就测数据完整性，改了 i18n 就测标签页文本。
- **Puppeteer 自动化测试**：如果修改了模板文件（`templates/`），且 Puppeteer 已安装（`node_modules/puppeteer` 存在），必须运行：
  ```bash
  export PATH="/c/Users/Administrator/AppData/Local/ms-playwright-go/1.57.0:/c/Users/Administrator/AppData/Roaming/npm:$PATH"
  cd D:\\test\\patent-hub-backup && node e2e_test.mjs
  ```
  确保全部测试通过（54/54 PASSED）。如果有失败项必须修复后才能提交。

- **HTML 模板函数完整性扫描（强制）**：每次修改 `templates/` 下任何 HTML 文件，编译前必须运行：
  ```bash
  node check_html_functions.mjs
  ```
  该扫描**自动发现 `templates/` 下所有 `*.html`（包括未来新增页面）**，不靠硬编码页面清单。它执行两条规则：
  1. 任何 `on*` 引用的函数未定义（按钮在、函数没了）→ 拦截
  2. 基线中存在的引用/定义在当前被移除（整块功能消失）→ 拦截（补齐静态扫描的原理盲区）
  **任何"按钮在、函数没了"或"整块功能消失"的情况（如历史事故 42c726e / 9f1a14b 两次重构误删函数）都会被拦截在编译前。**
  基线文件为 `docs/functions-manifest.json`（随代码提交）；新增页面/新增函数后，跑一次 `node check_html_functions.mjs --refresh` 更新基线，并把基线变更与模板变更一起提交（基线变更在 code review 中可审计）。
  退出码非 0 时必须先补齐函数定义再继续。

- **重构模板时的功能清单核对**：重写/重构任何页面模板前，先列出该页所有功能点（按钮 → 对应函数 → 对应 API），重构后逐项核对，禁止"删了再说"式重构。函数级删除必须先确认无引用。

### Step 6: 提交
- 提交信息格式：`feat/fix/refactor/chore/docs: 中文简要描述`
- 提交信息说明改了什么、为什么改
- 只提交相关文件，不提交临时产物

### Step 7: 记录
- 如果是新功能或重要修复，更新 `CHANGELOG.md`（遵循第三章格式）
- 如果属于某个版本计划任务，在 `docs/plans/` 对应文档中标注 ✅ + commit hash
- 向用户汇报：改了什么、影响范围、是否需要后续操作

---

### 特殊场景补充

**调研/分析类任务**（不涉及代码修改）：
- 执行 Step 0 → Step 1 → Step 2，然后输出分析结论
- 涉及深度分析时自动多轮迭代直到收敛，不需用户每次提醒

**紧急 bug 修复**：
- Step 3 可简化为一句话说明，不必等确认
- 但 Step 5 验证不可跳过

**禁止行为**（任何场景下都不可做）：
- 禁止大规模重构未经用户确认的代码
- 禁止引入新的 crate 依赖而不说明原因
- 禁止修改 DB schema 而不增加迁移版本
- 禁止在仓库根目录创建临时文件
- 禁止删除或覆盖用户未提交的工作
- 禁止跳过 Step 5 验证直接提交
- 禁止删除或篡改 `check_html_functions.mjs` 的扫描规则、或静默删除 `docs/functions-manifest.json` 基线。如需更改扫描规则或基线，必须在同一提交中说明原因并提交完整的 `--refresh` 更新结果，以便 code review 审查。
- 禁止在生产路径使用 `unwrap()` / `expect()`（见 2.7）

---

## 六、当前状态速查

- **版本**：见 `Cargo.toml` 的 `version` 字段（以此为准）
- **DB Schema**：见 `src/db/migrations.rs` 中最新迁移版本号
- **Pipeline 步骤**：15 步（见 `src/pipeline/state.rs` 的 `PipelineStep` 枚举）
- **AI 服务商**：支持多服务商（DeepSeek 为主，Gemini 为副，可在设置页切换）
- **搜索源**：2 个（SerpAPI / 本地 SQLite FTS5）
- **前端页面**：8 个（index/search/patent_detail/idea/ai/compare/settings/oa-response）

> 注意：版本号、Schema 版本等数字会随开发变化，请以源码文件为准，不要依赖本节的静态数字。
