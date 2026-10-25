# InnoForge 先进化 — AI 接力简报 / AI Handoff Briefing

> 生成日期：2026-08-05 | 基于 v0.7.4 代码审查
> 目标：让其它 AI Agent 快速了解项目状态并继续执行

---

## 一、项目全景

- **项目**: InnoForge — AI 驱动的专利检索与可行性分析
- **版本**: v0.7.4（2026-07-17）
- **技术栈**: Rust + Axum 0.6 / SQLite + FTS5 / 纯 HTML+CSS+JS
- **Pipeline**: 16 步（code 先算 + AI 后判）
- **8 个 AI provider**: DeepSeek 主 / Gemini 副
- **Gitee**: https://gitee.com/jsshwqz/innoforge
- **GitHub**: https://github.com/jsshwqz/innoforge
- **工作目录**: D:/test/patent-hub-backup

---

## 二、已完成的 Phase 0 — 代码修复

| Task | 状态 | Commit | 文件 | 说明 |
|---|---|---|---|---|
| 0.1 | ✅ | ad1e7fe | src/experiment/sandbox.rs, src/common.rs | sandbox 超时控制 + 临时文件迁移到 data/runtime-temp |
| 0.2 | ✅ | 625ac82 | src/db/patent.rs, src/routes/search.rs | IPC/CPC 过滤 N+1 → 批量 WHERE IN |
| 0.3 | ✅ | 625ac82 | src/bin/mcp-server.rs | 移除 unwrap_or_default + JSON-RPC id 处理 |

---

## 三、已完成的 Phase 1 — 部分

| Task | 状态 | Commit | 说明 |
|---|---|---|---|
| 1.A | ✅ | c2d3c3c | Fact-Check 已在流程中工作，仅清理过时标记 |

---

## 四、待完成的任务（按优先级）

### Phase 1（基础设施增强）— 继续执行

| Task | 名称 | 工作量 | 依赖 | 关键要点 |
|---|---|---|---|---|
| **1.C** | AI 成本追踪 | 1 周 | 无 | 新增 ai_cost_ledger 表 + 解析 usage 字段 + 前端展示 |
| **1.B** | 向量嵌入 + 混合搜索 | 2 周 | 需确认 | 新增 text2vec 依赖（纯 Rust）+ RRF 融合 + 需用户确认新增依赖 |
| **1.D** | RAG 管道 | 2 周 | 依赖 1.B | 专利全文切片 + 嵌入 + 检索增强生成 |

### Phase 2（智能升级）

| Task | 名称 | 工作量 | 依赖 |
|---|---|---|---|
| 2.E | 多智能体 Pipeline | 3 周 | 依赖 1.C |
| 2.F | 持久化记忆系统 | 2 周 | 独立 |

### Phase 3（高级功能）

| Task | 名称 | 工作量 | 依赖 |
|---|---|---|---|
| 3.G | Agentic 自主研究 | 3 周 | 依赖 2.E |
| 3.H | Notebook 沙箱 | 2 周 | 独立 |
| 3.I | 专利组合分析 | 3 周 | 独立 |

### Phase 4（工程化）

| Task | 名称 | 工作量 | 依赖 |
|---|---|---|---|
| 4.J | 可观测性 | 1-2 周 | 独立 |
| 4.K | 插件化架构 | 4 周 | 依赖 4.J |

---

## 五、已知问题（需处理）

| # | 问题 | 严重程度 | 状态 | 处理方式 |
|---|---|---|---|---|
| 1 | GitHub 推送 443 端口超时 | 中 | ⏸ 待处理 | 需要网络恢复或配置代理后执行 `git push --force-with-lease origin main` |
| 2 | MCP server 有 dead_code 警告 | 低 | ⏸ 待处理 | http_get, call_patent_* 等函数未使用（简化版本），后续可恢复 |
| 3 | sandbox.rs 中 test_timeout_terminates_script 为 #[ignore] | 低 | ⏸ 待处理 | 需手动运行验证超时逻辑 |

---

## 六、推荐执行顺序

```
1.C (AI 成本追踪) — 最快见效，无依赖
  │
1.B (向量嵌入) — 需用户确认新增 text2vec 依赖
  │
1.D (RAG 管道) — 依赖 1.B
  │
2.E (多智能体) — 依赖 1.C
  │
2.F (记忆系统) — 独立
  │
Phase 3+4 — 按需
```

---

## 七、关键文件索引

| 文件 | 用途 |
|---|---|
| docs/plans/2026-08-05-advanced-features-roadmap.md | 详细执行计划（含代码示例、DB SQL、验收标准） |
| docs/plans/2026-08-05-ai-work-log.md | AI 工作记录（含模板） |
| docs/plans/STATUS.md | 项目总体进度 |
| docs/plans/2026-08-05-ai-handoff-briefing.md | 本文档 |

---

*本文档由架构分析 Agent 创建于 2026-08-05*
