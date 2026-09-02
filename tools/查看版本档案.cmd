@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0mylist-build.ps1" -Mode List
pause
