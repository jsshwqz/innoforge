# 架构设计 / Architecture

> 最后更新：2026-07-08（v0.7.3 同步）  
> Last updated: 2026-07-08 (synced with v0.7.3)

## 概述 / Overview

InnoForge 采用经典的三层架构设计，注重跨平台兼容性和可扩展性。
Rust 后端 + 纯 HTML/CSS/JS 前端（服务端渲染，无构建工具）+ SQLite 本地数据库。

```
┌─────────────────────────────────────────────────────────┐
│         Frontend (HTML/CSS/JS, 服务端渲染)              │
│  templates/ (8 页面) + static/ (i18n, PWA, CSS)         │
└─────────────────────────┬───────────────────────────────┘
                          │ HTTP/JSON + SSE
┌─────────────────────────▼───────────────────────────────┐
│      Backend (Rust + Axum 0.6)                         │
│  ┌──────────────────────────────────────────────┐     │
│  │  routes/ — 13 个 API 路由模块（~130+ 端点）    │     │
│  │  ai · auth · chat · collections · feature_cards│     │
│  │  idea · ipc · pages · patent · search · settings│     │
│  │  upload                                       │     │
│  └──────────────────┬───────────────────────────┘     │
│                     │                                  │
│  ┌──────────────────▼───────────────────────────┐     │
│  │  pipeline/ — 16 步创新验证流水线               │     │
│  │  orchestrator/ — 状态机编排引擎               │     │
│  │  ai/ — 多 provider 容灾 AI 客户端             │     │
│  │  experiment/ — AI 生成脚本 + 沙箱执行         │     │
│  └──────┬───────────────────┬────────────────────┘     │
│         │                   │                          │
│  ┌──────▼──────┐   ┌───────▼────────────┐          │
│  │  db/        │   │  External APIs      │          │
│  │  SQLite     │   │  - AI (OpenAI 兼容) │          │
│  │  21 张表    │   │  - SerpAPI          │          │
│  │  schema v15 │   │  - MinerU (OCR)     │          │
│  │  FTS5+BM25  │   │  - Google OAuth     │          │
│  └─────────────┘   └────────────────────┘          │
└─────────────────────────────────────────────────────────┘
```

## 双入口架构 / Dual-Entry Architecture

```
main.rs (桌面端)  ──┐
                    ├── common.rs (共享初始化 + 路由注册 + 静态资源嵌入)
lib.rs  (移动端 FFI)──┘
```

- `main.rs`：Axum 服务器入口，支持端口回退（3000 → 3921 → 3100），自动打开浏览器
- `lib.rs`：移动端 FFI 入口（Android/iOS/HarmonyOS），通过 `#[no_mangle] extern "C"` 导出
- `common.rs`：**所有路由注册集中在一处**（`build_router`），消除双入口同步风险

## 核心模块 / Core Modules

### 1. pipeline/ — 16 步创新验证流水线

核心差异化能力。用户输入创意 → 自动走完 16 步生成新颖性报告。

```
PipelineStep 枚举（16 步固定序列）:

 0  ParseInput           CODE  解析输入，提取关键词，推断技术领域
 1  ExpandQuery          LLM   AI 扩展 6-8 条中英搜索词
 2  SearchWeb            CODE  SerpAPI 网络搜索（24h 缓存）
 3  SearchPatents        CODE  本地 FTS5 + SerpAPI Google Patents
 4  DiversityGate        CODE  5 维多样性检查（不足自动回退重搜）
 5  ComputeSimilarity    CODE  TF-IDF 余弦 + Jaccard 混合相似度 (0.6:0.4)
 6  RankAndFilter        CODE  排序去重取 Top-15
 7  PriorArtCluster      CODE  贪心聚类（Jaccard ≥ 0.25）
 8  DetectContradictions CODE  检测技术路线分歧（信号 = 创新空间）
 9  ScoreNovelty          CODE  算法化新颖性评分（非 AI 猜测）
10  AiDeepAnalysis       LLM   7 轮多维深度推演引擎（6 维度 + 综合）
11  AiActionPlan         LLM   AI 生成行动方案 + 特征卡片提取
12  ExperimentValidation LLM+CODE  AI 生成 Python 脚本 → 沙箱执行
13  BuildClaimTree       LLM   AI 起草权利要求树
14  Finalize              CODE  汇总报告 + 证据链持久化
15  GenerateOaResponse    LLM   OA 答复辅助分析
```

**关键设计**：代码先算，AI 后判。AI 接收结构化算法结果，非自己猜测。

子模块结构：
- `context.rs` — PipelineContext（步骤间数据载体）、证据链、研发状态机
- `state.rs` — PipelineStep 枚举（步骤类型、前后关系、快速模式跳过逻辑）
- `runner.rs` — PipelineRunner（Orchestrator 的薄包装）
- `steps/` — 15 个步骤实现模块 + `text_util.rs`（共享 Jaccard 相似度工具）

### 2. orchestrator/ — 状态机编排引擎

替代线性 runner，支持：
- **5 种命令**：Continue / Jump / Retry / Branch / Abort
- **失败回滚**：步骤执行前克隆 context，失败时回滚
- **关键步骤**（ParseInput/SearchWeb/ComputeSimilarity/ScoreNovelty）失败终止
- **DiversityGate 回退**：多样性 < 0.3 自动回退 SearchWeb，硬上限 2 轮
- **断点续跑**：每步成功后保存快照到 `pipeline_snapshots` 表

### 3. routes/ — API 路由层（13 个子模块）

| 模块 | 职责 | 端点数 |
|---|---|---|
| `ai.rs` | AI 聊天/摘要/对比/OA 答复/威胁评估/权利要求图 | ~25 |
| `idea.rs` | 创意验证/流水线/版本/分支/讨论/报告 | ~30 |
| `search.rs` | 本地+在线专利搜索/统计/CSV/XLSX 导出 | ~6 |
| `patent.rs` | 专利抓取/富化/PDF/法律状态/相似推荐 | ~12 |
| `auth.rs` | Google OAuth/gcloud CLI/ADC 认证；Token 状态先 SQLite 事务持久化再更新运行时内存 | ~6 |
| `chat.rs` | 跨设备聊天同步 | ~3 |
| `collections.rs` | 收藏夹+标签 CRUD | ~10 |
| `feature_cards.rs` | 特征卡片 CRUD + LCS diff | ~3 |
| `ipc.rs` | IPC 分类树浏览 | ~2 |
| `upload.rs` | 文件上传/PDF 提取/图片 OCR | ~4 |
| `settings.rs` | 系统设置 CRUD | ~5 |
| `pages.rs` | HTML 页面渲染（服务端模板插值） | ~8 |

### 4. db/ — 数据层（13 个子模块）

`Database` 结构（`Mutex<Connection>`），schema 版本 16，WAL 模式。

| 子模块 | 职责 |
|---|---|
| `mod.rs` | Database 结构、init()、schema_version 追踪 |
| `migrations.rs` | 版本化迁移引擎（v1→v16，22 张表，幂等） |
| `version.rs` | idea_versions / idea_branches / findings CRUD |
| `patent.rs` | 专利 CRUD、FTS5+BM25 搜索、智能类型检测 |
| `relevance.rs` | 相关性评分（字段匹配、中文 bigram、姓名检测） |
| `idea.rs` | 创意 CRUD、消息、feature_cards、pipeline_snapshots |
| `oa.rs` | OA 分析持久化（带版本号） |
| `chat.rs` | 跨设备聊天记录 |
| `collection.rs` | 收藏夹 + 标签 |
| `evidence.rs` | 证据链批量操作 |
| `research_state.rs` | 研发状态机快照 |
| `settings.rs` | 键值设置 + 搜索缓存（24h TTL）+ 多项设置原子事务保存（`set_settings_batch()`） |
| `tests.rs` | DB 层单元测试 |

**22 张表**：`patents` + `patents_fts`(FTS5) / `ideas` / `idea_messages` / `idea_research_state` / `feature_cards` / `pipeline_snapshots` / `evidence_chain` / `idea_versions` / `idea_branches` / `findings` / `claim_nodes` / `technical_features` / `collections` / `patent_collections` / `patent_tags` / `oa_analyses` / `oa_discussions` / `chat_records` / `app_settings` / `search_cache` / `schema_version`

### 5. ai/ — 多 AI 服务商容灾客户端

- 支持 6+ provider 自动 failover（DeepSeek / Gemini / Zhipu / OpenAI / OpenRouter / NVIDIA / SenseNova / 小米）
- 区分普通模型和专家模型
- SSE 流式输出、多模态图片、历史压缩
- 全局超时分级：聊天 60s / OA 分析 180s / 增强处理 300s，HTTP 客户端 300s

### 6. experiment/ — AI 驱动的实验沙箱

- AI 生成 Python 验证脚本 → 子进程隔离执行 → 提取 JSON 指标
- 失败不中断流水线

### 7a. bin/mcp-server.rs — MCP Server

独立二进制，通过 stdio JSON-RPC 暴露 9 个专利/创意工具，代理到 localhost:3000。

## 跨平台设计 / Cross-Platform Design

| 平台 | 入口 | 构建 |
|---|---|---|
| 桌面 (Windows/Linux/macOS) | `main.rs` → `cargo run` | 二进制 |
| Docker | `Dockerfile`（rust:1.82 → debian:bookworm-slim） | 容器 |
| Android | `lib.rs` → cdylib FFI | uniffi/JNI |
| iOS | `lib.rs` → cdylib FFI | uniffi |
| HarmonyOS | `lib.rs` → cdylib FFI | NAPI |

### 关键依赖选择

- **数据库**：`rusqlite`（bundled，无需系统 SQLite）
- **HTTP**：`reqwest`（rustls-tls，无需 OpenSSL）
- **静态资源**：`rust-embed`（编译进二进制）
- **环境变量**：`dotenvy`

## 部署选项 / Deployment Options

### 1. 独立二进制 / Standalone Binary

```bash
cargo build --release --bin innoforge
./target/release/innoforge    # 默认 http://0.0.0.0:3000
```

### 2. Docker 容器 / Docker Container

```bash
docker build -t innoforge .
docker run -p 3000:3000 -v ./innoforge.db:/app/innoforge.db innoforge
```

### 3. 系统服务 / System Service

- Windows: `start.bat`（自动打开浏览器）或任务计划程序
- Linux: systemd
- macOS: LaunchAgent

## 安全 / Security

| 措施 | 实现 |
|---|---|
| SQL 注入防护 | 全参数化查询 + FTS5 查询消毒 |
| XSS 防护 | DOMPurify（前端）+ `html_escape`（后端）+ X-Frame-Options: DENY |
| SSRF 防护 | 图片代理 allowlist（3 个专利图片域名） |
| CORS | tower-http 默认本机 allowlist；额外来源通过 `INNOFORGE_CORS_ORIGINS` 显式配置 |
| 提示注入防护 | CLAUDE.md 规范：超时 + 错误处理 + 注入防护 |
| 请求体限制 | 20MB |
| API 密钥 | .env 文件（gitignored） |

## 性能考虑 / Performance

- FTS5 全文检索 + BM25 排序（自定义列权重）
- 搜索缓存（24h TTL）
- 流水线断点续跑（快照恢复）
- 异步 I/O（Tokio）
- 静态资源嵌入二进制（无磁盘 IO）
