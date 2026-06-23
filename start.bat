@echo off
cd /d "%~dp0"

REM 将 Cargo 加入 PATH（若不在系统变量中）
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

REM Kill existing instance if running
taskkill /F /IM innoforge.exe >nul 2>nul
timeout /t 1 /nobreak >nul

REM 如果已存在 release 二进制，跳过编译直接启动（快）
if exist ".\target\release\innoforge.exe" (
    echo [InnoForge] Binary exists, skipping build...
    goto :run
)

REM 没有二进制才编译
echo [InnoForge] Building (release mode, optimized)...
echo [InnoForge] This may take 5-10 minutes on first build.
cargo build --release --bin innoforge
if errorlevel 1 (
    echo [InnoForge] Build failed!
    echo [InnoForge] Try running dev.bat (debug mode) for faster builds.
    pause
    exit /b 1
)

:run
echo [InnoForge] Launching server...
echo [InnoForge] Open http://127.0.0.1:3000 in your browser.
echo [InnoForge] Close this window or press Ctrl+C to stop.
echo.

REM Auto-open browser after 2s delay (server startup)
start "" http://127.0.0.1:3000

".\target\release\innoforge.exe"
echo.
echo [InnoForge] Server stopped (exit code: %ERRORLEVEL%).
pause