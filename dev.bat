@echo off
setlocal enabledelayedexpansion
cd /d "%~dp0"

REM ---- find cargo ----
set "CARGO_EXE="
if exist "%USERPROFILE%\.cargo\bin\cargo.exe" set "CARGO_EXE=%USERPROFILE%\.cargo\bin\cargo.exe"
if not defined CARGO_EXE (
    where cargo >nul 2>&1
    if not errorlevel 1 set "CARGO_EXE=cargo"
)
if not defined CARGO_EXE (
    echo [ERROR] Cannot find cargo.exe. Install Rust: https://rustup.rs
    pause
    exit /b 1
)

echo [InnoForge Dev] Building debug binary (fast)...
"%CARGO_EXE%" build --bin innoforge
if errorlevel 1 (
    echo [InnoForge Dev] Build FAILED!
    pause
    exit /b 1
)

echo [InnoForge Dev] Starting server at http://127.0.0.1:3000
start "" http://127.0.0.1:3000 2>nul
".\target\debug\innoforge.exe"
echo.
echo [InnoForge Dev] Server stopped (exit code: %ERRORLEVEL%).
pause