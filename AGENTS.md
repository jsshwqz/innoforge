# InnoForge 项目规约

> 本文件是所有 AI Agent 和人类贡献者的强制规范。修改代码前必须阅读并遵守。
> **优先级唯一权威 = GitHub issue #90**：排队与优先级以 #90 为准；本文件只讲规矩，不承载进度。现状见 `docs/plans/STATUS.md`。

## 记忆
按 Step 0b 检索、Step 0c 回写：用户反馈→`docs/feedback.md`；踩坑复盘→`docs/errors.md`；阶段事实→`docs/plans/STATUS.md`；长期偏好→本文件。冲突时以最新可验证来源（代码/文档/运行结果）为准并更新记忆。后端 `src/context.rs` 每轮自动把这些文件汇编为「项目记忆上下文」（≤6000 字符）注入系统提示词，缺失静默降级。

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
- 所有 API 端点统一在 `src/common.rs` 的 `build_router` 函数中注册；`src/main.rs`（桌面端 / Docker 独立运行入口）与 `src/lib.rs`（Android / iOS FFI 共享库入口）都通过调用同一个 `build_router` 获得路由，禁止在两处各自重复注册导致漂移
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
- **禁止裸字节切片 `&s[..n]` / `&s[n..]`**（#73 防线）：对 `&str` 按字节索引切片时，
  若 `n` 未经过 `is_char_boundary(n)` 回退或未由 `chars()` 间接保证落在字符边界，
  非_ascii 输入会在多字节字符中间切断，触发 `panic`。正确做法：
  - 截断文本 → 用 `crate::ai::client::safe_truncate`（字节）或 `safe_truncate_chars`（字符）；
  - 取前/后 N 个字符 → `s.chars().take(N).collect()` / `s.chars().skip(count-N).collect()`；
  - 确需按字节切 → 先 `while !s.is_char_boundary(end) && end > 0 { end -= 1; }` 回退到边界。
  - CI 侧有 `tests/no_unsafe_byte_slice.rs` 静态扫描守这条线，新增违例会红。

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
- 桌面端（Tauri）/ iOS / 鸿蒙 → 各自独立仓库，**目前尚未创建**（规划中）；本仓库仅在 `src/lib.rs` 导出 C ABI 供壳工程内嵌
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
- 固定先读 `AGENTS.md`，再读 `docs/plans/STATUS.md`（必读）；其余记忆**只展开与本任务直接相关的部分**（`docs/errors.md`/`CHANGELOG.md`/计划文档只看最近条目或标题，不整份塞入）。
- `docs/errors.md` 仅当本任务用到对应工具或该工具刚失败时展开；`docs/feedback.md` 只展开最相关的几条。
- **证据足够支撑判断即停**，不为"全面感"继续铺开或重复读；记忆与最新代码/文档/运行结果冲突时，以可验证来源为准并更新记忆。

#### Step 0c: 记忆存储规则（最终回复前静默执行）
- 每轮最终回复前必须静默判断是否需要维护记忆。命中以下任一条件则先写入、再回复，命中后无需向用户解释。
- **用户反馈**：当用户用「你这里不应该 / 这样不对 / 不是 X 而是 Y / 以后遇到这种情况 / 以后都 / 我更喜欢 / 默认按」等话术给出纠正、认可或边界澄清时，写入 `docs/feedback.md`（含：规则 / 原因 / 适用场景 / 时间）；**既记纠正也记肯定**，避免 agent 越来越保守。
- **工具踩坑**：新增的构建/CI/依赖/工具失败解法，写入 `docs/errors.md`（沿用其条目格式）。
- **不应沉淀**：一次性流水账、可从代码直接读取的信息、临时任务摘要；同主题可承载时优先更新旧条目，不重复新建。

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

> 版本/Schema/Pipeline/前端页面等**会变的数字不在此硬编码**。唯一权威：优先级=GitHub issue **#90**；版本=`Cargo.toml`；Schema=`src/db/migrations.rs`；现状=`docs/plans/STATUS.md`。本仓禁止 `git add -A` / `git add .`。

## 部署铁律 / Deployment Rules

1. **改模板必须 `cargo build --release` 且换进程**，否则用户永远看到旧版（模板通过 `include_str!` 编入二进制）。
2. **认版只认 `/api/version` 的 `git_commit` ＋ 模板 `sha256`**，不信版本号（版本号与 Cargo.toml 一模一样时分不出新旧二进制）。
3. **禁止 `git add -A` / `git add .`**，只允许显式列文件路径（防止误推 data/ 下用户数据到公网）。
4. **部署完必须跑一遍页内按钮接线检查**（`typeof window[每个 on* 函数名] === 'function'`），因为旧函数把后面声明吞进局部作用域的 bug，cargo check / 构建 / 静态扫描全部发现不了。
5. **改 `templates/*.html` 里的 JS 禁用 `sed`**，用 `python3` 或编辑器改（sed 对引号/转义/多行处理不可靠）。
6. **推送纪律**：只允许推 `jsshwqz/innoforge`（GitHub，`https://github.com/jsshwqz/innoforge`）的 `main` / `dev`。gitee、gitcode 一律禁止推送、禁止添加或使用这两个 remote、禁止重建镜像。
   - **禁"新分支"比禁"禁推的远端"更要紧**：本仓是从 `innoforge-legacy` 全新重建的孤立历史（331 文件、无旧提交）。把 legacy（或任何带旧历史的克隆）的历史推到这里，等于把案件专有信息重新放回公开面——`push main` 会因 non-fast-forward 被拒，但 `push dev`、`push feature/*`、`push` 任何 legacy 分支都会被当作**新分支接受**。
   - 因此：任何机器上的克隆在推送前必须先确认 `git log --oneline origin/main -1` 与本地历史的祖先后关系成立（`git merge-base --is-ancestor`）；不允许把旧仓库的 remote URL 原样指过来（改名后用同名新建仓库会使 `github.com/jsshwqz/innoforge` 的旧重定向**失效**，该 URL 现在就指向本公开仓）。云端 `/root/innoforge` 这类旧克隆应先 `git remote set-url --push origin no-push` 断写，再把 fetch URL 改指 `innoforge-legacy`。
   - 禁止 force-push，禁止以 rebase/squash 名义重写已推送历史。
   - 原因（事实依据）：gitee 镜像内容级实测 `main@fc102f0` 仍含申请人姓名与案号，任何再次推送都会把脏历史放回公开面。

## 案件数据一票否决 / Case Data Veto

**扫描范围**：全仓 git 跟踪文件（src、templates、docs、tests、*.md、Cargo.toml、build.rs 全含），不限于 src/** 与 templates/**。

**禁词表**（以下字面量禁止出现在任何 git 跟踪文件中（含 tests/fixtures/）；本表所有条目一律用分隔符书写，禁止逐字出现完整禁词）：
- 申请人姓名：王·青·芝（三字中文名）
- **姓名拼音形态**：w·a·n·g·q·i·n·g·z·h·i（拼音全拼，守卫用字符数组逐字符拼装构造，禁词表本身不含完整拼音字面量）
- 申请号：2022-1083-5143（12 位）、2022-1083-5143-9（13 位）、2022-1083-5143.9
- 公开号：CN-11507-6688（8 位）、CN-11507-6688-A
- 申请号：2022-1047-7324（12 位）、2022-1047-7324-9（13 位）、2022-1047-7324.9
- 公开号：CN-11496-2757（8 位）、CN-11496-2757-U
- **对比文件公开号**：CN-10313-3144-A（D1）、CN-11310-7684-A（D2）、CN-21095-1320-U（D3）、CN-21832-8141-U（D4）
- **发文序号**：2026-0710-0014-1920（驳回决定 16 位）、2026-0301-0002-8380（一通 16 位）
- 暴露法律处境的措辞：急需·高质量·答复

**机器守卫**：`tests/no_case_data_in_repo.rs` 在 `cargo test` 时自动遍历 `git ls-files` 输出的每个文件，执行四类检查：
1. 申请人姓名（汉字 + 拼音两种形态）
2. 对比文件公开号类目
3. 发文序号类目（16 位）
4. 申请号/公开号形状正则（`\d{13}(\.\d)?`、`CN\d{8}[A-Z]`、`CN\d{9}U`，仅 src/ 生代码区域）

**路径级扫描**：不只扫文件内容，还对 `git ls-files` 的每条路径本身执行同样四类匹配。本轮泄露正是路径级的带姓名 PDF 文件名（如 `d1_cn990001001a.pdf`），仅扫内容会漏掉路径中的案号和姓名。

**守卫绿不等于仓库干净，判据必须覆盖路径与数字全枚举。**

tests/fixtures/ 内容也执行四类检查（fixture 按定义是合成内容，更应被扫描）。白名单常量必须保持为空。违规消息不回显命中串。

**违反后果**：任何 AI 或开发者在提交中引入上述禁词（无论在内容还是路径中），`cargo test` 将失败，CI 阻断合并。
