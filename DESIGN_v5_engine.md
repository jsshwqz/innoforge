# InnoForge 通用 OA→答复书引擎重构设计方案

> 状态：设计稿，待用户拍板后进入实施
> 日期：2026-10-05
> 前置：v5 终稿已落盘（sha 5b15854e），本案作为回归样本

---

## 0. 问题诊断

当前 `generate_response_letter_stream` 在 `reject_review` 路径上有一个硬编码短路（patent.rs:1409-1418）：检测到 1990999999999/示例申请人/合成技术装置 时，直接返回烤进二进制的 `REEXAM_LETTER_SAMPLE` const（5415字符），不走任何管线。

- 只有本案能走结构化复审书路径，其他案走 AI 自由生成（质量不可控）
- const 把案号、人名、九条特征词、三个公开号、计数结果全部烤进源码
- 换一个专利，引擎什么也做不了——它不抽特征、不计数、不建依赖图、不写反驳段

**目标**：删 const，换确定性管线。输入任意 CaseInput，输出结构正确的答复书。AI 只做自然语言润色和模糊判断，不做结构决策。

---

## 1. CaseInput JSON Schema

```json
{
  "case_meta": {
    "application_number": "1990999999999",
    "applicant": "示例申请人",
    "invention_title": "一种合成技术装置的装置",
    "oa_date": "2026-07-10",
    "oa_type": "reject_review",
    "claim_basis_date": "2026-06-25",
    "claim_basis_text": "1. 一种合成技术装置的装置，其特征在于：..."
  },
  "oa_full_text": "驳回决定正文全文...",
  "reference_documents": [
    {
      "label": "D1",
      "publication_number": "CN990001001A",
      "full_text": "对比文件1原文全文...",
      "extraction_method": "ocr",
      "page_count": 9
    }
  ],
  "claims_under_review": "1. 一种合成技术装置的装置，其特征在于：包括燃气管...",
  "spec_effect_statements": [
    "利用电场引导燃烧的火焰发生形变，提高火焰燃烧速度",
    "通过通风管和过滤机构合成技术装置并提高燃烧效率"
  ]
}
```

### 字段取代关系

| 字段 | 取代哪个 const 内容 |
|------|---------------------|
| case_meta.application_number | const 第1216行硬编码"1990999999999" |
| case_meta.applicant | const 第1218行硬编码"示例申请人" |
| case_meta.invention_title | const 第1217行硬编码"一种合成技术装置的装置" |
| case_meta.oa_date | const 第1223行硬编码"2026年07月10日" |
| oa_full_text | 当前 office_action 参数 |
| reference_documents[] | const 中 D1/D2/D3 公开号和原文 |
| claims_under_review | const 中"权利要求第1项"引用 |
| spec_effect_statements[] | const 中"利用电场引导燃烧的火焰发生形变..."等硬编码效果句 |

**删除**：`REEXAM_LETTER_SAMPLE` 整个 const。
**保留**：`REEXAM_PRE_SUBMIT_CHECKLIST`（程序性固定件，与案无关）、`FACT_DISCIPLINE`、`MECHANISM_ANALYSIS`（通用AI指令）。

---

## 2. src/ai 各模块职责清点

### 2.1 通用逻辑（保留不改）

| 模块 | 行 | 职责 | 通用性 |
|------|----|------|--------|
| extraction.rs | 669 | PDF→文本，术语计数count_term/count_terms，normalize_text | 完全通用 |
| extraction_quality.rs | 262 | 提取方法可靠性标记 | 完全通用 |
| fact_check.rs | 1357 | AI输出事实核查：引文/领域/A33/编造 | 完全通用 |
| citation_check.rs | 222 | 引文与原文比对 | 完全通用 |
| paraphrase_check.rs | 209 | 转述失真检测 | 完全通用 |
| feature_attribution.rs | 243 | 特征归因检测 | 完全通用 |
| zero_evidence.rs | 184 | 零举证检测（泛化用语无引文） | 完全通用 |
| claim_coverage.rs | 232 | 权项覆盖检测 | 完全通用 |
| prohibition_check.rs | 283 | 禁止项检测 | 完全通用 |
| tech_problem_check.rs | 176 | 技术问题检测 | 完全通用 |
| targeted_claims.rs | 229 | 驳回权项定位 | 完全通用 |
| evidence.rs | 381 | 证据自动收集 | 完全通用 |
| adversarial.rs | 535 | 多轮对抗推演 | 完全通用 |
| chat.rs / client.rs / providers.rs | 1749 | AI客户端 | 完全通用 |

### 2.2 本案硬编码（需重构）

| 位置 | 行号 | 内容 | 处理 |
|------|------|------|------|
| patent.rs REEXAM_LETTER_SAMPLE | 1210-1337 | 5415字符完整复审书 | **删除** |
| patent.rs 示例申请人案短路 | 1409-1418 | contains("1990999999999")返回const | **删除** |
| patent.rs REEXAM_PRE_SUBMIT_CHECKLIST | 1196 | 8项提交前清单 | **保留** |
| 各模块 sample_case_* 测试 | 多处 | include_str fixtures | **保留为回归测试** |
| fact_check.rs 测试数据 | 1048-1127 | D1/D3公开号/术语/sha | **保留为测试** |

---

## 3. 管线设计

### 总览

```
CaseInput
  ├─ Stage A: OA区别特征抽取 → Vec<DistinguishingFeature>
  ├─ Stage B: 定性核词计数 → Vec<TermCountCheck> + Vec<UnsupportedClaim>
  ├─ Stage C: 逐特征三段式反驳 → Vec<FeatureRebuttal>
  ├─ Stage D: 作用相同断言对比 → Vec<EffectMismatch>
  ├─ Stage E: 程序性固定件组装 → 完整答复书
  └─ Guard: 输出前校验
```

### Stage A：OA区别特征抽取

**输入**：oa_full_text + claims_under_review
**输出**：Vec<DistinguishingFeature>

每项结构：
```json
{
  "ordinal": 3,
  "feature_name": "上下网状电极发生器+外壁",
  "oa_tag": "容易想到",
  "attributed_ref": "D2",
  "oa_excerpt": "上下网状电极发生器设置于通风管外壁，容易想到（引D2）"
}
```

**方法**：AI单次调用（structured output）。泛化标签枚举：容易想到/常规设计/常规选择/常规调整/惯用手段/常规设置（复用 zero_evidence.rs 已有标签集）。

**为什么用AI**：区别特征识别需语义理解（"通风管方/圆形"是一个特征还是两个），正则不可靠。但结构（序号+标签+归因）是确定的，AI只填空。

### Stage B：定性核词（术语计数）

**输入**：oa_full_text + reference_documents[]
**输出**：Vec<TermCountCheck> + Vec<UnsupportedClaim反驳段>

**方法**：纯确定性，复用 extraction.rs::count_term。

对Stage A每项特征，在其归因对比文件原文中对OA转述术语自动计数。count==0 → 生成"定性无支撑"反驳段+计数表。

输出示例：
```
审查员对D1的"对角安装"定性在D1原文无支撑，属最接近现有技术认定偏差
- "对角"在D1原文出现次数：0次
- "对角安装"在D1原文出现次数：0次
- 驳回决定中"对角"出现1次
【依据：C0实测计数；D1原文全文提取；提取日期：2026-10-05】
```

**不用AI**：计数是确定性操作，AI反而会出错。

### Stage C：逐特征三段式反驳

**输入**：Vec<DistinguishingFeature> + claims_under_review + reference_documents[]
**输出**：Vec<FeatureRebuttal>

**特征依赖图**：从权利要求文本自动建。

建图方法：
1. 从权利要求提取技术特征（顿号分隔的名词短语）
2. 共现关系建边：同一从句中出现的特征互为依赖
3. 输出邻接表

三段式模板（确定性）：
```
①{feature_name}——驳回决定标签"{oa_tag}"。
  【割裂论断】本申请的{feature_name}并非独立存在的常规部件，
  而是与{dependencies}协同构成{system_name}的一部分。
  将其割裂后单独认定为"{oa_tag}"，属于割裂技术特征的错误判断方式。
  【整体协同】（从依赖图+AI填充协同关系）
  【举证不足】驳回决定未分析该特征在{ref_label}中是否被公开、
  结合的技术启示来源以及技术效果是否被启示。
```

**AI仅对"整体协同"段落**：需语义理解的特征（如"陶瓷材质涉及耐高温、绝缘性能与电极发生器配合"），单次批量调用。割裂论断和举证不足用模板确定性生成。

### Stage D：作用相同断言对比

**输入**：oa_full_text + spec_effect_statements[]
**输出**：Vec<EffectMismatch>

**方法**：
1. 正则扫描OA：匹配 `作用相同`/`均为了`/`同样用于`/`等效于`
2. 提取OA声称的效果词
3. 与 spec_effect_statements[] 逐条对比
4. 不匹配 → 生成"机理与效果均不同，无结合动机"反驳段

输出示例：
```
驳回决定引用"作用相同，均为了提高稳焰效果"。
然而，本申请实际效果为"利用电场引导燃烧的火焰发生形变，提高火焰燃烧速度"。
机理与效果均不同于D2，本领域技术人员无结合动机。
```

**AI辅助**：判断OA声称效果与说明书效果句是否"实质不同"需语义理解。正则只做检测触发，AI做对比分析。

### Stage E：程序性固定件组装

**确定性拼接**，不用AI：

1. **抬头**：复审请求书 + 收件单位 + 案号/人名/发明名称/日期（从 case_meta）
2. **复审范围**：请求撤销驳回决定，并发回原审查部门继续审查
3. **论证段**：Stage B反驳段 + Stage C逐特征反驳 + Stage D效果对比
4. **修改说明**：本次不涉及修改（或从讨论记录提取修改内容）
5. **请求事项**：请求撤销 + 发回原审查部门 + 重点审查清单
6. **附件清单**：驳回决定 + 对比文件 + 原审查档案 + C0报告
7. **计数对照表**：从Stage B数据生成markdown表格
8. **提交前清单**：REEXAM_PRE_SUBMIT_CHECKLIST（已有const）

**引文白名单**：仅允许出现的公开号 = reference_documents 中的 publication_number。案号 = case_meta.application_number。

---

## 4. Guard（输出前校验）

三个校验全部通过才输出，否则拒绝并报错：

| 校验 | 方法 | 失败处理 |
|------|------|----------|
| 引文白名单 | grep输出中所有CN/US/EP公开号，与reference_documents白名单比对 | 拒绝，列出不在白名单的公开号 |
| 计数一致性 | 对Stage B每个声明"X出现N次"的断言，重新count_term验证 | 拒绝，列出不一致项 |
| 内部路径/OCR残留 | grep `data/uploads`、`tesseract`、`OcrFallback`、`TextLayer`、`normalize`、uuid正则 | 拒绝，列出残留项 |

实现位置：新模块 `src/ai/output_guard.rs`，在 Stage E 之后、返回给调用方之前调用。

---

## 5. 验收标准

### 5a. 本案v5回归

- 删除 REEXAM_LETTER_SAMPLE const 和示例申请人案短路
- 从 CaseInput（申请号1990999999999）经管线生成答复书
- 生成的答复书与 v5 文件 sha 不要求完全一致（管线生成 vs 手写const，措辞会有差异），但**结构必须一致**：
  - 含九条①-⑨逐段反驳
  - 含D2独立反击段（引"作用相同"）
  - 含D3反驳（卡槽6+卡块8 vs 定位孔/滑槽/滑块/弹簧/定位杆）
  - 结论含"发回原审查部门"
  - grep A9.1=0, data/uploads=0, tesseract=0
- 现有 sample_case_* 回归测试全过

### 5b. 合成第二案

新案：一种散热装置，3特征+2对比文件，全新术语与公开号。

```json
{
  "case_meta": {
    "application_number": "2025108888888",
    "applicant": "张三",
    "invention_title": "一种散热装置",
    "oa_type": "reject_review"
  },
  "oa_full_text": "驳回决定：权利要求1不具备创造性。区别特征：①散热鳍片—常规设计；②热管—容易想到（引D1）；③均温板—常规选择。D1（CN111122233A）公开了热管散热结构...",
  "reference_documents": [
    {"label": "D1", "publication_number": "CN111122233A", "full_text": "..."},
    {"label": "D2", "publication_number": "CN222233344U", "full_text": "..."}
  ],
  "claims_under_review": "1. 一种散热装置，包括散热鳍片、热管、均温板...",
  "spec_effect_statements": ["通过均温板将热量均匀传导至散热鳍片，提高散热效率"]
}
```

验收：引擎输出结构正确（三段式反驳、计数表、结论段），Guard全过。

### 5c. cargo test

全部测试通过（含现有508+测试 + 新增管线测试），贴 `cargo test 2>&1 | tail -20` 输出。

---

## 6. 分阶段实施 + 时长估计

> 华为云按核时计费，每阶段结束即停机等待用户确认，勿空转。

### Phase 1：CaseInput 类型 + 管线骨架（约2核时）

- 新建 `src/ai/case_input.rs`：定义 CaseInput 结构体 + serde
- 新建 `src/ai/response_pipeline.rs`：管线入口函数 `generate_response_letter_pipeline(input: CaseInput) -> String`
- 五个 Stage 函数签名定义（空实现，返回占位文本）
- Guard 模块 `src/ai/output_guard.rs`：空实现
- **不改 patent.rs**，不动现有代码
- cargo build 过

### Phase 2：Stage A + B（确定性核心）（约3核时）

- Stage A：AI调用抽区别特征（复用 chat_with_system）
- Stage B：术语计数（复用 extraction.rs::count_term），生成反驳段+计数表
- 单元测试：用本案 fixtures 验证 Stage A 抽出九条、Stage B 检出"对角"=0
- **此阶段结束即可验证**：Stage A+B 对本案的输出是否正确

### Phase 3：Stage C + D（三段式 + 效果对比）（约3核时）

- 特征依赖图：从权利要求文本建邻接表
- Stage C：三段式模板 + AI填充"整体协同"
- Stage D：正则扫描"作用相同" + AI对比效果句
- 单元测试：用本案验证九条三段式 + D2反击段

### Phase 4：Stage E + Guard + 集成（约2核时）

- Stage E：确定性拼接完整答复书
- Guard：三项校验实现
- 集成：修改 `generate_response_letter_stream`，删除 const 短路，reject_review 路径调管线
- API层：`api_ai_oa_generate_response_letter` 接受 CaseInput JSON
- 本案v5回归测试

### Phase 5：合成第二案 + 全量测试（约1核时）

- 创建第二案 fixtures
- 管线生成第二案答复书，Guard过
- cargo test 全量
- 贴测试输出

**总估计：约11核时**，分5次启动，每次结束停机等确认。

---

## 7. 需用户拍板的点

### 7.1 Stage A 用 AI 抽区别特征 vs 纯正则

我倾向 AI。理由：区别特征的粒度识别（"通风管方/圆形"是一个特征还是两个）需要语义判断，正则要么过度切分要么漏切。但 AI 调用有成本（约2秒/次）和不确定性。

→ **需拍板**：接受 AI 抽特征（有不确定性但更准），还是纯正则（确定性但可能漏/切错）？

### 7.2 v5 回归：sha一致 vs 结构一致

管线生成的答复书措辞不可能与手写 const 逐字一致（模板填充 vs 人工措辞）。我建议验收标准为"结构一致"（九条反驳/计数表/结论段都在），不要求 sha 一致。

→ **需拍板**：接受结构一致验收，还是要求逐字一致（需把管线模板调到与v5逐字匹配，工作量翻倍且脆弱）？

### 7.3 特征依赖图：共现建边 vs AI建边

我倾向共现建边（同一从句中的特征互为依赖）。理由：权利要求文本结构化程度高，顿号分隔的名词短语共现关系已能反映大部分协同关系。但有些协同关系不在同一从句中（如"陶瓷材质"与"网状电极发生器"的绝缘配合）。

→ **需拍板**：共现建边（简单、确定性，可能漏部分协同），还是AI建边（更全但有不确定性）？或者共现建边+AI补充（混合）？

### 7.4 API 兼容性：新端点 vs 改现有端点

现有 `/api/ai/oa-generate-response-letter` 接受 `{oa_type, office_action, analysis, discussion}`。CaseInput 需要更多字段（reference_documents、claims_under_review、spec_effect_statements）。

两个方案：
- **A**：改现有端点，向后兼容（缺字段时从 analysis/office_action 降级提取）
- **B**：新端点 `/api/ai/oa-generate-response-letter-v2`，旧端点不动

我倾向 A（不增加端点数，前端改动小），但需确认旧字段 analysis/discussion 的处理。

→ **需拍板**：改现有端点（A）还是新端点（B）？

### 7.5 我不认同的点：spec_effect_statements 需用户手填 vs 自动提取

用户方案中 spec_effect_statements 是 CaseInput 的输入字段，意味着用户需手填说明书效果句。我认为这应该从说明书全文自动提取（AI一次调用），降低用户负担。

但自动提取有风险：AI可能提取错效果句，导致 Stage D 对比出错。

→ **需拍板**：要求用户手填效果句（准确但增加负担），还是从说明书自动提取（省事但有风险）？或者两者都支持（手填优先，缺则自动提取）？

---

## 8. 风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| Stage A AI抽特征漏切/过度切分 | 后续全部基于特征列表，错误传播 | Guard校验特征数与权利要求部件数匹配；漏切时回退到AI自由生成 |
| Stage C 模板填充措辞生硬 | 答复书可读性下降 | "整体协同"段用AI润色；模板留弹性变量 |
| 第二案无真实OA文本 | 验收可能不够真实 | 合成OA尽量模拟真实驳回决定结构；可后续补真实案 |
| 删除const后旧测试断 | cargo test失败 | sample_case_* 测试改为对管线输出的结构断言，不断sha |
