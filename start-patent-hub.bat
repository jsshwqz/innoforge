@echo off
chcp 65001 >nul 2>nul
cd /d "%~dp0"

REM Kill existing instance if running
taskkill /F /IM patent-hub.exe >nul 2>nul
timeout /t 1 /nobreak >nul

echo [Patent Hub] Starting...
echo [Patent Hub] Building...
cargo build --release --bin patent-hub
if errorlevel 1 (
    echo [Patent Hub] Build failed!
    pause
    exit /b 1
)
echo [Patent Hub] Launching...
.\target\release\patent-hub.exe
pause
