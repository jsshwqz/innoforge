# 模型选型规划书（2026-10-01，数据截止当日）

> 规划会话依用户委托「全蓝星 AI 选型」调研撰写；结论供 InnoForge 的 AI 服务商/模型配置决策与后续派包引用。来源均为当日可访问的官方定价页、模型文档与第三方榜单（文末）。**本文只做选型与登记，不改代码；涉及代码的动作以「微包」形式待用户拍板后另行派发。**

## 1 场景需求（选型的四根尺子）

1. **超长中文上下文**：OA 分析实测 prompt 上限 30 万字符（≈15-20 万 token），复审场景还要叠对比文件全文——上下文 <1M 的模型直接出局；
2. **中文法律/技术文书质量**：复审请求书、意见陈述书是高 formal 文体，非通用聊天；
3. **结构化输出稳定性**：UA5 六固定标题节、UA6 逐断言出处锚点、`fact_check` 结构化键都要求模型守格式；
4. **境内直连 + 便宜**：软件跑在用户本地（Windows），本机代理不常开（2026-10-01 会话实测 `127.0.0.1` 代理进程不在）；用户成本约束为「尽量零成本」。

## 2 候选对照（2026-09-29 榜单 + 各家官方页实测）

| 模型 | 上下文/输出 | 价格（每百万 token） | 境内直连 | 软件现状 | 判断 |
|---|---|---|---|---|---|
| **DeepSeek V4-Pro** | 1M / 最大输出 384K | 输入 4.5 元（缓存命中 **0.15 元**）、输出 13.5 元；**空闲时段（北京时间 9-12/14-18 之外）半价**，周末全天低谷价 | ✅ | **已内置**（默认服务商） | **默认主力，维持** |
| **Kimi K3**（月之暗面） | 1,048,576 / — | 输入 $3（命中 $0.30）、输出 $15；原生 JSON 模式/结构化输出/工具调用，始终推理（`reasoning_effort` 可调） | ✅ | **未内置**（无 Moonshot 端点） | **高风险单件拔高档**（复审分析/终稿），需微包 MP1 |
| Qwen3.8-Max（阿里） | 1M（最大输入 991,808）/ 输出 131,072 | 输入 8-10.5 元、输出 28-33 元（快照档不同） | ✅ | 已内置（dashscope） | 内置档次选 |
| GLM-5.2（智谱） | 1M「无损」 | 当日未取到准确挂牌价 | ✅ | 已内置（open.bigmodel.cn） | 可用备选；**GLM-5.3 API 官方标注「即将上线」，暂不可选** |
| Claude Opus 5 | 1M | $5 / $25 | ❌ 境内 IP 直接触发风控/封号 | 已内置端点但不可直连 | 中文长文质量公认第一档；仅作体验项，不做主力 |
| GPT-6 Astra | 1.05M / 128K | $10 / $50 | ❌ 同上 | 同上 | 同上，且最贵 |
| Meta Muse Spark 1.3 等 2026-09 榜首 | 1.048M | — | ❌/未知 | 未内置 | 榜单分 ≠ 本场景分，不追 |

## 3 结论

1. **默认主力维持 DeepSeek**，模型名切到 `deepseek-v4-pro`（设置页操作，模型列表按服务商 API 实时查询，**零代码**）。它独有三件事：384K 输出上限（一份完整复审请求书 + 出处对照表可单次出完）；0.15 元/百万的缓存命中价（软件每次重发大而稳定的系统前缀 + 材料前缀，命中率天然高）；峰谷半价。
2. **成本估算**：一次完整复审流程（≈60 万输入 + 4 万输出）空闲时段 ≈ **3.3 元**、峰时段 ≈ 6.6 元 ⇒ **deep 档任务排空闲时段/周末跑**（成本减半，属使用建议不属代码）。
3. **拔高档 Kimi K3 走微包 MP1**（见 §4）：同量级一次复审 ≈ $2.4 ≈ 17 元，对「一锤子买卖」的案件完全可接受；不想加端点就用 Qwen3.8-Max 顶。
4. **换模型不解决幻觉**：LongNovel（2026-06，中文长上下文幻觉基准）结论是上下文越长幻觉仍是全模型共性难题，不存在「买一个就不幻觉」的模型 ⇒ UA6 出处锚点 + `fact_check` 是**地板**（软件焊死），模型只决定**天花板**。
5. 海外三强（Claude/GPT/Gemini）在「境内直连 + 零成本」两根尺子上直接淘汰出主力位；若用户日后主动上稳定代理，再按同表重评。

## 4 微包 MP1（可选，**等用户拍板再派**）

- **范围**：`src/config.rs` 服务商清单 + `templates/settings.html` 服务商卡片新增 Moonshot（`api.moonshot.cn`，OpenAI 兼容）；模型列表沿既有「按服务商实时查询」机制；i18n 双语键按 MB4 先例走预审稿。
- **红线**：`Cargo.*`/`migrations`/`common.rs`/`lib.rs` 零 diff；改模板则 HTML 扫描 + eslint + e2e 全套；基线对账 `824 + 2N`。
- **验收**：无 Key 环境下服务商卡片渲染 + 模型列表拉取失败如实降级（不伪造列表）；带 Key 冒烟一次 chat 流式。
- **不做**：不换默认服务商（DeepSeek 保持默认）；不做多模型自动路由（那是 M-C 的事）。

## 5 来源

[DeepSeek 官方价格](https://api-docs.deepseek.com/zh-cn/quick_start/pricing/) · [DeepSeek V4-Pro 永久价解析](https://apifox.com/apiskills/deepseek-v4-pro-permanent-price-cut-developers-guide/) · [Kimi K3 价格](https://www.kimi.com/zh-hans/resources/kimi-k3-pricing) · [Qwen3.8-Max 文档](https://help.aliyun.com/zh/model-studio/qwen3-8-max) · [GLM-5.3 文档（API 未上线）](https://docs.bigmodel.cn/cn/guide/models/text/glm-5.3) · [GPT-6 Astra](https://developers.openai.com/api/docs/models/gpt-6-astra) · [2026-09 主流模型榜](https://aiinking.com/article/67938) · [境内接入海外 API 风险实测](https://sevencoloryun.com/blog/china-developer-overseas-llm-api-access-guide-2026/) · [LongNovel 基准](https://arxiv.org/html/2608.18082) · [Kimi K3 公文写作实测](https://yunpan.plus/t/27651-1-1) · [Claude 中文长文实测](https://www.thinpa.com/tutorials/claude-long-form-writing-review/)
