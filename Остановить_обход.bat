@echo off
rem EMERGENCY STOP: force-kills the engine (winws.exe) and the app.
rem Use this if the bypass ever seems to "live" after closing the app.
net session >nul 2>&1 || (
    echo Requesting administrator rights via UAC...
    powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
    exit /b
)
echo Killing bypass processes...
taskkill /IM winws.exe /F 2>nul
taskkill /IM wotblitz-connector.exe /F 2>nul
echo Done. The bypass is fully stopped.
pause
