# Phase 3 — routes 巨型文件拆分（可与 Phase 5 并行）

> 前置阅读：task-breakdown.md Phase 3 表格；module-inventory.md §2 行号级证据（拆分的地图）
> 铁律：只搬不写。每搬一个函数群跑一次相关 e2e。

- [ ] T3.1 idea.rs(2194) 五拆：idea_crud / idea_chat(压缩算法下沉) / research_state / report_render(Markdown渲染器独立) / claim_tree(SQL 下沉 db)；测试 2→≥15
- [ ] T3.2 ai.rs(2038) 分组拆：oa / chat / compare_analyze；prompt 集中各文件头部常量区
- [ ] T3.3 patent.rs 外部客户端外迁 patent_sources/{epo,uspto,google_patents}（实现 trait）
- [ ] T3.4 search.rs 导出独立 export.rs；向量检索改调 Embedder port
- [ ] T3.5 common.rs 减负：build_router 按 API 族分组 + 路由数量断言测试（防丢路由，现 118 条）
- [ ] T3.6 pages.rs 统一 render_template 替代逐字段 replace 链

验收门禁：全套 DoD；单文件 ≤600 行；idea.rs 测试 ≥15
Notes:
- idea.html/patent_detail/OA 三页的 CAD 契约测试（tests/cad_template_contract.rs）是本阶段安全网，动聊天相关 handler 后必须跑
