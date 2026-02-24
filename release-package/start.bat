@echo off
chcp 65001 >nul
echo ========================================
echo    Patent Hub - 专利检索分析系统
echo ========================================
echo.

REM 检查 .env 文件是否存在
if not exist .env (
    echo [警告] 未找到 .env 配置文件
    echo.
    echo 请按以下步骤配置：
    echo 1. 复制 .env.example 为 .env
    echo 2. 编辑 .env 文件，填入你的 API 密钥
    echo.
    echo 按任意键继续使用示例配置...
    pause >nul
    copy .env.example .env >nul 2>&1
)

echo [启动] 正在启动 Patent Hub...
echo.

REM 启动应用
patent-hub.exe

echo.
echo [退出] Patent Hub 已停止
pause
