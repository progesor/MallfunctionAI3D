# Blender need not be on PATH. Uses the Python available for earlier QC scripts.
[CmdletBinding()]
param(
    [string]$BlenderPath = '',
    [string]$InputGlb = '',
    [string]$OutDir = '',
    [double]$HeightM = 0,
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($root)) {
    $root = Split-Path -Parent $MyInvocation.MyCommand.Path
}
$launcher = Join-Path $root 'tools\launch_blender.py'
$argsPython = @($launcher)
if ($BlenderPath) { $argsPython += @('--blender', $BlenderPath) }
if ($InputGlb) { $argsPython += @('--input', $InputGlb) }
if ($OutDir) { $argsPython += @('--outdir', $OutDir) }
if ($HeightM -gt 0) { $argsPython += @('--height-m', $HeightM.ToString([System.Globalization.CultureInfo]::InvariantCulture)) }
if ($DryRun) { $argsPython += '--dry-run' }
foreach ($pythonExe in @('py', 'python', 'python3')) {
    if (-not (Get-Command $pythonExe -ErrorAction SilentlyContinue)) { continue }
    if ($pythonExe -eq 'py') { & py -3 @argsPython } else { & $pythonExe @argsPython }
    exit $LASTEXITCODE
}
throw 'Python 3 bulunamadi. Daha once kullandigin py -3 komutunun PATH uzerinden erisilebilir oldugundan emin ol.'
