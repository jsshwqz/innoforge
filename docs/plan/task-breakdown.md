# InnoForge 完整重构任务分解 / Task Breakdown

> 版本：**v4.1（产品主轴版）——依据 prd-v1.md 重排，取代 v3 的结构主轴**
> 策略总纲：**产品能力跃迁优先，结构工作降级为支撑件。** 功能里程碑 M-A→M-B→M-C 串行推进；v3 的结构任务仅保留与功能直接相关者挂载执行，其余归档备查。
> **验收解释基准：`docs/plan/product-vision.md` 九段旅程到位标准**——任何任务的验收标准遇到歧义，以对应旅程段的到位标准为解释；功能取舍以"是否贴近检索员/分析师/代理师三个角色本职"为裁刀。
> 用户决策记录（累计）：①未提交改动审阅后提交✅；②死代码按质量裁决✅；③直接在 dev；④发布 0.1.0 全新版本线；⑤路线 A 重建式重构（结构部分）；⑥ **2026-08-15 方向修正：重构动机是功能不达预期（检索失败/中文给英文/召回不全；分析浅/幻觉/丢上下文），目标形态=个人本地利器，全流程主线全要——一切施工以 PRD 验收口径为准绳**。
> 执行协议、门禁命令、角色分工见 `docs/progress/MASTER.md` 与 `docs/progress/GATES.md`，仍然全部有效。

---

## 里程碑 M-A 可信中文检索（承载 PRD N1-N4）

> **施工规格书：`docs/analysis/search-sources-spec.md`**（统一契约/各源端点与坑位/执行链策略/冒烟清单）——MA1/MA2/MA3/MA5 开工前必读，规格中 ⚠️ 实证项必须逐条附证据。

| # | 任务 | 内容与验收标准 | 依赖 |
|---|------|----------------|------|
| MA1 | SearchProvider 多源框架 | ✅ **2026-09-20 完成**（分支 `exec/ma1-searchprovider`，前置 T1.1 `b08f1c5` + 接手 WIP `07bab1b` + 续建 WIP `fdbaf11`）：落地 SearchProvider trait（原 T2.1）：按规格书§1 契约建模；SerpAPI 迁入为首个实现（从 routes/search.rs 抽出，行为保持）。**证据**：契约六类型 + trait 见 `src/search/model.rs`/`provider.rs`，实现见 `src/search/providers/serpapi.rs`，`routes/search.rs` 1449→820 行、8 个函数迁入单一出处；行为保持由 `#[cfg(test)]` 内的**迁移前参考实现**逐用例对拍（`src/search/query.rs` `test_support::legacy_build_online_query`/`legacy_region_flags`、`src/search/providers/serpapi.rs` `legacy_url_from_q`/`legacy_map`）+ 端点四条终止响应形状快照测试；**门禁复跑（收尾 agent，tip `fdbaf11`，rustc/clippy 1.98.0）**：`cargo fmt --check` exit 0 ｜ `cargo clippy --all-targets -- -D warnings` exit 0 ｜ `cargo test` **457 passed / 0 failed / 3 ignored**（main 基线 369 → 457）｜ 端点形状对 main 逐条比对 **71/71 `.route()` 全等** ｜ templates/ 与 static/ 零改动 ⇒ HTML 扫描与 Puppeteer e2e 不适用；**已开 PR #11 待审**，接手取证与两处测试 bug 处置见 MASTER §5/§6。**⚠️ 下表 MA3 的证据行号已因本次迁移失效**（新位置：`auto_cn`→`src/search/query.rs::resolve_lang`、`country_param`→`src/search/providers/serpapi.rs:61`、`hl/gl/lr`→同文件 `:78`） | T1 地基 |
| MA2 | 备用源接入 ≥2 个 | Google Patents XHR 直抓（免费无 Key，规格书§2，含限速退避）+ EPO OPS（规格书§4）；**验收：人为禁用 SerpAPI 后自动降级仍出结果，诊断面板标红失败源** | MA1 |
| MA3 | 中文/CN 一等公民 | ⚠️ **2026-09-19 实测半程**（证据 `src/routes/search.rs:259` `auto_cn`、`:315` `country_param`——SerpAPI 已带 country 透传与 CN 自动判定），剩余口径见能力提升方案；query 渲染显式 country/language；检索页新增辖区+语言过滤（默认 CN+zh）；**验收：默认配置中文关键词首页以中文专利为主** | MA1 |
| MA4 | 召回增强 | 申请人精确匹配模式；结果强制入库+FTS 自动同步（单一写入口，规格书§7）；**验收：本人姓名可搜到自己名下专利；检索过的专利断网可复检** | MA1 |
| MA5 | 检索诊断面板 | AttemptReport 全量展示（源/状态/耗时/命中/用户可读错误）；前端新增面板；**验收：任一次失败可看到哪个源为什么失败** | MA2 |
| MA6 | SerpAPI 稳健化 | 配额预警接诊断面板、超时分级、错误信息用户可读化、熔断冷却（规格书§1 FailKind） | MA1 |

## 里程碑 M-B 可信深度分析（承载 PRD N5-N8，依赖 M-A 全文获取）

| # | 任务 | 内容与验收标准 | 依赖 |
|---|------|----------------|------|
| MB1 | 幻觉防线激活 | ⚠️ **2026-09-19 实测半程**（证据 `src/routes/ai.rs:1416` `check_oa_analysis` 已在 OA 路径真实接线；`src/pipeline/`、`src/routes/idea.rs` grep 无 fact_check，创意分析仍零调用），剩余口径见能力提升方案；fact_check.rs（设计 24/25 分）接入 OA 分析与创意分析流程；建幻觉拦截用例集；**验收：诱导编造法条/页码的用例全被标记或拒绝** | — |
| MB2 | RAG 重写接线 | rag/ 按 Phase 0.5 裁决重写为独立 service（切片→检索→组装→引用），分析前自动拉取 top-N 专利全文切片；**验收：深度模式报告引用 ≥5 篇专利全文片段** | MA4 |
| MB3 | Embedder 修对 | 统一 TF-IDF 实现（char-boundary 安全修中文 panic + 排序语义缺陷修复），删两份旧拷贝（原 T2.2）；embeddings 写入链贯通 | MA1 |
| MB4 | 上下文治理 | 关键事实结构化传递（专利号/日期/claims 不进摘要压缩）；OA 容量上限改"报错不截断"；**验收：20 轮对话首轮约束仍生效** | — |
| MB5 | 专业 prompt 库 | prompt 从 24+ 散点收拢为 crates 内 prompts 模块（IPC 知识/撰写规范/OA 论证结构分域常量区）；专家模型分工落地（ai_model_expert 用于深分析） | — |
| MB6 | 出处标注体系 | 分析输出强制结构：事实性结论必须附（专利号+段/权号），无源标【推测】；**验收：抽查 10 条 ≥9 条有源** | MB2 |
| MB7 | 决策输出标准化（旅程⑤） | 验证报告末尾强制"决策建议段"：明确回答 申请/放弃/转向 + ≥3 条理由，理由须引用报告内已有证据编号自洽溯源；**验收：三份真实创意报告均给出可执行决定且理由链完整** | MB6 |

## 里程碑 M-C 全流程贯通与交付

| # | 任务 | 内容与验收标准 |
|---|------|----------------|
| MC1 | 检索↔创意验证打通 | 创意流水线的 SearchPatents/PriorArtCluster 步骤改走新多源检索与全文上下文 |
| MC2 | OA 链路复用核查 | OA 分析/讨论/答复书全走 MB1 核查与 MB6 出处体系 |
| MC3 | innerHTML 高危审计 | 仅安全底线项：255 处中涉及用户输入路径的显式 sanitize 或 textContent 化 |
| MC4 | 文档与版本 | API/ARCHITECTURE/AGENTS 同步实际形态；CHANGELOG；发 v0.1.0 |
| MC5 | 交钥匙最后一公里 | delivery-boundary.md 三项：真实冒烟（重点补中文检索场景用例）/文档复核/tag 演练 |

## 结构支撑件（从 v3 挂载，服务而非主导）

| 任务 | 挂载点 | 说明 |
|---|---|---|
| T0 全套（T0.2-T0.9） | **立即** | 已进行中，任何路线都需要 |
| T1.1 类型地基（缩水版） | MA1 前 | 只拆 search/patent/chat/idea 四域 + SearchResult 双定义消灭（types-migration-map.md 仍有效） |
| T1.2 配置上提 + T1.4 unwrap 清零 | MA1 前 | 新代码的地基卫生 |
| T1.3 AppError 启用 | MA1 | 新端点统一错误契约 |
| T2.5 PDF 提取策略化 | MB2 前 | 服务全文获取可靠性 |
| T4.2 db 反向依赖解耦 | MB2 前 | AiCostRecord/Evidence/ResearchState 入 types |

## 归档（v3 中被 v4 取代的任务，留档备查不执行）

T3.0 workspace 物理化、T3.1-T3.6 routes 全面拆分、T5 前端组件化重构（除 MC3 安全底线外）、T6.1 覆盖率工程。理由：个人本地利器形态下，架构美学不解决用户痛点；待功能达标后如需再启动，v3 文档即图纸。
