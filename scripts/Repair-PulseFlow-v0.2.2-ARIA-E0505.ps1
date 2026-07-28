[CmdletBinding()]
param(
    [string]$Root = "$env:USERPROFILE\OneDrive\Desktop\pulseflow-governor",
    [switch]$Rollback,
    [switch]$SkipSmoke
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

function Stage([string]$Name, [string]$State, [string]$Detail = '') {
    $marker = '[>]'
    $color = 'Cyan'
    if ($State -eq 'PASS') { $marker = '[OK]'; $color = 'Green' }
    if ($State -eq 'FAIL') { $marker = '[X]'; $color = 'Red' }
    if ($State -eq 'INFO') { $marker = '[i]'; $color = 'Yellow' }
    Write-Host ("|-- {0} {1,-34} {2} {3}" -f $marker, $Name, $State, $Detail) -ForegroundColor $color
}

function Write-Utf8NoBom([string]$Path, [string]$Text) {
    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
}

function Sha256([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

Write-Host 'ARIA / PULSEFLOW E0505 REPAIR'
Write-Host '-----------------------------'

$Root = [System.IO.Path]::GetFullPath($Root)
if (-not (Test-Path -LiteralPath $Root -PathType Container)) {
    throw "PulseFlow repository not found: $Root"
}
Set-Location $Root
Stage 'repository identity' 'PASS' $Root

$MainPath = Join-Path $Root 'src\main.rs'
$CargoPath = Join-Path $Root 'Cargo.toml'
$VerifyPath = Join-Path $Root 'scripts\ARIA-Verify.ps1'
foreach ($Path in @($MainPath, $CargoPath, $VerifyPath)) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Required path missing: $Path"
    }
}
Stage 'required lattice' 'PASS' 'main.rs + Cargo.toml + verifier'

$RollbackRoot = Join-Path $Root 'state\aria-rollback'
$ReceiptRoot = Join-Path $Root 'state\aria-receipts'
New-Item -ItemType Directory -Force -Path $RollbackRoot, $ReceiptRoot | Out-Null

if ($Rollback) {
    $Latest = Get-ChildItem -LiteralPath $RollbackRoot -Directory | Sort-Object Name -Descending | Select-Object -First 1
    if (-not $Latest) { throw 'No ARIA rollback snapshot exists.' }
    Copy-Item -LiteralPath (Join-Path $Latest.FullName 'main.rs') -Destination $MainPath -Force
    Copy-Item -LiteralPath (Join-Path $Latest.FullName 'Cargo.toml') -Destination $CargoPath -Force
    Stage 'rollback snapshot' 'PASS' $Latest.Name
    exit 0
}

$Stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$Snapshot = Join-Path $RollbackRoot $Stamp
New-Item -ItemType Directory -Force -Path $Snapshot | Out-Null
Copy-Item -LiteralPath $MainPath -Destination (Join-Path $Snapshot 'main.rs')
Copy-Item -LiteralPath $CargoPath -Destination (Join-Path $Snapshot 'Cargo.toml')
$BeforeMain = Sha256 $MainPath
$BeforeCargo = Sha256 $CargoPath
Stage 'rollback boundary' 'PASS' $Snapshot

$Source = Get-Content -LiteralPath $MainPath -Raw
$Old = '    server::serve(&config.bind, state, config)'
$New = "    let bind = config.bind.clone();`r`n    server::serve(&bind, state, config)"
if ($Source.Contains($Old)) {
    $Source = $Source.Replace($Old, $New)
    Write-Utf8NoBom $MainPath $Source
    Stage 'borrow boundary repair' 'PASS' 'clone bind before moving config'
}
elseif ($Source.Contains('let bind = config.bind.clone();') -and $Source.Contains('server::serve(&bind, state, config)')) {
    Stage 'borrow boundary repair' 'PASS' 'already applied'
}
else {
    Stage 'borrow boundary repair' 'FAIL' 'expected source pattern not found'
    throw 'The main.rs server call does not match the known E0505 wound.'
}

$Cargo = Get-Content -LiteralPath $CargoPath -Raw
if ($Cargo -match 'version\s*=\s*"0\.2\.[01]"') {
    $Cargo = [regex]::Replace($Cargo, 'version\s*=\s*"0\.2\.[01]"', 'version = "0.2.2"', 1)
    Write-Utf8NoBom $CargoPath $Cargo
}
Stage 'candidate identity' 'PASS' 'PulseFlow 0.2.2'

$AfterMain = Sha256 $MainPath
$AfterCargo = Sha256 $CargoPath
$ReceiptPath = Join-Path $ReceiptRoot ("e0505-{0}.json" -f $Stamp)

$Status = 'FRACTURE'
$ExitCode = 1
try {
    Stage 'ARIA verification gates' 'ACTIVE' 'fmt -> check -> test -> release -> smoke'
    if ($SkipSmoke) {
        & powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File $VerifyPath -SkipSmoke
    }
    else {
        & powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File $VerifyPath
    }
    if ($LASTEXITCODE -ne 0) { throw "ARIA verifier returned exit code $LASTEXITCODE" }
    $Status = 'PROMOTED'
    $ExitCode = 0
    Stage 'candidate promotion' 'PASS' 'all declared gates passed'
}
catch {
    Stage 'candidate promotion' 'FAIL' 'not promoted; inspect next deterministic diagnostic'
    Write-Host $_.Exception.Message -ForegroundColor Red
}
finally {
    $Receipt = [ordered]@{
        schema = 'aria-repair-receipt/v1'
        project = 'pulseflow-governor'
        repair = 'rust-e0505-bind-borrow-before-config-move'
        timestamp_utc = (Get-Date).ToUniversalTime().ToString('o')
        status = $Status
        repository = $Root
        source = [ordered]@{
            aria_repository = 'https://github.com/jacksonjp0311-gif/ARIA'
            doctrine = 'verify-before-promotion; deterministic diagnostic; rollback preserved'
        }
        rollback_snapshot = $Snapshot
        hashes = [ordered]@{
            main_before = $BeforeMain
            main_after = $AfterMain
            cargo_before = $BeforeCargo
            cargo_after = $AfterCargo
        }
        verification = [ordered]@{
            command = '.\scripts\ARIA-Verify.ps1'
            skip_smoke = [bool]$SkipSmoke
            exit_code = $ExitCode
        }
    }
    Write-Utf8NoBom $ReceiptPath ($Receipt | ConvertTo-Json -Depth 8)
    Stage 'provenance receipt' 'PASS' $ReceiptPath
}

if ($ExitCode -ne 0) {
    Write-Host ''
    Write-Host 'The valid E0505 repair remains installed so the next compiler wound is visible.' -ForegroundColor Yellow
    Write-Host ("Rollback command: powershell.exe -ExecutionPolicy Bypass -File `"{0}`" -Rollback" -f $MyInvocation.MyCommand.Path) -ForegroundColor Yellow
    exit $ExitCode
}

Write-Host '+-- [OK] PULSEFLOW REPAIR VERIFIED AND PROMOTED' -ForegroundColor Green
exit 0
