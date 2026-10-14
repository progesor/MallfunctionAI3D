[CmdletBinding()]
param(
    [string]$ReferenceName = 'PLAYBOX_FoxPlush_reference.png',
    [string]$Source = '',
    [switch]$AllowOtherSource
)
$ErrorActionPreference = 'Stop'
$target = Join-Path $PSScriptRoot 'tools\prepare_workflows.py'
$arguments = @($target,'--reference',$ReferenceName)
if ($Source) {
    $arguments += @('--source',$Source)
} else {
    $arguments += @('--download') # Downloads ONLY ~100 KB official JSON to this separate bundle.
}
if ($AllowOtherSource) { $arguments += '--allow-other-source' }
$py = Get-Command py -ErrorAction SilentlyContinue
if ($py) { & py -3 @arguments; exit $LASTEXITCODE }
$py = Get-Command python -ErrorAction SilentlyContinue
if ($py) { & python @arguments; exit $LASTEXITCODE }
$py = Get-Command python3 -ErrorAction SilentlyContinue
if ($py) { & python3 @arguments; exit $LASTEXITCODE }
throw 'Python 3 could not be found in Windows PATH. Run from an existing Python 3 terminal (or use Blender Python).'
