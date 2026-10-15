# Local-only UV engine tests. No GitHub Actions, Blender bake, or ComfyUI job.
[CmdletBinding()]
param([switch]$Full)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
Set-Location $root
$python=Get-Command py -ErrorAction SilentlyContinue
if ($python) { $pythonExe=$python.Source; $pythonPrefix=@('-3') }
else {
    $python=Get-Command python -ErrorAction Stop
    $pythonExe=$python.Source
    $pythonPrefix=@()
}
$files=@(
    'test_uv_topology.py',
    'test_uv_proxy_transfer.py',
    'test_uv_xatlas_provider.py',
    'test_xatlas_installer.py',
    'test_uv_layer_lifecycle.py',
    'test_uv_planner.py',
    'test_uv_repair_contract.py'
)
foreach ($f in $files) {
    Write-Host "Offline UV test: $f" -ForegroundColor Cyan
    & $pythonExe @pythonPrefix -m unittest discover -s tests -p $f
    if($LASTEXITCODE -ne 0) { throw "UV offline regression failed: $f" }
}
Write-Host 'Offline UV regression suite PASS' -ForegroundColor Green
if ($Full) {
    Push-Location (Join-Path $root 'studio')
    try {
        & npm run build
        if ($LASTEXITCODE -ne 0) { throw 'Studio TSX/Vite build failed' }
    } finally { Pop-Location }
    & cargo check --manifest-path 'studio/src-tauri/Cargo.toml'
    if ($LASTEXITCODE -ne 0) { throw 'Rust/Tauri cargo check failed' }
    Write-Host 'Frontend and Rust checks PASS' -ForegroundColor Green
}
Write-Host 'Real Blender 5.1 + xatlas/native UV rendering remains a manual Studio acceptance test.' -ForegroundColor Yellow
