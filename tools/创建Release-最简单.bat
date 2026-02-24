@echo off
chcp 65001 >nul
cls
echo.
echo ========================================
echo    Patent Hub - 创建 GitHub Release
echo ========================================
echo.
echo 这是最简单的方法，无需 Token！
echo.
echo ========================================
echo    准备工作
echo ========================================
echo.

REM 检查 ZIP 文件
if not exist "patent-hub-v0.1.0-windows-x86_64.zip" (
    echo [X] 错误: 未找到 ZIP 文件
    echo.
    echo 请确保文件存在:
    echo   patent-hub-v0.1.0-windows-x86_64.zip
    echo.
    pause
    exit /b 1
)

echo [√] ZIP 文件已准备好
for %%A in ("patent-hub-v0.1.0-windows-x86_64.zip") do (
    set size=%%~zA
    set /a sizeMB=!size! / 1048576
)
echo     文件: patent-hub-v0.1.0-windows-x86_64.zip
echo     大小: 3.42 MB
echo.

echo ========================================
echo    Release 信息
echo ========================================
echo.
echo Tag:     v0.1.0
echo 标题:    Patent Hub v0.1.0 - AI 智能专利检索分析系统
echo 类型:    Latest Release
echo.

echo ========================================
echo    操作步骤（共 6 步）
echo ========================================
echo.
echo 1. 浏览器将打开 GitHub Release 创建页面
echo.
echo 2. 在 "Choose a tag" 下拉框中选择: v0.1.0
echo    (如果没有，输入 v0.1.0 并点击 "Create new tag")
echo.
echo 3. 在 "Release title" 输入框中输入:
echo    Patent Hub v0.1.0 - AI 智能专利检索分析系统
echo.
echo 4. 在 "Describe this release" 文本框中:
echo    - 点击文本框
echo    - 按 Ctrl+V 粘贴（说明已复制到剪贴板）
echo.
echo 5. 在 "Attach binaries" 区域:
echo    - 拖拽 patent-hub-v0.1.0-windows-x86_64.zip
echo    - 或点击选择文件
echo.
echo 6. 确保勾选 "Set as the latest release"
echo    然后点击绿色按钮 "Publish release"
echo.
echo ========================================
echo.

REM 复制 Release 说明到剪贴板
type RELEASE_DESCRIPTION.md | clip

echo [√] Release 说明已复制到剪贴板
echo     (在步骤 4 中按 Ctrl+V 粘贴)
echo.

pause

echo.
echo 正在打开浏览器...
echo.

REM 打开 GitHub Release 创建页面
start https://github.com/jsshwqz/patent-hub/releases/new?tag=v0.1.0^&title=Patent+Hub+v0.1.0+-+AI+智能专利检索分析系统

timeout /t 2 /nobreak >nul

echo [√] 浏览器已打开
echo.

REM 打开文件所在目录，方便拖拽
echo 正在打开文件所在目录...
echo.
explorer /select,"patent-hub-v0.1.0-windows-x86_64.zip"

echo [√] 文件夹已打开
echo     (可以直接拖拽 ZIP 文件到浏览器)
echo.

echo ========================================
echo    提示
echo ========================================
echo.
echo - Release 说明已在剪贴板中，直接 Ctrl+V 粘贴
echo - ZIP 文件在打开的文件夹中，拖拽到浏览器
echo - 完成后访问: https://github.com/jsshwqz/patent-hub/releases
echo.
echo ========================================
echo.

pause
