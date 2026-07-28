[CmdletBinding()]
param()

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Exe = Join-Path $Root 'target\release\pulseflow-governor.exe'
if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) {
    $Exe = Join-Path $Root 'target\release\pulseflow-governor'
}
if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) {
    throw 'Release binary was not found.'
}

$Port = 18791
$Base = 'http://127.0.0.1:{0}' -f $Port
$VerifyState = Join-Path $Root 'state\verify'
$VerifyConfig = Join-Path $VerifyState 'pulseflow.verify.json'
Remove-Item -LiteralPath $VerifyState -Recurse -Force -ErrorAction SilentlyContinue
New-Item -Path $VerifyState -ItemType Directory -Force | Out-Null

function New-ApiUri {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory=$true)][string]$Path,
        [hashtable]$Query
    )

    $Uri = '{0}{1}' -f $Base, $Path
    if ($Query -and $Query.Count -gt 0) {
        $Pairs = foreach ($Key in @($Query.Keys | Sort-Object)) {
            $EncodedKey = [Uri]::EscapeDataString([string]$Key)
            $EncodedValue = [Uri]::EscapeDataString([string]$Query[$Key])
            '{0}={1}' -f $EncodedKey, $EncodedValue
        }
        $Uri = '{0}?{1}' -f $Uri, ($Pairs -join '&')
    }
    return $Uri
}

function Invoke-ApiGet {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory=$true)][string]$Path,
        [hashtable]$Query
    )
    $Uri = New-ApiUri -Path $Path -Query $Query
    return Invoke-RestMethod -Uri $Uri
}

function Invoke-ApiPost {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory=$true)][string]$Path,
        [Parameter(Mandatory=$true)]$Payload
    )
    $Uri = New-ApiUri -Path $Path
    $Body = if ($Payload -is [string]) { $Payload } else { $Payload | ConvertTo-Json -Depth 20 }
    return Invoke-RestMethod -Uri $Uri -Method Post -ContentType 'application/json' -Body $Body
}

function Assert-Value {
    param([string]$Name, $Value)
    if ($null -eq $Value -or ([string]$Value).Length -eq 0) {
        throw ('Smoke contract returned no value for {0}.' -f $Name)
    }
}

$Config = Get-Content (Join-Path $Root 'config\pulseflow.json') -Raw | ConvertFrom-Json
$Config.bind = '127.0.0.1:{0}' -f $Port
$Config.event_ledger_path = (Join-Path $VerifyState 'events.jsonl')
$Config.storage.directory = (Join-Path $VerifyState 'sessions')
$Config.sample_interval_ms = 250
$Config.storage.metadata_flush_every_samples = 1
$Config.governor.minimum_dwell_ms = 0
$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[IO.File]::WriteAllText($VerifyConfig, ($Config | ConvertTo-Json -Depth 20), $Utf8NoBom)

$Target = $null

$Job = Start-Job -ScriptBlock {
    param($Binary, $WorkingRoot, $ConfigPath)
    Set-Location $WorkingRoot
    $env:PULSEFLOW_CONFIG = $ConfigPath
    & $Binary serve
} -ArgumentList $Exe, $Root, $VerifyConfig

try {
    $Ready = $false
    foreach ($Attempt in 1..40) {
        Start-Sleep -Milliseconds 250
        if ($Job.State -in @('Failed', 'Completed', 'Stopped')) {
            $Failure = Receive-Job -Job $Job -Keep -ErrorAction SilentlyContinue | Out-String
            throw ('PulseFlow verification server job failed. {0}' -f $Failure.Trim())
        }
        try {
            $Health = Invoke-RestMethod -Uri (New-ApiUri -Path '/health') -TimeoutSec 1
            if ($Health -eq 'ok') {
                $Ready = $true
                break
            }
        }
        catch { }
    }
    if (-not $Ready) {
        $ServerOutput = Receive-Job -Job $Job -Keep -ErrorAction SilentlyContinue | Out-String
        throw ('PulseFlow verification server did not become healthy. {0}' -f $ServerOutput.Trim())
    }

    Invoke-ApiGet -Path '/api/status' | Out-Null
    Invoke-ApiGet -Path '/api/config' | Out-Null
    Invoke-ApiGet -Path '/api/history' -Query @{ limit = 20 } | Out-Null
    Invoke-ApiGet -Path '/api/sessions' | Out-Null
    Invoke-ApiGet -Path '/api/directive' | Out-Null
    Invoke-ApiGet -Path '/api/interlink/handshake' | Out-Null
    Invoke-ApiGet -Path '/api/processes' | Out-Null
    Invoke-ApiGet -Path '/api/interlink/verify' | Out-Null
    Invoke-ApiGet -Path '/api/ledger/tail' -Query @{ limit = 20 } | Out-Null

    $Signal = [ordered]@{
        source = 'aria-smoke'
        agent = 'verification-agent'
        task_type = 'compiler-test'
        model = 'none'
        context_tokens = 256
        input_queue = 2
        output_queue = 1
        latency_ms = 42.0
        tokens_per_second = 12.5
        completed_units = 1
        success = $true
        busy = $true
    }
    Invoke-ApiPost -Path '/api/signal' -Payload $Signal | Out-Null

    foreach ($Mode in @('quiet', 'balanced', 'performance')) {
        Invoke-ApiPost -Path '/api/mode' -Payload @{ mode = $Mode } | Out-Null
    }
    Invoke-ApiPost -Path '/api/control' -Payload @{ command = 'reset' } | Out-Null
    foreach ($Enabled in @($false, $true)) {
        Invoke-ApiPost -Path '/api/recording' -Payload @{ enabled = $Enabled } | Out-Null
    }
    foreach ($Stage in @('recorder', 'analytics', 'replay', 'shadow', 'bounded_adaptive', 'agent_policy')) {
        Invoke-ApiPost -Path '/api/learning-stage' -Payload @{ stage = $Stage } | Out-Null
    }
    Invoke-ApiPost -Path '/api/tuning' -Payload @{
        balanced_setpoint = 0.66
        kp = 0.65
        ki = 0.08
        kd = 0.10
        kr = 0.34
        residue_decay = 0.82
        filter_alpha = 0.24
        slew_per_sample = 0.07
    } | Out-Null

    $Target = Start-Process -FilePath (Join-Path $env:SystemRoot 'System32\cmd.exe') -ArgumentList @('/c','ping -t 127.0.0.1') -WindowStyle Hidden -PassThru
    Invoke-ApiGet -Path '/api/processes' | Out-Null
    Invoke-ApiPost -Path '/api/interlink/connect' -Payload @{ pid = [uint32]$Target.Id } | Out-Null
    Invoke-ApiPost -Path '/api/interlink/verify' -Payload @{} | Out-Null
    Invoke-ApiPost -Path '/api/interlink/baseline' -Payload @{} | Out-Null
    Start-Sleep -Milliseconds 600
    Invoke-ApiPost -Path '/api/interlink/enable' -Payload @{} | Out-Null
    Start-Sleep -Seconds 2
    $Interlink = Invoke-ApiGet -Path '/api/interlink/verify'
    if (-not $Interlink.target_alive) { throw 'Interlink verification did not confirm the smoke target.' }
    if (-not $Interlink.governor_armed) { throw 'Interlink verification did not confirm armed process authority.' }
    if (-not $Interlink.process_qos_active) { throw 'Interlink verification did not confirm applied process QoS.' }
    Invoke-ApiPost -Path '/api/interlink/disconnect' -Payload @{} | Out-Null

    Start-Sleep -Seconds 2
    $Status = Invoke-ApiGet -Path '/api/status'
    $Baseline = [string]$Status.session_id
    Assert-Value -Name 'baseline session id' -Value $Baseline
    $BaselineSegment = [Uri]::EscapeDataString($Baseline)

    Invoke-ApiGet -Path ('/api/session/{0}' -f $BaselineSegment) -Query @{ limit = 100 } | Out-Null
    Invoke-ApiGet -Path ('/api/summary/{0}' -f $BaselineSegment) | Out-Null
    Invoke-WebRequest -Uri (New-ApiUri -Path ('/api/export/{0}' -f $BaselineSegment)) -UseBasicParsing | Out-Null
    Invoke-ApiPost -Path '/api/replay' -Payload @{ session_id = $Baseline } | Out-Null

    $New = Invoke-ApiPost -Path '/api/session/new' -Payload '{}'
    $Candidate = [string]$New.session_id
    Assert-Value -Name 'candidate session id' -Value $Candidate
    Start-Sleep -Seconds 2
    Invoke-ApiPost -Path '/api/compare' -Payload @{
        baseline_session_id = $Baseline
        candidate_session_id = $Candidate
    } | Out-Null

    $RootPage = Invoke-WebRequest -Uri (New-ApiUri -Path '/') -UseBasicParsing
    if ($RootPage.Content -notmatch 'PULSEFLOW GOVERNOR') {
        throw 'Dashboard root did not contain the application title.'
    }
    $IconResponse = Invoke-WebRequest -Uri (New-ApiUri -Path '/favicon.ico') -UseBasicParsing
    if ($IconResponse.RawContentLength -lt 1024) {
        throw 'Application icon response was unexpectedly small.'
    }
    $WebManifest = Invoke-ApiGet -Path '/site.webmanifest'
    if ($WebManifest.name -ne 'PulseFlow Governor' -or @($WebManifest.icons).Count -lt 2) {
        throw 'Installable web manifest did not expose the PulseFlow icon set.'
    }
}
finally {
    if ($Target -and -not $Target.HasExited) { Stop-Process -Id $Target.Id -Force -ErrorAction SilentlyContinue }
    Stop-Job -Job $Job -ErrorAction SilentlyContinue
    Remove-Job -Job $Job -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $VerifyState -Recurse -Force -ErrorAction SilentlyContinue
}
