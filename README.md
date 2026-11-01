# 创研台 InnoForge

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**仓库地址：** [GitHub](https://github.com/jsshwqz/innoforge

很多产品不是死在实现阶段，而是死在更早之前：想法看起来不错，却缺少判断依据；方向似乎可行，却没有清晰方案；产品刚开始推进，才发现已有技术、专利壁垒或知识产权保护缺口。

**创研台 InnoForge** 面向开发者和研发人员，帮助你把最初的想法逐步推进为决策依据、执行方案、产品落地和知识产权保护，减少盲目投入，提高从创意到成果的转化效率。

[English](#english)

---

### 📥 下载

| 平台 | 下载 |
|------|------|
| Windows (x86_64) | [📦 innoforge-windows-x86_64.zip](https://github.com/jsshwqz/innoforge/releases/latest) |
| Linux (x86_64) | [📦 innoforge-linux-x86_64.tar.gz](https://github.com/jsshwqz/innoforge/releases/latest) |
| Linux (aarch64) | [📦 innoforge-linux-aarch64.tar.gz](https://github.com/jsshwqz/innoforge/releases/latest) |
| macOS (Apple Silicon) | [📦 innoforge-macos-aarch64.tar.gz](https://github.com/jsshwqz/innoforge/releases/latest) |
| Docker | `docker build -t innoforge . && docker run -p 3000:3000 -v innoforge-data:/data innoforge` |

> 解压后直接运行主程序：Windows 双击 `innoforge-server.exe`，Linux / macOS 执行 `./innoforge-server`。程序会自动启动本地服务并打开浏览器 http://127.0.0.1:3000。无需安装数据库。
> 若本机 `3000` 端口被系统占用/保留，可设置环境变量 `INNOFORGE_PORT`（如 `3900`）后启动。

---

## 核心能力

### 1. 创新验证流水线（16 步全自动）

输入你的技术想法，系统按固定状态机依次完成下列 16 步（步骤定义见 `src/pipeline/state.rs` 的 `PipelineStep`，支持断点续跑与 SSE 进度推送）：

```
1 想法解析 ParseInput          2 关键词扩展 ExpandQuery
3 网络检索 SearchWeb            4 专利检索 SearchPatents
5 多样性把关 DiversityGate      6 相似度计算 ComputeSimilarity
7 排序筛选 RankAndFilter        8 现有技术聚类 PriorArtCluster
9 矛盾检测 DetectContradictions 10 新颖度评分 ScoreNovelty
11 AI 深度分析 AiDeepAnalysis   12 AI 行动方案 AiActionPlan
13 实验验证 ExperimentValidation 14 权利要求树 BuildClaimTree
15 综合报告 Finalize            16 OA 答复辅助 GenerateOaResponse
```

每一步的结果都可以查看和追溯，不是黑盒。

### 2. 多源专利检索

同时搜索多个数据源，自动级联回退，确保任何网络环境都能返回结果：

| 数据源 | 覆盖范围 | 国内直连 | 特点 |
|--------|---------|---------|------|
| SerpAPI | 全球专利 | 需 VPN | Google Patents 聚合 |
| Google Patents | 全球专利 | 需 VPN | 免费直查 |
| 本地数据库 | 已缓存数据 | 是 | 离线可用 |

另有 `src/search/providers/epo_ops.rs`（EPO OPS全文/著录补全），需要配置 EPO 凭据才启用；Firecrawl / Bing / 搜狗 / CNIPR 等历史源在当前版本已屏蔽，不出现在检索链里。

### 3. AI 深度分析

- **专利摘要** -- 一键生成结构化摘要（技术领域、解决问题、核心方案）
- **方案对比** -- 并排对比多个技术方案，支持上传 PDF/DOCX/图片
- **侵权风险评估** -- 逐条分析权利要求，给出风险等级
- **AI 问答** -- 针对具体专利的多轮技术讨论

### 4. 实用功能

- **收藏与标签** -- 管理和组织检索到的专利
- **导出** -- 搜索结果导出为 CSV / Excel
- **PDF 导出** -- 专利详情（含技术附图）导出为 PDF
- **中英双语** -- 完整的中文/英文界面切换
- **AI 自动容灾** -- 设置页内置 9 个服务商预设，主服务商调用失败自动改试下一个

---

## 快速开始

### 方式一：下载发布包（推荐）

1. 从 [GitHub Releases](https://github.com/jsshwqz/innoforge/releases) 下载对应平台的包
2. 解压，Windows 双击 `innoforge-server.exe`，Linux / macOS 执行 `./innoforge-server`
3. 浏览器自动打开 http://127.0.0.1:3000（未自动打开就手动访问）

### 方式二：源码编译

```bash
git clone https://github.com/jsshwqz/innoforge.git
cd innoforge
cp .env.example .env   # 编辑 .env 填入 API 密钥（可选）
cargo run --release --bin innoforge-server
```

需要 [Rust](https://rustup.rs/)；仓库根目录的 `rust-toolchain.toml` 已钉住工具链版本（当前 1.98.0），rustup 会自动安装。
Windows 用户也可以直接双击 `start.bat`（它需要先装好 Rust，会从源码增量编译再启动）。

### 方式三：Docker

```bash
docker build -t innoforge .
docker run -p 3000:3000 -v innoforge-data:/data innoforge
```

数据库与导出文件都落在容器内的 `/data`，用命名卷挂载即可跨容器重建保留。

### 方式四：移动端

本仓库以 `cdylib` / `staticlib` 形式导出 C ABI（`innoforge_start_server` / `innoforge_shutdown_server`），
供 Android / iOS / 鸿蒙的 WebView 壳内嵌本地服务。**壳工程与 APK / IPA 不在本仓库内**，因此本仓库不提供移动端安装包。

---

## 配置说明

### 终端契约（默认值）

| 配置项 | 默认值 | 说明 |
|--------|--------|------|
| 数据库 | `innoforge.db` | SQLite 数据库文件路径 |
| 监听地址 | `0.0.0.0:3000` | 主程序绑定所有网卡（`src/main.rs`），本机访问用 http://127.0.0.1:3000；可用 `INNOFORGE_PORT` 改端口。**没有访问鉴权，请勿把该端口直接暴露到公网**，需要远程访问请套一层反向代理或 SSH 隧道 |

所有设置均可通过**设置页面**（http://localhost:3000/settings）在线配置，也可编辑 `.env` 文件。

**没有 API 密钥也能用** -- 检索可直接使用本地数据库。无法获得 CNIPR 授权时，建议导入公开公告数据包作为本地主链路（见下方指南）。

### AI 服务商（任选一个）

设置页面内置以下 9 个服务商预设（Base URL 取自 `templates/settings.html`，任选其一填 Key 即可）：

| 服务商 | Base URL | 备注 |
|--------|----------|------|
| DeepSeek | `https://api.deepseek.com/v1` | 国内直连 |
| 智谱 GLM | `https://open.bigmodel.cn/api/paas/v4` | 国内直连 |
| 通义千问 Qwen | `https://dashscope.aliyuncs.com/compatible-mode/v1` | 国内直连 |
| 小米 MiMo | `https://api.xiaomimimo.com/v1` | 支持看图 |
| 商汤 SenseNova | `https://token.sensenova.cn/v1` | 支持看图 |
| OpenRouter | `https://openrouter.ai/api/v1` | 统一网关 |
| Google Gemini | `https://generativelanguage.googleapis.com/v1beta/openai` | OpenAI 兼容端点 |
| OpenAI | `https://api.openai.com/v1` | 原生端点 |
| Anthropic Claude | `https://api.anthropic.com/v1` | 经 OpenAI 兼容端点接入 |
| 自定义 | 任意 OpenAI 兼容地址 | 填 Base URL + Key 即可 |

计费与免费额度以各服务商官网为准；容灾链由设置页面配置，主服务商调用失败时自动依次改试后续服务商。

### 专利检索 API（可选）

| 变量 | 说明 |
|------|------|
| `SERPAPI_KEY` | [SerpAPI](https://serpapi.com/) 密钥，100次/月免费 |
| `CNIPR_USER` / `CNIPR_PASSWORD` | 兼容保留（当前默认在线链路不依赖该配置） |

---

## API 概览

| 接口 | 方法 | 说明 |
|------|------|------|
| `/api/search` | POST | 本地专利搜索 |
| `/api/search/online` | POST | 在线多源检索 |
| `/api/search/export/xlsx` | POST | 导出搜索结果 |
| `/api/ai/summarize` | POST | AI 专利摘要 |
| `/api/ai/compare` | POST | AI 方案对比 |
| `/api/ai/risk` | POST | 侵权风险评估 |
| `/api/ai/chat` | POST | AI 技术问答 |
| `/api/idea/submit` | POST | 提交想法 |
| `/api/idea/pipeline` | POST | 启动 16 步验证流水线 |
| `/api/patent/enrich/:id` | GET | 加载专利全文 + 附图 |
| `/api/patent/pdf/:id` | GET | 导出为 PDF |
| `/api/settings` | GET/POST | 配置管理 |

完整 API 文档：[docs/API.md](docs/API.md)

---

## 项目结构

```
innoforge/
  src/
    main.rs               # Web 服务器入口
    lib.rs                # 库导出 + Android JNI / iOS FFI
    patent.rs             # 数据模型
    ai/                   # AI 客户端（多服务商容灾）
      client.rs           # HTTP 客户端 + 故障转移
      chat.rs             # 对话接口
      patent.rs           # 专利分析（摘要/对比/风险）
      idea.rs             # 创意验证
    db/                   # SQLite 数据库
      patent.rs           # 专利 CRUD + FTS5 全文搜索
      settings.rs         # 配置持久化
      migrations.rs       # 数据库迁移
    routes/               # API 路由处理器
    pipeline/             # 16 步创新验证流水线
      steps/
        deep_reasoning.rs # AI 深度推演
    bin/
      mcp-server.rs       # MCP 协议服务器
  templates/              # HTML 页面模板
  static/                 # 静态资源（CSS/JS）
```

**关联仓库：** 桌面端（Tauri）、iOS 原生壳、鸿蒙原生壳均为**规划中的独立仓库，目前尚未创建**。本仓库的 `src/lib.rs` 已导出 C ABI（`innoforge_start_server` / `innoforge_shutdown_server`）以供这些壳工程内嵌，欢迎按需自行封装。

---

## 技术栈

- **后端**：Rust + Axum + SQLite（内嵌，零配置）
- **前端**：原生 HTML/CSS/JS（无需构建工具）
- **AI**：兼容任意 OpenAI API + 9 个内置预设自动容灾
- **搜索**：CNIPR + SerpAPI + Google Patents + 本地库（级联回退）
- **移动端**：Rust cdylib / staticlib 导出 C ABI，供任意 WebView 壳内嵌（壳工程不在本仓库）
- **国际化**：中英双语

### 已知限制

- 页面图表通过 jsDelivr CDN 加载 `chart.js@4`，**完全离线环境下图表不渲染**（其余功能不受影响）。
- 服务绑定 `0.0.0.0` 且无访问鉴权，请勿直接暴露公网。
- 在线专利检索依赖 SerpAPI / Google Patents 可达性；不可用时自动回退本地库，结果为空说明本地库尚未导入数据。

---

## 致谢

### 架构灵感

- [Harness Research](https://github.com/Nimo1987/harness-research) -- 验证流水线受其启发：状态机驱动的步骤链、LLM 与代码的职责分离、多层搜索降级。本项目所有实现均为原创。

### 使用的算法

- [TF-IDF](https://en.wikipedia.org/wiki/Tf%E2%80%93idf) -- 文本相似度匹配
- [Jaccard 相似系数](https://en.wikipedia.org/wiki/Jaccard_index) -- 重叠检测与矛盾分析
- [余弦相似度](https://en.wikipedia.org/wiki/Cosine_similarity) -- 向量空间相似度

### 第三方服务

- [Chart.js](https://www.chartjs.org/) v4 (MIT) -- 图表可视化
- [SerpAPI](https://serpapi.com/) / [CNIPR](https://open.cnipr.com/) -- 专利数据源

---

## 许可证

[MIT](LICENSE)

---

## 贡献

1. Fork 本仓库
2. 创建功能分支 (`git checkout -b feature/amazing`)
3. 提交更改
4. 发起 Pull Request

---

## 获取更新通知

- **GitHub Watch** -- 仓库右上角点击 **Watch → Custom → Releases**
- **RSS** -- `https://github.com/jsshwqz/innoforge/releases.atom`

---

<a name="english"></a>
## English

Most products don't fail at the implementation stage -- they fail much earlier: an idea looks promising but lacks supporting evidence; a direction seems viable but has no clear path; development kicks off only to hit existing patents, technical barriers, or IP protection gaps.

**InnoForge** helps developers and R&D teams turn initial ideas into informed decisions, actionable plans, shipped products, and protected intellectual property -- reducing blind investment and improving the conversion rate from concept to outcome.

### Core Features

- **16-Step Validation Pipeline** -- A fixed state machine from idea to feasibility report (see `PipelineStep` in `src/pipeline/state.rs`): parsing, keyword expansion, multi-source search, similarity, prior-art clustering, contradiction detection, novelty scoring, AI deep analysis, action plan, experiment validation, claim tree, final report, OA response support
- **Multi-Source Patent Search** -- CNIPR (China), SerpAPI (Google Patents), Google Patents Direct, Local DB -- cascading fallback ensures results in any network environment
- **AI Deep Analysis** -- Patent summarization, side-by-side comparison, infringement risk assessment, multi-round Q&A
- **Practical Tools** -- Collections & tags, CSV/Excel/PDF export, bilingual UI (Chinese/English)
- **AI Failover** -- 9 built-in provider presets with automatic failover (DeepSeek, Zhipu GLM, Qwen, Xiaomi MiMo, SenseNova, OpenRouter, Gemini, OpenAI, Anthropic)
- **Zero Config** -- Embedded SQLite + FTS5, no database setup needed

### Quick Start

```bash
# Download the archive for your platform from GitHub Releases:
#   innoforge-windows-x86_64.zip / innoforge-linux-x86_64.tar.gz /
#   innoforge-linux-aarch64.tar.gz / innoforge-macos-aarch64.tar.gz
# Extract and run innoforge-server.exe (Windows) or ./innoforge-server (Linux/macOS).
# The browser opens http://127.0.0.1:3000 automatically.

# Or build from source:
git clone https://github.com/jsshwqz/innoforge.git
cd innoforge && cp .env.example .env
cargo run --release --bin innoforge-server
```

### Configuration

All settings configurable via **Settings page** (http://localhost:3000/settings) or `.env` file.

#### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `INNOFORGE_PORT` | `3000` | HTTP listen port. When changed, OAuth callback URL follows automatically. |
| `INNOFORGE_OAUTH_CALLBACK_URL` | *(derived from `INNOFORGE_PORT`)* | Google OAuth redirect URI. Overrides `INNOFORGE_PORT`-derived URL when set. Must match the authorized redirect URI in Google Cloud Console. |
| `INNOFORGE_UPSTREAM_TIMEOUT_SECS` | `30` | Timeout in seconds for upstream HTTP calls (patent APIs, Google OAuth, AI model probes). |

**No API keys?** App still works -- search uses local database, AI features show setup guide.

The Settings page ships with 9 provider presets (Base URLs taken from `templates/settings.html`) plus a custom OpenAI-compatible entry:

| Provider | Base URL |
|----------|----------|
| DeepSeek | `https://api.deepseek.com/v1` |
| Zhipu GLM | `https://open.bigmodel.cn/api/paas/v4` |
| Qwen (DashScope) | `https://dashscope.aliyuncs.com/compatible-mode/v1` |
| Xiaomi MiMo | `https://api.xiaomimimo.com/v1` |
| SenseTime SenseNova | `https://token.sensenova.cn/v1` |
| OpenRouter | `https://openrouter.ai/api/v1` |
| Google Gemini | `https://generativelanguage.googleapis.com/v1beta/openai` |
| OpenAI | `https://api.openai.com/v1` |
| Anthropic Claude | `https://api.anthropic.com/v1` |

Pricing and free tiers are set by each provider; configure the failover chain on the Settings page and the app moves to the next provider automatically when one fails.

### Tech Stack

- **Backend**: Rust + Axum + SQLite (embedded, zero-config)
- **Frontend**: Vanilla HTML/CSS/JS (no build tools)
- **AI**: Any OpenAI-compatible API + 9 built-in provider presets with failover
- **Search**: CNIPR + SerpAPI + Google Patents + Local DB (cascade fallback)
- **Mobile**: Rust cdylib/staticlib exporting a C ABI for embedding in any WebView shell (shell projects live elsewhere)
- **i18n**: Chinese/English bilingual

### License

[MIT](LICENSE)
