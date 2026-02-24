@echo off
chcp 65001 >nul
title Patent Hub - Release 验证

echo ========================================
echo   Patent Hub v0.1.0 发布验证
echo ========================================
echo.

echo [检查 1/5] 编译产物...
if exist "target\release\patent-hub.exe" (
    echo ✓ 可执行文件存在
    for %%A in ("target\release\patent-hub.exe") do echo   大小: %%~zA 字节
) else (
    echo ✗ 可执行文件不存在
    goto :error
)

echo.
echo [检查 2/5] Git 状态...
git status --short >nul 2>&1
if errorlevel 1 (
    echo ✗ Git 仓库未初始化
    goto :error
) else (
    echo ✓ Git 仓库正常
)

echo.
echo [检查 3/5] 远程仓库...
git remote -v | findstr "github.com/jsshwqz/patent-hub" >nul
if errorlevel 1 (
    echo ✗ 远程仓库未配置
    goto :error
) else (
    echo ✓ 远程仓库已配置
    git remote -v
)

echo.
echo [检查 4/5] 必需文件...
set "files=LICENSE README.md CONTRIBUTING.md .gitignore Dockerfile"
for %%f in (%files%) do (
    if exist "%%f" (
        echo ✓ %%f
    ) else (
        echo ✗ %%f 缺失
        goto :error
    )
)

echo.
echo [检查 5/5] 文档完整性...
set "docs=docs\INSTALL.md docs\QUICK_START.md docs\API.md docs\ARCHITECTURE.md"
for %%d in (%docs%) do (
    if exist "%%d" (
        echo ✓ %%d
    ) else (
        echo ✗ %%d 缺失
        goto :error
    )
)

echo.
echo ========================================
echo   ✓ 所有检查通过！
echo ========================================
echo.
echo 项目状态: 可以发布
echo 仓库地址: https://github.com/jsshwqz/patent-hub
echo.
echo 下一步:
echo   1. 访问 https://github.com/jsshwqz/patent-hub/releases/new
echo   2. Tag: v0.1.0
echo   3. Title: v0.1.0 - Initial Release
echo   4. 复制 RELEASE_v0.1.0.md 的内容
echo   5. 点击 "Publish release"
echo.
pause
exit /b 0

:error
echo.
echo ========================================
echo   ✗ 验证失败
echo ========================================
echo.
echo 请检查上述错误并修复后重试。
echo.
pause
exit /b 1
