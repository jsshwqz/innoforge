# Phase 1 — 类型与错误地基（解反向依赖）

> 前置阅读：task-breakdown.md Phase 1 表格；module-inventory.md §3 类型重复清单、§4 违例清单

- [ ] T1.1 建 src/types/ 领域模块：patent.rs 29 类型按域拆分；消灭 SearchResult 双定义（patent.rs:218 vs pipeline/context.rs:114）
- [ ] T1.2 AppConfig/AppState 从 routes/mod.rs 上提 src/config.rs（common.rs 不再 use crate::routes）
- [ ] T1.3 AppError 启用：补错误码，全部路由改 Result<Json, AppError>，删手写元组
- [ ] T1.4 生产路径 unwrap/expect 清零（粗扫 ~25 处逐一甄别）
- [ ] T1.5 PathConfig 集中（data/uploads、runtime-temp、cad 目录、mcp BASE_URL 可配化）

验收门禁：全套 DoD + grep 验证（无手写错误元组、无生产 unwrap、common.rs 无反向依赖）
Notes:
- SearchResult 合并方向建议：以 patent.rs 版为准对外，pipeline 内部版本改名 PipelineSearchResult 或直接引用 types::search
- T1.3 注意前端对错误响应格式的依赖点（grep 前端 fetch 的 error/message 字段处理），保持 {status,message} 兼容并新增 code 字段
