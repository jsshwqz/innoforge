@echo off
chcp 65001 >nul
echo ========================================
echo    Patent Hub - 创建 GitHub Release
echo ========================================
echo.
echo 正在打开 GitHub Release 创建页面...
echo.
echo 请按照以下步骤操作：
echo.
echo 1. 选择 Tag: v0.1.0
echo 2. 标题: Patent Hub v0.1.0 - 专利检索分析系统
echo 3. 描述: 复制 RELEASE_NOTES_v0.1.0.md 的内容
echo 4. 上传文件: patent-hub-v0.1.0-windows-x86_64.zip
echo 5. 点击 "Publish release"
echo.
echo ========================================
echo.

start https://github.com/jsshwqz/patent-hub/releases/new

echo.
echo 浏览器已打开，请在网页中完成 Release 创建
echo.
pause
