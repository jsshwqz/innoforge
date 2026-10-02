# InnoForge 项目全景概览 / Project Overview

> 分析日期：2026-08-15（基于 HEAD `69b6a85`）
> 分析目的：完整重构前的现状基线（Phase 1 — Architecture & Stack）
> 数据来源：全仓源码实测统计 + git 历史 + 文档交叉验证

---

## 1. 项目身份

**InnoForge（创研台）** — 面向研发用户（发明人、工程师）的 AI 驱动专利检索与创新验证平台。

核心能力矩阵：

| 能力域 | 说明 | 关键模块 |
|--------|------|----------|
| 专利检索 | SerpAPI 在线检索 + 本地 SQLite FTS5 全文 + 向量混合搜索 | `routes/search.rs`, `db/patent.rs`, `vector/`, `rag/` |
| 专利管理 | 详情富化、法律状态、图片代理、收藏夹/标签、CSV/XLSX 导出 | `routes/patent.rs`, `db/collection.rs` |
| 创意验证 | **16 步创新流水线**（状态机 + 断点续跑）+ 迭代分支/版本 | `pipeline/`, `orchestrator/`, `db/version.rs` |
| OA 答复 | 审查意见分析→讨论→意见陈述书生成→DOCX 导出 | `ai/patent.rs`, `ai/fact_check.rs`, `docx_export/` |
| AI 对话 | 多服务商聊天（SSE 流式）、多模态图片、成本追踪 | `ai/client.rs`, `db/cost.rs` |
| FreeCAD 协同 | 自然语言 CAD 建图（经独立 AionCAD Rust HTTP 桥） | `cad.rs`, `routes/cad.rs`, `static/cad.js` |
| MCP 出口 | 对外提供 MCP Server | `bin/mcp-server.rs` |

## 2. 技术栈（实测）

| 层级 | 技术 | 版本 |
|------|------|------|
| 语言 | Rust edition 2021，crate-type = lib + cdylib + staticlib | — |
| Web | axum + tokio(full) + tower-http(fs/cors/set-header) | axum **0.6** |
| 数据库 | rusqlite(bundled) + FTS5，单连接 `Mutex<Connection>` | rusqlite 0.31 |
| HTTP 客户端 | reqwest(rustls, blocking, multipart, stream) | 0.11 |
| PDF 解析 | pdf-extract → pdftotext(外部) → PyMuPDF(python 外部) → MinerU(云 API) → OCR 多级降级链 | — |
| DOCX | 自研 zip+xml 写入器 | zip 2.x |
| 前端 | 纯 HTML/CSS/JS（无构建工具），RustEmbed 编译嵌入，DOMPurify 本地化，data-i18n 双语 | — |
| 页面渲染 | **`include_str!` + `.replace("{{var}}")` 手写占位符替换**（非 Askama，旧文档描述已漂移） | — |
| FFI | `#[no_mangle] extern "C"` 裸导出（uniffi 由消费仓使用） | — |
| CI | GitHub Actions + Gitee Actions 双镜像：fmt/clippy/test/lint/e2e 全门禁 | — |

## 3. 规模统计（实测）

- Rust：**87 个文件，25,784 行**（含测试）
- HTML 模板：8 个页面，共约 503KB；最大 `office_action_response.html` 153KB
- 静态资源：style.css 62KB / i18n.js 45KB / cad.js 9KB / purify.min.js 20KB
- API 路由注册：**118 条**（全部集中在 `src/common.rs::build_router`）
- DB Schema：**v23**（迁移历史 v1→v23，见 `src/db/migrations.rs`）
- 测试：159 个 Rust 测试全绿 + Puppeteer e2e 54 项 + HTML 函数完整性基线扫描
- git：479 commits（2026-02-24 起），远端 GitHub + Gitee 双推
- 版本：v0.7.4（Cargo.toml），CI 绿

## 4. 入口与启动链路（已验证）

```
┌─ src/main.rs (116行) ──── 桌面/Docker 独立二进制 innoforge-server
│                            └─→ common::init_app_state + build_router
├─ src/lib.rs (154行) ───── Android/iOS/HarmonyOS FFI（cdylib）
│                            └─→ 同上（共享，双入口已统一 ✅）
└─ src/bin/mcp-server.rs ── MCP Server 出口
```

关键事实：
- **路由注册已经集中**在 `common.rs::build_router()` 一处（118 条 route），main/lib 双入口通过共享函数消除漂移。AGENTS.md 中"两处路由必须保持一致"的条款**已过时**（实际机制更好）。
- `AppState { db: Arc<Database>, cad: Arc<CadService>, config: Arc<RwLock<AppConfig>>, pipeline_channels }`
- 移动端服务器绑定死端口 3000、线程内 block_on 运行时。

## 5. 实际分层架构

```
templates/*.html (include_str! 渲染)
        │
routes/* (13 子模块, handler 层——但普遍混入业务逻辑)
        │
   ┌────┼──────────┬─────────────┬────────────┐
   ▼    ▼          ▼             ▼            ▼
 ai/*  pipeline/* orchestrator/ rag/vector/  cad.rs
 (AI客户端+prompt) (16步状态机) (命令编排)   (检索增强)  (AionCAD桥)
   │        │          │            │
   └────────┴──────────┴────────────┘
                │
              db/* (17 文件, Mutex<Connection> 单连接, v23)
```

- `experiment/`（WASM 沙箱）为部分实现的预留子系统。
- 前端为"服务端整页 + 内联 JS"模式：每个模板内嵌全部交互逻辑（无模块化）。

## 6. 构建与部署矩阵

| 出口 | 产物 | 说明 |
|------|------|------|
| 桌面/Docker | innoforge-server 二进制 | start.bat / dev.bat / Dockerfile |
| 移动端 | cdylib/staticlib | 由独立仓库消费（innoforge-desktop/-ios/-harmony，不在本仓库） |
| MCP | innoforge-mcp 二进制 | `bin/mcp-server.rs` |
| 静态资源 | rust-embed 编译嵌入 | 改前端需重编译 |

## 7. 外部集成清单

| 集成 | 方向 | 位置 |
|------|------|------|
| SerpAPI（≤5 key 轮询） | 出站 | routes/search.rs |
| AI 服务商 ×10（DeepSeek/Gemini/商汤/智谱/千问/OpenAI/Kimi/豆包/OpenRouter/小米）+ Gemini CLI 子进程 | 出站 | ai/client.rs |
| Google OAuth / gcloud ADC 认证 | 出站 | routes/auth.rs |
| 图片代理（白名单 HTTPS） | 出站 | routes/patent.rs |
| AionCAD FreeCAD 桥 http://127.0.0.1:8080 | 出站 | cad.rs |
| pdftotext / PyMuPDF(python) / Umi-OCR / MinerU 云 | 本地进程+云 | routes/upload.rs |
| 远程专利 PDF 下载（HTTPS 白名单+SSRF 防护） | 出站 | routes/patent.rs |

## 8. 配置体系

- 主存储：SQLite settings 表（Android 友好）；后备：`.env`（桌面）
- `AppConfig` 含 10 个服务商独立 Key 的**硬编码字段**（ai_api_key_deepseek/_anthropic/_xiaomi/_sensetime/_openrouter/_gemini/_zhipu/_qwen/_openai…）
- 环境变量：INNOFORGE_CORS_ORIGINS、INNOFORGE_AIONCAD_WORKSPACE 等
- 运行时临时文件统一 `data/runtime-temp`（UUID+create_new，RAII 清理 ✅）

## 9. 与历史文档的差异（文档漂移清单）

| 文档说法 | 实际情况 |
|----------|----------|
| AGENTS.md §2.4：API 须在 main.rs 和 lib.rs 双处注册 | 已集中到 common.rs::build_router 一处（2026-07 后重构） |
| 旧分析(2026-07-05)：前端用 Askama 模板引擎 | 实际是 include_str! + .replace() 手写替换 |
| 旧分析：PipelineStep 16 步 | 正确（16 步，AGENTS.md 写的 15 步过时） |
| STATUS.md「当前版本 v0.7.4 开发中」 | Cargo.toml=0.7.4 且 CHANGELOG [Unreleased] 大量条目未发版 |

## 10. 工作区卫生（不影响 git 仓库本身）

根目录存在 ~250 个本地临时文件（`_*.txt`、`server_v*.log`、`*.zip`、`innoforge.db*` 等）。经 `git ls-files` 核对：**全部未被跟踪**（.gitignore 有效），属于本地工作区污染而非仓库问题，但对开发体验和扫描干扰极大。
