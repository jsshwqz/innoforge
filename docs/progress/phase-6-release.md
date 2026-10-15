# Phase 6 — 收尾发布 v0.1.0

- [ ] T6.1 覆盖率度量引入（cargo llvm-cov），记录重构前后对比
- [ ] T6.2 e2e 补强至 ≥60 项（新模块边界：报告 HTML、导出文件、向量检索开关、CAD 契约）
- [ ] T6.3 文档终同步（ARCHITECTURE/API/AGENTS/CHANGELOG）
- [ ] T6.4 MCP 出口决断：6 声明 vs 1 可用（补全或裁剪）；patent_compare 失效端点修正
- [ ] T6.5 发布 v0.1.0：Cargo.toml 版本、CHANGELOG 新版本段、tag 推双远端、release.yml 5 平台矩阵验证

验收门禁：M5 判定标准全过（milestones.md）
Notes:
- 用户决策：全新版本线 0.1.0；若实意为 1.0.0 仅改此处，无前置依赖
- release.yml 会校验 tag==Cargo.toml==CHANGELOG 三方一致
