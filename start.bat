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
    echo [ERROR] If already installed, add %%USERPROFILE%%\.cargo\bin to your system PATH.
    pause
    exit /b 1
)

REM ---- kill existing ----
taskkill /F /IM innoforge.exe >nul 2>&1
timeout /t 1 /nobreak >nul

REM ---- build if needed ----
if exist ".\target\release\innoforge.exe" (
    echo [InnoForge] Using existing binary, skipping build...
    goto :run
)

echo [InnoForge] Building release binary (first time may take 5-10 min)...
"%CARGO_EXE%" build --release --bin innoforge
if errorlevel 1 (
    echo [InnoForge] Build FAILED! Check error messages above.
    echo [InnoForge] Try running dev.bat instead (faster debug build).
    pause
    exit /b 1
)

:run
echo [InnoForge] Starting server at http://127.0.0.1:3000
echo [InnoForge] Press Ctrl+C or close window to stop.
echo.
start "" http://127.0.0.1:3000 2>nul
".\target\release\innoforge.exe"
echo.
echo [InnoForge] Server stopped (exit code: %ERRORLEVEL%).
pause