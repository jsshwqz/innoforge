# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.1.2] - 2026-02-27

### Added
- **安全修复**: API 密钥脱敏显示、输入验证、文件锁机制
- **功能修复**: 在线搜索 (SerpAPI) 正常工作、Timestamp 缓存破坏
- **UI 改进**: 所有页面添加设置按钮
- **文档系统**: 完整上下文文档、AI 助手入口、文档导航系统
- **技术决策**: 使用 `dotenv_override()` 强制覆盖环境变量

### Fixed
- 修复 SerpAPI 调用返回本地结果的问题
- 修复前端调用不存在的 `/api/search/local` 端点
- 修复 Timestamp 功能占位符缺失
- 修复 `.env` 文件 BOM 导致解析失败

### Technical
- 添加依赖：`fs2 = "0.4"` (文件锁)、`url = "2.5"` (URL 验证)
- 创建 `.kiro/agent.md` AI 助手入口文档
- 创建 `PROJECT_CONTEXT.md` 完整上下文文档
- 创建 `DOCS_INDEX.md` 文档导航索引

## [0.1.1] - 2026-02-24

### Changed
- 精简项目结构，根目录从 60+ 文件减少到 15 个
- 工具脚本统一移至 `tools/` 目录
- 移除竞品对比内容
- 添加 Gitee 镜像仓库，GitHub ↔ Gitee 双向同步

### Added
- GitHub → Gitee 自动同步 CI 配置
- `tools/` 目录及工具说明文档

## [0.1.0] - 2024-12-24

### Added
- 在线专利搜索 (SerpAPI + Google Patents)
- 本地数据库存储
- AI 智能分析 (支持 OpenAI 兼容 API)
- 专利对比功能
- 相似专利推荐
- 文件上传对比
- 搜索历史记录
- 统计分析图表
- Excel 导出功能
- Windows 开机自启动脚本

### Technical
- Rust + Axum 0.6 后端
- SQLite 数据库
- 原生 HTML/JS 前端
- 支持 Ollama 本地 AI
