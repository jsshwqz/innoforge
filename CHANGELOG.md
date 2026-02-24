# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

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
