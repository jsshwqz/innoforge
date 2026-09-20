# Phase 4 — 数据层与死代码收尾

> 前置阅读：task-breakdown.md Phase 4 表格 + Phase 0.5 裁决（用户已定，不再讨论）

- [ ] T4.1 死代码处置收尾：fact_check 接入 OA 分析流程；rag 按新 types/ 重写为独立 service 并接线；vector 合一收尾（T2.2）+ embeddings 写入链路贯通
- [ ] T4.2 db 层解反向依赖：db→pipeline::context 类型依赖反转为 types/ DTO；（可选）按域 Repository trait
- [ ] T4.3 Pipeline results 强类型化第一步：StepOutput 枚举写入侧替代 HashMap<String,Value>
- [ ] T4.4 Database 临界区审计：锁内禁外部 HTTP/AI 慢调用；并发冒烟

验收门禁：全套 DoD；db/*.rs 无 use pipeline::；16 步流水线 e2e 全过
Notes:
- FTS5 索引为手动同步（save_patent 内 DELETE+重灌），任何绕过 save_patent 的 patents 写入都会失联——T4.2 时把同步收敛到单一入口或加触发器评估
