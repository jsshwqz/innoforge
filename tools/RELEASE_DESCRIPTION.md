## 🎉 Patent Hub v0.1.1 - 项目结构优化版本

### ✨ 核心特性

#### 🔍 专利检索
- **在线搜索**: 集成 Google Patents，支持全球专利检索
- **本地存储**: SQLite 数据库，快速访问历史数据
- **高级筛选**: 按国家、年份、类型等多维度筛选

#### 🤖 AI 智能分析
- **技术要点分析**: AI 自动提取专利核心技术点
- **专利对比**: 智能对比多个专利的异同
- **文件上传对比**: 独有功能！上传本地专利文件进行对比分析
- **相似推荐**: 基于技术特征推荐相似专利

#### 📊 数据分析与导出
- **统计图表**: 可视化展示专利分布、趋势
- **Excel 导出**: 一键导出检索结果和分析报告
- **搜索历史**: 自动保存搜索记录，方便回溯

#### 🇨🇳 国内用户友好
- **智谱 GLM 支持**: 国内直连，无需代理
- **大部分功能免代理**: AI 分析、对比、本地数据均可直接使用
- **详细配置指南**: 提供国内用户专属配置文档

### 🚀 快速开始

#### Windows 用户（推荐）

1. **下载并解压**
   ```
   下载 patent-hub-v0.1.1-windows-x86_64.zip
   解压到任意目录
   ```

2. **配置环境变量**
   
   复制 `.env.example` 为 `.env`，填入 API 密钥：
   
   ```env
   # 智谱 GLM（推荐，国内直连）
   OPENAI_API_KEY=your_zhipu_api_key
   OPENAI_API_BASE=https://open.bigmodel.cn/api/paas/v4
   
   # SerpAPI（可选，用于在线搜索）
   SERPAPI_KEY=your_serpapi_key
   ```

3. **启动服务**
   ```
   双击运行 start.bat
   或在命令行执行: patent-hub.exe
   ```

4. **访问应用**
   ```
   浏览器打开: http://localhost:3000
   ```

#### 移动设备访问

启动后会显示局域网 IP 地址，手机/平板可通过该地址访问：
```
http://192.168.x.x:3000
```

### 🔑 获取 API 密钥

#### 智谱 GLM（推荐，国内用户）
- 官网: https://open.bigmodel.cn/
- 注册即送免费额度
- 国内直连，速度快
- 价格实惠

#### SerpAPI（可选）
- 官网: https://serpapi.com/
- 用于在线专利搜索
- 免费账户每月 100 次搜索

### 📚 文档

- [中文文档](https://github.com/jsshwqz/patent-hub/blob/main/README.md)
- [English Documentation](https://github.com/jsshwqz/patent-hub/blob/main/README.en.md)
- [国内用户指南](https://github.com/jsshwqz/patent-hub/blob/main/docs/国内用户指南.md)
- [快速开始](https://github.com/jsshwqz/patent-hub/blob/main/docs/QUICK_START.md)
- [API 文档](https://github.com/jsshwqz/patent-hub/blob/main/docs/API.md)

### 💡 核心优势

| 特性 | Patent Hub |
|------|-----------|
| 💰 价格 | 完全免费开源 |
| 🏠 部署方式 | 本地部署，数据完全私有 |
| 🔒 数据隐私 | 所有数据存储在本地 |
| 🤖 AI 分析 | 支持多种 AI 模型 |
| 📊 专利对比 | 智能对比分析 |
| 📁 文件上传对比 | 支持本地文件上传对比 |
| 📥 数据导出 | 无限制导出 Excel |
| 🛠️ 定制开发 | 开源可自由修改 |
| 🇨🇳 国内友好 | 支持智谱 GLM 直连 |

### 🛠️ 技术栈

- **后端**: Rust + Axum（高性能、内存安全）
- **数据库**: SQLite（轻量级、零配置）
- **前端**: HTML + CSS + JavaScript（简洁高效）
- **AI**: OpenAI-compatible API（支持多种模型）
- **搜索**: SerpAPI（Google Patents 集成）

### 📦 系统要求

- **操作系统**: Windows 10/11, Linux, macOS
- **内存**: 最低 512MB，推荐 1GB+
- **磁盘**: 50MB（不含数据库）
- **网络**: 
  - 在线搜索需要访问 Google Patents
  - AI 分析需要访问 AI API（国内可用智谱 GLM）
  - 本地功能无需网络

### 🔧 配置示例

#### 国内用户（推荐）
```env
OPENAI_API_KEY=your_zhipu_api_key
OPENAI_API_BASE=https://open.bigmodel.cn/api/paas/v4
SERPAPI_KEY=your_serpapi_key  # 可选
```

#### 国际用户
```env
OPENAI_API_KEY=sk-...
OPENAI_API_BASE=https://api.openai.com/v1
SERPAPI_KEY=your_serpapi_key
```

### 🐛 已知问题

- 在线搜索在国内需要代理（可使用本地数据和 AI 分析功能）
- 首次启动会创建数据库文件（约 1-2 秒）

### 🗺️ 未来计划

- [ ] 添加更多专利数据源
- [ ] 支持批量导入专利
- [ ] 增强 AI 分析能力
- [ ] 添加专利监控功能
- [ ] 支持团队协作
- [ ] 移动端原生应用

### 🤝 贡献

欢迎提交 Issue 和 Pull Request！

详见 [CONTRIBUTING.md](https://github.com/jsshwqz/patent-hub/blob/main/CONTRIBUTING.md)

### 📄 许可证

MIT License - 详见 [LICENSE](https://github.com/jsshwqz/patent-hub/blob/main/LICENSE)

---

**发布日期**: 2026-02-24  
**版本**: v0.1.1  
**构建**: Windows x86_64  
**文件大小**: 3.42 MB

**下载地址**: [patent-hub-v0.1.1-windows-x86_64.zip](https://github.com/jsshwqz/patent-hub/releases/download/v0.1.1/patent-hub-v0.1.1-windows-x86_64.zip)


