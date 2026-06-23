@echo off
cd /d "%~dp0"

REM 确保 cargo 在 PATH 中
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

echo [InnoForge Dev] Building (debug mode, fast)...
cargo build --bin innoforge
if errorlevel 1 (
    echo [InnoForge] Build failed!
    pause
    exit /b 1
)

echo [InnoForge Dev] Launching...
echo [InnoForge Dev] Open http://127.0.0.1:3000 in your browser.

REM Auto-open browser
start "" http://127.0.0.1:3000

".\target\debug\innoforge.exe"
echo.
echo [InnoForge Dev] Server stopped (exit code: %ERRORLEVEL%).
pause
