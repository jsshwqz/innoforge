# InnoForge 重构风险评估 / Risk Assessment

> 分析日期：2026-08-15（基于 HEAD `69b6a85`）
> 分析目的：完整重构前摸清危险区与必须先修的地基
> 评估框架：S.U.P.E.R（Single Purpose / Unidirectional Flow / Ports over Implementation / Environment-Agnostic / Replaceable Parts）
> 全项目加权健康分：**13.6/25** —— 重构必要性充分

---

## 1. 重构安全网现状（先说好消息）

重构最大的风险是"改坏"，本项目的防线异常扎实，这是敢于完整重构的前提：

| 防线 | 内容 | 状态 |
|------|------|------|
| Rust 测试 | 159 个单元+集成测试（cargo test） | ✅ 全绿 |
| E2E | Puppeteer 真实浏览器 54 项（e2e_test.mjs） | ✅ 全过 |
| 前端函数完整性 | check_html_functions.mjs 基线化扫描（防"按钮在函数没了"） | ✅ 已固化 |
| 静态检查 | cargo fmt --check + clippy -D warnings + ESLint | ✅ |
| CI | GitHub Actions + Gitee Actions 双镜像 test/lint/e2e 三关 | ✅ 绿 |
| Git 钩子 | .githooks/ pre-commit（core.hooksPath 共享） | ✅ |
| 复盘文化 | docs/errors.md 错误数据库 + STATUS.md 进度追踪 | ✅ |

**缺口**：无覆盖率度量；测试分布严重失衡——巨型文件几乎裸奔（idea.rs 2194 行仅 2 测、ai/client.rs 1069 行仅 1 测），最需要回归保护的编排/容灾/提取逻辑恰好零覆盖。

## 2. 风险清单（按 出事概率 × 影响 排序）

### 🔴 R1 HTTP 层被当作万能层（根因级问题，S/U/P 全面违反）
- routes/ 占 11,960 行 = 全部 Rust 代码的 46%
- idea.rs 内含 400 行单 handler（聊天+压缩+摘要+持久化）、内嵌 Markdown 渲染器、相似度算法、直连 SQL；ai.rs 内私建两套搜索爬取（serpapi/sogou）；patent.rs 内嵌 EPO/USPTO/GP 三套外部客户端
- **缓解**：先立端口（trait）再拆文件，避免边拆边造新轮子

### 🔴 R2 前端巨石文件是历史事故高发区
- office_action_response.html **152.9KB / 90 函数声明 / 68 处 innerHTML / 71 个 on\* 处理器**；idea.html 111KB/68 处 innerHTML/33 fetch
- 历史上两次重构误删功能事故（42c726e / 9f1a14b）均发生在此区域
- escapeHtml 在 6 页重复定义 7 次；导航渲染器藏在 i18n.js 里形成隐性加载耦合
- **缓解**："只搬不写"+每搬一批跑 e2e+函数基线扫描护航

### 🔴 R3 重复实现税：三套 SerpAPI 客户端 + 三套 TF-IDF + 双 SearchResult
- SearchResult 同名双定义（patent.rs:218 vs pipeline/context.rs:114）直接违反规约 §2.2
- 同一算法三份代码意味着改一处漏两处
- **缓解**：统一 SearchProvider/Embedder 端口后删除私有重写版

### 🟠 R4 反向依赖三处（U 违反热点）
- common.rs(基础设施) → routes::AppState（反向）
- db 层 → pipeline::context 业务类型（AiCostRecord/Evidence/ResearchState）
- AppConfig 定义在 routes/mod.rs（配置混入路由层）
- **缓解**：类型上提独立 types/config 模块，一次性解开

### 🟠 R5 死代码群 ~1,200 行悬而未决
- rag/(362行) + vector/(175行) 全仓零引用；fact_check.rs(700行) 设计最佳（24/25 分）却未接线
- 拖着的最差状态：维护者不知道该不该改它们
- **缓解**：Phase 决断——接线或摘除（需用户选择）

### 🟠 R6 AI 服务商知识三处硬编码（P 违反）
- AppConfig 字段族 + settings.rs provider_db_key 映射 + idea.rs:2196 注册表副本
- 新增服务商要改三处；AppConfig 已有 10 个 per-provider Key 字段
- **缓解**：表驱动 ProviderConfig 单一事实源

### 🟠 R7 环境硬编码散布（E 违反）
- cad.rs:120,153 写死开发机路径 `D:\test\aionui\aioncad\_*.log`（已复核）
- upload.rs Umi-OCR 写死 127.0.0.1:1224；mcp-server BASE_URL 写死 localhost:3000
- context.rs 相对路径依赖进程 CWD；PDF 六路提取隐式依赖本机装有 pdftotext/python/PyMuPDF
- **缓解**：集中 PathConfig/EndpointConfig；外部工具启动自检

### 🟡 R8 生产路径残留 ~25 处 unwrap/expect（粗扫）
- 与规约 §2.7 不符；集中在锁恢复以外的真实 panic 点需逐个甄别
- **缓解**：地基阶段逐个替换为 ? 传播

### 🟡 R9 Schema 版本漂移（已复核为命名漂移而非功能 bug）
- db/mod.rs SCHEMA_VERSION=22 vs 实际迁移至 v23；migrations::run 的 target_version 参数只用于日志不参与门控
- 当前无功能影响（逐级 current<N 门控），但属定时炸弹级不一致；AGENTS.md 还写着"当前 v11"
- **修复成本极低**：对齐常量并让 target 参数参与断言

### 🟡 R10 未提交工作与工作区污染
- src/cad.rs(+46/-21) 与 src/common.rs(+6/-2) 有未提交改动——**开工前必须由用户确认处置**
- 根目录 ~250 个本地临时文件未被 git 跟踪但干扰扫描检索
- **缓解**：Phase 0 处理；清理仅删可再生日志，db/zip 类先备份到仓库外

## 3. S.U.P.E.R 架构健康度总结

| 原则 | 总评(1-5) | 主要违反热点 |
|------|-----------|--------------|
| S 单一目的 | ★★☆☆☆ 2.5 | routes/ 万能层（46% 代码量）；idea.rs 七职合一；i18n.js 藏导航渲染器 |
| U 单向流 | ★★☆☆☆ 2.5 | common→routes 反向；db→pipeline 反向；routes 直连 rusqlite 绕过 db 层 |
| P 端口优先 | ★★☆☆☆ 2 | SearchProvider/Embedder/AiProvider/Repository 四大端口全缺失；AppError 设计好却闲置 |
| E 环境无关 | ★★★☆☆ 3 | cad/upload/mcp 硬编码地址路径；CORS/临时目录治理良好部分抵消 |
| R 可替换 | ★★★☆☆ 3 | pipeline steps 天然可插拔 ✅；Database 100 pub fn 单结构体难替换 |

**健康样本**（证明团队有能力写好架构）：docx_export 21/25、error.rs 21/25、fact_check 24/25、orchestrator 18/25、pipeline/steps 17/25。病灶高度集中而非弥漫——**适合原地分层重组，不适合推倒重写**。

## 4. 明确不做的事（防止过度工程）

- 不换 Web 框架（axum 0.6→0.7/0.8 升级单独评估，不与重构捆绑）
- 不引入前端构建工具（AGENTS.md 红线；用原生 `<script type="module">` 即可共享 JS）
- 不动 DB schema 兼容性（v23 迁移链是用户资产；仅修常量漂移）
- experiment/WASM 子系统维持 WIP 现状，不在本轮激活

---

## 5. 【2026-08-15 晚补充】实测门禁与环境发现（均已人工复核）

### 5.1 流程防线被绕空的实证
- **本地 main 领先 gitee/main 36 提交、领先 origin/main 60 提交**——所谓"CI 绿"只覆盖旧提交，最新工作从未过远端关卡
- 实测三红：fmt 漂移、**clippy 32 错误**（manual_clamp/collapsible_if/unused_import 等，集中于 vector/mod.rs 等休眠代码）、cargo test 链接失败
- **templates/patent_detail.html 带着未解决的合并冲突标记（<<<<<<< HEAD）被提交入库**——专利详情页聊天 JS 实际处于语法损坏状态
- **tests/cad_template_contract 3 项中 2 项红**：idea.html 在后续提交中丢失 FreeCAD 接线（shouldHandle/renderHistory）
- 三者同根因：提交节奏失控 + 本地钩子只查 HTML 函数不查 Rust 门禁

### 5.2 环境
- **D 盘一度仅剩 0.7GB，target/ 占 8.4GB**——已 cargo clean 释放至 7.5GB。执行 agent 开工前必查磁盘水位，<5GB 先 clean
- 根目录 innoforge.db 实测 **4.1GB**（活体库，gitignored）+ docs.zip 118MB

### 5.3 新确认的功能缺陷（重构中修复）
| 缺陷 | 位置 | 影响 |
|------|------|------|
| **TF-IDF 字节切片 CJK panic** | vector/mod.rs tokenize 与 routes/search.rs compute_char_tfidf_embedding 同源缺陷：`cleaned[i..i+n]` 遇多字节字符非边界 panic | /api/search/vector 收到中文查询即崩（逻辑级验证） |
| 向量层空转 | save_patent_embedding 仅 vector/mod.rs 自调用（零上游） | patents_embedding 恒为空，混合检索的向量支路从未生效 |
| Docker 出口损坏 | Dockerfile `--bin innoforge` ≠ Cargo bin 名 innoforge-server | docker build 必然失败 |
| MCP 半成品 | tools/list 声明 6 工具，分发仅实现 patent_search；patent_compare 指向不存在的 /api/ai/analyze-results | 对外承诺与能力不符 |
| 死配置 | FALLBACK_AI_{1-5}_{*} env 组零消费 | 误导配置者 |
| FTS5 手动同步 | save_patent 内 DELETE+重灌，无触发器 | 绕过该入口写 patents 即索引失联 |
| .env 明文密钥 | DeepSeek/Gemini/SenseNova/Zhipu/Google Secret | gitignored 但建议轮换（T0.9 用户动作） |

### 5.4 已当场修复（等待提交）
1. patent_detail.html 冲突解决：保留 CAD 拦截块，舍弃已被消息队列机制取代的旧"中止当前请求"块
2. idea.html CAD 接线恢复：sendChatMessage 增加 shouldHandle 拦截 + loadMessages 重绘后调用 renderHistory → cad_template_contract 预期转绿
3. src/cad.rs 剥离硬编码调试日志（D:\test\aionui 路径），保留端口 8080 默认值、无头预览放宽、db 路径 canonicalize 三处真修复
4. cargo fmt 应用；cargo clean 执行
