# Release v0.1.0 - Initial Release

## 🎉 首次发布 / Initial Release

Patent Hub 的第一个公开版本！这是一个基于 Rust + Axum 的开源专利检索与分析系统。

## ✨ 功能特性 / Features

### 核心功能
- ✅ **在线专利搜索** - 通过 SerpAPI 搜索 Google Patents，支持全球专利数据库
- ✅ **本地数据库** - SQLite 本地存储，快速检索历史数据
- ✅ **AI 智能分析** - 支持 Ollama、OpenAI、智谱 GLM 等多种 AI 服务
- ✅ **专利对比** - AI 驱动的专利对比分析，识别异同点
- ✅ **相似推荐** - 基于关键词的相似专利智能推荐
- ✅ **文件对比** - 上传 TXT 文件与专利进行 AI 对比

### 数据分析
- ✅ **搜索历史** - 自动保存最近 10 条搜索记录
- ✅ **高级筛选** - 按日期范围、国家/地区精确筛选
- ✅ **统计分析** - 申请人 TOP 10、国家分布、申请趋势图表
- ✅ **Excel 导出** - 一键导出搜索结果为 CSV 格式

### 跨平台支持
- ✅ **Windows** - 完整支持，包含一键启动脚本
- ✅ **macOS** - 提供安装脚本和文档
- ✅ **Linux** - 提供安装脚本和文档
- ✅ **Docker** - 容器化部署支持
- ✅ **移动设备** - 支持手机/平板浏览器访问

## 🚀 快速开始 / Quick Start

### Windows 用户

1. 下载 Release 中的 `patent-hub-windows.zip`
2. 解压到任意目录
3. 复制 `.env.example` 为 `.env` 并配置 API 密钥
4. 双击 `启动服务器.bat`
5. 浏览器访问 http://127.0.0.1:3000

### 从源码编译

```bash
# 克隆仓库
git clone https://github.com/jsshwqz/patent-hub.git
cd patent-hub

# 配置环境变量
cp .env.example .env
# 编辑 .env 文件配置 API 密钥

# 编译运行
cargo build --release
./target/release/patent-hub
```

### Docker 部署

```bash
docker build -t patent-hub .
docker run -d -p 3000:3000 \
  -v $(pwd)/.env:/app/.env \
  -v $(pwd)/patent_hub.db:/app/patent_hub.db \
  patent-hub
```

## 📋 系统要求 / Requirements

### 运行环境
- **操作系统**: Windows 10+, macOS 10.15+, Linux (Ubuntu 20.04+)
- **内存**: 最低 512 MB，推荐 1 GB
- **磁盘**: 最低 100 MB

### 开发环境
- **Rust**: 1.70 或更高版本
- **Cargo**: 包管理器
- **SQLite**: 3.x（bundled，无需单独安装）

## ⚙️ 配置 / Configuration

### 必需配置

编辑 `.env` 文件：

```env
# AI 服务（必需）
AI_BASE_URL=http://localhost:11434/v1  # Ollama 本地
AI_API_KEY=ollama
AI_MODEL=qwen2.5:7b

# 或使用智谱 GLM（免费）
AI_BASE_URL=https://open.bigmodel.cn/api/paas/v4
AI_API_KEY=your-glm-api-key
AI_MODEL=glm-4-flash
```

### 可选配置

```env
# SerpAPI（用于在线搜索）
SERPAPI_KEY=your-serpapi-key-here
```

获取 API 密钥：
- **SerpAPI**: https://serpapi.com/ （免费 100 次/月）
- **智谱 GLM**: https://open.bigmodel.cn/ （免费无限制）
- **Ollama**: https://ollama.com/ （本地运行，完全免费）

## 📚 文档 / Documentation

- [安装指南](docs/INSTALL.md) - 详细的安装步骤
- [快速开始](docs/QUICK_START.md) - 5 分钟上手指南
- [API 文档](docs/API.md) - 完整的 API 接口文档
- [架构设计](docs/ARCHITECTURE.md) - 系统架构说明
- [移动访问](docs/MOBILE_ACCESS.md) - 手机/平板访问指南
- [移动 APP 开发](docs/MOBILE_APP.md) - 原生 APP 开发指南

## 🛠️ 技术栈 / Tech Stack

- **后端**: Rust 1.70+ + Axum 0.6
- **数据库**: SQLite (rusqlite with bundled feature)
- **AI**: OpenAI-compatible API (Ollama/OpenAI/GLM)
- **搜索**: SerpAPI (Google Patents)
- **前端**: 原生 HTML + JavaScript + CSS
- **异步**: Tokio runtime
- **HTTP**: reqwest (rustls-tls)

## 📊 性能指标 / Performance

- **可执行文件大小**: 7.61 MB
- **启动时间**: < 1 秒
- **内存占用**: ~20 MB（空闲状态）
- **编译时间**: ~45 秒（release 模式）

## 🐛 已知问题 / Known Issues

1. **首次搜索较慢** - SerpAPI 首次请求需要建立连接
2. **AI 分析依赖网络** - 需要稳定的网络连接
3. **移动端 UI** - 当前为响应式 Web，原生 APP 开发中

## 🔜 下一步计划 / Roadmap

### v0.2.0（计划中）
- [ ] 单元测试覆盖
- [ ] 性能优化
- [ ] 用户认证系统
- [ ] PostgreSQL 支持

### v0.3.0（计划中）
- [ ] Flutter 移动 APP
- [ ] 高级搜索语法
- [ ] 专利组合分析
- [ ] 引用网络可视化

## 🤝 贡献 / Contributing

欢迎任何形式的贡献！

- 报告 Bug: [Issues](https://github.com/jsshwqz/patent-hub/issues)
- 功能建议: [Discussions](https://github.com/jsshwqz/patent-hub/discussions)
- 代码贡献: [Pull Requests](https://github.com/jsshwqz/patent-hub/pulls)
- 文档改进: 欢迎提交 PR

详见 [贡献指南](CONTRIBUTING.md)

## 📄 许可证 / License

MIT License - 详见 [LICENSE](LICENSE) 文件

这意味着你可以：
- ✅ 商业使用
- ✅ 修改
- ✅ 分发
- ✅ 私人使用

## 🙏 致谢 / Acknowledgments

感谢以下开源项目：
- [Rust](https://www.rust-lang.org/) - 系统编程语言
- [Axum](https://github.com/tokio-rs/axum) - Web 框架
- [Tokio](https://tokio.rs/) - 异步运行时
- [SQLite](https://www.sqlite.org/) - 嵌入式数据库
- [Ollama](https://ollama.com/) - 本地 AI 平台

## 📞 联系方式 / Contact

- **GitHub**: https://github.com/jsshwqz/patent-hub
- **Issues**: https://github.com/jsshwqz/patent-hub/issues
- **Discussions**: https://github.com/jsshwqz/patent-hub/discussions

---

**感谢使用 Patent Hub！如果觉得有用，请给个 ⭐️ Star！**

**Thank you for using Patent Hub! If you find it useful, please give it a ⭐️ Star!**
