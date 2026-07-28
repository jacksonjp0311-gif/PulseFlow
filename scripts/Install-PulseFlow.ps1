[CmdletBinding()]
param(
    [string]$InstallDirectory = '',
    [string]$ShortcutDirectory = '',
    [switch]$DesktopShortcut,
    [switch]$SkipBuild
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($InstallDirectory)) {
    $InstallDirectory = Join-Path $env:LOCALAPPDATA 'Programs\PulseFlow Governor'
}
$InstallDirectory = [IO.Path]::GetFullPath($InstallDirectory)
$Binary = Join-Path $Root 'target\release\pulseflow-governor.exe'

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
Copy-Item -LiteralPath $Binary -Destination $InstalledBinary -Force
Copy-Item -LiteralPath (Join-Path $Root 'config\pulseflow.json') -Destination (Join-Path $ConfigDirectory 'pulseflow.json') -Force
Copy-Item -LiteralPath (Join-Path $Root 'assets\icons\pulseflow-governor.ico') -Destination $InstalledIcon -Force

$LauncherSource = @'
[CmdletBinding()]
param([switch]$NoBrowser)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$InstallRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$Executable = Join-Path $InstallRoot 'bin\pulseflow-governor.exe'
$DashboardUrl = 'http://127.0.0.1:8791/'
$HealthUrl = 'http://127.0.0.1:8791/health'

function Test-PulseFlowReady {
    try {
        $Response = Invoke-WebRequest -UseBasicParsing -Uri $HealthUrl -TimeoutSec 1
        return ($Response.StatusCode -eq 200 -and $Response.Content.Trim() -eq 'ok')
    }
    catch {
        return $false
    }
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
    Start-Process $DashboardUrl
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
        [Parameter(Mandatory=$true)][string]$ShortcutPath
    )
    $Shell = New-Object -ComObject WScript.Shell
    $Shortcut = $Shell.CreateShortcut($ShortcutPath)
    $Shortcut.TargetPath = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $Shortcut.Arguments = ('-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "{0}"' -f $InstalledLauncher)
    $Shortcut.WorkingDirectory = $InstallDirectory
    $Shortcut.IconLocation = ('{0},0' -f $InstalledIcon)
    $Shortcut.Description = 'PulseFlow Governor workload-governance instrument'
    $Shortcut.Save()
}

$StartMenuShortcut = Join-Path $StartMenuDirectory 'PulseFlow Governor.lnk'
New-PulseFlowShortcut -ShortcutPath $StartMenuShortcut

if ($DesktopShortcut) {
    $DesktopDirectory = [Environment]::GetFolderPath([Environment+SpecialFolder]::Desktop)
    New-PulseFlowShortcut -ShortcutPath (Join-Path $DesktopDirectory 'PulseFlow Governor.lnk')
}

$InstallRecord = [ordered]@{
    schema = 'pulseflow.installation.v1'
    version = '0.3.1'
    installed_at_utc = [DateTime]::UtcNow.ToString('o')
    install_directory = $InstallDirectory
    executable = $InstalledBinary
    launcher = $InstalledLauncher
    icon = $InstalledIcon
    start_menu_shortcut = $StartMenuShortcut
    desktop_shortcut = [bool]$DesktopShortcut
}
[IO.File]::WriteAllText(
    (Join-Path $InstallDirectory 'installation.json'),
    ($InstallRecord | ConvertTo-Json -Depth 5),
    $Utf8NoBom
)

Write-Host ('PulseFlow Governor installed at {0}' -f $InstallDirectory) -ForegroundColor Green
Write-Host ('Start Menu shortcut: {0}' -f $StartMenuShortcut) -ForegroundColor Green
