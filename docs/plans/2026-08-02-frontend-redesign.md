# 前端全面重新设计 + Linear 视觉升级计划
# Frontend Redesign & Linear Visual Upgrade Plan

> 日期 / Date: 2026-08-02 · 状态 / Status: ✅ 已完成 / Completed

## 背景 / Background

用户反馈原有前端"太丑"，要求重新设计；多轮迭代后确认方向为 **Linear/Raycast 式高级科技深色**，并将导航从左侧移回右侧。过程中通过 review/security_review 发现并修复 OA 讨论模块的既有问题。

## 任务与完成情况 / Tasks & Completion

### 阶段 1：设计系统重建（精致深色专业风）
- ✅ 重写 `static/style.css` 为统一设计系统（token + 组件库）— `42c726e`
- ✅ 8 个页面模板（index/search/patent_detail/idea/ai/compare/settings/oa-response）重构 HTML 骨架、内联样式收敛 — `42c726e`
- ✅ 侧边导航升级（品牌区 + SVG 图标 + 激活态），`renderSidebar` 重构 — `42c726e`
- ✅ 首页 hero 渐变标题 + 三张模式卡片（对话/快速/深度）— `5f365d6`
- ✅ 统一页面标题区（渐变短横条）应用到 AI/对比/设置/OA 页 — `5f365d6`
- ✅ 首页功能卡入口（AI 推演/专利检索/对比分析）+ 搜索卡片徽章化 + IPC 代码块 — `6bc537a`
- ✅ search 页 Chart.js 改 `async` 加载（CDN 不可达不阻塞页面）— `42c726e`

### 阶段 2：Linear 式高级深色视觉升级
- ✅ 底色 `#08080f`（纯黑带紫调）、表面色/边框同步 — `5465371`
- ✅ 主色 `#6366f1`（靛蓝紫）+ 辅助紫 `#a855f7`/青 `#22d3ee` + 蓝紫渐变 — `5465371`
- ✅ 圆角加大（14/20px）+ 紫调柔光 token — `5465371`
- ✅ 卡片 hover 柔光、主按钮渐变光晕、输入框焦点光环 — `5465371`
- ✅ 清除旧蓝色硬编码残留（rgba(68,147,248)/rgba(77,159,255)）— `d091dd8`
- ✅ 导航移回页面右侧（`.page-sidebar order:1` + border-left）— `5465371`

### 阶段 3：既有问题修复（review 发现）
- ✅ 专利详情导航激活态映射到「搜索」— `56f10e4`
- ✅ OA 论点看板初始隐藏改内联 display — `56f10e4`
- ✅ OA 讨论附件随消息发送（pendingFile 缓存读取）— `e7defd4`
- ✅ 历史讨论 onclick 字符串插值 → data-id + getAttribute — `e7defd4`
- ✅ data-id 属性双引号转义（`escapeAttr`）彻底消除注入面 — `51388ec`
- ✅ onmouseleave 语法修复 + DOM 闭合 — `e7defd4`/`51388ec`
- ✅ CHANGELOG 双语记录（改进 + 修复）— `00c9e9a`

### 阶段 4：验证与发布
- ✅ Puppeteer E2E 48/48（release 实测）
- ✅ `cargo test --lib` 137 passed、`cargo fmt --check`、`cargo clippy -D warnings` 通过
- ✅ security_review / review 均无阻断
- ✅ release 二进制重建（`target/release/innoforge.exe`）

## 已知待办 / Known Follow-ups（不阻塞）

- review Nit：OA 历史讨论 `msgs` 计数拼接未做数字兜底（服务端计数数据源，低风险）
- review Nit：`--cyan` token 未引用、`.sidebar-nav a.active` 指示条方向（`inset 3px` → `-3px` 贴右）
- ✅ 深层防御建议（既有代码）：DOMPurify 兜底弱正则、`escapeHtml` 不转义单引号
- ✅ 创意讨论 TXT 附件正文传入 AI（修复链路断裂）— `b214c65`
- 版本号未提升（仍 v0.7.4 + [Unreleased]）；是否发版由发布流程决策
