# Patent Hub - 创建 GitHub Release 完整指南

## 当前状态

✓ 代码已推送到 GitHub  
✓ CI 全部通过  
✓ Tag v0.1.0 已创建  
✓ Windows 版本已构建  
✓ 发布包已打包: `patent-hub-v0.1.0-windows-x86_64.zip` (3.42 MB)

## 创建 Release 步骤

### 步骤 1: 打开 Release 创建页面

访问: https://github.com/jsshwqz/patent-hub/releases/new

或运行: `.\打开Release页面.bat`

### 步骤 2: 填写 Release 信息

#### Tag
选择或输入: `v0.1.0`

#### Release Title
```
Patent Hub v0.1.0 - 专利检索分析系统
```

#### Description
复制以下内容到描述框：

```markdown
## Patent Hub v0.1.0 - 首个公开发布版本

### 🎉 功能特性

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

### 🚀 快速开始

#### 1. 下载
下载 `patent-hub-v0.1.0-windows-x86_64.zip`

#### 2. 解压并配置
```bash
# 解压文件
unzip patent-hub-v0.1.0-windows-x86_64.zip

# 配置 API 密钥
copy .env.example .env
# 编辑 .env 文件，填入你的 API 密钥
```

#### 3. 启动
```bash
# Windows
start.bat

# 或直接运行
patent-hub.exe
```

#### 4. 访问
浏览器打开: http://localhost:3000

### 🔑 API 密钥获取
- **SerpAPI**: https://serpapi.com/
- **AI API (智谱GLM)**: https://open.bigmodel.cn/

### 💻 系统要求
- 操作系统: Windows 10/11, Linux, macOS
- 内存: 最低 512MB，推荐 1GB+
- 磁盘空间: 100MB+
- 需要网络连接以使用在线搜索功能

### 📦 包含内容
- `patent-hub.exe` - 主程序 (7.61 MB)
- `templates/` - HTML 模板文件
- `static/` - 静态资源 (CSS/JS/图片)
- `.env.example` - 环境变量配置示例
- `README.md` - 项目说明文档
- `LICENSE` - MIT 开源协议
- `start.bat` - 一键启动脚本
- `启动说明.txt` - 使用指南

### 🛠️ 技术栈
- **后端**: Rust + Axum Web 框架
- **数据库**: SQLite
- **模板引擎**: Tera
- **前端**: 响应式 HTML/CSS/JavaScript
- **API 集成**: SerpAPI, OpenAI 兼容接口

### 📚 文档
- [安装指南](https://github.com/jsshwqz/patent-hub/blob/main/docs/INSTALL.md)
- [快速开始](https://github.com/jsshwqz/patent-hub/blob/main/docs/QUICK_START.md)
- [API 文档](https://github.com/jsshwqz/patent-hub/blob/main/docs/API.md)
- [架构说明](https://github.com/jsshwqz/patent-hub/blob/main/docs/ARCHITECTURE.md)
- [移动设备访问](https://github.com/jsshwqz/patent-hub/blob/main/docs/MOBILE_ACCESS.md)

### 🐛 技术支持
- **GitHub Issues**: https://github.com/jsshwqz/patent-hub/issues
- **贡献指南**: https://github.com/jsshwqz/patent-hub/blob/main/CONTRIBUTING.md

### 📄 许可证
MIT License - 详见 [LICENSE](https://github.com/jsshwqz/patent-hub/blob/main/LICENSE)

---

**发布日期**: 2026-02-24  
**版本**: v0.1.0  
**构建**: Release
```

### 步骤 3: 上传文件

点击 "Attach binaries by dropping them here or selecting them" 区域，上传：

- `patent-hub-v0.1.0-windows-x86_64.zip` (位于项目根目录)

### 步骤 4: 发布设置

- ✓ 确保勾选 "Set as the latest release"
- ✓ 不要勾选 "Set as a pre-release"

### 步骤 5: 发布

点击绿色的 "Publish release" 按钮

## 发布后验证

### 1. 检查 Release 页面
访问: https://github.com/jsshwqz/patent-hub/releases

确认:
- ✓ v0.1.0 显示为 "Latest"
- ✓ 下载链接可用
- ✓ 文件大小正确 (3.42 MB)

### 2. 测试下载
```bash
# 下载链接格式
https://github.com/jsshwqz/patent-hub/releases/download/v0.1.0/patent-hub-v0.1.0-windows-x86_64.zip
```

### 3. 测试运行
1. 下载 ZIP 文件
2. 解压到测试目录
3. 配置 .env 文件
4. 运行 start.bat
5. 访问 http://localhost:3000
6. 测试基本功能

## 用户使用流程

用户可以通过以下方式获取和使用：

### 方式 1: 从 Releases 下载 (推荐)
1. 访问 https://github.com/jsshwqz/patent-hub/releases/latest
2. 下载 `patent-hub-v0.1.0-windows-x86_64.zip`
3. 解压并按照说明配置
4. 运行 start.bat

### 方式 2: 直接下载链接
```
https://github.com/jsshwqz/patent-hub/releases/download/v0.1.0/patent-hub-v0.1.0-windows-x86_64.zip
```

### 方式 3: 克隆源码构建
```bash
git clone https://github.com/jsshwqz/patent-hub.git
cd patent-hub
cargo build --release
```

## 常见问题

### Q: 为什么只有 Windows 版本？
A: Linux 和 macOS 版本需要在对应系统上构建。用户可以从源码自行编译，或者等待后续版本。

### Q: 如何更新到新版本？
A: 下载新版本的 ZIP 包，解压到新目录，复制旧的 .env 文件和 patent_hub.db 数据库文件。

### Q: 可以在服务器上运行吗？
A: 可以。确保服务器有网络连接，配置好 API 密钥，然后运行程序。建议使用 systemd 或 supervisor 管理进程。

### Q: 支持 Docker 部署吗？
A: 项目包含 Dockerfile，可以使用 Docker 部署。详见 docs/INSTALL.md。

## 后续计划

- [ ] 添加 Linux 和 macOS 预编译版本
- [ ] 改进 GitHub Actions 自动构建
- [ ] 添加更多语言支持
- [ ] 优化移动端界面
- [ ] 添加更多 AI 模型支持

## 技术支持

如有问题，请：
1. 查看文档: https://github.com/jsshwqz/patent-hub/tree/main/docs
2. 搜索 Issues: https://github.com/jsshwqz/patent-hub/issues
3. 创建新 Issue: https://github.com/jsshwqz/patent-hub/issues/new

---

**文档版本**: 1.0  
**更新日期**: 2026-02-24  
**适用版本**: v0.1.0
