# Phase 3 — server 新写层与 workspace 化（泳道 A 核心，可与 Phase 5 并行）

> 前置阅读：task-breakdown.md 头部「终态蓝图」+ Phase 3 表格；module-inventory.md §2 行号级证据
> 路线 A 铁律：**不是拆小旧文件，而是新写替代**——每个 API 族在 crates/server 新写薄 handler 并过 e2e 后，删除对应旧代码。旧文件只减不增，本阶段结束时 src/routes/ 不复存在。

- [ ] T3.0 workspace 化：根 Cargo.toml 转 [workspace]；src→crates/server/src；templates/static 留根修相对路径；FFI 四符号验证；e2e/扫描器路径核对
- [ ] T3.1 idea 域新写（crud/chat/research_state/report_render/claim_tree 五模块）→ 删 idea.rs；测试 ≥15
- [ ] T3.2 ai 域新写（oa/chat/compare_analyze 薄层 + prompt 入 crates/ai prompts 模块）→ 删 routes/ai.rs
- [ ] T3.3 patent/search/upload 域新写（编排薄层 + export 模块 + 提取注册表）→ 删三个旧文件
- [ ] T3.4 common.rs 消灭：路由按域 router() 组装 + 118 条数量断言测试
- [ ] T3.5 pages.rs render_template 统一 + axum 0.6/0.8 决策落地写死文档

验收门禁：全套 DoD；单 handler ≤80 行、单文件 ≤500 行；每删一个旧文件跑全量 e2e
Notes:
- T3.0 是唯一允许"纯机械移动"的步骤；T3.1 起一律新写，禁止把旧函数体原样复制进新模块了事——对照 module-inventory §2 的职责清单逐项落实 service 化
- tests/cad_template_contract 与 e2e 是安全网；聊天/OA 相关域落地后必跑
- FFI 符号契约：innoforge_start_server/innoforge_shutdown_server/patent_hub_start_server/patent_hub_shutdown_server 必须原签名保留（外部 desktop/ios/harmony 仓库依赖）
