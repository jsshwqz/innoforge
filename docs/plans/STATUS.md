# InnoForge 当前状态 / Current Status

> 本文档是**活的**——每个任务完成后必须更新。
> 下一个 Agent 进项目时应先读此文件了解当前焦点。

---

## 当前焦点 / Current Focus

**v0.7.2 — AI 上下文 10 倍扩容 + OA 答复增强规划**

当前版本 v0.7.2（Cargo.toml 已同步）。本版在 v0.7.1 基础上将 AI 上下文截断上限提升 10 倍（专利全文 30 万 / OA 20 万 / 引用 30 万字），让 AI 获取更完整的专利上下文；同时新增 JS 全局错误屏障系统根治白屏黑屏。下一阶段工作重心为 OA 答复增强（docx 导出 / 全流程持久化 / 统一讨论逻辑），详见 `docs/plans/oa-response-enhancement.md`。

v0.7.0（2026-06-26，此前 STATUS 未同步）已落地：
- **AI 对话角色预设系统** + 自定义 system_prompt
- **MCP 服务器新增 3 个专利分析工具**：威胁评估 / 权利要求对照 / 多维对比
- **专利威胁评估 API** `/api/ai/threat-assessment`
- **权利要求对照图表 API** `/api/ai/claim-chart`
- **流水线第 16 步 GenerateOaResponse** + 进度条 15→16
- **Anthropic Claude 服务商**（SSE 流式，base_url 自动检测）+ 模型列表含 sonnet-4-6
- **OA 讨论大幅增强**（AI 主动评估 / 具体修改建议 / 融合讨论生成答复书）
- **OA prompt 深度升级**（多维对比 / 组合动机 / AI 痕迹规避）
- **PDF 逐页提取** + `/api/patent/pdf/extract-text` 端点
- 一批上下文与截断修复（截断字节→字数、上下文限制放宽、DOMPurify 安全包装器、讨论历史对话格式）

### ⚠️ 已知遗留问题
1. **已全部提交**，工作区干净 ✅
2. v0.6.2 / v0.6.3 / v0.7.0 / v0.7.1 未打 git tag（tags 停留在 v0.6.1），v0.7.2 已补打并推送双仓库。如需可补打历史 tag。
3. `experiment::sandbox::tests::test_simple_python_experiment` 因本地无 Python 沙箱环境，cargo test 恒失败（37/38 通过）。

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

**CI 触发：** 推 `dev` / PR 到 `dev` 或 `main` → 自动 `cargo fmt + clippy + test`

**发布：** 推 tag `v*` → GitHub Actions 自动构建 win/linux/macos 三平台二进制并上传 Release

---

## 最近变更 / Recent Changes

| 日期 | 变更 | 类型 |
|------|------|------|
| 2026-07-01 | 切换 GitFlow 分支策略（main/dev/feature）+ CI 更新 + 分离 Gitee remote `4e9ebf9` | chore |
| 2026-07-01 | v0.7.2 正式发布：git tag 补打 + 双仓库推送 + 打包 `innoforge-v0.7.2.zip` | release |
| 2026-07-01 | 商汤模型添加 `sensenova-6.7-flash-lite` + `deepseek-v4-flash`（设置页）`8c1b486` | feat |
| 2026-06-30 | v0.7.2：AI 上下文 10 倍扩容（30K→300K）+ JS 错误屏障修复 + OA 答复增强规划 `c57ed8d` | feat |
| 2026-06-27 | v0.7.1：PDF 提取新增 MinerU 云端 API 兜底（第 6 级降级，中文专利扫描件/复杂版式优化）`4780356` | feat |
| 2026-06-26 | v0.7.0：角色预设/MCP 3 工具/威胁评估+权利要求对照 API/流水线第16步/PDF逐页端点 + Claude 服务商/OA讨论增强/OA prompt升级/上下文截断修复 `e058803` | feat |
| 2026-06-25 | OA prompt 深度升级（多维对比/组合动机/AI 痕迹规避）`df0b3f9` | feat |
| 2026-06-24 | 讨论历史裸JSON→「发明人/AI」对话格式，AI 回复不再跑题 `bf61a28` | fix |
| 2026-06-24 | DOMPurify 安全包装器 + 漏网引用修复 + 专利号/日期自动识别 `084a6b5` | fix |
| 2026-06-24 | 放宽上下文限制：分析 6 万 / 讨论 4 万 / OA 1.5 万字（安全网）`04c31ec` | fix |
| 2026-06-24 | 截断从字节改为字数：30k 中文字不再丢上下文 `060e6e2` | fix |
| 2026-06-24 | OA 讨论大幅增强：AI 主动评估、具体修改、融合讨论生成答复书 `1227eb0` | feat |
| 2026-06-24 | Claude 模型列表添加 sonnet-4-6 `140b61e` | fix |
| 2026-06-24 | 添加 Anthropic Claude 作为 AI 服务商选项 `83330a3` | feat |
| 2026-06-23 | start.bat 闪退彻底修复（纯 ASCII 无乱码 + cargo 路径探测）`775e61b` | fix |
| 2026-06-23 | OA 讨论模式 + SSE 流式 + start.bat 闪退修复 `b178e86` | feat |
| 2026-06-22 | v0.6.3：OA 一审结构化分析（5部分）+ 分段卡片 + 意见陈述书导出 + 代理/超时/DOMPurify/start.bat 修复 `8649b01`/`c250f50` | feat |
| 2026-06-22 | v0.6.2：搜索页 PDF 上传 + 首页文件持久化 + DOMPurify 本地化 + start/dev.bat + clippy + .gitignore | fix |
| 2026-05-25 | DOMPurify 本地化 + start.bat 修复：CDN → `static/purify.min.js` 编译嵌入 | fix |
| 2026-05-23 | v0.6.1：代码质量加固（unwrap消除）+ 聊天分段加载 + Pipeline 15步同步 + CI/CD + 文档治理 | release |
| 2026-05-23 | v0.6.0：OA 答复页面（三类型 + 三档深度 + AI 自检反驳）、通俗表达规则、中国时区、引用按钮 | feat |
| 2026-05-21 | 多 Agent 统一治理：`docs/errors.md` + `docs/plans/STATUS.md` + CLAUDE.md 流程增强 | docs |
| 2026-05-21 | 全面审计修复 18 个 Bug（凭据掩码/OAuth/阻塞调用/unwrap 等） | fix |
| 2026-05-14 | v0.5.9：AI 服务商预设、图片粘贴、专家模型、SerpAPI 余额、对话跨设备同步、CI/CD | feat |
| 2026-05-11 | v0.5.8：SerpAPI 多 Key 轮询、搜索源/AI 精简、AI 聊天持久化、网络错误重试 | feat |
| 2026-05-02 | v0.5.7：Firecrawl 专利兜底搜索 | feat |
| 2026-04-18 | v0.5.6：研发状态机持久化、中途重定向重跑 | feat |

---

## 已知问题 / Known Issues

当前无 HIGH/MEDIUM 未解决项（代码层面）。

### 本轮文档治理已修复（2026-06-28）
- ~~**STATUS.md 落后两个版本**（停留在 v0.6.3，实际已到 v0.7.1）~~ — ✅ 同步至 v0.7.1
- ~~**CHANGELOG v0.6.2 日期错位**（写成 2026-06-26，release commit 实为 06-22）~~ — ✅ 修正为 06-22
- ~~**CHANGELOG v0.7.0 日期错位**（写成 06-25，Cargo bump commit 实为 06-26）~~ — ✅ 修正为 06-26
- ~~**06-24/06-25 一批修复在 CHANGELOG 中无记录**（Claude 服务商/OA讨论增强/截断字数/上下文放宽/DOMPurify安全/讨论历史格式/OA prompt升级）~~ — ✅ 回填进 v0.7.0
- ~~**MinerU 云端 API（4780356）已合入主干却无 CHANGELOG/STATUS 记录、Cargo 未 bump**~~ — ✅ 新增 v0.7.1 条目 + Cargo 0.7.0→0.7.1
- ~~**errors.md 缺 06-24 一批 bug 修复复盘**~~ — ✅ 补 4 条

### 待办（需用户本地执行）
- [ ] 提交工作树 4 个未提交改动（先跑 `cargo fmt && cargo clippy && cargo test`）
- [x] 视需要为 v0.6.2 / v0.6.3 / v0.7.0 / v0.7.1 补打 git tag

### 历史（v0.6.1 已修复）
- ~~生产路径残留 unwrap()~~ — ✅ 消除 14 处
- ~~聊天消息分段加载~~ — ✅ 后端分页 + 前端加载更多
- ~~Pipeline 步骤可视化~~ — ✅ 15 步 + 通用子步骤（v0.7.0 起为 16 步）
- ~~Linux/macOS 安装包~~ — ✅ 全平台自动构建
- ~~Gitee Release 同步~~ — ✅ v0.5.10/v0.6.0 Release + v0.5.9 tag
- ~~根目录冗余文件~~ — ✅ 删除 + .gitignore 防护
- ~~引用按钮缺失~~ — ✅ 覆盖全部 5 个聊天/讨论页面

---

## 下一步 / Next Steps

### v0.7.2 已完成
- [x] AI 上下文 10 倍扩容：`src/ai/patent.rs` 3 函数 6 处（30K→300K / 20K→200K / 60K→600K）✅
- [x] 预览截断 10 倍扩容：`src/routes/ai.rs` 6 函数 10 处（3K→8K→80K / 2K→15K→150K 等）✅
- [x] JS 错误屏障守卫：OA 页 `if (!errored)` 防止误报 ✅
- [x] OA 答复增强规划文档：`docs/plans/oa-response-enhancement.md` ✅

### v0.7.2+ 待开始（详见 OA 答复增强规划）
- [ ] P0-1：Word (docx) 格式意见陈述书导出
- [ ] P0-2：全流程持久化（分析记录→答复草稿版次管理）
- [ ] P0-3：统一前端讨论逻辑（使用通用 `send_chat_stream`）
- [ ] P1：结构化拆条 + 案件时间线 + 逐条超范围检测 + OCR
- [ ] P2：段落级编辑 + 论据动态化
- [ ] P3：协作 + 多轮期限 + 策略统计

### v0.7.1 已完成
- [x] PDF 提取 MinerU 云端 API 兜底（第 6 级降级 `extract_pdf_text_mineru`，OCR+版面还原，中文专利优化）✅ `4780356`
- [x] 文档同步：CHANGELOG/STATUS/Cargo.toml 到 v0.7.1 + errors.md 补录 ✅ 本轮治理

### v0.7.0 已完成
- [x] AI 对话角色预设系统（5 种角色 + 自定义 system_prompt）✅ `e058803`
- [x] MCP 服务器新增 3 个专利分析工具（威胁评估/权利要求对照/多维对比）✅ `e058803`
- [x] 专利威胁评估 API `/api/ai/threat-assessment` ✅ `e058803`
- [x] 权利要求对照图表 API `/api/ai/claim-chart` ✅ `e058803`
- [x] 流水线第 16 步 GenerateOaResponse + 进度条 15→16 ✅ `e058803`
- [x] Anthropic Claude 服务商（SSE 流式，base_url 自动检测）✅ `83330a3`
- [x] Claude 模型列表 sonnet-4-6 ✅ `140b61e`
- [x] OA 讨论大幅增强（AI 主动评估/具体修改/融合讨论生成答复书）✅ `1227eb0`
- [x] OA prompt 深度升级（多维对比/组合动机/AI 痕迹规避）✅ `df0b3f9`
- [x] PDF 逐页提取 + `/api/patent/pdf/extract-text` 端点 ✅ `e058803`
- [x] 截断字节→字数（30k 中文字不再丢上下文）✅ `060e6e2`
- [x] 上下文限制放宽（分析 6 万/讨论 4 万/OA 1.5 万字）✅ `04c31ec`
- [x] DOMPurify 安全包装器 + 漏网引用 + 专利号/日期识别 ✅ `084a6b5`
- [x] 讨论历史裸 JSON →「发明人/AI」对话格式 ✅ `bf61a28`

### v0.6.3 已完成
- [x] OA 一审 Prompt 重写：5 部分结构化输出 ✅ `8649b01`
- [x] 分析结果分段展示 + 意见陈述书草稿导出 ✅ `8649b01`
- [x] 深度模式 critique 范围缩小至第五部分 ✅ `8649b01`
- [x] DOMPurify OA 页 CDN→本地 ✅ `c250f50`
- [x] reqwest `.no_proxy()` 绕过系统代理 ✅ `c250f50`
- [x] HTTP 超时 45s→180s ✅ `c250f50`
- [x] start.bat/dev.bat cargo PATH + 编译优化 ✅ `6b713f7`+`20de7e9`
- [x] OA SSE 流式 + OA 讨论模式 ✅ `b178e86`

### v0.6.2 已完成
- [x] DOMPurify CDN→本地 `/static/purify.min.js`（6 模板）
- [x] 搜索页 PDF 上传 + 首页文件 localStorage 持久化
- [x] start.bat 重写 + dev.bat
- [x] 3 个 clippy 警告修复 + .gitignore 增强

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

*最后更新: 2026-07-01*