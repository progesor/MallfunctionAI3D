# Optional app-scoped xatlas 0.0.11 installer — same verified wheel engine as Studio UI.
# Requires no pip; downloads a pinned cp313 win_amd64 wheel from PyPI,
# verifies PyPI SHA256 and unpacks only to the user's Studio library.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$BlenderPython,
    [string]$LibraryRoot = (Join-Path $env:APPDATA 'net.mallfunction.ai3dstudio\library')
)
$ErrorActionPreference='Stop'
if (-not (Test-Path -LiteralPath $BlenderPython -PathType Leaf)) {
    throw "Blender Python missing: $BlenderPython"
}
$version=(& $BlenderPython --version 2>&1) -join ' '
if ($LASTEXITCODE -ne 0 -or $version -notmatch 'Python 3\.13\.') {
    throw "Blender 5.1 Python 3.13 needed for xatlas cp313 wheel: $version"
}
$worker=Join-Path $PSScriptRoot '..\blender\install_xatlas_wheel.py'
if (-not (Test-Path -LiteralPath $worker -PathType Leaf)) {
    throw "Missing pinned xatlas installer script: $worker"
}
& $BlenderPython -I $worker $LibraryRoot
if ($LASTEXITCODE -ne 0) {
    throw "xatlas installation failed; source GLB and Blender site-packages unchanged."
}
$installed=Join-Path $LibraryRoot 'plugins\xatlas'
$probe='import sys;sys.path.insert(0,sys.argv[1]);import xatlas,numpy;print("XATLAS_IMPORT_OK")'
& $BlenderPython -c $probe $installed
if ($LASTEXITCODE -ne 0) {
    throw "xatlas installed but cannot be imported by Blender Python."
}
Write-Host "Blender + xatlas 0.0.11 ready (Studio owned plugin only)" -ForegroundColor Green
