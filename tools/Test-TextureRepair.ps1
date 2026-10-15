# M3 surface-space texture repair acceptance. Runs ONLY on local workstation.
# GitHub CI remains manual-only. This script never edits Studio library files.
[CmdletBinding()]
param([switch]$Full)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
Push-Location $root
try{
  $python=Get-Command py -ErrorAction SilentlyContinue
  if($python){$exe=$python.Source;$prefix=@('-3')}
  else{$python=Get-Command python -ErrorAction Stop;$exe=$python.Source;$prefix=@()}
  & $exe @prefix -m unittest discover -s tests -p test_texture_seam_contract.py
  if($LASTEXITCODE -ne 0){throw 'M3 source-safety regression failed'}
  Write-Host 'M3 static source invariants: PASS' -ForegroundColor Green
  if($Full){
    Push-Location (Join-Path $root 'studio')
    try {
      & npm run check:syntax
      if($LASTEXITCODE -ne 0){throw 'Frontend syntax invalid'}
      & npm run build
      if($LASTEXITCODE -ne 0){throw 'Studio frontend build failed'}
      & npx playwright test e2e/texture-surface.spec.ts --workers=1
      if($LASTEXITCODE -ne 0){throw 'Texture surface Playwright acceptance failed'}
    } finally{Pop-Location}
    & cargo test --manifest-path studio/src-tauri/Cargo.toml --lib texture_seam
    if($LASTEXITCODE -ne 0){throw 'Rust texture seam unit tests failed'}
    & cargo check --manifest-path studio/src-tauri/Cargo.toml
    if($LASTEXITCODE -ne 0){throw 'Rust/Tauri build check failed'}
    Write-Host 'Local M3 full suite PASS' -ForegroundColor Green
  }
  Write-Host 'Real Fox and another unrelated retail prop still need visual acceptance.' -ForegroundColor Yellow
}finally{Pop-Location}
