[CmdletBinding()]
param([switch]$SkipSmoke)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$VerificationClock = [Diagnostics.Stopwatch]::StartNew()

function Stage([string]$Label) {
    Write-Host ("|-- [>]  {0,-42} ACTIVE" -f $Label) -ForegroundColor Cyan
}
function Pass([string]$Label) {
    Write-Host ("|-- [OK] {0,-42} PASS" -f $Label) -ForegroundColor Green
}
function Fail([string]$Label, [string]$Message) {
    Write-Host ("|-- [X]  {0,-42} FAIL" -f $Label) -ForegroundColor Red
    throw $Message
}
function Require([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        Fail "tool $Name" "$Name was not found on PATH."
    }
}

Write-Host 'ARIA / PULSEFLOW VERIFICATION'
Write-Host '-----------------------------'

Stage 'verify required repository lattice'
$Required = @(
    'Cargo.toml','Cargo.lock','build.rs','README.md','config\pulseflow.json','web\index.html',
    'assets\icons\pulseflow-governor.ico',
    'assets\icons\pulseflow-governor-64.png',
    'assets\icons\pulseflow-governor-192.png',
    'assets\icons\pulseflow-governor-512.png',
    'schemas\ui-actions.json','schemas\observation-frame.schema.json',
    'schemas\agent-directive.schema.json','schemas\adaptive-suggestion.schema.json',
    'schemas\evidence-receipt.schema.json',
    'schemas\pulseflow-api.contract.json','schemas\powershell-runtime.contract.json',
    'aria\ARIA-CONNECT.json','scripts\ARIA-Handshake.ps1','scripts\ARIA-Smoke.ps1',
    'scripts\Install-PulseFlow.ps1',
    'docs\ARCHITECTURE.md','docs\CONTROL_MODEL.md','docs\DATA_MODEL.md',
    'docs\AUTHORITY_STATE_MODEL.md','docs\METRIC_GLOSSARY.md',
    'docs\EXPERIMENTAL_METHODOLOGY.md','docs\MIGRATION_V2.md',
    'docs\FUTURIST_GOVERNOR.md','docs\ICON_AND_INSTALLATION.md','src\authority.rs',
    'src\main.rs','src\server.rs','tests\authority_tests.rs','tests\ui_contract_tests.rs'
)
foreach ($Relative in $Required) {
    if (-not (Test-Path -LiteralPath (Join-Path $Root $Relative))) {
        Fail 'repository lattice' "Missing $Relative"
    }
}
Pass 'repository lattice'

Stage 'verify content-addressed package manifest'
$Manifest = Get-Content -LiteralPath 'MANIFEST.json' -Raw | ConvertFrom-Json
if ($Manifest.schema -ne 'pulseflow.package-manifest.v1') {
    Fail 'package manifest' 'Unexpected manifest schema.'
}
foreach ($Entry in @($Manifest.files)) {
    $ManifestPath = Join-Path $Root ([string]$Entry.path)
    if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
        Fail 'package manifest' ('Missing manifested file: {0}' -f $Entry.path)
    }
    $Info = Get-Item -LiteralPath $ManifestPath
    if ([int64]$Info.Length -ne [int64]$Entry.bytes) {
        Fail 'package manifest' ('Size mismatch: {0}' -f $Entry.path)
    }
    $ActualHash = (Get-FileHash -LiteralPath $ManifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($ActualHash -ne ([string]$Entry.sha256).ToLowerInvariant()) {
        Fail 'package manifest' ('SHA-256 mismatch: {0}' -f $Entry.path)
    }
}
Pass "package manifest $(@($Manifest.files).Count) files"

Stage 'establish deterministic ARIA handshake'
$HandshakeText = & "$PSScriptRoot\ARIA-Handshake.ps1" -Json
$Handshake = $HandshakeText | ConvertFrom-Json
if ($Handshake.schema -ne 'aria.pulseflow.handshake/v1') {
    Fail 'ARIA handshake' 'Unexpected handshake schema.'
}
if ($Handshake.authority.initial -ne 'none') {
    Fail 'ARIA authority boundary' 'Initial authority must remain none.'
}
if (-not ([string]$Handshake.digest).StartsWith('sha256:')) {
    Fail 'ARIA handshake identity' 'Handshake did not produce a SHA-256 identity.'
}
Pass ('ARIA handshake ' + ([string]$Handshake.digest).Substring(0, 19))

Stage 'parse configuration and contracts'
$JsonContracts = @(
    'config\pulseflow.json',
    'schemas\ui-actions.json',
    'schemas\observation-frame.schema.json',
    'schemas\agent-directive.schema.json',
    'schemas\adaptive-suggestion.schema.json',
    'schemas\evidence-receipt.schema.json',
    'schemas\pulseflow-api.contract.json',
    'schemas\powershell-runtime.contract.json',
    'aria\ARIA-CONNECT.json'
)
foreach ($JsonPath in $JsonContracts) {
    Get-Content -LiteralPath $JsonPath -Raw | ConvertFrom-Json | Out-Null
}
Pass "JSON contracts $($JsonContracts.Count)"

Stage 'verify PowerShell 5.1 semantic boundaries'
$PowerShellContract = Get-Content -LiteralPath 'schemas\powershell-runtime.contract.json' -Raw | ConvertFrom-Json
$PowerShellScripts = @(Get-ChildItem -LiteralPath (Join-Path $Root 'scripts') -Filter '*.ps1' -File | Sort-Object Name)
foreach ($ScriptPath in $PowerShellScripts) {
    $Bytes = [IO.File]::ReadAllBytes($ScriptPath.FullName)
    if ($PowerShellContract.rules.ascii_safe) {
        foreach ($Byte in $Bytes) {
            if ($Byte -gt 127) {
                Fail 'PowerShell ASCII boundary' "Non-ASCII byte in $($ScriptPath.Name)"
            }
        }
    }

    $Tokens = $null
    $ParseErrors = $null
    [Management.Automation.Language.Parser]::ParseFile(
        $ScriptPath.FullName,
        [ref]$Tokens,
        [ref]$ParseErrors
    ) | Out-Null
    if (@($ParseErrors).Count -gt 0) {
        Fail 'PowerShell parser' (($ParseErrors | ForEach-Object { $_.Message }) -join '; ')
    }

    $ScriptText = [IO.File]::ReadAllText($ScriptPath.FullName)
    if ($PowerShellContract.rules.strict_mode_required -and $ScriptText -notmatch 'Set-StrictMode') {
        Fail 'PowerShell strict mode' "Set-StrictMode missing from $($ScriptPath.Name)"
    }
    foreach ($Rule in @($PowerShellContract.rules.forbidden_patterns)) {
        if ([regex]::IsMatch($ScriptText, [string]$Rule.regex)) {
            Fail ('PowerShell semantic rule ' + [string]$Rule.id) $ScriptPath.Name
        }
    }
}
$SmokeText = [IO.File]::ReadAllText((Join-Path $Root ([string]$PowerShellContract.smoke_script.path)))
foreach ($Marker in @($PowerShellContract.smoke_script.required_markers)) {
    if (-not $SmokeText.Contains([string]$Marker)) {
        Fail 'PowerShell smoke contract' "Missing marker: $Marker"
    }
}
Pass "PowerShell scripts $($PowerShellScripts.Count)"

Stage 'verify every UI action and backend route'
$Html = Get-Content -LiteralPath 'web\index.html' -Raw
$Server = Get-Content -LiteralPath 'src\server.rs' -Raw
$Contract = Get-Content -LiteralPath 'schemas\ui-actions.json' -Raw | ConvertFrom-Json
$HtmlActions = [regex]::Matches($Html, 'data-action="([^"]+)"') | ForEach-Object { $_.Groups[1].Value } | Sort-Object -Unique
$ContractActions = $Contract.actions.id | Sort-Object -Unique
$Difference = Compare-Object $HtmlActions $ContractActions
if ($Difference) { Fail 'UI action contract' ($Difference | Out-String) }
foreach ($Action in $Contract.actions) {
    if ($Html -notmatch [regex]::Escape('"' + $Action.id + '":')) {
        Fail 'UI action handlers' "No JavaScript handler for $($Action.id)"
    }
    if ($Action.kind -eq 'http' -and $Server -notmatch [regex]::Escape($Action.route)) {
        Fail 'backend route contract' "No backend route marker for $($Action.id): $($Action.route)"
    }
}
$Tabs = [regex]::Matches($Html, 'data-tab="([^"]+)"') | ForEach-Object { $_.Groups[1].Value } | Sort-Object -Unique
$Views = [regex]::Matches($Html, 'data-view="([^"]+)"') | ForEach-Object { $_.Groups[1].Value } | Sort-Object -Unique
if (Compare-Object $Tabs $Views) { Fail 'tab/view contract' 'A tab does not have a matching view.' }
Pass "UI actions $($Contract.actions.Count)"

Stage 'parse browser JavaScript'
Require 'node'
$ScriptMatch = [regex]::Match($Html, '(?s)<script>(.*?)</script>')
if (-not $ScriptMatch.Success) { Fail 'browser JavaScript' 'Inline script block was not found.' }
$Scratch = Join-Path $Root 'state\aria-ui-check.js'
New-Item -Path (Split-Path -Parent $Scratch) -ItemType Directory -Force | Out-Null
$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[IO.File]::WriteAllText($Scratch, $ScriptMatch.Groups[1].Value, $Utf8NoBom)
& node --check $Scratch
if ($LASTEXITCODE -ne 0) { Fail 'browser JavaScript' 'node --check failed.' }
Remove-Item -LiteralPath $Scratch -Force
Pass 'browser JavaScript'

Stage 'format and parse Rust source'
Require 'cargo'
& cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { Fail 'cargo fmt check' 'Rust formatting check failed. Verification does not mutate source; run cargo fmt explicitly, review the diff, and verify again.' }
Pass 'cargo fmt / parser'

Stage 'compile all Rust targets'
& cargo check --all-targets
if ($LASTEXITCODE -ne 0) { Fail 'cargo check' 'Rust compilation failed.' }
Pass 'cargo check'

Stage 'execute unit and contract tests'
& cargo test --all-targets
if ($LASTEXITCODE -ne 0) { Fail 'cargo test' 'Rust tests failed.' }
Pass 'cargo test'

Stage 'compile optimized binary'
& cargo build --release
if ($LASTEXITCODE -ne 0) { Fail 'release build' 'Optimized build failed.' }
Pass 'release build'

if (-not $SkipSmoke) {
    Stage 'exercise live HTTP control surface'
    & "$PSScriptRoot\ARIA-Smoke.ps1"
    Pass 'HTTP smoke test'
}

$VerificationClock.Stop()
$ReceiptDirectory = Join-Path $Root 'state\aria-receipts'
New-Item -Path $ReceiptDirectory -ItemType Directory -Force | Out-Null
$ReceiptPath = Join-Path $ReceiptDirectory ('verification-{0}.json' -f (Get-Date -Format 'yyyyMMdd-HHmmss'))
$Receipt = [ordered]@{
    schema = 'aria.pulseflow.verification-receipt/v1'
    version = '0.3.1'
    timestamp_utc = [DateTime]::UtcNow.ToString('o')
    handshake_digest = [string]$Handshake.digest
    authority = 'none'
    gates = [ordered]@{
        repository = 'PASS'
        manifest = 'PASS'
        handshake = 'PASS'
        powershell_semantics = 'PASS'
        json_contracts = 'PASS'
        ui_contract = 'PASS'
        browser_javascript = 'PASS'
        cargo_check = 'PASS'
        cargo_test = 'PASS'
        release_build = 'PASS'
        live_smoke = $(if ($SkipSmoke) { 'SKIP' } else { 'PASS' })
    }
    duration_ms = [int][Math]::Round($VerificationClock.Elapsed.TotalMilliseconds)
}
[IO.File]::WriteAllText($ReceiptPath, ($Receipt | ConvertTo-Json -Depth 10), $Utf8NoBom)
Pass 'verification receipt'
Write-Host '+-- [OK] PULSEFLOW VERIFICATION                    PASS' -ForegroundColor Green
exit 0
