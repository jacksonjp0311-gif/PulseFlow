param(
    [Parameter(Position=0)]
    [ValidateSet(
        "activate", "bootstrap", "query", "remember", "consolidate", "ritual",
        "verify", "status", "graph", "telemetry", "environment", "meta-language",
        "thalamus", "interlink", "neural-replay", "doctor",
        "identity", "distill", "kernels", "interconnect", "immune", "metrics",
        "prune", "organism", "breathe", "causal", "glyphs", "evolve", "stream",
        "harness", "hygiene", "packs"
    )]
    [string]$Command = "activate",
    [string]$Task = "",
    [string]$Query = "",
    [string]$Kind = "discovery",
    [string]$Text = "",
    [string]$Path = "",
    [string]$Action = "status",
    [string]$Profile = "agent",
    [ValidateSet("before", "after")]
    [string]$Slot = "before",
    [int]$Budget = 800,
    [int]$K = 8,
    [switch]$Learn,
    [switch]$DryRun,
    [switch]$NoSeal,
    [switch]$DoctrineOnly,
    [switch]$Annotate,
    [switch]$Decay
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
if ([string]::IsNullOrWhiteSpace($EngineModuleRoot)) { $EngineModuleRoot = 'C:\Users\jacks\OneDrive\Desktop\Cortex' }
if (-not [string]::IsNullOrWhiteSpace($EngineModuleRoot) -and (Test-Path $EngineModuleRoot)) {
    if ([string]::IsNullOrWhiteSpace($env:PYTHONPATH)) { $env:PYTHONPATH = $EngineModuleRoot }
    if (-not [string]::IsNullOrWhiteSpace($env:PYTHONPATH) -and -not $env:PYTHONPATH.StartsWith($EngineModuleRoot)) {
        $env:PYTHONPATH = "$EngineModuleRoot;$env:PYTHONPATH"
    }
}

$EnginePython = [string]$Config.engine_python
if ([string]::IsNullOrWhiteSpace($EnginePython)) { $EnginePython = [string]$env:CORTEX_PYTHON }
if ([string]::IsNullOrWhiteSpace($EnginePython)) { $EnginePython = 'C:\Program Files\Python312\python.exe' }

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
if ($Command -eq "ritual") {
    if ([string]::IsNullOrWhiteSpace($Task)) { throw "-Task is required for ritual." }
    $ArgsList += @("ritual", "--repo", $RepoName, "--task", $Task, "--budget", "$Budget", "--json")
    if (-not [string]::IsNullOrWhiteSpace($Text)) {
        $ArgsList += @("--remember-kind", $Kind, "--remember-text", $Text)
    }
}
if ($Command -eq "verify") { $ArgsList += @("verify", "--repo", $RepoName, "--json") }
if ($Command -eq "status") { $ArgsList += @("status", "--repo", $RepoName, "--json") }
if ($Command -eq "graph") { $ArgsList += @("graph", "--repo", $RepoName, "--json") }
if ($Command -eq "telemetry") { $ArgsList += @("telemetry", "--repo", $RepoName, "--json") }
if ($Command -eq "environment") { $ArgsList += @("environment", "--repo", $RepoName, "--json") }
if ($Command -eq "meta-language") { $ArgsList += @("meta-language", "--repo", $RepoName, "--json") }
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
if ($Command -eq "identity") {
    $ArgsList += @("identity", "--json")
    if (-not [string]::IsNullOrWhiteSpace($RepoName)) { $ArgsList += @("--repo", $RepoName) }
    if (-not [string]::IsNullOrWhiteSpace($Path)) { $ArgsList += @("--path", $Path) }
}
if ($Command -eq "distill") {
    $ArgsList += @("distill", "--repo", $RepoName, "--json")
    if ($NoSeal) { $ArgsList += "--no-seal" }
    if ($DoctrineOnly) { $ArgsList += "--doctrine-only" }
}
if ($Command -eq "kernels") {
    $ArgsList += @("kernels", "--repo", $RepoName, "--json")
    if ($Annotate) { $ArgsList += "--annotate" }
}
if ($Command -eq "interconnect") { $ArgsList += @("interconnect", "--repo", $RepoName, "--json") }
if ($Command -eq "immune") { $ArgsList += @("immune", "--repo", $RepoName, "--json") }
if ($Command -eq "metrics") { $ArgsList += @("metrics", "--repo", $RepoName, "--json") }
if ($Command -eq "prune") {
    $ArgsList += @("prune", "--repo", $RepoName, "--json")
    if ($DryRun) { $ArgsList += "--dry-run" }
    if ($Decay) { $ArgsList += "--decay" }
}
if ($Command -eq "organism") {
    if ([string]::IsNullOrWhiteSpace($Task)) { throw "-Task is required for organism." }
    $ArgsList += @("organism", "--repo", $RepoName, "--task", $Task, "--budget", "$Budget", "--profile", $Profile, "--json")
}
if ($Command -eq "breathe") {
    $ArgsList += @("breathe", "--repo", $RepoName, "--budget", "$Budget", "--profile", $Profile, "--json")
    if (-not [string]::IsNullOrWhiteSpace($Task)) { $ArgsList += @("--task", $Task) }
}
if ($Command -eq "causal") {
    $ValidCausal = @("status", "report", "evaluate", "probe")
    if ($ValidCausal -notcontains $Action) {
        throw "-Action for causal must be one of: status, report, evaluate, probe"
    }
    $ArgsList += @("causal", $Action, "--repo", $RepoName, "--json")
    if ($Action -eq "probe") {
        if ([string]::IsNullOrWhiteSpace($Task) -and [string]::IsNullOrWhiteSpace($Query)) {
            throw "-Task or -Query is required for causal probe."
        }
        $ProbeText = if (-not [string]::IsNullOrWhiteSpace($Task)) { $Task } else { $Query }
        $ArgsList += @("--task", $ProbeText, "--slot", $Slot, "--k", "$K")
    }
}
if ($Command -eq "glyphs") {
    $ArgsList += @("glyphs", "--json")
}
if ($Command -eq "stream") {
    $StreamAction = if (-not [string]::IsNullOrWhiteSpace($Action)) { $Action } else { "status" }
    if ($StreamAction -eq "seal") {
        $ArgsList += @("stream", "seal", "--repo", $RepoName, "--json")
    } else {
        $ArgsList += @("stream", "status", "--repo", $RepoName, "--json")
    }
}
if ($Command -eq "harness") {
    $ArgsList += @("harness", "--repo", $RepoName, "--budget", "$Budget", "--json")
}
if ($Command -eq "hygiene") {
    $ArgsList += @("hygiene", "--repo", $RepoName, "--json")
}
if ($Command -eq "packs") {
    $PackAction = if (-not [string]::IsNullOrWhiteSpace($Action)) { $Action } else { "list" }
    if ($PackAction -eq "index") {
        $ArgsList += @("packs", "index", "--repo", $RepoName, "--json")
    } elseif ($PackAction -eq "probe") {
        $Probe = if (-not [string]::IsNullOrWhiteSpace($Task)) { $Task } else { $Query }
        $ArgsList += @("packs", "probe", "--task", $Probe, "--json")
    } else {
        $ArgsList += @("packs", "list", "--json")
    }
}
if ($Command -eq "evolve") {
    if ([string]::IsNullOrWhiteSpace($Text)) {
        throw "-Text must carry activation-id for evolve (use -Text <activation_id>)."
    }
    if ([string]::IsNullOrWhiteSpace($Kind)) {
        throw "-Kind is used as verification type for evolve."
    }
    $Status = if (-not [string]::IsNullOrWhiteSpace($Query)) { $Query } else { "verified" }
    $ArgsList += @("evolve", "--repo", $RepoName, "--activation-id", $Text, "--status", $Status, "--verification", $Kind, "--json")
    if (-not [string]::IsNullOrWhiteSpace($Task)) { $ArgsList += @("--task", $Task) }
}

& $ResolvedPython @ArgsList
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
