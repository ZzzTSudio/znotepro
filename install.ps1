#Requires -Version 5.1
param(
    [switch]$BuildGui
)

$ErrorActionPreference = "Stop"
$cliDir = "$env:LOCALAPPDATA\znote\cli"
$guiDir = "$env:LOCALAPPDATA\Programs\znote"
$noteDir = "$env:USERPROFILE\Documents\znote"

Write-Host "=== znote Windows Installer ===" -ForegroundColor Cyan

if (!(Test-Path $noteDir)) {
    New-Item -ItemType Directory -Path $noteDir | Out-Null
    Write-Host "Created note directory: $noteDir" -ForegroundColor Green
}

New-Item -ItemType Directory -Path $cliDir -Force | Out-Null
$scriptSrc = Join-Path $PSScriptRoot "cli\znote.js"
$cmdSrc = Join-Path $PSScriptRoot "cli\znote.cmd"
if (Test-Path $scriptSrc) {
    Copy-Item $scriptSrc "$cliDir\znote.js" -Force
    Copy-Item $cmdSrc "$cliDir\znote.cmd" -Force
    Write-Host "CLI installed to $cliDir" -ForegroundColor Green
} else {
    Write-Warning "CLI source not found at $scriptSrc"
}

$currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($currentPath -notlike "*$cliDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$currentPath;$cliDir", "User")
    Write-Host "Added $cliDir to user PATH" -ForegroundColor Green
}

if ($BuildGui) {
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        Write-Host "Building GUI with Tauri..." -ForegroundColor Cyan
        npm install
        npm run tauri-build
        $msi = Get-ChildItem "src-tauri\target\release\bundle\msi\*.msi" | Select-Object -First 1
        if ($msi) {
            Write-Host "MSI built: $($msi.FullName)" -ForegroundColor Green
            Start-Process msiexec.exe -ArgumentList "/i","`"$($msi.FullName)`"","/quiet" -Wait
            Write-Host "GUI installed" -ForegroundColor Green
        }
    } else {
        Write-Warning "Rust/Cargo not found. Skipping GUI build."
    }
}

Write-Host "Done. Use 'znote -help' to get started." -ForegroundColor Cyan
