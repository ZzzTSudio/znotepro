@echo off
setlocal enabledelayedexpansion
chcp 65001 >nul
title znote Pro production build

if exist "%ProgramFiles(x86)%\WiX Toolset v3.14\bin" set "PATH=%ProgramFiles(x86)%\WiX Toolset v3.14\bin;%PATH%"
if exist "%ProgramFiles(x86)%\NSIS" set "PATH=%ProgramFiles(x86)%\NSIS;%PATH%"
if exist "%USERPROFILE%\.cargo\bin" set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

echo ==========================================
echo   znote Pro Windows production build
echo   Mode: release bundle / MSI / NSIS / EXE
echo ==========================================
echo.

where node >nul 2>&1
if errorlevel 1 goto :no_node
where npm >nul 2>&1
if errorlevel 1 goto :no_npm
where cargo >nul 2>&1
if errorlevel 1 goto :no_cargo
where rustc >nul 2>&1
if errorlevel 1 goto :no_rustc

for /f "tokens=*" %%a in ('node -v') do echo [OK] Node.js %%a
for /f "tokens=*" %%a in ('npm -v') do echo [OK] npm %%a
for /f "tokens=*" %%a in ('cargo -V') do echo [OK] %%a
for /f "tokens=*" %%a in ('rustc -V') do echo [OK] %%a
echo.

set NEED_NPM=0
if not exist "node_modules" set NEED_NPM=1
if not exist "package-lock.json" set NEED_NPM=1
if exist "package.json" if exist "node_modules\.package-lock.json" (
    for %%F in (package.json) do for %%G in (node_modules\.package-lock.json) do (
        if %%~tF gtr %%~tG set NEED_NPM=1
    )
)

if "%NEED_NPM%"=="1" (
    echo [1/5] Installing or updating npm dependencies...
    call npm install
    if errorlevel 1 goto :error
) else (
    echo [1/5] npm dependencies are up to date, skipping npm install
)

echo [2/5] Fetching Rust dependencies...
pushd src-tauri
call cargo fetch
if errorlevel 1 (
    popd
    goto :error
)
popd

echo [3/5] Building frontend...
call npm run build
if errorlevel 1 goto :error

if exist "src-tauri\target\release\bundle\msi\*.msi" del /Q "src-tauri\target\release\bundle\msi\*.msi" >nul
if exist "src-tauri\target\release\bundle\nsis\*.exe" del /Q "src-tauri\target\release\bundle\nsis\*.exe" >nul

echo [4/5] Building Tauri release bundles...
call npm run tauri-build
if errorlevel 1 goto :error

echo [5/5] Collecting build artifacts...
set "RELEASE_DIR=src-tauri\target\release"
set "BUNDLE_DIR=%RELEASE_DIR%\bundle"

if not exist "%RELEASE_DIR%" mkdir "%RELEASE_DIR%"
if exist "README.md" copy /Y "README.md" "%RELEASE_DIR%\README.md" >nul
if exist "部署文档.md" copy /Y "部署文档.md" "%RELEASE_DIR%\部署文档.md" >nul

echo.
echo ==========================================
echo             Build completed
echo ==========================================
echo.
echo Artifacts:
if exist "%BUNDLE_DIR%\msi\*.msi" (
    for %%f in ("%BUNDLE_DIR%\msi\*.msi") do echo   [MSI]  %%~nxf
)
if exist "%BUNDLE_DIR%\nsis\*.exe" (
    for %%f in ("%BUNDLE_DIR%\nsis\*.exe") do echo   [NSIS] %%~nxf
)
if exist "%RELEASE_DIR%\znote.exe" (
    echo   [EXE]  znote.exe
    echo.
    echo File info:
    for %%F in ("%RELEASE_DIR%\znote.exe") do (
        echo   Size: %%~zF bytes
        echo   Modified: %%~tF
    )
)
echo.
echo Runtime notes:
echo   [OK] Frontend dist assets are embedded by Tauri
echo   [OK] Rust code is compiled into the executable
echo   [OK] VC++ CRT is configured for static linking
echo   [INFO] WebView2 is required on target Windows systems
echo.
exit /b 0

:no_node
echo [ERROR] Node.js was not found. Install Node.js 18 or newer.
exit /b 1

:no_npm
echo [ERROR] npm was not found. Reinstall Node.js with npm enabled.
exit /b 1

:no_cargo
echo [ERROR] Cargo was not found. Install Rust using rustup.
exit /b 1

:no_rustc
echo [ERROR] rustc was not found. Install Rust using rustup.
exit /b 1

:error
echo.
echo [ERROR] Build failed. Check the log above for the failing command.
echo Common causes:
echo   - Missing Visual Studio C++ Build Tools or Windows SDK
echo   - Rust dependencies still downloading on first build
echo   - Missing WebView2 packaging prerequisites
exit /b 1
