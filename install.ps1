#Requires -Version 5.1
param(
    [switch]$BuildGui
)

$ErrorActionPreference = "Stop"
$noteDir = "$env:USERPROFILE\Documents\znote"

Write-Host "=== znote Windows Installer ===" -ForegroundColor Cyan

if (!(Test-Path $noteDir)) {
    New-Item -ItemType Directory -Path $noteDir | Out-Null
    Write-Host "Created note directory: $noteDir" -ForegroundColor Green
} else {
    Write-Host "Note directory already exists: $noteDir" -ForegroundColor Green
}

if ($BuildGui) {
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        Write-Host "Building znote Pro with Tauri..." -ForegroundColor Cyan
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

Write-Host "Done. Launch znote Pro from the installed Windows application." -ForegroundColor Cyan
