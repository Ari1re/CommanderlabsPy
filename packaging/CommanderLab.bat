@echo off
rem Commander Lab launcher for Windows. Keeps decks/data next to the exe.
cd /d "%~dp0"
commander-lab.exe %*
if errorlevel 1 pause
