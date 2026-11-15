@echo off
rem EMERGENCY STOP: force-kills EVERY bypass engine on the system, including
rem the original zapret2-gui hidden service (zapret2-svc.exe) that keeps
rem its engine alive after a non-graceful exit.
net session >nul 2>&1 || (
    echo Requesting administrator rights via UAC...
    powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
    exit /b
)
echo Killing ALL bypass processes (engines, apps, hidden svc)...
taskkill /IM winws.exe /F 2>nul
taskkill /IM zapret2-svc.exe /F 2>nul
taskkill /IM zapret2.exe /F 2>nul
taskkill /IM wotblitz-connector.exe /F 2>nul
taskkill /IM vibes-connector.exe /F 2>nul
echo Done. Every bypass engine is stopped.
pause
