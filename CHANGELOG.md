# Changelog / 更新日志

所有重要变更都会记录在此文件中。格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/)。

---

## [v0.3.5] - 2026-03-29

### 新增
- 搜狗搜索内置免费方案，零配置开箱即用（国内无需 VPN 无需 Key）
- Bing Web Search API 支持（国内可用，可选配置）
- Lens.org 专利搜索 API 支持（国内可用，可选配置）
- 搜索降级链：SerpAPI -> Google Patents -> Bing -> Lens.org -> 搜狗免费 -> 本地 DB
- 设置页面新增「国内搜索配置」区域
- 使用免费搜索时自动提示升级到专业搜索

---

## [v0.3.4] - 2026-03-29

### 修复
- APP 端创意分析 AI 失败时降级评分
- Tauri 前端浏览器测试路由
- 文档上传支持 .docx 格式 + GBK 编码处理
- AI 错误提示改善
- Pipeline SSE 时序修复
- 空 query 搜索校验 + 收藏专利 ID 验证
- 收藏标签前端 UI 优化

---

## [v0.3.3] - 2026-03-27

### 新增
- 12 步创新验证流水线（ParseInput -> ScoreNovelty -> AI 分析 -> 报告生成）
- 设置持久化（SQLite 存储，重启不丢失）
- 鸿蒙 APP 构建配置
- 多平台 APP 支持框架
- 全面中文化

### 修复
- 测试断言修复
- 引用准确性提升
- i18n 补全
- Clippy 错误修复

---

## [v0.3.2] - 2026-03-26

### 新增
- 设置页面改造：多 AI 预设配置 + 注册引导 + 自定义支持
- 纯 Rust Android APP 方案（无 Java 依赖）

### 修复
- 设置保存逻辑优化（先更新内存，.env 写入改为可选）
- AI 未配置时显示友好中文提示
- Android APK 使用 cdylib 共享库替代可执行文件
- wry 0.46 API 变更适配

---

## [v0.3.1] - 2026-03-26

### 新增
- Android APP 支持（ARM64 + x86_64 双架构）
- Dioxus Mobile 移动端方案
- 纯 Java WebView 方案（最终采用）

### 修复
- APK 签名路径和上传条件
- Android APP 闪退和图标问题
- 静态文件内嵌二进制（Android 兼容）
- Android 9+ 允许 localhost 明文 HTTP
- CI 构建流程优化

---

## [v0.3.0] - 2026-03-25

### 新增
- IPC 分类浏览 API
- 混合相关性评分算法（TF-IDF + 位置加权）
- Chart.js 可视化统计图表
- 对比矩阵 + 侵权风险评估 UI
- 权利要求分析按钮 + 批量摘要 UI
- PWA 支持（可安装为桌面/移动应用）
- MCP Server（AI Agent 集成）

---

## [v0.2.0] - 2026-03-24

### 新增
- AI 多服务商自动容灾切换（智谱 GLM、OpenRouter、Gemini、OpenAI、NVIDIA、DeepSeek）
- 专利技术附图查看 + 本地图片代理
- PDF 导出（含附图）
- 中英双语国际化（i18n）
- 搜索结果智能去重

---

## [v0.1.0] - 2026-02-24

### 新增
- 在线专利搜索（SerpAPI + Google Patents）
- 本地 SQLite 数据库 + FTS5 全文搜索
- AI 智能分析（OpenAI 兼容 API）
- 专利对比分析
- 相似专利推荐
- 文件上传对比
- 搜索历史管理
- 统计图表展示
- Excel 数据导出
- 跨平台支持（Windows/Linux/macOS）
- 设置页面（网页配置 API Key）
