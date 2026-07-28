[CmdletBinding()]
param(
    [string]$BaseUrl = 'http://127.0.0.1:8791',
    [switch]$Force
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$Cortex = Join-Path $Root '.cortex\bin\cortex.ps1'
$RuntimeDirectory = Join-Path $Root '.cortex\runtime'
$MarkerPath = Join-Path $RuntimeDirectory 'pulseflow-synced-iterations.json'

if (-not (Test-Path -LiteralPath $Cortex -PathType Leaf)) {
    throw 'Cortex is not initialized. Run scripts\Initialize-PulseFlow-Cortex.ps1 first.'
}

$Uri = $BaseUrl.TrimEnd('/') + '/api/learning/iterations'
$IterationResponse = Invoke-RestMethod -Method Get -Uri $Uri
$Iterations = @($IterationResponse | ForEach-Object { $_ })
$Synced = @{}
if ((Test-Path -LiteralPath $MarkerPath -PathType Leaf) -and -not $Force) {
    $Prior = Get-Content -LiteralPath $MarkerPath -Raw | ConvertFrom-Json
    foreach ($Item in @($Prior | ForEach-Object { $_ })) {
        foreach ($Id in ([string]$Item -split '\s+')) {
            if (-not [string]::IsNullOrWhiteSpace($Id)) {
                $Synced[$Id] = $true
            }
        }
    }
}

New-Item -ItemType Directory -Path $RuntimeDirectory -Force | Out-Null
& $Cortex activate -Task 'Synchronize compact PulseFlow iteration discoveries into repository memory' -Budget 600 | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw 'Cortex activation failed.'
}

$Recorded = 0
foreach ($Iteration in $Iterations) {
    $Id = [string]$Iteration.iteration_id
    if ($Synced.ContainsKey($Id) -and -not $Force) {
        continue
    }
    $Text = 'PulseFlow iteration {0} on v{1}: {2} samples over {3:N1}s; ecosystem={4:N4}, latent={5:N4}, slack={6:N4}, transduction={7:N4}, net-vector={8:+0.0000;-0.0000;0.0000}. Evidence: GET /api/learning/dataset/{0}.' -f `
        $Id, $Iteration.app_version, $Iteration.samples, $Iteration.duration_seconds, `
        $Iteration.ecosystem_pressure, $Iteration.latent_pressure, `
        $Iteration.homeostatic_slack, $Iteration.pressure_transduction, `
        $Iteration.net_vector_pressure
    & $Cortex remember -Kind discovery -Text $Text | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "Cortex could not remember iteration $Id."
    }
    $Synced[$Id] = $true
    $Recorded += 1
}

if ($Recorded -gt 0) {
    & $Cortex consolidate | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw 'Cortex consolidation failed.'
    }
}

$MarkerJson = ConvertTo-Json -InputObject @($Synced.Keys | ForEach-Object { [string]$_ } | Sort-Object)
$Utf8NoBom = New-Object Text.UTF8Encoding($false)
[IO.File]::WriteAllText($MarkerPath, $MarkerJson, $Utf8NoBom)

Write-Host "PulseFlow Cortex sync complete. New iterations remembered: $Recorded" -ForegroundColor Green
