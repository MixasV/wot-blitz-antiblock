@echo off
rem WoT Blitz Connector launcher.
rem Finds the built app exe and passes the engine payload dir as a
rem --bin-dir command-line argument: UAC-elevated processes do NOT inherit
rem console environment variables, so arguments are the reliable carrier.
rem Elevation via UAC: WinDivert requires administrator rights.
setlocal
cd /d "%~dp0"

rem Prefer the release build, fall back to the debug build.
set "EXE=%~dp0src-tauri\target\release\wotblitz-connector.exe"
if not exist "%EXE%" set "EXE=%~dp0src-tauri\target\debug\wotblitz-connector.exe"

if not exist "%EXE%" (
    echo [!] wotblitz-connector.exe not found.
    echo     Build it first from the wotblitz folder:
    echo         npm install ^&^& npm run tauri build
    pause
    exit /b 1
)

set "BIN=%~dp0bin"

rem Elevate when running without administrator rights.
net session >nul 2>&1
if errorlevel 1 (
    echo Requesting administrator rights via UAC...
    powershell -NoProfile -Command "Start-Process -FilePath '%EXE%' -ArgumentList '-bin-dir','\"%BIN%\"' -Verb RunAs"
    exit /b 0
)

start "" "%EXE%" -bin-dir "%BIN%"
exit /b 0
