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
    echo [ERROR] Cannot find cargo.exe. Please install Rust from https://rustup.rs
    pause
    exit /b 1
)

REM ---- kill existing ----
taskkill /F /IM innoforge.exe >nul 2>&1
timeout /t 1 /nobreak >nul

REM ---- check args ----
set "BUILD_MODE="
set "BUILD_DIR="
set "BIN_PATH="
if "%1"=="--release" (
    set BUILD_MODE=--release
    set BUILD_DIR=.\target\release
    set BIN_PATH=.\target\release\innoforge.exe
) else (
    set BUILD_MODE=
    set BUILD_DIR=.\target\debug
    set BIN_PATH=.\target\debug\innoforge.exe
)

REM ---- use existing binary if found ----
if exist "%BIN_PATH%" (
    echo [InnoForge] Using existing %BUILD_DIR% binary, skipping build...
    goto :run
)

REM ---- try debug first (fast), fall back to release ----
if not "%1"=="--release" (
    echo [InnoForge] Building (debug mode, ~1 min)...
    "%CARGO_EXE%" build --bin innoforge
    if not errorlevel 1 (
        set "BIN_PATH=.\target\debug\innoforge.exe"
        echo [InnoForge] Debug build done.
        goto :run
    )
    echo [InnoForge] Debug build failed, trying release...
)

echo [InnoForge] Building (release mode, optimized, ~5 min)...
"%CARGO_EXE%" build --release --bin innoforge
if errorlevel 1 (
    echo [InnoForge] Build FAILED!
    pause
    exit /b 1
)
set "BIN_PATH=.\target\release\innoforge.exe"

:run
echo [InnoForge] Starting server at http://127.0.0.1:3000
echo [InnoForge] Pass --release on command line to force release mode.
echo [InnoForge] Press Ctrl+C or close window to stop.
echo.
start "" http://127.0.0.1:3000 2>nul
"%BIN_PATH%"
echo.
echo [InnoForge] Server stopped (exit code: %ERRORLEVEL%).
pause
