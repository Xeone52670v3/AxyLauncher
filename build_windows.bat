@echo off
setlocal EnableDelayedExpansion
REM ============================================================================
REM AxyLauncher — Windows Release Binary Builder (.exe)
REM Usage (run on Windows in CMD or PowerShell, or double-click):
REM   build_windows.bat               - Build native Windows release .exe
REM   build_windows.bat --offline     - Build offline using cached crates
REM ============================================================================

cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
    echo [ERROR] Rust / Cargo not found in PATH!
    echo Install Rust from https://rustup.rs ^(with Desktop C++ Build Tools^) and retry.
    exit /b 1
)

set "CARGO_FLAGS=--release"
if "%~1"=="--offline" (
    set "CARGO_FLAGS=--release --offline"
)

echo ==^> Building AxyLauncher for Windows ^(x86_64^)...
cargo build %CARGO_FLAGS%
if errorlevel 1 (
    echo [ERROR] Build failed!
    exit /b 1
)

if not exist "dist\windows" mkdir "dist\windows"
copy /Y "target\release\axylauncher.exe" "dist\windows\AxyLauncher.exe" >nul

echo ==^> Done! Windows executable created at:
echo     %~dp0dist\windows\AxyLauncher.exe
dir "dist\windows\AxyLauncher.exe"
endlocal
