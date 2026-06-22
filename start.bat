@echo off
chcp 65001 >nul 2>nul
cd /d "%~dp0"

REM 将 Cargo 加入 PATH（若不在系统变量中）
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

REM Kill existing instance if running
taskkill /F /IM innoforge.exe >nul 2>nul
timeout /t 2 /nobreak >nul

REM 总是重新编译，确保二进制与源代码一致
echo [InnoForge] Building (release mode, optimized)...
echo [InnoForge] First build may take 5-10 minutes...
cargo build --release --bin innoforge
if errorlevel 1 (
    echo [InnoForge] Build failed!
    echo [InnoForge] Possible causes:
    echo [InnoForge]   1. Rust/Cargo not installed
    echo [InnoForge]   2. Network issue downloading dependencies
    echo [InnoForge] Try running dev.bat (debug mode) for faster builds.
    echo [InnoForge] Press any key to exit...
    pause
    exit /b 1
)

echo [InnoForge] All set. Launching server...
echo [InnoForge] Open http://127.0.0.1:3000 in your browser.
".\target\release\innoforge.exe"
pause