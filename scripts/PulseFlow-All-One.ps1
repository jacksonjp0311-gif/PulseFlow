[CmdletBinding()]
param(
    [ValidateSet('Serve','Run','Attach','Build','Test','Verify','Package')]
    [string]$Mode = 'Serve',
    [string]$Command,
    [string[]]$Arguments = @(),
    [uint32]$PidToAttach = 0,
    [switch]$NoOpen
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0
$Root = Split-Path -Parent $PSScriptRoot
$Url = 'http://127.0.0.1:8791'
Set-Location $Root

function Write-Stage([string]$Label, [string]$State = 'ACTIVE') {
    $Marker = '[>]'
    $Color = 'Cyan'
    if ($State -eq 'PASS') { $Marker = '[OK]'; $Color = 'Green' }
    if ($State -eq 'FAIL') { $Marker = '[X]'; $Color = 'Red' }
    Write-Host ("|-- {0,-4} {1,-38} {2}" -f $Marker, $Label, $State) -ForegroundColor $Color
}

function Assert-Command([string]$Name) {
    $Resolved = Get-Command $Name -ErrorAction SilentlyContinue
    if (-not $Resolved) { throw "$Name is required but was not found on PATH." }
}

function Open-DashboardDeferred {
    if ($NoOpen) { return }
    Start-Job -ScriptBlock {
        param($Address)
        Start-Sleep -Seconds 2
        cmd.exe /c start "" $Address
    } -ArgumentList $Url | Out-Null
}

Write-Host 'PULSEFLOW / ARIA OPERATOR'
Write-Host '-------------------------'
Write-Stage 'resolve repository root'
Write-Stage $Root 'PASS'
Assert-Command 'cargo'

if ($Mode -eq 'Build') {
    Write-Stage 'compile optimized governor'
    & cargo build --release
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed.' }
    Write-Stage 'optimized governor' 'PASS'
    exit 0
}

if ($Mode -eq 'Test') {
    Write-Stage 'run Rust test lattice'
    & cargo test --all-targets
    if ($LASTEXITCODE -ne 0) { throw 'cargo test failed.' }
    Write-Stage 'Rust test lattice' 'PASS'
    exit 0
}

if ($Mode -eq 'Verify') {
    & "$PSScriptRoot\ARIA-Verify.ps1"
    Write-Stage 'ARIA verification' 'PASS'
    exit 0
}

if ($Mode -eq 'Package') {
    & "$PSScriptRoot\ARIA-Verify.ps1"
    $PackageRoot = Join-Path $Root 'dist\pulseflow-governor-v0.4.0'
    $Archive = Join-Path $Root 'dist\pulseflow-governor-v0.4.0.zip'
    Remove-Item $PackageRoot -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $Archive -Force -ErrorAction SilentlyContinue
    New-Item $PackageRoot -ItemType Directory -Force | Out-Null
    Get-ChildItem $Root -Force | Where-Object {
        $_.Name -notin @('target','dist','.git')
    } | ForEach-Object {
        Copy-Item $_.FullName $PackageRoot -Recurse -Force
    }
    Compress-Archive -Path "$PackageRoot\*" -DestinationPath $Archive -CompressionLevel Optimal
    Write-Stage "package $Archive" 'PASS'
    exit 0
}

if ($Mode -eq 'Attach') {
    if ($PidToAttach -eq 0) { throw 'Attach mode requires -PidToAttach with a numeric process ID.' }
    Write-Stage "attach target PID $PidToAttach"
    Open-DashboardDeferred
    $CargoArguments = @('run','--release','--','attach',[string]$PidToAttach)
    & cargo @CargoArguments
    exit $LASTEXITCODE
}

if ($Mode -eq 'Run') {
    if ([string]::IsNullOrWhiteSpace($Command)) { throw 'Run mode requires -Command with an executable path.' }
    Write-Stage "launch governed workload $Command"
    Open-DashboardDeferred
    $CargoArguments = @('run','--release','--','run','--',$Command) + $Arguments
    & cargo @CargoArguments
    exit $LASTEXITCODE
}

if ($Mode -eq 'Serve') {
    Write-Stage 'launch observation lab'
    Open-DashboardDeferred
    & cargo run --release -- serve
    exit $LASTEXITCODE
}
