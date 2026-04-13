# InnoForge 项目规约

> 本文件是所有 AI Agent 和人类贡献者的强制规范。修改代码前必须阅读并遵守。

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
- Node.js 相关文件（package.json 等）不应出现在本仓库

---

## 五、标准操作流程（所有 AI Agent 必须遵守）

任何 AI Agent 接到任务后，按以下步骤顺序执行。不可跳步。

### Step 0: 读规约
- 读取本文件（CLAUDE.md），理解项目定位、规范和禁忌
- 如果是首次进入项目，同时读 `CHANGELOG.md` 和 `Cargo.toml` 了解当前版本

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
- 三项全过才算完成，任一失败必须修复后重跑
- 优化：fmt 失败先修格式再跑后续；clippy 失败不必跑 test，先修警告

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
- 禁止在生产路径使用 `unwrap()` / `expect()`（见 2.7）

---

## 六、当前状态速查

- **版本**：见 `Cargo.toml` 的 `version` 字段（以此为准）
- **DB Schema**：见 `src/db/migrations.rs` 中最新迁移版本号
- **Pipeline 步骤**：13 步（见 `src/pipeline/state.rs` 的 `PipelineStep` 枚举）
- **AI 服务商**：7 个（智谱/DeepSeek/OpenRouter/Gemini/OpenAI/NVIDIA/Ollama）
- **搜索源**：5 个（CNIPR/SerpAPI/Lens.org/Google Patents/搜狗）
- **前端页面**：7 个（index/search/patent_detail/idea/ai/compare/settings）

> 注意：版本号、Schema 版本等数字会随开发变化，请以源码文件为准，不要依赖本节的静态数字。
