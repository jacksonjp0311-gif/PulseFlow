[CmdletBinding()]
param(
    [switch]$RunTests,
    [switch]$Force
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$EngineRoot = Join-Path $Root 'tools\cortex'
$Launcher = Join-Path $EngineRoot 'Cortex-All-One.ps1'

if (-not (Test-Path -LiteralPath $Launcher -PathType Leaf)) {
    throw "Vendored Cortex launcher is missing: $Launcher"
}

& $Launcher `
    -RepositoryPath $Root `
    -Name 'PulseFlow' `
    -Task 'Initialize computational-homeostasis iteration memory and graph-dataset recall' `
    -Force:$Force `
    -RunTests:$RunTests

if ($LASTEXITCODE -ne 0) {
    throw 'Cortex initialization failed.'
}

Write-Host 'PulseFlow Cortex memory initialized.' -ForegroundColor Green
Write-Host 'Activate with: .\.cortex\bin\cortex.ps1 activate -Task "<task>"' -ForegroundColor Green
