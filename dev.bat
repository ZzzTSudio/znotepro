@echo off
setlocal enabledelayedexpansion
chcp 65001 >nul
title znote Dev Server (Hot Reload)

echo ==========================================
echo   znote 开发热更新模式
echo ==========================================
echo.
echo 特性：
echo   - 前端修改自动刷新 (Vite HMR)
echo   - Rust 修改自动重编译 (Tauri dev)
echo   - 日志实时输出到终端
echo.

node -v >nul 2>&1
if errorlevel 1 (
    echo [错误] 未检测到 Node.js。请先安装 https://nodejs.org/
    pause
    exit /b 1
)
cargo -V >nul 2>&1
if errorlevel 1 (
    echo [错误] 未检测到 Cargo。请先安装 https://rustup.rs/
    pause
    exit /b 1
)

set NEED_NPM=0
if not exist "node_modules" (
    set NEED_NPM=1
) else (
    for %%F in (package.json) do (
        for %%G in (node_modules/.package-lock.json) do (
            if %%~tF gtr %%~tG set NEED_NPM=1
        )
    )
)

if %NEED_NPM%==1 (
    echo [1/2] 检测到依赖变更，执行 npm install ...
    call npm install
    if errorlevel 1 goto :error
) else (
    echo [1/2] 依赖已是最新，跳过 npm install
)

echo [2/2] 检查 Rust 依赖...
cd src-tauri
call cargo fetch
cd ..

echo.
echo ==========================================
echo 启动热更新开发服务器...
echo ==========================================
echo 快捷键：
echo   Ctrl+C  停止服务器
echo   Ctrl+S  保存笔记 (应用内)
echo.
call npm run tauri-dev
if errorlevel 1 goto :error

exit /b 0

:error
echo.
echo [错误] 开发服务器异常退出
echo 常见原因：
echo   1. 5173 端口被占用 (关闭其他 Vite 项目)
echo   2. WebView2 未安装 (Win10 需更新)
echo   3. Rust 编译错误 (查看上方日志)
pause
exit /b 1
