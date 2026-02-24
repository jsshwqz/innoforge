# Patent Hub - Release 准备脚本
# 自动复制 Release 说明到剪贴板并打开浏览器

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "   Patent Hub - Release 准备工具" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# 检查 ZIP 文件
$zipFile = "patent-hub-v0.1.0-windows-x86_64.zip"
if (Test-Path $zipFile) {
    $size = (Get-Item $zipFile).Length / 1MB
    Write-Host "✓ 发布包已准备: $zipFile ($([math]::Round($size, 2)) MB)" -ForegroundColor Green
} else {
    Write-Host "✗ 错误: 未找到 $zipFile" -ForegroundColor Red
    Write-Host "  请先运行构建命令: cargo build --release" -ForegroundColor Yellow
    exit 1
}

Write-Host ""

# Release 说明
$releaseNotes = @"
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
下载 ``patent-hub-v0.1.0-windows-x86_64.zip``

#### 2. 解压并配置
``````bash
# 解压文件
unzip patent-hub-v0.1.0-windows-x86_64.zip

# 配置 API 密钥
copy .env.example .env
# 编辑 .env 文件，填入你的 API 密钥
``````

#### 3. 启动
``````bash
# Windows
start.bat

# 或直接运行
patent-hub.exe
``````

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
- ``patent-hub.exe`` - 主程序 (7.61 MB)
- ``templates/`` - HTML 模板文件
- ``static/`` - 静态资源 (CSS/JS/图片)
- ``.env.example`` - 环境变量配置示例
- ``README.md`` - 项目说明文档
- ``LICENSE`` - MIT 开源协议
- ``start.bat`` - 一键启动脚本
- ``启动说明.txt`` - 使用指南

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
"@

# 复制到剪贴板
$releaseNotes | Set-Clipboard

Write-Host "✓ Release 说明已复制到剪贴板" -ForegroundColor Green
Write-Host ""

# 显示操作步骤
Write-Host "========================================" -ForegroundColor Yellow
Write-Host "   请按照以下步骤创建 Release:" -ForegroundColor Yellow
Write-Host "========================================" -ForegroundColor Yellow
Write-Host ""
Write-Host "1. 浏览器将打开 GitHub Release 创建页面"
Write-Host "2. 选择 Tag: v0.1.0"
Write-Host "3. 标题输入: Patent Hub v0.1.0 - 专利检索分析系统"
Write-Host "4. 描述框中粘贴 (Ctrl+V) - 已自动复制到剪贴板"
Write-Host "5. 点击 'Attach binaries' 上传: $zipFile"
Write-Host "6. 确保勾选 'Set as the latest release'"
Write-Host "7. 点击 'Publish release' 按钮"
Write-Host ""
Write-Host "========================================" -ForegroundColor Yellow
Write-Host ""

# 等待用户确认
Write-Host "按任意键打开浏览器..." -ForegroundColor Cyan
$null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")

# 打开浏览器
Start-Process "https://github.com/jsshwqz/patent-hub/releases/new"

Write-Host ""
Write-Host "✓ 浏览器已打开" -ForegroundColor Green
Write-Host ""
Write-Host "完成后，用户可以从以下地址下载:" -ForegroundColor Cyan
Write-Host "https://github.com/jsshwqz/patent-hub/releases/latest" -ForegroundColor Cyan
Write-Host ""
