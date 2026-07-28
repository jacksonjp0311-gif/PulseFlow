[CmdletBinding()]
param([string]$BaseUrl = 'http://127.0.0.1:8791')
$Directive = Invoke-RestMethod "$BaseUrl/api/directive"
$Directive | ConvertTo-Json -Depth 10
if (-not $Directive.shadow_only) {
    Write-Host "Directive is live: concurrency=$($Directive.recommended_concurrency), batch=$($Directive.recommended_batch_size), route=$($Directive.model_route)"
}
if ($Directive.shadow_only) {
    Write-Host 'Directive is shadow-only; log it but do not apply it.'
}
