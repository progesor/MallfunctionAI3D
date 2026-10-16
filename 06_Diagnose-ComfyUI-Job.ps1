# Read-only ComfyUI job diagnosis; never changes input, output or queued jobs.
[CmdletBinding()]
param([string]$ComfyUrl = 'http://127.0.0.1:8191', [string]$PromptId = '')
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($root)) { $root = Split-Path -Parent $MyInvocation.MyCommand.Path }
$reportDir = Join-Path $root 'reports'
New-Item -ItemType Directory -Path $reportDir -Force | Out-Null
$endpoint = $ComfyUrl.TrimEnd('/')
if ($PromptId -and $PromptId -notmatch '^[0-9a-fA-F-]{36}$') { throw 'PromptId gecersiz' }
$url = if ($PromptId) { "$endpoint/history/$PromptId" } else { "$endpoint/history?max_items=12" }
$response = Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 40 -Headers @{ Accept = 'application/json' }
$raw = [string]$response.Content
$data = ConvertFrom-Json -InputObject $raw
$stamp = Get-Date -Format 'yyyyMMdd_HHmmss'
$file = Join-Path $reportDir "ComfyHistory_$stamp.json"
[System.IO.File]::WriteAllText($file, $raw, (New-Object System.Text.UTF8Encoding $false))
Write-Host "History saved: $file" -ForegroundColor Green
if (-not $data.PSObject.Properties.Count) { Write-Host 'History empty.'; exit }
foreach ($item in $data.PSObject.Properties) {
    $id = $item.Name
    $entry = $item.Value
    $status = [string]$entry.status.status_str
    $completed = $entry.status.completed
    $outputNodes = @($entry.outputs.PSObject.Properties.Name) -join ','
    $saveInPrompt = $false
    try { $saveInPrompt = $null -ne $entry.prompt[2].'322' } catch {}
    Write-Host "`n$id status=$status completed=$completed save322=$saveInPrompt outputNodes=$outputNodes"
    foreach ($m in @($entry.status.messages)) {
        if (@($m).Count -lt 2) { continue }
        if ($m[0] -eq 'execution_error') {
            Write-Host "ERROR node=$($m[1].node_id) type=$($m[1].node_type): $($m[1].exception_message)" -ForegroundColor Yellow
        }
    }
}
Write-Host "`nOnly share this report privately. It may include local paths and input names." -ForegroundColor DarkYellow
