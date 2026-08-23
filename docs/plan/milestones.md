# 重构里程碑 / Milestones

> 判定标准全部为客观可验证条件；日期为相对工期（净工作日），非日历承诺。

## M0 — 开工就绪（Phase 0 完成）
- git status 干净；预处理修复已入库（冲突标记/CAD 接线/cad.rs 清理）
- **本地门禁全绿**：fmt/clippy/test（2026-08-15 实测曾三红：fmt 漂移、clippy 32 错、磁盘满致链接失败——T0.10 恢复）
- **双远端 CI 对最新提交全绿**：推送 main 与 dev（当前领先远端 60+/36 提交未经 CI——T0.6）
- dev 包含 main 全部提交（用户决策：直接在 dev 执行重构）
- 磁盘水位 ≥5GB（target/ 曾占 8.4GB，cargo clean 已释放）
- SCHEMA_VERSION=23 且空库初始化验证通过
- 文档漂移清零（AGENTS.md 双入口条款/v11/15步/AI超时 全部修正）

## M1 — 地基成型（Phase 1 完成）
- SearchResult 唯一定义；types/ 领域模块就位
- AppConfig/AppState 脱离 routes/mod.rs；common.rs 无反向依赖
- 全部路由统一 AppError 错误契约（含错误码）
- 生产路径 unwrap/expect = 0

## M2 — 端口贯通（Phase 2 完成）⭐ 核心里程碑
- SearchProvider / Embedder / PdfExtractor / ProviderMode adapter 四大端口落地
- SerpAPI 实现 ×3→×1；TF-IDF ×3→×1（char-boundary 安全版）；服务商知识 ×3→×1（表驱动）
- ai/client.rs 语义拆为 FailoverClient + 三 adapter，容灾循环测试 ≥8 个
- **意义**：此后所有新写都是"把调用方接到已有端口上"，不再产生新轮子

## M3 — 新骨架成型（Phase 3+4 完成）
- **workspace 化完成**：crates/{types,config,db,ai,search,pipeline,server} 就位，src/routes/ 整体消失
- routes 万能层被薄 handler + service 层替代；idea 域测试 ≥15 个
- rag/vector/fact_check 三块死代码有明确归宿并接线【裁决已定】
- db 层无 pipeline 类型反向依赖；Pipeline 写入侧强类型化

## M4 — 前端收敛（Phase 5 完成）
- escapeHtml 等 ×7 重复定义清零；i18n.js 只管翻译
- OA 页与 idea 页 JS 完成分段外置；innerHTML 255 处审计闭环、高危清零
- 函数完整性基线已 --refresh 并随代码提交

## M5 — 发布 v0.1.0（Phase 6 完成）
- 覆盖率数字落档（对比重构前）
- e2e ≥60 项；文档与代码零漂移
- CHANGELOG 完整、双远端 CI 绿、tag v0.1.0 发布（全新版本线，用户决策；若实意为 1.0.0 仅改此处）

---

### 风险提示
- M2 是整个方案的枢纽：若 T2.3（AI client 拆分）工作量超预期，允许裁剪为"仅抽 StreamParser + 保持容灾循环原样"，其余端口照常——M3 起不受影响。
- M3 的死代码决断（T4.1）是唯一需要用户在方案确认时一并拍板的事项。
