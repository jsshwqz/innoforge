@echo off
chcp 65001 >nul
echo ========================================
echo    Patent Hub - 国内用户快速配置
echo ========================================
echo.
echo 本脚本将帮助您快速配置 Patent Hub
echo.
echo ========================================
echo.

REM 检查 .env 文件
if exist .env (
    echo [提示] 检测到已有 .env 配置文件
    echo.
    choice /C YN /M "是否重新配置"
    if errorlevel 2 goto END
    echo.
)

echo 正在创建配置文件...
copy .env.example .env >nul 2>&1

echo.
echo ========================================
echo    配置 AI 服务（智谱 GLM - 国内可用）
echo ========================================
echo.
echo 1. 访问：https://open.bigmodel.cn/
echo 2. 注册账号（支持手机号）
echo 3. 创建 API Key
echo.
set /p AI_KEY="请输入您的智谱 GLM API Key: "

if "%AI_KEY%"=="" (
    echo.
    echo [警告] 未输入 API Key，将使用示例配置
    echo.
) else (
    echo.
    echo 正在配置 AI 服务...
    
    REM 更新 .env 文件
    powershell -Command "(Get-Content .env) -replace 'AI_API_KEY=.*', 'AI_API_KEY=%AI_KEY%' | Set-Content .env"
    powershell -Command "(Get-Content .env) -replace 'AI_API_BASE=.*', 'AI_API_BASE=https://open.bigmodel.cn/api/paas/v4' | Set-Content .env"
    powershell -Command "(Get-Content .env) -replace 'AI_MODEL=.*', 'AI_MODEL=glm-4-flash' | Set-Content .env"
    
    echo ✓ AI 服务配置完成
)

echo.
echo ========================================
echo    配置搜索服务（可选 - 需要代理）
echo ========================================
echo.
echo SerpAPI 用于在线搜索专利，需要代理访问
echo 如果暂时不需要，可以跳过此步骤
echo.
choice /C YN /M "是否配置 SerpAPI"

if errorlevel 2 (
    echo.
    echo [跳过] 未配置 SerpAPI
    echo 您仍可以使用本地数据和 AI 分析功能
    goto FINISH
)

echo.
echo 1. 访问：https://serpapi.com/
echo 2. 注册账号
echo 3. 获取 API Key
echo.
set /p SERP_KEY="请输入您的 SerpAPI Key: "

if "%SERP_KEY%"=="" (
    echo.
    echo [警告] 未输入 SerpAPI Key
    echo.
) else (
    echo.
    echo 正在配置搜索服务...
    powershell -Command "(Get-Content .env) -replace 'SERPAPI_KEY=.*', 'SERPAPI_KEY=%SERP_KEY%' | Set-Content .env"
    echo ✓ 搜索服务配置完成
)

:FINISH
echo.
echo ========================================
echo    配置完成！
echo ========================================
echo.
echo 当前配置：
echo.
if not "%AI_KEY%"=="" (
    echo ✓ AI 服务：已配置（智谱 GLM）
) else (
    echo ✗ AI 服务：未配置
)

if not "%SERP_KEY%"=="" (
    echo ✓ 搜索服务：已配置（需要代理）
) else (
    echo ✗ 搜索服务：未配置
)

echo.
echo ========================================
echo    使用提示
echo ========================================
echo.
echo 1. 如果配置了 AI 服务：
echo    - 可以使用 AI 分析功能
echo    - 可以使用专利对比功能
echo    - 可以使用文件对比功能
echo.
echo 2. 如果配置了搜索服务：
echo    - 需要开启代理才能搜索
echo    - 设置代理：set HTTP_PROXY=http://127.0.0.1:7890
echo.
echo 3. 启动应用：
echo    - 双击 start.bat
echo    - 或运行 patent-hub.exe
echo.
echo ========================================
echo.

choice /C YN /M "是否现在启动应用"

if errorlevel 2 goto END

echo.
echo 正在启动 Patent Hub...
start.bat

:END
echo.
echo 感谢使用 Patent Hub！
echo.
pause
