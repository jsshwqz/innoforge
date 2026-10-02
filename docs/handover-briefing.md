# InnoForge 项目交接文档

> 供接手的 AI 助手快速了解项目全貌，避免重复踩坑。

---

## 一、项目基本信息

| 项目 | 说明 |
|------|------|
| 名称 | InnoForge |
| 版本 | v0.4.2 |
| 技术栈 | Rust + Axum + SQLite（本地优先 Web 应用） |
| 仓库 | GitHub: https://github.com/jsshwqz/innoforge |
| 镜像 | Gitee: https://gitee.com/jsshwqz/innoforge |
| 许可 | MIT |
| 定位 | 从"专利搜索工具"升级为"研发创新验证助手" |

## 二、核心架构

```
innoforge/
  src/
    main.rs          # Axum Web 服务器入口，50+ 路由注册
    lib.rs           # 库导出 + Android JNI 入口
    ai.rs            # AI 多模型容灾客户端（6 服务商自动切换）
    db.rs            # SQLite + FTS5 数据库层（schema v5，5次迁移）
    patent.rs        # 数据结构定义（Patent, Idea, IdeaSummary 等）
    error.rs         # 统一错误类型
    routes/          # API 路由处理器
      mod.rs         # AppState, AppConfig 定义
      idea.rs        # 创意验证 + 多轮 AI 对话（核心功能）
      search.rs      # 专利搜索
      ai.rs          # AI 聊天/摘要/对比
      patent.rs      # 专利详情/PDF/图片代理
      settings.rs    # 系统设置
      collections.rs # 收藏夹
      upload.rs      # 文件上传
      ipc.rs         # IPC 分类
      pages.rs       # 页面渲染
    pipeline/        # 12 步创新验证流水线
      runner.rs      # 状态机驱动器（支持 quick_mode）
      context.rs     # 流水线上下文数据结构
      state.rs       # 步骤枚举
      steps/         # 12 个步骤实现
        parse.rs     # Step 1: 解析输入
        expand.rs    # Step 2: AI 扩展查询词
        search.rs    # Step 3-4: 搜索 Web + 专利
        diversity.rs # Step 5: 多样性检测
        similarity.rs# Step 6: TF-IDF + Jaccard 相似度
        rank.rs      # Step 7: 排序去重
        contradiction.rs # Step 8: 矛盾检测
        scoring.rs   # Step 9: 新颖性评分
        analysis.rs  # Step 10-11: AI 深度分析 + 行动计划
        finalize.rs  # Step 12: 汇总报告
    bin/
      mcp-server.rs  # MCP Server（6 工具）
      skill-router.rs# 技能路由 CLI
    skill_router/    # 技能路由引擎
  templates/         # HTML 页面模板（include_str! 内嵌）
  static/            # CSS + JS + i18n + PWA manifest
  tests/             # 集成测试
  mobile/            # Dioxus 移动端（已基本弃用）
```

## 三、关键功能

### 搜索降级链
SerpAPI → Google Patents → Bing → Lens.org → 搜狗免费 → 本地 SQLite FTS5

### AI 容灾链
主 AI → 最多 5 个备用 AI（任意 OpenAI 兼容 API）

### 创新验证 Pipeline（12 步）
ParseInput → ExpandQuery → SearchWeb → SearchPatents → DiversityGate → ComputeSimilarity → RankAndFilter → DetectContradictions → ScoreNovelty → AiDeepAnalysis → AiActionPlan → Finalize

### AI 对话
- 超 8 轮自动压缩旧消息为摘要（长记忆）
- 系统 prompt 含第一性原理、TRIZ、逆向工程等思维框架
- 分析上下文只取关键结论行（质量优先）

## 四、已知问题（按严重程度）

### Critical
1. **XSS 风险** — `templates/idea.html` 中 AI 聊天消息用 `innerHTML` 渲染，应改为 `textContent` 或白名单净化
2. **SSRF 风险** — `/api/patent/image-proxy` 缺少 URL 域名白名单校验

### High
3. **std::sync::Mutex 在 async 中** — `db.rs` 用标准库 Mutex 包裹 SQLite Connection，在 async handler 中持有会阻塞 tokio 线程
4. **AI 超时链过长** — 120s × 2 重试 × 6 provider = 最坏 24 分钟无响应
5. **pipeline_channels 内存泄漏** — HashMap 永不清理已完成管道的通道

### Medium
6. **API 不 RESTful** — `/api/idea/:id/delete` 用 POST 而非 DELETE（收藏夹正确用了 DELETE）
7. **错误格式不统一** — 部分 `{"status":"error","message":"..."}` vs `{"error":"..."}`
8. **无 CORS 中间件** — 跨域请求被拦截
9. **无优雅关机** — Ctrl+C 不等待请求完成
10. **端口硬编码** — HOST/PORT 环境变量存在但 main() 未使用
11. **无速率限制** — AI 端点可被刷
12. **Mutex 中毒恢复不安全** — `poisoned.into_inner()` 可能导致数据库不一致

### Low
13. 无 HTTPS/TLS 支持
14. db_path 硬编码为当前目录
15. main.rs 50+ 路由太长，可拆分
16. SQL 迁移内联在 init() 中，可用迁移工具

## 五、版本规划

详见 `docs/plans/研发助手升级规划-多方讨论.md`

### v0.4.x — 体验打磨（当前）
- [x] 首页从搜索框改为研发助手入口
- [x] AI 对话增强（专家级 prompt + 思维框架）
- [x] 智能上下文管理（自动压缩 + 长记忆）
- [x] 历史记录增强（时间标记 + 预览 + 对话计数）
- [x] 自审查系统（scripts/self-review.ps1）
- [ ] 修复上述 Critical/High 问题

### v0.5.0 — 核心专业化（最关键版本）
P0 必做 5 项：
1. 技术特征卡片（Feature Card）— 专利结构化
2. 最小差异定位（Minimal Diff）— 找新颖性差异
3. 证据链系统（Evidence Chain）— 结论可追溯
4. 研发状态机（ResearchState）— 中断恢复
5. 分层报告生成（Report Generator）— 领导版/研发版/专利版

### v0.6.0 — 搜索升级
- sqlite-vss 向量检索
- 学术搜索（Semantic Scholar / PubMed）

### v0.7.0 — 进阶能力
- 规避设计建议
- V-RAG 图文对齐

### v1.0.0 — 品牌发布
- 产品定名（替代 InnoForge）
- AionUi MCP 集成上线

## 六、待办事项

1. **AionUi PR** — 扩展包在 `D:/test/innoforge-aionui-extension/`，等稳定后 Fork https://github.com/iOfficeAI/AionUi 提 PR
2. **三个独立仓库推 Gitee** — innoforge-ios / harmony / desktop（本地 git init 完成，需用户在 Gitee 创建远程仓库）
3. **产品命名** — 候选名单在规划文档中

## 七、用户偏好

- **语言**：中文为第一语言，所有面向用户内容中文在前
- **双语规范**：代码注释、文档、Release Notes、Issue 模板全部中英双语
- **风格**：喜欢简洁直接，"好兵在质量不在数量"
- **技术偏好**：Rust 优先，本地优先（不依赖云服务），注重隐私

## 八、构建与运行

```bash
# 构建
cargo build --release --bin innoforge

# 运行
cargo run --release --bin innoforge
# 或双击 start-innoforge.bat

# 测试
cargo test

# 自审查
powershell -ExecutionPolicy Bypass -File scripts/self-review.ps1 -Force

# MCP Server
cargo run --release --bin innoforge-mcp
```

## 九、重要文件

| 文件 | 说明 |
|------|------|
| `Cargo.toml` | 依赖和版本号 |
| `CHANGELOG.md` | 完整中英双语更新日志 |
| `README.md` | 项目说明（中英双语） |
| `docs/plans/研发助手升级规划-多方讨论.md` | 产品规划（合并了用户、GPT、Gemini、Claude 的建议） |
| `docs/国内用户指南.md` | 国内无 VPN 使用指南 |
| `docs/API.md` | API 文档 |
| `docs/ARCHITECTURE.md` | 架构文档 |
| `.env.example` | 环境变量模板 |
| `scripts/self-review.ps1` | 自审查脚本 |

## 十、踩过的坑

1. **Windows BAT 脚本中文乱码** — GBK 编码问题，改用英文 label
2. **AI 分析全部失败** — 通常是 API Key 无效（401）或限速（429），需要用户在设置页面切换到可用的 AI 服务商
3. **cargo build "access denied"** — innoforge.exe 被运行中的进程锁住，需先 taskkill
4. **GitHub Release 重复 Changelog** — 已修复，根因是 `release.yml` 的 `generate_release_notes: true` 在 5 个矩阵构建任务中各追加一次
5. **设置下拉框不回显** — 已修复，页面加载时 `updateAiFields()` 覆盖了保存的值
6. **git push 用错 remote 名** — 仓库有两个 remote：`origin`（GitHub）和 `gitee`（Gitee）

---

*最后更新：2026-04-01 by Claude*
