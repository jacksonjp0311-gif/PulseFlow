[CmdletBinding()]
param(
    [string]$InstallDirectory = '',
    [string]$ShortcutDirectory = '',
    [switch]$DesktopShortcut,
    [switch]$SkipBuild,
    [switch]$NoStop
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($InstallDirectory)) {
    $InstallDirectory = Join-Path $env:LOCALAPPDATA 'Programs\PulseFlow Governor'
}
$InstallDirectory = [IO.Path]::GetFullPath($InstallDirectory)
$Binary = Join-Path $Root 'target\release\pulseflow-governor.exe'

function Get-PulseFlowVersion {
    $CargoToml = Join-Path $Root 'Cargo.toml'
    if (-not (Test-Path -LiteralPath $CargoToml -PathType Leaf)) {
        return '0.0.0'
    }
    foreach ($Line in Get-Content -LiteralPath $CargoToml) {
        if ($Line -match '^\s*version\s*=\s*"([^"]+)"') {
            return $Matches[1]
        }
    }
    return '0.0.0'
}

function Stop-PulseFlowProcesses {
    param([string]$InstalledBinaryPath)

    $Stopped = @()
    Get-Process -Name 'pulseflow-governor' -ErrorAction SilentlyContinue | ForEach-Object {
        try {
            Stop-Process -Id $_.Id -Force -ErrorAction Stop
            $Stopped += $_.Id
        }
        catch {
            Write-Warning ("Could not stop pulseflow-governor PID {0}: {1}" -f $_.Id, $_.Exception.Message)
        }
    }

    # Also free anyone listening on the dashboard port.
    try {
        Get-NetTCPConnection -LocalPort 8791 -State Listen -ErrorAction SilentlyContinue |
            Select-Object -ExpandProperty OwningProcess -Unique |
            ForEach-Object {
                if ($_ -and $_ -gt 0) {
                    try {
                        Stop-Process -Id $_ -Force -ErrorAction Stop
                        $Stopped += $_
                    }
                    catch { }
                }
            }
    }
    catch { }

    if ($Stopped.Count -gt 0) {
        Start-Sleep -Milliseconds 800
        Write-Host ('Stopped running PulseFlow process(es): {0}' -f (($Stopped | Select-Object -Unique) -join ', ')) -ForegroundColor Yellow
    }

    # Wait briefly if the installed binary is still locked.
    if (Test-Path -LiteralPath $InstalledBinaryPath -PathType Leaf) {
        for ($Attempt = 0; $Attempt -lt 20; $Attempt++) {
            try {
                $Stream = [IO.File]::Open(
                    $InstalledBinaryPath,
                    [IO.FileMode]::Open,
                    [IO.FileAccess]::ReadWrite,
                    [IO.FileShare]::None
                )
                $Stream.Close()
                break
            }
            catch {
                Start-Sleep -Milliseconds 250
            }
        }
    }
}

$Version = Get-PulseFlowVersion

if (-not $SkipBuild) {
    Push-Location $Root
    try {
        & cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw 'PulseFlow release build failed.'
        }
    }
    finally {
        Pop-Location
    }
}
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) {
    throw 'Release binary not found. Run cargo build --release or omit -SkipBuild.'
}

$ProgramDirectory = Join-Path $InstallDirectory 'bin'
$ConfigDirectory = Join-Path $InstallDirectory 'config'
$IconDirectory = Join-Path $InstallDirectory 'assets\icons'
New-Item -Path $ProgramDirectory, $ConfigDirectory, $IconDirectory -ItemType Directory -Force | Out-Null

$InstalledBinary = Join-Path $ProgramDirectory 'pulseflow-governor.exe'
$InstalledIcon = Join-Path $IconDirectory 'pulseflow-governor.ico'
$InstalledLauncher = Join-Path $InstallDirectory 'Launch-PulseFlow.ps1'
$InstallRecordPath = Join-Path $InstallDirectory 'installation.json'

# Always refresh desktop if a prior install put one there, or if requested now.
$PriorDesktop = $false
if (Test-Path -LiteralPath $InstallRecordPath -PathType Leaf) {
    try {
        $Prior = Get-Content -LiteralPath $InstallRecordPath -Raw | ConvertFrom-Json
        if ($Prior.desktop_shortcut -eq $true) {
            $PriorDesktop = $true
        }
        if ($Prior.version -and $Prior.version -ne $Version) {
            Write-Host ("Updating installed PulseFlow {0} -> {1}" -f $Prior.version, $Version) -ForegroundColor Cyan
        }
    }
    catch { }
}
$DesktopPath = Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::Desktop)) 'PulseFlow Governor.lnk'
if (Test-Path -LiteralPath $DesktopPath -PathType Leaf) {
    $PriorDesktop = $true
}
$WantDesktop = [bool]($DesktopShortcut -or $PriorDesktop)

if (-not $NoStop) {
    Stop-PulseFlowProcesses -InstalledBinaryPath $InstalledBinary
}

Copy-Item -LiteralPath $Binary -Destination $InstalledBinary -Force
Copy-Item -LiteralPath (Join-Path $Root 'config\pulseflow.json') -Destination (Join-Path $ConfigDirectory 'pulseflow.json') -Force
Copy-Item -LiteralPath (Join-Path $Root 'assets\icons\pulseflow-governor.ico') -Destination $InstalledIcon -Force
# Touch icon so Explorer picks up a fresh cache entry when the mark changes.
(Get-Item -LiteralPath $InstalledIcon).LastWriteTime = Get-Date
(Get-Item -LiteralPath $InstalledBinary).LastWriteTime = Get-Date

$LauncherSource = @'
[CmdletBinding()]
param([switch]$NoBrowser)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$InstallRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$Executable = Join-Path $InstallRoot 'bin\pulseflow-governor.exe'
$DashboardUrl = 'http://127.0.0.1:8791/'
$HealthUrl = 'http://127.0.0.1:8791/health'
$InstallMeta = Join-Path $InstallRoot 'installation.json'

function Test-PulseFlowReady {
    try {
        $Response = Invoke-WebRequest -UseBasicParsing -Uri $HealthUrl -TimeoutSec 1
        return ($Response.StatusCode -eq 200 -and $Response.Content.Trim() -eq 'ok')
    }
    catch {
        return $false
    }
}

function Get-InstalledVersion {
    if (Test-Path -LiteralPath $InstallMeta -PathType Leaf) {
        try {
            $Meta = Get-Content -LiteralPath $InstallMeta -Raw | ConvertFrom-Json
            if ($Meta.version) { return [string]$Meta.version }
        }
        catch { }
    }
    return 'unknown'
}

if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
    $Shell = New-Object -ComObject WScript.Shell
    $Shell.Popup('PulseFlow Governor executable is missing. Re-run Install-PulseFlow.ps1 from the repo.', 0, 'PulseFlow Governor', 16) | Out-Null
    exit 1
}

if (-not (Test-PulseFlowReady)) {
    Start-Process -FilePath $Executable -ArgumentList 'serve' -WorkingDirectory $InstallRoot -WindowStyle Hidden | Out-Null
    $Ready = $false
    for ($Attempt = 0; $Attempt -lt 40; $Attempt++) {
        Start-Sleep -Milliseconds 250
        if (Test-PulseFlowReady) {
            $Ready = $true
            break
        }
    }
    if (-not $Ready) {
        $Shell = New-Object -ComObject WScript.Shell
        $Shell.Popup('PulseFlow Governor could not start. Port 8791 may be used by another application.', 0, 'PulseFlow Governor', 16) | Out-Null
        exit 1
    }
}

if (-not $NoBrowser) {
    # Prefer a version-stamped query so browsers do not keep an old tab title forever.
    $Version = Get-InstalledVersion
    Start-Process ($DashboardUrl + '?v=' + [Uri]::EscapeDataString($Version))
}
'@
$Utf8NoBom = New-Object Text.UTF8Encoding($false)
[IO.File]::WriteAllText($InstalledLauncher, $LauncherSource, $Utf8NoBom)

if ([string]::IsNullOrWhiteSpace($ShortcutDirectory)) {
    $ProgramsDirectory = [Environment]::GetFolderPath([Environment+SpecialFolder]::Programs)
    $ShortcutDirectory = Join-Path $ProgramsDirectory 'PulseFlow Governor'
}
$StartMenuDirectory = [IO.Path]::GetFullPath($ShortcutDirectory)
New-Item -Path $StartMenuDirectory -ItemType Directory -Force | Out-Null

function New-PulseFlowShortcut {
    param(
        [Parameter(Mandatory = $true)][string]$ShortcutPath,
        [Parameter(Mandatory = $true)][string]$AppVersion
    )
    $Shell = New-Object -ComObject WScript.Shell
    # Delete first so Windows does not keep a stale link/icon handle.
    if (Test-Path -LiteralPath $ShortcutPath -PathType Leaf) {
        Remove-Item -LiteralPath $ShortcutPath -Force
    }
    $Shortcut = $Shell.CreateShortcut($ShortcutPath)
    $Shortcut.TargetPath = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $Shortcut.Arguments = ('-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "{0}"' -f $InstalledLauncher)
    $Shortcut.WorkingDirectory = $InstallDirectory
    $Shortcut.IconLocation = ('{0},0' -f $InstalledIcon)
    $Shortcut.Description = ('PulseFlow Governor v{0} - workload-governance instrument' -f $AppVersion)
    $Shortcut.Save()
}

$StartMenuShortcut = Join-Path $StartMenuDirectory 'PulseFlow Governor.lnk'
New-PulseFlowShortcut -ShortcutPath $StartMenuShortcut -AppVersion $Version

$DesktopShortcutPath = $null
if ($WantDesktop) {
    $DesktopDirectory = [Environment]::GetFolderPath([Environment+SpecialFolder]::Desktop)
    $DesktopShortcutPath = Join-Path $DesktopDirectory 'PulseFlow Governor.lnk'
    New-PulseFlowShortcut -ShortcutPath $DesktopShortcutPath -AppVersion $Version
}

$InstallRecord = [ordered]@{
    schema                 = 'pulseflow.installation.v1'
    version                = $Version
    source_binary          = $Binary
    source_binary_bytes    = (Get-Item -LiteralPath $Binary).Length
    installed_binary_bytes = (Get-Item -LiteralPath $InstalledBinary).Length
    installed_at_utc       = [DateTime]::UtcNow.ToString('o')
    install_directory      = $InstallDirectory
    executable             = $InstalledBinary
    launcher               = $InstalledLauncher
    icon                   = $InstalledIcon
    start_menu_shortcut    = $StartMenuShortcut
    desktop_shortcut       = [bool]$WantDesktop
    desktop_shortcut_path  = $DesktopShortcutPath
}
[IO.File]::WriteAllText(
    $InstallRecordPath,
    ($InstallRecord | ConvertTo-Json -Depth 5),
    $Utf8NoBom
)

# Nudge shell icon cache for this path (best-effort; no full cache wipe).
try {
    $ShellApp = New-Object -ComObject Shell.Application
    $null = $ShellApp
}
catch { }

Write-Host ('PulseFlow Governor v{0} installed at {1}' -f $Version, $InstallDirectory) -ForegroundColor Green
Write-Host ('Executable: {0} ({1:N0} bytes)' -f $InstalledBinary, (Get-Item $InstalledBinary).Length) -ForegroundColor Green
Write-Host ('Start Menu shortcut: {0}' -f $StartMenuShortcut) -ForegroundColor Green
if ($WantDesktop -and $DesktopShortcutPath) {
    Write-Host ('Desktop shortcut: {0}' -f $DesktopShortcutPath) -ForegroundColor Green
}
Write-Host 'Desktop/Start Menu shortcuts are rewritten on every install so the icon and version stay current.' -ForegroundColor DarkGray
