# 创建 GitHub Release 的脚本
# 需要先安装 GitHub CLI: https://cli.github.com/

$tag = "v0.1.0"
$title = "Patent Hub v0.1.0 - 专利检索分析系统"
$notes = @"
## Patent Hub v0.1.0 - 首个公开发布版本

### 功能特性
- ✓ 在线专利搜索 (集成 SerpAPI)
- ✓ 本地 SQLite 数据库存储
- ✓ AI 智能分析 (OpenAI 兼容接口)
- ✓ 专利对比分析
- ✓ 相似专利推荐
- ✓ 文件上传对比
- ✓ 搜索历史管理
- ✓ 统计图表展示
- ✓ Excel 数据导出
- ✓ 跨平台支持 (Windows/Linux/macOS)
- ✓ 移动设备访问支持 (Android/iOS/HarmonyOS)

### 技术栈
- Rust + Axum Web 框架
- SQLite 数据库
- Tera 模板引擎
- 响应式 Web 界面

### 下载说明
1. 下载对应平台的压缩包
2. 解压到任意目录
3. 配置 .env 文件（复制 .env.example）
4. 运行 start.bat (Windows) 或 start.sh (Linux/macOS)
5. 浏览器访问 http://localhost:3000

### 系统要求
- Windows 10/11, Linux, macOS
- 无需安装 Rust 或其他依赖
- 需要网络连接以使用在线搜索功能

### API 密钥获取
- SerpAPI: https://serpapi.com/
- AI API (智谱GLM): https://open.bigmodel.cn/

### 技术支持
- GitHub Issues: https://github.com/jsshwqz/patent-hub/issues
- 文档: https://github.com/jsshwqz/patent-hub/tree/main/docs
"@

Write-Host "Creating GitHub Release..."
Write-Host "Tag: $tag"
Write-Host "Title: $title"
Write-Host ""

# 检查 ZIP 文件是否存在
if (Test-Path "patent-hub-v0.1.0-windows-x86_64.zip") {
    $size = (Get-Item "patent-hub-v0.1.0-windows-x86_64.zip").Length / 1MB
    Write-Host "Found Windows package: $([math]::Round($size, 2)) MB"
    Write-Host ""
    Write-Host "To create the release manually:"
    Write-Host "1. Go to: https://github.com/jsshwqz/patent-hub/releases/new"
    Write-Host "2. Choose tag: v0.1.0"
    Write-Host "3. Set title: $title"
    Write-Host "4. Paste the release notes above"
    Write-Host "5. Upload: patent-hub-v0.1.0-windows-x86_64.zip"
    Write-Host "6. Click 'Publish release'"
} else {
    Write-Host "Error: ZIP file not found!"
}
