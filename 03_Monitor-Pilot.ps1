[CmdletBinding()]
param([string]$ComfyUrl='http://127.0.0.1:8188',[int]$MaxSeconds=1800,[int]$GpuIndex=-1,[string]$Station='unspecified')
$ErrorActionPreference='Stop'
$arguments=@((Join-Path $PSScriptRoot 'tools\monitor_pilot.py'),'--url',$ComfyUrl,'--seconds',$MaxSeconds)
if ($GpuIndex -ge 0) { $arguments += @('--gpu-index',$GpuIndex) }
if (-not [string]::IsNullOrWhiteSpace($Station)) { $arguments += @('--station',$Station) }
if (Get-Command py -ErrorAction SilentlyContinue) {& py -3 @arguments;exit $LASTEXITCODE}
if (Get-Command python -ErrorAction SilentlyContinue) {& python @arguments;exit $LASTEXITCODE}
if (Get-Command python3 -ErrorAction SilentlyContinue) {& python3 @arguments;exit $LASTEXITCODE}
throw 'Python 3 is required for the GPU logger.'
