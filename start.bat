@echo off
chcp 65001 >nul
title Patent Hub ???
echo ========================================
echo    Patent Hub ??????
echo ========================================
echo.
echo ???????...
echo.

cd /d "%~dp0"
if not exist "target\release\patent-hub.exe" (
    echo ??: ????????
    echo ???? build.bat ????
    pause
    exit /b 1
)

echo ?????: http://127.0.0.1:3000
echo.
echo ? Ctrl+C ?????
echo ========================================
echo.

target\release\patent-hub.exe

pause