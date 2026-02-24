# Patent Hub Release 验证脚本

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Patent Hub v0.1.0 发布验证" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

$allPassed = $true

# 检查 1: 编译产物
Write-Host "[检查 1/5] 编译产物..." -ForegroundColor Yellow
if (Test-Path "target\release\patent-hub.exe") {
    $size = (Get-Item "target\release\patent-hub.exe").Length / 1MB
    Write-Host "✓ 可执行文件存在 ($([math]::Round($size, 2)) MB)" -ForegroundColor Green
} else {
    Write-Host "✗ 可执行文件不存在" -ForegroundColor Red
    $allPassed = $false
}

# 检查 2: Git 状态
Write-Host "`n[检查 2/5] Git 状态..." -ForegroundColor Yellow
try {
    git status --short | Out-Null
    Write-Host "✓ Git 仓库正常" -ForegroundColor Green
} catch {
    Write-Host "✗ Git 仓库未初始化" -ForegroundColor Red
    $allPassed = $false
}

# 检查 3: 远程仓库
Write-Host "`n[检查 3/5] 远程仓库..." -ForegroundColor Yellow
$remote = git remote -v 2>$null | Select-String "github.com/jsshwqz/patent-hub"
if ($remote) {
    Write-Host "✓ 远程仓库已配置" -ForegroundColor Green
    git remote -v | ForEach-Object { Write-Host "  $_" -ForegroundColor Gray }
} else {
    Write-Host "✗ 远程仓库未配置" -ForegroundColor Red
    $allPassed = $false
}

# 检查 4: 必需文件
Write-Host "`n[检查 4/5] 必需文件..." -ForegroundColor Yellow
$requiredFiles = @("LICENSE", "README.md", "CONTRIBUTING.md", ".gitignore", "Dockerfile")
foreach ($file in $requiredFiles) {
    if (Test-Path $file) {
        Write-Host "✓ $file" -ForegroundColor Green
    } else {
        Write-Host "✗ $file 缺失" -ForegroundColor Red
        $allPassed = $false
    }
}

# 检查 5: 文档完整性
Write-Host "`n[检查 5/5] 文档完整性..." -ForegroundColor Yellow
$docs = @("docs\INSTALL.md", "docs\QUICK_START.md", "docs\API.md", "docs\ARCHITECTURE.md")
foreach ($doc in $docs) {
    if (Test-Path $doc) {
        Write-Host "✓ $doc" -ForegroundColor Green
    } else {
        Write-Host "✗ $doc 缺失" -ForegroundColor Red
        $allPassed = $false
    }
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan

if ($allPassed) {
    Write-Host "  ✓ 所有检查通过！" -ForegroundColor Green
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "项目状态: 可以发布" -ForegroundColor Green
    Write-Host "仓库地址: https://github.com/jsshwqz/patent-hub" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "下一步:" -ForegroundColor Yellow
    Write-Host "  1. 访问 https://github.com/jsshwqz/patent-hub/releases/new"
    Write-Host "  2. Tag: v0.1.0"
    Write-Host "  3. Title: v0.1.0 - Initial Release"
    Write-Host "  4. 复制 RELEASE_v0.1.0.md 的内容"
    Write-Host "  5. 点击 'Publish release'"
    Write-Host ""
} else {
    Write-Host "  ✗ 验证失败" -ForegroundColor Red
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "请检查上述错误并修复后重试。" -ForegroundColor Red
    Write-Host ""
}
