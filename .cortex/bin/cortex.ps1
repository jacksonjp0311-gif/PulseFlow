param(
    [Parameter(Position=0)]
    [ValidateSet("activate", "bootstrap", "query", "remember", "consolidate", "verify", "status", "graph", "telemetry", "environment", "thalamus", "interlink", "neural-replay", "doctor")]
    [string]$Command = "activate",
    [string]$Task = "",
    [string]$Query = "",
    [string]$Kind = "discovery",
    [string]$Text = "",
    [int]$Budget = 1200,
    [switch]$Learn
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$ConfigPath = Join-Path $RepoRoot ".cortex\config.json"
if (-not (Test-Path $ConfigPath)) {
    throw "Cortex config is missing: $ConfigPath. Re-run repository bootstrap."
}

$Config = Get-Content $ConfigPath -Raw | ConvertFrom-Json
$RepoName = [string]$Config.repository_name
$CortexHome = [string]$Config.cortex_home
if ([string]::IsNullOrWhiteSpace($CortexHome)) { $CortexHome = [string]$env:CORTEX_HOME }
if ([string]::IsNullOrWhiteSpace($CortexHome)) { $CortexHome = 'C:\Users\jacks\.cortex' }
$EngineModuleRoot = [string]$Config.engine_module_root
if ([string]::IsNullOrWhiteSpace($EngineModuleRoot)) { $EngineModuleRoot = 'C:\Users\jacks\OneDrive\Desktop\pulseflow-governor\tools\cortex' }
if (-not [string]::IsNullOrWhiteSpace($EngineModuleRoot) -and (Test-Path $EngineModuleRoot)) {
    if ([string]::IsNullOrWhiteSpace($env:PYTHONPATH)) { $env:PYTHONPATH = $EngineModuleRoot }
    if (-not [string]::IsNullOrWhiteSpace($env:PYTHONPATH) -and -not $env:PYTHONPATH.StartsWith($EngineModuleRoot)) {
        $env:PYTHONPATH = "$EngineModuleRoot;$env:PYTHONPATH"
    }
}

$EnginePython = [string]$Config.engine_python
if ([string]::IsNullOrWhiteSpace($EnginePython)) { $EnginePython = [string]$env:CORTEX_PYTHON }
if ([string]::IsNullOrWhiteSpace($EnginePython)) { $EnginePython = 'C:\Users\jacks\OneDrive\Desktop\pulseflow-governor\tools\cortex\.venv\Scripts\python.exe' }

$ResolvedPython = $null
if (Test-Path $EnginePython) { $ResolvedPython = (Resolve-Path $EnginePython).Path }
if ($null -eq $ResolvedPython) {
    $PythonCommand = Get-Command $EnginePython -ErrorAction SilentlyContinue
    if ($null -ne $PythonCommand) { $ResolvedPython = $PythonCommand.Source }
}
if ($null -eq $ResolvedPython) {
    $PythonCommand = Get-Command python -ErrorAction SilentlyContinue
    if ($null -ne $PythonCommand) { $ResolvedPython = $PythonCommand.Source }
}
if ($null -eq $ResolvedPython) {
    throw "Cortex Python was not found. Set CORTEX_PYTHON or re-run repository bootstrap."
}

& $ResolvedPython -c "import cortex" 2>$null
if ($LASTEXITCODE -ne 0) {
    throw "The selected Python cannot import Cortex. Set CORTEX_PYTHON or re-run repository bootstrap."
}

$ArgsList = @("-m", "cortex", "--home", $CortexHome)
if ($Command -eq "activate") {
    if ([string]::IsNullOrWhiteSpace($Task)) { throw "-Task is required for activate." }
    $ArgsList += @("activate", "--repo", $RepoName, "--task", $Task, "--budget", "$Budget", "--json")
}
if ($Command -eq "bootstrap") { $ArgsList += @("bootstrap", $RepoRoot, "--name", $RepoName, "--json") }
if ($Command -eq "query") {
    if ([string]::IsNullOrWhiteSpace($Query)) { throw "-Query is required for query." }
    $ArgsList += @("query", $Query, "--repo", $RepoName, "--json")
}
if ($Command -eq "remember") {
    if ([string]::IsNullOrWhiteSpace($Text)) { throw "-Text is required for remember." }
    $ArgsList += @("remember", "--repo", $RepoName, "--kind", $Kind, "--text", $Text, "--json")
}
if ($Command -eq "consolidate") { $ArgsList += @("consolidate", "--repo", $RepoName, "--json") }
if ($Command -eq "verify") { $ArgsList += @("verify", "--repo", $RepoName, "--json") }
if ($Command -eq "status") { $ArgsList += @("status", "--repo", $RepoName, "--json") }
if ($Command -eq "graph") { $ArgsList += @("graph", "--repo", $RepoName, "--json") }
if ($Command -eq "telemetry") { $ArgsList += @("telemetry", "--repo", $RepoName, "--json") }
if ($Command -eq "environment") { $ArgsList += @("environment", "--repo", $RepoName, "--json") }
if ($Command -eq "thalamus") {
    if ([string]::IsNullOrWhiteSpace($Task)) { throw "-Task is required for thalamus." }
    $ArgsList += @("thalamus", "--repo", $RepoName, "--task", $Task, "--budget", "$Budget", "--json")
}
if ($Command -eq "doctor") { $ArgsList += @("doctor", "--repo", $RepoName, "--json") }
if ($Command -eq "neural-replay") { $ArgsList += @("neural-replay", "--repo", $RepoName, "--json") }
if ($Command -eq "interlink") {
    if ([string]::IsNullOrWhiteSpace($Task)) { throw "-Task is required for interlink." }
    $ArgsList += @("interlink", "--repo", $RepoName, "--task", $Task, "--json")
    if ($Learn) { $ArgsList += "--learn" }
}

& $ResolvedPython @ArgsList
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
