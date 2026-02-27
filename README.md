# 🔍 Patent Hub - AI 智能专利检索分析系统

<div align="center">

**一键搜索全球专利 | AI 智能分析 | 专利对比 | 数据导出**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey.svg)](https://github.com/jsshwqz/patent-hub)

</div>

> 📚 **文档导航**: 第一次接触本项目？请从 [**DOCS_INDEX.md**](DOCS_INDEX.md) 开始！
> 
> 🔰 **完整上下文**: 技术决策和设计要求详见 [**PROJECT_CONTEXT.md**](PROJECT_CONTEXT.md)
> 
> 🤖 **AI 助手**: Kiro/Qwen Code 等会自动读取 [**.kiro/agent.md**](.kiro/agent.md)

> 🚀 **专为研发人员、专利工程师、知识产权从业者打造的专利检索神器**
> 💡 支持全球专利数据库搜索、AI 智能分析、专利对比、数据可视化
> 📱 支持电脑、手机、平板多端访问，随时随地查专利

[English](README.en.md) | 简体中文

---

## ✨ 为什么选择 Patent Hub？

- 🎯 **简单易用** - 下载解压即用，无需安装，3 分钟上手
- 🌍 **全球专利** - 接入 Google Patents，覆盖全球专利数据库
- 🤖 **AI 加持** - 智能分析专利技术要点、创新性、应用前景
- 📊 **数据可视化** - 申请趋势图、国家分布、申请人排名一目了然
- 💾 **本地存储** - 搜索历史本地保存，无需担心数据泄露
- 📱 **多端访问** - 支持 Windows/Mac/Linux/手机/平板
- 🆓 **完全免费** - 开源免费，MIT 协议，可商用

## 🎬 快速演示

```
1. 输入关键词 "人工智能" → 2. 点击搜索 → 3. 查看结果 → 4. AI 分析 → 5. 导出 Excel
```

**适用场景**：
- ✅ 技术调研：快速了解某技术领域的专利布局
- ✅ 竞品分析：查看竞争对手的专利申请情况
- ✅ 侵权排查：检索是否存在相似专利
- ✅ 专利撰写：参考同类专利的撰写方式
- ✅ 学术研究：专利数据分析和可视化

---

## 🇨🇳 国内用户使用指南

### ⚠️ 重要提示

本系统使用 **SerpAPI** 访问 Google Patents，在国内使用需要注意以下几点：

### 方案 1：使用代理（推荐）

如果您有代理工具（如 Clash、V2Ray 等），可以设置系统代理后使用：

```bash
# Windows 用户
set HTTP_PROXY=http://127.0.0.1:7890
set HTTPS_PROXY=http://127.0.0.1:7890
patent-hub.exe

# Linux/Mac 用户
export HTTP_PROXY=http://127.0.0.1:7890
export HTTPS_PROXY=http://127.0.0.1:7890
./patent-hub
```

### 方案 2：使用国内 AI 模型（已支持✅）

系统已完美集成**智谱 GLM** AI 模型，完全支持国内访问：

| 功能 | 是否需要代理 | 说明 |
|------|-------------|------|
| AI 分析 | ❌ 不需要 | 使用智谱 GLM，国内直连 |
| 专利对比 | ❌ 不需要 | 使用智谱 GLM，国内直连 |
| 文件对比 | ❌ 不需要 | 使用智谱 GLM，国内直连 |
| 在线搜索 | ⚠️ 需要 | 访问 Google Patents |
| 本地数据 | ❌ 不需要 | 本地 SQLite 数据库 |

### 方案 3：仅使用本地功能

即使无法访问 Google Patents，您仍可以：
- ✅ 查看已保存的专利数据
- ✅ 使用 AI 分析本地专利（智谱 GLM）
- ✅ 上传文件进行 AI 对比
- ✅ 查看搜索历史和统计图表
- ✅ 导出 Excel 数据

### 🔑 API 密钥获取（国内可用）

#### 智谱 GLM（强烈推荐 - 国内可用）

1. 访问：https://open.bigmodel.cn/
2. 使用手机号注册账号
3. 进入控制台创建 API Key
4. 免费额度：新用户赠送一定额度，足够日常使用

**优势**：
- ✅ 国内直连，速度快
- ✅ 中文理解能力强
- ✅ 价格便宜
- ✅ 免费额度充足

#### SerpAPI（需要代理）

1. 访问：https://serpapi.com/
2. 注册账号（支持 Google 登录）
3. 获取 API Key
4. 免费额度：100 次搜索/月

### 📝 配置示例（国内用户推荐）

编辑 `.env` 文件：

```env
# AI 配置（使用智谱 GLM - 国内可用，推荐！）
AI_API_KEY=你的智谱GLM密钥
AI_API_BASE=https://open.bigmodel.cn/api/paas/v4
AI_MODEL=glm-4-flash

# 搜索配置（需要代理或海外服务器）
SERPAPI_KEY=你的SerpAPI密钥
```

### 💡 使用建议

1. **优先使用 AI 功能**：即使无法搜索新专利，也可以对已有专利进行 AI 分析
2. **定期同步数据**：有代理时批量搜索并保存，之后可离线使用
3. **使用本地数据库**：所有搜索结果都会保存到本地，可反复查看
4. **考虑海外服务器**：如果是企业用户，可以部署到海外服务器使用

---

## 功能特性

### 核心功能

✅ **在线专利搜索** - 通过 SerpAPI 搜索 Google Patents（需代理）  
✅ **搜索历史** - 自动保存最近 10 条搜索记录  
✅ **高级筛选** - 按日期范围、国家/地区筛选  
✅ **统计分析** - 申请人 TOP 10、国家分布、申请趋势图  
✅ **导出功能** - 导出 Excel (CSV 格式)  

### AI 功能（支持国内）

✅ **AI 智能分析** - 专利摘要、技术分析（支持智谱 GLM）  
✅ **专利对比** - AI 智能对比两个专利（支持智谱 GLM）  
✅ **相似推荐** - 基于关键词推荐相似专利  
✅ **文件对比** - 上传 TXT 文件与专利进行 AI 对比（支持智谱 GLM）  

---

## 快速开始

### 📥 下载安装

#### 方式 1：下载预编译版本（推荐）

1. 访问 [Releases 页面](https://github.com/jsshwqz/patent-hub/releases/latest)
2. 下载 `patent-hub-v0.1.0-windows-x86_64.zip`
3. 解压到任意目录

#### 方式 2：从源码编译

```bash
git clone https://github.com/jsshwqz/patent-hub.git
cd patent-hub
cargo build --release
```

### ⚙️ 配置

1. 复制配置文件：
```bash
copy .env.example .env
```

2. 编辑 `.env` 文件，填入 API 密钥：
```env
# 智谱 GLM（国内推荐）
AI_API_KEY=你的智谱GLM密钥
AI_API_BASE=https://open.bigmodel.cn/api/paas/v4
AI_MODEL=glm-4-flash

# SerpAPI（可选，需要代理）
SERPAPI_KEY=你的SerpAPI密钥
```

### 🚀 启动

#### Windows 用户

**方式 1：双击启动（最简单）**
```
双击 start.bat 文件
```

**方式 2：命令行启动**
```bash
patent-hub.exe
```

#### Linux/Mac 用户

```bash
chmod +x patent-hub
./patent-hub
```

### 🌐 访问

启动后，浏览器会自动打开，或手动访问：

- **本地访问**：http://localhost:3000
- **移动设备**：http://[你的IP地址]:3000

---

## 📱 移动设备访问

### 手机/平板使用步骤

1. 确保手机和电脑在同一 WiFi 网络
2. 启动程序后，查看显示的 IP 地址
3. 在手机浏览器输入：`http://192.168.x.x:3000`
4. 添加到主屏幕，像 APP 一样使用

**支持的设备**：
- ✅ Android 手机/平板
- ✅ iPhone/iPad
- ✅ 鸿蒙系统设备

---

## 📊 使用示例

### 1. 搜索专利

```
关键词：人工智能
日期范围：2020-01-01 至 2024-12-31
国家：中国
```

### 2. AI 分析

点击任意专利的"AI 分析"按钮，系统会自动分析：
- 技术要点
- 创新性评估
- 应用前景
- 技术优势

### 3. 专利对比

选择两个专利，点击"对比分析"，AI 会对比：
- 技术方案差异
- 创新点对比
- 应用场景对比
- 优劣势分析

### 4. 数据导出

点击"导出 Excel"按钮，下载包含以下信息的表格：
- 专利标题
- 申请号
- 申请人
- 申请日期
- 摘要

---

## 🛠️ 技术栈

- **后端**：Rust + Axum Web 框架
- **数据库**：SQLite
- **模板引擎**：Tera
- **前端**：HTML5 + CSS3 + JavaScript
- **图表**：Chart.js
- **AI 集成**：OpenAI 兼容接口（支持智谱 GLM）
- **搜索 API**：SerpAPI

---

## 📚 文档

- [安装指南](docs/INSTALL.md)
- [快速开始](docs/QUICK_START.md)
- [API 文档](docs/API.md)
- [架构说明](docs/ARCHITECTURE.md)
- [移动设备访问](docs/MOBILE_ACCESS.md)
- [常见问题](docs/FAQ.md)

---

## 🤝 贡献

欢迎贡献代码、报告问题或提出建议！

1. Fork 本仓库
2. 创建特性分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 开启 Pull Request

详见 [贡献指南](CONTRIBUTING.md)

---

## 📄 许可证

本项目采用 MIT 许可证 - 详见 [LICENSE](LICENSE) 文件

---

## 🙏 致谢

- [Rust](https://www.rust-lang.org/) - 系统编程语言
- [Axum](https://github.com/tokio-rs/axum) - Web 框架
- [SerpAPI](https://serpapi.com/) - 搜索 API
- [智谱 AI](https://open.bigmodel.cn/) - AI 模型
- [Chart.js](https://www.chartjs.org/) - 图表库

---

## 📞 联系方式

- **GitHub Issues**：https://github.com/jsshwqz/patent-hub/issues
- **Email**：patent-hub@example.com

---

## ⭐ Star History

如果这个项目对你有帮助，请给个 Star ⭐️

---

**Made with ❤️ by Patent Hub Team**
