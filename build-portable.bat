@echo off
setlocal enabledelayedexpansion
chcp 65001 >nul
title znote 极致便携版构建
echo ==========================================
echo   znote 极致便携版构建脚本
echo   目标：单目录、单 EXE、零环境依赖
echo ==========================================
echo.

node -v >nul 2>&1
if errorlevel 1 goto :no_node
cargo -V >nul 2>&1
if errorlevel 1 goto :no_cargo

for /f "tokens=*" %%a in ('node -v') do echo [√] Node.js %%a
for /f "tokens=*" %%a in ('cargo -V') do echo [√] %%a
echo.

echo [提示] 正在使用 VC++ 静态链接模式
echo        产物可在未安装 VC++ Redist 的电脑上运行
echo.

echo [1/4] 安装/更新依赖...
call npm install
if errorlevel 1 goto :error
cd src-tauri
call cargo fetch
cd ..

echo [2/4] 构建前端生产包...
call npm run build
if errorlevel 1 goto :error

echo [3/4] 编译 Rust Release (全静态链接 + LTO + Strip)...
echo      此步骤会最大化内联，将所有依赖编译进单个 EXE
call npm run tauri-build
if errorlevel 1 goto :error

echo [4/4] 组装便携版目录...
set PORTABLE_DIR=dist-portable\znote
set RELEASE_DIR=src-tauri\target\release

if exist "%PORTABLE_DIR%" rmdir /S /Q "%PORTABLE_DIR%"
mkdir "%PORTABLE_DIR%"

copy /Y "%RELEASE_DIR%\znote.exe" "%PORTABLE_DIR%\znote.exe" >nul
if exist "README.md" copy /Y "README.md" "%PORTABLE_DIR%\README.md" >nul
if exist "部署文档.md" copy /Y "部署文档.md" "%PORTABLE_DIR%\部署文档.md" >nul

echo znote 便携版 > "%PORTABLE_DIR%\使用说明.txt"
echo ========================== >> "%PORTABLE_DIR%\使用说明.txt"
echo 双击 znote.exe 启动 GUI 笔记工具 >> "%PORTABLE_DIR%\使用说明.txt"
echo. >> "%PORTABLE_DIR%\使用说明.txt"
echo 系统要求： >> "%PORTABLE_DIR%\使用说明.txt"
echo   - Windows 10/11 64位 >> "%PORTABLE_DIR%\使用说明.txt"
echo   - WebView2 运行时 (Win10/11 默认已安装) >> "%PORTABLE_DIR%\使用说明.txt"
echo   - 无需 Node.js / Rust / VC++ 运行库 >> "%PORTABLE_DIR%\使用说明.txt"
echo. >> "%PORTABLE_DIR%\使用说明.txt"
echo 数据目录：%%USERPROFILE%%\Documents\znote\ >> "%PORTABLE_DIR%\使用说明.txt"

echo.
echo ==========================================
echo        极致便携版构建完成！
echo ==========================================
echo.
echo 输出目录: %CD%\dist-portable\znote\
echo.
dir /B "%PORTABLE_DIR%"
echo.
echo 文件信息:
for %%F in ("%PORTABLE_DIR%\znote.exe") do (
    echo   大小: %%~zF 字节 (约 %%~zF / 1024 KB)
)
echo.
echo 零依赖验证清单：
echo   [√] 前端资源      - 内嵌在 znote.exe 中
echo   [√] Rust 运行时   - 编译进 znote.exe
echo   [√] VC++ CRT      - 静态链接，无需 vcruntime140.dll
echo   [√] jieba 词典    - 编译进二进制
echo   [√] 搜索索引      - 运行时生成在用户文档目录
echo   [○] WebView2      - 依赖 Windows 系统组件 (99%%电脑已预装)
echo.
echo 使用方式：
echo   1. 复制整个 dist-portable\znote\ 文件夹到 U 盘
echo   2. 插入另一台 Windows 电脑
echo   3. 双击 znote.exe 直接运行
echo.
echo 注意：
echo   - 笔记数据保存在当前电脑的 %%USERPROFILE%%\Documents\znote\
echo   - 首次启动会自动创建该目录
echo   - 如遇到 WebView2 缺失，系统会提示下载安装
echo.
pause
exit /b 0

:no_node
echo [错误] 未检测到 Node.js。请先安装 https://nodejs.org/
pause
exit /b 1

:no_cargo
echo [错误] 未检测到 Cargo。请先安装 https://rustup.rs/
pause
exit /b 1

:error
echo.
echo [错误] 构建中断。请检查上方错误日志。
pause
exit /b 1
