[CmdletBinding()]
param([switch]$Json)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$ConnectPath = Join-Path $Root 'aria\ARIA-CONNECT.json'
if (-not (Test-Path -LiteralPath $ConnectPath -PathType Leaf)) {
    throw ('ARIA connection contract was not found: {0}' -f $ConnectPath)
}

function Get-Sha256Text([string]$Text) {
    $Sha = [Security.Cryptography.SHA256]::Create()
    try {
        $Bytes = [Text.UTF8Encoding]::new($false).GetBytes($Text)
        return ([BitConverter]::ToString($Sha.ComputeHash($Bytes))).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $Sha.Dispose()
    }
}

$Connection = Get-Content -LiteralPath $ConnectPath -Raw | ConvertFrom-Json
$CargoText = [IO.File]::ReadAllText((Join-Path $Root 'Cargo.toml'))
$VersionMatch = [regex]::Match($CargoText, '(?m)^version\s*=\s*"([^"]+)"')
if (-not $VersionMatch.Success) { throw 'Cargo package version could not be resolved.' }
$Version = $VersionMatch.Groups[1].Value

$Resources = @()
foreach ($RelativePath in @($Connection.read_order)) {
    $FullPath = Join-Path $Root ([string]$RelativePath)
    if (-not (Test-Path -LiteralPath $FullPath -PathType Leaf)) {
        throw ('Handshake resource is missing: {0}' -f $RelativePath)
    }
    $Hash = (Get-FileHash -LiteralPath $FullPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $Resources += [ordered]@{ path = [string]$RelativePath; sha256 = $Hash }
}

$IdentityLines = New-Object System.Collections.Generic.List[string]
$IdentityLines.Add('schema=aria.pulseflow.handshake/v1')
$IdentityLines.Add(('version={0}' -f $Version))
$IdentityLines.Add(('protocol={0}' -f (@($Connection.protocol) -join '>')))
$IdentityLines.Add(('authority={0}' -f [string]$Connection.authority.initial))
foreach ($Resource in $Resources) {
    $IdentityLines.Add(('{0}={1}' -f $Resource.path, $Resource.sha256))
}
$Digest = Get-Sha256Text ($IdentityLines -join "`n")

$Record = [ordered]@{
    schema = 'aria.pulseflow.handshake/v1'
    digest = ('sha256:{0}' -f $Digest)
    repository = [ordered]@{
        name = 'pulseflow-governor'
        version = $Version
        manifest = 'MANIFEST.json'
    }
    protocol = @($Connection.protocol)
    resources = $Resources
    commands = $Connection.commands
    authority = $Connection.authority
    next_boundary = [string]$Connection.next_boundary
}

if ($Json) {
    Write-Output ($Record | ConvertTo-Json -Depth 20 -Compress)
    exit 0
}

Write-Host 'ARIA / PULSEFLOW HANDSHAKE'
Write-Host '--------------------------'
Write-Host ('[OK] identity      {0}' -f $Record.digest)
Write-Host ('[OK] version       {0}' -f $Version)
Write-Host ('[OK] authority     {0}' -f $Record.authority.initial)
Write-Host ('[>]  next boundary {0}' -f $Record.next_boundary)
