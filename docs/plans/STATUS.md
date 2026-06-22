# InnoForge 当前状态 / Current Status

> 本文档是**活的**——每个任务完成后必须更新。
> 下一个 Agent 进项目时应先读此文件了解当前焦点。

---

## 当前焦点 / Current Focus

**v0.6.3 — OA 一审结构化分析 + 代码质量加固**

当前版本 v0.6.3 已发布。下一步重点：搜索质量升级（向量检索、学术搜索、CRAAP 评分）。

### ⚠️ 已知遗留问题
1. ~~**OA 页面全黑问题** — 已修复（DOMPurify 缺失），但启动方式需注意：使用 `dev.bat`（debug 模式）而非 start.bat，后者可能闪退~~ ✅ **已彻底修复**：DOMPurify 已本地化到 `static/purify.min.js`，通过 `rust_embed` 编译进二进制，不依赖任何 CDN。`start.bat` 已重写，改为每次启动前重新编译，消除二进制与源码不一致问题。现在 `dev.bat` 和 `start.bat` 均可正常使用。

---

## 最近变更 / Recent Changes

| 日期 | 变更 | 类型 |
|------|------|------|
| 2026-06-22 | v0.6.3 发布：OA 一审结构化分析（5部分输出） + 分段卡片展示 + 意见陈述书导出 + 代理/超时/DOMPurify/start.bat 修复 | feat |
| 2026-06-26 | v0.6.2 发布：搜索页 PDF 上传 + 首页文件持久化 + DOMPurify 本地化 + start.bat/dev.bat + clippy修复 + .gitignore增强 + 工作树清理 | fix |
| 2026-05-25 | DOMPurify 本地化 + start.bat 修复：CDN → `static/purify.min.js` 编译嵌入，start.bat 改为始终编译 | fix |
| 2026-05-23 | OA 答复深度增强：查库补全/分析历史DB v14/修改审查/论据库/检查清单 + 全页面引用按钮 | feat |
| 2026-05-23 | v0.6.1 发布：代码质量加固（unwrap消除）+ 聊天分段加载 + Pipeline 15步同步 + CI/CD 修复 + 文档治理 | release |
| 2026-05-23 | v0.6.0 治理维护：消除生产路径 `unwrap()` + 文档同步 + 根目录清理 + CLAUDE.md Pipeline 步数修正 | refactor |
| 2026-05-23 | v0.6.0 发布：OA 答复页面（三种类型 + 三档深度 + AI 自检反驳）、通俗表达规则、中国时区修复、引用按钮修复 | feat |
| 2026-05-21 | 多 Agent 统一治理体系建立：`docs/errors.md` + `docs/plans/STATUS.md` + CLAUDE.md 流程增强 | docs |
| 2026-05-21 | 全面审计修复 18 个 Bug（凭据掩码/OAuth/阻塞调用/unwrap 等） | fix |
| 2026-05-14 | v0.5.9 发布：AI 服务商预设、图片粘贴、专家模型、SerpAPI 余额、对话跨设备同步、CI/CD | feat |
| 2026-05-11 | v0.5.8 发布：SerpAPI 多 Key 轮询、搜索源/AI 精简、AI 聊天持久化、网络错误重试 | feat |
| 2026-05-02 | v0.5.7 发布：Firecrawl 专利兜底搜索 | feat |
| 2026-04-18 | v0.5.6 发布：研发状态机持久化、中途重定向重跑 | feat |

---

## 已知问题 / Known Issues

当前无 HIGH/MEDIUM 未解决项。全部已知问题已在 v0.6.1 中修复。

### v0.6.1 已修复
- ~~生产路径残留 unwrap()~~ — ✅ 消除 14 处
- ~~聊天消息分段加载~~ — ✅ 后端分页 + 前端加载更多
- ~~Pipeline 步骤可视化~~ — ✅ 15 步 + 通用子步骤
- ~~Linux/macOS 安装包~~ — ✅ 全平台自动构建
- ~~Gitee Release 同步~~ — ✅ v0.5.10/v0.6.0 Release + v0.5.9 tag
- ~~根目录冗余文件~~ — ✅ 删除 + .gitignore 防护
- ~~CLAUDE.md Pipeline 步数~~ — ✅ 修正为 15 步
- ~~docs/plans/ 缺计划文档~~ — ✅ v0.6.0/0.6.1 计划已创建
- ~~引用按钮缺失~~ — ✅ 覆盖全部 5 个聊天/讨论页面

---

## 下一步 / Next Steps

### v0.6.3 已完成
- [x] OA 一审 Prompt 重写：强制 5 部分结构化输出（权利要求解析/审查员逻辑/特征对比表/反驳论点/意见陈述书）
- [x] 分析结果分段展示：5 张彩色卡片，特征对比表突出显示
- [x] 意见陈述书草稿独立导出为 `.txt`
- [x] 深度模式 critique 范围缩小至第五部分
- [x] DOMPurify OA 页面 CDN → 本地（v0.6.2 遗漏）
- [x] reqwest 强制 `.no_proxy()` 绕过系统代理
- [x] HTTP 超时 45s → 180s
- [x] start.bat/dev.bat 加入 cargo PATH + 跳过编译优化
- [x] 实测验证：模拟审查意见通知书分析，21KB 完整五部分输出

### v0.6.2 已完成
- [x] DOMPurify CDN → 本地 `/static/purify.min.js`（6 模板全部替换）
- [x] 搜索页 PDF 上传区（拖拽/本地持久化/i18n 双语）
- [x] 首页文件上传 localStorage 持久化（刷新不丢失）
- [x] start.bat 重写（始终编译）+ dev.bat 调试启动脚本
- [x] 3 个 clippy 警告修复（manual_flatten / is_some_and）
- [x] .gitignore 增强（临时脚本/备份/适配器保护）
- [x] 清理 3 个 Claude 工作树分支 + 2 个 detached worktrees
- [x] 远程仓库同步（GitHub + Gitee）
- [x] 消除生产路径残留 unwrap() — 14 处
- [x] 清理根目录冗余文件
- [x] CLAUDE.md Pipeline 步数（13 → 15）
- [x] v0.6.0/0.6.1 计划文档
- [x] 清理远程 worktree 分支
- [x] 聊天消息分段加载
- [x] Pipeline 步骤可视化增强
- [x] 全平台 Release 包构建
- [x] Gitee Release 同步
- [x] OA 答复深度增强（查库/历史/修改审查/论据库/清单）
- [x] 引用按钮全页面覆盖

---

## 版本规划蓝图 / Roadmap

详见 `docs/plans/2026-05-23-v0.6.0-plan.md`（当前版本）
详见 `docs/plans/2026-05-11-v0.5.9-plan.md`（前一版本）
详见 `docs/plans/研发助手升级规划-多方讨论.md`（长期规划）
详见 `docs/plans/v0.5.0-remaining-tasks.md`（历史计划）

---

## 文档索引 / Document Index

| 文档 | 说明 | 必读 |
|------|------|------|
| `CLAUDE.md` | 项目宪法：规范、流程、禁忌 | 每次必读 |
| `CHANGELOG.md` | 版本历史：改了什么、加了什么 | 首次必读 |
| `docs/errors.md` | 错误复盘数据库：以前犯过什么错 | 首次必读 |
| `docs/plans/STATUS.md` | 本文档：当前焦点、下一步 | 每次必读 |
| `docs/ARCHITECTURE.md` | 架构决策记录 | 需要时读 |
| `docs/API.md` | API 文档 | 需要时读 |

---

*最后更新: 2026-06-22 22:30*
