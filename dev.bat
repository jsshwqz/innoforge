@echo off
chcp 65001 >nul 2>nul
cd /d "%~dp0"

echo [InnoForge Dev] Building (debug mode, fast)...
cargo build --bin innoforge
if errorlevel 1 (
    echo [InnoForge] Build failed!
    pause
    exit /b 1
)

echo [InnoForge Dev] Launching...
".\target\debug\innoforge.exe"
pause
