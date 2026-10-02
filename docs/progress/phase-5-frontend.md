# Phase 5 — 前端重构（泳道 B，可与 Phase 3 并行）

> 前置阅读：task-breakdown.md Phase 5 表格；module-inventory.md §5 前端盘点
> 铁律：只搬不写；禁构建工具（原生 ES module）；每次外置一批跑 e2e + check_html_functions

- [ ] T5.1 共享 JS 层 static/js/common.js（ES module）：escapeHtml(×7)/renderMarkdown(×3)/fetchJson/toast/showPdfPreview(×3) 收敛，8 页改 `<script type="module">`
- [ ] T5.2 i18n.js 减负：renderNavbar/renderSidebar/initChatHistory 剥离 layout.js；DOMPurify 全局保护保持在最先加载位（加载顺序契约写入 AGENTS.md）
- [ ] T5.3 OA 页(153KB/90函数) JS 按流程分段外置 static/js/oa/{analyze,discuss,generate,export}.js
- [ ] T5.4 idea 页(111KB) 同法外置
- [ ] T5.5 innerHTML 审计：255 处逐条标注数据来源落盘 docs/analysis/innerHTML-audit.md；用户输入路径显式 sanitize 或 textContent 化；高危清零
- [ ] T5.6 settings/compare/search/detail 页 JS 外置

验收门禁：全套 DoD；重复定义清零；基线 --refresh 随代码提交并说明
Notes:
- 历史事故区（42c726e/9f1a14b 两次误删）：T5.3/T5.4 分段粒度要小，每段一个 commit
- i18n.js 是 ESLint 唯一覆盖文件，剥离后保持其 lint 干净
