# 简单的 Release 创建脚本
# 使用 GitHub Web 界面

$repo = "jsshwqz/patent-hub"
$tag = "v0.1.0"
$zipFile = "patent-hub-v0.1.0-windows-x86_64.zip"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Patent Hub - 创建 GitHub Release" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# 检查 ZIP 文件
if (Test-Path $zipFile) {
    $size = (Get-Item $zipFile).Length / 1MB
    Write-Host "✓ 找到发布包: $zipFile" -ForegroundColor Green
    Write-Host "  大小: $([math]::Round($size, 2)) MB" -ForegroundColor Gray
} else {
    Write-Host "✗ 未找到发布包: $zipFile" -ForegroundColor Red
    Write-Host "  请先运行构建命令" -ForegroundColor Yellow
    exit 1
}

Write-Host ""
Write-Host "准备创建 Release:" -ForegroundColor Yellow
Write-Host "  仓库: $repo" -ForegroundColor Gray
Write-Host "  标签: $tag" -ForegroundColor Gray
Write-Host "  文件: $zipFile" -ForegroundColor Gray
Write-Host ""

# 方法 1: 使用 GitHub CLI (如果已安装)
Write-Host "尝试方法 1: 使用 GitHub CLI..." -ForegroundColor Cyan
if (Get-Command gh -ErrorAction SilentlyContinue) {
    Write-Host "✓ 检测到 GitHub CLI" -ForegroundColor Green
    Write-Host ""
    Write-Host "执行命令:" -ForegroundColor Yellow
    Write-Host "  gh release create $tag $zipFile --title 'Patent Hub v0.1.0' --notes-file RELEASE_NOTES_v0.1.0.md" -ForegroundColor Gray
    Write-Host ""
    
    $confirm = Read-Host "是否使用 GitHub CLI 创建 Release? (y/n)"
    if ($confirm -eq 'y') {
        gh release create $tag $zipFile `
            --title "Patent Hub v0.1.0 - 专利检索分析系统" `
            --notes-file RELEASE_NOTES_v0.1.0.md
        
        if ($LASTEXITCODE -eq 0) {
            Write-Host ""
            Write-Host "✓ Release 创建成功!" -ForegroundColor Green
            Write-Host "  访问: https://github.com/$repo/releases/tag/$tag" -ForegroundColor Cyan
            exit 0
        } else {
            Write-Host "✗ 创建失败，尝试其他方法..." -ForegroundColor Yellow
        }
    }
}

# 方法 2: 打开浏览器手动创建
Write-Host ""
Write-Host "方法 2: 使用浏览器手动创建" -ForegroundColor Cyan
Write-Host ""
Write-Host "步骤:" -ForegroundColor Yellow
Write-Host "  1. 浏览器将打开 GitHub Release 创建页面" -ForegroundColor Gray
Write-Host "  2. 选择 Tag: v0.1.0" -ForegroundColor Gray
Write-Host "  3. 标题: Patent Hub v0.1.0 - 专利检索分析系统" -ForegroundColor Gray
Write-Host "  4. 描述: 复制 RELEASE_NOTES_v0.1.0.md 的内容" -ForegroundColor Gray
Write-Host "  5. 上传文件: $zipFile" -ForegroundColor Gray
Write-Host "  6. 点击 'Publish release'" -ForegroundColor Gray
Write-Host ""

$confirm = Read-Host "是否打开浏览器? (y/n)"
if ($confirm -eq 'y') {
    # 打开 Release 创建页面
    Start-Process "https://github.com/$repo/releases/new?tag=$tag"
    
    # 打开文件所在目录
    Write-Host ""
    Write-Host "正在打开文件所在目录..." -ForegroundColor Cyan
    Start-Sleep -Seconds 2
    explorer.exe /select,$zipFile
    
    # 在记事本中打开 Release Notes
    Write-Host "正在打开 Release Notes..." -ForegroundColor Cyan
    Start-Sleep -Seconds 1
    notepad.exe RELEASE_NOTES_v0.1.0.md
    
    Write-Host ""
    Write-Host "✓ 已打开相关页面和文件" -ForegroundColor Green
    Write-Host "  请在浏览器中完成 Release 创建" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "完成后访问: https://github.com/$repo/releases" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
