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
timeout /t 2 /nobreak >nul

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

REM ---- smart rebuild: compare binary timestamp with latest git commit ----
if not exist "%BIN_PATH%" goto :build
set "NEED_BUILD=0"
for /f "usebackq" %%i in (`powershell -NoProfile -Command ^
  "$gitTs=(git -C '%~dp0' log -1 --format='%%ct' -- src Cargo.toml 2^>$null ^| Out-String).Trim();" ^
  "if($gitTs){$binTs=[int][double]::Parse((Get-Date -Date (Get-Item '%BIN_PATH%').LastWriteTimeUtc -UFormat %%s));" ^
  "if([long]$gitTs -gt $binTs){Write-Output '1'}else{Write-Output '0'}}else{Write-Output '1'}"`) do set "NEED_BUILD=%%i"
if "%NEED_BUILD%"=="0" (
    echo [InnoForge] Binary is up-to-date, skipping build.
    goto :run
)

:build
REM ---- try debug first (incremental -- fast if no changes), fall back to release ----
if not "%1"=="--release" (
    echo [InnoForge] Building debug, incremental - fast if no changes...
    "%CARGO_EXE%" build --bin innoforge
    if not errorlevel 1 (
        set "BIN_PATH=.\target\debug\innoforge.exe"
        echo [InnoForge] Debug build done.
        goto :run
    )
    echo [InnoForge] Debug build failed, trying release...
)

echo [InnoForge] Building (release mode, optimized)...
"%CARGO_EXE%" build --release --bin innoforge
if errorlevel 1 (
    echo [InnoForge] Build FAILED!
    pause
    exit /b 1
)
set "BIN_PATH=.\target\release\innoforge.exe"

:run
echo [InnoForge] Starting server at http://127.0.0.1:3000
echo [InnoForge] Press Ctrl+C or close window to stop.
echo.
REM Start server in background, wait for it to be ready, then open browser once
start /b "" "%BIN_PATH%" >nul 2>&1
timeout /t 4 /nobreak >nul
start "" http://127.0.0.1:3000 2>nul
echo [InnoForge] Server running. Press Ctrl+C in this window to stop, or close to force-quit.

REM Wait for server to exit
:waitloop
timeout /t 2 /nobreak >nul
tasklist /FI "IMAGENAME eq innoforge.exe" 2>nul | find /i "innoforge.exe" >nul
if errorlevel 1 goto :stopped
goto :waitloop

:stopped
echo [InnoForge] Server stopped.
pause
