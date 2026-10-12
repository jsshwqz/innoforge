# OA 答复功能增强开发规划

> 规划日期：2026-07-06 | 最后更新：2026-07-09
> Planning Date: 2026-07-06 | Last Updated: 2026-07-09

---

## 一、项目背景与现状

### 1.1 当前状态
- Patent-Hub 后端（Rust/Axum）已有基础的 OA 答复生成功能
- 前端（templates/）有基本的 OA 答复界面
- AI 模块（ai/）集成了 DeepSeek 等多种 AI 服务商

### 1.2 存在问题
1. **OA 答复质量不足**：当前生成的答复书过于简化，缺乏对审查意见的深入分析
2. **缺少对比分析**：无法有效对比不同权利要求/实施例之间的差异
3. **缺少论点构建**：无法系统性地构建反驳论点
4. **缺少迭代优化**：无法基于审查员的反馈迭代优化答复内容

---

## 二、核心功能规划

### 2.1 AI 驱动的 OA 答复分析流程

```
用户输入 OA 文本
    ↓
AI 深度分析（三步法）
    ├── 第一步：全面解析审查意见
    ├── 第二步：构建反驳论点体系
    └── 第三步：生成结构化答复书
    ↓
用户审查与调整
    ↓
输出最终答复书
```

### 2.2 功能模块设计

| 模块 | 功能 | 优先级 |
|------|------|--------|
| 审查意见解析器 | 提取审查员的核心论点、法条引用、证据 | P0 |
| 论点构建引擎 | 基于专利说明书/权利要求构建反驳逻辑 | P0 |
| 答复书生成器 | 生成符合专利法格式规范的答复书 | P0 |
| 对比分析工具 | 对比不同版本权利要求/实施例的差异 | P1 |
| 答复策略建议 | 基于历史案例提供答复策略 | P2 |

---

## 三、开发计划与里程碑

### 第一阶段：核心功能实现（2026-07-06 ~ 2026-07-10）✅ 已完成

#### M1.1 审查意见深度解析
- [x] 法条识别（专利法第22条、第26条等）
- [x] 证据提取（对比文件D1/D2等）
- [x] 审查员论点结构化提取
- **文件**: `ai/patent.rs` — `generate_oa_analysis()` 整合
- **实际实现**: 三步→一步 prompt 重构，deep mode 支持 300K 上下文

#### M1.2 论点构建与答复生成
- [x] 基于说明书/权利要求构建反驳论点
- [x] 分层反驳策略（核心论点→支持论据→证据链）
- [x] 答复书模板化生成
- **文件**: `ai/patent.rs` — `generate_oa_response_letter()`, `oa_discuss()`

#### M1.3 前端展示优化
- [x] 论点看板组件
- [x] 审查意见高亮标注
- [x] 答复书实时预览
- **文件**: `templates/office_action_response.html`

### 第二阶段：迭代优化（2026-07-10 ~ 2026-07-15）

#### M2.1 历史案例对比
- [ ] 建立历史 OA 答复案例库
- [ ] 相似度匹配算法
- [ ] 策略建议引擎

#### M2.2 答复质量评估
- [ ] AI 自评机制
- [ ] 专家人工评估接口
- [ ] 质量评分系统

### 第三阶段：高级功能（2026-07-15 ~ 2026-07-20）

#### M3.1 批量处理
- [ ] 多案号批量分析
- [ ] 批量答复生成

#### M3.2 协同工作
- [ ] 多人协同编辑
- [ ] 版本控制

---

## 四、技术架构设计

### 4.1 AI 分析流程
```
审查意见原文 + 专利说明书 + 权利要求书
    ↓ (AI 分析)
{
  "summary": "审查意见概述",
  "legal_basis": ["专利法第22条", ...],
  "prior_art": ["D1: 对比文件1", ...],
  "rebuttal_points": [
    {
      "point": "论点1",
      "basis": "法律依据",
      "evidence": ["说明书第X段", ...]
    }
  ],
  "response_letter": "完整答复书文本"
}
```

### 4.2 数据库扩展
```sql
-- OA 分析结果表
CREATE TABLE oa_analyses (
    id SERIAL PRIMARY KEY,
    case_document_id INTEGER REFERENCES case_documents(id),
    analysis_result JSONB,
    version INTEGER DEFAULT 1,
    created_at TIMESTAMP,
    updated_at TIMESTAMP
);

-- 论点库表
CREATE TABLE rebuttal_points (
    id SERIAL PRIMARY KEY,
    case_id INTEGER REFERENCES cases(id),
    point TEXT,
    basis TEXT,
    evidence JSONB,
    status VARCHAR(20) -- 'draft', 'used', 'rejected'
);
```

---

## 五、风险评估与应对

| 风险 | 概率 | 影响 | 应对措施 |
|------|------|------|----------|
| AI 生成内容不准确 | 中 | 高 | 增加人工审核环节，提供编辑功能 |
| 法条引用错误 | 低 | 高 | 建立法条知识库，AI 生成后校验 |
| 响应超时 | 中 | 中 | 分步骤生成，支持异步处理 |

---

## 六、验收标准

### 6.1 功能验收
- [x] 能正确解析审查意见中的法条和证据
- [x] 能生成有逻辑性的反驳论点
- [x] 答复书格式符合专利法规范
- [ ] 对比分析功能正常工作
- [ ] 历史案例推荐准确

### 6.2 性能验收
- [x] 单步 AI 分析 < 60s（普通模式）
- [x] Deep mode 分析 < 5 min
- [ ] 批量处理 < 30s/案

### 6.3 质量验收
- [x] 论点构建准确率 > 80%
- [ ] 答复书通过率 > 70%（模拟审查）
- [ ] 用户满意度 > 4.0/5.0

---

## 七、实际完成记录（2026-07-09 补充）

> 以下内容是规划阶段未预见的紧急修复，已在开发过程中完成。

### 7.1 紧急修复清单

| 问题 | 根因 | 修复 | 文件 |
|------|------|------|------|
| 三步串行 timeout | deepseek 单步 > 60s | 三步→一步 prompt | `ai/patent.rs` |
| 论点看板空白 | `extractArgConclusions` 调用不存在的后端 API | 改为调用 `/api/ai/chat` | `office_action_response.html` |
| AI 讨论超时 | chat 端点硬编码 60s/120s | 移除超时，让 provider 300s 兜底 | `routes/ai.rs` |
| DeepSeek 输出截断 | 默认 max_tokens=4K | 显式设置 16384 | `provider/deepseek.rs` |
| OA 重复调用浪费 Token | 无缓存 | patent_number+oa_type+depth 缓存 | `routes/ai.rs` |
| OA deep mode 输出过长 | 重复 5 段内容 | 精简为 300K 内，去除冗余段落 | `ai/patent.rs` |

### 7.2 当前技术架构（实际）

```
前端: office_action_response.html
  ├── OA 分析：POST /api/ai/oas/{id}/analyze-stream（SSE）
  ├── OA 讨论：POST /api/ai/oas/{id}/discuss-stream（SSE）
  ├── 论点提取：POST /api/ai/chat（本地 AI chat 调用）
  └── 答复生成：POST /api/ai/oas/{id}/generate-letter

后端: routes/ai.rs
  ├── OA 缓存（patent_number + oa_type + depth）
  ├── oa_analysis_stream → generate_oa_analysis（单步）
  └── oa_discuss_stream → oa_discuss（流式）

AI 逻辑: ai/patent.rs
  ├── generate_oa_analysis() — 单步 prompt，deep mode 末尾注入
  ├── generate_oa_response_letter() — 答复书生成
  ├── oa_discuss() — 流式讨论
  └── extract_arg_conclusions() — 论点提取
```

### 7.3 文档同步
- [x] `ARCHITECTURE.md` — 补充 5a 节 OA 模块描述
- [x] `STATUS.md` — 更新 v0.7.4 状态
- [x] `oa-response-enhancement.md` — 本文件同步实际完成记录
