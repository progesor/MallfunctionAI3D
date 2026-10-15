# OPTIONAL explicit xatlas native-Python provider installation for Studio.
# Does not alter Blender's system site-packages, ComfyUI, Rust or Unity.
# Requires the user to supply the Blender 5.1 bundled Python executable.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$BlenderPython,
    [string]$LibraryRoot = (Join-Path $env:APPDATA 'net.mallfunction.ai3dstudio\library')
)
$ErrorActionPreference='Stop'
if (-not (Test-Path -LiteralPath $BlenderPython -PathType Leaf)) {
    throw "Blender Python not found: $BlenderPython"
}
$version=(& $BlenderPython --version 2>&1) -join ' '
if ($LASTEXITCODE -ne 0 -or $version -notmatch 'Python 3\.13\.') {
    throw "Expected Blender 5.1 Python 3.13 (xatlas win_amd64 wheel): $version"
}
$dir=Join-Path $LibraryRoot 'plugins\xatlas'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
Write-Host "Installing optional MIT-licensed xatlas==0.0.11 only to $dir" -ForegroundColor Cyan
& $BlenderPython -m pip install --disable-pip-version-check --no-deps --only-binary=:all: --target $dir --upgrade 'xatlas==0.0.11'
if ($LASTEXITCODE -ne 0) {
    throw "Blender Python pip failed. Python may not have pip installed. No Studio file was modified."
}
$probe='import sys; sys.path.insert(0,sys.argv[1]); import xatlas; print("xatlas optional provider available")'
& $BlenderPython -c $probe $dir
if ($LASTEXITCODE -ne 0) {
    throw "xatlas wheel cannot be imported by Blender Python. Stop; Studio built-in Blender UV engine still works."
}
Write-Host 'Installed app-scoped xatlas provider. Restart Studio; do not regenerate ComfyUI assets.' -ForegroundColor Green
