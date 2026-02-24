# 使用 GitHub API 创建 Release
# 需要设置 GITHUB_TOKEN 环境变量

$owner = "jsshwqz"
$repo = "patent-hub"
$tag = "v0.1.0"
$name = "Patent Hub v0.1.0 - 专利检索分析系统"
$body = @"
## Patent Hub v0.1.0 - 首个公开发布版本

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

### 快速开始

#### 1. 下载
下载 Windows 版本: ``patent-hub-v0.1.0-windows-x86_64.zip``

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

### API 密钥获取
- **SerpAPI**: https://serpapi.com/
- **AI API (智谱GLM)**: https://open.bigmodel.cn/

### 系统要求
- Windows 10/11, Linux, macOS
- 内存: 最低 512MB，推荐 1GB+
- 磁盘空间: 100MB+
- 需要网络连接

### 技术支持
- **GitHub Issues**: https://github.com/jsshwqz/patent-hub/issues
- **文档**: https://github.com/jsshwqz/patent-hub/tree/main/docs

### 许可证
MIT License
"@

Write-Host "Creating GitHub Release via API..."
Write-Host "Repository: $owner/$repo"
Write-Host "Tag: $tag"
Write-Host ""

# 检查 GITHUB_TOKEN
if (-not $env:GITHUB_TOKEN) {
    Write-Host "Error: GITHUB_TOKEN environment variable not set!" -ForegroundColor Red
    Write-Host ""
    Write-Host "Please set your GitHub Personal Access Token:"
    Write-Host '  $env:GITHUB_TOKEN = "your_token_here"'
    Write-Host ""
    Write-Host "Or create the release manually at:"
    Write-Host "  https://github.com/$owner/$repo/releases/new"
    exit 1
}

# 创建 Release
$releaseData = @{
    tag_name = $tag
    name = $name
    body = $body
    draft = $false
    prerelease = $false
} | ConvertTo-Json

try {
    $headers = @{
        "Authorization" = "token $env:GITHUB_TOKEN"
        "Accept" = "application/vnd.github.v3+json"
    }
    
    $response = Invoke-RestMethod -Uri "https://api.github.com/repos/$owner/$repo/releases" -Method Post -Headers $headers -Body $releaseData -ContentType "application/json"
    
    Write-Host "✓ Release created successfully!" -ForegroundColor Green
    Write-Host "Release ID: $($response.id)"
    Write-Host "Upload URL: $($response.upload_url)"
    Write-Host ""
    
    # 上传 ZIP 文件
    $zipFile = "patent-hub-v0.1.0-windows-x86_64.zip"
    if (Test-Path $zipFile) {
        Write-Host "Uploading $zipFile..."
        
        $uploadUrl = $response.upload_url -replace '\{\?name,label\}', "?name=$zipFile"
        $zipBytes = [System.IO.File]::ReadAllBytes($zipFile)
        
        $uploadHeaders = @{
            "Authorization" = "token $env:GITHUB_TOKEN"
            "Content-Type" = "application/zip"
        }
        
        $uploadResponse = Invoke-RestMethod -Uri $uploadUrl -Method Post -Headers $uploadHeaders -Body $zipBytes
        
        Write-Host "✓ File uploaded successfully!" -ForegroundColor Green
        Write-Host "Download URL: $($uploadResponse.browser_download_url)"
    } else {
        Write-Host "Warning: $zipFile not found!" -ForegroundColor Yellow
    }
    
    Write-Host ""
    Write-Host "View release at: $($response.html_url)" -ForegroundColor Cyan
    
} catch {
    Write-Host "Error creating release: $_" -ForegroundColor Red
    Write-Host ""
    Write-Host "Please create the release manually at:"
    Write-Host "  https://github.com/$owner/$repo/releases/new"
}
