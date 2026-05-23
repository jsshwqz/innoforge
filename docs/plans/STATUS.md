# InnoForge 当前状态 / Current Status

> 本文档是**活的**——每个任务完成后必须更新。
> 下一个 Agent 进项目时应先读此文件了解当前焦点。

---

## 当前焦点 / Current Focus

**v0.6.0 — OA 答复页面 + 三档深度分析 + 治理体系完善**

当前版本 v0.6.0 已发布，核心功能包括 OA 答复页面（一审/二审/非正常/驳回复审）、三档分析深度（快速/标准/深度含 AI 自检反驳）。正在进行治理体系建设收尾（文档规范化、代码质量加固）。

---

## 最近变更 / Recent Changes

| 日期 | 变更 | 类型 |
|------|------|------|
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

### HIGH
1. **Pipeline 步骤可视化不完整** — 前端进度条只显示主步骤，缺少子步骤状态 ✅ 2026-05-23 已修复（15 步 + 通用子步骤）
2. **聊天消息分段加载未实现** — ✅ 2026-05-23 已实现（后端分页 + 前端加载更多）

### MEDIUM
3. **Linux/macOS 安装包未更新** — GitHub Release v0.6.0 缺 macOS 二进制（构建成功但资产未上传，疑似 download-artifact@v4 路径问题），可手动触发 workflow_dispatch 重试

### 已解决 / Resolved
- ~~Gitee Release 同步~~ — ✅ 2026-05-23 v0.5.10/v0.6.0 Release + v0.5.9 tag

### 已解决 / Resolved
- ~~生产路径残留 unwrap()~~ — ✅ 2026-05-23 修复 10+ 处
- ~~根目录冗余文件~~ — ✅ 2026-05-23 删除 + .gitignore 防护
- ~~CLAUDE.md Pipeline 步数不准确~~ — ✅ 2026-05-23 修正为 15 步
- ~~docs/plans/ 缺 v0.6.0 计划文档~~ — ✅ 2026-05-23 已创建

---

## 下一步 / Next Steps

- [x] 治理体系文档评审（CLAUDE.md 流程 + docs/errors.md + STATUS.md）
- [x] 消除生产路径残留 unwrap() — 约 10 处需要修复
- [x] 清理根目录冗余文件 main.rs + write_start.py
- [x] 更新 CLAUDE.md Pipeline 步数（13 → 15）
- [x] 创建 v0.6.0 计划文档 docs/plans/
- [x] 清理远程 worktree 分支 `claude/lucid-engelbart-8f807d`
- [ ] 聊天消息分段加载优化
- [ ] Pipeline 步骤可视化增强
- [ ] 全平台 Release 包构建

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

*最后更新: 2026-05-23*
