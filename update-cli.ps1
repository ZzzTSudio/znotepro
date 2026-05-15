$ErrorActionPreference = "Stop"
$cliDir = "$env:LOCALAPPDATA\znote\cli"
$srcCli = Join-Path $PSScriptRoot "cli\znote.js"
$srcCmd = Join-Path $PSScriptRoot "cli\znote.cmd"
New-Item -ItemType Directory -Path $cliDir -Force | Out-Null
Copy-Item $srcCli "$cliDir\znote.js" -Force
Copy-Item $srcCmd "$cliDir\znote.cmd" -Force
Write-Host "CLI updated at $cliDir" -ForegroundColor Green
