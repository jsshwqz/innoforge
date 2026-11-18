# 创研台 InnoForge 高频工程踩坑铁律摘要（≤4KB 常驻注入版）

> **定位**：本文件为 `src/context.rs` 常驻注入精炼版（硬控 ≤4KB）；完整 79 条历史排障档案已归档至 `docs/archive/errors-2026-history.md`。

## 1. Rust 字符串、并发与网络铁律
- **禁裸字节切片（#73）**：严禁对 `&str` 直接写 `&s[..n]` / `&s[n..]`（中文 UTF-8 多字节字符中间切断必 panic）；统一用 `chars().take(n)`、`char_indices()` 或 `safe_truncate`。
- **禁同步阻塞客户端（#55 / #74 / ERR-07）**：异步上下文严禁调用 `reqwest::blocking`；所有 `reqwest::Client` 必须通过 `builder()` 显式设置 `.connect_timeout(...)` 与 `.timeout(...)`（AI 单次调用上限 60s，检索编排改写子步骤设专属短超时 20s，见 #76）。
- **SQLite 并发写与时区（#77）**：开启 WAL + `busy_timeout`；长事务或快照落库通过 `tokio::task::spawn_blocking` 卸载；凡「今日/当日预算」统计必须两侧统一本地时区 `date(timestamp, 'localtime') = date('now', 'localtime')`。
- **生产零 panic**：生产路径严禁 `unwrap()` / `expect()`，API 错误统一转为双语友好 JSON，严禁泄露内部堆栈。

## 2. 防幻觉核验与安全隐私铁律
- **事实与引证硬阻断（#68 / #70 / ERR-02）**：专利段落号/权利要求号、附图部件标号、学术论文真实 DOI/BibTeX、实测实验记录 `Exp:{id}` 必须经确定性代码 O(1) 回查；核查被拒或零证据时**严禁仅打 `warn` 放行**，必须强制降级标注「待人工补证」或硬阻断。
- **第 33 条与公开时序守卫**：权利要求修改必须逐字核验原申请文件依据防超范围；同一技术点同时筹备论文/开源与专利时，未锁专利申请日前严禁无预警导出公开手稿。
- **隐私与默认回环绑定（ERR-01）**：真实在办案件号、申请人姓名、未公开图纸与手稿严禁入 Git / 测试夹具 / CI 日志；桌面端默认仅绑 `127.0.0.1:3000`，非回环绑定强制开启同源 Cookie / Bearer 鉴权，设置接口严禁回显明文 API Key。

## 3. 前端零构建、XSS 与双语对齐铁律
- **零构建与原生组件**：坚持原生 HTML/CSS + ES Modules + Web Components，严禁引入 Webpack / Vite / React 等构建工具。
- **DOMPurify 与安全渲染**：严禁 `innerHTML = 未清洗内容`；优先 `createElement + textContent`，富文本必经全局保护的 `DOMPurify.sanitize()`；禁止原生 `alert` / `confirm` / `prompt`。
- **双语与接线完整性（#56 / #67）**：所有用户可见文案必须在 `static/i18n.js` 中英双语 100% 齐平；修改模板必跑 `scripts/check_html_functions.mjs` 与 `eslint`。

## 4. 测试端口零竞态与契约防漂移铁律
- **消除 `free_port` TOCTOU 竞态（#75 / #78 / ERR-08）**：测试服务与本地 stub 一律绑定 `127.0.0.1:0` 后直接持有并传入已绑定的 `TcpListener` 实例，严禁「先探端口号释放、再二次 bind」。
- **干净测试环境与路由契约（#62 / #79）**：首启/未配置回归测试必须剥离宿主机 `AI_*` 环境变量并隔离临时目录；路由变更必须同步运行 `scripts/gen_route_inventory.js` 与 `scripts/gen_api_contracts.js`。
