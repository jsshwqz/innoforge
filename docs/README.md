# InnoForge 创研台文档中心

欢迎查阅 **InnoForge (创研台)** 文档中心。本项目致力于帮助开发者与研发团队将技术想法推进为可核验的决策依据与知识产权成果。

---

## 快速导航 (Quick Navigation)

| 领域 | 核心文档 | 说明 |
| :--- | :--- | :--- |
| **快速上手** | [国内用户指南](国内用户指南.md) · [OA 业务操作指引](oa-guide.md) | 环境配置、Key 设置与专利业务实操 |
| **系统架构** | [系统整体架构 (Architecture)](ARCHITECTURE.md) · [设计方案库 (Design)](design/) | 系统分层、状态机流水线与模块划分 |
| **API 规范** | [API 业务文档](API.md) · [API 契约 (Contracts)](API-CONTRACTS.md) · [路由清单 (Routes)](API-ROUTES.md) | 三位一体的接口语义、路由映射与输入输出契约 |
| **工程协同** | [当前版本与进展](plans/STATUS.md) · [避坑知识库](errors.md) · [协作规约](../AGENTS.md) | 多 AI 协同规范、近期焦点与回归防线 |

---

## 1. 业务与用户指引

面向研发人员与发明人视角的实操指引：
* **[国内用户直连与配置指南](国内用户指南.md)**：涵盖免翻墙网络配置、大模型 Key 填报、离线数据库初始化。
* **[OA 答复与复审全流程指引](oa-guide.md)**：审查意见通知书导入、对比文件解析、反驳策略与证据表生成。
* **[公共数据与本地管线指引](public-data-local-pipeline.md)**：专利公开数据本地化缓存与级联检索机制。

---

## 2. 系统架构与设计 (Architecture & Design)

* **[系统核心架构 (ARCHITECTURE.md)](ARCHITECTURE.md)**：介绍 Axum + Tokio 单体轻量后端、SQLite 状态持久化、以及 16 步推演流水线。
* **[设计方案库 (`docs/design/`)](design/)**：
  * [`DESIGN_v5_engine.md`](design/DESIGN_v5_engine.md)：新一代创新推演与决策引擎架构设计。
  * [`evidence-kernel-v2.md`](design/evidence-kernel-v2.md)：证据链内核与段落级锚点核验。
  * [`offline-cache-architecture.md`](design/offline-cache-architecture.md)：多源检索本地离线缓存系统。
  * [`open-source-readiness.md`](design/open-source-readiness.md)：开源就绪度与代码整洁标准。

---

## 3. API 规范体系（三维一体防漂移机制）

本项目拥有 130+ 条 API 端点。为保证文档、代码与外部调用永不漂移，文档体系严格划分为三份独立但联动的契约文件：

```
                    ┌───────────────────────────────┐
                    │  src/common.rs (源码单一事实)  │
                    └───────┬───────────────┬───────┘
                            │               │
            scripts/gen_route_inventory.js   scripts/gen_api_contracts.js
                            ▼               ▼
         ┌──────────────────────┐       ┌────────────────────────┐
         │  docs/API-ROUTES.md  │       │ docs/API-CONTRACTS.md  │
         │ (路由端点与代码行映射) │       │ (字段级入参与出参契约) │
         └──────────────────────┘       └────────────────────────┘
                            ▲               ▲
                            └───────┬───────┘
                                    │
                         ┌─────────────────────┐
                         │     docs/API.md     │
                         │ (叙述性业务说明与用例) │
                         └─────────────────────┘
```

1. **[API.md](API.md)**：面向开发者的人类可读业务文档，提供详细的参数说明、错误码释义及典型交互示例。
2. **[API-ROUTES.md](API-ROUTES.md)**：由 `scripts/gen_route_inventory.js` 从路由声明自动抽取，由测试守卫钉住行号。
3. **[API-CONTRACTS.md](API-CONTRACTS.md)**：由 `scripts/gen_api_contracts.js` 逐函数解析请求体/响应体字段，提供严格的类型契约保证。

---

## 4. 工程质量与协作治理

本项目采用人机共创与多 Agent 协同开发模式，沉淀了严谨的防御性工程规范：

* **[版本状态追踪 (plans/STATUS.md)](plans/STATUS.md)**：版本历史事实、近期演进轮次与核心改进记录。
* **[历史避坑知识库 (errors.md)](errors.md)**：超过 280KB 的技术事故复盘与实战排障数据库，防止重复踩坑。
* **[页面函数完整性清单 (functions-manifest.json)](functions-manifest.json)**：前端与后端交互函数签名基线，防止模板重构意外破坏。
* **[用户反馈与偏好 (feedback.md)](feedback.md)**：收集用户真实使用体验并持续反哺流程优化。

---

## 5. 历史归档与审计 (Archives)

* **[版本与里程碑规划 (`docs/plans/`)](plans/)**：历史版本规划、整顿基线与阶段性验收备忘录。
* **[深度质量分析 (`docs/analysis/`)](analysis/)**：系统质量体检报告、模块体量清单与风险评估。
* **[流程与发版复盘 (`docs/records/` & `docs/process/`)](records/)**：发版验证记录、E2E 测试证据。
