# Patent Hub v0.1.0 - 专利检索分析系统

## 首个公开发布版本 🎉

### 功能特性

#### 核心功能
- ✓ **在线专利搜索** - 集成 SerpAPI，支持全球专利数据库检索
- ✓ **本地数据库** - SQLite 存储，快速访问历史数据
- ✓ **AI 智能分析** - OpenAI 兼容接口，支持智谱 GLM 等模型
- ✓ **专利对比** - 多专利并行对比分析
- ✓ **相似推荐** - 智能推荐相关专利
- ✓ **文件上传** - 支持专利文件上传对比
- ✓ **搜索历史** - 完整的搜索历史记录管理
- ✓ **统计图表** - 数据可视化展示
- ✓ **Excel 导出** - 一键导出分析结果

#### 跨平台支持
- ✓ Windows 10/11 (x86_64)
- ✓ Linux (x86_64)
- ✓ macOS (x86_64)
- ✓ 移动设备访问 (Android/iOS/HarmonyOS)

### 技术栈
- **后端**: Rust + Axum Web 框架
- **数据库**: SQLite
- **模板引擎**: Tera
- **前端**: 响应式 HTML/CSS/JavaScript
- **API 集成**: SerpAPI, OpenAI 兼容接口

### 快速开始

#### 1. 下载
选择对应平台的压缩包：
- `patent-hub-windows-x86_64.zip` - Windows 版本
- `patent-hub-linux-x86_64.tar.gz` - Linux 版本
- `patent-hub-macos-x86_64.tar.gz` - macOS 版本

#### 2. 解压
将下载的文件解压到任意目录

#### 3. 配置
复制 `.env.example` 为 `.env`，编辑并填入你的 API 密钥：

```env
SERPAPI_KEY=你的_SerpAPI_密钥
AI_API_KEY=你的_AI_API_密钥
AI_API_BASE=https://open.bigmodel.cn/api/paas/v4
AI_MODEL=glm-4-flash
```

#### 4. 启动
- **Windows**: 双击 `start.bat` 或运行 `patent-hub.exe`
- **Linux/macOS**: 运行 `./start.sh` 或 `./patent-hub`

#### 5. 访问
浏览器打开：
- 本地访问: http://localhost:3000
- 移动设备: http://[你的IP地址]:3000

### API 密钥获取

#### SerpAPI
1. 访问 https://serpapi.com/
2. 注册账号
3. 获取 API Key

#### AI API (智谱 GLM)
1. 访问 https://open.bigmodel.cn/
2. 注册账号
3. 创建 API Key

### 系统要求
- 操作系统: Windows 10/11, Linux (glibc 2.31+), macOS 10.15+
- 内存: 最低 512MB，推荐 1GB+
- 磁盘空间: 100MB+
- 网络: 需要互联网连接以使用在线搜索功能

### 文件说明
- `patent-hub` / `patent-hub.exe` - 主程序
- `start.bat` / `start.sh` - 启动脚本
- `templates/` - HTML 模板文件
- `static/` - 静态资源 (CSS/JS/图片)
- `.env.example` - 环境变量配置示例
- `README.md` - 项目说明文档
- `LICENSE` - MIT 开源协议

### 使用说明

#### 专利搜索
1. 在首页输入关键词
2. 点击"搜索"按钮
3. 查看搜索结果列表
4. 点击专利标题查看详情

#### AI 分析
1. 在专利详情页点击"AI 分析"
2. 等待 AI 生成分析报告
3. 查看技术要点、创新性、应用前景等分析

#### 专利对比
1. 选择多个专利（勾选复选框）
2. 点击"对比分析"按钮
3. 查看并行对比结果

#### 数据导出
1. 在搜索结果页点击"导出 Excel"
2. 下载生成的 Excel 文件
3. 使用 Excel/WPS 等软件打开

### 已知问题
- 首次启动可能需要几秒钟初始化数据库
- AI 分析速度取决于 API 响应时间
- 移动设备需要与电脑在同一局域网

### 技术支持
- **GitHub Issues**: https://github.com/jsshwqz/patent-hub/issues
- **文档**: https://github.com/jsshwqz/patent-hub/tree/main/docs
- **贡献指南**: https://github.com/jsshwqz/patent-hub/blob/main/CONTRIBUTING.md

### 许可证
MIT License - 详见 [LICENSE](https://github.com/jsshwqz/patent-hub/blob/main/LICENSE)

### 更新日志
查看完整更新日志: [CHANGELOG.md](https://github.com/jsshwqz/patent-hub/blob/main/CHANGELOG.md)

---

**发布日期**: 2026-02-24  
**版本**: v0.1.0  
**构建**: Release
