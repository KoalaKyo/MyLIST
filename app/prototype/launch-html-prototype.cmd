@echo off
setlocal
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0run-prototype.ps1"
exit /b %ERRORLEVEL%