# InnoForge 当前状态 / Current Status

> 本文档是**活的**——每个任务完成后必须更新。
> 下一个 Agent 进项目时应先读此文件了解当前焦点。

---

## 当前焦点 / Current Focus

**v0.7.3 — OA 渲染乱码修复 + 讨论 SSE 增强 + 上下文 10 倍扩容**

当前版本 v0.7.3。本版修复了 OA 页面最深层的 3 个根因问题（DeepSeek max_tokens 截断 → 输出乱码、DOMPurify 崩溃 → JS 不执行、上下文截断 120K→300K），使 OA 分析/讨论/生成答复书完整链路可正常工作。后续任务聚焦 OA 答复增强（docx 导出 / 全流程持久化 / 统一讨论逻辑），详见 `docs/plans/oa-response-enhancement.md`。

### 本版（v0.7.3）已提交
- **OA 渲染乱码修复**：3 个根因解决
  1. `src/ai/chat.rs`: `max_tokens: 16384` 补全，防止 DeepSeek 默认 4K 截断输出乱码
  2. `static/purify.min.js`: 替换损坏版（18KB→完整版 21KB+sourcemap），修复 DOMPurify SyntaxError
  3. `src/ai/patent.rs`: OA 上下文截断从 120K/80K/120K → 300K/200K/300K
- **讨论面板 SSE 解析增强**：统一使用 `event:` 行解析（与 doOAAnalysis 一致），更稳定地处理 SSE error/done 事件
- **讨论传入 `oa_type`**：增强 AI 上下文感知
- **Fusion 多模型辩论引擎**：支持多模型协同辩论、流水线每步独立模型配置、输出格式校验
- **S.U.P.E.R 架构规范**：写入 CLAUDE.md

---

## 分支策略 / Branch Strategy

**2026-07-01 起切换为 GitFlow 简化版：**

| 分支 | 用途 | 保护 |
|------|------|------|
| `main` | 稳定发布版 | ✅ GitHub 受保护，仅 PR 合并 |
| `dev` | 开发主分支，CI 自动跑测试 | 一般 |
| `feature/*` | 功能分支，基于 dev 创建 | 临时 |

**工作流：**
```
用户提需求 → 基于 dev 创建 feature 分支 → 开发 → PR 到 dev → review → 合并 dev
→ 稳定后 PR dev→main → 打 tag → GitHub Actions 自动构建三平台 Release
```

---

## 最近变更 / Recent Changes

| 日期 | 变更 | 类型 |
|------|------|------|
| 2026-07-04 | OA 渲染乱码修复：max_tokens 补全 + DOMPurify 替换 + 上下文扩容 + 讨论 SSE 解析增强 `c39dd20` | fix |
| 2026-07-04 | OA 渲染乱码 HIGH 问题记录 + purify.min.js 修复 `470f265` | docs |
| 2026-07-04 | OA 分析上下文溢出导致乱码 + 诊断日志 + 限制合理化 `5ff5945` | fix |
| 2026-07-04 | clippy needless_borrow 5 warnings in upload.rs `7fc1aec` | chore |
| 2026-07-01 | Fusion 多模型辩论 + 每步模型配置 + 输出格式校验 + S.U.P.E.R 架构规范 `3aa1934` | feat |
| 2026-07-01 | 切换 GitFlow 分支策略（main/dev/feature）+ CI 更新 + 分离 Gitee remote `4e9ebf9` | chore |
| 2026-07-01 | v0.7.2 正式发布：git tag 补打 + 双仓库推送 + 打包 `innoforge-v0.7.2.zip` | release |
| 2026-07-01 | 商汤模型添加 `sensenova-6.7-flash-lite` + `deepseek-v4-flash`（设置页）`8c1b486` | feat |
| 2026-06-30 | v0.7.2：AI 上下文 10 倍扩容（30K→300K）+ JS 错误屏障修复 + OA 答复增强规划 `c57ed8d` | feat |
| 2026-06-27 | v0.7.1：PDF 提取新增 MinerU 云端 API 兜底（第 6 级降级，中文专利扫描件/复杂版式优化）`4780356` | feat |

---

## 已知问题 / Known Issues

### 🟡 MEDIUM — OA 渲染乱码修复后待验证
- 根因已修复：`max_tokens: 16384`（DeepSeek 截断）、`purify.min.js` 完整版（DOMPurify 崩溃）、上下文扩容 120K→300K
- 服务器 PID 4860 运行中
- **待排查**（需在真实浏览器中 F12 → Console 验证）：
  1. 前端 `renderMarkdown()` → `showOAAnalysisDiscuss()` 链路是否有残留字符转换异常
  2. `parseOASections()` 切分后 `s.text` 内容是否正常
  3. SSE 流式 `fullText` 拼接后是否丢失中文
  4. 浏览器缓存旧版编译嵌入的 HTML（Ctrl+F5 强制刷新）

### 🔴 HIGH — OA 分析结果浏览器端显示乱码（已修复）
- ~~现象：API 层（curl 测试）返回正确中文，但用户浏览器显示 `*` 替代中文字符~~ ✅ 已修复
- ~~`max_tokens: 16384` 补全（DeepSeek 截断）、`purify.min.js` 重新下载（SyntaxError 修复）~~ ✅ 已修复

### ⚠️ 已知
- `experiment::sandbox::tests::test_simple_python_experiment` 因本地无 Python 沙箱环境，cargo test 恒失败（37/38 通过）。

---

## 下一步 / Next Steps

### v0.7.3 已完成
- [x] OA 渲染乱码 3 根因修复：max_tokens + DOMPurify + 上下文扩容 `c39dd20` ✅
- [x] 讨论 SSE 解析增强：统一 event: 行处理模式 ✅
- [x] 讨论传入 oa_type 增强上下文感知 ✅
- [x] Fusion 多模型辩论引擎 + 每步模型配置 + 输出格式校验 `3aa1934` ✅
- [x] S.U.P.E.R 架构规范 `3aa1934` ✅

### v0.7.3+ 待开始（详见 OA 答复增强规划）
- [ ] P0-1：Word (docx) 格式意见陈述书导出
- [ ] P0-2：全流程持久化（分析记录→答复草稿版次管理）
- [ ] P0-3：统一前端讨论逻辑（使用通用 `send_chat_stream`）
- [ ] P1：结构化拆条 + 案件时间线 + 逐条超范围检测 + OCR
- [ ] P2：段落级编辑 + 论据动态化
- [ ] P3：协作 + 多轮期限 + 策略统计

### v0.7.2 已完成
- [x] AI 上下文 10 倍扩容：`src/ai/patent.rs` 3 函数 6 处（30K→300K / 20K→200K / 60K→600K）✅
- [x] 预览截断 10 倍扩容：`src/routes/ai.rs` 6 函数 10 处（3K→8K→80K / 2K→15K→150K 等）✅
- [x] JS 错误屏障守卫：OA 页 `if (!errored)` 防止误报 ✅
- [x] OA 答复增强规划文档：`docs/plans/oa-response-enhancement.md` ✅

---

## 版本规划蓝图 / Roadmap

详见 `docs/plans/oa-response-enhancement.md`（OA 答复增强规划，v0.7.2+）
详见 `docs/plans/2026-06-22-v0.6.3-plan.md`（v0.6.3）
详见 `docs/plans/pdf-extraction-enhancement.md`（PDF 提取增强，含 v0.7.0/v0.7.1 落地状态）
详见 `docs/plans/2026-05-23-v0.6.0-plan.md`（v0.6.0）
详见 `docs/plans/研发助手升级规划-多方讨论.md`（长期规划）

---

## 文档索引 / Document Index

| 文档 | 说明 | 必读 |
|------|------|------|
| `CLAUDE.md` | 项目宪法：规范、流程、禁忌 | 每次必读 |
| `CHANGELOG.md` | 版本历史：改了什么、加了什么 | 首次必读 |
| `docs/errors.md` | 错误复盘数据库：以前犯过什么错 | 首次必读 |
| `docs/plans/STATUS.md` | 本文档：当前焦点、下一步 | 每次必读 |
| `docs/plans/oa-response-enhancement.md` | OA 答复增强规划（v0.7.2+ P0-P3） | 需要时读 |
| `docs/ARCHITECTURE.md` | 架构决策记录 | 需要时读 |
| `docs/API.md` | API 文档 | 需要时读 |

---

*最后更新: 2026-07-04*