[CmdletBinding()]
param(
    [string]$BaseUrl = 'http://127.0.0.1:8791',
    [string]$Source = 'external-runtime',
    [string]$Agent = 'unbound',
    [string]$TaskType = 'unknown',
    [string]$Model = 'unknown',
    [uint64]$ContextTokens = 0,
    [uint32]$InputQueue = 0,
    [uint32]$OutputQueue = 0,
    [double]$LatencyMs = 0,
    [double]$TokensPerSecond = 0,
    [uint64]$CompletedUnits = 0,
    [Nullable[bool]]$Success = $null,
    [bool]$Busy = $false
)
$Body = @{
    source=$Source; agent=$Agent; task_type=$TaskType; model=$Model;
    context_tokens=$ContextTokens; input_queue=$InputQueue; output_queue=$OutputQueue;
    latency_ms=$LatencyMs; tokens_per_second=$TokensPerSecond; completed_units=$CompletedUnits;
    success=$Success; busy=$Busy
} | ConvertTo-Json
Invoke-RestMethod "$BaseUrl/api/signal" -Method Post -ContentType 'application/json' -Body $Body
